//! Cancellation on both sides of filing-owner admission, before a source ticket.
use super::*;
use crate::flow::clock::{WaitInterest, WaitOutcome, testing::FakePumpIo};

fn fill_trace(fx: &mut SummaryFixture) {
    let run = fx.run();
    let payload = serde_json::value::to_raw_value(&serde_json::json!({
        "fixture": "x".repeat(1024),
    }))
    .unwrap();
    for _ in 0..weaver_trace::serving::ORDINARY_CAPACITY * 2 {
        if run.context.barrier.recorder().boundary().queued
            == weaver_trace::serving::ORDINARY_CAPACITY
        {
            return;
        }
        assert!(matches!(
            run.context.barrier.submit_authored_with_clock(
                AuthoredEvent {
                    kind: Kind::ModelMeasurement,
                    subsystem: weaver_trace::Subsystem::Harness,
                    turn: Some(TurnRef("activation".into())),
                    source: AuthoredSource::Runtime,
                    causal_parent: None,
                    payload: Some(Payload::ModelMeasurement(payload.clone())),
                },
                RecordPurpose::Observation,
                &run.context.author,
                1,
                &mut run.context.inbox,
                &mut run.context.budget,
                &mut || run.clock.now(),
            ),
            RecordProgress::Recorded { .. }
        ));
    }
    panic!("fixture did not reach actual recorder capacity");
}

fn cancelled_without_effect(fx: &mut SummaryFixture, before: Option<s::PlanSummary>) {
    let bound = fx.run().context.plan_port_bound();
    fx.run().context.cancel_plan_operation();
    // Relieve pressure only after cancellation. A missing cancellation guard
    // can now admit its forbidden source instead of failing on queue capacity.
    fx.drain();
    assert_eq!(advance(fx).unwrap(), PlanPortProgress::Complete);
    assert!(matches!(finish(fx), PlanPortCompletion::Cancelled(_)));
    assert!(fx.records(Kind::PlanEvent).is_empty());
    assert!(fx.files.is_empty());
    assert!(fx.run().context.filing.is_none());
    assert!(fx.run().context.flow.filing_receipts().is_none());
    assert!(!fx.run().context.flow.has_filing_consumer());
    assert!(fx.run().context.waiting_content.is_none());
    assert!(fx.run().context.pending_author().is_none());
    assert_eq!(
        fx.run().context.committed_plan_summary().unwrap(),
        before.as_ref()
    );
    assert_eq!(
        fx.run().context.deadlines.get(PumpDeadline::Decision),
        bound
    );
    assert!(fx.run().context.failure().is_none());
    fx.boot.member.no_request();
}

#[test]
fn plan_activation_cancelled_expansion_never_begins_filing() {
    let (mut fx, certificate) = ready_effect(0);
    let before = fx.run().context.committed_plan_summary().unwrap().cloned();
    start_effect(&mut fx, &certificate);
    assert!(
        fx.run()
            .context
            .plan_port_work()
            .unwrap()
            .test_stage()
            .starts_with("EffectFiling")
    );
    assert!(fx.run().context.filing.is_none());
    fill_trace(&mut fx);
    cancelled_without_effect(&mut fx, before);
}

struct AtRead<'a> {
    clock: &'a mut FakePumpIo,
    remaining: usize,
    deadline: u64,
}
impl PumpIo for AtRead<'_> {
    fn now(&mut self) -> std::io::Result<u64> {
        self.remaining = self.remaining.saturating_sub(1);
        if self.remaining == 0 {
            self.clock.advance_to(self.deadline)?;
        }
        self.clock.now()
    }
    fn wait(
        &mut self,
        interests: &[WaitInterest<'_>],
        bound: Option<u64>,
    ) -> std::io::Result<WaitOutcome> {
        self.clock.wait(interests, bound)
    }
}

#[test]
fn plan_activation_cancelled_unstarted_filing_withdraws_original_under_pressure() {
    // An earlier pump deadline can yield after the filing owner accepts
    // the source but before that source receives a recorder ticket. Explore
    // clock boundaries instead of writing an owner, receipt or admission bit.
    for edge in 1..=32 {
        let (mut fx, certificate) = ready_effect(0);
        let before = fx.run().context.committed_plan_summary().unwrap().cloned();
        start_effect(&mut fx, &certificate);
        fill_trace(&mut fx);
        let run = fx.run();
        let label = AbsoluteBound::after(run.clock.now().unwrap(), 100).unwrap();
        run.context
            .arm(PumpDeadline::Label, label, &mut run.clock)
            .unwrap();
        let mut io = AtRead {
            clock: &mut run.clock,
            remaining: edge,
            deadline: label.deadline_ns(),
        };
        assert_eq!(
            run.context.advance_plan_operation(&mut ports(&mut io)),
            Err(RunFailure::Wait {
                key: PumpDeadline::Label,
                bound: label
            })
        );
        run.context.complete_wait(PumpDeadline::Label).unwrap();
        let unstarted = run.context.filing.is_some()
            && run.context.waiting_content.is_some()
            && run.context.pending_author().is_none();
        if unstarted {
            assert!(run.context.flow.has_filing_consumer());
            assert!(run.context.flow.filing_receipts().is_none());
            assert_eq!(
                run.context.waiting_content.as_ref().unwrap().kind,
                Kind::PlanEvent
            );
            cancelled_without_effect(&mut fx, before);
            return;
        }
        // Keep the capacity fixture's shutdown deterministic for clock edges
        // that precede owner admission or follow source ticket admission.
        fx.drain();
    }
    panic!("no clock edge reached the admitted but unstarted source");
}
