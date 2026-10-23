//! Management owns the accepted root connection and suspends, never replaces,
//! its input owner. Observe reads retained facts; Stop owns its recorded close.
use super::*;
use crate::flow::pump::{LifecycleMessage, LifecyclePeer};
use weaver_types::state_flow::enter::{ObservationAnswer, ObservationContent, ObservationEnvelope};
use weaver_types::{AgentState, LifecycleAnswer, LifecycleDirective};
#[path = "context_worker_stop.rs"]
pub(in crate::flow::context::worker_startup) mod stop;
use stop::StopWork;

enum ManagementAnswer {
    Observation(ObservationEnvelope),
    Lifecycle(OrganEnvelope),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoundationManagementProgress {
    Pending,
    /// The complete original directive and descriptors await their next owner.
    DirectivePending,
    Answered,
    /// The peer failed or supplied no valid directive. No answer is claimed.
    Dropped,
    Stopped,
}

/// One move-owned join. No raw socket, mutable run or fabricated load facts are
/// exported. Unsupported-yet lifecycle work keeps this complete owner.
///
/// ```compile_fail,E0507
/// fn steal(owner: &mut Box<weaver_harness::FoundationManagement<'_>>) {
///     let _ = owner.finish();
/// }
/// ```
pub struct FoundationManagement<'a> {
    input: Box<FoundationInput<'a>>,
    peer: LifecyclePeer,
    message: Option<LifecycleMessage>,
    answer: Option<ManagementAnswer>,
    stop: Option<StopWork>,
    peer_unavailable: bool,
    send: Option<OrganSend>,
    progress: FoundationManagementProgress,
    detail: Option<String>,
}

impl<'a> FoundationInput<'a> {
    pub fn management(mut self: Box<Self>) -> Result<Box<FoundationManagement<'a>>, Box<Self>> {
        if self.progress() != FoundationInputProgress::ManagementPending {
            return Err(self);
        }
        let peer = self
            .serving
            .owner
            .listener
            .take_connection()
            .expect("actual accepted root connection");
        Ok(Box::new(FoundationManagement {
            input: self,
            peer,
            message: None,
            answer: None,
            stop: None,
            peer_unavailable: false,
            send: None,
            progress: FoundationManagementProgress::Pending,
            detail: None,
        }))
    }
}
impl<'a> FoundationManagement<'a> {
    pub fn progress(&self) -> FoundationManagementProgress {
        self.progress
    }
    pub fn detail(&self) -> Option<&str> {
        self.detail.as_deref()
    }
    pub fn advance(&mut self) -> FoundationManagementProgress {
        self.advance_with_io(&mut MonotonicIo)
    }
    pub(super) fn advance_with_io(&mut self, io: &mut impl PumpIo) -> FoundationManagementProgress {
        if self.progress != FoundationManagementProgress::Pending {
            return self.progress;
        }
        if let Err(mut failure) = self.step(io) {
            if self
                .send
                .as_ref()
                .is_some_and(|send| send.sent() && send.sent_at().is_none())
                || self
                    .stop
                    .as_ref()
                    .and_then(|stop| stop.gate_send.as_ref())
                    .is_some_and(|send| send.sent() && send.sent_at().is_none())
            {
                failure = self.input.serving.owner.context.fail(
                    io,
                    FlowHaltReason::TraceFailed,
                    HaltPoint::Author,
                    format!("management send clock observation failed: {failure:?}"),
                );
            }
            self.detail = Some(format!("{failure:?}"));
            if matches!(
                failure,
                RunFailure::Transport {
                    source: PumpSource::AdminDial,
                    ..
                }
            ) {
                // Management departure is not a run failure. Already transferred
                // content and input deadlines stay with the exact original owner.
                self.progress = FoundationManagementProgress::Dropped;
            } else {
                if matches!(failure, RunFailure::Transport { .. }) {
                    self.input.serving.owner.context.fail(
                        io,
                        FlowHaltReason::ProtocolFailed,
                        HaltPoint::Barrier,
                        format!("serving management transport: {failure:?}"),
                    );
                }
                self.input.failure = Some(failure);
                self.input.stage = Stage::Stopped;
                self.progress = FoundationManagementProgress::Stopped;
            }
        }
        self.progress
    }
    fn step(&mut self, io: &mut impl PumpIo) -> Result<(), RunFailure> {
        self.input.serving.owner.context.live(io)?;
        let done = self.stop.as_ref().is_none_or(|stop| stop.gate_done);
        if done && let Some(message) = &self.message {
            if self.peer_unavailable {
                self.progress = FoundationManagementProgress::Dropped;
                return Ok(());
            }
            if let Some(send) = &self.send
                && send.sent()
                && self.peer.owns_send(send)
                && send.matches_exchange(&message.envelope.exchange)
            {
                self.progress = FoundationManagementProgress::Answered;
                return Ok(());
            }
        }
        let owner = &mut self.input.serving.owner;
        let pumped = owner.context.observe_ready(&mut PumpPorts {
            lifecycle: LifecyclePorts {
                listener: Some(&mut owner.listener),
                dial: (!self.peer_unavailable).then_some(&mut self.peer),
                residency: Some(owner.resident.residency),
                ..Default::default()
            },
            gate: Some(&mut owner.gate),
            decode: Some(owner.decode),
            label: None,
            io,
        });
        self.keep_dial_failure(pumped)?;
        if self.message.is_none() {
            let Some(message) = self.peer.held() else {
                if self.peer_unavailable {
                    self.progress = FoundationManagementProgress::Dropped;
                }
                return Ok(());
            };
            let envelope = &message.envelope;
            if envelope.exchange.opener != Opener::Admin
                || envelope.position != Position::Open
                || !matches!(envelope.payload, Payload::Directive(_))
                || ((message.sink.is_some() || message.state.is_some())
                    && !matches!(
                        envelope.payload,
                        Payload::Directive(LifecycleDirective::Enter { .. })
                    ))
            {
                self.detail = Some("invalid management opener or descriptors".into());
                self.progress = FoundationManagementProgress::Dropped;
                return Ok(());
            }
            if !matches!(
                envelope.payload,
                Payload::Directive(LifecycleDirective::Observe | LifecycleDirective::Stop { .. })
            ) {
                self.progress = FoundationManagementProgress::DirectivePending;
                return Ok(());
            }
            // An earlier measured refusal owns its close. Settle that exact
            // transaction before deciding whether this Stop has a turn left;
            // two owners must never author competing turn.closed events.
            if matches!(
                envelope.payload,
                Payload::Directive(LifecycleDirective::Stop { .. })
            ) && !self.input.settle_refusal_before_stop(
                (!self.peer_unavailable).then_some(&mut self.peer),
                io,
            )? {
                return Ok(());
            }
            self.message =
                self.peer
                    .take_completion()
                    .map_err(|failure| RunFailure::Transport {
                        source: PumpSource::AdminDial,
                        failure,
                    })?;
            let message = self.message.as_ref().unwrap();
            let owner = &mut self.input.serving.owner;
            match &message.envelope.payload {
                Payload::Directive(LifecycleDirective::Observe) => {
                    let Some(load) = owner.resident.evidence.startup.loaded.as_ref() else {
                        return Err(owner.context.fail(
                            io,
                            FlowHaltReason::ProtocolFailed,
                            HaltPoint::Barrier,
                            "entered worker lacks its original recorded load facts",
                        ));
                    };
                    self.answer = Some(ManagementAnswer::Observation(ObservationEnvelope {
                        exchange: message.envelope.exchange.clone(),
                        position: Position::Close,
                        payload: ObservationContent::Answer(ObservationAnswer::State {
                            state: if owner.context.open_turn.is_some() {
                                AgentState::Active
                            } else {
                                AgentState::Idle
                            },
                            load: Some(load.clone()),
                            constituents: Vec::new(),
                        }),
                    }));
                }
                Payload::Directive(LifecycleDirective::Stop { cause }) => {
                    if owner.context.open_turn.is_none() {
                        self.answer = Some(ManagementAnswer::Lifecycle(OrganEnvelope {
                            exchange: message.envelope.exchange.clone(),
                            position: Position::Close,
                            payload: Payload::Answer(LifecycleAnswer::AtRest),
                        }));
                    } else {
                        self.stop = Some(StopWork::begin(&mut self.input, *cause, io)?);
                    }
                }
                _ => unreachable!("judged management directive"),
            }
        }
        if let Some(stop) = &mut self.stop {
            let progress = stop.advance(
                &mut self.input,
                (!self.peer_unavailable).then_some(&mut self.peer),
                io,
            );
            self.keep_dial_failure(progress)?;
            if self.answer.is_none()
                && self.send.is_none()
                && let Some(answer) = self.stop.as_ref().unwrap().answer()
            {
                self.answer = Some(ManagementAnswer::Lifecycle(OrganEnvelope {
                    exchange: self.message.as_ref().unwrap().envelope.exchange.clone(),
                    position: Position::Close,
                    payload: Payload::Answer(answer),
                }));
            }
        }
        if !self.peer_unavailable
            && self.send.is_none()
            && let Some(answer) = self.answer.take()
        {
            let staged = match answer {
                ManagementAnswer::Observation(answer) => {
                    match self.peer.stage_observation(answer) {
                        Ok(TrackedStage::Queued(send)) => Ok(TrackedStage::Queued(send)),
                        Ok(TrackedStage::Busy(answer)) => {
                            Ok(TrackedStage::Busy(ManagementAnswer::Observation(answer)))
                        }
                        Err(error) => {
                            self.answer = Some(ManagementAnswer::Observation(error.input));
                            Err(error.failure)
                        }
                    }
                }
                ManagementAnswer::Lifecycle(answer) => match self.peer.stage_tracked(answer) {
                    Ok(TrackedStage::Queued(send)) => Ok(TrackedStage::Queued(send)),
                    Ok(TrackedStage::Busy(answer)) => {
                        Ok(TrackedStage::Busy(ManagementAnswer::Lifecycle(answer)))
                    }
                    Err(error) => {
                        self.answer = Some(ManagementAnswer::Lifecycle(error.input));
                        Err(error.failure)
                    }
                },
            };
            match staged {
                Ok(TrackedStage::Queued(send)) => self.send = Some(send),
                Ok(TrackedStage::Busy(answer)) => self.answer = Some(answer),
                Err(failure) => self.keep_dial_failure::<()>(Err(RunFailure::Transport {
                    source: PumpSource::AdminDial,
                    failure,
                }))?,
            }
        }
        if self.stop.as_ref().is_some_and(|stop| !stop.gate_done) {
            // After EAGAIN the gate needs the sole pump's ordinary writable
            // wait. Nonblocking lifecycle observation alone would strand the
            // owed close once the admin answer is sent or its peer departs.
            let owner = &mut self.input.serving.owner;
            let pumped = owner.context.pump_once(&mut PumpPorts {
                lifecycle: LifecyclePorts {
                    listener: Some(&mut owner.listener),
                    dial: (!self.peer_unavailable).then_some(&mut self.peer),
                    residency: Some(owner.resident.residency),
                    ..Default::default()
                },
                gate: Some(&mut owner.gate),
                decode: Some(owner.decode),
                label: None,
                io,
            });
            self.keep_dial_failure(pumped)?;
        }
        Ok(())
    }

    fn keep_dial_failure<T>(&mut self, result: Result<T, RunFailure>) -> Result<(), RunFailure> {
        match result {
            Ok(_) => Ok(()),
            Err(
                failure @ RunFailure::Transport {
                    source: PumpSource::AdminDial,
                    ..
                },
            ) if !self
                .send
                .as_ref()
                .is_some_and(|s| s.sent() && s.sent_at().is_none()) =>
            {
                // Once a Stop has been accepted, root departure cannot abandon
                // its close or the original gate response. Even a complete
                // request followed immediately by EOF is judged before disposal.
                self.peer_unavailable = true;
                self.detail = Some(format!("{failure:?}"));
                Ok(())
            }
            Err(failure) => Err(failure),
        }
    }
    /// Return the exact suspended input after an actual answer or a dropped
    /// connection. Pending directives and stopped owners remain wholly owned.
    pub fn finish(self: Box<Self>) -> Result<Box<FoundationInput<'a>>, Box<Self>> {
        if matches!(
            self.progress,
            FoundationManagementProgress::Answered | FoundationManagementProgress::Dropped
        ) {
            Ok(self.input)
        } else {
            Err(self)
        }
    }
}

#[cfg(test)]
#[path = "context_worker_management_tests.rs"]
pub(in super::super::super) mod tests;
