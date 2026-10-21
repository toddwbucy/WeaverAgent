//! Whole-copy ownership and source accounting under HARNESS-RECORD/BACKLOG.
//! The coordinator owns the ledger, trace completion proofs and halt latch.

use std::collections::{HashMap, VecDeque};

use weaver_trace::serving::{AdmittedCopy, SelectedBlock};
use weaver_types::state_flow::{FlowBacklog, SourceEnvelope};

use super::RecordedTerminal;

#[derive(Debug, PartialEq, Eq, Hash)]
pub(super) struct SourceEventKey {
    pub(super) run: String,
    pub(super) sequence: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct BlockCharge {
    pub(super) block: u64,
    pub(super) bytes: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct CopyReservation {
    pub(super) shared_bytes: u64,
    pub(super) blocks: Vec<BlockCharge>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum IntakeError {
    InvalidLimits,
    ClockReadFailed,
    InvalidEnvelope,
    InvalidIdentity,
    DuplicateBlock,
    DuplicateReservation,
    UnknownBlock,
    CounterOverflow,
    AllocationFailed,
    WaitingOccupied,
    FailureOccupied,
    Frozen,
}

impl std::fmt::Display for IntakeError {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.write_str(match self {
            Self::InvalidLimits => "invalid declared flow backlog",
            Self::ClockReadFailed => "copy intake monotonic clock failed",
            Self::InvalidEnvelope => "selected copy has an invalid source envelope",
            Self::InvalidIdentity => "selected copy identities disagree",
            Self::DuplicateBlock => "selected block identities repeat or are out of order",
            Self::DuplicateReservation => "source event already has a live reservation",
            Self::UnknownBlock => "terminal disposition names no charged block",
            Self::CounterOverflow => "retention counter overflow or underflow",
            Self::AllocationFailed => "whole-copy reservation allocation failed",
            Self::WaitingOccupied => "a second copy arrived while backlog was waiting",
            Self::FailureOccupied => "a copy arrived after an unhandled intake failure",
            Self::Frozen => "retention accounting is frozen for halt",
        })
    }
}

impl std::error::Error for IntakeError {}

#[derive(Debug)]
pub(super) struct IntakeFailure {
    pub(super) copy: AdmittedCopy,
    pub(super) detail: IntakeError,
}

#[must_use = "the record must preserve a returned failed copy"]
#[derive(Debug)]
pub(super) enum IntakeOutcome {
    Accepted,
    Deferred,
    IntakeFailed {
        copy: AdmittedCopy,
        detail: IntakeError,
    },
}

/// Source charges persist while a copy moves from the inbox into the ledger.
/// Waiting and failed copies are held separately and do not pretend to fit.
/// Live reservations reject duplicates. The coordinator's source-order check
/// also refuses replay after a terminal event's reservation has been removed.
#[derive(Debug)]
pub(super) struct RetentionBudget {
    limits: FlowBacklog,
    blocks: u64,
    bytes: u64,
    reservations: HashMap<SourceEventKey, CopyReservation>,
    waiting_copy: Option<AdmittedCopy>,
    waiting_intake_ns: Option<u64>,
    frozen: bool,
}

impl RetentionBudget {
    pub(super) fn new(limits: FlowBacklog) -> Result<Self, IntakeError> {
        limits.validate().map_err(|_| IntakeError::InvalidLimits)?;
        Ok(Self {
            limits,
            blocks: 0,
            bytes: 0,
            reservations: HashMap::new(),
            waiting_copy: None,
            waiting_intake_ns: None,
            frozen: false,
        })
    }

    pub(super) fn gauges(&self) -> (u64, u64) {
        (self.blocks, self.bytes)
    }

    pub(super) fn is_frozen(&self) -> bool {
        self.frozen
    }

    pub(super) fn limits(&self) -> &FlowBacklog {
        &self.limits
    }

    pub(super) fn waiting_copy(&self) -> Option<&AdmittedCopy> {
        self.waiting_copy.as_ref()
    }

    pub(super) fn reservation(&self, key: &SourceEventKey) -> Option<&CopyReservation> {
        self.reservations.get(key)
    }

    /// This records the coordinator's halt decision, and makes no such decision.
    pub(super) fn freeze(&mut self) {
        self.frozen = true;
    }

    /// Only the coordinator can mint this proof after the terminal trace record.
    /// A receipt, classification answer or socket write cannot release a charge.
    pub(super) fn release(&mut self, recorded: RecordedTerminal) -> Result<(), IntakeError> {
        if self.frozen {
            return Err(IntakeError::Frozen);
        }
        let source = recorded.into_source();
        let key = SourceEventKey {
            run: source.run,
            sequence: source.sequence.0,
        };
        let reservation = self
            .reservations
            .get(&key)
            .ok_or(IntakeError::UnknownBlock)?;
        let index = reservation
            .blocks
            .iter()
            .position(|charge| charge.block == source.block.0)
            .ok_or(IntakeError::UnknownBlock)?;
        let last = reservation.blocks.len() == 1;
        let released_bytes = reservation.blocks[index]
            .bytes
            .checked_add(if last { reservation.shared_bytes } else { 0 })
            .ok_or(IntakeError::CounterOverflow)?;
        let blocks = self
            .blocks
            .checked_sub(1)
            .ok_or(IntakeError::CounterOverflow)?;
        let bytes = self
            .bytes
            .checked_sub(released_bytes)
            .ok_or(IntakeError::CounterOverflow)?;
        // Nothing changes until every identity and counter has passed.
        if last {
            self.reservations.remove(&key);
        } else {
            self.reservations
                .get_mut(&key)
                .expect("the checked reservation is still held")
                .blocks
                .remove(index);
        }
        self.blocks = blocks;
        self.bytes = bytes;
        Ok(())
    }
}

/// These are notifications and owned copies, never a second failure latch.
#[derive(Debug, Default)]
pub(super) struct FlowInbox {
    copies: VecDeque<(AdmittedCopy, Option<u64>)>,
    failure: Option<IntakeFailure>,
    #[cfg(test)]
    fail_allocation: Option<AllocationPoint>,
}

impl FlowInbox {
    pub(super) fn new() -> Self {
        Self::default()
    }

    pub(super) fn accept(
        &mut self,
        copy: AdmittedCopy,
        budget: &mut RetentionBudget,
    ) -> IntakeOutcome {
        self.accept_at(copy, budget, None)
    }

    pub(super) fn accept_at(
        &mut self,
        copy: AdmittedCopy,
        budget: &mut RetentionBudget,
        intake_ns: Option<u64>,
    ) -> IntakeOutcome {
        match self.reserve(&copy, budget) {
            Ok(Some(PreparedCharge { plan, reservation })) => {
                if let Some(reservation) = reservation {
                    budget.reservations.insert(plan.source, reservation);
                    budget.blocks = plan.next_blocks;
                    budget.bytes = plan.next_bytes;
                }
                self.copies.push_back((copy, intake_ns));
                IntakeOutcome::Accepted
            }
            Ok(None) => {
                budget.waiting_copy = Some(copy);
                budget.waiting_intake_ns = intake_ns;
                IntakeOutcome::Deferred
            }
            Err(detail) => IntakeOutcome::IntakeFailed { copy, detail },
        }
    }

    /// Reconsideration follows a recorded disposition, not elapsed wall time.
    /// A frozen budget keeps the waiting copy exactly where it was at the halt.
    pub(super) fn resume_waiting(&mut self, budget: &mut RetentionBudget) -> Option<IntakeOutcome> {
        if budget.frozen {
            return None;
        }
        let copy = budget.waiting_copy.take()?;
        let intake_ns = budget.waiting_intake_ns.take();
        Some(self.accept_at(copy, budget, intake_ns))
    }

    /// The whole-copy ledger installation must itself be atomic. Popping does
    /// not release source charges, and failed installation must retain this copy.
    pub(super) fn pop_front(&mut self, budget: &RetentionBudget) -> Option<AdmittedCopy> {
        if budget.frozen {
            return None;
        }
        self.copies.pop_front().map(|(copy, _)| copy)
    }

    pub(super) fn pop_timed(
        &mut self,
        budget: &RetentionBudget,
    ) -> Option<(AdmittedCopy, Option<u64>)> {
        if budget.frozen {
            return None;
        }
        self.copies.pop_front()
    }

    pub(super) fn queued(&self) -> impl Iterator<Item = &AdmittedCopy> {
        self.copies.iter().map(|(copy, _)| copy)
    }

    pub(super) fn queued_timed(&self) -> impl Iterator<Item = (&AdmittedCopy, Option<u64>)> {
        self.copies
            .iter()
            .map(|(copy, intake_ns)| (copy, *intake_ns))
    }

    pub(super) fn failure(&self) -> Option<&IntakeFailure> {
        self.failure.as_ref()
    }

    /// The failure slot needs no allocation. A second failure returns its own
    /// copy untouched rather than replacing the first account.
    #[allow(clippy::result_large_err)]
    pub(super) fn preserve_failure(&mut self, failure: IntakeFailure) -> Result<(), IntakeFailure> {
        if self.failure.is_some() {
            return Err(failure);
        }
        self.failure = Some(failure);
        Ok(())
    }

    fn reserve(
        &mut self,
        copy: &AdmittedCopy,
        budget: &mut RetentionBudget,
    ) -> Result<Option<PreparedCharge>, IntakeError> {
        if budget.frozen {
            return Err(IntakeError::Frozen);
        }
        if self.failure.is_some() {
            return Err(IntakeError::FailureOccupied);
        }
        if budget.waiting_copy.is_some() {
            return Err(IntakeError::WaitingOccupied);
        }
        let plan = ChargePlan::inspect(copy, budget)?;
        if budget.reservations.contains_key(&plan.source) {
            return Err(IntakeError::DuplicateReservation);
        }
        if plan.next_blocks > budget.limits.max_pending_blocks.0
            || plan.next_bytes > budget.limits.max_pending_bytes.0
        {
            return Ok(None);
        }
        let reservation = if plan.block_count == 0 {
            None
        } else {
            self.allocation(AllocationPoint::Blocks)?;
            let mut blocks = Vec::new();
            blocks
                .try_reserve_exact(selected_blocks(copy).len())
                .map_err(|_| IntakeError::AllocationFailed)?;
            for selected in selected_blocks(copy) {
                blocks.push(BlockCharge {
                    block: selected.source.block,
                    bytes: count(selected.raw.len())?,
                });
            }
            self.allocation(AllocationPoint::Reservations)?;
            budget
                .reservations
                .try_reserve(1)
                .map_err(|_| IntakeError::AllocationFailed)?;
            Some(CopyReservation {
                shared_bytes: plan.shared_bytes,
                blocks,
            })
        };
        self.allocation(AllocationPoint::Inbox)?;
        self.copies
            .try_reserve(1)
            .map_err(|_| IntakeError::AllocationFailed)?;
        Ok(Some(PreparedCharge { plan, reservation }))
    }

    fn allocation(&mut self, point: AllocationPoint) -> Result<(), IntakeError> {
        #[cfg(test)]
        if self.fail_allocation == Some(point) {
            self.fail_allocation = None;
            return Err(IntakeError::AllocationFailed);
        }
        let _ = point;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AllocationPoint {
    Blocks,
    Reservations,
    Inbox,
}

struct ChargePlan {
    source: SourceEventKey,
    shared_bytes: u64,
    block_count: u64,
    next_blocks: u64,
    next_bytes: u64,
}

struct PreparedCharge {
    plan: ChargePlan,
    reservation: Option<CopyReservation>,
}

impl ChargePlan {
    fn inspect(copy: &AdmittedCopy, budget: &RetentionBudget) -> Result<Self, IntakeError> {
        let (envelope, rule, context) = match copy {
            AdmittedCopy::Statement(copy) => (
                &copy.source_envelope,
                &copy.partition_rule_id,
                copy.context.as_ref().map(|value| value.raw.as_ref()),
            ),
            AdmittedCopy::Pair(copy) => (&copy.source_envelope, &copy.partition_rule_id, None),
        };
        // The trace has already judged vocabulary and canonical rendering. The
        // shared closed envelope codec checks this public type's source identity.
        // serde allocations that abort are not recoverable, as at the trace tap.
        let source: SourceEnvelope =
            serde_json::from_str(envelope).map_err(|_| IntakeError::InvalidEnvelope)?;
        if source.run.is_empty() || rule.is_empty() {
            return Err(IntakeError::InvalidIdentity);
        }
        let selected = selected_blocks(copy);
        let block_count = count(selected.len())?;
        let mut shared_bytes = 0;
        let mut bytes = 0_u64;
        let mut previous = None;
        for block in selected {
            if block.source.run != source.run || block.source.sequence.0 != source.sequence.0 {
                return Err(IntakeError::InvalidIdentity);
            }
            if previous.is_some_and(|previous| block.source.block <= previous) {
                return Err(IntakeError::DuplicateBlock);
            }
            previous = Some(block.source.block);
            bytes = bytes
                .checked_add(count(block.raw.len())?)
                .ok_or(IntakeError::CounterOverflow)?;
        }
        if matches!(copy, AdmittedCopy::Pair(pair) if pair.block.source.block != 0) {
            return Err(IntakeError::InvalidIdentity);
        }
        if block_count != 0 {
            shared_bytes = count(envelope.len())?
                .checked_add(count(context.map_or(0, str::len))?)
                .ok_or(IntakeError::CounterOverflow)?;
            bytes = bytes
                .checked_add(shared_bytes)
                .ok_or(IntakeError::CounterOverflow)?;
        }
        Ok(Self {
            source: SourceEventKey {
                run: source.run,
                sequence: source.sequence.0,
            },
            shared_bytes,
            block_count,
            next_blocks: budget
                .blocks
                .checked_add(block_count)
                .ok_or(IntakeError::CounterOverflow)?,
            next_bytes: budget
                .bytes
                .checked_add(bytes)
                .ok_or(IntakeError::CounterOverflow)?,
        })
    }
}

fn count(value: usize) -> Result<u64, IntakeError> {
    u64::try_from(value).map_err(|_| IntakeError::CounterOverflow)
}

pub(super) fn selected_blocks(copy: &AdmittedCopy) -> &[SelectedBlock] {
    match copy {
        AdmittedCopy::Statement(copy) => &copy.blocks,
        AdmittedCopy::Pair(copy) => std::slice::from_ref(&copy.block),
    }
}

#[cfg(test)]
#[path = "intake_tests.rs"]
mod tests;
