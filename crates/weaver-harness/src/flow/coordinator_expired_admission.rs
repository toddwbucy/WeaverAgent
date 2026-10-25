//! Move an expired initial classifier admission out of the pump without
//! completing it. Only rollback's later release closes its actual endpoint.
use super::*;
use crate::flow::context::ClassifierAdmissionRollbackPermit;
use crate::flow::ste100_socket::{AdmissionPoll, RetiredSocketPort, SocketPort};

pub(in crate::flow) struct StoppedClassifierAdmission {
    live: Option<SocketPort>,
    retired: Option<RetiredSocketPort>,
    completion: Option<AdmissionPoll>,
    _probe_pending: bool,
}
impl StoppedClassifierAdmission {
    pub(in crate::flow) fn retire_transport(&mut self) {
        if let Some(port) = self.live.take() {
            self.retired = Some(port.retire_failed());
        }
    }
    pub(in crate::flow) fn completion(&self) -> Option<&AdmissionPoll> {
        self.completion.as_ref()
    }
    /// An actual refusal still owes its original Enter-bounded account. Retain
    /// it, but neither renew that authoring window nor report successful cleanup.
    pub(in crate::flow) fn account_complete(&self) -> bool {
        !matches!(self.completion, Some(AdmissionPoll::Refused(_)))
    }
    #[cfg(test)]
    pub(in crate::flow) fn transport_retired(&self) -> bool {
        self.live.is_none() && self.retired.is_some()
    }
    #[cfg(test)]
    pub(in crate::flow) fn test_evidence(&self) -> String {
        match (&self.live, &self.retired) {
            (Some(port), None) => port.test_evidence(),
            (None, Some(port)) => port.test_evidence(),
            _ => panic!("one retained admission transport"),
        }
    }
}

impl FlowCoordinator<SocketPort> {
    pub(in crate::flow) fn detach_expired_classifier_admission(
        &mut self,
        cx: &mut FlowRecording<'_>,
        permit: &ClassifierAdmissionRollbackPermit,
    ) -> Result<StoppedClassifierAdmission, FlowFailure> {
        self.observe_failures(cx)?;
        if !permit.matches_record(cx.barrier)
            || self.phase != CoordinatorPhase::Active
            || self.classifier.is_some()
            || self.classifier_retired
            || self.admission_refused
            || self.starting_classifier.as_ref().is_none_or(|port| {
                !port.matches_admission(&self.scheme, self.admission_completion.as_ref())
            })
            || self.state_retired
            || self
                .state
                .as_ref()
                .is_none_or(|state| !state.can_begin_filing() || state.failure().is_some())
            || self.state_recorded_ns.is_some()
            || self.state_release_timing.is_some()
            || self.stopped_plan_read.is_some()
            || self.classifier_release.is_some()
            || self.capture.is_some()
            || self.output_receipts.is_some()
            || self.repair_measurement.is_some()
            || self.refused_author.is_some()
            || cx.barrier.pending().is_some()
            || cx.barrier.pending_author().is_some()
            || !self.settled_except_author_and_admission(cx)
        {
            return Err(FlowFailure {
                point: "barrier",
                detail: "expired classifier admission retains foreign work".into(),
            });
        }
        let account = StoppedClassifierAdmission {
            live: self.starting_classifier.take(),
            retired: None,
            completion: self.admission_completion.take(),
            _probe_pending: self.admission_probe_pending,
        };
        self.admission_probe_pending = false;
        Ok(account)
    }
}
