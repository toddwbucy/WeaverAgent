//! The gpt-oss family, per `weaver-spu-Spec` section 5.
//!
//! Everything gpt-oss defines sits here: its Harmony markers, the rendering
//! that uses them, the parse of its own emissions, and its stop conditions.
//! The scanning beneath is [`super::scan`], shared and marker-free.
//!
//! **This family closes a call with a marker it also uses to end a turn**, so
//! its recovery reads a routing header rather than a JSON object. That is the
//! reason the surface takes a family's own recovery function rather than one
//! shared parse: the difference between the families is where a name sits, and
//! a kernel that guessed would be the silent substitution the registry refuses.

use weaver_types::ToolName;

use super::{Declaration, Family, FamilyName, Markers, Message, Parsed, RenderRefusal, lookup};

/// The Harmony markers. Written here and nowhere else in the crate.
pub const TURN_OPEN: &str = "<|start|>";
pub const MESSAGE: &str = "<|message|>";
pub const TURN_END: &str = "<|end|>";
pub const CHANNEL: &str = "<|channel|>";
pub const RETURN: &str = "<|return|>";

/// What opens a call in this family's emissions.
pub const CALL_OPEN: &str = "<|call|>";

/// **The markers this family's renderer emits into a prompt**, which is what
/// the inbound marker-promotion test of Spec section 10 tokenizes.
pub const RENDERED_MARKERS: &[&str] = &[TURN_OPEN, MESSAGE, TURN_END];

/// The markers this family's **model** emits and this module's parser reads.
///
/// Harmony carries its whole vocabulary as special tokens, so these promote
/// too, which is a fact rather than a requirement this act asserts.
pub const PARSED_MARKERS: &[&str] = &[CHANNEL, RETURN, CALL_OPEN];

/// This family's template, the one authority for it. The registry cites this
/// constant and [`Family::render_delta`] renders through it.
pub const TEMPLATE: &str = "<|start|>{role}<|message|>{message}<|end|>";

/// The assistant turn's opening, appended after the delta's rendering so the
/// generation completes the assistant's turn rather than electing a speaker,
/// per the note on [`crate::family::qwen2::GENERATION_OPENER`]. Harmony's
/// generation prompt ends at the role: the model emits its own channel and
/// message markers. Restates [`TURN_OPEN`] because a const cannot concatenate
/// one.
pub const GENERATION_OPENER: &str = "<|start|>assistant";

const STOP: &[&str] = &[RETURN, TURN_END];

/// This family's renderer, cited by its registry entry.
pub fn renderer() -> &'static dyn Family {
    &GptOss
}

/// How Harmony qualifies a callable name.
const RECIPIENT: &str = "to=functions.";

pub struct GptOss;

impl GptOss {
    fn markers() -> Markers {
        Markers {
            call_open: CALL_OPEN,
            call_close: Some(TURN_END),
        }
    }

    /// Recover a call's name from a Harmony fragment.
    ///
    /// The name arrives in the routing header as `to=functions.<name>`, with
    /// the arguments following the message marker. A fragment carrying no
    /// recipient is an attempted call whose name did not come back.
    fn recover(fragment: &str) -> Option<(ToolName, String)> {
        let at = fragment.find(RECIPIENT)? + RECIPIENT.len();
        let tail = &fragment[at..];
        let name: String = tail
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
            .collect();
        if name.is_empty() {
            return None;
        }
        let arguments = match tail.find(MESSAGE) {
            Some(at) => tail[at + MESSAGE.len()..].trim().to_string(),
            None => "{}".to_string(),
        };
        Some((ToolName(name), arguments))
    }
}

impl Family for GptOss {
    fn render_identity(&self, messages: &[Message]) -> Result<String, RenderRefusal> {
        super::render_each(self, messages)
    }

    /// The wire role is the canonical one here, so the shared name kernel
    /// serves. Harmony's routing header renames a *recipient*, which is the
    /// parse's business and not the render's.
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
        lookup(&FamilyName("gpt-oss".into())).expect("the registry carries gpt-oss")
    }
}
