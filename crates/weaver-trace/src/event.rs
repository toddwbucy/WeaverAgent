//! conforms: trace-identity-newtypes-harness-converts
//! conforms: trace-load-carries-the-tee-election
//! conforms: trace-kind-explicit-renames
//! conforms: trace-subsystem-case-set
//! conforms: trace-kind-enum-exhaustive
//! conforms: trace-turn-optional-never-inferred
//! conforms: trace-message-payloads-splice
//! conforms: trace-envelope-flattens
//! conforms: trace-no-envelope-field-named-payload
//! conforms: trace-payload-untagged-kind-discriminant
//! conforms: trace-bracket-kind-omits-payload
//! conforms: trace-turn-close-internally-tagged
//! conforms: trace-kind-payload-mapping-total
//! conforms: trace-splice-or-shape
//! conforms: trace-payload-serialize-only
//!
//! The event: the envelope, the kind set, and the payload shapes, per
//! `weaver-trace-Spec` section 3. The envelope flattens into the event and the
//! payload does not, so a line renders flat with `kind` at the top level, which
//! is what every consumer keys on first.

use std::sync::Arc;

use serde::Serialize;
use serde_json::value::RawValue;

use crate::canonical::{MonotonicNs, Sequence};

/// One event: the envelope's members and at most one `payload` member.
///
/// A bracket kind carries no payload member at all rather than a null one,
/// which `skip_serializing_if` carries. No field added to [`Envelope`] may be
/// named `payload` - the flatten puts both layers' members in one object.
#[derive(Debug, Clone, Serialize)]
pub struct Event {
    #[serde(flatten)]
    pub envelope: Envelope,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<Payload>,
}

/// The identity and ordering fields every event carries.
///
/// `turn` is optional and `causal_parent` is optional, and nothing else is. A
/// run-level event belongs to no turn, and the recorder never infers one: a
/// turn the recorder supplied would be a key that never travelled with the
/// work. The absent options emit no member, on the same argument the payload's
/// absence carries.
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

/// A session's identity as the harness converts it at the submit call, per
/// `weaver-trace-Spec` section 1: this crate links no internal crate, so the
/// floor's `SessionId` does not appear here and the harness converts at one
/// call site.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SessionRef(pub String);

/// The run's identity within its session, carried without interpretation.
///
/// **Not an ordinal.** It is minted by the party that performs the load and
/// is distinct within its session rather than positioned in a sequence, per
/// `weaver-admin-PRD` section 10. This crate neither mints it nor reads it,
/// which is why the change is a rename here and an election elsewhere. It
/// stops being `Copy` with the shape: a string is owned, and a run reference
/// crosses this crate once per submit rather than in a loop, so nothing here
/// wanted the copy.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RunRef(pub String);

/// The turn's identity as the harness converts it, the join key carried and
/// never invented.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TurnRef(pub String);

/// The producing party, at the granularity a reader needs.
///
/// Four cases are the crates that can produce a report. `Tool` is a producing
/// party and not a crate, because a tool's result reaches the record through
/// the harness and a record attributing it to the harness would lose the one
/// fact an operator reading a tool result wants first. **`SpuDecoder` is that
/// argument one level down**, per `weaver-trace-Spec` section 3: the SPU's
/// domain is every semantic operation in the text modality, so an event
/// stamped `Spu` loses which engine produced it once more than one lives
/// behind the organ. The three model events carry it; residency, admit,
/// release, and fault attribution stay `Spu`, being the organ's rather than
/// any engine's.
///
/// **A case arrives per producing party at the granularity a reader needs,
/// with its first emitter, a floor edit in the same act.** The encoder's
/// case, `spu_encoder`, is deliberately absent until the act that builds an
/// encoder gives it an emitter: a case with no producer is a reserved slot in
/// enum form, per apex section 9.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Subsystem {
    Admin,
    Harness,
    Spu,
    SpuDecoder,
    Gate,
    Tool,
}

/// The twenty-four event kinds, exhaustive, matching the charter's section 3.1
/// exactly. Every kind carries an explicit rename because no scheme produces
/// the charter's dotted names, and the enum is exhaustive because the set is
/// closed by ruling: an attribute that let a consumer absorb a further kind
/// into a wildcard would defeat the closure the corpus keys on. **The count
/// is pinned in `tests/kinds.rs`**, an array of twenty-four checked against an
/// exhaustive match over this enum, so a kind added here and answered in the
/// crate's own matches stops at that file's match. An act that writes the new
/// arm there too and leaves the array alone still passes, which is issue
/// #633's to close.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Kind {
    #[serde(rename = "load")]
    Load,
    #[serde(rename = "unload")]
    Unload,
    #[serde(rename = "session.closed")]
    SessionClosed,
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
    #[serde(rename = "tool.call.started")]
    ToolCallStarted,
    #[serde(rename = "tool.call.completed")]
    ToolCallCompleted,
    #[serde(rename = "fault")]
    Fault,
    #[serde(rename = "elision")]
    Elision,
    /// A typed refusal on any seam, per charter section 3.1's twenty-first
    /// kind: which ask was refused, which seam answered, and that seam's own
    /// case with the values it carries.
    #[serde(rename = "refusal")]
    Refusal,
    #[serde(rename = "flush")]
    Flush,
    #[serde(rename = "model.request")]
    ModelRequest,
    #[serde(rename = "model.output")]
    ModelOutput,
    #[serde(rename = "model.measurement")]
    ModelMeasurement,
    /// The probability field at one decode position, per charter section
    /// 3.1's nineteenth kind. The first kind recorded per position rather
    /// than per turn, and the first recorded only while an election
    /// stands.
    #[serde(rename = "model.field")]
    ModelField,
    #[serde(rename = "classify.request")]
    ClassifyRequest,
    #[serde(rename = "classify.output")]
    ClassifyOutput,
    /// An answered ask on the state seam, per charter section 3.1's
    /// twenty-second kind: which ask the harness made and the identities of
    /// the events custody answered with, never their contents.
    #[serde(rename = "recall")]
    Recall,
    /// One message of a restored conversation, seated at a restoring load's
    /// open ahead of every turn, per charter section 3.1's twenty-third
    /// kind: the message whole, its role and its content, turnless as the
    /// identity is. A kind of its own so the four turned message kinds stay
    /// turn-required and a restored message never reads as the identity.
    #[serde(rename = "message.restored")]
    MessageRestored,
    /// A task's verdict on its run, per charter section 3.1's twenty-fourth
    /// kind (#523): the predicate the task answered, whether it held, and the
    /// ratio over the task's denominator where one exists. Authored at the
    /// run's close, between turns, so it belongs to none.
    #[serde(rename = "score")]
    Score,
}

/// What an event carries beside its envelope. Untagged: the envelope's `kind`
/// is the discriminant, because a tag would wrap the spliced message bytes in
/// an object of this crate's making, the double encoding the `RawValue`
/// election exists to avoid. Admission enforces the kind-to-payload pairing,
/// since serde no longer can.
///
/// Derives `Serialize` and not `Deserialize`: this crate never reads an event
/// back, the working structure holding rendered lines, and the asymmetry is a
/// compile property pinned at the crate root.
///
/// The kind-to-payload mapping is total, twenty-four kinds and eighteen
/// dispositions, the payload-free case counting as one of them.
/// **`pairing_licensed` in `writer.rs` enforces the mapping and is the
/// authority on it**, this comment naming only which variant of this enum
/// each kind reaches. Variants throughout rather than the shapes they hold,
/// so a row reads against the licensing match with no translation step, and
/// because six variants hold `Box<RawValue>` and a shape cannot tell them
/// apart: `session.closed` and `turn.started` carry nothing, `unload`
/// carries `Unload` where a member stood and nothing where none did, `load`
/// carries `Elections`, the five message kinds carry `Message`,
/// `turn.closed` carries `TurnClosed`, `fault` carries `Fault`, `flush`
/// carries `Flush`, `elision` carries `Elision`, `refusal` carries
/// `Refusal`, the four model kinds carry their four own variants, the
/// classify pair carries its two, `recall` carries `Recall`, `score` carries
/// `Score`, and the tool
/// bracket's two carry `Deferred`.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum Payload {
    /// A message payload the harness pre-rendered, spliced as the bytes stand.
    Message(Box<RawValue>),
    /// The turn's close, the one payload the merged corpus fixes today.
    TurnClosed(TurnClose),
    /// The leave's reading of the store's boundary, per `weaver-trace-PRD`
    /// section 3.1 as of 2026-09-04, carried where a member stood and
    /// absent where none did.
    Unload(UnloadClose),
    /// The floor's fault-report shape: the reporting organ renders its
    /// account and names its case, the harness serializes the report whole
    /// and splices it, per apex section 5.2's custody rule. Formerly
    /// "rendered by the harness", which was true of the code until the act
    /// that shaped the report, and corrected in that act rather than ahead
    /// of it.
    Fault(Box<RawValue>),
    /// The request the model received, the SPU-rendered content the harness
    /// splices, per the custody act of 2026-08-11: the rendered prompt with
    /// its template and effective sampling, the organ's shape the record
    /// carries opaque.
    ModelRequest(Box<RawValue>),
    ModelOutput(ModelOutput),
    /// One decode position's ranked candidates, per charter section 3.1.
    /// Shaped rather than spliced on the same test the output meets: a
    /// decode position, a vector of pairs, and an index, all plain values
    /// the harness holds typed on their way through.
    ModelField(ModelField),
    /// The diagnostic elections the load declared, carried by its `load`
    /// event so a record declares the posture it was written in, per
    /// charter section 3.2.
    Elections(Elections),
    /// The flush's account: the resident token counts before and after the
    /// decode context returned to its prefix, per charter section 3.1's
    /// sixteenth kind. Both from the SPU's confirmation, the one authority
    /// on either number.
    Flush(FlushCounts),
    /// The elision's span and the resident counts either side, per
    /// `weaver-trace-PRD` section 3.1's twentieth kind.
    Elision(ElisionSpan),
    /// The refusing party's own account, spliced.
    ///
    /// **Spliced rather than shaped**, on the custody rule that already
    /// governs `Fault`: a refusal is produced by the party that refused, and
    /// its shape is the floor's `refusal-record` rather than this crate's.
    /// A typed payload declared here would make this crate hold four seam
    /// vocabularies and version them as seams change, which its own
    /// no-dependency rule forbids.
    Refusal(Box<RawValue>),
    /// The instrument readings, the SPU-rendered measurement the harness
    /// splices, its unproduced members produced absent by the SPU rather than
    /// omitted by a serde election of this crate's.
    ModelMeasurement(Box<RawValue>),
    /// The label seam's request side, shaped on the flush's precedent:
    /// plain small data the harness authors from typed wire answers, per
    /// charter section 3.1's seventeenth kind.
    ClassifyRequest(ClassifyAsk),
    /// The label seam's response side: the scored labels alone, per charter
    /// section 3.1's eighteenth kind.
    ///
    /// **A refused classify authors no output at all** and reaches the
    /// record under `Refusal`, as of 2026-08-22. The guarantee this variant
    /// carried is unchanged and moved with it: a typed refusal the exchange
    /// met is the record's own fact and never a fabricated answer. What
    /// changed is that one kind stopped meaning two things.
    ClassifyOutput(ClassifyScored),
    /// A recall's account, shaped on the flush's precedent: plain small data
    /// the harness authors from the state seam's answer, per charter section
    /// 3.1's twenty-second kind.
    Recall(RecallAccount),
    /// A task's verdict, shaped on the flush's precedent: plain small data the
    /// task hands the harness at the run's close, per charter section 3.1's
    /// twenty-fourth kind.
    Score(TaskScore),
    /// The payloads whose shapes their own workflows settle, since the trace
    /// act of 2026-08-02 the tool bracket's two alone. Raw bytes in the
    /// interim rather than a placeholder struct, because a struct shaped
    /// against no chartered content would be a reserved slot.
    Deferred(Box<RawValue>),
}

/// How a turn closed. Internally tagged under the two floor Specs' shared
/// test: `Clean` is fieldless and `Stopped` is struct-shaped, rendering
/// `{"close":"clean"}` and `{"close":"stopped","reason":...}`, one shape for
/// both closes.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "close", rename_all = "snake_case")]
pub enum TurnClose {
    Clean,
    Stopped { reason: StopReason },
}

/// Why a stopped turn stopped. A satellite election per `weaver-trace-Spec`
/// section 12: the stop directive is today's one clean interrupt and a fault
/// is the other way a turn ends early.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    Directive,
    Fault,
    /// The turn ended because a seam refused an ask it carried, per the
    /// charter's clause of 2026-08-22.
    ///
    /// **The close and the `refusal` event are two records of two facts.**
    /// This says the bracket ended and which kind of ending it was, the
    /// event says what was refused, and neither is recoverable from the
    /// other. It is the division `Fault` already runs on.
    Refused,
}

/// The resident token counts either side of a flush, both the SPU's own, as
/// its confirmation carried them.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FlushCounts {
    pub resident_before: u64,
    pub resident_after: u64,
}

/// A task's verdict on its run: which predicate it answered and whether that
/// held, and where the task supplies a denominator, the ratio as its terms.
///
/// **The ratio is carried as its two terms and never as a quotient.** A float
/// would enter the record rounded, and two readers dividing one recorded pair
/// agree to the bit where two recorded quotients need not, so the record holds
/// what was counted and a reader computes the quotient.
///
/// **Absent is absent.** A task that supplies no denominator records no ratio,
/// never a zero or a one standing for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaskScore {
    pub predicate: String,
    pub passed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ratio: Option<ScoreRatio>,
}

/// The ratio's two terms: what the run measured over what the task supplies,
/// the turns taken over the optimal count where the task is a map.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ScoreRatio {
    pub measured: u64,
    pub denominator: u64,
}

/// An answered ask on the state seam: what was asked, and which events
/// custody answered with.
///
/// **Identities and never contents.** The events a recall returns are
/// already in the record under their own identities, so the account names
/// them by run, turn, sequence and kind and carries none of their pairs: a
/// reader who wants a returned event's content reads it where it stands. What
/// the record lacked before this kind was the ask itself and which events
/// answered it, which is the input a loop then builds the next turn from.
///
/// **A partial answer lists every identity and a whole-session answer its
/// bounds.** A bounded recall is exactly the case where which events came
/// back is the fact, so each is named. The replay port's answer is the source
/// session from its first event to its last, so it is named by those two
/// and the count, and never by a list as long as the session.
///
/// **Only an answered ask is recorded here.** A seam that refused or died
/// answered nothing, and that fact is the refusal event's or the fault's, per
/// `weaver-trace-Spec` section 3's recall clause.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RecallAccount {
    pub ask: RecallAsk,
    pub returned: Vec<RecalledIdentity>,
    /// How many events answered, carried where `returned` holds the
    /// answer's bounds rather than every identity: a whole-session replay
    /// answer is identified by its first and last events and this count.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<u64>,
}

/// The ask as the harness sent it: which of the state seam's asks, and the
/// turn bound where the ask carried one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct RecallAsk {
    pub verb: RecallVerb,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_turns: Option<u64>,
}

/// Which state-seam ask returned the events: the enter's identity ask, a
/// recall, the enter's under a restoring load and the seat's after a flush
/// alike, or the replay port's whole-session ask.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecallVerb {
    Identity,
    Recall,
    Replay,
}

/// One returned event's identity as custody answered it. The turn is absent
/// where the event belongs to none, a seated prefix among them, and the
/// sequence is the decimal string the record spelled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RecalledIdentity {
    pub run: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn: Option<String>,
    pub sequence: String,
    pub kind: String,
}

/// What an elision removed and what the session held either side.
///
/// **The span is on the event because the counts cannot stand in for it.**
/// A flush is fully described by what it leaves, its outcome being a
/// prefix. An elision removes an interior, so two elisions reporting
/// identical counts can have removed different positions and left
/// different sequences. An edit that does not say where it fell is not
/// replayable, and the replay is this crate's own promise.
///
/// **The span comes from the harness's ask and the counts from the SPU's
/// answer**, each party writing what it is the authority on, per the decode
/// contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ElisionSpan {
    pub from: u64,
    pub to: u64,
    pub resident_before: u64,
    pub resident_after: u64,
}

/// The classify ask's account: the content the loop sent, per
/// `weaver-trace-Spec` section 3 as of the classifier act.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ClassifyAsk {
    pub content: String,
}

/// The classify answer's account: every label of the artifact's head
/// scored.
///
/// **A struct rather than an enum, as of 2026-08-22.** It held two variants
/// and the second, a free-form `Refused { refusal: String }`, moved to the
/// `refusal` kind when the refusal class took every seam's typed refusal
/// into one. A single-variant enum kept against a second that may come is a
/// reserved slot for a reader that does not exist, so what is left takes a
/// struct and a later outcome brings its own shape in the act that adds it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ClassifyScored {
    pub labels: Vec<(String, f64)>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ModelOutput {
    pub emission: String,
    pub finish: Finish,
    /// The session's token count as this generation closed and the ceiling
    /// the load resolved, per `weaver-trace-Spec` section 3: the pair the
    /// flush's counts and the overflow refusal carry, recorded here because
    /// a generation moves the resident context as a flush does. The harness
    /// reads them at this same close to answer the seat's fullness port, so
    /// the record and the loop take one reading rather than two.
    pub resident: u64,
    pub capacity: u64,
}

/// One decode position's probability field, per `weaver-trace-Spec`
/// section 3: the ranked candidates with their probabilities and the rank
/// the draw landed on.
///
/// **Recorded only while the field election of `weaver-spu-PRD` section
/// 13.11 stands**, which makes it the first kind of which that is true.
/// Whether the election stood is the `load` event's to say, per charter
/// section 3.2, so an absent field is distinguishable from an election
/// that stood and produced nothing.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ModelField {
    pub position: u64,
    pub ranked: Vec<Candidate>,
    pub realized: u32,
}

/// One ranked candidate: the token and the probability the distribution
/// gave it at that position.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Candidate {
    pub token: u32,
    pub probability: f32,
}

/// The `unload` event's payload, per `weaver-trace-PRD` section 3.1 as of
/// 2026-09-04: the grant surface read back at the leave against the reading
/// taken at the enter.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UnloadClose {
    pub grant_surface: GrantSurface,
}

/// What the leave found, in the envelope the confirm drivers carry: the
/// surface read the same, read different, or not readable at the close, the
/// last said rather than reported unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum GrantSurface {
    Unchanged,
    Varied,
    Unreadable,
}

/// The store the state member stands on, per `weaver-trace-PRD` section 3.1
/// as of 2026-09-04: the engine by its name, and under the service engine
/// the database and the role. Written whole on the load. This crate spells
/// the shape itself, as it does the loop's, because the floor's election
/// type is the declaration's and the record names what was resolved.
#[derive(Default, Debug, Clone, PartialEq, Serialize)]
pub struct StoreIdentity {
    pub engine: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

/// The diagnostic elections a load declared, per charter section 3.2.
///
/// **Each is named individually and none is bundled under a profile
/// name.** A named set drifts as members join it, and every record already
/// carrying the name silently becomes a record of something else. Naming
/// each is what keeps a record's posture recoverable from the record:
/// without it, a record holding no field and a record whose election stood
/// and produced nothing are one absence on disk, a configuration and a
/// fault wearing one face.
/// The loop that composes a run's prompts, named on the `load` event per
/// `weaver-trace-Spec` section 3. `binary` is the worker's own name for
/// itself. Where the loop is a file the worker reads, `file` is the path it
/// resolved and `sha256` the digest of that file as read at the load, both
/// absent for a loop compiled into the binary. The digest is the file at
/// the load and not at each crossing, a bound the Spec states.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LoopIdentity {
    pub binary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

impl LoopIdentity {
    /// A loop compiled into the binary: no file, no digest.
    pub fn compiled(binary: &str) -> LoopIdentity {
        LoopIdentity {
            binary: binary.to_string(),
            file: None,
            sha256: None,
        }
    }

    /// A loop the binary read from a file, with the digest where the file
    /// could be read at the load.
    pub fn file(binary: &str, path: &std::path::Path, sha256: Option<String>) -> LoopIdentity {
        LoopIdentity {
            binary: binary.to_string(),
            file: Some(path.display().to_string()),
            sha256,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Elections {
    pub residual_readout: bool,
    /// The field's declared depth where its election stood, absent where
    /// it did not.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<u32>,
    /// Whether the per-position surprisal was elected, per
    /// `weaver-spu-PRD` section 13.12.
    ///
    /// **Serialized even when false**, which is the opposite election from
    /// the field's option above. The field's absence is the whole of what
    /// an unelected field has to say. This one distinguishes a record
    /// whose operator declined the vector from a record written before the
    /// election existed, where the member is absent altogether: absent,
    /// false, and true are three states and the shape keeps them three.
    pub surprisal: bool,
    /// Whether the state member's end arrived on the enter, per
    /// `weaver-trace-Spec` section 3 as of 2026-09-03: the harness's own
    /// knowledge and never a read of the deployment. **Serialized even
    /// when false**, for the same three-state reason as the surprisal: a
    /// record written before the member existed is absent, not false.
    pub state_member: bool,
    /// The store the member stands on, per `weaver-trace-PRD` section 3.1
    /// as of 2026-09-04: the engine, and under the service engine the
    /// database and role, copied from the enter, the floor's own shape.
    /// Written beside `state_member` because the two answer different
    /// questions: the election is what the deployment asked for and the
    /// member is whether an end arrived.
    pub state_store: StoreIdentity,
    /// The declaration file's digest the load was built from, as admin read
    /// it at the inventory and the enter carried it, per `weaver-trace-PRD`
    /// section 3.1 as of 2026-09-04: a record names its own declaration
    /// rather than leaning on a deposit beside it.
    pub declaration: String,
    /// The loop that assembled this run's prompts, per `weaver-trace-Spec`
    /// section 3: the binary that ran it, and the file and its digest at
    /// the load where the loop is a file. Two loops assemble different
    /// prompts from one declaration, so a record that does not name its
    /// composer cannot be compared with one that does.
    pub composer: LoopIdentity,
    /// The tee's rule, this crate's own `Election`, written whole on every
    /// `load` the harness authors: every load has an election, a deployment
    /// that elects nothing running under the default, per
    /// `weaver-trace-Spec` section 3. The `Option` is the reader's, per the
    /// compatibility rule: absence means the record predates the member and
    /// the rule that built the state is unrecoverable, never that the
    /// election was the default, so such a record replays its token path
    /// and cannot be certified for state.
    pub tee: Option<crate::tee::Election>,
    /// Where the session stands from a record, per `weaver-trace-Spec`
    /// section 3 as of 2026-09-06 and the charter's 3.1: the parent's
    /// session, the run the cut falls in, and the turn the holdings stop at,
    /// copied from the enter. Absent otherwise, never null.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lineage: Option<Box<Lineage>>,
    /// The digests of the organ binaries admin started, keyed by name, per
    /// the same section, so a record is sufficient for its own conditions.
    pub stack: std::collections::BTreeMap<String, String>,
}

/// A restore's lineage as the load event names it, per `weaver-trace-Spec`
/// section 3.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Lineage {
    pub parent: String,
    pub run: String,
    pub through: u64,
}

#[cfg(test)]
mod lineage_tests {
    use super::*;

    fn elections(lineage: Option<Box<Lineage>>) -> Elections {
        let mut stack = std::collections::BTreeMap::new();
        stack.insert("weaver-harness".to_string(), "ab12".to_string());
        Elections {
            residual_readout: false,
            field: None,
            surprisal: false,
            state_member: true,
            state_store: StoreIdentity {
                engine: "sqlite".into(),
                database: None,
                role: None,
            },
            composer: LoopIdentity::compiled("test"),
            declaration: "d".into(),
            tee: None,
            lineage,
            stack,
        }
    }

    /// **The load names its lineage where the session stands from a record
    /// and its stack on every load**, per `weaver-trace-Spec` section 3 as
    /// of 2026-09-06: the lineage absent rather than null where the load
    /// stands from nothing, and the stack keyed by name.
    ///
    /// Perturbation: drop the `skip_serializing_if` on `lineage` and the
    /// absence assertion fails on a null member. Watched under exactly that
    /// removal.
    #[test]
    fn the_load_names_its_lineage_and_its_stack() {
        let rendered = serde_json::to_string(&elections(None)).expect("renders");
        assert!(
            !rendered.contains("lineage"),
            "absent, never null: {rendered}"
        );
        assert!(rendered.contains("\"stack\":{\"weaver-harness\":\"ab12\"}"));
        let rendered = serde_json::to_string(&elections(Some(Box::new(Lineage {
            parent: "s-1".into(),
            run: "r-a".into(),
            through: 2,
        }))))
        .expect("renders");
        assert!(
            rendered.contains("\"lineage\":{\"parent\":\"s-1\",\"run\":\"r-a\",\"through\":2}"),
            "{rendered}"
        );
    }
}

/// How the generation ended, per the charter: completed or stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Finish {
    Completed,
    Stopped,
    /// The turn's token limit reached, and that limit alone: this crate's
    /// mirror of the floor's case, converted by the harness at the one
    /// site, per `weaver-trace-Spec` section 3.
    Length,
}

/// The measurement and the request retired their typed shapes with the custody
/// act of 2026-08-11: both are the SPU's rendering the record carries opaque,
/// so their satellites, the model and template identities, the weights hash,
/// the token identifier, the per-token bits, the prompt block, and the decode
/// timings, are the SPU's construction shape and no longer this crate's, per
/// `weaver-trace-Spec` section 3. `ModelOutput` alone stays shaped, its
/// emission a string the harness consumes and its finish the turn's close.
///
/// A pre-rendered payload arrives as validated octets: `RawValue` construction
/// validates JSON, and the separator check ahead of it closes the hole
/// validation leaves - JSON holds no raw newline inside a string, but holds
/// them freely between tokens, so pretty-printed octets are valid JSON that
/// would split the frame. A legitimately escaped newline inside a string is
/// the two octets `\` `n`, so scanning for the raw byte has zero false
/// positives on well-formed payloads. The render path holds the same check as
/// the backstop for a `RawValue` built any other way.
pub fn raw_payload(octets: &str) -> Option<Box<RawValue>> {
    if octets.contains('\n') || octets.contains('\r') {
        return None;
    }
    RawValue::from_string(octets.to_string()).ok()
}

/// The line type both sinks hold: one rendering, two holders.
pub type Line = Arc<str>;
