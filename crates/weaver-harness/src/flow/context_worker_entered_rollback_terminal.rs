//! The accepted Leave's final account and answer retain their actual receipts.
//! Once Unload records, errors return custody for disposal without another author.
use crate::flow::author::{AuthoredEvent, AuthoredSource};
use crate::flow::clock::{AbsoluteBound, PumpIo, WaitInterest};
use crate::flow::context::failed_enter_driver::{ChildState, FailedEnterResources};
use crate::flow::context::worker_startup::failed::StartupGate;
use crate::flow::context::{RecordingContext, RecordingPhase, StoppedAuthor};
use crate::flow::pump::{LifecycleMessage, LifecyclePeer, LifecyclePorts, PumpDeadline, PumpPorts};
use crate::flow::pump_transport::{Failure, OrganSend, TrackedStage};
use nix::poll::PollFlags;
use weaver_trace::serving::{self, Kind};
use weaver_trace::{GrantSurface, MemberRelease, ReleaseOutcome, Sequence, Subsystem};
use weaver_types::state_flow::{SavePointReport, capture::*};
use weaver_types::{Cause, LifecycleAnswer, LifecycleRefusal, OrganEnvelope, Payload, Position};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    Unload,
    Drain,
    Children,
    Answer,
    Complete,
}

#[derive(Clone)]
enum AnswerCarry {
    Rollback(OrganEnvelope),
    Captured(CaptureEnvelope),
}
impl AnswerCarry {
    #[allow(clippy::result_large_err)] // Return the original envelope and transport failure together.
    fn stage(self, peer: &mut LifecyclePeer) -> Result<TrackedStage<Self>, (Self, Failure)> {
        match self {
            Self::Rollback(envelope) => match peer.stage_tracked(envelope) {
                Ok(TrackedStage::Queued(send)) => Ok(TrackedStage::Queued(send)),
                Ok(TrackedStage::Busy(envelope)) => {
                    Ok(TrackedStage::Busy(Self::Rollback(envelope)))
                }
                Err(error) => Err((Self::Rollback(error.input), error.failure)),
            },
            Self::Captured(envelope) => match peer.stage_capture(envelope) {
                Ok(TrackedStage::Queued(send)) => Ok(TrackedStage::Queued(send)),
                Ok(TrackedStage::Busy(envelope)) => {
                    Ok(TrackedStage::Busy(Self::Captured(envelope)))
                }
                Err(error) => Err((Self::Captured(error.input), error.failure)),
            },
        }
    }
}

pub(super) struct EnteredRollbackTerminal {
    stage: Stage,
    unload: Option<AuthoredEvent>,
    started: bool,
    returned: StoppedAuthor,
    sequence: Option<Sequence>,
    spu_stood: bool,
    refusal: Option<LifecycleRefusal>,
    answer: Option<AnswerCarry>,
    capture: Option<SavePointReport>,
    serving: bool,
    forced: bool,
    send: Option<OrganSend>,
    _returned_answer: Option<AnswerCarry>,
    _answer_failure: Option<Failure>,
    stopped: Option<String>,
}

impl EnteredRollbackTerminal {
    #[cfg(test)]
    pub(super) fn test_detail(&self) -> String {
        format!(
            "refusal={:?}, stopped={:?}, sequence={:?}",
            self.refusal, self.stopped, self.sequence
        )
    }

    pub(super) fn new(
        cause: Cause,
        surface: GrantSurface,
        spu: Option<ReleaseOutcome>,
        member: MemberRelease,
        refusal: Option<LifecycleRefusal>,
    ) -> Self {
        Self {
            stage: Stage::Unload,
            unload: Some(AuthoredEvent {
                kind: Kind::Unload,
                subsystem: Subsystem::Harness,
                turn: None,
                source: AuthoredSource::Runtime,
                causal_parent: None,
                payload: Some(serving::Payload::Unload(Box::new(
                    serving::payloads::UnloadClose {
                        grant_surface: Some(surface),
                        cause: Some(weaver_trace::Cause { uid: cause.uid }),
                        forced: true,
                        forced_by: None,
                        release: serving::payloads::Release {
                            spu,
                            member: Some(member),
                        },
                        end_reason: None,
                        state_flow_failure: None,
                    },
                ))),
            }),
            started: false,
            returned: StoppedAuthor::default(),
            sequence: None,
            spu_stood: spu.is_some(),
            refusal,
            answer: None,
            capture: None,
            serving: false,
            forced: true,
            send: None,
            _returned_answer: None,
            _answer_failure: None,
            stopped: None,
        }
    }

    pub(super) fn captured(&mut self, report: SavePointReport) {
        assert!(self.stage == Stage::Unload && self.capture.is_none());
        self.serving = true;
        self.capture = Some(report);
    }

    pub(super) fn refused_capture(&mut self) {
        assert!(self.stage == Stage::Unload && self.capture.is_none() && self.forced);
        self.serving = true;
    }

    pub(super) fn leave_outcome(&mut self, forced: bool, forced_by: Option<Cause>) {
        assert!(self.stage == Stage::Unload && !self.started);
        let Some(serving::Payload::Unload(unload)) = &mut self.unload.as_mut().unwrap().payload
        else {
            unreachable!("original terminal Unload");
        };
        unload.forced = forced;
        unload.forced_by = forced_by.map(|cause| weaver_trace::Cause { uid: cause.uid });
        self.forced = forced;
    }

    pub(super) fn capture_answer(&self) -> Option<&CaptureEnvelope> {
        match &self.answer {
            Some(AnswerCarry::Captured(answer)) if self.complete() => Some(answer),
            _ => None,
        }
    }

    pub(super) fn sequence(&self) -> Option<Sequence> {
        self.sequence
    }

    pub(super) fn complete(&self) -> bool {
        self.stage == Stage::Complete
    }

    #[cfg(test)]
    pub(super) fn test_swap_refusal(
        &mut self,
        refusal: Option<LifecycleRefusal>,
    ) -> Option<LifecycleRefusal> {
        assert!(self.complete());
        std::mem::replace(&mut self.refusal, refusal)
    }

    pub(super) fn process_exit(&self) -> Option<crate::FoundationProcessExit> {
        self.complete().then_some(if self.refusal.is_none() {
            // Completed release/drain/reap remains complete when the final
            // caller departed. This does not claim the answer was sent.
            crate::FoundationProcessExit::Success
        } else {
            crate::FoundationProcessExit::Failure
        })
    }

    #[allow(clippy::too_many_arguments)] // Borrow the actual owners without replacing their custody.
    pub(super) fn step(
        &mut self,
        context: &mut RecordingContext,
        resources: &mut FailedEnterResources,
        gate: &mut StartupGate,
        peer: &mut LifecyclePeer,
        message: &LifecycleMessage,
        leave: AbsoluteBound,
        io: &mut impl PumpIo,
    ) -> Result<bool, String> {
        if let Some(stopped) = &self.stopped {
            return Err(stopped.clone());
        }
        if self.complete() {
            return Ok(true);
        }
        let result = self.step_inner(context, resources, gate, peer, message, leave, io);
        if let Err(detail) = &result {
            self.stopped = Some(detail.clone());
        }
        result
    }

    #[allow(clippy::too_many_arguments)]
    fn step_inner(
        &mut self,
        context: &mut RecordingContext,
        resources: &mut FailedEnterResources,
        gate: &mut StartupGate,
        peer: &mut LifecyclePeer,
        message: &LifecycleMessage,
        leave: AbsoluteBound,
        io: &mut impl PumpIo,
    ) -> Result<bool, String> {
        // The parent observes the raw clock after Unload. Never renew its Leave
        // or use context.live after the successful drain closed the context.
        if context.deadlines.get(PumpDeadline::Leave) != Some(leave)
            || leave.expired(context.last_ns)
        {
            return Err("entered rollback terminal lost its live original Leave".into());
        }
        match self.stage {
            Stage::Unload => {
                if context.phase != RecordingPhase::Releasing
                    || context.failure.is_some()
                    || context.flow.halt().is_some()
                    || !context.flow.classifier_retired()
                    || context.flow.standing_state().is_some()
                    || !resources.matches(context)
                    || !self.spu_stood
                    || resources.spu_child().is_some() != self.spu_stood
                    || !gate
                        .child
                        .as_ref()
                        .is_some_and(|child| matches!(child.state(), ChildState::Reaped(_)))
                    || !matches!(resources.classifier_child(), Some(ChildState::Reaped(_)))
                    || (!self.started && context.release_recorded_sequence().is_some())
                {
                    return Err("entered rollback Unload requires actual released resources".into());
                }
                self.returned
                    .check()
                    .map_err(|error| format!("entered rollback Unload: {error:?}"))?;
                let mut ports = PumpPorts {
                    lifecycle: LifecyclePorts::default(),
                    gate: None,
                    decode: None,
                    label: None,
                    io,
                };
                let result = if let Some(unload) = self.unload.take() {
                    self.started = true;
                    context.author_and_wait(&mut ports, unload)
                } else {
                    context.resume_author_and_wait(&mut ports)
                };
                // The Recorded boundary precedes the fallible final observer.
                // Keep that actual sequence even if this call returns failure.
                self.sequence = result
                    .as_ref()
                    .ok()
                    .copied()
                    .or_else(|| context.release_recorded_sequence());
                match result {
                    Ok(_) => self.stage = Stage::Drain,
                    Err(error) => {
                        return Err(format!(
                            "entered rollback Unload: {:?}",
                            self.returned.retain_started(error, &mut self.started)
                        ));
                    }
                }
            }
            Stage::Drain => {
                context
                    .close_after_drain(&mut PumpPorts {
                        lifecycle: LifecyclePorts::default(),
                        gate: None,
                        decode: None,
                        label: None,
                        io,
                    })
                    .map_err(|error| format!("entered rollback drain: {error:?}"))?;
                self.stage = Stage::Children;
            }
            Stage::Children => {
                for child in [
                    gate.child.as_mut().map(|child| child.poll()),
                    resources.poll_classifier_child(),
                    resources.poll_spu_child(),
                ] {
                    match child {
                        Some(ChildState::Reaped(_)) => {}
                        Some(ChildState::Running) => {
                            let wake = context
                                .last_ns
                                .saturating_add(10_000_000)
                                .min(leave.deadline_ns());
                            io.wait(&[], Some(wake))
                                .map_err(|error| format!("entered rollback child wait: {error}"))?;
                            return Ok(false);
                        }
                        state => {
                            return Err(format!("entered rollback missing actual reap: {state:?}"));
                        }
                    }
                }
                self.answer = Some(if self.serving {
                    AnswerCarry::Captured(CaptureEnvelope {
                        exchange: message.envelope.exchange.clone(),
                        position: Position::Close,
                        payload: self.refusal.clone().map_or_else(
                            || {
                                CapturePayload::Answer(CaptureAnswer::Left {
                                    save_point: self.capture.clone(),
                                    forced: self.forced,
                                    no_state: false,
                                })
                            },
                            |refusal| CapturePayload::Refusal(CaptureRefusal::Lifecycle(refusal)),
                        ),
                    })
                } else {
                    AnswerCarry::Rollback(OrganEnvelope {
                        exchange: message.envelope.exchange.clone(),
                        position: Position::Close,
                        payload: self.refusal.clone().map_or_else(
                            || {
                                Payload::Answer(LifecycleAnswer::Left {
                                    save_point: None,
                                    forced: self.forced,
                                    no_state: false,
                                })
                            },
                            Payload::Refusal,
                        ),
                    })
                });
                self.stage = Stage::Answer;
            }
            Stage::Answer => {
                if self.send.is_none() {
                    match self.answer.as_ref().unwrap().clone().stage(peer) {
                        Ok(TrackedStage::Queued(send)) => self.send = Some(send),
                        Ok(TrackedStage::Busy(answer)) => {
                            self._returned_answer = Some(answer);
                            return Err("entered rollback answer has a competing sender".into());
                        }
                        Err((input, failure)) => {
                            self._returned_answer = Some(input);
                            self._answer_failure = Some(failure);
                            self.stage = Stage::Complete;
                            return Ok(true);
                        }
                    }
                }
                if let Err(failure) = peer.try_send_observed(io) {
                    let unclocked = self
                        .send
                        .as_ref()
                        .is_some_and(|send| send.sent() && send.sent_at().is_none());
                    let detail = format!("entered rollback answer send: {failure:?}");
                    self._answer_failure = Some(failure);
                    if unclocked {
                        return Err(detail);
                    }
                    // The accepted directive still owns completed cleanup even
                    // when its peer cannot receive the answer. No Sent is made.
                    self.stage = Stage::Complete;
                    return Ok(true);
                }
                let send = self.send.as_ref().unwrap();
                if send.sent() {
                    if !peer.owns_send(send)
                        || !send.matches_exchange(&message.envelope.exchange)
                        || !send
                            .sent_at()
                            .is_some_and(|at| at >= leave.began_ns() && !leave.expired(at))
                    {
                        return Err(
                            "entered rollback answer lacks its timely whole-send witness".into(),
                        );
                    }
                    self.stage = Stage::Complete;
                } else {
                    io.wait(
                        &[WaitInterest {
                            fd: peer.fd(),
                            events: PollFlags::POLLOUT
                                | PollFlags::POLLHUP
                                | PollFlags::POLLERR
                                | PollFlags::POLLNVAL,
                        }],
                        Some(leave.deadline_ns()),
                    )
                    .map_err(|error| format!("entered rollback answer wait: {error}"))?;
                }
            }
            Stage::Complete => (),
        }
        Ok(self.complete())
    }
}
