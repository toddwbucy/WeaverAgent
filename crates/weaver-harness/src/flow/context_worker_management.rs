//! Management owns the accepted root connection and suspends, never replaces,
//! its input owner. Observe reads only retained run facts and authors nothing.
use super::*;
use crate::flow::pump::{LifecycleMessage, LifecyclePeer};
use weaver_types::state_flow::enter::{ObservationAnswer, ObservationContent, ObservationEnvelope};
use weaver_types::{AgentState, LifecycleDirective};

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
    answer: Option<ObservationEnvelope>,
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
    fn advance_with_io(&mut self, io: &mut impl PumpIo) -> FoundationManagementProgress {
        if self.progress != FoundationManagementProgress::Pending {
            return self.progress;
        }
        if let Err(mut failure) = self.step(io) {
            if self
                .send
                .as_ref()
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
        let owner = &mut self.input.serving.owner;
        owner.context.live(io)?;
        if let Some(send) = &self.send
            && send.sent()
            && self.peer.owns_send(send)
            && self
                .message
                .as_ref()
                .is_some_and(|message| send.matches_exchange(&message.envelope.exchange))
        {
            self.progress = FoundationManagementProgress::Answered;
            return Ok(());
        }
        owner.context.observe_ready(&mut PumpPorts {
            lifecycle: LifecyclePorts {
                // The active dial suppresses a second listener accept. It is
                // not an original Enter peer or a granted Leave owner.
                listener: Some(&mut owner.listener),
                dial: Some(&mut self.peer),
                residency: Some(owner.resident.residency),
                ..Default::default()
            },
            gate: Some(&mut owner.gate),
            decode: Some(owner.decode),
            label: None,
            io,
        })?;
        if self.message.is_none()
            && let Some(message) = self.peer.held()
        {
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
                Payload::Directive(LifecycleDirective::Observe)
            ) {
                self.progress = FoundationManagementProgress::DirectivePending;
                return Ok(());
            }
            self.message =
                self.peer
                    .take_completion()
                    .map_err(|failure| RunFailure::Transport {
                        source: PumpSource::AdminDial,
                        failure,
                    })?;
            let Some(load) = owner.resident.evidence.startup.loaded.as_ref() else {
                return Err(owner.context.fail(
                    io,
                    FlowHaltReason::ProtocolFailed,
                    HaltPoint::Barrier,
                    "entered worker lacks its original recorded load facts",
                ));
            };
            self.answer = Some(ObservationEnvelope {
                exchange: self.message.as_ref().unwrap().envelope.exchange.clone(),
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
            });
        }
        if self.send.is_none()
            && let Some(answer) = self.answer.take()
        {
            match self.peer.stage_observation(answer) {
                Ok(TrackedStage::Queued(send)) => self.send = Some(send),
                Ok(TrackedStage::Busy(answer)) => self.answer = Some(answer),
                Err(error) => {
                    self.answer = Some(error.input);
                    return Err(RunFailure::Transport {
                        source: PumpSource::AdminDial,
                        failure: error.failure,
                    });
                }
            }
        }
        Ok(())
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
