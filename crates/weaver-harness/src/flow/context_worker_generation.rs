//! Move the actual accepted input through task filing, placement and generation.
//! Every workflow transition is an explicit call from the worker. No copied
//! message, caller receipt or raw decoder answer can substitute for these owners.
use super::*;
use crate::flow::clock::AbsoluteBound;
use crate::flow::context::marks_driver::BeginTaskRefusal;
use crate::flow::context::{
    GenerationCompletion, PlacementCompletion, RecordingContext, TaskCompletion,
};
use crate::flow::material::HeldMaterial;
use crate::flow::place::{PlaceError, PlaceRequest};
use crate::flow::pump::{LifecyclePeer, PumpDeadline};
use weaver_types::state_flow::{CutMethod, PlacementContext};

#[cfg(test)]
#[path = "context_worker_generation_tests.rs"]
pub(in crate::flow::context) mod tests;

#[cfg(test)]
#[path = "context_worker_admission_tests.rs"]
pub(in crate::flow::context) mod admission_tests;

#[cfg(test)]
#[path = "context_worker_queued_input_tests.rs"]
pub(in crate::flow::context) mod queued_input_tests;

#[cfg(test)]
#[path = "context_worker_return_tests.rs"]
pub(in crate::flow::context) mod return_tests;

#[path = "context_worker_emission.rs"]
pub(crate) mod emission;

#[path = "context_worker_final.rs"]
mod answer;

#[cfg(test)]
#[path = "context_worker_final_tests.rs"]
pub(in crate::flow::context) mod final_tests;

#[cfg(test)]
#[path = "context_worker_session_tests.rs"]
pub(in crate::flow::context) mod session_tests;

#[derive(Clone, Copy, PartialEq, Eq)]
enum EngineStage {
    StartTask,
    Task,
    TaskReady,
    Prepare,
    Place,
    Placed,
    StartGeneration,
    Generation,
    Generated,
    Emission,
    Refused,
    Cancelled,
    Closing,
    Answer,
}

pub(super) struct EngineWork {
    stage: EngineStage,
    admission: Option<BeginTaskRefusal>,
    task: Option<TaskCompletion>,
    material: Option<HeldMaterial>,
    selection: Option<(String, CutMethod)>,
    request: Option<PlaceRequest>,
    placement: Option<PlacementCompletion>,
    generation: Option<GenerationCompletion>,
    detail: Option<String>,
    emission: Option<Box<emission::EmissionWork>>,
    return_calls: Vec<weaver_types::state_flow::CallId>,
    return_units: Option<Vec<crate::flow::material::SelectedUnit>>,
    rounds: Vec<RetainedRound>,
    answer: Option<super::close::WorldClose>,
}
// A later model pass does not discard the decoder, repair, sort and pair
// evidence of its predecessor. The service owner retains the whole chain.
struct RetainedRound {
    _generation: Option<GenerationCompletion>,
    _emission: Option<Box<emission::EmissionWork>>,
}
impl EngineWork {
    pub(super) fn progress(&self) -> FoundationInputProgress {
        match self.stage {
            EngineStage::TaskReady => FoundationInputProgress::Task,
            EngineStage::Placed => FoundationInputProgress::Placed,
            EngineStage::Generated => FoundationInputProgress::Generated,
            EngineStage::Emission => self.emission.as_ref().unwrap().progress(),
            EngineStage::Refused | EngineStage::Cancelled => {
                FoundationInputProgress::OperationRefused
            }
            _ => FoundationInputProgress::Pending,
        }
    }
}

impl<'a> FoundationInput<'a> {
    /// Elect begin_task using the one accepted input. It waits for the actual
    /// input and mark filings before supplying task material to the next call.
    pub fn start_task(&mut self, turn: TurnKey) -> Result<(), TurnKey> {
        if !self.accepts_turn(&turn) {
            return Err(turn);
        }
        self.turn = Some(turn);
        self.stage = Stage::Engine;
        self.engine = Some(Box::new(EngineWork {
            stage: EngineStage::StartTask,
            admission: None,
            task: None,
            material: None,
            selection: None,
            request: None,
            placement: None,
            generation: None,
            detail: None,
            emission: None,
            return_calls: vec![],
            return_units: None,
            rounds: vec![],
            answer: None,
        }));
        Ok(())
    }

    /// Preflight a loop-supplied identity without consuming the accepted input.
    /// The context's existing turn set is authoritative across session wrappers.
    pub fn accepts_turn(&self, turn: &TurnKey) -> bool {
        self.progress() == FoundationInputProgress::Accepted
            && !turn.0.is_empty()
            && !self.serving.owner.context.input_turns.contains(&turn.0)
    }

    /// Select this filed task's exact source as the complete fresh selection.
    /// The current argument is retained without a cut. The caller supplies the
    /// method; with no cut the journal applies neither destructive operation.
    pub fn place_task(&mut self, label: String, method: CutMethod) -> Result<(), String> {
        if self.progress() != FoundationInputProgress::Task || label.is_empty() {
            return Err(label);
        }
        let work = self.engine.as_mut().unwrap();
        work.selection = Some((label, method));
        work.stage = EngineStage::Prepare;
        Ok(())
    }

    /// Consume the private measured/Recorded placement grant. No generation is
    /// sent by start_task, task filing, selection or placement alone.
    pub fn generate(&mut self) -> Result<(), FoundationInputProgress> {
        if self.progress() != FoundationInputProgress::Placed {
            return Err(self.progress());
        }
        self.engine.as_mut().unwrap().stage = EngineStage::StartGeneration;
        Ok(())
    }

    /// Select complete retained results for the next pass. This chooses no
    /// call, workflow or final answer, and generates nothing until generate().
    /// The no-cut placement still requires each original request and statement.
    pub fn place_results(
        &mut self,
        calls: Vec<weaver_types::state_flow::CallId>,
        label: String,
        method: CutMethod,
    ) -> Result<(), Vec<weaver_types::state_flow::CallId>> {
        if self.progress() != FoundationInputProgress::PairsComplete
            || calls.is_empty()
            || label.is_empty()
        {
            return Err(calls);
        }
        let mut units = vec![];
        for (index, call) in calls.iter().enumerate() {
            if calls[..index].contains(call) {
                return Err(calls);
            }
            let Ok(unit) = self.serving.owner.context.hold_place_return(call) else {
                return Err(calls);
            };
            units.push(unit);
        }
        // A retained terminal is not permission to append an orphan result.
        // Validate the unchanged journal before consuming this round's owners.
        let Some(journal) = self.serving.owner.context.boot_journal.as_ref() else {
            return Err(calls);
        };
        if !journal
            .project(None)
            .is_ok_and(|prefix| journal.validate_selection(&prefix, &units).is_ok())
        {
            return Err(calls);
        }
        let work = self.engine.as_mut().unwrap();
        work.rounds.push(RetainedRound {
            _generation: work.generation.take(),
            _emission: work.emission.take(),
        });
        work.return_calls = calls;
        work.selection = Some((label, method));
        work.return_units = Some(units);
        work.stage = EngineStage::Prepare;
        Ok(())
    }

    pub fn operation_detail(&self) -> Option<&str> {
        self.engine.as_ref().and_then(|w| {
            w.emission
                .as_ref()
                .and_then(|e| e.detail())
                .or(w.detail.as_deref())
        })
    }

    /// Preserve the member's exact typed read refusal for workflow judgment.
    /// The private account continues to own its release and trace evidence.
    pub fn task_read_refusal(&self) -> Option<(weaver_types::state_flow::ReadRefusalCode, &str)> {
        let TaskCompletion::Refused {
            refusal:
                crate::flow::context::marks_driver::TaskRefusal::Read(
                    crate::flow::read::ReadFailure::Refused { code, detail },
                ),
            ..
        } = self.engine.as_ref()?.task.as_ref()?
        else {
            return None;
        };
        Some((*code, detail))
    }

    pub(super) fn settle_refusal_before_stop(
        &mut self,
        peer: Option<&mut LifecyclePeer>,
        io: &mut impl PumpIo,
    ) -> Result<bool, RunFailure> {
        if self.stage == Stage::Engine
            && self.engine.as_ref().unwrap().stage == EngineStage::Answer
            && self
                .engine
                .as_ref()
                .unwrap()
                .answer
                .as_ref()
                .unwrap()
                .started()
        {
            self.advance_engine(peer, io)?;
            return Ok(false);
        }
        if self.stage == Stage::Engine
            && ((self.engine.as_ref().unwrap().stage == EngineStage::Place
                && self.serving.owner.context.placement_refusing())
                || self
                    .engine
                    .as_ref()
                    .unwrap()
                    .emission
                    .as_ref()
                    .is_some_and(|work| work.closing_correction()))
        {
            self.advance_engine(peer, io)?;
            return Ok(false);
        }
        Ok(true)
    }

    pub(super) fn advance_engine(
        &mut self,
        peer: Option<&mut LifecyclePeer>,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        if self.engine.as_ref().is_some_and(|work| {
            work.stage == EngineStage::Emission
                && work
                    .emission
                    .as_ref()
                    .is_some_and(|emission| emission.ready_to_close_refusal())
        }) {
            let close = super::close::WorldClose::refused(self).ok_or_else(|| {
                fault(
                    self.serving.owner.context,
                    io,
                    "emission refusal lost its turn or original gate request".into(),
                )
            })?;
            let work = self.engine.as_mut().unwrap();
            work.answer = Some(close);
            work.stage = EngineStage::Answer;
        }
        self.serving.owner.context.live(io)?;
        self.serving.retain_queued_input(io)?;
        let owner = &mut self.serving.owner;
        let work = self.engine.as_mut().unwrap();
        let context = &mut owner.context;
        let mut ports = PumpPorts {
            lifecycle: LifecyclePorts {
                listener: Some(&mut owner.listener),
                dial: peer,
                residency: Some(owner.resident.residency),
                ..Default::default()
            },
            gate: Some(&mut owner.gate),
            decode: Some(owner.decode),
            label: None,
            io,
        };
        match work.stage {
            EngineStage::Answer => {
                if work.answer.as_mut().unwrap().advance(context, &mut ports)? {
                    self.stage = work.answer.as_ref().unwrap().completed();
                }
            }
            EngineStage::StartTask => {
                decision(context, ports.io)?;
                let token = self.token.take().expect("actual accepted input");
                if let Err(error) = context.begin_task(
                    token,
                    TurnRef(self.turn.as_ref().unwrap().0.clone()),
                    owner
                        .certificate
                        .as_ref()
                        .expect("task owns its certificate"),
                    ports.io,
                ) {
                    self.token = Some(error.input);
                    if let Some(failure) = error.failure {
                        return Err(failure);
                    }
                    if error.refusal == BeginTaskRefusal::Busy {
                        context.pump_once(&mut ports)?;
                    } else {
                        work.detail = Some(format!("begin_task: {:?}", error.refusal));
                        work.admission = Some(error.refusal);
                        work.stage = EngineStage::Refused;
                    }
                } else {
                    work.stage = EngineStage::Task;
                }
            }
            EngineStage::Task => {
                context.advance_task(&mut ports)?;
                if let Some(done) = context.take_task(ports.io)? {
                    work.stage = match &done {
                        TaskCompletion::Begun(_) => EngineStage::TaskReady,
                        TaskCompletion::Refused { refusal, .. } => {
                            work.detail = Some(format!("begin_task: {refusal:?}"));
                            EngineStage::Refused
                        }
                        TaskCompletion::Cancelled(_) => EngineStage::Cancelled,
                    };
                    work.task = Some(done);
                } else {
                    context.pump_once(&mut ports)?;
                }
            }
            EngineStage::Prepare => {
                decision(context, ports.io)?;
                if work.return_calls.is_empty() && work.material.is_none() && work.request.is_none()
                {
                    let Some(TaskCompletion::Begun(task)) = work.task.take() else {
                        unreachable!("only a begun task grants selection")
                    };
                    match context.hold_task_material(task, ports.io) {
                        Ok(material) => work.material = Some(material),
                        Err(error) => {
                            work.task = Some(TaskCompletion::Begun(*error.task));
                            return Err(error.failure.unwrap_or_else(|| {
                                fault(
                                    context,
                                    ports.io,
                                    format!("task material: {:?}", error.kind),
                                )
                            }));
                        }
                    }
                }
                if work.request.is_none() {
                    let (label, method) = work.selection.as_ref().unwrap();
                    let prefix = match context.prepare_prefix(None, *method, ports.io) {
                        Ok(prefix) => prefix,
                        Err(PlaceError::Busy(_)) => {
                            context.pump_once(&mut ports)?;
                            return Ok(());
                        }
                        Err(error) => {
                            return Err(fault(
                                context,
                                ports.io,
                                format!("worker prefix: {error:?}"),
                            ));
                        }
                    };
                    work.request = Some(PlaceRequest {
                        prefix,
                        selection: if work.return_calls.is_empty() {
                            work.material.take().unwrap().into_units()
                        } else {
                            work.return_units.take().unwrap()
                        },
                        tool_schemas: context.loaded_place_schemas().unwrap().to_vec(),
                        context: PlacementContext {
                            label: label.clone(),
                            instruction: None,
                        },
                    });
                }
                match context.begin_place(
                    work.request.take().unwrap(),
                    ports.decode.as_deref_mut().unwrap(),
                    ports.io,
                ) {
                    Ok(()) => work.stage = EngineStage::Place,
                    Err(error) => {
                        work.request = Some(error.input);
                        if let Some(failure) = error.failure {
                            return Err(failure);
                        }
                        if !matches!(error.refusal, PlaceError::Busy(_)) {
                            return Err(fault(
                                context,
                                ports.io,
                                format!("task place: {:?}", error.refusal),
                            ));
                        }
                        context.pump_once(&mut ports)?;
                    }
                }
            }
            EngineStage::Place => {
                context.advance_place(&mut ports)?;
                if let Some(done) = context.take_placement(ports.io)? {
                    work.stage = match &done {
                        PlacementCompletion::Granted(_) => EngineStage::Placed,
                        PlacementCompletion::Refused(refused) => {
                            work.detail = Some(refused.detail.clone());
                            EngineStage::Closing
                        }
                        PlacementCompletion::Cancelled(_) => EngineStage::Cancelled,
                    };
                    work.placement = Some(done);
                }
            }
            EngineStage::StartGeneration => {
                decision(context, ports.io)?;
                let Some(PlacementCompletion::Granted(grant)) = work.placement.take() else {
                    unreachable!("only the retained private grant starts generation")
                };
                match context.begin_generation(
                    grant,
                    ports.decode.as_deref_mut().unwrap(),
                    ports.io,
                ) {
                    Ok(()) => {
                        work.stage = EngineStage::Generation;
                    }
                    Err(error) => {
                        work.placement = Some(PlacementCompletion::Granted(error.input));
                        if let Some(failure) = error.failure {
                            return Err(failure);
                        }
                        if !matches!(error.refusal, PlaceError::Busy(_)) {
                            return Err(fault(
                                context,
                                ports.io,
                                format!("task generation: {:?}", error.refusal),
                            ));
                        }
                        context.pump_once(&mut ports)?;
                    }
                }
            }
            EngineStage::Generation => {
                let progress = context.advance_generation(&mut ports)?;
                if let Some(done) = context.take_placed_generation(ports.io)? {
                    let proof = match &done {
                        GenerationCompletion::Generated(proof) => Some(proof.as_ref()),
                        GenerationCompletion::Cancelled(account)
                        | GenerationCompletion::Refused(account) => account.confirmed_generation(),
                    };
                    let acknowledgement = if let Some(proof) = proof {
                        work.return_calls
                            .iter()
                            .try_for_each(|call| context.acknowledge_delivery(call, proof))
                    } else {
                        Ok(())
                    };
                    work.stage = match &done {
                        GenerationCompletion::Generated(_) => EngineStage::Generated,
                        GenerationCompletion::Refused(_) => EngineStage::Refused,
                        GenerationCompletion::Cancelled(_) => EngineStage::Cancelled,
                    };
                    work.generation = Some(done);
                    acknowledgement.map_err(|e| {
                        fault(context, ports.io, format!("worker result delivery: {e:?}"))
                    })?;
                } else if progress == crate::flow::context::PlaceProgress::Pending {
                    context.pump_once(&mut ports)?;
                }
            }
            EngineStage::Emission => {
                work.emission.as_mut().unwrap().advance(
                    &mut work.generation,
                    &mut owner.certificate,
                    context,
                    &mut ports,
                )?;
            }
            EngineStage::Closing => {
                if self.close.is_none() && self.send.is_none() {
                    let frame = context.place_close().ok_or(RunFailure::PlaceOwned)?;
                    self.close = Some(OrganEnvelope {
                        exchange: self.received.as_ref().unwrap().envelope().exchange.clone(),
                        position: Position::Close,
                        payload: Payload::Frame(frame.clone()),
                    });
                }
                if let Some(close) = self.close.take() {
                    match ports.gate.as_deref_mut().unwrap().stage_tracked(close) {
                        Ok(TrackedStage::Queued(send)) => self.send = Some(send),
                        Ok(TrackedStage::Busy(close)) => self.close = Some(close),
                        Err(error) => {
                            self.close = Some(error.input);
                            return Err(RunFailure::Transport {
                                source: PumpSource::Gate,
                                failure: error.failure,
                            });
                        }
                    }
                }
                if self.send.as_ref().is_some_and(|send| {
                    send.sent()
                        && send.belongs_to(ports.gate.as_deref().unwrap())
                        && send
                            .matches_exchange(&self.received.as_ref().unwrap().envelope().exchange)
                }) {
                    if context.place_close().is_some() {
                        context.place_response_sent(ports.io)?;
                    }
                    self.stage = Stage::Refused;
                } else {
                    context.pump_once(&mut ports)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// A Stop owns this work until its exact in-flight exchange and filings
    /// settle. Successful retained task/output proofs remain facts, never a
    /// permission to continue after this operation has been cancelled.
    pub(super) fn cancel_engine(&mut self, io: &mut impl PumpIo) -> Result<(), RunFailure> {
        let Some(work) = self.engine.as_mut() else {
            return Ok(());
        };
        let owner = &mut self.serving.owner;
        if work.stage == EngineStage::Generated {
            work.emission = Some(Box::new(emission::EmissionWork::stopped()));
            work.stage = EngineStage::Emission;
        }
        if work.stage == EngineStage::Emission {
            return work.emission.as_mut().unwrap().cancel(
                owner.context,
                Some(owner.decode),
                Some(&mut owner.gate),
                io,
            );
        }
        owner.context.cancel_task();
        owner.context.revoke_place();
        if owner.context.measure.is_some() {
            owner.context.cancel_measurement(io)?;
        }
        if let Some(PlacementCompletion::Granted(_)) = work.placement {
            let Some(PlacementCompletion::Granted(grant)) = work.placement.take() else {
                unreachable!()
            };
            match owner.context.cancel_placement_grant(grant) {
                Ok(account) => work.placement = Some(PlacementCompletion::Cancelled(account)),
                Err(error) => {
                    work.placement = Some(PlacementCompletion::Granted(error.input));
                    return Err(error.failure.unwrap_or(RunFailure::PlaceOwned));
                }
            }
        }
        if owner.context.generation.is_some() {
            owner.context.cancel_generation(owner.decode, io)?;
        }
        if !matches!(
            work.stage,
            EngineStage::Task | EngineStage::Place | EngineStage::Generation
        ) {
            owner.context.complete_wait(PumpDeadline::Decision);
            work.stage = EngineStage::Cancelled;
        }
        Ok(())
    }

    pub(super) fn engine_settled(&self) -> bool {
        self.engine.as_ref().is_none_or(|w| {
            w.stage == EngineStage::Cancelled
                || (w.stage == EngineStage::Emission && w.emission.as_ref().unwrap().settled())
        })
    }
}

pub(super) fn decision(
    context: &mut RecordingContext,
    io: &mut impl PumpIo,
) -> Result<(), RunFailure> {
    if context.deadlines.get(PumpDeadline::Decision).is_none() {
        let elapsed = context
            .load_duty
            .as_ref()
            .unwrap()
            .instruction
            .decision_wait
            .elapsed_ns
            .0;
        let bound = AbsoluteBound::after(context.last_ns, elapsed)
            .map_err(|e| fault(context, io, e.to_string()))?;
        context.arm(PumpDeadline::Decision, bound, io)?;
    }
    Ok(())
}
fn fault(context: &mut RecordingContext, io: &mut impl PumpIo, detail: String) -> RunFailure {
    context.fail(
        io,
        FlowHaltReason::ProtocolFailed,
        HaltPoint::Barrier,
        detail,
    )
}

/// The generated/check-recorded result and every source/decoder receipt remain
/// with the original serving owner. OUTPUT may require repair; this is not a
/// final-answer or dispatch grant. Subsequent sort/pair composition consumes it.
///
/// ```compile_fail,E0616
/// fn raw(output: weaver_harness::FoundationGeneration<'_>) { let _ = output.input; }
/// ```
///
/// ```compile_fail,E0277
/// fn cloned<T: Clone>() {}
/// cloned::<weaver_harness::FoundationGeneration<'static>>();
/// ```
pub struct FoundationGeneration<'a> {
    pub(super) input: Box<FoundationInput<'a>>,
}
impl<'a> FoundationGeneration<'a> {
    pub fn into_service(self) -> Box<FoundationInput<'a>> {
        self.input
    }
    pub fn turn(&self) -> &TurnKey {
        self.input.turn.as_ref().unwrap()
    }
}
