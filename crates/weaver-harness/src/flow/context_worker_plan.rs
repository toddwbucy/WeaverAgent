//! The accepted root peer survives independently of secondary management.
//! Its complete account stays with this serving run, including lost delivery.
use super::*;
use crate::flow::clock::AbsoluteBound;
use crate::flow::context::RecordingContext;
use crate::flow::context::plan_update_driver::{RootPlanProgress, settlement::RootPlanCompletion};
use crate::flow::pump::{PumpDeadline, RetiredLifecyclePeer};
use crate::flow::templates::TemplateCertificate;
use weaver_types::ExchangeId;

#[cfg(test)]
#[path = "context_worker_plan_tests.rs"]
pub(in crate::flow::context::worker_startup) mod tests;

pub(in crate::flow::context::worker_startup) struct PendingPlan {
    peer: Option<LifecyclePeer>,
    retired: Option<RetiredLifecyclePeer>,
    exchange: ExchangeId,
    bound: AbsoluteBound,
    completion: Option<RootPlanCompletion>,
    answer: Option<OrganEnvelope>,
    send: Option<OrganSend>,
    done: bool,
}

impl PendingPlan {
    pub(super) fn new(peer: LifecyclePeer, exchange: ExchangeId, bound: AbsoluteBound) -> Self {
        Self {
            peer: Some(peer),
            retired: None,
            exchange,
            bound,
            completion: None,
            answer: None,
            send: None,
            done: false,
        }
    }
    pub(in crate::flow::context::worker_startup) fn active(&self) -> bool {
        !self.done
    }
    pub(super) fn port(&mut self) -> Option<&mut LifecyclePeer> {
        self.peer.as_mut()
    }
    fn retire_peer(&mut self) {
        if let Some(peer) = self.peer.take() {
            self.retired = Some(peer.retire_failed());
        }
    }
    pub(in crate::flow::context::worker_startup) fn retire_failed(
        mut self: Box<Self>,
    ) -> Box<Self> {
        self.retire_peer();
        self
    }
    fn send_clock_failed(&self) -> bool {
        self.send
            .as_ref()
            .is_some_and(|send| send.sent() && send.sent_at().is_none())
    }
    fn retain_interrupt(&mut self, context: &mut RecordingContext, failure: &RunFailure) -> bool {
        if context.failure().is_some() || self.send_clock_failed() {
            return false;
        }
        match failure {
            RunFailure::Transport {
                source: PumpSource::LeavePeer,
                ..
            } if self.peer.is_some() => {
                // A complete accepted request continues after EOF. This only
                // retires delivery; the context still owns its actual work.
                self.retire_peer();
                true
            }
            RunFailure::Wait {
                key: PumpDeadline::PlanAnswer,
                bound,
            } if self.completion.is_some() && *bound == self.bound => {
                context.complete_wait(PumpDeadline::PlanAnswer);
                self.retire_peer();
                self.done = true;
                true
            }
            _ => false,
        }
    }
    fn finish_delivery(&mut self, context: &mut RecordingContext) {
        context.complete_wait(PumpDeadline::PlanAnswer);
        self.retire_peer();
        self.done = true;
    }
    fn send_answer(
        &mut self,
        context: &mut RecordingContext,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        context.live(io)?;
        if self.peer.is_none() || matches!(self.completion, Some(RootPlanCompletion::Cancelled(_)))
        {
            self.finish_delivery(context);
            return Ok(());
        }
        if self.bound.expired(context.last_ns) {
            self.finish_delivery(context);
            return Ok(());
        }
        if context.deadlines.get(PumpDeadline::PlanAnswer).is_none() {
            context.arm(PumpDeadline::PlanAnswer, self.bound, io)?;
        }
        if self.answer.is_none() && self.send.is_none() {
            let payload = match self.completion.as_ref().expect("settled root account") {
                RootPlanCompletion::Updated(updated) => Payload::Answer(updated.answer()),
                RootPlanCompletion::Refused { refusal, .. } => Payload::Refusal(refusal.clone()),
                RootPlanCompletion::Cancelled(_) => unreachable!("cancelled delivery retired"),
            };
            self.answer = Some(OrganEnvelope {
                exchange: self.exchange.clone(),
                position: Position::Close,
                payload,
            });
        }
        let peer = self.peer.as_mut().unwrap();
        if let Some(answer) = self.answer.take() {
            match peer.stage_tracked(answer) {
                Ok(TrackedStage::Queued(send)) => self.send = Some(send),
                Ok(TrackedStage::Busy(answer)) => self.answer = Some(answer),
                Err(error) => {
                    self.answer = Some(error.input);
                    return Err(RunFailure::Transport {
                        source: PumpSource::LeavePeer,
                        failure: error.failure,
                    });
                }
            }
        }
        peer.try_send_observed(io)
            .map_err(|failure| RunFailure::Transport {
                source: PumpSource::LeavePeer,
                failure,
            })?;
        if let Some(send) = self.send.as_ref().filter(|send| send.sent()) {
            if !peer.owns_send(send)
                || !send.matches_exchange(&self.exchange)
                || send.sent_at().is_none()
            {
                return Err(context.fail(
                    io,
                    FlowHaltReason::TraceFailed,
                    HaltPoint::Author,
                    "root answer lost its send identity or clock",
                ));
            }
            self.finish_delivery(context);
        }
        Ok(())
    }

    fn advance<I: PumpIo>(
        &mut self,
        context: &mut RecordingContext,
        certificate: &TemplateCertificate,
        ports: PumpPorts<'_, I>,
    ) -> Result<(), RunFailure> {
        if self.done {
            return Ok(());
        }
        let PumpPorts {
            lifecycle,
            gate,
            decode,
            label,
            io,
        } = ports;
        let result = if self.completion.is_some() {
            (|| {
                self.send_answer(context, io)?;
                if !self.done {
                    context.pump_once(&mut PumpPorts {
                        lifecycle: LifecyclePorts {
                            leave_peer: self.peer.as_mut(),
                            ..lifecycle
                        },
                        gate,
                        decode,
                        label,
                        io: &mut *io,
                    })?;
                }
                Ok(())
            })()
        } else {
            let mut ports = PumpPorts {
                lifecycle: LifecyclePorts {
                    leave_peer: self.peer.as_mut(),
                    ..lifecycle
                },
                gate,
                decode,
                label,
                io: &mut *io,
            };
            (|| {
                loop {
                    let progress = context.advance_root_plan_update(&mut ports)?;
                    match progress {
                        RootPlanProgress::EffectReady => {
                            context.begin_root_plan_effect(certificate, ports.io)?;
                        }
                        RootPlanProgress::RefusalReady
                        | RootPlanProgress::Cancelled
                        | RootPlanProgress::SummaryReady
                        | RootPlanProgress::Complete => {
                            if context.settle_root_plan_update(&mut ports)?
                                == RootPlanProgress::Complete
                            {
                                self.completion = context.take_root_plan_update(ports.io)?;
                                return Ok(());
                            }
                            context.pump_once(&mut ports)?;
                            return Ok(());
                        }
                        RootPlanProgress::Pending if context.root_plan_wait_ready() => {
                            context.pump_once(&mut ports)?;
                            return Ok(());
                        }
                        _ => (),
                    }
                }
            })()
        };
        if let Err(failure) = result {
            if self.send_clock_failed() {
                return Err(context.fail(
                    io,
                    FlowHaltReason::TraceFailed,
                    HaltPoint::Author,
                    format!("root answer send clock failed: {failure:?}"),
                ));
            }
            if !self.retain_interrupt(context, &failure) {
                return Err(failure);
            }
        }
        Ok(())
    }
}

pub(super) fn retained_port<'a>(
    plan: &'a mut Option<Box<PendingPlan>>,
    save: &'a mut Option<Box<save::PendingSave>>,
) -> Option<&'a mut LifecyclePeer> {
    if plan.as_ref().is_some_and(|plan| plan.active()) {
        plan.as_mut().unwrap().port()
    } else {
        save.as_mut().and_then(|save| save.port())
    }
}

impl FoundationInput<'_> {
    pub(super) fn plan_pending(&self) -> bool {
        self.serving.plan.as_ref().is_some_and(|plan| plan.active())
    }
    /// State-owner work excludes new input, effects and capture boundaries.
    /// Completed untaken results retain custody but have no pending I/O duty.
    /// An owed next_ready observation is not state-owner work: it waits for an
    /// authorship window the input's placement and generation may hold, so it
    /// must not exclude the input's progress. The sites it must block name
    /// next_ready_pending themselves.
    pub(super) fn plan_work_pending(&self) -> bool {
        self.plan_pending() || self.flags_pending() || self.plan_operation_pending()
    }
    pub(super) fn plan_at_rest(&self) -> bool {
        self.stage == Stage::Waiting
            && self.received.is_none()
            && self.token.is_none()
            && self.report.is_none()
            && !self.plan_work_pending()
            && !self.next_ready_pending()
            && self.serving.save.is_none()
            && !self.has_graceful_leave()
    }
    pub(super) fn retain_plan_interrupt(&mut self, failure: &RunFailure) -> bool {
        self.serving
            .plan
            .as_mut()
            .is_some_and(|plan| plan.retain_interrupt(self.serving.owner.context, failure))
    }
    pub(super) fn advance_plan(
        &mut self,
        peer: Option<&mut LifecyclePeer>,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        let Some(plan) = self.serving.plan.as_mut().filter(|plan| plan.active()) else {
            return Ok(());
        };
        let owner = &mut self.serving.owner;
        let Some(certificate) = owner.certificate.as_ref() else {
            return Err(owner.context.fail(
                io,
                FlowHaltReason::ProtocolFailed,
                HaltPoint::Barrier,
                "root update lost its loaded template certificate",
            ));
        };
        plan.advance(
            owner.context,
            certificate,
            PumpPorts {
                lifecycle: LifecyclePorts {
                    listener: Some(&mut owner.listener),
                    dial: peer,
                    residency: Some(owner.resident.residency),
                    ..Default::default()
                },
                gate: Some(&mut owner.gate),
                decode: Some(owner.decode),
                label: None,
                io,
            },
        )
    }
}
