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
