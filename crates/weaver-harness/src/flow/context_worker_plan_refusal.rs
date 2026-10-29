//! A rejected root opener owns its trace before delivery, beside the suspended
//! operation. Neither pressure nor EOF can admit the rejected plan.
use super::*;
use crate::flow::context::RecordingContext;
use crate::flow::context::serving_observation::ServingObservationAuthor;
use std::sync::Arc;
use weaver_types::{ExchangeId, LifecycleAsk, LifecycleRefusal, RefusalRecord};

#[cfg(test)]
#[path = "context_worker_plan_refusal_tests.rs"]
pub(in crate::flow::context::worker_startup) mod tests;

#[cfg(test)]
#[path = "context_worker_plan_refusal_composition_tests.rs"]
pub(in crate::flow::context::worker_startup) mod composition_tests;

pub(super) struct PlanRefusal {
    author: Arc<ServingObservationAuthor>,
    original: Option<AuthoredEvent>,
    returned: StoppedAuthor,
    started: bool,
    sequence: Option<weaver_trace::Sequence>,
    bound: AbsoluteBound,
    expired: bool,
}

impl PlanRefusal {
    fn new(
        context: &mut RecordingContext,
        refusal: &LifecycleRefusal,
        heard_ns: u64,
        io: &mut impl PumpIo,
    ) -> Result<Self, RunFailure> {
        let elapsed = context
            .load_duty
            .as_ref()
            .expect("serving retains its loaded decision bound")
            .instruction
            .decision_wait
            .elapsed_ns
            .0;
        let bound = AbsoluteBound::after(heard_ns, elapsed).map_err(|error| {
            context.fail(
                io,
                FlowHaltReason::ProtocolFailed,
                HaltPoint::Author,
                error.to_string(),
            )
        })?;
        Ok(Self {
            author: Arc::new(ServingObservationAuthor::plan_refusal(context, refusal)),
            original: Some(AuthoredEvent {
                kind: serving::Kind::Refusal,
                subsystem: Subsystem::Harness,
                turn: None,
                source: AuthoredSource::Runtime,
                causal_parent: None,
                payload: Some(serving::Payload::Refusal(
                    serde_json::value::to_raw_value(&RefusalRecord::Lifecycle {
                        asked: LifecycleAsk::PlanUpdate,
                        refusal: refusal.clone(),
                    })
                    .expect("typed plan refusal serializes"),
                )),
            }),
            returned: StoppedAuthor::default(),
            started: false,
            sequence: None,
            bound,
            expired: false,
        })
    }

    pub(super) fn complete(&self) -> bool {
        self.sequence.is_some()
    }

    fn advance<I: PumpIo>(
        &mut self,
        context: &mut RecordingContext,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<bool, RunFailure> {
        if !self.expired {
            match context.deadlines.get(PumpDeadline::PlanRefusal) {
                None => context.arm(PumpDeadline::PlanRefusal, self.bound, ports.io)?,
                Some(bound) if bound == self.bound => (),
                Some(_) => {
                    return Err(context.fail(
                        ports.io,
                        FlowHaltReason::ProtocolFailed,
                        HaltPoint::Author,
                        "root admission refusal found a foreign interval",
                    ));
                }
            }
        }
        if self.complete() {
            return Ok(true);
        }
        self.returned.check()?;
        if !self.started && !context.serving_observation_author_available() {
            return Ok(false);
        }
        let result = if self.started {
            context.resume_serving_observation(ports, &self.author)
        } else {
            self.started = true;
            context.author_serving_observation(ports, &self.author, self.original.take().unwrap())
        };
        match result {
            Ok(sequence) => self.sequence = Some(sequence),
            Err(mut error)
                if error.original.is_some()
                    && error.held.is_none()
                    && self.author.ticket().is_none()
                    && context.failure().is_none()
                    && matches!(
                        error.reason,
                        RunFailure::Wait { .. }
                            | RunFailure::Busy(_)
                            | RunFailure::Transport {
                                source: PumpSource::AdminDial,
                                ..
                            }
                    ) =>
            {
                // Preflight returned this still-unaccepted original. Its owner
                // keeps it while the interrupt's owner settles the earlier duty.
                // A recorder ticket or a fatal failure never takes this route.
                self.original = error.original.take();
                self.started = false;
                return Err(error.reason);
            }
            Err(error) => return Err(self.returned.retain_started(error, &mut self.started)),
        }
        Ok(true)
    }
}

impl FoundationManagement<'_> {
    pub(super) fn refuse_plan(
        &mut self,
        exchange: ExchangeId,
        refusal: LifecycleRefusal,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        self.answer = Some(ManagementAnswer::Lifecycle(OrganEnvelope {
            exchange,
            position: Position::Close,
            payload: Payload::Refusal(refusal.clone()),
        }));
        self.plan_refusal = Some(PlanRefusal::new(
            self.input.serving.owner.context,
            &refusal,
            self.heard_ns
                .expect("judged root opener retains its receive reading"),
            io,
        )?);
        // Install the interval before another organ or prior owner can wait.
        // A refused packet and its chosen answer are already retained if this fails.
        self.input.serving.owner.context.arm(
            PumpDeadline::PlanRefusal,
            self.plan_refusal.as_ref().unwrap().bound,
            io,
        )
    }

    pub(super) fn advance_plan_refusal(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<bool, RunFailure> {
        let Some(record) = &mut self.plan_refusal else {
            return Ok(true);
        };
        let owner = &mut self.input.serving.owner;
        let result = record.advance(
            owner.context,
            &mut PumpPorts {
                lifecycle: LifecyclePorts {
                    listener: Some(&mut owner.listener),
                    dial: (!self.peer_unavailable).then_some(&mut self.peer),
                    residency: Some(owner.resident.residency),
                    leave_peer: super::super::plan::retained_port(
                        &mut self.input.serving.plan,
                        &mut self.input.serving.save,
                    ),
                    ..Default::default()
                },
                gate: Some(&mut owner.gate),
                decode: Some(owner.decode),
                label: None,
                io,
            },
        );
        match result {
            Ok(true) => Ok(true),
            Ok(false) => {
                // Run only the earlier owner while its original blocks the slot.
                // Its own continuation consumes its receipt; this refusal never
                // resumes or takes another caller's author ticket.
                let peer = (!self.peer_unavailable).then_some(&mut self.peer);
                if self.input.flags_pending() {
                    self.input.advance_flags(peer, io)?;
                } else if self.input.plan_pending() {
                    self.input.advance_plan(peer, io)?;
                } else if self.input.serving.save.is_some() {
                    self.input.advance_save(io, false)?;
                } else if matches!(
                    self.input.stage,
                    Stage::Checking | Stage::Source | Stage::Engine
                ) {
                    self.input.step_with_peer(peer, io)?;
                }
                Ok(false)
            }
            Err(error) => {
                self.keep_dial_failure::<()>(Err(error))?;
                Ok(false)
            }
        }
    }

    pub(super) fn expire_plan_refusal(
        &mut self,
        failure: &RunFailure,
        io: &mut impl PumpIo,
    ) -> Result<bool, RunFailure> {
        let Some(record) = &mut self.plan_refusal else {
            return Ok(false);
        };
        if !matches!(failure, RunFailure::Wait { key: PumpDeadline::PlanRefusal, bound } if *bound == record.bound)
        {
            return Ok(false);
        }
        let context = &mut self.input.serving.owner.context;
        if record.author.sequence().is_none() {
            context.failed_wait = Some(crate::flow::context::WaitAtHalt {
                stage: serving::payloads::BlockedStage::Decision,
                began_ns: record.bound.began_ns().into(),
                deadline_ns: record.bound.deadline_ns().into(),
            });
            return Err(context.fail(
                io,
                FlowHaltReason::DecisionBound,
                HaltPoint::Author,
                "root admission refusal was not Recorded within its original decision bound",
            ));
        }
        // A recorded ticket still belongs to this exact claim until consumed.
        // Retire delivery, then let that claim finish without a renewed window.
        context.complete_wait(PumpDeadline::PlanRefusal);
        record.expired = true;
        if record.complete()
            && self.send.as_ref().is_some_and(|send| {
                self.peer.owns_send(send)
                    && self
                        .declined_plan
                        .as_ref()
                        .is_some_and(|packet| send.matches_exchange(&packet.envelope().exchange))
                    && send
                        .sent_at()
                        .is_some_and(|at| at < record.bound.deadline_ns())
            })
        {
            // Delivery already completed inside this interval. A later reducer
            // wake cannot turn that observed delivery into a dropped answer.
            self.progress = FoundationManagementProgress::Answered;
            return Ok(true);
        }
        self.peer_unavailable = true;
        Ok(true)
    }
}
