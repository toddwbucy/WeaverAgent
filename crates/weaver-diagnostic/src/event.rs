//! conforms: diagnostic-kind-set-exhaustive
//! conforms: diagnostic-readout-rides-the-measurement
//!
//! The envelope, the kind set, and the payload shapes, per
//! `weaver-diagnostic-Spec` section 3.
//!
//! **The envelope is the serving envelope's, field for field**, per section
//! 3.1, and the shapes follow `weaver-trace-Spec` section 3 under G5 without
//! linking their author: the two records share a form and not a type, per
//! section 1's no-trace-dependency election. Field order is declaration order
//! and declaration order here matches the serving declaration exactly, which
//! is what the canonical byte-comparison watches.

use serde::{Serialize, Serializer};
use serde_json::value::RawValue;

/// The identity and ordering fields every event carries, flattened so `kind`
/// sits at the top level of every line where a consumer keys on it.
///
/// `turn` and `causal_parent` are optional and nothing else is, per
/// `weaver-trace-Spec` section 3: an event belonging to no turn carries none,
/// and the recorder never infers one. The absent options emit no member.
#[derive(Debug, Clone, Serialize)]
pub struct Envelope {
    pub session: SessionRef,
    pub run: RunRef,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn: Option<TurnRef>,
    pub sequence: Sequence,
    pub kind: Kind,
    pub subsystem: Subsystem,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub causal_parent: Option<Sequence>,
    pub wall_ms: u64,
    pub monotonic_ns: MonotonicNs,
}

/// One event: the envelope's members and the payload's in one object.
///
/// A kind that carries no payload emits no `payload` member at all rather
/// than a null one. No field added to [`Envelope`] may be named `payload`,
/// the flatten putting both layers' members in one object.
#[derive(Debug, Clone, Serialize)]
pub struct Event {
    #[serde(flatten)]
    pub envelope: Envelope,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<Payload>,
}

/// The nineteen kinds, exhaustive, per section 3.2. A kind added beyond
/// this set breaks every consumer's match, which is the compile pin.
///
/// Fifteen spellings are the serving vocabulary's and mean there what they
/// mean here. Four are this record's own, the `replay.` trio and
/// `residual.column`, and no serving record carries any of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Kind {
    #[serde(rename = "replay.opened")]
    ReplayOpened,
    #[serde(rename = "replay.identity")]
    ReplayIdentity,
    #[serde(rename = "replay.closed")]
    ReplayClosed,
    #[serde(rename = "residual.column")]
    ResidualColumn,
    #[serde(rename = "turn.started")]
    TurnStarted,
    #[serde(rename = "turn.closed")]
    TurnClosed,
    #[serde(rename = "message.system")]
    MessageSystem,
    #[serde(rename = "message.user")]
    MessageUser,
    #[serde(rename = "message.assistant")]
    MessageAssistant,
    #[serde(rename = "message.tool_result")]
    MessageToolResult,
    #[serde(rename = "model.request")]
    ModelRequest,
    #[serde(rename = "model.output")]
    ModelOutput,
    #[serde(rename = "model.measurement")]
    ModelMeasurement,
    #[serde(rename = "model.field")]
    ModelField,
    #[serde(rename = "flush")]
    Flush,
    #[serde(rename = "refusal")]
    Refusal,
    #[serde(rename = "fault")]
    Fault,
    /// An answered state-seam ask, the serving vocabulary's `recall`: the
    /// replay port authors one for the holdings it answered, inside the
    /// bracket. The replay's enter records nothing, as it records no load.
    #[serde(rename = "recall")]
    Recall,
    /// A span of the resident removed, the serving vocabulary's `elision`:
    /// the replay reproduces a source's elision where it fell, as it does a
    /// flush, so its record carries the act for the flush's reason.
    #[serde(rename = "elision")]
    Elision,
}

/// What an event carries beside its envelope. Untagged: the envelope's
/// `kind` is the discriminant, and admission enforces the kind-to-payload
/// pairing, per section 3.3's total mapping.
///
/// **Fifteen kinds splice the serving payload of the same name**, arriving
/// pre-rendered from the harness and carried verbatim, which is what the
/// `raw_value` dependency buys and why this crate re-encodes nothing. The
/// residual readout rides the spliced `model.measurement` exactly where a
/// serving record puts it, per section 3.4: density differs between the two
/// records and shape does not.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum Payload {
    ReplayOpened(ReplayOpened),
    ReplayIdentity(ReplayIdentity),
    ReplayClosed(ReplayClosed),
    ResidualColumn(ResidualColumn),
    Spliced(Box<RawValue>),
}

/// The pass's bracket opens. Carries only what the load declared, per
/// section 3.3: the provenance of the record under replay rides
/// `replay.identity` once step one has established it, never this event.
#[derive(Debug, Clone, Serialize)]
pub struct ReplayOpened {
    pub reader_elected: bool,
}

/// The input identity the pass established: read and checked, not merely
/// read. A pass that refused its holdings authors none of these.
#[derive(Debug, Clone, Serialize)]
pub struct ReplayIdentity {
    pub replayed_session: String,
    pub model: ModelId,
    pub weights_hash: WeightsHash,
    pub template: TemplateId,
}

/// The pass's bracket closes, carrying its outcome. A pass that died authors
/// no close at all: the fourth outcome is the absence of this event.
#[derive(Debug, Clone, Serialize)]
pub struct ReplayClosed {
    pub outcome: ReplayOutcome,
}

/// Three of the four outcomes a reader must separate, per
/// `weaver-analysis-PRD` section 4. The fourth, not ended, is the absence of
/// a `replay.closed` event and is therefore free.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReplayOutcome {
    Certified,
    Diverged { divergence: Divergence },
    Abandoned { reason: AbandonReason },
}

/// The two comparisons certification performs, kept apart: the token path is
/// integers and matches exactly or does not, and the reader's vectors compare
/// within the GPU float tolerance the apex names. Each carries the first
/// divergent position, the token path's carrying both identifiers so a reader
/// can say how the two differ without rerunning anything. **`position` is
/// the resident length at the draw in both**, the coordinate `model.field`
/// keys on, per section 3.3 on the ruling of 2026-09-09, so a divergence and
/// the field row it fell in share a key; the harness derives it from the
/// re-fed answer's closing count, and a divergence in the appended input
/// names the resident length that token occupies.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Divergence {
    TokenPath {
        position: u64,
        recorded: TokenId,
        recomputed: TokenId,
    },
    Readout {
        position: u64,
        layer: u32,
    },
}

/// Why a pass reached its own end without certifying, per section 8's
/// satellite election. The loop's likeliest is a refusal at input identity
/// before any forward pass ran.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AbandonReason {
    ReplayAskUnanswered,
    IdentityRefused { detail: String },
}

/// One sampled position's residual columns, where the ask stood: the
/// position it names, the layer count, the tap's width, and the values in
/// the floor's provisional bare JSON, layer-major, per section 3.2. The
/// efficient encoding is section 8's open election.
#[derive(Debug, Clone, Serialize)]
pub struct ResidualColumn {
    pub position: u64,
    pub layers: u32,
    pub width: u32,
    pub values: Vec<Vec<f32>>,
}

/// The run-scoped event ordinal: strictly increasing, gapless over admitted
/// events, starting at zero. Serializes as a decimal string, because a
/// consumer parsing JSON numbers as doubles gets a silently different value
/// past 2^53 with no error and no way back, per the canonical rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Sequence(pub u64);

impl Serialize for Sequence {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0.to_string())
    }
}

/// A monotonic reading in nanoseconds, serialized as a decimal string under
/// the same rule. The origin is `replay.opened`, per section 3.3: this set
/// carries no `load`, so the pass's own opening event is the origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct MonotonicNs(pub u64);

impl Serialize for MonotonicNs {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0.to_string())
    }
}

/// The authoring subsystems a diagnostic pass carries. The spelling is the
/// serving record's for the shared cases, per the compatibility rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Subsystem {
    Harness,
    Spu,
    SpuDecoder,
}

/// The diagnostic run's session, the replay's own and never the replayed
/// one: the recorder binds it by construction, per section 3.1, and what
/// names the replayed session is the identity payload.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SessionRef(pub String);

/// The diagnostic run's reference, admin-minted like any run's.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RunRef(pub String);

/// A replayed turn's key, carried as the record carried it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TurnRef(pub String);

/// A token's identifier, as the engine numbers its vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct TokenId(pub u32);

/// The replayed model's identity, as its record names it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ModelId(pub String);

/// The replayed weights' hash, as the record's measurement carries it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WeightsHash(pub String);

/// The template identity the replayed record rendered under.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TemplateId(pub String);
