//! The Granite family, per `weaver-spu-Spec` section 5.
//!
//! **Granite renders qwen2's ChatML turns and opens the model's turn with an
//! empty thought.** The pinned template writes every system and user turn as
//! qwen2's [`qwen2::TEMPLATE`] does, so this module calls qwen2's renderer for
//! those turns and does not spell the template again. What it owns is what
//! differs: the thinking-off generation opener, the two thought markers that
//! opener renders, and the parse that names this family's grammar.
//!
//! **An assistant turn carries the empty thought the opener gave it.** During
//! a live turn the opener puts `<think></think>` in front of what the model
//! emits, so a turn rebuilt from its message after a flush, a recall or a
//! reload must carry the same prefix, or it rebuilds a turn the model never
//! had (Spec section 5). Every assistant turn this module renders followed
//! its own opener, so every one gains the pair, whatever its text holds, and
//! none is trimmed: the live cache held the emission as emitted.
//!
//! **That departs from the pinned template for two kinds of text**, where the
//! template follows a rule of its own and the cache did not:
//!
//! - Text that holds a thought marker anywhere. The template adds no pair to
//!   it, so a turn quoting `</think>` would rebuild two tokens short of the
//!   cache. This module adds the pair.
//! - A closed thought in a turn before the last user turn. The template
//!   (`truncate_history_thinking`, default true) keeps only what follows the
//!   last `</think>`, so `<think>x</think>A1` renders as `<think></think>A1`.
//!   This module keeps the emission as the cache held it, after the pair.
//!
//! The template also trims assistant text, and this module does not.
//!
//! **The R1 path is the one served.** It is one system turn, one user turn and
//! this opener, with no tools. The pinned template departs from qwen2's
//! rendering in three further places, and this module builds none of them:
//!
//! - It writes an empty system turn when no system message leads. This module
//!   writes no turn that the messages do not carry.
//! - It closes a tool response with a newline before the turn close, and
//!   merges consecutive tool responses into one user turn. This module refuses
//!   a tool result.
//! - Its model calls tools in an XML form, `<function=...>` inside
//!   `<tool_call>`. This module refuses a recovered call in a prompt and
//!   reports every emitted call as unrecovered.
//!
//! The tests pin each of these as a named limit or a refusal against the
//! pinned template's own renderings in `granite/goldens.json`.
//!
//! **One further departure is not this module's to refuse.** The template
//! writes tool schemas into its system turn under a `# Tools` preamble.
//! [`super::render_generation`] carries schemas for every family as one
//! trailing user message, which reaches this renderer as plain text and
//! renders as a qwen2 user turn. A renderer cannot tell that message from a
//! user's, so the limit is named here rather than refused.
//!
//! **Admission does not tell Granite 4 from Granite 3.** This is the only
//! entry for the `granite` architecture, so `select` returns it without reading
//! the artifact's template, per Spec section 5's rule for an uncontested
//! architecture. Granite 3 dense artifacts declare the same architecture and
//! render `<|start_of_role|>` turns. Such an artifact is admitted and prompted
//! in this module's ChatML, with no refusal. Closing that is the operator's
//! ruling: a selection that judges an uncontested template, or a measured
//! second row. A test pins today's behaviour.

use super::{
    ContentBlock, Declaration, Family, FamilyName, Markers, Message, Parsed, RenderRefusal, Role,
    lookup, qwen2, render_each, scan, text_content,
};

/// The Hugging Face revision whose `chat_template.jinja` the goldens render.
pub const SOURCE_REVISION: &str = "9e668ce1c538387ef24d3644e9b0606647762636";

/// The sha256 of that template file, which `granite/regenerate.py` checks
/// before it renders.
pub const TEMPLATE_SHA256: &str =
    "f0ba43f79b3cabca5e5a7584c77aec92f49feae45cdf7779c87d9fc54cd90258";

/// The thought markers. The generation opener renders both, so they are
/// inbound and the marker-promotion test of Spec section 10 tokenizes them.
pub const THINK_OPEN: &str = "<think>";
pub const THINK_CLOSE: &str = "</think>";

/// **The markers this family's renderer emits into a prompt**: qwen2's turn
/// markers and the thought pair the opener carries.
pub const RENDERED_MARKERS: &[&str] =
    &[qwen2::TURN_OPEN, qwen2::TURN_CLOSE, THINK_OPEN, THINK_CLOSE];

/// **The markers that select this entry where `granite` is contested**: the
/// turn markers only, as [`super::mistral3::SELECTING_MARKERS`] is a subset of
/// its rendered set.
///
/// `select` reads this against the template detector's own rendering of a
/// probe conversation, not against this module's. The pinned llama.cpp
/// classifies any template containing `<|im_start|>` as ChatML
/// (`llama-chat.cpp`, `llm_chat_detect_template`) and renders the probe with
/// its own ChatML turns and generation prompt, `<|im_start|>assistant\n`,
/// which carry no thought pair. A selecting set carrying the pair could never
/// match.
pub const SELECTING_MARKERS: &[&str] = qwen2::RENDERED_MARKERS;

/// **The assistant's turn, opened, with thinking off.** It is qwen2's opener
/// followed by an empty thought and no newline, which is what the template
/// writes when `enable_thinking` is false. The adapter was trained and scored
/// with this suffix inside the prompt, so the generation starts after
/// `</think>`. The string restates qwen2's opener because a const cannot
/// concatenate one, and a test binds the two.
pub const GENERATION_OPENER: &str = "<|im_start|>assistant\n<think></think>";

const STOP: &[&str] = &[qwen2::TURN_CLOSE];

/// This family's renderer, cited by its registry entry.
pub fn renderer() -> &'static dyn Family {
    &Granite
}

pub struct Granite;

impl Family for Granite {
    fn render_identity(&self, messages: &[Message]) -> Result<String, RenderRefusal> {
        render_each(self, messages)
    }

    /// **qwen2's rendering behind the text-only guard, with the assistant's
    /// empty thought restored.** [`text_content`] accepts text, and an
    /// unrecovered call in the assistant's turn, and refuses every other
    /// block. That refuses a tool result, because the template's tool
    /// response differs from qwen2's by a newline and by merging. It refuses
    /// a recovered call, because qwen2 would write it as JSON and this model
    /// calls tools in XML. Each refusal stands in for a rendering this module
    /// does not build.
    ///
    /// Every assistant turn gains the empty thought as a leading text block,
    /// so qwen2 renders it in front of the turn's own blocks, unchanged and
    /// untrimmed. The condition is the role alone: text that quotes a thought
    /// marker followed the opener as well.
    fn render_delta(&self, message: &Message) -> Result<String, RenderRefusal> {
        text_content(message)?;
        if message.role == Role::Assistant {
            let mut content = Vec::with_capacity(message.content.len() + 1);
            content.push(ContentBlock::Text {
                text: format!("{THINK_OPEN}{THINK_CLOSE}"),
            });
            content.extend(message.content.iter().cloned());
            return qwen2::renderer().render_delta(&Message {
                role: Role::Assistant,
                content,
            });
        }
        qwen2::renderer().render_delta(message)
    }

    /// The call markers are qwen2's, and the call inside them is XML that this
    /// module does not recover. Every attempted call is reported unrecovered
    /// under this family's grammar, and none becomes text.
    fn parse(&self, emission: &str, ended_by: Option<&str>) -> Parsed {
        scan(
            emission,
            ended_by,
            Markers {
                family: "granite",
                call_open: qwen2::CALL_OPEN,
                call_close: Some(qwen2::CALL_CLOSE),
            },
            |_| Err("Granite's XML tool-call recovery is not implemented".into()),
        )
    }

    fn stop_conditions(&self) -> &'static [&'static str] {
        STOP
    }

    fn declaration(&self) -> &'static Declaration {
        lookup(&FamilyName("granite".into())).expect("the registry carries granite")
    }
}

#[cfg(test)]
mod tests;
