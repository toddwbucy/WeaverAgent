//! Dormant BOOT-REPAIR mechanics. The prior-Open prerequisite has no production
//! constructor until the checked Open owner exists. Completion certifies only
//! this repair and its empty recheck, never lifecycle Enter or user-gate readiness.

use super::*;
use crate::flow::author::AuthoredSource;
use crate::flow::ledger::{LedgerFile, SourceDisposition, TerminalOutcome};
use crate::flow::read::ReadFailure;
use receipt_driver::FilingOwner;
use weaver_trace::Subsystem;
use weaver_types::state_flow::{
    PairEnded, PairFailure, PairFile, PairRecord, QueryResult, ReadSelector, SourceBlockId,
    StateFrame, TerminalPairFate,
};

/// Intentionally no production constructor, public fields or conversion. The
/// future checked Open owner must retain its actual proof when licensing this.
#[derive(Debug)]
pub(in crate::flow) struct PriorBootOpen {
    origin: std::sync::Arc<()>,
    head: Sequence,
    enter: AbsoluteBound,
}

impl PriorBootOpen {
    #[cfg(test)]
    pub(super) fn assume_checked_open_for_test(context: &RecordingContext) -> Self {
        // Explicit missing-integration assumption for mechanics tests. The real
        // load, certified templates and Released/Recorded BootHead are required.
        let head = context.boot_head.as_ref().expect("real recorded boot head");
        Self {
            origin: context.read_origin.clone(),
            head: head.recall_sequence(),
            enter: head.enter_bound(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum BootPairsBeginRefusal {
    WrongPhase,
    WrongOpen,
    AlreadyStarted,
    Busy,
}

#[derive(Debug)]
pub(in crate::flow) struct BootPairsBeginError {
    pub prior: PriorBootOpen,
    pub refusal: BootPairsBeginRefusal,
    pub failure: Option<RunFailure>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum BootPairsProgress {
    Pending,
    Complete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum BootPairsRefusal {
    Read,
    NonemptyRecheck,
}

/// Both actual read proofs and all committed terminals remain together.
#[derive(Debug)]
pub(in crate::flow) struct RepairedBootPairs {
    account: Box<BootPairsTransaction>,
}

impl RepairedBootPairs {
    pub(in crate::flow) fn belongs_to(&self, context: &RecordingContext) -> bool {
        std::sync::Arc::ptr_eq(&self.account.prior.origin, &context.read_origin)
    }
    pub(in crate::flow) fn initial(&self) -> &RecordedRead {
        match self.account.initial.as_ref().unwrap() {
            ReadCompletion::Recorded(read) => read,
            _ => unreachable!("only complete recorded reads grant repair evidence"),
        }
    }
    pub(in crate::flow) fn recheck(&self) -> &RecordedRead {
        match self.account.recheck.as_ref().unwrap() {
            ReadCompletion::Recorded(read) => read,
            _ => unreachable!("only an empty recorded recheck grants repair evidence"),
        }
    }
    pub(in crate::flow) fn terminals(&self) -> &[FilingCompletion] {
        &self.account.terminals
    }
}

/// Retained cancellation/refusal evidence has no success-proof accessor.
#[derive(Debug)]
pub(in crate::flow) struct BootPairsAccount {
    work: Box<BootPairsTransaction>,
}

impl BootPairsAccount {
    pub(in crate::flow) fn initial(&self) -> Option<&ReadCompletion> {
        self.work.initial.as_ref()
    }
    pub(in crate::flow) fn recheck(&self) -> Option<&ReadCompletion> {
        self.work.recheck.as_ref()
    }
    pub(in crate::flow) fn terminals(&self) -> &[FilingCompletion] {
        &self.work.terminals
    }
    pub(in crate::flow) fn cancelled_event(&self) -> Option<&AuthoredEvent> {
        self.work.cancelled_event.as_ref()
    }
    pub(in crate::flow) fn refusal(&self) -> Option<BootPairsRefusal> {
        self.work.refusal
    }
    pub(in crate::flow) fn read_refusal(&self) -> Option<&ReadFailure> {
        [&self.work.initial, &self.work.recheck]
            .into_iter()
            .find_map(|read| match read.as_ref() {
                Some(ReadCompletion::Refused(refusal)) => Some(refusal),
                _ => None,
            })
    }
}

#[derive(Debug)]
pub(in crate::flow) enum BootPairsCompletion {
    Repaired(RepairedBootPairs),
    Refused {
        refusal: BootPairsRefusal,
        account: BootPairsAccount,
    },
    Cancelled(BootPairsAccount),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Initial,
    Terminals,
    Recheck,
    Complete,
}

pub(super) struct BootPairsTransaction {
    prior: PriorBootOpen,
    start: StartIdentity,
    driving: bool,
    stage: Stage,
    initial: Option<ReadCompletion>,
    recheck: Option<ReadCompletion>,
    terminals: Vec<FilingCompletion>,
    expected: Option<PairEnded>,
    bound: Option<AbsoluteBound>,
    receipt_settled: bool,
    cancelled: bool,
    cancelled_event: Option<AuthoredEvent>,
    refusal: Option<BootPairsRefusal>,
}

impl std::fmt::Debug for BootPairsTransaction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BootPairsTransaction")
            .field("stage", &self.stage)
            .field("terminals", &self.terminals.len())
            .field("cancelled", &self.cancelled)
            .finish_non_exhaustive()
    }
}

impl RecordingContext {
    pub(super) fn boot_pairs_blocks_external(&self) -> bool {
        self.boot_pairs.as_ref().is_some_and(|work| !work.driving)
    }

    pub(super) fn boot_pairs_read_allowed(&self) -> bool {
        self.boot_pairs.as_ref().is_some_and(|work| {
            work.driving && matches!(work.stage, Stage::Initial | Stage::Recheck)
        })
    }

    pub(super) fn boot_pairs_filing_allowed(&self) -> bool {
        self.boot_pairs.as_ref().is_some_and(|work| {
            work.driving
                && work.stage == Stage::Terminals
                && work.expected.is_some()
                && work.bound.is_some()
        })
    }

    pub(super) fn boot_pairs_wait_owned(&self, key: PumpDeadline) -> bool {
        self.boot_pairs.as_ref().is_some_and(|work| {
            key == PumpDeadline::Enter || (key == PumpDeadline::Decision && work.bound.is_some())
        })
    }

    /// Retire only a timely Recorded receipt. This runs before a later live
    /// sample, so caller retrieval time cannot turn completed work into a miss.
    pub(super) fn settle_boot_pairs_receipt_wait(&mut self) {
        let Some(work) = &self.boot_pairs else { return };
        let Some(bound) = work.bound else { return };
        if !work.receipt_settled && self.owned_filing_recorded_before(FilingOwner::BootPairs, bound)
        {
            self.deadlines.disarm(PumpDeadline::Decision);
            self.boot_pairs.as_mut().unwrap().receipt_settled = true;
        }
    }

    #[allow(clippy::result_large_err)] // Return the sole private prerequisite on refusal.
    pub(in crate::flow) fn begin_boot_pairs(
        &mut self,
        prior: PriorBootOpen,
        io: &mut impl PumpIo,
    ) -> Result<(), BootPairsBeginError> {
        let failure = self.live(io).err();
        let refusal = if failure.is_some()
            || self.phase != RecordingPhase::Entered
            || self.flow.phase() != CoordinatorPhase::Active
            || !self.barrier.is_active()
        {
            Some(BootPairsBeginRefusal::WrongPhase)
        } else if self.boot_pairs_started {
            Some(BootPairsBeginRefusal::AlreadyStarted)
        } else if !std::sync::Arc::ptr_eq(&prior.origin, &self.read_origin)
            || self.boot_head.as_ref().is_none_or(|head| {
                !head.belongs_to(self)
                    || head.recall_sequence() != prior.head
                    || head.enter_bound() != prior.enter
            })
            || self.deadlines.get(PumpDeadline::Enter) != Some(prior.enter)
            || self.open_start.is_none()
            || self.load_duty.is_none()
        {
            Some(BootPairsBeginRefusal::WrongOpen)
        } else if self.open_turn.is_some()
            || self.input.snapshot().is_some()
            || self.input_turn.is_some()
            || self.input_author.is_some()
            || self.output.is_some()
            || self.pair_work.is_some()
            || self.repair.is_some()
            || self.filing.is_some()
            || self.read.is_some()
            || self.marks.is_some()
            || self.freshness.is_some()
            || self.measure.is_some()
            || self.member_capture.is_some()
            || self.templates_block_content()
            || self.waiting_content.is_some()
            || self.completed_author.is_some()
            || self.template_blocked_author.is_some()
            || self.barrier.pending().is_some()
            || self.barrier.pending_author().is_some()
            || self.flow.has_pending_record()
            || self.flow.has_filing_consumer()
            || self.flow.has_output_consumer()
            || self.flow.pending_control().is_some()
            || self.flow.pending_check().is_some()
            || self.flow.operation_completion().is_some()
            || !self.flow.state().outstanding().is_empty()
            || self.flow.state().held_measurement().is_some()
            || [
                PumpDeadline::Decision,
                PumpDeadline::Cut,
                PumpDeadline::Gate,
                PumpDeadline::StateControl,
            ]
            .into_iter()
            .any(|key| self.deadlines.get(key).is_some())
        {
            Some(BootPairsBeginRefusal::Busy)
        } else {
            None
        };
        if let Some(refusal) = refusal {
            return Err(BootPairsBeginError {
                prior,
                refusal,
                failure,
            });
        }
        self.boot_pairs_started = true;
        self.boot_pairs = Some(BootPairsTransaction {
            prior,
            start: self.open_start.as_ref().unwrap().clone(),
            driving: false,
            stage: Stage::Initial,
            initial: None,
            recheck: None,
            terminals: vec![],
            expected: None,
            bound: None,
            receipt_settled: false,
            cancelled: false,
            cancelled_event: None,
            refusal: None,
        });
        Ok(())
    }

    pub(in crate::flow) fn advance_boot_pairs<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<BootPairsProgress, RunFailure> {
        self.live(ports.io)?;
        self.boot_pairs_wait_matches(ports.io)?;
        self.boot_pairs.as_mut().unwrap().driving = true;
        let result = self.advance_boot_pairs_owned(ports);
        self.boot_pairs.as_mut().unwrap().driving = false;
        result
    }

    fn advance_boot_pairs_owned<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<BootPairsProgress, RunFailure> {
        match self.boot_pairs.as_ref().unwrap().stage {
            Stage::Initial | Stage::Recheck => self.advance_boot_pairs_read(ports)?,
            Stage::Terminals => self.advance_boot_pairs_terminal(ports)?,
            Stage::Complete => {}
        }
        Ok(
            if self.boot_pairs.as_ref().unwrap().stage == Stage::Complete {
                BootPairsProgress::Complete
            } else {
                BootPairsProgress::Pending
            },
        )
    }

    fn advance_boot_pairs_read<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<(), RunFailure> {
        if self.read.is_none() {
            if self.boot_pairs.as_ref().unwrap().cancelled {
                self.boot_pairs.as_mut().unwrap().stage = Stage::Complete;
                return Ok(());
            }
            match self.begin_boot_pairs_read(ports.io) {
                Ok(()) => {}
                Err(error) => {
                    if let Some(failure) = error.failure {
                        return Err(failure);
                    }
                    if error.refusal == ReadBeginRefusal::Busy {
                        self.reduce_flow(ports.io)?;
                        return Ok(());
                    }
                    return Err(self.boot_pairs_fault(ports.io, "boot pair read lost admission"));
                }
            }
        }
        if self.boot_pairs.as_ref().unwrap().cancelled {
            self.cancel_read(ports.io)?;
        }
        self.advance_read(ports)?;
        let Some(read) = self.take_read(ports.io)? else {
            return Ok(());
        };
        // No live hook after take: retain the exact consumed proof first.
        let work = self.boot_pairs.as_mut().unwrap();
        let initial = work.stage == Stage::Initial;
        if initial {
            work.initial = Some(read);
        } else {
            work.recheck = Some(read);
        }
        if work.cancelled {
            work.stage = Stage::Complete;
            return Ok(());
        }
        let read = if initial {
            &work.initial
        } else {
            &work.recheck
        };
        let read = match read.as_ref().unwrap() {
            ReadCompletion::Recorded(read) => read,
            ReadCompletion::Refused(_) => {
                work.refusal = Some(BootPairsRefusal::Read);
                work.stage = Stage::Complete;
                return Ok(());
            }
            ReadCompletion::Cancelled(_) => {
                return Err(self.boot_pairs_fault(ports.io, "boot pair read lost consumption"));
            }
        };
        if !std::sync::Arc::ptr_eq(read.origin(), &work.prior.origin)
            || read.selector() != &(ReadSelector::OpenPairs {})
        {
            return Err(self.boot_pairs_fault(ports.io, "foreign boot pair read"));
        }
        let QueryResult::OpenPairs { pairs } = read.result() else {
            return Err(self.boot_pairs_fault(ports.io, "boot pair read changed variant"));
        };
        if initial {
            work.stage = if pairs.is_empty() {
                Stage::Recheck
            } else {
                Stage::Terminals
            };
        } else {
            if !pairs.is_empty() {
                work.refusal = Some(BootPairsRefusal::NonemptyRecheck);
            }
            work.stage = Stage::Complete;
        }
        if self.boot_pairs.as_ref().unwrap().stage == Stage::Terminals {
            // The first required terminal now blocks progress. Its wait and
            // retained proposal install before another live sample can yield.
            self.prepare_boot_pair_terminal(ports.io)?;
        }
        Ok(())
    }

    fn advance_boot_pairs_terminal<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<(), RunFailure> {
        if self.boot_pairs.as_ref().unwrap().cancelled
            && let Some(event) = self.withdraw_unstarted_boot_pair()
        {
            let work = self.boot_pairs.as_mut().unwrap();
            work.cancelled_event = Some(event);
            work.stage = Stage::Complete;
            work.bound = None;
            self.deadlines.disarm(PumpDeadline::Decision);
            return Ok(());
        }
        if self.filing.is_none() {
            if self.boot_pairs.as_ref().unwrap().cancelled {
                let work = self.boot_pairs.as_mut().unwrap();
                work.stage = Stage::Complete;
                work.bound = None;
                self.deadlines.disarm(PumpDeadline::Decision);
                return Ok(());
            }
            self.prepare_boot_pair_terminal(ports.io)?;
            let ended = self.boot_pairs.as_ref().unwrap().expected.as_ref().unwrap();
            let event = AuthoredEvent {
                kind: Kind::ToolCallCompleted,
                subsystem: Subsystem::Harness,
                turn: None,
                source: AuthoredSource::Loop,
                causal_parent: None,
                payload: Some(Payload::PairEnded(Box::new(
                    super::super::mirror::pair_ended(ended),
                ))),
            };
            match self.begin_owned_filing(
                event,
                PumpDeadline::Decision,
                FilingOwner::BootPairs,
                ports.io,
            ) {
                Ok(()) => return Ok(()),
                Err(error) => {
                    if let Some(failure) = error.failure {
                        return Err(failure);
                    }
                    if error.refusal == FilingBeginRefusal::Busy {
                        self.reduce_flow(ports.io)?;
                        return Ok(());
                    }
                    return Err(
                        self.boot_pairs_fault(ports.io, "boot pair terminal lost admission")
                    );
                }
            }
        }
        self.advance_owned_filing(FilingOwner::BootPairs, ports)?;
        let Some(filing) = self.take_owned_filing(FilingOwner::BootPairs, ports.io)? else {
            return Ok(());
        };
        // Install the exact receipt before validation or the next yielding act.
        self.boot_pairs.as_mut().unwrap().terminals.push(filing);
        self.check_boot_pair_receipt(ports.io)?;
        let work = self.boot_pairs.as_mut().unwrap();
        work.expected = None;
        work.bound = None;
        work.receipt_settled = false;
        self.deadlines.disarm(PumpDeadline::Decision);
        let ReadCompletion::Recorded(read) = work.initial.as_ref().unwrap() else {
            unreachable!()
        };
        let QueryResult::OpenPairs { pairs } = read.result() else {
            unreachable!()
        };
        if work.cancelled {
            work.stage = Stage::Complete;
        } else if work.terminals.len() == pairs.len() {
            work.stage = Stage::Recheck;
        }
        // A later terminal owns a distinct receipt barrier. Install it in the
        // same non-yielding transfer that consumed the previous exact receipt.
        if self.boot_pairs.as_ref().unwrap().stage == Stage::Terminals {
            self.prepare_boot_pair_terminal(ports.io)?;
        }
        Ok(())
    }

    fn prepare_boot_pair_terminal(&mut self, io: &mut impl PumpIo) -> Result<(), RunFailure> {
        let work = self.boot_pairs.as_ref().unwrap();
        if work.expected.is_some() {
            return Ok(());
        }
        let ReadCompletion::Recorded(read) = work.initial.as_ref().unwrap() else {
            unreachable!()
        };
        let QueryResult::OpenPairs { pairs } = read.result() else {
            unreachable!()
        };
        let pair = &pairs[work.terminals.len()];
        if pair.call_id.run == self.flow.run() {
            return Err(self.boot_pairs_fault(io, "boot abandonment requires a prior-run call"));
        }
        let ended = PairEnded {
            call_id: pair.call_id.clone(),
            execution: None,
            fate: TerminalPairFate::Failed,
            reason: Some(PairFailure::AbandonedAtLoad {}),
            outcome: None,
            loaded_from: Some(work.start.clone()),
        };
        let duration = self
            .load_duty
            .as_ref()
            .unwrap()
            .instruction
            .decision_wait
            .elapsed_ns
            .0;
        let Some(mut end) = self.last_ns.checked_add(duration) else {
            return Err(self.boot_pairs_fault(io, "boot pair receipt wait overflow"));
        };
        end = end.min(work.prior.enter.deadline_ns());
        if let Some(leave) = self.deadlines.get(PumpDeadline::Leave) {
            end = end.min(leave.deadline_ns());
        }
        let bound = AbsoluteBound::from_absolute(self.last_ns, end)
            .map_err(|error| self.boot_pairs_fault(io, error.to_string()))?;
        self.deadlines
            .arm(PumpDeadline::Decision, bound)
            .map_err(|error| self.boot_pairs_fault(io, error.to_string()))?;
        let work = self.boot_pairs.as_mut().unwrap();
        work.expected = Some(ended);
        work.bound = Some(bound);
        work.receipt_settled = false;
        Ok(())
    }

    fn check_boot_pair_receipt(&mut self, io: &mut impl PumpIo) -> Result<(), RunFailure> {
        let valid = {
            let work = self.boot_pairs.as_ref().unwrap();
            let filing = work.terminals.last().unwrap();
            let mut dispositions = filing.receipts().dispositions();
            match (dispositions.next(), dispositions.next()) {
                (Some(SourceDisposition::Committed(source)), None) => {
                    let common = source.common();
                    boot_pair_source_matches(source.source(), self.flow.run(), filing.sequence())
                        && common.session_id == self.flow.state().identity().0
                        && common.run_id == self.flow.run()
                        && source.terminal().source == *source.source()
                        && source.terminal().outcome == TerminalOutcome::Filed
                        && matches!(source.receipt(), StateFrame::Filed { block_id, .. } if block_id == source.source())
                        && matches!(source.file(), LedgerFile::Pair(file)
                        if boot_pair_content_matches(
                            file,
                            &common.session_id,
                            &common.run_id,
                            &self.declaration.scheme.scheme_id,
                            work.expected.as_ref(),
                        ))
                }
                _ => false,
            }
        };
        if !valid {
            return Err(self.boot_pairs_fault(io, "boot repair lost its exact terminal receipt"));
        }
        Ok(())
    }

    pub(in crate::flow) fn cancel_boot_pairs(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        self.live(io)?;
        self.boot_pairs_wait_matches(io)?;
        // Like begin_task, cancellation revokes an untaken result even after
        // settlement. The account keeps both reads and every committed receipt.
        self.boot_pairs.as_mut().unwrap().cancelled = true;
        Ok(())
    }

    pub(in crate::flow) fn take_boot_pairs(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<Option<BootPairsCompletion>, RunFailure> {
        self.live(io)?;
        self.boot_pairs_wait_matches(io)?;
        if self.boot_pairs.as_ref().unwrap().stage != Stage::Complete {
            return Ok(None);
        }
        let work = Box::new(self.boot_pairs.take().unwrap());
        Ok(Some(if work.cancelled {
            BootPairsCompletion::Cancelled(BootPairsAccount { work })
        } else if let Some(refusal) = work.refusal {
            BootPairsCompletion::Refused {
                refusal,
                account: BootPairsAccount { work },
            }
        } else {
            BootPairsCompletion::Repaired(RepairedBootPairs { account: work })
        }))
    }

    fn boot_pairs_wait_matches(&mut self, io: &mut impl PumpIo) -> Result<(), RunFailure> {
        let Some(work) = self.boot_pairs.as_ref() else {
            return Err(self.boot_pairs_fault(io, "boot repair operation has no owner"));
        };
        if self.deadlines.get(PumpDeadline::Enter) != Some(work.prior.enter)
            || work.bound.is_some_and(|bound| {
                self.deadlines.get(PumpDeadline::Decision)
                    != if work.receipt_settled {
                        None
                    } else {
                        Some(bound)
                    }
            })
        {
            return Err(self.boot_pairs_fault(io, "boot repair lost its inherited wait"));
        }
        Ok(())
    }

    fn boot_pairs_fault(&mut self, io: &mut impl PumpIo, detail: impl Into<String>) -> RunFailure {
        self.fail(
            io,
            FlowHaltReason::ProtocolFailed,
            HaltPoint::Barrier,
            detail,
        )
    }
}

// Pure association predicates share exactly the production checks with tests
// inspecting copies of actual receipt values. They mint no receipt or grant.
fn boot_pair_source_matches(source: &SourceBlockId, run: &str, sequence: Sequence) -> bool {
    source.run == run && source.sequence.0 == sequence.0 && source.block.0 == 0
}

fn boot_pair_content_matches(
    file: &PairFile,
    session: &str,
    run: &str,
    scheme: &str,
    expected: Option<&PairEnded>,
) -> bool {
    file.scheme_id == scheme
        && file.source.session == session
        && file.source.run == run
        && file.source.kind == "tool.call.completed"
        && file.source.channel == "tool"
        && file.source.origin == "loop"
        && file.source.subsystem == "harness"
        && file.source.turn.is_none()
        && file.source.causal_parent.is_none()
        && file.validate_structure().is_ok()
        && matches!(&file.pair, PairRecord::Ended(ended) if Some(ended) == expected)
}

#[cfg(test)]
#[path = "context_boot_pairs_validation_tests.rs"]
pub(super) mod validation_tests;
