//! conforms: traits-message-round-trip
//! conforms: traits-unknown-tag-refuses
//! conforms: traits-role-plain-string
//! conforms: traits-content-block-internally-tagged
//!
//! The perturbation-verified tests of `weaver-traits-Spec` section 7. Each test
//! names its perturbation: the mutation under which it was watched to fail, per
//! apex section 11's rule that a test passing either way is worse than no test.

use weaver_traits::{ContentBlock, Message, Role, ToolCall};

/// A message round-trips through serialization unchanged, block sequence and
/// roles intact.
///
/// Perturbation: drop a block variant from the assembled message (or reorder the
/// sequence in a hand-written Deserialize) and the equality fails. Verified by
/// removing the `ToolCall` block from the round-tripped value.
#[test]
fn message_round_trips_unchanged() {
    let msg = Message {
        role: Role::Assistant,
        content: vec![
            ContentBlock::Text {
                text: "running the calculator".into(),
            },
            ContentBlock::ToolCall(ToolCall {
                name: "calculator".into(),
                arguments: "{}".into(),
            }),
        ],
    };
    let json = serde_json::to_string(&msg).expect("serializes");
    let back: Message = serde_json::from_str(&json).expect("deserializes");
    assert_eq!(msg, back);
}

/// A role serializes as a plain renamed string, per the tagging test: fieldless,
/// so `"user"` and not `{"type": "user"}`.
///
/// Perturbation: drop `#[serde(rename_all = "snake_case")]` from `Role` and the
/// rename stops happening. Verified by removing that attribute and watching
/// this test fail at its first assertion, `"User"` against `"user"`, so the
/// `ToolResult` assertion below is never reached.
#[test]
fn role_is_a_plain_string() {
    assert_eq!(serde_json::to_string(&Role::User).unwrap(), "\"user\"");
    assert_eq!(
        serde_json::to_string(&Role::ToolResult).unwrap(),
        "\"tool_result\""
    );
}

/// A content block is internally tagged on a stable member name.
///
/// Perturbation: drop `tag = "type"` from `ContentBlock` and serde's default
/// external tagging makes the variant name a key. Verified by removing the tag
/// and watching this test fail at its one assertion,
/// `{"text":{"text":"hello"}}` against `{"type":"text","text":"hello"}`.
#[test]
fn content_block_is_internally_tagged() {
    let block = ContentBlock::Text {
        text: "hello".into(),
    };
    assert_eq!(
        serde_json::to_string(&block).unwrap(),
        "{\"type\":\"text\",\"text\":\"hello\"}"
    );
}

/// An unknown tag on deserialization refuses rather than defaulting.
///
/// Perturbation: a hand-written Deserialize with a fallback arm makes these pass
/// a value through; the derive refuses, and removing the refusal is watched to
/// flip both assertions. Verified by feeding a role this crate does not define -
/// the example was "system" until the system role act made it real, so the
/// unknown here is one no conversation model names.
#[test]
fn unknown_tag_refuses() {
    assert!(serde_json::from_str::<Role>("\"narrator\"").is_err());
    assert!(serde_json::from_str::<ContentBlock>("{\"type\":\"image\",\"data\":\"...\"}").is_err());
}

/// The system role is real since its act: it round-trips as "system" and
/// its licensed content is text alone, per the Spec's combinations.
///
/// This reads `traits-role-plain-string` at the arm the identity door writes
/// and declares no claim of its own, which is why the header above carries
/// four citations over five tests.
///
/// Perturbation: the same one the role test names, dropping
/// `#[serde(rename_all = "snake_case")]` from `Role`. Verified by removing that
/// attribute and watching this test fail at its `from_str`, serde refusing
/// `"system"` for a variant now spelled `System`.
#[test]
fn the_system_role_round_trips() {
    let role: Role = serde_json::from_str("\"system\"").expect("real since its act");
    assert_eq!(serde_json::to_string(&role).unwrap(), "\"system\"");
}
