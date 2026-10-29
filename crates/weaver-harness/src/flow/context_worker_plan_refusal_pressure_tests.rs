//! Trace capacity and the boundary between Recorded and caller completion.
use super::*;
use crate::flow::clock::{WaitInterest, WaitOutcome};
use crate::flow::recording::{RecordProgress, RecordPurpose};
use std::io::Read;

struct RecordedExpiry {
    claim: Arc<ServingObservationAuthor>,
    before: u64,
    after: u64,
}
impl PumpIo for RecordedExpiry {
    fn now(&mut self) -> std::io::Result<u64> {
        Ok(if self.claim.sequence().is_some() {
            self.after
        } else {
            self.before
        })
    }
    fn wait(&mut self, _: &[WaitInterest<'_>], _: Option<u64>) -> std::io::Result<WaitOutcome> {
        panic!("an empty recorder needs no capacity wait");
    }
}

pub(super) fn fill(context: &mut RecordingContext) {
    let payload =
        serde_json::value::to_raw_value(&serde_json::json!({"fixture":"x".repeat(32_768)}))
            .unwrap();
    for _ in 0..serving::ORDINARY_CAPACITY + 64 {
        if context.barrier.recorder().boundary().queued == serving::ORDINARY_CAPACITY {
            return;
        }
        assert!(matches!(
            context.barrier.submit_authored_with_clock(
                AuthoredEvent {
                    kind: serving::Kind::ModelMeasurement,
                    subsystem: Subsystem::Harness,
                    turn: Some(weaver_trace::TurnRef("trace-pressure-fixture".into())),
                    source: AuthoredSource::Runtime,
                    causal_parent: None,
                    payload: Some(serving::Payload::ModelMeasurement(payload.clone())),
                },
                RecordPurpose::Observation,
                &context.author,
                1,
                &mut context.inbox,
                &mut context.budget,
                &mut || MonotonicIo.now(),
            ),
            RecordProgress::Recorded { .. }
        ));
    }
    panic!("trace writer never reached capacity");
}

pub(super) fn drain(context: &RecordingContext, reader: &mut UnixStream) -> Vec<u8> {
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut bytes = vec![];
    loop {
        match reader.read_to_end(&mut bytes) {
            Ok(_) => (),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => (),
            Err(e) => panic!("trace drain: {e}"),
        }
        if context.barrier.recorder().boundary().queued == 0 {
            return bytes;
        }
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
}

pub(super) fn exercise(
    guard: &mut Guard<'_>,
    peer: &OwnedFd,
    reader: &mut UnixStream,
    bytes: &[u8],
    mode: &str,
) {
    let owner = guard.management.as_mut().unwrap();
    let bound = owner.plan_refusal.as_ref().unwrap().bound;
    if mode.ends_with("-recorded-expiry") {
        let mut clock = RecordedExpiry {
            claim: Arc::clone(&owner.plan_refusal.as_ref().unwrap().author),
            before: MonotonicIo.now().unwrap(),
            after: bound.deadline_ns(),
        };
        assert_eq!(
            owner.advance_with_io(&mut clock),
            FoundationManagementProgress::Pending
        );
        let record = owner.plan_refusal.as_ref().unwrap();
        let sequence = record
            .author
            .sequence()
            .expect("real Recorded receipt precedes expiry");
        assert!(
            record.sequence.is_none(),
            "caller must still owe its completion"
        );
        assert!(record.expired);
        assert_eq!(
            owner.input.serving.owner.context.completed_author,
            Some(sequence)
        );
        assert!(owner.send.is_none());
        for _ in 0..5 {
            if owner.advance_with_io(&mut clock) != FoundationManagementProgress::Pending {
                break;
            }
        }
        assert_eq!(owner.progress(), FoundationManagementProgress::Dropped);
        assert_eq!(
            owner.plan_refusal.as_ref().unwrap().sequence,
            Some(sequence)
        );
        assert!(owner.input.serving.owner.context.completed_author.is_none());
        assert!(
            owner
                .input
                .serving
                .owner
                .context
                .serving_observation_author
                .is_none()
        );
        assert!(owner.input.serving.owner.context.failure().is_none());
        assert!(read_packet(peer).is_none());
        let rows = trace_through(
            reader,
            vec![],
            sequence,
            Instant::now() + Duration::from_secs(3),
        )
        .unwrap();
        assert_eq!(rows.iter().filter(|r| r["kind"] == "refusal").count(), 1);
        guard.input = Some(guard.management.take().unwrap().finish().ok().unwrap());
        return;
    }

    fill(owner.input.serving.owner.context);
    let interrupt = AbsoluteBound::after(MonotonicIo.now().unwrap(), 20_000_000).unwrap();
    owner
        .input
        .serving
        .owner
        .context
        .arm(PumpDeadline::Label, interrupt, &mut MonotonicIo)
        .unwrap();
    assert!(
        matches!(owner.advance_plan_refusal(&mut MonotonicIo), Err(RunFailure::Wait { key: PumpDeadline::Label, bound }) if bound == interrupt)
    );
    let ticket = owner
        .plan_refusal
        .as_ref()
        .unwrap()
        .author
        .ticket()
        .expect("refusal admitted under real recorder pressure");
    assert_eq!(
        owner
            .input
            .serving
            .owner
            .context
            .pending_author()
            .unwrap()
            .ticket,
        ticket
    );
    assert!(
        owner
            .plan_refusal
            .as_ref()
            .unwrap()
            .author
            .sequence()
            .is_none()
    );
    assert!(owner.send.is_none());
    assert!(read_packet(peer).is_none());
    owner
        .input
        .serving
        .owner
        .context
        .complete_wait(PumpDeadline::Label)
        .unwrap();
    if mode.ends_with("-pending-expiry") {
        let mut clock = NoReadyIo {
            now: bound.deadline_ns(),
            waits: 0,
        };
        assert_eq!(
            owner.advance_with_io(&mut clock),
            FoundationManagementProgress::Stopped
        );
        assert!(matches!(
            owner.input.serving.owner.context.failure(),
            Some(RunFailure::Flow {
                reason: FlowHaltReason::DecisionBound,
                ..
            })
        ));
        assert_eq!(
            owner
                .input
                .serving
                .owner
                .context
                .pending_author()
                .unwrap()
                .ticket,
            ticket
        );
        drain(owner.input.serving.owner.context, reader);
        failed_handoff(guard, &mut clock, bytes, bound, Some(ticket));
        return;
    }
    assert!(mode.ends_with("-trace-eof"));
    nix::sys::socket::shutdown(peer.as_raw_fd(), nix::sys::socket::Shutdown::Both).unwrap();
    let trace_bytes = drain(owner.input.serving.owner.context, reader);
    let watch = Instant::now() + Duration::from_secs(3);
    while owner.advance() == FoundationManagementProgress::Pending {
        assert!(Instant::now() < watch);
    }
    assert_eq!(owner.progress(), FoundationManagementProgress::Dropped);
    let record = owner.plan_refusal.as_ref().unwrap();
    assert_eq!(record.author.ticket(), Some(ticket));
    let sequence = record.sequence.unwrap();
    assert_eq!(record.author.sequence(), Some(sequence));
    assert!(owner.input.serving.owner.context.failure().is_none());
    assert_eq!(
        owner
            .input
            .serving
            .owner
            .context
            .deadlines
            .get(PumpDeadline::PlanRefusal),
        Some(bound)
    );
    assert_eq!(owner.declined_plan.as_ref().unwrap().wire(), bytes);
    assert!(owner.send.is_none());
    let rows = trace_through(reader, trace_bytes, sequence, watch).unwrap();
    assert_eq!(rows.iter().filter(|r| r["kind"] == "refusal").count(), 1);
    guard.input = Some(guard.management.take().unwrap().finish().ok().unwrap());
}
