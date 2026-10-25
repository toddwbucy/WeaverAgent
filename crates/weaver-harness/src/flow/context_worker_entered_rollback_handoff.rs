//! The real pending root rollback consumes a pristine successful Ready seat.
use super::*;
use crate::flow::clock::AbsoluteBound;
use crate::flow::context::worker_startup::entered_rollback::{
    FoundationEnteredRollbackHandoff, idle_context,
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
        mut self: Box<Self>,
        io: &mut impl PumpIo,
    ) -> Result<Box<FoundationEnteredRollbackHandoff>, Box<Self>> {
        // Mere socket acceptance is not Leave authority. A complete original
        // directive remains usable even if its peer has since disappeared.
        if self.progress != FoundationManagementProgress::DirectivePending
            || self.message.is_some()
            || self.answer.is_some()
            || self.send.is_some()
            || self.stop.is_some()
            || self.input.stage != Stage::Waiting
            || self.input.received.is_some()
            || self.input.token.is_some()
            || self.input.turn.is_some()
            || self.input.seated.is_some()
            || self.input.close.is_some()
            || self.input.send.is_some()
            || self.input.report.is_some()
            || self.input.report_started
            || self.input.returned_report.failure.is_some()
            || self.input.failure.is_some()
            || self.input.engine.is_some()
            || !self.peer.held().is_some_and(|message| {
                message.envelope.exchange.opener == Opener::Admin
                    && message.envelope.position == Position::Open
                    && message.sink.is_none()
                    && message.state.is_none()
                    && matches!(
                        message.envelope.payload,
                        Payload::Directive(LifecycleDirective::Leave { rollback: true, .. })
                    )
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
        let owner = &seat.owner;
        let source = owner.resident.evidence.startup.original_enter.as_ref();
        let valid = idle_context(owner.context)
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
            && seat.gate_outgoing == owner.gate_raise.exchange.ordinal
            && seat.gate_exchanges.is_empty()
            && seat.queued_inputs.is_empty();
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
        let FoundationInput { serving, .. } = *input;
        let FoundationServing {
            owner,
            gate_exchanges,
            queued_inputs,
            gate_outgoing,
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
        Ok(Box::new(FoundationEnteredRollbackHandoff {
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
            peer: Some(peer),
            message: message.expect("consumed actual rollback"),
            peer_unavailable,
            detail,
        }))
    }
}
