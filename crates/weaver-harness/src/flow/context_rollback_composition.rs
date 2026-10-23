//! Actual Recorded Enter refusal -> tracked response -> accepted rollback ->
//! measured current Grants and release phase. Terminal cleanup remains owned.
use super::*;
use crate::flow::pump::{LifecycleMessage, LifecyclePeer};
use crate::flow::pump_transport::{OrganSend, TrackedStage};
use weaver_types::{LifecycleDirective, Position};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum ResponseStage {
    Sending,
    Complete,
    Stopped,
}
#[derive(Debug)]
pub(in crate::flow) enum ResponseStop {
    Run(RunFailure),
    Busy,
    Stage(crate::flow::pump_transport::Failure),
    WrongSend,
}

pub(in crate::flow) struct EnterRefusalResponse<'a> {
    refusal: RecordedStartupRefusal<'a>,
    peer: LifecyclePeer,
    stage: ResponseStage,
    send: Option<OrganSend>,
    returned: Option<OrganEnvelope>,
    stop: Option<ResponseStop>,
}

impl<'a> RecordedStartupRefusal<'a> {
    /// The caller supplies the original Enter connection; this does not invent
    /// proof of admin's entry judgment. The exact staged send is tied to this peer.
    pub(in crate::flow) fn into_response(self, peer: LifecyclePeer) -> EnterRefusalResponse<'a> {
        EnterRefusalResponse {
            refusal: self,
            peer,
            stage: ResponseStage::Sending,
            send: None,
            returned: None,
            stop: None,
        }
    }
}

impl EnterRefusalResponse<'_> {
    pub(in crate::flow) fn stage(&self) -> ResponseStage {
        self.stage
    }
    pub(in crate::flow) fn stop(&self) -> Option<&ResponseStop> {
        self.stop.as_ref()
    }
    pub(in crate::flow) fn send(&self) -> Option<&OrganSend> {
        self.send.as_ref()
    }

    fn check(&mut self, io: &mut impl PumpIo) -> Result<(), RunFailure> {
        self.refusal.owner.check(&mut PumpPorts {
            lifecycle: LifecyclePorts {
                peer: Some(&mut self.peer),
                ..Default::default()
            },
            gate: None,
            decode: None,
            label: None,
            io,
        })
    }
    fn stopped(&mut self, cause: ResponseStop) -> ResponseStage {
        self.stop = Some(cause);
        self.stage = ResponseStage::Stopped;
        self.stage
    }
    pub(in crate::flow) fn advance(&mut self, io: &mut impl PumpIo) -> ResponseStage {
        if self.stage == ResponseStage::Stopped {
            return self.stage;
        }
        if let Err(error) = self.check(io) {
            return self.stopped(ResponseStop::Run(error));
        }
        if self.stage == ResponseStage::Complete {
            return self.stage;
        }
        if self.send.is_none() {
            match self.peer.stage_tracked(self.refusal.envelope().clone()) {
                Ok(TrackedStage::Queued(send)) => self.send = Some(send),
                Ok(TrackedStage::Busy(original)) => {
                    self.returned = Some(original);
                    return self.stopped(ResponseStop::Busy);
                }
                Err(error) => {
                    self.returned = Some(error.input);
                    return self.stopped(ResponseStop::Stage(error.failure));
                }
            }
        }
        let context = &mut self.refusal.owner.startup.context;
        if let Err(error) = context.pump_once(&mut PumpPorts {
            lifecycle: LifecyclePorts {
                peer: Some(&mut self.peer),
                ..Default::default()
            },
            gate: None,
            decode: None,
            label: None,
            io,
        }) {
            // The transport marks Sent before observing its common clock. A
            // missing timestamp after a whole send is that failed clock read,
            // never permission to resend or to resume healthy rollback.
            if self
                .send
                .as_ref()
                .is_some_and(|send| send.sent() && send.sent_at().is_none())
            {
                context.fail(
                    io,
                    FlowHaltReason::TraceFailed,
                    HaltPoint::Author,
                    format!("Enter response clock observation failed: {error:?}"),
                );
            }
            return self.stopped(ResponseStop::Run(error));
        }
        if let Err(error) = self.check(io) {
            return self.stopped(ResponseStop::Run(error));
        }
        let send = self.send.as_ref().expect("actual staged response");
        if !self.peer.owns_send(send) || !send.matches_exchange(&self.refusal.envelope().exchange) {
            return self.stopped(ResponseStop::WrongSend);
        }
        if send.sent() {
            let bound = self.refusal.owner.bound;
            if !send
                .sent_at()
                .is_some_and(|at| at >= bound.began_ns() && !bound.expired(at))
            {
                return self.stopped(ResponseStop::WrongSend);
            }
            self.stage = ResponseStage::Complete;
        }
        self.stage
    }
}

#[derive(Debug)]
pub(in crate::flow) enum RollbackJoinReason {
    NoDirective,
    WrongDirective,
    WrongWindow,
}
pub(in crate::flow) struct RollbackJoinError<'a> {
    pub response: EnterRefusalResponse<'a>,
    pub peer: LifecyclePeer,
    pub bound: AbsoluteBound,
    pub reason: RollbackJoinReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum RollbackStage {
    Transition,
    RetireClassifier,
    Grants,
    Release,
    Prepared,
    Stopped,
}
#[derive(Debug)]
// Preserve an actual stopped inspection without allocating on its failure path.
#[allow(clippy::large_enum_variant)]
pub(in crate::flow) enum RollbackStop {
    Run(RunFailure),
    WrongPhase,
    WrongWindow,
    InspectionStart(InspectionBeginError),
    Inspection(InspectionCompletion),
}

/// The actual accepted Leave remains beside the old response transport and its
/// exact send evidence. No old response is pumped after this handoff. This does
/// not synthesize Unload/Left for a child which never existed.
pub(in crate::flow) struct RollbackComposition<'a> {
    response: EnterRefusalResponse<'a>,
    peer: LifecyclePeer,
    directive: LifecycleMessage,
    leave: AbsoluteBound,
    stage: RollbackStage,
    inspecting: bool,
    current: Option<InspectionCompletion>,
    stop: Option<RollbackStop>,
}

impl<'a> EnterRefusalResponse<'a> {
    /// The outer listener provides the actual received peer and the original
    /// Leave allowance. Rejection returns both owners and all held handles.
    #[allow(clippy::result_large_err)]
    pub(in crate::flow) fn accept_rollback(
        self,
        mut peer: LifecyclePeer,
        bound: AbsoluteBound,
    ) -> Result<RollbackComposition<'a>, RollbackJoinError<'a>> {
        let reason = match peer.held() {
            None => Some(RollbackJoinReason::NoDirective),
            Some(message)
                if message.envelope.exchange.opener != Opener::Admin
                    || message.envelope.position != Position::Open
                    || message.sink.is_some()
                    || message.state.is_some()
                    || !matches!(
                        message.envelope.payload,
                        weaver_types::Payload::Directive(LifecycleDirective::Leave {
                            rollback: true,
                            ..
                        })
                    ) =>
            {
                Some(RollbackJoinReason::WrongDirective)
            }
            Some(_) => None,
        };
        if let Some(reason) = reason {
            return Err(RollbackJoinError {
                response: self,
                peer,
                bound,
                reason,
            });
        }
        let context = &self.refusal.owner.startup.context;
        let original = self.refusal.owner.bound;
        let actual_enter = match context.deadlines.get(PumpDeadline::Enter) {
            Some(enter) => enter == original,
            None => context.retired_enter == Some(original),
        };
        if !actual_enter || context.deadlines.get(PumpDeadline::Leave).is_some() {
            return Err(RollbackJoinError {
                response: self,
                peer,
                bound,
                reason: RollbackJoinReason::WrongWindow,
            });
        }
        let directive = peer
            .take_completion()
            .expect("held actual Leave is retained despite peer close")
            .expect("validated actual held Leave");
        Ok(RollbackComposition {
            response: self,
            peer,
            directive,
            leave: bound,
            stage: RollbackStage::Transition,
            inspecting: false,
            current: None,
            stop: None,
        })
    }
}

impl RollbackComposition<'_> {
    pub(in crate::flow) fn stage(&self) -> RollbackStage {
        self.stage
    }
    pub(in crate::flow) fn stop(&self) -> Option<&RollbackStop> {
        self.stop.as_ref()
    }
    pub(in crate::flow) fn directive(&self) -> &LifecycleMessage {
        &self.directive
    }
    /// Admin Spec 3 S1 x Leave: rollback is forced regardless of its wire bit.
    /// The original directive remains unchanged for custody and attribution.
    pub(in crate::flow) fn forced(&self) -> bool {
        true
    }
    pub(in crate::flow) fn leave(&self) -> AbsoluteBound {
        self.leave
    }
    pub(in crate::flow) fn current(&self) -> Option<&InspectionCompletion> {
        self.current.as_ref()
    }
    pub(in crate::flow) fn initial(&self) -> &InspectionCompletion {
        self.response
            .refusal
            .owner
            .startup
            .initial_grants
            .as_ref()
            .expect("classifier refusal follows the retained initial Grants")
    }
    fn stopped(&mut self, cause: RollbackStop) -> RollbackStage {
        self.stop = Some(cause);
        self.stage = RollbackStage::Stopped;
        self.stage
    }
    pub(in crate::flow) fn advance(&mut self, io: &mut impl PumpIo) -> RollbackStage {
        if self.stage == RollbackStage::Stopped {
            return self.stage;
        }
        if let Err(error) = self.step(io) {
            self.stopped(RollbackStop::Run(error));
        }
        self.stage
    }
    fn step(&mut self, io: &mut impl PumpIo) -> Result<(), RunFailure> {
        let context = &mut self.response.refusal.owner.startup.context;
        if let Some(failure) = context.failure().cloned() {
            return Err(failure);
        }
        if self.stage == RollbackStage::Transition {
            if context.phase != RecordingPhase::Entered {
                self.stopped(RollbackStop::WrongPhase);
                return Ok(());
            }
            let original = self.response.refusal.owner.bound;
            // An actual rollback may arrive after Enter expired. Only that exact
            // wait may be retired; another live/clock failure never grants this.
            match context.live(io) {
                Ok(()) => {}
                Err(RunFailure::Wait {
                    key: PumpDeadline::Enter,
                    bound,
                }) if bound == original => {}
                Err(error) => return Err(error),
            }
            let now = context.last_ns;
            if now < self.leave.began_ns() || self.leave.expired(now) {
                self.stopped(RollbackStop::WrongWindow);
                return Ok(());
            }
            match context.deadlines.get(PumpDeadline::Enter) {
                Some(bound) if bound == original => {
                    if context.complete_wait(PumpDeadline::Enter) != Some(original) {
                        self.stopped(RollbackStop::WrongWindow);
                        return Ok(());
                    }
                }
                None if context.retired_enter == Some(original) => {}
                _ => {
                    self.stopped(RollbackStop::WrongWindow);
                    return Ok(());
                }
            }
            if context.deadlines.get(PumpDeadline::Leave).is_some() {
                self.stopped(RollbackStop::WrongWindow);
                return Ok(());
            }
            context.arm(PumpDeadline::Leave, self.leave, io)?;
            self.stage = RollbackStage::RetireClassifier;
            return Ok(());
        }
        if context.deadlines.get(PumpDeadline::Leave) != Some(self.leave) {
            self.stopped(RollbackStop::WrongWindow);
            return Ok(());
        }
        context.live(io)?;
        match self.stage {
            RollbackStage::RetireClassifier => {
                context.retire_refused_classifier(io)?;
                self.response
                    .refusal
                    .owner
                    .startup
                    .resources
                    .poll_classifier_child();
                self.stage = RollbackStage::Grants;
            }
            RollbackStage::Grants => {
                if !self.inspecting {
                    if let Err(error) = context.begin_inspection(InspectionKind::Grants, None, io) {
                        self.stopped(RollbackStop::InspectionStart(error));
                        return Ok(());
                    }
                    self.inspecting = true;
                }
                let mut ports = PumpPorts {
                    lifecycle: LifecyclePorts {
                        peer: Some(&mut self.peer),
                        ..Default::default()
                    },
                    gate: None,
                    decode: None,
                    label: None,
                    io,
                };
                let progress = context.advance_inspection(&mut ports)?;
                if let Some(completion) = context.take_inspection(ports.io)? {
                    match completion {
                        granted @ InspectionCompletion::Grants { .. } => {
                            self.current = Some(granted);
                            self.stage = RollbackStage::Release;
                        }
                        stopped => {
                            self.stopped(RollbackStop::Inspection(stopped));
                        }
                    }
                } else if progress == InspectionProgress::Pending {
                    context.pump_once(&mut ports)?;
                }
            }
            RollbackStage::Release => {
                context.begin_releasing(self.leave, io)?;
                self.response
                    .refusal
                    .owner
                    .startup
                    .resources
                    .poll_classifier_child();
                self.stage = RollbackStage::Prepared;
            }
            RollbackStage::Prepared => {}
            RollbackStage::Transition | RollbackStage::Stopped => unreachable!(),
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "context_rollback_composition_tests.rs"]
pub(super) mod tests;
