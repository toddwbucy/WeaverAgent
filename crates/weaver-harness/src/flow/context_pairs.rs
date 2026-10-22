//! The recorded, committed lifecycle of accepted requests barred from execution.
//! The loop supplies the cause. This owner performs no decoder pass, gate call,
//! retry, placement or tool clock. Receipt waits borrow one original Decision bound.

use super::pairs::{CompletedPair, PairRecordPhase};
use super::receipt_driver::FilingOwner;
use super::repair::RepairAccount;
use super::*;
use crate::flow::author::AuthoredSource;
use crate::flow::coordinator::{CheckedGeneration, OriginalRequests};
use crate::flow::ledger::{LeaseSlotStatus, SourceDisposition};
use crate::flow::mirror;
use crate::flow::report::OutstandingPair;
use weaver_trace::Subsystem;
use weaver_types::state_flow::{CallId, NonDispatchCause, SourceBlockId, Ste100Target};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum PairBeginRefusal {
    WrongPhase,
    Busy,
    MissingWait,
    WrongGeneration,
    LostCustody,
    InvalidBatch,
}

#[derive(Debug)]
pub(in crate::flow) struct PairBeginError {
    pub original: OriginalRequests,
    pub refusal: PairBeginRefusal,
    pub failure: Option<RunFailure>,
}

#[derive(Debug)]
pub(in crate::flow) struct RepairPairBeginError {
    pub account: RepairAccount,
    pub refusal: PairBeginRefusal,
    pub failure: Option<RunFailure>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum PairProgress {
    Progress,
    Pending,
    Complete,
}

#[derive(Debug)]
pub(in crate::flow) enum PairReadError {
    Run(RunFailure),
    UnknownCall,
}

pub(super) struct PairLeg {
    call: CallId,
    phase: PairRecordPhase,
    recorded: Option<SourceBlockId>,
}

pub(super) struct PairTransaction {
    pub(super) inward: Option<super::inward_recall::InwardWork>,
    pub(super) bound: Option<AbsoluteBound>,
    pub(super) leg: Option<PairLeg>,
    // A failed receipt check must not drop the exact evidence it rejected.
    refused_receipt: Option<SourceDisposition>,
    refused_filing: Option<FilingCompletion>,
    pub(super) send: Option<crate::flow::pump_transport::OrganSend>,
    pub(super) refused_send: Option<weaver_types::OrganEnvelope>,
}

impl PairTransaction {
    pub(super) fn new(bound: AbsoluteBound) -> Self {
        Self {
            inward: None,
            bound: Some(bound),
            leg: None,
            refused_receipt: None,
            refused_filing: None,
            send: None,
            refused_send: None,
        }
    }
}

impl RecordingContext {
    /// Account all accepted requests, including accepted siblings of blocks still
    /// awaiting repair. Return the original ownership unchanged on caller refusal.
    #[allow(clippy::result_large_err)]
    pub(in crate::flow) fn begin_not_dispatched(
        &mut self,
        original: OriginalRequests,
        cause: NonDispatchCause,
        io: &mut impl PumpIo,
    ) -> Result<(), PairBeginError> {
        if let Err(failure) = self.live(io) {
            return Err(PairBeginError {
                original,
                refusal: PairBeginRefusal::WrongPhase,
                failure: Some(failure),
            });
        }
        let refusal = self.pair_begin_refusal().or_else(|| {
            if !self.pair_generation_matches(original.generation()) {
                Some(PairBeginRefusal::WrongGeneration)
            } else if !self.pair_custody_matches(&original) {
                Some(PairBeginRefusal::LostCustody)
            } else {
                None
            }
        });
        if let Some(refusal) = refusal {
            return Err(PairBeginError {
                original,
                refusal,
                failure: None,
            });
        }
        if let Err((original, _)) = self.pairs.accept(original) {
            return Err(PairBeginError {
                original,
                refusal: PairBeginRefusal::InvalidBatch,
                failure: None,
            });
        }
        self.start_pair_work(cause);
        Ok(())
    }

    /// The repair owner supplies the complete private substitution proof. Public
    /// corrected references cannot be used to create this account or its receipts.
    #[allow(clippy::result_large_err)]
    pub(in crate::flow) fn begin_repair_not_dispatched(
        &mut self,
        account: RepairAccount,
        cause: NonDispatchCause,
        io: &mut impl PumpIo,
    ) -> Result<(), RepairPairBeginError> {
        if let Err(failure) = self.live(io) {
            return Err(RepairPairBeginError {
                account,
                refusal: PairBeginRefusal::WrongPhase,
                failure: Some(failure),
            });
        }
        let refusal = self.pair_begin_refusal().or_else(|| {
            if !account
                .generations()
                .all(|generation| self.pair_generation_matches(generation))
            {
                Some(PairBeginRefusal::WrongGeneration)
            } else if !self.repair_pair_custody_matches(&account) {
                Some(PairBeginRefusal::LostCustody)
            } else {
                None
            }
        });
        if let Some(refusal) = refusal {
            return Err(RepairPairBeginError {
                account,
                refusal,
                failure: None,
            });
        }
        if let Err((account, _)) = self.pairs.accept_repair(account) {
            return Err(RepairPairBeginError {
                account,
                refusal: PairBeginRefusal::InvalidBatch,
                failure: None,
            });
        }
        self.start_pair_work(cause);
        Ok(())
    }

    pub(super) fn pair_begin_refusal(&self) -> Option<PairBeginRefusal> {
        if self.phase != RecordingPhase::Entered
            || self.flow.phase() != CoordinatorPhase::Active
            || !self.barrier.is_active()
        {
            Some(PairBeginRefusal::WrongPhase)
        } else if self.marks.is_some()
            || (self.repair_blocks_external() && self.repair_owns_pairs())
            || self.freshness.is_some()
            || self.pair_work.is_some()
            || self.pair_rejected_authoring
            || self.pairs.pending_rejection().is_some()
            || self.output.is_some()
            || self.filing.is_some()
            || self.templates_block_content()
            || self.template_blocked_author.is_some()
            || self.input.snapshot().is_some()
            || self.input_turn.is_some()
            || self.input_author.is_some()
            || self.member_capture.is_some()
            || self.read.is_some()
            || self.measure_blocks_external()
            || self.waiting_content.is_some()
            || self.completed_author.is_some()
            || self.barrier.pending().is_some()
            || self.barrier.pending_author().is_some()
            || self.flow.has_pending_record()
            || self.flow.pending_check().is_some()
            || self.flow.pending_control().is_some()
            || self.flow.operation_completion().is_some()
        {
            Some(PairBeginRefusal::Busy)
        } else if self.deadlines.get(PumpDeadline::Decision).is_none() {
            Some(PairBeginRefusal::MissingWait)
        } else {
            None
        }
    }

    fn start_pair_work(&mut self, cause: NonDispatchCause) {
        self.pair_work = Some(PairTransaction::new(
            self.deadlines
                .get(PumpDeadline::Decision)
                .expect("checked bound"),
        ));
        // accept establishes the active batch. A failure here is an internal
        // contract error, with the batch still retained in the frozen owner.
        self.pairs
            .set_not_dispatched(cause)
            .expect("new healthy batch");
    }

    pub(super) fn pair_custody_matches(&self, original: &OriginalRequests) -> bool {
        self.generation_custody_matches(original.generation())
            && original.dependencies().all(|statement| {
                self.pair_dependency_ready(original.generation(), &statement.block_id)
            })
    }

    pub(super) fn pair_generation_matches(&self, generation: &CheckedGeneration) -> bool {
        generation.scheme() == &self.declaration.scheme
            && generation.source().block_id.run == self.flow.state().identity().1
            && self.open_turn.as_ref() == Some(&generation.original().turn)
    }

    pub(super) fn generation_custody_matches(&self, generation: &CheckedGeneration) -> bool {
        if !self.pair_generation_matches(generation) {
            return false;
        }
        let Some(lease) = generation.receipt_lease() else {
            return false;
        };
        let Ok(sources) = self.flow.ledger().source_lease_sources(lease) else {
            return false;
        };
        let targets = generation
            .blocks()
            .iter()
            .filter(|check| check.selected())
            .map(|check| &check.request().target);
        sources
            .map(|id| Ste100Target::Source(id.clone()))
            .eq(targets.cloned())
    }

    fn pair_dependency_ready(
        &self,
        generation: &CheckedGeneration,
        source: &SourceBlockId,
    ) -> bool {
        generation.receipt_lease().is_some_and(|lease| {
            matches!(
                self.flow.source_receipt_status(lease, source),
                Ok(LeaseSlotStatus::Pending | LeaseSlotStatus::Ready)
            )
        })
    }

    pub(super) fn repair_pair_custody_matches(&self, account: &RepairAccount) -> bool {
        account
            .generations()
            .all(|generation| self.generation_custody_matches(generation))
            && account.requests().iter().all(|request| {
                account.generations().any(|generation| {
                    generation.blocks().iter().any(|check| {
                        check.request().target
                            == Ste100Target::Source(request.statement.block_id.clone())
                    }) && self.pair_dependency_ready(generation, &request.statement.block_id)
                })
            })
    }

    /// Accepted stop/leave may replace an unclaimed ordinary cause. A record
    /// already handed to the author remains immutable, including a pending ticket.
    pub(in crate::flow) fn bar_pending_pairs(
        &mut self,
        cause: NonDispatchCause,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        self.live(io)?;
        if self.repair_blocks_external() && self.repair_owns_pairs() {
            return Err(RunFailure::RepairOwned);
        }
        if self.pairs.executing().is_some() {
            return Err(RunFailure::PairOwned);
        }
        self.pairs
            .set_not_dispatched(cause)
            .map_err(|error| self.pair_fault(io, format!("pair disposition: {error:?}")))
    }

    pub(super) fn pair_wait_still_owned(&self, key: PumpDeadline) -> bool {
        if key == PumpDeadline::Decision && self.pair_rejection_wait.is_some() {
            return true;
        }
        key == PumpDeadline::Decision
            && self
                .pair_work
                .as_ref()
                .is_some_and(|work| work.bound.is_some())
            && !self.pairs.is_complete()
    }

    pub(super) fn pair_wait_matches(&mut self, io: &mut impl PumpIo) -> Result<(), RunFailure> {
        let Some(work) = &self.pair_work else {
            return Err(RunFailure::PairOwned);
        };
        let actual = self.deadlines.get(PumpDeadline::Decision);
        if actual != work.bound && !(actual.is_none() && self.pairs.is_complete()) {
            return Err(self.pair_fault(io, "pair accounting lost its original wait"));
        }
        Ok(())
    }

    pub(in crate::flow) fn advance_pairs<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<PairProgress, RunFailure> {
        self.live(ports.io)?;
        if self.repair_blocks_external() && self.repair_owns_pairs() {
            return Err(RunFailure::RepairOwned);
        }
        if self.pair_rejected_authoring || self.pairs.pending_rejection().is_some() {
            return Ok(PairProgress::Pending);
        }
        self.pair_wait_matches(ports.io)?;
        if self
            .pair_work
            .as_ref()
            .is_some_and(|work| work.inward.is_some())
        {
            return Ok(PairProgress::Pending);
        }
        if self
            .pair_work
            .as_ref()
            .is_some_and(|work| work.bound.is_none())
            || self.pairs.pending_rejection().is_some()
        {
            return Ok(PairProgress::Pending);
        }
        self.collect_pair_statement(ports.io)?;
        if self.pair_work.as_ref().unwrap().leg.is_some() {
            let progress = self.advance_owned_filing(FilingOwner::Pair, ports)?;
            self.collect_pair_statement(ports.io)?;
            if progress != FilingProgress::Complete {
                return Ok(if progress == FilingProgress::Progress {
                    PairProgress::Progress
                } else {
                    PairProgress::Pending
                });
            }
            let completion = self
                .take_owned_filing(FilingOwner::Pair, ports.io)?
                .expect("complete pair filing still owned");
            self.accept_pair_filing(completion, ports.io)?;
            return Ok(if self.pairs.is_complete() {
                PairProgress::Complete
            } else {
                PairProgress::Progress
            });
        }
        if self.pairs.is_complete() {
            return Ok(PairProgress::Complete);
        }
        // Spec 05 permits overlapping openings but does not require them. This
        // execution owner has one receipt wait: finish its first pair before
        // another opening can acquire that wait or obstruct the first answer.
        let opening = if self.pairs.has_effective() {
            self.pairs.next_execution_opening()
        } else {
            self.pairs.next_opening()
        };
        let next = if let Some((call, record)) = opening {
            Some((
                call.clone(),
                PairRecordPhase::Opening,
                Payload::PairStarted(Box::new(mirror::pair_started(record))),
            ))
        } else if let Some((call, record)) = self.pairs.next_terminal() {
            Some((
                call.clone(),
                PairRecordPhase::Terminal,
                Payload::PairEnded(Box::new(mirror::pair_ended(record))),
            ))
        } else {
            None
        };
        let Some((call, phase, payload)) = next else {
            return Ok(if self.reduce_flow(ports.io)? == Advance::Progress {
                PairProgress::Progress
            } else {
                PairProgress::Pending
            });
        };
        let opening = phase == PairRecordPhase::Opening;
        let gate = !opening
            && self.pairs.terminal_is_gate(&call).map_err(|error| {
                self.pair_fault(ports.io, format!("pair attribution: {error:?}"))
            })?;
        let turn = self
            .pairs
            .original()
            .expect("active batch")
            .generation()
            .original()
            .turn
            .clone();
        let event = AuthoredEvent {
            kind: if opening {
                Kind::ToolCallStarted
            } else {
                Kind::ToolCallCompleted
            },
            subsystem: if gate {
                Subsystem::Gate
            } else {
                Subsystem::Harness
            },
            turn: Some(turn),
            source: if opening {
                AuthoredSource::Model
            } else if gate {
                AuthoredSource::Tool
            } else {
                AuthoredSource::Loop
            },
            causal_parent: Some(Sequence(call.sequence.0)),
            payload: Some(payload),
        };
        // A background measurement can still own the author between pair legs.
        // Busy starts no record and preserves the unclaimed candidate.
        if let Err(error) =
            self.begin_owned_filing(event, PumpDeadline::Decision, FilingOwner::Pair, ports.io)
        {
            if error.failure.is_none() && error.refusal == FilingBeginRefusal::Busy {
                return Ok(if self.reduce_flow(ports.io)? == Advance::Progress {
                    PairProgress::Progress
                } else {
                    PairProgress::Pending
                });
            }
            self.retain_refused(error.event);
            return Err(error.failure.unwrap_or_else(|| {
                self.pair_fault(
                    ports.io,
                    format!("pair filing could not start: {:?}", error.refusal),
                )
            }));
        }
        // begin only reserves custody, it submits no author ticket. Claim now,
        // before advance can record or any failure callback can observe progress.
        self.pairs
            .mark_authoring(&call, phase)
            .map_err(|error| self.pair_fault(ports.io, format!("pair author: {error:?}")))?;
        self.pair_work.as_mut().unwrap().leg = Some(PairLeg {
            call,
            phase,
            recorded: None,
        });
        Ok(PairProgress::Progress)
    }

    fn collect_pair_statement(&mut self, io: &mut impl PumpIo) -> Result<(), RunFailure> {
        let Some(source) = self.pairs.next_statement().cloned() else {
            return Ok(());
        };
        let lease = self
            .pairs
            .receipt_lease(&source)
            .expect("accepted source-specific private lease");
        let disposition = self
            .flow
            .take_source_receipt(lease, &source)
            .map_err(|error| self.pair_fault(io, format!("pair statement custody: {error}")))?;
        let Some(disposition) = disposition else {
            return Ok(());
        };
        let refused = match disposition {
            SourceDisposition::Committed(committed) => {
                match self.pairs.install_statement(committed) {
                    Ok(()) => return Ok(()),
                    Err((receipt, _)) => SourceDisposition::Committed(receipt),
                }
            }
            rejected @ SourceDisposition::CorrectlyRejected(_) => rejected,
        };
        self.pair_work.as_mut().unwrap().refused_receipt = Some(refused);
        Err(self.pair_fault(io, "pair dependency differs from the accepted original"))
    }

    pub(super) fn bind_pair_author(
        &mut self,
        sequence: Sequence,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        let Some(leg) = self.pair_work.as_mut().and_then(|work| work.leg.as_mut()) else {
            return Ok(());
        };
        if leg.recorded.is_some() {
            return Ok(());
        }
        let source = SourceBlockId {
            run: leg.call.run.clone(),
            sequence: sequence.0.into(),
            block: 0.into(),
        };
        leg.recorded = Some(source.clone());
        let result = self.pairs.mark_recorded(&leg.call, leg.phase, source);
        result.map_err(|error| self.pair_fault(io, format!("recorded pair source: {error:?}")))
    }

    fn accept_pair_filing(
        &mut self,
        completion: FilingCompletion,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        let leg = self.pair_work.as_ref().unwrap().leg.as_ref().unwrap();
        let terminal = leg.phase == PairRecordPhase::Terminal;
        if leg.recorded.as_ref().map(|source| source.sequence.0) != Some(completion.sequence().0)
            || completion.receipts().sources().count() != 1
        {
            self.pair_work.as_mut().unwrap().refused_filing = Some(completion);
            return Err(self.pair_fault(io, "pair filing did not select its one whole record"));
        }
        let receipt = completion
            .into_receipts()
            .into_dispositions()
            .expect("complete private filing receipts")
            .next()
            .expect("one pair source");
        let result = match receipt {
            SourceDisposition::Committed(committed) => {
                let leg = self.pair_work.as_ref().unwrap().leg.as_ref().unwrap();
                match leg.phase {
                    PairRecordPhase::Opening => self.pairs.install_opening(&leg.call, committed),
                    PairRecordPhase::Terminal => self.pairs.install_terminal(&leg.call, committed),
                }
            }
            rejected @ SourceDisposition::CorrectlyRejected(_) => {
                self.pair_work.as_mut().unwrap().refused_receipt = Some(rejected);
                return Err(self.pair_fault(io, "pair metadata acquired a conformance disposition"));
            }
        };
        if let Err((receipt, error)) = result {
            self.pair_work.as_mut().unwrap().refused_receipt =
                Some(SourceDisposition::Committed(receipt));
            return Err(self.pair_fault(io, format!("pair receipt mismatch: {error:?}")));
        }
        self.pair_work.as_mut().unwrap().leg = None;
        if terminal && self.pairs.has_effective() && !self.pairs.is_complete() {
            self.begin_next_pair_send_wait(io)?;
        }
        Ok(())
    }

    pub(in crate::flow) fn take_pair_accounting(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<Option<OriginalRequests>, RunFailure> {
        self.live(io)?;
        if self.repair_blocks_external() && self.repair_owns_pairs() {
            return Err(RunFailure::RepairOwned);
        }
        if self.pair_work.is_none() {
            return Ok(None);
        }
        if self.pairs.has_repair_account() || self.pairs.has_effective() {
            return Err(RunFailure::PairOwned);
        }
        self.pair_wait_matches(io)?;
        if !self.pairs.is_complete() {
            return Ok(None);
        }
        let original = self
            .pairs
            .finish_batch()
            .map_err(|error| self.pair_fault(io, format!("pair completion: {error:?}")))?;
        self.pair_work = None;
        Ok(Some(original))
    }

    pub(in crate::flow) fn take_repair_accounting(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<Option<RepairAccount>, RunFailure> {
        self.live(io)?;
        if self.repair_blocks_external() && self.repair_owns_pairs() {
            return Err(RunFailure::RepairOwned);
        }
        if self.pair_work.is_none() {
            return Ok(None);
        }
        if !self.pairs.has_repair_account() || self.pairs.has_effective() {
            return Err(RunFailure::PairOwned);
        }
        self.pair_wait_matches(io)?;
        if !self.pairs.is_complete() {
            return Ok(None);
        }
        let account = self
            .pairs
            .finish_repair_batch()
            .map_err(|error| self.pair_fault(io, format!("repair pair completion: {error:?}")))?;
        self.pair_work = None;
        Ok(Some(account))
    }

    pub(in crate::flow) fn completed_pair(
        &mut self,
        call: &CallId,
        io: &mut impl PumpIo,
    ) -> Result<Option<&CompletedPair>, PairReadError> {
        self.live(io).map_err(PairReadError::Run)?;
        self.pairs
            .completed(call)
            .map_err(|_| PairReadError::UnknownCall)
    }

    pub(in crate::flow) fn outstanding_pairs(&self) -> Vec<OutstandingPair> {
        let mut facts = self.pairs.outstanding();
        // A local commit may already be held when a subsequent clock or carry
        // fails. Report that fact without transferring a completion grant.
        if let Some(leg) = self.pair_work.as_ref().and_then(|work| work.leg.as_ref())
            && let Some(source) = &leg.recorded
            && self
                .flow
                .filing_receipts()
                .is_some_and(|set| set.has_recorded_commit(source))
            && let Some(fact) = facts.iter_mut().find(|fact| fact.call_id == leg.call)
        {
            match leg.phase {
                PairRecordPhase::Opening => fact.opening_filed = true,
                PairRecordPhase::Terminal => fact.terminal_filed = true,
            }
        }
        facts
    }

    pub(super) fn pair_fault(
        &mut self,
        io: &mut impl PumpIo,
        detail: impl Into<String>,
    ) -> RunFailure {
        self.fail(
            io,
            FlowHaltReason::ProtocolFailed,
            HaltPoint::Barrier,
            detail,
        )
    }
}
