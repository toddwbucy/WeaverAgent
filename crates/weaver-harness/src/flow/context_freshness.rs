//! A bounded filing pause over exact selected source identities.
//! This owner creates no placement, snapshot, capture or decoder authority.

use super::*;
use weaver_types::state_flow::SourceBlockId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum FreshnessBeginRefusal {
    WrongPhase,
    Busy,
}

#[derive(Debug)]
pub(in crate::flow) struct FreshnessBeginError {
    pub required_ids: Vec<SourceBlockId>,
    pub refusal: FreshnessBeginRefusal,
    pub failure: Option<RunFailure>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum FreshnessProgress {
    Progress,
    Pending,
    Complete,
}

/// The exact selected dispositions were recorded in this context. No public
/// constructor or conversion from a sequence can manufacture this evidence.
/// An empty required set is allowed and makes no claim about unrequested work.
#[derive(Debug)]
pub(in crate::flow) struct PreparedFreshness {
    origin: std::sync::Arc<()>,
    required_ids: Vec<SourceBlockId>,
    bound: AbsoluteBound,
}

impl PreparedFreshness {
    pub(in crate::flow) fn belongs_to(&self, context: &RecordingContext) -> bool {
        std::sync::Arc::ptr_eq(&self.origin, &context.read_origin)
    }

    pub(in crate::flow) fn required_ids(&self) -> &[SourceBlockId] {
        &self.required_ids
    }

    pub(in crate::flow) fn bound(&self) -> AbsoluteBound {
        self.bound
    }
}

#[derive(Debug)]
pub(in crate::flow) enum FreshnessCompletion {
    Prepared(PreparedFreshness),
    Cancelled { required_ids: Vec<SourceBlockId> },
}

pub(super) struct FreshnessTransaction {
    required_ids: Vec<SourceBlockId>,
    bound: AbsoluteBound,
    cancelled: bool,
    complete: bool,
}

impl RecordingContext {
    /// Start the filing pause before a mandatory fresh read. Existing generic
    /// Non-turn author input remains owned. A retained turn start and explicit
    /// transactions must finish first.
    pub(in crate::flow) fn prepare_cut(
        &mut self,
        required_ids: Vec<SourceBlockId>,
        io: &mut impl PumpIo,
    ) -> Result<(), FreshnessBeginError> {
        if let Err(failure) = self.live(io) {
            return Err(FreshnessBeginError {
                required_ids,
                refusal: FreshnessBeginRefusal::WrongPhase,
                failure: Some(failure),
            });
        }
        if self.phase != RecordingPhase::Entered
            || self.flow.phase() != CoordinatorPhase::Active
            || !self.barrier.is_active()
        {
            return Err(FreshnessBeginError {
                required_ids,
                refusal: FreshnessBeginRefusal::WrongPhase,
                failure: None,
            });
        }
        if self.freshness.is_some()
            || self.marks.is_some()
            || self.boot_pairs.is_some()
            || self.open_turn.is_some()
            || self
                .waiting_content
                .as_ref()
                .is_some_and(|event| event.kind == Kind::TurnStarted)
            || self
                .author_turn
                .as_ref()
                .is_some_and(|(kind, _)| *kind == Kind::TurnStarted)
            || self.input.snapshot().is_some()
            || self.input_author.is_some()
            || self.input_turn.is_some()
            || self.output.is_some()
            || self.filing.is_some()
            || self.flow.has_output_consumer()
            || self.flow.has_filing_consumer()
            || self.pair_work.is_some()
            || self.pair_rejected_authoring
            || self.repair.is_some()
            || self.repair_certificate.is_some()
            || self.member_capture.is_some()
            || self.read.is_some()
            || self.inspection.is_some()
            || self.measure.is_some()
            || self.templates_block_content()
            || self.template_blocked_author.is_some()
            || self.flow.pending_control().is_some()
            || self.flow.pending_check().is_some()
            || self.flow.operation_completion().is_some()
            || self.deadlines.get(PumpDeadline::Gate).is_some()
            || self.deadlines.get(PumpDeadline::Cut).is_some()
            || self.deadlines.get(PumpDeadline::StateControl).is_some()
        {
            return Err(FreshnessBeginError {
                required_ids,
                refusal: FreshnessBeginRefusal::Busy,
                failure: None,
            });
        }
        let bound = match AbsoluteBound::after(self.last_ns, self.declaration.cut_wait.elapsed_ns.0)
        {
            Ok(bound) => bound,
            Err(error) => {
                return Err(FreshnessBeginError {
                    required_ids,
                    refusal: FreshnessBeginRefusal::WrongPhase,
                    failure: Some(self.freshness_fault(io, error.to_string())),
                });
            }
        };
        // No yielding hook separates the armed bound from its retained owner.
        if let Err(error) = self.deadlines.arm(PumpDeadline::Cut, bound) {
            return Err(FreshnessBeginError {
                required_ids,
                refusal: FreshnessBeginRefusal::WrongPhase,
                failure: Some(self.freshness_fault(io, error.to_string())),
            });
        }
        self.freshness = Some(FreshnessTransaction {
            required_ids,
            bound,
            cancelled: false,
            complete: false,
        });
        Ok(())
    }

    pub(super) fn freshness_wait_still_owned(&self, key: PumpDeadline) -> bool {
        key == PumpDeadline::Cut && self.freshness.as_ref().is_some_and(|owner| !owner.complete)
    }

    fn freshness_wait_matches(&mut self, io: &mut impl PumpIo) -> Result<(), RunFailure> {
        let Some(owner) = &self.freshness else {
            return Err(self.freshness_fault(io, "freshness operation has no owner"));
        };
        let expected = (!owner.complete).then_some(owner.bound);
        if self.deadlines.get(PumpDeadline::Cut) != expected {
            return Err(self.freshness_fault(io, "freshness lost its original Cut wait"));
        }
        Ok(())
    }

    pub(in crate::flow) fn advance_prepare_cut<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<FreshnessProgress, RunFailure> {
        self.live(ports.io)?;
        self.freshness_wait_matches(ports.io)?;
        if self.freshness.as_ref().unwrap().complete {
            return Ok(FreshnessProgress::Complete);
        }
        if self.completed_author.is_none()
            && (self.waiting_content.is_some()
                || (self.barrier.pending_author().is_some() && !self.flow.has_pending_record()))
        {
            let sequence = self.resume_owned_author_and_wait(ports)?;
            // The original caller still owns this return value. Retain it
            // before any subsequent flow, clock or recoverable lifecycle work.
            self.completed_author = Some(sequence);
        }
        let progress = self.reduce_flow(ports.io)?;
        if self.waiting_content.is_some()
            || self.barrier.pending().is_some()
            || self.barrier.pending_author().is_some()
            || self.flow.has_pending_observations()
            || self.inbox.queued().next().is_some()
            || self.budget.waiting_copy().is_some()
        {
            return Ok(FreshnessProgress::Pending);
        }
        // Only now are all previously admitted copies visible in the ledger.
        // A deferred whole copy must not be mistaken for an unknown identity.
        let ready = self
            .flow
            .ledger()
            .barrier(&self.freshness.as_ref().unwrap().required_ids)
            .map_err(|error| self.freshness_fault(ports.io, error.to_string()))?;
        if !ready {
            return Ok(if progress == Advance::Progress {
                FreshnessProgress::Progress
            } else {
                FreshnessProgress::Pending
            });
        }
        self.live(ports.io)?;
        self.freshness.as_mut().unwrap().complete = true;
        self.deadlines.disarm(PumpDeadline::Cut);
        Ok(FreshnessProgress::Complete)
    }

    /// Cancellation suppresses the readiness result, not any admitted filing.
    /// Drive and take this same owner under its unchanged bound before reuse.
    pub(in crate::flow) fn cancel_prepare_cut(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        self.live(io)?;
        self.freshness_wait_matches(io)?;
        self.freshness.as_mut().unwrap().cancelled = true;
        Ok(())
    }

    pub(in crate::flow) fn take_prepared_cut(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<Option<FreshnessCompletion>, RunFailure> {
        self.live(io)?;
        self.freshness_wait_matches(io)?;
        if !self.freshness.as_ref().unwrap().complete {
            return Ok(None);
        }
        // No recoverable hook follows custody transfer.
        let owner = self.freshness.take().unwrap();
        Ok(Some(if owner.cancelled {
            FreshnessCompletion::Cancelled {
                required_ids: owner.required_ids,
            }
        } else {
            FreshnessCompletion::Prepared(PreparedFreshness {
                origin: self.read_origin.clone(),
                required_ids: owner.required_ids,
                bound: owner.bound,
            })
        }))
    }

    fn freshness_fault(&mut self, io: &mut impl PumpIo, detail: impl Into<String>) -> RunFailure {
        self.fail(
            io,
            FlowHaltReason::ProtocolFailed,
            HaltPoint::Barrier,
            detail,
        )
    }
}
