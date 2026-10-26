//! One worker-elected correction. Retain every attempt's instruction, measured
//! grant and complete decoder account after its checked output moves to repair.
use super::*;
use crate::flow::material::SelectedUnit;
use crate::flow::pump_transport::DecodePort;

#[cfg(test)]
#[path = "context_worker_correction_tests.rs"]
pub(in crate::flow::context) mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CorrectionStage {
    Prepare,
    Place,
    Placed,
    Start,
    Generate,
    Observe,
    Done,
}

pub(super) struct CorrectionWork {
    stage: CorrectionStage,
    restatement: RestatementRequest,
    label: String,
    method: CutMethod,
    unit: Option<SelectedUnit>,
    request: Option<PlaceRequest>,
    placement: Option<PlacementCompletion>,
    generation: Option<GenerationCompletion>,
    checked: Option<OriginalRequests>,
    cancelled: bool,
    detail: Option<String>,
}
impl CorrectionWork {
    pub(super) fn finish_refusal(
        &self,
        context: &mut RecordingContext,
        io: &mut impl PumpIo,
    ) -> Result<RepairAccount, RunFailure> {
        if let Some(PlacementCompletion::Refused(refusal)) = &self.placement {
            context.finish_refused_restatement(&self.restatement, refusal, io)
        } else if let Some(GenerationCompletion::Refused(account)) = &self.generation {
            context.finish_refused_restatement_generation(&self.restatement, account, io)
        } else {
            Err(RunFailure::PlaceOwned)
        }
    }
    pub(super) fn progress(&self) -> FoundationInputProgress {
        if self.stage == CorrectionStage::Placed {
            FoundationInputProgress::CorrectionPlaced
        } else {
            FoundationInputProgress::Pending
        }
    }
    pub(super) fn refused(&self) -> bool {
        self.detail.is_some()
    }
    pub(super) fn detail(&self) -> Option<&str> {
        self.detail.as_deref()
    }

    pub(super) fn cancel(
        &mut self,
        context: &mut RecordingContext,
        decode: Option<&mut DecodePort>,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        self.cancelled = true;
        context.revoke_place();
        if context.measure.is_some() {
            context.cancel_measurement(io)?;
        }
        if matches!(self.placement, Some(PlacementCompletion::Granted(_))) {
            let Some(PlacementCompletion::Granted(grant)) = self.placement.take() else {
                unreachable!()
            };
            match context.cancel_placement_grant(grant) {
                Ok(account) => self.placement = Some(PlacementCompletion::Cancelled(account)),
                Err(error) => {
                    self.placement = Some(PlacementCompletion::Granted(error.input));
                    return Err(error.failure.unwrap_or(RunFailure::PlaceOwned));
                }
            }
        }
        if context.generation.is_some() {
            context.cancel_generation(decode.ok_or(RunFailure::PlaceOwned)?, io)?;
        }
        if !matches!(
            self.stage,
            CorrectionStage::Place | CorrectionStage::Generate | CorrectionStage::Observe
        ) {
            context.complete_wait(PumpDeadline::Decision);
            self.stage = CorrectionStage::Done;
        }
        Ok(())
    }

    pub(super) fn advance<I: PumpIo>(
        &mut self,
        context: &mut RecordingContext,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<bool, RunFailure> {
        match self.stage {
            CorrectionStage::Prepare => {
                decision(context, ports.io)?;
                if self.unit.is_none() && self.request.is_none() {
                    match context.restatement_material(&mut self.restatement, ports.io) {
                        Ok(Some(unit)) => self.unit = Some(unit),
                        Ok(None) => {
                            context.pump_once(ports)?;
                            return Ok(false);
                        }
                        Err(PlaceError::InvalidContext) => {
                            self.detail =
                                Some("restatement instruction has no selected filing".into());
                            context.complete_wait(PumpDeadline::Decision);
                            self.stage = CorrectionStage::Done;
                            return Ok(true);
                        }
                        Err(error) => {
                            return Err(fault(
                                context,
                                ports.io,
                                format!("restatement material: {error:?}"),
                            ));
                        }
                    }
                }
                if self.request.is_none() {
                    let prefix = match context.prepare_restatement_prefix(
                        &self.restatement,
                        self.method,
                        ports.io,
                    ) {
                        Ok(prefix) => prefix,
                        Err(PlaceError::Busy(_)) => {
                            context.pump_once(ports)?;
                            return Ok(false);
                        }
                        Err(error) => {
                            return Err(fault(
                                context,
                                ports.io,
                                format!("restatement prefix: {error:?}"),
                            ));
                        }
                    };
                    self.request = Some(PlaceRequest {
                        prefix,
                        selection: vec![self.unit.take().unwrap()],
                        tool_schemas: context.loaded_place_schemas().unwrap().to_vec(),
                        context: PlacementContext {
                            label: self.label.clone(),
                            instruction: Some(self.restatement.recorded().block_id.clone()),
                        },
                    });
                }
                match context.begin_restatement_place(
                    &self.restatement,
                    self.request.take().unwrap(),
                    ports.decode.as_deref_mut().unwrap(),
                    ports.io,
                ) {
                    Ok(()) => self.stage = CorrectionStage::Place,
                    Err(error) => {
                        self.request = Some(error.input);
                        if let Some(failure) = error.failure {
                            return Err(failure);
                        }
                        if !matches!(error.refusal, PlaceError::Busy(_)) {
                            return Err(fault(
                                context,
                                ports.io,
                                format!("restatement placement: {:?}", error.refusal),
                            ));
                        }
                        context.pump_once(ports)?;
                    }
                }
            }
            CorrectionStage::Place => {
                context.advance_place(ports)?;
                if let Some(done) = context.take_placement(ports.io)? {
                    self.stage = match &done {
                        PlacementCompletion::Granted(_) => CorrectionStage::Placed,
                        PlacementCompletion::Refused(refused) => {
                            self.detail = Some(refused.detail.clone());
                            CorrectionStage::Done
                        }
                        PlacementCompletion::Cancelled(_) => CorrectionStage::Done,
                    };
                    self.placement = Some(done);
                }
            }
            CorrectionStage::Start => {
                decision(context, ports.io)?;
                let Some(PlacementCompletion::Granted(grant)) = self.placement.take() else {
                    unreachable!()
                };
                match context.begin_restatement_generation(
                    &self.restatement,
                    grant,
                    ports.decode.as_deref_mut().unwrap(),
                    ports.io,
                ) {
                    Ok(()) => self.stage = CorrectionStage::Generate,
                    Err(error) => {
                        self.placement = Some(PlacementCompletion::Granted(error.input));
                        if let Some(failure) = error.failure {
                            return Err(failure);
                        }
                        if !matches!(error.refusal, PlaceError::Busy(_)) {
                            return Err(fault(
                                context,
                                ports.io,
                                format!("restatement decode: {:?}", error.refusal),
                            ));
                        }
                        context.pump_once(ports)?;
                    }
                }
            }
            CorrectionStage::Generate => {
                let progress = context.advance_generation(ports)?;
                if let Some(done) = context.take_placed_generation(ports.io)? {
                    self.generation = Some(done);
                    self.stage = CorrectionStage::Observe;
                } else if progress == crate::flow::context::PlaceProgress::Pending {
                    context.pump_once(ports)?;
                }
            }
            CorrectionStage::Observe => {
                if self.checked.is_none() {
                    let slot = self.generation.as_mut().unwrap().correction_output();
                    if let Some(slot) = slot
                        && let Some(output) = slot.take()
                    {
                        match OriginalRequests::from_checked(
                            output,
                            context.repair_request_scheme().unwrap(),
                        ) {
                            Ok(checked) => self.checked = Some(checked),
                            Err(error) => {
                                *slot = Some(error.generation);
                                return Err(fault(
                                    context,
                                    ports.io,
                                    format!("correction associations: {:?}", error.error),
                                ));
                            }
                        }
                    }
                }
                if let Some(checked) = self.checked.take() {
                    if let Err(error) =
                        context.observe_correction(self.restatement.source(), checked, ports.io)
                    {
                        self.checked = Some(error.correction);
                        return Err(error.failure.unwrap_or_else(|| {
                            fault(
                                context,
                                ports.io,
                                format!("correction return: {:?}", error.error),
                            )
                        }));
                    }
                } else if !self.cancelled {
                    self.detail = Some("correction decoder refused".into());
                }
                self.stage = CorrectionStage::Done;
            }
            CorrectionStage::Placed | CorrectionStage::Done => {}
        }
        Ok(self.stage == CorrectionStage::Done)
    }
}

impl FoundationInput<'_> {
    /// Elect the placement or decoder refusal close. This pass's purpose now
    /// bars dispatch; all original conforming requests receive real terminal
    /// filings before the refused turn and original world exchange close.
    pub fn close_correction_refusal(&mut self) -> Result<(), FoundationInputProgress> {
        if self.progress() != FoundationInputProgress::OperationRefused {
            return Err(self.progress());
        }
        let work = self
            .engine
            .as_mut()
            .unwrap()
            .emission
            .as_mut()
            .ok_or(FoundationInputProgress::OperationRefused)?;
        if !work.corrections.last().is_some_and(|correction| {
            matches!(correction.placement, Some(PlacementCompletion::Refused(_)))
                || matches!(
                    correction.generation,
                    Some(GenerationCompletion::Refused(_))
                )
        }) || work.closing_refusal.is_some()
        {
            return Err(FoundationInputProgress::OperationRefused);
        }
        work.closing_refusal = Some(
            if matches!(
                work.corrections.last().unwrap().placement,
                Some(PlacementCompletion::Refused(_))
            ) {
                RefusalElection::Placement
            } else {
                RefusalElection::Operation
            },
        );
        work.stage = EmissionStage::CorrectionRefusal;
        Ok(())
    }
    /// Place only this attempt's certified, actually filed instruction. The
    /// existing turn argument stays intact; repair never selects a cut.
    pub fn place_restatement(&mut self, label: String, method: CutMethod) -> Result<(), String> {
        if self.progress() != FoundationInputProgress::Restatement || label.is_empty() {
            return Err(label);
        }
        let work = self.engine.as_mut().unwrap().emission.as_mut().unwrap();
        work.corrections.push(CorrectionWork {
            stage: CorrectionStage::Prepare,
            restatement: work.restatement.take().unwrap(),
            label,
            method,
            unit: None,
            request: None,
            placement: None,
            generation: None,
            checked: None,
            cancelled: false,
            detail: None,
        });
        work.stage = EmissionStage::Correction;
        Ok(())
    }
    /// Use the actual measured grant once. Completion returns to repair and
    /// may yield another restatement, exhaustion, or checked effective output.
    pub fn generate_correction(&mut self) -> Result<(), FoundationInputProgress> {
        if self.progress() != FoundationInputProgress::CorrectionPlaced {
            return Err(self.progress());
        }
        self.engine
            .as_mut()
            .unwrap()
            .emission
            .as_mut()
            .unwrap()
            .corrections
            .last_mut()
            .unwrap()
            .stage = CorrectionStage::Start;
        Ok(())
    }
}
