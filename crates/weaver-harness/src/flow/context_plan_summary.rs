//! Mandatory dormant boot/post-effect summary refresh over the one read owner.
use super::*;
use crate::flow::ledger::{LedgerFile, SourceDisposition};
use crate::flow::plan::{CommittedPlanSummary, EffectExpectation, PlanSummaryUnavailable};
use crate::flow::pump::LifecyclePorts;
use crate::flow::read::ReadFailure;
use std::sync::Arc;
use weaver_types::state_flow::{PlanSummary, StatementEffect};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum PlanSummaryBeginRefusal {
    WrongPhase,
    WrongProof,
    Busy,
    AlreadyStarted,
}
#[derive(Debug)]
pub(in crate::flow) struct PlanSummaryBeginError<T> {
    pub prerequisite: T,
    pub refusal: PlanSummaryBeginRefusal,
    pub failure: Option<RunFailure>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum PlanSummaryProgress {
    Progress,
    Pending,
    Complete,
}

#[derive(Debug)]
enum Prerequisite {
    Boot(RepairedBootPairs),
    Effect(FilingCompletion),
}
#[derive(Debug)]
pub(super) struct PlanSummaryTransaction {
    prerequisite: Prerequisite,
    key: PumpDeadline,
    bound: AbsoluteBound,
    driving: bool,
    started: bool,
    cancelled: bool,
    complete: bool,
    read_settled: bool,
    recorded: Option<Arc<CommittedPlanSummary>>,
    rejected: Option<RecordedRead>,
    refusal: Option<ReadFailure>,
    account_deadline: u64,
    account_settled: bool,
    incomplete: Option<String>,
    stopped_read: Option<Box<read_driver::ReadTransaction>>,
}
impl PlanSummaryTransaction {
    fn effect(&self) -> bool {
        matches!(self.prerequisite, Prerequisite::Effect(_))
    }
}

#[derive(Debug)]
pub(in crate::flow) struct BootPlanSummary {
    repaired: RepairedBootPairs,
    committed: Arc<CommittedPlanSummary>,
}
impl BootPlanSummary {
    pub(in crate::flow) fn repaired(&self) -> &RepairedBootPairs {
        &self.repaired
    }
    pub(in crate::flow) fn read(&self) -> &RecordedRead {
        self.committed.read()
    }
    pub(in crate::flow) fn summary(&self) -> Option<&PlanSummary> {
        self.committed.summary()
    }
}
#[derive(Debug)]
pub(in crate::flow) struct EffectPlanSummary {
    filing: FilingCompletion,
    committed: Arc<CommittedPlanSummary>,
}
impl EffectPlanSummary {
    pub(in crate::flow) fn filing(&self) -> &FilingCompletion {
        &self.filing
    }
    pub(in crate::flow) fn read(&self) -> &RecordedRead {
        self.committed.read()
    }
    pub(in crate::flow) fn summary(&self) -> Option<&PlanSummary> {
        self.committed.summary()
    }
}
#[derive(Debug)]
pub(in crate::flow) struct PlanSummaryAccount {
    work: Box<PlanSummaryTransaction>,
}
impl PlanSummaryAccount {
    pub(in crate::flow) fn recorded_read(&self) -> Option<&RecordedRead> {
        self.work
            .recorded
            .as_ref()
            .map(|record| record.read())
            .or(self.work.rejected.as_ref())
            .or_else(|| {
                self.work
                    .stopped_read
                    .as_ref()
                    .and_then(|read| read.orphan_recorded())
            })
    }
    pub(in crate::flow) fn refusal(&self) -> Option<&ReadFailure> {
        self.work.refusal.as_ref()
    }
    pub(in crate::flow) fn read_evidence(&self) -> Option<&read_driver::ReadTransaction> {
        self.work.stopped_read.as_deref()
    }
    pub(in crate::flow) fn incomplete(&self) -> Option<&str> {
        self.work.incomplete.as_deref()
    }
    pub(in crate::flow) fn repaired(&self) -> Option<&RepairedBootPairs> {
        match &self.work.prerequisite {
            Prerequisite::Boot(proof) => Some(proof),
            _ => None,
        }
    }
    pub(in crate::flow) fn filing(&self) -> Option<&FilingCompletion> {
        match &self.work.prerequisite {
            Prerequisite::Effect(proof) => Some(proof),
            _ => None,
        }
    }
}
#[derive(Debug)]
pub(in crate::flow) enum PlanSummaryCompletion {
    Boot(BootPlanSummary),
    Effect(EffectPlanSummary),
    Refused(PlanSummaryAccount),
    Cancelled(PlanSummaryAccount),
}

/// A rollback-only seal from this owner's immutable read and recorder slots.
/// No caller can construct it or use it to resume a query or authorize success.
pub(in crate::flow) struct PlanReadRollbackPermit {
    expected: Option<(
        weaver_types::state_flow::DecimalU64,
        weaver_types::state_flow::StateControl,
    )>,
    ticket: Option<u64>,
    record_kind: Option<Kind>,
}
impl PlanReadRollbackPermit {
    pub(in crate::flow) fn matches_record(&self, barrier: &RecordBarrier) -> bool {
        barrier.pending_author().map(|held| held.ticket) == self.ticket
            && barrier.pending().map(|event| event.envelope.kind) == self.record_kind
    }
    pub(in crate::flow) fn expected(
        &self,
    ) -> Option<&(
        weaver_types::state_flow::DecimalU64,
        weaver_types::state_flow::StateControl,
    )> {
        self.expected.as_ref()
    }
    pub(in crate::flow) fn measurement_pending(&self) -> bool {
        self.record_kind == Some(Kind::StateStoreMeasurement) && self.ticket.is_none()
    }
}

/// Only a completely released summary result can owe this stopped recall.
pub(in crate::flow) struct OwedPlanRecall {
    event: AuthoredEvent,
}
impl OwedPlanRecall {
    pub(in crate::flow) fn event(&self) -> &AuthoredEvent {
        &self.event
    }
}

impl RecordingContext {
    pub(super) fn plan_summary_blocks_external(&self) -> bool {
        self.plan_summary.as_ref().is_some_and(|work| !work.driving)
            || (self.plan_cache.required.is_some() && self.plan_summary.is_none())
            || (self.plan_summary.is_none()
                && self.phase == RecordingPhase::Entered
                && matches!(
                    self.plan_cache.unavailable,
                    Some(PlanSummaryUnavailable::Revoked | PlanSummaryUnavailable::Failed)
                ))
    }
    pub(super) fn plan_summary_blocks_generation(&self) -> bool {
        self.plan_effect_inflight()
            || self.plan_summary.is_some()
            || self.plan_cache.required.is_some()
            || (self.plan_cache.unavailable.is_some()
                && (self.plan_cache.boot_started
                    || self.plan_cache.unavailable != Some(PlanSummaryUnavailable::Uninitialized)))
    }
    pub(super) fn plan_summary_allows_turn_close(&self, event: &AuthoredEvent) -> bool {
        self.phase == RecordingPhase::Entered
            && self.plan_summary.is_none()
            && self.plan_cache.required.is_none()
            && !self.plan_effect_inflight()
            && self.plan_cache.unavailable == Some(PlanSummaryUnavailable::Revoked)
            && event.kind == Kind::TurnClosed
            && event.turn.is_some()
            && event.turn == self.open_turn
    }
    pub(super) fn plan_summary_read_allowed(&self) -> bool {
        self.plan_summary.as_ref().is_some_and(|work| work.driving)
    }
    pub(super) fn plan_summary_read_bound(&self) -> Option<AbsoluteBound> {
        self.plan_summary.as_ref().map(|work| work.bound)
    }
    pub(super) fn plan_summary_wait_owned(&self, key: PumpDeadline) -> bool {
        (key == PumpDeadline::Leave
            && (self.plan_summary.is_some() || self.plan_cache.required.is_some()))
            || self
                .plan_summary
                .as_ref()
                .is_some_and(|work| work.key == key)
            || self
                .plan_cache
                .required
                .as_ref()
                .is_some_and(|duty| duty.key == key)
    }
    pub(in crate::flow) fn previous_plan_summary(&self) -> Option<&PlanSummary> {
        self.plan_cache
            .committed
            .as_ref()
            .and_then(|cache| cache.summary())
    }
    pub(in crate::flow) fn committed_plan_summary(
        &mut self,
    ) -> Result<Option<&PlanSummary>, PlanSummaryUnavailable> {
        self.observe_plan_effect_receipts();
        if self.failure.is_some() {
            return Err(PlanSummaryUnavailable::Failed);
        }
        if self.plan_effect_inflight()
            || self.plan_summary.is_some()
            || self.plan_cache.required.is_some()
        {
            return Err(PlanSummaryUnavailable::Pending);
        }
        if let Some(reason) = self.plan_cache.unavailable {
            return Err(reason);
        }
        self.plan_cache
            .committed
            .as_ref()
            .map(|cache| cache.summary())
            .ok_or(PlanSummaryUnavailable::Uninitialized)
    }

    /// Called before every live/admission check and before receipt transfer.
    /// Evidence iteration includes cancelled subscriptions and releases no grant.
    pub(super) fn observe_plan_effect_receipts(&mut self) {
        let Some(filing) = self.filing.as_ref() else {
            return;
        };
        let Some(receipts) = self.flow.filing_receipts() else {
            return;
        };
        if self.plan_cache.invalid.is_some() {
            return;
        }
        let mut commits = receipts.recorded_dispositions().filter_map(|disposition| {
            let SourceDisposition::Committed(commit) = disposition else { return None };
            matches!(commit.file(), LedgerFile::Statement(file) if matches!(file.effect, Some(StatementEffect::Plan(_))))
                .then_some(commit)
        });
        let Some(commit) = commits.next() else { return };
        if !filing.cancelled()
            && self.plan_cache.unavailable != Some(PlanSummaryUnavailable::Failed)
        {
            self.plan_cache.unavailable = Some(PlanSummaryUnavailable::Pending);
        }
        if commits.next().is_some() {
            self.plan_cache.invalid = Some("one plan-effect filing committed several plan effects");
            return;
        }
        if self
            .plan_cache
            .required
            .as_ref()
            .is_some_and(|old| Arc::ptr_eq(&old.receipt, filing.receipt_identity()))
        {
            return;
        }
        let Some(start) = self.open_start.as_ref() else {
            self.plan_cache.invalid = Some("committed plan effect lost its start identity");
            return;
        };
        let expected = EffectExpectation::from_commit(
            commit,
            start,
            self.barrier.run(),
            self.author.session(),
            filing.receipt_identity().clone(),
            filing.wait_key(),
            filing.wait_bound(),
            self.plan_cache
                .committed
                .as_ref()
                .and_then(|old| old.summary()),
        );
        match expected {
            Ok(expected) => self.plan_cache.required = Some(expected),
            Err(detail) => self.plan_cache.invalid = Some(detail),
        }
    }

    pub(super) fn observe_plan_summary_lifecycle(&mut self, ports: &LifecyclePorts<'_>) {
        if self.plan_summary.is_none() {
            return;
        }
        let cancelled = ports
            .peer
            .as_deref()
            .and_then(|peer| peer.held())
            .is_some_and(|held| {
                held.envelope.exchange.opener == weaver_types::Opener::Admin
                    && held.envelope.position == weaver_types::Position::Open
                    && matches!(
                        held.envelope.payload,
                        weaver_types::Payload::Directive(
                            weaver_types::LifecycleDirective::Stop { .. }
                                | weaver_types::LifecycleDirective::Leave { .. }
                                | weaver_types::LifecycleDirective::JoinLeave { .. }
                        )
                    )
            });
        if cancelled {
            self.plan_summary.as_mut().unwrap().cancelled = true;
            self.plan_cache.revoke();
            self.cancel_plan_summary_read();
        }
    }

    fn plan_summary_admission_busy(&self) -> bool {
        self.plan_summary.is_some()
            || self.filing.is_some()
            || self.output.is_some()
            || self.read.is_some()
            || self.boot_pairs.is_some()
            || self.marks.is_some()
            || self.repair.is_some()
            || self.pair_work.is_some()
            || self.inspection.is_some()
            || self.freshness.is_some()
            || self.member_capture.is_some()
            || self.measure_blocks_external()
            || self.boot_open_blocks_external()
            || self.input.snapshot().is_some()
            || self.input_author.is_some()
            || self.input_turn.is_some()
            || self.waiting_content.is_some()
            || self.completed_author.is_some()
            || self.barrier.pending().is_some()
            || self.barrier.pending_author().is_some()
            || self.flow.has_pending_record()
            || self.flow.has_filing_consumer()
            || self.flow.has_output_consumer()
            || self.flow.pending_control().is_some()
            || self.flow.pending_check().is_some()
            || self.flow.operation_completion().is_some()
            || !self.flow.state().outstanding().is_empty()
    }

    #[allow(clippy::result_large_err)]
    pub(in crate::flow) fn begin_boot_plan_summary(
        &mut self,
        repaired: RepairedBootPairs,
        io: &mut impl PumpIo,
    ) -> Result<(), PlanSummaryBeginError<RepairedBootPairs>> {
        if let Err(failure) = self.live(io) {
            return Err(PlanSummaryBeginError {
                prerequisite: repaired,
                refusal: PlanSummaryBeginRefusal::WrongPhase,
                failure: Some(failure),
            });
        }
        let refusal = if self.phase != RecordingPhase::Entered
            || !self.barrier.is_active()
            || self.flow.phase() != CoordinatorPhase::Active
        {
            Some(PlanSummaryBeginRefusal::WrongPhase)
        } else if self.plan_cache.boot_started {
            Some(PlanSummaryBeginRefusal::AlreadyStarted)
        } else if !repaired.belongs_to(self)
            || repaired.checked_open().is_none()
            || repaired.checked_open().is_some_and(|proof| {
                proof.run() != self.barrier.run()
                    || self.boot_journal.is_none()
                    || self.boot_enter_cancelled
                    || self
                        .boot_head
                        .as_ref()
                        .is_none_or(|head| head.recall_sequence() != proof.head())
                    || self.deadlines.get(PumpDeadline::Enter) != Some(proof.enter())
            })
        {
            Some(PlanSummaryBeginRefusal::WrongProof)
        } else if self.plan_summary_admission_busy() || self.plan_cache.required.is_some() {
            Some(PlanSummaryBeginRefusal::Busy)
        } else {
            None
        };
        if let Some(refusal) = refusal {
            return Err(PlanSummaryBeginError {
                prerequisite: repaired,
                refusal,
                failure: None,
            });
        }
        let bound = repaired.checked_open().unwrap().enter();
        self.plan_cache.boot_started = true;
        self.plan_cache.unavailable = Some(PlanSummaryUnavailable::Pending);
        self.plan_summary = Some(PlanSummaryTransaction {
            prerequisite: Prerequisite::Boot(repaired),
            key: PumpDeadline::Enter,
            bound,
            driving: false,
            started: false,
            cancelled: false,
            complete: false,
            read_settled: false,
            recorded: None,
            rejected: None,
            refusal: None,
            account_deadline: bound.deadline_ns(),
            account_settled: false,
            incomplete: None,
            stopped_read: None,
        });
        Ok(())
    }

    #[allow(clippy::result_large_err)]
    pub(in crate::flow) fn begin_effect_plan_summary(
        &mut self,
        filing: FilingCompletion,
        io: &mut impl PumpIo,
    ) -> Result<(), PlanSummaryBeginError<FilingCompletion>> {
        if let Err(failure) = self.live(io) {
            return Err(PlanSummaryBeginError {
                prerequisite: filing,
                refusal: PlanSummaryBeginRefusal::WrongPhase,
                failure: Some(failure),
            });
        }
        let valid = filing.belongs_to(self) && !filing.receipts().is_cancelled() && filing.receipts().is_complete()
            && self.plan_cache.committed.is_some() && self.plan_cache.boot_started
            && self.plan_cache.required.as_ref().is_some_and(|expected|
                Arc::ptr_eq(&expected.receipt, filing.receipt_identity())
                && filing.wait_key() == expected.key && filing.wait_bound() == expected.bound
                && self.deadlines.get(expected.key) == Some(expected.bound)
                && filing.receipts().dispositions().any(|disposition| matches!(disposition,
                    SourceDisposition::Committed(commit) if commit.source() == &expected.source.block_id)));
        let refusal = if self.phase != RecordingPhase::Entered {
            Some(PlanSummaryBeginRefusal::WrongPhase)
        } else if !valid {
            Some(PlanSummaryBeginRefusal::WrongProof)
        } else if self.plan_summary_admission_busy() {
            Some(PlanSummaryBeginRefusal::Busy)
        } else {
            None
        };
        if let Some(refusal) = refusal {
            return Err(PlanSummaryBeginError {
                prerequisite: filing,
                refusal,
                failure: None,
            });
        }
        let key = filing.wait_key();
        let bound = filing.wait_bound();
        self.plan_summary = Some(PlanSummaryTransaction {
            prerequisite: Prerequisite::Effect(filing),
            key,
            bound,
            driving: false,
            started: false,
            cancelled: false,
            complete: false,
            read_settled: false,
            recorded: None,
            rejected: None,
            refusal: None,
            account_deadline: bound.deadline_ns(),
            account_settled: false,
            incomplete: None,
            stopped_read: None,
        });
        Ok(())
    }

    /// The read calls this immediately on actual recall Recorded, before its
    /// next clock/intake hook. The full proof is retained even on mismatch.
    #[allow(clippy::result_large_err)] // A failed binding returns the exact owned read.
    pub(super) fn bind_plan_summary_recorded(
        &mut self,
        read: RecordedRead,
    ) -> Result<(), (RecordedRead, &'static str)> {
        let Some(work) = self.plan_summary.as_mut() else {
            return Err((read, "summary Recorded has no owner"));
        };
        if work.recorded.is_some() || work.rejected.is_some() {
            return Err((read, "summary Recorded twice"));
        }
        if !Arc::ptr_eq(read.origin(), &self.read_origin) {
            return Err((read, "summary Recorded belongs to another context"));
        }
        let expected = if work.effect() {
            let Some(expected) = self.plan_cache.required.as_ref() else {
                return Err((
                    read,
                    "summary effect lost its independently held expectation",
                ));
            };
            Some(expected)
        } else {
            None
        };
        match CommittedPlanSummary::from_recorded(read, expected) {
            Ok(recorded) => {
                work.recorded = Some(Arc::new(recorded));
                Ok(())
            }
            Err((read, detail)) => Err((read, detail)),
        }
    }

    pub(in crate::flow) fn advance_plan_summary<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<PlanSummaryProgress, RunFailure> {
        if self.plan_summary.is_none() {
            return Err(RunFailure::PlanSummaryOwned);
        }
        self.observe_plan_summary_lifecycle(&ports.lifecycle);
        if let Err(failure) = self.live(ports.io) {
            return Err(self.plan_summary_failure(ports.io, failure));
        }
        let work = self.plan_summary.as_ref().unwrap();
        if work.complete {
            return Ok(PlanSummaryProgress::Complete);
        }
        if self.deadlines.get(work.key) != Some(work.bound) {
            return Err(self.plan_summary_fault(ports.io, "summary lost its inherited wait"));
        }
        if work.cancelled && !work.started {
            self.plan_summary.as_mut().unwrap().complete = true;
            self.plan_summary.as_mut().unwrap().account_settled = true;
            return Ok(PlanSummaryProgress::Complete);
        }
        if work.read_settled {
            return self.finish_plan_summary_owner(ports.io);
        }
        if !work.started {
            self.plan_summary.as_mut().unwrap().driving = true;
            let result = self.begin_plan_summary_read(ports.io);
            self.plan_summary.as_mut().unwrap().driving = false;
            match result {
                Ok(()) => {
                    self.plan_summary.as_mut().unwrap().started = true;
                    return Ok(PlanSummaryProgress::Progress);
                }
                Err(error) => {
                    // The read may have accepted ownership before a later live
                    // hook yielded. Never begin a replacement read on retry.
                    if self.read.is_some() {
                        self.plan_summary.as_mut().unwrap().started = true;
                    }
                    if let Some(failure) = error.failure {
                        return Err(self.plan_summary_failure(ports.io, failure));
                    }
                    return Ok(PlanSummaryProgress::Pending);
                }
            }
        }
        self.plan_summary.as_mut().unwrap().driving = true;
        let result = self.advance_read(ports);
        self.plan_summary.as_mut().unwrap().driving = false;
        let progress = result.map_err(|failure| self.plan_summary_failure(ports.io, failure))?;
        if progress != ReadProgress::Complete {
            return Ok(PlanSummaryProgress::Pending);
        }
        // The successful proof is already retained by the Recorded hook. A
        // typed refusal remains exact evidence, never a committed-none cache.
        let refusal = self.finish_plan_summary_read()?;
        self.plan_summary.as_mut().unwrap().read_settled = true;
        if let Some(refusal) = refusal {
            self.plan_summary.as_mut().unwrap().refusal = Some(refusal);
            self.plan_cache.revoke();
            if self.plan_summary.as_ref().unwrap().effect() {
                return Err(
                    self.plan_summary_fault(ports.io, "required post-effect summary read refused")
                );
            }
        }
        self.finish_plan_summary_owner(ports.io)
    }

    fn finish_plan_summary_owner(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<PlanSummaryProgress, RunFailure> {
        if let Err(failure) = self.live(io) {
            return Err(self.plan_summary_failure(io, failure));
        }
        let work = self.plan_summary.as_mut().unwrap();
        if work.cancelled {
            self.plan_cache.revoke();
        } else if work.refusal.is_none() {
            let recorded = work
                .recorded
                .as_ref()
                .expect("successful summary retains actual Recorded proof");
            self.plan_cache.committed = Some(recorded.clone());
            self.plan_cache.required = None;
            self.plan_cache.unavailable = None;
        }
        work.complete = true;
        work.account_settled = true;
        Ok(PlanSummaryProgress::Complete)
    }

    pub(in crate::flow) fn cancel_plan_summary(
        &mut self,
        _io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        let Some(work) = self.plan_summary.as_mut() else {
            return Err(RunFailure::PlanSummaryOwned);
        };
        work.cancelled = true;
        self.plan_cache.revoke();
        if self.read.is_some() {
            self.cancel_plan_summary_read();
        }
        Ok(())
    }
    pub(in crate::flow) fn take_plan_summary(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<Option<PlanSummaryCompletion>, RunFailure> {
        self.live(io)?;
        if self.plan_summary.as_ref().is_none_or(|work| !work.complete) {
            return Ok(None);
        }
        if self
            .plan_summary
            .as_ref()
            .is_some_and(|work| !work.effect() && (work.cancelled || work.refusal.is_some()))
        {
            // Failed Enter rollback retains its seal until the lifecycle has
            // moved this owner to Releasing; ordinary extraction cannot strand it.
            return Err(RunFailure::PlanSummaryOwned);
        }
        let work = self.plan_summary.take().unwrap();
        if work.cancelled {
            self.plan_cache.required = None;
            return Ok(Some(PlanSummaryCompletion::Cancelled(PlanSummaryAccount {
                work: Box::new(work),
            })));
        }
        if work.refusal.is_some() {
            return Ok(Some(PlanSummaryCompletion::Refused(PlanSummaryAccount {
                work: Box::new(work),
            })));
        }
        let committed = work.recorded.expect("complete successful summary proof");
        Ok(Some(match work.prerequisite {
            Prerequisite::Boot(repaired) => PlanSummaryCompletion::Boot(BootPlanSummary {
                repaired,
                committed,
            }),
            Prerequisite::Effect(filing) => {
                PlanSummaryCompletion::Effect(EffectPlanSummary { filing, committed })
            }
        }))
    }
    /// A failed Enter retains its summary proof/account until rollback owns it.
    /// An unfinished live read must drain; only expiry permits incomplete disposal.
    pub(in crate::flow) fn begin_boot_plan_summary_release(
        &mut self,
        leave: AbsoluteBound,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        let Some(work) = self.plan_summary.as_mut() else {
            return Err(RunFailure::PlanSummaryOwned);
        };
        if work.effect() || self.phase != RecordingPhase::Entered {
            return Err(RunFailure::PlanSummaryOwned);
        }
        work.cancelled = true;
        self.plan_cache.revoke();
        self.cancel_plan_summary_read();
        if let Some(failure) = &self.failure {
            return Err(failure.clone());
        }
        let now = io.now().map_err(|error| {
            self.plan_summary_fault(io, format!("summary rollback clock: {error}"))
        })?;
        if now < self.last_ns {
            return Err(self.plan_summary_fault(io, "summary rollback clock reversed"));
        }
        self.last_ns = now;
        if let Some((key, bound)) = self
            .deadlines
            .expired(now)
            .min_by_key(|(_, bound)| bound.deadline_ns())
            && !matches!(key, PumpDeadline::Enter | PumpDeadline::Leave)
        {
            // A strictly earlier read/control timeout is still its existing
            // foundation failure. Rollback cannot reclassify it as expired Enter.
            return Err(self.expired(io, key, bound));
        }
        self.freeze_plan_summary_account_bound();
        let work = self.plan_summary.as_ref().unwrap();
        if !work.account_settled && now < work.account_deadline {
            return Err(RunFailure::PlanSummaryOwned);
        }
        if self.filing.is_some()
            || self.output.is_some()
            || self.pair_work.is_some()
            || self.repair.is_some()
            || self.boot_pairs.is_some()
            || self.marks.is_some()
            || self.member_capture.is_some()
            || self.freshness.is_some()
            || self.inspection.is_some()
            || self.measure_blocks_external()
            || self.boot_open_blocks_external()
            || self.open_turn.is_some()
            || self.input.snapshot().is_some()
            || self.input_turn.is_some()
            || self.input_author.is_some()
            || self.waiting_content.is_some()
            || self.completed_author.is_some()
        {
            return Err(RunFailure::PlanSummaryOwned);
        }
        let permit = self.plan_read_rollback_permit()?;
        if let Some(held) = self.deadlines.get(PumpDeadline::Leave) {
            if held != leave {
                return Err(RunFailure::PlanSummaryOwned);
            }
        } else {
            self.deadlines
                .arm(PumpDeadline::Leave, leave)
                .map_err(|error| {
                    self.plan_summary_fault(io, format!("summary rollback Leave: {error}"))
                })?;
        }
        self.flow
            .begin_plan_read_rollback(
                &mut FlowRecording {
                    barrier: &mut self.barrier,
                    inbox: &mut self.inbox,
                    budget: &mut self.budget,
                    author: &self.author,
                },
                &permit,
            )
            .map_err(|failure| self.flow_failed(io, failure))?;
        self.phase = RecordingPhase::Releasing;
        self.freeze_plan_summary_account_bound();
        let incomplete = (!self.plan_summary.as_ref().unwrap().account_settled)
            .then_some("summary read reached its original bound before settlement");
        self.settle_plan_summary_account(incomplete);
        if let Some(detail) = self.flow.stopped_plan_read_close_error() {
            self.plan_summary
                .as_mut()
                .unwrap()
                .incomplete
                .get_or_insert_with(|| format!("summary channel close: {detail}"));
        }
        // The original bounds remain in the sealed account. Only disposal has
        // finished; no read or success can obtain time from the Leave window.
        self.deadlines.disarm(PumpDeadline::StateControl);
        self.deadlines.disarm(PumpDeadline::Enter);
        Ok(())
    }

    fn plan_read_rollback_permit(&self) -> Result<PlanReadRollbackPermit, RunFailure> {
        let read = self.read.as_ref();
        if read.is_some_and(|read| !read.is_summary()) {
            return Err(RunFailure::PlanSummaryOwned);
        }
        let ticket = self.barrier.pending_author().map(|held| held.ticket);
        let record_kind = self.barrier.pending().map(|event| event.envelope.kind);
        match record_kind {
            Some(Kind::Recall)
                if read.is_some_and(|read| {
                    read.plan_ticket().is_some() && read.plan_ticket() == ticket
                }) => {}
            Some(Kind::StateStoreMeasurement)
                if ticket.is_none() && read.is_some_and(|read| read.plan_expected().is_some()) => {}
            None if ticket.is_none() => {}
            _ => return Err(RunFailure::PlanSummaryOwned),
        }
        Ok(PlanReadRollbackPermit {
            expected: read.and_then(|read| read.plan_expected()).cloned(),
            ticket,
            record_kind,
        })
    }

    pub(super) fn plan_owns_pending_recall(&self) -> bool {
        self.plan_summary.is_some()
            && self.read.as_ref().is_some_and(|read| {
                read.is_summary()
                    && read.plan_ticket().is_some()
                    && read.plan_ticket() == self.barrier.pending_author().map(|held| held.ticket)
            })
    }
    pub(super) fn freeze_plan_summary_account_bound(&mut self) {
        let read = self
            .read
            .as_ref()
            .map(|read| read.plan_bound().deadline_ns());
        let leave = self
            .deadlines
            .get(PumpDeadline::Leave)
            .map(AbsoluteBound::deadline_ns);
        let halt = self.flow.halt().and_then(|halt| halt.deadline().ok());
        if let Some(work) = &mut self.plan_summary {
            for bound in read.into_iter().chain(leave).chain(halt) {
                work.account_deadline = work.account_deadline.min(bound);
            }
        }
    }
    fn plan_summary_account_clock(&mut self, io: &mut impl PumpIo) -> bool {
        self.freeze_plan_summary_account_bound();
        let bound = self.plan_summary.as_ref().unwrap().account_deadline;
        match io.now() {
            Ok(now) if now >= self.last_ns && now < bound => {
                self.last_ns = now;
                true
            }
            _ => false,
        }
    }
    fn settle_plan_summary_account(&mut self, incomplete: Option<&str>) {
        let read = self.read.take();
        let work = self.plan_summary.as_mut().unwrap();
        work.cancelled = true;
        work.account_settled = true;
        if let Some(read) = read {
            if let Some(refusal) = read.plan_refusal() {
                work.refusal = Some(refusal.clone());
            }
            work.stopped_read = Some(Box::new(read));
        }
        if let Some(detail) = incomplete {
            work.incomplete.get_or_insert_with(|| detail.into());
        }
        self.plan_cache.revoke();
    }
    fn abandon_plan_summary_account(&mut self, detail: &str) {
        if self.plan_owns_pending_recall() {
            let ticket = self.read.as_ref().unwrap().plan_ticket().unwrap();
            let _ = self.barrier.abandon_boot_author(ticket);
        }
        self.settle_plan_summary_account(Some(detail));
        if let Some(halt) = self.flow.halt_mut() {
            let _ = halt.append_secondary(SecondaryFailure {
                reason: FlowHaltReason::CarryFailed,
                point: HaltPoint::Teardown,
                detail: detail.into(),
                observed_ns: Some(self.last_ns.into()),
            });
        }
    }

    /// Bounded trace-only continuation after first-failure reporting. No new
    /// query, Release, filing, cache success or ordinary live call is admitted.
    pub(in crate::flow) fn advance_plan_summary_account(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<PlanSummaryProgress, RunFailure> {
        if self.phase != RecordingPhase::Failed || self.plan_summary.is_none() {
            return Err(RunFailure::PlanSummaryOwned);
        }
        if self.plan_summary.as_ref().unwrap().account_settled {
            return Ok(PlanSummaryProgress::Complete);
        }
        self.plan_summary.as_mut().unwrap().cancelled = true;
        self.plan_cache.fail();
        if !self.plan_summary_account_clock(io) {
            self.abandon_plan_summary_account(
                "summary account reached its original bound or lost the common clock",
            );
            return Ok(PlanSummaryProgress::Complete);
        }
        if !self
            .halt_report
            .as_ref()
            .is_some_and(|report| report.attempted())
        {
            return Ok(PlanSummaryProgress::Pending);
        }
        let Some(read) = self.read.as_ref() else {
            self.settle_plan_summary_account(None);
            return Ok(PlanSummaryProgress::Complete);
        };
        if !read.is_summary() {
            return Err(RunFailure::PlanSummaryOwned);
        }
        if read.plan_recorded().is_some() || read.plan_refusal().is_some() {
            self.settle_plan_summary_account(None);
            return Ok(PlanSummaryProgress::Complete);
        }
        let Some(event) = read.plan_recall().cloned() else {
            // The foundation already closed the failed read channel. Keep its
            // exact partial result/request/answer, never guess a cleanup ID.
            self.abandon_plan_summary_account(
                "summary read failed before complete released provenance",
            );
            return Ok(PlanSummaryProgress::Complete);
        };
        if let Some(ticket) = read.plan_ticket() {
            if self.barrier.pending_author().map(|held| held.ticket) != Some(ticket) {
                self.abandon_plan_summary_account("summary recall lost its exact stopped ticket");
                return Ok(PlanSummaryProgress::Complete);
            }
        } else {
            if self.barrier.pending().is_some()
                || self.barrier.pending_author().is_some()
                || self.flow.has_pending_record()
            {
                return Ok(PlanSummaryProgress::Pending);
            }
            let wall = match wall_ms() {
                Ok(wall) => wall,
                Err(detail) => {
                    self.abandon_plan_summary_account(&detail);
                    return Ok(PlanSummaryProgress::Complete);
                }
            };
            if let Err(detail) = self.barrier.queue_plan_recall_account(
                &OwedPlanRecall { event },
                &self.author,
                self.last_ns,
                wall,
            ) {
                self.abandon_plan_summary_account(&detail);
                return Ok(PlanSummaryProgress::Complete);
            }
            self.retain_read_author_ticket();
        }
        match self.barrier.resume_trace_only() {
            Ok(Some(sequence)) => {
                if let Err(detail) = self.retain_read_recorded(sequence) {
                    self.abandon_plan_summary_account(detail);
                } else {
                    self.settle_plan_summary_account(None);
                }
            }
            Ok(None) => {}
            Err(error) => {
                self.abandon_plan_summary_account(&format!("summary stopped recall: {error:?}"))
            }
        }
        if !self.plan_summary.as_ref().unwrap().account_settled
            && !self.plan_summary_account_clock(io)
        {
            self.abandon_plan_summary_account(
                "summary account reached its original bound after progress",
            );
        }
        Ok(if self.plan_summary.as_ref().unwrap().account_settled {
            PlanSummaryProgress::Complete
        } else {
            PlanSummaryProgress::Pending
        })
    }

    pub(in crate::flow) fn take_stopped_plan_summary(
        &mut self,
    ) -> Result<Option<PlanSummaryAccount>, RunFailure> {
        if !matches!(
            self.phase,
            RecordingPhase::Failed | RecordingPhase::Releasing
        ) {
            return Err(RunFailure::PlanSummaryOwned);
        }
        if self
            .plan_summary
            .as_ref()
            .is_none_or(|work| !work.account_settled)
        {
            return Ok(None);
        }
        self.plan_cache.required = None;
        Ok(Some(PlanSummaryAccount {
            work: Box::new(self.plan_summary.take().unwrap()),
        }))
    }

    fn plan_summary_failure(&mut self, io: &mut impl PumpIo, failure: RunFailure) -> RunFailure {
        self.plan_cache.revoke();
        if self
            .plan_summary
            .as_ref()
            .is_some_and(PlanSummaryTransaction::effect)
            && self.failure.is_none()
        {
            self.plan_summary_fault(
                io,
                format!("required post-effect summary failed: {failure:?}"),
            )
        } else {
            failure
        }
    }
    fn plan_summary_fault(
        &mut self,
        io: &mut impl PumpIo,
        detail: impl Into<String>,
    ) -> RunFailure {
        self.plan_cache.fail();
        self.fail(
            io,
            FlowHaltReason::ProtocolFailed,
            HaltPoint::StateReply,
            detail,
        )
    }
}
