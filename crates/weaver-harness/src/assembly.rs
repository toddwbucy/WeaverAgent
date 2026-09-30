//! conforms: harness-assembly-kind-filter-at-read-site
//! conforms: harness-prompt-part-order
//! conforms: harness-deterministic-assembly
//! conforms: harness-provider-injected-no-wire-format
//!
//! Prompt assembly's deterministic floor, per `weaver-harness-Spec` section 5.
//! The per-model layer is deferred with the token workflow; what is fixed now
//! is the order of parts and the property that assembly is deterministic over
//! the working structure's contents.

use weaver_trace::{Kind, WorkingStructure};
use weaver_traits::Message;

/// The three message kinds, and nothing else. The filter is the kind set at
/// the read site rather than a judgment applied after a full read, so the
/// measurement, lifecycle, and custody events never enter a prompt because the
/// assembly path cannot see them.
const MESSAGE_KINDS: [Kind; 3] = [
    Kind::MessageUser,
    Kind::MessageAssistant,
    Kind::MessageToolResult,
];

/// The parts of a prompt, in the order apex section 3 step 4 fixes: the
/// identity prefix, then the message sequence, then the tool schemas. The
/// per-model phrasing is re-erected per decoder and is not this crate's.
#[derive(Debug, Clone, PartialEq)]
pub struct Prompt {
    pub identity: String,
    pub messages: Vec<Message>,
    pub tool_schemas: Vec<String>,
    /// **Message-kind records that did not decode**, counted rather than
    /// dropped in silence. Every message this crate authors renders as a
    /// `Message`, so this is zero today - but a second producer of
    /// `Payload::Message`, or a change to the message model, would otherwise
    /// turn a decode failure into a context with a hole in it that nothing
    /// reports. A non-zero count is a fault the caller authors.
    pub undecodable: usize,
}

impl Prompt {
    /// The rendered order, which is what determinism is asserted over: the
    /// same records assemble the same prompt, byte for byte.
    pub fn render(&self) -> String {
        let mut out = String::new();
        out.push_str(&self.identity);
        out.push('\n');
        for message in &self.messages {
            out.push_str(&serde_json::to_string(message).unwrap_or_default());
            out.push('\n');
        }
        for schema in &self.tool_schemas {
            out.push_str(schema);
            out.push('\n');
        }
        out
    }
}

/// Assembles a prompt by iterating the working structure in sequence order,
/// selecting on the lifted `kind` the index holds for exactly this read,
/// taking the three message kinds, and decoding their payloads.
///
/// Iteration is sequence order because the structure is append-only and its
/// iterator yields landing order, which is what makes the assembly
/// deterministic over one structure's contents.
pub fn assemble(structure: &WorkingStructure, identity: &str, tool_schemas: &[String]) -> Prompt {
    let mut messages = Vec::new();
    let mut undecodable = 0;
    for record in structure.iter() {
        if !MESSAGE_KINDS.contains(&record.kind) {
            continue;
        }
        match decode_message(record.line.as_ref()) {
            Some(message) => messages.push(message),
            None => undecodable += 1,
        }
    }
    Prompt {
        identity: identity.to_string(),
        messages,
        tool_schemas: tool_schemas.to_vec(),
        undecodable,
    }
}

/// One JSON parse per message per assembly, the cost `weaver-trace-Spec`
/// section 4 states rather than hides: a decoded cache is the second
/// representation the working-structure ruling retired.
fn decode_message(line: &str) -> Option<Message> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    let payload = value.get("payload")?;
    serde_json::from_value(payload.clone()).ok()
}
