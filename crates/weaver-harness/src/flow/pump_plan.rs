//! A plan input is transferred only from the actual root coordination peer.
use super::*;
use weaver_types::{LifecycleDirective, LifecycleRefusal, Opener, Payload, Position};

/// Neither a decoded lifecycle value nor a model frame constructs this receipt.
pub(in crate::flow) struct RootPlanPacket {
    message: LifecycleMessage,
    wire: Vec<u8>,
}

/// Exact refused opener, with no authority to start a root plan operation.
/// A judged PlanUpdate carries no descriptors, including after peer EOF.
pub(in crate::flow) struct RefusedPlanPacket {
    message: LifecycleMessage,
    _wire: Vec<u8>,
}
impl RefusedPlanPacket {
    pub(in crate::flow) fn envelope(&self) -> &OrganEnvelope {
        &self.message.envelope
    }
    #[cfg(test)]
    pub(in crate::flow) fn wire(&self) -> &[u8] {
        &self._wire
    }
}

#[cfg(test)]
#[path = "pump_plan_tests.rs"]
pub(in crate::flow) mod tests;
impl RootPlanPacket {
    pub(in crate::flow) fn into_refused(self) -> RefusedPlanPacket {
        RefusedPlanPacket {
            message: self.message,
            _wire: self.wire,
        }
    }
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
    /// Retaining a rejected request does not authenticate its cause or grant
    /// plan admission. It keeps the received bytes beside the decoded value.
    pub(in crate::flow) fn take_refused_plan_update(
        &mut self,
    ) -> Result<Option<RefusedPlanPacket>, Failure> {
        let Some(held) = &self.held else {
            self.live()?;
            return Ok(None);
        };
        if self.projection != LifecycleProjection::ServingOpener
            || held.envelope.exchange.opener != Opener::Admin
            || held.envelope.position != Position::Open
            || held.sink.is_some()
            || held.state.is_some()
            || !matches!(
                held.envelope.payload,
                Payload::Directive(LifecycleDirective::PlanUpdate { .. })
            )
        {
            return Err(invalid_failure(
                "refused plan requires a judged PlanUpdate opener",
            ));
        }
        Ok(Some(RefusedPlanPacket {
            message: self.held.take().unwrap(),
            _wire: self.wire.take().expect("received plan retains exact wire"),
        }))
    }

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
