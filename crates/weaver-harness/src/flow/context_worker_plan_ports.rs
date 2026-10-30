//! Explicit plan checks share the worker's read, author and management owners.
use super::*;
use crate::flow::clock::AbsoluteBound;
use crate::flow::context::plan_port_driver::{
    PlanPortAdmission, PlanPortCompletion, PlanPortProgress,
};
use crate::flow::pump::PumpDeadline;
use weaver_types::state_flow::{
    PlanAct, PlanOperationRefused, PlanRef, SourcePosition, SpanRef, StatementRef, StepId, WorkRef,
};

pub use crate::flow::plan::ports::{
    CheckedStepSuccessor, CheckedWork, ClosureEvidence, PlanRequest as FoundationPlanRequest,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoundationPlanAdmissionRefusal {
    Busy,
    WrongPhase,
    MissingWait,
    InvalidAttribution,
    Stopped,
}

/// Admission returns the exact unaccepted request and caller attribution.
#[derive(Debug)]
pub struct FoundationPlanOperationError {
    pub request: FoundationPlanRequest,
    pub turn: Option<TurnKey>,
    pub refusal: FoundationPlanAdmissionRefusal,
}

/// A view of a settled operation. Metadata cannot reconstruct its private grant.
#[derive(Debug)]
pub enum FoundationPlanOutcome<'a> {
    Successor(&'a weaver_types::state_flow::StepQueryRef),
    QueryRefused {
        code: weaver_types::state_flow::ReadRefusalCode,
        detail: &'a str,
    },
    Activated(&'a weaver_types::state_flow::PlanStep),
    BreakRecorded {
        current: Option<&'a weaver_types::state_flow::PlanSnapshot>,
    },
    Allowed,
    Work(&'a WorkRef),
    /// attest_closure granted unreviewed ClosureEvidence for this work.
    Attested(&'a WorkRef),
    /// outcome_span's next unallocated span from a fresh Marks read.
    OutcomeSpan(&'a SpanRef),
    /// close committed this snapshot, as its mandatory refresh adopted it.
    Closed(&'a weaver_types::state_flow::PlanSnapshot),
    Refused(&'a PlanOperationRefused),
    Cancelled,
}

/// The completed owner retains its request, read proofs and private grant.
/// Inspecting it dispatches nothing. Later consumers must check this context,
/// the current revision and the exact operation before using its grant.
///
/// ```compile_fail,E0277
/// fn cloned<T: Clone>() {}
/// cloned::<weaver_harness::FoundationPlanOperation>();
/// ```
pub struct FoundationPlanOperation {
    completion: PlanPortCompletion,
}

impl FoundationPlanOperation {
    /// Consume the settled query proof. Refusal/cancellation returns the entire
    /// original result, never a constructed successor from public metadata.
    pub fn into_successor(self) -> Result<CheckedStepSuccessor, Self> {
        self.completion
            .into_successor()
            .map_err(|completion| Self { completion })
    }

    /// Move the checked work into attest_closure. Any other result returns
    /// the entire original operation; WorkRef metadata cannot rebuild it.
    pub fn into_checked_work(self) -> Result<CheckedWork, Self> {
        self.completion
            .into_checked_work()
            .map_err(|completion| Self { completion })
    }

    /// Move the single-use grant into close. Any other result returns whole.
    pub fn into_closure_evidence(self) -> Result<ClosureEvidence, Self> {
        self.completion
            .into_closure_evidence()
            .map_err(|completion| Self { completion })
    }

    pub fn request(&self) -> &FoundationPlanRequest {
        self.completion.request()
    }
    pub fn outcome(&self) -> FoundationPlanOutcome<'_> {
        match &self.completion {
            PlanPortCompletion::Successor(_) => {
                FoundationPlanOutcome::Successor(self.completion.successor().unwrap().reference())
            }
            PlanPortCompletion::QueryRefused(_) => {
                let (code, detail) = self.completion.query_refusal().unwrap();
                FoundationPlanOutcome::QueryRefused { code, detail }
            }
            PlanPortCompletion::Activated(step) => FoundationPlanOutcome::Activated(step.step()),
            PlanPortCompletion::BreakRecorded(_) => FoundationPlanOutcome::BreakRecorded {
                current: self.completion.current_snapshot(),
            },
            PlanPortCompletion::Allowed(_) => FoundationPlanOutcome::Allowed,
            PlanPortCompletion::Work(work) => FoundationPlanOutcome::Work(work.reference()),
            PlanPortCompletion::Attested(evidence) => {
                FoundationPlanOutcome::Attested(evidence.work())
            }
            PlanPortCompletion::OutcomeSpan(_) => FoundationPlanOutcome::OutcomeSpan(
                self.completion
                    .outcome_span()
                    .expect("a settled outcome_span retains its span"),
            ),
            PlanPortCompletion::Closed(_) => FoundationPlanOutcome::Closed(
                self.completion
                    .current_snapshot()
                    .expect("a committed close retains its refreshed plan"),
            ),
            PlanPortCompletion::Refused { refusal, .. } => FoundationPlanOutcome::Refused(refusal),
            PlanPortCompletion::Cancelled(_) => FoundationPlanOutcome::Cancelled,
        }
    }
}

impl std::fmt::Debug for FoundationPlanOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FoundationPlanOperation")
            .field("request", self.request())
            .field("outcome", &self.outcome())
            .finish()
    }
}

pub(in crate::flow::context::worker_startup) struct PendingPlanOperation {
    owned_decision: Option<AbsoluteBound>,
    result: Option<Box<FoundationPlanOperation>>,
    cancelled: bool,
}

impl FoundationInput<'_> {
    /// Read only the exact successor selector elected by the caller. A refused
    /// STEP-QUERY keeps its ordinary read outcome and cannot activate a step.
    #[allow(clippy::result_large_err)]
    pub fn step_successor(
        &mut self,
        expected: PlanRef,
        previous_step: Option<StepId>,
        turn: Option<TurnKey>,
    ) -> Result<(), FoundationPlanOperationError> {
        self.begin_plan_operation(
            FoundationPlanRequest::StepSuccessor {
                expected,
                previous_step,
            },
            turn,
            &mut MonotonicIo,
        )
    }

    /// Activate only this checked successor. The original Decision bounds the
    /// allocation, effect, summary refresh and native-scope adoption.
    #[allow(clippy::result_large_err)]
    pub fn activate(
        &mut self,
        expected: PlanRef,
        step_id: StepId,
        successor: CheckedStepSuccessor,
        turn: TurnKey,
    ) -> Result<(), FoundationPlanOperationError> {
        self.begin_plan_operation(
            FoundationPlanRequest::Activate {
                expected,
                step_id,
                successor,
            },
            Some(turn),
            &mut MonotonicIo,
        )
    }

    /// Record only the requested break. A locked plan requires an open turn
    /// for its classified effect. Without a locked plan this records metadata
    /// alone and returns the held current snapshot, which may be absent.
    #[allow(clippy::result_large_err)]
    pub fn record_break(
        &mut self,
        expected: PlanRef,
        step_id: Option<StepId>,
        case: weaver_types::state_flow::PlanBreakCase,
        proposed_source: Option<weaver_types::state_flow::SourceBlockId>,
        turn: Option<TurnKey>,
    ) -> Result<(), FoundationPlanOperationError> {
        self.begin_plan_operation(
            FoundationPlanRequest::RecordBreak {
                expected,
                step_id,
                case,
                proposed_source,
            },
            turn,
            &mut MonotonicIo,
        )
    }

    /// Elect only the checked predicate, without dispatch or plan mutation.
    #[allow(clippy::result_large_err)]
    pub fn assess_act(
        &mut self,
        expected: PlanRef,
        step_id: StepId,
        statement: StatementRef,
        act: PlanAct,
        turn: Option<TurnKey>,
    ) -> Result<(), FoundationPlanOperationError> {
        self.begin_plan_operation(
            FoundationPlanRequest::AssessAct {
                expected,
                step_id,
                statement,
                act,
            },
            turn,
            &mut MonotonicIo,
        )
    }

    /// Request the exact active span and committed frontier. No result escapes
    /// until the admitted read, release and any refusal record have settled.
    #[allow(clippy::result_large_err)]
    pub fn work_reference(
        &mut self,
        expected: PlanRef,
        step_id: StepId,
        span: SpanRef,
        through: SourcePosition,
        turn: Option<TurnKey>,
    ) -> Result<(), FoundationPlanOperationError> {
        self.begin_plan_operation(
            FoundationPlanRequest::WorkReference {
                expected,
                step_id,
                span,
                through,
            },
            turn,
            &mut MonotonicIo,
        )
    }

    /// Attest unreviewed closure for this moved checked work under the
    /// operator's category. It reads nothing and files nothing.
    #[allow(clippy::result_large_err)]
    pub fn attest_closure(
        &mut self,
        expected: PlanRef,
        step_id: StepId,
        work: CheckedWork,
        turn: Option<TurnKey>,
    ) -> Result<(), FoundationPlanOperationError> {
        self.begin_plan_operation(
            FoundationPlanRequest::AttestClosure {
                expected,
                step_id,
                work,
            },
            turn,
            &mut MonotonicIo,
        )
    }

    /// Allocate close's outcome_span: one fresh recorded Marks read through
    /// the plan port, whose outcome shows the next unallocated span. It holds
    /// nothing for close and files nothing; a refused read keeps its outcome.
    #[allow(clippy::result_large_err)]
    pub fn outcome_span(
        &mut self,
        expected: PlanRef,
        turn: Option<TurnKey>,
    ) -> Result<(), FoundationPlanOperationError> {
        self.begin_plan_operation(
            FoundationPlanRequest::OutcomeSpan { expected },
            turn,
            &mut MonotonicIo,
        )
    }

    /// Close the active step with this consumed grant. After the work reread
    /// close takes its own Marks read, and refuses invalid_target unless
    /// outcome_span is that read's next allocation exactly (as from
    /// [`Self::outcome_span`]). The original Decision bounds both reads, the
    /// step.outcome, its refresh and native adoption.
    ///
    /// The loop cannot supply a Marks read: the request has no such field,
    /// and no read or grant type for one is public.
    ///
    /// ```compile_fail,E0026
    /// fn held(request: weaver_harness::FoundationPlanRequest) {
    ///     if let weaver_harness::FoundationPlanRequest::Close { marks, .. } = request {}
    /// }
    /// ```
    ///
    /// ```compile_fail,E0425
    /// fn held(_: weaver_harness::OutcomeMarks) {}
    /// ```
    #[allow(clippy::result_large_err)]
    pub fn close(
        &mut self,
        expected: PlanRef,
        step_id: StepId,
        evidence: ClosureEvidence,
        outcome_span: SpanRef,
        turn: TurnKey,
    ) -> Result<(), FoundationPlanOperationError> {
        self.begin_plan_operation(
            FoundationPlanRequest::Close {
                expected,
                step_id,
                evidence,
                outcome_span,
            },
            Some(turn),
            &mut MonotonicIo,
        )
    }

    #[allow(clippy::result_large_err)]
    fn begin_plan_operation(
        &mut self,
        request: FoundationPlanRequest,
        turn: Option<TurnKey>,
        io: &mut impl PumpIo,
    ) -> Result<(), FoundationPlanOperationError> {
        let error = |request, turn, refusal| FoundationPlanOperationError {
            request,
            turn,
            refusal,
        };
        if let Err(failure) = self.serving.owner.context.live(io) {
            self.failure = Some(failure);
            self.stage = Stage::Stopped;
        }
        if self.stage == Stage::Stopped {
            return Err(error(
                request,
                turn,
                FoundationPlanAdmissionRefusal::Stopped,
            ));
        }
        if self.serving.plan_operation.is_some()
            || self.serving.owner.listener.held().is_some()
            || self.plan_work_pending()
            || self.serving.save.is_some()
            || self.has_graceful_leave()
            || self.report.is_some()
        {
            return Err(error(request, turn, FoundationPlanAdmissionRefusal::Busy));
        }
        let attribution = turn.as_ref().map(|turn| TurnRef(turn.0.clone()));
        let context = &mut self.serving.owner.context;
        if let Err(refusal) = context.plan_port_admission(attribution.as_ref()) {
            return Err(error(request, turn, admission_refusal(refusal)));
        }
        let previous = context.deadlines.get(PumpDeadline::Decision);
        if let Err(failure) = generation::decision(context, io) {
            self.failure = Some(failure);
            self.stage = Stage::Stopped;
            return Err(error(
                request,
                turn,
                FoundationPlanAdmissionRefusal::Stopped,
            ));
        }
        let owned_decision = previous
            .is_none()
            .then(|| context.deadlines.get(PumpDeadline::Decision).unwrap());
        match context.begin_plan_operation(request, attribution, io) {
            Ok(()) => {
                self.serving.plan_operation = Some(Box::new(PendingPlanOperation {
                    owned_decision,
                    result: None,
                    cancelled: false,
                }));
                Ok(())
            }
            Err(rejected) => {
                if let Some(failure) = rejected.failure {
                    self.failure = Some(failure);
                    self.stage = Stage::Stopped;
                } else if let Some(bound) = owned_decision
                    && (context.deadlines.get(PumpDeadline::Decision) != Some(bound)
                        || context.complete_wait(PumpDeadline::Decision) != Some(bound))
                {
                    self.failure = Some(context.fail(
                        io,
                        FlowHaltReason::ProtocolFailed,
                        HaltPoint::Barrier,
                        "unaccepted plan operation lost its own Decision",
                    ));
                    self.stage = Stage::Stopped;
                }
                Err(error(
                    rejected.request,
                    turn,
                    if self.stage == Stage::Stopped {
                        FoundationPlanAdmissionRefusal::Stopped
                    } else {
                        admission_refusal(rejected.refusal)
                    },
                ))
            }
        }
    }

    pub fn plan_operation_pending(&self) -> bool {
        self.serving
            .plan_operation
            .as_ref()
            .is_some_and(|work| work.result.is_none())
    }

    pub fn take_plan_operation(&mut self) -> Option<FoundationPlanOperation> {
        if let Err(failure) = self.serving.owner.context.live(&mut MonotonicIo) {
            self.failure = Some(failure);
            self.stage = Stage::Stopped;
        }
        if self.stage == Stage::Stopped || self.plan_operation_pending() {
            return None;
        }
        self.serving
            .plan_operation
            .take()
            .and_then(|work| work.result.map(|result| *result))
    }

    pub(super) fn cancel_plan_operation(&mut self) {
        self.serving.owner.context.cancel_plan_operation();
        if let Some(work) = self.serving.plan_operation.as_mut() {
            work.cancelled = true;
            // Move the original grant into its cancelled account. No new proof
            // or successful continuation is exposed after this election.
            if let Some(result) = work.result.take() {
                work.result = Some(Box::new(FoundationPlanOperation {
                    completion: result.completion.cancel(),
                }));
            }
        }
    }

    pub(super) fn advance_plan_operation(
        &mut self,
        peer: Option<&mut LifecyclePeer>,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        if !self.plan_operation_pending() {
            return Ok(());
        }
        let owner = &mut self.serving.owner;
        let mut ports = PumpPorts {
            lifecycle: LifecyclePorts {
                listener: Some(&mut owner.listener),
                dial: peer,
                residency: Some(owner.resident.residency),
                leave_peer: plan::retained_port(&mut self.serving.plan, &mut self.serving.save),
                ..Default::default()
            },
            gate: Some(&mut owner.gate),
            decode: Some(owner.decode),
            label: None,
            io,
        };
        loop {
            match owner.context.advance_plan_operation(&mut ports)? {
                PlanPortProgress::EffectReady => {
                    let Some(certificate) = owner.certificate.as_ref() else {
                        return Err(owner.context.fail(
                            ports.io,
                            FlowHaltReason::ProtocolFailed,
                            HaltPoint::Barrier,
                            "plan effect lost its loaded template certificate",
                        ));
                    };
                    owner.context.begin_plan_effect(certificate, ports.io)?;
                }
                PlanPortProgress::Progress => (),
                PlanPortProgress::Pending => {
                    owner.context.pump_once(&mut ports)?;
                    return Ok(());
                }
                PlanPortProgress::Complete => {
                    let completion = owner
                        .context
                        .take_plan_operation(ports.io)?
                        .expect("complete plan operation retains its account");
                    let matches = completion.belongs_to(owner.context);
                    let work = self.serving.plan_operation.as_mut().unwrap();
                    let completion = if work.cancelled {
                        completion.cancel()
                    } else {
                        completion
                    };
                    work.result = Some(Box::new(FoundationPlanOperation { completion }));
                    if !matches {
                        return Err(owner.context.fail(
                            ports.io,
                            FlowHaltReason::ProtocolFailed,
                            HaltPoint::Author,
                            "plan operation completion changed its recording context",
                        ));
                    }
                    if let Some(bound) = work.owned_decision {
                        if owner.context.deadlines.get(PumpDeadline::Decision) != Some(bound)
                            || owner.context.complete_wait(PumpDeadline::Decision) != Some(bound)
                        {
                            return Err(owner.context.fail(
                                ports.io,
                                FlowHaltReason::ProtocolFailed,
                                HaltPoint::Author,
                                "plan operation completion lost its own Decision",
                            ));
                        }
                        work.owned_decision = None;
                    }
                    return Ok(());
                }
            }
        }
    }
}

fn admission_refusal(refusal: PlanPortAdmission) -> FoundationPlanAdmissionRefusal {
    match refusal {
        PlanPortAdmission::Busy => FoundationPlanAdmissionRefusal::Busy,
        PlanPortAdmission::WrongPhase => FoundationPlanAdmissionRefusal::WrongPhase,
        PlanPortAdmission::MissingWait => FoundationPlanAdmissionRefusal::MissingWait,
        PlanPortAdmission::InvalidAttribution => FoundationPlanAdmissionRefusal::InvalidAttribution,
    }
}

#[cfg(test)]
#[path = "context_worker_plan_ports_tests.rs"]
pub(in crate::flow::context::worker_startup) mod tests;
