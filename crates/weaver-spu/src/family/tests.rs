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
            .unwrap_or_else(|| panic!("{} is a spelling this table does not record", row.family));
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
/// Perturbation: in `scan`, replace the refused fragment's push of
/// `Content::Unrecovered` with `Content::Text(written.to_string())` and this
/// test fails, because the fragment then arrives as ordinary assistant text.
/// Watched under exactly that collapse (#31).
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
        let declaration =
            lookup(&FamilyName(family.into())).unwrap_or_else(|_| panic!("{family} is carried"));
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
    let folded =
        fold_system_into_first_user(&[said(Role::System, "You are Karl.")]).expect("renderable");
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

        let delta =
            render_each(family, &turns[1..]).unwrap_or_else(|_| panic!("{name} renders a delta"));
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

/// **An unclosed call reports `unclosed`, its fragment the span as written**,
/// per Spec section 5 on the operator's ruling of 2026-10-13 (#31). The
/// emission is #31's own shape: a second opening marker where the closing one
/// belongs, then the end. The second opener ends the first call where it
/// stands, so the record carries two attempts: the model's fragment and a bare
/// opener, both `unclosed`, and nothing is lost.
///
/// Perturbation: skip the kernel's closing-marker check, so the family is
/// asked, and the first reason reads `unparsed`.
#[test]
fn an_unclosed_call_reports_unclosed_with_the_span_as_written() {
    let first = format!("{}{{\"name\":\"a\"", qwen2::CALL_OPEN);
    let emission = format!("ok {first}{}", qwen2::CALL_OPEN);
    let parsed = qwen2::Qwen2.parse(&emission);
    let entries: Vec<(&str, &Reason)> = parsed
        .unrecovered
        .iter()
        .map(|u| (u.fragment.as_str(), &u.reason))
        .collect();
    assert_eq!(
        entries,
        vec![
            (first.as_str(), &Reason::Unclosed),
            (qwen2::CALL_OPEN, &Reason::Unclosed),
        ]
    );
    assert_eq!(
        parsed.content,
        vec![
            Content::Text("ok ".into()),
            Content::Unrecovered {
                fragment: first.clone()
            },
            Content::Unrecovered {
                fragment: qwen2::CALL_OPEN.into()
            },
        ]
    );
    assert_eq!(parsed.text(), "ok ");
}

/// **A re-opened marker ends the open call, and a well-formed call after it
/// still recovers**, per Spec section 5 on the operator's ruling of
/// 2026-10-13: a second opening marker met before any closer ends the first
/// call as `unclosed` there and scanning resumes at the second marker.
///
/// Perturbation: search the closer across the whole remainder, as the kernel
/// did before the ruling, and the two calls read as one `unparsed` fragment
/// with the well-formed call lost.
#[test]
fn a_re_opened_marker_ends_the_open_call_and_the_next_recovers() {
    let broken = format!("{}{{\"name\":\"a\"", qwen2::CALL_OPEN);
    let good = format!(
        "{}{{\"name\":\"read_file\",\"arguments\":{{}}}}{}",
        qwen2::CALL_OPEN,
        qwen2::CALL_CLOSE
    );
    let parsed = qwen2::Qwen2.parse(&format!("{broken}{good}"));
    assert_eq!(parsed.unrecovered.len(), 1, "{:?}", parsed.unrecovered);
    assert_eq!(parsed.unrecovered[0].fragment, broken);
    assert_eq!(parsed.unrecovered[0].reason, Reason::Unclosed);
    assert!(
        matches!(&parsed.content[..], [Content::Unrecovered { .. }, Content::Call { name, .. }] if name.0 == "read_file"),
        "{:?}",
        parsed.content
    );
}

/// **Each family's grammar names a family the registry carries**, so the name
/// written in `markers()` cannot drift from the registry's spelling. Read
/// through the parse, the grammar being where the name reaches a reader, and
/// against the registry rather than through `declaration()`, which panics for
/// llama, whose architecture the registry marks contested (reported on #31's
/// code act, not this test's to settle).
///
/// Perturbation: misspell a family's name in its `markers()` and this fails.
#[test]
fn each_familys_grammar_names_its_registry_row() {
    let families: Vec<(Box<dyn Family>, &str)> = vec![
        (Box::new(qwen2::Qwen2), qwen2::CALL_OPEN),
        (Box::new(llama::Llama), llama::CALL_OPEN),
        (Box::new(gemma4::Gemma4), gemma4::CALL_OPEN),
        (Box::new(mistral3::Mistral3), mistral3::CALL_OPEN),
        (Box::new(gpt_oss::GptOss), gpt_oss::CALL_OPEN),
    ];
    for (family, open) in families {
        let parsed = family.parse(&format!("{open}garbage"));
        let grammar = parsed
            .unrecovered
            .first()
            .expect("an attempted call")
            .grammar;
        assert!(
            REGISTRY.iter().any(|row| row.family == grammar.family),
            "{grammar:?} names no registry row"
        );
        assert_eq!(grammar.open, open);
    }
}

/// **An unparsed call carries the family's detail and the grammar that
/// judged it**, per Spec section 5: the fragment with both markers, a
/// non-empty detail, and the family's name and markers.
///
/// Perturbation: drop the family's error text from `recover` and the
/// detail arrives empty.
#[test]
fn an_unparsed_call_carries_the_familys_detail_and_grammar() {
    let emission = format!("{}not json{}", qwen2::CALL_OPEN, qwen2::CALL_CLOSE);
    let parsed = qwen2::Qwen2.parse(&emission);
    let entry = &parsed.unrecovered[0];
    assert_eq!(entry.fragment, emission);
    assert!(
        matches!(&entry.reason, Reason::Unparsed(detail) if !detail.is_empty()),
        "{:?}",
        entry.reason
    );
    assert_eq!(entry.grammar.family, "qwen2");
    assert_eq!(entry.grammar.open, qwen2::CALL_OPEN);
    assert_eq!(entry.grammar.close, Some(qwen2::CALL_CLOSE));
}

/// **Two unrecovered calls report in emission order, block and entry alike.**
#[test]
fn two_unrecovered_calls_report_in_emission_order() {
    let first = format!("{}x{}", qwen2::CALL_OPEN, qwen2::CALL_CLOSE);
    let second = format!("{}y{}", qwen2::CALL_OPEN, qwen2::CALL_CLOSE);
    let parsed = qwen2::Qwen2.parse(&format!("{first}mid{second}"));
    let reported: Vec<&str> = parsed
        .unrecovered
        .iter()
        .map(|u| u.fragment.as_str())
        .collect();
    assert_eq!(reported, vec![first.as_str(), second.as_str()]);
    let blocks: Vec<&str> = parsed
        .content
        .iter()
        .filter_map(|c| match c {
            Content::Unrecovered { fragment } => Some(fragment.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(blocks, reported);
}

/// **A family that closes nothing runs the fragment to the end of the
/// emission and names no closing marker**, per Spec section 5.
#[test]
fn a_close_less_family_runs_the_fragment_to_the_end() {
    let fragment = format!("{}not a call", mistral3::CALL_OPEN);
    let parsed = mistral3::Mistral3.parse(&format!("before {fragment}"));
    assert_eq!(parsed.unrecovered.len(), 1);
    assert_eq!(parsed.unrecovered[0].fragment, fragment);
    assert_eq!(parsed.unrecovered[0].grammar.family, "mistral3");
    assert_eq!(parsed.unrecovered[0].grammar.close, None);
    assert!(matches!(parsed.unrecovered[0].reason, Reason::Unparsed(_)));
}

/// **An `UnrecoveredCall` block renders back as written**, per Spec section
/// 5, in qwen2's own renderer and through `text_content`, and a user message
/// carrying one refuses.
///
/// Perturbation: render the fragment with its markers stripped and the
/// rendered bytes no longer hold it.
#[test]
fn an_unrecovered_call_block_renders_back_as_written() {
    let fragment = format!("{}{{\"name\":\"a\"{}", qwen2::CALL_OPEN, qwen2::CALL_OPEN);
    let assistant = weaver_traits::Message {
        role: Role::Assistant,
        content: vec![
            ContentBlock::Text { text: "ok ".into() },
            ContentBlock::UnrecoveredCall {
                fragment: fragment.clone(),
            },
        ],
    };
    let rendered = qwen2::Qwen2.render_delta(&assistant).expect("renders");
    assert!(rendered.contains(&format!("ok {fragment}")), "{rendered}");
    let user = weaver_traits::Message {
        role: Role::User,
        content: assistant.content.clone(),
    };
    assert!(qwen2::Qwen2.render_delta(&user).is_err());

    let shared = text_content(&assistant).expect("the shared kernel renders it");
    assert_eq!(shared, format!("ok {fragment}"));
    assert!(text_content(&user).is_err());
}

/// **A closing marker that is also the family's stop is met at the end of
/// the emission**, per Spec section 5: the stop token is drawn but not
/// retained, so a well-formed llama or gpt-oss call reaches the parse without
/// its closer, and the closer came all the same. Such a call recovers, as it
/// did before #31, and is never judged `unclosed`. qwen2, whose closer is no
/// stop, still reports a missing closer as `unclosed`.
///
/// Perturbation: drop the stop-condition check from the kernel's unclosed
/// judgement and both live-shaped calls report `unclosed` instead of
/// recovering.
#[test]
fn a_closer_that_is_a_stop_is_met_at_the_end_of_the_emission() {
    let llama_live = format!(
        "{}{{\"name\":\"calculator\",\"parameters\":{{\"expression\":\"1+1\"}}}}",
        llama::CALL_OPEN
    );
    let parsed = llama::Llama.parse(&llama_live);
    assert!(parsed.unrecovered.is_empty(), "{:?}", parsed.unrecovered);
    assert!(
        matches!(&parsed.content[..], [Content::Call { name, .. }] if name.0 == "calculator"),
        "{:?}",
        parsed.content
    );

    let gpt_live = format!(
        "{}to=functions.calculator<|message|>{{\"expression\":\"1+1\"}}",
        gpt_oss::CALL_OPEN
    );
    let parsed = gpt_oss::GptOss.parse(&gpt_live);
    assert!(parsed.unrecovered.is_empty(), "{:?}", parsed.unrecovered);
    assert!(
        matches!(&parsed.content[..], [Content::Call { name, .. }] if name.0 == "calculator"),
        "{:?}",
        parsed.content
    );

    let qwen_open = format!("{}{{\"name\":\"a\"}}", qwen2::CALL_OPEN);
    assert_eq!(
        qwen2::Qwen2.parse(&qwen_open).unrecovered[0].reason,
        Reason::Unclosed
    );
}
