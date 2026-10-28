//! The serial lifecycle before a Serving run owns a recording context.
//! Diagnostic Enter stays intact for its separately elected caller.
use super::{FoundationProcessExit, FoundationRun};
use crate::flow::clock::{AbsoluteBound, MonotonicIo, PumpIo, WaitInterest};
use crate::flow::pump::enter::{
    AdmissionReceiveFailure, AdoptionFailureKind, BeforeEnterOpener, EnterReceiver, EnterTimeError,
    ReceivedBeforeEnter, ReceivedEnter,
};
use crate::flow::pump::{LifecycleListener, LifecyclePeer};
use crate::flow::pump_transport::{OrganSend, TrackedStage};
use crate::{ChannelFault, CoordinationListener};
use nix::poll::PollFlags;
use weaver_types::{
    BindingKind, LifecycleAnswer, LifecycleDirective, LifecycleRefusal, OrganEnvelope, Payload,
    Position,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoundationAdmissionProgress {
    Pending,
    Received,
    Stopped,
}

struct Response {
    answer: Option<OrganEnvelope>,
    send: Option<OrganSend>,
    enter: bool,
    bound: Option<AbsoluteBound>,
}

/// No child or recording context exists until successful Serving adoption.
/// Every accepted peer has one receiver; later connections stay in the backlog.
pub struct FoundationAdmission {
    listener: LifecycleListener,
    pub(super) receiver: Option<EnterReceiver>,
    pub(super) received: Option<ReceivedEnter>,
    ordinary: Option<ReceivedBeforeEnter>,
    response: Option<Response>,
    ready: bool,
    stopped: bool,
    pub(super) detail: Option<String>,
    last_ns: Option<u64>,
    wait_ready: bool,
    delivery_loss: Option<String>,
}

impl FoundationAdmission {
    pub fn listen(listener: CoordinationListener) -> Result<Self, String> {
        crate::lifecycle::clear_dumpable()
            .map_err(|error| format!("admission hygiene: {error:?}"))?;
        Ok(Self {
            listener: LifecycleListener::new(listener).map_err(|error| format!("{error:?}"))?,
            receiver: None,
            received: None,
            ordinary: None,
            response: None,
            ready: false,
            stopped: false,
            detail: None,
            last_ns: None,
            wait_ready: false,
            delivery_loss: None,
        })
    }

    pub fn progress(&self) -> FoundationAdmissionProgress {
        if self.stopped {
            FoundationAdmissionProgress::Stopped
        } else if self.ready {
            FoundationAdmissionProgress::Received
        } else {
            FoundationAdmissionProgress::Pending
        }
    }

    /// The original canonical binding. A Diagnostic result is never adopted by
    /// this Serving owner and must return intact to its separate caller.
    pub fn binding_kind(&self) -> Option<BindingKind> {
        self.ready.then(
            || match self.received.as_ref().unwrap().envelope().enter().binding {
                weaver_types::state_flow::enter::EnterBinding::Serving { .. } => {
                    BindingKind::Serving
                }
                weaver_types::state_flow::enter::EnterBinding::Diagnostic { .. } => {
                    BindingKind::Diagnostic
                }
            },
        )
    }

    pub fn detail(&self) -> Option<&str> {
        self.detail.as_deref()
    }

    /// Only a terminal admission failure permits whole-worker failure exit.
    /// Refused or expired unadmitted input is still a standing before-Enter service.
    pub fn process_exit(&self) -> Option<FoundationProcessExit> {
        self.stopped.then_some(FoundationProcessExit::Failure)
    }

    /// The worker reports an undelivered answer once. No next connection is
    /// accepted until this notice is taken, so delayed callers cannot lose one
    /// notice to another or cause an unbounded diagnostic queue.
    pub fn take_delivery_loss(&mut self) -> Option<String> {
        self.delivery_loss.take()
    }

    pub fn advance(&mut self) -> FoundationAdmissionProgress {
        self.advance_with_io(&mut MonotonicIo)
    }

    pub(super) fn advance_with_io(&mut self, io: &mut impl PumpIo) -> FoundationAdmissionProgress {
        self.wait_ready = false;
        if self.progress() != FoundationAdmissionProgress::Pending || self.delivery_loss.is_some() {
            return self.progress();
        }
        if self.response.is_some() {
            return self.advance_response(io);
        }
        if self.receiver.is_none() {
            match self.listener.accept_enter() {
                Ok(Some(peer)) => {
                    // The prior unadmitted source has finished its disposition;
                    // only now may the next root exchange replace its evidence.
                    self.received = None;
                    self.ordinary = None;
                    self.detail = None;
                    self.receiver = Some(EnterReceiver::from_peer(peer).ok().expect("unused peer"));
                }
                Ok(None) => {
                    self.wait_ready = true;
                    return self.progress();
                }
                Err(error) => return self.stop(format!("admission listener: {error:?}")),
            }
        }
        let result = self.receiver.as_mut().unwrap().advance_admission(io);
        if let Err(error) = result {
            return match error {
                AdmissionReceiveFailure::Channel(failure)
                    if failure.fault == ChannelFault::Closed =>
                {
                    self.receiver = None;
                    self.progress()
                }
                AdmissionReceiveFailure::Channel(failure) => {
                    self.stop(format!("admission receive: {failure:?}"))
                }
                AdmissionReceiveFailure::Clock(detail) => {
                    self.stop(format!("admission receive clock: {detail}"))
                }
            };
        }
        let receiver = self.receiver.take().unwrap();
        let receiver = match receiver.into_ordinary() {
            Ok(ordinary) => {
                if let Err(detail) = self.observe(ordinary.received_ns) {
                    self.ordinary = Some(ordinary);
                    return self.stop(detail);
                }
                let (exchange, answer) = match &ordinary.message.envelope {
                    BeforeEnterOpener::Ordinary(envelope) => (
                        envelope.exchange.clone(),
                        if matches!(
                            envelope.payload,
                            Payload::Directive(LifecycleDirective::Observe)
                        ) {
                            Payload::Answer(LifecycleAnswer::State {
                                state: weaver_types::AgentState::Unloaded,
                                load: None,
                                constituents: Vec::new(),
                            })
                        } else {
                            Payload::Refusal(LifecycleRefusal::OutOfOrder)
                        },
                    ),
                    BeforeEnterOpener::Save(envelope) => (
                        envelope.exchange.clone(),
                        Payload::Refusal(LifecycleRefusal::OutOfOrder),
                    ),
                };
                self.ordinary = Some(ordinary);
                self.response = Some(Response {
                    answer: Some(OrganEnvelope {
                        exchange,
                        position: Position::Close,
                        payload: answer,
                    }),
                    send: None,
                    enter: false,
                    bound: None,
                });
                return self.advance_response(io);
            }
            Err(receiver) => receiver,
        };
        let original = match receiver.into_received() {
            Ok(original) => original,
            Err(receiver) => {
                self.receiver = Some(receiver);
                self.wait_ready = true;
                return self.progress();
            }
        };
        if let Err(detail) = self.observe(original.observed_ns()) {
            self.received = Some(original);
            return self.stop(detail);
        }
        let timing = original.check_window(io);
        let observed = original.observed_ns();
        self.received = Some(original);
        if let Err(detail) = self.observe(observed) {
            return self.stop(detail);
        }
        match timing {
            Ok(()) => self.ready = true,
            Err(EnterTimeError::Future) => {
                self.detail = Some("Malformed Enter: window starts after packet receipt".into());
                let bound = self.received.as_ref().unwrap().window();
                self.refuse(LifecycleRefusal::Malformed, bound);
                return self.advance_response(io);
            }
            Err(EnterTimeError::Expired) => {
                self.detail = Some("original Enter expired before admission".into());
                self.received.as_mut().unwrap().close_unadmitted();
            }
            Err(EnterTimeError::Clock(detail)) => {
                return self.stop(format!("Enter clock: {detail}"));
            }
        }
        self.progress()
    }

    /// Adopt only the canonical Serving packet and its original handles/window.
    /// Recoverable failure returns this owner with its exact refusal or expiry
    /// still to drive; Diagnostic is returned without consuming any source.
    #[allow(clippy::result_large_err)]
    pub fn adopt(self) -> Result<FoundationRun, Self> {
        self.adopt_with_io(&mut MonotonicIo)
    }

    #[allow(clippy::result_large_err)]
    pub(super) fn adopt_with_io(mut self, io: &mut impl PumpIo) -> Result<FoundationRun, Self> {
        if self.binding_kind() != Some(BindingKind::Serving) {
            return Err(self);
        }
        self.ready = false;
        let original = self.received.take().unwrap();
        match original.adopt(io) {
            Ok(adopted) => {
                let (mut context, original) = adopted.into_parts();
                let resources = context
                    .failed_enter_resources()
                    .expect("fresh adopted context");
                Ok(FoundationRun {
                    context,
                    resources,
                    listener: Some(self.listener),
                    original: Some(original),
                })
            }
            Err(failure) => {
                self.detail = Some(failure.detail.clone());
                self.received = Some(failure.source);
                let observed = self.received.as_ref().unwrap().observed_ns();
                if let Err(detail) = self.observe(observed) {
                    self.stop(detail);
                    return Err(self);
                }
                match failure.kind {
                    AdoptionFailureKind::Refused(refusal) => {
                        let bound = self.received.as_ref().unwrap().window();
                        self.refuse(refusal, bound);
                    }
                    AdoptionFailureKind::Expired => {
                        self.received.as_mut().unwrap().close_unadmitted()
                    }
                    AdoptionFailureKind::Channel(failure)
                        if failure.fault == ChannelFault::Closed =>
                    {
                        self.received.as_mut().unwrap().close_unadmitted();
                    }
                    AdoptionFailureKind::Diagnostic => self.ready = true,
                    AdoptionFailureKind::Clock
                    | AdoptionFailureKind::Channel(_)
                    | AdoptionFailureKind::Invalid => {
                        self.stop(failure.detail);
                    }
                }
                Err(self)
            }
        }
    }

    fn refuse(&mut self, refusal: LifecycleRefusal, bound: Option<AbsoluteBound>) {
        let original = self.received.as_mut().expect("actual unadmitted Enter");
        original.close_unadmitted();
        self.response = Some(Response {
            answer: Some(OrganEnvelope {
                exchange: original.envelope().exchange.clone(),
                position: Position::Close,
                payload: Payload::Refusal(refusal),
            }),
            send: None,
            enter: true,
            bound,
        });
    }

    fn peer_mut(&mut self, enter: bool) -> &mut LifecyclePeer {
        if enter {
            self.received.as_mut().unwrap().peer_mut()
        } else {
            &mut self.ordinary.as_mut().unwrap().peer
        }
    }

    fn advance_response(&mut self, io: &mut impl PumpIo) -> FoundationAdmissionProgress {
        let now = match self.clock(io) {
            Ok(now) => now,
            Err(detail) => return self.stop(detail),
        };
        let mut response = self.response.take().unwrap();
        if response.bound.is_some_and(|bound| bound.expired(now)) {
            return self.progress();
        }
        let mut failure = None;
        if let Some(answer) = response.answer.take() {
            match self.peer_mut(response.enter).stage_tracked(answer) {
                Ok(TrackedStage::Queued(send)) => response.send = Some(send),
                Ok(TrackedStage::Busy(answer)) => response.answer = Some(answer),
                Err(error) => {
                    response.answer = Some(error.input);
                    failure = Some(error.failure);
                }
            }
        }
        if failure.is_none() {
            // Serialization/staging is admission work too. It does not earn a
            // later first-send allowance from the original Enter window.
            let now = match self.clock(io) {
                Ok(now) => now,
                Err(detail) => {
                    self.response = Some(response);
                    return self.stop(detail);
                }
            };
            if response.bound.is_some_and(|bound| bound.expired(now)) {
                return self.progress();
            }
            failure = self.peer_mut(response.enter).try_send_observed(io).err();
        }
        if let Some(send) = &response.send
            && send.sent()
        {
            let Some(sent_at) = send.sent_at() else {
                self.response = Some(response);
                return self.stop("admission answer lost its send clock".into());
            };
            if let Err(detail) = self.observe(sent_at) {
                self.response = Some(response);
                return self.stop(detail);
            }
            // The admin closes an answered connection. Keep the sole receive
            // owner for another directive or its EOF; dropping a socket with a
            // second unread packet can reset it before the first answer is read.
            let receiver = if response.enter {
                match self.received.take().unwrap().resume() {
                    Ok(receiver) => receiver,
                    Err(original) => {
                        self.received = Some(original);
                        self.response = Some(response);
                        return self.stop("answered Enter peer cannot resume receive".into());
                    }
                }
            } else {
                match self.ordinary.take().unwrap().resume() {
                    Ok(receiver) => receiver,
                    Err(original) => {
                        self.ordinary = Some(original);
                        self.response = Some(response);
                        return self.stop("answered lifecycle peer cannot resume receive".into());
                    }
                }
            };
            self.receiver = Some(receiver);
            return self.progress();
        }
        if let Some(failure) = failure {
            if failure.fault == ChannelFault::Closed {
                self.delivery_loss = Some(format!(
                    "lifecycle answer was not delivered: {}",
                    failure.detail
                ));
                self.ordinary = None;
                return self.progress();
            }
            self.response = Some(response);
            return self.stop(format!("admission answer: {failure:?}"));
        }
        self.response = Some(response);
        self.wait_ready = true;
        self.progress()
    }

    /// A before-Enter peer has no invented opener deadline. A valid received
    /// Enter refusal keeps only that original window while waiting to send.
    pub fn wait_for_progress(&mut self) -> FoundationAdmissionProgress {
        self.wait_with_io(&mut MonotonicIo)
    }

    fn wait_with_io(&mut self, io: &mut impl PumpIo) -> FoundationAdmissionProgress {
        if self.progress() != FoundationAdmissionProgress::Pending
            || !self.wait_ready
            || self.delivery_loss.is_some()
        {
            return self.progress();
        }
        self.wait_ready = false;
        let now = match self.clock(io) {
            Ok(now) => now,
            Err(detail) => return self.stop(detail),
        };
        let bound = self.response.as_ref().and_then(|response| response.bound);
        if bound.is_some_and(|bound| bound.expired(now)) {
            return self.progress();
        }
        let interest = if let Some(response) = &self.response {
            let fd = if response.enter {
                self.received.as_ref().unwrap().peer().fd()
            } else {
                self.ordinary.as_ref().unwrap().peer.fd()
            };
            // A second unread packet cannot wake an answer waiting for capacity.
            WaitInterest {
                fd,
                events: PollFlags::POLLOUT
                    | PollFlags::POLLHUP
                    | PollFlags::POLLERR
                    | PollFlags::from_bits_retain(nix::libc::POLLRDHUP),
            }
        } else if let Some(receiver) = &self.receiver {
            receiver.interest()
        } else {
            let (fd, events) = self.listener.interest();
            WaitInterest { fd, events }
        };
        if let Err(error) = io.wait(&[interest], bound.map(AbsoluteBound::deadline_ns)) {
            return self.stop(format!("admission wait: {error}"));
        }
        if let Err(detail) = self.clock(io) {
            return self.stop(detail);
        }
        self.progress()
    }

    fn observe(&mut self, now: u64) -> Result<(), String> {
        if self.last_ns.is_some_and(|previous| now < previous) {
            return Err("admission clock reversed".into());
        }
        self.last_ns = Some(now);
        Ok(())
    }

    fn clock(&mut self, io: &mut impl PumpIo) -> Result<u64, String> {
        let now = io
            .now()
            .map_err(|error| format!("admission clock: {error}"))?;
        self.observe(now)?;
        Ok(now)
    }

    fn stop(&mut self, detail: String) -> FoundationAdmissionProgress {
        if !self.stopped {
            self.stopped = true;
            self.detail = Some(detail);
        }
        self.progress()
    }
}

#[cfg(test)]
#[path = "context_worker_pre_enter_owner_tests.rs"]
mod tests;
