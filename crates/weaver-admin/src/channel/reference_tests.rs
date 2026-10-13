//! Test-only (#118): the `reference_tests` module, moved from `src/channel.rs` verbatim.

use super::*;

/// **Two references minted back to back differ**, which is the whole of
/// what the contract's distinctness guarantee asks for. The instants can
/// be equal and the agent is the same, so what this watches is the random
/// part carrying the guarantee alone.
///
/// Perturbation: drop the nonce from `mint_run_reference` and this fails,
/// because two mints inside one millisecond render the same string.
/// Watched under exactly that removal.
#[test]
fn two_references_for_one_agent_differ() {
    let a = mint_run_reference("alpha").expect("randomness reads");
    let b = mint_run_reference("alpha").expect("randomness reads");
    assert_ne!(a.0, b.0, "the reference is not distinct: {} {}", a.0, b.0);
}

/// The reference reads as a date and names its agent, which is what makes
/// an artifact legible without a join.
#[test]
fn the_reference_reads_as_a_date_and_names_its_agent() {
    let r = mint_run_reference("alpha").expect("randomness reads").0;
    assert!(r.contains("-alpha-"), "the agent is not named: {r}");
    assert_eq!(&r[4..5], "-", "not a rendered date: {r}");
    assert!(r.contains('T') && r.contains("Z-"), "not RFC 3339: {r}");
}

/// The civil-date arithmetic is hand-rolled, so it is pinned against
/// instants a reader can check by eye, including a leap day and the epoch.
#[test]
fn the_instant_renders_against_known_values() {
    assert_eq!(rfc3339_millis(0), "1970-01-01T00:00:00.000Z");
    assert_eq!(
        rfc3339_millis(1_709_164_800_000),
        "2024-02-29T00:00:00.000Z"
    );
    assert_eq!(
        rfc3339_millis(1_786_665_731_482),
        "2026-08-14T00:02:11.482Z"
    );
}
