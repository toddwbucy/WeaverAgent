//! Real recorder/classifier/member fixtures for the private filing pause.

use super::*;
use crate::flow::clock::{WaitInterest, WaitOutcome};

const _: fn() = || {
    trait MustNotClone<Marker> {
        fn check() {}
    }
    impl<T: ?Sized> MustNotClone<()> for T {}
    impl<T: Clone> MustNotClone<u8> for T {}
    let _ = <PreparedFreshness as MustNotClone<_>>::check;
    trait MustNotFromSequence<Marker> {
        fn check() {}
    }
    impl<T: ?Sized> MustNotFromSequence<()> for T {}
    impl<T: From<Sequence>> MustNotFromSequence<u8> for T {}
    let _ = <PreparedFreshness as MustNotFromSequence<_>>::check;
};

fn fixture(limit: u64) -> ReceiptFixture {
    let mut fx = ReceiptFixture::new(limit);
    assert_eq!(
        fx.input.run.context.complete_wait(PumpDeadline::Decision),
        Some(fx.bound)
    );
    fx
}

fn sources(sequence: Sequence, blocks: u64) -> Vec<s::SourceBlockId> {
    (0..blocks)
        .map(|block| s::SourceBlockId {
            run: "run".into(),
            sequence: sequence.0.into(),
            block: block.into(),
        })
        .collect()
}

fn begin(fx: &mut ReceiptFixture, ids: Vec<s::SourceBlockId>) -> AbsoluteBound {
    fx.input
        .run
        .context
        .prepare_cut(ids, &mut fx.input.run.clock)
        .unwrap();
    fx.input
        .run
        .context
        .deadlines
        .get(PumpDeadline::Cut)
        .unwrap()
}

fn advance(fx: &mut ReceiptFixture) -> Result<FreshnessProgress, RunFailure> {
    fx.input
        .run
        .context
        .advance_prepare_cut(&mut ports(&mut fx.input.run.clock))
}

fn finish(fx: &mut ReceiptFixture, reject: Option<&str>) -> FreshnessCompletion {
    for _ in 0..256 {
        if advance(fx).unwrap() == FreshnessProgress::Complete {
            return fx
                .input
                .run
                .context
                .take_prepared_cut(&mut fx.input.run.clock)
                .unwrap()
                .unwrap();
        }
        fx.classifier_reply(reject);
        fx.member(true);
    }
    panic!("freshness never settled");
}

#[test]
fn freshness_requires_each_selected_recorded_disposition_and_releases_cut_before_read() {
    let mut fx = fixture(100);
    let sequence = fx
        .input
        .run
        .author(event(&["Read alpha.", "Read rejected."]))
        .unwrap();
    let ids = sources(sequence, 2);
    let began = fx.input.run.clock.now().unwrap();
    let bound = begin(&mut fx, ids.clone());
    assert_eq!(bound.began_ns(), began);
    assert_eq!(
        bound.deadline_ns(),
        began + fx.input.run.context.declaration.cut_wait.elapsed_ns.0
    );
    assert_eq!(fx.input.run.context.complete_wait(PumpDeadline::Cut), None);
    assert!(
        fx.input
            .run
            .context
            .take_prepared_cut(&mut fx.input.run.clock)
            .unwrap()
            .is_none()
    );
    let selector = s::ReadSelector::History {
        noun: "alpha".into(),
        verb: None,
        condition: None,
    };
    let refused = fx
        .input
        .run
        .context
        .begin_read(selector.clone(), None, &mut fx.input.run.clock)
        .unwrap_err();
    assert_eq!(refused.refusal, ReadBeginRefusal::Busy);
    assert!(
        fx.input
            .run
            .context
            .deadlines
            .get(PumpDeadline::StateControl)
            .is_none()
    );
    let FreshnessCompletion::Prepared(proof) = finish(&mut fx, Some("Read rejected.")) else {
        panic!("healthy cut was cancelled");
    };
    assert_eq!(proof.required_ids(), ids);
    assert_eq!(proof.bound(), bound);
    assert!(proof.belongs_to(&fx.input.run.context));
    let other = fixture(100);
    assert!(!proof.belongs_to(&other.input.run.context));
    assert_eq!(fx.file_requests.len(), 1);
    assert!(
        fx.input
            .records(Kind::StateFlow)
            .iter()
            .any(|record| record["payload"]["phase"] == "correctly_rejected")
    );
    assert_eq!(fx.input.run.context.budget.gauges(), (0, 0));
    assert!(
        fx.input
            .run
            .context
            .deadlines
            .get(PumpDeadline::Cut)
            .is_none()
    );
    let read_began = fx.input.run.clock.now().unwrap();
    fx.input
        .run
        .context
        .begin_read(selector, None, &mut fx.input.run.clock)
        .unwrap();
    assert_eq!(
        fx.input
            .run
            .context
            .deadlines
            .get(PumpDeadline::StateControl)
            .unwrap()
            .began_ns(),
        read_began
    );
    assert!(
        fx.input
            .run
            .context
            .deadlines
            .get(PumpDeadline::Cut)
            .is_none()
    );
}

#[test]
fn freshness_member_ack_waits_for_its_terminal_record_under_trace_pressure() {
    let mut fx = fixture(100);
    let sequence = fx.input.run.author(event(&["Read held."])).unwrap();
    begin(&mut fx, sources(sequence, 1));
    for _ in 0..128 {
        advance(&mut fx).unwrap();
        fx.classifier_reply(None);
        fx.member(true);
        if fx
            .input
            .run
            .context
            .flow
            .ledger()
            .facts()
            .any(|fact| fact.state == LedgerState::FiledHeld)
        {
            break;
        }
    }
    assert!(
        fx.input
            .run
            .context
            .flow
            .ledger()
            .facts()
            .any(|fact| fact.state == LedgerState::FiledHeld)
    );
    fx.input.fill_trace();
    assert_eq!(advance(&mut fx).unwrap(), FreshnessProgress::Pending);
    assert!(fx.input.run.context.flow.has_pending_record());
    assert_eq!(fx.input.run.context.budget.gauges().0, 1);
    assert!(
        fx.input
            .run
            .context
            .take_prepared_cut(&mut fx.input.run.clock)
            .unwrap()
            .is_none()
    );
    assert!(
        fx.input
            .run
            .context
            .deadlines
            .get(PumpDeadline::Cut)
            .is_some()
    );
    fx.input.drain_trace();
    assert!(matches!(
        finish(&mut fx, None),
        FreshnessCompletion::Prepared(_)
    ));
    assert_eq!(fx.file_requests.len(), 1);
    assert_eq!(
        fx.input
            .records(Kind::StateFlow)
            .iter()
            .filter(|record| record["payload"]["phase"] == "filed")
            .count(),
        1
    );
}

#[test]
fn freshness_drains_deferred_whole_copy_without_inventing_an_unknown_source() {
    let mut fx = fixture(1);
    let older = fx.input.run.author(event(&["Read older."])).unwrap();
    let watched = fx.input.run.author(event(&["Read watched."])).unwrap();
    assert!(watched.0 > older.0);
    assert!(fx.input.run.context.budget.waiting_copy().is_some());
    let ids = sources(watched, 1);
    begin(&mut fx, ids.clone());
    let FreshnessCompletion::Prepared(proof) = finish(&mut fx, None) else {
        panic!("prepared");
    };
    assert_eq!(proof.required_ids(), ids);
    assert_eq!(fx.file_requests.len(), 2);
    assert!(fx.input.run.context.budget.waiting_copy().is_none());
    assert!(fx.input.run.context.inbox.queued().next().is_none());
}

#[test]
fn freshness_unknown_foreign_and_unselected_ids_fail_without_a_read() {
    for defect in 0..3 {
        let mut fx = fixture(100);
        let sequence = fx.input.run.author(event(&["Read known."])).unwrap();
        begin(&mut fx, sources(sequence, 1));
        finish(&mut fx, None);
        let mut ids = sources(sequence, 1);
        match defect {
            0 => ids[0].sequence = (sequence.0 + 500).into(),
            1 => ids[0].run = "foreign".into(),
            _ => ids[0].block = 99.into(),
        }
        begin(&mut fx, ids);
        assert!(matches!(
            advance(&mut fx),
            Err(RunFailure::Flow {
                reason: FlowHaltReason::ProtocolFailed,
                point: HaltPoint::Barrier,
                ..
            })
        ));
        assert!(fx.input.run.context.freshness.is_some());
        assert!(
            fx.input
                .run
                .context
                .take_prepared_cut(&mut fx.input.run.clock)
                .is_err()
        );
        assert!(fx.input.run.context.read.is_none());
        assert_eq!(fx.file_requests.len(), 1);
    }
}

#[test]
fn freshness_accepts_already_disposed_ids_without_stealing_receipt_consumers() {
    let mut fx = ReceiptFixture::new(100);
    fx.begin(event(&["Read filed."]));
    let filing = fx.complete(None);
    let ids = filing.receipts().sources().cloned().collect::<Vec<_>>();
    fx.input
        .run
        .context
        .complete_wait(PumpDeadline::Decision)
        .unwrap();
    begin(&mut fx, ids.clone());
    let FreshnessCompletion::Prepared(proof) = finish(&mut fx, None) else {
        panic!("prepared");
    };
    assert_eq!(proof.required_ids(), ids);
    assert_eq!(filing.receipts().dispositions().count(), 1);
    assert_eq!(fx.file_requests.len(), 1);
}

#[test]
fn freshness_waits_for_queued_observations_even_without_selected_requirements() {
    let mut fx = fixture(100);
    fx.input.run.author(event(&["Read first."])).unwrap();
    fx.input.run.author(event(&["Read second."])).unwrap();
    assert!(fx.input.run.context.flow.has_pending_observations());
    begin(&mut fx, vec![]);
    assert_eq!(advance(&mut fx).unwrap(), FreshnessProgress::Pending);
    assert!(fx.input.run.context.flow.has_pending_observations());
    assert!(
        fx.input
            .run
            .context
            .take_prepared_cut(&mut fx.input.run.clock)
            .unwrap()
            .is_none()
    );
    let FreshnessCompletion::Prepared(proof) = finish(&mut fx, None) else {
        panic!("prepared");
    };
    assert!(proof.required_ids().is_empty());
    assert!(!fx.input.run.context.flow.has_pending_observations());
}

#[test]
fn freshness_cancellation_keeps_owed_work_and_the_original_wait() {
    let mut fx = fixture(100);
    let sequence = fx.input.run.author(event(&["Read retained."])).unwrap();
    let ids = sources(sequence, 1);
    let bound = begin(&mut fx, ids.clone());
    fx.input
        .run
        .context
        .cancel_prepare_cut(&mut fx.input.run.clock)
        .unwrap();
    fx.input
        .run
        .context
        .cancel_prepare_cut(&mut fx.input.run.clock)
        .unwrap();
    assert_eq!(
        fx.input.run.context.deadlines.get(PumpDeadline::Cut),
        Some(bound)
    );
    assert_eq!(fx.input.run.context.complete_wait(PumpDeadline::Cut), None);
    assert_eq!(fx.input.run.context.flow.ledger().facts().count(), 1);
    let refused = fx
        .input
        .run
        .context
        .prepare_cut(ids.clone(), &mut fx.input.run.clock)
        .unwrap_err();
    assert_eq!(refused.required_ids, ids);
    assert_eq!(refused.refusal, FreshnessBeginRefusal::Busy);
    assert_eq!(
        fx.input.run.context.deadlines.get(PumpDeadline::Cut),
        Some(bound)
    );
    let FreshnessCompletion::Cancelled { required_ids } = finish(&mut fx, None) else {
        panic!("cancelled cut granted readiness");
    };
    assert_eq!(required_ids, ids);
    assert_eq!(fx.file_requests.len(), 1);
    assert!(fx.input.run.context.flow.ledger().facts().next().is_none());
    assert!(
        fx.input
            .run
            .context
            .deadlines
            .get(PumpDeadline::Cut)
            .is_none()
    );
}

#[test]
fn freshness_cut_expiry_and_earlier_leave_preserve_the_same_owner() {
    for leave_first in [true, false] {
        let mut fx = fixture(100);
        let sequence = fx.input.run.author(event(&["Read unanswered."])).unwrap();
        let bound = begin(&mut fx, sources(sequence, 1));
        for _ in 0..128 {
            advance(&mut fx).unwrap();
            fx.classifier_reply(None);
            fx.member(false);
            if !fx.file_requests.is_empty() {
                break;
            }
        }
        assert_eq!(fx.file_requests.len(), 1);
        let leave = AbsoluteBound::after(fx.input.run.clock.now().unwrap(), 5).unwrap();
        if leave_first {
            fx.input
                .run
                .context
                .arm(PumpDeadline::Leave, leave, &mut fx.input.run.clock)
                .unwrap();
            fx.input.run.clock.advance_to(leave.deadline_ns()).unwrap();
            assert_eq!(
                advance(&mut fx).unwrap_err(),
                RunFailure::Wait {
                    key: PumpDeadline::Leave,
                    bound: leave
                }
            );
            assert!(fx.input.run.context.failure().is_none());
            assert_eq!(
                fx.input.run.context.complete_wait(PumpDeadline::Leave),
                Some(leave)
            );
            assert_eq!(
                fx.input.run.context.deadlines.get(PumpDeadline::Cut),
                Some(bound)
            );
        }
        fx.input.run.clock.advance_to(bound.deadline_ns()).unwrap();
        assert!(matches!(
            advance(&mut fx),
            Err(RunFailure::Flow {
                reason: FlowHaltReason::CutBound,
                ..
            })
        ));
        assert_eq!(fx.input.run.context.flow.ledger().facts().count(), 1);
        assert!(fx.input.run.context.freshness.is_some());
        assert!(
            fx.input
                .run
                .context
                .take_prepared_cut(&mut fx.input.run.clock)
                .is_err()
        );
        assert!(
            fx.input
                .run
                .context
                .cancel_prepare_cut(&mut fx.input.run.clock)
                .is_err()
        );
        assert_eq!(fx.file_requests.len(), 1);
    }
}

fn note() -> AuthoredEvent {
    let mut event = super::super::tests::event(
        Kind::ModelMeasurement,
        Some(Payload::ModelMeasurement(
            serde_json::value::to_raw_value(
                &serde_json::json!({"fixture":"pending freshness author"}),
            )
            .unwrap(),
        )),
    );
    event.turn = Some(weaver_trace::TurnRef("receipt-turn".into()));
    event
}

struct CrossingClock {
    before: u64,
    after: u64,
    remaining: usize,
}
impl PumpIo for CrossingClock {
    fn now(&mut self) -> std::io::Result<u64> {
        if self.remaining == 0 {
            Ok(self.after)
        } else {
            self.remaining -= 1;
            Ok(self.before)
        }
    }
    fn wait(&mut self, _: &[WaitInterest<'_>], _: Option<u64>) -> std::io::Result<WaitOutcome> {
        panic!("unblocked local author needs no transport wait");
    }
}

#[test]
fn freshness_retains_a_deferred_generic_author_and_its_sequence_across_leave() {
    for remaining in 0..14 {
        let mut fx = fixture(100);
        let now = fx.input.run.clock.now().unwrap();
        let initial = AbsoluteBound::after(now, 1).unwrap();
        fx.input
            .run
            .context
            .arm(PumpDeadline::Leave, initial, &mut fx.input.run.clock)
            .unwrap();
        fx.input
            .run
            .clock
            .advance_to(initial.deadline_ns())
            .unwrap();
        let original = note();
        assert_eq!(
            fx.input.run.author(original.clone()).unwrap_err(),
            RunFailure::Wait {
                key: PumpDeadline::Leave,
                bound: initial
            }
        );
        assert!(fx.input.run.context.author_admission_pending);
        fx.input
            .run
            .context
            .complete_wait(PumpDeadline::Leave)
            .unwrap();
        let bound = begin(&mut fx, vec![]);
        let before = fx.input.run.clock.now().unwrap();
        let leave = AbsoluteBound::after(before, 5).unwrap();
        fx.input
            .run
            .context
            .arm(PumpDeadline::Leave, leave, &mut fx.input.run.clock)
            .unwrap();
        let mut clock = CrossingClock {
            before,
            after: leave.deadline_ns(),
            remaining,
        };
        let result = fx
            .input
            .run
            .context
            .advance_prepare_cut(&mut ports(&mut clock));
        if let Err(failure) = result {
            assert_eq!(
                failure,
                RunFailure::Wait {
                    key: PumpDeadline::Leave,
                    bound: leave
                }
            );
        }
        fx.input.run.clock.advance_to(leave.deadline_ns()).unwrap();
        fx.input
            .run
            .context
            .complete_wait(PumpDeadline::Leave)
            .unwrap();
        let FreshnessCompletion::Prepared(proof) = finish(&mut fx, None) else {
            panic!("prepared");
        };
        assert_eq!(proof.bound(), bound);
        let sequence = fx.input.run.context.completed_author.unwrap();
        assert_eq!(
            fx.input
                .run
                .context
                .resume_author_and_wait(&mut ports(&mut fx.input.run.clock))
                .unwrap(),
            sequence
        );
        assert!(fx.input.run.context.completed_author.is_none());
        let records = fx.input.records(Kind::ModelMeasurement);
        assert_eq!(records.len(), 1, "interruption sample {remaining}");
        assert_eq!(records[0]["sequence"], sequence.0.to_string());
        assert_eq!(records[0]["payload"]["fixture"], "pending freshness author");
        assert!(fx.input.run.context.refused_author.is_none());
    }
}

#[test]
fn freshness_resumes_the_existing_trace_ticket_without_taking_its_caller_result() {
    let mut fx = fixture(100);
    fx.input.fill_trace();
    let leave = AbsoluteBound::after(fx.input.run.clock.now().unwrap(), 5).unwrap();
    fx.input
        .run
        .context
        .arm(PumpDeadline::Leave, leave, &mut fx.input.run.clock)
        .unwrap();
    assert_eq!(
        fx.input.run.author(note()).unwrap_err(),
        RunFailure::Wait {
            key: PumpDeadline::Leave,
            bound: leave
        }
    );
    let ticket = fx.input.run.context.pending_author().unwrap().ticket;
    // fill_trace authors ModelMeasurement pressure records itself. Inspect
    // this exact caller's payload, not every record sharing the kind.
    let caller_records = |fx: &ReceiptFixture| {
        fx.input
            .records(Kind::ModelMeasurement)
            .into_iter()
            .filter(|record| record["payload"]["fixture"] == "pending freshness author")
            .collect::<Vec<_>>()
    };
    assert!(caller_records(&fx).is_empty());
    fx.input
        .run
        .context
        .complete_wait(PumpDeadline::Leave)
        .unwrap();
    begin(&mut fx, vec![]);
    assert_eq!(
        fx.input.run.context.pending_author().unwrap().ticket,
        ticket
    );
    fx.input.drain_trace();
    finish(&mut fx, None);
    let sequence = fx.input.run.context.completed_author.unwrap();
    assert_eq!(
        fx.input
            .run
            .context
            .resume_author_and_wait(&mut ports(&mut fx.input.run.clock))
            .unwrap(),
        sequence
    );
    let records = caller_records(&fx);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["sequence"], sequence.0.to_string());
    assert!(fx.input.run.context.pending_author().is_none());
}

#[test]
fn freshness_complete_owner_blocks_new_intake_until_its_result_is_taken() {
    for operation in 0..4 {
        let mut fx = fixture(100);
        begin(&mut fx, vec![]);
        assert_eq!(advance(&mut fx).unwrap(), FreshnessProgress::Complete);
        assert!(
            fx.input
                .run
                .context
                .deadlines
                .get(PumpDeadline::Cut)
                .is_none()
        );
        let count = fx.input.run.context.barrier.recorder().structure().len();
        match operation {
            0 => {
                let error = fx
                    .input
                    .run
                    .context
                    .begin_read(
                        s::ReadSelector::SystemPrompt {},
                        None,
                        &mut fx.input.run.clock,
                    )
                    .unwrap_err();
                assert_eq!(error.refusal, ReadBeginRefusal::Busy);
            }
            1 => {
                let frame = weaver_types::TurnFrame::carry_from(br#"{"text":"Read next."}"#, 42);
                let error = fx
                    .input
                    .run
                    .context
                    .begin_input(frame.clone(), &mut fx.input.run.clock)
                    .unwrap_err();
                assert_eq!(error.frame, frame);
                assert!(error.failure.is_none());
            }
            2 => {
                assert_eq!(
                    fx.input.run.author(note()).unwrap_err(),
                    RunFailure::FreshnessOwned
                );
                assert!(fx.input.run.context.waiting_content.is_none());
            }
            _ => {
                let bound = AbsoluteBound::after(fx.input.run.clock.now().unwrap(), 100).unwrap();
                fx.input
                    .run
                    .context
                    .arm(PumpDeadline::Decision, bound, &mut fx.input.run.clock)
                    .unwrap();
                let error = fx
                    .input
                    .run
                    .context
                    .begin_filing(
                        event(&["Read next."]),
                        PumpDeadline::Decision,
                        &mut fx.input.run.clock,
                    )
                    .unwrap_err();
                assert_eq!(error.refusal, FilingBeginRefusal::Busy);
                assert!(error.failure.is_none());
            }
        }
        assert_eq!(
            fx.input.run.context.barrier.recorder().structure().len(),
            count
        );
        assert!(fx.input.run.context.freshness.is_some());
        assert!(fx.input.run.context.failure().is_none());
    }
}

#[test]
fn freshness_refuses_an_existing_explicit_owner_without_consuming_either_input() {
    let mut fx = ReceiptFixture::new(100);
    fx.begin(event(&["Read filing."]));
    let ids = sources(Sequence(900), 1);
    let error = fx
        .input
        .run
        .context
        .prepare_cut(ids.clone(), &mut fx.input.run.clock)
        .unwrap_err();
    assert_eq!(error.required_ids, ids);
    assert_eq!(error.refusal, FreshnessBeginRefusal::Busy);
    assert!(error.failure.is_none());
    assert!(fx.input.run.context.freshness.is_none());
    assert!(
        fx.input
            .run
            .context
            .deadlines
            .get(PumpDeadline::Cut)
            .is_none()
    );
    assert_eq!(fx.complete(None).receipts().dispositions().count(), 1);
}

#[test]
fn freshness_checked_deadline_overflow_returns_ids_without_arming_cut() {
    let mut fx = fixture(100);
    fx.input.run.clock.advance_to(u64::MAX - 1).unwrap();
    let ids = sources(Sequence(99), 1);
    let error = fx
        .input
        .run
        .context
        .prepare_cut(ids.clone(), &mut fx.input.run.clock)
        .unwrap_err();
    assert_eq!(error.required_ids, ids);
    assert!(matches!(
        error.failure,
        Some(RunFailure::Flow {
            reason: FlowHaltReason::ProtocolFailed,
            ..
        })
    ));
    assert!(fx.input.run.context.freshness.is_none());
    assert!(
        fx.input
            .run
            .context
            .deadlines
            .get(PumpDeadline::Cut)
            .is_none()
    );
}
