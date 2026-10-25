//! Shared terminal for explicit rollback. An unactivated Load retains its
//! unused Open; admitted-organ cleanup supplies actual Grants and release facts.
use super::*;
use crate::flow::context::startup_composition::refusal::response::listener::{
    RetiredRollbackDials, RollbackDials,
};
use crate::flow::context::*;
use crate::flow::coordinator::FlowRecording;
use weaver_trace::{GrantSurface, MemberRelease, ReleaseOutcome, Sequence, Subsystem};
use weaver_types::Payload as WirePayload;

pub(in crate::flow) struct ExpiredPreflightPermit {
    barrier: Arc<()>,
}
impl ExpiredPreflightPermit {
    pub(in crate::flow) fn matches_record(&self, barrier: &Arc<()>) -> bool {
        Arc::ptr_eq(&self.barrier, barrier)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    Retire,
    Seal,
    Unload,
    Drain,
    Children,
    Answer,
    Complete,
}
pub(super) struct ExpiredPreflightRollback {
    permit: Option<ExpiredPreflightPermit>,
    stage: Stage,
    dials: Option<RollbackDials>,
    retired_dials: Option<RetiredRollbackDials>,
    unload: Option<AuthoredEvent>,
    started: bool,
    returned: StoppedAuthor,
    sequence: Option<Sequence>,
    answer_left: bool,
}
impl ExpiredPreflightRollback {
    /// The same terminal follows actual retired classifier/state custody. The
    /// caller retains both measured Grants readings in its preceding owner.
    pub(super) fn released(
        owner: &mut FoundationExpiredRollback,
        dials: RollbackDials,
        surface: GrantSurface,
        spu: Option<ReleaseOutcome>,
        account_complete: bool,
    ) -> Self {
        let context = &owner.run.context;
        assert_eq!(context.phase, RecordingPhase::Releasing);
        assert!(context.failure.is_none() && context.flow.halt().is_none());
        assert!(context.flow.classifier_retired());
        assert!(context.flow.standing_state().is_none());
        assert_eq!(owner.run.resources.spu_child().is_some(), spu.is_some());
        assert!(matches!(
            owner.run.resources.classifier_child(),
            Some(crate::flow::context::failed_enter_driver::ChildState::Reaped(_))
        ));
        let cause = owner.original.as_ref().unwrap().payload().cause;
        Self {
            permit: None,
            stage: Stage::Unload,
            dials: Some(dials),
            retired_dials: None,
            unload: Some(unload(cause, surface, spu)),
            started: false,
            returned: StoppedAuthor::default(),
            sequence: None,
            answer_left: account_complete,
        }
    }
    pub(super) fn complete(&self) -> bool {
        self.stage == Stage::Complete
    }
    pub(super) fn sequence(&self) -> Option<u64> {
        self.sequence.map(|sequence| sequence.0)
    }
    pub(super) fn retire_dials(&mut self) {
        if let Some(dials) = self.dials.take() {
            self.retired_dials = Some(dials.retire_failed());
        }
    }
}

impl FoundationExpiredRollback {
    fn preflight_rollback(&mut self) -> Option<ExpiredPreflightRollback> {
        let context = &self.run.context;
        let leave = self.leave?;
        if self.before_load
            || leave.expired(context.last_ns)
            || context.phase != RecordingPhase::Entered
            || context.failure.is_some()
            || context.flow.halt().is_some()
            || context.flow.phase() != CoordinatorPhase::Preflight
            || !context.barrier.is_loaded()
            || context.barrier.is_active()
            || context.barrier.is_refused()
            || context.deadlines.get(PumpDeadline::Enter) != Some(self.handoff.identity.enter)
            || context.deadlines.get(PumpDeadline::Leave) != Some(leave)
            || self.run.resources.spu_child().is_some()
            || self.run.resources.classifier_child().is_some()
            || !matches!(self.handoff.held.as_ref(), HeldStartup::Initial(account) if account.load_sequence.is_some())
        {
            return None;
        }
        let cause = self.original.as_ref()?.payload().cause;
        Some(ExpiredPreflightRollback {
            permit: Some(ExpiredPreflightPermit {
                barrier: context.barrier.author_identity(),
            }),
            stage: Stage::Retire,
            dials: Some(RollbackDials::new(self.handoff.listener.take()?)),
            retired_dials: None,
            // No initial semantic reading ever stood on this road. The
            // retained unused Open is not a fabricated Grants read.
            unload: Some(unload(cause, GrantSurface::Unreadable, None)),
            started: false,
            returned: StoppedAuthor::default(),
            sequence: None,
            answer_left: true,
        })
    }

    pub(super) fn advance_preflight(&mut self, io: &mut impl PumpIo) -> Result<(), String> {
        if self.preflight.is_none() && self.active.is_none() {
            self.preflight = self.preflight_rollback();
        }
        let Some(mut preflight) = self.preflight.take() else {
            return self.advance_active(io);
        };
        let result = preflight.step(self, io);
        self.preflight = Some(preflight);
        result
    }
}

fn unload(
    cause: weaver_types::Cause,
    surface: GrantSurface,
    spu: Option<ReleaseOutcome>,
) -> AuthoredEvent {
    AuthoredEvent {
        kind: Kind::Unload,
        subsystem: Subsystem::Harness,
        turn: None,
        source: crate::flow::author::AuthoredSource::Runtime,
        causal_parent: None,
        payload: Some(serving::Payload::Unload(Box::new(
            serving::payloads::UnloadClose {
                grant_surface: Some(surface),
                cause: Some(weaver_trace::Cause { uid: cause.uid }),
                forced: true,
                forced_by: None,
                release: serving::payloads::Release {
                    spu,
                    member: Some(MemberRelease::Closed),
                },
                end_reason: None,
                state_flow_failure: None,
            },
        ))),
    }
}

impl ExpiredPreflightRollback {
    fn step(
        &mut self,
        owner: &mut FoundationExpiredRollback,
        io: &mut impl PumpIo,
    ) -> Result<(), String> {
        let context = &mut owner.run.context;
        let leave = owner.leave.unwrap();
        match self.stage {
            Stage::Retire => {
                let retired = context
                    .flow
                    .begin_expired_preflight_rollback(
                        &mut FlowRecording {
                            barrier: &mut context.barrier,
                            inbox: &mut context.inbox,
                            budget: &mut context.budget,
                            author: &context.author,
                        },
                        self.permit.as_ref().expect("unactivated retirement permit"),
                    )
                    .map_err(|failure| {
                        let failure = context.flow_failed(io, failure);
                        format!("expired preflight disposal: {failure:?}")
                    })?;
                if retired == Advance::Progress {
                    if context.complete_wait(PumpDeadline::Enter)
                        != Some(owner.handoff.identity.enter)
                    {
                        return Err("expired preflight lost its original Enter".into());
                    }
                    context.phase = RecordingPhase::Releasing;
                    self.stage = Stage::Seal;
                }
            }
            Stage::Seal => {
                if self
                    .dials
                    .as_mut()
                    .unwrap()
                    .seal_step(context, leave, io)
                    .map_err(|error| format!("expired preflight seal: {error:?}"))?
                {
                    self.stage = Stage::Unload;
                }
            }
            Stage::Unload => {
                self.returned
                    .check()
                    .map_err(|error| format!("expired preflight Unload: {error:?}"))?;
                let mut ports = crate::flow::pump::PumpPorts {
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
                self.sequence = result
                    .as_ref()
                    .ok()
                    .copied()
                    .or_else(|| context.release_recorded_sequence());
                match result {
                    Ok(_) => self.stage = Stage::Drain,
                    Err(error) => {
                        return Err(format!(
                            "expired preflight Unload: {:?}",
                            self.returned.retain_started(error, &mut self.started)
                        ));
                    }
                }
            }
            Stage::Drain => {
                context
                    .close_after_drain(&mut crate::flow::pump::PumpPorts {
                        lifecycle: LifecyclePorts::default(),
                        gate: None,
                        decode: None,
                        label: None,
                        io,
                    })
                    .map_err(|error| format!("expired preflight drain: {error:?}"))?;
                self.stage = Stage::Children;
            }
            Stage::Children => {
                use crate::flow::context::failed_enter_driver::ChildState;
                for child in [
                    owner.run.resources.poll_classifier_child(),
                    owner.run.resources.poll_spu_child(),
                ] {
                    match child {
                        None | Some(ChildState::Reaped(_)) => {}
                        Some(ChildState::Running) => {
                            let wake = context
                                .last_ns
                                .saturating_add(10_000_000)
                                .min(leave.deadline_ns());
                            io.wait(&[], Some(wake))
                                .map_err(|error| format!("expired rollback child wait: {error}"))?;
                            return Ok(());
                        }
                        Some(state) => return Err(format!("expired rollback child: {state:?}")),
                    }
                }
                if !self.answer_left {
                    // HARNESS-OPEN forbids Left after incomplete accounting,
                    // even when release and the actual terminal drain finish.
                    self.finish(owner);
                    return Ok(());
                }
                let dial = owner.dial.as_mut().unwrap();
                dial.answer = Some(OrganEnvelope {
                    exchange: dial.message.as_ref().unwrap().envelope.exchange.clone(),
                    position: Position::Close,
                    payload: WirePayload::Answer(LifecycleAnswer::Left {
                        save_point: None,
                        forced: true,
                        no_state: false,
                    }),
                });
                self.stage = Stage::Answer;
            }
            Stage::Answer => {
                let dial = owner.dial.as_mut().unwrap();
                if let Some(answer) = dial.answer.take() {
                    match dial.peer.stage_tracked(answer) {
                        Ok(TrackedStage::Queued(send)) => dial.send = Some(send),
                        Ok(TrackedStage::Busy(answer)) => {
                            dial.answer = Some(answer);
                            return Err("expired preflight Left has a competing sender".into());
                        }
                        Err(error) => {
                            dial.answer = Some(error.input);
                            return Err(format!(
                                "expired preflight Left stage: {:?}",
                                error.failure
                            ));
                        }
                    }
                }
                dial.peer
                    .try_send_observed(io)
                    .map_err(|error| format!("expired preflight Left send: {error:?}"))?;
                if let Some(send) = &dial.send
                    && send.sent()
                {
                    if !dial.peer.owns_send(send)
                        || !send.matches_exchange(&dial.message.as_ref().unwrap().envelope.exchange)
                        || !send
                            .sent_at()
                            .is_some_and(|at| at >= leave.began_ns() && !leave.expired(at))
                    {
                        return Err(
                            "expired preflight Left lacks its timely whole-send witness".into()
                        );
                    }
                    self.finish(owner);
                } else {
                    io.wait(
                        &[WaitInterest {
                            fd: dial.peer.fd(),
                            events: PollFlags::POLLOUT,
                        }],
                        Some(leave.deadline_ns()),
                    )
                    .map_err(|error| format!("expired preflight Left wait: {error}"))?;
                }
            }
            Stage::Complete => (),
        }
        Ok(())
    }

    fn finish(&mut self, owner: &mut FoundationExpiredRollback) {
        self.retire_dials();
        let dial = owner.dial.take().unwrap();
        owner.retired_dial = Some(RetiredDial {
            _peer: dial.peer.retire_failed(),
            _read: dial.read,
            _message: dial.message,
            _answer: dial.answer,
            _send: dial.send,
        });
        self.stage = Stage::Complete;
    }
}
