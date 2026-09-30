//! conforms: trace-kind-payload-mapping-total
//!
//! **The kind set's count, read by the compiler rather than by a reader.**
//! `weaver-trace-Spec` section 3 states the mapping is total over twenty-four
//! kinds and eighteen dispositions, and records that the recount has gone wrong
//! twice, the second time silently, both halves standing at eighteen kinds
//! against a crate compiling thirteen. Until this file the count stood in prose
//! and in no instrument, and a mapping that is stale reads exactly like a
//! mapping that is total.
//!
//! **What this reaches is the kind set and not the mapping.** The array's
//! declared length fixes the number, and `ordinal`'s match, exhaustive and
//! wildcard-free, is what a kind added to the enum has to be written into.
//! The array is written out rather than derived, so the two assertions below
//! are what close the distance between the two.
//!
//! **What a grown kind set stops on here is `ordinal`, and nothing else.** An
//! act that adds a kind and answers the lib's own matches stops at this file,
//! whose match no longer covers the enum. **An act that also writes the new
//! arm here and leaves `ALL` alone passes**, whatever that arm returns, both
//! assertions walking `ALL` and so never reaching a kind the array does not
//! name. The number can still drift by one act. Closing that wants the array
//! written against the match rather than beside it, which is a representation
//! election and issue #633's rather than this file's.
//!
//! Neither the disposition count nor the mapping's totality is reached, both
//! being properties of `pairing_licensed`, which is crate-private and which
//! this file cannot see from outside the crate. **That is this act's placement
//! and not a property of the crate**: `writer.rs` carries no test module today
//! and a unit test there would see the function and reach all three claims. The
//! cited record keeps its tag because this file reaches one of the three, and
//! the split that would let the tag move is issue #633 rather than an
//! assumption left in prose.

use weaver_trace::Kind;

/// Each kind's ordinal, matched exhaustively and with no wildcard arm, so a
/// kind added to the enum leaves this match non-exhaustive and a kind dropped
/// leaves an arm naming nothing. Either stops the build.
fn ordinal(kind: Kind) -> usize {
    match kind {
        Kind::Load => 0,
        Kind::Unload => 1,
        Kind::SessionClosed => 2,
        Kind::TurnStarted => 3,
        Kind::TurnClosed => 4,
        Kind::MessageSystem => 5,
        Kind::MessageUser => 6,
        Kind::MessageAssistant => 7,
        Kind::MessageToolResult => 8,
        Kind::ToolCallStarted => 9,
        Kind::ToolCallCompleted => 10,
        Kind::Fault => 11,
        Kind::Elision => 12,
        Kind::Refusal => 13,
        Kind::Flush => 14,
        Kind::ModelRequest => 15,
        Kind::ModelOutput => 16,
        Kind::ModelMeasurement => 17,
        Kind::ModelField => 18,
        Kind::ClassifyRequest => 19,
        Kind::ClassifyOutput => 20,
        Kind::Recall => 21,
        Kind::MessageRestored => 22,
        Kind::Score => 23,
    }
}

/// **The kind set is twenty-four, and each of the twenty-four stands in it
/// once**, per `weaver-trace-Spec` section 3.
///
/// The array's declared length is the count and the compiler checks it. The
/// two assertions read different objects, which is why each can fail with the
/// other passing.
///
/// The first is about `ALL` alone and reaches no match: no kind is named
/// twice. Perturbation: name `Kind::ClassifyRequest` in place of
/// `Kind::ClassifyOutput` and it fails, the twenty-four entries no longer
/// naming twenty-four kinds.
///
/// The second is about `ordinal` against `ALL`: every ordinal the exhaustive
/// match produces is reached. **Marking happens without asserting**, so a
/// collision leaves a slot unset for this assertion to find rather than
/// stopping the walk where the pigeonhole would make the second unreachable.
/// Perturbation: return 19 from the `Kind::ClassifyOutput` arm of `ordinal`
/// and it fails while the first passes, `ALL` being untouched and its
/// twenty-four kinds still distinct.
#[test]
fn the_kind_set_is_twenty_four() {
    const ALL: [Kind; 24] = [
        Kind::Load,
        Kind::Unload,
        Kind::SessionClosed,
        Kind::TurnStarted,
        Kind::TurnClosed,
        Kind::MessageSystem,
        Kind::MessageUser,
        Kind::MessageAssistant,
        Kind::MessageToolResult,
        Kind::ToolCallStarted,
        Kind::ToolCallCompleted,
        Kind::Fault,
        Kind::Elision,
        Kind::Refusal,
        Kind::Flush,
        Kind::ModelRequest,
        Kind::ModelOutput,
        Kind::ModelMeasurement,
        Kind::ModelField,
        Kind::ClassifyRequest,
        Kind::ClassifyOutput,
        Kind::Recall,
        Kind::MessageRestored,
        Kind::Score,
    ];

    for (at, kind) in ALL.iter().enumerate() {
        assert!(
            !ALL[..at].contains(kind),
            "the twenty-four entries name one kind twice: {kind:?}"
        );
    }

    let mut reached = [false; ALL.len()];
    for kind in ALL {
        reached[ordinal(kind)] = true;
    }
    assert!(
        reached.iter().all(|seen| *seen),
        "every kind the enum declares stands in the set: {reached:?}"
    );
}
