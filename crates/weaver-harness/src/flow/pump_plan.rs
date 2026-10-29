//! A plan input is transferred only from the actual root coordination peer.
use super::*;
use weaver_types::{LifecycleDirective, LifecycleRefusal, Opener, Payload, Position};

/// Neither a decoded lifecycle value nor a model frame constructs this receipt.
pub(in crate::flow) struct RootPlanPacket {
    message: LifecycleMessage,
    wire: Vec<u8>,
}

#[cfg(test)]
#[path = "pump_plan_tests.rs"]
pub(in crate::flow) mod tests;
impl RootPlanPacket {
    pub(in crate::flow) fn envelope(&self) -> &OrganEnvelope {
        &self.message.envelope
    }
    pub(in crate::flow) fn wire(&self) -> &[u8] {
        &self.wire
    }
    pub(in crate::flow) fn input(
        &self,
    ) -> (weaver_types::Cause, &weaver_types::state_flow::Digest, &str) {
        let Payload::Directive(LifecycleDirective::PlanUpdate {
            cause,
            document_sha256,
            document,
        }) = &self.message.envelope.payload
        else {
            unreachable!("root packet retains its judged directive")
        };
        (*cause, document_sha256, document)
    }
}

#[derive(Debug)]
pub(in crate::flow) enum PlanReceiveError {
    Transport(Failure),
    Refused(LifecycleRefusal),
}

impl LifecyclePeer {
    /// A failed judgment leaves the original packet and handles in this slot.
    /// A complete accepted packet survives a subsequent peer EOF.
    pub(in crate::flow) fn take_root_plan_update(
        &mut self,
    ) -> Result<Option<RootPlanPacket>, PlanReceiveError> {
        let Some(held) = &self.held else {
            self.live().map_err(PlanReceiveError::Transport)?;
            return Ok(None);
        };
        let credentials = getsockopt(&self.sender.fd(), sockopt::PeerCredentials)
            .map_err(|e| PlanReceiveError::Transport(io_failure(e)))?;
        if credentials.uid() != 0 {
            return Err(PlanReceiveError::Refused(LifecycleRefusal::Unauthorized));
        }
        if self.projection != LifecycleProjection::ServingOpener
            || held.envelope.exchange.opener != Opener::Admin
            || held.envelope.position != Position::Open
            || held.sink.is_some()
            || held.state.is_some()
        {
            return Err(PlanReceiveError::Refused(LifecycleRefusal::Malformed));
        }
        let Payload::Directive(LifecycleDirective::PlanUpdate { cause, .. }) =
            &held.envelope.payload
        else {
            return Err(PlanReceiveError::Refused(LifecycleRefusal::OutOfOrder));
        };
        if cause.uid != 0 {
            return Err(PlanReceiveError::Refused(LifecycleRefusal::Unauthorized));
        }
        Ok(Some(RootPlanPacket {
            message: self.held.take().unwrap(),
            wire: self
                .wire
                .take()
                .expect("received root packet retains its wire"),
        }))
    }
}
