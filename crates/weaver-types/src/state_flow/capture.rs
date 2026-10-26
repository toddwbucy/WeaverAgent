//! The paired serving capture projection, held for FOUNDATION-TRANSITION.
//!
//! The codec checks representation and direction. The receiving actor owns the
//! original exchange, phase, hashes, loaded limits and absolute expiry checks.
//! Selecting this projection does not select replacement serving or grant a save.

use super::{
    DecimalU64, Digest, PersistencePhase, RetainedArtifact, SavePointReport, SaveWindow,
    SourcePosition, StartIdentity,
};
use crate::{ExchangeId, Opener, Position, RunId};
use serde::{Deserialize, Serialize};
use std::fmt;

/// The party carrying a packet, independent of the exchange's original opener.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureSender {
    Admin,
    Harness,
}

super::codec::closed_object! {
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CaptureEnvelope {
    #[serde(deserialize_with = "deserialize_exchange")]
    pub exchange: ExchangeId,
    pub position: Position,
    #[serde(deserialize_with = "super::codec::deserialize_object")]
    pub payload: CapturePayload,
}
}

fn deserialize_exchange<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<ExchangeId, D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Fields {
        opener: Opener,
        ordinal: u64,
    }
    let Fields { opener, ordinal } = super::codec::deserialize_object(deserializer)?;
    Ok(ExchangeId { opener, ordinal })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "body",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum CapturePayload {
    SavePointBeginning(CaptureBeginning),
    SavePointImage(CaptureImage),
    SavePointImageChecked(CaptureImageChecked),
    SavePointImageFinished(CaptureImageFinished),
    SavePointPrepared(CaptureArtifact),
    SavePointCommit(CaptureIdentity),
    SavePointPublished(CaptureArtifact),
    SavePointCaptureComplete(CaptureIdentity),
    SavePointAborted(CaptureAborted),
    Answer(CaptureAnswer),
    Refusal(CaptureRefusal),
}

super::codec::closed_object! {
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CaptureBeginning {
    pub capture_id: DecimalU64,
    pub window: SaveWindow,
    #[serde(deserialize_with = "super::required_option")]
    pub tail_window: Option<SaveWindow>,
}
}
super::codec::closed_object! {
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CaptureImage {
    pub capture_id: DecimalU64,
    pub room_name: String,
    pub size_bytes: DecimalU64,
    #[serde(deserialize_with = "super::required_option")]
    pub position: Option<SourcePosition>,
}
}
super::codec::closed_object! {
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CaptureImageChecked {
    pub capture_id: DecimalU64,
    pub dump_sha256: Digest,
}
}
super::codec::closed_object! {
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CaptureImageFinished {
    pub capture_id: DecimalU64,
    pub room_name: String,
}
}
super::codec::closed_object! {
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CaptureArtifact {
    pub capture_id: DecimalU64,
    pub artifact_id: String,
    pub dump_sha256: Digest,
    pub config_sha256: Digest,
}
}
super::codec::closed_object! {
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CaptureIdentity {
    pub capture_id: DecimalU64,
}
}
super::codec::closed_object! {
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CaptureAborted {
    pub capture_id: DecimalU64,
    pub code: String,
    pub detail: String,
}
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CaptureAnswer {
    Left {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        save_point: Option<SavePointReport>,
        forced: bool,
        no_state: bool,
    },
    SavePointTaken {
        report: SavePointReport,
    },
}

/// Replacement serving legs. Component death is a failed run, not a missed save.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SavePointLeg {
    Answer,
    Finished,
    Published,
    CaptureNotReady,
}

/// The replacement refusal shares the existing lifecycle tag and common refusals.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CaptureRefusal {
    Save(SaveRefusal),
    Lifecycle(crate::LifecycleRefusal),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SaveRefusal {
    SavePointNotTaken {
        missed: SavePointLeg,
    },
    FailedRun {
        run: RunId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        capture_id: Option<DecimalU64>,
        phase: PersistencePhase,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        published: Option<Box<StartIdentity>>,
        artifacts: Vec<RetainedArtifact>,
        detail: String,
    },
}

#[derive(Debug)]
pub enum CaptureCodecError {
    TooLarge,
    Malformed(serde_json::Error),
    WrongExchange,
    WrongDirection,
    InvalidShape(&'static str),
}
impl fmt::Display for CaptureCodecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge => formatter.write_str("capture envelope exceeds the packet bound"),
            Self::Malformed(error) => write!(formatter, "malformed capture envelope: {error}"),
            Self::WrongExchange => formatter.write_str("wrong capture exchange or position"),
            Self::WrongDirection => formatter.write_str("wrong capture sender"),
            Self::InvalidShape(detail) => formatter.write_str(detail),
        }
    }
}
impl std::error::Error for CaptureCodecError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Malformed(error) => Some(error),
            _ => None,
        }
    }
}

impl CaptureEnvelope {
    pub fn capture_id(&self) -> Option<DecimalU64> {
        match &self.payload {
            CapturePayload::SavePointBeginning(value) => Some(value.capture_id),
            CapturePayload::SavePointImage(value) => Some(value.capture_id),
            CapturePayload::SavePointImageChecked(value) => Some(value.capture_id),
            CapturePayload::SavePointImageFinished(value) => Some(value.capture_id),
            CapturePayload::SavePointPrepared(value)
            | CapturePayload::SavePointPublished(value) => Some(value.capture_id),
            CapturePayload::SavePointCommit(value)
            | CapturePayload::SavePointCaptureComplete(value) => Some(value.capture_id),
            CapturePayload::SavePointAborted(value) => Some(value.capture_id),
            CapturePayload::Answer(CaptureAnswer::Left { save_point, .. }) => {
                save_point.as_ref().map(|value| value.capture_id)
            }
            CapturePayload::Answer(CaptureAnswer::SavePointTaken { report }) => {
                Some(report.capture_id)
            }
            CapturePayload::Refusal(CaptureRefusal::Save(SaveRefusal::FailedRun {
                capture_id,
                ..
            })) => *capture_id,
            CapturePayload::Refusal(_) => None,
        }
    }

    fn check(&self, sender: CaptureSender) -> Result<(), CaptureCodecError> {
        use CapturePayload::*;
        let (position, direction) = match &self.payload {
            SavePointBeginning(_)
            | SavePointImage(_)
            | SavePointImageFinished(_)
            | SavePointCommit(_)
            | SavePointCaptureComplete(_) => (Position::Continue, Some(CaptureSender::Harness)),
            SavePointImageChecked(_) | SavePointPrepared(_) | SavePointPublished(_) => {
                (Position::Continue, Some(CaptureSender::Admin))
            }
            SavePointAborted(_) => (Position::Continue, None),
            Answer(_) | Refusal(_) => (Position::Close, Some(CaptureSender::Harness)),
        };
        if self.exchange.opener != Opener::Admin || self.position != position {
            return Err(CaptureCodecError::WrongExchange);
        }
        if direction.is_some_and(|expected| sender != expected) {
            return Err(CaptureCodecError::WrongDirection);
        }
        if self.capture_id().is_some_and(|id| id.0 == 0) {
            return Err(CaptureCodecError::InvalidShape(
                "capture_id must be positive",
            ));
        }
        match &self.payload {
            SavePointBeginning(value) => {
                value
                    .window
                    .validate()
                    .map_err(|_| CaptureCodecError::InvalidShape("invalid capture window"))?;
                if let Some(tail) = value.tail_window {
                    tail.validate()
                        .map_err(|_| CaptureCodecError::InvalidShape("invalid leave window"))?;
                }
            }
            Answer(CaptureAnswer::Left {
                save_point: Some(_),
                no_state: true,
                ..
            }) => {
                return Err(CaptureCodecError::InvalidShape(
                    "a no-state leave cannot carry a save report",
                ));
            }
            Refusal(CaptureRefusal::Lifecycle(refusal)) if contains_legacy_save(refusal) => {
                return Err(CaptureCodecError::InvalidShape(
                    "capture requires the replacement save refusal",
                ));
            }
            _ => {}
        }
        Ok(())
    }

    /// Bound the entire escaped packet before any send can be staged.
    pub fn encode(&self, sender: CaptureSender) -> Result<Vec<u8>, CaptureCodecError> {
        self.check(sender)?;
        let bytes = serde_json::to_vec(self).map_err(CaptureCodecError::Malformed)?;
        if bytes.len() > crate::MAX_ENVELOPE_BYTES {
            return Err(CaptureCodecError::TooLarge);
        }
        Ok(bytes)
    }

    pub fn decode(bytes: &[u8], sender: CaptureSender) -> Result<Self, CaptureCodecError> {
        if bytes.len() > crate::MAX_ENVELOPE_BYTES {
            return Err(CaptureCodecError::TooLarge);
        }
        let text = std::str::from_utf8(bytes).map_err(|error| {
            CaptureCodecError::Malformed(serde_json::Error::io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                error,
            )))
        })?;
        let value: Self = super::decode_strict(text).map_err(CaptureCodecError::Malformed)?;
        value.check(sender)?;
        Ok(value)
    }
}

fn contains_legacy_save(refusal: &crate::LifecycleRefusal) -> bool {
    match refusal {
        crate::LifecycleRefusal::SavePointNotTaken { .. } => true,
        crate::LifecycleRefusal::OrganRefused { reason, .. } => contains_legacy_save(reason),
        _ => false,
    }
}

#[cfg(test)]
#[path = "capture_tests.rs"]
mod tests;
