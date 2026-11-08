//! The Granite family, per `weaver-spu-Spec` section 5.
//!
//! **Granite renders ChatML turns and opens the model's turn with an empty
//! thought.** IBM's pinned template writes every system and user turn in
//! ChatML, a form this lineage adopted by choice, as it adopted an XML tool
//! call. Every fact the module renders with is its own copy: the turn markers,
//! the template, the call markers, the delta renderer, the thinking-off
//! generation opener, and the two thought markers that opener renders. No
//! other family's change can move this one, because model configurations stay
//! segregated by family (the operator's ruling of 2026-11-08) and a lineage can
//! change its template at any time.
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
//! this opener, with no tools. The pinned template has three further forms,
//! and this module builds none of them:
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
//! renders as a user turn. A renderer cannot tell that message from a user's,
//! so the limit is named here rather than refused.
//!
//! **Admission does not tell Granite 4 from Granite 3.** This is the only
//! entry for the `granite` architecture, so `select` returns it without reading
//! the artifact's template, per Spec section 5's rule for an uncontested
//! architecture. Granite 3 dense artifacts declare the same architecture and
//! render `<|start_of_role|>` turns. Such an artifact is admitted and prompted
//! in this module's ChatML, with no refusal. Closing that is #483. A test pins
//! today's behaviour.

use super::{
    Declaration, Family, FamilyName, Markers, Message, Parsed, RenderRefusal, Role,
    common_role_name, lookup, render_each, render_template, scan, text_content,
};

/// The Hugging Face revision whose `chat_template.jinja` the goldens render.
pub const SOURCE_REVISION: &str = "9e668ce1c538387ef24d3644e9b0606647762636";

/// The sha256 of that template file, which `granite/regenerate.py` checks
/// before it renders.
pub const TEMPLATE_SHA256: &str =
    "f0ba43f79b3cabca5e5a7584c77aec92f49feae45cdf7779c87d9fc54cd90258";

/// The ChatML turn markers as this family's template writes them.
pub const TURN_OPEN: &str = "<|im_start|>";
pub const TURN_CLOSE: &str = "<|im_end|>";

/// What opens and closes a call in this family's emissions. The call between
/// them is XML, which this module does not recover.
pub const CALL_OPEN: &str = "<tool_call>";
pub const CALL_CLOSE: &str = "</tool_call>";

/// The thought markers. The generation opener renders both, so they are
/// inbound and the marker-promotion test of Spec section 10 tokenizes them.
pub const THINK_OPEN: &str = "<think>";
pub const THINK_CLOSE: &str = "</think>";

/// This family's turn template, the one authority for it. The registry cites
/// this constant and [`Family::render_delta`] renders through it. The string
/// restates [`TURN_OPEN`] and [`TURN_CLOSE`] because a const cannot
/// concatenate them, and a test binds the three.
pub const TEMPLATE: &str = "<|im_start|>{role}\n{message}<|im_end|>\n";

/// **The markers this family's renderer emits into a prompt**: the turn
/// markers and the thought pair the opener carries.
pub const RENDERED_MARKERS: &[&str] = &[TURN_OPEN, TURN_CLOSE, THINK_OPEN, THINK_CLOSE];

/// **The markers that select this entry where `granite` is contested**: the
/// turn markers only, a subset of the rendered set.
///
/// `select` reads this against the template detector's own rendering of a
/// probe conversation, not against this module's. The pinned llama.cpp
/// classifies any template containing `<|im_start|>` as ChatML
/// (`llama-chat.cpp`, `llm_chat_detect_template`) and renders the probe with
/// its own ChatML turns and generation prompt, `<|im_start|>assistant\n`,
/// which carry no thought pair. A selecting set carrying the pair could never
/// match.
pub const SELECTING_MARKERS: &[&str] = &[TURN_OPEN, TURN_CLOSE];

/// **The assistant's turn, opened, with thinking off.** It is the turn opener
/// for the assistant followed by an empty thought and no newline, which is
/// what the template writes when `enable_thinking` is false. The adapter was
/// trained and scored with this suffix inside the prompt, so the generation
/// starts after `</think>`. The string restates the markers because a const
/// cannot concatenate them, and a test binds them.
pub const GENERATION_OPENER: &str = "<|im_start|>assistant\n<think></think>";

const STOP: &[&str] = &[TURN_CLOSE];

/// This family's renderer, cited by its registry entry.
pub fn renderer() -> &'static dyn Family {
    &Granite
}

pub struct Granite;

impl Family for Granite {
    fn render_identity(&self, messages: &[Message]) -> Result<String, RenderRefusal> {
        render_each(self, messages)
    }

    /// **Text turns only, with the assistant's empty thought restored.**
    /// [`text_content`] accepts text, and an unrecovered call in the
    /// assistant's turn, and refuses every other block. That refuses a
    /// recovered call, because this model calls tools in an XML form this
    /// module does not build. A tool result refuses by its role as well as by
    /// its blocks, because the template's tool response is a user turn with
    /// its own bracket and newlines, which this module does not build either.
    /// Each refusal stands in for a rendering this module does not have.
    ///
    /// Every assistant turn opens its body with the empty thought, in front of
    /// the turn's own text, unchanged and untrimmed. The condition is the role
    /// alone: text that quotes a thought marker followed the opener as well.
    fn render_delta(&self, message: &Message) -> Result<String, RenderRefusal> {
        if message.role == Role::ToolResult {
            return Err(RenderRefusal::MalformedForFamily);
        }
        let text = text_content(message)?;
        let body = if message.role == Role::Assistant {
            format!("{THINK_OPEN}{THINK_CLOSE}{text}")
        } else {
            text
        };
        Ok(render_template(
            TEMPLATE,
            common_role_name(&message.role)?,
            &body,
        ))
    }

    /// The call inside this family's call markers is XML that this module
    /// does not recover. Every attempted call is reported unrecovered under
    /// this family's grammar, and none becomes text.
    fn parse(&self, emission: &str, ended_by: Option<&str>) -> Parsed {
        scan(
            emission,
            ended_by,
            Markers {
                family: "granite",
                call_open: CALL_OPEN,
                call_close: Some(CALL_CLOSE),
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
