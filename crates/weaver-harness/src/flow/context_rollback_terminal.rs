//! Finish the actual no-SPU rollback after its retained Grants comparison.
//! A stopped owner retains every original; it never grants a successful Left.
use super::*;
use weaver_trace::MemberRelease;
use weaver_types::{LifecycleAnswer, Payload as WirePayload};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum TerminalStage {
    Seal,
    Classifier,
    State,
    Unload,
    Drain,
    Answer,
    Complete,
    Stopped,
}

pub(in crate::flow) struct RollbackTerminal<'a> {
    owner: PreparedRollback<'a>,
    stage: TerminalStage,
    unload: Option<AuthoredEvent>,
    unload_started: bool,
    returned: StoppedAuthor,
    unload_sequence: Option<Sequence>,
    answer: Option<OrganEnvelope>,
    send: Option<OrganSend>,
    detail: Option<String>,
}

impl<'a> PreparedRollback<'a> {
    #[allow(clippy::result_large_err)]
    pub(in crate::flow) fn into_terminal(self) -> Result<RollbackTerminal<'a>, Self> {
        let startup = &self.owner.response.refusal.owner.startup;
        if startup.resources.spu_child().is_some()
            || !startup.resources.matches(startup.context)
            || startup.context.phase != RecordingPhase::Releasing
            || startup.context.failure().is_some()
            || !startup.context.flow.classifier_retired()
            || self.owner.response.original.is_none()
            || startup.context.deadlines.get(PumpDeadline::Leave) != Some(self.owner.leave)
        {
            return Err(self);
        }
        let cause = self
            .owner
            .response
            .original
            .as_ref()
            .unwrap()
            .payload()
            .cause;
        let unload = AuthoredEvent {
            kind: Kind::Unload,
            subsystem: weaver_trace::Subsystem::Harness,
            turn: None,
            source: crate::flow::author::AuthoredSource::Runtime,
            causal_parent: None,
            payload: Some(Payload::Unload(Box::new(serving::payloads::UnloadClose {
                grant_surface: Some(self.grant_surface),
                cause: Some(weaver_trace::Cause { uid: cause.uid }),
                forced: true,
                forced_by: None,
                release: serving::payloads::Release {
                    spu: None,
                    member: Some(MemberRelease::Closed),
                },
                end_reason: None,
                state_flow_failure: None,
            }))),
        };
        Ok(RollbackTerminal {
            owner: self,
            stage: TerminalStage::Seal,
            unload: Some(unload),
            unload_started: false,
            returned: StoppedAuthor::default(),
            unload_sequence: None,
            answer: None,
            send: None,
            detail: None,
        })
    }
}

impl RollbackTerminal<'_> {
    pub(in crate::flow) fn stage(&self) -> TerminalStage {
        self.stage
    }
    pub(in crate::flow) fn detail(&self) -> Option<&str> {
        self.detail.as_deref()
    }
    pub(in crate::flow) fn leave(&self) -> AbsoluteBound {
        self.owner.owner.leave
    }
    pub(in crate::flow) fn sequence(&self) -> Option<Sequence> {
        self.unload_sequence
    }

    fn check_clock(&mut self, io: &mut impl PumpIo) -> Result<(), String> {
        let context = &mut self.owner.owner.response.refusal.owner.startup.context;
        if context.phase != RecordingPhase::Closed {
            return match context.live(io) {
                Ok(()) => Ok(()),
                Err(error) => self
                    .owner
                    .owner
                    .dials
                    .recover(context, error, io)
                    .map_err(|error| format!("rollback: {error:?}")),
            };
        }
        let now = io
            .now()
            .map_err(|error| format!("rollback answer clock: {error}"))?;
        if now < context.last_ns {
            return Err("rollback answer clock reversed".into());
        }
        context.last_ns = now;
        if self.owner.owner.leave.expired(now) {
            return Err("rollback answer window expired".into());
        }
        Ok(())
    }

    pub(in crate::flow) fn advance(&mut self, io: &mut impl PumpIo) -> TerminalStage {
        if matches!(self.stage, TerminalStage::Complete | TerminalStage::Stopped) {
            return self.stage;
        }
        if let Err(detail) = self.check_clock(io).and_then(|()| self.step(io)) {
            self.detail = Some(detail);
            self.stage = TerminalStage::Stopped;
        }
        self.stage
    }

    fn step(&mut self, io: &mut impl PumpIo) -> Result<(), String> {
        let rollback = &mut self.owner.owner;
        let startup = &mut rollback.response.refusal.owner.startup;
        let context = &mut startup.context;
        match self.stage {
            TerminalStage::Seal => {
                if rollback
                    .dials
                    .seal_step(context, rollback.leave, io)
                    .map_err(|error| format!("rollback seal: {error:?}"))?
                {
                    self.stage = TerminalStage::Classifier;
                }
            }
            TerminalStage::Classifier => {
                use crate::flow::context::failed_enter_driver::ChildState;
                match startup.resources.poll_classifier_child() {
                    Some(ChildState::Reaped(_)) => self.stage = TerminalStage::State,
                    Some(ChildState::Running) => {
                        let wake = context
                            .last_ns
                            .saturating_add(10_000_000)
                            .min(rollback.leave.deadline_ns());
                        io.wait(&[], Some(wake))
                            .map_err(|error| format!("rollback child wait: {error}"))?;
                    }
                    _ => return Err("rollback classifier child custody unavailable".into()),
                }
            }
            TerminalStage::State => {
                if context
                    .retire_state(io)
                    .map_err(|error| format!("rollback member: {error:?}"))?
                    == Advance::Progress
                {
                    self.stage = TerminalStage::Unload;
                }
            }
            TerminalStage::Unload => {
                self.returned
                    .check()
                    .map_err(|error| format!("rollback Unload: {error:?}"))?;
                let mut ports = PumpPorts {
                    lifecycle: LifecyclePorts::default(),
                    gate: None,
                    decode: None,
                    label: None,
                    io,
                };
                let result = if let Some(unload) = self.unload.take() {
                    self.unload_started = true;
                    context.author_and_wait(&mut ports, unload)
                } else {
                    context.resume_author_and_wait(&mut ports)
                };
                self.unload_sequence = result
                    .as_ref()
                    .ok()
                    .copied()
                    .or_else(|| context.release_recorded_sequence());
                match result {
                    Ok(_) => self.stage = TerminalStage::Drain,
                    Err(error) => {
                        return Err(format!(
                            "rollback Unload: {:?}",
                            self.returned
                                .retain_started(error, &mut self.unload_started)
                        ));
                    }
                }
            }
            TerminalStage::Drain => {
                let mut ports = PumpPorts {
                    lifecycle: LifecyclePorts::default(),
                    gate: None,
                    decode: None,
                    label: None,
                    io,
                };
                context
                    .close_after_drain(&mut ports)
                    .map_err(|error| format!("rollback drain: {error:?}"))?;
                self.answer = Some(OrganEnvelope {
                    exchange: rollback.directive.envelope.exchange.clone(),
                    position: Position::Close,
                    payload: WirePayload::Answer(LifecycleAnswer::Left {
                        save_point: None,
                        forced: true,
                        no_state: false,
                    }),
                });
                self.stage = TerminalStage::Answer;
            }
            TerminalStage::Answer => {
                if let Some(answer) = self.answer.take() {
                    match rollback.peer.stage_tracked(answer) {
                        Ok(TrackedStage::Queued(send)) => self.send = Some(send),
                        Ok(TrackedStage::Busy(answer)) => {
                            self.answer = Some(answer);
                            return Err("rollback Left has a competing sender".into());
                        }
                        Err(error) => {
                            self.answer = Some(error.input);
                            return Err(format!("rollback Left: {:?}", error.failure));
                        }
                    }
                }
                rollback
                    .peer
                    .try_send_observed(io)
                    .map_err(|error| format!("rollback Left send: {error:?}"))?;
                if let Some(send) = &self.send
                    && send.sent()
                {
                    if !rollback.peer.owns_send(send)
                        || !send.matches_exchange(&rollback.directive.envelope.exchange)
                        || !send.sent_at().is_some_and(|at| {
                            at >= rollback.leave.began_ns() && !rollback.leave.expired(at)
                        })
                    {
                        return Err("rollback Left lacks its timely whole-send witness".into());
                    }
                    self.stage = TerminalStage::Complete;
                } else {
                    use crate::flow::clock::WaitInterest;
                    io.wait(
                        &[WaitInterest {
                            fd: rollback.peer.fd(),
                            events: nix::poll::PollFlags::POLLOUT,
                        }],
                        Some(rollback.leave.deadline_ns()),
                    )
                    .map_err(|error| format!("rollback Left wait: {error}"))?;
                }
            }
            TerminalStage::Complete | TerminalStage::Stopped => unreachable!(),
        }
        self.check_clock(io)
    }
}

/// Final evidence after Left's exact whole send. No lifecycle descriptor remains
/// live, and the borrowed run cannot be reused while this receipt is retained.
pub(in crate::flow) struct CompletedRollback<'a> {
    _refusal: RecordedStartupRefusal<'a>,
    _original: Option<crate::flow::pump::enter::DetachedEnter>,
    _enter_peer: crate::flow::pump::RetiredLifecyclePeer,
    _enter_send: Option<OrganSend>,
    _enter_returned: Option<OrganEnvelope>,
    _enter_stop: Option<ResponseStop>,
    _peer: crate::flow::pump::RetiredLifecyclePeer,
    _dials: listener::RetiredRollbackDials,
    _directive: LifecycleMessage,
    _current: Option<InspectionCompletion>,
    _grant_surface: GrantSurface,
    _leave: AbsoluteBound,
    _unload_sequence: Sequence,
    _send: OrganSend,
}
impl<'a> RollbackTerminal<'a> {
    #[allow(clippy::result_large_err)]
    pub(in crate::flow) fn finish(self) -> Result<CompletedRollback<'a>, Self> {
        if self.stage != TerminalStage::Complete {
            return Err(self);
        }
        let PreparedRollback {
            owner,
            grant_surface,
        } = self.owner;
        let RollbackComposition {
            response,
            peer,
            dials,
            directive,
            leave,
            current,
            ..
        } = owner;
        let EnterRefusalResponse {
            refusal,
            peer: enter_peer,
            send: enter_send,
            returned,
            stop,
            original,
            ..
        } = response;
        Ok(CompletedRollback {
            _refusal: refusal,
            _original: original,
            _enter_peer: enter_peer.retire_failed(),
            _enter_send: enter_send,
            _enter_returned: returned,
            _enter_stop: stop,
            _peer: peer.retire_failed(),
            _dials: dials.retire_failed(),
            _directive: directive,
            _current: current,
            _grant_surface: grant_surface,
            _leave: leave,
            _unload_sequence: self.unload_sequence.expect("actual Unload recorded"),
            _send: self.send.expect("actual Left sent"),
        })
    }
}

#[cfg(test)]
impl RollbackTerminal<'_> {
    pub(in crate::flow) fn test_context(&mut self) -> &mut RecordingContext {
        self.owner.owner.response.refusal.owner.startup.context
    }
    pub(in crate::flow) fn test_secondary_bound(&self) -> Option<AbsoluteBound> {
        self.owner
            .owner
            .dials
            .active
            .as_ref()
            .and_then(|dial| dial.read)
    }
    pub(in crate::flow) fn test_peer(&self) -> &LifecyclePeer {
        &self.owner.owner.peer
    }
}
