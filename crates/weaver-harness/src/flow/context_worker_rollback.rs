//! Worker custody from the actual classifier refusal to explicit rollback.
use super::*;
use crate::flow::clock::AbsoluteBound;
use crate::flow::context::startup_composition::refusal::response::terminal::{
    CompletedRollback, RollbackTerminal, TerminalStage,
};
use crate::flow::context::startup_composition::refusal::response::{
    EnterRefusalResponse, ResponseStage, RollbackComposition, RollbackStage,
};
use crate::flow::context::startup_composition::refusal::{
    EnterRefusalComposition, RecordedStartupRefusal,
};
use crate::flow::pump::{LifecycleMessage, LifecyclePeer};
use crate::flow::pump_transport::{OrganSend, TrackedStage};
use weaver_types::{
    LifecycleAnswer, LifecycleDirective, LifecycleRefusal, Opener, OrganEnvelope, Payload, Position,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoundationRollbackProgress {
    Pending,
    AwaitRollback,
    Closed,
    Stopped,
}

enum RollbackState<'a> {
    Record(Box<EnterRefusalComposition<'a>>),
    Recorded(Box<RecordedStartupRefusal<'a>>),
    Response(Box<EnterRefusalResponse<'a>>),
    Await(Box<EnterRefusalResponse<'a>>),
    Rollback(Box<RollbackComposition<'a>>),
    Prepared(
        Box<crate::flow::context::startup_composition::refusal::response::PreparedRollback<'a>>,
    ),
    Terminal(Box<RollbackTerminal<'a>>),
    Closed(Box<CompletedRollback<'a>>),
}
struct AwaitDial {
    peer: LifecyclePeer,
    bound: AbsoluteBound,
    message: Option<LifecycleMessage>,
    send: Option<OrganSend>,
}
/// Retains the original Enter, refusal, process ownership and accepted Leave.
/// A stopped result is custody for teardown, never a successful rollback.
///
/// ```compile_fail,E0277
/// fn cloned<T: Clone>() {}
/// cloned::<weaver_harness::FoundationRollback<'static>>();
/// ```
pub struct FoundationRollback<'a> {
    state: Option<RollbackState<'a>>,
    listener: Option<Box<LifecycleListener>>,
    dial: Option<AwaitDial>,
    enter: AbsoluteBound,
    leave: Option<AbsoluteBound>,
    refusal_sequence: Option<u64>,
    unload_sequence: Option<u64>,
    detail: Option<String>,
}
impl<'a> FoundationEnter<'a> {
    /// Consume only an actual classifier-admission refusal. Every failed join
    /// returns this unchanged startup chain, including its child custody.
    pub fn rollback_refusal(mut self) -> Result<Box<FoundationRollback<'a>>, Self> {
        if self.listener.is_none() || !matches!(self.state, Some(EnterState::Startup(_))) {
            return Err(self);
        }
        let Some(EnterState::Startup(owner)) = self.state.take() else {
            unreachable!()
        };
        let owner = match owner.into_original_refusal() {
            Ok(owner) => owner,
            Err((owner, _)) => {
                self.state = Some(EnterState::Startup(Box::new(owner)));
                return Err(self);
            }
        };
        let enter = owner.original_bound();
        Ok(Box::new(FoundationRollback {
            state: Some(RollbackState::Record(Box::new(owner))),
            listener: self.listener.take(),
            dial: None,
            enter,
            leave: None,
            refusal_sequence: None,
            unload_sequence: None,
            detail: None,
        }))
    }
}
impl FoundationRollback<'_> {
    pub fn progress(&self) -> FoundationRollbackProgress {
        if self.detail.is_some() {
            FoundationRollbackProgress::Stopped
        } else if matches!(self.state, Some(RollbackState::Closed(_))) {
            FoundationRollbackProgress::Closed
        } else if matches!(self.state, Some(RollbackState::Await(_))) {
            FoundationRollbackProgress::AwaitRollback
        } else {
            FoundationRollbackProgress::Pending
        }
    }
    pub fn detail(&self) -> Option<&str> {
        self.detail.as_deref()
    }
    pub fn refusal_sequence(&self) -> Option<u64> {
        self.refusal_sequence
    }
    pub fn unload_sequence(&self) -> Option<u64> {
        self.unload_sequence
    }
    pub(super) fn enter_window(&self) -> AbsoluteBound {
        self.enter
    }
    pub(super) fn leave_window(&self) -> Option<AbsoluteBound> {
        self.leave
    }
    pub fn advance(&mut self) -> FoundationRollbackProgress {
        self.advance_with_io(&mut MonotonicIo)
    }
    fn advance_with_io(&mut self, io: &mut impl PumpIo) -> FoundationRollbackProgress {
        if matches!(
            self.progress(),
            FoundationRollbackProgress::Closed | FoundationRollbackProgress::Stopped
        ) {
            return self.progress();
        }
        let state = self.state.take().expect("one retained rollback stage");
        self.state = Some(match state {
            RollbackState::Record(mut owner) => {
                let result =
                    owner.advance_original_record(self.listener.as_deref_mut().unwrap(), io);
                self.refusal_sequence = owner.recorded_sequence().map(|sequence| sequence.0);
                if self.refusal_sequence.is_some() {
                    match owner.finish_original() {
                        Ok(owner) => RollbackState::Recorded(Box::new(owner)),
                        Err(owner) => {
                            self.detail = Some(format!("refusal record stopped: {result:?}"));
                            RollbackState::Record(Box::new(owner))
                        }
                    }
                } else {
                    if let Err(error) = result {
                        self.detail = Some(format!("refusal record: {error:?}"));
                    }
                    RollbackState::Record(owner)
                }
            }
            RollbackState::Recorded(owner) => match owner.into_original_response() {
                Ok(owner) => RollbackState::Response(Box::new(owner)),
                Err(owner) => {
                    self.detail = Some("refusal lost its original Enter".into());
                    RollbackState::Recorded(Box::new(owner))
                }
            },
            RollbackState::Response(mut owner) => {
                match owner.advance(io) {
                    ResponseStage::Complete => RollbackState::Await(owner),
                    ResponseStage::Stopped
                        if owner.context().failure().is_none()
                            && awaitable_response(&owner, self.enter) =>
                    {
                        // The retained actual refusal may await root after its
                        // response peer departs or the original Enter expires.
                        RollbackState::Await(owner)
                    }
                    ResponseStage::Stopped => {
                        self.detail = Some(format!("refusal response: {:?}", owner.stop()));
                        RollbackState::Response(owner)
                    }
                    ResponseStage::Sending => RollbackState::Response(owner),
                }
            }
            RollbackState::Await(mut owner) => match self.await_step(&mut owner, io) {
                Ok(Some(bound)) => {
                    self.leave = Some(bound);
                    let peer = self.dial.take().unwrap().peer;
                    let listener = *self.listener.take().unwrap();
                    match owner.accept_rollback(peer, listener, bound) {
                        Ok(owner) => RollbackState::Rollback(Box::new(owner)),
                        Err(error) => {
                            self.detail = Some(format!("rollback join: {:?}", error.reason));
                            self.listener = Some(Box::new(error.listener));
                            self.dial = Some(AwaitDial {
                                peer: error.peer,
                                bound,
                                message: None,
                                send: None,
                            });
                            RollbackState::Await(Box::new(error.response))
                        }
                    }
                }
                Ok(None) => RollbackState::Await(owner),
                Err(error) => {
                    self.detail = Some(error);
                    RollbackState::Await(owner)
                }
            },
            RollbackState::Rollback(mut owner) => match owner.advance(io) {
                RollbackStage::Prepared => match owner.into_prepared() {
                    Ok(owner) => RollbackState::Prepared(Box::new(owner)),
                    Err(owner) => {
                        self.detail = Some("rollback lost its Grants comparison".into());
                        RollbackState::Rollback(Box::new(owner))
                    }
                },
                RollbackStage::Stopped => {
                    self.detail = Some(format!("rollback: {:?}", owner.stop()));
                    RollbackState::Rollback(owner)
                }
                _ => RollbackState::Rollback(owner),
            },
            RollbackState::Prepared(owner) => match owner.into_terminal() {
                Ok(owner) => RollbackState::Terminal(Box::new(owner)),
                Err(owner) => {
                    self.detail = Some("rollback terminal handoff refused".into());
                    RollbackState::Prepared(Box::new(owner))
                }
            },
            RollbackState::Terminal(mut owner) => {
                let stage = owner.advance(io);
                self.unload_sequence = owner.sequence().map(|sequence| sequence.0);
                match stage {
                    TerminalStage::Complete => RollbackState::Closed(Box::new(
                        owner.finish().ok().expect("completed terminal"),
                    )),
                    TerminalStage::Stopped => {
                        self.detail = owner.detail().map(str::to_owned);
                        RollbackState::Terminal(owner)
                    }
                    _ => RollbackState::Terminal(owner),
                }
            }
            closed @ RollbackState::Closed(_) => closed,
        });
        self.progress()
    }

    fn await_step(
        &mut self,
        owner: &mut EnterRefusalResponse<'_>,
        io: &mut impl PumpIo,
    ) -> Result<Option<AbsoluteBound>, String> {
        let now = owner
            .observe_await(io)
            .map_err(|error| format!("await rollback: {error:?}"))?;
        let listener = self.listener.as_deref_mut().unwrap();
        if self.dial.is_none() {
            listener
                .accept_observed(now)
                .map_err(|error| format!("rollback accept: {error:?}"))?;
            if let Some((peer, accepted)) = listener.take_observed_connection() {
                let bound =
                    AbsoluteBound::after(accepted, crate::state::ANSWER_BOUND_MS * 1_000_000)
                        .map_err(|error| error.to_string())?;
                self.dial = Some(AwaitDial {
                    peer,
                    bound,
                    message: None,
                    send: None,
                });
            } else {
                owner
                    .pump_await(
                        &mut crate::flow::pump::PumpPorts {
                            lifecycle: LifecyclePorts {
                                listener: Some(listener),
                                ..Default::default()
                            },
                            gate: None,
                            decode: None,
                            label: None,
                            io,
                        },
                        None,
                    )
                    .map_err(|error| format!("rollback accept wait: {error:?}"))?;
                return Ok(None);
            }
        }
        let dial = self.dial.as_mut().unwrap();
        if dial.bound.expired(now) {
            self.dial = None;
            return Ok(None);
        }
        if dial.message.is_none() && dial.peer.held().is_none() && dial.peer.try_receive().is_err()
        {
            self.dial = None;
            return Ok(None);
        }
        let now = owner
            .observe_await(io)
            .map_err(|error| format!("rollback receive clock: {error:?}"))?;
        if dial.bound.expired(now) {
            self.dial = None;
            return Ok(None);
        }
        if let Some(message) = dial.peer.held() {
            if message.envelope.exchange.opener != Opener::Admin
                || message.envelope.position != Position::Open
                || message.sink.is_some()
                || message.state.is_some()
            {
                self.dial = None;
                return Ok(None);
            }
            if matches!(
                message.envelope.payload,
                Payload::Directive(LifecycleDirective::Leave { rollback: true, .. })
            ) {
                return AbsoluteBound::after(now, weaver_types::LEAVE_BOUND_MS * 1_000_000)
                    .map(Some)
                    .map_err(|error| error.to_string());
            }
            dial.message = dial
                .peer
                .take_completion()
                .map_err(|error| format!("rollback directive: {error:?}"))?;
            let message = dial.message.as_ref().unwrap();
            let payload = match &message.envelope.payload {
                Payload::Directive(LifecycleDirective::Observe) => {
                    Payload::Answer(LifecycleAnswer::InTransition {
                        constituents: Vec::new(),
                    })
                }
                Payload::Directive(_) => Payload::Refusal(LifecycleRefusal::OutOfOrder),
                _ => {
                    self.dial = None;
                    return Ok(None);
                }
            };
            let envelope = OrganEnvelope {
                exchange: message.envelope.exchange.clone(),
                position: Position::Close,
                payload,
            };
            match dial.peer.stage_tracked(envelope) {
                Ok(TrackedStage::Queued(send)) => dial.send = Some(send),
                _ => {
                    self.dial = None;
                    return Ok(None);
                }
            }
        }
        if dial.send.is_some() {
            if dial.peer.try_send_observed(io).is_err() {
                if dial
                    .send
                    .as_ref()
                    .is_some_and(|send| send.sent() && send.sent_at().is_none())
                {
                    return Err("rollback secondary send clock failed".into());
                }
                self.dial = None;
                return Ok(None);
            }
            if dial.send.as_ref().is_some_and(OrganSend::sent) {
                self.dial = None;
                return Ok(None);
            }
        }
        let result = owner.pump_await(
            &mut crate::flow::pump::PumpPorts {
                lifecycle: LifecyclePorts {
                    dial: Some(&mut dial.peer),
                    ..Default::default()
                },
                gate: None,
                decode: None,
                label: None,
                io,
            },
            Some(dial.bound),
        );
        match result {
            Ok(()) => {}
            Err(crate::flow::context::RunFailure::Wait {
                key: crate::flow::pump::PumpDeadline::AdminRead,
                bound,
            }) if bound == dial.bound => self.dial = None,
            Err(crate::flow::context::RunFailure::Transport {
                source: crate::flow::pump::PumpSource::AdminDial,
                ..
            }) => {
                if dial
                    .send
                    .as_ref()
                    .is_some_and(|send| send.sent() && send.sent_at().is_none())
                {
                    return Err("rollback secondary send clock failed".into());
                }
                self.dial = None;
            }
            Err(error) => return Err(format!("rollback dial wait: {error:?}")),
        }
        Ok(None)
    }
}

#[cfg(test)]
#[path = "context_worker_rollback_tests.rs"]
pub(super) mod tests;

fn awaitable_response(owner: &EnterRefusalResponse<'_>, enter: AbsoluteBound) -> bool {
    use crate::flow::context::RunFailure;
    use crate::flow::context::startup_composition::refusal::response::ResponseStop;
    match owner.stop() {
        Some(ResponseStop::Run(RunFailure::Wait {
            key: crate::flow::pump::PumpDeadline::Enter,
            bound,
        })) => *bound == enter,
        Some(ResponseStop::Run(RunFailure::Transport {
            source: crate::flow::pump::PumpSource::Admin,
            failure,
        }))
        | Some(ResponseStop::Stage(failure)) => failure.fault == crate::ChannelFault::Closed,
        _ => false,
    }
}
