//! Own the exact replacement Enter and its ancillary handles before adoption.
//! Shares the existing recvmsg/custody implementation; no active listener selects it.
use super::*;
use weaver_types::state_flow::enter::EnterEnvelope;

pub(in crate::flow) struct EnterReceiver {
    peer: LifecyclePeer,
    held: Option<LifecycleMessage<EnterEnvelope>>,
}
pub(in crate::flow) struct ReceivedEnter {
    peer: LifecyclePeer,
    message: LifecycleMessage<EnterEnvelope>,
    wire: Vec<u8>,
    adopted_origin: Option<std::sync::Arc<()>>,
}
impl EnterReceiver {
    #[allow(clippy::result_large_err)]
    pub(in crate::flow) fn from_peer(peer: LifecyclePeer) -> Result<Self, LifecyclePeer> {
        if peer.held.is_some()
            || peer.wire.is_some()
            || peer.failure().is_some()
            || peer.zero_received
            || peer.sender.interest().writable
        {
            return Err(peer);
        }
        Ok(Self { peer, held: None })
    }
    pub(in crate::flow) fn advance(&mut self) -> Result<ReceiveProgress, Failure> {
        if self.held.is_some() {
            // Keep completed input even when a later terminal observation fails.
            self.peer.check_terminal()?;
            return Ok(ReceiveProgress::Pending);
        }
        let message = self.peer.receive_message(|bytes| {
            EnterEnvelope::decode(bytes).map_err(|error| {
                serde_json::Error::io(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("replacement Enter: {error:?}"),
                ))
            })
        })?;
        match message {
            Some(message) => {
                self.held = Some(message);
                Ok(ReceiveProgress::Held)
            }
            None => Ok(ReceiveProgress::Pending),
        }
    }
    #[allow(clippy::result_large_err)]
    pub(in crate::flow) fn into_received(mut self) -> Result<ReceivedEnter, Self> {
        let Some(message) = self.held.take() else {
            return Err(self);
        };
        let wire = self
            .peer
            .wire
            .take()
            .expect("one actual receive owns its bytes");
        Ok(ReceivedEnter {
            peer: self.peer,
            message,
            wire,
            adopted_origin: None,
        })
    }
}
impl ReceivedEnter {
    pub(in crate::flow) fn envelope(&self) -> &EnterEnvelope {
        &self.message.envelope
    }
    pub(in crate::flow) fn wire(&self) -> &[u8] {
        &self.wire
    }
    pub(in crate::flow) fn into_parts(
        self,
    ) -> (LifecyclePeer, LifecycleMessage<EnterEnvelope>, Vec<u8>) {
        (self.peer, self.message, self.wire)
    }
}

/// The original Enter and peer stay beside the one recording context. No caller
/// can substitute a different flow declaration, session, run or descriptor here.
pub(in crate::flow) struct AdoptedEnter {
    source: ReceivedEnter,
    context: super::super::context::RecordingContext,
}
pub(in crate::flow) struct AdoptionFailure {
    pub(in crate::flow) source: ReceivedEnter,
    pub(in crate::flow) detail: String,
}
impl ReceivedEnter {
    #[allow(clippy::result_large_err)]
    pub(in crate::flow) fn adopt(
        mut self,
        remaining_enter_ns: u64,
        io: &mut impl PumpIo,
    ) -> Result<AdoptedEnter, AdoptionFailure> {
        use super::super::{channel::StateChannel, context::RecordingContext};
        use weaver_types::state_flow::enter::EnterBinding;
        let result = (|| {
            self.peer
                .check_terminal()
                .map_err(|error| format!("Enter peer: {error:?}"))?;
            let payload = self.message.envelope.enter();
            let EnterBinding::Serving {
                state_management,
                start,
                ..
            } = &payload.binding
            else {
                return Err("recording adoption requires Serving".to_owned());
            };
            if payload.state_store.engine != weaver_types::StoreEngine::Sqlite
                || payload.declaration != start.config_sha256.as_str()
                || state_management.start.dump_sha256 != start.dump_sha256
            {
                return Err("Enter store or artifact identity disagrees".to_owned());
            }
            if self.message.sink.is_none() || self.message.state.is_none() {
                return Err("Serving Enter requires trace and state descriptors".to_owned());
            }
            // Validate before consuming any handle. Lower constructors own and
            // close consumed handles on failure; source bytes/peer remain owed.
            state_management
                .flow
                .validate()
                .map_err(|error| error.to_string())?;
            let state = StateChannel::new(
                std::os::unix::net::UnixStream::from(self.message.state.take().unwrap()),
                payload.session.0.clone(),
                payload.run.0.clone(),
            )
            .map_err(|error| format!("state descriptor: {error}"))?;
            RecordingContext::new(
                self.message.sink.take().unwrap(),
                state,
                payload.session.0.clone(),
                payload.run.0.clone(),
                payload.operator,
                state_management.flow.clone(),
                remaining_enter_ns,
                io,
            )
            .map_err(|error| error.0)
        })();
        match result {
            Ok(context) => {
                self.adopted_origin = Some(std::sync::Arc::clone(context.enter_origin()));
                Ok(AdoptedEnter {
                    source: self,
                    context,
                })
            }
            Err(detail) => Err(AdoptionFailure {
                source: self,
                detail,
            }),
        }
    }
}
impl AdoptedEnter {
    pub(in crate::flow) fn into_parts(
        self,
    ) -> (super::super::context::RecordingContext, ReceivedEnter) {
        (self.context, self.source)
    }
}

#[cfg(test)]
#[path = "pump_enter_tests.rs"]
mod tests;

impl ReceivedEnter {
    pub(in crate::flow) fn adopted_by(
        &self,
        context: &super::super::context::RecordingContext,
    ) -> bool {
        self.adopted_origin
            .as_ref()
            .is_some_and(|origin| std::sync::Arc::ptr_eq(origin, context.enter_origin()))
    }
}
/// Lend the actual source peer while keeping the immutable original payload.
/// A caller cannot replace that connection during a composed Enter stage.
pub(in crate::flow) fn with_enter<'a, T>(
    original: Option<&'a mut ReceivedEnter>,
    mut lifecycle: LifecyclePorts<'a>,
    operation: impl FnOnce(
        LifecyclePorts<'a>,
        Option<&'a weaver_types::state_flow::enter::EnterPayload>,
    ) -> T,
) -> Result<T, Failure> {
    let payload = if let Some(original) = original {
        if lifecycle.peer.is_some() {
            return Err(invalid_failure(
                "composed Enter received a substitute admin peer",
            ));
        }
        lifecycle.peer = Some(&mut original.peer);
        Some(original.message.envelope.enter())
    } else {
        None
    };
    Ok(operation(lifecycle, payload))
}

/// Probe the retained source only after the owning stage checks its deadline.
pub(in crate::flow) fn probe_enter_peer(lifecycle: &mut LifecyclePorts<'_>) -> Result<(), Failure> {
    lifecycle
        .peer
        .as_mut()
        .expect("original peer lent by with_enter")
        .check_terminal()
        .map(|_| ())
}
