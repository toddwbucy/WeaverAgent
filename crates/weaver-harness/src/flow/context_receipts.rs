//! One explicitly awaited content author and its exact selected filing outcomes.
//! The caller owns the enclosing Decision or Cut wait. This owner never elects,
//! renews or disarms that bound, and an empty projection grants no committed source.

use super::*;
use crate::flow::ledger::ReceiptSet;

#[path = "context_boot_filing_expired.rs"]
mod expired;
pub(in crate::flow) use expired::BootFilingRollbackPermit;
pub(in crate::flow::context) use expired::StoppedBootFilingAccount;

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
    origin: std::sync::Arc<()>,
    receipt_identity: std::sync::Arc<()>,
    key: PumpDeadline,
    bound: AbsoluteBound,
    sequence: Sequence,
    recorded_ns: u64,
    receipts: ReceiptSet,
}

impl FilingCompletion {
    pub(in crate::flow) fn belongs_to(&self, context: &RecordingContext) -> bool {
        std::sync::Arc::ptr_eq(&self.origin, &context.read_origin)
    }
    pub(super) fn receipt_identity(&self) -> &std::sync::Arc<()> {
        &self.receipt_identity
    }
    pub(super) fn wait_key(&self) -> PumpDeadline {
        self.key
    }
    pub(super) fn wait_bound(&self) -> AbsoluteBound {
        self.bound
    }
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
    receipt_identity: std::sync::Arc<()>,
    plan_effect: bool,
    pub(super) outbound: Option<super::sort_driver::OutboundWork>,
    owner: FilingOwner,
    key: PumpDeadline,
    bound: AbsoluteBound,
    sequence: Option<Sequence>,
    author_done: bool,
    returned_author: StoppedAuthor,
    cancelled: bool,
    cancelled_event: Option<AuthoredEvent>,
    cancelled_receipts: Option<ReceiptSet>,
}

impl FilingTransaction {
    pub(super) fn owns_recorded_pair_terminal(
        &self,
        source: &weaver_types::state_flow::SourceBlockId,
    ) -> bool {
        self.owner == FilingOwner::Pair
            && !self.cancelled
            && self.author_done
            && self.sequence == Some(Sequence(source.sequence.0))
    }

    pub(super) fn is_boot_cleanup_filing(&self) -> bool {
        self.owner == FilingOwner::BootPairs
    }

    pub(super) fn cancelled(&self) -> bool {
        self.cancelled
    }
    pub(super) fn receipt_identity(&self) -> &std::sync::Arc<()> {
        &self.receipt_identity
    }
    pub(super) fn wait_key(&self) -> PumpDeadline {
        self.key
    }
    pub(super) fn wait_bound(&self) -> AbsoluteBound {
        self.bound
    }
}
pub(super) fn is_plan_effect_event(event: &AuthoredEvent) -> bool {
    matches!(event.payload.as_ref(), Some(Payload::PlanEffect(_)))
        || matches!(event.payload.as_ref(), Some(Payload::StepOutcome(record)) if record.context.is_some())
}

impl RecordingContext {
    pub(super) fn plan_effect_inflight(&self) -> bool {
        self.filing.as_ref().is_some_and(|owner| owner.plan_effect)
    }
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
        // Boot must establish even the committed-null summary before a plan
        // effect can create a refresh duty. Return the unchanged event before
        // reserving a receipt subscription or admitting any author input.
        let plan_effect = is_plan_effect_event(&event);
        let owned_continuation = match owner {
            FilingOwner::Pair => self.pair_work.as_ref().is_some_and(|work| {
                work.leg.is_none() && work.bound == bound && key == PumpDeadline::Decision
            }),
            FilingOwner::Marks => self.marks_filing_allowed(key),
            FilingOwner::BootPairs => self.boot_pairs_filing_allowed(),
            FilingOwner::Ordinary | FilingOwner::Outbound => false,
        };
        if (plan_effect && (!self.plan_cache.boot_started || self.plan_cache.committed.is_none()))
            || (self.boot_pairs.is_some() && owner != FilingOwner::BootPairs)
            || (owner == FilingOwner::BootPairs && !self.boot_pairs_filing_allowed())
            || self.output.is_some()
            || self.freshness.is_some()
            || self.filing.is_some()
            || self.flow.has_filing_consumer()
            || self.templates_block_content()
            || self.caller_author_blocks_continuation(|| owned_continuation)
            || self.input.snapshot().is_some()
            || self.input_turn.is_some()
            || self.input_author.is_some()
            || self.member_capture.is_some()
            || self.read.is_some()
            || self.inspection.is_some()
            || self.boot_open_blocks_external()
            || self.plan_summary_blocks_external()
            || (self.place_blocks_external()
                && !(self.reload_draining()
                    && matches!(owner,FilingOwner::Pair if self.pair_work.is_some())
                    || self.reload_draining()
                        && matches!(owner,FilingOwner::Marks if self.marks.is_some())))
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
            receipt_identity: std::sync::Arc::new(()),
            plan_effect,
            outbound: None,
            owner,
            key,
            bound: bound.expect("existing borrowed bound"),
            sequence: None,
            author_done: false,
            returned_author: StoppedAuthor::default(),
            cancelled: false,
            cancelled_event: None,
            cancelled_receipts: None,
        });
        // The unchanged original remains in the context's established pre-ticket
        // custody slot, including when older filings delay this author.
        self.waiting_content = Some(WaitingAuthor::ordinary(event));
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
        let event = self.waiting_content.take().map(WaitingAuthor::into_event);
        self.flow.cancel_filing_receipts();
        self.filing = None;
        event
    }

    pub(super) fn filing_wait_still_owned(&self, key: PumpDeadline) -> bool {
        self.filing.as_ref().is_some_and(|owner| {
            if owner.key != key {
                return false;
            }
            if owner.plan_effect && owner.cancelled_event.is_none() {
                return true;
            }
            let settled = owner.outbound.is_none()
                && owner.author_done
                && self
                    .flow
                    .filing_receipts()
                    .is_some_and(ReceiptSet::is_complete)
                && self
                    .flow
                    .filing_recorded_ns()
                    .is_some_and(|at| !owner.bound.expired(at));
            !(owner.cancelled || settled)
        })
    }

    fn filing_wait_matches(&mut self, io: &mut impl PumpIo) -> Result<(), RunFailure> {
        let owner = self.filing.as_ref().ok_or(RunFailure::Closed)?;
        let current = self.deadlines.get(owner.key);
        let retired_after_completion = !owner.plan_effect
            && current.is_none()
            && owner.author_done
            && self
                .flow
                .filing_receipts()
                .is_some_and(ReceiptSet::is_complete)
            && self
                .flow
                .filing_recorded_ns()
                .is_some_and(|at| !owner.bound.expired(at));
        if (!owner.cancelled || (owner.plan_effect && owner.cancelled_event.is_none()))
            && current != Some(owner.bound)
            && !retired_after_completion
        {
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
        let plan_effect = owner.plan_effect;
        self.filing.as_mut().unwrap().sequence = Some(sequence);
        if !cancelled || plan_effect {
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
        owner.returned_author.check()?;
        if !owner.author_done {
            let sequence = if let Some(sequence) = self.completed_author.take() {
                sequence
            } else if self.waiting_content.is_some() {
                self.continue_waiting_content(ports).map_err(|failure| {
                    self.filing
                        .as_mut()
                        .unwrap()
                        .returned_author
                        .retain(failure)
                })?
            } else if self.barrier.pending_author().is_some() && !self.flow.has_pending_record() {
                let progress =
                    self.barrier
                        .resume_with_clock(&mut self.inbox, &mut self.budget, &mut || {
                            ports.io.now()
                        });
                self.wait_recorded(ports, progress).map_err(|failure| {
                    self.filing
                        .as_mut()
                        .unwrap()
                        .returned_author
                        .retain(failure)
                })?
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
        if owner.cancelled && (!owner.plan_effect || owner.cancelled_event.is_some()) {
            return Ok(FilingProgress::Cancelled);
        }
        if self
            .flow
            .filing_receipts()
            .is_some_and(ReceiptSet::is_complete)
        {
            return Ok(if owner.cancelled {
                FilingProgress::Cancelled
            } else {
                FilingProgress::Complete
            });
        }
        let advance = self.reduce_flow(ports.io)?;
        if self
            .flow
            .filing_receipts()
            .is_some_and(ReceiptSet::is_complete)
        {
            Ok(if self.filing.as_ref().unwrap().cancelled {
                FilingProgress::Cancelled
            } else {
                FilingProgress::Complete
            })
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
        let receipt_identity = owner.receipt_identity.clone();
        let key = owner.key;
        let bound = owner.bound;
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
        self.observe_plan_effect_receipts();
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
            origin: self.read_origin.clone(),
            receipt_identity,
            key,
            bound,
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
        let event = self.waiting_content.take().map(WaitingAuthor::into_event);
        self.flow.cancel_filing_receipts();
        self.filing = None;
        event
    }

    /// Stop/Leave cancellation is terminal, including an unstarted effect whose
    /// event is returned unchanged. This is not a retry/withdraw operation: a
    /// future non-terminal withdrawal must preserve an unchanged committed cache.
    pub(in crate::flow) fn cancel_filing(&mut self) {
        let Some(owner) = self.filing.as_mut() else {
            return;
        };
        if owner.owner != FilingOwner::Ordinary || owner.cancelled {
            return;
        }
        owner.cancelled = true;
        let unstarted = owner.sequence.is_none() && self.waiting_content.is_some();
        if owner.plan_effect {
            // A late member commit must retain its receipt subscription even
            // after the consumer cancels. Revocation is not a claimed commit.
            self.plan_cache.revoke();
        }
        if !owner.plan_effect || unstarted {
            owner.cancelled_receipts = self.flow.cancel_filing_receipts();
        }
        if unstarted {
            // No author ticket was allocated. Return the unchanged caller input.
            owner.cancelled_event = self.waiting_content.take().map(WaitingAuthor::into_event);
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
        let retained_plan = self
            .filing
            .as_ref()
            .is_some_and(|owner| owner.plan_effect && owner.cancelled_event.is_none());
        if retained_plan {
            if !self
                .flow
                .filing_receipts()
                .is_some_and(ReceiptSet::is_complete)
                || self.flow.has_pending_record()
                || self.barrier.pending().is_some()
            {
                return None;
            }
            self.observe_plan_effect_receipts();
            self.filing.as_mut().unwrap().cancelled_receipts = self.flow.cancel_filing_receipts();
            // The cancelled account owns the exact committed facts. It never
            // reopens the previous cache or licenses another effect/refresh.
            self.plan_cache.required = None;
            self.plan_cache.revoke();
        }
        let owner = self.filing.take().expect("completed cancellation");
        Some(FilingCancellation {
            sequence: owner.sequence,
            event: owner.cancelled_event,
            receipts: owner.cancelled_receipts,
        })
    }
}
