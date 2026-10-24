//! Paired replacement observation, selected only with replacement Enter.
use super::{CodecError, LoadedStateFacts};
use crate::{AgentState, ExchangeId, Opener, Position};
use serde::{Deserialize, Serialize};

super::super::codec::closed_object! {
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LoadFacts {
    pub session: crate::SessionId,
    pub run: crate::RunId,
    pub declaration: String,
    pub artifact: crate::ArtifactRef,
    pub residual_readout: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<u32>,
    pub surprisal: bool,
    pub state: LoadedStateFacts,
    pub state_store: crate::config::StateStore,
    pub state_member: bool,
    pub composer: crate::Composer,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drain_bound: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wind_down_bound: Option<u64>,
}
}
super::super::codec::closed_object! {
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ObservationEnvelope {
    pub exchange: ExchangeId,
    pub position: Position,
    pub payload: ObservationContent,
}
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "body",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum ObservationContent {
    Answer(ObservationAnswer),
    Refusal(crate::LifecycleRefusal),
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ObservationAnswer {
    State {
        state: AgentState,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        load: Option<Box<LoadFacts>>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        constituents: Vec<u32>,
    },
    InTransition {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        constituents: Vec<u32>,
    },
}
impl ObservationEnvelope {
    fn check(&self) -> Result<(), CodecError> {
        if self.exchange.opener != Opener::Admin || self.position != Position::Close {
            return Err(CodecError::WrongExchange);
        }
        if let ObservationContent::Answer(ObservationAnswer::State { state, load, .. }) =
            &self.payload
        {
            // This is Observe, not the load verb's Idle/no-facts answer.
            if matches!(state, AgentState::Idle | AgentState::Active) != load.is_some() {
                return Err(CodecError::InvalidLoadedState);
            }
            if let Some(load) = load
                && matches!(load.state, LoadedStateFacts::Serving { .. })
                && (!load.state_member
                    || load.state_store.engine != crate::config::StoreEngine::Sqlite)
            {
                return Err(CodecError::InvalidLoadedState);
            }
        }
        Ok(())
    }
    pub fn encode(&self) -> Result<Vec<u8>, CodecError> {
        self.check()?;
        let bytes = serde_json::to_vec(self).map_err(CodecError::Malformed)?;
        if bytes.len() > crate::MAX_ENVELOPE_BYTES {
            return Err(CodecError::TooLarge);
        }
        Ok(bytes)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, CodecError> {
        if bytes.len() > crate::MAX_ENVELOPE_BYTES {
            return Err(CodecError::TooLarge);
        }
        let text = std::str::from_utf8(bytes).map_err(|error| {
            CodecError::Malformed(serde_json::Error::io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                error,
            )))
        })?;
        let value: Self = super::super::decode_strict(text).map_err(CodecError::Malformed)?;
        value.check()?;
        Ok(value)
    }
}

#[cfg(test)]
#[path = "enter_observation_tests.rs"]
mod tests;
