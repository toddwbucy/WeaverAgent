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

pub use crate::flow::plan::ports::PlanRequest as FoundationPlanRequest;

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
    Allowed,
    Work(&'a WorkRef),
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
    pub fn request(&self) -> &FoundationPlanRequest {
        self.completion.request()
    }
    pub fn outcome(&self) -> FoundationPlanOutcome<'_> {
        match &self.completion {
            PlanPortCompletion::Allowed(_) => FoundationPlanOutcome::Allowed,
            PlanPortCompletion::Work(work) => FoundationPlanOutcome::Work(work.reference()),
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
            || self.plan_pending()
            || self.flags_pending()
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
