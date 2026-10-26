//! Move only an expired template owner's classifier and author slots into
//! rollback custody. The state seam remains available for cleanup Grants.
use super::*;
use crate::flow::context::TemplateRollbackPermit;
use crate::flow::ste100_socket::{RetiredSocketPort, SocketPort};

pub(in crate::flow) struct StoppedTemplateCheck {
    live: Option<Classifier<SocketPort>>,
    retired: Option<Classifier<RetiredSocketPort>>,
    _operation: Option<CheckOperation>,
    _completion: Option<OperationCompletion>,
    _release: Option<ClassifierRelease>,
    _pending: Option<PendingRecord>,
    _record: crate::flow::recording::StoppedStartupRecord,
}

impl StoppedTemplateCheck {
    #[cfg(test)]
    pub(in crate::flow) fn transport_retired(&self) -> bool {
        self.live.is_none() && self.retired.is_some()
    }
    /// Only rollback's sealed release, actual failed exit or expired Leave
    /// disposes this endpoint. Its original semantic/framing evidence stays.
    pub(in crate::flow) fn retire_transport(&mut self) {
        if let Some(classifier) = self.live.take() {
            self.retired = Some(classifier.retire_into(SocketPort::retire_failed));
        }
    }

    pub(in crate::flow) fn request(&self) -> Option<&Ste100Request> {
        if let Some(classifier) = &self.live {
            classifier.outstanding()
        } else {
            self.retired.as_ref()?.outstanding()
        }
    }
    pub(in crate::flow) fn reply(&self) -> Option<&Ste100Reply> {
        let held = if let Some(classifier) = &self.live {
            classifier.held()
        } else {
            self.retired.as_ref().and_then(Classifier::held)
        };
        held.or_else(|| match self._completion.as_ref() {
            Some(OperationCompletion::Check(record)) => Some(record.reply()),
            _ => None,
        })
    }
    #[cfg(test)]
    pub(in crate::flow) fn test_record(&self) -> Option<(&weaver_trace::serving::Event, u64)> {
        self._record.test_record()
    }
    #[cfg(test)]
    pub(in crate::flow) fn test_recorded_source(&self) -> Option<&SourcePosition> {
        match self._completion.as_ref() {
            Some(OperationCompletion::Check(record)) => Some(record.source()),
            _ => self._release.as_ref().map(|release| &release.source),
        }
    }
}

impl FlowCoordinator<SocketPort> {
    pub(in crate::flow) fn detach_expired_template_check(
        &mut self,
        cx: &mut FlowRecording<'_>,
        permit: &TemplateRollbackPermit,
    ) -> Result<StoppedTemplateCheck, FlowFailure> {
        self.observe_failures(cx)?;
        let expected = permit.operation();
        let request_id = permit.request_id();
        let classifier_matches = self.classifier.as_ref().is_some_and(|classifier| {
            classifier.run() == self.run && classifier.scheme() == &self.scheme
                && classifier.failure().is_none()
                && match request_id {
                    None => classifier.state() == ClassifierState::Idle
                        && classifier.outstanding().is_none() && classifier.held().is_none()
                        && self.check_operation.is_none() && self.operation_completion.is_none(),
                    Some(id) => {
                        let in_flight = matches!(classifier.state(), ClassifierState::InFlight | ClassifierState::ResultHeld)
                            && self.check_operation.as_ref() == expected
                            && self.operation_completion.is_none()
                            && classifier.outstanding().is_some_and(|request| {
                                request.request_id == id && request.scheme_id == self.scheme.scheme_id
                                    && expected.is_some_and(|op| request.target == op.target() && request.text == op.text())
                            });
                        let completed = classifier.state() == ClassifierState::Idle
                            && classifier.outstanding().is_none() && classifier.held().is_none()
                            && self.check_operation.is_none()
                            && matches!(self.operation_completion.as_ref(), Some(OperationCompletion::Check(record))
                                if Some(record.operation()) == expected && record.reply().request_id == id
                                    && record.source().block_id.run == self.run);
                        in_flight || completed
                    }
                }
        });
        let record_matches = match self.pending_record {
            None => {
                cx.barrier.pending().is_none()
                    && cx.barrier.pending_author().is_none()
                    && self.classifier_release.is_none()
            }
            Some(PendingRecord::Classifier) => {
                self.classifier_release.is_none()
                    && self
                        .classifier
                        .as_ref()
                        .and_then(Classifier::held)
                        .is_some_and(|reply| cx.barrier.pending_classifier_result_matches(reply))
            }
            Some(PendingRecord::ClassifierRelease) => {
                self.classifier_release.is_some()
                    && self
                        .classifier
                        .as_ref()
                        .and_then(Classifier::held)
                        .is_some()
                    && cx.barrier.pending().is_none()
                    && cx.barrier.pending_author().is_none()
            }
            _ => false,
        };
        if self.phase != CoordinatorPhase::Active
            || self.classifier_retired
            || self.starting_classifier.is_some()
            || self.admission_completion.is_some()
            || self.state_retired
            || self.state.as_ref().is_none_or(|state| {
                !state.outstanding().is_empty()
                    || state.held_measurement().is_some()
                    || state.failure().is_some()
            })
            || self.state_operation.is_some()
            || self.state_recorded_ns.is_some()
            || self.state_release_timing.is_some()
            || self.stopped_plan_read.is_some()
            || self.capture.is_some()
            || self.receipt_reservation.is_some()
            || self.output_checks.is_some()
            || self.output_receipts.is_some()
            || self.instruction_check.is_some()
            || self.input_account.is_some()
            || self.reuse.is_some()
            || self.repair_measurement.is_some()
            || self.refused_author.is_some()
            || self.ledger.receipts().is_some()
            || self.ledger.facts().next().is_some()
            || cx.inbox.queued().next().is_some()
            || cx.inbox.failure().is_some()
            || cx.budget.waiting_copy().is_some()
            || cx.budget.gauges() != (0, 0)
            || !self.notices.is_empty()
            || !classifier_matches
            || !record_matches
            || !permit.matches_record(cx.barrier)
        {
            return Err(FlowFailure {
                point: "barrier",
                detail: "expired template rollback retains foreign work".into(),
            });
        }
        let record = cx
            .barrier
            .retain_template_rollback_record(permit)
            .map_err(|detail| FlowFailure {
                point: "barrier",
                detail: detail.into(),
            })?;
        Ok(StoppedTemplateCheck {
            live: self.classifier.take(),
            retired: None,
            _operation: self.check_operation.take(),
            _completion: self.operation_completion.take(),
            _release: self.classifier_release.take(),
            _pending: self.pending_record.take(),
            _record: record,
        })
    }
}
