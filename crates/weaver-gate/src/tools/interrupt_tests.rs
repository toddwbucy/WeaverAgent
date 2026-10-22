//! The typed interrupt's real Unix control path and containment boundary.
//!
//! Tests that spawn a shell need the delegated cgroup lane, like `tests.rs`.
//! Host tests exercise the same control reader and absolute deadline with a
//! file-backed events instrument. They do not claim kernel containment.

use super::tests::{ScratchTools, bare, exchange, pair};
use super::*;
use std::cell::RefCell;
use std::sync::mpsc::{Receiver, SyncSender, TryRecvError, sync_channel};
use std::time::{Duration, Instant};

type ContainmentGate = (SyncSender<()>, Receiver<()>);

std::thread_local! {
    static CONTAINMENT_GATE: RefCell<Option<ContainmentGate>> = const { RefCell::new(None) };
    static POPULATED_OBSERVER: RefCell<Option<SyncSender<()>>> = const { RefCell::new(None) };
}

pub(super) fn hold_next_containment(entered: SyncSender<()>, released: Receiver<()>) {
    CONTAINMENT_GATE.with(|slot| {
        assert!(slot.borrow_mut().replace((entered, released)).is_none());
    });
}

/// The test waits after the actual kill, before any empty observation or
/// terminal answer. Controls queued here were present before containment
/// completed, so they cannot be dismissed as a later idle interrupt.
pub(super) fn containment_entered() {
    CONTAINMENT_GATE.with(|slot| {
        if let Some((entered, release)) = slot.borrow_mut().take() {
            entered.send(()).expect("the containment observer lives");
            release
                .recv_timeout(Duration::from_secs(5))
                .expect("the test releases containment");
        }
    });
}

/// Only the executing thread arms this observation. The test's own reads
/// and fixture teardown cannot satisfy it in place of the production wait.
pub(super) fn populated_observed(value: Option<bool>) {
    if value == Some(true) {
        POPULATED_OBSERVER.with(|slot| {
            if let Some(observer) = slot.borrow_mut().take() {
                observer.send(()).expect("the populated observer lives");
            }
        });
    }
}

/// A fresh process enters the actual tools group after its first kill.
/// It holds the kernel's populated bit until the test releases it, making
/// a removed empty-wait observable even when the original shell died fast.
struct PopulatedHold {
    child: std::process::Child,
    reaped: bool,
}

impl PopulatedHold {
    fn new(tools: &Tools) -> Self {
        use std::os::unix::process::CommandExt;
        let descriptor = tools.raw();
        let parent = nix::unistd::getpid();
        let mut command = std::process::Command::new("sleep");
        command
            .arg("30")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        // SAFETY: move_self uses the same async-signal-safe move as the
        // shell. The borrowed tools descriptor remains alive through spawn.
        unsafe {
            command.pre_exec(move || cgroup::move_self(descriptor, parent));
        }
        Self {
            child: command.spawn().expect("the populated hold starts"),
            reaped: false,
        }
    }

    fn release(&mut self) -> bool {
        if self.reaped {
            return true;
        }
        let _ = self.child.kill();
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if matches!(self.child.try_wait(), Ok(Some(_))) {
                self.reaped = true;
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for PopulatedHold {
    fn drop(&mut self) {
        let _ = self.release();
    }
}

fn interrupt(channel: &Channel, by: KillCause) {
    channel
        .send(&OrganEnvelope {
            exchange: exchange(),
            position: Position::Continue,
            payload: Payload::ToolInterrupt { by },
        })
        .unwrap();
}

/// The shell and its descendants use the real delegated cgroup. The hook
/// holds only observation, never replaces kill or populated verification.
fn interrupted_execution(
    initial: Option<KillCause>,
    during_containment: &[KillCause],
    command: &str,
) -> Result<ToolOutcome, Unanswered> {
    let scratch = ScratchTools::new();
    let (gate, harness) = pair();
    let (entered, waiting) = sync_channel(1);
    let (release, released) = sync_channel(1);
    let (observed, populated) = sync_channel(1);
    let (answer, answered) = sync_channel(1);
    if let Some(by) = initial {
        interrupt(&harness, by);
    }
    let execution = ToolExecution {
        name: weaver_types::ToolName(SHELL_NAME.into()),
        arguments: serde_json::json!({"command": command}).to_string(),
        clock_ms: 2_000,
    };
    std::thread::scope(|scope| {
        let worker = scope.spawn(|| {
            CONTAINMENT_GATE.with(|slot| *slot.borrow_mut() = Some((entered, released)));
            POPULATED_OBSERVER.with(|slot| *slot.borrow_mut() = Some(observed));
            answer
                .send(super::execute_in(
                    &execution,
                    &gate,
                    &exchange(),
                    &scratch.tools,
                    &bare(),
                ))
                .unwrap();
        });
        waiting
            .recv_timeout(Duration::from_secs(5))
            .expect("the actual kill precedes the containment observation");
        assert!(
            matches!(answered.try_recv(), Err(TryRecvError::Empty)),
            "no terminal outcome leaves before containment completes"
        );
        let mut hold = PopulatedHold::new(&scratch.tools);
        assert_eq!(scratch.tools.populated(), Some(true));
        for by in during_containment {
            interrupt(&harness, *by);
        }
        release.send(()).unwrap();
        populated
            .recv_timeout(Duration::from_secs(2))
            .expect("the executing thread observes the still-populated group");
        assert!(
            matches!(
                answered.recv_timeout(Duration::from_millis(50)),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout)
            ),
            "the execution cannot answer while its tools group is populated"
        );
        assert!(hold.release(), "the test-owned population holder is reaped");
        let result = answered
            .recv_timeout(Duration::from_secs(5))
            .expect("the contained execution finishes");
        worker.join().unwrap();
        assert_eq!(
            scratch.tools.populated(),
            Some(false),
            "a result or protocol fault follows actual containment"
        );
        result
    })
}

/// Removing FlowFailure recognition makes the clock or a refusal replace
/// the fault cause. Returning before containment misses the hook entirely.
#[test]
#[ignore = "runs a shell: needs a delegated cgroup scope"]
fn flow_failure_interrupt_returns_only_after_real_containment() {
    let result = interrupted_execution(Some(KillCause::FlowFailure), &[], "sleep 5");
    assert!(
        matches!(
            result,
            Ok(ToolOutcome::Killed {
                by: KillCause::FlowFailure,
                ..
            })
        ),
        "{result:?}"
    );
}

/// Taking only the first queued control during cleanup leaves FlowFailure
/// behind the duplicate unload and reports the wrong terminal cause.
#[test]
#[ignore = "runs a shell: needs a delegated cgroup scope"]
fn queued_fault_upgrades_unload_during_real_containment_without_downgrade() {
    let result = interrupted_execution(
        Some(KillCause::Unload),
        &[KillCause::Unload, KillCause::FlowFailure, KillCause::Unload],
        "sleep 5",
    );
    assert!(
        matches!(
            result,
            Ok(ToolOutcome::Killed {
                by: KillCause::FlowFailure,
                ..
            })
        ),
        "{result:?}"
    );
}

/// An invalid typed cause is never proof that a call was contained by that
/// cause. The seam fault is delivered only after the same cleanup path.
#[test]
#[ignore = "runs a shell: needs a delegated cgroup scope"]
fn invalid_interrupt_cause_faults_only_after_real_containment() {
    for by in [KillCause::Clock, KillCause::Cancel, KillCause::Fault] {
        let result = interrupted_execution(Some(by), &[], "sleep 5");
        assert!(
            matches!(result, Err(Unanswered::Channel(ChannelFault::Undecodable))),
            "{result:?}"
        );
    }
}

/// A completed shell keeps its output and exit status when a fault arrives
/// during descendant cleanup. The interrupt cannot rewrite that outcome.
#[test]
#[ignore = "runs a shell: needs a delegated cgroup scope"]
fn completed_shell_result_survives_an_interrupt_during_cleanup() {
    let result = interrupted_execution(None, &[KillCause::FlowFailure], "printf finished; exit 7");
    assert!(
        matches!(result, Ok(ToolOutcome::Result { ref content }) if content.contains("finished") && content.contains("exit status: 7")),
        "{result:?}"
    );
}

#[test]
fn supervision_accepts_only_the_two_typed_interrupt_causes() {
    for by in [KillCause::Unload, KillCause::FlowFailure] {
        let (gate, harness) = pair();
        let exchange = exchange();
        interrupt(&harness, by);
        let mut reader = Supervision::new(&gate, &exchange);
        assert_eq!(reader.wait(Duration::ZERO), Ok(Some(by)));
    }
    for by in [KillCause::Clock, KillCause::Cancel, KillCause::Fault] {
        let (gate, harness) = pair();
        let exchange = exchange();
        interrupt(&harness, by);
        let mut reader = Supervision::new(&gate, &exchange);
        assert_eq!(reader.wait(Duration::ZERO), Err(ChannelFault::Undecodable));
    }
}

#[test]
fn misplaced_valid_interrupt_refuses_its_exchange_and_keeps_supervising() {
    let current = exchange();
    let mut foreign = current.clone();
    foreign.ordinal += 1;
    for (wrong_exchange, wrong_position) in [
        (foreign, Position::Continue),
        (current.clone(), Position::Open),
        (current.clone(), Position::Close),
    ] {
        let (gate, harness) = pair();
        harness
            .send(&OrganEnvelope {
                exchange: wrong_exchange.clone(),
                position: wrong_position,
                payload: Payload::ToolInterrupt {
                    by: KillCause::FlowFailure,
                },
            })
            .unwrap();
        let mut reader = Supervision::new(&gate, &current);
        assert_eq!(reader.wait(Duration::ZERO), Ok(None));
        assert_eq!(reader.wait(Duration::ZERO), Ok(None));
        assert_eq!(
            harness.recv().unwrap(),
            OrganEnvelope {
                exchange: wrong_exchange,
                position: Position::Close,
                payload: Payload::Refusal(LifecycleRefusal::OutOfOrder),
            }
        );
        interrupt(&harness, KillCause::FlowFailure);
        assert_eq!(
            reader.wait(Duration::ZERO),
            Ok(Some(KillCause::FlowFailure))
        );
    }
}

/// The only cause change during containment is unload to flow failure.
/// Replacing the guarded assignment with a generic last-cause-wins fails.
#[test]
fn containment_drains_all_queued_controls_and_only_upgrades_unload() {
    for (initial, expected) in [
        (None, None),
        (Some(KillCause::Clock), Some(KillCause::Clock)),
        (Some(KillCause::Cancel), Some(KillCause::Cancel)),
        (Some(KillCause::Unload), Some(KillCause::FlowFailure)),
        (Some(KillCause::FlowFailure), Some(KillCause::FlowFailure)),
    ] {
        let (gate, harness) = pair();
        let exchange = exchange();
        for by in [KillCause::Unload, KillCause::FlowFailure, KillCause::Unload] {
            interrupt(&harness, by);
        }
        let supervisor = ContainmentSupervisor::new(
            Supervision::new(&gate, &exchange),
            Ok(initial),
            Instant::now() + Duration::from_secs(1),
        );
        assert_eq!(supervisor.finish().unwrap(), expected);
    }
}

struct EventsInstrument {
    path: std::path::PathBuf,
    tools: Tools,
}

impl EventsInstrument {
    fn populated() -> Self {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let ordinal = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "weaver-interrupt-events-{}-{ordinal}",
            std::process::id()
        ));
        std::fs::create_dir(&path).unwrap();
        std::fs::write(path.join("cgroup.events"), "populated 1\n").unwrap();
        let tools = Tools::from_owned(std::fs::File::open(&path).unwrap().into());
        Self { path, tools }
    }
}

impl Drop for EventsInstrument {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Repeated interrupt processing cannot create another containment interval.
/// The iteration ceiling also makes a removed expiry check fail finitely.
#[test]
fn repeated_interrupts_do_not_restart_the_containment_deadline() {
    let instrument = EventsInstrument::populated();
    let (gate, harness) = pair();
    let exchange = exchange();
    let start = Instant::now();
    let deadline = start + Duration::from_millis(45);
    let mut supervisor = ContainmentSupervisor::new(
        Supervision::new(&gate, &exchange),
        Ok(Some(KillCause::Unload)),
        deadline,
    );
    let mut observations = 0;
    let mut waits = 0;
    let contained = instrument.tools.wait_empty_until_with(deadline, |slice| {
        observations += 1;
        assert!(
            observations <= 64,
            "an expired deadline must end observation"
        );
        interrupt(&harness, KillCause::Unload);
        supervisor.wait(Duration::ZERO);
        if !slice.is_zero() {
            waits += 1;
            assert!(waits <= 12, "repeated controls cannot renew the deadline");
            std::thread::sleep(slice);
        }
    });
    assert!(
        !contained,
        "a populated group never becomes contained by a control"
    );
    assert!(Instant::now() >= deadline);
    assert!(start.elapsed() < Duration::from_millis(500));
    assert_eq!(supervisor.finish().unwrap(), Some(KillCause::Unload));
}

/// A control protocol fault is retained while populated observation still
/// proceeds. It is never converted into a successful killed outcome.
#[test]
fn invalid_control_during_containment_keeps_cleanup_running() {
    let instrument = EventsInstrument::populated();
    let (gate, harness) = pair();
    let exchange = exchange();
    let deadline = Instant::now() + Duration::from_secs(1);
    let mut supervisor = ContainmentSupervisor::new(
        Supervision::new(&gate, &exchange),
        Ok(Some(KillCause::Unload)),
        deadline,
    );
    interrupt(&harness, KillCause::Fault);
    let mut observations = 0;
    let contained = instrument.tools.wait_empty_until_with(deadline, |slice| {
        observations += 1;
        supervisor.wait(slice);
        if observations == 2 {
            std::fs::write(instrument.path.join("cgroup.events"), "populated 0\n").unwrap();
        }
    });
    assert!(contained);
    assert!(
        observations >= 2,
        "protocol failure does not bypass cleanup"
    );
    assert!(matches!(
        supervisor.finish(),
        Err(ShellEnd::Channel(ChannelFault::Undecodable))
    ));
}

pub(super) fn fill_answer_socket(channel: &Channel) -> usize {
    let envelope = OrganEnvelope {
        exchange: exchange(),
        position: Position::Continue,
        payload: Payload::ToolCancel,
    };
    let mut filled = 0;
    while channel.try_send(&envelope).unwrap() {
        filled += 1;
        assert!(filled < 100_000, "the finite Unix socket must fill");
    }
    assert!(filled > 0);
    filled
}

/// This is the delivery half, handed an already-contained private answer.
/// Kernel containment itself is tested by the delegated tests above.
#[test]
fn socket_pressure_keeps_unsent_unload_revisable_until_actual_send() {
    let (gate, harness) = pair();
    let filled = fill_answer_socket(&gate);
    let (sent, sent_result) = sync_channel(1);
    std::thread::scope(|scope| {
        let worker = scope.spawn(|| {
            sent.send(super::send_answer(
                &gate,
                &exchange(),
                ExecutionAnswer {
                    outcome: ToolOutcome::Killed {
                        partial: Some("partial".into()),
                        by: KillCause::Unload,
                    },
                    containment_deadline: Some(Instant::now() + Duration::from_secs(2)),
                },
                &BTreeSet::new(),
            ))
            .unwrap();
        });
        interrupt(&harness, KillCause::Unload);
        interrupt(&harness, KillCause::FlowFailure);
        assert!(matches!(sent_result.try_recv(), Err(TryRecvError::Empty)));
        for _ in 0..filled {
            assert_eq!(harness.recv().unwrap().payload, Payload::ToolCancel);
        }
        let terminal = harness.recv().unwrap();
        assert_eq!(terminal.exchange, exchange());
        assert_eq!(terminal.position, Position::Close);
        assert_eq!(
            terminal.payload,
            Payload::ToolAnswer(ToolOutcome::Killed {
                partial: Some("partial".into()),
                by: KillCause::FlowFailure,
            })
        );
        sent_result
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .unwrap();
        worker.join().unwrap();
    });
}

/// Leaving the peer's receive queue full must consume the original bound,
/// never a fresh delivery clock or a blocking send outside that bound.
#[test]
fn a_backpressured_answer_expires_at_its_original_containment_deadline() {
    let (gate, harness) = pair();
    let filled = fill_answer_socket(&gate);
    let deadline = Instant::now() + Duration::from_millis(40);
    interrupt(&harness, KillCause::Unload);
    interrupt(&harness, KillCause::FlowFailure);
    let result = super::send_answer(
        &gate,
        &exchange(),
        ExecutionAnswer {
            outcome: ToolOutcome::Killed {
                partial: None,
                by: KillCause::Unload,
            },
            containment_deadline: Some(deadline),
        },
        &BTreeSet::new(),
    );
    assert!(matches!(
        result,
        Err(Unanswered::DeliveryBound {
            exchange: failed,
            answer: UndeliveredAnswer::ToolAnswer,
        }) if failed == exchange()
    ));
    assert!(Instant::now() >= deadline);
    for _ in 0..filled {
        assert_eq!(harness.recv().unwrap().payload, Payload::ToolCancel);
    }
    use nix::poll::{PollFd, PollFlags, poll};
    let mut fds = [PollFd::new(harness.as_fd(), PollFlags::POLLIN)];
    assert_eq!(poll(&mut fds, 0u16).unwrap(), 0, "no terminal was sent");
}

#[test]
fn an_ordinary_result_is_not_rewritten_while_waiting_to_send() {
    let (gate, harness) = pair();
    interrupt(&harness, KillCause::FlowFailure);
    let outcome = ToolOutcome::Result {
        content: "finished".into(),
    };
    super::send_answer(
        &gate,
        &exchange(),
        ExecutionAnswer {
            outcome: outcome.clone(),
            containment_deadline: Some(Instant::now() + Duration::from_secs(1)),
        },
        &BTreeSet::new(),
    )
    .unwrap();
    assert_eq!(
        harness.recv().unwrap().payload,
        Payload::ToolAnswer(outcome)
    );
}

pub(super) fn assert_no_peer_answer(peer: &Channel) {
    use nix::poll::{PollFd, PollFlags, poll};
    let mut fds = [PollFd::new(peer.as_fd(), PollFlags::POLLIN)];
    assert_eq!(
        poll(&mut fds, 0u16).unwrap(),
        0,
        "no extra answer is emitted"
    );
}

/// The active B call does not create a second response to controls for an
/// already answered A call. Its own matching interrupt still takes effect.
#[test]
fn late_answered_controls_are_drained_while_a_later_call_runs() {
    let (gate, harness) = pair();
    let answered = BTreeSet::from([1]);
    let mut current = exchange();
    current.ordinal = 2;
    for payload in [
        Payload::ToolCancel,
        Payload::ToolInterrupt {
            by: KillCause::Unload,
        },
        Payload::ToolInterrupt {
            by: KillCause::FlowFailure,
        },
    ] {
        harness
            .send(&OrganEnvelope {
                exchange: exchange(),
                position: Position::Continue,
                payload,
            })
            .unwrap();
    }
    harness
        .send(&OrganEnvelope {
            exchange: current.clone(),
            position: Position::Continue,
            payload: Payload::ToolInterrupt {
                by: KillCause::FlowFailure,
            },
        })
        .unwrap();
    let mut reader = Supervision::with_answered(&gate, &current, Some(&answered));
    for _ in 0..3 {
        assert_eq!(reader.wait(Duration::ZERO), Ok(None));
        assert!(reader.pending_refusal.is_none());
    }
    assert_no_peer_answer(&harness);
    assert_eq!(
        reader.wait(Duration::ZERO),
        Ok(Some(KillCause::FlowFailure))
    );
}

#[test]
fn an_invalid_cause_for_an_answered_call_still_faults_the_protocol() {
    let (gate, harness) = pair();
    let answered = BTreeSet::from([1]);
    let mut current = exchange();
    current.ordinal = 2;
    interrupt(&harness, KillCause::Clock);
    let mut reader = Supervision::with_answered(&gate, &current, Some(&answered));
    assert_eq!(reader.wait(Duration::ZERO), Err(ChannelFault::Undecodable));
}

#[test]
fn answered_ordinal_does_not_hide_foreign_opener_or_wrong_position() {
    let answered = BTreeSet::from([1]);
    let mut current = exchange();
    current.ordinal = 2;
    let mut foreign = exchange();
    foreign.opener = weaver_types::Opener::Gate;
    for (source, position) in [
        (foreign, Position::Continue),
        (exchange(), Position::Open),
        (exchange(), Position::Close),
    ] {
        let (gate, harness) = pair();
        harness
            .send(&OrganEnvelope {
                exchange: source.clone(),
                position,
                payload: Payload::ToolInterrupt {
                    by: KillCause::Unload,
                },
            })
            .unwrap();
        let mut reader = Supervision::with_answered(&gate, &current, Some(&answered));
        assert_eq!(reader.wait(Duration::ZERO), Ok(None));
        assert!(reader.pending_refusal.is_some());
        assert_eq!(reader.wait(Duration::ZERO), Ok(None));
        assert_eq!(
            harness.recv().unwrap(),
            OrganEnvelope {
                exchange: source,
                position: Position::Close,
                payload: Payload::Refusal(LifecycleRefusal::OutOfOrder),
            }
        );
    }
}

#[test]
fn a_later_answer_sends_no_second_outcome_for_earlier_late_controls() {
    let (gate, harness) = pair();
    let answered = BTreeSet::from([1]);
    let mut current = exchange();
    current.ordinal = 2;
    interrupt(&harness, KillCause::Unload);
    interrupt(&harness, KillCause::FlowFailure);
    let outcome = ToolOutcome::Result {
        content: "B finished".into(),
    };
    super::send_answer(
        &gate,
        &current,
        ExecutionAnswer {
            outcome: outcome.clone(),
            containment_deadline: Some(Instant::now() + Duration::from_secs(1)),
        },
        &answered,
    )
    .unwrap();
    assert_eq!(
        harness.recv().unwrap(),
        OrganEnvelope {
            exchange: current,
            position: Position::Close,
            payload: Payload::ToolAnswer(outcome),
        }
    );
    assert_no_peer_answer(&harness);
}
