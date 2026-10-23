//! Final aggregate Enter answer on the original connection, with all arms held.
//! This is a lifecycle handoff; workflow selection belongs to worker/dev_loop.
use super::*;
use crate::flow::context::failed_enter_driver::{ChildState, ResidentBootResources};
use crate::flow::templates::TemplateCertificate;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum ReadyStage {
    Sending,
    Complete,
    Stopped,
}
#[derive(Debug)]
pub(in crate::flow) enum ReadyStop {
    Run(RunFailure),
    Interrupted,
    ChildStopped,
    Busy,
    Stage(crate::flow::pump_transport::Failure),
    InvalidSend,
    Retirement,
}

pub(in crate::flow) struct EnterReady<'a> {
    raised: RaisedBoot<'a>,
    answer: OrganEnvelope,
    enter: AbsoluteBound,
    stage: ReadyStage,
    send: Option<OrganSend>,
    returned: Option<OrganEnvelope>,
    stop: Option<ReadyStop>,
}

/// The consumed successful handoff keeps every owned transport and evidence item.
/// The original admin peer may close normally after receiving its Ready answer.
pub(in crate::flow) struct EnteredServing<'a> {
    pub context: &'a mut RecordingContext,
    pub decode: &'a mut DecodePort,
    pub(in crate::flow::context) resident: ResidentBootResources<'a>,
    pub certificate: TemplateCertificate,
    pub listener: LifecycleListener,
    pub gate: OrganPort,
    pub(in crate::flow::context) gate_child: OwnedChild,
    pub tools: ToolsCgroup,
    pub gate_request: GateStartupRequest,
    pub gate_raise: OrganEnvelope,
    pub gate_send: OrganSend,
    pub gate_reply: ReceivedOrgan,
    pub ready: OrganSend,
    pub answer: OrganEnvelope,
    pub enter: AbsoluteBound,
}

impl<'a> RaisedBoot<'a> {
    /// Primitive-only boot/gate fixtures cannot mint aggregate readiness. Only
    /// the composed original Enter plus actual resident arms can enter this owner.
    #[allow(clippy::result_large_err)]
    pub(in crate::flow) fn into_ready(self) -> Result<EnterReady<'a>, Self> {
        let owner = &self.owner;
        let Some(resident) = owner.boot.resident.as_ref() else {
            return Err(self);
        };
        let Some(source) = resident.evidence.startup.original_enter.as_ref() else {
            return Err(self);
        };
        let Some(enter) = owner.enter else {
            return Err(self);
        };
        if owner.stage != GateStartupStage::Complete
            || owner.stop.is_some()
            || owner.listener.held().is_some()
            || !source.adopted_by(owner.boot.context)
            || owner.boot.context.phase != RecordingPhase::Entered
            || owner.boot.context.deadlines.get(PumpDeadline::Enter) != Some(enter)
            || resident.evidence.enter != enter
            || !resident.evidence.reply.belongs_to(resident.residency)
            || !resident.request.send().sent()
            || owner.child.is_none()
            || owner.tools.is_none()
            || !owner.reply.as_ref().is_some_and(|reply| {
                owner
                    .port
                    .as_ref()
                    .is_some_and(|port| reply.belongs_to(port))
                    && matches!(
                        reply.envelope().payload,
                        weaver_types::Payload::Answer(LifecycleAnswer::Ready)
                    )
            })
        {
            return Err(self);
        }
        let answer = source.ready_envelope();
        Ok(EnterReady {
            raised: self,
            answer,
            enter,
            stage: ReadyStage::Sending,
            send: None,
            returned: None,
            stop: None,
        })
    }
}
impl EnterReady<'_> {
    pub(in crate::flow) fn stage(&self) -> ReadyStage {
        self.stage
    }
    pub(in crate::flow) fn stop(&self) -> Option<&ReadyStop> {
        self.stop.as_ref()
    }
    pub(in crate::flow) fn send(&self) -> Option<&OrganSend> {
        self.send.as_ref()
    }
    fn stopped(&mut self, cause: ReadyStop) -> ReadyStage {
        self.stop = Some(cause);
        self.stage = ReadyStage::Stopped;
        self.stage
    }
    fn check(&mut self, io: &mut impl PumpIo) -> Result<(), ReadyStop> {
        let owner = &mut self.raised.owner;
        let context = &mut owner.boot.context;
        if let Some(failure) = context.failure().cloned() {
            return Err(ReadyStop::Run(failure));
        }
        if context.phase != RecordingPhase::Entered
            || context.deadlines.get(PumpDeadline::Enter) != Some(self.enter)
            || context.place_engine.is_none()
            || context.committed_plan_summary().is_err()
        {
            return Err(ReadyStop::InvalidSend);
        }
        context.live(io).map_err(ReadyStop::Run)?;
        let resident = owner
            .boot
            .resident
            .as_mut()
            .expect("composed resident arms");
        let source = resident
            .evidence
            .startup
            .original_enter
            .as_mut()
            .expect("original Enter");
        let ports = LifecyclePorts {
            peer: Some(source.peer_mut()),
            ..Default::default()
        };
        context.observe_boot_lifecycle(&ports);
        context.observe_place_lifecycle(&ports);
        if context.boot_enter_cancelled || context.place_admission_stopped() {
            return Err(ReadyStop::Interrupted);
        }
        if resident.spu.poll() != ChildState::Running
            || resident.classifier.poll() != ChildState::Running
            || owner.child.as_mut().unwrap().poll() != ChildState::Running
        {
            return Err(ReadyStop::ChildStopped);
        }
        Ok(())
    }
    pub(in crate::flow) fn advance(&mut self, io: &mut impl PumpIo) -> ReadyStage {
        if self.stage != ReadyStage::Sending {
            return self.stage;
        }
        if let Err(stop) = self.check(io) {
            return self.stopped(stop);
        }
        let owner = &mut self.raised.owner;
        let resident = owner
            .boot
            .resident
            .as_mut()
            .expect("composed resident arms");
        let source = resident
            .evidence
            .startup
            .original_enter
            .as_mut()
            .expect("original Enter");
        if self.send.is_none() {
            match source.peer_mut().stage_enter_ready(self.answer.clone()) {
                Ok(TrackedStage::Queued(send)) => self.send = Some(send),
                Ok(TrackedStage::Busy(original)) => {
                    self.returned = Some(original);
                    return self.stopped(ReadyStop::Busy);
                }
                Err(error) => {
                    self.returned = Some(error.input);
                    return self.stopped(ReadyStop::Stage(error.failure));
                }
            }
        }
        let result = owner.boot.context.pump_once(&mut PumpPorts {
            lifecycle: LifecyclePorts {
                listener: Some(&mut owner.listener),
                peer: Some(source.peer_mut()),
                residency: Some(&mut *resident.residency),
                ..Default::default()
            },
            gate: owner.port.as_mut(),
            decode: Some(owner.boot.decode),
            label: None,
            io,
        });
        if let Err(failure) = result {
            // Whole-send evidence precedes any fallible clock observation.
            // A failed observation grants no retry, even when bytes already left.
            if self
                .send
                .as_ref()
                .is_some_and(|send| send.sent() && send.sent_at().is_none())
            {
                owner.boot.context.fail(
                    io,
                    FlowHaltReason::TraceFailed,
                    HaltPoint::Author,
                    format!("Ready send clock observation failed: {failure:?}"),
                );
            }
            return self.stopped(ReadyStop::Run(failure));
        }
        if let Err(stop) = self.check(io) {
            return self.stopped(stop);
        }
        let owner = &mut self.raised.owner;
        let source = owner
            .boot
            .resident
            .as_ref()
            .unwrap()
            .evidence
            .startup
            .original_enter
            .as_ref()
            .unwrap();
        let send = self.send.as_ref().expect("actual staged answer");
        if !source.peer().owns_send(send) || !send.matches_exchange(&source.envelope().exchange) {
            return self.stopped(ReadyStop::InvalidSend);
        }
        if send.sent() {
            if !send
                .sent_at()
                .is_some_and(|at| at >= self.enter.began_ns() && !self.enter.expired(at))
            {
                return self.stopped(ReadyStop::InvalidSend);
            }
            if owner.boot.context.complete_wait(PumpDeadline::Enter) != Some(self.enter) {
                return self.stopped(ReadyStop::Retirement);
            }
            self.stage = ReadyStage::Complete;
        }
        self.stage
    }
}
impl<'a> EnterReady<'a> {
    #[allow(clippy::result_large_err)]
    pub(in crate::flow) fn finish(self) -> Result<EnteredServing<'a>, Self> {
        if self.stage != ReadyStage::Complete {
            return Err(self);
        }
        let owner = self.raised.owner;
        let (context, decode, certificate, resident) = owner.boot.into_parts();
        Ok(EnteredServing {
            context,
            decode,
            certificate,
            resident: resident.expect("actual resident arms"),
            listener: owner.listener,
            gate: owner.port.expect("actual gate"),
            gate_child: owner.child.expect("actual gate child"),
            tools: owner.tools.expect("actual containment"),
            gate_request: owner.request,
            gate_raise: owner.raise,
            gate_send: owner.send.expect("whole Raise"),
            gate_reply: owner.reply.expect("actual gate Ready"),
            ready: self.send.expect("whole Ready"),
            answer: self.answer,
            enter: self.enter,
        })
    }
}

#[cfg(test)]
#[path = "context_enter_ready_tests.rs"]
pub(super) mod tests;
