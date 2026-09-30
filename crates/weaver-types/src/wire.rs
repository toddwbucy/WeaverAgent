//! conforms: types-loop0-encoding-json
//! conforms: types-tagging-test
//! conforms: types-envelope-bound-64k
//! conforms: types-socket-seqpacket
//! conforms: types-frame-survives-arbitrary-octets
//! conforms: types-enter-binding-disagreement-unrepresentable
//!
//! The loop 0 wire vocabulary, per `weaver-types-Spec` section 4: the envelope
//! every organ channel carries and loop 0's trio, named for the loop whose
//! traffic it carries. Loop 0's traffic is JSON, one envelope to one message,
//! low in volume and diagnostic in audience. The tagging follows the mechanical
//! test the two floor Specs share: a fieldless enum is a plain renamed string,
//! an enum whose every variant is struct-shaped or wraps a struct is internally
//! tagged, an enum with any variant wrapping a primitive, a sequence, or
//! another tagged enum is adjacently tagged, and the same arm takes a variant
//! wrapping a struct that carries a spliced member, internal tagging buffering
//! the read side into a shape that cannot represent pre-serialized JSON.
//!
//! The socket type election (`SOCK_SEQPACKET`) and the boundary obligations bind
//! the pair-creating crates, per the Spec; this crate opens no socket and the
//! one number a builder needs is [`MAX_ENVELOPE_BYTES`].

use serde::{Deserialize, Serialize};

use crate::config::{FieldName, GateInstruction, SpuInstruction, ToolName};

/// The receiver's buffer bound: a message exceeding it sets the truncation flag
/// and is treated as a channel fault rather than a message, because a silently
/// shortened directive is the failure mode the boundary property was elected to
/// prevent. Generous against loop 0's traffic, whose largest case is an enter
/// payload of identifiers.
pub const MAX_ENVELOPE_BYTES: usize = 64 * 1024;

/// The decode seam's total message bound, per `weaver-types-Spec` section
/// 4.4's segment series: a frame past the envelope crosses as a preamble
/// and counted slices, reassembled under this bound. Eight mebibytes,
/// elected against the close's growth - a four-thousand-token turn's close
/// measures in the hundreds of kibibytes, so the bound covers a sixteenfold
/// cap without renegotiation.
pub const DECODE_MESSAGE_BOUND: usize = 8 * 1024 * 1024;

/// The segment series' preamble, per `weaver-types-Spec` section 4.4:
/// exactly two members, both unsigned, and nothing else - the
/// unknown-field refusal is what makes a three-member kindless frame a
/// fault rather than a guess. The floor holds the shape and no logic:
/// each side of the seam owns its own segmentation and reassembly, per
/// the charter's one-rule carve-out.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SegmentPreamble {
    pub segments: u64,
    pub bytes: u64,
}

/// One message on an organ channel.
///
/// Three members of one object, nothing flattened, so no two layers can
/// contribute one key:
///
/// ```text
/// {"exchange":{"opener":"admin","ordinal":7},
///  "position":"open",
///  "payload":{"kind":"directive","body":{"kind":"load","agent":"alpha"}}}
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrganEnvelope {
    pub exchange: ExchangeId,
    pub position: Position,
    pub payload: Payload,
}

/// The opening party and an ordinal, per `weaver-organ-channel` section 1.
/// Naming the party rather than carrying a bare bit is what lets a capture read
/// as itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExchangeId {
    pub opener: Opener,
    pub ordinal: u64,
}

/// The four parties that hold an organ channel. The enum grows when an organ
/// does, which is a floor edit in the act that charters the organ.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Opener {
    Admin,
    Harness,
    Spu,
    Gate,
}

/// A message's position in its exchange: open, continue, or close, per
/// `weaver-organ-channel` section 1. Three rather than two because the channel
/// layer permits an intermediate message between a directive and its answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Position {
    Open,
    Continue,
    Close,
}

/// What an envelope carries: a typed enum rather than octets, which is what
/// makes the encoding legible. Adjacently tagged, because its variants wrap
/// enums that carry a tag of their own.
///
/// A later loop's vocabulary enters this enum in the act that charters that
/// loop. `Fault` enters on different grounds: a fault report is what any organ
/// hands the harness across whatever channel it holds, and the gate has no
/// second socket to carry it.
// The size skew is the admit's instruction, loop 0's low-volume diagnostic
// traffic: twice per run and once per stop, so a box would trade wire-shape
// churn for nothing measurable. The lint crossed its threshold when the
// instruction gained the classify role, not when the shape changed kind.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "body", rename_all = "snake_case")]
pub enum Payload {
    Directive(LifecycleDirective),
    Answer(LifecycleAnswer),
    Refusal(LifecycleRefusal),
    Frame(TurnFrame),
    Fault(FaultReport),
    /// The execution exchange's ask, harness-opened per recovered call, as of
    /// the tool workflow's opening act: the enum's own rule, a later loop's
    /// vocabulary entering in the act that charters that loop.
    Tool(ToolExecution),
    /// The execution exchange's answer, closing it, one of the four contents
    /// `weaver-harness-gate-contract` section 2 names.
    ToolAnswer(ToolOutcome),
    /// Cancel the open execution at the continue position.
    ToolCancel,
}

/// One tool call as it crosses the gate seam: the name and arguments exactly
/// as the family parse recovered them from the emission, uninterpreted by
/// the harness that carries them, and the caller's clock, per
/// `weaver-harness-gate-contract` section 2 as amended by the tool boundary
/// ruling: one clock, the caller's, validated against the tool's declared
/// maximum at the refusal layer and adopted as the kill clock.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolExecution {
    pub name: ToolName,
    pub arguments: String,
    /// The caller's clock in milliseconds. The unit is this definition's
    /// election, the contract fixing the rule and not the representation.
    pub clock_ms: u64,
}

/// The four contents an execution's answer carries, told apart by tag
/// alone, every one of them content rather than a channel fault, because
/// each is a fact the model must learn. The rule beneath the four is who
/// speaks in the return: a result is the tool's own words, a refusal is the
/// gate's voice with nothing run, an error is the machinery's, and a kill
/// carries no tool voice by construction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum ToolOutcome {
    /// The invocation ran and the tool answers in its own words. For the
    /// shell a nonzero exit is a result, accounted in content, not an
    /// error.
    Result { content: String },
    /// Nothing ran, and the gate speaks in its own voice: a name it does
    /// not hold - never a nearest match, the family registry's discipline
    /// one organ over - malformed arguments, or a clock beyond the
    /// declared maximum. No side effect exists.
    Refused { reason: String },
    /// The invocation machinery failed - the fork, a pipe, the supervisor -
    /// and the account's speaker is the infrastructure, never the tool.
    Errored { detail: String },
    /// The caller's clock expired or the caller cancelled, and the gate killed the invocation's
    /// whole process group. No account from the tool exists by
    /// construction - the absence of the tool's words is the fact - and
    /// output drained before the kill rides as an attachment, never a
    /// result.
    Killed {
        partial: Option<String>,
        by: KillCause,
    },
}

/// What ended a killed execution, per the gate contract's one-clock rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KillCause {
    Clock,
    Cancel,
}

/// The organs that refuse inside a fan-out: only the SPU and the gate. Admin
/// does not refuse to itself and the harness returns the aggregate rather than
/// appearing inside it, so reusing the four-case `Opener` would let a
/// well-typed aggregate claim that admin refused as an organ.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefusingOrgan {
    Spu,
    Gate,
}

/// One directive type for loop 0, carrying every case that crosses any of its
/// four seams; each contract's vocabulary clause names the subset that crosses
/// its own. Internally tagged, which is why every case carrying a value takes a
/// struct variant. Exhaustive: a case added later breaks every consumer's match
/// at compile time, in the same act that edits the floor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LifecycleDirective {
    /// Boxed, per `weaver-types-Spec` section 4.2 as of 2026-09-06 and issue
    /// #475: the payload carried the enum's whole size onto every unit
    /// variant, and a boxed field crosses the seam as the field does.
    Enter {
        payload: Box<EnterPayload>,
    },
    Leave,
    Stop,
    /// **Observe the run**, per `weaver-admin-harness-contract` section 3 as
    /// of 2026-09-04: admin asks what stands and carries nothing, and the
    /// harness answers `State` from whichever position it holds, the load's
    /// facts beside the state where a run stands. Authors nothing.
    Observe,
    Admit {
        instruction: SpuInstruction,
    },
    Release,
    /// **Two fields because two authors.** The instruction is the operator's
    /// election carried uninterpreted, and the socket is the program's
    /// deployment fact, supplied by the harness inside the unit's runtime
    /// directory so the manager's create-and-destroy makes a stale pathname
    /// unreachable, per `weaver-gate-PRD` section 2. A single field would put
    /// the operator's name on a value they do not choose.
    Raise {
        instruction: GateInstruction,
        socket: std::path::PathBuf,
    },
    Lower,
    Load {
        agent: AgentName,
    },
    Unload {
        agent: AgentName,
    },
    Validate {
        agent: AgentName,
    },
    List,
    Show {
        agent: AgentName,
    },
}

/// Every directive receives exactly one answer: `Enter` answers `Ready`,
/// `Leave` answers `Left`, `Stop` answers `TurnAborted` or `AtRest` by what it
/// interrupted, `Admit` answers `Admitted`, `Release` answers `Released`,
/// `Raise` answers `GateReady`, `Lower` answers `GateStopped`, `Validate`
/// answers `Validated`, `Load`, `Unload`, and `Show` answer `State`, and `List`
/// answers `Agents`. Any directive may answer a [`LifecycleRefusal`] instead,
/// which is the second half of what one answer per request means.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LifecycleAnswer {
    Ready,
    Left,
    TurnAborted {
        turn: TurnKey,
    },
    AtRest,
    Admitted,
    Released,
    GateReady,
    GateStopped,
    Validated,
    /// The agent's state and, where a run stands, the load's facts, per
    /// `weaver-types-Spec` section 4 as of 2026-09-04. `load` is absent
    /// exactly where the state is `Absent` or `Unloaded`.
    State {
        state: AgentState,
        /// Boxed on the same ground as the enter's payload, per the same
        /// section and issue.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        load: Option<Box<LoadFacts>>,
    },
    Agents {
        agents: Vec<AgentSummary>,
    },
}

/// **What a standing run was built from**, per `weaver-types-Spec` section
/// 4.2 as of 2026-09-04: the floor's shape of what the `load` event names,
/// answered to the observation exchange from the run and never read from the
/// record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoadFacts {
    pub session: SessionId,
    pub run: RunId,
    /// The declaration file's digest as admin read it at the inventory.
    pub declaration: String,
    pub artifact: crate::ArtifactRef,
    pub residual_readout: bool,
    /// The field election's depth where one stands, as the load event's
    /// `field` member carries it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<u32>,
    pub surprisal: bool,
    pub state_election: crate::config::StateElection,
    pub state_store: crate::config::StateStore,
    pub state_member: bool,
    pub composer: Composer,
}

/// The composing loop, this crate's spelling: the record's own is
/// `weaver-trace`'s and the floor links downward only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Composer {
    pub binary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<std::path::PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

/// The refusal cases, drawn from the merged contracts, the set closed at this
/// crate. A receiving party matches its own cases and refuses the rest as
/// `OutOfOrder`, which is a real obligation rather than a formality.
///
/// `OrganRefused` boxes an inner refusal because the aggregate carries it
/// unchanged: a refusing organ's reason reaches admin without translation, so
/// the harness wraps rather than re-encodes, and the box keeps the enum's size
/// from being set by its deepest case.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LifecycleRefusal {
    Unauthorized,
    Malformed,
    NoSuchAgent,
    CarriedWork,
    OutOfOrder,
    DescriptorsUnusable,
    BoundaryUnverified,
    ConfigInvalid {
        field: Option<FieldName>,
    },
    ArtifactUnresolvable,
    ArtifactUnreadable,
    DeviceCannotAdmit,
    NoResidency,
    BindFailed,
    /// A prior process for this unit exited non-zero and its name is still
    /// held by the manager, so a later start under that name refuses until it
    /// is reaped.
    ///
    /// **It claims that and no more.** `failed` is the one state the init
    /// boundary reports without ambiguity, per
    /// `weaver-admin-systemd-contract` section 3, and it does not say whether
    /// the worker bound or how long it served: a unit that bound its socket,
    /// served, and exited non-zero later reads the same. What it says is what
    /// refused the load, and that is a different fact from a socket that would
    /// not bind, which is `BindFailed`.
    PriorUnitUnreaped,
    OrganRefused {
        organ: RefusingOrgan,
        reason: Box<LifecycleRefusal>,
    },
    ActivityNotAtRest,
}

/// What admin supplies in the enter directive, per
/// `weaver-admin-harness-contract` sections 3 and 5.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct EnterPayload {
    pub session: SessionId,
    pub run: RunId,
    pub spu_instruction: SpuInstruction,
    /// The kind resolved, with what the kind requires riding inside it.
    pub binding: EnterBinding,
    /// The tee's election, resolved: admin fills the ruled default where
    /// the declaration is silent, per `weaver-admin-harness-contract`
    /// section 5, so what crosses is always the election whole.
    pub state_election: crate::config::StateElection,
    /// The store the member stands on, resolved the same way: the embedded
    /// engine where the declaration is silent, and under the service engine
    /// the database and role, per `weaver-types-Spec` section 4 as of
    /// 2026-09-04. The harness names it on the load event.
    pub state_store: crate::config::StateStore,
    /// The declaration file's digest as admin read it at the inventory, per
    /// `weaver-types-Spec` section 4 as of 2026-09-04, so the run and the
    /// record both name what they were built from and the harness holds no
    /// file.
    pub declaration: String,
    /// The lineage of the restore where the declaration elects one, resolved
    /// by admin and never the record's path, per `weaver-types-Spec` section
    /// 4 as of 2026-09-04 and issue #432: the harness names the parent on the
    /// load event and starts its turn ordinal from the cut without opening
    /// anything.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restore: Option<Lineage>,
    /// The digests of the organ binaries admin started, keyed by the
    /// binary's name, so the load event names the stack that ran it, per the
    /// same section. Admin's fact, authored by the harness as it authors the
    /// store's.
    #[serde(default)]
    pub stack: std::collections::BTreeMap<String, String>,
}

/// A restore's lineage as admin resolved it: the parent's session, the run
/// the cut falls in, and the turn the holdings stop at, a whole record
/// resolved to its last run's last turn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Lineage {
    pub parent: SessionId,
    pub run: RunId,
    pub through: u64,
}

/// The binding kind as admin resolved it, per `weaver-types-Spec` section 4:
/// the config holds the kind as the operator may state it and this enum holds
/// it decided, so the resolution point is visible in the types.
///
/// **A directive disagreeing with its kind is unrepresentable rather than
/// refused.** The gate instruction rides inside the serving case, so a
/// diagnostic enter has no field for it and a serving enter cannot omit it -
/// the wrong pairing is a struct that does not exist. The grouping is
/// representation rather than a term of its own: the vocabulary node stays
/// `binding-kind` alone.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum EnterBinding {
    Serving {
        gate_instruction: crate::config::GateInstruction,
    },
    Diagnostic,
}

/// A session's identifier. An identifier choice with no cross-crate
/// consequence, shaped in this crate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionId(pub String);

/// A run's identifier, minted by admin at the load and distinct within its
/// session, per `weaver-admin-PRD` section 10.
///
/// **Not an ordinal.** Nothing program-side counts runs, because nothing
/// program-side is alive between two of them, and the requirement the record
/// carries is distinctness rather than position. What makes one distinct is
/// argued where it is minted: this crate fixes only that it is an identifier
/// rather than a number, which is what lets the mint answer without anything
/// being remembered between invocations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunId(pub String);

/// The join key that names a turn. An identifier choice with no cross-crate
/// consequence, shaped in this crate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TurnKey(pub String);

/// An agent's name, as the operator provisioned it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentName(pub String);

/// What an operator can be told about an agent's place in the lifecycle. The
/// four states are the apex section 6 diagram's, the floor of the set; whether
/// it carries more is settled with the operator surface's own design.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentState {
    /// No provisioned identity exists.
    Absent,
    /// Provisioned and unloaded: identity exists, nothing resident.
    Unloaded,
    /// Loaded and idle: resident and interruptible, ready for the next run.
    Idle,
    /// Work in flight.
    Active,
}

/// One row of the `List` answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentSummary {
    pub name: AgentName,
    pub state: AgentState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub load: Option<LoadFacts>,
}

/// A turn frame, opaque to the gate: whatever the client sent, and the
/// answer going back, one definition for both directions per the charter.
///
/// The member is the line's octets encoded base64, per the election of
/// `weaver-types-Spec` section 4.1 and the ruling of 2026-08-12: RFC 4648
/// section 4's standard alphabet, padded, no line breaks and no interior
/// whitespace, so one octet sequence has exactly one carried form. The
/// encoding rides here with the type, one implementation holding the
/// canonical form for every party, and [`TurnFrame::octets`] refuses what
/// [`TurnFrame::carry`] would not produce.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TurnFrame {
    pub octets: String,
}

impl TurnFrame {
    /// Carries octets as a frame, encoded to the one canonical form.
    pub fn carry(octets: &[u8]) -> TurnFrame {
        TurnFrame {
            octets: encode_base64(octets),
        }
    }

    /// The carried octets, decoded, or `None` where the member is not the
    /// form the encode produces: a length off the four-boundary, a byte
    /// outside the alphabet, padding anywhere but the tail, or trailing
    /// bits the encode would have zeroed.
    pub fn octets(&self) -> Option<Vec<u8>> {
        decode_base64(&self.octets)
    }
}

const BASE64_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn encode_base64(octets: &[u8]) -> String {
    let mut out = String::with_capacity(octets.len().div_ceil(3) * 4);
    for chunk in octets.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = u32::from(chunk.get(1).copied().unwrap_or(0));
        let b2 = u32::from(chunk.get(2).copied().unwrap_or(0));
        let word = (b0 << 16) | (b1 << 8) | b2;
        out.push(char::from(BASE64_ALPHABET[(word >> 18) as usize & 0x3f]));
        out.push(char::from(BASE64_ALPHABET[(word >> 12) as usize & 0x3f]));
        out.push(if chunk.len() > 1 {
            char::from(BASE64_ALPHABET[(word >> 6) as usize & 0x3f])
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            char::from(BASE64_ALPHABET[word as usize & 0x3f])
        } else {
            '='
        });
    }
    out
}

fn base64_value(byte: u8) -> Option<u32> {
    match byte {
        b'A'..=b'Z' => Some(u32::from(byte - b'A')),
        b'a'..=b'z' => Some(u32::from(byte - b'a') + 26),
        b'0'..=b'9' => Some(u32::from(byte - b'0') + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

fn decode_base64(text: &str) -> Option<Vec<u8>> {
    let bytes = text.as_bytes();
    if bytes.is_empty() {
        return Some(Vec::new());
    }
    if !bytes.len().is_multiple_of(4) {
        return None;
    }
    let groups = bytes.len() / 4;
    let mut out = Vec::with_capacity(groups * 3);
    for g in 0..groups {
        let group = &bytes[g * 4..g * 4 + 4];
        let is_last = g + 1 == groups;
        let pad = group.iter().filter(|b| **b == b'=').count();
        let pad_at_tail = match pad {
            0 => true,
            1 => group[3] == b'=',
            2 => group[2] == b'=' && group[3] == b'=',
            _ => false,
        };
        if !pad_at_tail || (pad > 0 && !is_last) {
            return None;
        }
        let v0 = base64_value(group[0])?;
        let v1 = base64_value(group[1])?;
        let v2 = if pad >= 2 { 0 } else { base64_value(group[2])? };
        let v3 = if pad >= 1 { 0 } else { base64_value(group[3])? };
        // The encode zeroes the bits no octet fills, so a set bit there is
        // a second spelling of the same octets and is refused.
        if pad == 2 && v1 & 0x0f != 0 {
            return None;
        }
        if pad == 1 && v2 & 0x03 != 0 {
            return None;
        }
        let word = (v0 << 18) | (v1 << 12) | (v2 << 6) | v3;
        out.push((word >> 16) as u8);
        if pad < 2 {
            out.push((word >> 8) as u8);
        }
        if pad < 1 {
            out.push(word as u8);
        }
    }
    Some(out)
}

/// The decode seam's ask, per `weaver-types-Spec` section 4.4: the cases are
/// `weaver-harness-spu-decode-contract` section 2's four exchanges read from
/// the harness's side, the seam's fifth message being the SPU's emitted
/// report, owed nothing back and taking its wire case with `FaultReport`'s
/// shape, and this crate holds rather than creates them. The
/// messages are `weaver-traits`' [`weaver_traits::Message`], drawn rather than
/// restated.
///
/// **No sampling value crosses this seam inbound.** The tunable map left this
/// directive when the values moved to the declaration, per `weaver-spu-Spec`
/// section 8: the engine builds its sampler once at session open, so a value
/// arriving with a turn had no engine to reach. The value discipline that
/// guarded the map moved with it to [`crate::config::DecoderInstruction`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TokenDirective {
    Open {
        session: SessionId,
        messages: Vec<weaver_traits::Message>,
        /// The column ask, per `weaver-spu-PRD` section 13.7's cadence
        /// election: it crosses once, at session open, and the one answer
        /// runs for the residency. A serving harness never writes it, the
        /// discipline watched on the harness, and an ask against any arm
        /// of that clause's registry refuses typed at the open. Defaulted
        /// so a directive written before the member existed asks nothing.
        #[serde(default)]
        column_ask: bool,
    },
    AppendAndGenerate {
        turn: TurnKey,
        delta: Vec<weaver_traits::Message>,
    },
    /// The re-feed drive, per `weaver-spu-PRD` section 13.14 and the decode
    /// contract's sixth exchange: the recorded generation's rendered
    /// contribution - the string `model.request` recorded, tokenized and
    /// appended with no family rendering on the way, per the ruling of
    /// 2026-08-12 - and the recorded token path the forward passes run
    /// along. The drive draws no token of its own.
    ReFeed {
        turn: TurnKey,
        rendered: String,
        path: Vec<u32>,
    },
    Cancel {
        turn: TurnKey,
    },
    /// The flush names its cut, per the decode contract as amended
    /// 2026-08-19: `keep` is the resident length the session returns to,
    /// bounded by the seam below at the identity prefix and above at the
    /// resident count, zero being the prefix-only state the flush has
    /// always meant.
    Flush {
        keep: u64,
    },
    /// The elision names a half-open span of resident positions, per the
    /// decode contract as amended 2026-08-22 and `weaver-spu-PRD` section
    /// 13.13: `from` inclusive, `to` exclusive, and what the span covers
    /// leaves the resident sequence while everything else keeps its order.
    ///
    /// **The pair says what leaves where the flush's `keep` says what
    /// stays.** They also take opposite rules at their edges: an
    /// over-large `keep` bounds, and a span describing no removable region
    /// refuses, there being a smaller true version of the first and none of
    /// the second.
    ///
    /// **These positions index the resident sequence and never a trace.**
    Elide {
        from: u64,
        to: u64,
    },
}

/// The decode seam's answer, per `weaver-types-Spec` section 4.4. A cancel
/// with nothing in flight answers `AtRest` rather than refusing, an answer
/// because it is not a failure of the ask. The SPU's fault report takes no
/// case here: it is the seam's one emission, owed nothing back per the
/// ruling of 2026-08-12, and its wire case arrives with `FaultReport`'s
/// shape.
///
/// Adjacently tagged under the spliced-member arm of section 4.3's test:
/// `Generated` wraps the one struct in the vocabulary carrying a `RawValue`,
/// and internal tagging was measured failing the round trip at
/// deserialization, an invalid-type error at the splice, because the tagged
/// content is buffered on the read side and the buffer cannot represent
/// pre-serialized JSON. A wire type one party can write and the other cannot
/// read is not a wire type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "body", rename_all = "snake_case")]
pub enum TokenAnswer {
    Opened,
    /// The stream's intermediate, per the contract's section 2 as of the
    /// streaming ruling: any number cross before the close and none closes
    /// the exchange, which stays `Generated`'s alone. The identifier is a
    /// bare `u32` because the wire's numbers are, and the piece is the
    /// family's rendering that became emittable at this token, which is the
    /// same text a consumer accumulating pieces holds as the emission grows.
    Token {
        token: u32,
        piece: String,
    },
    /// The probability field at one decode position, per
    /// `weaver-harness-spu-decode-contract` section 2 as of 2026-08-21 and
    /// `weaver-spu-PRD` section 13.11: the ranked candidates with their
    /// probabilities and the rank the draw landed on. An intermediate like
    /// [`TokenAnswer::Token`], closing nothing.
    ///
    /// **Its own message rather than a member of the token's**, and the
    /// reason is mechanical: a token message crosses once per renderable
    /// piece and a piece may span several tokens, so a field carried inside
    /// one would be paired with a position it does not name. It carries its
    /// position and a consumer joins on that.
    ///
    /// **Absent where the election is absent**, the message simply not
    /// sent, so a consumer counts what was produced rather than filtering
    /// what was not.
    Field {
        position: u64,
        ranked: Vec<Candidate>,
        realized: u32,
    },
    /// The decode seam's third intermediate, per `weaver-spu-PRD` section
    /// 13.7 and the decode contract as amended 2026-08-31: one sampled
    /// position's residual columns, the tap's own copy per layer at the
    /// width the artifact sets, crossing where the column ask stands and
    /// never otherwise. Carries its position like `Field` and closes
    /// nothing. The layers ride layer-major in section 4.4's provisional
    /// bare JSON, the efficient framing staying `weaver-spu-Spec` section
    /// 12's open election.
    Column {
        position: u64,
        layers: Vec<Vec<f32>>,
    },
    Generated(Generation),
    /// The re-feed's own answer, per `weaver-spu-PRD` section 13.14: the
    /// generation's payload under its own arm, the recomputed draw
    /// identifiers sitting where sampled identifiers would, so a supplied
    /// path can never wear a sampled path's clothes - the variant and not
    /// the payload is what the type buys, and the protoautonomic collapse
    /// is unrepresentable rather than discouraged.
    ReFed(Generation),
    AtRest,
    /// The flush confirmed, carrying both resident counts: the SPU is the
    /// one authority on either number, and the harness authors the
    /// record's flush event from exactly them, per the decode contract.
    Flushed {
        resident_before: u64,
        resident_after: u64,
    },
    /// The elision confirmed, carrying both resident counts and echoing no
    /// span: the harness named the span and needs no confirmation of its
    /// own ask, the SPU is the one authority on either count, and each
    /// party writes what it is the authority on, per the decode contract.
    Elided {
        resident_before: u64,
        resident_after: u64,
    },
    /// The seam's one SPU-originated emission, per the decode contract's
    /// second ruling of 2026-08-12: a case of the answer because the
    /// SPU-to-harness traffic is one enum, and the prose fact the type cannot
    /// carry is the Spec's - **it closes no exchange and answers nothing**,
    /// the trace entry being the acknowledgment. A fault arising inside a
    /// generation is that exchange's typed answer instead, so one fact never
    /// travels twice.
    Fault(FaultReport),
}

/// What a seam turned away, as the record carries it, per
/// `weaver-types-Spec` section 4 and `weaver-trace-PRD` section 3.1's
/// twenty-first kind.
///
/// **The seam is named and the case is the seam's own**, so a consumer
/// dispatches on the variant. That is what typing this here buys over a
/// reason field the trace would carry as prose, and it is defined in this
/// crate because `weaver-trace` depends on no crate of this program and a
/// refusal typed there would make it hold and version four seam
/// vocabularies.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "seam", rename_all = "snake_case")]
pub enum RefusalRecord {
    Decode {
        asked: TokenAsk,
        refusal: TokenRefusal,
    },
    Lifecycle {
        asked: LifecycleAsk,
        refusal: LifecycleRefusal,
    },
    /// **Classify carries no ask**, its content standing in the
    /// `classify.request` event the harness authored before the exchange.
    Classify { refusal: LabelRefusal },
}

/// Which decode ask a refusal answered.
///
/// **A value rides the ask when no other event carries it, and is named and
/// left alone when one does**, per `weaver-types-Spec` section 4. The open's
/// messages reach `message.system`, the append's delta is authored as the
/// turn's message kinds before the exchange, and the cancel's turn is the
/// envelope's field on every event. A refused flush's cut and a refused
/// elision's span reach no event at all, the `elision` kind recording only
/// removals that happened, so those two carry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "ask", rename_all = "snake_case")]
pub enum TokenAsk {
    Open,
    AppendAndGenerate,
    /// The re-feed drive, per the decode contract's sixth exchange.
    ReFeed,
    Cancel,
    Flush {
        keep: u64,
    },
    Elide {
        from: u64,
        to: u64,
    },
}

/// Which lifecycle ask a refusal answered, named without reproducing the
/// declaration, which the load event's own posture carries.
///
/// **An enter refused before its bracket is established has no run record**,
/// the load event being what opens the run, so such a refusal reaches the
/// operator's answer and nothing else. That hole is named in
/// `weaver-types-Spec` section 4 rather than closed by this type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "ask", rename_all = "snake_case")]
pub enum LifecycleAsk {
    Enter,
    Leave,
    Stop,
    Observe,
    Admit,
    Release,
}

/// The decode seam's refusal, per `weaver-types-Spec` section 4.4: the four
/// cases are the decode contract's section 5 and this type adds none.
/// `OutOfOrder` is the same word loop 0's refusal carries and is not the same
/// case, the states it is judged against being the decode seam's, which is why
/// this trio carries its own rather than drawing loop 0's.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TokenRefusal {
    NotOpen,
    OutOfOrder,
    Overflow {
        resident: u64,
        requested: u64,
        capacity: u64,
    },
    MalformedDelta,
    /// The re-feed registry's first arm, per `weaver-spu-PRD` section
    /// 13.14: the drive arrived against an instruction carrying no admitted
    /// permission. A unit variant: the fact refused carries no value the
    /// record holds nowhere else.
    RefeedPermissionAbsent,
    /// The registry's second arm: an empty recorded path, a replay of
    /// nothing wearing an exchange.
    RefeedPathEmpty,
    /// The open's column registry, per `weaver-spu-PRD` section 13.7: three
    /// arms and no others, each a unit variant because the fact refused
    /// carries no value the record holds nowhere else. The instruction
    /// carries no admitted column permission.
    ColumnPermissionAbsent,
    /// The readout was not elected at admit: no election means no tap runs
    /// and no column exists to continue.
    ColumnReadoutUnelected,
    /// The family's declaration holds no column, per `weaver-spu-Spec`
    /// section 7: judged at the open rather than at admit because admission
    /// cannot know an ask will come, and norms-alone against an untapped
    /// column is a working configuration.
    ColumnUndeclared,
    /// The elision's span describes no removable region, per the decode
    /// contract as amended 2026-08-22: it overlaps the identity prefix,
    /// runs past the resident count, ends before it starts, or is empty.
    ///
    /// **It carries the span it refused and the bounds it was judged
    /// against**, because the loop reads a refusal to learn which edge it
    /// crossed. A refusal saying only that the ask was wrong sends the loop
    /// back to guess between four cases it could have distinguished.
    UnremovableSpan {
        from: u64,
        to: u64,
        prefix: u64,
        resident: u64,
    },
}

/// The label seam's ask, per `weaver-types-Spec` section 4.5: the content to
/// classify and the turn identity where one stands, optional because apex
/// invariant 5.3 is conditional on an existing turn and the loop that
/// classifies between turns belongs to none.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LabelDirective {
    Classify {
        turn: Option<TurnKey>,
        content: String,
    },
}

/// The label seam's answer, per `weaver-types-Spec` section 4.5. `Ready` is
/// the readiness emission, the seam's first message and the fan-out arm's
/// confirmation. `Scored` carries every label of the artifact's head, none
/// elided and none beyond, the turn identity echoed exactly as it arrived.
///
/// Adjacently tagged under the spliced-member arm of section 4.3's test:
/// `Fault` wraps `FaultReport`, whose account is a spliced `RawValue`, the
/// same fact that shaped `TokenAnswer`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "body", rename_all = "snake_case")]
pub enum LabelAnswer {
    Ready,
    Scored {
        turn: Option<TurnKey>,
        labels: Vec<ScoredLabel>,
    },
    Fault(FaultReport),
}

/// One label of the artifact's head with its score, the head's softmax: a
/// finite JSON number by construction over finite logits, per
/// `weaver-types-Spec` section 4.5, a scorer producing otherwise having
/// faulted rather than answered.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScoredLabel {
    pub label: String,
    pub score: f64,
}

/// The label seam's typed refusals, per `weaver-types-Spec` section 4.5.
/// `NotAdmitted` is the readiness edge's failure, traveling in the enter
/// aggregate. `Oversized` counts in the artifact's own tokens, the
/// tokenizer's count of the content against the bound the artifact resolved.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LabelRefusal {
    NotAdmitted { reason: String },
    NotReady,
    Oversized { requested: u64, bound: u64 },
    MalformedContent,
}

/// One ranked candidate of the probability field: the token and the
/// probability the distribution gave it at that position, per
/// `weaver-types-Spec` section 4.4.
///
/// The probability rather than the logit, because the consumer's axis is
/// probability and a consumer re-normalising per position would repeat
/// arithmetic the SPU already did over the whole vocabulary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Candidate {
    pub token: u32,
    pub probability: f32,
}

/// What a generation answers with: shaped where the harness consumes and
/// spliced where it forwards, per `weaver-types-Spec` section 4.4. The
/// emission enters the working structure and the finish closes the turn, so
/// both are shaped here. The measurement is pre-serialized JSON written in
/// place, consumed by nothing on the way to the trace's model events, and its
/// conformance to what those events accept is the harness's to enforce at the
/// submit call, the splice being opaque to the compiler.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Generation {
    pub emission: String,
    pub finish: Finish,
    /// The canonical parse of the emission, the family module's bridge from
    /// the verbatim to the conversation's blocks, as of the tool workflow's
    /// opening act: text as text and every recovered call as a `ToolCall`
    /// block, in emission order. Family knowledge stays in the family module
    /// and the canonical form is what crosses, so the harness dispatches
    /// calls it never parsed for. A fragment that opened a call and could
    /// not be recovered is reported by the SPU on its operator channel and
    /// is deliberately not here: a failed call must not arrive as prose.
    pub content: Vec<weaver_traits::ContentBlock>,
    /// The model.request content the SPU rendered whole, the rendered prompt
    /// with its template and effective sampling, spliced into the request
    /// event's box, per `weaver-types-Spec` section 4.4 as of the custody act.
    /// Named `request` and not `rendered` because it carries the whole request
    /// and not the prompt alone.
    pub request: Box<serde_json::value::RawValue>,
    pub measurement: Box<serde_json::value::RawValue>,
    /// The session's token count as this generation closed, terminator
    /// included, and the ceiling the load resolved: the same two numbers
    /// the overflow refusal carries after the wall, carried here so the
    /// asking loop sees pressure before it, per `weaver-types-Spec`
    /// section 4.4. Plain counts with no judgment.
    pub resident: u64,
    pub capacity: u64,
}

impl PartialEq for Generation {
    fn eq(&self, other: &Self) -> bool {
        self.emission == other.emission
            && self.content == other.content
            && self.finish == other.finish
            && self.request.get() == other.request.get()
            && self.measurement.get() == other.measurement.get()
            && self.resident == other.resident
            && self.capacity == other.capacity
    }
}

/// How the generation ended, per `weaver-types-Spec` section 4.4.
/// `weaver-trace` shapes its own `Finish` and the harness converts at one call
/// site, the arrangement `SessionId` and `SessionRef` already take.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Finish {
    Completed,
    Stopped,
    /// The turn's token limit reached, and that limit alone, per
    /// `weaver-types-Spec` section 4.4: a cap that exited as `Completed`
    /// left a reader unable to distinguish a finished answer from a cut
    /// one, per issue #218.
    Length,
}

/// A fault report: what any organ hands the harness across whatever channel it
/// holds.
///
/// **The shape is the closed election of `weaver-types-Spec` section 6**,
/// decided by apex section 5.2's custody rule: the `case` is what the harness
/// itself consumes, typed and closed, because the judgment of what a fault
/// means for the turn and the residency is the harness's to make. The
/// `account` is the reporting organ's own rendering of what happened, spliced
/// verbatim, so the floor carries no organ's descriptive vocabulary and a
/// later organ's richer account grows no shared type.
///
/// **The reporting organ names its own case and the harness classifies
/// nothing.** A report arrives whole, per the contracts, so a case exists
/// before any `fault` event is authored and no raw account reaches the
/// record unclassified.
///
/// No member names the raiser and no member names the turn: the envelope's
/// field and the seam's context respectively, per the Spec's section 4.2.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaultReport {
    pub case: FaultCase,
    pub account: Box<serde_json::value::RawValue>,
}

/// The spliced member compares by its octets, the pattern `Generation` set:
/// `RawValue` carries no `PartialEq` of its own and the derive would be lost
/// with it, so the comparison every test wants is written once here.
impl PartialEq for FaultReport {
    fn eq(&self, other: &Self) -> bool {
        self.case == other.case && self.account.get() == other.account.get()
    }
}

/// The three charters' closed eleven, per `weaver-types-Spec` section 4.2:
/// three of `weaver-spu-PRD` section 13.10, three of `weaver-gate-PRD`
/// section 13.4, five of `weaver-harness-PRD` section 5. **A twelfth case is
/// a charter act before it is a code change**, and the tenth and eleventh
/// were both exactly that: the act typing these found the harness authoring
/// an assembly fault no case covered, and the act answering #369 found it
/// leaving a seated prefix unaccounted for. The charter widened
/// before this enum did, both times. The harness's five ride the same shape
/// although they cross no socket, one shape serving the wire and the `fault`
/// event's payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FaultCase {
    DeviceFaultDuringGeneration,
    ResidencyDegraded,
    ReadoutFaultWhileElected,
    ListenerLost,
    ClientConnectionFailedMidTurn,
    AdmissionFailingSystematically,
    RecorderCommitPressure,
    StreamWriteFailed,
    OrganDeathObserved,
    MessageRecordUndecodable,
    /// The record cannot account for a seated identity prefix, per
    /// `weaver-harness-PRD` section 5. The prefix is seated at the session's
    /// open whether the identity door wrote it or not, so without this case
    /// the record reads as an agent that seated nothing.
    ///
    /// **One case rather than one per cause**, on the custody rule's own
    /// criterion: the case is what the harness consumes, and a refused role,
    /// an unlicensed content block, an unrenderable message, and a recorder
    /// that would not take the write all mean the same thing for the turn and
    /// the residency - nothing, the load standing either way. Which of them
    /// happened is the account's to carry, that being the reporting organ's
    /// own rendering by construction.
    IdentityPrefixUnrecorded,
}
