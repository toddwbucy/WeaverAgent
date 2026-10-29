//! Read-only plan operations retain their exact request and checked proofs in
//! the one allocator slot. No result escapes before its owed account settles.
use super::*;
use crate::flow::plan::CommittedPlanSummary;
use crate::flow::plan::ports::{self as checked, PlanCheckFailure, PlanRequest};
use marks_driver::MarkOperation;
use std::sync::Arc;
use weaver_trace::TurnRef;
use weaver_types::state_flow as s;

#[path = "context_plan_ports_record.rs"]
mod record;
use record::PlanPortRecord;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum PlanPortAdmission {
    Busy,
    WrongPhase,
    MissingWait,
    InvalidAttribution,
}

#[derive(Debug)]
pub(in crate::flow) struct PlanPortBeginError {
    pub request: PlanRequest,
    pub turn: Option<TurnRef>,
    pub refusal: PlanPortAdmission,
    pub failure: Option<RunFailure>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum PlanPortProgress {
    Progress,
    Pending,
    Complete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Validate,
    Read,
    Refusal,
    Complete,
}

pub(super) struct PlanPortTransaction {
    pub(super) driving: bool,
    origin: Arc<()>,
    request: PlanRequest,
    turn: Option<TurnRef>,
    bound: AbsoluteBound,
    stage: Stage,
    current: Option<Arc<CommittedPlanSummary>>,
    read: Option<read_driver::RetainedReadCompletion>,
    refusal: Option<s::PlanOperationRefused>,
    parent: Option<Sequence>,
    record: Option<PlanPortRecord>,
    work_ref: Option<s::WorkRef>,
    cancelled: bool,
}

/// An account cannot grant work. The exact request and consumed proofs stay
/// retained after refusal or cancellation and through later teardown.
pub(in crate::flow) struct PlanPortAccount {
    work: Box<PlanPortTransaction>,
}

/// Only the checked predicate creates this grant, with its actual proofs.
/// The later consumer must recheck the context, current revision and exact act.
pub(in crate::flow) struct AllowedAct {
    account: PlanPortAccount,
}
impl AllowedAct {
    pub(in crate::flow) fn request(&self) -> &PlanRequest {
        &self.account.work.request
    }
}

/// Public WorkRef metadata alone cannot construct this checked work account.
pub(in crate::flow) struct CheckedWork {
    account: PlanPortAccount,
}
impl CheckedWork {
    pub(in crate::flow) fn reference(&self) -> &s::WorkRef {
        self.account
            .work
            .work_ref
            .as_ref()
            .expect("checked preview")
    }
}

pub(in crate::flow) enum PlanPortCompletion {
    Allowed(AllowedAct),
    Work(CheckedWork),
    Refused {
        refusal: s::PlanOperationRefused,
        account: PlanPortAccount,
    },
    Cancelled(PlanPortAccount),
}

impl PlanPortCompletion {
    fn account(&self) -> &PlanPortAccount {
        match self {
            Self::Allowed(grant) => &grant.account,
            Self::Work(work) => &work.account,
            Self::Refused { account, .. } | Self::Cancelled(account) => account,
        }
    }
    pub(in crate::flow) fn request(&self) -> &PlanRequest {
        &self.account().work.request
    }
    pub(in crate::flow) fn belongs_to(&self, context: &RecordingContext) -> bool {
        Arc::ptr_eq(&self.account().work.origin, &context.read_origin)
    }
    pub(in crate::flow) fn cancel(self) -> Self {
        let mut account = match self {
            Self::Allowed(grant) => grant.account,
            Self::Work(work) => work.account,
            Self::Refused { account, .. } | Self::Cancelled(account) => account,
        };
        account.work.cancelled = true;
        Self::Cancelled(account)
    }
}

impl RecordingContext {
    pub(super) fn plan_port_work(&self) -> Option<&PlanPortTransaction> {
        match self.marks.as_ref()? {
            MarkOperation::Port(work) => Some(work),
            _ => None,
        }
    }
    fn plan_port_work_mut(&mut self) -> Option<&mut PlanPortTransaction> {
        match self.marks.as_mut()? {
            MarkOperation::Port(work) => Some(work),
            _ => None,
        }
    }
    pub(super) fn plan_port_bound(&self) -> Option<AbsoluteBound> {
        self.plan_port_work().map(|work| work.bound)
    }
    pub(super) fn plan_port_read_allowed(
        &self,
        selector: &s::ReadSelector,
        turn: Option<&TurnRef>,
    ) -> bool {
        self.plan_port_work().is_some_and(|work| {
            work.driving
                && !work.cancelled
                && work.stage == Stage::Read
                && work.current.is_some()
                && &work.request.selector() == selector
                && work.turn.as_ref() == turn
                && self.deadlines.get(PumpDeadline::Decision) == Some(work.bound)
        })
    }

    /// The worker checks admission before arming a new Decision. The original
    /// owner repeats the same check before retaining the accepted request.
    pub(in crate::flow) fn plan_port_admission(
        &self,
        turn: Option<&TurnRef>,
    ) -> Result<(), PlanPortAdmission> {
        if self.phase != RecordingPhase::Entered
            || self.flow.phase() != CoordinatorPhase::Active
            || !self.barrier.is_active()
            || self.open_start.is_none()
            || self.load_duty.is_none()
        {
            return Err(PlanPortAdmission::WrongPhase);
        }
        if turn.is_some_and(|turn| turn.0.is_empty() || Some(turn) != self.open_turn.as_ref()) {
            return Err(PlanPortAdmission::InvalidAttribution);
        }
        if self.marks.is_some()
            || self.boot_pairs.is_some()
            || self.freshness.is_some()
            || self.output.is_some()
            || self.filing.is_some()
            || self.pair_work.is_some()
            || self.repair.is_some()
            || self.read.is_some()
            || self.inspection.is_some()
            || self.member_capture.is_some()
            || self.boot_open_blocks_external()
            || self.plan_summary_blocks_external()
            || self.place_blocks_external()
            || self.measure_blocks_external()
            || self.input_turn.is_some()
            || self.input_author.is_some()
            || self.input.snapshot().is_some()
            || self.templates_block_content()
            || !self.serving_observation_author_available()
            || self.flow.has_filing_consumer()
            || self.flow.has_output_consumer()
            || self.flow.pending_check().is_some()
            || self.flow.pending_control().is_some()
            || self.flow.operation_completion().is_some()
            || !self.flow.state().outstanding().is_empty()
            || self.flow.state().held_measurement().is_some()
            || self.deadlines.get(PumpDeadline::Cut).is_some()
            || self.deadlines.get(PumpDeadline::Gate).is_some()
            || self.deadlines.get(PumpDeadline::StateControl).is_some()
        {
            return Err(PlanPortAdmission::Busy);
        }
        Ok(())
    }

    #[allow(clippy::result_large_err)] // Return the exact unaccepted request.
    pub(in crate::flow) fn begin_plan_operation(
        &mut self,
        request: PlanRequest,
        turn: Option<TurnRef>,
        io: &mut impl PumpIo,
    ) -> Result<(), PlanPortBeginError> {
        let error = |request, turn, refusal, failure| PlanPortBeginError {
            request,
            turn,
            refusal,
            failure,
        };
        if let Err(failure) = self.live(io) {
            return Err(error(
                request,
                turn,
                PlanPortAdmission::WrongPhase,
                Some(failure),
            ));
        }
        if let Err(refusal) = self.plan_port_admission(turn.as_ref()) {
            return Err(error(request, turn, refusal, None));
        }
        let Some(bound) = self.deadlines.get(PumpDeadline::Decision) else {
            return Err(error(request, turn, PlanPortAdmission::MissingWait, None));
        };
        self.marks = Some(MarkOperation::Port(Box::new(PlanPortTransaction {
            driving: false,
            origin: Arc::clone(&self.read_origin),
            request,
            turn,
            bound,
            stage: Stage::Validate,
            current: None,
            read: None,
            refusal: None,
            parent: None,
            record: None,
            work_ref: None,
            cancelled: false,
        })));
        Ok(())
    }

    pub(in crate::flow) fn advance_plan_operation<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<PlanPortProgress, RunFailure> {
        self.live(ports.io)?;
        let work = self.plan_port_work().ok_or(RunFailure::MarksOwned)?;
        if self.deadlines.get(PumpDeadline::Decision) != Some(work.bound) {
            return Err(self.plan_port_fault(ports.io, "plan operation lost its original Decision"));
        }
        if work.stage == Stage::Complete {
            return Ok(PlanPortProgress::Complete);
        }
        self.plan_port_work_mut().unwrap().driving = true;
        let result = self.advance_plan_operation_owned(ports);
        self.plan_port_work_mut().unwrap().driving = false;
        result
    }

    fn advance_plan_operation_owned<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<PlanPortProgress, RunFailure> {
        let work = self.plan_port_work().unwrap();
        if work.cancelled && self.read.is_none() && work.stage != Stage::Refusal {
            self.plan_port_work_mut().unwrap().stage = Stage::Complete;
            return Ok(PlanPortProgress::Complete);
        }
        match work.stage {
            Stage::Validate => {
                let seed = &self.open_start.as_ref().unwrap().seed_id;
                if let Some(refusal) = work.request.local_refusal(seed) {
                    let work = self.plan_port_work_mut().unwrap();
                    work.refusal = Some(refusal);
                    work.stage = Stage::Refusal;
                    return Ok(PlanPortProgress::Progress);
                }
                if self.committed_plan_summary().is_err() {
                    return Err(self.plan_port_fault(
                        ports.io,
                        "plan predicate has no usable committed cache",
                    ));
                }
                let current = Arc::clone(self.plan_cache.committed.as_ref().unwrap());
                let work = self.plan_port_work_mut().unwrap();
                work.parent = Some(current.read().recall_sequence());
                work.current = Some(current);
                let work = self.plan_port_work().unwrap();
                match checked::active(&work.request, self, work.current.as_ref().unwrap()) {
                    Ok(_) => self.plan_port_work_mut().unwrap().stage = Stage::Read,
                    Err(failure) => self.retain_plan_check_failure(failure, ports.io)?,
                }
                Ok(PlanPortProgress::Progress)
            }
            Stage::Read => self.advance_plan_port_read(ports),
            Stage::Refusal => {
                if !self.record_plan_port_refusal(ports)? {
                    return Ok(PlanPortProgress::Pending);
                }
                self.live(ports.io)?;
                self.plan_port_work_mut().unwrap().stage = Stage::Complete;
                Ok(PlanPortProgress::Complete)
            }
            Stage::Complete => Ok(PlanPortProgress::Complete),
        }
    }

    fn advance_plan_port_read<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<PlanPortProgress, RunFailure> {
        let work = self.plan_port_work().unwrap();
        let selector = work.request.selector();
        if self.read.is_none() {
            let turn = work.turn.clone();
            if let Err(error) = self.begin_plan_port_read(selector.clone(), turn, ports.io) {
                if let Some(failure) = error.failure {
                    return Err(failure);
                }
                if error.refusal == ReadBeginRefusal::Busy {
                    return Ok(PlanPortProgress::Pending);
                }
                return Err(
                    self.plan_port_fault(ports.io, "plan predicate lost its read admission")
                );
            }
        }
        if self.plan_port_work().unwrap().cancelled {
            self.cancel_read(ports.io)?;
        }
        let progress = self.advance_read(ports)?;
        let Some(done) = self.take_read_retained(ports.io)? else {
            return Ok(if progress == ReadProgress::Pending {
                PlanPortProgress::Pending
            } else {
                PlanPortProgress::Progress
            });
        };
        self.plan_port_work_mut().unwrap().read = Some(done);
        let work = self.plan_port_work().unwrap();
        if work.cancelled {
            self.plan_port_work_mut().unwrap().stage = Stage::Complete;
            return Ok(PlanPortProgress::Complete);
        }
        let retained = work.read.as_ref().unwrap();
        if let Some(proof) = &retained.refusal {
            let refusal = checked::read_refusal(self, &selector, proof)
                .map_err(|detail| self.plan_port_fault(ports.io, detail))?;
            let work = self.plan_port_work_mut().unwrap();
            work.parent = Some(
                work.read
                    .as_ref()
                    .unwrap()
                    .refusal
                    .as_ref()
                    .unwrap()
                    .measurement_sequence(),
            );
            work.refusal = Some(refusal);
            work.stage = Stage::Refusal;
            return Ok(PlanPortProgress::Progress);
        }
        let ReadCompletion::Recorded(proof) = &retained.completion else {
            return Err(
                self.plan_port_fault(ports.io, "plan predicate received cancelled read evidence")
            );
        };
        let parent = proof.recall_sequence();
        let current = work.current.as_ref().unwrap();
        let result = match &work.request {
            PlanRequest::AssessAct { .. } => checked::assess(
                &work.request,
                self,
                current,
                proof,
                &self.load_duty.as_ref().unwrap().tools,
            )
            .map(|()| None),
            PlanRequest::WorkReference { .. } => {
                checked::work_reference(&work.request, self, current, proof).map(Some)
            }
        };
        self.plan_port_work_mut().unwrap().parent = Some(parent);
        match result {
            Ok(reference) => {
                self.live(ports.io)?;
                let work = self.plan_port_work_mut().unwrap();
                work.work_ref = reference;
                work.stage = Stage::Complete;
                Ok(PlanPortProgress::Complete)
            }
            Err(failure) => {
                self.retain_plan_check_failure(failure, ports.io)?;
                Ok(PlanPortProgress::Progress)
            }
        }
    }

    fn retain_plan_check_failure(
        &mut self,
        failure: PlanCheckFailure,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        match failure {
            PlanCheckFailure::Protocol(detail) => Err(self.plan_port_fault(io, detail)),
            PlanCheckFailure::Refused(refusal) => {
                let work = self.plan_port_work_mut().unwrap();
                work.refusal = Some(refusal);
                work.stage = Stage::Refusal;
                Ok(())
            }
        }
    }

    pub(in crate::flow) fn cancel_plan_operation(&mut self) {
        if let Some(work) = self.plan_port_work_mut()
            && work.stage != Stage::Complete
        {
            work.cancelled = true;
        }
    }

    pub(in crate::flow) fn take_plan_operation(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<Option<PlanPortCompletion>, RunFailure> {
        self.live(io)?;
        let Some(work) = self.plan_port_work() else {
            return Ok(None);
        };
        if work.stage != Stage::Complete {
            return Ok(None);
        }
        if self.deadlines.get(PumpDeadline::Decision) != Some(work.bound) {
            return Err(self.plan_port_fault(io, "plan result lost its caller's Decision"));
        }
        if !work.cancelled
            && work.refusal.is_none()
            && !self.plan_cache.committed.as_ref().is_some_and(|current| {
                work.current
                    .as_ref()
                    .is_some_and(|held| Arc::ptr_eq(current, held))
                    && self.plan_cache.unavailable.is_none()
                    && self.plan_cache.required.is_none()
            })
        {
            return Err(self.plan_port_fault(io, "plan result lost its checked current revision"));
        }
        let Some(MarkOperation::Port(work)) = self.marks.take() else {
            unreachable!()
        };
        let account = PlanPortAccount { work };
        // This port borrows the caller's Decision. It never disarms that wait.
        Ok(Some(if account.work.cancelled {
            PlanPortCompletion::Cancelled(account)
        } else if let Some(refusal) = account.work.refusal.clone() {
            PlanPortCompletion::Refused { refusal, account }
        } else if matches!(account.work.request, PlanRequest::AssessAct { .. }) {
            PlanPortCompletion::Allowed(AllowedAct { account })
        } else {
            PlanPortCompletion::Work(CheckedWork { account })
        }))
    }

    fn plan_port_fault(&mut self, io: &mut impl PumpIo, detail: impl Into<String>) -> RunFailure {
        self.fail(
            io,
            FlowHaltReason::ProtocolFailed,
            HaltPoint::Barrier,
            detail,
        )
    }
}
