//! Actual context/transport failures and actual fork custody. Protocol children
//! are test doubles, never an STE100 adapter or production success path.
use super::*;
use crate::flow::clock::{ReadyFd, testing::FakePumpIo};
use crate::flow::context::tests::{Fixture, config, event};
use crate::flow::pump::LifecyclePorts;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

pub(in crate::flow::context) fn take_context(fx: &mut Fixture) -> RecordingContext {
    // Fixture has a Drop guard. Replace it with a separate unstarted context,
    // never move an owner out through unsafe code or erase its original fields.
    let (sink, _reader) = UnixStream::pair().unwrap();
    let (state, _peer) = UnixStream::pair().unwrap();
    let replacement = RecordingContext::new(
        sink.into(),
        StateChannel::new(
            state,
            "replacement-session".into(),
            "replacement-run".into(),
        )
        .unwrap(),
        "replacement-session".into(),
        "replacement-run".into(),
        42,
        config(),
        AbsoluteBound::after(1, 10).unwrap(),
        &mut FakePumpIo::new(1),
    )
    .unwrap();
    std::mem::replace(&mut fx.context, replacement)
}

pub(in crate::flow::context) fn state_eof(fx: &mut Fixture) -> RunFailure {
    fx.state.shutdown(std::net::Shutdown::Write).unwrap();
    for _ in 0..32 {
        let now = fx.clock.now().unwrap() + 1;
        fx.clock
            .schedule(
                now,
                vec![ReadyFd {
                    index: 0,
                    events: PollFlags::POLLHUP,
                }],
            )
            .unwrap();
        if let Err(failure) = fx.context.pump_once(&mut PumpPorts {
            lifecycle: LifecyclePorts::default(),
            gate: None,
            decode: None,
            label: None,
            io: &mut fx.clock,
        }) {
            assert_eq!(fx.context.phase, RecordingPhase::Failed);
            return failure;
        }
    }
    panic!("real state EOF must latch");
}

pub(in crate::flow::context) fn start(
    fx: &mut Fixture,
    resources: FailedEnterResources,
) -> FailedEnterCleanup {
    match take_context(fx).into_failed_enter_cleanup(resources) {
        Ok(owner) => owner,
        Err(error) => panic!("unexpected failed cleanup refusal: {:?}", error.refusal),
    }
}

pub(in crate::flow::context) fn drive(owner: &mut FailedEnterCleanup, clock: &mut FakePumpIo) {
    for _ in 0..3000 {
        if owner.advance(clock) == FailedEnterProgress::Closed {
            return;
        }
        // Only the fixture yields for its real child/writer; production polling
        // is separately bounded by the retained PumpIo deadline.
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("cleanup did not close");
}

pub(in crate::flow::context) fn records(fx: &mut Fixture) -> Vec<serde_json::Value> {
    fx._reader.set_nonblocking(true).unwrap();
    let mut bytes = Vec::new();
    loop {
        let mut part = [0; 4096];
        match fx._reader.read(&mut part) {
            Ok(0) => break,
            Ok(n) => bytes.extend_from_slice(&part[..n]),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(error) => panic!("trace read: {error}"),
        }
    }
    String::from_utf8(bytes)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn failed_enter_cleanup_state_eof_keeps_first_fault_and_closes_after_one_unload() {
    let mut fx = Fixture::new(config());
    fx.activate();
    let resources = fx.context.failed_enter_resources().unwrap();
    let first = state_eof(&mut fx);
    let origin = fx.context.origin_ns();
    let mut owner = start(&mut fx, resources);
    drive(&mut owner, &mut fx.clock);
    assert_eq!(owner.failure(), &first);
    assert_eq!(owner.context.origin_ns(), origin);
    assert_eq!(owner.context.phase, RecordingPhase::Closed);
    assert_eq!(owner.context.flow.phase(), CoordinatorPhase::Failed);
    assert!(owner.retired.as_ref().unwrap().state.is_some());
    assert!(owner.context.flow.standing_state().is_none());
    assert_eq!(
        fx.state.read(&mut [0]).unwrap(),
        0,
        "retirement must close the descriptor"
    );
    assert!(!owner.abandoned());
    assert!(owner.unload_sequence().is_some());
    let rows = records(&mut fx);
    let faults: Vec<_> = rows.iter().filter(|row| row["kind"] == "fault").collect();
    let unloads: Vec<_> = rows.iter().filter(|row| row["kind"] == "unload").collect();
    assert_eq!(faults.len(), 1);
    assert_eq!(unloads.len(), 1);
    assert_eq!(unloads[0]["payload"]["end_reason"], "component_failure");
    assert_eq!(unloads[0]["payload"]["grant_surface"], "unreadable");
    assert_eq!(
        unloads[0]["payload"]["state_flow_failure"]["fault_sequence"],
        faults[0]["sequence"]
    );
    assert_eq!(
        unloads[0]["payload"]["state_flow_failure"]["account"]["detail"],
        faults[0]["payload"]["account"]["detail"]
    );
    assert_eq!(owner.advance(&mut fx.clock), FailedEnterProgress::Closed);
    assert!(records(&mut fx).is_empty());
    assert!(owner.finish().is_ok());
}

#[test]
fn failed_enter_cleanup_refuses_live_foreign_and_actually_admitted_turn_without_taking_originals() {
    let mut live = Fixture::new(config());
    live.activate();
    let resources = live.context.failed_enter_resources().unwrap();
    let rejected = match take_context(&mut live).into_failed_enter_cleanup(resources) {
        Err(error) => error,
        Ok(_) => panic!("live admitted"),
    };
    assert_eq!(rejected.refusal, FailedEnterRefusal::NotFailed);
    assert!(rejected.resources.matches(&rejected.context));
    let mut fx = Fixture::new(config());
    fx.activate();
    let foreign = rejected.resources;
    state_eof(&mut fx);
    let rejected = match take_context(&mut fx).into_failed_enter_cleanup(foreign) {
        Err(error) => error,
        Ok(_) => panic!("foreign admitted"),
    };
    assert_eq!(rejected.refusal, FailedEnterRefusal::ForeignResources);
    assert!(!rejected.resources.matches(&rejected.context));
    let mut fx = Fixture::new(config());
    fx.activate();
    let mut turn = event(Kind::TurnStarted, None);
    turn.turn = Some(weaver_trace::TurnRef("actual-started-turn".into()));
    fx.author(turn).unwrap();
    let mut closed = event(
        Kind::TurnClosed,
        Some(Payload::TurnClosed(weaver_trace::TurnClose::Clean)),
    );
    closed.turn = Some(weaver_trace::TurnRef("actual-started-turn".into()));
    fx.author(closed).unwrap();
    assert!(fx.context.ever_started_turn);
    assert!(fx.context.open_turn.is_none());
    let resources = fx.context.failed_enter_resources().unwrap();
    state_eof(&mut fx);
    let rejected = match take_context(&mut fx).into_failed_enter_cleanup(resources) {
        Err(error) => error,
        Ok(_) => panic!("turn admitted"),
    };
    assert_eq!(rejected.refusal, FailedEnterRefusal::LiveDuty);
    assert!(rejected.context.ever_started_turn);
    assert!(rejected.context.open_turn.is_none());
}

#[test]
fn failed_enter_cleanup_original_leave_cap_closes_without_reanchoring_or_inventing_unload() {
    let mut fx = Fixture::new(config());
    fx.activate();
    let now = fx.clock.now().unwrap();
    let leave = AbsoluteBound::after(now, 100).unwrap();
    fx.context
        .arm(PumpDeadline::Leave, leave, &mut fx.clock)
        .unwrap();
    let resources = fx.context.failed_enter_resources().unwrap();
    let first = state_eof(&mut fx);
    let mut owner = start(&mut fx, resources);
    assert_eq!(owner.deadline(), Some(leave.deadline_ns()));
    fx.clock.advance_to(leave.deadline_ns()).unwrap();
    assert_eq!(owner.advance(&mut fx.clock), FailedEnterProgress::Closed);
    assert_eq!(owner.failure(), &first);
    assert!(owner.abandoned());
    assert_eq!(owner.deadline(), Some(leave.deadline_ns()));
    assert!(owner.unload_sequence().is_none());
    assert!(owner.context.flow.standing_state().is_none());
    assert!(records(&mut fx).iter().all(|row| row["kind"] != "unload"));
}

pub(in crate::flow::context) fn child_arguments(name: &str) -> Vec<String> {
    vec![
        "--exact".into(),
        format!("flow::context::failed_enter_driver::tests::{name}"),
        "--nocapture".into(),
    ]
}

fn child_mode(name: &str) -> bool {
    let arguments: Vec<_> = std::env::args().collect();
    arguments.iter().any(|argument| argument == "--exact")
        && arguments
            .iter()
            .any(|argument| argument.ends_with(&format!("::{name}")))
}

fn child_release(wrong: u8) {
    let mut bytes = [0u8; 8192];
    // SAFETY: this exec fixture owns the two descriptors placed by the actual
    // producer. It performs only test-peer reads/writes, no handle adoption.
    let received = unsafe { nix::libc::recv(3, bytes.as_mut_ptr().cast(), bytes.len(), 0) };
    assert!(received > 0);
    let request: OrganEnvelope = serde_json::from_slice(&bytes[..received as usize]).unwrap();
    assert!(matches!(
        request.payload,
        weaver_types::Payload::Directive(LifecycleDirective::Release)
    ));
    assert_eq!(request.position, Position::Open);
    // Another fixture's fork may briefly inherit a copy until its exec. Only
    // actual EOF establishes closure; EAGAIN is a bounded wait, never success.
    review_tests::await_decoder_eof(4, Duration::from_secs(3));
    let mut exchange = request.exchange;
    if wrong == 1 {
        exchange.ordinal += 1;
    }
    let reply = serde_json::to_vec(&OrganEnvelope {
        exchange,
        position: if wrong == 2 {
            Position::Continue
        } else {
            Position::Close
        },
        payload: if wrong == 4 {
            weaver_types::Payload::Refusal(weaver_types::LifecycleRefusal::OutOfOrder)
        } else {
            weaver_types::Payload::Answer(if wrong == 3 {
                LifecycleAnswer::Ready
            } else {
                LifecycleAnswer::Released
            })
        },
    })
    .unwrap();
    let sent = unsafe {
        nix::libc::send(
            3,
            reply.as_ptr().cast(),
            reply.len(),
            nix::libc::MSG_NOSIGNAL,
        )
    };
    assert_eq!(sent as usize, reply.len());
}

#[test]
fn failed_enter_process_release_peer() {
    if child_mode("failed_enter_process_release_peer") {
        child_release(0);
    }
}
#[test]
fn failed_enter_process_wrong_release_peer() {
    if child_mode("failed_enter_process_wrong_release_peer") {
        child_release(1);
    }
}

#[test]
fn failed_enter_process_wrong_position_peer() {
    if child_mode("failed_enter_process_wrong_position_peer") {
        child_release(2);
    }
}
#[test]
fn failed_enter_process_wrong_answer_peer() {
    if child_mode("failed_enter_process_wrong_answer_peer") {
        child_release(3);
    }
}

#[test]
fn failed_enter_cleanup_real_fork_release_requires_exact_reply_and_reaps_original_child() {
    for (name, expected) in [
        (
            "failed_enter_process_release_peer",
            ReleaseOutcome::Confirmed,
        ),
        (
            "failed_enter_process_wrong_release_peer",
            ReleaseOutcome::Unconfirmed,
        ),
        (
            "failed_enter_process_wrong_position_peer",
            ReleaseOutcome::Unconfirmed,
        ),
        (
            "failed_enter_process_wrong_answer_peer",
            ReleaseOutcome::Unconfirmed,
        ),
    ] {
        let mut fx = Fixture::new(config());
        fx.activate();
        let mut resources = fx.context.failed_enter_resources().unwrap();
        // SAFETY: this test thread stays alive through the child's actual reap.
        unsafe {
            resources.spawn_spu(
                &mut fx.context,
                &std::env::current_exe().unwrap(),
                &child_arguments(name),
                &mut fx.clock,
            )
        }
        .unwrap();
        let first = state_eof(&mut fx);
        let mut owner = start(&mut fx, resources);
        drive(&mut owner, &mut fx.clock);
        assert_eq!(owner.failure(), &first);
        assert_eq!(owner.release, expected);
        assert!(owner.release_send.as_ref().unwrap().sent());
        assert!(owner.release_reply.is_some());
        assert!(matches!(
            owner.resources.spu.as_ref().unwrap().child.state(),
            ChildState::Reaped(nix::sys::wait::WaitStatus::Exited(_, 0))
        ));
        let spu = owner.resources.spu.as_ref().unwrap();
        assert!(spu.decode.is_none() && spu.residency.is_none());
        assert!(spu.retired_decode.is_some() && spu.retired_residency.is_some());
        assert!(owner.unload_sequence().is_some());
        let rows = records(&mut fx);
        let unloads: Vec<_> = rows.iter().filter(|row| row["kind"] == "unload").collect();
        assert_eq!(unloads.len(), 1);
        assert_eq!(
            unloads[0]["payload"]["release"]["spu"],
            if expected == ReleaseOutcome::Confirmed {
                "confirmed"
            } else {
                "unconfirmed"
            }
        );
        assert_eq!(unloads[0]["payload"]["release"]["member"], "unconfirmed");
    }
}

#[test]
fn failed_enter_cleanup_real_child_at_cap_never_claims_kill_is_reap() {
    let mut fx = Fixture::new(config());
    fx.activate();
    let mut resources = fx.context.failed_enter_resources().unwrap();
    // SAFETY: this test retains the original process owner and reaps it before
    // the forking test thread returns; production exits failed at this boundary.
    unsafe {
        resources.spawn_spu(
            &mut fx.context,
            Path::new("/bin/sleep"),
            &["60".into()],
            &mut fx.clock,
        )
    }
    .unwrap();
    let leave = AbsoluteBound::after(fx.clock.now().unwrap(), 100).unwrap();
    fx.context
        .arm(PumpDeadline::Leave, leave, &mut fx.clock)
        .unwrap();
    state_eof(&mut fx);
    let mut owner = start(&mut fx, resources);
    fx.clock.advance_to(leave.deadline_ns()).unwrap();
    assert_eq!(owner.advance(&mut fx.clock), FailedEnterProgress::Closed);
    assert!(owner.abandoned());
    let child = &owner.resources.spu.as_ref().unwrap().child;
    assert!(child.kill_attempt().is_some());
    assert_ne!(child.state(), ChildState::NotWaitable);
    let mut finished = match owner.finish() {
        Ok(done) => done,
        Err(_) => panic!("not terminal"),
    };
    // Explicitly outside the driver's cap, the test verifies eventual OS reap.
    for _ in 0..3000 {
        if finished.poll_children() {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(!finished.resources().has_unreaped_child());
    assert!(matches!(
        finished.resources().spu.as_ref().unwrap().child.state(),
        ChildState::Reaped(_)
    ));
}

#[test]
fn failed_enter_process_open_then_release_peer() {
    if !child_mode("failed_enter_process_open_then_release_peer") {
        return;
    }
    let mut bytes = [0u8; 8192];
    // SAFETY: fd4 is this exec fixture's actual producer-placed decode endpoint.
    let received = unsafe { nix::libc::recv(4, bytes.as_mut_ptr().cast(), bytes.len(), 0) };
    assert!(received > 0);
    let request: weaver_types::TokenDirective =
        serde_json::from_slice(&bytes[..received as usize]).unwrap();
    let weaver_types::TokenDirective::Open { session, .. } = request else {
        panic!("Open");
    };
    let rendered = "<system>Read caf\u{e9}.</system>";
    let opened = weaver_types::Opened { session, prefix_tokens: 3, capacity: 16384,
        generation_limit: 16, terminal_tokens: 1,
        request: serde_json::value::to_raw_value(&serde_json::json!({"rendered":rendered,"template":"fixture"})).unwrap(),
        measurement: serde_json::value::to_raw_value(&serde_json::json!({"model":"fixture","weights_hash":"fixture",
            "prefix_token_ids":[41,7,89],"blocks":[{"label":"identity-prefix","start":0,"end":rendered.len()}]})).unwrap(),
    };
    let response = serde_json::to_vec(&weaver_types::TokenAnswer::Opened(opened)).unwrap();
    let sent = unsafe {
        nix::libc::send(
            4,
            response.as_ptr().cast(),
            response.len(),
            nix::libc::MSG_NOSIGNAL,
        )
    };
    assert_eq!(sent as usize, response.len());
    child_release(0);
}

#[test]
fn failed_enter_cleanup_actual_classifier_child_eof_retains_admission_and_reaps_without_ready() {
    let (bootstrap, scheme, _) = crate::flow::context::edge_tests::classifier_fixture();
    let mut declaration = config();
    declaration.scheme = scheme;
    let mut fx = Fixture::new(declaration);
    crate::flow::context::edge_tests::activate_classifier_duty(&mut fx, &bootstrap);
    let mut resources = fx.context.failed_enter_resources().unwrap();
    // /bin/true is an explicit process-death instrument, never a classifier.
    // SAFETY: this test thread owns and reaps the child before returning.
    unsafe {
        resources.spawn_classifier(
            &mut fx.context,
            Path::new("/bin/true"),
            bootstrap,
            &mut fx.clock,
        )
    }
    .unwrap();
    for _ in 0..3000 {
        if resources.classifier.as_mut().unwrap().poll() != ChildState::Running {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(matches!(
        resources.classifier.as_ref().unwrap().state(),
        ChildState::Reaped(_)
    ));
    let failure = fx
        .context
        .pump_once(&mut PumpPorts {
            lifecycle: LifecyclePorts::default(),
            gate: None,
            decode: None,
            label: None,
            io: &mut fx.clock,
        })
        .unwrap_err();
    assert_eq!(fx.context.phase, RecordingPhase::Failed);
    assert!(fx.context.flow.starting_classifier().is_some());
    let mut owner = start(&mut fx, resources);
    drive(&mut owner, &mut fx.clock);
    assert_eq!(owner.failure(), &failure);
    assert!(owner.context.flow.starting_classifier().is_none());
    assert!(owner.context.flow.standing_classifier().is_none());
    assert!(!owner.resources.has_unreaped_child());
    assert_eq!(
        owner
            .context
            .barrier
            .recorder()
            .structure()
            .by_kind(Kind::Load)
            .count(),
        1
    );
    assert_eq!(
        owner
            .context
            .barrier
            .recorder()
            .structure()
            .by_kind(Kind::Ste100Result)
            .count(),
        0
    );
    assert!(owner.unload_sequence().is_some());
}

#[test]
fn failed_enter_cleanup_preopen_failure_closes_without_fabricated_load_sample_fault_or_unload() {
    let mut fx = Fixture::new(config());
    let resources = fx.context.failed_enter_resources().unwrap();
    let first = state_eof(&mut fx);
    let mut owner = start(&mut fx, resources);
    drive(&mut owner, &mut fx.clock);
    assert_eq!(owner.failure(), &first);
    assert!(owner.account_failure.is_some());
    assert!(owner.context.flow.halt().unwrap().account().is_none());
    assert!(owner.context.halt_report.is_none());
    assert!(owner.unload_sequence().is_none());
    assert!(owner.unload_unavailable.is_some());
    assert!(owner.context.flow.standing_state().is_none());
    assert!(records(&mut fx).is_empty());
    let done = match owner.finish() {
        Ok(done) => done,
        Err(_) => panic!("closed"),
    };
    assert!(!done.abandoned());
    assert!(done.trace_drained());
    assert!(done.unload_attempted());
    assert!(done.unload_sequence().is_none());
    assert!(done.unload_unavailable().is_some());
    assert!(done.report_unavailable().is_some());
    assert!(done.account_failure().is_some());
}

fn fill_trace(fx: &mut Fixture) {
    let payload =
        serde_json::value::to_raw_value(&serde_json::json!({"fixture":"x".repeat(32_768)}))
            .unwrap();
    for _ in 0..serving::ORDINARY_CAPACITY + 64 {
        if fx.context.barrier.recorder().boundary().queued == serving::ORDINARY_CAPACITY {
            return;
        }
        let now = fx.clock.now().unwrap();
        assert!(matches!(
            fx.context.barrier.submit_authored_with_clock(
                AuthoredEvent {
                    kind: Kind::ModelMeasurement,
                    subsystem: Subsystem::Harness,
                    turn: Some(weaver_trace::TurnRef("pressure".into())),
                    source: AuthoredSource::Runtime,
                    causal_parent: None,
                    payload: Some(Payload::ModelMeasurement(payload.clone()))
                },
                RecordPurpose::Observation,
                &fx.context.author,
                1,
                &mut fx.context.inbox,
                &mut fx.context.budget,
                &mut || Ok(now)
            ),
            RecordProgress::Recorded { .. }
        ));
    }
    panic!("ordinary writer capacity did not fill");
}

#[test]
fn failed_enter_cleanup_reserved_terminals_do_not_wait_for_ordinary_pressure_or_lose_original_ticket()
 {
    let mut fx = Fixture::new(config());
    fx.activate();
    let leave = AbsoluteBound::after(fx.clock.now().unwrap(), 1000).unwrap();
    fx.context
        .arm(PumpDeadline::Leave, leave, &mut fx.clock)
        .unwrap();
    fill_trace(&mut fx);
    let now = fx.clock.now().unwrap();
    let mut original = event(
        Kind::ModelMeasurement,
        Some(Payload::ModelMeasurement(
            serde_json::value::to_raw_value(&serde_json::json!({"owed":"exact-pressure-ticket"}))
                .unwrap(),
        )),
    );
    original.turn = Some(weaver_trace::TurnRef("pressure".into()));
    assert!(matches!(
        fx.context.barrier.submit_authored_with_clock(
            original,
            RecordPurpose::Observation,
            &fx.context.author,
            1,
            &mut fx.context.inbox,
            &mut fx.context.budget,
            &mut || Ok(now)
        ),
        RecordProgress::Pending
    ));
    let ticket = fx.context.pending_author().unwrap().ticket;
    let event = format!("{:?}", fx.context.pending_author().unwrap().event);
    let resources = fx.context.failed_enter_resources().unwrap();
    let first = state_eof(&mut fx);
    let mut owner = start(&mut fx, resources);
    for _ in 0..16 {
        owner.advance(&mut fx.clock);
        if owner.stage == Stage::Drain {
            break;
        }
    }
    assert_eq!(owner.stage, Stage::Drain);
    assert!(owner.unload_sequence().is_some());
    assert_eq!(
        owner
            .context
            .barrier
            .recorder()
            .structure()
            .by_kind(Kind::Fault)
            .count(),
        1
    );
    assert_eq!(
        owner
            .context
            .barrier
            .recorder()
            .structure()
            .by_kind(Kind::Unload)
            .count(),
        1
    );
    assert_eq!(owner.context.pending_author().unwrap().ticket, ticket);
    assert_eq!(
        format!("{:?}", owner.context.pending_author().unwrap().event),
        event
    );
    assert_eq!(owner.advance(&mut fx.clock), FailedEnterProgress::Pending);
    fx.clock.advance_to(leave.deadline_ns()).unwrap();
    assert_eq!(owner.advance(&mut fx.clock), FailedEnterProgress::Closed);
    assert!(owner.abandoned());
    assert_eq!(owner.failure(), &first);
    assert_eq!(owner.context.pending_author().unwrap().ticket, ticket);
    assert_eq!(owner.deadline(), Some(leave.deadline_ns()));
}

struct ClockFailsAfterFault {
    now: u64,
    calls: usize,
}
impl PumpIo for ClockFailsAfterFault {
    fn now(&mut self) -> std::io::Result<u64> {
        self.calls += 1;
        if self.calls >= 3 {
            Err(std::io::Error::other("post-fault clock unavailable"))
        } else {
            Ok(self.now)
        }
    }
    fn wait(
        &mut self,
        _: &[WaitInterest<'_>],
        _: Option<u64>,
    ) -> std::io::Result<crate::flow::clock::WaitOutcome> {
        panic!("nonblocking reporting must not wait")
    }
}

#[test]
fn failed_enter_cleanup_post_fault_clock_failure_keeps_recorded_sequence_and_first_cause() {
    let mut fx = Fixture::new(config());
    fx.activate();
    let resources = fx.context.failed_enter_resources().unwrap();
    let first = state_eof(&mut fx);
    let mut io = ClockFailsAfterFault {
        now: fx.clock.now().unwrap(),
        calls: 0,
    };
    let mut owner = start(&mut fx, resources);
    assert_eq!(owner.advance(&mut io), FailedEnterProgress::Closed);
    assert!(owner.abandoned());
    assert_eq!(owner.failure(), &first);
    assert!(
        owner
            .context
            .halt_report
            .as_ref()
            .unwrap()
            .fault_sequence()
            .is_some()
    );
    assert!(owner.unload_sequence().is_none());
    assert_eq!(
        owner
            .context
            .barrier
            .recorder()
            .structure()
            .by_kind(Kind::Fault)
            .count(),
        1
    );
    assert!(owner.context.flow.standing_state().is_none());
}

#[test]
fn failed_enter_cleanup_refuses_actual_ordinary_read_owner_with_original_custody() {
    let mut fx = Fixture::new(config());
    fx.activate();
    fx.context.complete_wait(PumpDeadline::Enter).unwrap();
    let selector = weaver_types::state_flow::ReadSelector::History {
        noun: "record".into(),
        verb: None,
        condition: None,
    };
    fx.context
        .begin_read(selector, None, &mut fx.clock)
        .unwrap();
    let resources = fx.context.failed_enter_resources().unwrap();
    let first = state_eof(&mut fx);
    let rejected = match take_context(&mut fx).into_failed_enter_cleanup(resources) {
        Err(error) => error,
        Ok(_) => panic!("ordinary read is not Boot cleanup"),
    };
    assert_eq!(rejected.refusal, FailedEnterRefusal::LiveDuty);
    assert_eq!(rejected.context.failure(), Some(&first));
    assert!(rejected.context.read.is_some());
    assert!(rejected.resources.matches(&rejected.context));
}

#[test]
fn failed_enter_process_premature_release_peer() {
    if !child_mode("failed_enter_process_premature_release_peer") {
        return;
    }
    let reply = serde_json::to_vec(&OrganEnvelope {
        exchange: ExchangeId {
            opener: Opener::Harness,
            ordinal: 1,
        },
        position: Position::Close,
        payload: weaver_types::Payload::Answer(LifecycleAnswer::Released),
    })
    .unwrap();
    // SAFETY: only the actual exec fixture owns its placed fd3.
    assert_eq!(
        unsafe {
            nix::libc::send(
                3,
                reply.as_ptr().cast(),
                reply.len(),
                nix::libc::MSG_NOSIGNAL,
            )
        } as usize,
        reply.len()
    );
    let mut byte = [0u8];
    assert_eq!(
        unsafe { nix::libc::recv(3, byte.as_mut_ptr().cast(), 1, 0) },
        0,
        "a premature answer must never trigger a Release send"
    );
}

#[test]
fn failed_enter_cleanup_actual_premature_release_reply_cannot_confirm_an_unsent_request() {
    let mut fx = Fixture::new(config());
    fx.activate();
    let mut resources = fx.context.failed_enter_resources().unwrap();
    // SAFETY: the original child is reaped before this test thread returns.
    unsafe {
        resources.spawn_spu(
            &mut fx.context,
            &std::env::current_exe().unwrap(),
            &child_arguments("failed_enter_process_premature_release_peer"),
            &mut fx.clock,
        )
    }
    .unwrap();
    for _ in 0..3000 {
        let port = resources.residency_mut().unwrap();
        port.try_receive().unwrap();
        if port.held().is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(resources.residency_mut().unwrap().held().is_some());
    state_eof(&mut fx);
    let mut owner = start(&mut fx, resources);
    drive(&mut owner, &mut fx.clock);
    assert_eq!(owner.release, ReleaseOutcome::Unconfirmed);
    assert!(!owner.release_send.as_ref().unwrap().sent());
    let reply = owner.release_reply.as_ref().unwrap();
    assert!(serde_json::from_slice::<OrganEnvelope>(reply.raw_bytes()).is_ok());
    assert_eq!(
        reply.envelope().exchange,
        owner.release_exchange.clone().unwrap()
    );
    assert!(matches!(
        owner.resources.spu.as_ref().unwrap().child.state(),
        ChildState::Reaped(nix::sys::wait::WaitStatus::Exited(_, 0))
    ));
}

#[test]
fn failed_enter_cleanup_retirement_moves_actual_partial_state_bytes_without_reconstruction() {
    let mut fx = Fixture::new(config());
    fx.activate();
    let raw = b"{\"partial_actual_state_frame\": ";
    fx.state.write_all(raw).unwrap();
    let resources = fx.context.failed_enter_resources().unwrap();
    state_eof(&mut fx);
    assert_eq!(fx.context.flow.state().retained_received_bytes(), raw);
    let mut owner = start(&mut fx, resources);
    drive(&mut owner, &mut fx.clock);
    assert_eq!(
        owner
            .retired
            .as_ref()
            .unwrap()
            .state
            .as_ref()
            .unwrap()
            .retained_received_bytes(),
        raw
    );
    assert_eq!(fx.state.read(&mut [0]).unwrap(), 0);
}

#[test]
fn failed_enter_cleanup_same_context_cannot_issue_a_second_bag_omitting_an_actual_child() {
    let mut fx = Fixture::new(config());
    fx.activate();
    let mut resources = fx.context.failed_enter_resources().unwrap();
    // SAFETY: this test retains/reaps the actual child before returning.
    unsafe { resources.spawn_spu(&mut fx.context, Path::new("/bin/true"), &[], &mut fx.clock) }
        .unwrap();
    assert!(fx.context.failed_enter_resources().is_none());
    state_eof(&mut fx);
    assert!(fx.context.failed_enter_resources().is_none());
    let mut owner = start(&mut fx, resources);
    drive(&mut owner, &mut fx.clock);
    assert!(matches!(
        owner.resources.spu_child(),
        Some(ChildState::Reaped(_))
    ));
}

#[test]
fn failed_enter_cleanup_foreign_replaced_producer_ports_return_exact_resources_before_cleanup() {
    for decoder in [true, false] {
        let mut fx = Fixture::new(config());
        fx.activate();
        let mut resources = fx.context.failed_enter_resources().unwrap();
        // SAFETY: original actual child is retained/reaped through the retry.
        unsafe { resources.spawn_spu(&mut fx.context, Path::new("/bin/true"), &[], &mut fx.clock) }
            .unwrap();
        let (foreign, _peer) = crate::channel::OrganChannel::pair().unwrap();
        let mut old_decode = None;
        let mut old_residency = None;
        if decoder {
            old_decode = Some(std::mem::replace(
                resources.decode_mut().unwrap(),
                DecodePort::from_owned(foreign.into_fd()).unwrap(),
            ));
        } else {
            old_residency = Some(std::mem::replace(
                resources.residency_mut().unwrap(),
                OrganPort::from_channel(foreign).unwrap(),
            ));
            let next = resources.spu.as_ref().unwrap().next_ordinal;
            assert!(
                resources
                    .stage_residency(review_tests::prior_admission())
                    .is_err()
            );
            assert_eq!(resources.spu.as_ref().unwrap().next_ordinal, next);
        }
        state_eof(&mut fx);
        let rejected = match take_context(&mut fx).into_failed_enter_cleanup(resources) {
            Err(error) => error,
            Ok(_) => panic!("replaced producer endpoint admitted"),
        };
        assert_eq!(rejected.refusal, FailedEnterRefusal::ForeignTransport);
        let mut resources = rejected.resources;
        if let Some(original) = old_decode {
            *resources.decode_mut().unwrap() = original;
        }
        if let Some(original) = old_residency {
            *resources.residency_mut().unwrap() = original;
        }
        let mut owner = match rejected.context.into_failed_enter_cleanup(resources) {
            Ok(owner) => owner,
            Err(error) => panic!("restored producer: {:?}", error.refusal),
        };
        drive(&mut owner, &mut fx.clock);
        assert!(matches!(
            owner.resources.spu_child(),
            Some(ChildState::Reaped(_))
        ));
    }
}

struct CrossDuringAccount {
    before: u64,
    deadline: u64,
    calls: usize,
}
impl PumpIo for CrossDuringAccount {
    fn now(&mut self) -> std::io::Result<u64> {
        self.calls += 1;
        Ok(if self.calls == 2 {
            self.deadline
        } else {
            self.before
        })
    }
    fn wait(
        &mut self,
        _: &[WaitInterest<'_>],
        _: Option<u64>,
    ) -> std::io::Result<crate::flow::clock::WaitOutcome> {
        panic!("account continuation cannot wait")
    }
}

#[test]
fn failed_enter_cleanup_account_step_cap_observation_cannot_be_erased_before_release_or_unload() {
    for target in [Stage::Release, Stage::Unload] {
        let mut fx = Fixture::new(config());
        fx.activate();
        let mut resources = fx.context.failed_enter_resources().unwrap();
        if target == Stage::Release {
            // SAFETY: actual child remains owned and is reaped below.
            unsafe {
                resources.spawn_spu(
                    &mut fx.context,
                    Path::new("/bin/sleep"),
                    &["60".into()],
                    &mut fx.clock,
                )
            }
            .unwrap();
        }
        state_eof(&mut fx);
        let mut owner = start(&mut fx, resources);
        for _ in 0..16 {
            if owner.stage == target {
                break;
            }
            owner.advance(&mut fx.clock);
        }
        assert_eq!(owner.stage, target);
        let mut io = CrossDuringAccount {
            before: fx.clock.now().unwrap(),
            deadline: owner.deadline().unwrap(),
            calls: 0,
        };
        assert_eq!(owner.advance(&mut io), FailedEnterProgress::Closed);
        assert!(
            owner
                .context
                .halt_report
                .as_ref()
                .unwrap()
                .aborted()
                .is_some()
        );
        assert!(owner.release_send.is_none());
        assert!(!owner.unload_attempted);
        assert!(owner.unload_sequence().is_none());
        assert!(owner.abandoned());
        let mut finished = match owner.finish() {
            Ok(done) => done,
            Err(_) => panic!("not closed"),
        };
        for _ in 0..3000 {
            if finished.poll_children() {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(!finished.resources().has_unreaped_child());
    }
}

type ObservedWait = (Vec<(i32, PollFlags)>, Option<u64>);

struct RecordingWait {
    now: u64,
    calls: Vec<ObservedWait>,
}
impl PumpIo for RecordingWait {
    fn now(&mut self) -> std::io::Result<u64> {
        Ok(self.now)
    }
    fn wait(
        &mut self,
        interests: &[WaitInterest<'_>],
        deadline: Option<u64>,
    ) -> std::io::Result<crate::flow::clock::WaitOutcome> {
        use std::os::fd::AsRawFd;
        self.calls.push((
            interests
                .iter()
                .map(|interest| (interest.fd.as_raw_fd(), interest.events))
                .collect(),
            deadline,
        ));
        self.now = deadline.expect("every failed cleanup wait is bounded");
        Ok(crate::flow::clock::WaitOutcome::Timeout)
    }
}

#[test]
fn failed_enter_cleanup_wait_uses_original_cap_with_actual_silent_child_and_trace_sink() {
    use std::os::fd::AsRawFd;
    let mut fx = Fixture::new(config());
    fx.activate();
    let leave = AbsoluteBound::after(fx.clock.now().unwrap(), 25_000_000).unwrap();
    fx.context
        .arm(PumpDeadline::Leave, leave, &mut fx.clock)
        .unwrap();
    let mut resources = fx.context.failed_enter_resources().unwrap();
    // SAFETY: this fixture retains and polls the original child through reap.
    unsafe {
        resources.spawn_spu(
            &mut fx.context,
            Path::new("/bin/sleep"),
            &["60".into()],
            &mut fx.clock,
        )
    }
    .unwrap();
    let first = state_eof(&mut fx);
    let mut owner = start(&mut fx, resources);
    let trace_fd = owner.context.barrier.capacity_fd().as_raw_fd();
    let mut io = RecordingWait {
        now: fx.clock.now().unwrap(),
        calls: Vec::new(),
    };
    let initial = io.now;
    assert!(leave.deadline_ns() > initial + 20_000_000);
    for expected in [
        initial + 10_000_000,
        initial + 20_000_000,
        leave.deadline_ns(),
    ] {
        let progress = owner.wait_for_progress(&mut io);
        assert_eq!(
            io.calls.last().unwrap(),
            &(vec![(trace_fd, PollFlags::POLLIN)], Some(expected))
        );
        assert_eq!(owner.deadline(), Some(leave.deadline_ns()));
        assert_eq!(
            progress,
            if expected == leave.deadline_ns() {
                FailedEnterProgress::Closed
            } else {
                FailedEnterProgress::Pending
            }
        );
    }
    assert_eq!(io.calls.len(), 3);
    assert_eq!(
        owner.wait_for_progress(&mut io),
        FailedEnterProgress::Closed
    );
    assert_eq!(io.calls.len(), 3, "Closed cannot wait again");
    assert_eq!(owner.failure(), &first);
    assert!(owner.abandoned());
    assert!(owner.unload_sequence().is_none());
    let mut finished = match owner.finish() {
        Ok(done) => done,
        Err(_) => panic!("not closed"),
    };
    // Evidence collection after the production cap is explicitly nonblocking.
    for _ in 0..3000 {
        if finished.poll_children() {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(!finished.resources().has_unreaped_child());
}

#[path = "context_failed_enter_review_tests.rs"]
mod review_tests;
