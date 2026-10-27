//! Retain initial classifier admission under the original expired Enter and
//! the actual rollback Leave. It grants no Ready, template or bootstrap work.
use super::worker_startup::expired::ExpiredStartupAccountPermit;
use super::*;
use crate::flow::coordinator::StoppedClassifierAdmission;
use std::sync::Arc;

pub(in crate::flow) struct ClassifierAdmissionRollbackPermit {
    barrier: Arc<()>,
}
impl ClassifierAdmissionRollbackPermit {
    pub(in crate::flow) fn matches_record(&self, barrier: &RecordBarrier) -> bool {
        Arc::ptr_eq(&self.barrier, &barrier.author_identity())
    }
}

pub(super) struct StoppedAdmissionAccount {
    pub(super) enter: AbsoluteBound,
    pub(super) admission: StoppedClassifierAdmission,
}
impl StoppedAdmissionAccount {
    pub(super) fn retire_transport(&mut self) {
        self.admission.retire_transport();
    }
}

impl RecordingContext {
    pub(super) fn can_detach_expired_admission(
        &self,
        permit: &ExpiredStartupAccountPermit,
    ) -> bool {
        permit.allows(self)
            && self.flow.starting_classifier().is_some()
            && self.flow.standing_classifier().is_none()
            && !self.flow.admission_refused()
    }
    pub(super) fn detach_expired_admission(
        &mut self,
        permit: &ExpiredStartupAccountPermit,
        io: &mut impl PumpIo,
    ) -> Result<StoppedAdmissionAccount, RunFailure> {
        self.release_live(io)?;
        if !self.can_detach_expired_admission(permit)
            || self.template_checks.is_some()
            || self.template_blocked_author.is_some()
            || self.template_setup_refusal.is_some()
            || self.read.is_some()
            || self.inspection.is_some()
            || self.boot_pairs.is_some()
            || self.repair.is_some()
            || self.plan_summary.is_some()
            || self.boot_open.is_some()
            || self.measure_blocks_external()
            || self.place_owed()
            || self.filing.is_some()
            || self.output.is_some()
            || self.pair_work.is_some()
            || self.marks.is_some()
            || self.member_capture.is_some()
            || self.freshness.is_some()
            || self.open_turn.is_some()
            || self.input.snapshot().is_some()
            || self.input_turn.is_some()
            || self.input_author.is_some()
            || self.waiting_content.is_some()
            || self.completed_author.is_some()
            || self.enter_refusal_record.is_some()
        {
            return Err(RunFailure::TemplateBlocked);
        }
        self.check_held_input()?;
        let seal = ClassifierAdmissionRollbackPermit {
            barrier: self.barrier.author_identity(),
        };
        let admission = self
            .flow
            .detach_expired_classifier_admission(
                &mut FlowRecording {
                    barrier: &mut self.barrier,
                    inbox: &mut self.inbox,
                    budget: &mut self.budget,
                    author: &self.author,
                },
                &seal,
            )
            .map_err(|error| self.flow_failed(io, error))?;
        Ok(StoppedAdmissionAccount {
            enter: self.deadlines.get(PumpDeadline::Enter).unwrap(),
            admission,
        })
    }
}
