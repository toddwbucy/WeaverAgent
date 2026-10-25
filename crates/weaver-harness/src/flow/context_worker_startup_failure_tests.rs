use super::super::tests::{Member, packet, reap, trace_through, worker};
use super::*;
use crate::flow::clock::testing::FakePumpIo;
use crate::flow::context::startup_composition::tests::CircuitIo;
use crate::flow::pump::PumpPorts;
use std::io::Read;
use std::os::fd::OwnedFd;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};

struct EnterGuard<'a>(Option<FoundationEnter<'a>>);
impl Drop for EnterGuard<'_> {
    fn drop(&mut self) {
        let child = match self.0.as_mut().and_then(|owner| owner.state.as_mut()) {
            Some(EnterState::Gate(owner)) => owner.test_gate_child(),
            Some(EnterState::Raised(owner)) => owner.test_gate_child(),
            Some(EnterState::Ready(owner)) => owner.test_gate_child(),
            Some(EnterState::Entered(owner)) => Some(&mut owner.gate_child),
            _ => None,
        };
        if let Some(child) = child {
            reap(child);
        }
    }
}
struct Guard(Box<FoundationStartupExit>);
impl Drop for Guard {
    fn drop(&mut self) {
        if let Some(child) = self
            .0
            .handoff
            .gate
            .as_mut()
            .and_then(|arms| arms.child.as_mut())
        {
            reap(child);
        }
        self.0.tail.reap_composition_test_children();
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

pub(in crate::flow::context::worker_startup) fn context<'b>(
    enter: &'b mut FoundationEnter<'_>,
) -> &'b mut RecordingContext {
    match enter.state.as_mut().unwrap() {
        EnterState::Startup(owner) => owner.failure_test_context(),
        EnterState::Residency(owner) => owner.failure_test_context(),
        EnterState::Boot(owner) => owner.failure_test_context(),
        EnterState::Gate(owner) => owner.failure_test_context(),
        EnterState::Ready(owner) => owner.failure_test_context(),
        _ => panic!("unexpected failure injection stage"),
    }
}
pub(in crate::flow::context::worker_startup) fn reached(
    enter: &FoundationEnter<'_>,
    mode: &str,
) -> bool {
    use crate::flow::context::boot_composition::BootStage;
    use crate::flow::context::startup_composition::StartupStage;
    match (mode, enter.state.as_ref().unwrap()) {
        ("failed-startup-preopen", EnterState::Startup(owner)) => {
            owner.stage() == StartupStage::Open
        }
        ("failed-startup-opened", EnterState::Startup(owner)) => {
            owner.stage() == StartupStage::Load
        }
        ("failed-startup-activation", EnterState::Startup(owner)) => {
            owner.stage() == StartupStage::Activate
        }
        ("failed-startup-loaded", EnterState::Startup(owner)) => {
            owner.stage() == StartupStage::OpenMeasurement
        }
        ("failed-startup-classifier", EnterState::Residency(owner)) => {
            owner.stage() == ResidencyStage::Fork
        }
        ("failed-startup-classifying", EnterState::Startup(owner)) => {
            owner.failure_test_classifying()
        }
        ("failed-startup-admit", EnterState::Residency(owner)) => {
            owner.stage() == ResidencyStage::Admit
        }
        ("failed-startup-admit-sent", EnterState::Residency(owner)) => {
            owner.stage() == ResidencyStage::Admit && owner.failure_test_admit_sent()
        }
        ("failed-startup-boot", EnterState::Boot(owner)) => owner.stage() == BootStage::Templates,
        ("failed-startup-prompt", EnterState::Boot(owner)) => owner.stage() == BootStage::Prompt,
        ("failed-startup-summary-read", EnterState::Boot(owner)) => {
            owner.stage() == BootStage::Summary
        }
        ("failed-startup-open", EnterState::Boot(owner)) => {
            owner.stage() == BootStage::Open && owner.failure_test_open_sent()
        }
        ("failed-startup-open-unsent", EnterState::Boot(owner)) => owner.stage() == BootStage::Open,
        ("failed-startup-repair", EnterState::Boot(owner)) => owner.stage() == BootStage::Repair,
        ("failed-startup-summary", EnterState::Boot(owner)) => owner.stage() == BootStage::Install,
        ("failed-startup-installed", EnterState::Gate(owner)) => {
            owner.stage() == GateStartupStage::Boundary
        }
        (_, EnterState::Ready(_)) => true,
        _ => false,
    }
}

pub(in crate::flow::context::worker_startup) fn enter(
    enter: FoundationEnter<'_>,
    member: &mut Member,
    reader: &mut UnixStream,
    clock: &mut FakePumpIo,
    mode: &str,
) -> Option<Box<FoundationStartupHandoff>> {
    let mut guard = EnterGuard(Some(enter));
    let _panic_reader = PanicReader(reader.try_clone().unwrap());
    let limit = Instant::now() + Duration::from_secs(8);
    if mode == "failed-startup-expiry" {
        let bound = context(guard.0.as_mut().unwrap())
            .deadlines
            .get(crate::flow::pump::PumpDeadline::Enter)
            .unwrap();
        clock.advance_to(bound.deadline_ns()).unwrap();
    }
    while !reached(guard.0.as_ref().unwrap(), mode)
        || mode.ends_with("-refusal")
        || mode.ends_with("-expiry")
    {
        assert!(
            Instant::now() < limit,
            "startup target {mode}: {:?}",
            guard.0.as_ref().unwrap().detail()
        );
        // SAFETY: the fixture retains and reaps every actual child on this thread.
        let progress = unsafe {
            guard
                .0
                .as_mut()
                .unwrap()
                .advance_with_io(&mut CircuitIo(clock))
        };
        if progress == FoundationEnterProgress::Stopped {
            break;
        }
        assert_eq!(
            progress,
            FoundationEnterProgress::Pending,
            "failure target must precede Ready"
        );
        member.step(clock.now().unwrap(), "ready");
        std::thread::sleep(Duration::from_millis(1));
    }
    if mode.ends_with("-refusal") || mode.ends_with("-expiry") {
        assert_eq!(
            guard.0.as_ref().unwrap().progress(),
            FoundationEnterProgress::Stopped
        );
        assert!(context(guard.0.as_mut().unwrap()).failure().is_none());
        let before = guard.0.as_ref().unwrap().detail().unwrap().to_owned();
        guard.0 = Some(
            guard
                .0
                .take()
                .unwrap()
                .failed_handoff()
                .err()
                .expect("healthy stopped owner is not a component failure"),
        );
        assert_eq!(guard.0.as_ref().unwrap().detail(), Some(before.as_str()));
        return None;
    }
    assert!(reached(guard.0.as_ref().unwrap(), mode));
    if mode.ends_with("-trace") {
        fill_trace(context(guard.0.as_mut().unwrap()), clock);
    }
    member.stop_responses();
    let context = context(guard.0.as_mut().unwrap());
    let confirmed = if matches!(
        mode,
        "failed-startup-opened" | "failed-startup-activation" | "failed-startup-loaded"
    ) {
        Some(
            context
                .flow
                .standing_state()
                .unwrap()
                .held_measurement()
                .unwrap()
                .harness_received_ns
                .0,
        )
    } else {
        None
    };
    // The actual member socket supplies EOF. No fabricated failure is assigned.
    let mut ports = PumpPorts {
        lifecycle: Default::default(),
        gate: None,
        decode: None,
        label: None,
        io: &mut CircuitIo(clock),
    };
    let _ = context.pump_once(&mut ports);
    assert!(
        context.failure().is_some(),
        "actual EOF must latch at {mode}"
    );
    assert_eq!(context.phase, RecordingPhase::Failed);
    if let Some(confirmed) = confirmed {
        assert_eq!(
            context
                .flow
                .final_sample()
                .unwrap()
                .unwrap()
                .sample_start_ns
                .0,
            confirmed,
            "final sample retains actual Open confirmation before its measurement releases"
        );
    }
    // SAFETY: the stopped owner is consumed below; fixture guards retain children.
    assert_eq!(
        unsafe {
            guard
                .0
                .as_mut()
                .unwrap()
                .advance_with_io(&mut CircuitIo(clock))
        },
        FoundationEnterProgress::Stopped
    );
    let owner = guard.0.take().unwrap();
    match owner.failed_handoff() {
        Ok(handoff) => Some(handoff),
        Err(owner) => {
            guard.0 = Some(owner);
            panic!("actual failed startup could not detach");
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(in crate::flow::context::worker_startup) fn finish(
    mut run: Box<FoundationRun>,
    mut handoff: Box<FoundationStartupHandoff>,
    peer: &OwnedFd,
    member: &mut Member,
    reader: &mut UnixStream,
    clock: &mut FakePumpIo,
    path: &Path,
    original: &[u8],
    mode: &str,
) {
    assert_eq!(handoff.original.as_ref().unwrap().retained_wire(), original);
    assert!(!path.exists(), "failure seals the actual listener");
    assert!(
        packet(peer).is_none(),
        "failed Enter emits neither Ready nor Left"
    );
    let boot = match handoff.held.as_ref() {
        HeldStartup::Boot(boot) => Some(boot),
        HeldStartup::Gate(gate) | HeldStartup::Ready { gate, .. } => Some(&gate.boot),
        _ => None,
    };
    if let Some(boot) = boot {
        if mode != "failed-startup-boot" {
            assert!(boot.certificate.is_some());
        }
        if mode == "failed-startup-repair" {
            assert!(boot.prior.is_some());
        }
        if mode == "failed-startup-summary" {
            assert!(boot.summary.is_some());
        }
    }
    let admitted = match handoff.held.as_ref() {
        HeldStartup::Residency(account) => account.reply.as_ref(),
        HeldStartup::Admitted(_, resident) => resident.reply.as_ref(),
        _ => boot
            .and_then(|boot| boot.resident.as_ref())
            .and_then(|resident| resident.reply.as_ref()),
    }
    .map(|reply| {
        (
            reply.raw_bytes().as_ptr() as usize,
            reply.raw_bytes().to_vec(),
        )
    });
    let first = run.context.failure().unwrap().clone();
    let deadline = handoff.identity.deadline;
    let expected_spu = run.resources.spu_child().is_some();
    let expected_load = !matches!(mode, "failed-startup-preopen" | "failed-startup-opened");
    let _panic_reader = PanicReader(reader.try_clone().unwrap());
    if mode.ends_with("-lower-early") {
        let port = handoff.gate.as_mut().unwrap().port.as_mut().unwrap();
        let until = Instant::now() + Duration::from_secs(3);
        while port.held().is_none() {
            assert!(Instant::now() < until, "premature GateStopped not received");
            port.try_receive().unwrap();
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    if mode.ends_with("-foreign") {
        let mut foreign =
            crate::flow::context::tests::Fixture::new(crate::flow::context::tests::config());
        let mut context =
            crate::flow::context::failed_enter_driver::tests::take_context(&mut foreign);
        let resources = context.failed_enter_resources().unwrap();
        let foreign = Box::new(FoundationRun {
            context,
            resources,
            listener: None,
            original: None,
        });
        let (foreign, returned) = foreign
            .failed_startup_exit(handoff)
            .err()
            .expect("foreign run rejected");
        assert!(foreign.context.failure().is_none());
        drop(foreign);
        handoff = returned;
        let (near, far) = crate::channel::DecodeChannel::pair().unwrap();
        let mut replacement = crate::flow::pump_transport::DecodePort::from_channel(near).unwrap();
        std::mem::swap(run.resources.decode_mut().unwrap(), &mut replacement);
        let (returned_run, returned_handoff) = run
            .failed_startup_exit(handoff)
            .err()
            .expect("substitute decode rejected");
        run = returned_run;
        handoff = returned_handoff;
        assert_eq!(run.context.failure(), Some(&first));
        std::mem::swap(run.resources.decode_mut().unwrap(), &mut replacement);
        drop((replacement, far));
    }
    let exit = if mode.ends_with("-worker") {
        worker::failed_startup_exit(run, handoff)
    } else {
        run.failed_startup_exit(handoff)
    }
    .ok()
    .expect("matching run and original custody");
    let mut guard = Guard(exit);
    let _panic_after_owner = PanicReader(reader.try_clone().unwrap());
    assert_eq!(guard.0.tail.failure(), &first);
    assert_eq!(guard.0.tail.deadline(), deadline);
    if let Some((pointer, bytes)) = admitted {
        let retained = guard
            .0
            .tail
            .startup_test_admit_reply()
            .expect("consumed Admit reply retained");
        assert_eq!(retained.raw_bytes(), bytes);
        assert_eq!(
            retained.raw_bytes().as_ptr() as usize,
            pointer,
            "original packet moved, never reconstructed"
        );
    }
    if mode.ends_with("-cap") {
        clock.advance_to(deadline.unwrap()).unwrap();
    }
    if !mode.ends_with("-worker") {
        let limit = Instant::now() + Duration::from_secs(8);
        let mut wait_failed = false;
        loop {
            assert!(
                Instant::now() < limit,
                "failed startup cleanup stalled in {:?}",
                guard.0.stage
            );
            let reporting = guard.0.stage == ExitStage::Reporting;
            let progress = guard.0.advance_with_io(&mut CircuitIo(clock));
            assert_eq!(guard.0.tail.failure(), &first);
            assert_eq!(guard.0.tail.deadline(), deadline);
            if reporting && expected_load && !mode.ends_with("-cap") {
                assert!(
                    guard
                        .0
                        .tail
                        .context
                        .halt_report()
                        .unwrap()
                        .fault_sequence()
                        .is_some(),
                    "first fault precedes Lower and owed Open accounting"
                );
                assert!(guard.0.lower_send.is_none());
            }
            if progress == FoundationStartupExitProgress::Closed {
                break;
            }
            if mode.ends_with("-trace") && guard.0.unload_sequence().is_some() {
                assert_eq!(guard.0.progress(), FoundationStartupExitProgress::Progress);
                clock.advance_to(deadline.unwrap()).unwrap();
            } else if progress == FoundationStartupExitProgress::Pending {
                if mode.ends_with("-lower-silent") && guard.0.stage == ExitStage::Gate {
                    assert!(guard.0.lower_send.as_ref().is_some_and(OrganSend::sent));
                    guard.0.wait_with_io(clock);
                    assert_eq!(clock.waits().last().unwrap().absolute_deadline, deadline);
                } else if mode.ends_with("-wait") && !wait_failed {
                    let mut broken = BrokenWait(clock);
                    guard.0.wait_with_io(&mut broken);
                    wait_failed = true;
                } else {
                    guard.0.wait_with_io(&mut CircuitIo(clock));
                }
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        if mode.ends_with("-wait") {
            assert!(wait_failed);
        }
    }
    assert_eq!(guard.0.channel_fault(), Some(crate::ChannelFault::Closed));
    if let Some(arms) = &guard.0.handoff.gate {
        assert!(
            arms.port.is_none(),
            "Closed retires the actual gate descriptor"
        );
        assert!(arms.tools.is_none(), "Closed retires the tools handle");
    }
    assert!(packet(peer).is_none());
    member.assert_closed_without_request();
    if mode.ends_with("-cap")
        || mode.ends_with("-wait")
        || mode.ends_with("-trace")
        || mode.ends_with("-lower-silent")
    {
        assert!(guard.0.abandoned());
        if !mode.ends_with("-trace") {
            assert!(guard.0.unload_sequence().is_none());
        }
        return;
    }
    assert!(!guard.0.abandoned());
    if mode.ends_with("-lower-wrong") || mode.ends_with("-lower-early") {
        assert!(
            guard.0.lower_reply.is_some(),
            "bad completion remains retained"
        );
        assert!(
            guard.0.containment_started && guard.0.contained,
            "bad completion requires real containment"
        );
        if mode.ends_with("-lower-early") {
            assert!(guard.0.lower_send.is_none());
        }
    }
    if expected_load {
        let unload = guard.0.unload_sequence().expect("actual terminal record");
        reader.set_nonblocking(true).unwrap();
        let rows = trace_through(
            reader,
            vec![],
            weaver_trace::Sequence(unload),
            Instant::now() + Duration::from_secs(3),
        )
        .unwrap();
        let terminal = rows.last().unwrap();
        assert_eq!(terminal["kind"], "unload");
        assert_eq!(rows.iter().filter(|r| r["kind"] == "unload").count(), 1);
        assert_eq!(terminal["payload"]["end_reason"], "component_failure");
        assert_eq!(
            terminal["payload"]["release"].get("spu").is_some(),
            expected_spu
        );
        if expected_spu && mode != "failed-startup-admit" {
            assert_eq!(terminal["payload"]["release"]["spu"], "confirmed");
            assert!(guard.0.tail.release_outcome() == weaver_trace::ReleaseOutcome::Confirmed);
        }
        assert_eq!(terminal["payload"]["release"]["member"], "unconfirmed");
        assert_eq!(terminal["payload"]["grant_surface"], "unreadable");
        assert!(
            rows.iter()
                .all(|r| !matches!(r["kind"].as_str(), Some("turn.started" | "save_point")))
        );
        if mode == "failed-startup-open" {
            assert_eq!(
                rows.iter()
                    .filter(|r| r["kind"] == "state.placement")
                    .count(),
                1,
                "owed Open accounted exactly once"
            );
            assert!(guard.0.tail.context.boot_journal.is_none());
        }
        if matches!(
            mode,
            "failed-startup-raised" | "failed-startup-worker" | "failed-startup-foreign"
        ) {
            assert!(guard.0.lower_send.as_ref().is_some_and(OrganSend::sent));
            assert!(guard.0.lower_reply.is_some() && guard.0.contained);
        }
    } else {
        assert!(guard.0.unload_sequence().is_none());
        // Closed retains the terminal evidence until the worker consumes it.
        // The empty preflight writer closes when that last owner is dropped.
        drop(guard);
        reader.set_nonblocking(true).unwrap();
        let mut bytes = vec![];
        let until = Instant::now() + Duration::from_secs(3);
        loop {
            match reader.read_to_end(&mut bytes) {
                Ok(_) => break,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(Instant::now() < until, "pre-load writer did not close");
                    std::thread::sleep(Duration::from_millis(1));
                }
                Err(error) => panic!("pre-load trace: {error}"),
            }
        }
        assert!(bytes.is_empty(), "no fabricated pre-load bracket");
    }
}
pub(in crate::flow::context::worker_startup) fn fill_trace(
    context: &mut RecordingContext,
    clock: &mut FakePumpIo,
) {
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
            AuthoredEvent {
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
            "startup trace pressure: {progress:?}, {:?}",
            context.barrier.failure()
        );
    }
    panic!("startup ordinary trace queue did not fill");
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
        assert!(deadline.is_some(), "cleanup wait cannot be unbounded");
        Err(std::io::Error::other("fixture wait failed"))
    }
}
