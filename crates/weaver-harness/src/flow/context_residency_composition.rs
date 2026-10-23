//! Actual decoder fork and residency Admit between classifier startup and boot.
use super::*;
use crate::flow::context::startup_composition::{PreparedStartup, StartupEvidence};
use crate::flow::pump::LifecyclePorts;
use crate::flow::pump_transport::{OrganSend, ReceivedOrgan};
use crate::flow::templates::CompiledInventory;
use std::path::PathBuf;
use weaver_types::{LifecycleAnswer, LifecycleDirective, Position, SpuInstruction};

pub(in crate::flow) struct ResidencyRequest {
    pub binary: PathBuf,
    pub arguments: Vec<String>,
    pub instruction: SpuInstruction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum ResidencyStage {
    Fork,
    Admit,
    Complete,
    Stopped,
}

#[derive(Debug)]
pub(in crate::flow) enum ResidencyStop {
    Run(RunFailure),
    Interrupted,
    InvalidEntry,
    Fork(String),
    Stage(String),
    /// The exact reason remains in the retained received envelope, unchanged.
    Refused,
}

pub(in crate::flow) struct ResidencyEvidence {
    pub startup: StartupEvidence,
    pub request: ResidencyRequest,
    pub send: OrganSend,
    pub reply: ReceivedOrgan,
}

pub(in crate::flow) struct StoppedResidency {
    pub cause: ResidencyStop,
    pub startup: StartupEvidence,
    pub request: ResidencyRequest,
    pub inventory: CompiledInventory,
    pub send: Option<OrganSend>,
    pub reply: Option<ReceivedOrgan>,
    pub returned_directive: Option<LifecycleDirective>,
}

pub(in crate::flow) struct AdmittedStartup<'a> {
    context: &'a mut RecordingContext,
    resources: &'a mut FailedEnterResources,
    inventory: CompiledInventory,
    evidence: ResidencyEvidence,
}

impl<'a> AdmittedStartup<'a> {
    pub(in crate::flow) fn into_parts(
        self,
    ) -> (
        &'a mut RecordingContext,
        &'a mut FailedEnterResources,
        CompiledInventory,
        ResidencyEvidence,
    ) {
        (self.context, self.resources, self.inventory, self.evidence)
    }
}

pub(in crate::flow) struct ResidencyComposition<'a> {
    context: &'a mut RecordingContext,
    resources: &'a mut FailedEnterResources,
    inventory: CompiledInventory,
    startup: StartupEvidence,
    request: ResidencyRequest,
    enter: Option<AbsoluteBound>,
    stage: ResidencyStage,
    send: Option<OrganSend>,
    reply: Option<ReceivedOrgan>,
    returned_directive: Option<LifecycleDirective>,
    stop: Option<ResidencyStop>,
}

impl<'a> ResidencyComposition<'a> {
    pub(in crate::flow) fn new(startup: PreparedStartup<'a>, request: ResidencyRequest) -> Self {
        let (context, resources, inventory, startup) = startup.into_parts();
        let invalid = context.phase != RecordingPhase::Entered
            || !resources.matches(context)
            || resources.spu_child().is_some();
        let enter = context.deadlines.get(PumpDeadline::Enter);
        Self {
            context,
            resources,
            inventory,
            startup,
            request,
            enter,
            stage: if invalid {
                ResidencyStage::Stopped
            } else {
                ResidencyStage::Fork
            },
            send: None,
            reply: None,
            returned_directive: None,
            stop: invalid.then_some(ResidencyStop::InvalidEntry),
        }
    }

    pub(in crate::flow) fn stage(&self) -> ResidencyStage {
        self.stage
    }
    pub(in crate::flow) fn stopped(&self) -> Option<&ResidencyStop> {
        self.stop.as_ref()
    }

    fn stop(&mut self, cause: ResidencyStop) -> ResidencyStage {
        self.stop = Some(cause);
        self.stage = ResidencyStage::Stopped;
        self.stage
    }

    fn protocol(&mut self, io: &mut impl PumpIo, detail: &str) -> RunFailure {
        self.context.fail(
            io,
            FlowHaltReason::ProtocolFailed,
            HaltPoint::Barrier,
            detail,
        )
    }

    fn check(&mut self, lifecycle: &LifecyclePorts<'_>, io: &mut impl PumpIo) -> bool {
        if self.stage == ResidencyStage::Stopped {
            return false;
        }
        if let Some(failure) = self.context.failure().cloned() {
            self.stop(ResidencyStop::Run(failure));
            return false;
        }
        self.context.observe_boot_lifecycle(lifecycle);
        self.context.observe_place_lifecycle(lifecycle);
        if self.context.boot_enter_cancelled || self.context.place_admission_stopped() {
            self.stop(ResidencyStop::Interrupted);
            return false;
        }
        if self.enter.is_none() || self.context.deadlines.get(PumpDeadline::Enter) != self.enter {
            let failure = self.protocol(io, "residency lost original Enter wait");
            self.stop(ResidencyStop::Run(failure));
            return false;
        }
        if let Err(failure) = self.context.live(io) {
            self.stop(ResidencyStop::Run(failure));
            return false;
        }
        true
    }

    /// # Safety
    /// Run on the serving thread, which must outlive and reap its children under
    /// spawn::fork_organ's existing lifetime contract.
    pub(in crate::flow) unsafe fn advance(
        &mut self,
        lifecycle: LifecyclePorts<'_>,
        io: &mut impl PumpIo,
    ) -> ResidencyStage {
        if !self.check(&lifecycle, io) {
            return self.stage;
        }
        // SAFETY: forwarded from this driver's caller contract.
        if let Err(failure) = unsafe { self.step(lifecycle, io) } {
            self.stop(ResidencyStop::Run(failure));
        }
        self.stage
    }

    unsafe fn step(
        &mut self,
        mut lifecycle: LifecyclePorts<'_>,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        match self.stage {
            ResidencyStage::Fork => {
                // SAFETY: forwarded from advance's serving-thread contract.
                match unsafe {
                    self.resources.spawn_spu(
                        self.context,
                        &self.request.binary,
                        &self.request.arguments,
                        io,
                    )
                } {
                    Ok(()) => {
                        self.stage = ResidencyStage::Admit;
                    }
                    Err(error) => {
                        self.stop(ResidencyStop::Fork(error));
                    }
                }
            }
            ResidencyStage::Admit => {
                if self.send.is_none() {
                    let directive = LifecycleDirective::Admit {
                        instruction: self.request.instruction.clone(),
                    };
                    match self.resources.stage_residency(directive) {
                        Ok(send) => self.send = Some(send),
                        Err((original, detail)) => {
                            self.returned_directive = Some(original);
                            self.stop(ResidencyStop::Stage(detail));
                            return Ok(());
                        }
                    }
                }
                // Probe the same owned port before allowing its first write. An
                // already-arrived answer cannot acquire validity by sending later.
                let port = self
                    .resources
                    .residency_mut()
                    .expect("actual fork retains residency");
                let receive_error = if port.held().is_none() {
                    port.try_receive().err()
                } else {
                    None
                };
                if let Some(reply) =
                    port.take_received()
                        .map_err(|failure| RunFailure::Transport {
                            source: PumpSource::Residency,
                            failure,
                        })?
                {
                    self.reply = Some(reply);
                    return self.consume_reply(io);
                }
                if let Some(failure) = receive_error {
                    return Err(RunFailure::Transport {
                        source: PumpSource::Residency,
                        failure,
                    });
                }
                // Use only the endpoints from this actual resource owner. The
                // outer caller's other lifecycle ports retain their identities.
                if lifecycle.residency.is_some() {
                    return Err(self.protocol(
                        io,
                        "residency composition received a foreign residency port",
                    ));
                }
                let (residency, decode) = self
                    .resources
                    .spu_ports_mut()
                    .expect("actual fork retains both ports");
                let ports = LifecyclePorts {
                    listener: lifecycle.listener.take(),
                    peer: lifecycle.peer.take(),
                    residency: Some(residency),
                };
                self.context.pump_once(&mut PumpPorts {
                    lifecycle: ports,
                    gate: None,
                    decode: Some(decode),
                    label: None,
                    io,
                })?;
                // Install the complete original reply before any fallible check.
                let port = self.resources.residency_mut().expect("same residency");
                if let Some(reply) =
                    port.take_received()
                        .map_err(|failure| RunFailure::Transport {
                            source: PumpSource::Residency,
                            failure,
                        })?
                {
                    self.reply = Some(reply);
                    self.consume_reply(io)?;
                }
            }
            ResidencyStage::Complete | ResidencyStage::Stopped => {}
        }
        Ok(())
    }

    fn consume_reply(&mut self, io: &mut impl PumpIo) -> Result<(), RunFailure> {
        let reply = self
            .reply
            .as_ref()
            .expect("reply retained before validation");
        let port = self
            .resources
            .residency_mut()
            .expect("original residency port");
        let send = self.send.as_ref().expect("original Admit ticket");
        let envelope = reply.envelope();
        if !reply.belongs_to(port)
            || !send.belongs_to(port)
            || !send.sent()
            || !send.matches_exchange(&envelope.exchange)
            || envelope.position != Position::Close
        {
            return Err(self.protocol(
                io,
                "Admit reply is not this completely sent residency exchange",
            ));
        }
        match &envelope.payload {
            weaver_types::Payload::Answer(LifecycleAnswer::Admitted) => {
                self.stage = ResidencyStage::Complete;
            }
            weaver_types::Payload::Refusal(_) => {
                self.stop(ResidencyStop::Refused);
            }
            _ => return Err(self.protocol(io, "Admit reply has the wrong terminal arm")),
        }
        Ok(())
    }

    #[allow(clippy::result_large_err)]
    pub(in crate::flow) fn finish(
        mut self,
        lifecycle: LifecyclePorts<'_>,
        io: &mut impl PumpIo,
    ) -> Result<AdmittedStartup<'a>, Self> {
        if self.stage != ResidencyStage::Complete || !self.check(&lifecycle, io) {
            return Err(self);
        }
        Ok(AdmittedStartup {
            context: self.context,
            resources: self.resources,
            inventory: self.inventory,
            evidence: ResidencyEvidence {
                startup: self.startup,
                request: self.request,
                send: self.send.expect("actual sent Admit"),
                reply: self.reply.expect("actual Admitted"),
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
            StoppedResidency,
        ),
        Self,
    > {
        if self.stage != ResidencyStage::Stopped {
            return Err(self);
        }
        Ok((
            self.context,
            self.resources,
            StoppedResidency {
                cause: self.stop.expect("stopped cause"),
                startup: self.startup,
                request: self.request,
                inventory: self.inventory,
                send: self.send,
                reply: self.reply,
                returned_directive: self.returned_directive,
            },
        ))
    }
}

#[cfg(test)]
#[path = "context_residency_composition_tests.rs"]
pub(super) mod tests;
