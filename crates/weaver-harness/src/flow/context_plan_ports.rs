//! Plan operations retain their exact request and checked proofs in
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
#[path = "context_plan_effect.rs"]
mod effect;
use effect::PlanEffectWork;
#[path = "context_plan_break.rs"]
mod break_plan;

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
    EffectReady,
    Progress,
    Pending,
    Complete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Validate,
    Read,
    Effect,
    EffectFiling,
    EffectFiled,
    Summary,
    Native,
    TemplateFault,
    BreakReport,
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
    effect: Option<PlanEffectWork>,
    break_report: Option<s::PlanBreakReport>,
    successor: Option<checked::CheckedStepSuccessor>,
    setup: Option<s::ReviewSetupRef>,
    outcome_span: Option<s::SpanRef>,
    held: Option<HeldOutcome>,
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
/// It retains the released preview read whose exact bytes its digest names.
///
/// ```compile_fail,E0277
/// fn cloned<T: Clone>() {}
/// cloned::<weaver_harness::CheckedWork>();
/// ```
pub struct CheckedWork {
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
    pub(in crate::flow) fn preview(&self) -> &RecordedRead {
        let ReadCompletion::Recorded(read) = &self.account.work.read.as_ref().unwrap().completion
        else {
            unreachable!("checked work retains its released preview")
        };
        read
    }
    pub(in crate::flow) fn belongs_to(&self, context: &RecordingContext) -> bool {
        Arc::ptr_eq(&self.account.work.origin, &context.read_origin)
    }
    /// The latest recorded proof this work consumed, its preview read.
    fn sequence(&self) -> Sequence {
        self.account.work.parent.unwrap()
    }
}
impl std::fmt::Debug for CheckedWork {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("CheckedWork")
            .field(self.reference())
            .finish()
    }
}

/// CLOSURE-EVIDENCE: a private, single-use, run-bound grant. Only the
/// attest_closure port constructs it, and close consumes it by value.
/// It holds the attestation account, which retains the moved checked work.
///
/// ```compile_fail,E0277
/// fn cloned<T: Clone>() {}
/// cloned::<weaver_harness::ClosureEvidence>();
/// ```
///
/// ```compile_fail,E0277
/// fn serialized(evidence: &weaver_harness::ClosureEvidence) {
///     let _ = serde_json::to_vec(evidence);
/// }
/// ```
///
/// ```compile_fail,E0277
/// let _ = serde_json::from_str::<weaver_harness::ClosureEvidence>("{}");
/// ```
///
/// ```compile_fail,E0451
/// fn forged() -> weaver_harness::ClosureEvidence {
///     weaver_harness::ClosureEvidence { account: todo!() }
/// }
/// ```
pub struct ClosureEvidence {
    account: PlanPortAccount,
}
impl ClosureEvidence {
    fn from_attestation(account: PlanPortAccount) -> Self {
        assert!(
            matches!(account.work.request, PlanRequest::AttestClosure { .. })
                && account.work.setup.is_some()
                && account.work.refusal.is_none()
                && !account.work.cancelled,
            "only a settled attestation grants closure evidence"
        );
        Self { account }
    }
    fn work_check(&self) -> &CheckedWork {
        let PlanRequest::AttestClosure { work, .. } = &self.account.work.request else {
            unreachable!("private attestation account")
        };
        work
    }
    pub(in crate::flow) fn plan(&self) -> &s::PlanRef {
        self.account.work.request.expected()
    }
    pub(in crate::flow) fn step_id(&self) -> &s::StepId {
        self.account.work.request.step_id().unwrap()
    }
    /// attest_closure grants unreviewed evidence only.
    pub(in crate::flow) fn review_call(&self) -> Option<&s::CallId> {
        None
    }
    pub(in crate::flow) fn work(&self) -> &s::WorkRef {
        self.work_check().reference()
    }
    pub(in crate::flow) fn setup(&self) -> &s::ReviewSetupRef {
        self.account.work.setup.as_ref().unwrap()
    }
    pub(in crate::flow) fn belongs_to(&self, context: &RecordingContext) -> bool {
        Arc::ptr_eq(&self.account.work.origin, &context.read_origin)
            && self.work_check().belongs_to(context)
    }
    /// The attestation's parent: the later of the cache and the checked work.
    pub(in crate::flow) fn sequence(&self) -> Sequence {
        self.account.work.parent.unwrap()
    }
}
impl std::fmt::Debug for ClosureEvidence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClosureEvidence")
            .field("plan", self.plan())
            .field("step_id", self.step_id())
            .field("review_call", &self.review_call())
            .field("work", self.work())
            .field("setup", self.setup())
            .finish()
    }
}

/// The owner's held outcome_span read: its recorded Marks catalog and the
/// plan revision it was requested under. Only a settled outcome_span stores
/// it, a later outcome_span replaces it, and only an admitted close takes it.
pub(in crate::flow) struct HeldOutcome {
    expected: s::PlanRef,
    read: RecordedRead,
}
impl HeldOutcome {
    pub(in crate::flow) fn expected(&self) -> &s::PlanRef {
        &self.expected
    }
    pub(in crate::flow) fn read(&self) -> &RecordedRead {
        &self.read
    }
    pub(in crate::flow) fn belongs_to(&self, context: &RecordingContext) -> bool {
        self.read.belongs_to(context)
    }
    pub(in crate::flow) fn sequence(&self) -> Sequence {
        self.read.recall_sequence()
    }
}

pub(in crate::flow) struct ActivatedStep {
    account: PlanPortAccount,
}
impl ActivatedStep {
    pub(in crate::flow) fn step(&self) -> &s::PlanStep {
        self.account
            .work
            .effect
            .as_ref()
            .unwrap()
            .active
            .as_ref()
            .unwrap()
    }
}

pub(in crate::flow) enum PlanPortCompletion {
    Successor(PlanPortAccount),
    QueryRefused(PlanPortAccount),
    Activated(ActivatedStep),
    BreakRecorded(PlanPortAccount),
    Allowed(AllowedAct),
    Work(CheckedWork),
    Attested(ClosureEvidence),
    OutcomeSpan(PlanPortAccount),
    Closed(PlanPortAccount),
    Refused {
        refusal: s::PlanOperationRefused,
        account: PlanPortAccount,
    },
    Cancelled(PlanPortAccount),
}

impl PlanPortCompletion {
    pub(in crate::flow) fn break_snapshot(&self) -> Option<&s::PlanSnapshot> {
        let Self::BreakRecorded(account) = self else {
            return None;
        };
        let s::QueryResult::PlanCurrent { current, .. } =
            account.work.current.as_ref().unwrap().read().result()
        else {
            unreachable!()
        };
        current.as_ref()
    }

    /// close returns the committed snapshot its mandatory refresh adopted.
    pub(in crate::flow) fn closed_snapshot(&self) -> Option<&s::PlanSnapshot> {
        let Self::Closed(account) = self else {
            return None;
        };
        let s::QueryResult::PlanCurrent { current, .. } =
            account.work.current.as_ref().unwrap().read().result()
        else {
            unreachable!()
        };
        current.as_ref()
    }

    pub(in crate::flow) fn closure_evidence(&self) -> Option<&ClosureEvidence> {
        match self {
            Self::Attested(evidence) => Some(evidence),
            _ => None,
        }
    }

    /// outcome_span's allocation. Its read stays with the owner for close.
    pub(in crate::flow) fn outcome_span(&self) -> Option<&s::SpanRef> {
        match self {
            Self::OutcomeSpan(account) => account.work.outcome_span.as_ref(),
            _ => None,
        }
    }

    pub(in crate::flow) fn checked_work(&self) -> Option<&CheckedWork> {
        match self {
            Self::Work(work) => Some(work),
            _ => None,
        }
    }

    /// Move the checked work into attest_closure. Any other result returns whole.
    pub(in crate::flow) fn into_checked_work(self) -> Result<CheckedWork, Self> {
        match self {
            Self::Work(work) => Ok(work),
            other => Err(other),
        }
    }

    /// Move the single-use grant into close. Any other result returns whole.
    pub(in crate::flow) fn into_closure_evidence(self) -> Result<ClosureEvidence, Self> {
        match self {
            Self::Attested(evidence) => Ok(evidence),
            other => Err(other),
        }
    }

    pub(in crate::flow) fn successor(&self) -> Option<&checked::CheckedStepSuccessor> {
        match self {
            Self::Successor(account) => account.work.successor.as_ref(),
            _ => None,
        }
    }
    pub(in crate::flow) fn query_refusal(&self) -> Option<(s::ReadRefusalCode, &str)> {
        let Self::QueryRefused(account) = self else {
            return None;
        };
        let ReadCompletion::Refused(crate::flow::read::ReadFailure::Refused { code, detail }) =
            &account.work.read.as_ref().unwrap().completion
        else {
            unreachable!()
        };
        Some((*code, detail))
    }
    pub(in crate::flow) fn into_successor(self) -> Result<checked::CheckedStepSuccessor, Self> {
        let Self::Successor(mut account) = self else {
            return Err(self);
        };
        Ok(account.work.successor.take().unwrap())
    }

    fn account(&self) -> &PlanPortAccount {
        match self {
            Self::Activated(grant) => &grant.account,
            Self::Allowed(grant) => &grant.account,
            Self::Work(work) => &work.account,
            Self::Attested(evidence) => &evidence.account,
            Self::BreakRecorded(account)
            | Self::Closed(account)
            | Self::OutcomeSpan(account)
            | Self::Successor(account)
            | Self::QueryRefused(account)
            | Self::Refused { account, .. }
            | Self::Cancelled(account) => account,
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
            Self::Activated(grant) => grant.account,
            Self::Allowed(grant) => grant.account,
            Self::Work(work) => work.account,
            Self::Attested(evidence) => evidence.account,
            Self::BreakRecorded(account)
            | Self::Closed(account)
            | Self::OutcomeSpan(account)
            | Self::Successor(account)
            | Self::QueryRefused(account)
            | Self::Refused { account, .. }
            | Self::Cancelled(account) => account,
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
                && (work.current.is_some()
                    || matches!(
                        work.request,
                        PlanRequest::StepSuccessor { .. } | PlanRequest::OutcomeSpan { .. }
                    ))
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
        let break_needs_turn = matches!(request, PlanRequest::RecordBreak { .. })
            && self.plan_cache.committed.as_ref().is_some_and(|proof|
                matches!(proof.read().result(), s::QueryResult::PlanCurrent { current: Some(plan), .. }
                    if plan.status == s::PlanStatus::Locked));
        // activate and close author effects whose trace kinds require a turn.
        if (matches!(
            request,
            PlanRequest::Activate { .. } | PlanRequest::Close { .. }
        ) || break_needs_turn)
            && turn.is_none()
        {
            return Err(error(
                request,
                turn,
                PlanPortAdmission::InvalidAttribution,
                None,
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
            effect: None,
            break_report: None,
            successor: None,
            setup: None,
            outcome_span: None,
            held: None,
            cancelled: false,
        })));
        // Admission settles the held read: a new outcome_span replaces it,
        // and close takes it once, whether it then closes or refuses.
        match &self.plan_port_work().unwrap().request {
            PlanRequest::OutcomeSpan { .. } => self.outcome_marks = None,
            PlanRequest::Close { .. } => {
                let held = self.outcome_marks.take();
                self.plan_port_work_mut().unwrap().held = held;
            }
            _ => (),
        }
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
        if work.cancelled
            && self.read.is_none()
            && matches!(work.stage, Stage::Validate | Stage::Read | Stage::Effect)
        {
            self.plan_port_work_mut().unwrap().stage = Stage::Complete;
            return Ok(PlanPortProgress::Complete);
        }
        match work.stage {
            Stage::Validate => {
                if matches!(
                    work.request,
                    PlanRequest::StepSuccessor { .. } | PlanRequest::OutcomeSpan { .. }
                ) {
                    self.plan_port_work_mut().unwrap().stage = Stage::Read;
                    return Ok(PlanPortProgress::Progress);
                }
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
                if matches!(
                    self.plan_port_work().unwrap().request,
                    PlanRequest::RecordBreak { .. }
                ) {
                    return self.prepare_plan_break(ports.io);
                }
                if matches!(
                    self.plan_port_work().unwrap().request,
                    PlanRequest::AttestClosure { .. } | PlanRequest::Close { .. }
                ) {
                    return self.validate_plan_closure(ports.io);
                }
                let work = self.plan_port_work().unwrap();
                let check = if let PlanRequest::Activate { successor, .. } = &work.request {
                    match checked::locked(&work.request, self, work.current.as_ref().unwrap()) {
                        Ok(_) => {
                            let parent =
                                Sequence(work.parent.unwrap().0.max(successor.sequence().0));
                            self.plan_port_work_mut().unwrap().parent = Some(parent);
                            let work = self.plan_port_work().unwrap();
                            checked::activation::waiting(
                                &work.request,
                                self,
                                work.current.as_ref().unwrap(),
                            )
                        }
                        Err(failure) => Err(failure),
                    }
                } else {
                    checked::active(&work.request, self, work.current.as_ref().unwrap())
                };
                match check {
                    Ok(_) => self.plan_port_work_mut().unwrap().stage = Stage::Read,
                    Err(failure) => self.retain_plan_check_failure(failure, ports.io)?,
                }
                Ok(PlanPortProgress::Progress)
            }
            Stage::Read => self.advance_plan_port_read(ports),
            Stage::Effect => Ok(PlanPortProgress::EffectReady),
            Stage::EffectFiling => self.advance_plan_effect_filing(ports),
            Stage::EffectFiled | Stage::Summary => self.advance_plan_effect_summary(ports),
            Stage::Native => self.advance_plan_effect_native(ports),
            Stage::TemplateFault => self.advance_plan_effect_fault(ports),
            Stage::BreakReport => self.advance_plan_break_report(ports),
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
        if matches!(work.request, PlanRequest::StepSuccessor { .. }) {
            return self.finish_step_successor_read(ports.io);
        }
        if matches!(work.request, PlanRequest::OutcomeSpan { .. }) {
            return self.finish_outcome_span_read(ports.io);
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
        if matches!(
            work.request,
            PlanRequest::Activate { .. } | PlanRequest::Close { .. }
        ) {
            let turn = work.turn.clone().unwrap();
            let prepared = if matches!(work.request, PlanRequest::Close { .. }) {
                checked::closure::prepare_close(
                    &work.request,
                    work.held.as_ref(),
                    self,
                    current,
                    proof,
                    turn,
                    work.bound,
                )
            } else {
                checked::activation::prepare(&work.request, self, current, proof, turn, work.bound)
            };
            self.plan_port_work_mut().unwrap().parent = Some(parent);
            match prepared {
                Ok(prepared) => {
                    let work = self.plan_port_work_mut().unwrap();
                    work.effect = Some(PlanEffectWork::new(prepared));
                    work.stage = Stage::Effect;
                }
                Err(failure) => self.retain_plan_check_failure(failure, ports.io)?,
            }
            return Ok(PlanPortProgress::Progress);
        }
        let result = match &work.request {
            PlanRequest::RecordBreak { .. }
            | PlanRequest::Activate { .. }
            | PlanRequest::Close { .. }
            | PlanRequest::AttestClosure { .. }
            | PlanRequest::OutcomeSpan { .. }
            | PlanRequest::StepSuccessor { .. } => unreachable!(),
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

    fn finish_step_successor_read(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<PlanPortProgress, RunFailure> {
        let retained = self.plan_port_work_mut().unwrap().read.take().unwrap();
        match retained.completion {
            ReadCompletion::Recorded(read) => {
                match checked::CheckedStepSuccessor::from_read(self, read) {
                    Ok(successor) => self.plan_port_work_mut().unwrap().successor = Some(successor),
                    Err((read, detail)) => {
                        self.plan_port_work_mut().unwrap().read =
                            Some(read_driver::RetainedReadCompletion {
                                completion: ReadCompletion::Recorded(read),
                                refusal: retained.refusal,
                            });
                        return Err(self.plan_port_fault(io, detail));
                    }
                }
            }
            ReadCompletion::Refused(crate::flow::read::ReadFailure::Refused { .. })
                if retained.refusal.is_some() =>
            {
                self.plan_port_work_mut().unwrap().read = Some(retained);
            }
            _ => {
                self.plan_port_work_mut().unwrap().read = Some(retained);
                return Err(self.plan_port_fault(io, "step query lacks its complete read outcome"));
            }
        }
        self.plan_port_work_mut().unwrap().stage = Stage::Complete;
        Ok(PlanPortProgress::Complete)
    }

    /// outcome_span allocates from its recorded catalog with the one span
    /// allocator, under the loaded seed. A refused read keeps its outcome.
    fn finish_outcome_span_read(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<PlanPortProgress, RunFailure> {
        let retained = self.plan_port_work().unwrap().read.as_ref().unwrap();
        match &retained.completion {
            ReadCompletion::Recorded(read) => {
                let seed = &self.open_start.as_ref().unwrap().seed_id;
                match checked::activation::SpanAllocator::from_marks(self, read, seed) {
                    Ok(spans) => {
                        let span = spans.peek();
                        self.plan_port_work_mut().unwrap().outcome_span = Some(span);
                    }
                    Err(_) => {
                        return Err(
                            self.plan_port_fault(io, "outcome span read has no marks catalog")
                        );
                    }
                }
            }
            ReadCompletion::Refused(crate::flow::read::ReadFailure::Refused { .. })
                if retained.refusal.is_some() => {}
            _ => {
                return Err(
                    self.plan_port_fault(io, "outcome span lacks its complete read outcome")
                );
            }
        }
        self.plan_port_work_mut().unwrap().stage = Stage::Complete;
        Ok(PlanPortProgress::Complete)
    }

    /// attest_closure settles here without a read. close continues to the
    /// reread of its exact held work selector. Both consume the committed cache
    /// and the moved proof, so a refusal's parent is the later of the two.
    fn validate_plan_closure(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<PlanPortProgress, RunFailure> {
        let work = self.plan_port_work().unwrap();
        let held = match &work.request {
            PlanRequest::AttestClosure { work, .. } if work.belongs_to(self) => {
                Some(work.sequence())
            }
            PlanRequest::Close { evidence, .. } if evidence.belongs_to(self) => Some(Sequence(
                evidence.sequence().0.max(
                    work.held
                        .as_ref()
                        .filter(|held| held.belongs_to(self))
                        .map_or(0, |held| held.sequence().0),
                ),
            )),
            _ => None,
        };
        if let Some(held) = held {
            let parent = Sequence(work.parent.unwrap().0.max(held.0));
            self.plan_port_work_mut().unwrap().parent = Some(parent);
        }
        let work = self.plan_port_work().unwrap();
        let current = work.current.as_ref().unwrap();
        if matches!(work.request, PlanRequest::Close { .. }) {
            match checked::closure::validate_close(&work.request, work.held.as_ref(), self, current)
            {
                Ok(_) => self.plan_port_work_mut().unwrap().stage = Stage::Read,
                Err(failure) => self.retain_plan_check_failure(failure, io)?,
            }
            return Ok(PlanPortProgress::Progress);
        }
        match checked::closure::attest(&work.request, self, current) {
            Ok(setup) => {
                self.live(io)?;
                let work = self.plan_port_work_mut().unwrap();
                work.setup = Some(setup);
                work.stage = Stage::Complete;
                Ok(PlanPortProgress::Complete)
            }
            Err(failure) => {
                self.retain_plan_check_failure(failure, io)?;
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
            && !matches!(
                work.request,
                PlanRequest::StepSuccessor { .. } | PlanRequest::OutcomeSpan { .. }
            )
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
        } else if matches!(account.work.request, PlanRequest::StepSuccessor { .. }) {
            if account.work.successor.is_some() {
                PlanPortCompletion::Successor(account)
            } else {
                PlanPortCompletion::QueryRefused(account)
            }
        } else if matches!(account.work.request, PlanRequest::OutcomeSpan { .. }) {
            if account.work.outcome_span.is_some() {
                // The owner keeps the recorded catalog; the caller gets the span.
                let expected = account.work.request.expected().clone();
                let mut account = account;
                let retained = account.work.read.take().unwrap();
                let ReadCompletion::Recorded(read) = retained.completion else {
                    unreachable!("an allocated outcome span retains its recorded read")
                };
                self.outcome_marks = Some(HeldOutcome { expected, read });
                PlanPortCompletion::OutcomeSpan(account)
            } else {
                PlanPortCompletion::QueryRefused(account)
            }
        } else if let Some(refusal) = account.work.refusal.clone() {
            PlanPortCompletion::Refused { refusal, account }
        } else if matches!(account.work.request, PlanRequest::RecordBreak { .. }) {
            PlanPortCompletion::BreakRecorded(account)
        } else if matches!(account.work.request, PlanRequest::Activate { .. }) {
            PlanPortCompletion::Activated(ActivatedStep { account })
        } else if matches!(account.work.request, PlanRequest::AssessAct { .. }) {
            PlanPortCompletion::Allowed(AllowedAct { account })
        } else if matches!(account.work.request, PlanRequest::AttestClosure { .. }) {
            PlanPortCompletion::Attested(ClosureEvidence::from_attestation(account))
        } else if matches!(account.work.request, PlanRequest::Close { .. }) {
            PlanPortCompletion::Closed(account)
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
