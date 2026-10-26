//! Negative probes called only after the real gate fixture owns a trace ticket.
use super::*;
use crate::flow::pump::LifecyclePorts;
use std::sync::Arc;

fn ports<I: PumpIo>(io: &mut I) -> PumpPorts<'_, I> {
    PumpPorts {
        lifecycle: LifecyclePorts::default(),
        gate: None,
        decode: None,
        label: None,
        io,
    }
}

fn competitor() -> AuthoredEvent {
    AuthoredEvent {
        kind: Kind::Refusal,
        subsystem: weaver_trace::Subsystem::Harness,
        turn: None,
        source: crate::flow::author::AuthoredSource::Runtime,
        causal_parent: None,
        payload: Some(Payload::Refusal(
            serde_json::value::to_raw_value(&weaver_types::RefusalRecord::Lifecycle {
                asked: weaver_types::LifecycleAsk::Leave,
                refusal: weaver_types::LifecycleRefusal::Unloading,
            })
            .unwrap(),
        )),
    }
}

fn exact(event: &AuthoredEvent) -> (usize, String) {
    let Some(Payload::Refusal(raw)) = event.payload.as_ref() else {
        panic!("actual gate and competing event are typed refusals")
    };
    (raw.get().as_ptr() as usize, format!("{event:?}"))
}

struct PendingGate {
    claim: Arc<worker_startup::expired::gate::RollbackGateAuthor>,
    ticket: u64,
    event: (usize, String),
    prepared: serde_json::Value,
    author_turn: Option<(Kind, Option<weaver_trace::TurnRef>)>,
    attempts: usize,
}

impl PendingGate {
    fn actual(context: &RecordingContext) -> Self {
        let claim = Arc::clone(
            context
                .rollback_gate_author
                .as_ref()
                .expect("fixture reached a real scoped gate author"),
        );
        let pending = context
            .barrier
            .pending_author()
            .expect("fixture reached a real capacity-pending ticket");
        assert_eq!(claim.ticket(), Some(pending.ticket));
        assert!(claim.sequence().is_none());
        assert!(context.waiting_content.is_none());
        assert!(context.completed_author.is_none());
        Self {
            claim,
            ticket: pending.ticket,
            event: exact(&pending.event),
            prepared: serde_json::to_value(context.barrier.pending().unwrap()).unwrap(),
            author_turn: context.author_turn.clone(),
            attempts: context.barrier.recorder().structure().len(),
        }
    }

    fn unchanged(&self, context: &RecordingContext) {
        assert!(Arc::ptr_eq(
            &self.claim,
            context.rollback_gate_author.as_ref().unwrap()
        ));
        let pending = context.barrier.pending_author().unwrap();
        assert_eq!(pending.ticket, self.ticket);
        assert_eq!(exact(&pending.event), self.event);
        assert_eq!(self.claim.ticket(), Some(self.ticket));
        assert!(self.claim.sequence().is_none());
        assert_eq!(
            serde_json::to_value(context.barrier.pending().unwrap()).unwrap(),
            self.prepared
        );
        assert_eq!(context.author_turn, self.author_turn);
        assert_eq!(context.barrier.recorder().structure().len(), self.attempts);
        assert!(context.waiting_content.is_none());
        assert!(context.completed_author.is_none());
    }
}

pub(in crate::flow::context) fn competing_author(
    context: &mut RecordingContext,
    io: &mut impl PumpIo,
) {
    let pending = PendingGate::actual(context);
    assert!(context.failure().is_none());
    assert!(!context.rollback_gate_author_available());
    let event = competitor();
    let expected = exact(&event);
    let refused = context.author_and_wait(&mut ports(io), event).unwrap_err();
    assert_eq!(refused.reason, RunFailure::Busy(HeldInput::Caller));
    assert_eq!(refused.original.as_ref().map(exact), Some(expected));
    pending.unchanged(context);
    for failure in [
        context.resume_author_and_wait(&mut ports(io)).unwrap_err(),
        context
            .continue_claimed_content(&mut ports(io), None)
            .unwrap_err(),
        context
            .resume_claimed_author_and_wait(&mut ports(io), None)
            .unwrap_err(),
    ] {
        assert_eq!(failure.reason, RunFailure::Busy(HeldInput::Caller));
        assert!(failure.original.is_none());
    }
    pending.unchanged(context);
    assert!(context.refused_author.is_none());
    assert!(context.failure().is_none());
    assert!(context.flow.halt().is_none());
}

pub(in crate::flow::context) fn first_fatal(context: &mut RecordingContext, io: &mut impl PumpIo) {
    let pending = PendingGate::actual(context);
    let first = context
        .failure()
        .expect("real fixture clock failed")
        .clone();
    assert!(matches!(
        first,
        RunFailure::Flow {
            reason: FlowHaltReason::TraceFailed,
            ..
        }
    ));
    let cap = context.flow.halt().unwrap().deadline();
    let leave = context.deadlines.get(PumpDeadline::Leave);
    assert!(context.refused_author.is_none());
    let event = competitor();
    let expected = exact(&event);
    let refused = context.author_and_wait(&mut ports(io), event).unwrap_err();
    assert_eq!(refused.reason, first);
    assert!(refused.original.is_none());
    assert_eq!(
        context.refused_author.as_ref().map(exact),
        Some(expected.clone())
    );
    let later = competitor();
    let later_expected = exact(&later);
    let refused = context.author_and_wait(&mut ports(io), later).unwrap_err();
    assert_eq!(refused.reason, first);
    assert_eq!(refused.original.as_ref().map(exact), Some(later_expected));
    assert_eq!(context.refused_author.as_ref().map(exact), Some(expected));
    for failure in [
        context.resume_author_and_wait(&mut ports(io)).unwrap_err(),
        context
            .continue_claimed_content(&mut ports(io), None)
            .unwrap_err(),
        context
            .resume_claimed_author_and_wait(&mut ports(io), None)
            .unwrap_err(),
    ] {
        assert_eq!(failure.reason, first);
        assert!(failure.original.is_none());
    }
    pending.unchanged(context);
    assert_eq!(context.failure(), Some(&first));
    assert_eq!(context.flow.halt().unwrap().deadline(), cap);
    assert_eq!(context.deadlines.get(PumpDeadline::Leave), leave);
}
