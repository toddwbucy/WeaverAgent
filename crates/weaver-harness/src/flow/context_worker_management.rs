//! Management owns the accepted root connection and suspends, never replaces,
//! its input owner. Observe reads retained facts; Stop owns its recorded close.
use super::*;
use crate::flow::clock::AbsoluteBound;
use crate::flow::pump::PumpDeadline;
use crate::flow::pump::{LifecycleMessage, LifecyclePeer, ReceivedCapture, RetiredReceivedCapture};
use weaver_types::state_flow::capture::{
    CaptureDirective, CaptureEnvelope, CapturePayload, CaptureRefusal,
};
use weaver_types::state_flow::enter::{ObservationAnswer, ObservationContent, ObservationEnvelope};
use weaver_types::{AgentState, LifecycleAnswer, LifecycleDirective};
#[path = "context_worker_stop.rs"]
pub(in crate::flow::context::worker_startup) mod stop;
use stop::StopWork;

#[path = "context_worker_entered_rollback_handoff.rs"]
mod rollback_handoff;

#[path = "context_worker_graceful_handoff.rs"]
mod graceful_handoff;

#[path = "context_worker_save_handoff.rs"]
mod save_handoff;

enum ManagementAnswer {
    Observation(ObservationEnvelope),
    Lifecycle(OrganEnvelope),
    Capture(CaptureEnvelope),
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
    pub(super) input: Box<FoundationInput<'a>>,
    peer: LifecyclePeer,
    message: Option<LifecycleMessage>,
    save_message: Option<ReceivedCapture>,
    answer: Option<ManagementAnswer>,
    stop: Option<StopWork>,
    peer_unavailable: bool,
    send: Option<OrganSend>,
    progress: FoundationManagementProgress,
    detail: Option<String>,
    heard_ns: Option<u64>,
    read: Option<AbsoluteBound>,
    read_armed: bool,
    wait_ready: bool,
}

/// A failed management exchange retains its exact request, answer and send;
/// only its descriptors retire. It cannot produce a successful Leave.
pub(super) struct FailedManagement {
    _peer: crate::flow::pump::RetiredLifecyclePeer,
    _message: Option<crate::flow::pump::RetiredLifecycleMessage>,
    _save_message: Option<RetiredReceivedCapture>,
    _answer: Option<ManagementAnswer>,
    _stop: Option<StopWork>,
    _peer_unavailable: bool,
    _send: Option<OrganSend>,
    _progress: FoundationManagementProgress,
    _detail: Option<String>,
    _heard_ns: Option<u64>,
    _read: Option<AbsoluteBound>,
    _read_armed: bool,
}
impl FailedManagement {
    pub(super) fn world_was_sent(&self) -> bool {
        self._stop
            .as_ref()
            .and_then(StopWork::gate_send)
            .is_some_and(OrganSend::sent)
    }
}

impl<'a> FoundationInput<'a> {
    pub fn management(mut self: Box<Self>) -> Result<Box<FoundationManagement<'a>>, Box<Self>> {
        if self.progress() != FoundationInputProgress::ManagementPending {
            return Err(self);
        }
        let read = if self.has_graceful_leave() || self.serving.save.is_some() {
            let Some(accepted) = self.serving.owner.listener.accepted_ns() else {
                return Err(self);
            };
            let Ok(bound) =
                AbsoluteBound::after(accepted, crate::state::ANSWER_BOUND_MS * 1_000_000)
            else {
                return Err(self);
            };
            Some(bound)
        } else {
            None
        };
        let mut peer = self
            .serving
            .owner
            .listener
            .take_connection()
            .expect("actual accepted root connection");
        peer.enable_serving_openers()
            .expect("pristine accepted serving connection");
        Ok(Box::new(FoundationManagement {
            input: self,
            peer,
            message: None,
            save_message: None,
            answer: None,
            stop: None,
            peer_unavailable: false,
            send: None,
            progress: FoundationManagementProgress::Pending,
            detail: None,
            heard_ns: None,
            read,
            read_armed: false,
            wait_ready: false,
        }))
    }
}
impl<'a> FoundationManagement<'a> {
    pub(super) fn into_failed_parts(self) -> (Box<FoundationInput<'a>>, FailedManagement) {
        let Self {
            input,
            peer,
            message,
            save_message,
            answer,
            stop,
            peer_unavailable,
            send,
            progress,
            detail,
            heard_ns,
            read,
            read_armed,
            wait_ready: _,
        } = self;
        (
            input,
            FailedManagement {
                _peer: peer.retire_failed(),
                _message: message.map(LifecycleMessage::retire_failed),
                _save_message: save_message.map(ReceivedCapture::retire_failed),
                _answer: answer,
                _stop: stop,
                _peer_unavailable: peer_unavailable,
                _send: send,
                _progress: progress,
                _detail: detail,
                _heard_ns: heard_ns,
                _read: read,
                _read_armed: read_armed,
            },
        )
    }
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
        self.wait_ready = false;
        let result = self.step(io);
        self.reduce_result(result, io)
    }

    /// Wait only after the reducer reaches transport or organ-settlement work.
    /// Held requests and unfinished Stop work return to their original reducer.
    /// The common pump retains this peer and every existing absolute deadline.
    pub fn wait_for_progress(&mut self) -> FoundationManagementProgress {
        self.wait_with_io(&mut MonotonicIo)
    }

    fn wait_with_io(&mut self, io: &mut impl PumpIo) -> FoundationManagementProgress {
        if self.progress != FoundationManagementProgress::Pending || !self.wait_ready {
            return self.progress;
        }
        // A wake must be reduced before another wait. In particular, a newly
        // retained EOF still needs its observation's original settlement wake.
        self.wait_ready = false;
        // The flow owner reduces its own buffered replies and observation tick.
        // Otherwise a due tick would remain immediately ready on every poll.
        // Private input/Stop completions stay with their existing owners.
        let result = match self.input.serving.owner.context.reduce_flow(io) {
            Ok(crate::flow::coordinator::Advance::Progress) => Ok(()),
            Ok(_) => self.pump(io, true),
            Err(failure) => Err(failure),
        };
        self.reduce_result(result, io)
    }

    fn reduce_result(
        &mut self,
        result: Result<(), RunFailure>,
        io: &mut impl PumpIo,
    ) -> FoundationManagementProgress {
        if let Err(mut failure) = result {
            if self.input.final_send_clock_failed()
                || self
                    .send
                    .as_ref()
                    .is_some_and(|send| send.sent() && send.sent_at().is_none())
                || self
                    .stop
                    .as_ref()
                    .and_then(|stop| stop.gate_send())
                    .is_some_and(|send| send.sent() && send.sent_at().is_none())
            {
                failure = self.input.serving.owner.context.fail(
                    io,
                    FlowHaltReason::TraceFailed,
                    HaltPoint::Author,
                    format!("management send clock observation failed: {failure:?}"),
                );
            }
            if matches!(&failure, RunFailure::Wait { key: PumpDeadline::AdminRead, bound }
                if self.read_armed && self.read == Some(*bound))
            {
                self.input
                    .serving
                    .owner
                    .context
                    .complete_wait(PumpDeadline::AdminRead);
                self.read_armed = false;
                self.progress = FoundationManagementProgress::Dropped;
                self.detail = Some("secondary Leave dial supplied no timely directive".into());
                return self.progress;
            }
            match self.input.retain_organ_end(&failure, io) {
                Ok(true) => return self.progress,
                Ok(false) => (),
                Err(error) => failure = error,
            }
            if self.input.retain_save_interrupt(&failure)
                || self.input.retain_graceful_expiry(&failure)
                || self.input.retain_quiesce_gate_end(&failure)
            {
                // Conversion does not supply a directive. Keep this owner's
                // original read/answer work runnable until its actual outcome.
                return self.progress;
            }
            if self.input.serving.save.is_some()
                && self.input.serving.owner.context.failure().is_none()
                && matches!(
                    failure,
                    RunFailure::Wait {
                        key: PumpDeadline::CaptureResult | PumpDeadline::CaptureWindow,
                        ..
                    }
                )
            {
                // The retained save owns timeout reduction and any correlated
                // cleanup. An unrelated secondary dial cannot strand it here.
                self.progress = FoundationManagementProgress::Dropped;
                self.detail =
                    Some("secondary management yielded to the original save deadline".into());
                return self.progress;
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
        self.input.prepare_save_window_expiry(io)?;
        if let Some(bound) = self.read
            && !self.read_armed
            && self.message.is_none()
            && self.save_message.is_none()
            && self.progress == FoundationManagementProgress::Pending
        {
            self.input
                .serving
                .owner
                .context
                .arm(PumpDeadline::AdminRead, bound, io)?;
            self.read_armed = true;
        }
        match self
            .input
            .advance_organ_faults((!self.peer_unavailable).then_some(&mut self.peer), io)
        {
            Ok(true) => (),
            Ok(false) => {
                self.wait_ready = true;
                return Ok(());
            }
            Err(failure) => {
                // A root departure must not strand its already accepted Stop.
                // The observation and the original directive keep their owners.
                self.keep_dial_failure::<()>(Err(failure))?;
                return Ok(());
            }
        }
        self.input.serving.owner.context.live(io)?;
        let done = self.stop.as_ref().is_none_or(|stop| stop.gate_done);
        let exchange = self
            .message
            .as_ref()
            .map(|message| &message.envelope.exchange)
            .or_else(|| {
                self.save_message
                    .as_ref()
                    .map(|message| &message.message.envelope.exchange)
            });
        if done && let Some(exchange) = exchange {
            if self.peer_unavailable {
                self.progress = FoundationManagementProgress::Dropped;
                return Ok(());
            }
            if let Some(send) = &self.send
                && send.sent()
                && self.peer.owns_send(send)
                && send.matches_exchange(exchange)
            {
                self.progress = FoundationManagementProgress::Answered;
                return Ok(());
            }
        }
        self.pump(io, false)?;
        if self.heard_ns.is_none()
            && (self.peer.capture_held().is_some() || self.peer.held().is_some())
        {
            // The opener is already retained by the sole recvmsg owner. Its
            // receive reading must follow that read, not the pump's earlier
            // liveness sample: admin may have sent between those two actions.
            let context = &mut self.input.serving.owner.context;
            let now = io.now().map_err(|error| {
                context.fail(
                    io,
                    FlowHaltReason::TraceFailed,
                    HaltPoint::Author,
                    format!("management receive clock: {error}"),
                )
            })?;
            if now < context.last_ns {
                return Err(context.fail(
                    io,
                    FlowHaltReason::TraceFailed,
                    HaltPoint::Author,
                    "management receive clock reversed",
                ));
            }
            context.last_ns = now;
            self.heard_ns = Some(now);
        }
        if self.message.is_none()
            && self.save_message.is_none()
            && let Some(message) = self.peer.capture_held()
        {
            if message.envelope.exchange.opener != Opener::Admin
                || message.envelope.position != Position::Open
                || !matches!(
                    message.envelope.payload,
                    CapturePayload::Directive(CaptureDirective::SavePoint { .. })
                )
                || message.sink.is_some()
                || message.state.is_some()
            {
                self.detail = Some("invalid serving save opener or descriptors".into());
                self.progress = FoundationManagementProgress::Dropped;
                return Ok(());
            }
            if self.read_armed {
                self.input
                    .serving
                    .owner
                    .context
                    .complete_wait(PumpDeadline::AdminRead);
                self.read_armed = false;
            }
            if !self.input.has_graceful_leave() && self.input.serving.save.is_none() {
                self.progress = FoundationManagementProgress::DirectivePending;
                return Ok(());
            }
            self.save_message =
                self.peer
                    .take_capture()
                    .map_err(|failure| RunFailure::Transport {
                        source: PumpSource::AdminDial,
                        failure,
                    })?;
            self.peer
                .enable_capture()
                .map_err(|failure| RunFailure::Transport {
                    source: PumpSource::AdminDial,
                    failure,
                })?;
            self.answer = Some(ManagementAnswer::Capture(CaptureEnvelope {
                exchange: self
                    .save_message
                    .as_ref()
                    .unwrap()
                    .message
                    .envelope
                    .exchange
                    .clone(),
                position: Position::Close,
                payload: CapturePayload::Refusal(CaptureRefusal::Lifecycle(
                    weaver_types::LifecycleRefusal::OutOfOrder,
                )),
            }));
        }
        if self.message.is_none() && self.save_message.is_none() {
            let Some(message) = self.peer.held() else {
                if self.peer_unavailable {
                    self.progress = FoundationManagementProgress::Dropped;
                } else {
                    self.wait_ready = true;
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
            let leaving = self.input.has_graceful_leave() || self.input.serving.save.is_some();
            let delegate = if leaving {
                self.input.has_graceful_leave()
                    && matches!(
                        envelope.payload,
                        Payload::Directive(LifecycleDirective::JoinLeave { .. })
                    )
            } else {
                matches!(
                    envelope.payload,
                    Payload::Directive(LifecycleDirective::Leave { .. })
                )
            };
            if delegate {
                if self.read_armed {
                    self.input
                        .serving
                        .owner
                        .context
                        .complete_wait(PumpDeadline::AdminRead);
                    self.read_armed = false;
                }
                self.progress = FoundationManagementProgress::DirectivePending;
                return Ok(());
            }
            // An earlier measured refusal owns its close. Settle that exact
            // transaction before deciding whether this Stop has a turn left;
            // two owners must never author competing turn.closed events.
            if !leaving
                && matches!(
                    envelope.payload,
                    Payload::Directive(LifecycleDirective::Stop { .. })
                )
                && !self.input.settle_refusal_before_stop(
                    (!self.peer_unavailable).then_some(&mut self.peer),
                    io,
                )?
            {
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
            if self.read_armed {
                self.input
                    .serving
                    .owner
                    .context
                    .complete_wait(PumpDeadline::AdminRead);
                self.read_armed = false;
            }
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
                        payload: ObservationContent::Answer(if leaving {
                            ObservationAnswer::InTransition {
                                constituents: Vec::new(),
                            }
                        } else {
                            ObservationAnswer::State {
                                state: if owner.context.open_turn.is_some() {
                                    AgentState::Active
                                } else {
                                    AgentState::Idle
                                },
                                load: Some(load.clone()),
                                constituents: Vec::new(),
                            }
                        }),
                    }));
                }
                Payload::Directive(directive)
                    if leaving || !matches!(directive, LifecycleDirective::Stop { .. }) =>
                {
                    self.answer = Some(ManagementAnswer::Lifecycle(OrganEnvelope {
                        exchange: message.envelope.exchange.clone(),
                        position: Position::Close,
                        payload: Payload::Refusal(weaver_types::LifecycleRefusal::OutOfOrder),
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
                ManagementAnswer::Capture(answer) => match self.peer.stage_capture(answer) {
                    Ok(TrackedStage::Queued(send)) => Ok(TrackedStage::Queued(send)),
                    Ok(TrackedStage::Busy(answer)) => {
                        Ok(TrackedStage::Busy(ManagementAnswer::Capture(answer)))
                    }
                    Err(error) => {
                        self.answer = Some(ManagementAnswer::Capture(error.input));
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
        self.wait_ready = !self.peer_unavailable
            && self.stop.as_ref().is_none_or(|stop| stop.gate_done)
            && self.answer.is_none()
            && self.send.as_ref().is_some_and(|send| !send.sent());
        Ok(())
    }

    fn pump(&mut self, io: &mut impl PumpIo, wait: bool) -> Result<(), RunFailure> {
        let retained_gate_end = self.input.retained_quiesce_gate_end();
        let owner = &mut self.input.serving.owner;
        let pumped = owner.context.pump_with_wait(
            &mut PumpPorts {
                lifecycle: LifecyclePorts {
                    listener: Some(&mut owner.listener),
                    dial: (!self.peer_unavailable).then_some(&mut self.peer),
                    residency: Some(owner.resident.residency),
                    leave_peer: self
                        .input
                        .serving
                        .save
                        .as_mut()
                        .and_then(|save| save.port()),
                    ..Default::default()
                },
                gate: (!retained_gate_end).then_some(&mut owner.gate),
                decode: Some(owner.decode),
                label: None,
                io,
            },
            wait,
        );
        self.keep_dial_failure(pumped)
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
    pub fn finish(mut self: Box<Self>) -> Result<Box<FoundationInput<'a>>, Box<Self>> {
        if matches!(
            self.progress,
            FoundationManagementProgress::Answered | FoundationManagementProgress::Dropped
        ) {
            if self.read_armed {
                self.input
                    .serving
                    .owner
                    .context
                    .complete_wait(PumpDeadline::AdminRead);
                self.read_armed = false;
            }
            Ok(self.input)
        } else {
            Err(self)
        }
    }
}

#[cfg(test)]
#[path = "context_worker_management_tests.rs"]
pub(in super::super::super) mod tests;

#[cfg(test)]
#[path = "context_worker_management_wait_tests.rs"]
pub(in crate::flow::context::worker_startup) mod wait_tests;

#[cfg(test)]
#[path = "context_worker_dispatch_tests.rs"]
pub(in crate::flow::context::worker_startup) mod dispatch_tests;
