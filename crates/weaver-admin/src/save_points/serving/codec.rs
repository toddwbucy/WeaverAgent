//! Pure codecs for the bound-artifact repository, per PERSIST-ARTIFACT-CODEC.
//!
//! These values describe publication evidence. Decoding them grants no filesystem
//! access, capture confirmation, or permission to publish.

use std::collections::BTreeMap;
use std::fmt;

use serde_json::value::RawValue;
use weaver_types::state_flow::{
    Confirmation, DecimalU64, Digest, PersistenceCause, PersistencePhase, PublicationState,
    SourcePosition, decode_strict,
};

pub(super) const SCHEMA_ID: &str = "state-content/1";
pub(super) const GUARD_FORMAT: &str = "state-publication/1";

#[derive(Debug)]
pub(crate) enum Error {
    Invalid(String),
    Json(serde_json::Error),
    Utf8(std::str::Utf8Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(detail) => f.write_str(detail),
            Self::Json(error) => error.fmt(f),
            Self::Utf8(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Invalid(_) => None,
            Self::Json(error) => Some(error),
            Self::Utf8(error) => Some(error),
        }
    }
}

impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

impl From<std::str::Utf8Error> for Error {
    fn from(error: std::str::Utf8Error) -> Self {
        Self::Utf8(error)
    }
}

fn invalid(detail: impl Into<String>) -> Error {
    Error::Invalid(detail.into())
}

/// A directory basename: the positive creation ordinal and the exact digest B.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ArtifactId {
    text: String,
    ordinal: DecimalU64,
    digest: Digest,
}

impl ArtifactId {
    pub(crate) fn from_parts(ordinal: DecimalU64, digest: Digest) -> Result<Self, Error> {
        if ordinal.0 == 0 {
            return Err(invalid("artifact creation ordinal must be positive"));
        }
        Ok(Self {
            text: format!("{:020}-{digest}", ordinal.0),
            ordinal,
            digest,
        })
    }

    pub(crate) fn parse(text: &str) -> Result<Self, Error> {
        let bytes = text.as_bytes();
        if bytes.len() != 85 || bytes[20] != b'-' || !bytes[..20].iter().all(u8::is_ascii_digit) {
            return Err(invalid(
                "artifact ID requires 20 decimal digits, a hyphen and digest B",
            ));
        }
        let ordinal = text[..20]
            .parse::<u64>()
            .map_err(|_| invalid("artifact creation ordinal exceeds u64"))?;
        let digest = Digest::new(&text[21..]).map_err(|error| invalid(error.to_string()))?;
        Self::from_parts(DecimalU64(ordinal), digest)
    }

    /// The root selector permits one final LF and no other whitespace.
    pub(crate) fn parse_selector(bytes: &[u8]) -> Result<Self, Error> {
        let text = std::str::from_utf8(bytes.strip_suffix(b"\n").unwrap_or(bytes))?;
        Self::parse(text)
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.text
    }

    pub(crate) fn creation_ordinal(&self) -> DecimalU64 {
        self.ordinal
    }

    pub(crate) fn config_digest(&self) -> &Digest {
        &self.digest
    }
}

impl fmt::Display for ArtifactId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Every field is required, including explicit null parent and source frontier.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct StateBinding {
    pub(super) version: u32,
    pub(super) schema_id: String,
    pub(super) seed_id: String,
    pub(super) dump_sha256: Digest,
    pub(super) config_sha256: Digest,
    pub(super) parent_artifact_id: Option<ArtifactId>,
    pub(super) position: Option<SourcePosition>,
    pub(super) confirmation: Confirmation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Arrival {
    Published,
    Selected,
}

impl Arrival {
    fn parse(text: &str) -> Result<Self, Error> {
        match text {
            "published" => Ok(Self::Published),
            "selected" => Ok(Self::Selected),
            _ => Err(invalid("unknown artifact arrival")),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Published => "published",
            Self::Selected => "selected",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct StateIndexEntry {
    pub(super) ordinal: DecimalU64,
    pub(super) artifact_id: ArtifactId,
    pub(super) dump_sha256: Digest,
    pub(super) config_sha256: Digest,
    pub(super) seed_id: String,
    pub(super) parent_artifact_id: Option<ArtifactId>,
    pub(super) cause: PersistenceCause,
    pub(super) position: Option<SourcePosition>,
    pub(super) confirmation: Confirmation,
    pub(super) arrival: Arrival,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PublicationGuard {
    pub(super) format: String,
    pub(super) artifact_id: ArtifactId,
    pub(super) capture_id: DecimalU64,
    pub(super) state: PublicationState,
    pub(super) phase: PersistencePhase,
    pub(super) locations: Vec<String>,
}

// Keep original field slices until the recursive duplicate scan has succeeded.
// The local records deliberately need no serde dependency or shadow floor types.
struct Fields(BTreeMap<String, Box<RawValue>>);

impl Fields {
    fn from_json(bytes: &[u8]) -> Result<Self, Error> {
        Ok(Self(decode_strict(std::str::from_utf8(bytes)?)?))
    }

    fn take(&mut self, name: &str) -> Result<Box<RawValue>, Error> {
        self.0
            .remove(name)
            .ok_or_else(|| invalid(format!("missing artifact field {name}")))
    }

    fn finish(self) -> Result<(), Error> {
        if let Some(name) = self.0.keys().next() {
            return Err(invalid(format!("unknown artifact field {name}")));
        }
        Ok(())
    }
}

macro_rules! field {
    ($fields:ident, $name:literal, $ty:ty) => {
        serde_json::from_str::<$ty>($fields.take($name)?.get())?
    };
}

// Values are serialized directly, in the order written here. No Value/map round
// trip may reorder a nested SourcePosition or Confirmation before canonical output.
macro_rules! object {
    ($first:literal => $value:expr $(, $name:literal => $rest:expr)* $(,)?) => {{
        let mut bytes = Vec::from(concat!("{\"", $first, "\":").as_bytes());
        bytes.extend_from_slice(&serde_json::to_vec(&$value)?);
        $(
            bytes.extend_from_slice(concat!(",\"", $name, "\":").as_bytes());
            bytes.extend_from_slice(&serde_json::to_vec(&$rest)?);
        )*
        bytes.push(b'}');
        Ok(bytes)
    }};
}

fn parse_parent(parent: Option<String>) -> Result<Option<ArtifactId>, Error> {
    parent.as_deref().map(ArtifactId::parse).transpose()
}

fn validate_identity(
    seed_id: &str,
    parent: Option<&ArtifactId>,
    position: Option<&SourcePosition>,
    confirmation: &Confirmation,
) -> Result<(), Error> {
    if seed_id.len() != 32
        || !seed_id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(invalid("seed ID must be 32 lowercase hexadecimal bytes"));
    }
    if position.is_some_and(|position| position.block_id.run.is_empty()) {
        return Err(invalid("source frontier has an empty run identity"));
    }
    match confirmation {
        Confirmation::Seed { seed_id: confirmed } => {
            if confirmed != seed_id || parent.is_some() || position.is_some() {
                return Err(invalid(
                    "seed confirmation disagrees with seed, parent or position",
                ));
            }
        }
        Confirmation::Rebind { parent_artifact_id } => {
            let confirmed = ArtifactId::parse(parent_artifact_id)?;
            if parent != Some(&confirmed) {
                return Err(invalid(
                    "rebind confirmation must name the same nonnull parent",
                ));
            }
        }
        Confirmation::Serving { run_id, capture_id } => {
            if run_id.is_empty() || capture_id.0 == 0 {
                return Err(invalid(
                    "serving confirmation requires a run and positive capture ID",
                ));
            }
        }
    }
    Ok(())
}

impl StateBinding {
    pub(super) fn validate(&self) -> Result<(), Error> {
        if self.version != 1 || self.schema_id != SCHEMA_ID {
            return Err(invalid("unsupported state binding version or schema"));
        }
        validate_identity(
            &self.seed_id,
            self.parent_artifact_id.as_ref(),
            self.position.as_ref(),
            &self.confirmation,
        )
    }

    pub(super) fn from_json(bytes: &[u8]) -> Result<Self, Error> {
        let mut fields = Fields::from_json(bytes)?;
        let result = Self {
            version: field!(fields, "version", u32),
            schema_id: field!(fields, "schema_id", String),
            seed_id: field!(fields, "seed_id", String),
            dump_sha256: field!(fields, "dump_sha256", Digest),
            config_sha256: field!(fields, "config_sha256", Digest),
            parent_artifact_id: parse_parent(field!(fields, "parent_artifact_id", Option<String>))?,
            position: field!(fields, "position", Option<SourcePosition>),
            confirmation: field!(fields, "confirmation", Confirmation),
        };
        fields.finish()?;
        result.validate()?;
        Ok(result)
    }

    pub(super) fn to_json(&self) -> Result<Vec<u8>, Error> {
        self.validate()?;
        object! {
            "version" => self.version,
            "schema_id" => self.schema_id,
            "seed_id" => self.seed_id,
            "dump_sha256" => self.dump_sha256,
            "config_sha256" => self.config_sha256,
            "parent_artifact_id" => self.parent_artifact_id.as_ref().map(ArtifactId::as_str),
            "position" => self.position,
            "confirmation" => self.confirmation,
        }
    }
}

impl StateIndexEntry {
    pub(super) fn validate(&self) -> Result<(), Error> {
        if self.ordinal.0 == 0 || self.artifact_id.config_digest() != &self.config_sha256 {
            return Err(invalid("invalid index ordinal or artifact digest"));
        }
        if self.parent_artifact_id.as_ref() == Some(&self.artifact_id) {
            return Err(invalid("an artifact cannot be its own parent"));
        }
        match self.arrival {
            Arrival::Published if self.ordinal != self.artifact_id.creation_ordinal() => {
                return Err(invalid(
                    "publication ordinal differs from artifact creation ordinal",
                ));
            }
            Arrival::Selected if self.ordinal <= self.artifact_id.creation_ordinal() => {
                return Err(invalid("selection must follow artifact creation"));
            }
            _ => {}
        }
        validate_identity(
            &self.seed_id,
            self.parent_artifact_id.as_ref(),
            self.position.as_ref(),
            &self.confirmation,
        )?;
        let cause_matches = matches!(
            (&self.confirmation, self.cause),
            (Confirmation::Seed { .. }, PersistenceCause::Seed)
                | (Confirmation::Rebind { .. }, PersistenceCause::Rebind)
                | (
                    Confirmation::Serving { .. },
                    PersistenceCause::Requested
                        | PersistenceCause::GracefulUnload
                        | PersistenceCause::ForcedUnload
                )
        );
        if !cause_matches {
            return Err(invalid("publication cause disagrees with confirmation"));
        }
        Ok(())
    }

    pub(super) fn binding(&self) -> StateBinding {
        StateBinding {
            version: 1,
            schema_id: SCHEMA_ID.into(),
            seed_id: self.seed_id.clone(),
            dump_sha256: self.dump_sha256.clone(),
            config_sha256: self.config_sha256.clone(),
            parent_artifact_id: self.parent_artifact_id.clone(),
            position: self.position.clone(),
            confirmation: self.confirmation.clone(),
        }
    }

    pub(super) fn matches_binding(&self, binding: &StateBinding) -> bool {
        self.binding() == *binding
    }

    pub(super) fn from_json(bytes: &[u8]) -> Result<Self, Error> {
        let mut fields = Fields::from_json(bytes)?;
        let result = Self {
            ordinal: field!(fields, "ordinal", DecimalU64),
            artifact_id: ArtifactId::parse(&field!(fields, "artifact_id", String))?,
            dump_sha256: field!(fields, "dump_sha256", Digest),
            config_sha256: field!(fields, "config_sha256", Digest),
            seed_id: field!(fields, "seed_id", String),
            parent_artifact_id: parse_parent(field!(fields, "parent_artifact_id", Option<String>))?,
            cause: field!(fields, "cause", PersistenceCause),
            position: field!(fields, "position", Option<SourcePosition>),
            confirmation: field!(fields, "confirmation", Confirmation),
            arrival: Arrival::parse(&field!(fields, "arrival", String))?,
        };
        fields.finish()?;
        result.validate()?;
        Ok(result)
    }

    pub(super) fn to_json(&self) -> Result<Vec<u8>, Error> {
        self.validate()?;
        object! {
            "ordinal" => self.ordinal,
            "artifact_id" => self.artifact_id.as_str(),
            "dump_sha256" => self.dump_sha256,
            "config_sha256" => self.config_sha256,
            "seed_id" => self.seed_id,
            "parent_artifact_id" => self.parent_artifact_id.as_ref().map(ArtifactId::as_str),
            "cause" => self.cause,
            "position" => self.position,
            "confirmation" => self.confirmation,
            "arrival" => self.arrival.as_str(),
        }
    }
}

/// Check the whole append-only stream, never accepting a valid earlier prefix.
pub(super) fn validate_index(entries: &[StateIndexEntry]) -> Result<(), Error> {
    let mut last = 0_u64;
    let mut publications = BTreeMap::new();
    for entry in entries {
        entry.validate()?;
        let next = last
            .checked_add(1)
            .ok_or_else(|| invalid("index ordinal exhausted"))?;
        if entry.ordinal.0 != next {
            return Err(invalid(
                "index ordinals must start at one and remain contiguous",
            ));
        }
        match entry.arrival {
            Arrival::Published => {
                if publications.insert(&entry.artifact_id, entry).is_some() {
                    return Err(invalid("duplicate artifact publication"));
                }
            }
            Arrival::Selected => {
                let original = publications
                    .get(&entry.artifact_id)
                    .ok_or_else(|| invalid("selection names an unpublished artifact"))?;
                if entry.cause != original.cause || entry.binding() != original.binding() {
                    return Err(invalid("selection changes the original artifact identity"));
                }
            }
        }
        last = next;
    }
    Ok(())
}

pub(super) fn decode_index(bytes: &[u8]) -> Result<Vec<StateIndexEntry>, Error> {
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    let complete = bytes
        .strip_suffix(b"\n")
        .ok_or_else(|| invalid("artifact index ends in an incomplete record"))?;
    let entries = complete
        .split(|byte| *byte == b'\n')
        .map(StateIndexEntry::from_json)
        .collect::<Result<Vec<_>, _>>()?;
    validate_index(&entries)?;
    Ok(entries)
}

impl PublicationGuard {
    pub(super) fn validate(&self) -> Result<(), Error> {
        if self.format != GUARD_FORMAT || self.capture_id.0 == 0 {
            return Err(invalid("invalid publication guard format or capture ID"));
        }
        for name in &self.locations {
            if name.is_empty()
                || matches!(name.as_str(), "." | "..")
                || name.contains('/')
                || name.contains('\0')
            {
                return Err(invalid("guard locations must be relative basenames"));
            }
        }
        Ok(())
    }

    pub(super) fn from_json(bytes: &[u8]) -> Result<Self, Error> {
        let mut fields = Fields::from_json(bytes)?;
        let result = Self {
            format: field!(fields, "format", String),
            artifact_id: ArtifactId::parse(&field!(fields, "artifact_id", String))?,
            capture_id: field!(fields, "capture_id", DecimalU64),
            state: field!(fields, "state", PublicationState),
            phase: field!(fields, "phase", PersistencePhase),
            locations: field!(fields, "locations", Vec<String>),
        };
        fields.finish()?;
        result.validate()?;
        Ok(result)
    }

    pub(super) fn to_json(&self) -> Result<Vec<u8>, Error> {
        self.validate()?;
        object! {
            "format" => self.format,
            "artifact_id" => self.artifact_id.as_str(),
            "capture_id" => self.capture_id,
            "state" => self.state,
            "phase" => self.phase,
            "locations" => self.locations,
        }
    }
}

#[cfg(test)]
mod tests;
