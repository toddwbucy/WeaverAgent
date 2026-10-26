use super::super::super::tests::{Member, dial, packet, trace_through, worker};
use super::*;
use crate::flow::clock::testing::FakePumpIo;
use crate::flow::context::startup_composition::tests::CircuitIo;
use crate::flow::pump::PumpPorts;
use nix::sys::socket::{MsgFlags, send};
use std::io::Read;
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};
use weaver_types::{Cause, ExchangeId};

pub(in crate::flow::context::worker_startup) struct Case {
    handoff: Box<FoundationRollbackHandoff>,
    peer: Option<OwnedFd>,
    trace: Vec<u8>,
    first: Option<RunFailure>,
    requests: Vec<String>,
    owed: Option<Owed>,
}
struct Owed {
    bound: AbsoluteBound,
    request: weaver_types::state_flow::DecimalU64,
    ticket: u64,
    bytes: Vec<u8>,
}
struct Guard(Box<FoundationRollbackExit>);
impl Drop for Guard {
    fn drop(&mut self) {
        match &mut self.0.driver {
            RollbackDriver::Halt(tail) => tail.reap_composition_test_children(),
            RollbackDriver::NoRecord(owner) => owner.resources.reap_startup_test_children(),
        }
    }
}
struct PanicReader(UnixStream);
impl Drop for PanicReader {
    fn drop(&mut self) {
        if std::thread::panicking() {
            let _ = self.0.shutdown(std::net::Shutdown::Both);
        }
    }
}
fn context<'b>(owner: &'b mut FoundationRollback<'_>) -> &'b mut RecordingContext {
    match owner.state.as_mut().unwrap() {
        RollbackState::Record(owner) => owner.failure_test_context(),
        RollbackState::Recorded(owner) => owner.failure_test_context(),
        RollbackState::Response(owner) | RollbackState::Await(owner) => {
            owner.failure_test_context()
        }
        RollbackState::Rollback(owner) => owner.failure_test_context(),
        RollbackState::Prepared(owner) => owner.failure_test_context(),
        RollbackState::Terminal(owner) => owner.test_context(),
        RollbackState::Closed(_) => panic!("completed rollback is not a failure target"),
    }
}
fn reached(owner: &FoundationRollback<'_>, member: &Member, mode: &str) -> bool {
    match (mode, owner.state.as_ref().unwrap()) {
        ("rollback-failed-record", RollbackState::Record(_))
        | ("rollback-failed-recorded", RollbackState::Recorded(_))
        | ("rollback-failed-response", RollbackState::Response(_))
        | ("rollback-failed-await", RollbackState::Await(_)) => true,
        (
            "rollback-failed-transition" | "rollback-failed-leave-cap",
            RollbackState::Rollback(owner),
        ) => owner.stage() == RollbackStage::Transition,
        (
            "rollback-failed-grants"
            | "rollback-failed-grant-pressure"
            | "rollback-failed-grant-recorded",
            RollbackState::Rollback(owner),
        ) => owner.stage() == RollbackStage::Grants && member.held_control.is_some(),
        (
            "rollback-failed-prepared"
            | "rollback-failed-trace"
            | "rollback-failed-foreign"
            | "rollback-failed-worker"
            | "rollback-failed-wait",
            RollbackState::Prepared(_),
        ) => true,
        ("rollback-failed-seal", RollbackState::Terminal(owner)) => {
            owner.stage() == TerminalStage::Seal
        }
        ("rollback-failed-state", RollbackState::Terminal(owner)) => {
            owner.stage() == TerminalStage::State
        }
        (
            "rollback-failed-unload"
            | "rollback-failed-observer"
            | "rollback-failed-drain-cap"
            | "rollback-failed-recorded-cap",
            RollbackState::Terminal(owner),
        ) => owner.stage() == TerminalStage::Unload,
        ("rollback-failed-left-peer", RollbackState::Terminal(owner)) => {
            owner.stage() == TerminalStage::Answer
        }
        _ => false,
    }
}

// Every input is an actual end of the same fork/socket circuit.
#[allow(clippy::too_many_arguments)]
pub(in crate::flow::context::worker_startup) fn enter(
    enter: FoundationEnter<'_>,
    original: &OwnedFd,
    member: &mut Member,
    reader: &mut UnixStream,
    clock: &mut FakePumpIo,
    path: &Path,
    mode: &str,
) -> Option<Case> {
    let mut owner = enter
        .rollback_refusal()
        .ok()
        .expect("actual classifier refusal");
    let _panic_reader = PanicReader(reader.try_clone().unwrap());
    // The public gate must return an unfailed owner, without consuming its bytes.
    owner = owner
        .failed_handoff()
        .err()
        .expect("progressing rollback stays owned");
    let until = Instant::now() + Duration::from_secs(8);
    let mut peer = None;
    let mut trace = Vec::new();
    let mut answered = false;
    reader.set_nonblocking(true).unwrap();
    if matches!(
        mode,
        "rollback-failed-grants"
            | "rollback-failed-grant-pressure"
            | "rollback-failed-grant-recorded"
    ) {
        member.hold_control = Some("grants");
    }
    while !reached(&owner, member, mode) {
        assert!(
            Instant::now() < until,
            "rollback target {mode}: {:?}",
            owner.detail()
        );
        if owner.progress() == FoundationRollbackProgress::AwaitRollback && peer.is_none() {
            let answer = packet(original).expect("original Enter refusal delivered");
            assert!(matches!(answer.payload, Payload::Refusal(_)));
            answered = true;
            if mode == "rollback-failed-healthy" {
                clock
                    .advance_to(owner.enter_window().deadline_ns() + 1)
                    .unwrap();
                assert_eq!(
                    owner.advance_with_io(&mut CircuitIo(clock)),
                    FoundationRollbackProgress::AwaitRollback
                );
                owner = owner
                    .failed_handoff()
                    .err()
                    .expect("healthy refusal awaiting root is not a failure");
                assert!(context(&mut owner).failure().is_none());
                return None;
            }
            let fd = dial(path);
            let leave = OrganEnvelope {
                exchange: ExchangeId {
                    opener: Opener::Admin,
                    ordinal: 94,
                },
                position: Position::Open,
                payload: Payload::Directive(LifecycleDirective::Leave {
                    cause: Cause { uid: 987 },
                    forced: false,
                    rollback: true,
                }),
            };
            let bytes = serde_json::to_vec(&leave).unwrap();
            assert_eq!(
                send(fd.as_raw_fd(), &bytes, MsgFlags::MSG_NOSIGNAL),
                Ok(bytes.len())
            );
            peer = Some(fd);
        }
        assert_ne!(
            owner.advance_with_io(&mut CircuitIo(clock)),
            FoundationRollbackProgress::Stopped,
            "{:?}",
            owner.detail()
        );
        member.step(clock.now().unwrap(), mode);
        let _ = reader.read_to_end(&mut trace);
        std::thread::sleep(Duration::from_millis(1));
    }
    let mut owed = None;
    if mode.starts_with("rollback-failed-grant-") {
        let request = member.held_control.as_ref().unwrap().common().request_id;
        let bound = context(&mut owner)
            .deadlines
            .get(PumpDeadline::StateControl)
            .unwrap();
        fill_trace(context(&mut owner), clock);
        member.hold_control = None;
        member.step(clock.now().unwrap(), mode);
        while context(&mut owner).barrier.pending_author().is_none() {
            assert!(
                Instant::now() < until,
                "Grants measurement did not become owed"
            );
            assert_ne!(
                owner.advance_with_io(&mut CircuitIo(clock)),
                FoundationRollbackProgress::Stopped
            );
        }
        let cx = context(&mut owner);
        let author = cx.barrier.pending_author().unwrap();
        assert_eq!(
            author.event.kind,
            weaver_trace::serving::Kind::StateStoreMeasurement
        );
        owed = Some(Owed {
            bound,
            request,
            ticket: author.ticket,
            bytes: serde_json::to_vec(cx.flow.state().held_measurement().unwrap()).unwrap(),
        });
        // Failure of the actual shared clock, followed by a valid first-latch
        // observation, leaves a bounded trace-only account, never a live read.
        assert!(
            cx.live(&mut FailFirstClock {
                clock,
                failed: false
            })
            .is_err()
        );
        assert!(cx.failure().is_some());
        if mode == "rollback-failed-grant-recorded" {
            loop {
                let _ = reader.read_to_end(&mut trace);
                if cx.barrier.recorder().boundary().queued == 0 {
                    break;
                }
                assert!(Instant::now() < until, "pressure did not drain");
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }
    if mode == "rollback-failed-transition" {
        clock
            .advance_to(owner.leave_window().unwrap().began_ns() + 1_000)
            .unwrap();
    }
    if mode == "rollback-failed-await" {
        assert!(matches!(
            packet(original).unwrap().payload,
            Payload::Refusal(_)
        ));
        answered = true;
        // A second original peer is retained while failure seals the listener.
        peer = Some(dial(path));
        owner.advance_with_io(&mut CircuitIo(clock));
        assert!(owner.dial.is_some());
    }
    if matches!(mode, "rollback-failed-trace" | "rollback-failed-wait") {
        fill_trace(context(&mut owner), clock);
    }
    match mode {
        "rollback-failed-leave-cap" => {
            clock
                .advance_to(owner.leave_window().unwrap().deadline_ns())
                .unwrap();
        }
        "rollback-failed-observer" => {
            context(&mut owner)
                .barrier
                .fail_next_recorded_observer_for_test();
        }
        "rollback-failed-drain-cap" | "rollback-failed-recorded-cap" => {
            let event = crate::flow::context::AuthoredEvent {
                kind: weaver_trace::serving::Kind::Fault,
                subsystem: weaver_trace::Subsystem::Harness,
                turn: None,
                causal_parent: None,
                source: crate::flow::author::AuthoredSource::Runtime,
                payload: Some(weaver_trace::serving::Payload::Fault(
                    serde_json::value::to_raw_value(
                        &serde_json::json!({"fixture":"x".repeat(2_000_000)}),
                    )
                    .unwrap(),
                )),
            };
            context(&mut owner)
                .author_and_wait(
                    &mut PumpPorts {
                        lifecycle: Default::default(),
                        gate: None,
                        decode: None,
                        label: None,
                        io: &mut CircuitIo(clock),
                    },
                    event,
                )
                .unwrap();
            if mode == "rollback-failed-recorded-cap" {
                context(&mut owner)
                    .barrier
                    .fail_next_recorded_observer_for_test();
            } else {
                assert_ne!(
                    owner.advance_with_io(&mut CircuitIo(clock)),
                    FoundationRollbackProgress::Stopped
                );
                assert!(owner.unload_sequence().is_some());
                let cap = owner.leave_window().unwrap().deadline_ns();
                let mut expire = ExpireWait { clock, cap };
                assert_eq!(
                    owner.advance_with_io(&mut expire),
                    FoundationRollbackProgress::Stopped
                );
            }
        }
        "rollback-failed-left-peer" => {
            nix::sys::socket::shutdown(
                peer.as_ref().unwrap().as_raw_fd(),
                nix::sys::socket::Shutdown::Read,
            )
            .unwrap();
        }
        "rollback-failed-grant-pressure" | "rollback-failed-grant-recorded" => {}
        "rollback-failed-unload" => {
            let result = context(&mut owner).live(&mut BrokenClock);
            assert!(result.is_err());
            assert!(context(&mut owner).failure().is_some());
        }
        _ => {
            member.stop_responses();
            let _ = context(&mut owner).pump_once(&mut PumpPorts {
                lifecycle: Default::default(),
                gate: None,
                decode: None,
                label: None,
                io: &mut CircuitIo(clock),
            });
            assert!(
                context(&mut owner).failure().is_some(),
                "actual EOF at {mode}"
            );
        }
    }
    assert_eq!(
        owner.advance_with_io(&mut CircuitIo(clock)),
        FoundationRollbackProgress::Stopped,
        "{mode}: {:?}",
        owner.detail()
    );
    if !answered {
        assert!(
            packet(original).is_none(),
            "no refusal manufactured after component failure"
        );
    }
    let first = context(&mut owner).failure().cloned();
    let handoff = owner
        .failed_handoff()
        .ok()
        .expect("stopped rollback joins its exact failed route");
    Some(Case {
        handoff,
        peer,
        trace,
        first,
        requests: member.requests.clone(),
        owed,
    })
}

#[allow(clippy::too_many_arguments)]
pub(in crate::flow::context::worker_startup) fn finish(
    mut run: Box<FoundationRun>,
    case: Case,
    original: &OwnedFd,
    member: &mut Member,
    reader: &mut UnixStream,
    clock: &mut FakePumpIo,
    path: &Path,
    wire: &[u8],
    load: Cause,
    mode: &str,
) {
    let Case {
        mut handoff,
        peer,
        mut trace,
        first,
        requests,
        owed,
    } = case;
    let _panic_reader = PanicReader(reader.try_clone().unwrap());
    assert!(
        !path.exists(),
        "failure retires the actual lifecycle listener"
    );
    let retained = match &handoff.held {
        HeldRollback::Record(owner) | HeldRollback::Recorded(owner) => owner.retained_enter_wire(),
        HeldRollback::Response(owner) | HeldRollback::Await(owner) => owner.retained_enter_wire(),
        HeldRollback::Rollback(owner) => owner.retained_enter_wire(),
        HeldRollback::Prepared(owner) => owner.retained_enter_wire(),
        HeldRollback::Terminal(owner) => owner.retained_enter_wire(),
    };
    assert_eq!(
        retained, wire,
        "the original Enter bytes survive every representation"
    );
    let comparison = match &handoff.held {
        HeldRollback::Prepared(owner) => Some(owner.retained_comparison()),
        HeldRollback::Terminal(owner) => Some(owner.retained_comparison()),
        _ => None,
    };
    if let Some((initial, current, directive, surface)) = comparison {
        use crate::flow::context::InspectionCompletion;
        for reading in [initial, current] {
            let InspectionCompletion::Grants { surface, .. } = reading else {
                panic!("actual Grants receipt lost");
            };
            assert_eq!(surface, &["owner:actual"]);
        }
        assert_eq!(surface, weaver_trace::GrantSurface::Unchanged);
        assert!(
            matches!(
                directive.payload,
                Payload::Directive(LifecycleDirective::Leave {
                    cause: Cause { uid: 987 },
                    forced: false,
                    rollback: true
                })
            ),
            "original directive differs from derived forced/load-cause facts"
        );
    }
    let cap = handoff.identity.deadline;
    let route = handoff.identity.route;
    let accepted = handoff.identity.accepted_leave;
    let recorded = handoff.identity.terminal;
    assert_eq!(run.context.failure(), first.as_ref());
    let actual_cap = match run.context.flow.halt() {
        Some(halt) => halt
            .deadline()
            .ok()
            .map(|end| accepted.map_or(end, |leave| end.min(leave.deadline_ns()))),
        None => accepted.map(AbsoluteBound::deadline_ns),
    };
    assert_eq!(
        cap, actual_cap,
        "original halt capped by actual accepted Leave"
    );
    if mode == "rollback-failed-transition" {
        assert!(
            run.context.flow.halt().unwrap().deadline().unwrap() > accepted.unwrap().deadline_ns()
        );
        assert_eq!(cap, Some(accepted.unwrap().deadline_ns()));
    }

    if mode == "rollback-failed-foreign" {
        let mut fixture =
            crate::flow::context::tests::Fixture::new(crate::flow::context::tests::config());
        let mut context =
            crate::flow::context::failed_enter_driver::tests::take_context(&mut fixture);
        let resources = context.failed_enter_resources().unwrap();
        let foreign = Box::new(FoundationRun {
            context,
            resources,
            listener: None,
            original: None,
        });
        let (foreign, returned) = foreign
            .failed_rollback_exit(handoff)
            .err()
            .expect("foreign run cannot consume this rollback");
        assert!(foreign.context.failure().is_none());
        drop(foreign);
        handoff = returned;
        let actual_origin = Arc::clone(&run.context.read_origin);
        run.context.read_origin = Arc::new(());
        let (returned_run, returned) = run
            .failed_rollback_exit(handoff)
            .err()
            .expect("substitute context cannot consume original resources");
        run = returned_run;
        handoff = returned;
        run.context.read_origin = actual_origin;
    }
    let exit = if mode == "rollback-failed-worker" {
        worker::failed_rollback_exit(run, handoff)
    } else {
        run.failed_rollback_exit(handoff)
    }
    .ok()
    .expect("original run joins original handoff");
    if mode == "rollback-failed-worker" {
        assert_eq!(
            exit.progress(),
            FoundationRollbackExitProgress::Closed,
            "worker must not return while cleanup is pending"
        );
    }
    let mut guard = Guard(exit);
    let _panic_after_owner = PanicReader(reader.try_clone().unwrap());
    let until = Instant::now() + Duration::from_secs(8);
    let mut wait_failed = false;
    if mode == "rollback-failed-recorded-cap" {
        clock.advance_to(cap.unwrap() - 1).unwrap();
    }
    loop {
        assert!(
            Instant::now() < until,
            "rollback cleanup did not finish: {mode}"
        );
        let progress = guard.0.advance_with_io(&mut CircuitIo(clock));
        match &guard.0.driver {
            RollbackDriver::Halt(tail) => {
                assert_eq!(Some(tail.failure()), first.as_ref());
                assert_eq!(tail.deadline(), cap);
            }
            RollbackDriver::NoRecord(owner) => {
                assert_eq!(owner.context.failure(), first.as_ref());
                assert_eq!(owner.deadline, cap);
            }
        }
        if progress == FoundationRollbackExitProgress::Closed {
            break;
        }
        if matches!(
            mode,
            "rollback-failed-trace" | "rollback-failed-grant-pressure"
        ) && guard.0.unload_sequence().is_some()
        {
            clock.advance_to(cap.unwrap()).unwrap();
        } else if progress == FoundationRollbackExitProgress::Pending {
            if mode == "rollback-failed-recorded-cap" {
                guard.0.wait_with_io(clock);
                assert_eq!(clock.waits().last().unwrap().absolute_deadline, cap);
                assert_eq!(clock.now().unwrap(), cap.unwrap());
            } else if mode == "rollback-failed-wait" && !wait_failed {
                guard.0.wait_with_io(&mut BrokenWait(clock));
                wait_failed = true;
            } else {
                guard.0.wait_with_io(&mut CircuitIo(clock));
            }
        }
        if !matches!(
            mode,
            "rollback-failed-trace"
                | "rollback-failed-wait"
                | "rollback-failed-drain-cap"
                | "rollback-failed-recorded-cap"
                | "rollback-failed-grant-pressure"
        ) {
            let _ = reader.read_to_end(&mut trace);
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    if mode == "rollback-failed-wait" {
        assert!(wait_failed);
    }
    assert_eq!(guard.0.channel_fault(), Some(crate::ChannelFault::Closed));
    assert!(packet(original).is_none(), "no new Enter answer");
    if let Some(peer) = peer {
        assert!(packet(&peer).is_none(), "failed exit cannot send Left");
    }
    assert_eq!(
        member.requests, requests,
        "no Grants retry, state write or save"
    );
    member.assert_closed_without_request();
    assert_eq!(
        guard.0.advance_with_io(&mut CircuitIo(clock)),
        FoundationRollbackExitProgress::Closed
    );
    if let Some(owed) = owed {
        let RollbackDriver::Halt(tail) = &guard.0.driver else {
            panic!("owed read requires failure route");
        };
        let account = tail
            .rollback_test_read()
            .expect("original current Grants account");
        assert_eq!(account.bound, owed.bound);
        assert_eq!(account.request_id, Some(owed.request));
        assert_eq!(account.ticket, Some(owed.ticket));
        assert_eq!(
            tail.rollback_test_measurement().as_deref(),
            Some(owed.bytes.as_slice())
        );
        assert!(tail.context.inspection.is_some());
        assert!(tail.context.flow.operation_completion().is_none());
        if mode == "rollback-failed-grant-recorded" {
            let seq = account
                .trace_sequence
                .expect("available owed measurement is Recorded");
            assert!(
                tail.context
                    .halt_report()
                    .unwrap()
                    .fault_sequence()
                    .unwrap()
                    < seq
            );
            assert_eq!(
                tail.context.barrier.recorded_author_ticket(seq),
                Some(owed.ticket)
            );
        } else {
            assert!(account.trace_sequence.is_none());
            assert_eq!(
                tail.context.barrier.pending_author().map(|a| a.ticket),
                Some(owed.ticket)
            );
        }
    }
    if matches!(
        mode,
        "rollback-failed-trace"
            | "rollback-failed-wait"
            | "rollback-failed-drain-cap"
            | "rollback-failed-leave-cap"
            | "rollback-failed-unload"
            | "rollback-failed-recorded-cap"
            | "rollback-failed-grant-pressure"
    ) {
        assert!(guard.0.abandoned());
        if matches!(mode, "rollback-failed-leave-cap" | "rollback-failed-unload") {
            assert!(guard.0.unload_sequence().is_none());
        }
        if matches!(
            mode,
            "rollback-failed-drain-cap" | "rollback-failed-recorded-cap"
        ) {
            assert_eq!(guard.0.unload_sequence(), recorded.map(|s| s.0));
        }
        return;
    }
    assert!(!guard.0.abandoned());
    let terminal = guard.0.unload_sequence().expect("actual terminal receipt");
    let rows = trace_through(reader, trace, Sequence(terminal), until).unwrap();
    assert_eq!(rows.iter().filter(|r| r["kind"] == "unload").count(), 1);
    assert!(
        rows.iter()
            .all(|r| !matches!(r["kind"].as_str(), Some("turn.started" | "save_point")))
    );
    let unload = rows.last().unwrap();
    assert_eq!(unload["kind"], "unload");
    assert!(unload["payload"]["release"].get("spu").is_none());
    if accepted.is_some() {
        assert_eq!(unload["payload"]["cause"]["uid"], load.uid);
        assert_eq!(unload["payload"]["forced"], true);
    } else {
        assert!(unload["payload"]["cause"].is_null());
        assert_eq!(unload["payload"]["forced"], false);
    }
    if route == ExitRoute::Recorded {
        assert_eq!(terminal, recorded.unwrap().0);
        assert_eq!(unload["payload"]["grant_surface"], "unchanged");
        assert_eq!(unload["payload"]["release"]["member"], "closed");
        assert_ne!(unload["payload"]["end_reason"], "component_failure");
        assert_eq!(
            rows.iter().filter(|r| r["kind"] == "fault").count(),
            0,
            "no fault may follow Recorded Unload"
        );
    } else {
        assert_eq!(unload["payload"]["end_reason"], "component_failure");
        assert_eq!(rows.iter().filter(|r| r["kind"] == "fault").count(), 1);
    }
}

fn fill_trace(context: &mut RecordingContext, clock: &mut FakePumpIo) {
    use crate::flow::recording::{RecordProgress, RecordPurpose};
    let payload =
        serde_json::value::to_raw_value(&serde_json::json!({"fixture":"x".repeat(32_768)}))
            .unwrap();
    for _ in 0..weaver_trace::serving::ORDINARY_CAPACITY + 64 {
        if context.barrier.recorder().boundary().queued == weaver_trace::serving::ORDINARY_CAPACITY
        {
            return;
        }
        let now = clock.now().unwrap();
        let progress = context.barrier.submit_authored_with_clock(
            crate::flow::context::AuthoredEvent {
                kind: weaver_trace::serving::Kind::Refusal,
                subsystem: weaver_trace::Subsystem::Harness,
                turn: None,
                source: crate::flow::author::AuthoredSource::Runtime,
                causal_parent: None,
                payload: Some(weaver_trace::serving::Payload::Refusal(payload.clone())),
            },
            RecordPurpose::Observation,
            &context.author,
            1,
            &mut context.inbox,
            &mut context.budget,
            &mut || Ok(now),
        );
        assert!(
            matches!(progress, RecordProgress::Recorded { .. }),
            "pressure: {progress:?}"
        );
    }
    panic!("ordinary trace queue did not fill");
}
struct BrokenClock;
impl PumpIo for BrokenClock {
    fn now(&mut self) -> std::io::Result<u64> {
        Err(std::io::Error::other("fixture clock failure"))
    }
    fn wait(
        &mut self,
        _: &[WaitInterest<'_>],
        _: Option<u64>,
    ) -> std::io::Result<crate::flow::clock::WaitOutcome> {
        panic!("clock must fail first")
    }
}
struct BrokenWait<'a>(&'a mut FakePumpIo);
impl PumpIo for BrokenWait<'_> {
    fn now(&mut self) -> std::io::Result<u64> {
        self.0.now()
    }
    fn wait(
        &mut self,
        _: &[WaitInterest<'_>],
        deadline: Option<u64>,
    ) -> std::io::Result<crate::flow::clock::WaitOutcome> {
        assert!(deadline.is_some());
        Err(std::io::Error::other("fixture wait failure"))
    }
}
struct ExpireWait<'a> {
    clock: &'a mut FakePumpIo,
    cap: u64,
}
impl PumpIo for ExpireWait<'_> {
    fn now(&mut self) -> std::io::Result<u64> {
        self.clock.now()
    }
    fn wait(
        &mut self,
        _: &[WaitInterest<'_>],
        deadline: Option<u64>,
    ) -> std::io::Result<crate::flow::clock::WaitOutcome> {
        assert_eq!(deadline, Some(self.cap));
        self.clock.advance_to(self.cap)?;
        Ok(crate::flow::clock::WaitOutcome::Timeout)
    }
}

struct FailFirstClock<'a> {
    clock: &'a mut FakePumpIo,
    failed: bool,
}
impl PumpIo for FailFirstClock<'_> {
    fn now(&mut self) -> std::io::Result<u64> {
        if !self.failed {
            self.failed = true;
            Err(std::io::Error::other("fixture shared clock failure"))
        } else {
            self.clock.now()
        }
    }
    fn wait(
        &mut self,
        _: &[WaitInterest<'_>],
        _: Option<u64>,
    ) -> std::io::Result<crate::flow::clock::WaitOutcome> {
        panic!("clock fault before any wait")
    }
}
