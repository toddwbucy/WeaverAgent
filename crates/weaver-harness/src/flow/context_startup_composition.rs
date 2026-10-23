//! Connect structural Open, untapped load, activation and actual classifier start.
//! The result is a boot prerequisite, never lifecycle Ready or a serving election.
use super::*;
use crate::flow::channel::StateCompletion;
use crate::flow::pump::LifecyclePorts;
use crate::flow::ste100_socket::AdmissionPoll;
use crate::flow::templates::CompiledInventory;
use std::path::PathBuf;

pub(in crate::flow) struct StartupRequest {
    pub start: StartIdentity,
    pub limits: ReadLimits,
    pub load: AuthoredEvent,
    pub bootstrap: Ste100Bootstrap,
    pub classifier_binary: PathBuf,
    pub inventory: CompiledInventory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum StartupStage {
    Open,
    Load,
    Activate,
    OpenMeasurement,
    InitialGrants,
    Classifier,
    Complete,
    Stopped,
}

#[derive(Debug)]
pub(in crate::flow) enum StartupStop {
    Run(RunFailure),
    Interrupted,
    InvalidEntry,
    OpenRefused(weaver_types::state_flow::StateRefusalCode, String),
    Activation(ActivationFailure),
    InitialGrants(InspectionBeginError),
    ClassifierStart(String),
    ClassifierRefused(weaver_types::state_flow::Ste100AdmissionRefused),
}

/// Returned alongside boot, so consumed observations do not disappear at the join.
pub(in crate::flow) struct StartupEvidence {
    pub opened: StateCompletion,
    pub load: Sequence,
    pub initial_grants: InspectionCompletion,
    pub classifier: weaver_types::state_flow::Ste100Ready,
}

pub(in crate::flow) struct StartupComposition<'a> {
    context: &'a mut RecordingContext,
    enter_bound: Option<AbsoluteBound>,
    resources: &'a mut FailedEnterResources,
    start: StartIdentity,
    limits: ReadLimits,
    load: Option<AuthoredEvent>,
    bootstrap: Ste100Bootstrap,
    binary: PathBuf,
    inventory: CompiledInventory,
    stage: StartupStage,
    started: bool,
    load_sequence: Option<Sequence>,
    opened: Option<StateCompletion>,
    initial_grants: Option<InspectionCompletion>,
    classifier: Option<weaver_types::state_flow::Ste100Ready>,
    stop: Option<StartupStop>,
}

pub(in crate::flow) struct PreparedStartup<'a> {
    context: &'a mut RecordingContext,
    resources: &'a mut FailedEnterResources,
    inventory: CompiledInventory,
    evidence: StartupEvidence,
}

/// All unconsumed inputs and consumed observations accompany stopped custody.
/// This account grants neither a retry nor a healthy lifecycle answer.
pub(in crate::flow) struct StoppedStartupAccount {
    pub cause: StartupStop,
    pub enter_bound: Option<AbsoluteBound>,
    pub start: StartIdentity,
    pub limits: ReadLimits,
    pub load: Option<AuthoredEvent>,
    pub bootstrap: Ste100Bootstrap,
    pub binary: PathBuf,
    pub inventory: CompiledInventory,
    pub load_sequence: Option<Sequence>,
    pub opened: Option<StateCompletion>,
    pub initial_grants: Option<InspectionCompletion>,
    pub classifier: Option<weaver_types::state_flow::Ste100Ready>,
}

impl<'a> PreparedStartup<'a> {
    pub(in crate::flow) fn into_parts(
        self,
    ) -> (
        &'a mut RecordingContext,
        &'a mut FailedEnterResources,
        CompiledInventory,
        StartupEvidence,
    ) {
        (self.context, self.resources, self.inventory, self.evidence)
    }
}

impl<'a> StartupComposition<'a> {
    pub(in crate::flow) fn new(
        context: &'a mut RecordingContext,
        resources: &'a mut FailedEnterResources,
        request: StartupRequest,
    ) -> Self {
        let invalid = context.phase != RecordingPhase::Preflight
            || context.open_start.is_some()
            || !resources.matches(context)
            || resources.classifier_child().is_some()
            || resources.spu_child().is_some()
            || request.load.kind != Kind::Load;
        let enter_bound = context.deadlines.get(PumpDeadline::Enter);
        Self {
            context,
            enter_bound,
            resources,
            start: request.start,
            limits: request.limits,
            load: Some(request.load),
            bootstrap: request.bootstrap,
            binary: request.classifier_binary,
            inventory: request.inventory,
            stage: if invalid {
                StartupStage::Stopped
            } else {
                StartupStage::Open
            },
            started: false,
            load_sequence: None,
            opened: None,
            initial_grants: None,
            classifier: None,
            stop: invalid.then_some(StartupStop::InvalidEntry),
        }
    }

    pub(in crate::flow) fn stage(&self) -> StartupStage {
        self.stage
    }
    pub(in crate::flow) fn stopped(&self) -> Option<&StartupStop> {
        self.stop.as_ref()
    }

    fn stop(&mut self, cause: StartupStop) -> StartupStage {
        self.stop = Some(cause);
        self.stage = StartupStage::Stopped;
        self.stage
    }

    fn check(&mut self, lifecycle: &LifecyclePorts<'_>, io: &mut impl PumpIo) -> bool {
        if self.stage == StartupStage::Stopped {
            return false;
        }
        if let Some(failure) = self.context.failure().cloned() {
            self.stop(StartupStop::Run(failure));
            return false;
        }
        self.context.observe_boot_lifecycle(lifecycle);
        self.context.observe_place_lifecycle(lifecycle);
        if self.context.boot_enter_cancelled || self.context.place_admission_stopped() {
            self.stop(StartupStop::Interrupted);
            return false;
        }
        if self.enter_bound.is_none()
            || self.context.deadlines.get(PumpDeadline::Enter) != self.enter_bound
        {
            let failure = self.context.fail(
                io,
                FlowHaltReason::ProtocolFailed,
                HaltPoint::Barrier,
                "startup lost original Enter wait",
            );
            self.stop(StartupStop::Run(failure));
            return false;
        }
        if let Err(error) = self.context.live(io) {
            self.stop(StartupStop::Run(error));
            return false;
        }
        true
    }

    /// # Safety
    /// This driver must run on the serving thread and that thread must outlive
    /// every child it creates, as required by spawn::fork_organ.
    pub(in crate::flow) unsafe fn advance(
        &mut self,
        lifecycle: LifecyclePorts<'_>,
        io: &mut impl PumpIo,
    ) -> StartupStage {
        if !self.check(&lifecycle, io) {
            return self.stage;
        }
        // SAFETY: forwarded from this driver's serving-thread lifetime contract.
        if let Err(error) = unsafe { self.step(lifecycle, io) } {
            self.stop(StartupStop::Run(error));
        }
        self.stage
    }

    unsafe fn step(
        &mut self,
        lifecycle: LifecyclePorts<'_>,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        let mut ports = PumpPorts {
            lifecycle,
            gate: None,
            decode: None,
            label: None,
            io,
        };
        match self.stage {
            StartupStage::Open => {
                if !self.started {
                    self.context
                        .begin_open(self.start.clone(), self.limits.clone(), ports.io)?;
                    self.started = true;
                }
                if let Some((code, detail)) = self.context.open_refusal() {
                    self.stop(StartupStop::OpenRefused(code, detail.to_owned()));
                } else if self.context.opened_identity().is_some() {
                    self.stage = StartupStage::Load;
                } else {
                    self.context.pump_once(&mut ports)?;
                }
            }
            StartupStage::Load => {
                let mut event = self.load.take().expect("one retained load input");
                if let Some(Payload::Elections(load)) = &mut event.payload {
                    load.origin_ns = self.context.origin_ns().into();
                    load.state_member_observation = crate::flow::mirror::member(
                        self.context
                            .opened_identity()
                            .expect("checked structural Open"),
                    );
                }
                let sequence = self.context.author_and_wait(&mut ports, event)?;
                self.load_sequence = Some(sequence);
                self.stage = StartupStage::Activate;
            }
            StartupStage::Activate => match self.context.activate(ports.io) {
                Ok(()) => {
                    self.stage = StartupStage::OpenMeasurement;
                }
                Err(error) => {
                    self.stop(StartupStop::Activation(error));
                }
            },
            StartupStage::OpenMeasurement => {
                let progress = self.context.reduce_flow(ports.io)?;
                match self.context.take_operation_completion(ports.io)? {
                    Some(OperationCompletion::Opened(opened)) => {
                        self.opened = Some(opened);
                        self.started = false;
                        self.stage = StartupStage::InitialGrants;
                    }
                    None if progress == Advance::Pending => {
                        self.context.pump_once(&mut ports)?;
                    }
                    None => {}
                    Some(_) => unreachable!("exclusive startup admits only Open"),
                }
            }
            StartupStage::InitialGrants => {
                if !self.started {
                    if let Err(error) =
                        self.context
                            .begin_inspection(InspectionKind::Grants, None, ports.io)
                    {
                        self.stop(StartupStop::InitialGrants(error));
                        return Ok(());
                    }
                    self.started = true;
                }
                let progress = self.context.advance_inspection(&mut ports)?;
                if let Some(completion) = self.context.take_inspection(ports.io)? {
                    // A measured read refusal is an unavailable initial reading,
                    // not an empty surface or a component failure. Preserve it
                    // intact for the terminal comparison, just like a success.
                    let usable = matches!(
                        completion,
                        InspectionCompletion::Grants { .. } | InspectionCompletion::Refused { .. }
                    );
                    self.initial_grants = Some(completion);
                    if usable {
                        self.started = false;
                        self.stage = StartupStage::Classifier;
                    } else {
                        self.stop(StartupStop::Interrupted);
                    }
                } else if progress == InspectionProgress::Pending {
                    self.context.pump_once(&mut ports)?;
                }
            }
            StartupStage::Classifier => {
                if !self.started {
                    // SAFETY: the caller carries this same serving-thread contract.
                    let result = unsafe {
                        self.resources.spawn_classifier(
                            self.context,
                            &self.binary,
                            self.bootstrap.clone(),
                            ports.io,
                        )
                    };
                    if let Err(error) = result {
                        self.stop(StartupStop::ClassifierStart(error));
                        return Ok(());
                    }
                    self.started = true;
                }
                self.context.pump_once(&mut ports)?;
                match self.context.take_classifier_admission(ports.io)? {
                    Some(AdmissionPoll::Ready(ready)) => {
                        self.classifier = Some(ready);
                        self.stage = StartupStage::Complete;
                    }
                    Some(AdmissionPoll::Refused(refused)) => {
                        self.stop(StartupStop::ClassifierRefused(refused));
                    }
                    None | Some(AdmissionPoll::Pending) => {}
                }
            }
            StartupStage::Complete | StartupStage::Stopped => {}
        }
        Ok(())
    }

    /// The original owner is returned on expiry, failure or interruption.
    #[allow(clippy::result_large_err)]
    pub(in crate::flow) fn finish(
        mut self,
        lifecycle: LifecyclePorts<'_>,
        io: &mut impl PumpIo,
    ) -> Result<PreparedStartup<'a>, Self> {
        if self.stage != StartupStage::Complete || !self.check(&lifecycle, io) {
            return Err(self);
        }
        Ok(PreparedStartup {
            context: self.context,
            resources: self.resources,
            inventory: self.inventory,
            evidence: StartupEvidence {
                opened: self.opened.expect("actual recorded Open retained"),
                load: self.load_sequence.expect("actual load retained"),
                initial_grants: self.initial_grants.expect("actual initial Grants retained"),
                classifier: self.classifier.expect("actual classifier Ready retained"),
            },
        })
    }

    #[allow(clippy::result_large_err)]
    pub(in crate::flow) fn into_stopped(
        self,
    ) -> Result<
        (
            &'a mut RecordingContext,
            &'a mut FailedEnterResources,
            StoppedStartupAccount,
        ),
        Self,
    > {
        if self.stage != StartupStage::Stopped {
            return Err(self);
        }
        Ok((
            self.context,
            self.resources,
            StoppedStartupAccount {
                cause: self.stop.expect("stopped cause retained"),
                enter_bound: self.enter_bound,
                start: self.start,
                limits: self.limits,
                load: self.load,
                bootstrap: self.bootstrap,
                binary: self.binary,
                inventory: self.inventory,
                load_sequence: self.load_sequence,
                opened: self.opened,
                initial_grants: self.initial_grants,
                classifier: self.classifier,
            },
        ))
    }
}

#[cfg(test)]
#[path = "context_startup_composition_tests.rs"]
pub(super) mod tests;

#[path = "context_enter_refusal_composition.rs"]
pub(in crate::flow) mod refusal;
