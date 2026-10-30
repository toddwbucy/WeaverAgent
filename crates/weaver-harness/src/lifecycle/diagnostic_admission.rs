//! The canonical Diagnostic admission joins the existing replay path.
use super::*;
use crate::channel::diagnostic::DiagnosticRequest;

pub(crate) enum DiagnosticService {
    BeforeEnter {
        listener: CoordinationListener,
        peer: OrganChannel,
    },
    Finished(Result<Outcome, ChannelFault>),
}

/// An already entered Diagnostic run refuses another canonical Enter without
/// reinterpreting its binding or dropping into the old Serving implementation.
pub(crate) fn receive_entered(
    connection: &OrganChannel,
    record_failed: bool,
) -> Result<Option<OrganEnvelope>, ChannelFault> {
    // Decoding can itself answer a refusal. The caller's current record latch
    // therefore gates that answer as it gates ordinary lifecycle dispatch.
    if record_failed {
        return Err(ChannelFault::Closed);
    }
    if !connection.is_diagnostic_wire() {
        return connection.recv().map(Some);
    }
    match connection.recv_diagnostic()? {
        DiagnosticRequest::Ordinary(envelope) => Ok(Some(envelope)),
        DiagnosticRequest::Refused { exchange, reason } => {
            connection.send(&OrganEnvelope {
                exchange,
                position: Position::Close,
                payload: weaver_types::Payload::Refusal(reason),
            })?;
            Ok(None)
        }
    }
}

impl Harness {
    pub(crate) fn serve_diagnostic(
        mut self,
        connection: OrganChannel,
        source: &weaver_types::state_flow::enter::EnterEnvelope,
        sink: Option<OwnedFd>,
        state: Option<OwnedFd>,
        composer: weaver_trace::LoopIdentity,
    ) -> DiagnosticService {
        let Some(payload) = crate::channel::diagnostic::diagnostic_payload(source) else {
            return DiagnosticService::Finished(Err(ChannelFault::Undecodable));
        };
        self.composer = Some(composer);
        match self.dispatch_on(
            &connection,
            source.exchange.clone(),
            LifecycleDirective::Enter {
                payload: Box::new(payload),
            },
            sink,
            state,
        ) {
            Ok(None) => (),
            Ok(Some(outcome)) => return DiagnosticService::Finished(Ok(outcome)),
            Err(fault) => {
                self.unwind_if_entered(weaver_types::FaultCase::OrganDeathObserved, &fault);
                return DiagnosticService::Finished(Err(fault));
            }
        }
        if matches!(self.state, ChannelState::BeforeEnter) {
            return DiagnosticService::BeforeEnter {
                listener: self.coordination,
                peer: connection,
            };
        }
        // An admitted Diagnostic run is driven through replay and Leave by
        // enter_diagnostic_managed. Only a before-load refusal returns a seat.
        DiagnosticService::Finished(Err(ChannelFault::Undecodable))
    }
}
