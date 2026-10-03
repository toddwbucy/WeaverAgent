//! conforms: types-one-policy-function
//! conforms: types-denial-precedes-permission
//!
//! Peer identity and the authorization predicate, per `weaver-types-Spec`
//! section 3: the identity type, the rule it is judged against, and the one
//! policy function, drawn by the seams that admit an outside principal, two today
//! and a third chartered by the egress ruling of 2026-08-07 whose contract is not
//! written. One
//! shared definition is the only way separate processes provably enforce the
//! same rule, and this crate holds one policy function and gains no second,
//! which is charter enforcement read at review.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

/// What the kernel reports about a connecting peer, via `SO_PEERCRED`.
///
/// Derives `Serialize` and deliberately not `Deserialize`: the kernel supplies
/// it and the peer never asserts it, and a derived `Deserialize` is the
/// machinery for constructing one from bytes that arrived over a socket, one
/// careless call from the substitution `SO_PEERCRED` exists to prevent.
/// Serialization stays, because a refusal that names the peer it refused is
/// worth recording. The absence is pinned by a compile-fail doctest at the
/// crate root.
///
/// `pid` is carried and is never the basis of a decision: `SO_PEERCRED`
/// reports it, so dropping it would be lying by omission, and it is unsound as
/// an authorization input because a pid is reused. It exists for the record and
/// for a diagnostic, and the predicate ignores it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PeerIdentity {
    pub uid: u32,
    pub gid: u32,
    pub pid: i32,
}

/// The rule a peer is judged against, one shared shape rather than one each
/// consumer invents.
///
/// The three sets are what the consumers need and no more: the gate admits
/// front-end principals by uid or group on its world-opened seam, the operator
/// surface admits by group membership, and both exclude the agent uid. The gate's
/// agent-opened seam is a third consumer and needs no fourth set: it admits
/// registered tools by the same uid and group sets and excludes the agent uid by
/// the same denial. What it needs beyond this rule is proof that a uid is the
/// registered tool rather than a squatter, which is the tool-seam contract's and
/// is not a shape this type carries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct AccessRule {
    pub allowed_uids: BTreeSet<u32>,
    pub allowed_gids: BTreeSet<u32>,
    pub denied_uids: BTreeSet<u32>,
}

/// The one policy function: a free function over data that reaches nothing.
///
/// No file, no environment, no clock, no global. Where the rule comes from,
/// how a failure is handled, and what happens on refusal live in the consuming
/// crates.
///
/// Denial wins over permission, and the ordering is the security property: a
/// peer in `denied_uids` is refused whatever the allow sets say, so a broad
/// group grant cannot readmit the one principal the boundary exists to keep
/// out.
pub fn authorized(peer: &PeerIdentity, against: &AccessRule) -> bool {
    if against.denied_uids.contains(&peer.uid) {
        return false;
    }
    against.allowed_uids.contains(&peer.uid) || against.allowed_gids.contains(&peer.gid)
}

/// The agent's `roles.toml`, per `weaver-types-Spec` section 3.1: the one
/// trace reader the trace relay admits, and nothing else. Boundary and never
/// constitution, so it is held apart from the declaration and its digest.
///
/// The lifecycle half a draft carried, role groups mapped to verbs, does not
/// return: who may issue which verb is the sudo rule's, per
/// `weaver-admin-Spec` section 2. An unknown key refuses, as the
/// declaration's do, so a misspelt reader never reads as no reader.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct BoundaryFile {
    /// A user name, and only a user: a group would admit every member of it
    /// to the agent's whole record.
    pub trace_reader: String,
}

/// The trace door's one request, per `weaver-types-Spec` section 3.1: a byte
/// offset on a record boundary and the sha256 hex of the record ending there,
/// absent only at offset zero. Sent once, as one newline-terminated line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TraceRequest {
    pub offset: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prior_digest: Option<String>,
}

/// The file the relay serves, by identity: device, inode, and birth time in
/// nanoseconds since the epoch, so a reader holds what it is reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceHeader {
    pub device: u64,
    pub inode: u64,
    pub birth_ns: i128,
}

/// Every line the trace stream adds, per `weaver-types-Spec` section 3.1: one
/// JSON object whose one member is `trace_stream`, which no trace event
/// carries, so a reader tells the stream's own lines from the record's by
/// that member alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TraceLine {
    pub trace_stream: TraceControl,
}

/// What a stream line says, externally tagged by case: the header first,
/// a heartbeat while idle, and `truncated` when the run's file shrinks below
/// the stream's position, after which the stream ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TraceControl {
    Header(TraceHeader),
    Heartbeat { wall_ms: u64 },
    Truncated { size: u64 },
}

/// Who asked for a change, per `weaver-types-Spec` section 3.1: the uid sudo
/// reports, or 0 at a root shell, read by admin and never from the caller.
/// It rides the enter, the leave and the stop, and the harness records it on
/// the event each one authors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cause {
    pub uid: u32,
}
