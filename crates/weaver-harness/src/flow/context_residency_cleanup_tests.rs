use super::*;
use crate::flow::context::failed_enter_driver::tests::{records, take_context};
use crate::flow::context::tests::{Fixture, config};
use std::time::Duration;
use weaver_trace::ReleaseOutcome;

struct RejectedGuard(Option<CleanupJoinRejected>);
impl Drop for RejectedGuard {
    fn drop(&mut self) {
        if let Some(rejected) = &mut self.0 {
            rejected.resources.reap_startup_test_children();
        }
    }
}
struct CleanupGuard(Option<ResidencyCleanup>);
impl Drop for CleanupGuard {
    fn drop(&mut self) {
        if let Some(owner) = &mut self.0 {
            owner.cleanup.reap_composition_test_children();
        }
    }
}

pub(in crate::flow::context) fn drive(
    fx: &mut Fixture,
    mut resources: FailedEnterResources,
    mut account: StoppedResidency,
    mode: &str,
) {
    let mut context = take_context(fx);
    let origin = context.origin_ns();
    let first = context.failure().cloned();
    if mode == "cleanup-before-fork" {
        assert!(resources.spu_child().is_none());
        let rejected = match account.into_cleanup(context, resources) {
            Err(rejected) => rejected,
            Ok(_) => panic!("no-SPU terminal domain was activated before #285"),
        };
        let guard = RejectedGuard(Some(rejected));
        let rejected = guard.0.as_ref().unwrap();
        assert!(matches!(rejected.refusal, CleanupJoinRefusal::NoSpu));
        assert_eq!(rejected.context.failure(), first.as_ref());
        assert_eq!(
            rejected
                .context
                .barrier
                .recorder()
                .structure()
                .by_kind(Kind::Unload)
                .count(),
            0
        );
        return;
    }
    let raw = account.reply.as_ref().unwrap().raw_bytes().to_vec();
    let raw_ptr = account.reply.as_ref().unwrap().raw_bytes().as_ptr();
    assert!(matches!(
        account.startup.initial_grants,
        InspectionCompletion::Grants { .. }
    ));
    assert!(resources.residency_request().unwrap().send().sent());

    if mode == "cleanup-ready-foreign" {
        let mut foreign = Fixture::new(config());
        let rejected = match account.into_cleanup(take_context(&mut foreign), resources) {
            Err(rejected) => rejected,
            Ok(_) => panic!("foreign context accepted actual residency account"),
        };
        let mut guard = RejectedGuard(Some(rejected));
        assert!(matches!(
            guard.0.as_ref().unwrap().refusal,
            CleanupJoinRefusal::ForeignContext
        ));
        let returned = guard.0.take().unwrap();
        account = returned.account;
        resources = returned.resources;
        assert_eq!(
            account.reply.as_ref().unwrap().raw_bytes().as_ptr(),
            raw_ptr
        );
    }
    if mode == "cleanup-ready-leave" {
        let leave =
            AbsoluteBound::from_absolute(fx.clock.now().unwrap(), fx.clock.now().unwrap() + 10_000)
                .unwrap();
        context.deadlines.arm(PumpDeadline::Leave, leave).unwrap();
        let rejected = match account.into_cleanup(context, resources) {
            Err(rejected) => rejected,
            Ok(_) => panic!("accepted Leave lost its cause through generic cleanup"),
        };
        let mut guard = RejectedGuard(Some(rejected));
        assert!(matches!(
            guard.0.as_ref().unwrap().refusal,
            CleanupJoinRefusal::AcceptedLeave
        ));
        let returned = guard.0.take().unwrap();
        context = returned.context;
        resources = returned.resources;
        account = returned.account;
        assert_eq!(context.deadlines.disarm(PumpDeadline::Leave), Some(leave));
    }
    if mode == "cleanup-refuse" {
        let rejected = match account.into_cleanup(context, resources) {
            Err(rejected) => rejected,
            Ok(_) => panic!("semantic refusal became Failed cleanup"),
        };
        let guard = RejectedGuard(Some(rejected));
        let rejected = guard.0.as_ref().unwrap();
        assert!(matches!(
            rejected.refusal,
            CleanupJoinRefusal::Cleanup(FailedEnterRefusal::NotFailed)
        ));
        assert!(rejected.context.failure().is_none());
        assert_eq!(rejected.context.phase(), RecordingPhase::Entered);
        assert_eq!(
            rejected
                .account
                .reply
                .as_ref()
                .unwrap()
                .raw_bytes()
                .as_ptr(),
            raw_ptr
        );
        assert!(rejected.resources.residency_request().is_some());
        return;
    }
    let first = first.expect("actual failure before consuming teardown");
    let deadline = context.flow.halt().unwrap().deadline().unwrap();
    let owner = match account.into_cleanup(context, resources) {
        Ok(owner) => owner,
        Err(rejected) => {
            let guard = RejectedGuard(Some(rejected));
            panic!(
                "actual failed composition refused: {:?}",
                guard.0.as_ref().unwrap().refusal
            );
        }
    };
    let mut guard = CleanupGuard(Some(owner));
    for _ in 0..3000 {
        let owner = guard.0.as_mut().unwrap();
        assert_eq!(owner.failure(), &first);
        assert_eq!(owner.deadline(), Some(deadline));
        if owner.advance(&mut fx.clock) == FailedEnterProgress::Closed {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    let finished = match guard.0.take().unwrap().finish() {
        Ok(finished) => finished,
        Err(owner) => {
            guard.0 = Some(owner);
            panic!("cleanup did not finish");
        }
    };
    assert_eq!(finished.cleanup.failure(), &first);
    assert_eq!(finished.cleanup.context().origin_ns(), origin);
    assert_eq!(finished.cleanup.context().phase(), RecordingPhase::Closed);
    assert!(!finished.cleanup.resources().has_unreaped_child());
    assert!(!finished.cleanup.abandoned());
    assert!(finished.cleanup.trace_drained());
    assert!(finished.cleanup.unload_sequence().is_some());
    assert!(finished.cleanup.unload_unavailable().is_none());
    assert_eq!(
        finished
            .cleanup
            .prior_residency_reply()
            .unwrap()
            .raw_bytes(),
        raw
    );
    assert_eq!(
        finished
            .cleanup
            .prior_residency_reply()
            .unwrap()
            .raw_bytes()
            .as_ptr(),
        raw_ptr
    );
    assert!(
        finished.account.reply.is_none(),
        "reply moved into teardown once"
    );
    assert!(matches!(
        finished.account.startup.initial_grants,
        InspectionCompletion::Grants { .. }
    ));
    if matches!(mode, "cleanup-wrong" | "cleanup-release-refusal") {
        assert_eq!(
            finished.cleanup.release_outcome(),
            ReleaseOutcome::Unconfirmed
        );
        assert!(finished.cleanup.release_send().is_none());
        assert!(finished.cleanup.release_unavailable().is_some());
    } else {
        assert_eq!(
            finished.cleanup.release_outcome(),
            ReleaseOutcome::Confirmed
        );
        assert!(finished.cleanup.release_send().unwrap().sent());
        assert!(finished.cleanup.release_reply().is_some());
    }
    let rows = records(fx);
    let unloads: Vec<_> = rows.iter().filter(|row| row["kind"] == "unload").collect();
    assert_eq!(unloads.len(), 1);
    assert_eq!(unloads[0]["payload"]["end_reason"], "component_failure");
    assert_eq!(unloads[0]["payload"]["grant_surface"], "unreadable");
    assert_eq!(
        unloads[0]["payload"]["release"]["spu"],
        if matches!(mode, "cleanup-wrong" | "cleanup-release-refusal") {
            "unconfirmed"
        } else {
            "confirmed"
        }
    );
}

macro_rules! scenario {
    ($name:ident, $mode:literal) => {
        #[test]
        fn $name() {
            // This joined fixture retains several complete success/refusal
            // return values at once. Use an explicit 8 MiB fixture stack;
            // production stack and memory qualification remain separate.
            std::thread::Builder::new()
                .stack_size(8 * 1024 * 1024)
                .spawn(|| crate::flow::context::startup_composition::tests::residency($mode))
                .unwrap()
                .join()
                .unwrap();
        }
    };
}
scenario!(
    residency_teardown_composition_retains_consumed_admitted_through_release_and_unload,
    "cleanup-ready"
);
scenario!(
    residency_teardown_composition_keeps_wrong_reply_and_never_sends_release,
    "cleanup-wrong"
);
scenario!(
    residency_teardown_composition_returns_foreign_context_inputs_without_losing_custody,
    "cleanup-ready-foreign"
);
scenario!(
    residency_teardown_composition_rejects_leave_before_losing_its_cause,
    "cleanup-ready-leave"
);
scenario!(
    residency_teardown_composition_semantic_refusal_does_not_create_failure,
    "cleanup-refuse"
);

scenario!(
    residency_teardown_composition_keeps_no_spu_domain_held_and_returns_actual_inputs,
    "cleanup-before-fork"
);
scenario!(
    residency_teardown_composition_release_only_refusal_cannot_close_admit,
    "cleanup-release-refusal"
);
