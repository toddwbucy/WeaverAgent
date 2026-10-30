//! Mandatory dormant boot/post-effect summary refresh over the one read owner.
use super::*;
use crate::flow::ledger::{LedgerFile, SourceDisposition};
use crate::flow::plan::{CommittedPlanSummary, EffectExpectation, PlanSummaryUnavailable};
use crate::flow::pump::LifecyclePorts;
use crate::flow::read::ReadFailure;
use std::sync::Arc;
use weaver_types::state_flow::{PlanSummary, StatementEffect};

#[path = "context_plan_summary_expired.rs"]
mod expired;

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
pub(super) enum Prerequisite {
    Boot(RepairedBootPairs),
    Effect(FilingCompletion),
}
#[derive(Debug)]
pub(super) struct PlanSummaryTransaction<P = Prerequisite, R = Option<Arc<CommittedPlanSummary>>> {
    prerequisite: P,
    recorded: R,
    rollback: PlanSummaryRollback,
}

/// The complete original rollback owner moves together when evidence becomes
/// statically known. No field is reconstructed from successful read evidence.
#[derive(Debug)]
struct PlanSummaryRollback {
    key: PumpDeadline,
    bound: AbsoluteBound,
    driving: bool,
    started: bool,
    cancelled: bool,
    complete: bool,
    read_settled: bool,
    rejected: Option<RecordedRead>,
    refusal: Option<ReadFailure>,
    account_deadline: u64,
    account_settled: bool,
    incomplete: Option<String>,
    stopped_read: Option<Box<read_driver::ReadTransaction>>,
}
impl PlanSummaryTransaction {
    fn new(prerequisite: Prerequisite, key: PumpDeadline, bound: AbsoluteBound) -> Self {
        Self {
            prerequisite,
            recorded: None,
            rollback: PlanSummaryRollback {
                key,
                bound,
                driving: false,
                started: false,
                cancelled: false,
                complete: false,
                read_settled: false,
                rejected: None,
                refusal: None,
                account_deadline: bound.deadline_ns(),
                account_settled: false,
                incomplete: None,
                stopped_read: None,
            },
        }
    }

    fn effect(&self) -> bool {
        matches!(self.prerequisite, Prerequisite::Effect(_))
    }
}

impl<P, R> PlanSummaryTransaction<P, R> {
    fn split_evidence(self) -> (P, R, PlanSummaryRollback) {
        let Self {
            prerequisite,
            recorded,
            rollback,
        } = self;
        (prerequisite, recorded, rollback)
    }
    #[cfg(test)]
    pub(super) fn disposition_for_test(&self) -> PlanSummaryDisposition {
        PlanSummaryDisposition {
            complete: self.rollback.complete,
            cancelled: self.rollback.cancelled,
            key: self.rollback.key,
            bound: self.rollback.bound,
            account_deadline: self.rollback.account_deadline,
        }
    }
}
impl PlanSummaryRollback {
    fn with_evidence<P, R>(self, prerequisite: P, recorded: R) -> PlanSummaryTransaction<P, R> {
        PlanSummaryTransaction {
            prerequisite,
            recorded,
            rollback: self,
        }
    }
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PlanSummaryDisposition {
    pub complete: bool,
    pub cancelled: bool,
    pub key: PumpDeadline,
    pub bound: AbsoluteBound,
    pub account_deadline: u64,
}

#[derive(Debug)]
pub(in crate::flow) struct BootPlanSummary {
    // Successful Boot carries statically required repair/read evidence beside
    // every unchanged rollback field. The nested repair Box and read Arc retain
    // their original allocations through this safe exhaustive move.
    work: Box<PlanSummaryTransaction<RepairedBootPairs, Arc<CommittedPlanSummary>>>,
}
impl BootPlanSummary {
    pub(in crate::flow) fn repaired(&self) -> &RepairedBootPairs {
        &self.work.prerequisite
    }
    pub(in crate::flow) fn read(&self) -> &RecordedRead {
        self.work.recorded.read()
    }
    pub(in crate::flow) fn summary(&self) -> Option<&PlanSummary> {
        self.work.recorded.summary()
    }
    #[cfg(test)]
    pub(super) fn disposition_for_test(&self) -> PlanSummaryDisposition {
        self.work.disposition_for_test()
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
    #[cfg(test)]
    pub(super) fn disposition_for_test(&self) -> PlanSummaryDisposition {
        self.work.disposition_for_test()
    }

    pub(in crate::flow) fn recorded_read(&self) -> Option<&RecordedRead> {
        self.work
            .recorded
            .as_ref()
            .map(|record| record.read())
            .or(self.work.rollback.rejected.as_ref())
            .or_else(|| {
                self.work
                    .rollback
                    .stopped_read
                    .as_ref()
                    .and_then(|read| read.orphan_recorded())
            })
    }
    pub(in crate::flow) fn refusal(&self) -> Option<&ReadFailure> {
        self.work.rollback.refusal.as_ref()
    }
    pub(in crate::flow) fn read_evidence(&self) -> Option<&read_driver::ReadTransaction> {
        self.work.rollback.stopped_read.as_deref()
    }
    pub(in crate::flow) fn incomplete(&self) -> Option<&str> {
        self.work.rollback.incomplete.as_deref()
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
    pub(super) fn boot_summary_proof_extracted(&self) -> bool {
        self.plan_cache.boot_started
            && self.plan_cache.committed.is_some()
            && self.plan_summary.is_none()
    }

    pub(super) fn plan_summary_is_boot_cleanup(&self) -> bool {
        self.plan_summary.as_ref().is_none_or(|work| !work.effect())
    }

    pub(super) fn plan_summary_blocks_external(&self) -> bool {
        self.plan_summary
            .as_ref()
            .is_some_and(|work| !work.rollback.driving)
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
        self.plan_summary
            .as_ref()
            .is_some_and(|work| work.rollback.driving)
    }
    pub(super) fn plan_summary_read_bound(&self) -> Option<AbsoluteBound> {
        self.plan_summary.as_ref().map(|work| work.rollback.bound)
    }
    pub(super) fn plan_summary_owns_read_wait(&self, key: PumpDeadline) -> bool {
        self.plan_summary.as_ref().is_some_and(|work| {
            work.rollback.driving
                && work.rollback.key == key
                && self.deadlines.get(key) == Some(work.rollback.bound)
        })
    }
    pub(super) fn plan_summary_wait_owned(&self, key: PumpDeadline) -> bool {
        (key == PumpDeadline::Leave
            && (self.plan_summary.is_some() || self.plan_cache.required.is_some()))
            || self
                .plan_summary
                .as_ref()
                .is_some_and(|work| work.rollback.key == key)
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
            || self.root_plan_work().is_some()
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
        // The author admission guard forbids an effect outside this flagged
        // owner. Ordinary filings cannot create a plan-summary obligation.
        if !self.plan_effect_inflight() {
            return;
        }
        let filing = self.filing.as_ref().expect("flagged filing exists");
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
        )
        .and_then(|mut expected| {
            if let Some(work) = self.root_plan_work() {
                let effect = work
                    .effect()
                    .ok_or("root receipt lost its private effect")?;
                expected.digest = Some(effect.digest_for_commit(self, commit)?.clone());
            }
            Ok(expected)
        });
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
            // Stop revokes future use even after a refresh completed. Its exact
            // already-completed disposition remains success evidence; it is not
            // retroactively changed into an unfinished/cancelled transaction.
            self.plan_cache.revoke();
            if !self.plan_summary.as_ref().unwrap().rollback.complete {
                self.plan_summary.as_mut().unwrap().rollback.cancelled = true;
                self.cancel_plan_summary_read();
            }
        }
    }

    fn plan_summary_owner_busy(&self, key: PumpDeadline, drains_owed_effect: bool) -> bool {
        // An effect refresh retains its own Decision or Cut. A separate Cut
        // may also bound an already-owed effect while its reload drains. Other
        // waits and owners still exclude admission; boot requires no open turn.
        (self.place_blocks_external() && !drains_owed_effect)
            || self.plan_summary.is_some()
            || (key == PumpDeadline::Enter && self.open_turn.is_some())
            || self.filing.is_some()
            || self.output.is_some()
            || self.read.is_some()
            || self.boot_pairs.is_some()
            || (self.marks.is_some()
                && !self.root_plan_summary_allowed(key)
                && !self.plan_port_summary_allowed(key))
            || self.repair.is_some()
            || self.pair_work.is_some()
            || self.inspection.is_some()
            || self.freshness.is_some()
            || self.member_capture.is_some()
            || self.measure.is_some()
            || self.boot_open_blocks_external()
            || self.templates_block_content()
            || self.caller_author_held()
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
            || self.flow.standing_state().is_some_and(|state| {
                !state.outstanding().is_empty() || state.held_measurement().is_some()
            })
    }

    fn plan_summary_admission_busy(&self, key: PumpDeadline) -> bool {
        let drains_owed_effect = matches!(key, PumpDeadline::Decision | PumpDeadline::Cut)
            && self.reload_drain_bound().is_some();
        self.flow.standing_state().is_none()
            || self.plan_summary_owner_busy(key, drains_owed_effect)
            || [
                PumpDeadline::Decision,
                PumpDeadline::Cut,
                PumpDeadline::Gate,
                PumpDeadline::StateControl,
            ]
            .into_iter()
            .any(|other| {
                other != key
                    && !(other == PumpDeadline::Cut && drains_owed_effect)
                    && self.deadlines.get(other).is_some()
            })
    }

    pub(in crate::flow) fn boot_proof_current(&self, repaired: &RepairedBootPairs) -> bool {
        repaired.belongs_to(self)
            && repaired.checked_open().is_some_and(|open| {
                Arc::ptr_eq(open.origin(), &self.read_origin)
                    && open.run() == self.barrier.run()
                    && self.boot_journal.is_some()
                    && self.boot_head.as_ref().is_some_and(|head| {
                        head.recall_sequence() == open.head() && head.enter_bound() == open.enter()
                    })
            })
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
        } else if !self.boot_proof_current(&repaired)
            || self.boot_enter_cancelled
            || repaired
                .checked_open()
                .is_some_and(|open| self.deadlines.get(PumpDeadline::Enter) != Some(open.enter()))
        {
            Some(PlanSummaryBeginRefusal::WrongProof)
        } else if self.plan_summary_admission_busy(PumpDeadline::Enter)
            || self.plan_cache.required.is_some()
        {
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
        self.plan_summary = Some(PlanSummaryTransaction::new(
            Prerequisite::Boot(repaired),
            PumpDeadline::Enter,
            bound,
        ));
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
        } else if self.plan_summary_admission_busy(filing.wait_key()) {
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
        self.plan_summary = Some(PlanSummaryTransaction::new(
            Prerequisite::Effect(filing),
            key,
            bound,
        ));
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
        if work.recorded.is_some() || work.rollback.rejected.is_some() {
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
        if work.rollback.complete {
            return Ok(PlanSummaryProgress::Complete);
        }
        if self.deadlines.get(work.rollback.key) != Some(work.rollback.bound) {
            return Err(self.plan_summary_fault(ports.io, "summary lost its inherited wait"));
        }
        if work.rollback.cancelled && !work.rollback.started {
            self.plan_summary.as_mut().unwrap().rollback.complete = true;
            self.plan_summary.as_mut().unwrap().rollback.account_settled = true;
            return Ok(PlanSummaryProgress::Complete);
        }
        if work.rollback.read_settled {
            return self.finish_plan_summary_owner(ports.io);
        }
        if !work.rollback.started {
            self.plan_summary.as_mut().unwrap().rollback.driving = true;
            let result = self.begin_plan_summary_read(ports.io);
            self.plan_summary.as_mut().unwrap().rollback.driving = false;
            match result {
                Ok(()) => {
                    self.plan_summary.as_mut().unwrap().rollback.started = true;
                    return Ok(PlanSummaryProgress::Progress);
                }
                Err(error) => {
                    // The read may have accepted ownership before a later live
                    // hook yielded. Never begin a replacement read on retry.
                    if self.read.is_some() {
                        self.plan_summary.as_mut().unwrap().rollback.started = true;
                    }
                    if let Some(failure) = error.failure {
                        return Err(self.plan_summary_failure(ports.io, failure));
                    }
                    return Ok(PlanSummaryProgress::Pending);
                }
            }
        }
        self.plan_summary.as_mut().unwrap().rollback.driving = true;
        let result = self.advance_read(ports);
        self.plan_summary.as_mut().unwrap().rollback.driving = false;
        let progress = result.map_err(|failure| self.plan_summary_failure(ports.io, failure))?;
        if progress != ReadProgress::Complete {
            return Ok(if progress == ReadProgress::Progress {
                PlanSummaryProgress::Progress
            } else {
                PlanSummaryProgress::Pending
            });
        }
        // The successful proof is already retained by the Recorded hook. A
        // typed refusal remains exact evidence, never a committed-none cache.
        let refusal = self.finish_plan_summary_read()?;
        self.plan_summary.as_mut().unwrap().rollback.read_settled = true;
        if let Some(refusal) = refusal {
            self.plan_summary.as_mut().unwrap().rollback.refusal = Some(refusal);
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
        if work.rollback.cancelled {
            self.plan_cache.revoke();
        } else if work.rollback.refusal.is_none() {
            let recorded = work
                .recorded
                .as_ref()
                .expect("successful summary retains actual Recorded proof");
            self.plan_cache.committed = Some(recorded.clone());
            self.plan_cache.required = None;
            self.plan_cache.unavailable = None;
        }
        work.rollback.complete = true;
        work.rollback.account_settled = true;
        Ok(PlanSummaryProgress::Complete)
    }

    pub(in crate::flow) fn cancel_plan_summary(
        &mut self,
        _io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        let Some(work) = self.plan_summary.as_mut() else {
            return Err(RunFailure::PlanSummaryOwned);
        };
        self.plan_cache.revoke();
        if !work.rollback.complete {
            work.rollback.cancelled = true;
            if self.read.is_some() {
                self.cancel_plan_summary_read();
            }
        }
        Ok(())
    }
    pub(in crate::flow) fn take_plan_summary(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<Option<PlanSummaryCompletion>, RunFailure> {
        self.live(io)?;
        let Some(work) = self.plan_summary.take() else {
            return Ok(None);
        };
        if !work.rollback.complete {
            self.plan_summary = Some(work);
            return Ok(None);
        }
        let (prerequisite, recorded, mut rollback) = work.split_evidence();
        match prerequisite {
            Prerequisite::Boot(repaired) => {
                if rollback.cancelled
                    || rollback.refusal.is_some()
                    || self.boot_enter_cancelled
                    || self.plan_cache.unavailable == Some(PlanSummaryUnavailable::Revoked)
                {
                    self.plan_summary =
                        Some(rollback.with_evidence(Prerequisite::Boot(repaired), recorded));
                    return Err(RunFailure::PlanSummaryOwned);
                }
                let Some(recorded) = recorded else {
                    self.plan_summary =
                        Some(rollback.with_evidence(Prerequisite::Boot(repaired), None));
                    return Err(
                        self.plan_summary_fault(io, "completed Boot lost its actual Recorded read")
                    );
                };
                // Freeze the same current minima before transferring the owner.
                self.freeze_taken_plan_summary_bound(&mut rollback);
                Ok(Some(PlanSummaryCompletion::Boot(BootPlanSummary {
                    work: Box::new(rollback.with_evidence(repaired, recorded)),
                })))
            }
            Prerequisite::Effect(filing) => {
                if rollback.cancelled || rollback.refusal.is_some() {
                    let cancelled = rollback.cancelled;
                    if cancelled {
                        self.plan_cache.required = None;
                    }
                    let account = PlanSummaryAccount {
                        work: Box::new(
                            rollback.with_evidence(Prerequisite::Effect(filing), recorded),
                        ),
                    };
                    return Ok(Some(if cancelled {
                        PlanSummaryCompletion::Cancelled(account)
                    } else {
                        PlanSummaryCompletion::Refused(account)
                    }));
                }
                let Some(committed) = recorded else {
                    self.plan_summary =
                        Some(rollback.with_evidence(Prerequisite::Effect(filing), None));
                    return Err(self
                        .plan_summary_fault(io, "completed effect lost its actual Recorded read"));
                };
                Ok(Some(PlanSummaryCompletion::Effect(EffectPlanSummary {
                    filing,
                    committed,
                })))
            }
        }
    }

    /// Return the exact previously extracted Boot owner for cleanup only. This
    /// moves custody without live admission or a new clock, including after the
    /// first failure. Existing release/halt methods still own bounded disposal.
    #[allow(clippy::result_large_err)] // Refusal returns the same owned proof.
    pub(in crate::flow) fn restore_boot_plan_summary_for_rollback(
        &mut self,
        boot: BootPlanSummary,
    ) -> Result<(), PlanSummaryBeginError<BootPlanSummary>> {
        let work = &boot.work;
        let cleanup = matches!(
            self.phase,
            RecordingPhase::Failed | RecordingPhase::Releasing
        );
        let registered_enter = match self.deadlines.get(PumpDeadline::Enter) {
            Some(current) => current == work.rollback.bound,
            None => cleanup && self.retired_enter == Some(work.rollback.bound),
        };
        let valid = self.boot_proof_current(boot.repaired())
            && boot.read().belongs_to(self)
            && self.plan_cache.boot_started
            && self
                .plan_cache
                .committed
                .as_ref()
                .is_some_and(|cached| Arc::ptr_eq(cached, &work.recorded))
            && boot.repaired().checked_open().is_some_and(|open| {
                work.rollback.key == PumpDeadline::Enter
                    && work.rollback.bound == open.enter()
                    && registered_enter
            });
        let refusal = if !matches!(
            self.phase,
            RecordingPhase::Entered | RecordingPhase::Failed | RecordingPhase::Releasing
        ) || (!cleanup && self.flow.standing_state().is_none())
        {
            Some(PlanSummaryBeginRefusal::WrongPhase)
        } else if !valid {
            Some(PlanSummaryBeginRefusal::WrongProof)
        } else if self.place_engine.is_some()
            || self.plan_cache.required.is_some()
            || if cleanup {
                self.plan_summary_owner_busy(PumpDeadline::Enter, false)
            } else {
                self.plan_summary_admission_busy(PumpDeadline::Enter)
            }
        {
            Some(PlanSummaryBeginRefusal::Busy)
        } else {
            None
        };
        if let Some(refusal) = refusal {
            return Err(PlanSummaryBeginError {
                prerequisite: boot,
                refusal,
                failure: self.failure.clone(),
            });
        }
        let (repaired, recorded, rollback) = (*boot.work).split_evidence();
        self.plan_summary =
            Some(rollback.with_evidence(Prerequisite::Boot(repaired), Some(recorded)));
        self.plan_cache.revoke();
        // take_stopped_plan_summary does not sample a clock or freeze a bound;
        // retain the existing Leave/halt minimum before exposing that path.
        self.freeze_plan_summary_account_bound();
        Ok(())
    }
    /// A failed Enter retains its summary proof/account until rollback owns it.
    /// An unfinished live read must drain; only expiry permits incomplete disposal.
    pub(in crate::flow) fn begin_boot_plan_summary_release(
        &mut self,
        leave: AbsoluteBound,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        self.begin_boot_plan_summary_release_owned(leave, false, io)
    }

    pub(super) fn park_expired_boot_plan_summary(
        &mut self,
        permit: &super::worker_startup::expired::ExpiredStartupAccountPermit,
        io: &mut impl PumpIo,
    ) -> Result<PlanSummaryAccount, RunFailure> {
        self.release_live(io)?;
        if !permit.allows(self)
            || self.plan_summary.as_ref().is_none_or(|work| {
                work.effect()
                    || work.rollback.key != PumpDeadline::Enter
                    || Some(work.rollback.bound) != self.deadlines.get(PumpDeadline::Enter)
                    || work.rollback.account_deadline > self.last_ns
            })
        {
            return Err(RunFailure::PlanSummaryOwned);
        }
        self.begin_boot_plan_summary_release_owned(
            self.deadlines.get(PumpDeadline::Leave).unwrap(),
            true,
            io,
        )?;
        self.take_stopped_plan_summary()?
            .ok_or(RunFailure::PlanSummaryOwned)
    }

    fn begin_boot_plan_summary_release_owned(
        &mut self,
        leave: AbsoluteBound,
        defer_close: bool,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        // An unheld release attempt already revokes future use, even when a
        // later prerequisite refuses the transition. Custody alone must not
        // initiate that cancellation; admitted reads keep their continuation.
        if !self.caller_author_held()
            && self.phase == RecordingPhase::Entered
            && self
                .plan_summary
                .as_ref()
                .is_some_and(|work| !work.effect())
        {
            if !self.plan_summary.as_ref().unwrap().rollback.complete {
                self.plan_summary.as_mut().unwrap().rollback.cancelled = true;
                self.cancel_plan_summary_read();
            }
            self.plan_cache.revoke();
        }
        self.release_live(io)?;
        let Some(work) = self.plan_summary.as_ref() else {
            return Err(RunFailure::PlanSummaryOwned);
        };
        if work.effect() || self.phase != RecordingPhase::Entered {
            return Err(RunFailure::PlanSummaryOwned);
        }
        let account_deadline = self
            .plan_summary_account_minimum()
            .map_or(work.rollback.account_deadline, |bound| {
                work.rollback.account_deadline.min(bound)
            });
        let account_ready = work.rollback.account_settled || self.last_ns >= account_deadline;
        let read_started = work.rollback.started;
        let account_cancelled = work.rollback.cancelled;
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
        if let Some(held) = self.deadlines.get(PumpDeadline::Leave)
            && held != leave
        {
            return Err(RunFailure::PlanSummaryOwned);
        }
        // An unfinished account is an independent ownership refusal. Held
        // input still excludes starting its cancellation as a release side effect;
        // the already admitted read may continue through its private owner.
        if !account_ready
            && self.caller_author_held()
            && (read_started || account_cancelled || self.held_input() == Some(HeldInput::Caller))
        {
            return Err(RunFailure::PlanSummaryOwned);
        }
        self.check_held_input()?;
        self.freeze_plan_summary_account_bound();
        if !account_ready {
            return Err(RunFailure::PlanSummaryOwned);
        }
        if self.deadlines.get(PumpDeadline::Leave).is_none() {
            self.deadlines
                .arm(PumpDeadline::Leave, leave)
                .map_err(|error| {
                    self.plan_summary_fault(io, format!("summary rollback Leave: {error}"))
                })?;
        }
        let mut cx = FlowRecording {
            barrier: &mut self.barrier,
            inbox: &mut self.inbox,
            budget: &mut self.budget,
            author: &self.author,
        };
        let result = if defer_close {
            self.flow.park_boot_read_rollback(&mut cx, &permit)
        } else {
            self.flow.begin_plan_read_rollback(&mut cx, &permit)
        };
        result.map_err(|failure| self.flow_failed(io, failure))?;
        self.phase = RecordingPhase::Releasing;
        self.freeze_plan_summary_account_bound();
        let incomplete = (!self.plan_summary.as_ref().unwrap().rollback.account_settled)
            .then_some("summary read reached its original bound before settlement");
        self.settle_plan_summary_account(incomplete);
        if let Some(detail) = self.flow.stopped_plan_read_close_error() {
            self.plan_summary
                .as_mut()
                .unwrap()
                .rollback
                .incomplete
                .get_or_insert_with(|| format!("summary channel close: {detail}"));
        }
        // The original bounds remain in the sealed account. Only disposal has
        // finished; no read or success can obtain time from the Leave window.
        self.deadlines.disarm(PumpDeadline::StateControl);
        self.disarm_wait(PumpDeadline::Enter);
        Ok(())
    }

    fn plan_read_rollback_permit(&self) -> Result<ReadRollbackPermit, RunFailure> {
        if self.read.as_ref().is_some_and(|read| !read.is_summary()) {
            return Err(RunFailure::PlanSummaryOwned);
        }
        self.read_rollback_permit()
            .map_err(|_| RunFailure::PlanSummaryOwned)
    }

    pub(super) fn plan_owns_pending_recall(&self) -> bool {
        self.plan_summary.is_some()
            && self.read.as_ref().is_some_and(|read| {
                read.is_summary()
                    && read.plan_ticket().is_some()
                    && read.plan_ticket() == self.barrier.pending_author().map(|held| held.ticket)
            })
    }
    fn plan_summary_account_minimum(&self) -> Option<u64> {
        let read = self
            .read
            .as_ref()
            .map(|read| read.plan_bound().deadline_ns());
        let leave = self
            .deadlines
            .get(PumpDeadline::Leave)
            .map(AbsoluteBound::deadline_ns);
        let halt = self.flow.halt().and_then(|halt| halt.deadline().ok());
        read.into_iter().chain(leave).chain(halt).min()
    }
    fn freeze_taken_plan_summary_bound(&self, rollback: &mut PlanSummaryRollback) {
        if let Some(bound) = self.plan_summary_account_minimum() {
            rollback.account_deadline = rollback.account_deadline.min(bound);
        }
    }
    pub(super) fn freeze_plan_summary_account_bound(&mut self) {
        let minimum = self.plan_summary_account_minimum();
        if let Some(work) = &mut self.plan_summary
            && let Some(bound) = minimum
        {
            work.rollback.account_deadline = work.rollback.account_deadline.min(bound);
        }
    }
    fn plan_summary_account_clock(&mut self, io: &mut impl PumpIo) -> bool {
        self.freeze_plan_summary_account_bound();
        let bound = self
            .plan_summary
            .as_ref()
            .unwrap()
            .rollback
            .account_deadline;
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
        // Disposal cannot rewrite a read that had already completed. Revoked
        // future use remains a separate cache/Enter admission fact.
        if !work.rollback.complete {
            work.rollback.cancelled = true;
        }
        work.rollback.account_settled = true;
        if let Some(read) = read {
            if let Some(refusal) = read.plan_refusal() {
                work.rollback.refusal = Some(refusal.clone());
            }
            work.rollback.stopped_read = Some(Box::new(read));
        }
        if let Some(detail) = incomplete {
            work.rollback
                .incomplete
                .get_or_insert_with(|| detail.into());
        }
        self.plan_cache.revoke();
    }
    pub(super) fn abandon_plan_summary_account(&mut self, detail: &str) {
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

    pub(super) fn dispose_expired_startup_summary(
        &mut self,
        permit: &super::worker_startup::expired::ExpiredStartupDisposal,
    ) {
        assert!(permit.allows(self), "exact expired startup disposal");
        if self
            .plan_summary
            .as_ref()
            .is_some_and(|work| !work.rollback.account_settled)
        {
            self.freeze_plan_summary_account_bound();
            self.abandon_plan_summary_account("original Enter and accepted rollback Leave expired");
        }
        self.plan_cache.revoke();
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
        if self.plan_summary.as_ref().unwrap().rollback.account_settled {
            return Ok(PlanSummaryProgress::Complete);
        }
        self.plan_summary.as_mut().unwrap().rollback.cancelled = true;
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
        match self.resume_trace_only_and_bind() {
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
        if !self.plan_summary.as_ref().unwrap().rollback.account_settled
            && !self.plan_summary_account_clock(io)
        {
            self.abandon_plan_summary_account(
                "summary account reached its original bound after progress",
            );
        }
        Ok(
            if self.plan_summary.as_ref().unwrap().rollback.account_settled {
                PlanSummaryProgress::Complete
            } else {
                PlanSummaryProgress::Pending
            },
        )
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
            .is_none_or(|work| !work.rollback.account_settled)
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

// A successful Boot's prerequisite and recorded read are static fields. These
// pins fail to compile if either becomes an enum/Option requiring a runtime arm.
#[cfg(test)]
const _: fn(&BootPlanSummary) -> &RepairedBootPairs = |proof| &proof.work.prerequisite;
#[cfg(test)]
const _: fn(&BootPlanSummary) -> &Arc<CommittedPlanSummary> = |proof| &proof.work.recorded;
