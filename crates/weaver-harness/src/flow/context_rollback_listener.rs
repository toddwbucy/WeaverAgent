//! Secondary dials belong to the accepted rollback, never to boot cancellation.
use super::*;
use crate::flow::pump::{LifecycleListener, PumpSource};
use weaver_types::{LifecycleAnswer, LifecycleRefusal, Payload};

pub(super) struct RollbackDials {
    pub(super) listener: LifecycleListener,
    pub(super) active: Option<PendingDial>,
    sealing: bool,
}

pub(super) struct PendingDial {
    pub(super) peer: LifecyclePeer,
    pub(super) read: Option<AbsoluteBound>,
    pub(super) message: Option<LifecycleMessage>,
    pub(super) answer: Option<OrganEnvelope>,
    pub(super) send: Option<OrganSend>,
}

impl RollbackDials {
    pub(super) fn new(listener: LifecycleListener) -> Self {
        Self {
            listener,
            active: None,
            sealing: false,
        }
    }

    fn drop_dial(&mut self, context: &mut RecordingContext, detail: &str) {
        if let Some(dial) = self.active.take() {
            if let Some(message) = dial.message.as_ref().or_else(|| dial.peer.held()) {
                eprintln!(
                    "weaver-harness: answer to exchange {} not delivered/completed ({detail}); accepted Leave continues",
                    message.envelope.exchange.ordinal
                );
            }
            if let Some(bound) = dial.read {
                // Never retire another owner's deadline on a malformed handoff.
                if context.deadlines.get(PumpDeadline::AdminRead) == Some(bound) {
                    context.complete_wait(PumpDeadline::AdminRead);
                }
            }
        }
    }

    pub(super) fn prepare(
        &mut self,
        context: &mut RecordingContext,
        leave: AbsoluteBound,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        context.live(io)?;
        if self.active.is_none() && self.listener.held().is_some() {
            let Some(accepted) = self.listener.accepted_ns() else {
                return Err(context.fail(
                    io,
                    FlowHaltReason::ProtocolFailed,
                    HaltPoint::Barrier,
                    "rollback listener has an unclocked accepted connection",
                ));
            };
            if accepted > context.last_ns {
                return Err(context.fail(
                    io,
                    FlowHaltReason::TraceFailed,
                    HaltPoint::Author,
                    "rollback dial clock precedes its actual accept observation",
                ));
            }
            let end = accepted
                .saturating_add(crate::state::ANSWER_BOUND_MS * 1_000_000)
                .min(leave.deadline_ns());
            let bound = AbsoluteBound::from_absolute(accepted, end).map_err(|error| {
                context.fail(
                    io,
                    FlowHaltReason::ProtocolFailed,
                    HaltPoint::Barrier,
                    error.to_string(),
                )
            })?;
            let (peer, observed) = self
                .listener
                .take_observed_connection()
                .expect("validated held listener observation remains owned");
            debug_assert_eq!(observed, accepted);
            self.active = Some(PendingDial {
                peer,
                read: Some(bound),
                message: None,
                answer: None,
                send: None,
            });
            context.arm(PumpDeadline::AdminRead, bound, io)?;
            // Acceptance may have preceded a delayed reduction. Do not grant a
            // late directive a fresh two seconds when the owner finally runs.
            context.live(io)?;
        }
        let Some(dial) = &mut self.active else {
            return Ok(());
        };
        if dial.peer.failure().is_some() {
            self.drop_dial(context, "dialer transport closed or failed");
            return Ok(());
        }
        if dial.send.as_ref().is_some_and(OrganSend::sent) {
            self.active = None;
            return Ok(());
        }
        if dial.message.is_none() && dial.peer.held().is_some() {
            dial.message =
                dial.peer
                    .take_completion()
                    .map_err(|failure| RunFailure::Transport {
                        source: PumpSource::AdminDial,
                        failure,
                    })?;
            if let Some(bound) = dial.read.take()
                && context.complete_wait(PumpDeadline::AdminRead) != Some(bound)
            {
                return Err(context.fail(
                    io,
                    FlowHaltReason::ProtocolFailed,
                    HaltPoint::Barrier,
                    "rollback dial lost its original read deadline",
                ));
            }
            let message = dial
                .message
                .as_ref()
                .expect("held directive was consumed once");
            if message.envelope.exchange.opener != Opener::Admin
                || message.envelope.position != Position::Open
                || ((message.sink.is_some() || message.state.is_some())
                    && !matches!(
                        message.envelope.payload,
                        Payload::Directive(LifecycleDirective::Enter { .. })
                    ))
            {
                self.drop_dial(context, "invalid lifecycle opener or handles");
                return Ok(());
            }
            let payload = match &message.envelope.payload {
                Payload::Directive(LifecycleDirective::Observe) if !self.sealing => {
                    Payload::Answer(LifecycleAnswer::InTransition {
                        constituents: Vec::new(),
                    })
                }
                // In particular, JoinLeave cannot join a rollback, and another
                // Leave cannot replace its original cause, forced state or cap.
                Payload::Directive(_) => Payload::Refusal(LifecycleRefusal::OutOfOrder),
                _ => {
                    self.drop_dial(context, "lifecycle dial did not open a directive");
                    return Ok(());
                }
            };
            dial.answer = Some(OrganEnvelope {
                exchange: message.envelope.exchange.clone(),
                position: Position::Close,
                payload,
            });
        }
        if dial.send.is_none()
            && let Some(answer) = &dial.answer
        {
            match dial.peer.stage_tracked(answer.clone()) {
                Ok(TrackedStage::Queued(send)) => dial.send = Some(send),
                Ok(TrackedStage::Busy(_)) => {
                    return Err(context.fail(
                        io,
                        FlowHaltReason::ProtocolFailed,
                        HaltPoint::Barrier,
                        "fresh rollback dial already has an outgoing answer",
                    ));
                }
                Err(error) => {
                    return Err(RunFailure::Transport {
                        source: PumpSource::AdminDial,
                        failure: error.failure,
                    });
                }
            }
        }
        Ok(())
    }

    /// Seal before organ release. Unlink once, exhaust the actual backlog and
    /// answer every secondary directive OutOfOrder within the existing Leave.
    pub(super) fn seal_step(
        &mut self,
        context: &mut RecordingContext,
        leave: AbsoluteBound,
        io: &mut impl PumpIo,
    ) -> Result<bool, RunFailure> {
        if !self.sealing {
            self.listener.unlink();
            self.sealing = true;
        }
        let result = (|| {
            context.live(io)?;
            let attempted = self.active.is_none();
            let accepted = if attempted {
                self.listener
                    .accept_observed(context.last_ns)
                    .map_err(|failure| RunFailure::Transport {
                        source: PumpSource::Listener,
                        failure,
                    })?
            } else {
                false
            };
            self.prepare(context, leave, io)?;
            if attempted && self.active.is_none() && self.listener.held().is_none() && !accepted {
                return Ok(true);
            }
            if self.active.is_none() {
                return Ok(false);
            }
            context.pump_once(&mut PumpPorts {
                lifecycle: LifecyclePorts {
                    dial: self.active.as_mut().map(|dial| &mut dial.peer),
                    ..Default::default()
                },
                gate: None,
                decode: None,
                label: None,
                io,
            })?;
            Ok(false)
        })();
        match result {
            Ok(done) => Ok(done),
            Err(error) => {
                self.recover(context, error, io)?;
                Ok(false)
            }
        }
    }

    pub(super) fn ports<'a>(&'a mut self, peer: &'a mut LifecyclePeer) -> LifecyclePorts<'a> {
        LifecyclePorts {
            listener: Some(&mut self.listener),
            leave_peer: peer.failure().is_none().then_some(peer),
            dial: self.active.as_mut().map(|dial| &mut dial.peer),
            ..Default::default()
        }
    }

    /// Only a secondary peer or its exact silent-read expiry is recoverable.
    /// Listener/state/trace/clock failure and the original Leave expiry survive.
    pub(super) fn recover(
        &mut self,
        context: &mut RecordingContext,
        error: RunFailure,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        match &error {
            RunFailure::Wait {
                key: PumpDeadline::AdminRead,
                bound,
            } if self.active.as_ref().and_then(|dial| dial.read) == Some(*bound) => {
                self.drop_dial(context, "silent dial exceeded its original read allowance");
                context.live(io)
            }
            RunFailure::Transport {
                source: PumpSource::AdminDial,
                ..
            } if self.active.is_some() => {
                if self
                    .active
                    .as_ref()
                    .and_then(|dial| dial.send.as_ref())
                    .is_some_and(|send| send.sent() && send.sent_at().is_none())
                {
                    return Err(context.fail(
                        io,
                        FlowHaltReason::TraceFailed,
                        HaltPoint::Author,
                        format!("rollback dial send clock observation failed: {error:?}"),
                    ));
                }
                self.drop_dial(context, "dialer transport closed or failed");
                context.live(io)
            }
            _ => Err(error),
        }
    }
}

pub(super) struct RetiredRollbackDials {
    pub(super) listener: crate::flow::pump::RetiredLifecycleListener,
    pub(super) active: Option<RetiredRollbackDial>,
}
pub(super) struct RetiredRollbackDial {
    pub(super) peer: crate::flow::pump::RetiredLifecyclePeer,
    pub(super) read: Option<AbsoluteBound>,
    pub(super) message: Option<crate::flow::pump::RetiredLifecycleMessage>,
    pub(super) answer: Option<OrganEnvelope>,
    pub(super) send: Option<OrganSend>,
}
impl RollbackDials {
    pub(super) fn retire_failed(self) -> RetiredRollbackDials {
        RetiredRollbackDials {
            listener: self.listener.retire_failed(),
            active: self.active.map(|dial| RetiredRollbackDial {
                peer: dial.peer.retire_failed(),
                read: dial.read,
                message: dial.message.map(LifecycleMessage::retire_failed),
                answer: dial.answer,
                send: dial.send,
            }),
        }
    }
}
