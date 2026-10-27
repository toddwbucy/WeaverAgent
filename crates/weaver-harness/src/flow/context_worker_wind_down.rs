//! Certified lifecycle instructions and their real turn stay with the original
//! Leave. The worker elects every pass, repair and close; no world reply exists.
use super::*;
use crate::flow::clock::AbsoluteBound;
use crate::flow::context::{
    FilingCompletion, FilingProgress, GenerationCompletion, PlacementCompletion, RecordingContext,
};
use crate::flow::ledger::{LedgerFile, SourceDisposition};
use crate::flow::material::SelectedUnit;
use crate::flow::place::{PlaceError, PlaceRequest};
use crate::flow::pump::PumpDeadline;
use crate::flow::templates::{ExpansionIdentity, TemplateExpansion, expand_wind_down};
use generation::emission::{EmissionWork, FoundationRestatement};
use weaver_trace::{Sequence, StopReason, TurnClose};
use weaver_types::state_flow::{CutMethod, InstructionRefusal, PlacementContext, SourceBlockId};
use weaver_types::{FaultCase, FaultReport, RefusalRecord};

#[path = "context_worker_wind_down_cancel.rs"]
mod cancel;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoundationWindDownProgress {
    Pending,
    Placed,
    Generated,
    Restatement,
    CorrectionPlaced,
    Effective,
    Refused,
    /// Every associated pair has its terminal filing. The worker chooses
    /// another pass only after a clean result, or the trace-only turn close.
    PassComplete,
    Complete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WindStage {
    Start,
    Instruction,
    Filing,
    Prepare,
    Place,
    Placed,
    Generate,
    Generation,
    Generated,
    Emission,
    Account,
    PassComplete,
    Closing,
    Complete,
}

struct Pass {
    expansion: Option<TemplateExpansion>,
    event: Option<AuthoredEvent>,
    filing: Option<FilingCompletion>,
    instruction: Option<SourceBlockId>,
    unit: Option<SelectedUnit>,
    request: Option<PlaceRequest>,
    placement: Option<PlacementCompletion>,
    generation: Option<GenerationCompletion>,
    emission: Option<Box<EmissionWork>>,
    refusal: Option<AuthoredEvent>,
    refusal_started: bool,
    returned_refusal: StoppedAuthor,
    refusal_record: Option<Sequence>,
}
impl Pass {
    fn new() -> Self {
        Self {
            expansion: None,
            event: None,
            filing: None,
            instruction: None,
            unit: None,
            request: None,
            placement: None,
            generation: None,
            emission: None,
            refusal: None,
            refusal_started: false,
            returned_refusal: StoppedAuthor::default(),
            refusal_record: None,
        }
    }
}

pub(in crate::flow::context::worker_startup) struct WindDown {
    pub(super) began_ns: u64,
    pub(super) bound: Option<AbsoluteBound>,
    turn: TurnRef,
    method: CutMethod,
    stage: WindStage,
    pass: Pass,
    previous: Vec<Pass>,
    outcome: TurnClose,
    started: Option<Sequence>,
    closed: Option<Sequence>,
    author_started: bool,
    turn_event: Option<AuthoredEvent>,
    unadmitted_close: Option<AuthoredEvent>,
    returned_author: StoppedAuthor,
    cancelled: bool,
    cancel_settled: bool,
}
impl WindDown {
    pub(super) fn settled(&self) -> bool {
        self.stage == WindStage::Complete && self.started.is_some() && self.closed.is_some()
    }
    pub(super) fn active(&self) -> bool {
        self.stage != WindStage::Complete
    }
    fn progress(&self) -> FoundationWindDownProgress {
        use FoundationWindDownProgress as P;
        match self.stage {
            WindStage::Placed => P::Placed,
            WindStage::Generated => P::Generated,
            WindStage::Emission => match self.pass.emission.as_ref().unwrap().progress() {
                FoundationInputProgress::Restatement => P::Restatement,
                FoundationInputProgress::CorrectionPlaced => P::CorrectionPlaced,
                FoundationInputProgress::Effective => P::Effective,
                FoundationInputProgress::OperationRefused => P::Refused,
                _ => P::Pending,
            },
            WindStage::PassComplete => P::PassComplete,
            WindStage::Complete => P::Complete,
            _ => P::Pending,
        }
    }
}

impl FoundationInput<'_> {
    /// Only the retained Leave and its completed Quiesce authorize this phase.
    /// The caller supplies a fresh identity; this creates no external input.
    pub fn begin_graceful_wind_down(
        &mut self,
        turn: TurnKey,
        method: CutMethod,
    ) -> Result<(), TurnKey> {
        self.begin_graceful_wind_down_with_io(turn, method, &mut MonotonicIo)
    }
    fn begin_graceful_wind_down_with_io(
        &mut self,
        turn: TurnKey,
        method: CutMethod,
        io: &mut impl PumpIo,
    ) -> Result<(), TurnKey> {
        if self.progress() != FoundationInputProgress::Quiesced
            || turn.0.is_empty()
            || self
                .serving
                .graceful
                .as_ref()
                .is_none_or(|leave| leave.forced || leave.wind_down.is_some())
            || self.serving.owner.context.open_turn.is_some()
            || self.serving.owner.context.wind_down.is_some()
            || self.serving.owner.context.input_turns.contains(&turn.0)
            || self.serving.owner.context.input.snapshot().is_some()
            || self.serving.owner.context.marks.is_some()
            || self
                .serving
                .owner
                .context
                .deadlines
                .get(PumpDeadline::GracefulDrain)
                .is_some()
            || self
                .serving
                .owner
                .context
                .deadlines
                .get(PumpDeadline::Leave)
                .is_some()
        {
            return Err(turn);
        }
        let context = &mut self.serving.owner.context;
        if let Err(failure) = context.live(io) {
            self.failure = Some(failure);
            self.stage = Stage::Stopped;
            return Err(turn);
        }
        let began_ns = context.last_ns;
        let bound = self
            .serving
            .graceful
            .as_ref()
            .unwrap()
            .wind_down_seconds
            .map(|seconds| {
                seconds
                    .checked_mul(1_000_000_000)
                    .and_then(|elapsed| AbsoluteBound::after(began_ns, elapsed).ok())
            });
        if matches!(bound, Some(None)) {
            return Err(turn);
        }
        let bound = bound.flatten();
        let reference = TurnRef(turn.0.clone());
        context.input_turns.insert(turn.0.clone());
        context.wind_down = Some((reference.clone(), None));
        self.serving.graceful.as_mut().unwrap().wind_down = Some(Box::new(WindDown {
            began_ns,
            bound,
            turn: reference,
            method,
            stage: WindStage::Start,
            pass: Pass::new(),
            previous: Vec::new(),
            outcome: TurnClose::Clean,
            started: None,
            closed: None,
            author_started: false,
            turn_event: None,
            unadmitted_close: None,
            returned_author: StoppedAuthor::default(),
            cancelled: false,
            cancel_settled: false,
        }));
        if let Some(bound) = bound
            && let Err(failure) = context.arm(PumpDeadline::WindDown, bound, io)
        {
            self.failure = Some(failure);
            self.stage = Stage::Stopped;
        }
        Ok(())
    }
    pub fn wind_down_progress(&self) -> Option<FoundationWindDownProgress> {
        self.serving
            .graceful
            .as_ref()?
            .wind_down
            .as_ref()
            .map(|work| work.progress())
    }
    pub(super) fn wind_down_input_progress(&self) -> Option<FoundationInputProgress> {
        let leave = self.serving.graceful.as_ref()?;
        leave.wind_down.as_ref()?;
        Some(if leave.forced {
            FoundationInputProgress::LeavePending
        } else {
            FoundationInputProgress::WindDown
        })
    }
    pub fn wind_down_can_continue(&self) -> bool {
        self.progress() == FoundationInputProgress::WindDown
            && self
                .serving
                .graceful
                .as_ref()
                .and_then(|leave| leave.wind_down.as_ref())
                .is_some_and(|work| {
                    work.stage == WindStage::PassComplete
                        && matches!(work.outcome, TurnClose::Clean)
                })
    }
    pub fn next_wind_down_pass(&mut self) -> bool {
        if !self.wind_down_can_continue() {
            return false;
        }
        let work = self
            .serving
            .graceful
            .as_mut()
            .unwrap()
            .wind_down
            .as_mut()
            .unwrap();
        work.previous
            .push(std::mem::replace(&mut work.pass, Pass::new()));
        work.stage = WindStage::Instruction;
        self.serving.owner.context.wind_down.as_mut().unwrap().1 = None;
        true
    }
    pub fn generate_wind_down(&mut self) -> bool {
        if self.progress() != FoundationInputProgress::WindDown
            || self.wind_down_progress() != Some(FoundationWindDownProgress::Placed)
        {
            return false;
        }
        self.serving
            .graceful
            .as_mut()
            .unwrap()
            .wind_down
            .as_mut()
            .unwrap()
            .stage = WindStage::Generate;
        true
    }
    pub fn repair_wind_down(&mut self, family: &'static FoundationRestatement) -> bool {
        if self.progress() != FoundationInputProgress::WindDown
            || self.wind_down_progress() != Some(FoundationWindDownProgress::Generated)
        {
            return false;
        }
        let certificate = self.serving.owner.certificate.as_ref().unwrap();
        if family
            .template
            .expand(
                certificate,
                ExpansionIdentity {
                    run_id: certificate.run_id(),
                    scheme: certificate.scheme(),
                    checker_artifact_sha256: certificate.checker_artifact_sha256(),
                    inventory_sha256: certificate.inventory_sha256(),
                },
            )
            .is_err()
        {
            return false;
        }
        let work = self
            .serving
            .graceful
            .as_mut()
            .unwrap()
            .wind_down
            .as_mut()
            .unwrap();
        work.pass.emission = Some(Box::new(EmissionWork::new(Some(&family.template))));
        work.stage = WindStage::Emission;
        true
    }
    pub fn place_wind_down_restatement(&mut self) -> bool {
        if self.progress() != FoundationInputProgress::WindDown
            || self.wind_down_progress() != Some(FoundationWindDownProgress::Restatement)
        {
            return false;
        }
        let work = self
            .serving
            .graceful
            .as_mut()
            .unwrap()
            .wind_down
            .as_mut()
            .unwrap();
        work.pass
            .emission
            .as_mut()
            .unwrap()
            .place_restatement("wind down correction".into(), work.method)
            .is_ok()
    }
    pub fn generate_wind_down_correction(&mut self) -> bool {
        if self.progress() != FoundationInputProgress::WindDown
            || self.wind_down_progress() != Some(FoundationWindDownProgress::CorrectionPlaced)
        {
            return false;
        }
        self.serving
            .graceful
            .as_mut()
            .unwrap()
            .wind_down
            .as_mut()
            .unwrap()
            .pass
            .emission
            .as_mut()
            .unwrap()
            .generate_correction()
            .is_ok()
    }
    pub fn finish_wind_down_pass(&mut self) -> bool {
        if self.progress() != FoundationInputProgress::WindDown {
            return false;
        }
        let leave = self.serving.graceful.as_mut().unwrap();
        let work = leave.wind_down.as_mut().unwrap();
        match work.progress() {
            FoundationWindDownProgress::Effective => {
                let calls = work
                    .pass
                    .emission
                    .as_mut()
                    .unwrap()
                    .finish_wind_down()
                    .unwrap();
                if calls {
                    work.outcome = TurnClose::Stopped {
                        reason: StopReason::Unload,
                        cause: Some(weaver_trace::Cause {
                            uid: leave.cause.uid,
                        }),
                    };
                }
            }
            FoundationWindDownProgress::Refused => {
                let emission = work.pass.emission.as_mut().unwrap();
                if emission.close_correction_refusal().is_err() && !emission.close_refusal() {
                    return false;
                }
                work.outcome = refused();
            }
            _ => return false,
        }
        work.stage = WindStage::Account;
        true
    }
    pub fn close_wind_down(&mut self) -> bool {
        if self.progress() != FoundationInputProgress::WindDown
            || self.wind_down_progress() != Some(FoundationWindDownProgress::PassComplete)
        {
            return false;
        }
        self.serving
            .graceful
            .as_mut()
            .unwrap()
            .wind_down
            .as_mut()
            .unwrap()
            .stage = WindStage::Closing;
        true
    }
    pub(super) fn retain_wind_down_expiry(&mut self, failure: &RunFailure) -> bool {
        let RunFailure::Wait {
            key: PumpDeadline::WindDown,
            bound,
        } = failure
        else {
            return false;
        };
        let Some(leave) = self.serving.graceful.as_mut() else {
            return false;
        };
        if leave
            .wind_down
            .as_ref()
            .is_none_or(|work| work.bound != Some(*bound))
            || self
                .serving
                .owner
                .context
                .deadlines
                .get(PumpDeadline::WindDown)
                != Some(*bound)
            || self.serving.owner.context.failure().is_some()
        {
            return false;
        }
        leave.forced = true;
        self.serving
            .owner
            .context
            .complete_wait(PumpDeadline::WindDown);
        true
    }
    pub(super) fn advance_wind_down(&mut self, io: &mut impl PumpIo) -> Result<(), RunFailure> {
        let serving = &mut self.serving;
        let owner = &mut serving.owner;
        let work = serving
            .graceful
            .as_mut()
            .unwrap()
            .wind_down
            .as_mut()
            .unwrap();
        let context = &mut *owner.context;
        context.live(io)?;
        let mut ports = PumpPorts {
            lifecycle: LifecyclePorts {
                listener: Some(&mut owner.listener),
                residency: Some(owner.resident.residency),
                ..Default::default()
            },
            gate: Some(&mut owner.gate),
            decode: Some(owner.decode),
            label: None,
            io,
        };
        if (work.pass.refusal_started || work.pass.refusal.is_some())
            && work.pass.refusal_record.is_none()
        {
            return record_refusal(work, context, &mut ports);
        }
        match work.stage {
            WindStage::Start | WindStage::Closing => {
                work.record_turn(context, &mut ports)?;
            }
            WindStage::Instruction => {
                generation::decision(context, ports.io)?;
                if work.pass.event.is_none() && work.pass.refusal.is_none() {
                    let certificate = owner.certificate.as_ref().unwrap();
                    let load = context.load_duty.as_ref().unwrap();
                    let expansion = expand_wind_down(
                        certificate,
                        ExpansionIdentity {
                            run_id: context.flow.run(),
                            scheme: &context.declaration.scheme,
                            checker_artifact_sha256: certificate.checker_artifact_sha256(),
                            inventory_sha256: certificate.inventory_sha256(),
                        },
                    );
                    match expansion {
                        Ok(expansion)
                            if context.repair_certificate_matches(certificate)
                                && expansion.inventory_sha256().as_str()
                                    == load.template_inventory.inventory_sha256 =>
                        {
                            work.pass.event = Some(AuthoredEvent {
                                kind: serving::Kind::LoopInstruction,
                                subsystem: Subsystem::Harness,
                                turn: Some(work.turn.clone()),
                                source: AuthoredSource::Loop,
                                causal_parent: None,
                                payload: Some(serving::Payload::LoopInstruction(Box::new(
                                    serving::payloads::LoopInstructionRecord {
                                        template_id: expansion.template_id().into(),
                                        message: serde_json::value::to_raw_value(
                                            expansion.message(),
                                        )
                                        .expect("certified Message"),
                                        instruction:
                                            serving::payloads::LoopInstructionMeta::WindDown {},
                                    },
                                ))),
                            });
                            work.pass.expansion = Some(expansion);
                        }
                        _ => {
                            instruction_refusal(
                                work,
                                InstructionRefusal::InvalidTemplate,
                                "wind-down family is not in this run's certificate",
                            );
                        }
                    }
                }
                if work.pass.refusal.is_some() {
                    return record_refusal(work, context, &mut ports);
                }
                match context.begin_filing(
                    work.pass.event.take().unwrap(),
                    PumpDeadline::Decision,
                    ports.io,
                ) {
                    Ok(()) => work.stage = WindStage::Filing,
                    Err(error) => {
                        work.pass.event = Some(error.event);
                        if let Some(failure) = error.failure {
                            return Err(failure);
                        }
                        if error.refusal != crate::flow::context::FilingBeginRefusal::Busy {
                            return Err(fault(
                                context,
                                ports.io,
                                format!("wind-down instruction filing: {:?}", error.refusal),
                            ));
                        }
                        context.pump_once(&mut ports)?;
                    }
                }
            }
            WindStage::Filing => {
                if work.pass.filing.is_none() {
                    let progress = context.advance_filing(&mut ports)?;
                    work.pass.filing = context.take_filing(ports.io)?;
                    if work.pass.filing.is_none() {
                        if progress == FilingProgress::Pending {
                            context.pump_once(&mut ports)?;
                        }
                        return Ok(());
                    }
                }
                if rejected_filing(work) {
                    return record_refusal(work, context, &mut ports);
                }
                let proof = work.pass.filing.as_ref().unwrap();
                let sources: Vec<_> = proof.receipts().dispositions().collect();
                let valid = proof.belongs_to(context)
                    && sources.len() == 1
                    && matches!(sources[0], SourceDisposition::Committed(source)
                    if matches!(source.file(), LedgerFile::Statement(file) if file.source.kind == "loop.instruction" && file.source.origin == "loop"
                        && file.block_id.run == context.flow.run() && file.block_id.sequence.0 == proof.sequence().0
                        && file.text == work.pass.expansion.as_ref().unwrap().text()));
                if !valid {
                    instruction_refusal(
                        work,
                        InstructionRefusal::InvalidContext,
                        "wind-down instruction has no exact selected filing",
                    );
                    return record_refusal(work, context, &mut ports);
                }
                let proof = work.pass.filing.take().unwrap();
                let SourceDisposition::Committed(source) = proof
                    .into_receipts()
                    .into_dispositions()
                    .unwrap()
                    .next()
                    .unwrap()
                else {
                    unreachable!()
                };
                work.pass.instruction = Some(source.source().clone());
                context.wind_down.as_mut().unwrap().1 = work.pass.instruction.clone();
                // The move-only receipt remains in SelectedUnit and then in the
                // real placement grant. Neither copied text nor an ID grants use.
                work.pass.unit = Some(
                    crate::flow::material::from_filed(
                        std::sync::Arc::clone(&context.read_origin),
                        source,
                    )
                    .expect("checked complete instruction"),
                );
                work.stage = WindStage::Prepare;
            }
            WindStage::Prepare => {
                generation::decision(context, ports.io)?;
                if work.pass.request.is_none() {
                    let prefix = match context.prepare_prefix(None, work.method, ports.io) {
                        Ok(prefix) => prefix,
                        Err(PlaceError::Busy(_)) => {
                            context.pump_once(&mut ports)?;
                            return Ok(());
                        }
                        Err(error) => {
                            return Err(fault(
                                context,
                                ports.io,
                                format!("wind-down prefix: {error:?}"),
                            ));
                        }
                    };
                    work.pass.request = Some(PlaceRequest {
                        prefix,
                        selection: vec![work.pass.unit.take().unwrap()],
                        tool_schemas: context.loaded_place_schemas().unwrap().to_vec(),
                        context: PlacementContext {
                            label: "wind down".into(),
                            instruction: work.pass.instruction.clone(),
                        },
                    });
                }
                match context.begin_place(
                    work.pass.request.take().unwrap(),
                    ports.decode.as_deref_mut().unwrap(),
                    ports.io,
                ) {
                    Ok(()) => work.stage = WindStage::Place,
                    Err(error) => {
                        work.pass.request = Some(error.input);
                        if let Some(failure) = error.failure {
                            return Err(failure);
                        }
                        if !matches!(error.refusal, PlaceError::Busy(_)) {
                            return Err(fault(
                                context,
                                ports.io,
                                format!("wind-down place: {:?}", error.refusal),
                            ));
                        }
                        context.pump_once(&mut ports)?;
                    }
                }
            }
            WindStage::Place => {
                let progress = context.advance_place(&mut ports)?;
                if let Some(done) = context.take_placement(ports.io)? {
                    work.stage = match &done {
                        PlacementCompletion::Granted(_) => WindStage::Placed,
                        PlacementCompletion::Refused(_) => {
                            work.outcome = refused();
                            WindStage::PassComplete
                        }
                        PlacementCompletion::Cancelled(_) => WindStage::PassComplete,
                    };
                    work.pass.placement = Some(done);
                    if matches!(work.pass.placement, Some(PlacementCompletion::Cancelled(_))) {
                        return Err(fault(
                            context,
                            ports.io,
                            "unexpected wind-down placement cancellation".into(),
                        ));
                    }
                } else if progress == crate::flow::context::PlaceProgress::Pending {
                    context.pump_once(&mut ports)?;
                }
            }
            WindStage::Generate => {
                generation::decision(context, ports.io)?;
                let Some(PlacementCompletion::Granted(grant)) = work.pass.placement.take() else {
                    unreachable!()
                };
                match context.begin_generation(
                    grant,
                    ports.decode.as_deref_mut().unwrap(),
                    ports.io,
                ) {
                    Ok(()) => work.stage = WindStage::Generation,
                    Err(error) => {
                        work.pass.placement = Some(PlacementCompletion::Granted(error.input));
                        if let Some(failure) = error.failure {
                            return Err(failure);
                        }
                        if !matches!(error.refusal, PlaceError::Busy(_)) {
                            return Err(fault(
                                context,
                                ports.io,
                                format!("wind-down generation: {:?}", error.refusal),
                            ));
                        }
                        context.pump_once(&mut ports)?;
                    }
                }
            }
            WindStage::Generation => {
                let progress = context.advance_generation(&mut ports)?;
                if let Some(done) = context.take_placed_generation(ports.io)? {
                    work.stage = match &done {
                        GenerationCompletion::Generated(_) => WindStage::Generated,
                        GenerationCompletion::Refused(_) => {
                            work.outcome = refused();
                            WindStage::PassComplete
                        }
                        GenerationCompletion::Cancelled(_) => WindStage::PassComplete,
                    };
                    work.pass.generation = Some(done);
                    if matches!(
                        work.pass.generation,
                        Some(GenerationCompletion::Cancelled(_))
                    ) {
                        return Err(fault(
                            context,
                            ports.io,
                            "unexpected wind-down generation cancellation".into(),
                        ));
                    }
                } else if progress == crate::flow::context::PlaceProgress::Pending {
                    context.pump_once(&mut ports)?;
                }
            }
            WindStage::Emission | WindStage::Account => {
                let emission = work.pass.emission.as_mut().unwrap();
                emission.advance(
                    &mut work.pass.generation,
                    &mut owner.certificate,
                    context,
                    &mut ports,
                )?;
                if work.stage == WindStage::Account
                    && (emission.settled() || emission.ready_to_close_refusal())
                {
                    work.stage = WindStage::PassComplete;
                }
            }
            _ => {}
        }
        Ok(())
    }
}
fn rejected_filing(work: &mut WindDown) -> bool {
    let proof = work.pass.filing.as_ref().unwrap();
    let Some(rejected) = proof
        .receipts()
        .dispositions()
        .find_map(|source| match source {
            SourceDisposition::CorrectlyRejected(rejected) => Some(rejected),
            _ => None,
        })
    else {
        return false;
    };
    if work.pass.refusal.is_none() && work.pass.refusal_record.is_none() {
        let fault = FaultReport {
            case: FaultCase::StateTemplateRejected,
            account: serde_json::value::to_raw_value(&serde_json::json!({
                "template_id": "wind_down", "source_id": rejected.source(), "result": rejected.reply()
            })).unwrap(),
        };
        work.pass.refusal = Some(AuthoredEvent {
            kind: serving::Kind::Fault,
            subsystem: Subsystem::Harness,
            turn: Some(work.turn.clone()),
            source: AuthoredSource::Runtime,
            causal_parent: Some(proof.sequence()),
            payload: Some(serving::Payload::Fault(
                serde_json::value::to_raw_value(&fault).unwrap(),
            )),
        });
    }
    true
}
fn refused() -> TurnClose {
    TurnClose::Stopped {
        reason: StopReason::Refused,
        cause: None,
    }
}
fn instruction_refusal(work: &mut WindDown, case: InstructionRefusal, detail: &str) {
    if work.pass.refusal.is_some() {
        return;
    }
    let record = RefusalRecord::Instruction {
        template_id: "wind_down".into(),
        case,
        detail: detail.into(),
    };
    work.pass.refusal = Some(AuthoredEvent {
        kind: serving::Kind::Refusal,
        subsystem: Subsystem::Harness,
        turn: Some(work.turn.clone()),
        source: AuthoredSource::Runtime,
        causal_parent: None,
        payload: Some(serving::Payload::Refusal(
            serde_json::value::to_raw_value(&record).unwrap(),
        )),
    });
}
fn record_refusal<I: PumpIo>(
    work: &mut WindDown,
    context: &mut RecordingContext,
    ports: &mut PumpPorts<'_, I>,
) -> Result<(), RunFailure> {
    // Instruction, Filing or force already armed Decision. Preserve the
    // caller's original bound through this refusal's authoring attempts.
    generation::decision(context, ports.io)?;
    work.pass.returned_refusal.check()?;
    let result = if work.pass.refusal_started {
        context.resume_author_and_wait(ports)
    } else {
        work.pass.refusal_started = true;
        context.author_and_wait(ports, work.pass.refusal.take().unwrap())
    };
    let recorded = match result {
        Ok(recorded) => recorded,
        Err(mut failure) => {
            if failure.reclaim_held(context) {
                work.pass.refusal_started = false;
                work.pass.refusal = failure.original.take();
                // The refusal remains owed through force. Its caller keeps the
                // unadmitted event while the earlier owner keeps its answer.
                // Idle yields to the earlier owner of a completed answer.
                if context.reduce_flow(ports.io)? == crate::flow::coordinator::Advance::Pending {
                    context.pump_once(ports)?;
                }
                return Ok(());
            }
            return Err(work
                .pass
                .returned_refusal
                .retain_started(failure, &mut work.pass.refusal_started));
        }
    };
    work.pass.refusal_started = false;
    work.pass.refusal_record = Some(recorded);
    work.outcome = refused();
    work.stage = WindStage::PassComplete;
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
