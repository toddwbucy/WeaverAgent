//! conforms: spu-system-folds-where-the-template-has-no-system-turn
//!
//! The mistral3 family, per `weaver-spu-Spec` section 5.
//!
//! Everything mistral3 defines sits here: its instruction markers, the
//! rendering that uses them, the preamble its prefix opens with, the parse of
//! its own emissions, and its stop conditions. The scanning beneath is
//! [`super::scan`], shared and marker-free.
//!
//! **This is the first family that names no role at all.** Every other carried
//! family writes the speaker into the turn, as `<|im_start|>user` or
//! `<|turn>model`. Mistral wraps the user's text in `[INST]` and `[/INST]` and
//! leaves the model's text bare, so the role is carried by the shape rather
//! than by a word. There is nothing for a `{role}` placeholder to substitute,
//! which is why this module renders each turn directly instead of driving
//! [`super::render_template`].
//!
//! **Its generation opener is empty, and that is the whole point of the
//! family.** `[/INST]` is emitted by the *user's* turn and is already the
//! assistant's turn opened and left unfinished, so there is nothing to append
//! after a delta. Every other family's opener exists because its user turn
//! closes without electing the next speaker; this one's does elect it.
//!
//! **The prefix opens with `<s>`, for a reason worth reading twice.** The
//! artifact declares `add_bos_token: true`, so its own tooling expects the
//! tokenizer to add the BOS and the template emits `bos_token` besides.
//! [`crate::residency::Resident::tokenize`] calls `str_to_token` with
//! `AddBos::Never`, so nothing downstream adds one here and the renderer must.
//! **[`gemma4`](super::gemma4) needs the same preamble and arrives at it from
//! the opposite declaration**, `add_bos_token: false` with the template
//! emitting it. Two families, two conventions, one joint.
//!
//! Measured against `Devstral-Small-2-24B-Instruct-2512-Q4_K_M.gguf`, whose
//! header declares `general.architecture = mistral3`, on 2026-08-17.
//! **Most Mistral artifacts do not declare this architecture.** Mistral-Small
//! 3.1, Mistral-Small 3.2 and Magistral-Small all report `llama`, which is the
//! census fact behind #88: the header names an architecture and an
//! architecture does not determine a template.

use weaver_traits::Role;
use weaver_types::ToolName;

use super::{Declaration, Family, FamilyName, Markers, Message, Parsed, RenderRefusal, lookup};

/// The instruction markers. Written here and nowhere else in the crate.
pub const TURN_OPEN: &str = "[INST]";
pub const TURN_CLOSE: &str = "[/INST]";

/// **The preamble the identity prefix opens with, once**, and the turn's end.
///
/// `</s>` is both this artifact's `eos_token_id` and the marker that closes a
/// model turn, and here the two coincide **by construction rather than by
/// luck**: the template emits `eos_token` to close the assistant, so the
/// family's turn close and the artifact's end-of-sequence are the same string
/// because the template says so. The Qwen artifacts' coincidence was the other
/// kind, and it is what hid the stop set's defect.
pub const BOS: &str = "<s>";
pub const TURN_END: &str = "</s>";

/// What opens a call in this family's emissions, and what separates the name
/// from its arguments.
pub const CALL_OPEN: &str = "[TOOL_CALLS]";
pub const ARGS: &str = "[ARGS]";

/// **The markers this family's renderer emits into a prompt**, which is what
/// the inbound marker-promotion test of Spec section 10 tokenizes.
pub const RENDERED_MARKERS: &[&str] = &[BOS, TURN_OPEN, TURN_CLOSE, TURN_END];

/// **The markers a detector rendering emits, which select this format's rows
/// among entries sharing an architecture.** Not [`RENDERED_MARKERS`], and the
/// missing `<s>` is the difference: the BOS is the tokenizer's to add rather
/// than the template's to emit, so no rendering ever contains it and a
/// selecting set requiring it refuses every artifact of the format it names.
/// Measured 2026-08-17 against Mistral-Small-3.2's own template through the
/// detector: `[INST] u[/INST] a</s>`, with `[SYSTEM_PROMPT]` opening and no
/// `<s>` anywhere in the output.
pub const SELECTING_MARKERS: &[&str] = &[TURN_OPEN, TURN_CLOSE, TURN_END];

/// The markers this family's **model** emits and this module's parser reads.
///
/// Outbound, so promotion does not bind them the way it binds
/// [`RENDERED_MARKERS`]. Both promote under this artifact's tokenizer, at ids 9
/// and 32, which is a fact rather than a requirement this act asserts.
pub const PARSED_MARKERS: &[&str] = &[CALL_OPEN, ARGS];

/// **The user turn's shape, and what the record names as this family's
/// template.**
///
/// **This family's rendering is not one substitution and this constant is not
/// what renders it.** The assistant's turn is bare text closed by
/// [`TURN_END`], so no single string describes both halves and
/// [`Family::render_delta`] builds each role's turn directly. The constant is
/// carried because the registry declares one and the measurement records it as
/// the template identity, which is the thing #129 and #88 are about: a
/// template identity that is really an architecture key.
pub const TEMPLATE: &str = "[INST]{message}[/INST]";

/// **Empty, and correctly so.** See the module note: `[/INST]` closes the
/// user's turn and opens the model's in one marker, so a delta ending there is
/// already the assistant's turn opened and left unfinished, which is what the
/// opener exists to guarantee. Appending anything would put a second speaker
/// election after the one the user's turn already made.
pub const GENERATION_OPENER: &str = "";

/// The turn's close is the stop condition and it is the artifact's own
/// `eos_token_id`, token 2.
const STOP: &[&str] = &[TURN_END];

/// This family's renderer, cited by its registry entry.
pub fn renderer() -> &'static dyn Family {
    &Mistral3
}

pub struct Mistral3;

impl Mistral3 {
    fn markers() -> Markers {
        Markers {
            call_open: CALL_OPEN,
            call_close: None,
        }
    }

    /// Recover a call's name from a mistral3 fragment.
    ///
    /// The family emits `[TOOL_CALLS]<name>[ARGS]<json>`, a name before a
    /// separator rather than the JSON object qwen2 emits or the routing header
    /// gpt-oss emits. A fragment carrying no `[ARGS]` is an attempted call
    /// whose name did not come back, and the caller turns that into a reported
    /// fact rather than into text.
    ///
    /// The call runs to the end of the emission because the family closes it
    /// with the turn rather than with a marker of its own, which is what
    /// [`Markers::call_close`] being `None` says.
    fn recover(fragment: &str) -> Option<(ToolName, String)> {
        let at = fragment.find(ARGS)?;
        let name = fragment[..at].trim();
        if name.is_empty() {
            return None;
        }
        let arguments = fragment[at + ARGS.len()..].trim();
        Some((
            ToolName(name.to_string()),
            if arguments.is_empty() {
                "{}".to_string()
            } else {
                arguments.to_string()
            },
        ))
    }
}

impl Family for Mistral3 {
    /// The prefix opens with [`BOS`], once, and the turns never repeat it.
    fn render_identity(&self, messages: &[Message]) -> Result<String, RenderRefusal> {
        // **A seated `System` prefix folds into the first user turn**, per
        // `weaver-spu-Spec` section 5. This template names no system turn,
        // which is a fact about the template and not about the floor's
        // vocabulary: the canonical role of an identity prefix is `System`
        // per `weaver-types-Spec` section 2, and refusing it here would
        // leave this family with no usable prefix at all once the parse
        // began requiring the role. Folding is what this family's published
        // template does with system content, so it follows the template
        // authority rather than minting a turn the template never had.
        //
        // conforms: spu-system-folds-where-the-template-has-no-system-turn
        let mut rendered = String::from(BOS);
        rendered.push_str(&super::render_each(self, messages)?);
        Ok(rendered)
    }

    /// This template names no system turn, so a `System` message folds into
    /// the user turn that follows, per `weaver-spu-Spec` section 5.
    ///
    /// conforms: spu-system-folds-where-the-template-has-no-system-turn
    fn fold_for_template(&self, messages: &[Message]) -> Result<Vec<Message>, RenderRefusal> {
        super::fold_system_into_first_user(messages)
    }

    /// **The role decides the wrapper rather than filling a placeholder.**
    ///
    /// **`Role::System` renders as this template's user turn**, per the
    /// operator's ruling of 2026-08-28. This family's template names no
    /// system turn in the material the module was built from, and that is a
    /// fact about the template rather than about the floor's vocabulary: the
    /// canonical role of a seated identity is `System`, so the role is
    /// carried here rather than refused.
    ///
    /// That is not the invention an earlier form of this clause was written
    /// against. Folding system content into the user turn is what this
    /// family's published template does, so it is the template authority
    /// followed, where a `[SYSTEM]` wrapper of our own devising would be the
    /// shape invented.
    ///
    /// **This arm is what a caller reaching `render_delta` directly gets.**
    /// The production path is `render_each`, which applies
    /// `fold_for_template` first, so a `System` message with a user turn
    /// after it is merged before it arrives and this arm sees only what
    /// stands alone.
    ///
    /// It was added because the control loop's opening and its re-entry are
    /// `System` and travel as deltas. With the fold wired into
    /// `render_identity` alone they reached this arm unmerged and an arm
    /// refusing them failed every turn of a dev-loop run while the
    /// declaration's prefix rendered perfectly. The fold has since moved onto
    /// the trait, so that is no longer the path they take - **and this arm
    /// still may not refuse**, a lone `System` and any caller rendering one
    /// message at a time both landing here.
    ///
    /// `Role::ToolResult` refuses rather than rendering. Mistral carries tool
    /// results in their own `[TOOL_RESULTS]` block rather than as a turn, and
    /// inventing a shape for it would be the silent substitution the registry
    /// refuses one level up. It renders when the tool workflow lands and names
    /// the block, and not before.
    fn render_delta(&self, message: &Message) -> Result<String, RenderRefusal> {
        let content = super::text_content(message)?;
        Ok(match message.role {
            Role::System | Role::User => format!("{TURN_OPEN}{content}{TURN_CLOSE}"),
            Role::Assistant => format!("{content}{TURN_END}"),
            _ => return Err(RenderRefusal::MalformedForFamily),
        })
    }

    fn parse(&self, emission: &str) -> Parsed {
        super::scan(emission, Self::markers(), Self::recover)
    }

    fn stop_conditions(&self) -> &'static [&'static str] {
        STOP
    }

    fn declaration(&self) -> &'static Declaration {
        lookup(&FamilyName("mistral3".into())).expect("the registry carries mistral3")
    }
}
