//! The recorded, committed lifecycle of accepted requests barred from execution.
//! The loop supplies the cause. This owner performs no decoder pass, gate call,
//! retry, placement or tool clock. Receipt waits borrow one original Decision bound.

use super::pairs::{CompletedPair, PairRecordPhase};
use super::receipt_driver::FilingOwner;
use super::*;
use crate::flow::author::AuthoredSource;
use crate::flow::coordinator::OriginalRequests;
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

struct PairLeg {
    call: CallId,
    phase: PairRecordPhase,
    recorded: Option<SourceBlockId>,
}

pub(super) struct PairTransaction {
    bound: AbsoluteBound,
    leg: Option<PairLeg>,
    // A failed receipt check must not drop the exact evidence it rejected.
    refused_receipt: Option<SourceDisposition>,
    refused_filing: Option<FilingCompletion>,
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
        let refusal = if self.phase != RecordingPhase::Entered
            || self.flow.phase() != CoordinatorPhase::Active
            || !self.barrier.is_active()
        {
            Some(PairBeginRefusal::WrongPhase)
        } else if self.pair_work.is_some()
            || self.output.is_some()
            || self.filing.is_some()
            || self.templates_block_content()
            || self.template_blocked_author.is_some()
            || self.input.snapshot().is_some()
            || self.input_turn.is_some()
            || self.input_author.is_some()
            || self.member_capture.is_some()
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
        } else if original.generation().scheme() != &self.declaration.scheme
            || original.generation().source().block_id.run != self.flow.state().identity().1
            || self.open_turn.as_ref() != Some(&original.generation().original().turn)
        {
            Some(PairBeginRefusal::WrongGeneration)
        } else if !self.pair_custody_matches(&original) {
            Some(PairBeginRefusal::LostCustody)
        } else {
            None
        };
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
        self.pair_work = Some(PairTransaction {
            bound: self
                .deadlines
                .get(PumpDeadline::Decision)
                .expect("checked bound"),
            leg: None,
            refused_receipt: None,
            refused_filing: None,
        });
        // accept establishes the active batch. A failure here is an internal
        // contract error, with the batch still retained in the frozen owner.
        self.pairs
            .set_not_dispatched(cause)
            .expect("new healthy batch");
        Ok(())
    }

    fn pair_custody_matches(&self, original: &OriginalRequests) -> bool {
        let Some(lease) = original.generation().receipt_lease() else {
            return false;
        };
        let Ok(sources) = self.flow.ledger().source_lease_sources(lease) else {
            return false;
        };
        let targets = original
            .generation()
            .blocks()
            .iter()
            .filter(|check| check.selected())
            .map(|check| &check.request().target);
        if !sources
            .map(|id| Ste100Target::Source(id.clone()))
            .eq(targets.cloned())
        {
            return false;
        }
        original.dependencies().all(|statement| {
            matches!(
                self.flow.source_receipt_status(lease, &statement.block_id),
                Ok(LeaseSlotStatus::Pending | LeaseSlotStatus::Ready)
            )
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
        self.pairs
            .set_not_dispatched(cause)
            .map_err(|error| self.pair_fault(io, format!("pair disposition: {error:?}")))
    }

    pub(super) fn pair_wait_still_owned(&self, key: PumpDeadline) -> bool {
        key == PumpDeadline::Decision && self.pair_work.is_some() && !self.pairs.is_complete()
    }

    fn pair_wait_matches(&mut self, io: &mut impl PumpIo) -> Result<(), RunFailure> {
        let Some(work) = &self.pair_work else {
            return Err(RunFailure::PairOwned);
        };
        let actual = self.deadlines.get(PumpDeadline::Decision);
        if actual != Some(work.bound) && !(actual.is_none() && self.pairs.is_complete()) {
            return Err(self.pair_fault(io, "pair accounting lost its original wait"));
        }
        Ok(())
    }

    pub(in crate::flow) fn advance_pairs<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<PairProgress, RunFailure> {
        self.live(ports.io)?;
        self.pair_wait_matches(ports.io)?;
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
        let next = if let Some((call, record)) = self.pairs.next_opening() {
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
            subsystem: Subsystem::Harness,
            turn: Some(turn),
            source: if opening {
                AuthoredSource::Model
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
        let lease = self.pairs.receipt_lease().expect("accepted private lease");
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
        Ok(())
    }

    pub(in crate::flow) fn take_pair_accounting(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<Option<OriginalRequests>, RunFailure> {
        self.live(io)?;
        if self.pair_work.is_none() {
            return Ok(None);
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

    fn pair_fault(&mut self, io: &mut impl PumpIo, detail: impl Into<String>) -> RunFailure {
        self.fail(
            io,
            FlowHaltReason::ProtocolFailed,
            HaltPoint::Barrier,
            detail,
        )
    }
}
