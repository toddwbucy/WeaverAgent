//! The real recorder and halt owner at the observed containment-outcome seam.
//! These cases do not claim to exercise a failed kernel cgroup kill.
use super::*;
use crate::flow::clock::{WaitInterest, WaitOutcome};
use crate::flow::context::input_tests::InputFixture;
use crate::flow::halt_snapshot::ExternalFacts;
use nix::sys::socket::{AddressFamily, SockFlag, SockType, socketpair};
use std::io::Read;
use std::time::{Duration, Instant};

struct NoWaitClock(u64);
impl PumpIo for NoWaitClock {
    fn now(&mut self) -> std::io::Result<u64> {
        Ok(self.0)
    }
    fn wait(&mut self, _: &[WaitInterest<'_>], _: Option<u64>) -> std::io::Result<WaitOutcome> {
        panic!("containment failure must latch before a trace or last-word wait");
    }
}

fn ports<I: PumpIo>(io: &mut I) -> PumpPorts<'_, I> {
    PumpPorts {
        lifecycle: Default::default(),
        gate: None,
        decode: None,
        label: None,
        io,
    }
}

/// A retained observation for input's clock/trace fixtures. Actual child loss
/// and the continuing lifecycle are exercised by the worker process circuit.
pub(in crate::flow::context) fn retain_spu_for_input(context: &mut RecordingContext) {
    let (channel, _peer) = crate::channel::OrganChannel::pair().unwrap();
    let port = OrganPort::from_channel(channel).unwrap();
    context.serving_organs.spu = Some(ObservedSpu {
        decode: port.port_identity(),
        residency: port.port_identity(),
        observation: None,
    });
}

fn observed_gate(now: u64, outcome: Containment) -> Observation {
    let mut observation = Observation::new(
        "gate",
        Failure {
            fault: crate::failure::ChannelFault::Closed,
            detail: "original gate EOF".into(),
            errno: None,
        },
        false,
    );
    observation.anchor(now, None);
    observation.containment = Some(outcome);
    observation
}

pub(in crate::flow::context) fn settle_contained_gate(
    context: &mut RecordingContext,
    gate: &mut OrganPort,
    io: &mut impl PumpIo,
) -> Result<bool, RunFailure> {
    assert_eq!(gate.failure().unwrap().fault, crate::ChannelFault::Closed);
    context.serving_organs.gate = Some(ObservedGate {
        port: gate.port_identity(),
        observation: None,
    });
    let observation = observed_gate(io.now().unwrap(), Containment::Emptied);
    let mut ports = ports(io);
    ports.gate = Some(gate);
    observation.settle_gate_pairs(context, &mut ports)
}

#[test]
fn organ_last_word_advance_returns_its_original_wake_without_waiting() {
    let mut fx = InputFixture::new();
    let now = fx.run.clock.now().unwrap();
    let mut observation = observed_gate(now, Containment::Emptied);
    let settle_by = observation.settle_by.unwrap();
    let mut io = NoWaitClock(now);
    let progress = observation
        .advance(&mut fx.run.context, &mut ports(&mut io), None)
        .unwrap();
    assert!(matches!(progress, ObservationProgress::Pending { wake_ns }
        if wake_ns == (now + 10_000_000).min(settle_by)));
    assert!(observation.author.is_none());
    assert!(observation.sequence.is_none());
    assert_eq!(io.0, now);
    io.0 = settle_by;
    assert!(matches!(
        observation
            .advance(&mut fx.run.context, &mut ports(&mut io), None)
            .unwrap(),
        ObservationProgress::Complete
    ));
    assert_eq!(observation.settle_by, Some(settle_by));
    assert!(observation.sequence.is_some());
    assert_eq!(fx.records(Kind::Fault).len(), 1);
    // Exercise the getter before any halt can clear the cache or any pump can
    // phase-filter it. A Recorded observation is no longer a runnable wake.
    let (channel, _peer) = crate::channel::OrganChannel::pair().unwrap();
    let gate = OrganPort::from_channel(channel).unwrap();
    let organs = ObservedOrgans {
        gate: Some(ObservedGate {
            port: gate.port_identity(),
            observation: Some(observation),
        }),
        wake_ns: Some((true, now)),
        ..Default::default()
    };
    assert!(fx.run.context.failure().is_none());
    assert!(
        organs.wake().is_none(),
        "Recorded completion retires its cached wake without a halt"
    );
}

fn install_and_latch(fx: &mut InputFixture) -> RunFailure {
    let now = fx.run.clock.now().unwrap();
    let mut observation = observed_gate(now, Containment::PastBound);
    observation.last_word = Some(crate::spawn::LastWordSnapshot {
        word: Some(serde_json::json!({"original":"last word"})),
        eof: true,
    });
    let first = observation
        .advance(&mut fx.run.context, &mut ports(&mut NoWaitClock(now)), None)
        .err()
        .expect("containment failure");
    let (near, _far) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .unwrap();
    let port = OrganPort::from_owned(near).unwrap();
    fx.run.context.serving_organs.gate = Some(ObservedGate {
        port: port.port_identity(),
        observation: Some(observation),
    });
    let context = &mut fx.run.context;
    context
        .freeze_halt_account(ExternalFacts {
            outstanding_pairs: context.outstanding_pairs(),
            capture: None,
            wait: context.halt_wait().cloned(),
            input_id: context.input.snapshot().map(|input| input.input_id),
        })
        .unwrap();
    context.begin_halt_report().unwrap();
    assert!(matches!(
        context.advance_halt_trace(&mut fx.run.clock).unwrap(),
        HaltTraceProgress::FaultRecorded(_)
    ));
    first
}

fn drain(reader: &mut std::os::unix::net::UnixStream, context: &RecordingContext) {
    reader.set_nonblocking(true).unwrap();
    let watch = Instant::now() + Duration::from_secs(3);
    let mut bytes = [0_u8; 65536];
    while context.barrier.recorder().boundary().queued != 0 {
        assert!(Instant::now() < watch, "real trace writer must drain");
        match reader.read(&mut bytes) {
            Ok(_) => (),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::yield_now();
            }
            Err(error) => panic!("trace fixture: {error}"),
        }
    }
}

#[test]
fn organ_loss_containment_failure_latches_before_trace_or_last_word_wait() {
    for outcome in [Containment::PastBound, Containment::Undelivered] {
        let mut fx = InputFixture::new();
        fx.fill_trace();
        let now = fx.run.clock.now().unwrap();
        let mut observation = observed_gate(now, outcome);
        let first = observation
            .advance(&mut fx.run.context, &mut ports(&mut NoWaitClock(now)), None)
            .err()
            .expect("failed containment ends service before reporting");
        assert!(matches!(
            first,
            RunFailure::Flow {
                reason: FlowHaltReason::ProtocolFailed,
                ..
            }
        ));
        assert_eq!(fx.run.context.failure(), Some(&first));
        assert_eq!(fx.run.context.phase(), RecordingPhase::Failed);
        assert!(fx.run.context.barrier.recorder().tap_stopped());
        assert!(observation.sequence.is_none());
        assert!(fx.records(Kind::Fault).is_empty());
    }
}

#[test]
fn organ_loss_halt_preserves_exact_ticket_and_records_once_without_a_live_copy() {
    let mut fx = InputFixture::new();
    fx.fill_trace();
    let first = install_and_latch(&mut fx);
    fx.run.context.serving_organs.wake_ns = Some((true, fx.run.context.last_ns));
    let deadline = fx.run.context.halt().unwrap().deadline();
    let copies = fx.run.context.flow.ledger().facts().count();
    assert_eq!(
        fx.run
            .context
            .advance_halt_trace(&mut fx.run.clock)
            .unwrap(),
        HaltTraceProgress::Pending
    );
    assert!(
        fx.run.context.serving_organs.wake().is_none(),
        "halt must retire the serving observation's old wake"
    );
    let ticket = fx.run.context.pending_author().unwrap().ticket;
    let original = format!("{:?}", fx.run.context.pending_author().unwrap().event);
    for _ in 0..3 {
        assert_eq!(
            fx.run
                .context
                .advance_halt_trace(&mut fx.run.clock)
                .unwrap(),
            HaltTraceProgress::Pending
        );
        assert_eq!(fx.run.context.pending_author().unwrap().ticket, ticket);
        assert_eq!(
            format!("{:?}", fx.run.context.pending_author().unwrap().event),
            original
        );
    }
    assert_eq!(
        fx.records(Kind::Fault).len(),
        1,
        "only the reserved halt fault"
    );
    fx.drain_trace();
    let HaltTraceProgress::TraceRecorded(sequence) = fx
        .run
        .context
        .advance_halt_trace(&mut fx.run.clock)
        .unwrap()
    else {
        panic!("owed ordinary observation records after capacity returns")
    };
    assert_eq!(
        fx.run
            .context
            .advance_halt_trace(&mut fx.run.clock)
            .unwrap(),
        HaltTraceProgress::Idle
    );
    assert_eq!(
        fx.run
            .context
            .serving_organs
            .observation(true)
            .unwrap()
            .sequence,
        Some(sequence)
    );
    assert!(fx.run.context.serving_observation_author.is_none());
    let faults = fx.records(Kind::Fault);
    assert_eq!(faults.len(), 2);
    assert_eq!(faults[1]["payload"]["case"], "organ_death_observed");
    assert!(faults[1]["turn"].is_null());
    assert!(faults[1].to_string().contains("original gate EOF"));
    assert!(faults[1].to_string().contains("last word"));
    assert_eq!(fx.run.context.failure(), Some(&first));
    assert_eq!(fx.run.context.halt().unwrap().deadline(), deadline);
    assert_eq!(fx.run.context.flow.ledger().facts().count(), copies);
    assert!(fx.run.context.inbox.queued().next().is_none());
}

#[test]
fn organ_loss_halt_record_survives_turn_account_retirement_into_release_tail() {
    let mut fx = InputFixture::new();
    fx.fill_trace();
    let first = install_and_latch(&mut fx);
    assert_eq!(
        fx.run
            .context
            .advance_halt_trace(&mut fx.run.clock)
            .unwrap(),
        HaltTraceProgress::Pending
    );
    let ticket = fx.run.context.pending_author().unwrap().ticket;
    let resources = fx.run.context.failed_enter_resources().unwrap();
    let context = failed_enter_driver::tests::take_context(&mut fx.run);
    let mut cleanup =
        failed_enter_driver::FailedRunCleanup::after_serving(context, resources, false);
    assert_eq!(
        cleanup.settle_serving_turn_accounts(&mut fx.run.clock),
        Some(true)
    );
    assert_eq!(cleanup.context().pending_author().unwrap().ticket, ticket);
    drain(&mut fx.run._reader, cleanup.context());
    assert!(cleanup.advance_accounts(&mut fx.run.clock));
    assert!(
        cleanup
            .context()
            .serving_organs
            .observation(true)
            .unwrap()
            .sequence
            .is_some()
    );
    assert!(cleanup.context().pending_author().is_none());
    assert_eq!(cleanup.context().failure(), Some(&first));
    assert_eq!(
        cleanup
            .context()
            .barrier
            .recorder()
            .structure()
            .by_kind(Kind::Fault)
            .count(),
        2
    );
}

#[test]
fn organ_loss_halt_trace_expiry_preserves_first_cause_and_original_event() {
    let mut fx = InputFixture::new();
    fx.fill_trace();
    let first = install_and_latch(&mut fx);
    assert_eq!(
        fx.run
            .context
            .advance_halt_trace(&mut fx.run.clock)
            .unwrap(),
        HaltTraceProgress::Pending
    );
    let ticket = fx.run.context.pending_author().unwrap().ticket;
    let deadline = fx.run.context.halt().unwrap().deadline().unwrap();
    fx.run.clock.advance_to(deadline).unwrap();
    assert_eq!(
        fx.run.context.advance_halt_organ_trace(&mut fx.run.clock),
        HaltTraceProgress::AbortRequired
    );
    assert_eq!(fx.run.context.pending_author().unwrap().ticket, ticket);
    assert_eq!(fx.run.context.failure(), Some(&first));
    assert_eq!(fx.run.context.halt().unwrap().deadline().unwrap(), deadline);
    assert_eq!(fx.records(Kind::Fault).len(), 1);
    assert!(
        fx.run
            .context
            .serving_organs
            .observation(true)
            .unwrap()
            .sequence
            .is_none()
    );
}

#[test]
fn organ_loss_halt_keeps_unrecorded_original_before_session_close_and_never_retries_it() {
    let mut fx = InputFixture::new();
    fx.fill_trace();
    let first = install_and_latch(&mut fx);
    assert_eq!(
        fx.run
            .context
            .advance_halt_trace(&mut fx.run.clock)
            .unwrap(),
        HaltTraceProgress::Pending
    );
    let ticket = fx.run.context.pending_author().unwrap().ticket;
    let prepared = format!("{:?}", fx.run.context.barrier.pending().unwrap());
    let resources = fx.run.context.failed_enter_resources().unwrap();
    let context = failed_enter_driver::tests::take_context(&mut fx.run);
    let mut cleanup =
        failed_enter_driver::FailedRunCleanup::after_serving(context, resources, false);
    assert_eq!(
        cleanup.settle_serving_turn_accounts(&mut fx.run.clock),
        Some(true)
    );
    for _ in 0..16 {
        if cleanup.unload_sequence().is_some() {
            break;
        }
        cleanup.advance(&mut fx.run.clock);
    }
    assert!(
        cleanup.unload_sequence().is_some(),
        "ordinary pressure cannot hold mandatory Unload"
    );
    let observation = cleanup.context().serving_organs.observation(true).unwrap();
    assert!(observation.sequence.is_none());
    assert!(observation.unavailable.is_some());
    let (original, held_ticket) = observation
        .stopped_record
        .as_ref()
        .unwrap()
        .test_record()
        .unwrap();
    assert_eq!(held_ticket, ticket);
    assert_eq!(format!("{original:?}"), prepared);
    assert!(cleanup.context().pending_author().is_none());
    assert!(cleanup.context().serving_observation_author.is_none());
    let structure = cleanup.context().barrier.recorder().structure();
    let closes: Vec<_> = structure.by_kind(Kind::SessionClosed).collect();
    assert_eq!(closes.len(), 1);
    assert_eq!(structure.by_kind(Kind::Fault).count(), 1);
    assert_eq!(cleanup.context().failure(), Some(&first));
    drain(&mut fx.run._reader, cleanup.context());
    for _ in 0..16 {
        if cleanup.advance(&mut fx.run.clock) == FailedEnterProgress::Closed {
            break;
        }
    }
    assert_eq!(cleanup.context().phase(), RecordingPhase::Closed);
    assert_eq!(
        cleanup
            .context()
            .barrier
            .recorder()
            .structure()
            .by_kind(Kind::Fault)
            .count(),
        1
    );
    assert!(
        cleanup
            .context()
            .serving_organs
            .observation(true)
            .unwrap()
            .sequence
            .is_none()
    );
}

#[test]
fn organ_loss_halt_records_in_observation_order_when_spu_precedes_gate() {
    let mut fx = InputFixture::new();
    let now = fx.run.clock.now().unwrap();
    let mut spu = Observation::new(
        "spu",
        Failure {
            fault: crate::failure::ChannelFault::Closed,
            detail: "earlier original SPU EOF".into(),
            errno: None,
        },
        false,
    );
    spu.anchor(now, None);
    spu.last_word = Some(crate::spawn::LastWordSnapshot {
        word: None,
        eof: true,
    });
    let (near, _far) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .unwrap();
    let decode = DecodePort::from_owned(near).unwrap();
    let (near, _far) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .unwrap();
    let residency = OrganPort::from_owned(near).unwrap();
    fx.run.context.serving_organs.spu = Some(ObservedSpu {
        decode: decode.port_identity(),
        residency: residency.port_identity(),
        observation: Some(spu),
    });
    fx.run.context.serving_organs.first_gate = Some(false);
    let first = install_and_latch(&mut fx);
    for _ in 0..4 {
        if fx
            .run
            .context
            .advance_halt_trace(&mut fx.run.clock)
            .unwrap()
            == HaltTraceProgress::Idle
        {
            break;
        }
    }
    let faults = fx.records(Kind::Fault);
    assert_eq!(faults.len(), 3);
    assert_eq!(faults[0]["payload"]["case"], "state_flow_failed");
    assert_eq!(faults[1]["payload"]["account"]["organ"], "spu");
    assert_eq!(faults[2]["payload"]["account"]["organ"], "gate");
    assert_eq!(fx.run.context.failure(), Some(&first));
    assert!(!fx.run.context.has_owed_organ_observation());
}

#[test]
fn closed_gate_receive_debt_stays_runnable_before_its_terminal_deadline() {
    let mut fx = InputFixture::new();
    let now = fx.run.clock.now().unwrap();
    let (channel, peer) = crate::channel::OrganChannel::pair().unwrap();
    let mut gate = OrganPort::from_channel(channel).unwrap();
    gate.stage(weaver_types::OrganEnvelope {
        exchange: weaver_types::ExchangeId {
            opener: weaver_types::Opener::Harness,
            ordinal: 99,
        },
        position: weaver_types::Position::Open,
        payload: weaver_types::Payload::Directive(weaver_types::LifecycleDirective::Lower),
    })
    .unwrap();
    gate.try_send().unwrap();
    // Closing with the unread request produces the actual reset ahead of EOF.
    drop(peer);
    let failure = gate.try_receive().unwrap_err();
    assert_eq!(failure.fault, crate::ChannelFault::Closed);
    assert!(gate.closed_receive_pending());
    fx.run.context.serving_organs.gate = Some(ObservedGate {
        port: gate.port_identity(),
        observation: None,
    });
    let bound = AbsoluteBound::after(now, 100).unwrap();
    fx.run
        .context
        .arm(PumpDeadline::Decision, bound, &mut fx.run.clock)
        .unwrap();
    let mut io = crate::flow::clock::testing::FakePumpIo::new(now);
    let mut ports = ports(&mut io);
    ports.gate = Some(&mut gate);
    assert_eq!(
        fx.run.context.pump_with_wait(&mut ports, true).unwrap(),
        PumpStep::Observation
    );
    assert!(
        !ports.gate.as_deref().unwrap().closed_receive_pending(),
        "the common pump consumed the closed-tail wake even without an input owner"
    );
    assert_eq!(
        fx.run.context.deadlines.get(PumpDeadline::Decision),
        Some(bound)
    );
    assert_eq!(
        ports.io.waits().last().unwrap().absolute_deadline,
        Some(now)
    );
    ports.io.advance_to(bound.deadline_ns()).unwrap();
    assert!(matches!(
        fx.run.context.pump_with_wait(&mut ports, true),
        Err(RunFailure::Flow {
            reason: FlowHaltReason::DecisionBound,
            ..
        })
    ));
}

#[test]
fn contained_gate_with_no_executing_call_does_not_create_a_terminal_wait() {
    super::super::pair_tests::with_recorded_pair_batch(|context, clock| {
        let now = clock.now().unwrap();
        // Isolate the settlement boundary with a real checked batch and owner.
        // No executing call or terminal receipt authorizes a new terminal wait.
        context.pair_work.as_mut().unwrap().bound = None;
        assert!(context.complete_wait(PumpDeadline::Decision).is_some());
        let (channel, peer) = crate::channel::OrganChannel::pair().unwrap();
        let mut gate = OrganPort::from_channel(channel).unwrap();
        drop(peer);
        assert_eq!(
            gate.try_receive().unwrap_err().fault,
            crate::ChannelFault::Closed
        );
        gate.receive_closed().unwrap();
        assert!(!gate.closed_receive_pending());
        context.serving_organs.gate = Some(ObservedGate {
            port: gate.port_identity(),
            observation: None,
        });
        let mut ports = ports(clock);
        ports.gate = Some(&mut gate);
        let observation = observed_gate(now, Containment::Emptied);
        assert!(observation.settle_gate_pairs(context, &mut ports).unwrap());
        assert!(context.deadlines.get(PumpDeadline::Decision).is_none());
        assert!(context.pair_work.as_ref().unwrap().bound.is_none());
        assert!(context.pairs.executing().is_none());
    });
}

#[test]
fn organ_observation_resumes_its_recorded_completion_after_save_expiry() {
    use crate::flow::context::serving_observation::tests::{expire_after_record, record_next_save};
    let mut fx = InputFixture::new();
    let now = fx.run.clock.now().unwrap();
    let mut observation = observed_gate(now, Containment::Emptied);
    observation.last_word = Some(crate::spawn::LastWordSnapshot {
        word: None,
        eof: true,
    });
    observation.prepare(&fx.run.context);
    let bound = AbsoluteBound::after(now, 100).unwrap();
    fx.run
        .context
        .arm(PumpDeadline::SaveQueued, bound, &mut fx.run.clock)
        .unwrap();
    let owner = Arc::clone(observation.author.as_ref().unwrap());
    let mut io = expire_after_record(Arc::clone(&owner), now, bound.deadline_ns());
    let mut ports = ports(&mut io);
    assert!(
        matches!(observation.advance(&mut fx.run.context, &mut ports, None), Err(RunFailure::Wait { key: PumpDeadline::SaveQueued, bound: actual }) if actual == bound)
    );
    let sequence = owner
        .sequence()
        .expect("actual record precedes post-work expiry");
    assert_eq!(fx.run.context.completed_author, Some(sequence));
    assert!(fx.run.context.serving_observation_author.is_some());
    assert!(observation.started);
    assert!(
        observation.sequence.is_none(),
        "Recorded evidence alone is not completed continuation"
    );
    assert_eq!(
        fx.run.context.complete_wait(PumpDeadline::SaveQueued),
        Some(bound)
    );
    assert!(matches!(
        observation
            .advance(&mut fx.run.context, &mut ports, None)
            .unwrap(),
        ObservationProgress::Complete
    ));
    assert_eq!(observation.sequence, Some(sequence));
    assert!(fx.run.context.completed_author.is_none());
    assert!(fx.run.context.serving_observation_author.is_none());
    assert!(record_next_save(&mut fx.run.context, &mut ports).0 > sequence.0);
    assert_eq!(fx.records(Kind::Fault).len(), 1);
    assert_eq!(fx.records(Kind::SavePoint).len(), 1);
}

#[test]
fn closed_gate_malformed_tail_returns_its_refusal_without_an_input_driver() {
    use std::os::fd::AsRawFd;
    let mut fx = InputFixture::new();
    let now = fx.run.clock.now().unwrap();
    let (channel, peer) = crate::channel::OrganChannel::pair().unwrap();
    let mut gate = OrganPort::from_channel(channel).unwrap();
    gate.stage(weaver_types::OrganEnvelope {
        exchange: weaver_types::ExchangeId {
            opener: weaver_types::Opener::Harness,
            ordinal: 99,
        },
        position: weaver_types::Position::Open,
        payload: weaver_types::Payload::Directive(weaver_types::LifecycleDirective::Lower),
    })
    .unwrap();
    gate.try_send().unwrap();
    let raw = b"{ \"malformed_tail\" : [";
    nix::sys::socket::send(
        peer.as_fd().as_raw_fd(),
        raw,
        nix::sys::socket::MsgFlags::MSG_NOSIGNAL,
    )
    .unwrap();
    drop(peer);
    let first = gate.try_receive().unwrap_err();
    assert_eq!(first.fault, crate::ChannelFault::Closed);
    fx.run.context.serving_organs.gate = Some(ObservedGate {
        port: gate.port_identity(),
        observation: None,
    });
    let mut io = crate::flow::clock::testing::FakePumpIo::new(now);
    let mut ports = ports(&mut io);
    ports.gate = Some(&mut gate);
    assert!(matches!(fx.run.context.pump_with_wait(&mut ports, true),
        Err(RunFailure::Transport { source: PumpSource::Gate, failure })
            if failure.fault == crate::ChannelFault::Undecodable));
    let gate = ports.gate.as_deref_mut().unwrap();
    for _ in 0..2 {
        assert!(
            !gate.closed_receive_pending(),
            "retained refusal is no receive work"
        );
        assert_eq!(
            gate.receive_closed().unwrap_err().fault,
            crate::ChannelFault::Undecodable
        );
        assert_eq!(gate.failure(), Some(&first));
        assert_eq!(gate.failed_wire(), Some(raw.as_slice()));
    }
}
