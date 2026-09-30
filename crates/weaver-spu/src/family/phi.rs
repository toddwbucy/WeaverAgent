//! The phi family, per `weaver-spu-Spec` section 5, and it is one module
//! holding two formats.
//!
//! **Phi is the vendor lineage the charter's "one module per family" names,
//! and its current line renders two incompatible formats under one
//! architecture string.** Three first-party Microsoft artifacts declare
//! `general.architecture = phi3`: Phi-3.5-mini and Phi-4-mini render role
//! tags, `<|user|>text<|end|>`, and Phi-4 14B renders ChatML with a separator
//! token, `<|im_start|>user<|im_sep|>text<|im_end|>`. The marker sets are
//! disjoint to the point that the mini's vocabulary does not contain
//! `<|im_start|>` at all, so the registry carries `phi3` twice and the
//! artifact's own template selects between the entries, which is exactly the
//! contested-architecture mechanism of Spec section 5 and this family is its
//! second user after `llama`.
//!
//! **Both formats live here rather than one of them citing another module**,
//! because everything phi defines should be defined in phi's module and
//! nowhere else, per charter section 14, and because the ChatML-with-separator
//! format is not qwen2's ChatML: the separator token replaces the newline
//! after the role and there is no newline after the close, so citing
//! `qwen2::TEMPLATE` would render a shape no Phi model was trained against.
//!
//! Measured against `microsoft_Phi-4-mini-instruct-Q4_K_M.gguf` (tag format)
//! and `phi-4-Q4_K_S.gguf` (separator format), both declaring `phi3`, on
//! 2026-08-17.

use super::{Content, Declaration, Family, Message, Parsed, RenderRefusal};
use weaver_traits::Role;

/// The tag format's markers. Written here and nowhere else in the crate.
///
/// The role's own tag is built by the template substituting into `<|{role}|>`,
/// so the markers a prompt carries are the role tags and the turn's end.
pub const TAG_USER: &str = "<|user|>";
pub const TAG_ASSISTANT: &str = "<|assistant|>";
pub const TAG_END: &str = "<|end|>";

/// The separator format's markers.
pub const SEP_OPEN: &str = "<|im_start|>";
pub const SEP: &str = "<|im_sep|>";
pub const SEP_CLOSE: &str = "<|im_end|>";

/// **The markers each renderer emits into a prompt**, which is what the
/// inbound marker-promotion test of Spec section 10 tokenizes, and what the
/// registry's contested selection matches against the detector's rendering.
///
/// The tag set does not list `<|system|>`: the floor's `Role` carries no
/// system case, so no turn this crate renders emits it. It is in the
/// artifact's vocabulary and in the detector's rendering of the template, and
/// it joins this set in the act that gives the floor a system role.
pub const TAG_RENDERED_MARKERS: &[&str] = &[TAG_USER, TAG_ASSISTANT, TAG_END];
pub const SEP_RENDERED_MARKERS: &[&str] = &[SEP_OPEN, SEP, SEP_CLOSE];

/// The tag format's template. `{role}` substitutes inside the tag itself,
/// which is the format's whole signature: the role is the marker rather than
/// text between markers.
pub const TAG_TEMPLATE: &str = "<|{role}|>{message}<|end|>";

/// The separator format's template. **Not qwen2's ChatML**: `<|im_sep|>`
/// stands where qwen2 puts a newline, and no newline follows the close.
pub const SEP_TEMPLATE: &str = "<|im_start|>{role}<|im_sep|>{message}<|im_end|>";

/// Each format's generation opener: the assistant's turn opened and left
/// unfinished, per the artifact templates' own `add_generation_prompt`
/// branches.
pub const TAG_GENERATION_OPENER: &str = "<|assistant|>";
pub const SEP_GENERATION_OPENER: &str = "<|im_start|>assistant<|im_sep|>";

/// Each format's stop set: the turn's close first, per the promotion rule
/// that reads the first declared condition as the terminator. The artifacts'
/// declared end-of-sequence, `<|endoftext|>`, joins at promotion as the
/// backstop rather than being declared here.
const TAG_STOP: &[&str] = &[TAG_END];
const SEP_STOP: &[&str] = &[SEP_CLOSE];

/// The tag renderer, cited by the registry entry serving Phi-3.5-mini and
/// Phi-4-mini artifacts.
pub fn tag_renderer() -> &'static dyn Family {
    &PhiTag
}

/// The separator renderer, cited by the entry serving Phi-4 artifacts.
pub fn sep_renderer() -> &'static dyn Family {
    &PhiSep
}

/// **This family's role map, shared by both formats and refusing the tool
/// role.** Neither template carries a tool turn: the tag format's `<|tool|>`
/// pair wraps tool *definitions* inside a system turn, which is the tool
/// workflow's business, and the separator template branches on three roles
/// and drops everything else on the floor. Rendering a `<|tool|>` turn or an
/// `<|im_start|>tool` turn would be a shape no Phi model was trained against,
/// the silent substitution the registry refuses one level up.
fn role_name(role: &Role) -> Result<&'static str, RenderRefusal> {
    Ok(match role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
        _ => return Err(RenderRefusal::MalformedForFamily),
    })
}

/// **Every phi emission parses as text, whole.** No Phi template in this
/// module's evidence gives the model a call format to emit for text turns:
/// the tag format's `<|tool|>` pair wraps tool *definitions* inside a system
/// turn, a prompt-side fact, and the separator template carries no call
/// construct at all. A parse that fed [`super::scan`] an invented call marker
/// would convert ordinary prose containing that string into reported call
/// attempts, so the honest parse holds no marker and reports nothing
/// unrecovered. The day Microsoft documents an emission format for calls,
/// it lands here with its markers.
fn parse_all_text(emission: &str) -> Parsed {
    Parsed {
        verbatim: emission.to_string(),
        content: if emission.is_empty() {
            Vec::new()
        } else {
            vec![Content::Text(emission.to_string())]
        },
        unrecovered: Vec::new(),
    }
}

/// The row in the registry whose selecting markers are this format's.
///
/// **By markers rather than by name, because the name alone no longer finds
/// one row.** `phi3` is contested, so `lookup` refuses it by design and the
/// decode path reads the declaration `select` resolved. What a format can
/// truthfully answer for is the row that selects on its own marker set.
fn row_selecting(markers: &'static [&'static str]) -> &'static Declaration {
    super::REGISTRY
        .iter()
        .find(|declaration| {
            declaration.family == "phi3" && declaration.selecting_markers == markers
        })
        .expect("the registry carries the phi3 row this format renders")
}

pub struct PhiTag;

impl Family for PhiTag {
    /// No preamble: the artifact declares `add_bos_token: false` and its
    /// template emits nothing before the first turn, so the prefix is the
    /// turns and nothing else.
    fn render_identity(&self, messages: &[Message]) -> Result<String, RenderRefusal> {
        super::render_each(self, messages)
    }

    fn render_delta(&self, message: &Message) -> Result<String, RenderRefusal> {
        Ok(super::render_template(
            TAG_TEMPLATE,
            role_name(&message.role)?,
            &super::text_content(message)?,
        ))
    }

    fn parse(&self, emission: &str) -> Parsed {
        parse_all_text(emission)
    }

    fn stop_conditions(&self) -> &'static [&'static str] {
        TAG_STOP
    }

    fn declaration(&self) -> &'static Declaration {
        row_selecting(TAG_RENDERED_MARKERS)
    }
}

pub struct PhiSep;

impl Family for PhiSep {
    /// No preamble, for the tag format's reason: nothing before the first
    /// turn in the artifact's own template.
    fn render_identity(&self, messages: &[Message]) -> Result<String, RenderRefusal> {
        super::render_each(self, messages)
    }

    fn render_delta(&self, message: &Message) -> Result<String, RenderRefusal> {
        Ok(super::render_template(
            SEP_TEMPLATE,
            role_name(&message.role)?,
            &super::text_content(message)?,
        ))
    }

    fn parse(&self, emission: &str) -> Parsed {
        parse_all_text(emission)
    }

    fn stop_conditions(&self) -> &'static [&'static str] {
        SEP_STOP
    }

    fn declaration(&self) -> &'static Declaration {
        row_selecting(SEP_RENDERED_MARKERS)
    }
}
