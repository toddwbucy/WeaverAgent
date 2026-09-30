//! conforms: spu-shard-widths-are-a-set
//! conforms: spu-widths-set-pinned-by-doctest
//! conforms: spu-registry-no-silent-substitution
//! conforms: spu-share-kernels-own-orchestration
//! conforms: spu-parse-reports-unrecovered-call
//! conforms: spu-system-folds-where-the-template-has-no-system-turn
//! conforms: spu-architecture-and-markers-are-unique
//!
//! The family surface and its registry, per `weaver-spu-Spec` section 5.
//!
//! **One module per family, and the kernels shared beneath.** Everything a
//! family defines lives in that family's module: its marker vocabulary, its
//! template rendering, the parse of its own output, and its stop conditions.
//! What is common sits here as machinery the families drive rather than as
//! behaviour they inherit, which is the archived tree's share-kernels-own-
//! orchestration rule promoted to structure. [`scan`] is that kernel: it walks
//! an emission against whatever [`Markers`] a family hands it and owns no
//! marker of its own.
//!
//! The placement is the whole of the claim, and Spec section 5 buys it by
//! non-purchase: a module boundary reads the same to a test as to a reader, so
//! no test is written for it here. What a reader checks is that no marker
//! string appears in this file and that each family's constants sit in its own.
//!
//! **The registry is compile-time and admission consults it.** A family the
//! binary does not carry is a refused admit **naming the family**, which is the
//! archived tree's own no-silent-substitution ruling carried forward from its
//! encoder registry. Nothing here falls back to a nearest match: a substitution
//! that succeeds quietly is how a model runs under the wrong template.
//!
//! **The shard widths are a set rather than a maximum.** The field's type is a
//! set, so a maximum can no longer be declared, only read wrongly. The doctest
//! below reads a declaration carrying a non-contiguous set literal, which is
//! what makes the type unable to express the maximum it replaced:
//!
//! ```
//! use weaver_spu::family::Declaration;
//! // A non-contiguous set: this backend shards across one device or four, and
//! // not across two or three. No maximum describes that, which is the point.
//! const SPARSE: Declaration = Declaration {
//!     family: "sparse-example",
//!     shard_widths: &[1, 4],
//!     template: "{message}",
//!     generation_opener: "",
//!     renderer: weaver_spu::family::qwen2::renderer,
//!     // Empty: this example's architecture is its own, so nothing competes
//!     // with it and no marker set is read to choose.
//!     selecting_markers: &[],
//!     flush: weaver_spu::decoder::backend::FlushMechanism::TruncateToPosition,
//!     taps_readout: true,
//!     taps_column: false,
//! };
//! assert!(SPARSE.shards_across(4));
//! assert!(!SPARSE.shards_across(2));
//! assert!(!SPARSE.shards_across(3));
//! ```

use weaver_traits::{ContentBlock, Message, Role};
use weaver_types::{LifecycleRefusal, TokenRefusal, ToolName};

use crate::decoder::backend::FlushMechanism;

pub mod gemma4;
pub mod gpt_oss;
pub mod llama;
pub mod mistral3;
pub mod modernbert;
pub mod phi;
pub mod qwen2;

/// Why a family could not render a message.
///
/// **The crate's own vocabulary rather than the floor's**, for the reason
/// [`FamilyRefusal`] is: what a render refuses on is a family fact, and the
/// wire's word for it is the composition root's to choose. The [`From`] below
/// is where the two meet.
///
/// One case, and both of the floor's growth points feed it. `Role` and
/// `ContentBlock` are each `non_exhaustive`, so a family's rendering is a match
/// with a wildcard arm, and what the wildcard means is that this family has no
/// rendering for what arrived. **That is the contract's own case rather than
/// one invented here:** the tool shapes are blocked with the tool workflow and
/// the families render text today.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderRefusal {
    /// The message carries a role or a block this family has no rendering for.
    MalformedForFamily,
}

impl From<RenderRefusal> for TokenRefusal {
    /// What crosses the seam. A render that refused is the delta malformed for
    /// the family, which is the only shape the decode contract carries for it.
    fn from(_: RenderRefusal) -> Self {
        TokenRefusal::MalformedDelta
    }
}

/// **The role names most families render, as a shared kernel they drive.**
///
/// A family calls this because its wire vocabulary happens to be the canonical
/// one, not because it inherits it. A family whose wire name differs writes its
/// own match and does not call this, which is the whole of how [`gemma4`]
/// renders its assistant as `model` without any other family learning the word.
///
/// The wildcard arm is the floor's growth point, `Role` being `non_exhaustive`.
/// No test can construct its subject today, every current case being rendered.
pub fn common_role_name(role: &Role) -> Result<&'static str, RenderRefusal> {
    Ok(match role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::ToolResult => "tool",
        _ => return Err(RenderRefusal::MalformedForFamily),
    })
}

/// A message's text, joined, for the families that render text and nothing
/// else.
///
/// A block that is not text is unrenderable rather than skipped: a tool call
/// silently dropped from a prompt is a turn the model answers without knowing
/// what it was asked, which is the silent substitution this crate refuses
/// everywhere else.
pub fn text_content(message: &Message) -> Result<String, RenderRefusal> {
    let mut content = String::new();
    for block in &message.content {
        match block {
            ContentBlock::Text { text } => content.push_str(text),
            _ => return Err(RenderRefusal::MalformedForFamily),
        }
    }
    Ok(content)
}

/// **The shared rendering kernel: every message through the family's own delta
/// rendering, in order, and nothing else.**
///
/// It holds no marker, no role name, and no preamble, which is what keeps a
/// family's whole vocabulary inside that family's module. A family whose
/// identity prefix opens with something the turns do not repeat adds it around
/// this call rather than inside it - see [`gemma4::Gemma4::render_identity`],
/// where the once-per-prefix `<bos>` lives.
pub fn render_each(family: &dyn Family, messages: &[Message]) -> Result<String, RenderRefusal> {
    // **The family's own preparation runs first, here and not at each call
    // site.** Every prefix rendering and the decode seam's delta rendering
    // both funnel through this function, so a family that folds is folded for
    // on both paths without either remembering to ask.
    let prepared = family.fold_for_template(messages)?;
    let mut rendered = String::new();
    for message in &prepared {
        rendered.push_str(&family.render_delta(message)?);
    }
    Ok(rendered)
}

/// Renders a `System` message as user content, for the families whose
/// template names no system turn, merging it into the `User` turn that
/// follows where there is one.
///
/// **The canonical role is the floor's and the shape is the family's.**
/// `weaver-traits`' `Role` is the vocabulary the whole program speaks, and a
/// seated identity prefix is `System` in it, per `weaver-types-Spec` section
/// 2. A family whose template has no system turn is saying something about
/// its template, not about the vocabulary, so it renders that role rather
/// than refusing it.
///
/// **This is the template authority followed rather than a shape invented**,
/// which is the distinction `mistral3::Mistral3::render_delta` draws. Gemma
/// and Mistral both publish templates that carry system content into the
/// first user turn, so folding is what those authorities name; inventing
/// would be minting the `system` role into a template whose own marker set
/// has no such turn - `<|turn|>system` for gemma, a `[SYSTEM]` block for
/// mistral, neither of which either template names.
///
/// **Every `System` message folds, not only a leading one.** An earlier form
/// of this doc said otherwise while the code folded all of them, and the
/// code is the one that decides: a family that renders system content as
/// user content has no position at which the role becomes unrenderable, and
/// a mid-prefix system message refused while a leading one folded would be a
/// distinction with nothing behind it. `render_delta` renders a lone
/// `System` the same way for the same reason, which is what carries the
/// loop's own voice as well as the declaration's prefix.
///
/// A `System` message with no `User` after it becomes a user turn of its
/// own, there being nothing to fold into and a prefix that renders as
/// nothing being a prefix the record cannot account for. A prefix carrying
/// no `System` role is returned untouched.
///
/// **A block the family cannot render refuses rather than being dropped.**
/// The content is read through [`text_content`], so a `ToolCall` inside a
/// seated identity is `MalformedForFamily` as it was before this fold
/// existed. Copying only the text blocks would have opened the session on a
/// silently truncated prefix, which is an absence that reads as a
/// configuration.
///
/// conforms: spu-system-folds-where-the-template-has-no-system-turn
pub fn fold_system_into_first_user(messages: &[Message]) -> Result<Vec<Message>, RenderRefusal> {
    let mut out: Vec<Message> = Vec::with_capacity(messages.len());
    let mut carried: Option<String> = None;
    for message in messages {
        match (&message.role, carried.take()) {
            (Role::System, carried_before) => {
                // Several system messages in a row accumulate rather than
                // the last winning, an operator who wrote two having meant
                // both.
                let mut text = carried_before.unwrap_or_default();
                let piece = text_content(message)?;
                // Both sides are tested, not the buffer alone: an
                // empty-content `System` between two others would otherwise
                // contribute a separator and nothing to separate, giving
                // `one\n\n\n\nask`.
                if !text.is_empty() && !piece.is_empty() {
                    text.push_str("\n\n");
                }
                text.push_str(&piece);
                carried = Some(text);
            }
            (Role::User, Some(prefix)) => {
                let mut text = prefix;
                let piece = text_content(message)?;
                // Both sides, as the accumulate arm above.
                if !text.is_empty() && !piece.is_empty() {
                    text.push_str("\n\n");
                }
                text.push_str(&piece);
                out.push(Message {
                    role: Role::User,
                    content: vec![ContentBlock::Text { text }],
                });
            }
            (_, prefix) => {
                // The carried prefix has nothing of its own role to join, so
                // it stands as a user turn ahead of whatever this is.
                if let Some(text) = prefix {
                    out.push(Message {
                        role: Role::User,
                        content: vec![ContentBlock::Text { text }],
                    });
                }
                out.push(message.clone());
            }
        }
    }
    if let Some(text) = carried {
        out.push(Message {
            role: Role::User,
            content: vec![ContentBlock::Text { text }],
        });
    }
    Ok(out)
}

/// One piece of an emission, recovered.
#[derive(Debug, Clone, PartialEq)]
pub enum Content {
    /// Ordinary assistant text.
    Text(String),
    /// A call the parser recovered whole: the marker opened and the name came
    /// back.
    Call { name: ToolName, arguments: String },
}

/// A call the emission attempted and the parser could not recover.
///
/// **This is its own reported fact rather than a clean turn**, per Spec section
/// 5. The archived tree drew the same line between a call that could not be
/// rendered and a call whose name could not be recovered, and the second is
/// this. The fragment travels with it so the record holds what was attempted,
/// and it is deliberately not also emitted as [`Content::Text`]: a fragment
/// that arrives as ordinary prose is a failed call the turn reads as success.
#[derive(Debug, Clone, PartialEq)]
pub struct Unrecovered {
    /// What sat between the call marker and the end of the emission or its
    /// closing marker.
    pub fragment: String,
}

/// What a parse answers with.
///
/// The verbatim emission rides alongside the canonical form because the decode
/// contract has both reaching the record, per the operator's end-to-end
/// requirement.
#[derive(Debug, Clone, PartialEq)]
pub struct Parsed {
    pub verbatim: String,
    pub content: Vec<Content>,
    pub unrecovered: Vec<Unrecovered>,
}

impl Parsed {
    /// Whether the emission attempted a call the parse could not recover.
    pub fn has_unrecovered_call(&self) -> bool {
        !self.unrecovered.is_empty()
    }

    /// The assistant text alone, joined. Used by tests that must show a
    /// fragment did **not** arrive as prose.
    pub fn text(&self) -> String {
        self.content
            .iter()
            .filter_map(|piece| match piece {
                Content::Text(text) => Some(text.as_str()),
                Content::Call { .. } => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }
}

/// A family's control markers, handed to the shared kernel.
///
/// Every string here is the family's own and none is written in this module,
/// which is the placement rule stated as a type: the kernel cannot recognise a
/// marker no family gave it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Markers {
    /// What opens a call in this family's emissions.
    pub call_open: &'static str,
    /// What closes one, where the family closes them. `None` means the call
    /// runs to the end of the emission.
    pub call_close: Option<&'static str>,
}

/// The surface a family implements, small and named, per Spec section 5 and
/// charter section 14.
///
/// Six members and no seventh. Membership is the charter's enumeration and
/// takes no record of its own.
///
/// **The messages are the floor's, which is what section 5 says and what this
/// surface now reads.** It formerly took a message shape local to this module,
/// and nothing on the decode path called it: the render ran in the composition
/// root off the [`Declaration`]'s template string, so a family could hold no
/// rendering fact beyond that one constant. A family whose wire role differs
/// from the canonical one, or whose prefix opens with a string its turns do not
/// repeat, had nowhere to say so. Both arrived with [`gemma4`], and the repair
/// is this surface carrying the traffic Spec section 5 always said it carried.
pub trait Family {
    /// Render an identity prefix from canonical messages.
    ///
    /// The prefix's turns are all complete, so no generation opener belongs
    /// here. What may belong is a once-per-prefix preamble the per-turn
    /// rendering must not repeat.
    fn render_identity(&self, messages: &[Message]) -> Result<String, RenderRefusal>;

    /// Render a turn's delta.
    fn render_delta(&self, message: &Message) -> Result<String, RenderRefusal>;

    /// **What this family must do to a message sequence before rendering it**,
    /// defaulting to nothing.
    ///
    /// A family whose template names a system turn renders one and overrides
    /// nothing here. A family whose template names none folds `System` into
    /// the user turn that follows, per Spec section 5, and says so by
    /// overriding this.
    ///
    /// **It hangs off the trait rather than off each `render_identity`
    /// because the prefix is not the only path.** The control loop's opening
    /// and its re-entry are `System` and travel as a delta, so a fold wired
    /// into `render_identity` alone left them to render as two adjacent user
    /// turns - a shape both published templates avoid by merging, and one
    /// mistral's own template raises on. Placed here, [`render_each`] applies
    /// it and every caller is covered.
    ///
    /// **The merge is within one render call and does not span the prefix and
    /// the delta**, which are two: the prefix renders at `Open` and the delta
    /// at `AppendAndGenerate`. So a seated identity folds to one user turn and
    /// the first delta folds to a second immediately after it, adjacent with
    /// no assistant between. That adjacency stands and is not something this
    /// fold can close, the prefix being tokenized and resident before any
    /// delta exists - seating it later would retire the ruling of 2026-08-20
    /// that the prefix is processed at load as the functionality test. What
    /// the fold removes is the adjacency *within* each call, which on the old
    /// `role: user` declarations was three turns rather than two.
    fn fold_for_template(&self, messages: &[Message]) -> Result<Vec<Message>, RenderRefusal> {
        Ok(messages.to_vec())
    }

    /// Parse an emission into canonical content, this family's markers
    /// recognised.
    fn parse(&self, emission: &str) -> Parsed;

    /// The stop conditions this family declares.
    fn stop_conditions(&self) -> &'static [&'static str];

    /// The capabilities admission judges against.
    ///
    /// **A module serving several architecture keys answers for the key it is
    /// named after, not for the artifact in hand.** [`qwen2`] serves seven keys,
    /// so `Qwen2::declaration()` is qwen2's row whichever of them was admitted.
    /// A per-artifact fact is read from the entry admission resolved instead: by
    /// [`lookup`] against the architecture the header declares, or by [`select`]
    /// against the artifact's chat template where that architecture is contested,
    /// and that entry is retained for the session.
    fn declaration(&self) -> &'static Declaration;
}

/// The shared parse kernel.
///
/// **It owns no marker.** Every string it matches on arrives in `markers`, so a
/// family's vocabulary lives in the family's module and this function is the
/// orchestration the families drive rather than inherit.
///
/// `recover` is the family's own name extraction, because how a name sits
/// inside a call fragment is a family fact. Returning `None` is what produces
/// an [`Unrecovered`], and the fragment does not also become text.
pub fn scan(
    emission: &str,
    markers: Markers,
    recover: impl Fn(&str) -> Option<(ToolName, String)>,
) -> Parsed {
    let mut content = Vec::new();
    let mut unrecovered = Vec::new();
    let mut rest = emission;

    while let Some(open) = rest.find(markers.call_open) {
        let (before, after_open) = rest.split_at(open);
        if !before.is_empty() {
            content.push(Content::Text(before.to_string()));
        }
        let after_open = &after_open[markers.call_open.len()..];

        // Where the fragment ends: the family's closing marker, or the whole
        // remainder where the family closes nothing.
        let (fragment, remainder) = match markers.call_close {
            Some(close) => match after_open.find(close) {
                Some(at) => (&after_open[..at], &after_open[at + close.len()..]),
                // The marker opened and never closed. That is an attempted call
                // whose extent is the rest of the emission.
                None => (after_open, ""),
            },
            None => (after_open, ""),
        };

        match recover(fragment) {
            Some((name, arguments)) => content.push(Content::Call { name, arguments }),
            None => unrecovered.push(Unrecovered {
                fragment: fragment.to_string(),
            }),
        }
        rest = remainder;
    }

    if !rest.is_empty() {
        content.push(Content::Text(rest.to_string()));
    }

    Parsed {
        verbatim: emission.to_string(),
        content,
        unrecovered,
    }
}

/// A family's name, as the artifact header declares it.
///
/// Held in the header's own spelling, which is what a refusal names. The
/// registry matches it by [`same_key`], so the spelling is kept for the record
/// and folded for the comparison.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FamilyName(pub String);

/// What one family declares about itself, at compile time.
///
/// **No [`PartialEq`], and the reason is [`Declaration::renderer`].** A
/// function pointer's address is not guaranteed unique, so a derived comparison
/// would answer about addresses rather than about families and the compiler
/// says so. Nothing compares declarations - what callers read is a field - so
/// the derive is dropped rather than hand-written around the one member that
/// cannot answer.
#[derive(Debug, Clone, Copy)]
pub struct Declaration {
    /// The name an artifact header must carry to select this family.
    pub family: &'static str,
    /// **The widths this backend can shard across, as a set.** Membership is
    /// the test, never a comparison against a bound: a set is what lets a
    /// backend declare that it shards across one or four and not two.
    pub shard_widths: &'static [u32],
    /// The template this family renders the harness's canonical messages
    /// through.
    pub template: &'static str,
    /// **What a delta's rendering closes with: the assistant's turn opened
    /// and left unfinished**, so the generation completes that turn rather
    /// than electing a speaker. Appended to the delta's rendering only, never
    /// to the identity prefix, whose turns are all complete.
    pub generation_opener: &'static str,
    /// **The family's renderer, cited rather than written here**, for the
    /// reason [`Declaration::template`] is cited: the module is the authority
    /// and this table points at it.
    ///
    /// A function returning the object rather than the object itself, because
    /// a `&dyn` field would take this struct's derived [`PartialEq`] with it
    /// and the width judgment reads declarations by value. **One table, not
    /// two:** a second map from family name to renderer would be the same fact
    /// in two places with no authority named, which G5 files as a defect.
    pub renderer: fn() -> &'static dyn Family,
    /// **The markers this entry renders, which select it among the entries
    /// sharing its architecture**, per Spec section 5.
    ///
    /// Cited from the family module rather than written here, for the reason
    /// [`Declaration::template`] is cited: the module is the authority for
    /// what it emits, and a second spelling here would be one fact in two
    /// places with no authority named.
    ///
    /// **Read only where an architecture is contested.** An architecture
    /// carrying one entry selects on the architecture and never renders, which
    /// is what keeps a family the template detector does not recognise
    /// admissible. Gemma4 is such a family today.
    pub selecting_markers: &'static [&'static str],
    /// **How this family's flush reaches its fixed outcome**, per Spec section
    /// 4.4. Declared here rather than inferred from a version string, because
    /// a truncation that returns success while recurrent state stays is the
    /// silent failure the append-only discipline exists to prevent.
    pub flush: FlushMechanism,
    /// **Whether this family's engine can tap for residual readout**, per Spec
    /// section 5's capability list and section 7's admit judgment. Declared
    /// rather than probed, because an election judged at admit cannot wait for
    /// a forward to find out.
    ///
    /// **Every shipped family declares `false` today and that is the truthful
    /// value, not a placeholder.** Nothing implements [`crate::readout::Tap`],
    /// because neither backend exists, so a family advertising a tap it cannot
    /// perform would be admitted under an election nothing could honor, which
    /// is the expensive lie the admit judgment exists to prevent. Each flips in
    /// the act that stands its engine's tap up, and not before.
    pub taps_readout: bool,
    /// **Whether this family's tap holds a column to answer**, per
    /// `weaver-spu-Spec` section 7's diagnostic clause and charter section
    /// 13.7: the GGUF tap's one-column copy can continue instead of
    /// dropping, the native tap folds in place and holds none. Held to the
    /// tap flag's own bar - declared only where the answer has been shown
    /// on the engine that would serve it - so qwen2 declares it on the GGUF
    /// answer shown first and every other family declares `false` until its
    /// own act.
    pub taps_column: bool,
}

impl Declaration {
    /// **Whether this family's session state permits truncation to a
    /// position**, per Spec section 5's surface and section 4.4's mechanism.
    ///
    /// Derived from [`Declaration::flush`] rather than declared beside it,
    /// because 4.4 makes them one fact: truncation is permitted exactly where
    /// the flush is reached by truncating. It was a second declaration on the
    /// [`Family`] trait until 2026-08-17, which is one fact in two places with
    /// no authority named, and the two could disagree - every family said
    /// `true` there, including the two whose engines refuse to roll back.
    ///
    /// **It lives here and not on the trait because this is keyed per
    /// architecture.** A module serving several keys has one trait object and
    /// several rows, and the answer differs between them: the qwen2 module
    /// serves both a truncating qwen2 and a re-establishing qwen35.
    pub const fn permits_truncation(&self) -> bool {
        matches!(self.flush, FlushMechanism::TruncateToPosition)
    }

    /// Whether this backend shards across exactly this many devices.
    ///
    /// Membership rather than a bound. A wider set refuses against the
    /// declaration rather than against a hidden limit, so the day an N-way path
    /// lands the declaration changes and nothing else does.
    pub const fn shards_across(&self, width: u32) -> bool {
        let mut index = 0;
        while index < self.shard_widths.len() {
            if self.shard_widths[index] == width {
                return true;
            }
            index += 1;
        }
        false
    }
}

/// The compile-time table.
///
/// **Today the salvaged tensor-parallel path is a two-device implementation,**
/// `forward_tp2` with an all-reduce kernel written for a pair, so the declared
/// set is one or two. It is written as a set rather than as the maximum two so
/// that a three-device path arriving without a two-device path is expressible.
/// **The template strings are each family's own and are referenced here rather
/// than written here**, per the placement rule of Spec section 5. A template
/// spelled in this file would be family-specific material outside its module,
/// and once the family module renders through the same constant it would also
/// be one fact in two places with no authority named, which G5 files as a
/// defect rather than something to resolve by picking. The family module is the
/// authority and this table cites it.
pub const REGISTRY: &[Declaration] = &[
    Declaration {
        family: "llama",
        shard_widths: &[1, 2],
        template: llama::TEMPLATE,
        generation_opener: llama::GENERATION_OPENER,
        renderer: llama::renderer,
        selecting_markers: llama::RENDERED_MARKERS,
        flush: FlushMechanism::TruncateToPosition,
        taps_readout: false,
        taps_column: false,
    },
    Declaration {
        // **The llama architecture's second reading, and the entry that makes
        // the architecture contested.** SmolLM2 declares `general.architecture
        // = llama` and renders ChatML, so the entry above hands it a Llama 3
        // stop set it was never trained against and the load refuses. The
        // refusal is correct and the gap was the table, which had one row for
        // an architecture its artifacts read two ways.
        //
        // **Everything here is the ChatML module's**, cited for the reason
        // `nemotron_h_moe` cites it: the format is that module's to explain
        // and this row points at it. What distinguishes this row from that one
        // is the architecture it answers for, which is the key.
        family: "llama",
        shard_widths: &[1, 2],
        template: qwen2::TEMPLATE,
        generation_opener: qwen2::GENERATION_OPENER,
        renderer: qwen2::renderer,
        selecting_markers: qwen2::RENDERED_MARKERS,
        flush: FlushMechanism::TruncateToPosition,
        taps_readout: false,
        taps_column: false,
    },
    Declaration {
        // **The llama architecture's third reading.** Mistral Small 3.2
        // declares `general.architecture = llama` and renders `[INST]`, so
        // before this row the two-field key refused it - correctly, per the
        // 2026-08-17 sweep, where every model-needing test recorded the
        // refusal (#186). The format is the mistral3 module's and this row
        // cites it, the way the row above cites qwen2.
        //
        // The selecting set is [`mistral3::SELECTING_MARKERS`] rather than the
        // module's full rendered set, because a rendering never emits `<s>`:
        // the BOS is the tokenizer's. Measured against the artifact's own
        // template through the detector before this row was written.
        family: "llama",
        shard_widths: &[1, 2],
        template: mistral3::TEMPLATE,
        generation_opener: mistral3::GENERATION_OPENER,
        renderer: mistral3::renderer,
        selecting_markers: mistral3::SELECTING_MARKERS,
        flush: FlushMechanism::TruncateToPosition,
        taps_readout: false,
        taps_column: false,
    },
    Declaration {
        family: "qwen2",
        shard_widths: &[1, 2],
        template: qwen2::TEMPLATE,
        generation_opener: qwen2::GENERATION_OPENER,
        renderer: qwen2::renderer,
        selecting_markers: qwen2::RENDERED_MARKERS,
        flush: FlushMechanism::TruncateToPosition,
        // The native tap stands for this family, 2026-08-19: the fork's
        // forward_with_intermediates for qwen2 on the single device and the
        // pair's own per-layer fold. The GGUF side cannot tap - standing
        // that tap up is code this program has not written - and the judge
        // reads the container beside this flag, per readout::judge.
        taps_readout: true,
        // The GGUF tap's column answer, shown for this family first, per
        // Spec section 7's diagnostic clause.
        taps_column: true,
    },
    Declaration {
        // **Qwen3 declares its own architecture and renders the same ChatML
        // scaffolding**, so it cites the qwen2 module rather than growing a
        // second one that would hold the same two markers. Measured: the
        // marker vocabulary is identical and both promote to one token, and the
        // turn shape this template encodes is what both models' own templates
        // produce.
        //
        // Their full templates are not identical. Qwen3's is longer and adds
        // `<think>` reasoning blocks and an `enable_thinking` switch. That is
        // outbound, so it reaches the parse rather than the render: a reasoning
        // block arrives as ordinary assistant text today. Named here so the day
        // it needs separating is a change with a stated starting point rather
        // than a surprise in a trace.
        family: "qwen3",
        shard_widths: &[1, 2],
        template: qwen2::TEMPLATE,
        generation_opener: qwen2::GENERATION_OPENER,
        renderer: qwen2::renderer,
        selecting_markers: qwen2::RENDERED_MARKERS,
        flush: FlushMechanism::TruncateToPosition,
        // **Declared on a measurement, 2026-09-02**, per charter section
        // 13.7's bar and the rule that flipping this is a claim about the
        // family rather than a line in a table. The claim is that the GGUF
        // tap reads this architecture's residual correctly and changes no
        // token doing it, bought by `tests/readout_neutral.rs` against
        // Qwen3-8B-BF16 on a real device: two seeds, sixty-four tokens
        // each, byte-identical with the election on and off, 2,340 figures
        // folded over sixty-five forwards at thirty-six layers. The column
        // half is bought beside it by `tests/loaded.rs` against the same
        // artifact, one message per sampled position at thirty-six by four
        // thousand and ninety-six. **The sibling keys are not flipped by
        // this act**: `qwen3moe` and `qwen35` declare their own
        // architectures and each owes its own measurement.
        taps_readout: true,
        taps_column: true,
    },
    Declaration {
        // **Qwen3's sparse sibling declares its own architecture**, so it is
        // its own key citing the qwen2 module for the reason the dense one
        // does. Measured by the marker probe against a Qwen3-30B-A3B
        // tokenizer: the rendered set is qwen2's exactly, carrying not even
        // the vision markers the qwen35 artifacts do.
        family: "qwen3moe",
        shard_widths: &[1, 2],
        template: qwen2::TEMPLATE,
        generation_opener: qwen2::GENERATION_OPENER,
        renderer: qwen2::renderer,
        selecting_markers: qwen2::RENDERED_MARKERS,
        flush: FlushMechanism::TruncateToPosition,
        taps_readout: false,
        taps_column: false,
    },
    Declaration {
        // **Qwen3.5 declares its own architecture and renders the same ChatML
        // scaffolding**, so it cites the qwen2 module for the reason qwen3
        // does rather than growing a third holding the same two markers.
        // Measured by the marker probe of `tests/markers.rs` against a
        // Qwen3.6 tokenizer, which is what declares this architecture: the
        // rendered set promotes to one token each, and a lookalike that no
        // family declares does not, so the vocabulary agrees rather than the
        // tokenizer promoting everything.
        //
        // The artifacts also carry vision markers, `<|vision_start|>` and its
        // neighbours, which no text turn renders. They are named here as
        // present and unused rather than left for a later reader to wonder at.
        //
        // **The flush is by re-establishing, and this is the first entry where
        // it is.** Corrected 2026-08-17: it declared truncation, copied from
        // the qwen2 entry along with the template it legitimately shares.
        // Sharing a marker vocabulary says nothing about how state rolls back.
        //
        // `llm_arch_is_hybrid` in the pinned llama.cpp names `QWEN35` and
        // `QWEN35MOE`, so these artifacts carry recurrent layers beside their
        // attention. A recurrent state is a running summary rather than a
        // per-position cache, so it cannot be partially erased, and the engine
        // says so: measured on a Qwen3.6 artifact, `seq_rm` answered `false`
        // while the seam reported the turn flushed.
        family: "qwen35",
        shard_widths: &[1, 2],
        template: qwen2::TEMPLATE,
        generation_opener: qwen2::GENERATION_OPENER,
        renderer: qwen2::renderer,
        selecting_markers: qwen2::RENDERED_MARKERS,
        flush: FlushMechanism::ReestablishAndReprefill,
        taps_readout: false,
        taps_column: false,
    },
    Declaration {
        // The sparse sibling declares its own architecture again, so it is its
        // own key and its own probe. Two keys citing one module is a claim
        // about their markers agreeing, and this one is measured rather than
        // inherited from the dense entry above.
        // Its sparse sibling is hybrid for the same reason and by the same
        // list, and it is the artifact the measurement above was taken on.
        family: "qwen35moe",
        shard_widths: &[1, 2],
        template: qwen2::TEMPLATE,
        generation_opener: qwen2::GENERATION_OPENER,
        renderer: qwen2::renderer,
        selecting_markers: qwen2::RENDERED_MARKERS,
        flush: FlushMechanism::ReestablishAndReprefill,
        // **Declared on a measurement, 2026-08-23**, per charter section
        // 13.7's bar and the Spec's rule that flipping this is a claim about
        // the family rather than a line in a table. The claim is that the
        // GGUF tap reads this architecture's residual correctly and changes
        // no token doing it, and it is bought by
        // `tests/readout_neutral.rs` against the deployed artifact on a real
        // device. This is the architecture the workshop serves, so the
        // election it grants is one an operator can actually make.
        taps_readout: true,
        taps_column: false,
    },
    Declaration {
        // **The first entry that had to grow a module rather than cite one.**
        // Every qwen key above renders qwen2's scaffolding under its own
        // architecture, so the entry was the whole act. This family renames a
        // role and opens its prefix with a token its turns do not repeat, and
        // neither is a string a table row can hold.
        //
        // Measured by the marker probe of `tests/markers.rs` against
        // `gemma-4-26B-A4B-it-qat-UD-Q4_K_XL.gguf`, whose header declares this
        // architecture: all five rendered markers promote to exactly one token,
        // and the lookalike control does not, so the vocabulary agrees rather
        // than the tokenizer promoting everything. Two of the five, the channel
        // pair, are `USER_DEFINED` rather than `CONTROL` in that artifact's
        // token table, which is why reading the template was not enough.
        //
        // The flush is by truncation for the reason every entry above declares
        // it: attention KV rolls back by position. The interleaved sliding
        // window this family's attention uses is a window over positions and
        // does not change that. It is declared rather than inferred from the
        // architecture string, per Spec section 4.4.
        family: "gemma4",
        shard_widths: &[1, 2],
        template: gemma4::TEMPLATE,
        generation_opener: gemma4::GENERATION_OPENER,
        renderer: gemma4::renderer,
        selecting_markers: gemma4::RENDERED_MARKERS,
        flush: FlushMechanism::TruncateToPosition,
        taps_readout: false,
        taps_column: false,
    },
    Declaration {
        // **The same module as the qwen keys, and a different flush.** Its
        // template is `<|im_start|>role\n ... <|im_end|>\n`, qwen2's
        // scaffolding exactly, so it cites that module for the reason qwen3
        // does. Measured by the marker probe against a Nemotron 3 Nano 30B A3B
        // artifact: the rendered set is qwen2's and both markers promote.
        //
        // **What it does not share is how state rolls back.**
        // `llm_arch_is_hybrid` names `NEMOTRON_H_MOE`, so this entry declares
        // `ReestablishAndReprefill` beside four qwen keys citing the same
        // module, two of which truncate and two of which do not. That is the
        // whole reason the flush is declared per architecture here rather than
        // answered by the shared renderer: one module, one trait object, and
        // three different answers across the rows it serves.
        //
        // The `<think>` pair its model emits is outbound, reaching the parse
        // rather than the render, on the same footing as qwen3's and named at
        // that entry.
        family: "nemotron_h_moe",
        shard_widths: &[1, 2],
        template: qwen2::TEMPLATE,
        generation_opener: qwen2::GENERATION_OPENER,
        renderer: qwen2::renderer,
        selecting_markers: qwen2::RENDERED_MARKERS,
        flush: FlushMechanism::ReestablishAndReprefill,
        taps_readout: false,
        taps_column: false,
    },
    Declaration {
        // **The first family that names no role**, wrapping the user's text in
        // `[INST]` and `[/INST]` and leaving the model's bare, so the shape
        // carries the speaker and there is nothing for a placeholder to fill.
        // Its generation opener is empty because `[/INST]` is the assistant's
        // turn already opened. Both are the module's to explain.
        //
        // Measured by the marker probe against
        // `Devstral-Small-2-24B-Instruct-2512-Q4_K_M.gguf`: all four rendered
        // markers promote to one token, at ids 1, 2, 3 and 4, and the lookalike
        // control does not.
        //
        // **Most Mistral artifacts do not declare this architecture.**
        // Mistral-Small 3.1, Mistral-Small 3.2 and Magistral-Small report
        // `llama` and would resolve to that entry's llama-3 header markers,
        // which their tokenizers do not promote. That is #88's open case and
        // this entry does not close it: what this entry serves is the artifacts
        // whose header says `mistral3`.
        //
        // The flush is by truncation: this family is absent from
        // `llm_arch_is_hybrid` and from `llm_arch_is_recurrent`, so its
        // attention KV rolls back by position, and `tests/markers.rs` checks
        // that declaration against the artifact rather than trusting this
        // comment.
        family: "mistral3",
        shard_widths: &[1, 2],
        template: mistral3::TEMPLATE,
        generation_opener: mistral3::GENERATION_OPENER,
        renderer: mistral3::renderer,
        // [`mistral3::SELECTING_MARKERS`], not the full rendered set: a
        // selecting set carrying `<s>` can never match a rendering, and a
        // value that is only read when the architecture is contested must
        // still be one that can be read.
        selecting_markers: mistral3::SELECTING_MARKERS,
        flush: FlushMechanism::TruncateToPosition,
        taps_readout: false,
        taps_column: false,
    },
    Declaration {
        // **The second contested architecture, and the first where a vendor's
        // own current line is what contests it.** Three first-party Microsoft
        // artifacts declare `phi3`: the minis render role tags and Phi-4 14B
        // renders ChatML with a separator token. The marker sets are disjoint
        // to the point that the mini's vocabulary carries no `<|im_start|>`
        // at all, so the two rows cannot be ambiguous under any rendering.
        //
        // Measured by the marker probe against
        // `microsoft_Phi-4-mini-instruct-Q4_K_M.gguf`: the three tag markers
        // promote to one token each, ids 200021, 200019, 200020, and the
        // lookalike control does not. The flush is by truncation, `phi3`
        // absent from both of the engine's cannot-roll-back lists, and the
        // probe's cross-check reads that against the artifact rather than
        // trusting this comment.
        family: "phi3",
        shard_widths: &[1, 2],
        template: phi::TAG_TEMPLATE,
        generation_opener: phi::TAG_GENERATION_OPENER,
        renderer: phi::tag_renderer,
        selecting_markers: phi::TAG_RENDERED_MARKERS,
        flush: FlushMechanism::TruncateToPosition,
        taps_readout: false,
        taps_column: false,
    },
    Declaration {
        // The separator row of the pair above, serving Phi-4 14B artifacts.
        // **Not a qwen2 citation on purpose**: `<|im_sep|>` stands where
        // qwen2's ChatML puts a newline and no newline follows the close, so
        // the format is phi's own and lives in phi's module.
        family: "phi3",
        shard_widths: &[1, 2],
        template: phi::SEP_TEMPLATE,
        generation_opener: phi::SEP_GENERATION_OPENER,
        renderer: phi::sep_renderer,
        selecting_markers: phi::SEP_RENDERED_MARKERS,
        flush: FlushMechanism::TruncateToPosition,
        taps_readout: false,
        taps_column: false,
    },
    Declaration {
        // The key is what llama.cpp writes into `general.architecture`, which
        // is hyphenated. A key spelled any other way is a family no artifact
        // header ever selects, unreachable rather than wrong-looking.
        family: "gpt-oss",
        shard_widths: &[1, 2],
        template: gpt_oss::TEMPLATE,
        generation_opener: gpt_oss::GENERATION_OPENER,
        renderer: gpt_oss::renderer,
        selecting_markers: gpt_oss::RENDERED_MARKERS,
        flush: FlushMechanism::TruncateToPosition,
        taps_readout: false,
        taps_column: false,
    },
];

/// Render one message through a family's template, in a single pass.
///
/// **The shared kernel again, and it holds no marker.** The two placeholders
/// are the template's own vocabulary rather than any family's. A single pass is
/// what keeps a message whose text contains `{role}` from being substituted
/// into: successive `replace` calls would rewrite the output of the one before.
pub fn render_template(template: &str, role: &str, message: &str) -> String {
    let mut rendered = String::with_capacity(template.len() + role.len() + message.len());
    let mut rest = template;
    while let Some(at) = rest.find('{') {
        rendered.push_str(&rest[..at]);
        let tail = &rest[at..];
        if let Some(stripped) = tail.strip_prefix("{role}") {
            rendered.push_str(role);
            rest = stripped;
        } else if let Some(stripped) = tail.strip_prefix("{message}") {
            rendered.push_str(message);
            rest = stripped;
        } else {
            // Not a placeholder. Emit the brace and continue past it, so a
            // template carrying a literal brace renders unharmed.
            rendered.push('{');
            rest = &tail[1..];
        }
    }
    rendered.push_str(rest);
    rendered
}

/// Why a family lookup or a width judgment refused.
///
/// This is the crate's own vocabulary rather than the floor's, because the
/// floor's refusal set carries no case that names a family, and **what the
/// no-silent-substitution test reads is the family the refusal names.** A
/// refusal arriving on some other ground does not satisfy it, so the name has to
/// survive to the place the test reads.
#[derive(Debug, Clone, PartialEq)]
pub enum FamilyRefusal {
    /// The artifact header names a family this binary does not carry. The name
    /// travels with the refusal rather than being flattened to a generic
    /// unreadable, which is what makes the substitution visible.
    UnknownFamily(FamilyName),
    /// **More than one entry declares this architecture, and the caller asked
    /// with the architecture alone.** Answering by position would make the
    /// table's order the selector, so this is a refusal rather than a first
    /// match. The caller holding the artifact's template reaches [`select`].
    ArchitectureContested(FamilyName),
    /// The architecture is contested and the artifact declares no chat
    /// template, so the fact that would choose between the entries is absent.
    TemplateAbsent(FamilyName),
    /// The architecture is contested and the template detector did not
    /// recognise this artifact's template, so no marker set was derived.
    TemplateUnrecognised(FamilyName),
    /// The artifact rendered, and its markers match no entry sharing its
    /// architecture. The refusal names the architecture it declared, per Spec
    /// section 5, rather than reporting the family as uncarried.
    MarkersMatchNoEntry(FamilyName),
    /// **The artifact's markers match more than one entry.** The compile-time
    /// uniqueness pin should make this unreachable, and it is carried because
    /// a table this crate cannot read unambiguously is one it must not read by
    /// position. Belt and braces, per Spec section 5: order decides nothing at
    /// either.
    MarkersAmbiguous(FamilyName),
    /// **The registry carries this family and the named backend does not serve
    /// it.** Distinct from [`FamilyRefusal::UnknownFamily`], which says the
    /// binary carries no such family at all: a reader told that about a family
    /// the registry does serve goes looking for a row that is present. The
    /// backend travels with the name because which of the two peers was asked
    /// is the whole of the fact.
    BackendDoesNotServe {
        family: FamilyName,
        backend: &'static str,
    },
    /// The family is carried, but it does not shard across the requested width.
    WidthNotDeclared {
        family: FamilyName,
        requested: u32,
        declared: &'static [u32],
    },
}

impl From<FamilyRefusal> for LifecycleRefusal {
    /// What crosses the seam. The floor's set is closed at `weaver-types` and
    /// carries no family-naming case, so the name is lost at the boundary and
    /// kept on this side of it. Both cases are admission judgments about a
    /// device set or an artifact this binary cannot serve.
    fn from(refusal: FamilyRefusal) -> Self {
        match refusal {
            FamilyRefusal::UnknownFamily(_)
            // **Every unresolved selection crosses as unreadable**, which is
            // what the floor's closed set carries for an artifact this binary
            // cannot serve. The distinctions above are this crate's to act on
            // and the operator's to read in the refusal, and none of them is a
            // device condition, so none maps to `DeviceCannotAdmit`.
            | FamilyRefusal::ArchitectureContested(_)
            | FamilyRefusal::TemplateAbsent(_)
            | FamilyRefusal::TemplateUnrecognised(_)
            | FamilyRefusal::MarkersMatchNoEntry(_)
            | FamilyRefusal::MarkersAmbiguous(_)
            // A family one peer serves and the other does not is a fact about
            // the artifact and the backend it was handed to, and no more a
            // device condition than the rest.
            | FamilyRefusal::BackendDoesNotServe { .. } => LifecycleRefusal::ArtifactUnreadable,
            FamilyRefusal::WidthNotDeclared { .. } => LifecycleRefusal::DeviceCannotAdmit,
        }
    }
}

/// Byte equality for two string constants, in a const context.
const fn same_text(left: &str, right: &str) -> bool {
    let (left, right) = (left.as_bytes(), right.as_bytes());
    if left.len() != right.len() {
        return false;
    }
    let mut index = 0;
    while index < left.len() {
        if left[index] != right[index] {
            return false;
        }
        index += 1;
    }
    true
}

/// Whether a byte is a separator the family key does not read.
///
/// **The rule of the key lives here and in [`key_byte_folded`], once.** The two
/// containers do not spell one architecture alike: GGUF writes `qwen35moe` and
/// `gpt-oss` where a safetensors `config.json` writes `qwen3_5_moe` and
/// `gpt_oss`, so the separators are what the spellings differ by and the key
/// drops them, per Spec section 5.
const fn key_byte_ignored(byte: u8) -> bool {
    matches!(byte, b'_' | b'-' | b'.')
}

/// A byte folded to the family key's case. ASCII letters fold to lower case
/// and every other byte stands as written.
const fn key_byte_folded(byte: u8) -> u8 {
    byte.to_ascii_lowercase()
}

/// The next byte of `text` from `at` that the key reads, folded, with the
/// index after it. `None` once the text is spent.
const fn next_key_byte(text: &[u8], mut at: usize) -> Option<(u8, usize)> {
    while at < text.len() {
        let byte = text[at];
        at += 1;
        if !key_byte_ignored(byte) {
            return Some((key_byte_folded(byte), at));
        }
    }
    None
}

/// **Whether two spellings name one family key**, in a const context, per Spec
/// section 5.
///
/// Both sides pass through the same fold before they meet, so the registry's
/// spelling stays whatever container it was measured against and the artifact's
/// spelling stays whatever its header declared. What is compared is the key and
/// what is refused by is the spelling: a miss names the string the header
/// carried, not the fold of it.
pub const fn same_key(left: &str, right: &str) -> bool {
    let (left, right) = (left.as_bytes(), right.as_bytes());
    let (mut at_left, mut at_right) = (0, 0);
    loop {
        match (next_key_byte(left, at_left), next_key_byte(right, at_right)) {
            (None, None) => return true,
            (Some((l, next_left)), Some((r, next_right))) => {
                if l != r {
                    return false;
                }
                at_left = next_left;
                at_right = next_right;
            }
            _ => return false,
        }
    }
}

/// The family key a spelling folds to, as a string, by the same walk
/// [`same_key`] compares by.
///
/// Read by the tests and by nothing on the admission path, which compares
/// without allocating. `same_key(a, b)` and `normalised_key(a) ==
/// normalised_key(b)` are one rule reached two ways, and the test module holds
/// them to that.
pub fn normalised_key(spelled: &str) -> String {
    let bytes = spelled.as_bytes();
    let mut folded = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while let Some((byte, next)) = next_key_byte(bytes, at) {
        folded.push(byte);
        at = next;
    }
    // The fold drops and lowercases ASCII bytes only, and a multi-byte
    // sequence has no ASCII byte in it, so what remains is the UTF-8 that
    // arrived minus some ASCII.
    String::from_utf8(folded).expect("dropping and folding ASCII bytes preserves UTF-8")
}

/// Whether every marker in `inner` appears in `outer`.
const fn contained_in(inner: &[&str], outer: &[&str]) -> bool {
    let mut i = 0;
    while i < inner.len() {
        let mut found = false;
        let mut j = 0;
        while j < outer.len() {
            if same_text(inner[i], outer[j]) {
                found = true;
            }
            j += 1;
        }
        if !found {
            return false;
        }
        i += 1;
    }
    true
}

/// **Whether the table can be read unambiguously**, which is the compile-time
/// property Spec section 5 requires of it.
///
/// **The pin is subset-freedom rather than inequality, and the difference is
/// what makes the guarantee true.** Selection matches an entry when every one
/// of its markers appears in what the artifact rendered, so two entries whose
/// sets merely differ can still both match: where one set is contained in the
/// other, an artifact rendering the larger satisfies both. Pinning inequality
/// alone would leave ambiguity reachable while reading as though it were
/// closed, and section 5 says the compile-time check is what makes ambiguity
/// unreachable. Equal sets are contained in each other, so this subsumes the
/// uniqueness the section names rather than replacing it.
///
/// Marker sets are compared only within one architecture, because entries that
/// declare different architectures never compete. Sharing is judged by
/// [`same_key`] rather than by spelling, so that the pin reads the table the
/// way selection reads it.
const fn table_reads_unambiguously() -> bool {
    let mut i = 0;
    while i < REGISTRY.len() {
        let mut j = i + 1;
        while j < REGISTRY.len() {
            if same_key(REGISTRY[i].family, REGISTRY[j].family)
                && (contained_in(REGISTRY[i].selecting_markers, REGISTRY[j].selecting_markers)
                    || contained_in(REGISTRY[j].selecting_markers, REGISTRY[i].selecting_markers))
            {
                return false;
            }
            j += 1;
        }
        i += 1;
    }
    true
}

const _: () = assert!(
    table_reads_unambiguously(),
    "two registry entries share an architecture and neither marker set \
     distinguishes them, so the table's order would be the selector"
);

/// **Whether no two spellings in the table fold to one key**, which Spec
/// section 5 requires of it beside the marker-set uniqueness above.
///
/// Two entries spelled alike are one contested architecture and are allowed,
/// `llama` and `phi3` being the standing cases. Two entries spelled differently
/// that fold to one key would be one architecture the table admits under two
/// names, one per container, with nothing saying which row a given artifact
/// reaches, so the build refuses the table rather than admission reading it.
const fn spellings_do_not_collapse() -> bool {
    let mut i = 0;
    while i < REGISTRY.len() {
        let mut j = i + 1;
        while j < REGISTRY.len() {
            if same_key(REGISTRY[i].family, REGISTRY[j].family)
                && !same_text(REGISTRY[i].family, REGISTRY[j].family)
            {
                return false;
            }
            j += 1;
        }
        i += 1;
    }
    true
}

const _: () = assert!(
    spellings_do_not_collapse(),
    "two registry entries spelled differently fold to one family key, so the \
     table carries one architecture under two names"
);

/// The conversation every contested artifact is asked to render.
///
/// **Frozen here rather than built where it is called**, per Spec section 5. A
/// derivation whose input varies by call site compares two artifacts against
/// two different questions, and the comparison is only meaningful where every
/// artifact answered the same one. The contents carry no meaning: what is read
/// is the scaffolding the template puts around them, so the bodies are single
/// characters and the roles are the three the canonical shape names.
///
/// Read only by the detector arm below, so a build carrying no engine has no
/// reader for it. The allow says that rather than widening the build's surface
/// to quiet a lint, which is the form [`crate::residency`] already uses for the
/// flush mechanism.
#[cfg_attr(not(feature = "gguf"), allow(dead_code))]
const PROBE_CONVERSATION: &[(&str, &str)] = &[("system", "s"), ("user", "u"), ("assistant", "a")];

/// Render the probe through the template detector and return what it emits.
///
/// **This is a detector rather than an evaluator**, per Spec section 5: it
/// matches the template text against a table of families it carries and emits
/// its own rendering of the one it settles on. It is not a jinja parser and
/// upstream says so. It takes no model handle, so this reaches no weights.
#[cfg(feature = "gguf")]
fn render_probe(template: &str) -> Option<String> {
    use std::ffi::CString;
    let template = CString::new(template).ok()?;
    let held: Vec<(CString, CString)> = PROBE_CONVERSATION
        .iter()
        .map(|(role, body)| (CString::new(*role).unwrap(), CString::new(*body).unwrap()))
        .collect();
    let chat: Vec<llama_cpp_sys_2::llama_chat_message> = held
        .iter()
        .map(|(role, body)| llama_cpp_sys_2::llama_chat_message {
            role: role.as_ptr(),
            content: body.as_ptr(),
        })
        .collect();
    // The rendering is the probe's scaffolding around six bytes of body, so a
    // fixed buffer is sufficient and a refusal is what an overrun becomes.
    let mut buffer = vec![0u8; 8192];
    // SAFETY: the template and the messages outlive the call, `chat` describes
    // its own length, and the buffer's length is passed as written.
    let written = unsafe {
        llama_cpp_sys_2::llama_chat_apply_template(
            template.as_ptr(),
            chat.as_ptr(),
            chat.len(),
            true,
            buffer.as_mut_ptr().cast::<std::os::raw::c_char>(),
            buffer.len() as i32,
        )
    };
    let written = usize::try_from(written).ok()?;
    if written > buffer.len() {
        return None;
    }
    buffer.truncate(written);
    String::from_utf8(buffer).ok()
}

/// **A build carrying no GGUF backend carries no detector**, so a contested
/// architecture has no derivation available and refuses rather than picking.
/// Such a build serves no GGUF artifact at all, so nothing that could reach
/// this is admissible for other reasons first.
#[cfg(not(feature = "gguf"))]
fn render_probe(_template: &str) -> Option<String> {
    None
}

/// Look a family up in the compile-time table.
///
/// **No silent substitution.** A miss is a refusal naming the family, never a
/// nearest match and never a default declaration.
///
/// **A contested architecture refuses here rather than answering by position.**
/// Where more than one entry declares the name, this function cannot choose
/// between them and returning the first would make the table's order the
/// selector. [`select`] is the door that can choose, because it holds the
/// artifact's template as well as its architecture.
///
/// **The name is matched by key rather than by spelling**, per Spec section 5:
/// [`same_key`] folds the entry's spelling and the header's alike, so a
/// safetensors `qwen3_5_moe` finds the row spelled `qwen35moe`. The refusal
/// still carries the spelling the header declared.
pub fn lookup(name: &FamilyName) -> Result<&'static Declaration, FamilyRefusal> {
    let mut found = None;
    for declaration in REGISTRY {
        if same_key(declaration.family, &name.0) {
            if found.is_some() {
                return Err(FamilyRefusal::ArchitectureContested(name.clone()));
            }
            found = Some(declaration);
        }
    }
    found.ok_or_else(|| FamilyRefusal::UnknownFamily(name.clone()))
}

/// Select the entry this artifact belongs to, from its architecture and, where
/// the architecture is contested, the marker set it renders.
///
/// **Where one architecture carries one entry the key is unchanged and nothing
/// is rendered**, per Spec section 5. That is every family this binary carried
/// before the act that added this function, and it is what keeps a family the
/// detector does not recognise admissible: gemma4's template returns an error
/// rather than a rendering, so an unconditional render would refuse a family
/// the registry serves.
///
/// **Where it is contested the artifact's own rendering decides**, and a
/// derivation that produces no set refuses rather than falling back to the
/// architecture, because falling back is the silent substitution this reached
/// for the template to prevent.
///
/// **The candidates are gathered by key rather than by spelling**, per Spec
/// section 5, so the two containers' spellings of one architecture reach one
/// set of rows. What the fold bridges is a spelling and nothing more: a
/// safetensors `mistral` is not the registry's `mistral3` under any fold, and a
/// family the registry spells differently from a container is a row owed
/// rather than a wider fold.
pub fn select(
    name: &FamilyName,
    chat_template: Option<&str>,
) -> Result<&'static Declaration, FamilyRefusal> {
    let candidates: Vec<&'static Declaration> = REGISTRY
        .iter()
        .filter(|declaration| same_key(declaration.family, &name.0))
        .collect();
    match candidates.as_slice() {
        [] => return Err(FamilyRefusal::UnknownFamily(name.clone())),
        [only] => return Ok(only),
        _ => {}
    }

    let Some(template) = chat_template else {
        return Err(FamilyRefusal::TemplateAbsent(name.clone()));
    };
    let Some(rendered) = render_probe(template) else {
        return Err(FamilyRefusal::TemplateUnrecognised(name.clone()));
    };

    let mut matched = candidates.iter().filter(|declaration| {
        declaration
            .selecting_markers
            .iter()
            .all(|marker| rendered.contains(marker))
    });
    match (matched.next(), matched.next()) {
        (Some(one), None) => Ok(one),
        (Some(_), Some(_)) => Err(FamilyRefusal::MarkersAmbiguous(name.clone())),
        (None, _) => Err(FamilyRefusal::MarkersMatchNoEntry(name.clone())),
    }
}

/// Judge a requested shard width against what the selected entry declares.
///
/// The width condition reads nothing, which is why Spec section 3 judges it
/// before the room and reach conditions that each cost a driver query.
///
/// **It takes the entry rather than the architecture, because an architecture
/// is not always enough to find one.** Where two entries share an
/// architecture, resolving it needs the artifact's template, and the caller
/// has already done that: admit selects once and judges width against what it
/// selected, so the width answer and everything after it read one declaration
/// rather than two resolutions that could disagree.
pub fn judge_width(declaration: &Declaration, requested: u32) -> Result<(), FamilyRefusal> {
    if declaration.shards_across(requested) {
        return Ok(());
    }
    Err(FamilyRefusal::WidthNotDeclared {
        family: FamilyName(declaration.family.to_string()),
        requested,
        declared: declaration.shard_widths,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **A family the binary does not carry is refused, and the refusal names
    /// the family.** What this test reads is the name, not the load's outcome,
    /// so a refusal arriving on some other ground does not satisfy it.
    ///
    /// Perturbation: make `lookup` fall back to `REGISTRY[0]` on a miss and
    /// this test fails, because the lookup then succeeds and no refusal is
    /// produced at all. Watched under exactly that substitution.
    #[test]
    fn an_uncarried_family_refuses_by_name() {
        let absent = FamilyName("not-a-family-this-binary-carries".into());
        let outcome = lookup(&absent);
        // Read as a pattern rather than by equality: [`Declaration`] carries a
        // function pointer and so cannot answer `==` meaningfully. The claim is
        // unchanged, and a lookup that fell back to a nearest match returns
        // `Ok` and fails this the same way.
        assert!(
            matches!(&outcome, Err(FamilyRefusal::UnknownFamily(named)) if named == &absent),
            "the refusal carries the family the header named, got {:?}",
            outcome.as_ref().map(|declaration| declaration.family)
        );
    }

    /// **The two containers' spellings of one architecture select one entry**,
    /// per Spec section 5 and #212's table of 2026-08-21. Each pair is the
    /// safetensors `model_type` beside the GGUF `general.architecture`, and the
    /// `qwen3_5` pair is the one this workshop's own library adds to the table:
    /// five Qwen3.5 exports declare it against the registry's `qwen35`.
    ///
    /// The two selections are compared by address, because the claim is that
    /// one row is reached, not that two rows agree about their fields.
    ///
    /// Perturbation: put `declaration.family == name.0` back in `select` and
    /// every safetensors spelling here refuses as `UnknownFamily`, which is the
    /// admission refusal #212 found. Watched under exactly that reversion.
    #[test]
    fn one_architecture_selects_one_entry_under_either_containers_spelling() {
        let pairs = [
            ("qwen3_5_moe", "qwen35moe"),
            ("qwen3_moe", "qwen3moe"),
            ("qwen3_5", "qwen35"),
            ("gpt_oss", "gpt-oss"),
            ("nemotron_h_moe", "nemotron_h_moe"),
        ];
        for (safetensors, gguf) in pairs {
            let by_safetensors = select(&FamilyName(safetensors.into()), None)
                .unwrap_or_else(|refusal| panic!("{safetensors} refused: {refusal:?}"));
            let by_gguf = select(&FamilyName(gguf.into()), None)
                .unwrap_or_else(|refusal| panic!("{gguf} refused: {refusal:?}"));
            assert!(
                std::ptr::eq(by_safetensors, by_gguf),
                "{safetensors} and {gguf} reached two rows, {} and {}",
                by_safetensors.family,
                by_gguf.family
            );
            assert!(
                same_key(safetensors, gguf),
                "{safetensors} and {gguf} are one key by the const comparison too"
            );
        }
    }

    /// **A spelling difference that is a different family is not the fold's
    /// to bridge.** Safetensors Mistral exports declare `mistral` and the
    /// registry carries `mistral3`, and the two are two names for two things,
    /// so `mistral` refuses naming itself while `mistral3` selects, per Spec
    /// section 5. The registry question that leaves open is #212's, not this
    /// test's.
    ///
    /// Perturbation: widen the fold to drop ASCII digits as well. Both compile
    /// pins stop the build first, the registry's own qwen keys folding
    /// together and sharing markers, and with both pins removed `mistral`
    /// reaches the `mistral3` row and this test fails on the first assertion.
    /// Watched under exactly that pair.
    #[test]
    fn a_different_family_spelled_differently_is_not_bridged() {
        let mistral = FamilyName("mistral".into());
        let outcome = select(&mistral, None);
        assert!(
            matches!(&outcome, Err(FamilyRefusal::UnknownFamily(named)) if named == &mistral),
            "mistral is not carried and the refusal names it, got {:?}",
            outcome.as_ref().map(|declaration| declaration.family)
        );
        assert_eq!(
            select(&FamilyName("mistral3".into()), None)
                .expect("the registry carries mistral3")
                .family,
            "mistral3"
        );
        assert!(!same_key("mistral", "mistral3"));
    }

    /// **No two registry spellings fold to one key**, per Spec section 5,
    /// read at runtime beside the compile-time pin that says the same. Two
    /// rows spelled alike are a contested architecture and pass, so what is
    /// asserted is that agreement under the fold implies agreement in
    /// spelling. The table of expected keys is written out so that a reader
    /// can check each registry spelling against its fold by eye, and so that
    /// a row entering under a new spelling has to be recorded here.
    ///
    /// Perturbation: add a row spelled `qwen3_moe` beside `qwen3moe` and the
    /// build stops on `spellings_do_not_collapse` before this runs. Remove
    /// that pin and this test fails on the pair instead, which is why both
    /// stand. Watched under the added row with the pin removed.
    #[test]
    fn no_two_registry_spellings_fold_to_one_key() {
        let expected = [
            ("llama", "llama"),
            ("qwen2", "qwen2"),
            ("qwen3", "qwen3"),
            ("qwen3moe", "qwen3moe"),
            ("qwen35", "qwen35"),
            ("qwen35moe", "qwen35moe"),
            ("gemma4", "gemma4"),
            ("nemotron_h_moe", "nemotronhmoe"),
            ("mistral3", "mistral3"),
            ("phi3", "phi3"),
            ("gpt-oss", "gptoss"),
        ];
        for row in REGISTRY {
            let folded = normalised_key(row.family);
            let recorded = expected
                .iter()
                .find(|(spelled, _)| *spelled == row.family)
                .unwrap_or_else(|| {
                    panic!("{} is a spelling this table does not record", row.family)
                });
            assert_eq!(
                folded, recorded.1,
                "{} folds otherwise than recorded",
                row.family
            );
        }
        for (i, left) in REGISTRY.iter().enumerate() {
            for right in &REGISTRY[i + 1..] {
                if same_key(left.family, right.family) {
                    assert_eq!(
                        left.family, right.family,
                        "two spellings fold to one key, so the table carries one \
                         architecture under two names"
                    );
                }
                assert_eq!(
                    same_key(left.family, right.family),
                    normalised_key(left.family) == normalised_key(right.family),
                    "the const comparison and the string fold disagree on {} and {}",
                    left.family,
                    right.family
                );
            }
        }
    }

    /// **The fold is lower case with `_`, `-`, and `.` dropped, and nothing
    /// else.** Digits stand, other punctuation stands, and a non-ASCII byte
    /// passes through unfolded, so the rule is exactly the one Spec section 5
    /// states rather than a looser one that happens to pass the pairs above.
    ///
    /// Perturbation: fold `/` as a separator too, which no registry key
    /// carries and so no compile pin sees, and the fourth assertion fails.
    /// Watched under exactly that widening.
    #[test]
    fn the_fold_drops_three_separators_and_folds_ascii_case_only() {
        assert_eq!(normalised_key("Qwen3_5-MoE.x"), "qwen35moex");
        assert_eq!(normalised_key("GPT_OSS"), "gptoss");
        assert_eq!(normalised_key("__-.-__"), "");
        assert_eq!(normalised_key("a/b"), "a/b");
        assert_eq!(normalised_key("caf\u{e9}"), "caf\u{e9}");
        assert!(same_key("Qwen3_5-MoE", "qwen35moe"));
        assert!(!same_key("qwen35moe", "qwen35"));
        assert!(!same_key("qwen35", "qwen35moe"));
    }

    /// **The width is judged by membership rather than against a bound.**
    ///
    /// Perturbation: change `shards_across` to `width <= max(shard_widths)` and
    /// this test still passes on the registry's contiguous sets, which is why
    /// the non-contiguous case is asserted here as well as pinned by the
    /// module doctest. Under that change the sparse assertion below fails.
    #[test]
    fn a_width_outside_the_declared_set_refuses() {
        // Judged against an entry, because `llama` is a contested
        // architecture and naming it alone reaches no single declaration.
        let entry = select(&FamilyName("qwen2".into()), None).expect("the registry carries qwen2");
        assert_eq!(judge_width(entry, 1), Ok(()));
        assert_eq!(judge_width(entry, 2), Ok(()));
        assert!(matches!(
            judge_width(entry, 3),
            Err(FamilyRefusal::WidthNotDeclared { requested: 3, .. })
        ));

        // The membership property, on a set no maximum describes.
        const SPARSE: Declaration = Declaration {
            family: "sparse",
            shard_widths: &[1, 4],
            template: "{message}",
            generation_opener: "",
            renderer: qwen2::renderer,
            selecting_markers: qwen2::RENDERED_MARKERS,
            flush: FlushMechanism::TruncateToPosition,
            taps_readout: true,
            taps_column: false,
        };
        assert!(SPARSE.shards_across(1));
        assert!(!SPARSE.shards_across(2), "a bound would admit this");
        assert!(!SPARSE.shards_across(3), "a bound would admit this");
        assert!(SPARSE.shards_across(4));
    }

    /// **An emission that opens a call marker and names nothing the parser can
    /// recover answers with that fact rather than with a clean turn.**
    ///
    /// The fixture is a rendered string against each family module, so this
    /// reads the parse's own answer and reaches neither a model nor a device,
    /// per Spec section 10. Every family is exercised, because the claim is
    /// about each family's parser and one passing says nothing about the rest.
    ///
    /// What it reads is two things and both matter: that the unrecovered fact
    /// is reported, and that the fragment did **not** also arrive as assistant
    /// text. The second is the half a collapse into the text path would break
    /// while the first went on looking fine.
    ///
    /// Perturbation: in `scan`, replace the `None` arm's push to `unrecovered`
    /// with `content.push(Content::Text(fragment.to_string()))` and this test
    /// fails, because the fragment then arrives as ordinary assistant text and
    /// no fact is reported. Watched under exactly that collapse.
    #[test]
    fn an_unrecoverable_call_is_reported_rather_than_read_as_prose() {
        let cases: Vec<(&str, Box<dyn Family>, String)> = vec![
            (
                "qwen2",
                Box::new(qwen2::Qwen2),
                format!(
                    "here you go{}not json at all{}",
                    qwen2::CALL_OPEN,
                    qwen2::CALL_CLOSE
                ),
            ),
            (
                "llama",
                Box::new(llama::Llama),
                format!(
                    "here you go{}not json at all{}",
                    llama::CALL_OPEN,
                    llama::CALL_CLOSE
                ),
            ),
            (
                "gpt-oss",
                Box::new(gpt_oss::GptOss),
                format!(
                    "here you go{}no recipient here{}",
                    gpt_oss::CALL_OPEN,
                    gpt_oss::TURN_END
                ),
            ),
        ];

        for (name, family, emission) in cases {
            let parsed = family.parse(&emission);

            assert!(
                parsed.has_unrecovered_call(),
                "{name}: the attempted call is its own reported fact"
            );

            let text = parsed.text();
            assert!(
                !text.contains("not json at all") && !text.contains("no recipient here"),
                "{name}: the fragment must not arrive as assistant text, got {text:?}"
            );
            assert!(
                text.contains("here you go"),
                "{name}: the prose before the marker is still text, got {text:?}"
            );

            // The verbatim emission survives whatever the parse concluded,
            // because the contract has both reaching the record.
            assert_eq!(parsed.verbatim, emission, "{name}: the verbatim is kept");
        }
    }

    /// **Qwen3 is carried, and it is served by the qwen2 module.**
    ///
    /// The architecture is its own, so without an entry the lookup refuses
    /// `UnknownFamily("qwen3")` before any device call, correctly and
    /// uselessly. What this guards is the entry existing: the marker probe of
    /// `tests/markers.rs` verifies the vocabularies agree, and needs the `gguf`
    /// feature to do it, so this side of the claim is asserted where every
    /// build can see it.
    /// **The qwen2 renderer speaks both halves of the tool bracket.** An
    /// assistant message carrying a recovered call renders it back inside
    /// `<tool_call>` tags with the name-and-arguments object, and a tool
    /// result renders as a `user` turn wrapping its content in
    /// `<tool_response>` tags - the two forms the family's tuning expects,
    /// asserted against the module's own marker constants so a drift in
    /// either is a drift this test names.
    #[test]
    fn qwen2_renders_the_tool_bracket() {
        let assistant = Message {
            role: Role::Assistant,
            content: vec![
                ContentBlock::Text {
                    text: "checking".to_string(),
                },
                ContentBlock::ToolCall(weaver_traits::ToolCall {
                    name: "calculator".to_string(),
                    arguments: r#"{"expression":"2^10"}"#.to_string(),
                }),
            ],
        };
        let rendered = qwen2::renderer()
            .render_delta(&assistant)
            .expect("the call renders");
        assert!(
            rendered.contains(qwen2::CALL_OPEN) && rendered.contains(qwen2::CALL_CLOSE),
            "the call wears its tags: {rendered}"
        );
        assert!(
            rendered.contains(r#""name":"calculator""#),
            "the name crosses: {rendered}"
        );
        assert!(
            rendered.contains(r#""arguments":{"expression":"2^10"}"#),
            "the arguments render as the object the model spoke: {rendered}"
        );
        assert!(rendered.contains("checking"), "the text stays: {rendered}");

        let result = Message {
            role: Role::ToolResult,
            content: vec![ContentBlock::ToolResult(weaver_traits::ToolResultBlock {
                content: "1024".to_string(),
            })],
        };
        let rendered = qwen2::renderer()
            .render_delta(&result)
            .expect("the result renders");
        assert!(
            rendered.starts_with("<|im_start|>user\n"),
            "a tool response rides a user turn: {rendered}"
        );
        assert!(
            rendered.contains(qwen2::RESPONSE_OPEN)
                && rendered.contains("1024")
                && rendered.contains(qwen2::RESPONSE_CLOSE),
            "the response wears its tags: {rendered}"
        );

        let misplaced = Message {
            role: Role::Assistant,
            content: vec![ContentBlock::ToolResult(weaver_traits::ToolResultBlock {
                content: "1024".to_string(),
            })],
        };
        assert!(
            qwen2::renderer().render_delta(&misplaced).is_err(),
            "a result in the wrong role refuses"
        );
    }

    /// **Qwen3.5 and its sparse sibling are carried, and served by the qwen2
    /// module.** Each declares its own architecture, so without an entry the
    /// lookup refuses `UnknownFamily` before any device call, correctly and
    /// uselessly. What this guards is the entries existing.
    ///
    /// The claim they rest on, that the marker vocabularies agree, is
    /// measured by `tests/markers.rs` against a tokenizer of each rather than
    /// inherited from qwen2 or from each other.
    #[test]
    fn the_qwen_family_keys_resolve_through_the_qwen2_module() {
        for family in ["qwen3moe", "qwen35", "qwen35moe"] {
            let declaration = lookup(&FamilyName(family.into()))
                .unwrap_or_else(|_| panic!("{family} is carried"));
            assert_eq!(declaration.template, qwen2::TEMPLATE, "{family}");
            assert_eq!(
                declaration.generation_opener,
                qwen2::GENERATION_OPENER,
                "{family}"
            );
        }
    }

    /// **gemma4 is carried, and it resolves to its own module rather than to
    /// another family's constants.**
    ///
    /// Without an entry the lookup refuses `UnknownFamily("gemma4")` before any
    /// device call, correctly and uselessly. What this guards is the entry
    /// existing and citing the right module: the template and the opener are
    /// gemma4's own, and the renderer the declaration hands out is the one that
    /// knows the word `model`.
    ///
    /// The marker vocabulary it rests on is measured by `tests/markers.rs`
    /// against a gemma4 tokenizer, which needs the `gguf` feature, so this side
    /// of the claim is asserted where every build can see it.
    #[test]
    fn gemma4_resolves_to_its_own_module() {
        let declaration = lookup(&FamilyName("gemma4".into())).expect("gemma4 is carried");
        assert_eq!(declaration.template, gemma4::TEMPLATE);
        assert_eq!(declaration.generation_opener, gemma4::GENERATION_OPENER);
        assert!(
            declaration
                .generation_opener
                .contains(gemma4::CHANNEL_CLOSE),
            "the opener carries the closed thought channel the artifact's own \
             template emits with thinking off"
        );

        // The renderer is this family's, which is what carries the role map and
        // the preamble. Read through a rendering rather than by comparing
        // pointers, a function pointer's address answering nothing.
        let rendered = (declaration.renderer)()
            .render_identity(&[said(Role::Assistant, "hi")])
            .expect("gemma4 renders");
        assert!(
            rendered.starts_with("<bos><|turn>model\n"),
            "the declaration hands out gemma4's own renderer, got {rendered:?}"
        );
    }

    /// **A family can carry the speaker in the turn's shape rather than in a
    /// word, and mistral3 is the first that does.**
    ///
    /// The user's text is wrapped and the model's is bare, so the two roles
    /// render to structurally different strings from the same content. Both
    /// halves are read, because a renderer that wrapped everything or wrapped
    /// nothing would satisfy either assertion alone.
    ///
    /// Perturbation: render the assistant through the user's arm and the
    /// second assertion fails, the model's turn arriving inside an instruction
    /// block the model was trained to read as the user speaking.
    #[test]
    fn a_family_may_carry_the_role_in_the_turns_shape() {
        let user = render_each(mistral3::renderer(), &[said(Role::User, "ping")])
            .expect("mistral3 renders a user turn");
        assert_eq!(user, "[INST]ping[/INST]", "the user's text is wrapped");

        let assistant = render_each(mistral3::renderer(), &[said(Role::Assistant, "pong")])
            .expect("mistral3 renders an assistant turn");
        assert_eq!(
            assistant, "pong</s>",
            "the model's text is bare and closed by the turn's end"
        );
    }

    /// **An empty generation opener is a declaration, not an omission.**
    ///
    /// Every other carried family appends an opener so the generation completes
    /// the assistant's turn rather than electing a speaker. mistral3 appends
    /// nothing because `[/INST]` already did both, and reading the two together
    /// is what says so: the delta ends at the marker that opens the model's
    /// turn.
    ///
    /// Perturbation: give the entry qwen2's opener and the second assertion
    /// fails, a ChatML turn header landing after an instruction block.
    #[test]
    fn the_mistral3_delta_ends_at_the_marker_that_opens_the_model_turn() {
        let declaration = lookup(&FamilyName("mistral3".into())).expect("mistral3 is carried");
        let delta = render_each(mistral3::renderer(), &[said(Role::User, "ping")])
            .expect("mistral3 renders a delta");

        assert!(
            delta.ends_with(mistral3::TURN_CLOSE),
            "the delta closes with the marker that opens the model's turn, got {delta:?}"
        );
        assert_eq!(
            declaration.generation_opener, "",
            "so nothing is appended after it"
        );

        // And the prefix carries the preamble the turns do not repeat, the
        // same joint gemma4 needed for a different reason.
        let prefix = mistral3::renderer()
            .render_identity(&[said(Role::User, "ping")])
            .expect("mistral3 renders a prefix");
        assert_eq!(prefix, "<s>[INST]ping[/INST]");
        assert!(!delta.contains(mistral3::BOS), "and a delta does not");
    }

    /// **One module, one trait object, and three different flushes across the
    /// rows it serves.**
    ///
    /// Six architecture keys cite the qwen2 module, and what they share is a
    /// rendering, not a mechanism: qwen2, qwen3 and qwen3moe truncate, qwen35
    /// and qwen35moe re-establish because they are hybrid, and
    /// `nemotron_h_moe` re-establishes for the same reason while being a
    /// different vendor's model entirely. **This is why the flush is read from
    /// the row rather than from the renderer**, and why `permits_truncation`
    /// could not stay on the trait: `Qwen2::declaration()` answers for qwen2
    /// whichever key was admitted, and here that answer is wrong for three of
    /// the six.
    ///
    /// **The sparse siblings are the pair worth reading twice.** `qwen3moe`
    /// truncates and `qwen35moe` does not, though both are Qwen mixtures of
    /// experts citing one module, because sparsity is not what decides this
    /// and the recurrent layers are.
    ///
    /// Perturbation: read the flush through `qwen2::renderer().declaration()`
    /// instead of through `lookup` and every non-qwen2 assertion below fails.
    #[test]
    fn one_module_serves_keys_whose_flush_mechanisms_differ() {
        let flush_of = |family: &str| {
            lookup(&FamilyName(family.into()))
                .unwrap_or_else(|_| panic!("{family} is carried"))
                .flush
        };

        for truncating in ["qwen2", "qwen3", "qwen3moe"] {
            assert_eq!(
                flush_of(truncating),
                FlushMechanism::TruncateToPosition,
                "{truncating} rolls back by position"
            );
            assert!(
                lookup(&FamilyName(truncating.into()))
                    .unwrap()
                    .permits_truncation(),
                "{truncating}"
            );
        }

        for reestablishing in ["qwen35", "qwen35moe", "nemotron_h_moe"] {
            assert_eq!(
                flush_of(reestablishing),
                FlushMechanism::ReestablishAndReprefill,
                "{reestablishing} carries recurrent state and cannot be partially erased"
            );
            assert!(
                !lookup(&FamilyName(reestablishing.into()))
                    .unwrap()
                    .permits_truncation(),
                "{reestablishing}"
            );
        }

        // All five render through the one module, which is the half that makes
        // the differing flush interesting rather than incidental.
        for shared in [
            "qwen2",
            "qwen3",
            "qwen3moe",
            "qwen35",
            "qwen35moe",
            "nemotron_h_moe",
        ] {
            assert_eq!(
                lookup(&FamilyName(shared.into())).unwrap().template,
                qwen2::TEMPLATE,
                "{shared} renders qwen2's scaffolding"
            );
        }
    }

    /// **One architecture, two formats, and the role tag is the marker.**
    ///
    /// The phi3 pair is the second contested architecture and the first where
    /// the role substitutes into the marker itself, so the tag renderer's
    /// output carries `<|user|>` as a built token rather than a role word
    /// between markers. Both formats are read so the disjointness the
    /// registry's selection rests on is a rendered fact rather than a table
    /// comment.
    ///
    /// Perturbation: render `PhiSep` through `TAG_TEMPLATE` and the second
    /// assertion fails, the separator format then rendering tags. Watched
    /// under exactly that.
    #[test]
    fn the_phi_formats_render_disjoint_markers_from_one_architecture() {
        let turn = [said(Role::User, "hi")];

        let tag = render_each(phi::tag_renderer(), &turn).expect("the tag format renders");
        assert_eq!(tag, "<|user|>hi<|end|>");

        let sep = render_each(phi::sep_renderer(), &turn).expect("the sep format renders");
        assert_eq!(sep, "<|im_start|>user<|im_sep|>hi<|im_end|>");

        // The disjointness selection rests on: no marker of one format
        // appears in the other's rendering.
        for marker in phi::TAG_RENDERED_MARKERS {
            assert!(
                !sep.contains(marker),
                "{marker} leaked into the sep rendering"
            );
        }
        for marker in phi::SEP_RENDERED_MARKERS {
            assert!(
                !tag.contains(marker),
                "{marker} leaked into the tag rendering"
            );
        }

        // And the tool role refuses under both, neither template carrying a
        // tool turn.
        let tool = [said(Role::ToolResult, "42")];
        assert!(render_each(phi::tag_renderer(), &tool).is_err());
        assert!(render_each(phi::sep_renderer(), &tool).is_err());
    }

    /// **Each phi row is reachable by its markers and answers its own
    /// declaration**, which is what keeps `Family::declaration` truthful on
    /// the first module whose architecture a by-name lookup refuses.
    #[test]
    fn the_phi_rows_select_by_their_own_marker_sets() {
        let name = FamilyName("phi3".into());
        assert!(
            matches!(lookup(&name), Err(FamilyRefusal::ArchitectureContested(_))),
            "phi3 is contested, so the by-name door refuses"
        );

        let tag = phi::tag_renderer().declaration();
        assert_eq!(tag.template, phi::TAG_TEMPLATE);
        assert_eq!(tag.generation_opener, phi::TAG_GENERATION_OPENER);

        let sep = phi::sep_renderer().declaration();
        assert_eq!(sep.template, phi::SEP_TEMPLATE);
        assert_eq!(sep.generation_opener, phi::SEP_GENERATION_OPENER);
    }

    #[test]
    fn qwen3_resolves_through_the_qwen2_module() {
        let declaration = lookup(&FamilyName("qwen3".into())).expect("qwen3 is carried");
        assert_eq!(
            declaration.template,
            qwen2::TEMPLATE,
            "the two keys share one module's template rather than a copy of it"
        );
    }

    /// The families that serve a conversation, **derived from `REGISTRY`
    /// rather than listed here**.
    ///
    /// A hand-written list makes the loop-roles test below assert only what
    /// its author remembered: a new registry row pointing at a module with no
    /// `System` arm would compile, pass, and fail on the operator's box, which
    /// is the failure that test exists to prevent. Deriving it means the row
    /// carries the family into the test by existing.
    ///
    /// Deduped on the renderer's function pointer, several rows selecting one
    /// module by different artifact names, and labelled by the first row that
    /// reaches each. The classifier needs no exclusion: `modernbert` renders
    /// no conversation and is in no registry row.
    fn conversational_families() -> Vec<(String, &'static dyn Family)> {
        let mut seen: Vec<*const dyn Family> = Vec::new();
        let mut out: Vec<(String, &'static dyn Family)> = Vec::new();
        for (at, row) in REGISTRY.iter().enumerate() {
            // **Identity is the trait object, not the function pointer.**
            // Identical-code folding can merge two `renderer()` bodies that
            // compile to the same instructions, which would silently drop a
            // family from this watch - the failure deriving from `REGISTRY`
            // exists to prevent, and one the length guard below cannot see.
            // `ptr::eq` on a wide pointer compares the data address and the
            // vtable both, so it is exact.
            let renderer: *const dyn Family = (row.renderer)();
            if seen.iter().any(|seen| std::ptr::eq(*seen, renderer)) {
                continue;
            }
            seen.push(renderer);
            // **Labelled with the row and not the family name alone**, two
            // rows declaring `llama` while reaching different renderers:
            // mistral artifacts carry `general.architecture = llama` and are
            // told apart by their markers. A failure reading `llama` against
            // a `[INST]` rendering sends a reader to the wrong module.
            out.push((
                format!("{} (registry row {at})", row.family),
                (row.renderer)(),
            ));
        }
        out
    }

    /// One text message, for the rendering tests below.
    fn said(role: Role, text: &str) -> Message {
        Message {
            role,
            content: vec![ContentBlock::Text {
                text: text.to_string(),
            }],
        }
    }

    /// **The fold carries a seated prefix into a template that names no
    /// system turn**, per the operator's ruling of 2026-08-28. Before it,
    /// `role: system` refused at `render_identity` on gemma4 and mistral3
    /// while `role: user` refused at the parse, which left those families no
    /// usable identity prefix at all.
    ///
    /// **The perturbation is the merge and no longer the refusal.** Until the
    /// delta path gained its `System` arm, dropping the fold made
    /// `render_each` answer `MalformedForFamily` and that was the watch. The
    /// arm made a bare `System` renderable, so the fold stopped being what
    /// prevents a refusal and became only what merges: one turn carrying both
    /// texts against two turns carrying one each. The old sentence stood
    /// while the property it named had moved, which is why a perturbation
    /// wants re-watching whenever an act makes a refusing path succeed.
    ///
    /// Perturbation: make the `(Role::User, Some(prefix))` arm push the two
    /// messages separately rather than joining them, and this fails on the
    /// merged text. Watched under exactly that change.
    ///
    /// **This test's watch is the helper's own and not the wiring's**, since
    /// it calls the fold directly and no `render_identity` sits in its path.
    /// The family test below is where replacing the call is watched. Writing
    /// one sentence for both was the same error this round corrected: a
    /// perturbation naming a path the test does not take.
    ///
    /// conforms: spu-system-folds-where-the-template-has-no-system-turn
    #[test]
    fn a_leading_system_message_folds_into_the_first_user_turn() {
        let folded = fold_system_into_first_user(&[
            said(Role::System, "You are Karl."),
            said(Role::User, "hello"),
        ])
        .expect("renderable");
        assert_eq!(folded.len(), 1, "two messages became one user turn");
        assert_eq!(folded[0].role, Role::User);
        assert_eq!(
            text_content(&folded[0]).expect("text"),
            "You are Karl.\n\nhello"
        );
    }

    /// **The two families that name no system turn now carry a seated
    /// prefix**, which is the case that broke: `role: user` refuses at the
    /// parse per `weaver-types-Spec` section 2, and `role: system` refused
    /// here, so a gemma or mistral agent had no usable prefix at all.
    ///
    /// **The assertion is the merged text and not merely a successful
    /// render**, the delta path's `System` arm having made an unfolded prefix
    /// render perfectly well as two turns. What the fold decides is whether
    /// the model reads one turn or two, so that is what is pinned: the two
    /// texts contiguous, which no unfolded rendering produces because the
    /// template's own turn markers sit between them.
    ///
    /// Perturbation: replace `render_each`'s `fold_for_template` call with the
    /// messages unchanged and this fails on the merged text. Watched under
    /// exactly that replacement. **The fold is no longer called from either
    /// `render_identity`**, having moved onto the trait so the delta path is
    /// covered too, so a perturbation naming that site would now change
    /// nothing.
    ///
    /// conforms: spu-system-folds-where-the-template-has-no-system-turn
    #[test]
    fn the_no_system_turn_families_render_a_seated_prefix() {
        let prefix = [
            said(Role::System, "You are Karl."),
            said(Role::User, "hello"),
        ];
        for (name, family) in [
            ("gemma4", &crate::family::gemma4::Gemma4 as &dyn Family),
            (
                "mistral3",
                &crate::family::mistral3::Mistral3 as &dyn Family,
            ),
        ] {
            let rendered = family
                .render_identity(&prefix)
                .unwrap_or_else(|e| panic!("{name} refused a seated prefix: {e:?}"));
            assert!(
                rendered.contains("You are Karl.\n\nhello"),
                "{name} rendered the prefix as separate turns rather than one: \
                 {rendered:?}"
            );
        }
    }

    /// **Every role the control loop emits renders on every family that
    /// serves a conversation.** This is the test whose absence let a live
    /// regression through: the loop's opening and re-entry are `System` and
    /// travel as *deltas*, so they reach `render_delta` and never reach
    /// `render_identity`'s fold. The fold covered the declaration's prefix,
    /// `render_delta` still refused, and every dev-loop turn on gemma and
    /// mistral failed while the prefix rendered perfectly.
    ///
    /// `cargo test --workspace` could not have caught it: `weaver-harness`
    /// does not depend on `weaver-spu`, so nothing compiles the loop's roles
    /// against the families that must render them. This test is that
    /// dependency written down from the SPU's side.
    ///
    /// Perturbation: return `MalformedForFamily` for `Role::System` in
    /// either family's delta path and this fails. Watched under exactly that
    /// change.
    ///
    /// conforms: spu-system-folds-where-the-template-has-no-system-turn
    #[test]
    fn every_conversational_family_renders_the_roles_a_loop_emits() {
        // What `dev_loop::contribution` puts in a delta: the opening and the
        // re-entry are System, the request is User, and the model answers.
        let emitted = [Role::System, Role::User, Role::Assistant];
        let families = conversational_families();
        // **Every registry row reaches this watch**, which is the property
        // the derivation exists to hold and is stronger than either a
        // vacuous-empty guard or a pinned count.
        //
        // `len() >= 2` passed a dedupe that silently dropped one of seven,
        // which is the failure deriving from `REGISTRY` was meant to prevent.
        // A pinned seven would catch that and make a new family edit a number
        // here, so the count would drift into the thing being asserted. This
        // asks the question directly: for each row, is the renderer it cites
        // present in the list.
        for row in REGISTRY {
            let cited: *const dyn Family = (row.renderer)();
            assert!(
                families
                    .iter()
                    .any(|(_, held)| std::ptr::eq(*held as *const dyn Family, cited)),
                "registry row {} cites a renderer no family in the watch holds",
                row.family
            );
        }
        assert!(
            !families.is_empty(),
            "the family list derives from REGISTRY and came back empty"
        );
        for (name, family) in families {
            for role in &emitted {
                let message = said(role.clone(), "x");
                family
                    .render_delta(&message)
                    .unwrap_or_else(|e| panic!("{name} refused {role:?} as a delta: {e:?}"));
            }
            // And the same roles as a seated prefix, which takes the fold.
            family
                .render_identity(&[said(Role::System, "framing"), said(Role::User, "ask")])
                .unwrap_or_else(|e| panic!("{name} refused a seated prefix: {e:?}"));
        }
    }

    /// **The loop's delta renders as one turn and never as two adjacent user
    /// turns.** This is the watch whose absence let a silent malformation
    /// through: `every_conversational_family_renders_the_roles_a_loop_emits`
    /// renders each role in isolation and so cannot see what a *sequence*
    /// becomes.
    ///
    /// `dev_loop::contribution` emits `[System(framing), User(request)]` on a
    /// first turn and after every flush. With the fold wired into
    /// `render_identity` only, that reached the decode seam unfolded and
    /// rendered as two user turns back to back - a shape both published
    /// templates avoid by merging, and one mistral's own template raises on.
    /// The previous round had made it worse rather than better: before the
    /// delta path carried `System` at all, this sequence refused loudly.
    ///
    /// Perturbation: drop the `fold_for_template` call from `render_each` and
    /// this fails on both folding families. Watched under exactly that
    /// removal.
    ///
    /// conforms: spu-system-folds-where-the-template-has-no-system-turn
    #[test]
    fn the_loops_delta_renders_as_one_turn_on_the_folding_families() {
        let delta = [said(Role::System, "FRAMING"), said(Role::User, "REQUEST")];
        for (name, family) in conversational_families() {
            let rendered = render_each(family, &delta)
                .unwrap_or_else(|e| panic!("{name} refused the loop's delta: {e:?}"));
            if family.fold_for_template(&delta).expect("folds") == delta.to_vec() {
                // A family that names a system turn renders one, and two
                // turns is the right answer there.
                continue;
            }
            assert!(
                rendered.contains("FRAMING\n\nREQUEST"),
                "{name} rendered the loop's delta as separate turns: {rendered:?}"
            );
        }
    }

    /// **What the fold does not close: the prefix and the delta are two
    /// render calls, and the turns they produce sit adjacent.**
    ///
    /// The seated identity renders at `Open` and the first delta at
    /// `AppendAndGenerate`, so each folds within itself and neither can reach
    /// the other. On the folding families that leaves two user turns back to
    /// back with no assistant between. This test asserts that rather than
    /// wishing it away, because an earlier form of the doc claimed the funnel
    /// covered "both paths, by construction" - true within a call and not
    /// across the boundary.
    ///
    /// **It is not something the fold can close.** The prefix is tokenized
    /// and resident before any delta exists, and seating it later would
    /// retire the ruling of 2026-08-20 that the prefix is processed at load
    /// as the functionality test. What the fold removed is the adjacency
    /// within each call: on the old `role: user` declarations this same
    /// exchange rendered three turns rather than two.
    #[test]
    fn the_prefix_and_the_delta_remain_two_turns_and_the_test_says_so() {
        let prefix = [said(Role::System, "ID")];
        let delta = [said(Role::System, "FRAMING"), said(Role::User, "REQUEST")];
        for (name, family) in conversational_families() {
            if family.fold_for_template(&delta).expect("folds") == delta.to_vec() {
                continue; // a family that names a system turn renders one
            }
            let whole = format!(
                "{}{}",
                family.render_identity(&prefix).expect("prefix renders"),
                render_each(family, &delta).expect("delta renders"),
            );
            // Each call folded within itself.
            assert!(
                whole.contains("FRAMING\n\nREQUEST"),
                "{name} did not fold the delta: {whole:?}"
            );
            // And the identity did not reach across into it, which is the
            // limit this test exists to record.
            assert!(
                !whole.contains("ID\n\nFRAMING"),
                "{name} folded across the prefix boundary, which would mean \
                 the prefix is no longer seated at open: {whole:?}"
            );
        }
    }

    /// **An unrenderable block refuses rather than being dropped.** The fold
    /// reads content through `text_content`, so a `ToolCall` inside a seated
    /// identity is `MalformedForFamily` as it was before the fold existed.
    /// Copying only the text blocks would open the session on a silently
    /// truncated prefix, the harness authoring a fault for the bad message
    /// but not aborting the load.
    #[test]
    fn a_block_the_family_cannot_render_refuses_rather_than_vanishing() {
        let bad = Message {
            role: Role::System,
            content: vec![ContentBlock::ToolCall(weaver_traits::ToolCall {
                name: "calculator".into(),
                arguments: "{}".into(),
            })],
        };
        assert!(
            fold_system_into_first_user(&[bad]).is_err(),
            "a block this family cannot render is refused, not skipped"
        );
    }

    /// A prefix with no user turn after it still reaches the model, there
    /// being nothing to fold into and a prefix rendering as nothing being a
    /// prefix the record cannot account for.
    #[test]
    fn a_system_message_alone_becomes_its_own_user_turn() {
        let folded = fold_system_into_first_user(&[said(Role::System, "You are Karl.")])
            .expect("renderable");
        assert_eq!(folded.len(), 1);
        assert_eq!(folded[0].role, Role::User);
        assert_eq!(text_content(&folded[0]).expect("text"), "You are Karl.");
    }

    /// Several system messages accumulate rather than the last winning, an
    /// operator who wrote two having meant both.
    #[test]
    fn consecutive_system_messages_accumulate() {
        let folded = fold_system_into_first_user(&[
            said(Role::System, "one"),
            said(Role::System, "two"),
            said(Role::User, "ask"),
        ])
        .expect("renderable");
        assert_eq!(folded.len(), 1);
        assert_eq!(text_content(&folded[0]).expect("text"), "one\n\ntwo\n\nask");
    }

    /// **A declaration that never used a system role renders as before**, so
    /// the fold is reachable only by the case it was added for.
    #[test]
    fn a_prefix_without_a_system_role_passes_through_untouched() {
        let original = vec![said(Role::User, "hello"), said(Role::Assistant, "hi")];
        let folded = fold_system_into_first_user(&original).expect("renderable");
        assert_eq!(folded, original);
    }

    /// **A family's wire role is the family's own, and gemma4's differs.**
    ///
    /// The floor's `Role::Assistant` renders as `model` here and as `assistant`
    /// under a family that calls the shared name kernel. Both halves are read,
    /// because the claim is a difference and a test of one side alone would
    /// pass under a shared map that had simply been renamed.
    ///
    /// Perturbation: give `Gemma4::render_delta` the shared
    /// [`common_role_name`] and the gemma4 half fails, the turn then opening
    /// `<|turn>assistant` against an artifact whose own template never writes
    /// that word.
    #[test]
    fn the_wire_role_is_the_familys_own() {
        let turn = [said(Role::Assistant, "hello")];

        let rendered = render_each(gemma4::renderer(), &turn).expect("gemma4 renders an assistant");
        assert!(
            rendered.starts_with("<|turn>model\n"),
            "gemma4 calls the assistant `model`, got {rendered:?}"
        );

        let rendered = render_each(qwen2::renderer(), &turn).expect("qwen2 renders an assistant");
        assert!(
            rendered.starts_with("<|im_start|>assistant\n"),
            "the canonical name is unchanged where the family shares it, got {rendered:?}"
        );
    }

    /// **A family's identity prefix carries its preamble once, and no delta
    /// carries it at all.**
    ///
    /// This is the distinction the surface exists for. The prefix opens a
    /// session and every later turn appends to it, so a preamble repeated per
    /// turn is a control token in the middle of a conversation, and a preamble
    /// missing from the prefix is a model reading its first turn without the
    /// token it was trained to start from.
    ///
    /// Both carried preambles are read, gemma4's `<bos>` and llama's
    /// `<|begin_of_text|>`, and the count is asserted rather than the presence:
    /// two messages through the prefix must still yield one.
    ///
    /// Perturbation: move the preamble into the family's `TEMPLATE` and this
    /// fails twice over - the prefix carries two and the delta carries one.
    /// Watched under exactly that move.
    #[test]
    fn the_preamble_belongs_to_the_prefix_and_appears_once() {
        let cases: [(&str, &'static dyn Family, &str); 2] = [
            ("gemma4", gemma4::renderer(), gemma4::BOS),
            ("llama", llama::renderer(), llama::TEXT_BEGIN),
        ];
        let turns = [said(Role::User, "one"), said(Role::Assistant, "two")];

        for (name, family, preamble) in cases {
            let prefix = family
                .render_identity(&turns)
                .unwrap_or_else(|_| panic!("{name} renders an identity prefix"));
            assert_eq!(
                prefix.matches(preamble).count(),
                1,
                "{name}: the prefix carries its preamble exactly once, got {prefix:?}"
            );
            assert!(
                prefix.starts_with(preamble),
                "{name}: and it opens with it, got {prefix:?}"
            );

            let delta = render_each(family, &turns[1..])
                .unwrap_or_else(|_| panic!("{name} renders a delta"));
            assert!(
                !delta.contains(preamble),
                "{name}: a delta appends to an open session and must not repeat it, got {delta:?}"
            );
        }
    }

    /// **The template renders in one pass, so neither field can be substituted
    /// into.** Successive `replace` calls protect only whichever field is
    /// substituted last: the first call's output is still in the string when
    /// the second one runs, so a placeholder carried by the earlier field is
    /// rewritten. A single pass has no second call to be caught by.
    ///
    /// The role case is the one that discriminates, and the message case is
    /// kept beside it to say why. Under
    /// `replace("{role}", role).replace("{message}", message)` a message
    /// carrying `{role}` comes back **intact**, the role pass having already
    /// run, so asserting only that would be a test the perturbation cannot
    /// fail. Watched: that implementation leaves the message case passing and
    /// fails the role case.
    #[test]
    fn neither_field_is_substituted_into() {
        // The discriminating half: the field substituted first carries the
        // other's placeholder.
        assert_eq!(
            render_template("[{role}]{message}", "weird{message}role", "BODY"),
            "[weird{message}role]BODY",
            "the role's placeholder is not filled by the message"
        );

        // The other direction, which successive replaces also survive. Kept so
        // a later reader does not mistake it for what buys the claim.
        assert_eq!(
            render_template("[{role}]{message}", "assistant", "the user typed {role}"),
            "[assistant]the user typed {role}"
        );
    }

    /// A brace that opens no placeholder renders unharmed, which is the branch
    /// the walk needs so a template carrying JSON is not silently eaten.
    #[test]
    fn a_literal_brace_survives_the_render() {
        assert_eq!(
            render_template("{{\"a\":1}} {role}", "user", "ignored"),
            "{{\"a\":1}} user"
        );
    }

    /// A call the parser **can** recover comes back as a call, which is what
    /// makes the test above a distinction rather than a parser that reports
    /// everything as unrecoverable.
    #[test]
    fn a_recoverable_call_comes_back_whole() {
        let emission = format!(
            "{}{{\"name\":\"read_file\",\"arguments\":{{\"path\":\"/tmp/x\"}}}}{}",
            qwen2::CALL_OPEN,
            qwen2::CALL_CLOSE
        );
        let parsed = qwen2::Qwen2.parse(&emission);
        assert!(!parsed.has_unrecovered_call(), "the name came back");
        assert!(matches!(
            parsed.content.as_slice(),
            [Content::Call { name, .. }] if name.0 == "read_file"
        ));
    }
}
