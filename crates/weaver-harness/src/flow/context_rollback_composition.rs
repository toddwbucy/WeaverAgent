//! Actual Recorded Enter refusal -> tracked response -> accepted rollback ->
//! measured initial/current Grants comparison and an owned release handoff.
//! Terminal cleanup remains separate; this owner grants no Unload or Left.
use super::*;
use crate::flow::pump::{LifecycleListener, LifecycleMessage, LifecyclePeer};
#[path = "context_rollback_listener.rs"]
mod listener;
use crate::flow::pump_transport::{OrganSend, TrackedStage};
use listener::RollbackDials;
use weaver_trace::GrantSurface;
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
    original: Option<crate::flow::pump::enter::DetachedEnter>,
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
            original: None,
        }
    }

    #[allow(clippy::result_large_err)]
    pub(in crate::flow) fn into_original_response(
        mut self,
    ) -> Result<EnterRefusalResponse<'a>, Self> {
        let Some(original) = self.owner.startup.original_enter.take() else {
            return Err(self);
        };
        let (peer, original) = original.into_response_peer();
        let mut response = self.into_response(peer);
        response.original = Some(original);
        Ok(response)
    }
}

impl EnterRefusalResponse<'_> {
    /// Await only root's explicit rollback. An expired original Enter is not a
    /// new failure or time allowance; every other live/clock failure survives.
    pub(in crate::flow) fn observe_await(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<u64, RunFailure> {
        let context = &mut self.refusal.owner.startup.context;
        match context.live(io) {
            Ok(()) => {}
            Err(RunFailure::Wait {
                key: PumpDeadline::Enter,
                bound,
            }) if bound == self.refusal.owner.bound => {}
            Err(error) => return Err(error),
        }
        Ok(context.last_ns)
    }
    /// Transport observation while awaiting root is not boot work. Keep the
    /// original Enter registered and retained, but do not use its expiry to
    /// suppress state/trace failure observation or the actual rollback accept.
    pub(in crate::flow) fn pump_await(
        &mut self,
        ports: &mut PumpPorts<'_, impl PumpIo>,
        read: Option<AbsoluteBound>,
    ) -> Result<(), RunFailure> {
        self.observe_await(ports.io)?;
        let context = &mut self.refusal.owner.startup.context;
        if context.phase != RecordingPhase::Entered
            || context.deadlines.get(PumpDeadline::Enter) != Some(self.refusal.owner.bound)
            || context.deadlines.get(PumpDeadline::Leave).is_some()
        {
            return Err(RunFailure::EnterRefusalOwned);
        }
        let mut waits = crate::flow::clock::DeadlineRegistry::new();
        for (key, bound) in context.deadlines.entries() {
            if key != PumpDeadline::Enter {
                waits
                    .arm(key, bound)
                    .map_err(|_| RunFailure::EnterRefusalOwned)?;
            }
        }
        if let Some(read) = read {
            waits
                .arm(PumpDeadline::AdminRead, read)
                .map_err(|_| RunFailure::EnterRefusalOwned)?;
        }
        let result = context.pump.step(
            ports,
            Some(&mut context.flow),
            &mut FlowRecording {
                barrier: &mut context.barrier,
                inbox: &mut context.inbox,
                budget: &mut context.budget,
                author: &context.author,
            },
            &waits,
        );
        match result {
            Ok(crate::flow::pump::PumpStep::Deadline { key, bound }) => {
                return Err(RunFailure::Wait { key, bound });
            }
            Ok(_) => {}
            Err(crate::flow::pump::PumpFailure::Transport { source, failure }) => {
                return Err(RunFailure::Transport { source, failure });
            }
            Err(error) => {
                if let Some(failure) = context.flow.failure().cloned() {
                    return Err(context.flow_failed(ports.io, failure));
                }
                if context.barrier.failure().is_some()
                    || context.barrier.standing_failure().is_some()
                {
                    return Err(context.record_failed(ports.io));
                }
                return Err(context.fail(
                    ports.io,
                    FlowHaltReason::TraceFailed,
                    HaltPoint::Author,
                    format!("rollback wait: {error:?}"),
                ));
            }
        }
        self.observe_await(ports.io).map(|_| ())
    }
    pub(in crate::flow) fn recorded_sequence(&self) -> Sequence {
        self.refusal.sequence()
    }
    pub(in crate::flow) fn original_bound(&self) -> AbsoluteBound {
        self.refusal.owner.bound
    }
    pub(in crate::flow) fn context(&self) -> &RecordingContext {
        self.refusal.owner.startup.context
    }
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
    UnclockedListener,
}
pub(in crate::flow) struct RollbackJoinError<'a> {
    pub response: EnterRefusalResponse<'a>,
    pub peer: LifecyclePeer,
    pub listener: LifecycleListener,
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
    dials: RollbackDials,
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
        listener: LifecycleListener,
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
                listener,
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
                listener,
                bound,
                reason: RollbackJoinReason::WrongWindow,
            });
        }
        if listener.held().is_some() && listener.accepted_ns().is_none() {
            return Err(RollbackJoinError {
                response: self,
                peer,
                listener,
                bound,
                reason: RollbackJoinReason::UnclockedListener,
            });
        }
        let directive = peer
            .take_completion()
            .expect("held actual Leave is retained despite peer close")
            .expect("validated actual held Leave");
        Ok(RollbackComposition {
            response: self,
            peer,
            dials: RollbackDials::new(listener),
            directive,
            leave: bound,
            stage: RollbackStage::Transition,
            inspecting: false,
            current: None,
            stop: None,
        })
    }
}

impl<'a> RollbackComposition<'a> {
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
    /// Move the whole actual chain into release preparation. No caller can
    /// provide a substitute initial reading, result, cause, deadline or peer.
    /// Rejection returns the unchanged owner for its ongoing work/accounting.
    #[allow(clippy::result_large_err)]
    pub(in crate::flow) fn into_prepared(self) -> Result<PreparedRollback<'a>, Self> {
        if self.stage != RollbackStage::Prepared {
            return Err(self);
        }
        let Some(current) = self.current.as_ref() else {
            return Err(self);
        };
        let grant_surface = match (self.initial(), current) {
            (
                InspectionCompletion::Grants {
                    surface: initial, ..
                },
                InspectionCompletion::Grants {
                    surface: current, ..
                },
            ) if initial == current => GrantSurface::Unchanged,
            (InspectionCompletion::Grants { .. }, InspectionCompletion::Grants { .. }) => {
                GrantSurface::Varied
            }
            (
                InspectionCompletion::Grants { .. } | InspectionCompletion::Refused { .. },
                InspectionCompletion::Refused { .. },
            )
            | (InspectionCompletion::Refused { .. }, InspectionCompletion::Grants { .. }) => {
                GrantSurface::Unreadable
            }
            _ => return Err(self),
        };
        Ok(PreparedRollback {
            owner: self,
            grant_surface,
        })
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
            let context = &mut self.response.refusal.owner.startup.context;
            let result = if matches!(
                error,
                RunFailure::Transport {
                    source: crate::flow::pump::PumpSource::LeavePeer,
                    ..
                }
            ) {
                eprintln!(
                    "weaver-harness: accepted Leave answer path failed ({error:?}); rollback continues"
                );
                context.live(io)
            } else {
                self.dials.recover(context, error, io)
            };
            if let Err(error) = result {
                self.stopped(RollbackStop::Run(error));
            }
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
        self.dials.prepare(context, self.leave, io)?;
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
                    // The original Leave peer remains only an answer path;
                    // secondary dials cannot become new cancellation authority.
                    lifecycle: self.dials.ports(&mut self.peer),
                    gate: None,
                    decode: None,
                    label: None,
                    io,
                };
                let progress = context.advance_inspection(&mut ports)?;
                if let Some(completion) = context.take_inspection(ports.io)? {
                    match completion {
                        reading @ (InspectionCompletion::Grants { .. }
                        | InspectionCompletion::Refused { .. }) => {
                            // A typed read refusal is a recorded observation of
                            // unreadability, not authority to stop healthy Leave.
                            self.current = Some(reading);
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

/// Preparation is a retained fact, not terminal permission. The final lifecycle
/// owner must still service the listener, release resources, record Unload and
/// drain before answering Left. There is no mutable context/peer escape here.
pub(in crate::flow) struct PreparedRollback<'a> {
    owner: RollbackComposition<'a>,
    grant_surface: GrantSurface,
}

impl PreparedRollback<'_> {
    pub(in crate::flow) fn grant_surface(&self) -> GrantSurface {
        self.grant_surface
    }
    pub(in crate::flow) fn initial(&self) -> &InspectionCompletion {
        self.owner.initial()
    }
    pub(in crate::flow) fn current(&self) -> &InspectionCompletion {
        self.owner
            .current()
            .expect("prepared rollback retains its current reading")
    }
    pub(in crate::flow) fn directive(&self) -> &LifecycleMessage {
        self.owner.directive()
    }
    pub(in crate::flow) fn forced(&self) -> bool {
        self.owner.forced()
    }
    pub(in crate::flow) fn leave(&self) -> AbsoluteBound {
        self.owner.leave()
    }
}

#[cfg(test)]
#[path = "context_rollback_composition_tests.rs"]
pub(super) mod tests;

#[path = "context_rollback_halt.rs"]
pub(in crate::flow) mod halt;

#[path = "context_rollback_terminal.rs"]
pub(in crate::flow) mod terminal;
