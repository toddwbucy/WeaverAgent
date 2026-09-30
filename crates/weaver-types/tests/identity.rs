//! conforms: types-denial-precedes-permission
//!
//! The denial-precedence test of `weaver-types-Spec` section 3, derived from the
//! threat walk: the adversary is a process on the host that is not a front-end
//! principal, and admission rests on identity rather than reachability.

use std::collections::BTreeSet;

use weaver_types::{AccessRule, PeerIdentity, authorized};

fn rule(allowed_uids: &[u32], allowed_gids: &[u32], denied_uids: &[u32]) -> AccessRule {
    AccessRule {
        allowed_uids: BTreeSet::from_iter(allowed_uids.iter().copied()),
        allowed_gids: BTreeSet::from_iter(allowed_gids.iter().copied()),
        denied_uids: BTreeSet::from_iter(denied_uids.iter().copied()),
    }
}

/// `authorized` returns false for a peer whose uid is the agent's against a
/// rule whose allow sets include the agent's group.
///
/// Perturbation: reorder the predicate so permission is evaluated first and
/// this returns true - the agent's group membership readmits the one principal
/// the boundary exists to keep out. Watched to fail under exactly that
/// reordering.
#[test]
fn denial_precedes_permission() {
    let agent = PeerIdentity {
        uid: 1701,
        gid: 900,
        pid: 4242,
    };
    let boundary = rule(&[], &[900], &[1701]);
    assert!(!authorized(&agent, &boundary));

    let front_end = PeerIdentity {
        uid: 1000,
        gid: 900,
        pid: 5555,
    };
    assert!(authorized(&front_end, &boundary));
}

/// Admission is by uid or by group, and a peer in neither is refused.
#[test]
fn unlisted_peer_is_refused() {
    let boundary = rule(&[1000], &[], &[]);
    assert!(authorized(
        &PeerIdentity {
            uid: 1000,
            gid: 1,
            pid: 1
        },
        &boundary
    ));
    assert!(!authorized(
        &PeerIdentity {
            uid: 1001,
            gid: 1,
            pid: 1
        },
        &boundary
    ));
}

/// The pid never enters the decision: two peers differing only by pid judge
/// identically, because a pid is reused and is unsound as an authorization
/// input.
#[test]
fn pid_is_never_the_basis_of_a_decision() {
    let boundary = rule(&[1000], &[], &[]);
    let first = PeerIdentity {
        uid: 1000,
        gid: 1,
        pid: 10,
    };
    let second = PeerIdentity {
        uid: 1000,
        gid: 1,
        pid: 20,
    };
    assert_eq!(
        authorized(&first, &boundary),
        authorized(&second, &boundary)
    );
}
