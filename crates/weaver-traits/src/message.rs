//! conforms: traits-role-kind-one-to-one
//! conforms: traits-content-is-block-sequence
//! conforms: traits-role-plain-string
//! conforms: traits-content-block-internally-tagged
//! conforms: traits-non-exhaustive-per-charter
//! conforms: traits-serde-from-derives
//!
//! The message model: provider-agnostic conversation content, per
//! `weaver-traits-Spec` section 3. The harness assembles prompts from it and the
//! trace records `message.user`, `message.assistant`, and `message.tool_result`
//! payloads carrying it, opaque to the recorder.

use serde::{Deserialize, Serialize};

/// One conversation message: a role and a sequence of content blocks.
///
/// Content is a sequence rather than a string because an assistant turn is not
/// one: prose and tool calls arrive in one emission and stay distinguishable from
/// authoring through to the trace.
///
/// The licensed role-to-block combinations are stated in `weaver-traits-Spec`
/// section 3 and enforced by the harness, the sole author; the recorder judges
/// the envelope and never the interior, so the rule is not represented here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: Vec<ContentBlock>,
}

/// The message's role, mapping one to one onto the trace's four
/// `message.*` event kinds.
///
/// Fieldless, so it serializes as a plain renamed string, `"user"`, per the
/// tagging test the two floor Specs share. The set grows with the conversation
/// model, so the enum is non-exhaustive and consumers keep a wildcard arm.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// The framing of the field, the operator's or the loop's, per the
    /// system role act of 2026-08-19: carries `Text` blocks only, never a
    /// call and never a result, and where it sits in a conversation is
    /// the loop's composition and no rule of this shape.
    System,
    User,
    Assistant,
    ToolResult,
}

/// One block of a message's content.
///
/// Every variant is struct-shaped or wraps a struct, so the enum is internally
/// tagged and a block reads as `{"type": "text", "text": "..."}`, keyed on a
/// stable member name for the non-Rust consumer on the stream. The set grows
/// with the conversation model, so the enum is non-exhaustive.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text { text: String },
    ToolCall(ToolCall),
    ToolResult(ToolResultBlock),
}

/// The conversation's view of a tool invocation, shaped with the tool
/// workflow's opening act per `weaver-traits-Spec` section 3: the name and
/// arguments exactly as the family parse recovered them from the emission, so
/// the record holds what the model spoke. The name is a primitive for section
/// 5's cycle reason - this crate is the floor-link `weaver-types` names, so
/// no type of that crate can appear here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    pub name: String,
    pub arguments: String,
}

/// The conversation's view of a tool result's content, shaped with the tool
/// workflow's opening act: what a family renders into the tool-result turn.
///
/// **A record and not a grant.** This block deserializes wherever the
/// conversation crosses a seam, the decode seam above all, and what it grants
/// is nothing: the capability to author a tool-result message is the
/// harness's granted value, constructed at the execution exchange's
/// completion alone, per `weaver-harness-Spec` section 6.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolResultBlock {
    pub content: String,
}
