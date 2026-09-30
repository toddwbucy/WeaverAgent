//! The llama family, per `weaver-spu-Spec` section 5.
//!
//! Everything llama defines sits here: its header markers, the rendering that
//! uses them, the parse of its own emissions, and its stop conditions. The
//! scanning beneath is [`super::scan`], shared and marker-free.

use weaver_types::ToolName;

use super::{Declaration, Family, FamilyName, Markers, Message, Parsed, RenderRefusal, lookup};

/// The header markers. Written here and nowhere else in the crate.
pub const TEXT_BEGIN: &str = "<|begin_of_text|>";
pub const HEADER_OPEN: &str = "<|start_header_id|>";
pub const HEADER_CLOSE: &str = "<|end_header_id|>";
pub const TURN_END: &str = "<|eot_id|>";

/// What opens a call in this family's emissions, and what ends one.
pub const CALL_OPEN: &str = "<|python_tag|>";
pub const CALL_CLOSE: &str = "<|eom_id|>";

/// **The markers this family's renderer emits into a prompt**, which is what
/// the inbound marker-promotion test of Spec section 10 tokenizes.
pub const RENDERED_MARKERS: &[&str] = &[TEXT_BEGIN, HEADER_OPEN, HEADER_CLOSE, TURN_END];

/// The markers this family's **model** emits and this module's parser reads.
///
/// **These arrived with Llama 3.1 and the tokenizer this workshop can reach is
/// 3.0-era, where both degrade to sub-word text**, measured at six and seven
/// tokens. That costs nothing today because they are matched as text against an
/// emission and never tokenized by this crate. It would cost something the day a
/// prior turn's call is rendered back into a prompt, which is inbound and would
/// bring them under [`RENDERED_MARKERS`]. Named here rather than left for that
/// day to discover.
pub const PARSED_MARKERS: &[&str] = &[CALL_OPEN, CALL_CLOSE];

/// This family's template, the one authority for it. The registry cites this
/// constant and [`Family::render_delta`] renders through it.
pub const TEMPLATE: &str = "<|start_header_id|>{role}<|end_header_id|>\n\n{message}<|eot_id|>";

/// The assistant turn's opening, appended after the delta's rendering so the
/// generation completes the assistant's turn rather than electing a speaker,
/// per the note on [`crate::family::qwen2::GENERATION_OPENER`]. Restates the
/// header markers because a const cannot concatenate them.
pub const GENERATION_OPENER: &str = "<|start_header_id|>assistant<|end_header_id|>\n\n";

const STOP: &[&str] = &[TURN_END, CALL_CLOSE];

/// This family's renderer, cited by its registry entry.
pub fn renderer() -> &'static dyn Family {
    &Llama
}

pub struct Llama;

impl Llama {
    fn markers() -> Markers {
        Markers {
            call_open: CALL_OPEN,
            call_close: Some(CALL_CLOSE),
        }
    }

    /// Recover a call's name from a llama fragment.
    ///
    /// The family emits a JSON object carrying `name`. Anything else is an
    /// attempted call whose name did not come back.
    fn recover(fragment: &str) -> Option<(ToolName, String)> {
        let value: serde_json::Value = serde_json::from_str(fragment.trim()).ok()?;
        let name = value.get("name")?.as_str()?;
        if name.is_empty() {
            return None;
        }
        let arguments = value
            .get("parameters")
            .or_else(|| value.get("arguments"))
            .map(|a| a.to_string())
            .unwrap_or_else(|| "{}".to_string());
        Some((ToolName(name.to_string()), arguments))
    }
}

impl Family for Llama {
    /// **The identity prefix opens with [`TEXT_BEGIN`] and the turns never
    /// repeat it**, which is what a once-per-prefix preamble is. It was written
    /// here from the first act and reached no prompt until this surface went
    /// live, the composition root having rendered every turn off the template
    /// constant alone.
    fn render_identity(&self, messages: &[Message]) -> Result<String, RenderRefusal> {
        let mut rendered = String::from(TEXT_BEGIN);
        rendered.push_str(&super::render_each(self, messages)?);
        Ok(rendered)
    }

    /// The wire role is the canonical one here, so the shared name kernel
    /// serves. Llama 3's header carries `user`, `assistant` and `tool` under
    /// those names.
    fn render_delta(&self, message: &Message) -> Result<String, RenderRefusal> {
        Ok(super::render_template(
            TEMPLATE,
            super::common_role_name(&message.role)?,
            &super::text_content(message)?,
        ))
    }

    fn parse(&self, emission: &str) -> Parsed {
        super::scan(emission, Self::markers(), Self::recover)
    }

    fn stop_conditions(&self) -> &'static [&'static str] {
        STOP
    }

    fn declaration(&self) -> &'static Declaration {
        lookup(&FamilyName("llama".into())).expect("the registry carries llama")
    }
}
