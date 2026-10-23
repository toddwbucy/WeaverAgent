//! One explicitly awaited content author and its exact selected filing outcomes.
//! The caller owns the enclosing Decision or Cut wait. This owner never elects,
//! renews or disarms that bound, and an empty projection grants no committed source.

use super::*;
use crate::flow::ledger::ReceiptSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum FilingBeginRefusal {
    WrongPhase,
    Busy,
    MissingWait,
    NotContent,
    ReservedPair,
}

#[derive(Debug)]
pub(in crate::flow) struct FilingBeginError {
    pub event: AuthoredEvent,
    pub refusal: FilingBeginRefusal,
    pub failure: Option<RunFailure>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum FilingProgress {
    Progress,
    Pending,
    Complete,
    Cancelled,
}

#[derive(Debug)]
pub(in crate::flow) struct FilingCompletion {
    sequence: Sequence,
    recorded_ns: u64,
    receipts: ReceiptSet,
}

impl FilingCompletion {
    pub(in crate::flow) fn sequence(&self) -> Sequence {
        self.sequence
    }
    pub(in crate::flow) fn recorded_ns(&self) -> u64 {
        self.recorded_ns
    }
    pub(in crate::flow) fn receipts(&self) -> &ReceiptSet {
        &self.receipts
    }
    pub(in crate::flow) fn into_receipts(self) -> ReceiptSet {
        self.receipts
    }
}

/// Cancellation returns custody and evidence, never a fulfilled filing grant.
#[derive(Debug)]
pub(in crate::flow) struct FilingCancellation {
    pub sequence: Option<Sequence>,
    pub event: Option<AuthoredEvent>,
    pub receipts: Option<ReceiptSet>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FilingOwner {
    Ordinary,
    Pair,
    Outbound,
    Marks,
    BootPairs,
}

pub(super) struct FilingTransaction {
    pub(super) outbound: Option<super::sort_driver::OutboundWork>,
    owner: FilingOwner,
    key: PumpDeadline,
    bound: AbsoluteBound,
    sequence: Option<Sequence>,
    author_done: bool,
    cancelled: bool,
    cancelled_event: Option<AuthoredEvent>,
    cancelled_receipts: Option<ReceiptSet>,
}

impl RecordingContext {
    // A refused operation returns the original event without an error-path allocation.
    #[allow(clippy::result_large_err)]
    pub(in crate::flow) fn begin_filing(
        &mut self,
        event: AuthoredEvent,
        key: PumpDeadline,
        io: &mut impl PumpIo,
    ) -> Result<(), FilingBeginError> {
        if self.boot_pairs.is_some() || self.marks.is_some() || self.repair_blocks_external() {
            return Err(FilingBeginError {
                event,
                refusal: FilingBeginRefusal::Busy,
                failure: None,
            });
        }
        self.begin_owned_filing(event, key, FilingOwner::Ordinary, io)
    }

    #[allow(clippy::result_large_err)]
    pub(super) fn begin_owned_filing(
        &mut self,
        event: AuthoredEvent,
        key: PumpDeadline,
        owner: FilingOwner,
        io: &mut impl PumpIo,
    ) -> Result<(), FilingBeginError> {
        if owner == FilingOwner::Ordinary
            && matches!(event.kind, Kind::ToolCallStarted | Kind::ToolCallCompleted)
        {
            return Err(FilingBeginError {
                event,
                refusal: FilingBeginRefusal::ReservedPair,
                failure: None,
            });
        }
        if owner == FilingOwner::Ordinary && self.pair_work.is_some() {
            return Err(FilingBeginError {
                event,
                refusal: FilingBeginRefusal::Busy,
                failure: None,
            });
        }
        if let Err(failure) = self.live(io) {
            return Err(FilingBeginError {
                event,
                refusal: FilingBeginRefusal::WrongPhase,
                failure: Some(failure),
            });
        }
        if self.phase != RecordingPhase::Entered
            || !self.barrier.is_active()
            || self.flow.phase() != CoordinatorPhase::Active
            || !self.author_allowed(&event)
        {
            return Err(FilingBeginError {
                event,
                refusal: FilingBeginRefusal::WrongPhase,
                failure: None,
            });
        }
        if serving::content_schema(event.kind) == ContentSchema::None {
            return Err(FilingBeginError {
                event,
                refusal: FilingBeginRefusal::NotContent,
                failure: None,
            });
        }
        let bound = self.deadlines.get(key);
        if !matches!(key, PumpDeadline::Decision | PumpDeadline::Cut) || bound.is_none() {
            return Err(FilingBeginError {
                event,
                refusal: FilingBeginRefusal::MissingWait,
                failure: None,
            });
        }
        if (self.boot_pairs.is_some() && owner != FilingOwner::BootPairs)
            || (owner == FilingOwner::BootPairs && !self.boot_pairs_filing_allowed())
            || self.output.is_some()
            || self.freshness.is_some()
            || self.filing.is_some()
            || self.flow.has_filing_consumer()
            || self.templates_block_content()
            || self.template_blocked_author.is_some()
            || self.input.snapshot().is_some()
            || self.input_turn.is_some()
            || self.input_author.is_some()
            || self.member_capture.is_some()
            || self.read.is_some()
            || self.inspection.is_some()
            || self.boot_open_blocks_external()
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
            return Err(FilingBeginError {
                event,
                refusal: FilingBeginRefusal::Busy,
                failure: None,
            });
        }
        let reserved = self.flow.reserve_filing_receipts(&mut FlowRecording {
            barrier: &mut self.barrier,
            inbox: &mut self.inbox,
            budget: &mut self.budget,
            author: &self.author,
        });
        if let Err(error) = reserved {
            return Err(FilingBeginError {
                event,
                refusal: FilingBeginRefusal::WrongPhase,
                failure: Some(self.flow_failed(io, error)),
            });
        }
        self.filing = Some(FilingTransaction {
            outbound: None,
            owner,
            key,
            bound: bound.expect("existing borrowed bound"),
            sequence: None,
            author_done: false,
            cancelled: false,
            cancelled_event: None,
            cancelled_receipts: None,
        });
        // The unchanged original remains in the context's established pre-ticket
        // custody slot, including when older filings delay this author.
        self.waiting_content = Some(event);
        Ok(())
    }

    pub(super) fn owned_filing_recorded_before(
        &self,
        owner: FilingOwner,
        bound: AbsoluteBound,
    ) -> bool {
        self.filing.as_ref().is_some_and(|filing| {
            filing.owner == owner
                && filing.bound == bound
                && filing.author_done
                && !filing.cancelled
        }) && self
            .flow
            .filing_receipts()
            .is_some_and(ReceiptSet::is_complete)
            && self
                .flow
                .filing_recorded_ns()
                .is_some_and(|at| !bound.expired(at))
    }

    pub(super) fn withdraw_unstarted_boot_pair(&mut self) -> Option<AuthoredEvent> {
        let owner = self.filing.as_ref()?;
        if owner.owner != FilingOwner::BootPairs
            || owner.sequence.is_some()
            || self.barrier.pending_author().is_some()
            || self.waiting_content.is_none()
        {
            return None;
        }
        let event = self.waiting_content.take();
        self.flow.cancel_filing_receipts();
        self.filing = None;
        event
    }

    pub(super) fn filing_wait_still_owned(&self, key: PumpDeadline) -> bool {
        self.filing.as_ref().is_some_and(|owner| {
            owner.key == key
                && !owner.cancelled
                && !(owner.outbound.is_none()
                    && owner.author_done
                    && self
                        .flow
                        .filing_receipts()
                        .is_some_and(ReceiptSet::is_complete)
                    && self
                        .flow
                        .filing_recorded_ns()
                        .is_some_and(|at| !owner.bound.expired(at)))
        })
    }

    fn filing_wait_matches(&mut self, io: &mut impl PumpIo) -> Result<(), RunFailure> {
        let owner = self.filing.as_ref().ok_or(RunFailure::Closed)?;
        let current = self.deadlines.get(owner.key);
        let retired_after_completion = current.is_none()
            && owner.author_done
            && self
                .flow
                .filing_receipts()
                .is_some_and(ReceiptSet::is_complete)
            && self
                .flow
                .filing_recorded_ns()
                .is_some_and(|at| !owner.bound.expired(at));
        if !owner.cancelled && current != Some(owner.bound) && !retired_after_completion {
            return Err(self.fail(
                io,
                FlowHaltReason::ProtocolFailed,
                HaltPoint::Barrier,
                "filing consumer's enclosing wait was removed or replaced",
            ));
        }
        Ok(())
    }

    /// Hook at the original recorder completion, before live/deadline checks can
    /// return control and before any selected result can be reduced.
    pub(super) fn bind_filing_author(
        &mut self,
        sequence: Sequence,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        if self.bind_outbound_fault(sequence) {
            return Ok(());
        }
        let Some(owner) = self.filing.as_ref() else {
            return Ok(());
        };
        if owner.sequence.is_some() || owner.author_done {
            return Err(self.fail(
                io,
                FlowHaltReason::ProtocolFailed,
                HaltPoint::Author,
                "another author completed inside a filing transaction",
            ));
        }
        let cancelled = owner.cancelled;
        self.filing.as_mut().unwrap().sequence = Some(sequence);
        if !cancelled {
            let recorded_ns = io.now().map_err(|error| {
                self.fail(
                    io,
                    FlowHaltReason::ProtocolFailed,
                    HaltPoint::Barrier,
                    error.to_string(),
                )
            })?;
            let result = self.flow.bind_filing_receipts(
                &mut FlowRecording {
                    barrier: &mut self.barrier,
                    inbox: &mut self.inbox,
                    budget: &mut self.budget,
                    author: &self.author,
                },
                sequence,
                recorded_ns,
            );
            result.map_err(|error| self.flow_failed(io, error))?;
        }
        Ok(())
    }

    pub(in crate::flow) fn advance_filing<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<FilingProgress, RunFailure> {
        self.advance_owned_filing(FilingOwner::Ordinary, ports)
    }

    pub(super) fn advance_owned_filing<I: PumpIo>(
        &mut self,
        owner: FilingOwner,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<FilingProgress, RunFailure> {
        if self
            .filing
            .as_ref()
            .is_some_and(|filing| filing.owner != owner)
        {
            return Err(RunFailure::PairOwned);
        }
        self.live(ports.io)?;
        self.filing_wait_matches(ports.io)?;
        let owner = self.filing.as_ref().expect("checked owner");
        if !owner.author_done {
            let sequence = if let Some(sequence) = self.completed_author.take() {
                sequence
            } else if self.waiting_content.is_some() {
                self.continue_waiting_content(ports)?
            } else if self.barrier.pending_author().is_some() && !self.flow.has_pending_record() {
                let progress =
                    self.barrier
                        .resume_with_clock(&mut self.inbox, &mut self.budget, &mut || {
                            ports.io.now()
                        });
                self.wait_recorded(ports, progress)?
            } else {
                return Err(self.fail(
                    ports.io,
                    FlowHaltReason::ProtocolFailed,
                    HaltPoint::Author,
                    "filing transaction lost its original author",
                ));
            };
            let owner = self.filing.as_mut().unwrap();
            if owner.sequence != Some(sequence) {
                return Err(self.fail(
                    ports.io,
                    FlowHaltReason::ProtocolFailed,
                    HaltPoint::Author,
                    "filing author returned another source",
                ));
            }
            owner.author_done = true;
            return Ok(if owner.cancelled {
                FilingProgress::Cancelled
            } else {
                FilingProgress::Progress
            });
        }
        if owner.cancelled {
            return Ok(FilingProgress::Cancelled);
        }
        if self
            .flow
            .filing_receipts()
            .is_some_and(ReceiptSet::is_complete)
        {
            return Ok(FilingProgress::Complete);
        }
        let advance = self.reduce_flow(ports.io)?;
        if self
            .flow
            .filing_receipts()
            .is_some_and(ReceiptSet::is_complete)
        {
            Ok(FilingProgress::Complete)
        } else if advance == Advance::Progress {
            Ok(FilingProgress::Progress)
        } else {
            Ok(FilingProgress::Pending)
        }
    }

    pub(in crate::flow) fn take_filing(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<Option<FilingCompletion>, RunFailure> {
        self.take_owned_filing(FilingOwner::Ordinary, io)
    }

    pub(super) fn take_owned_filing(
        &mut self,
        owner: FilingOwner,
        io: &mut impl PumpIo,
    ) -> Result<Option<FilingCompletion>, RunFailure> {
        let result = self.collect_owned_filing(owner, io)?;
        if result.is_some() {
            self.filing = None;
        }
        Ok(result)
    }

    /// Collect evidence while an outbound owner retains the exclusive author
    /// slot for an owed runtime-template fault.
    pub(super) fn collect_owned_filing(
        &mut self,
        owner: FilingOwner,
        io: &mut impl PumpIo,
    ) -> Result<Option<FilingCompletion>, RunFailure> {
        if self
            .filing
            .as_ref()
            .is_some_and(|filing| filing.owner != owner)
        {
            return Err(RunFailure::PairOwned);
        }
        if self.filing.is_none() {
            self.live(io)?;
            return Ok(None);
        }
        self.live(io)?;
        self.filing_wait_matches(io)?;
        let owner = self.filing.as_ref().expect("checked owner");
        if owner.cancelled || !owner.author_done {
            return Ok(None);
        }
        let sequence = owner.sequence.expect("recorded original");
        if !self
            .flow
            .filing_receipts()
            .is_some_and(ReceiptSet::is_complete)
        {
            return Ok(None);
        }
        let recorded_ns = self.flow.filing_recorded_ns().ok_or_else(|| {
            self.fail(
                io,
                FlowHaltReason::ProtocolFailed,
                HaltPoint::Barrier,
                "complete filing lost its Recorded timestamp",
            )
        })?;
        let result = self.flow.take_filing_receipts(&mut FlowRecording {
            barrier: &mut self.barrier,
            inbox: &mut self.inbox,
            budget: &mut self.budget,
            author: &self.author,
        });
        let Some(receipts) = result.map_err(|error| self.flow_failed(io, error))? else {
            return Ok(None);
        };
        Ok(Some(FilingCompletion {
            sequence,
            recorded_ns,
            receipts,
        }))
    }

    /// Withdraw only an original that has not reached the recorder ticket.
    /// A pending/admitted author keeps its owed filing even during cancellation.
    pub(super) fn withdraw_unstarted_mark(&mut self) -> Option<AuthoredEvent> {
        let owner = self.filing.as_ref()?;
        if owner.owner != FilingOwner::Marks
            || owner.sequence.is_some()
            || self.barrier.pending_author().is_some()
            || self.waiting_content.is_none()
        {
            return None;
        }
        let event = self.waiting_content.take();
        self.flow.cancel_filing_receipts();
        self.filing = None;
        event
    }

    pub(in crate::flow) fn cancel_filing(&mut self) {
        let Some(owner) = self.filing.as_mut() else {
            return;
        };
        if owner.owner != FilingOwner::Ordinary || owner.cancelled {
            return;
        }
        owner.cancelled = true;
        owner.cancelled_receipts = self.flow.cancel_filing_receipts();
        if owner.sequence.is_none() && self.waiting_content.is_some() {
            // No author ticket was allocated. Return the unchanged caller input.
            owner.cancelled_event = self.waiting_content.take();
            owner.author_done = true;
        }
    }

    pub(in crate::flow) fn take_cancelled_filing(&mut self) -> Option<FilingCancellation> {
        if !self
            .filing
            .as_ref()
            .is_some_and(|r| r.owner == FilingOwner::Ordinary && r.cancelled && r.author_done)
        {
            return None;
        }
        let owner = self.filing.take().expect("completed cancellation");
        Some(FilingCancellation {
            sequence: owner.sequence,
            event: owner.cancelled_event,
            receipts: owner.cancelled_receipts,
        })
    }
}
