//! S9 keeps the actual entered run and accepts a new root operation.
use super::*;
use crate::flow::pump::ReceivedCapture;
use weaver_types::state_flow::capture::{CaptureDirective, CapturePayload};
use weaver_types::state_flow::enter::{LoadFacts, ObservationAnswer, ObservationContent};

pub(in crate::flow::context) enum RestingRequest {
    Leave(LifecycleMessage),
    Save(ReceivedCapture),
}

pub(in crate::flow::context) struct RestingCommand {
    pub(in crate::flow::context) peer: LifecyclePeer,
    pub(in crate::flow::context) request: RestingRequest,
    pub(in crate::flow::context) heard_ns: u64,
}

pub(in crate::flow::context) struct RetiredRestingCommand {
    _peer: crate::flow::pump::RetiredLifecyclePeer,
    _leave: Option<crate::flow::pump::RetiredLifecycleMessage>,
    _save: Option<crate::flow::pump::RetiredReceivedCapture>,
    _heard_ns: u64,
}
impl RestingCommand {
    pub(in crate::flow::context) fn retire(self) -> RetiredRestingCommand {
        let (leave, save) = match self.request {
            RestingRequest::Leave(message) => (Some(message.retire_failed()), None),
            RestingRequest::Save(message) => (None, Some(message.retire_failed())),
        };
        RetiredRestingCommand {
            _peer: self.peer.retire_failed(),
            _leave: leave,
            _save: save,
            _heard_ns: self.heard_ns,
        }
    }
}

impl RollbackDials {
    pub(in crate::flow::context) fn freeze_refusal(&mut self) {
        self.freeze_outcome();
        self.refusing = !self.outcome().0;
    }

    pub(in crate::flow::context) fn restart_leave(&mut self, forced: bool, no_residency: bool) {
        assert!(!self.sealing && self.refusing && self.active.is_none());
        assert!(
            self.graceful
                .as_ref()
                .is_none_or(|held| held.joins.is_empty())
        );
        self.graceful = None;
        self.refusing = false;
        if !forced {
            self.adopt_graceful(no_residency, None, Vec::new());
        }
    }

    pub(in crate::flow::context) fn resting_ports(&mut self) -> LifecyclePorts<'_> {
        LifecyclePorts {
            listener: Some(&mut self.listener),
            dial: self.active.as_mut().map(|dial| &mut dial.peer),
            ..Default::default()
        }
    }

    pub(in crate::flow::context) fn prepare_resting(
        &mut self,
        context: &mut RecordingContext,
        load: &LoadFacts,
        accept_operations: bool,
        io: &mut impl PumpIo,
    ) -> Result<Option<RestingCommand>, RunFailure> {
        assert!(self.refusing && !self.sealing);
        self.prepare_dial(context, None, io)?;
        let Some(dial) = &mut self.active else {
            return Ok(None);
        };
        // A silent connection accepted during the ended Leave keeps its read
        // allowance. New S9 connections get their own original read allowance.
        if !dial.serving_opener
            && dial.message.is_none()
            && dial.save.is_none()
            && dial.peer.held().is_none()
        {
            dial.peer
                .enable_serving_openers()
                .map_err(|failure| RunFailure::Transport {
                    source: PumpSource::AdminDial,
                    failure,
                })?;
            dial.serving_opener = true;
        }
        if dial.save.is_none()
            && let Some(message) = dial.peer.capture_held()
        {
            if message.envelope.exchange.opener != Opener::Admin
                || message.envelope.position != Position::Open
                || message.sink.is_some()
                || message.state.is_some()
                || !matches!(
                    message.envelope.payload,
                    CapturePayload::Directive(CaptureDirective::SavePoint { .. })
                )
            {
                self.drop_dial(context, "invalid S9 save opener");
                return Ok(None);
            }
            Self::finish_resting_read(dial, context, io)?;
            let request = dial
                .peer
                .take_capture()
                .map_err(|failure| RunFailure::Transport {
                    source: PumpSource::AdminDial,
                    failure,
                })?
                .expect("complete S9 save opener");
            if accept_operations {
                let dial = self.active.take().unwrap();
                return Ok(Some(RestingCommand {
                    peer: dial.peer,
                    request: RestingRequest::Save(request),
                    heard_ns: context.last_ns,
                }));
            }
            dial.save_answer = Some(CaptureEnvelope {
                exchange: request.message.envelope.exchange.clone(),
                position: Position::Close,
                payload: CapturePayload::Refusal(
                    weaver_types::state_flow::capture::CaptureRefusal::Lifecycle(
                        LifecycleRefusal::OutOfOrder,
                    ),
                ),
            });
            dial.save = Some(request);
            dial.peer
                .enable_capture()
                .map_err(|failure| RunFailure::Transport {
                    source: PumpSource::AdminDial,
                    failure,
                })?;
        }
        if dial.message.is_none() && dial.peer.held().is_some() {
            dial.message =
                dial.peer
                    .take_completion()
                    .map_err(|failure| RunFailure::Transport {
                        source: PumpSource::AdminDial,
                        failure,
                    })?;
            Self::finish_resting_read(dial, context, io)?;
            let message = dial.message.as_ref().unwrap();
            if message.envelope.exchange.opener != Opener::Admin
                || message.envelope.position != Position::Open
                || message.sink.is_some()
                || message.state.is_some()
            {
                self.drop_dial(context, "invalid S9 lifecycle opener");
                return Ok(None);
            }
            match &message.envelope.payload {
                Payload::Directive(LifecycleDirective::Leave {
                    rollback: false, ..
                }) if accept_operations => {
                    let dial = self.active.take().unwrap();
                    return Ok(Some(RestingCommand {
                        peer: dial.peer,
                        request: RestingRequest::Leave(dial.message.unwrap()),
                        heard_ns: context.last_ns,
                    }));
                }
                Payload::Directive(LifecycleDirective::Observe) => {
                    let plan = context.committed_plan_summary().map(|value| value.cloned());
                    dial.observation = Some(ObservationEnvelope {
                        exchange: message.envelope.exchange.clone(),
                        position: Position::Close,
                        payload: ObservationContent::Answer(match plan {
                            Ok(plan) => ObservationAnswer::State {
                                plan,
                                state: weaver_types::AgentState::Idle,
                                load: Some(Box::new(load.clone())),
                                constituents: Vec::new(),
                            },
                            Err(_) => ObservationAnswer::InTransition {
                                constituents: Vec::new(),
                            },
                        }),
                    });
                }
                Payload::Directive(_) => {
                    dial.answer = Some(OrganEnvelope {
                        exchange: message.envelope.exchange.clone(),
                        position: Position::Close,
                        payload: if matches!(
                            message.envelope.payload,
                            Payload::Directive(LifecycleDirective::Stop { .. })
                        ) {
                            Payload::Answer(LifecycleAnswer::AtRest)
                        } else {
                            Payload::Refusal(LifecycleRefusal::OutOfOrder)
                        },
                    });
                }
                _ => {
                    self.drop_dial(context, "S9 dial did not open a directive");
                    return Ok(None);
                }
            }
        }
        if dial.send.is_none() {
            let result = if let Some(answer) = &dial.save_answer {
                dial.peer
                    .stage_capture(answer.clone())
                    .map(|stage| match stage {
                        TrackedStage::Queued(send) => Some(send),
                        TrackedStage::Busy(_) => None,
                    })
                    .map_err(|error| error.failure)
            } else if let Some(answer) = &dial.observation {
                dial.peer
                    .stage_observation(answer.clone())
                    .map(|stage| match stage {
                        TrackedStage::Queued(send) => Some(send),
                        TrackedStage::Busy(_) => None,
                    })
                    .map_err(|error| error.failure)
            } else if let Some(answer) = &dial.answer {
                dial.peer
                    .stage_tracked(answer.clone())
                    .map(|stage| match stage {
                        TrackedStage::Queued(send) => Some(send),
                        TrackedStage::Busy(_) => None,
                    })
                    .map_err(|error| error.failure)
            } else {
                return Ok(None);
            };
            dial.send = result.map_err(|failure| RunFailure::Transport {
                source: PumpSource::AdminDial,
                failure,
            })?;
            if dial.send.is_none() {
                return Err(context.fail(
                    io,
                    FlowHaltReason::ProtocolFailed,
                    HaltPoint::Barrier,
                    "S9 dial has a competing answer",
                ));
            }
        }
        Ok(None)
    }

    fn finish_resting_read(
        dial: &mut PendingDial,
        context: &mut RecordingContext,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        if let Some(bound) = dial.read.take()
            && context.complete_wait(PumpDeadline::AdminRead) != Some(bound)
        {
            return Err(context.fail(
                io,
                FlowHaltReason::ProtocolFailed,
                HaltPoint::Barrier,
                "S9 dial lost its original read allowance",
            ));
        }
        Ok(())
    }
}
