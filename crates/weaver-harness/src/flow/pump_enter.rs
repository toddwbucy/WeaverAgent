//! Own the exact replacement Enter and its ancillary handles before adoption.
//! Shares the existing recvmsg/custody implementation; no active listener selects it.
use super::*;
use weaver_types::state_flow::enter::EnterEnvelope;

pub(in crate::flow) struct EnterReceiver {
    peer: LifecyclePeer,
    held: Option<LifecycleMessage<EnterEnvelope>>,
    received_ns: Option<u64>,
}
pub(in crate::flow) struct ReceivedEnter {
    peer: LifecyclePeer,
    message: LifecycleMessage<EnterEnvelope>,
    wire: Vec<u8>,
    adopted_origin: Option<std::sync::Arc<()>>,
    received_ns: u64,
    observed_ns: std::cell::Cell<u64>,
    closed_unadmitted: Option<(bool, bool)>,
}

/// Complete original Enter evidence after its actual handles have closed.
pub(in crate::flow) struct RetiredEnter {
    _peer: RetiredLifecyclePeer,
    _envelope: EnterEnvelope,
    _wire: Vec<u8>,
    _had_sink: bool,
    _had_state: bool,
    _adopted_origin: Option<std::sync::Arc<()>>,
    _received_ns: u64,
    _observed_ns: u64,
    _closed_unadmitted: Option<(bool, bool)>,
}
#[cfg(test)]
impl RetiredEnter {
    pub(in crate::flow) fn retained_wire(&self) -> &[u8] {
        &self._wire
    }
}

/// The original received Enter after its authenticated peer moves into a
/// response owner. This keeps its bytes and descriptor history, not a new port.
pub(in crate::flow) struct DetachedEnter {
    message: LifecycleMessage<EnterEnvelope>,
    _wire: Vec<u8>,
    _adopted_origin: Option<std::sync::Arc<()>>,
    _received_ns: u64,
    _observed_ns: u64,
    _closed_unadmitted: Option<(bool, bool)>,
}
impl DetachedEnter {
    #[cfg(test)]
    pub(in crate::flow) fn retained_wire(&self) -> &[u8] {
        &self._wire
    }

    pub(in crate::flow) fn payload(&self) -> &weaver_types::state_flow::enter::EnterPayload {
        self.message.envelope.enter()
    }
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
        Ok(Self {
            peer,
            held: None,
            received_ns: None,
        })
    }
    pub(in crate::flow) fn advance(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<ReceiveProgress, Failure> {
        self.advance_with_decoder(io, EnterEnvelope::decode)
    }
    fn advance_with_decoder(
        &mut self,
        io: &mut impl PumpIo,
        decode: impl FnOnce(&[u8]) -> Result<EnterEnvelope, weaver_types::state_flow::enter::CodecError>,
    ) -> Result<ReceiveProgress, Failure> {
        if self.held.is_some() {
            // Keep completed input even when a later terminal observation fails.
            self.peer.check_terminal()?;
            return Ok(ReceiveProgress::Pending);
        }
        let message = self.peer.receive_message(|bytes| {
            // recvmsg has completed and every installed fd has an owner. Sample
            // before parsing; decode/dispatch delay must spend the same window.
            self.received_ns = Some(io.now().map_err(serde_json::Error::io)?);
            decode(bytes).map_err(|error| {
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
            received_ns: self
                .received_ns
                .expect("completed packet sampled before parse"),
            observed_ns: std::cell::Cell::new(self.received_ns.expect("received clock")),
            closed_unadmitted: None,
        })
    }
}
#[derive(Debug, PartialEq, Eq)]
pub(in crate::flow) enum EnterTimeError {
    Future,
    Expired,
    Clock(String),
}
impl ReceivedEnter {
    pub(in crate::flow) fn into_response_peer(self) -> (LifecyclePeer, DetachedEnter) {
        (
            self.peer,
            DetachedEnter {
                message: self.message,
                _wire: self.wire,
                _adopted_origin: self.adopted_origin,
                _received_ns: self.received_ns,
                _observed_ns: self.observed_ns.get(),
                _closed_unadmitted: self.closed_unadmitted,
            },
        )
    }
    pub(in crate::flow) fn check_window(&self, io: &mut impl PumpIo) -> Result<(), EnterTimeError> {
        let weaver_types::state_flow::enter::EnterBinding::Serving { enter_window, .. } =
            &self.envelope().enter().binding
        else {
            return Ok(());
        };
        if enter_window.began_ns.0 > self.received_ns {
            return Err(EnterTimeError::Future);
        }
        let now = io.now().map_err(|e| EnterTimeError::Clock(e.to_string()))?;
        if now < self.observed_ns.get() {
            return Err(EnterTimeError::Clock(
                "clock reversed after Enter receipt".into(),
            ));
        }
        self.observed_ns.set(now);
        if now >= enter_window.deadline_ns.0 {
            return Err(EnterTimeError::Expired);
        }
        Ok(())
    }
    pub(in crate::flow) fn close_unadmitted(&mut self) {
        if self.closed_unadmitted.is_none() {
            self.closed_unadmitted =
                Some((self.message.sink.is_some(), self.message.state.is_some()));
            drop(self.message.sink.take());
            drop(self.message.state.take());
        }
    }
    pub(in crate::flow) fn retire_failed(self) -> RetiredEnter {
        RetiredEnter {
            _peer: self.peer.retire_failed(),
            _envelope: self.message.envelope,
            _wire: self.wire,
            _had_sink: self.message.sink.is_some(),
            _had_state: self.message.state.is_some(),
            _adopted_origin: self.adopted_origin,
            _received_ns: self.received_ns,
            _observed_ns: self.observed_ns.get(),
            _closed_unadmitted: self.closed_unadmitted,
        }
    }
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
        io: &mut impl PumpIo,
    ) -> Result<AdoptedEnter, AdoptionFailure> {
        use super::super::{channel::StateChannel, context::RecordingContext};
        use weaver_types::state_flow::enter::EnterBinding;
        if let Err(error) = self.check_window(io) {
            if error == EnterTimeError::Expired {
                self.close_unadmitted();
            }
            return Err(AdoptionFailure {
                source: self,
                detail: format!("Enter window: {error:?}"),
            });
        }
        let result = (|| {
            self.peer
                .check_terminal()
                .map_err(|error| format!("Enter peer: {error:?}"))?;
            let payload = self.message.envelope.enter();
            let EnterBinding::Serving {
                state_management,
                start,
                enter_window,
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
                super::super::clock::AbsoluteBound::from_absolute(
                    enter_window.began_ns.0,
                    enter_window.deadline_ns.0,
                )
                .map_err(|error| error.to_string())?,
                io,
            )
            .map_err(|error| error.0)
        })();
        match result {
            Ok(context) => {
                if context.origin_ns() < self.observed_ns.get() {
                    return Err(AdoptionFailure {
                        source: self,
                        detail: "Enter clock reversed during context construction".into(),
                    });
                }
                self.observed_ns.set(context.origin_ns());
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

impl ReceivedEnter {
    pub(in crate::flow) fn peer(&self) -> &LifecyclePeer {
        &self.peer
    }
    pub(in crate::flow) fn peer_mut(&mut self) -> &mut LifecyclePeer {
        &mut self.peer
    }
    /// The response exchange comes from the retained original, never a caller.
    pub(in crate::flow) fn ready_envelope(&self) -> OrganEnvelope {
        OrganEnvelope {
            exchange: self.message.envelope.exchange.clone(),
            position: weaver_types::Position::Close,
            payload: weaver_types::Payload::Answer(weaver_types::LifecycleAnswer::Ready),
        }
    }
}
