//! Paired Diagnostic management over the existing replay lifecycle. The codec
//! is selected only by transfer of a checked canonical Diagnostic Enter.
use super::*;
use weaver_types::state_flow::enter::{
    EnterBinding, EnterEnvelope, LoadFacts, LoadedStateFacts, ObservationAnswer,
    ObservationContent, ObservationEnvelope,
};
use weaver_types::{LifecycleAnswer, LifecycleDirective, Payload};

pub(crate) enum DiagnosticRequest {
    Ordinary(OrganEnvelope),
    Enter(Box<EnterEnvelope>),
}

impl CoordinationListener {
    pub(crate) fn diagnostic_wire(&mut self, selected: bool) {
        self.canonical_diagnostic = selected;
    }
}

impl OrganChannel {
    pub(crate) fn is_diagnostic_wire(&self) -> bool {
        self.canonical_diagnostic
    }

    pub(crate) fn diagnostic_from_owned(end: OwnedFd) -> Self {
        Self {
            end,
            canonical_diagnostic: true,
        }
    }

    pub(crate) fn recv_diagnostic(&self) -> Result<DiagnosticRequest, ChannelFault> {
        let (request, sink, state) = self.recv_with_decoder(|bytes| {
            if let Ok(enter) = EnterEnvelope::decode(bytes) {
                return Ok(DiagnosticRequest::Enter(Box::new(enter)));
            }
            let envelope: OrganEnvelope = weaver_types::state_flow::decode_strict(
                std::str::from_utf8(bytes).map_err(|_| ChannelFault::Undecodable)?,
            )
            .map_err(|_| ChannelFault::Undecodable)?;
            if envelope.exchange.opener != weaver_types::Opener::Admin
                || envelope.position != weaver_types::Position::Open
                || !matches!(envelope.payload, Payload::Directive(_))
                || matches!(
                    envelope.payload,
                    Payload::Directive(LifecycleDirective::Enter { .. })
                )
            {
                return Err(ChannelFault::Undecodable);
            }
            Ok(DiagnosticRequest::Ordinary(envelope))
        })?;
        // Enter may carry its usual descriptors even when out of order. They
        // have exactly one owner here and close with this rejected opener.
        if matches!(request, DiagnosticRequest::Ordinary(_)) && (sink.is_some() || state.is_some())
        {
            return Err(ChannelFault::Undecodable);
        }
        Ok(request)
    }
}

pub(super) fn encode_answer(envelope: &OrganEnvelope) -> Result<Vec<u8>, ChannelFault> {
    let payload = match &envelope.payload {
        Payload::Answer(LifecycleAnswer::State {
            plan,
            state,
            load,
            constituents,
        }) => {
            if plan.is_some() {
                return Err(ChannelFault::Undecodable);
            }
            ObservationAnswer::State {
                state: state.clone(),
                load: load.as_ref().map(|load| {
                    Box::new(LoadFacts {
                        session: load.session.clone(),
                        run: load.run.clone(),
                        declaration: load.declaration.clone(),
                        artifact: load.artifact.clone(),
                        residual_readout: load.residual_readout,
                        field: load.field,
                        surprisal: load.surprisal,
                        state: LoadedStateFacts::Diagnostic {
                            state_election: load.state_election.clone(),
                        },
                        state_store: load.state_store.clone(),
                        state_member: load.state_member,
                        composer: load.composer.clone(),
                        drain_bound: load.drain_bound,
                        wind_down_bound: load.wind_down_bound,
                    })
                }),
                plan: None,
                constituents: constituents.clone(),
            }
        }
        Payload::Answer(LifecycleAnswer::InTransition { constituents }) => {
            ObservationAnswer::InTransition {
                constituents: constituents.clone(),
            }
        }
        _ => return serde_json::to_vec(envelope).map_err(|_| ChannelFault::Undecodable),
    };
    ObservationEnvelope {
        exchange: envelope.exchange.clone(),
        position: envelope.position.clone(),
        payload: ObservationContent::Answer(payload),
    }
    .encode()
    .map_err(|_| ChannelFault::Undecodable)
}

/// Only a Diagnostic source can enter the existing Diagnostic implementation.
/// This is an in-memory projection of the same facts, never a legacy wire retry.
pub(crate) fn diagnostic_payload(source: &EnterEnvelope) -> Option<weaver_types::EnterPayload> {
    let weaver_types::state_flow::enter::EnterPayload {
        session,
        run,
        spu_instruction,
        binding,
        state_store,
        declaration,
        restore,
        reset,
        stack,
        boundary,
        cause,
        operator,
        library_path,
        drain_bound,
        wind_down_bound,
    } = source.enter();
    let EnterBinding::Diagnostic { state_election } = binding else {
        return None;
    };
    Some(weaver_types::EnterPayload {
        session: session.clone(),
        run: run.clone(),
        spu_instruction: spu_instruction.clone(),
        binding: weaver_types::EnterBinding::Diagnostic,
        state_election: state_election.clone(),
        state_store: state_store.clone(),
        declaration: declaration.clone(),
        restore: restore.clone(),
        reset: reset.clone(),
        stack: stack.clone(),
        boundary: boundary.clone(),
        cause: *cause,
        operator: *operator,
        library_path: library_path.clone(),
        drain_bound: *drain_bound,
        wind_down_bound: *wind_down_bound,
    })
}
