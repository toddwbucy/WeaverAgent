//! Test the public disposition on an actual completed rollback owner. Its
//! successful terminal stays present while each independent disqualifier is
//! varied, so an absent terminal cannot mask a deleted outer guard.
use super::*;
use crate::flow::coordinator::FlowRecording;
use crate::flow::report::{FlowHaltReason, HaltPoint, SecondaryFailure};

impl FoundationEnteredRollback {
    pub(in crate::flow::context::worker_startup) fn test_completed_exit_guards(&mut self) {
        use crate::FoundationProcessExit::{Failure, Success};
        assert_eq!(self.progress, FoundationEnteredRollbackProgress::Closed);
        assert_eq!(
            self.terminal.as_ref().unwrap().process_exit(),
            Some(Success)
        );
        assert_eq!(self.process_exit(), Some(Success));
        assert!(self.failed.is_none());
        assert!(self.run.as_ref().unwrap().context.failure.is_none());
        assert!(self.run.as_ref().unwrap().context.flow.halt().is_none());
        let sequence = self.unload_sequence();

        self.progress = FoundationEnteredRollbackProgress::Pending;
        assert_eq!(
            self.process_exit(),
            None,
            "terminal evidence does not close its outer owner"
        );
        self.progress = FoundationEnteredRollbackProgress::Closed;

        // Keep the successful run and terminal intact while varying only the
        // failed-owner slot. Moving the run would also trip the separate run
        // guard and hide a missing failed-owner check.
        let mut fixture =
            crate::flow::context::tests::Fixture::new(crate::flow::context::tests::config());
        let resources = fixture.context.failed_enter_resources().unwrap();
        crate::flow::context::failed_enter_driver::tests::state_eof(&mut fixture);
        let context = crate::flow::context::failed_enter_driver::tests::take_context(&mut fixture);
        self.failed = Some(FailedRunCleanup::after_startup(context, resources));
        assert_eq!(
            self.terminal.as_ref().unwrap().process_exit(),
            Some(Success)
        );
        assert_eq!(
            self.process_exit(),
            Some(Failure),
            "a retained failed owner overrides a successful terminal"
        );
        drop(self.failed.take());
        assert_eq!(self.process_exit(), Some(Success));

        assert!(
            self.terminal
                .as_mut()
                .unwrap()
                .test_swap_refusal(Some(weaver_types::LifecycleRefusal::NoResidency),)
                .is_none()
        );
        assert_eq!(
            self.terminal.as_ref().unwrap().process_exit(),
            Some(Failure)
        );
        assert_eq!(
            self.process_exit(),
            Some(Failure),
            "a completed terminal refusal is not a successful process exit"
        );
        assert_eq!(
            self.terminal.as_mut().unwrap().test_swap_refusal(None),
            Some(weaver_types::LifecycleRefusal::NoResidency),
        );
        assert_eq!(self.process_exit(), Some(Success));

        self.abandoned = true;
        assert_eq!(
            self.process_exit(),
            Some(Failure),
            "abandonment overrides a successful terminal"
        );
        self.abandoned = false;
        assert_eq!(self.process_exit(), Some(Success));

        self.run.as_mut().unwrap().context.failure = Some(RunFailure::Flow {
            reason: FlowHaltReason::TraceFailed,
            point: HaltPoint::Teardown,
            detail: "retained completion failure".into(),
        });
        assert_eq!(
            self.process_exit(),
            Some(Failure),
            "retained failure overrides a successful terminal"
        );
        self.run.as_mut().unwrap().context.failure = None;
        assert_eq!(self.process_exit(), Some(Success));

        let context = &mut self.run.as_mut().unwrap().context;
        context.flow.latch(
            SecondaryFailure {
                reason: FlowHaltReason::ProtocolFailed,
                point: HaltPoint::Teardown,
                detail: "retained completion halt".into(),
                observed_ns: Some(context.last_ns.into()),
            },
            context
                .deadlines
                .get(PumpDeadline::Leave)
                .map(AbsoluteBound::deadline_ns),
            &mut FlowRecording {
                barrier: &mut context.barrier,
                inbox: &mut context.inbox,
                budget: &mut context.budget,
                author: &context.author,
            },
        );
        assert!(
            context.failure.is_none(),
            "the halt guard is exercised independently"
        );
        assert!(context.flow.halt().is_some());
        assert_eq!(
            self.terminal.as_ref().unwrap().process_exit(),
            Some(Success)
        );
        assert_eq!(
            self.process_exit(),
            Some(Failure),
            "retained halt overrides a successful terminal"
        );
        assert_eq!(
            self.unload_sequence(),
            sequence,
            "disposition grants no new author"
        );
    }
}
