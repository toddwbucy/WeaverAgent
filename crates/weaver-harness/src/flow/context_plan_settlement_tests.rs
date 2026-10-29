//! Root terminal custody through the actual recorder and shared allocator.
use super::*;
use crate::flow::context::plan_update_driver::settlement::{RootPlanCompletion, RootPlanUpdated};
use weaver_types::{LifecycleAnswer, LifecycleAsk, LifecycleRefusal, RefusalRecord};

const _: fn() = || {
    trait NotClone<M> {
        fn pin() {}
    }
    impl<T: ?Sized> NotClone<()> for T {}
    impl<T: Clone> NotClone<u8> for T {}
    let _ = <RootPlanCompletion as NotClone<_>>::pin;
    let _ = <RootPlanUpdated as NotClone<_>>::pin;
};

fn refused_update() -> (SummaryFixture, BootPlanSummary) {
    let (mut fx, boot) = ready();
    begin(&mut fx);
    let request = next(&mut fx);
    fx.boot
        .member
        .respond(&request, refused(s::ReadRefusalCode::ResultUnavailable));
    assert!((0..128).any(|_| advance(&mut fx) == RootPlanProgress::RefusalReady));
    (fx, boot)
}
fn settle(fx: &mut SummaryFixture) -> Result<RootPlanProgress, RunFailure> {
    let run = fx.run();
    run.context
        .settle_root_plan_update(&mut ports(&mut run.clock))
}
fn take(fx: &mut SummaryFixture) -> Option<RootPlanCompletion> {
    let run = fx.run();
    run.context.take_root_plan_update(&mut run.clock).unwrap()
}

#[test]
#[ignore = "actual root coordination peer"]
fn root_plan_settlement_read_refusal_records_exact_parent_then_releases_allocator() {
    let (mut fx, _boot) = refused_update();
    assert!(take(&mut fx).is_none());
    let parent = fx
        .run()
        .context
        .root_plan_work()
        .unwrap()
        .read_refusal()
        .unwrap()
        .measurement_sequence();
    assert_eq!(settle(&mut fx).unwrap(), RootPlanProgress::Complete);
    assert_eq!(fx.run().context.deadlines.get(PumpDeadline::Decision), None);
    let records = fx.records(Kind::Refusal);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["causal_parent"], serde_json::json!(parent));
    let expected = LifecycleRefusal::PlanUpdateRefused {
        case: s::PlanUpdateCase::ReadRefused,
        detail: "scripted read refusal".into(),
        rejected_blocks: vec![],
        read_code: Some(s::ReadRefusalCode::ResultUnavailable),
    };
    let record: RefusalRecord = serde_json::from_value(records[0]["payload"].clone()).unwrap();
    assert_eq!(
        record,
        RefusalRecord::Lifecycle {
            asked: LifecycleAsk::PlanUpdate,
            refusal: expected.clone()
        }
    );
    assert_eq!(
        fx.run()
            .context
            .root_plan_work()
            .unwrap()
            .refusal_sequence()
            .map(serde_json::to_value)
            .transpose()
            .unwrap(),
        Some(records[0]["sequence"].clone())
    );
    let Some(RootPlanCompletion::Refused { refusal, account }) = take(&mut fx) else {
        panic!("recorded root refusal required")
    };
    assert_eq!(refusal, expected);
    assert_eq!(account.packet().input().2, document());
    assert_eq!(
        account
            .work()
            .read_refusal()
            .unwrap()
            .measurement_sequence(),
        parent
    );
    assert!(fx.run().context.root_plan_work().is_none());
    assert_eq!(fx.run().context.committed_plan_summary().unwrap(), None);
    assert!(take(&mut fx).is_none());
    begin(&mut fx);
    fx.run().context.cancel_root_plan_update();
    assert_eq!(advance(&mut fx), RootPlanProgress::Cancelled);
    assert_eq!(settle(&mut fx).unwrap(), RootPlanProgress::Complete);
    assert!(matches!(
        take(&mut fx),
        Some(RootPlanCompletion::Cancelled(_))
    ));
    assert_eq!(fx.records(Kind::Refusal).len(), 1);
}

#[test]
#[ignore = "actual root coordination peer"]
fn root_plan_settlement_success_requires_summary_and_freezes_before_late_stop() {
    let (mut fx, _boot, certificate) = ready_with_inventory(inventory());
    prepare(&mut fx, "create");
    assert_eq!(settle(&mut fx).unwrap_err(), RunFailure::MarksOwned);
    assert!(take(&mut fx).is_none());
    start_effect(&mut fx, &certificate);
    assert_eq!(finish_effect(&mut fx, false), RootPlanProgress::EffectFiled);
    assert_eq!(settle(&mut fx).unwrap_err(), RunFailure::MarksOwned);
    let expected = updated_snapshot(&mut fx);
    release_summary(&mut fx, expected.clone());
    assert!((0..128).any(|_| advance(&mut fx) == RootPlanProgress::SummaryReady));
    assert!(take(&mut fx).is_none());
    let original_bound = fx.run().context.root_plan_bound().unwrap();
    assert_eq!(settle(&mut fx).unwrap(), RootPlanProgress::Complete);
    fx.run().context.cancel_root_plan_update();
    // Retrieval after the completed operation's old deadline cannot turn its
    // already checked receipt into a fresh wait or a cancellation.
    let mut late = SummaryBoundaryClock {
        before: original_bound.deadline_ns(),
        after: original_bound.deadline_ns(),
        boundary: 1,
        samples: 0,
        broken: false,
    };
    let Some(RootPlanCompletion::Updated(updated)) =
        fx.run().context.take_root_plan_update(&mut late).unwrap()
    else {
        panic!("late Stop changed settled success")
    };
    assert_eq!(
        updated.answer(),
        LifecycleAnswer::PlanUpdated {
            plan: expected.plan.clone(),
            source: expected.last_source.clone()
        }
    );
    assert!(updated.account().work().summary().is_some());
    assert_eq!(
        fx.run()
            .context
            .committed_plan_summary()
            .unwrap()
            .unwrap()
            .plan,
        expected.plan
    );
    assert!(fx.run().context.root_plan_work().is_none());
    assert_eq!(fx.run().context.deadlines.get(PumpDeadline::Decision), None);
    assert!(fx.records(Kind::Refusal).is_empty());
}

#[test]
#[ignore = "actual root coordination peer"]
fn root_plan_settlement_waiting_and_pending_refusal_keep_their_claim_through_stop() {
    for queued in [false, true] {
        let (mut fx, _boot) = refused_update();
        let original = fx.run().context.root_plan_bound().unwrap();
        if queued {
            fx.boot.member.fill_trace();
            let run = fx.run();
            let label = AbsoluteBound::after(run.clock.now().unwrap(), 100).unwrap();
            run.context
                .arm(PumpDeadline::Label, label, &mut run.clock)
                .unwrap();
            assert_eq!(
                settle(&mut fx),
                Err(RunFailure::Wait {
                    key: PumpDeadline::Label,
                    bound: label
                })
            );
            let run = fx.run();
            assert_eq!(
                run.context.root_plan_work().unwrap().refusal_ticket(),
                Some(run.context.pending_author().unwrap().ticket)
            );
            run.context.complete_wait(PumpDeadline::Label).unwrap();
        } else {
            fx.run().context.pause_claim_queue_for_test = true;
            assert_eq!(settle(&mut fx), Err(RunFailure::MarksOwned));
            assert!(fx.run().context.waiting_content.is_some());
            assert!(
                fx.run()
                    .context
                    .root_plan_work()
                    .unwrap()
                    .refusal_ticket()
                    .is_none()
            );
        }
        let ticket = fx.run().context.root_plan_work().unwrap().refusal_ticket();
        let run = fx.run();
        let foreign = AuthorClaim::RootPlan(std::sync::Arc::new(()));
        assert_eq!(
            run.context
                .resume_claimed_author_and_wait(&mut ports(&mut run.clock), Some(foreign))
                .unwrap_err()
                .reason,
            RunFailure::MarksOwned
        );
        assert_eq!(
            run.context.root_plan_work().unwrap().refusal_ticket(),
            ticket
        );
        assert_eq!(
            run.context.deadlines.get(PumpDeadline::Decision),
            Some(original)
        );
        assert!(take(&mut fx).is_none());
        fx.run().context.cancel_root_plan_update();
        // The worker resumes the operation reducer before settlement. It must
        // not turn an already owed refusal into an unrecorded cancellation.
        assert_eq!(advance(&mut fx), RootPlanProgress::RefusalReady);
        if queued {
            fx.drain();
        }
        assert_eq!(settle(&mut fx).unwrap(), RootPlanProgress::Complete);
        let Some(RootPlanCompletion::Cancelled(account)) = take(&mut fx) else {
            panic!("cancelled operation cannot answer success")
        };
        assert!(account.work().refusal_sequence().is_some());
        assert_eq!(fx.records(Kind::Refusal).len(), 1);
        assert_eq!(fx.run().context.committed_plan_summary().unwrap(), None);
        assert!(fx.run().context.halt().is_none());
    }
}

#[test]
#[ignore = "actual root coordination peer"]
fn root_plan_settlement_post_record_clock_failure_retains_exact_refusal_without_success() {
    let mut witnessed = false;
    for boundary in 1..=24 {
        let (mut fx, _boot) = refused_update();
        let run = fx.run();
        let before = run.clock.now().unwrap();
        let mut clock = SummaryBoundaryClock {
            before,
            after: before,
            boundary,
            samples: 0,
            broken: true,
        };
        let result = run.context.settle_root_plan_update(&mut ports(&mut clock));
        let records = fx.records(Kind::Refusal);
        if result.is_ok() || records.len() != 1 {
            continue;
        }
        witnessed = true;
        let work = fx.run().context.root_plan_work().unwrap();
        assert_eq!(
            serde_json::json!(work.refusal_sequence().unwrap()),
            records[0]["sequence"]
        );
        assert!(work.refusal_ticket().is_some());
        assert!(work.summary().is_none());
        let run = fx.run();
        assert!(run.context.take_root_plan_update(&mut run.clock).is_err());
        assert!(run.context.root_plan_work().is_some());
        break;
    }
    assert!(
        witnessed,
        "fixture did not cross a real Recorded refusal before failing"
    );
}

#[test]
#[ignore = "actual root coordination peer"]
fn root_plan_settlement_foreign_author_keeps_first_fatal_and_original_custody() {
    let (mut fx, _boot) = refused_update();
    fx.run().context.pause_claim_queue_for_test = true;
    assert_eq!(settle(&mut fx), Err(RunFailure::MarksOwned));
    let run = fx.run();
    let original_claim = run.context.root_plan_author_claim().unwrap();
    let first = run.context.fail(
        &mut run.clock,
        FlowHaltReason::TraceFailed,
        HaltPoint::Author,
        "root settlement first fatal",
    );
    for claim in [None, Some(AuthorClaim::RootPlan(std::sync::Arc::new(())))] {
        assert_eq!(
            run.context
                .continue_claimed_content(&mut ports(&mut run.clock), claim.clone())
                .unwrap_err()
                .reason,
            first
        );
        assert_eq!(
            run.context
                .resume_claimed_author_and_wait(&mut ports(&mut run.clock), claim)
                .unwrap_err()
                .reason,
            first
        );
        assert!(
            run.context
                .waiting_content
                .as_ref()
                .unwrap()
                .matches(Some(&original_claim))
        );
        assert!(
            run.context
                .root_plan_work()
                .unwrap()
                .refusal_ticket()
                .is_none()
        );
        assert!(
            run.context
                .root_plan_work()
                .unwrap()
                .refusal_sequence()
                .is_none()
        );
        assert_eq!(run.context.failure(), Some(&first));
        assert!(run.context.take_root_plan_update(&mut run.clock).is_err());
    }
}
