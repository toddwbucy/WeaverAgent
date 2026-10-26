//! Retain a boot filing in its original ledger while rollback releases resources.
//! No state progress, receipt disposition or new filing follows this transfer.
use super::*;
use crate::flow::context::BootFilingRollbackPermit;
use crate::flow::intake::{FlowInbox, RetentionBudget};
use weaver_trace::serving::AdmittedCopy;

pub(super) struct StoppedBootFiling {
    permit: BootFilingRollbackPermit,
    state: Option<StateChannel>,
    retired: Option<StateChannel<()>>,
    _reservation: ReceiptReservation,
    _receipts: Option<super::super::ledger::ReceiptSet>,
    _pending_record: Option<PendingRecord>,
    _record: super::super::recording::StoppedStartupRecord,
    gauges: (u64, u64),
    pub(super) parked: bool,
    close_error: Option<String>,
}

impl StoppedBootFiling {
    pub(super) fn failed(&self) -> bool {
        self.close_error.is_some()
            || self
                .state
                .as_ref()
                .is_some_and(|state| state.failure().is_some())
            || self
                .retired
                .as_ref()
                .is_some_and(|state| state.failure().is_some())
    }

    fn retire_transport(&mut self) -> weaver_trace::MemberRelease {
        if let Some(state) = self.state.take() {
            self.close_error = state
                .close_failed_read()
                .err()
                .map(|error| error.to_string());
            self.retired = Some(state.retire_failed());
        }
        if self.close_error.is_some() {
            weaver_trace::MemberRelease::Unconfirmed
        } else {
            weaver_trace::MemberRelease::Closed
        }
    }
}

fn copy_matches(copy: &AdmittedCopy, permit: &BootFilingRollbackPermit) -> bool {
    let AdmittedCopy::Pair(copy) = copy else {
        return false;
    };
    permit.source.as_ref().is_some_and(|source| {
        copy.block.source.run == source.run
            && copy.block.source.sequence.0 == source.sequence.0
            && copy.block.source.block == source.block.0
            && copy.block.source_path.is_empty()
            && serde_json::from_str::<weaver_types::state_flow::PairEnded>(&copy.block.raw)
                .is_ok_and(|ended| ended == permit.expected)
    })
}

impl<P: Ste100Port> FlowCoordinator<P> {
    #[cfg(test)]
    pub(in crate::flow) fn test_boot_filing_record(&self) -> Option<(Vec<u8>, u64)> {
        let (event, ticket) = self.stopped_boot_filing.as_ref()?._record.test_record()?;
        Some((serde_json::to_vec(event).unwrap(), ticket))
    }

    #[cfg(test)]
    pub(in crate::flow) fn test_boot_filing_evidence(&self) -> String {
        let (reservation, pending) = match self.stopped_boot_filing.as_ref() {
            Some(held) => (Some(&held._reservation), held._pending_record),
            None => (self.receipt_reservation.as_ref(), self.pending_record),
        };
        format!(
            "{:?}",
            (
                reservation,
                pending,
                &self.started_file,
                self.ledger.test_boot_filing_evidence(
                    self.stopped_boot_filing
                        .as_ref()
                        .and_then(|held| held._receipts.as_ref())
                )
            )
        )
    }

    #[cfg(test)]
    pub(in crate::flow) fn test_boot_filing_channel(&self) -> Option<String> {
        match self.stopped_boot_filing.as_ref() {
            Some(held) if held.parked => held
                .state
                .as_ref()
                .map(StateChannel::test_retained_evidence)
                .or_else(|| {
                    held.retired
                        .as_ref()
                        .map(StateChannel::test_retained_evidence)
                }),
            _ => self
                .state
                .as_ref()
                .map(StateChannel::test_retained_evidence),
        }
    }

    #[cfg(test)]
    pub(in crate::flow) fn test_boot_filing_retired(&self) -> bool {
        self.stopped_boot_filing
            .as_ref()
            .is_some_and(|held| held.parked && held.retired.is_some() && held.state.is_none())
    }
    #[cfg(test)]
    pub(in crate::flow) fn test_boot_filing_progress_current(&self) -> bool {
        let Some(held) = self.stopped_boot_filing.as_ref() else {
            return false;
        };
        let outstanding = held
            .state
            .as_ref()
            .map(StateChannel::outstanding)
            .or_else(|| held.retired.as_ref().map(StateChannel::outstanding))
            .unwrap_or_default();
        outstanding.first().is_none_or(|actual| {
            self.ledger.facts().next().is_some_and(|fact| {
                fact.progress == Some(actual)
                    && fact.state
                        == if actual.bytes_sent == actual.frame_bytes {
                            super::super::ledger::LedgerState::AwaitingReceipt
                        } else {
                            super::super::ledger::LedgerState::Sending
                        }
            })
        })
    }
    pub(super) fn retained_boot_filing(&self, cx: &FlowRecording<'_>) -> bool {
        self.stopped_boot_filing.as_ref().is_some_and(|held| {
            held.parked
                && matches!(
                    self.phase,
                    CoordinatorPhase::Releasing | CoordinatorPhase::Closed
                )
                && self.state.is_none()
                && self.receipt_reservation.is_none()
                && self.pending_record.is_none()
                && cx.budget.gauges() == held.gauges
                && cx.inbox.failure().is_none()
                && cx
                    .inbox
                    .queued()
                    .all(|copy| copy_matches(copy, &held.permit))
                && cx
                    .budget
                    .waiting_copy()
                    .is_none_or(|copy| copy_matches(copy, &held.permit))
                && self
                    .ledger
                    .boot_filing_matches(held.permit.source.as_ref(), &held.permit.expected)
        })
    }

    pub(super) fn releasing_obligations(&self, cx: &FlowRecording<'_>) -> bool {
        self.clean_obligations(cx)
            || (self.retained_boot_filing(cx)
                && cx.barrier.pending().is_none()
                && cx.barrier.pending_author().is_none())
    }

    pub(super) fn retire_stopped_boot_filing_transport(&mut self) {
        if let Some(held) = &mut self.stopped_boot_filing {
            held.retire_transport();
        }
    }

    pub(in crate::flow) fn retire_boot_filing_state(
        &mut self,
        cx: &mut FlowRecording<'_>,
    ) -> Result<weaver_trace::MemberRelease, FlowFailure> {
        self.observe_failures(cx)?;
        if !self.retained_boot_filing(cx) || !self.classifier_retired || self.state_retired {
            return Err(self.fail(
                cx,
                "barrier",
                "boot filing release requires its retained account and retired classifier",
            ));
        }
        let member = self
            .stopped_boot_filing
            .as_mut()
            .unwrap()
            .retire_transport();
        self.state_retired = true;
        Ok(member)
    }

    pub(in crate::flow) fn boot_filing_rollback_matches(
        &self,
        barrier: &RecordBarrier,
        inbox: &FlowInbox,
        budget: &RetentionBudget,
        permit: &BootFilingRollbackPermit,
    ) -> bool {
        let Some(state) = self.state.as_ref() else {
            return false;
        };
        permit.matches_record(barrier)
            && self.phase == CoordinatorPhase::Active
            && self.failure.is_none()
            && self.stopped_boot_filing.is_none()
            && self.stopped_plan_read.is_none()
            && !self.state_retired
            && !self.classifier_retired
            && self.capture.is_none()
            && self.output_checks.is_none()
            && self.output_receipts.is_none()
            && self.instruction_check.is_none()
            && self.repair_measurement.is_none()
            && self.input_account.is_none()
            && self.reuse.is_none()
            && self.last_input_id == 0
            && self.state_operation.is_none()
            && self.operation_completion.is_none()
            && self.check_operation.is_none()
            && self.classifier_release.is_none()
            && self.admission_completion.is_none()
            && self.starting_classifier.is_none()
            && self
                .classifier
                .as_ref()
                .is_some_and(|owner| owner.state() == ClassifierState::Idle)
            && state.failure().is_none()
            && !budget.is_frozen()
            && inbox.failure().is_none()
            && self.refused_author.is_none()
            && self.receipt_reservation.as_ref().is_some_and(|owner| {
                owner.sequence == permit.source.as_ref().map(|source| source.sequence.0)
                    && owner.expected.iter().eq(permit.source.iter())
                    && owner.registered == self.ledger.receipts().is_some()
            })
            && self
                .ledger
                .boot_filing_matches(permit.source.as_ref(), &permit.expected)
            && inbox.queued().all(|copy| copy_matches(copy, permit))
            && budget
                .waiting_copy()
                .is_none_or(|copy| copy_matches(copy, permit))
            && self
                .started_file
                .as_ref()
                .is_none_or(|source| Some(source) == permit.source.as_ref())
            && state.outstanding().iter().all(|exchange| {
                exchange.source.as_ref() == permit.source.as_ref()
                    && permit.source.is_some()
                    && exchange.common.run_id == self.run
                    && exchange.common.session_id == state.identity().0
                    && exchange.operation == weaver_types::state_flow::StateOperation::FilePair
            })
            && self
                .notices
                .iter()
                .all(|notice| notice.input_id.is_none() && notice.turn.is_none())
            && match self.pending_record {
                None => barrier
                    .pending()
                    .is_none_or(|event| event.envelope.kind == Kind::ToolCallCompleted),
                Some(PendingRecord::State) => barrier
                    .pending()
                    .is_some_and(|event| event.envelope.kind == Kind::StateStoreMeasurement),
                Some(PendingRecord::Flow) => barrier
                    .pending()
                    .is_some_and(|event| event.envelope.kind == Kind::StateFlow),
                Some(PendingRecord::StateRelease) => barrier.pending().is_none(),
                _ => false,
            }
            && barrier.boot_filing_record_matches(
                permit,
                state.held_measurement(),
                self.observations.as_ref().and_then(Observation::pending),
            )
    }

    pub(in crate::flow) fn park_boot_filing_rollback(
        &mut self,
        cx: &mut FlowRecording<'_>,
        permit: BootFilingRollbackPermit,
    ) -> Result<(bool, bool), FlowFailure> {
        self.observe_failures(cx)?;
        if !self.boot_filing_rollback_matches(cx.barrier, cx.inbox, cx.budget, &permit) {
            return Err(FlowFailure {
                point: "barrier",
                detail: "expired boot filing retains foreign work".into(),
            });
        }
        let settled = permit.author_done
            && self
                .ledger
                .receipts()
                .is_some_and(super::super::ledger::ReceiptSet::is_complete)
            && self
                .filing_recorded_ns()
                .is_some_and(|at| !permit.bound.expired(at))
            && self.ledger.facts().next().is_none()
            && cx.inbox.queued().next().is_none()
            && cx.budget.waiting_copy().is_none()
            && cx.budget.gauges() == (0, 0)
            && self.notices.is_empty()
            && self.pending_record.is_none()
            && cx.barrier.pending().is_none()
            && self.state().can_begin_filing();
        let unstarted = permit.unstarted
            && self.ledger.facts().next().is_none()
            && self.ledger.receipts().is_none()
            && cx.inbox.queued().next().is_none()
            && cx.budget.waiting_copy().is_none()
            && cx.budget.gauges() == (0, 0)
            && self.notices.is_empty()
            && self.pending_record.is_none()
            && self.state().can_begin_filing();
        let complete = settled || unstarted;
        // Preserve the last actual send/receive facts before freezing the
        // original ledger. Later failure snapshots cannot probe this channel.
        if !complete && let Some(progress) = self.state().outstanding().first() {
            self.ledger
                .observe_progress(progress, self.state().first_sent_ns())
                .map_err(|error| FlowFailure {
                    point: "barrier",
                    detail: error.to_string(),
                })?;
        }
        let record = cx
            .barrier
            .retain_boot_filing_record(&permit)
            .map_err(|detail| FlowFailure {
                point: "barrier",
                detail: detail.into(),
            })?;
        let receipts = if settled {
            Some(
                self.ledger
                    .retain_boot_filing_receipts()
                    .map_err(|error| FlowFailure {
                        point: "barrier",
                        detail: error.to_string(),
                    })?,
            )
        } else {
            None
        };
        if !complete {
            self.ledger.freeze();
            self.phase = CoordinatorPhase::Releasing;
        }
        self.stopped_boot_filing = Some(Box::new(StoppedBootFiling {
            permit,
            state: if complete { None } else { self.state.take() },
            retired: None,
            _reservation: self.receipt_reservation.take().unwrap(),
            _receipts: receipts,
            _pending_record: self.pending_record.take(),
            _record: record,
            gauges: cx.budget.gauges(),
            parked: !complete,
            close_error: None,
        }));
        Ok((!complete, complete))
    }
}
