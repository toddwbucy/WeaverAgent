//! The replacement boot sequence, driven by one caller under one Enter window.
//! No user gate, Ready answer, process creation or serving activation lives here.
use super::*;
use crate::flow::pump::LifecyclePorts;
use crate::flow::pump_transport::DecodePort;
use crate::flow::templates::{CompiledInventory, TemplateCertificate};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum BootStage {
    Templates,
    Prompt,
    Open,
    Repair,
    Summary,
    Install,
    Complete,
    Stopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum BootCompositionProgress {
    Progress,
    Pending,
    Complete,
    Stopped,
}

/// These are actual returned owners, never reconstructions from their summaries.
#[derive(Debug)]
pub(in crate::flow) enum BootCompositionStop {
    Run(RunFailure),
    Interrupted,
    Templates(TemplateBeginError),
    TemplateRejected,
    PromptStart(BootReadBeginError),
    Prompt(BootReadCompletion),
    OpenStart(BootOpenBeginError),
    Open(BootOpenProgress),
    RepairStart(BootPairsBeginError),
    Repair(BootPairsCompletion),
    SummaryStart(PlanSummaryBeginError<RepairedBootPairs>),
    Summary(PlanSummaryCompletion),
    Install(PlaceInstallError),
}

/// Exclusive borrows prevent another caller from admitting work between stages.
/// The prepared context already owns actual state Open, untapped Load, semantic
/// activation and classifier admission. The constructor grants no missing proof.
/// Every primitive rechecks its own admission and retains its original evidence.
pub(in crate::flow) struct BootComposition<'a> {
    context: &'a mut RecordingContext,
    decode: &'a mut DecodePort,
    stage: BootStage,
    inventory: Option<CompiledInventory>,
    certificate: Option<TemplateCertificate>,
    prior: Option<PriorBootOpen>,
    repaired: Option<RepairedBootPairs>,
    summary: Option<BootPlanSummary>,
    started: bool,
    stop: Option<BootCompositionStop>,
}

/// Completion of this sequence is not lifecycle Ready. The outer lifecycle must
/// still stand the gate and complete its other elected Enter obligations.
pub(in crate::flow) struct ComposedBoot<'a> {
    context: &'a mut RecordingContext,
    decode: &'a mut DecodePort,
    certificate: TemplateCertificate,
}

/// A stopped sequence returns every outstanding prerequisite beside the same
/// context and decode borrow. The lifecycle caller still owes bounded cleanup.
pub(in crate::flow) struct StoppedBoot<'a> {
    pub context: &'a mut RecordingContext,
    pub decode: &'a mut DecodePort,
    pub cause: BootCompositionStop,
    pub inventory: Option<CompiledInventory>,
    pub certificate: Option<TemplateCertificate>,
    pub prior: Option<PriorBootOpen>,
    pub repaired: Option<RepairedBootPairs>,
    pub summary: Option<BootPlanSummary>,
}

impl<'a> ComposedBoot<'a> {
    pub(in crate::flow) fn into_parts(
        self,
    ) -> (
        &'a mut RecordingContext,
        &'a mut DecodePort,
        TemplateCertificate,
    ) {
        (self.context, self.decode, self.certificate)
    }
}

impl<'a> BootComposition<'a> {
    pub(in crate::flow) fn new(
        context: &'a mut RecordingContext,
        decode: &'a mut DecodePort,
        inventory: CompiledInventory,
    ) -> Self {
        Self {
            context,
            decode,
            stage: BootStage::Templates,
            inventory: Some(inventory),
            certificate: None,
            prior: None,
            repaired: None,
            summary: None,
            started: false,
            stop: None,
        }
    }

    pub(in crate::flow) fn stage(&self) -> BootStage {
        self.stage
    }

    pub(in crate::flow) fn stopped(&self) -> Option<&BootCompositionStop> {
        self.stop.as_ref()
    }

    fn stop(&mut self, cause: BootCompositionStop) -> BootCompositionProgress {
        self.stop = Some(cause);
        self.stage = BootStage::Stopped;
        BootCompositionProgress::Stopped
    }

    fn next(&mut self, stage: BootStage) -> BootCompositionProgress {
        self.stage = stage;
        self.started = false;
        BootCompositionProgress::Progress
    }

    pub(in crate::flow) fn advance<I: PumpIo>(
        &mut self,
        lifecycle: LifecyclePorts<'_>,
        io: &mut I,
    ) -> BootCompositionProgress {
        if self.stage == BootStage::Stopped {
            return BootCompositionProgress::Stopped;
        }
        if let Some(failure) = self.context.failure().cloned() {
            return self.stop(BootCompositionStop::Run(failure));
        }
        self.context.observe_boot_lifecycle(&lifecycle);
        self.context.observe_place_lifecycle(&lifecycle);
        if self.context.boot_enter_cancelled || self.context.place_admission_stopped() {
            return self.stop(BootCompositionStop::Interrupted);
        }
        if let Err(failure) = self.context.live(io) {
            return self.stop(BootCompositionStop::Run(failure));
        }
        match self.step(lifecycle, io) {
            Ok(progress) => progress,
            Err(failure) => self.stop(BootCompositionStop::Run(failure)),
        }
    }

    fn step<I: PumpIo>(
        &mut self,
        lifecycle: LifecyclePorts<'_>,
        io: &mut I,
    ) -> Result<BootCompositionProgress, RunFailure> {
        use BootCompositionProgress::{Complete, Pending};
        match self.stage {
            BootStage::Templates => {
                if !self.started {
                    let inventory = self.inventory.take().expect("initial inventory retained");
                    if let Err(error) = self.context.begin_template_checks(inventory, io) {
                        return Ok(self.stop(BootCompositionStop::Templates(error)));
                    }
                    self.started = true;
                }
                match self.context.advance_template_checks(io)? {
                    TemplateProgress::Rejected => {
                        return Ok(self.stop(BootCompositionStop::TemplateRejected));
                    }
                    TemplateProgress::Complete => {
                        if let Some(certificate) = self.context.take_template_certificate(io)? {
                            self.certificate = Some(certificate);
                            return Ok(self.next(BootStage::Prompt));
                        }
                    }
                    TemplateProgress::Progress => return Ok(BootCompositionProgress::Progress),
                    TemplateProgress::Pending => {}
                }
                self.context.pump_once(&mut PumpPorts {
                    lifecycle,
                    gate: None,
                    decode: Some(self.decode),
                    label: None,
                    io,
                })?;
                Ok(Pending)
            }
            BootStage::Prompt => {
                if !self.started {
                    if let Err(error) = self.context.begin_boot_read(io) {
                        return Ok(self.stop(BootCompositionStop::PromptStart(error)));
                    }
                    self.started = true;
                }
                let mut ports = PumpPorts {
                    lifecycle,
                    gate: None,
                    decode: Some(self.decode),
                    label: None,
                    io,
                };
                let progress = self.context.advance_read(&mut ports)?;
                match self.context.finish_boot_read(ports.io)? {
                    Some(BootReadCompletion::Seated) => Ok(self.next(BootStage::Open)),
                    Some(other) => Ok(self.stop(BootCompositionStop::Prompt(other))),
                    None => {
                        if progress == ReadProgress::Pending {
                            self.context.pump_once(&mut ports)?;
                            Ok(Pending)
                        } else {
                            Ok(BootCompositionProgress::Progress)
                        }
                    }
                }
            }
            BootStage::Open => {
                if !self.started {
                    if let Err(error) = self.context.begin_boot_open(self.decode, io) {
                        return Ok(self.stop(BootCompositionStop::OpenStart(error)));
                    }
                    self.started = true;
                }
                match self.context.advance_boot_open(&mut PumpPorts {
                    lifecycle,
                    gate: None,
                    decode: Some(self.decode),
                    label: None,
                    io,
                })? {
                    BootOpenProgress::Complete => {
                        if let Some(open) = self.context.take_boot_open(io)? {
                            self.prior = Some(PriorBootOpen::from_checked(open));
                            return Ok(self.next(BootStage::Repair));
                        }
                        Ok(Pending)
                    }
                    BootOpenProgress::Pending | BootOpenProgress::PlacementRecorded => Ok(Pending),
                    other => Ok(self.stop(BootCompositionStop::Open(other))),
                }
            }
            BootStage::Repair => {
                if !self.started {
                    let prior = self.prior.take().expect("checked Open retained");
                    if let Err(error) = self.context.begin_boot_pairs(prior, io) {
                        return Ok(self.stop(BootCompositionStop::RepairStart(error)));
                    }
                    self.started = true;
                }
                let mut ports = PumpPorts {
                    lifecycle,
                    gate: None,
                    decode: Some(self.decode),
                    label: None,
                    io,
                };
                let progress = self.context.advance_boot_pairs(&mut ports)?;
                match self.context.take_boot_pairs(ports.io)? {
                    Some(BootPairsCompletion::Repaired(repaired)) => {
                        self.repaired = Some(repaired);
                        Ok(self.next(BootStage::Summary))
                    }
                    Some(other) => Ok(self.stop(BootCompositionStop::Repair(other))),
                    None => {
                        if progress == BootPairsProgress::Pending {
                            self.context.pump_once(&mut ports)?;
                            Ok(Pending)
                        } else {
                            Ok(BootCompositionProgress::Progress)
                        }
                    }
                }
            }
            BootStage::Summary => {
                if !self.started {
                    let repaired = self.repaired.take().expect("actual repair retained");
                    if let Err(error) = self.context.begin_boot_plan_summary(repaired, io) {
                        return Ok(self.stop(BootCompositionStop::SummaryStart(error)));
                    }
                    self.started = true;
                }
                let mut ports = PumpPorts {
                    lifecycle,
                    gate: None,
                    decode: Some(self.decode),
                    label: None,
                    io,
                };
                let progress = self.context.advance_plan_summary(&mut ports)?;
                match self.context.take_plan_summary(ports.io)? {
                    Some(PlanSummaryCompletion::Boot(summary)) => {
                        self.summary = Some(summary);
                        Ok(self.next(BootStage::Install))
                    }
                    Some(other) => Ok(self.stop(BootCompositionStop::Summary(other))),
                    None => {
                        if progress == PlanSummaryProgress::Pending {
                            self.context.pump_once(&mut ports)?;
                            Ok(Pending)
                        } else {
                            Ok(BootCompositionProgress::Progress)
                        }
                    }
                }
            }
            BootStage::Install => {
                let summary = self.summary.take().expect("actual boot summary retained");
                if let Err(error) = self.context.install_place(summary, io) {
                    return Ok(self.stop(BootCompositionStop::Install(error)));
                }
                self.stage = BootStage::Complete;
                Ok(Complete)
            }
            BootStage::Complete => Ok(Complete),
            BootStage::Stopped => Ok(BootCompositionProgress::Stopped),
        }
    }

    /// Return custody without granting success, resetting a phase or sampling
    /// another clock. This also retains a successful proof stopped between owners.
    #[allow(clippy::result_large_err)]
    pub(in crate::flow) fn into_stopped(self) -> Result<StoppedBoot<'a>, Self> {
        if self.stage != BootStage::Stopped {
            return Err(self);
        }
        Ok(StoppedBoot {
            context: self.context,
            decode: self.decode,
            cause: self.stop.expect("stopped sequence retains its cause"),
            inventory: self.inventory,
            certificate: self.certificate,
            prior: self.prior,
            repaired: self.repaired,
            summary: self.summary,
        })
    }

    /// Final extraction checks the current lifecycle and original Enter again.
    /// A revoked/expired sequence returns its entire owner to the caller.
    #[allow(clippy::result_large_err)]
    pub(in crate::flow) fn finish(
        mut self,
        lifecycle: LifecyclePorts<'_>,
        io: &mut impl PumpIo,
    ) -> Result<ComposedBoot<'a>, Self> {
        if self.stage != BootStage::Complete
            || self.advance(lifecycle, io) != BootCompositionProgress::Complete
        {
            return Err(self);
        }
        Ok(ComposedBoot {
            context: self.context,
            decode: self.decode,
            certificate: self.certificate.expect("complete certification retained"),
        })
    }
}

#[cfg(test)]
#[path = "context_boot_composition_tests.rs"]
pub(super) mod tests;
