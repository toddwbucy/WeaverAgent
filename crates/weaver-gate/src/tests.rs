use super::*;
use std::collections::BTreeSet;
use weaver_types::{AccessRule, AgentName, ExchangeId, GateInstruction};

/// conforms: gate-cancel-past-the-answer-is-dropped
/// The real served channel must not insert a refusal before the next
/// execution's answer. Removing only the late-cancel drop fails this.
/// **The unload's interrupt crossing the answer is dropped the same way**
/// (the first outcome wins, `weaver-harness-gate-contract` section 2):
/// the result already sent stands, and no `Killed` and no refusal follow
/// it. Removing the late-interrupt drop fails the third execution.
#[test]
#[ignore = "runs a shell: needs a delegated cgroup scope"]
fn a_cancel_past_the_answer_is_dropped_in_tools() {
    use nix::sys::socket::{MsgFlags, recv, send};
    use std::os::fd::AsRawFd;
    if std::env::var_os("WEAVER_LATE_CANCEL_CHILD").is_some() {
        let channel = channel::adopt().expect("inherited channel");
        weaver_gate::tools::cgroup::adopt().expect("the scratch tools/ at 4");
        assert_eq!(serve(channel), ExitCode::SUCCESS);
        return;
    }
    let (harness, child_end) = test_common::seqpacket_pair();
    test_common::bound_receives(&harness, 5);
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "tests::a_cancel_past_the_answer_is_dropped_in_tools",
            "--include-ignored",
            "--nocapture",
        ])
        .env("WEAVER_LATE_CANCEL_CHILD", "1");
    let tools = test_common::ScratchTools::new();
    test_common::place_inherited(
        &mut command,
        &[child_end.as_raw_fd(), tools.dir.as_raw_fd()],
    );
    let mut child = command.spawn().unwrap();
    drop(child_end);
    let late_cancel = scratch("late-cancel");
    let (_shell, grant) = granted_shell("late-cancel");
    let ready = test_common::ask(
        &harness,
        1,
        LifecycleDirective::Raise {
            instruction: instruction(),
            socket: late_cancel.to_path_buf(),
            shell: Some(grant),
        },
    );
    assert_eq!(ready.payload, Payload::Answer(LifecycleAnswer::GateReady));
    let write = |envelope: &OrganEnvelope| {
        send(
            harness.as_raw_fd(),
            &serde_json::to_vec(envelope).unwrap(),
            MsgFlags::empty(),
        )
        .unwrap();
    };
    let read = || {
        let mut buffer = vec![0; weaver_types::MAX_ENVELOPE_BYTES];
        let n = recv(harness.as_raw_fd(), &mut buffer, MsgFlags::empty()).unwrap();
        serde_json::from_slice::<OrganEnvelope>(&buffer[..n]).unwrap()
    };
    for ordinal in [2, 3, 4] {
        let exchange = weaver_types::ExchangeId {
            opener: Opener::Harness,
            ordinal,
        };
        write(&OrganEnvelope {
            exchange: exchange.clone(),
            position: Position::Open,
            payload: Payload::Tool(weaver_types::ToolExecution {
                name: weaver_types::ToolName("sh".into()),
                arguments: r#"{"command":"echo completed"}"#.into(),
                clock_ms: 1_000,
            }),
        });
        let answer = read();
        assert_eq!(answer.exchange, exchange);
        assert!(
            matches!(
                answer.payload,
                Payload::ToolAnswer(weaver_types::ToolOutcome::Result { .. })
            ),
            "{answer:?}"
        );
        let late = match ordinal {
            2 => Some(Payload::ToolCancel),
            3 => Some(Payload::ToolInterrupt),
            _ => None,
        };
        if let Some(payload) = late {
            write(&OrganEnvelope {
                exchange,
                position: Position::Continue,
                payload,
            });
        }
    }
    // Other misplaced payloads and positions still refuse.
    for (position, payload) in [
        (
            Position::Continue,
            Payload::Directive(LifecycleDirective::Stop {
                cause: weaver_types::Cause { uid: 0 },
            }),
        ),
        (Position::Open, Payload::ToolCancel),
        (Position::Open, Payload::ToolInterrupt),
    ] {
        write(&OrganEnvelope {
            exchange: weaver_types::ExchangeId {
                opener: Opener::Harness,
                ordinal: 5,
            },
            position,
            payload,
        });
        assert_eq!(
            read().payload,
            Payload::Refusal(LifecycleRefusal::OutOfOrder)
        );
    }
    drop(harness);
    assert!(test_common::wait_bounded(&mut child, 5, "late-cancel child").success());
}

/// **The unload's interrupt, then the quiesce, through the served loop**
/// (`weaver-gate-Spec` section 9's enforcement bullet, the unload's
/// interrupt and the first outcome; section 4, the quiesce): a `Quiesce`
/// read while an execution is open is answered `OutOfOrder` and the
/// execution carries on; `ToolInterrupt` ends it `Killed { by: unload }`;
/// and the `Quiesce` after it answers `GateQuiesced`. Perturbation: let the
/// supervision end the execution on any message, or name the interrupt a
/// cancel, and this fails.
#[test]
#[ignore = "runs a shell: needs a delegated cgroup scope"]
fn the_interrupt_then_the_quiesce_through_the_served_loop_in_tools() {
    use nix::sys::socket::{MsgFlags, recv, send};
    use std::os::fd::AsRawFd;
    if std::env::var_os("WEAVER_INTERRUPT_QUIESCE_CHILD").is_some() {
        let channel = channel::adopt().expect("inherited channel");
        weaver_gate::tools::cgroup::adopt().expect("the scratch tools/ at 4");
        assert_eq!(serve(channel), ExitCode::SUCCESS);
        return;
    }
    let (harness, child_end) = test_common::seqpacket_pair();
    test_common::bound_receives(&harness, 10);
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "tests::the_interrupt_then_the_quiesce_through_the_served_loop_in_tools",
            "--include-ignored",
            "--nocapture",
        ])
        .env("WEAVER_INTERRUPT_QUIESCE_CHILD", "1");
    let tools = test_common::ScratchTools::new();
    test_common::place_inherited(
        &mut command,
        &[child_end.as_raw_fd(), tools.dir.as_raw_fd()],
    );
    let mut child = command.spawn().unwrap();
    drop(child_end);
    let socket = scratch("interrupt-quiesce");
    let (_shell, grant) = granted_shell("interrupt-quiesce");
    let ready = test_common::ask(
        &harness,
        1,
        LifecycleDirective::Raise {
            instruction: instruction(),
            socket: socket.to_path_buf(),
            shell: Some(grant),
        },
    );
    assert_eq!(ready.payload, Payload::Answer(LifecycleAnswer::GateReady));
    let write = |envelope: &OrganEnvelope| {
        send(
            harness.as_raw_fd(),
            &serde_json::to_vec(envelope).unwrap(),
            MsgFlags::empty(),
        )
        .unwrap();
    };
    let read = || {
        let mut buffer = vec![0; weaver_types::MAX_ENVELOPE_BYTES];
        let n = recv(harness.as_raw_fd(), &mut buffer, MsgFlags::empty()).unwrap();
        serde_json::from_slice::<OrganEnvelope>(&buffer[..n]).unwrap()
    };
    let execution = ExchangeId {
        opener: Opener::Harness,
        ordinal: 2,
    };
    write(&OrganEnvelope {
        exchange: execution.clone(),
        position: Position::Open,
        payload: Payload::Tool(weaver_types::ToolExecution {
            name: weaver_types::ToolName("sh".into()),
            // A loop of builtins: the confined set holds no `sleep`.
            arguments: r#"{"command":"while :; do :; done"}"#.into(),
            clock_ms: 8_000,
        }),
    });
    std::thread::sleep(std::time::Duration::from_millis(100));
    let early = ExchangeId {
        opener: Opener::Harness,
        ordinal: 3,
    };
    write(&OrganEnvelope {
        exchange: early.clone(),
        position: Position::Open,
        payload: Payload::Directive(LifecycleDirective::Quiesce),
    });
    let refused = read();
    assert_eq!(refused.exchange, early);
    assert_eq!(
        refused.payload,
        Payload::Refusal(LifecycleRefusal::OutOfOrder),
        "a quiesce while the execution is open"
    );
    write(&OrganEnvelope {
        exchange: execution.clone(),
        position: Position::Continue,
        payload: Payload::ToolInterrupt,
    });
    let killed = read();
    assert_eq!(
        killed.exchange, execution,
        "the execution carried on until the interrupt"
    );
    assert!(
        matches!(
            killed.payload,
            Payload::ToolAnswer(weaver_types::ToolOutcome::Killed {
                by: weaver_types::KillCause::Unload,
                ..
            })
        ),
        "{killed:?}"
    );
    let quiesced = test_common::ask(&harness, 4, LifecycleDirective::Quiesce);
    assert_eq!(
        quiesced.payload,
        Payload::Answer(LifecycleAnswer::GateQuiesced)
    );
    drop(harness);
    assert!(test_common::wait_bounded(&mut child, 10, "interrupt-quiesce child").success());
}

fn opened(directive: LifecycleDirective) -> OrganEnvelope {
    OrganEnvelope {
        exchange: ExchangeId {
            opener: Opener::Harness,
            ordinal: 1,
        },
        position: Position::Open,
        payload: Payload::Directive(directive),
    }
}

fn instruction() -> GateInstruction {
    GateInstruction {
        access_rule: AccessRule {
            allowed_uids: BTreeSet::new(),
            allowed_gids: BTreeSet::new(),
            denied_uids: BTreeSet::new(),
        },
    }
}

/// **A real grant for the served-loop tests** (#29): the static busybox
/// `WEAVER_BUSYBOX` names, copied into a scratch `libexec/`, and a scratch
/// `commands/` holding the default applets and no `sh`, as admin builds it.
/// The served gate builds its confinement from it at the raise, under this
/// suite's uid, so its home is that uid's and `.tmp` is made there if absent.
fn granted_shell(name: &str) -> (Scratch, weaver_types::ShellGrant) {
    let busybox = std::env::var_os("WEAVER_BUSYBOX")
        .map(std::path::PathBuf::from)
        .expect("WEAVER_BUSYBOX names a static busybox built from the pinned configuration");
    let layout = scratch(&format!("{name}-shell"));
    let binary = layout.dir.join("busybox");
    std::fs::copy(&busybox, &binary).expect("the busybox copies");
    let commands = layout.dir.join("commands");
    std::fs::create_dir_all(&commands).unwrap();
    for applet in [
        "ls", "cat", "head", "tail", "wc", "grep", "sort", "uniq", "cut", "tr", "date", "echo",
        "pwd", "mktemp",
    ] {
        std::os::unix::fs::symlink(&binary, commands.join(applet)).unwrap();
    }
    (layout, weaver_types::ShellGrant { binary, commands })
}

/// A socket path inside a directory of its own, the directory removed
/// when the test ends, pass or fail: the guard drops on the unwind a
/// failed assertion takes as on a clean return (#690 item C2.9). It reads
/// as the socket's path, and a caller handing the path to a directive
/// holds the guard for as long as the socket is wanted.
struct Scratch {
    dir: std::path::PathBuf,
    file: std::path::PathBuf,
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

impl std::ops::Deref for Scratch {
    type Target = std::path::Path;
    fn deref(&self) -> &std::path::Path {
        &self.file
    }
}

impl AsRef<std::path::Path> for Scratch {
    fn as_ref(&self) -> &std::path::Path {
        &self.file
    }
}

/// A scratch socket path, pre-cleaned, matching the boundary and entry
/// helpers so a previous run's leftover cannot make a raise refuse.
fn scratch(name: &str) -> Scratch {
    let dir = std::env::temp_dir().join(format!("weaver-gate-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch dir");
    let path = dir.join("gate.sock");
    std::fs::remove_file(&path).ok();
    Scratch { dir, file: path }
}

/// **A lower before any raise answers `OutOfOrder`.** The order is judged
/// against the position before the hook is touched.
///
/// Perturbation: give `(BeforeRaise, Lower)` an arm that answers
/// `GateStopped` and this test fails. Watched under exactly that addition.
#[test]
fn a_lower_before_any_raise_answers_out_of_order() {
    let mut state = HookState::BeforeRaise;
    assert_eq!(
        dispatch(&mut state, &opened(LifecycleDirective::Lower)),
        Payload::Refusal(LifecycleRefusal::OutOfOrder)
    );
    assert!(matches!(state, HookState::BeforeRaise), "and is not queued");
}

/// **The lowered position is terminal.** A directive of any kind arriving
/// after a lower answers `OutOfOrder`.
#[test]
fn any_directive_after_a_lower_answers_out_of_order() {
    let mut state = HookState::Lowered;
    let terminal = scratch("terminal");
    for case in [
        LifecycleDirective::Raise {
            instruction: instruction(),
            socket: terminal.to_path_buf(),
            shell: None,
        },
        LifecycleDirective::Lower,
        LifecycleDirective::Stop {
            cause: weaver_types::Cause { uid: 0 },
        },
    ] {
        assert_eq!(
            dispatch(&mut state, &opened(case)),
            Payload::Refusal(LifecycleRefusal::OutOfOrder)
        );
        assert!(
            matches!(state, HookState::Lowered),
            "terminal stays terminal"
        );
    }
}

/// **A raise answers ready, and a lower after it answers stopped**, each
/// once, in order.
#[test]
fn raise_then_lower_walks_before_raise_raised_and_lowered() {
    let path = scratch("walk");
    let mut state = HookState::BeforeRaise;

    let ready = dispatch(
        &mut state,
        &opened(LifecycleDirective::Raise {
            instruction: instruction(),
            socket: path.to_path_buf(),
            shell: None,
        }),
    );
    assert_eq!(ready, Payload::Answer(LifecycleAnswer::GateReady));
    assert!(matches!(state, HookState::Raised(_, _)));
    assert!(path.exists(), "ready is a fact about a bound listener");

    // A second raise is out of order even though the first succeeded.
    assert_eq!(
        dispatch(
            &mut state,
            &opened(LifecycleDirective::Raise {
                instruction: instruction(),
                socket: path.to_path_buf(),
                shell: None,
            }),
        ),
        Payload::Refusal(LifecycleRefusal::OutOfOrder)
    );

    let stopped = dispatch(&mut state, &opened(LifecycleDirective::Lower));
    assert_eq!(stopped, Payload::Answer(LifecycleAnswer::GateStopped));
    assert!(matches!(state, HookState::Lowered));
    // **This crate unlinks nothing**: the path is the operator's artifact
    // and survives the lower.
    assert!(
        path.exists(),
        "the lower closes the listener and unlinks nothing"
    );
    std::fs::remove_file(&path).ok();
}

/// **A quiesce from the raised position closes the listener and drains**
/// (`weaver-gate-Spec` section 4; `weaver-admin-Spec` 3.0 graceful step
/// 2): it answers `GateQuiesced`, the position is draining, and a dial
/// finds no listener. Perturbation: answer `GateQuiesced` leaving the
/// position raised, and the position and the dial both fail.
#[test]
fn a_quiesce_from_raised_closes_the_listener_and_drains() {
    let path = scratch("quiesce");
    let mut state = HookState::BeforeRaise;
    dispatch(
        &mut state,
        &opened(LifecycleDirective::Raise {
            instruction: instruction(),
            socket: path.to_path_buf(),
            shell: None,
        }),
    );
    assert!(matches!(state, HookState::Raised(_, _)));
    assert_eq!(
        dispatch(&mut state, &opened(LifecycleDirective::Quiesce)),
        Payload::Answer(LifecycleAnswer::GateQuiesced)
    );
    assert!(matches!(state, HookState::Draining(_, None)));
    // A parallel test in this binary that spawns a child holds a copy of
    // every descriptor between its fork and its exec, the listener's
    // among them, so one dial in that window still connects; the gate
    // itself forks only for an execution, never during a quiesce. The
    // listener is closed once a dial is refused, within a bound.
    let refused = (0..50).any(|_| {
        let refused = std::os::unix::net::UnixStream::connect(&*path).is_err();
        if !refused {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        refused
    });
    assert!(refused, "no new connection is accepted from the quiesce");
    std::fs::remove_file(&*path).ok();
}

/// **A lower from draining is answered last, and a second one is out of
/// order** (`weaver-gate-Spec` section 4): the first is deferred, the
/// position holding its exchange and the settle instant one margin inside
/// the lower bound; a second while it waits answers `OutOfOrder` and
/// leaves the first's exchange in place, one `Lower` per leave (the
/// harness never sends two). Perturbation: answer the first at once, or
/// let the second replace the first's exchange, and this fails.
#[test]
fn a_lower_from_draining_is_deferred_and_a_second_is_out_of_order() {
    let mut state = HookState::Draining(Box::new(Relay::new()), None);
    let heard = std::time::Instant::now();
    assert!(matches!(
        dispatch(&mut state, &opened(LifecycleDirective::Lower)),
        Dispatched::Deferred
    ));
    let HookState::Draining(_, Some(stopping)) = &state else {
        panic!("the lower waits in the draining position");
    };
    assert_eq!(stopping.exchange.ordinal, 1);
    let settle = std::time::Duration::from_millis(
        weaver_types::LOWER_BOUND_MS - weaver_types::LOWER_SETTLE_MARGIN_MS,
    );
    assert!(stopping.deadline >= heard + settle);
    assert!(
        stopping.deadline < heard + std::time::Duration::from_millis(weaver_types::LOWER_BOUND_MS)
    );

    let mut second = opened(LifecycleDirective::Lower);
    second.exchange.ordinal = 2;
    assert_eq!(
        dispatch(&mut state, &second),
        Payload::Refusal(LifecycleRefusal::OutOfOrder)
    );
    let HookState::Draining(_, Some(stopping)) = &state else {
        panic!("still waiting");
    };
    assert_eq!(
        stopping.exchange.ordinal, 1,
        "the first lower's exchange stands"
    );
}

/// **The lower's settle instant runs from when it was heard** (Codex on
/// #114): a deferral judged after a slow step still answers by the
/// instant measured from the read, never from the judgment. Perturbation:
/// take the instant at the judgment, and the deadline moves late.
#[test]
fn the_settle_instant_runs_from_when_the_lower_was_heard() {
    let heard = std::time::Instant::now() - std::time::Duration::from_secs(5);
    let mut state = HookState::Draining(Box::new(Relay::new()), None);
    assert!(matches!(
        dispatch_at(&mut state, &opened(LifecycleDirective::Lower), heard),
        Dispatched::Deferred
    ));
    let HookState::Draining(_, Some(stopping)) = &state else {
        panic!("the lower waits");
    };
    assert_eq!(stopping.deadline, settle_instant(heard));
    assert_eq!(
        settle_instant(heard) - heard,
        std::time::Duration::from_millis(
            weaver_types::LOWER_BOUND_MS - weaver_types::LOWER_SETTLE_MARGIN_MS
        )
    );
}

/// **A quiesce anywhere but raised is out of order** (`weaver-gate-Spec`
/// section 4), not queued: before a raise, a second one while draining,
/// and after a lower. Perturbation: give `(Draining, Quiesce)` an arm
/// answering `GateQuiesced`, and the second case fails.
#[test]
fn a_quiesce_outside_the_raised_position_is_out_of_order() {
    for mut state in [
        HookState::BeforeRaise,
        HookState::Draining(Box::new(Relay::new()), None),
        HookState::Lowered,
    ] {
        let before = std::mem::discriminant(&state);
        assert_eq!(
            dispatch(&mut state, &opened(LifecycleDirective::Quiesce)),
            Payload::Refusal(LifecycleRefusal::OutOfOrder)
        );
        assert_eq!(std::mem::discriminant(&state), before, "and is not queued");
    }
}

/// **A refused raise leaves nothing held.** The position is unchanged, so
/// the aggregate's rollback has nothing of this crate's to unwind, and the
/// refusal is answered rather than exited on.
///
/// Perturbation: move `*position = HookState::Raised(..)` before the bind's
/// result is judged and this test fails. Watched under exactly that move.
#[test]
fn a_refused_raise_holds_nothing_and_is_answered() {
    // A path inside a directory that does not exist cannot be bound. Built
    // from this test's own scratch directory with a missing component in
    // the middle, so the fixture does not rest on `/nonexistent` being
    // absent on whatever machine runs it.
    let unbindable = scratch("refused")
        .parent()
        .expect("the scratch dir")
        .join("no-such-directory")
        .join("gate.sock");
    let mut state = HookState::BeforeRaise;
    assert_eq!(
        dispatch(
            &mut state,
            &opened(LifecycleDirective::Raise {
                instruction: instruction(),
                socket: unbindable.clone(),
                shell: None,
            }),
        ),
        Payload::Refusal(LifecycleRefusal::BindFailed),
        "the refusal is answered, never exited on"
    );
    assert!(
        matches!(state, HookState::BeforeRaise),
        "and nothing is held: no listener, no half-bound socket"
    );
}

/// **A grant whose confinement cannot be built refuses the raise
/// `BoundaryUnverified`, with nothing held** (#29, `weaver-gate-Spec`
/// sections 3 and 8.1): the confinement is built before the bind, so a
/// busybox path that does not stand leaves no listener and no socket file,
/// and no shell is ever offered unconfined. The control: the same raise with
/// no grant binds and answers ready.
///
/// Perturbation: answer the raise from the bind alone, ignoring the
/// confinement's fault, and the first raise answers ready.
#[test]
fn a_grant_that_cannot_be_confined_refuses_the_raise() {
    let socket = scratch("unconfinable");
    let missing = socket
        .parent()
        .expect("the scratch dir")
        .join("no-such-busybox");
    let mut state = HookState::BeforeRaise;
    let raise = |shell| {
        opened(LifecycleDirective::Raise {
            instruction: instruction(),
            socket: socket.to_path_buf(),
            shell,
        })
    };
    assert_eq!(
        dispatch(
            &mut state,
            &raise(Some(weaver_types::ShellGrant {
                binary: missing.clone(),
                commands: missing.clone(),
            })),
        ),
        Payload::Refusal(LifecycleRefusal::BoundaryUnverified),
    );
    assert!(matches!(state, HookState::BeforeRaise), "nothing is held");
    assert!(!socket.exists(), "no socket file stands");
    assert_eq!(
        dispatch(&mut state, &raise(None)),
        Payload::Answer(LifecycleAnswer::GateReady),
        "the control: no grant, and the raise binds"
    );
}

/// Every directive the gate does not draw, but `Enter` and `Admit`,
/// whose payloads are the harness's and the SPU's instructions and
/// which the same arm refuses.
fn undrawn() -> Vec<LifecycleDirective> {
    let cause = weaver_types::Cause { uid: 0 };
    let agent = || AgentName("alpha".into());
    vec![
        LifecycleDirective::Leave {
            cause,
            forced: false,
            rollback: false,
        },
        LifecycleDirective::Leave {
            cause,
            forced: true,
            rollback: true,
        },
        LifecycleDirective::JoinLeave { cause },
        LifecycleDirective::SavePoint { cause },
        LifecycleDirective::Stop { cause },
        LifecycleDirective::Observe,
        LifecycleDirective::Release,
        LifecycleDirective::Load { agent: agent() },
        LifecycleDirective::Unload { agent: agent() },
        LifecycleDirective::Validate { agent: agent() },
        LifecycleDirective::Show { agent: agent() },
        LifecycleDirective::SavePointVerb { agent: agent() },
        LifecycleDirective::Restore { agent: agent() },
        LifecycleDirective::ForceUnload { agent: agent() },
    ]
}

/// **Everything outside the drawn vocabulary refuses as out of order,
/// before the raise and while raised** (#99 area 3, G01 and G02): a
/// raised gate that lowered on a `Leave` or a `ForceUnload`, or answered
/// a `SavePoint` or a `Restore`, would close the world's door on a
/// directive the contract never gave it. The position is unchanged after
/// each. Perturbation: give `(Raised, Leave)` the lower's arm, or answer
/// `SavePoint` with `GateStopped`, and this fails.
#[test]
fn a_directive_outside_the_drawn_vocabulary_refuses() {
    let mut state = HookState::BeforeRaise;
    for case in undrawn() {
        let named = format!("{case:?}");
        assert_eq!(
            dispatch(&mut state, &opened(case)),
            Payload::Refusal(LifecycleRefusal::OutOfOrder),
            "{named} before the raise"
        );
        assert!(
            matches!(state, HookState::BeforeRaise),
            "{named}: not queued"
        );
    }
    let socket = scratch("vocabulary");
    assert_eq!(
        dispatch(
            &mut state,
            &opened(LifecycleDirective::Raise {
                instruction: instruction(),
                socket: socket.to_path_buf(),
                shell: None,
            })
        ),
        Payload::Answer(LifecycleAnswer::GateReady)
    );
    for case in undrawn() {
        let named = format!("{case:?}");
        assert_eq!(
            dispatch(&mut state, &opened(case)),
            Payload::Refusal(LifecycleRefusal::OutOfOrder),
            "{named} while raised"
        );
        assert!(
            matches!(state, HookState::Raised(_, _)),
            "{named}: the gate stays raised"
        );
    }
    assert_eq!(
        dispatch(&mut state, &opened(LifecycleDirective::Lower)),
        Payload::Answer(LifecycleAnswer::GateStopped)
    );
}

/// A mis-shapen exchange is refused before the directive's case is read.
#[test]
fn a_mis_shapen_exchange_refuses_before_the_directive_runs() {
    let mut state = HookState::BeforeRaise;
    let misshapen = scratch("misshapen");
    let mut closing = opened(LifecycleDirective::Raise {
        instruction: instruction(),
        socket: misshapen.to_path_buf(),
        shell: None,
    });
    closing.position = Position::Close;
    assert_eq!(
        dispatch(&mut state, &closing),
        Payload::Refusal(LifecycleRefusal::OutOfOrder)
    );
    assert!(matches!(state, HookState::BeforeRaise), "nothing was bound");
}

/// **The handle on `tools/` never reaches a shell** (`weaver-gate-Spec`
/// section 2): entry marks descriptor 4 close-on-exec, so it stays open
/// across the shell's fork for the move and closes at its exec, and the
/// shell's own descriptor table holds no 4. Perturbation: drop the flag's
/// set from the handle's adoption, and the shell lists 4.
#[test]
#[ignore = "runs a shell: needs a delegated cgroup scope"]
fn the_tools_handle_never_reaches_the_shell_in_tools() {
    use nix::sys::socket::{MsgFlags, recv, send};
    use std::os::fd::AsRawFd;
    if std::env::var_os("WEAVER_HANDLE_SHELL_CHILD").is_some() {
        let channel = channel::adopt().expect("inherited channel");
        weaver_gate::tools::cgroup::adopt().expect("the scratch tools/ at 4");
        assert_eq!(serve(channel), ExitCode::SUCCESS);
        return;
    }
    let (harness, child_end) = test_common::seqpacket_pair();
    test_common::bound_receives(&harness, 10);
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "tests::the_tools_handle_never_reaches_the_shell_in_tools",
            "--include-ignored",
            "--nocapture",
        ])
        .env("WEAVER_HANDLE_SHELL_CHILD", "1");
    let tools = test_common::ScratchTools::new();
    test_common::place_inherited(
        &mut command,
        &[child_end.as_raw_fd(), tools.dir.as_raw_fd()],
    );
    let mut child = command.spawn().unwrap();
    drop(child_end);
    let socket = scratch("handle-shell");
    let (_shell, grant) = granted_shell("handle-shell");
    let ready = test_common::ask(
        &harness,
        1,
        LifecycleDirective::Raise {
            instruction: instruction(),
            socket: socket.to_path_buf(),
            shell: Some(grant),
        },
    );
    assert_eq!(ready.payload, Payload::Answer(LifecycleAnswer::GateReady));
    let execution = ExchangeId {
        opener: Opener::Harness,
        ordinal: 2,
    };
    send(
        harness.as_raw_fd(),
        &serde_json::to_vec(&OrganEnvelope {
            exchange: execution.clone(),
            position: Position::Open,
            payload: Payload::Tool(weaver_types::ToolExecution {
                name: weaver_types::ToolName("sh".into()),
                // The wall grants no `/proc`, so the shell asks the
                // descriptor itself: a redirect from 4 fails where 4 is
                // closed, and the control opens 4 and sees it. `true`, not
                // the special builtin `:`, whose failed redirect would end
                // a POSIX shell's script.
                arguments: r#"{"command":"if true <&4 2>/dev/null; then echo has4; else echo no4; fi; exec 4</dev/null; if true <&4; then echo control4; fi; cat /etc/hostname >/dev/null 2>&1; echo etc=$?"}"#.into(),
                clock_ms: 2_000,
            }),
        })
        .unwrap(),
        MsgFlags::empty(),
    )
    .unwrap();
    let mut buffer = vec![0; weaver_types::MAX_ENVELOPE_BYTES];
    let n = recv(harness.as_raw_fd(), &mut buffer, MsgFlags::empty()).unwrap();
    let answer = serde_json::from_slice::<OrganEnvelope>(&buffer[..n]).unwrap();
    drop(harness);
    assert!(test_common::wait_bounded(&mut child, 5, "handle-shell child").success());
    let Payload::ToolAnswer(weaver_types::ToolOutcome::Result { content }) = answer.payload else {
        panic!("a result: {answer:?}");
    };
    let listed: Vec<&str> = content.lines().collect();
    assert!(listed.contains(&"no4"), "the shell holds no 4: {listed:?}");
    assert!(
        !listed.contains(&"has4"),
        "the shell holds no 4: {listed:?}"
    );
    assert!(
        listed.contains(&"control4"),
        "the check sees a 4 the shell opened: {listed:?}"
    );
    // **The served shell runs under its layers** (#29): the wall grants no
    // `/etc`, so a read there fails in the execution itself.
    assert!(
        listed.contains(&"etc=1"),
        "the wall holds in the served shell: {listed:?}"
    );
}
