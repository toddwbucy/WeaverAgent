//! Strict outer envelopes. Raw fields are never buffered through `serde_json::Value`.

use std::collections::BTreeMap;
use std::fmt;

use serde::de::{DeserializeOwned, Error, MapAccess, Visitor};
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::value::RawValue;

use super::{
    CaptureRefusalCode, ControlRefusalCode, DecimalU64, MemberObservationIdentity, PairFile,
    ReadLimits, ReadRefusalCode, SeedCreated, SeedRefusal, SeedRequest, SourceBlockId,
    StartIdentity, StateControl, StateControlAnswer, StateOperation, StateOperationObservation,
    StatementFile, ValidationError,
};

pub const STATE_FLOW_PROTOCOL: &str = "state-flow/1";
pub const STATE_SEED_PROTOCOL: &str = "state-seed/1";
pub const STATE_CONTENT_SCHEMA: &str = "state-content/1";

/// Exchange identity. Protocol and operation are fixed by the enclosing variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Common {
    pub session_id: String,
    pub run_id: String,
    pub request_id: DecimalU64,
}

impl Common {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.session_id.is_empty() || self.run_id.is_empty() || self.request_id.0 == 0 {
            return Err(ValidationError("empty serving identity or zero request ID"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StateRefusalCode {
    MalformedFiling,
    WrongRun,
    DuplicateSource,
    UnsupportedSchema,
    SchemeMismatch,
    InvalidBinding,
    InvalidEffect,
    WrongPhase,
    WriteFailed,
    UnknownPair,
    AlreadyTerminal,
    InvalidPair,
    StatementMissing,
    DuplicatePair,
    InvalidReadLimits,
    OpenFailed,
}

impl StateRefusalCode {
    /// A refusal must be legal for the outstanding request, not just a known word.
    pub fn allowed_for(self, operation: StateOperation) -> bool {
        use StateOperation::{File, FilePair, Open};
        use StateRefusalCode::*;
        match self {
            UnsupportedSchema | SchemeMismatch => matches!(operation, Open | File | FilePair),
            InvalidReadLimits | OpenFailed => operation == Open,
            MalformedFiling | WrongRun | DuplicateSource | InvalidBinding | InvalidEffect
            | WrongPhase | WriteFailed => matches!(operation, File | FilePair),
            UnknownPair | AlreadyTerminal | InvalidPair | StatementMissing | DuplicatePair => {
                operation == FilePair
            }
        }
    }
}

/// Serving wire variants. Receipt validation never grants transaction authority.
#[derive(Debug, Clone)]
pub enum StateFrame {
    Open {
        common: Common,
        schema_id: String,
        scheme_id: String,
        start: StartIdentity,
        read_limits: ReadLimits,
    },
    Opened {
        common: Common,
        schema_id: String,
        scheme_id: String,
        start: StartIdentity,
        read_limits: ReadLimits,
        member_observation: MemberObservationIdentity,
        observation: StateOperationObservation,
    },
    File {
        common: Common,
        filing: StatementFile,
    },
    FilePair {
        common: Common,
        filing: PairFile,
    },
    Filed {
        common: Common,
        block_id: SourceBlockId,
        observation: StateOperationObservation,
    },
    Refused {
        common: Common,
        code: StateRefusalCode,
        detail: String,
        block_id: Option<SourceBlockId>,
        observation: StateOperationObservation,
    },
    Control {
        common: Common,
        frame: StateControl,
    },
    ControlAnswer {
        common: Common,
        frame: StateControlAnswer,
        observation: StateOperationObservation,
    },
}

impl StateFrame {
    pub fn common(&self) -> &Common {
        match self {
            Self::Open { common, .. }
            | Self::Opened { common, .. }
            | Self::File { common, .. }
            | Self::FilePair { common, .. }
            | Self::Filed { common, .. }
            | Self::Refused { common, .. }
            | Self::Control { common, .. }
            | Self::ControlAnswer { common, .. } => common,
        }
    }

    pub fn op(&self) -> &'static str {
        match self {
            Self::Open { .. } => "open",
            Self::Opened { .. } => "opened",
            Self::File { .. } => "file",
            Self::FilePair { .. } => "file_pair",
            Self::Filed { .. } => "filed",
            Self::Refused { .. } => "refused",
            Self::Control { .. } => "control",
            Self::ControlAnswer { .. } => "control_answer",
        }
    }

    pub fn is_request(&self) -> bool {
        matches!(
            self,
            Self::Open { .. } | Self::File { .. } | Self::FilePair { .. } | Self::Control { .. }
        )
    }

    pub fn request_operation(&self) -> Option<StateOperation> {
        match self {
            Self::Open { .. } => Some(StateOperation::Open),
            Self::File { .. } => Some(StateOperation::File),
            Self::FilePair { .. } => Some(StateOperation::FilePair),
            Self::Control { frame, .. } => Some(frame.operation()),
            _ => None,
        }
    }

    pub fn observation(&self) -> Option<&StateOperationObservation> {
        match self {
            Self::Opened { observation, .. }
            | Self::Filed { observation, .. }
            | Self::Refused { observation, .. }
            | Self::ControlAnswer { observation, .. } => Some(observation),
            _ => None,
        }
    }

    fn validate_envelope(&self) -> Result<(), ValidationError> {
        self.common().validate()?;
        if matches!(self, Self::Open { .. } | Self::Opened { .. })
            && self.common().request_id.0 != 1
        {
            return Err(ValidationError("the open exchange must use request ID 1"));
        }
        Ok(())
    }

    /// Check a complete response before exposing its data to the caller.
    ///
    /// The channel owns sequencing and supplies the next observation ID. Capture
    /// continuations can reuse the exchange ID while advancing that observation ID.
    pub fn validate_response_to(
        &self,
        request: &Self,
        next_operation_id: DecimalU64,
    ) -> Result<(), ValidationError> {
        self.validate_envelope()?;
        request.validate_envelope()?;
        let operation = request
            .request_operation()
            .ok_or(ValidationError("expected an outstanding request"))?;
        if self.common() != request.common() {
            return Err(ValidationError(
                "response exchange identity differs from its request",
            ));
        }
        self.observation()
            .ok_or(ValidationError("request cannot answer a request"))?
            .validate(request.common().request_id, operation, next_operation_id)?;
        match (self, request) {
            (
                Self::Opened {
                    schema_id,
                    scheme_id,
                    start,
                    read_limits,
                    member_observation,
                    ..
                },
                Self::Open {
                    schema_id: expected_schema,
                    scheme_id: expected_scheme,
                    start: expected_start,
                    read_limits: expected_limits,
                    ..
                },
            ) => {
                if schema_id != expected_schema
                    || scheme_id != expected_scheme
                    || start != expected_start
                    || read_limits != expected_limits
                {
                    return Err(ValidationError(
                        "opened does not echo the complete accepted declaration",
                    ));
                }
                member_observation.validate_current_open()?;
            }
            (Self::Filed { block_id, .. }, Self::File { filing, .. })
                if block_id == &filing.block_id => {}
            (Self::Filed { block_id, .. }, Self::FilePair { filing, .. })
                if block_id == &filing.block_id => {}
            (Self::Refused { code, block_id, .. }, _) => {
                if !code.allowed_for(operation) {
                    return Err(ValidationError(
                        "refusal code is not allowed for the outstanding operation",
                    ));
                }
                let expected = match request {
                    Self::Open { .. } => None,
                    Self::File { filing, .. } => Some(&filing.block_id),
                    Self::FilePair { filing, .. } => Some(&filing.block_id),
                    _ => return Err(ValidationError("control refuses through control_answer")),
                };
                if block_id.as_ref() != expected {
                    return Err(ValidationError(
                        "refusal source identity differs from the request",
                    ));
                }
            }
            (Self::ControlAnswer { frame: answer, .. }, Self::Control { frame: request, .. }) => {
                answer.validate_for(request)?;
            }
            _ => {
                return Err(ValidationError(
                    "response kind or source does not match the request",
                ));
            }
        }
        Ok(())
    }
}

impl StateControl {
    pub fn operation(&self) -> StateOperation {
        match self {
            Self::Read { .. } => StateOperation::Read,
            Self::Next { .. } => StateOperation::Next,
            Self::Release { .. } => StateOperation::Release,
            Self::Shape {} => StateOperation::Shape,
            Self::Held {} => StateOperation::Held,
            Self::Position {} => StateOperation::Position,
            Self::Grants {} => StateOperation::Grants,
            Self::Snapshot { .. } => StateOperation::Snapshot,
            Self::Acknowledge { .. } => StateOperation::Acknowledge,
            Self::Abort { .. } => StateOperation::Abort,
            Self::ReleaseCapture { .. } => StateOperation::ReleaseCapture,
        }
    }
}

impl ControlRefusalCode {
    pub fn allowed_for(self, operation: StateOperation) -> bool {
        use StateOperation::*;
        let control = !matches!(operation, Open | File | FilePair);
        match self {
            Self::Read(ReadRefusalCode::AnswerTooLarge) => control,
            // Capture holding prevents every incompatible control, including reads.
            Self::Capture(CaptureRefusalCode::InvalidCaptureState) => control,
            Self::Capture(_) => {
                matches!(operation, Snapshot | Acknowledge | Abort | ReleaseCapture)
            }
            Self::Read(_) => matches!(
                operation,
                Read | Next | Release | Shape | Held | Position | Grants
            ),
        }
    }
}

impl StateControlAnswer {
    /// Match the response tag and all identities supplied by its request.
    /// Cursor assembly and publication checks remain with their owners.
    pub fn validate_for(&self, request: &StateControl) -> Result<(), ValidationError> {
        let matches = match (self, request) {
            (Self::ReadPage(page), StateControl::Read { .. }) => page.offset.0 == 0,
            (
                Self::ReadPage(page),
                StateControl::Next {
                    snapshot_id,
                    offset,
                },
            ) => page.snapshot_id == *snapshot_id && page.offset == *offset,
            (Self::Released { snapshot_id: got }, StateControl::Release { snapshot_id }) => {
                got == snapshot_id
            }
            (Self::Shape(_), StateControl::Shape {})
            | (Self::Held { .. }, StateControl::Held {})
            | (Self::Position(_), StateControl::Position {})
            | (Self::Grants { .. }, StateControl::Grants {}) => true,
            (
                Self::Snapshot {
                    capture_id: got, ..
                },
                StateControl::Snapshot { capture_id },
            )
            | (
                Self::Finished {
                    capture_id: got, ..
                },
                StateControl::Acknowledge { capture_id, .. },
            )
            | (Self::Aborted { capture_id: got }, StateControl::Abort { capture_id })
            | (
                Self::CaptureReleased { capture_id: got },
                StateControl::ReleaseCapture { capture_id },
            ) => got == capture_id,
            (Self::ControlRefused { code, .. }, _) => code.allowed_for(request.operation()),
            _ => false,
        };
        if !matches {
            return Err(ValidationError(
                "control answer does not match its outstanding operation",
            ));
        }
        Ok(())
    }
}

impl Serialize for StateFrame {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.validate_envelope()
            .map_err(serde::ser::Error::custom)?;
        let common = self.common();
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("protocol", STATE_FLOW_PROTOCOL)?;
        map.serialize_entry("session_id", &common.session_id)?;
        map.serialize_entry("run_id", &common.run_id)?;
        map.serialize_entry("request_id", &common.request_id)?;
        map.serialize_entry("op", self.op())?;
        match self {
            Self::Open {
                schema_id,
                scheme_id,
                start,
                read_limits,
                ..
            } => {
                map.serialize_entry("schema_id", schema_id)?;
                map.serialize_entry("scheme_id", scheme_id)?;
                map.serialize_entry("start", start)?;
                map.serialize_entry("read_limits", read_limits)?;
            }
            Self::Opened {
                schema_id,
                scheme_id,
                start,
                read_limits,
                member_observation,
                observation,
                ..
            } => {
                map.serialize_entry("schema_id", schema_id)?;
                map.serialize_entry("scheme_id", scheme_id)?;
                map.serialize_entry("start", start)?;
                map.serialize_entry("read_limits", read_limits)?;
                map.serialize_entry("member_observation", member_observation)?;
                map.serialize_entry("observation", observation)?;
            }
            Self::File { filing, .. } => map.serialize_entry("filing", filing)?,
            Self::FilePair { filing, .. } => map.serialize_entry("filing", filing)?,
            Self::Filed {
                block_id,
                observation,
                ..
            } => {
                map.serialize_entry("block_id", block_id)?;
                map.serialize_entry("observation", observation)?;
            }
            Self::Refused {
                code,
                detail,
                block_id,
                observation,
                ..
            } => {
                map.serialize_entry("code", code)?;
                map.serialize_entry("detail", detail)?;
                map.serialize_entry("block_id", block_id)?;
                map.serialize_entry("observation", observation)?;
            }
            Self::Control { frame, .. } => map.serialize_entry("frame", frame)?,
            Self::ControlAnswer {
                frame, observation, ..
            } => {
                map.serialize_entry("frame", frame)?;
                map.serialize_entry("observation", observation)?;
            }
        }
        map.end()
    }
}

/// A decoded frame and the exact owned outer RawValue bytes materialized by
/// its strict decoder. This is one copy category, not a complete operation total.
/// None means the checked sum overflowed. No failed decode produces an answer.
#[derive(Debug)]
pub struct DecodedStateFrame {
    pub frame: StateFrame,
    pub raw_field_bytes: Option<u64>,
}

impl<'de> Deserialize<'de> for DecodedStateFrame {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let fields = RawFields::deserialize(deserializer)?;
        let raw_field_bytes = fields.0.values().try_fold(0_u64, |sum, value| {
            sum.checked_add(u64::try_from(value.get().len()).ok()?)
        });
        let frame = StateFrame::from_fields(fields)?;
        Ok(Self {
            frame,
            raw_field_bytes,
        })
    }
}

impl<'de> Deserialize<'de> for StateFrame {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::from_fields(RawFields::deserialize(deserializer)?)
    }
}

impl StateFrame {
    fn from_fields<E: Error>(mut fields: RawFields) -> Result<Self, E> {
        let protocol: String = fields.take("protocol")?;
        if protocol != STATE_FLOW_PROTOCOL {
            return Err(E::custom("unknown serving protocol"));
        }
        let common = Common {
            session_id: fields.take("session_id")?,
            run_id: fields.take("run_id")?,
            request_id: fields.take("request_id")?,
        };
        let op: String = fields.take("op")?;
        fields.require_keys(match op.as_str() {
            "open" => &["schema_id", "scheme_id", "start", "read_limits"],
            "opened" => &[
                "schema_id",
                "scheme_id",
                "start",
                "read_limits",
                "member_observation",
                "observation",
            ],
            "file" | "file_pair" => &["filing"],
            "filed" => &["block_id", "observation"],
            "refused" => &["code", "detail", "block_id", "observation"],
            "control" => &["frame"],
            "control_answer" => &["frame", "observation"],
            _ => return Err(E::custom("unknown serving operation")),
        })?;
        let result = match op.as_str() {
            "open" => Self::Open {
                common,
                schema_id: fields.take("schema_id")?,
                scheme_id: fields.take("scheme_id")?,
                start: fields.take("start")?,
                read_limits: fields.take("read_limits")?,
            },
            "opened" => Self::Opened {
                common,
                schema_id: fields.take("schema_id")?,
                scheme_id: fields.take("scheme_id")?,
                start: fields.take("start")?,
                read_limits: fields.take("read_limits")?,
                member_observation: fields.take("member_observation")?,
                observation: fields.take("observation")?,
            },
            "file" => Self::File {
                common,
                filing: fields.take("filing")?,
            },
            "file_pair" => Self::FilePair {
                common,
                filing: fields.take("filing")?,
            },
            "filed" => Self::Filed {
                common,
                block_id: fields.take("block_id")?,
                observation: fields.take("observation")?,
            },
            "refused" => Self::Refused {
                common,
                code: fields.take("code")?,
                detail: fields.take("detail")?,
                block_id: fields.take("block_id")?,
                observation: fields.take("observation")?,
            },
            "control" => Self::Control {
                common,
                frame: fields.take("frame")?,
            },
            "control_answer" => Self::ControlAnswer {
                common,
                frame: fields.take("frame")?,
                observation: fields.take("observation")?,
            },
            _ => return Err(E::custom("unknown serving operation")),
        };
        fields.finish()?;
        result.validate_envelope().map_err(E::custom)?;
        Ok(result)
    }
}

/// Pre-agent seed exchange. It cannot carry a serving session or observation.
#[derive(Debug, Clone)]
pub enum SeedFrame {
    Seed { request: Box<SeedRequest> },
    Created { created: SeedCreated },
    Refused { code: SeedRefusal, detail: String },
}

impl SeedFrame {
    pub fn is_request(&self) -> bool {
        matches!(self, Self::Seed { .. })
    }

    pub fn op(&self) -> &'static str {
        match self {
            Self::Seed { .. } => "seed",
            Self::Created { .. } => "created",
            Self::Refused { .. } => "refused",
        }
    }

    pub fn validate_response_to(&self, request: &SeedRequest) -> Result<(), ValidationError> {
        match self {
            Self::Created { created } => {
                if created.schema_id != request.schema_id || created.seed_id != request.seed_id {
                    return Err(ValidationError(
                        "created image differs from its seed request",
                    ));
                }
                Ok(())
            }
            Self::Refused { .. } => Ok(()),
            Self::Seed { .. } => Err(ValidationError("a seed request is not a response")),
        }
    }
}

impl Serialize for SeedFrame {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("protocol", STATE_SEED_PROTOCOL)?;
        map.serialize_entry("request_id", &DecimalU64(1))?;
        map.serialize_entry("op", self.op())?;
        match self {
            Self::Seed { request } => map.serialize_entry("request", request)?,
            Self::Created { created } => map.serialize_entry("created", created)?,
            Self::Refused { code, detail } => {
                map.serialize_entry("code", code)?;
                map.serialize_entry("detail", detail)?;
            }
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for SeedFrame {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut fields = RawFields::deserialize(deserializer)?;
        let protocol: String = fields.take("protocol")?;
        let request_id: DecimalU64 = fields.take("request_id")?;
        if protocol != STATE_SEED_PROTOCOL || request_id.0 != 1 {
            return Err(D::Error::custom("unknown seed protocol or exchange ID"));
        }
        let op: String = fields.take("op")?;
        fields.require_keys(match op.as_str() {
            "seed" => &["request"],
            "created" => &["created"],
            "refused" => &["code", "detail"],
            _ => return Err(D::Error::custom("unknown seed operation")),
        })?;
        let result = match op.as_str() {
            "seed" => Self::Seed {
                request: fields.take("request")?,
            },
            "created" => Self::Created {
                created: fields.take("created")?,
            },
            "refused" => Self::Refused {
                code: fields.take("code")?,
                detail: fields.take("detail")?,
            },
            _ => return Err(D::Error::custom("unknown seed operation")),
        };
        fields.finish()?;
        Ok(result)
    }
}

/// Keep raw slices until the full map has closed. Every value is scanned as original
/// JSON so direct Deserialize also rejects duplicates within an opaque payload.
struct RawFields(BTreeMap<String, Box<RawValue>>);

impl RawFields {
    fn require_keys<E: Error>(&self, names: &[&'static str]) -> Result<(), E> {
        for name in names {
            if !self.0.contains_key(*name) {
                return Err(E::missing_field(name));
            }
        }
        if self.0.len() != names.len() {
            return Err(E::custom("unexpected frame member"));
        }
        Ok(())
    }

    fn take<T: DeserializeOwned, E: Error>(&mut self, name: &'static str) -> Result<T, E> {
        let raw = self.0.remove(name).ok_or_else(|| E::missing_field(name))?;
        serde_json::from_str(raw.get()).map_err(E::custom)
    }

    fn finish<E: Error>(self) -> Result<(), E> {
        if self.0.is_empty() {
            Ok(())
        } else {
            Err(E::custom("unexpected frame member"))
        }
    }
}

impl<'de> Deserialize<'de> for RawFields {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct FieldsVisitor;
        impl<'de> Visitor<'de> for FieldsVisitor {
            type Value = RawFields;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a closed frame object")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut fields = BTreeMap::new();
                while let Some(key) = map.next_key::<String>()? {
                    if fields.contains_key(&key) {
                        return Err(A::Error::custom("duplicate frame member"));
                    }
                    let value: Box<RawValue> = map.next_value()?;
                    super::reject_duplicate_keys(value.get()).map_err(A::Error::custom)?;
                    fields.insert(key, value);
                }
                Ok(RawFields(fields))
            }
        }
        deserializer.deserialize_map(FieldsVisitor)
    }
}
