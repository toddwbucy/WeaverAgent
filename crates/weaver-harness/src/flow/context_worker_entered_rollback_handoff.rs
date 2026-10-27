//! The real pending root rollback consumes the successful Ready seat and its actual serving account.
use super::*;
use crate::flow::clock::AbsoluteBound;
use crate::flow::context::worker_startup::entered_rollback::{
    FoundationEnteredRollbackHandoff, LeaveKind, rollback_context,
};
use crate::flow::context::worker_startup::failed::StartupGate;
use crate::flow::pump::PumpDeadline;
use std::sync::Arc;

impl FoundationManagement<'_> {
    /// End the serving loans only for this retained rollback and Ready. Other
    /// directives, unfinished input and held-only admission return this owner.
    pub fn rollback_handoff(
        self: Box<Self>,
    ) -> Result<Box<FoundationEnteredRollbackHandoff>, Box<Self>> {
        self.rollback_handoff_with_io(&mut MonotonicIo)
    }

    pub(in crate::flow::context) fn rollback_handoff_with_io(
        self: Box<Self>,
        io: &mut impl PumpIo,
    ) -> Result<Box<FoundationEnteredRollbackHandoff>, Box<Self>> {
        self.entered_leave_handoff(LeaveKind::Rollback, io)
    }

    pub fn forced_leave_handoff(
        self: Box<Self>,
    ) -> Result<Box<crate::FoundationForcedLeaveHandoff>, Box<Self>> {
        self.entered_leave_handoff(LeaveKind::Forced, &mut MonotonicIo)
            .map(|inner| Box::new(crate::FoundationForcedLeaveHandoff { inner }))
    }

    fn entered_leave_handoff(
        mut self: Box<Self>,
        kind: LeaveKind,
        io: &mut impl PumpIo,
    ) -> Result<Box<FoundationEnteredRollbackHandoff>, Box<Self>> {
        // Mere socket acceptance is not Leave authority. A complete original
        // directive remains usable even if its peer has since disappeared.
        if self.progress != FoundationManagementProgress::DirectivePending
            || self.message.is_some()
            || self.answer.is_some()
            || self.send.is_some()
            || self.stop.is_some()
            || self.input.has_graceful_leave()
            || self.input.serving.save.is_some()
            || !self.input.rollback_admissible()
            || !self.peer.held().is_some_and(|message| {
                message.envelope.exchange.opener == Opener::Admin
                    && message.envelope.position == Position::Open
                    && message.sink.is_none()
                    && message.state.is_none()
                    && kind.accepts(&message.envelope.payload)
            })
        {
            return Err(self);
        }
        let seat = &mut self.input.serving;
        if let Err(failure) = seat.owner.context.live(io) {
            self.input.failure = Some(failure);
            self.input.stage = Stage::Stopped;
            self.progress = FoundationManagementProgress::Stopped;
            return Err(self);
        }
        let valid = self.input.entered_leave_evidence_valid();
        if !valid {
            return Err(self);
        }
        if self.input.serving.owner.context.check_held_input().is_err() {
            return Err(self);
        }
        let context = &mut self.input.serving.owner.context;
        let leave =
            match AbsoluteBound::after(context.last_ns, weaver_types::LEAVE_BOUND_MS * 1_000_000) {
                Ok(bound) => bound,
                Err(error) => {
                    self.detail = Some(format!("rollback Leave bound: {error}"));
                    return Err(self);
                }
            };
        self.message = match self.peer.take_completion() {
            Ok(Some(message)) => Some(message),
            Ok(None) => return Err(self),
            Err(error) => {
                self.detail = Some(format!("rollback directive: {error:?}"));
                return Err(self);
            }
        };
        if let Err(failure) = self
            .input
            .serving
            .owner
            .context
            .arm(PumpDeadline::Leave, leave, io)
        {
            self.input.failure = Some(failure);
            self.input.stage = Stage::Stopped;
            self.progress = FoundationManagementProgress::Stopped;
            return Err(self);
        }
        let Self {
            input,
            peer,
            message,
            peer_unavailable,
            detail,
            ..
        } = *self;
        Ok(input.into_entered_leave_handoff(
            leave,
            LeaveDirective {
                kind,
                peer,
                message: message.expect("consumed actual Leave"),
                peer_unavailable,
                detail,
                graceful_tail: None,
            },
        ))
    }
}

struct LeaveDirective {
    kind: LeaveKind,
    peer: LifecyclePeer,
    message: LifecycleMessage,
    peer_unavailable: bool,
    detail: Option<String>,
    graceful_tail: Option<Box<super::super::graceful::GracefulTail>>,
}

impl FoundationInput<'_> {
    fn entered_leave_evidence_valid(&self) -> bool {
        let seat = &self.serving;
        let owner = &seat.owner;
        let source = owner.resident.evidence.startup.original_enter.as_ref();
        rollback_context(owner.context)
            && seat.save.is_none()
            && owner.context.deadlines.get(PumpDeadline::Leave).is_none()
            && owner.context.retired_enter == Some(owner.enter)
            && owner.context.deadlines.get(PumpDeadline::Enter).is_none()
            && Arc::ptr_eq(&owner.context.read_origin, &owner.resident.evidence.origin)
            && owner.resident.evidence.enter == owner.enter
            && owner
                .resident
                .evidence
                .reply
                .belongs_to(owner.resident.residency)
            && owner.resident.request.send().sent()
            && owner
                .resident
                .request
                .send()
                .belongs_to(owner.resident.residency)
            && owner
                .resident
                .request
                .send()
                .matches_exchange(&owner.resident.evidence.reply.envelope().exchange)
            && source.is_some_and(|source| {
                source.adopted_by(owner.context)
                    && source.peer().owns_send(&owner.ready)
                    && owner.ready.matches_exchange(&source.envelope().exchange)
                    && owner.answer == source.ready_envelope()
            })
            && owner.ready.sent()
            && owner
                .ready
                .sent_at()
                .is_some_and(|at| at >= owner.enter.began_ns() && !owner.enter.expired(at))
            && owner.gate_send.sent()
            && owner.gate_send.belongs_to(&owner.gate)
            && owner.gate_send.matches_exchange(&owner.gate_raise.exchange)
            && owner
                .gate_send
                .sent_at()
                .is_some_and(|at| at >= owner.enter.began_ns() && !owner.enter.expired(at))
            && owner.gate_reply.belongs_to(&owner.gate)
            && owner.gate_reply.envelope().exchange == owner.gate_raise.exchange
            && matches!(
                owner.gate_reply.envelope().payload,
                Payload::Answer(LifecycleAnswer::GateReady)
            )
            && seat.gate_outgoing >= owner.gate_raise.exchange.ordinal
    }

    fn into_entered_leave_handoff(
        self: Box<Self>,
        leave: AbsoluteBound,
        directive: LeaveDirective,
    ) -> Box<FoundationEnteredRollbackHandoff> {
        let (serving, input) = self.into_rollback_parts();
        let FoundationServing {
            owner,
            gate_exchanges,
            queued_inputs,
            gate_outgoing,
            graceful,
            save,
        } = serving;
        let crate::flow::context::boot_composition::gate_startup::ready::EnteredServing {
            context,
            decode,
            resident,
            certificate,
            listener,
            gate,
            gate_child,
            tools,
            gate_request,
            gate_raise,
            gate_send,
            gate_reply,
            ready,
            answer,
            enter,
        } = *owner;
        let origin = Arc::clone(&context.read_origin);
        let decode = decode.port_identity();
        let residency = resident.residency.port_identity();
        let crate::flow::context::residency_composition::ResidencyEvidence {
            origin: resident_origin,
            enter: resident_enter,
            startup,
            request,
            reply,
        } = resident.evidence;
        assert!(
            graceful.is_none(),
            "the actual graceful holder must already be detached"
        );
        assert!(
            save.is_none(),
            "an on-demand save cannot become a Leave holder"
        );
        Box::new(FoundationEnteredRollbackHandoff {
            kind: directive.kind,
            graceful_tail: directive.graceful_tail,
            origin,
            decode,
            residency,
            enter,
            leave,
            resident_origin,
            resident_enter,
            startup,
            residency_request: request,
            admit_reply: Some(reply),
            certificate,
            listener: Some(listener),
            arms: StartupGate {
                port: Some(gate),
                child: Some(gate_child),
                tools: Some(tools),
            },
            gate_request,
            gate_raise,
            gate_send,
            gate_reply,
            ready: Some(ready),
            answer,
            gate_exchanges,
            queued_inputs,
            gate_outgoing,
            input,
            peer: Some(directive.peer),
            message: directive.message,
            peer_unavailable: directive.peer_unavailable,
            detail: directive.detail,
        })
    }

    /// Move the original graceful phases and their unfinished accounts.
    /// A force settles the wind-down turn before the prior input and capture.
    pub fn graceful_leave_handoff(
        mut self: Box<Self>,
    ) -> Result<Box<crate::FoundationGracefulLeaveHandoff>, Box<Self>> {
        let mut io = MonotonicIo;
        let Some(original) = self.serving.graceful.as_ref() else {
            return Err(self);
        };
        let converted = original.converted();
        if !self.rollback_admissible()
            || (!converted && !original.phases_settled())
            || self.serving.owner.listener.held().is_some()
            || (!converted && self.serving.owner.context.open_turn.is_some())
            || (self.serving.owner.context.wind_down.is_some()
                && !(converted
                    && original
                        .wind_down
                        .as_ref()
                        .is_some_and(|work| work.owns_context(self.serving.owner.context))))
            || (!converted && self.serving.owner.context.input.snapshot().is_some())
            || self
                .serving
                .owner
                .context
                .deadlines
                .get(PumpDeadline::WindDown)
                .is_some()
            || self
                .serving
                .owner
                .context
                .deadlines
                .get(PumpDeadline::GracefulDrain)
                .is_some()
        {
            return Err(self);
        }
        if let Err(failure) = self.serving.owner.context.live(&mut io) {
            self.failure = Some(failure);
            self.stage = Stage::Stopped;
            return Err(self);
        }
        if !self.entered_leave_evidence_valid()
            || self.serving.owner.context.check_held_input().is_err()
        {
            return Err(self);
        }
        let context = &mut self.serving.owner.context;
        let Ok(leave) =
            AbsoluteBound::after(context.last_ns, weaver_types::LEAVE_BOUND_MS * 1_000_000)
        else {
            return Err(self);
        };
        if let Err(failure) = context.arm(PumpDeadline::Leave, leave, &mut io) {
            self.failure = Some(failure);
            self.stage = Stage::Stopped;
            return Err(self);
        }
        let original = self.serving.graceful.take().unwrap().into_tail();
        let inner = self.into_entered_leave_handoff(
            leave,
            LeaveDirective {
                kind: LeaveKind::Graceful,
                peer: original.peer,
                message: original.message,
                peer_unavailable: original.peer_unavailable,
                detail: original.detail,
                graceful_tail: Some(original.tail),
            },
        );
        Ok(Box::new(crate::FoundationGracefulLeaveHandoff { inner }))
    }
}
