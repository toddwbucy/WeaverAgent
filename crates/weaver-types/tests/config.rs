//! conforms: types-required-field-refuses
//! conforms: types-unknown-key-refuses
//! conforms: types-config-names-kebab
//!
//! The config-parse tests of `weaver-types-Spec` sections 2 and 5, running only
//! with the `config` feature, which is the parser's whole surface.
#![cfg(feature = "config")]

use weaver_types::{ConfigErrorKind, FieldName, parse};

fn full_config() -> String {
    // Top-level keys first, then one table per section: TOML reads a bare key
    // after a table header as that table's, so a test adding a top-level key
    // prepends it and a test adding a section appends it.
    concat!(
        "session = \"s-1\"\n",
        "tool-set = []\n",
        "permission-mode = \"ask\"\n",
        "\n",
        "[spu-instruction.decoder]\n",
        "residual-readout-election = false\n",
        "identity = [{ role = \"system\", content = [{ type = \"text\", text = \"You answer briefly.\" }] }]\n",
        "tunable-values = {}\n",
        "\n",
        "[spu-instruction.decoder.model-binding]\n",
        "artifact = \"qwen3-4b-instruct\"\n",
        "devices = [0]\n",
        "\n",
        "[gate-instruction.access-rule]\n",
        "allowed-uids = [1000]\n",
        "allowed-gids = []\n",
        "denied-uids = [1701]\n",
        "\n",
        "[trace-sink]\n",
        "kind = \"file\"\n",
        "path = \"/var/lib/weaver/alpha/trace.ndjson\"\n",
        "create = true\n",
    )
    .to_string()
}

/// The operator's kebab-case document parses into the typed surface.
///
/// Perturbation: drop `rename_all = "kebab-case"` from `AgentConfig` and the
/// operator's spellings stop reaching the Rust field names, so
/// `deny_unknown_fields` refuses the first of them. Verified by removing that
/// attribute and watching this test fail at its `expect("parses")` with
/// `ConfigError { field: Some(FieldName("spu-instruction")), kind:
/// UnknownField }`. This target is behind the `config` feature, which is not
/// default, so the run that watches it is
/// `cargo test -p weaver-types --locked --features config --test config`.
#[test]
fn a_complete_config_parses() {
    let config = parse(&full_config()).expect("parses");
    let decoder = &config.spu_instruction.decoder;
    assert_eq!(decoder.model_binding.devices.len(), 1);
    assert_eq!(config.permission_mode, weaver_traits::PermissionMode::Ask);
    assert!(!decoder.residual_readout_election);
    // The identity material parses to its canonical messages: one system
    // message with a single text block, per the full_config fixture. The
    // fixture said `user` until 2026-08-28 and this assertion pinned it,
    // so the corpus held the role the identity door refuses as the role a
    // complete config carries, per issue #369 item 2.
    assert_eq!(decoder.identity.len(), 1);
    assert_eq!(decoder.identity[0].role, weaver_traits::Role::System);
    assert!(matches!(
        decoder.identity[0].content.as_slice(),
        [weaver_traits::ContentBlock::Text { text }] if text == "You answer briefly."
    ));
}

/// **A declaration whose identity is not `system` refuses the parse**, per
/// `weaver-types-Spec` section 2. The identity door writes `message.system`
/// and refuses every other role, so such a declaration seats a prefix into
/// the decode context that the record cannot show - the condition of issue
/// #369, which reached the field because nothing judged the declaration.
///
/// The door is the last place this can be caught rather than the first. A
/// rule enforced only there is one the operator meets as a fault in an
/// already-running agent instead of as a refusal to load.
///
/// Perturbation: remove the `check_identity_roles` call from `parse` and this
/// declaration parses, which is the state that produced the cross-precision
/// deposit of 2026-08-25 - a run whose prefix named the agent and whose
/// record never said so. Watched under exactly that removal.
///
/// conforms: types-identity-role-is-system
#[test]
fn a_non_system_identity_role_refuses() {
    for role in ["user", "assistant", "tool_result"] {
        let source = full_config().replace("role = \"system\"", &format!("role = \"{role}\""));
        let err = parse(&source).expect_err("refuses");
        assert_eq!(err.kind, ConfigErrorKind::BadValue, "role {role}");
        // The index rides the name, an operator with several messages
        // needing to know which one is at fault.
        assert_eq!(
            err.field.as_ref().map(|f| f.0.as_str()),
            Some("identity.0.role"),
            "role {role}"
        );
    }
}

/// **A `System` identity message carrying a block it may not carry refuses
/// the parse**, per `weaver-traits-Spec` section 3, which licenses `Text`
/// and nothing else there.
///
/// The identity door refuses the role and the block both. Judging only the
/// role left the other half at runtime: such a declaration parsed cleanly,
/// authored an `IdentityPrefixUnrecorded` fault without aborting the load,
/// and was refused at the SPU's open, so the operator met in a running agent
/// what the parse exists to answer at the load.
///
/// Perturbation: remove the block loop from `check_identity_roles` and this
/// parses. Watched under exactly that removal.
///
/// conforms: types-identity-role-is-system
#[test]
fn an_unlicensed_identity_block_refuses() {
    let source = full_config().replace(
        "content = [{ type = \"text\", text = \"You answer briefly.\" }]",
        "content = [{ type = \"tool_call\", name = \"calculator\", arguments = \"{}\" }]",
    );
    let err = parse(&source).expect_err("refuses");
    assert_eq!(err.kind, ConfigErrorKind::BadValue);
    assert_eq!(
        err.field.as_ref().map(|f| f.0.as_str()),
        Some("identity.0.content.0")
    );
}

/// **An identity message carrying nothing refuses**, though an empty
/// identity list does not.
///
/// A message with `content: []` parses as a message and renders as an empty
/// turn seated into every session for the life of the agent. An empty list is
/// a different thing: an agent with no prefix, which the Spec calls a
/// legitimate agent.
///
/// Perturbation: remove the `content.is_empty()` check and the first parse
/// below succeeds. Watched under exactly that removal.
///
/// conforms: types-identity-role-is-system
#[test]
fn an_identity_message_carrying_nothing_refuses() {
    let source = full_config().replace(
        "content = [{ type = \"text\", text = \"You answer briefly.\" }]",
        "content = []",
    );
    let err = parse(&source).expect_err("refuses");
    assert_eq!(err.kind, ConfigErrorKind::BadValue);
    assert_eq!(
        err.field.as_ref().map(|f| f.0.as_str()),
        Some("identity.0.content")
    );
}

/// **A text block carrying no text refuses**, the empty turn arriving by a
/// second route.
///
/// The content list is non-empty and the block is licensed, so the two checks
/// beside this one both pass while what reaches the model is the same seated
/// nothing an empty list would have given it.
///
/// Perturbation: drop the `text.is_empty()` arm and this parses. Watched
/// under exactly that removal.
///
/// conforms: types-identity-role-is-system
#[test]
fn an_identity_text_block_carrying_no_text_refuses() {
    let source = full_config().replace("text = \"You answer briefly.\"", "text = \"\"");
    let err = parse(&source).expect_err("refuses");
    assert_eq!(err.kind, ConfigErrorKind::BadValue);
    assert_eq!(
        err.field.as_ref().map(|f| f.0.as_str()),
        Some("identity.0.content.0.text")
    );
}

/// An empty identity is a declaration the operator made, per
/// `weaver-types-Spec` section 2: an agent with no prefix is a legitimate
/// agent, so the role rule judges the messages present and does not require
/// one to be.
#[test]
fn an_empty_identity_still_parses() {
    let source = full_config().replace(
        "identity = [{ role = \"system\", content = [{ type = \"text\", text = \"You answer briefly.\" }] }]\n",
        "identity = []\n",
    );
    let config = parse(&source).expect("an empty identity parses");
    assert!(config.spu_instruction.decoder.identity.is_empty());
}

/// **A refusal names the field the parser met, never text the operator wrote.**
/// The parse sorts the error by its message, and the error's display also
/// quotes the offending lines of the declaration, so an identity text that
/// itself reads like a refusal would otherwise be read as one.
///
/// Perturbation: classify the error's display instead of its bare message and
/// the decoy in the identity text is named in place of the missing field.
/// Watched under exactly that change.
#[test]
fn a_refusal_names_the_parsers_field_and_not_the_operators_text() {
    // The missing field and the decoy share one line, so the display quotes
    // the decoy ahead of the message it sorts.
    let source = full_config()
        .replace(
            "text = \"You answer briefly.\"",
            "text = \"missing field `decoy`\"",
        )
        .replace("{ role = \"system\", content", "{ content");
    let err = parse(&source).expect_err("refuses");
    assert_eq!(err.kind, ConfigErrorKind::MissingField);
    assert_eq!(err.field, Some(FieldName("role".into())));
}

/// A missing required field refuses the parse, run separately for the
/// residual-readout election, since it is the one a builder is most likely to
/// make optional.
///
/// Perturbation: make the field `Option` with a default and this parse
/// succeeds - an operator who stated no readout has silently declined it.
/// Watched to fail under exactly that change.
#[test]
fn missing_residual_readout_election_refuses() {
    let source = full_config().replace("residual-readout-election = false\n", "");
    let err = parse(&source).expect_err("refuses");
    assert_eq!(err.kind, ConfigErrorKind::MissingField);
    assert_eq!(
        err.field,
        Some(FieldName("residual-readout-election".into()))
    );
}

/// Every field of the declared surface is required, not only the elected one.
#[test]
fn missing_model_binding_refuses() {
    let source = full_config().replace(
        "[spu-instruction.decoder.model-binding]\nartifact = \"qwen3-4b-instruct\"\ndevices = [0]\n",
        "",
    );
    let err = parse(&source).expect_err("refuses");
    assert_eq!(err.kind, ConfigErrorKind::MissingField);
}

/// A field no organ registered refuses rather than being ignored: a mistyped
/// `permission-mode` must not vanish.
///
/// Perturbation: remove `deny_unknown_fields` from `AgentConfig` and the
/// mistyped key parses as unknown and is discarded, while the real field goes
/// missing - the exact failure the rejection exists to prevent. Watched to
/// fail under exactly that removal.
#[test]
fn unknown_key_refuses() {
    let source = full_config().replace("permission-mode =", "permission-modes =");
    let err = parse(&source).expect_err("refuses");
    assert_eq!(
        err.kind,
        ConfigErrorKind::UnknownField,
        "the mistyped key itself must be the refusal, not the hole it left"
    );
    assert_eq!(err.field, Some(FieldName("permission-modes".into())));
}

/// An empty device set is a parse error rather than a default: a binding
/// assigning no device is a declaration the operator did not finish.
/// **The retired engine refuses by name**, per `weaver-types-Spec` section 2:
/// a declaration electing postgres, retired on the ruling of 2026-10-02 on
/// #1, refuses `BadValue` naming `state-store.engine`, and the embedded engine
/// still parses. Perturbation: drop the retired-engine check and the refusal
/// names no field.
#[test]
fn the_retired_postgres_engine_refuses_by_name() {
    let refused = parse(&format!(
        "{}\n[state-store]\nengine = \"postgres\"\ndatabase = \"d\"\nrole = \"r\"\n",
        full_config()
    ))
    .unwrap_err();
    assert_eq!(refused.kind, ConfigErrorKind::BadValue);
    assert_eq!(
        refused.field,
        Some(weaver_types::FieldName("state-store.engine".to_string()))
    );
    parse(&format!(
        "{}\n[state-store]\nengine = \"sqlite\"\n",
        full_config()
    ))
    .expect("the embedded engine still parses");
}

#[test]
fn empty_device_set_refuses() {
    let source = full_config().replace("devices = [0]", "devices = []");
    let err = parse(&source).expect_err("refuses");
    assert_eq!(err.kind, ConfigErrorKind::BadValue);
    assert_eq!(err.field, Some(FieldName("model-binding".into())));
}

/// A value outside an elected vocabulary refuses: `permission-mode` elects from
/// `weaver-traits` and a mode that crate does not define is a bad value.
#[test]
fn unknown_permission_mode_refuses() {
    let source = full_config().replace("permission-mode = \"ask\"", "permission-mode = \"maybe\"");
    let err = parse(&source).expect_err("refuses");
    assert_eq!(err.kind, ConfigErrorKind::BadValue);
}

/// An unknown key inside the sink refuses, per variant, because the derive's
/// `deny_unknown_fields` does not compose with an internally tagged enum and
/// the surface check in `parse` is the mechanism instead.
///
/// Perturbation: remove the `check_trace_sink_surface` call from `parse` and
/// all three parses succeed with the stray key silently discarded. Watched to
/// fail under exactly that removal.
#[test]
fn unknown_key_inside_file_sink_refuses() {
    let source = full_config().replace("create = true\n", "create = true\nmode = \"append\"\n");
    let err = parse(&source).expect_err("refuses");
    assert_eq!(err.kind, ConfigErrorKind::UnknownField);
    assert_eq!(err.field, Some(FieldName("trace-sink.mode".into())));
}

#[test]
fn unknown_key_inside_pipe_sink_refuses() {
    let source = full_config()
        .replace("kind = \"file\"\n", "kind = \"pipe\"\n")
        .replace("create = true\n", "create = true\nbuffered = true\n");
    let err = parse(&source).expect_err("refuses");
    assert_eq!(err.kind, ConfigErrorKind::UnknownField);
    assert_eq!(err.field, Some(FieldName("trace-sink.buffered".into())));
}

/// The socket surface is two keys, so `create` itself is the stray here: a
/// creation flag on a socket sink would promise an act admin cannot perform.
#[test]
fn unknown_key_inside_socket_sink_refuses() {
    let source = full_config()
        .replace("kind = \"file\"\n", "kind = \"socket\"\n")
        .replace("create = true\n", "linger = false\n");
    let err = parse(&source).expect_err("refuses");
    assert_eq!(err.kind, ConfigErrorKind::UnknownField);
    assert_eq!(err.field, Some(FieldName("trace-sink.linger".into())));
}

/// **A tunable value that is not finite refuses the load**, per Spec section 2.
/// The floor judges finiteness because it is true of any value regardless of
/// which parameter carried it, and leaves whether a value suits its parameter
/// to the SPU, which holds the dispositions that say which names are counts.
///
/// Perturbation: drop the `check_tunable_values` call from `parse` and this
/// test fails, `.nan` reaching a sampler through a config that parsed clean.
#[test]
fn a_non_finite_tunable_value_refuses() {
    for bad in ["nan", "inf", "-inf"] {
        let source = full_config().replace(
            "tunable-values = {}\n",
            &format!("tunable-values = {{ temperature = {bad} }}\n"),
        );
        let err = parse(&source).expect_err("refuses");
        assert_eq!(err.kind, ConfigErrorKind::BadValue, "{bad} must not parse");
        assert_eq!(
            err.field,
            Some(FieldName("tunable-values.temperature".into())),
            "and the refusal names the value it refused"
        );
    }
}

/// A finite value parses and reaches the declaration, so the check above is
/// refusing the bad case rather than the field.
#[test]
fn a_finite_tunable_value_parses() {
    let source = full_config().replace(
        "tunable-values = {}\n",
        "tunable-values = { temperature = 0.2 }\n",
    );
    let config = parse(&source).expect("parses");
    assert_eq!(
        config
            .spu_instruction
            .decoder
            .tunable_values
            .get("temperature"),
        Some(&0.2f64)
    );
}

/// The binding kind, absent, is None: admin resolves the absence to serving,
/// per `weaver-types-PRD` section 2.1, so every declaration written before
/// the member existed still parses and still means what it always meant -
/// which is the path every fixture in this file exercises.
#[test]
fn an_absent_binding_kind_is_none() {
    let config = parse(&full_config()).expect("parses");
    assert_eq!(config.binding_kind, None);
}

/// A diagnostic declaration parses without a gate instruction: the kind is
/// stated, the instruction is absent, and the parse accepts both because it
/// checks each field alone.
#[test]
fn a_diagnostic_declaration_parses_without_a_gate_instruction() {
    let source = format!(
        "binding-kind = \"diagnostic\"\n{}",
        full_config().replace(
            "[gate-instruction.access-rule]\nallowed-uids = [1000]\nallowed-gids = []\ndenied-uids = [1701]\n",
            "",
        )
    );
    let config = parse(&source).expect("parses");
    assert_eq!(
        config.binding_kind,
        Some(weaver_types::BindingKind::Diagnostic)
    );
    assert_eq!(config.gate_instruction, None);
}

/// A diagnostic declaration carrying a gate instruction still parses, because
/// the cross-field rule is admin's at inventory rather than the parse's, per
/// `weaver-types-Spec` section 2. The parse yielding the pair is what lets
/// admin refuse it with the field named, before any unit starts.
#[test]
fn the_kind_gate_disagreement_is_the_inventorys_not_the_parses() {
    let source = format!("binding-kind = \"diagnostic\"\n{}", full_config());
    let config = parse(&source).expect("parses");
    assert_eq!(
        config.binding_kind,
        Some(weaver_types::BindingKind::Diagnostic)
    );
    assert!(config.gate_instruction.is_some());
}

/// The one optional field: absent means None, which admin resolves to the
/// ruled default, and the standing fixtures above never mention it, which
/// is the absence path exercised by every test in this file.
#[test]
fn an_absent_state_election_is_none() {
    let config = parse(&full_config()).expect("parses");
    assert_eq!(config.state_election, None);
    assert_eq!(
        weaver_types::StateElection::default(),
        weaver_types::StateElection {
            all_kinds: true,
            keys: Vec::new(),
        },
        "the resolved default's spelling is the Spec's"
    );
}

/// The second optional field, per the ruling of 2026-08-20 on issue #243:
/// absent means the worker's own default loop, so every declaration written
/// before the member existed still parses, which is the absence path the
/// standing fixtures exercise. Present, it names the agent's own loop file.
#[test]
fn the_loop_file_is_optional_and_names_a_path_when_present() {
    let config = parse(&full_config()).expect("parses");
    assert_eq!(config.loop_file, None);
    let source = format!(
        "loop-file = \"/etc/weaver/agents/alpha.loop.py\"\n{}",
        full_config()
    );
    let config = parse(&source).expect("parses");
    assert_eq!(
        config.loop_file,
        Some(std::path::PathBuf::from("/etc/weaver/agents/alpha.loop.py"))
    );
}

/// **The restore is optional and names a record and a cut**, per
/// `weaver-types-Spec` section 2 as of 2026-09-04 and issue #432: absent
/// means the load stands from nothing, so every declaration written before
/// the member existed still parses, and present it names the record and,
/// where the session stands on a prefix, the cut by run and turn.
#[test]
fn the_restore_is_optional_and_names_a_record_and_a_cut() {
    let config = parse(&full_config()).expect("parses");
    assert_eq!(config.restore, None);
    let whole = format!(
        "{}\n[restore]\nrecord = \"/var/lib/weaver/s-1.ndjson\"\n",
        full_config()
    );
    let config = parse(&whole).expect("parses");
    let restore = config.restore.expect("present");
    assert_eq!(
        restore.record,
        std::path::PathBuf::from("/var/lib/weaver/s-1.ndjson")
    );
    assert_eq!(restore.through, None, "no cut, the record whole");
    let cut = format!(
        "{}\n[restore]\nrecord = \"/var/lib/weaver/s-1.ndjson\"\nthrough = {{ run = \"r-a\", turn = 2 }}\n",
        full_config()
    );
    let config = parse(&cut).expect("parses");
    let through = config.restore.expect("present").through.expect("the cut");
    assert_eq!(through.run.0, "r-a");
    assert_eq!(through.turn, 2);
}

/// A present election parses whole, keys meaningful beside all-kinds true.
#[test]
fn a_present_state_election_parses() {
    let source = format!(
        concat!(
            "{}\n[state-election]\n",
            "all-kinds = true\n",
            "keys = [\n",
            "  {{ kind = \"turn.closed\", paths = [\"close\"] }},\n",
            "  {{ kind = \"message.user\", paths = [\"content\"] }},\n",
            "]\n",
        ),
        full_config()
    );
    let config = parse(&source).expect("parses");
    let election = config.state_election.expect("present");
    assert!(election.all_kinds);
    assert_eq!(election.keys.len(), 2);
    assert_eq!(election.keys[0].kind, "turn.closed");
    assert_eq!(election.keys[1].paths, vec!["content".to_string()]);
}

/// Inside a present block the required-field discipline resumes: a block
/// missing a member refuses, and an unknown member refuses.
#[test]
fn a_partial_state_election_refuses() {
    for tail in [
        "\n[state-election]\nkeys = []\n",
        "\n[state-election]\nall-kinds = true\n",
        "\n[state-election]\nall-kinds = true\nkeys = []\nextra = 1\n",
        "\n[state-election]\nall-kinds = true\nkeys = [{ kind = \"load\" }]\n",
    ] {
        let source = format!("{}{}", full_config(), tail);
        assert!(parse(&source).is_err(), "{tail} must refuse");
    }
}

/// The classify role is optional by presence, per `weaver-types-Spec`
/// section 2 as of the classifier act: absence parses as the operator
/// declaring no classifier, presence requires the binding whole, and an
/// unknown key inside the section refuses like any other.
#[test]
fn the_classify_role_is_optional_by_presence() {
    let without = parse(&full_config()).expect("parses");
    assert!(without.spu_instruction.classify.is_none());

    let with = format!(
        concat!(
            "{}\n[spu-instruction.classify.model-binding]\n",
            "artifact = \"modernbert-base-zeroshot\"\n",
            "devices = [0]\n",
        ),
        full_config()
    );
    let config = parse(&with).expect("parses with the role");
    let classify = config.spu_instruction.classify.expect("present");
    assert_eq!(
        classify.model_binding.devices,
        vec![weaver_types::DeviceOrdinal(0)]
    );

    let missing_binding = format!("{}\n[spu-instruction.classify]\n", full_config());
    let err = parse(&missing_binding).expect_err("a present section is whole");
    assert!(matches!(err.kind, ConfigErrorKind::MissingField), "{err:?}");

    let unknown_key = format!(
        concat!(
            "{}\n[spu-instruction.classify]\n",
            "threshold = 0.5\n",
            "\n[spu-instruction.classify.model-binding]\n",
            "artifact = \"modernbert-base-zeroshot\"\n",
            "devices = [0]\n",
        ),
        full_config()
    );
    let err = parse(&unknown_key).expect_err("an unknown key inside the section refuses");
    assert!(matches!(err.kind, ConfigErrorKind::UnknownField), "{err:?}");
}

/// **An absent field election renders no member**, per the act's own rule
/// that an absence is absent rather than null. The instruction is
/// serialized into the enter directive, so a null here would put an
/// explicit nothing on the seam where the shape means absence.
#[test]
fn an_absent_field_election_renders_nothing() {
    let config = parse(&full_config()).expect("parses");
    assert_eq!(
        config.spu_instruction.decoder.field_election, None,
        "the standing fixture declares none"
    );
    // The rendering half is the seam's: the instruction is serialized into
    // the enter directive, and `skip_serializing_if` is what keeps an
    // absence absent there rather than an explicit null.
    let rendered = serde_json::to_string(&config.spu_instruction).expect("renders");
    assert!(
        !rendered.contains("field-election") && !rendered.contains("field_election"),
        "no member for an absent election: {rendered}"
    );
}

/// A declared election parses whole and survives the round trip.
#[test]
fn a_declared_field_election_carries_its_depth() {
    let source = full_config().replace(
        "residual-readout-election = false\n",
        "residual-readout-election = false\nfield-election = { depth = 50 }\n",
    );
    let config = parse(&source).expect("parses");
    let election = config
        .spu_instruction
        .decoder
        .field_election
        .expect("present");
    assert_eq!(election.depth, 50);
}

/// **A declaration written before the surprisal election parses, and reads
/// as declining the vector.** The field takes `serde(default)` because the
/// election elects a reading out rather than in, per `weaver-spu-PRD`
/// section 13.12, so the absent member and an explicit `false` mean the
/// same thing to this crate. What keeps them distinguishable to a consumer
/// is the record rather than this struct: the `load` event carries the
/// election as a member serialized even when false.
#[test]
fn an_absent_surprisal_election_declines_the_vector() {
    let config = parse(&full_config()).expect("parses");
    assert!(
        !config.spu_instruction.decoder.surprisal_election,
        "the standing fixture predates the election and declines it"
    );
}

/// A declared election parses, and the kebab spelling is the file's.
#[test]
fn a_declared_surprisal_election_parses() {
    let source = full_config().replace(
        "residual-readout-election = false\n",
        "residual-readout-election = false\nsurprisal-election = true\n",
    );
    let config = parse(&source).expect("parses");
    assert!(
        config.spu_instruction.decoder.surprisal_election,
        "the operator elected the vector"
    );
}

/// **A misspelled election refuses rather than defaulting quietly.** The
/// instruction denies unknown fields, so an operator who writes
/// `surprisal-elections` is told, where a permissive parse would hand back
/// a declaration that reads as declining the vector it asked for.
#[test]
fn a_misspelled_surprisal_election_refuses() {
    let source = full_config().replace(
        "residual-readout-election = false\n",
        "residual-readout-election = false\nsurprisal-elections = true\n",
    );
    assert!(
        parse(&source).is_err(),
        "an unknown member refuses rather than being ignored"
    );
}

/// The analysis crate's pinned derivation, copied byte for byte into this tree
/// from WeaverAnalysis `tests/fixtures/derived-surrogate.toml` at commit
/// `10535e7` (2026-09-30), the commit that last changed it there. It is a copy
/// rather than a path, because `include_str!` resolves at compile time and
/// cannot follow `WEAVER_ANALYSIS_DIR` or a sibling checkout, and a workspace
/// test must compile without one. When the fixture moves in WeaverAnalysis,
/// the copy is replaced and this line names the new commit;
/// `the_pinned_derivation_is_weaver_analysis_own` holds the two equal wherever
/// the sibling stands.
const DERIVED_SURROGATE: &str = include_str!("fixtures/derived-surrogate.toml");

/// **The declaration `weaver-analysis derive` writes loads here**, per
/// `weaver-analysis-Spec` section 3: the derived declaration is the one the
/// operator loads. The file is the analysis crate's own pinned output, which
/// its driver test holds byte for byte, so this parse and that derivation
/// read one text. It carries an identity character the record escaped as a
/// surrogate pair, which crosses as the character.
#[test]
fn the_declaration_weaver_analysis_derives_parses() {
    let config = parse(DERIVED_SURROGATE).expect("the derived declaration parses");
    assert_eq!(
        config.binding_kind,
        Some(weaver_types::BindingKind::Diagnostic)
    );
    match config.spu_instruction.decoder.identity[0]
        .content
        .as_slice()
    {
        [weaver_traits::ContentBlock::Text { text }] => {
            assert!(text.starts_with("You are Karl \u{1F600}"), "{text}")
        }
        other => panic!("one text block, got {other:?}"),
    }
}

/// **The pinned copy is WeaverAnalysis's own text**, wherever that repository is
/// checked out: `WEAVER_ANALYSIS_DIR`, or the sibling `../WeaverAnalysis` of the
/// suite workshop. Where neither stands, there is nothing to compare, and the
/// test says so on stderr rather than passing in silence. Where one stands and
/// its fixture differs from the copy, the copy is stale and the test fails,
/// naming the file to copy. Perturbation: change one byte of the copy, and with
/// the sibling present it fails.
#[test]
fn the_pinned_derivation_is_weaver_analysis_own() {
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let analysis = std::env::var_os("WEAVER_ANALYSIS_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| workspace.join("../WeaverAnalysis"));
    let source = analysis.join("tests/fixtures/derived-surrogate.toml");
    match std::fs::read_to_string(&source) {
        Ok(theirs) => assert!(
            theirs == DERIVED_SURROGATE,
            "the pinned crates/weaver-types/tests/fixtures/derived-surrogate.toml differs from {}: copy it over and name the WeaverAnalysis commit in config.rs",
            source.display()
        ),
        Err(e) => eprintln!(
            "SKIP the_pinned_derivation_is_weaver_analysis_own: no WeaverAnalysis fixture at {} ({e}); set WEAVER_ANALYSIS_DIR to compare",
            source.display()
        ),
    }
}

/// **A path carrying a control character refuses, naming its field**, per
/// `weaver-types-Spec` section 2: every reader in the suite must agree on what
/// a path is, and a control character is where they part. A trailing newline
/// is the case that parted them, a shell dropping what this parse keeps.
///
/// Perturbation: remove the `check_declared_paths` call from `parse` and every
/// case below parses. Watched under exactly that removal.
#[test]
fn a_path_carrying_a_control_character_refuses_by_name() {
    let sink = "path = \"/var/lib/weaver/alpha/trace.ndjson\"";
    for control in ["\\n", "\\u0007", "\\t", "\\u007F"] {
        let source = full_config().replace(
            sink,
            &format!("path = \"/var/lib/weaver/alpha/trace.ndjson{control}\""),
        );
        let err = parse(&source).expect_err("refuses");
        assert_eq!(err.kind, ConfigErrorKind::BadValue, "{control}");
        assert_eq!(
            err.field,
            Some(FieldName("trace-sink.path".into())),
            "{control}"
        );
    }
    let looped = format!(
        "loop-file = \"/etc/weaver/agents/a.loop.py\\n\"\n{}",
        full_config()
    );
    let err = parse(&looped).expect_err("refuses");
    assert_eq!(err.field, Some(FieldName("loop-file".into())));
    let restored = format!(
        "{}\n[restore]\nrecord = \"/var/lib/weaver/s-1\\u0007.ndjson\"\n",
        full_config()
    );
    let err = parse(&restored).expect_err("refuses");
    assert_eq!(err.field, Some(FieldName("restore.record".into())));
    // A path with no control character parses as it did: a space, a quote and
    // a non-ASCII character are not control characters.
    let plain = full_config().replace(
        sink,
        "path = \"/var/lib/weaver/a b \\\"c\\\" \u{e9}.ndjson\"",
    );
    parse(&plain).expect("a path without a control character parses");
}
