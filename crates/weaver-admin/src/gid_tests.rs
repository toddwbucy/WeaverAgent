//! Test-only (#118): the `gid_tests` module, moved from `src/main.rs` verbatim.

use super::merge_gids;

/// **The group the unit sets is in the set, and the passwd primary alone
/// is not the set.**
///
/// This is the custody hole olympus found on 2026-08-29: the unit forces
/// `Group={identity}` so the worker's egid is `weaver-<agent>`, while the
/// denial walk read the passwd primary. Where the operator provisioned a
/// shared primary - `users`, `nogroup` - the two disagree, and a sink at
/// `root:weaver-karl` mode `0710` passes a walk the running worker can
/// then traverse to rewrite the trace admin holds custody of.
///
/// Perturbation: returning `vec![primary]` fails this. Watched failing
/// 2026-08-29.
///
/// conforms: admin-boundary-reads-every-gid-the-worker-holds
#[test]
fn the_walk_reads_the_group_the_unit_sets_and_not_passwd_alone() {
    // The shared-primary case the mode's own justification cites.
    let gids = merge_gids(100, Some(2001), &[100]);
    assert!(
        gids.contains(&2001),
        "the egid the unit sets is walked: {gids:?}"
    );
    assert!(gids.contains(&100), "and the passwd primary too: {gids:?}");

    // Supplementary memberships reach the sink as well.
    let gids = merge_gids(100, Some(2001), &[100, 27, 998]);
    assert!(
        [100, 2001, 27, 998].iter().all(|gid| gids.contains(gid)),
        "every gid the worker holds is walked: {gids:?}"
    );

    // Deduped, and the primary stays first: the walk asks `contains`, so
    // a repeat is only noise, but a set that grows per call is a leak.
    assert_eq!(merge_gids(100, Some(100), &[100, 100]), vec![100]);
    assert_eq!(merge_gids(100, None, &[7]), vec![100, 7]);
}
