//! Consume an expired boot summary between reads without renewing its authority.
//! The original prerequisite, read proof and disposition remain stopped evidence.
use super::*;
use crate::flow::context::worker_startup::expired::ExpiredStartupAccountPermit;

impl PlanSummaryTransaction {
    fn expired_boot_owner_matches(&self, context: &RecordingContext) -> bool {
        Some(self.rollback.bound) == context.deadlines.get(PumpDeadline::Enter)
            && self.expired_boot_evidence_matches(context)
    }

    fn expired_boot_evidence_matches(&self, context: &RecordingContext) -> bool {
        matches!(&self.prerequisite, Prerequisite::Boot(repaired)
            if context.boot_proof_current(repaired)
                && repaired.checked_open().is_some_and(|open| open.enter() == self.rollback.bound))
            && !self.rollback.driving
            && self.rollback.key == PumpDeadline::Enter
            && self.rollback.account_deadline <= self.rollback.bound.deadline_ns()
            && self.rollback.account_deadline <= context.last_ns
            && self.rollback.stopped_read.is_none()
            && (!self.rollback.started || self.rollback.read_settled || self.rollback.complete)
            && context.plan_cache.boot_started
            && context.plan_cache.required.is_none()
            && context.plan_cache.invalid.is_none()
            && self.recorded.as_ref().is_none_or(|recorded| {
                recorded.read().belongs_to(context)
                    && context
                        .plan_cache
                        .committed
                        .as_ref()
                        .is_none_or(|cached| Arc::ptr_eq(cached, recorded))
            })
    }

    #[cfg(test)]
    pub(in crate::flow::context) fn expired_test_evidence(&self) -> String {
        format!("{:?}", (self, self.recorded.as_ref().map(Arc::as_ptr)))
    }
}

impl PlanSummaryAccount {
    pub(in crate::flow::context) fn permits_expired_grants(
        &self,
        context: &RecordingContext,
        permit: &crate::flow::context::worker_startup::expired::ExpiredStartupGrantsPermit,
    ) -> bool {
        permit.allows(context)
            && self.work.expired_boot_evidence_matches(context)
            && context.retired_enter == Some(self.work.rollback.bound)
            && context.plan_summary.is_none()
            && context.plan_cache.unavailable == Some(PlanSummaryUnavailable::Revoked)
    }

    #[cfg(test)]
    pub(in crate::flow::context) fn expired_test_evidence(&self) -> String {
        self.work.expired_test_evidence()
    }
}

impl RecordingContext {
    #[cfg(test)]
    pub(in crate::flow::context) fn expired_summary_guard_probes(
        &mut self,
        permit: &ExpiredStartupAccountPermit,
    ) {
        assert!(self.can_detach_expired_boot_summary(permit));
        self.plan_summary.as_mut().unwrap().rollback.key = PumpDeadline::Decision;
        assert!(
            !self.can_detach_expired_boot_summary(permit),
            "summary keeps its original Enter key"
        );
        self.plan_summary.as_mut().unwrap().rollback.key = PumpDeadline::Enter;
        let old = self.plan_summary.as_ref().unwrap().rollback.bound;
        self.plan_summary.as_mut().unwrap().rollback.bound =
            AbsoluteBound::from_absolute(old.began_ns() + 1, old.deadline_ns()).unwrap();
        assert!(
            !self.can_detach_expired_boot_summary(permit),
            "summary keeps its original Enter bound"
        );
        self.plan_summary.as_mut().unwrap().rollback.bound = old;
        self.plan_summary.as_mut().unwrap().rollback.driving = true;
        assert!(
            !self.can_detach_expired_boot_summary(permit),
            "cannot detach a driving summary owner"
        );
        self.plan_summary.as_mut().unwrap().rollback.driving = false;
        let journal = self.boot_journal.take().unwrap();
        assert!(
            !self.can_detach_expired_boot_summary(permit),
            "checked boot journal remains required"
        );
        self.boot_journal = Some(journal);
        assert!(self.can_detach_expired_boot_summary(permit));
    }
    #[cfg(test)]
    pub(in crate::flow::context) fn expired_summary_cache_evidence(&self) -> String {
        format!(
            "{:?}",
            (
                &self.plan_cache.committed,
                self.plan_cache.committed.as_ref().map(Arc::as_ptr)
            )
        )
    }

    pub(in crate::flow::context) fn can_detach_expired_boot_summary(
        &self,
        permit: &ExpiredStartupAccountPermit,
    ) -> bool {
        permit.allows(self)
            && self
                .plan_summary
                .as_ref()
                .is_some_and(|work| work.expired_boot_owner_matches(self))
            && self.read.is_none()
            && self.deadlines.get(PumpDeadline::StateControl).is_none()
            && self.read_rollback_permit().is_ok()
    }

    pub(in crate::flow::context) fn detach_expired_boot_summary(
        &mut self,
        permit: &ExpiredStartupAccountPermit,
        io: &mut impl PumpIo,
    ) -> Result<PlanSummaryAccount, RunFailure> {
        self.release_live(io)?;
        if !self.can_detach_expired_boot_summary(permit)
            || self.repair.is_some()
            || self.boot_pairs.is_some()
            || self.filing.is_some()
            || self.output.is_some()
            || self.pair_work.is_some()
            || self.marks.is_some()
            || self.member_capture.is_some()
            || self.freshness.is_some()
            || self.inspection.is_some()
            || self.measure_blocks_external()
            || self.boot_open_blocks_external()
            || self.place_owed()
            || self.open_turn.is_some()
            || self.input.snapshot().is_some()
            || self.input_turn.is_some()
            || self.input_author.is_some()
            || self.waiting_content.is_some()
            || self.completed_author.is_some()
        {
            return Err(RunFailure::PlanSummaryOwned);
        }
        self.flow
            .require_releasing(&mut FlowRecording {
                barrier: &mut self.barrier,
                inbox: &mut self.inbox,
                budget: &mut self.budget,
                author: &self.author,
            })
            .map_err(|failure| self.flow_failed(io, failure))?;
        self.check_held_input()?;
        // Revocation prevents future cache use. It does not rewrite a completed
        // read, cancellation, refusal or accounting deadline in the moved owner.
        self.plan_cache.revoke();
        Ok(PlanSummaryAccount {
            work: Box::new(self.plan_summary.take().unwrap()),
        })
    }
}
