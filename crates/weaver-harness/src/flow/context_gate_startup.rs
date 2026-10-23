//! Completed boot -> actual contained gate fork -> checked Raise completion.
//! The caller still owns the admission judgment and the original Enter peer.
use super::*;
use crate::flow::context::failed_enter_driver::OwnedChild;
use crate::flow::pump::{LifecycleListener, PumpSource};
use crate::flow::pump_transport::{OrganPort, OrganSend, ReceivedOrgan, TrackedStage};
use crate::tools_cgroup::ToolsCgroup;
use std::os::fd::AsFd;
use std::path::PathBuf;
use weaver_types::{
    GateInstruction, LifecycleAnswer, LifecycleDirective, Opener, OrganEnvelope, Position,
};

pub(in crate::flow) struct GateStartupRequest {
    pub binary: PathBuf,
    pub instruction: GateInstruction,
    pub shell: Option<weaver_types::ShellGrant>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum GateStartupStage {
    Boundary,
    Fork,
    Raise,
    Complete,
    Stopped,
}
#[derive(Debug)]
pub(in crate::flow) enum GateStartupStop {
    Run(RunFailure),
    Interrupted,
    InvalidEntry,
    Boundary(crate::tools_cgroup::OpenRefused),
    Fork(String),
    Stage(String),
    /// The exact refusing envelope remains in reply.
    Refused,
}

/// This owner can only be constructed by consuming actual completed boot. It
/// keeps the gate child/port/containment together through every stopped outcome.
pub(in crate::flow) struct GateStartup<'a> {
    boot: ComposedBoot<'a>,
    request: GateStartupRequest,
    listener: LifecycleListener,
    enter: Option<AbsoluteBound>,
    stage: GateStartupStage,
    tools: Option<ToolsCgroup>,
    child: Option<OwnedChild>,
    port: Option<OrganPort>,
    raise: OrganEnvelope,
    send: Option<OrganSend>,
    reply: Option<ReceivedOrgan>,
    returned: Option<OrganEnvelope>,
    stop: Option<GateStartupStop>,
}

/// A gate's Ready is a retained arm confirmation, not the aggregate admin answer.
/// The outer lifecycle must retain its actual Enter judgment and other arms.
pub(in crate::flow) struct RaisedBoot<'a> {
    owner: GateStartup<'a>,
}

impl<'a> ComposedBoot<'a> {
    pub(in crate::flow) fn into_gate(
        self,
        request: GateStartupRequest,
        listener: LifecycleListener,
    ) -> GateStartup<'a> {
        let enter = self.context.deadlines.get(PumpDeadline::Enter);
        let invalid = self.context.phase != RecordingPhase::Entered
            || enter.is_none()
            || self.context.place_engine.is_none()
            || self.context.committed_plan_summary().is_err();
        let raise = OrganEnvelope {
            exchange: weaver_types::ExchangeId {
                opener: Opener::Harness,
                ordinal: 1,
            },
            position: Position::Open,
            payload: weaver_types::Payload::Directive(LifecycleDirective::Raise {
                instruction: request.instruction.clone(),
                socket: listener.gate_socket(),
                shell: request.shell.clone(),
            }),
        };
        GateStartup {
            boot: self,
            request,
            listener,
            enter,
            stage: if invalid {
                GateStartupStage::Stopped
            } else {
                GateStartupStage::Boundary
            },
            tools: None,
            child: None,
            port: None,
            raise,
            send: None,
            reply: None,
            returned: None,
            stop: invalid.then_some(GateStartupStop::InvalidEntry),
        }
    }
}
impl<'a> GateStartup<'a> {
    /// The outer lifecycle judges this real accepted connection, then supplies
    /// it as the primary peer while it reduces the pending directive.
    pub(in crate::flow) fn take_accepted_connection(
        &mut self,
    ) -> Option<(crate::flow::pump::LifecyclePeer, u64)> {
        self.listener.take_observed_connection()
    }
    pub(in crate::flow) fn stage(&self) -> GateStartupStage {
        self.stage
    }
    pub(in crate::flow) fn stop(&self) -> Option<&GateStartupStop> {
        self.stop.as_ref()
    }
    fn stopped(&mut self, cause: GateStartupStop) -> GateStartupStage {
        self.stop = Some(cause);
        self.stage = GateStartupStage::Stopped;
        self.stage
    }
    fn protocol(&mut self, io: &mut impl PumpIo, detail: impl Into<String>) -> RunFailure {
        self.boot.context.fail(
            io,
            FlowHaltReason::ProtocolFailed,
            HaltPoint::Barrier,
            detail,
        )
    }
    fn check(&mut self, lifecycle: &LifecyclePorts<'_>, io: &mut impl PumpIo) -> bool {
        if self.stage == GateStartupStage::Stopped {
            return false;
        }
        if let Some(failure) = self.boot.context.failure().cloned() {
            self.stopped(GateStartupStop::Run(failure));
            return false;
        }
        if lifecycle.listener.is_some() {
            let error = self.protocol(
                io,
                "gate startup received a substitute coordination listener",
            );
            self.stopped(GateStartupStop::Run(error));
            return false;
        }
        self.boot.context.observe_boot_lifecycle(lifecycle);
        self.boot.context.observe_place_lifecycle(lifecycle);
        if self.boot.context.boot_enter_cancelled || self.boot.context.place_admission_stopped() {
            self.stopped(GateStartupStop::Interrupted);
            return false;
        }
        if self.enter.is_none()
            || self.boot.context.deadlines.get(PumpDeadline::Enter) != self.enter
        {
            let error = self.protocol(io, "gate startup lost its original Enter bound");
            self.stopped(GateStartupStop::Run(error));
            return false;
        }
        if let Err(failure) = self.boot.context.live(io) {
            self.stopped(GateStartupStop::Run(failure));
            return false;
        }
        true
    }
    /// # Safety
    /// The serving thread outlives/reaps its actual children under fork_organ's
    /// existing lifetime rule. This function never waits on a child exit.
    pub(in crate::flow) unsafe fn advance(
        &mut self,
        lifecycle: LifecyclePorts<'_>,
        io: &mut impl PumpIo,
    ) -> GateStartupStage {
        if !self.check(&lifecycle, io) {
            return self.stage;
        }
        // SAFETY: the caller carries the serving-thread child lifetime contract.
        if let Err(error) = unsafe { self.step(lifecycle, io) } {
            self.stopped(GateStartupStop::Run(error));
        }
        self.stage
    }
    unsafe fn step(
        &mut self,
        mut lifecycle: LifecyclePorts<'_>,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        match self.stage {
            GateStartupStage::Boundary => match ToolsCgroup::open_from_own() {
                Ok(tools) => {
                    self.tools = Some(tools);
                    self.stage = GateStartupStage::Fork;
                }
                Err(error) => {
                    self.stopped(GateStartupStop::Boundary(error));
                }
            },
            GateStartupStage::Fork => {
                let created = (|| {
                    let (near, far) = crate::channel::OrganChannel::pair()
                        .map_err(|e| format!("gate pair: {e:?}"))?;
                    let port =
                        OrganPort::from_channel(near).map_err(|e| format!("gate port: {e:?}"))?;
                    // SAFETY: forwarded from advance. Only the organ endpoint and
                    // judged tools directory cross as descriptors 3 and 4.
                    let child = unsafe {
                        OwnedChild::spawn(
                            &self.request.binary,
                            &[
                                far.as_fd(),
                                self.tools.as_ref().expect("judged tools").as_fd(),
                            ],
                            &[],
                        )
                    }?;
                    Ok::<_, String>((port, child))
                })();
                match created {
                    Ok((port, child)) => {
                        self.port = Some(port);
                        self.child = Some(child);
                        self.stage = GateStartupStage::Raise;
                    }
                    Err(error) => {
                        self.stopped(GateStartupStop::Fork(error));
                    }
                }
            }
            GateStartupStage::Raise => {
                if self.send.is_none() {
                    match self
                        .port
                        .as_mut()
                        .unwrap()
                        .stage_tracked(self.raise.clone())
                    {
                        Ok(TrackedStage::Queued(send)) => self.send = Some(send),
                        Ok(TrackedStage::Busy(original)) => {
                            self.returned = Some(original);
                            self.stopped(GateStartupStop::Stage("fresh gate port is busy".into()));
                            return Ok(());
                        }
                        Err(error) => {
                            self.returned = Some(error.input);
                            self.stopped(GateStartupStop::Stage(format!("{:?}", error.failure)));
                            return Ok(());
                        }
                    }
                }
                // A reply already present before the whole request was sent
                // must never become valid because a later pump sends the request.
                let port = self.port.as_mut().unwrap();
                let receive_error = if port.held().is_none() {
                    port.try_receive().err()
                } else {
                    None
                };
                if let Some(reply) =
                    port.take_received()
                        .map_err(|failure| RunFailure::Transport {
                            source: PumpSource::Gate,
                            failure,
                        })?
                {
                    self.reply = Some(reply);
                    return self.consume_reply(io);
                }
                if let Some(failure) = receive_error {
                    return Err(RunFailure::Transport {
                        source: PumpSource::Gate,
                        failure,
                    });
                }
                let lifecycle = LifecyclePorts {
                    listener: Some(&mut self.listener),
                    peer: lifecycle.peer.take(),
                    residency: lifecycle.residency.take(),
                    dial: lifecycle.dial.take(),
                    leave_peer: lifecycle.leave_peer.take(),
                };
                let result = self.boot.context.pump_once(&mut PumpPorts {
                    lifecycle,
                    gate: self.port.as_mut(),
                    decode: Some(self.boot.decode),
                    label: None,
                    io,
                });
                if let Err(error) = result {
                    if self
                        .send
                        .as_ref()
                        .is_some_and(|send| send.sent() && send.sent_at().is_none())
                    {
                        self.boot.context.fail(
                            io,
                            FlowHaltReason::TraceFailed,
                            HaltPoint::Author,
                            format!("gate Raise send clock observation failed: {error:?}"),
                        );
                    }
                    return Err(error);
                }
                if let Some(reply) =
                    self.port
                        .as_mut()
                        .unwrap()
                        .take_received()
                        .map_err(|failure| RunFailure::Transport {
                            source: PumpSource::Gate,
                            failure,
                        })?
                {
                    self.reply = Some(reply);
                    self.consume_reply(io)?;
                }
            }
            GateStartupStage::Complete | GateStartupStage::Stopped => {}
        }
        // Retain actual child, send and reply before this fallible observer.
        if self.boot.context.boot_enter_cancelled || self.boot.context.place_admission_stopped() {
            self.stopped(GateStartupStop::Interrupted);
        }
        if self.stage != GateStartupStage::Stopped {
            self.boot.context.live(io)?;
        }
        Ok(())
    }
    fn consume_reply(&mut self, io: &mut impl PumpIo) -> Result<(), RunFailure> {
        let reply = self.reply.as_ref().unwrap();
        let port = self.port.as_ref().unwrap();
        let bound = self.enter.expect("original Enter");
        let valid = reply.belongs_to(port)
            && reply.envelope().exchange == self.raise.exchange
            && reply.envelope().position == Position::Close
            && self.send.as_ref().is_some_and(|send| {
                send.belongs_to(port)
                    && send.sent()
                    && send.matches_exchange(&reply.envelope().exchange)
                    && send
                        .sent_at()
                        .is_some_and(|at| at >= bound.began_ns() && !bound.expired(at))
            });
        if !valid {
            return Err(self.protocol(io, "gate response did not close its exact whole-sent Raise"));
        }
        match &reply.envelope().payload {
            weaver_types::Payload::Answer(LifecycleAnswer::Ready) => {
                self.stage = GateStartupStage::Complete
            }
            weaver_types::Payload::Refusal(_) => {
                self.stopped(GateStartupStop::Refused);
            }
            _ => {
                return Err(self.protocol(io, "gate Raise returned an invalid response vocabulary"));
            }
        }
        Ok(())
    }
    #[allow(clippy::result_large_err)]
    pub(in crate::flow) fn finish(
        mut self,
        lifecycle: LifecyclePorts<'_>,
        io: &mut impl PumpIo,
    ) -> Result<RaisedBoot<'a>, Self> {
        if self.stage != GateStartupStage::Complete
            || !self.check(&lifecycle, io)
            || self.listener.held().is_some()
        {
            return Err(self);
        }
        Ok(RaisedBoot { owner: self })
    }
}
impl RaisedBoot<'_> {
    pub(in crate::flow) fn reply(&self) -> &ReceivedOrgan {
        self.owner.reply.as_ref().expect("actual Ready retained")
    }
    pub(in crate::flow) fn enter(&self) -> AbsoluteBound {
        self.owner.enter.expect("original Enter")
    }
    pub(in crate::flow) fn shell_granted(&self) -> bool {
        self.owner.request.shell.is_some()
    }
}

#[cfg(test)]
#[path = "context_gate_startup_tests.rs"]
pub(in crate::flow::context) mod tests;
