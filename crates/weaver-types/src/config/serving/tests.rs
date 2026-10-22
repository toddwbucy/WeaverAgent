use super::*;

// Synthetic parser inputs, not production measurement or model qualification.
const COMMON: &str = r#"
session = "session-a"
tool-set = ["sh"]
binding-kind = "serving"
spu-instruction.decoder.residual-readout-election = true
spu-instruction.decoder.field-election.depth = 8
spu-instruction.decoder.surprisal-election = true
spu-instruction.decoder.refeed-permission = false
spu-instruction.decoder.column-permission = false
spu-instruction.decoder.tunable-values = { temperature = 0.5, top-k = 20.0 }
spu-instruction.decoder.model-binding.artifact = "decoder-fixture"
spu-instruction.decoder.model-binding.devices = [1, 0]
spu-instruction.classify.model-binding.artifact = "label-fixture"
spu-instruction.classify.model-binding.devices = [2]
gate-instruction.access-rule.allowed-uids = [1000]
gate-instruction.access-rule.allowed-gids = [1001]
gate-instruction.access-rule.denied-uids = [1701]
trace-sink.kind = "file"
trace-sink.path = "/var/lib/weaver/trace.ndjson"
trace-sink.create = true
state-store.engine = "sqlite"
lifecycle.drain-bound = 5
lifecycle.wind-down-bound = 6
"#;

const STATE: &str = r#"
state-management.flow.version = 1
state-management.flow.partition = [{ id = "user", kind = "message.user", channels = ["input"], origins = ["operator", "external"], blocks = ["content.text[*]"] }]
state-management.flow.gate-wait.elapsed-ns = "11"
state-management.flow.gate-wait.measurement-ref = "fixture-gate"
state-management.flow.cut-wait.elapsed-ns = "12"
state-management.flow.cut-wait.measurement-ref = "fixture-cut"
state-management.flow.observation-interval.elapsed-ns = "13"
state-management.flow.observation-interval.measurement-ref = "fixture-cadence"
state-management.flow.backlog.max-pending-blocks = "14"
state-management.flow.backlog.max-pending-bytes = "15"
state-management.flow.backlog.lag-alarm-turns = "16"
state-management.flow.backlog.measurement-ref = "fixture-backlog"
state-management.flow.scheme.scheme-id = "ste100:@A@"
state-management.flow.scheme.artifact-sha256 = "@A@"
state-management.flow.scheme.verbs = ["read", "hold"]
state-management.start.dump-sha256 = "@B@"
state-management.persistence.save-wait.elapsed-ns = "17"
state-management.persistence.save-wait.measurement-ref = "fixture-save"
state-management.persistence.snapshot-wait.elapsed-ns = "120000000000"
state-management.persistence.snapshot-wait.measurement-ref = "initial-merged-120s"
state-management.reads.max-result-bytes = "18"
state-management.reads.measurement-ref = "fixture-read"
state-management.ste100.model-binding.artifact = "ste100-fixture"
state-management.ste100.model-binding.devices = [3, 4]
state-management.ste100.model-artifact-sha256 = "@C@"
state-management.ste100.instances = 2
state-management.ste100.max-wire-bytes = "19"
state-management.ste100.measurement-ref = "fixture-ste100"
state-management.ste100.repair-cap.attempts = 3
state-management.ste100.repair-cap.measurement-ref = "fixture-repair"
state-management.ste100.decision-wait.elapsed-ns = "20"
state-management.ste100.decision-wait.measurement-ref = "fixture-decision"
state-management.tools.sh.clock-ms = 60000
state-management.tools.sh.measurement-ref = "fixture-shell"
state-management.placement.c-head = 21
state-management.placement.c-tail = 22
state-management.placement.measurement-ref = "fixture-placement"
"#;

fn full() -> String {
    format!("{COMMON}{STATE}")
        .replace("@A@", &"a".repeat(64))
        .replace("@B@", &"b".repeat(64))
        .replace("@C@", &"c".repeat(64))
}

fn without(source: &str, key: &str) -> String {
    let prefix = format!("{key} =");
    assert!(
        source.lines().any(|line| line.starts_with(&prefix)),
        "{key}"
    );
    source
        .lines()
        .filter(|line| !line.starts_with(&prefix))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

fn replace(source: &str, key: &str, value: &str) -> String {
    let prefix = format!("{key} =");
    assert!(
        source.lines().any(|line| line.starts_with(&prefix)),
        "{key}"
    );
    source
        .lines()
        .map(|line| {
            if line.starts_with(&prefix) {
                format!("{key} = {value}")
            } else {
                line.into()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

fn assert_bad(source: &str, field: &str) {
    let error = parse_serving_draft(source).expect_err(field);
    assert_eq!(error.kind, ConfigErrorKind::BadValue, "{field}: {error}");
    assert_eq!(error.field, Some(FieldName(field.into())));
}

#[test]
fn complete_serving_grammar_retains_every_common_and_new_setting() {
    let parsed = parse_serving_declaration(&full()).unwrap();
    assert_eq!(parsed.agent(), &super::super::parse(COMMON).unwrap());
    let state = parsed.state_management();
    assert_eq!(state.flow.version, 1);
    assert_eq!(state.flow.partition[0].origins, ["operator", "external"]);
    assert_eq!(state.flow.gate_wait.elapsed_ns.0, 11);
    assert_eq!(state.flow.cut_wait.elapsed_ns.0, 12);
    assert_eq!(state.flow.observation_interval.elapsed_ns.0, 13);
    assert_eq!(state.flow.backlog.max_pending_blocks.0, 14);
    assert_eq!(state.flow.backlog.max_pending_bytes.0, 15);
    assert_eq!(state.flow.backlog.lag_alarm_turns.0, 16);
    assert_eq!(state.flow.scheme.verbs, ["read", "hold"]);
    assert_eq!(state.start.dump_sha256.as_str(), "b".repeat(64));
    assert_eq!(state.persistence.save_wait.elapsed_ns.0, 17);
    assert_eq!(
        state.persistence.snapshot_wait.elapsed_ns.0,
        120_000_000_000
    );
    assert_eq!(state.reads.max_result_bytes.0, 18);
    assert_eq!(state.ste100.model_binding.devices[0].0, 3);
    assert_eq!(state.ste100.instances, 2);
    assert_eq!(state.ste100.max_wire_bytes.0, 19);
    assert_eq!(state.ste100.repair_cap.attempts, 3);
    assert_eq!(state.ste100.decision_wait.elapsed_ns.0, 20);
    assert_eq!(state.tools.sh.as_ref().unwrap().clock_ms, 60_000);
    assert_eq!(state.placement.c_head, 21);
    assert_eq!(state.placement.c_tail, 22);
    state.validate(&["sh".into()]).unwrap();
    assert!(parsed.agent().state_election.is_none());
    assert!(parsed.agent().restore.is_none());
}

#[test]
fn draft_absence_is_limited_to_the_start_hash_and_binding_changes_only_it() {
    let source = without(&full(), "state-management.start.dump-sha256");
    let expected = parse_serving_declaration(&full()).unwrap();
    for source in [
        source.clone(),
        format!("{source}\n[state-management.start]\n"),
    ] {
        let draft = parse_serving_draft(&source).unwrap();
        assert!(draft.state_management().start.is_none());
        let error = parse_serving_declaration(&source).unwrap_err();
        assert_eq!(error.kind, ConfigErrorKind::MissingField);
        assert_eq!(
            error.field,
            Some(FieldName("state-management.start.dump-sha256".into()))
        );
        assert_eq!(
            draft.bind_start(Digest::new("b".repeat(64)).unwrap()),
            expected
        );
    }
    let changed = expected.with_start(Digest::new("d".repeat(64)).unwrap());
    let draft = parse_serving_draft(&full()).unwrap();
    assert_eq!(
        changed,
        draft.bind_start(Digest::new("d".repeat(64)).unwrap())
    );
    assert_eq!(changed.agent(), expected.agent());
    assert_eq!(
        expected.state_management().start.dump_sha256.as_str(),
        "b".repeat(64)
    );
    assert_eq!(
        changed.with_start(Digest::new("b".repeat(64)).unwrap()),
        expected
    );
    for invalid in ["\"\"", "\"abc\"", "0", "true", "[]", "{}"] {
        assert_bad(
            &replace(&full(), "state-management.start.dump-sha256", invalid),
            "state-management.start.dump-sha256",
        );
    }
}

#[test]
fn every_required_state_field_refuses_absence_even_in_authoring_mode() {
    for line in STATE.lines().filter(|line| !line.trim().is_empty()) {
        let key = line.split(" = ").next().unwrap();
        if key == "state-management.start.dump-sha256" {
            continue;
        }
        let error = parse_serving_draft(&without(&full(), key)).expect_err(key);
        assert_eq!(error.kind, ConfigErrorKind::MissingField, "{key}: {error}");
        assert!(error.field.is_some(), "{key}");
    }
    let error = parse_serving_draft(COMMON).unwrap_err();
    assert_eq!(error.field, Some(FieldName("state-management".into())));
}

#[test]
fn decimal_bounds_native_values_and_model_bindings_are_attributed() {
    for (key, value) in [
        ("state-management.flow.version", "2"),
        ("state-management.flow.gate-wait.elapsed-ns", "\"0\""),
        ("state-management.flow.gate-wait.elapsed-ns", "11"),
        ("state-management.flow.gate-wait.elapsed-ns", "\"01\""),
        ("state-management.flow.gate-wait.elapsed-ns", "\"+1\""),
        ("state-management.flow.gate-wait.elapsed-ns", r#""\u0661""#),
        ("state-management.flow.gate-wait.measurement-ref", "\"\""),
        (
            "state-management.flow.observation-interval.elapsed-ns",
            "\"0\"",
        ),
        ("state-management.flow.backlog.max-pending-blocks", "\"0\""),
        (
            "state-management.flow.backlog.max-pending-bytes",
            "\"18446744073709551616\"",
        ),
        ("state-management.flow.backlog.lag-alarm-turns", "\"0\""),
        (
            "state-management.flow.scheme.scheme-id",
            "\"another-scheme\"",
        ),
        ("state-management.flow.scheme.artifact-sha256", "\"BAD\""),
        ("state-management.flow.scheme.verbs", "[\"read\", \"read\"]"),
        (
            "state-management.persistence.snapshot-wait.elapsed-ns",
            "\"0\"",
        ),
        ("state-management.reads.max-result-bytes", "\"0\""),
        ("state-management.ste100.instances", "0"),
        ("state-management.ste100.max-wire-bytes", "\"0\""),
        ("state-management.ste100.model-binding.artifact", "\"\""),
        ("state-management.ste100.model-binding.devices", "[]"),
        ("state-management.ste100.model-binding.devices", "[0, 0]"),
        ("state-management.ste100.repair-cap.attempts", "0"),
        ("state-management.ste100.decision-wait.elapsed-ns", "\"0\""),
        ("state-management.tools.sh.clock-ms", "0"),
        ("state-management.tools.sh.clock-ms", "60001"),
        ("state-management.placement.c-head", "0"),
        ("state-management.placement.c-tail", "-1"),
    ] {
        assert_bad(&replace(&full(), key, value), key);
    }
    let maximum = replace(
        &full(),
        "state-management.flow.gate-wait.elapsed-ns",
        "\"18446744073709551615\"",
    );
    assert_eq!(
        parse_serving_draft(&maximum)
            .unwrap()
            .state_management()
            .flow
            .gate_wait
            .elapsed_ns
            .0,
        u64::MAX
    );
}

#[test]
fn retired_and_root_only_fields_never_enter_the_operational_copy() {
    for key in [
        "state-election",
        "state-flow",
        "restore",
        "restore-artifact",
        "identity",
        "identity-file",
        "system-prompt",
        "prompt-path",
        "binaries",
        "load-bound-seconds",
    ] {
        let error = parse_serving_draft(&format!("{key} = {{}}\n{}", full())).unwrap_err();
        assert_eq!(error.kind, ConfigErrorKind::UnknownField, "{key}: {error}");
        assert_eq!(error.field, Some(FieldName(key.into())));
    }
    assert_bad(
        &replace(&full(), "binding-kind", "\"diagnostic\""),
        "binding-kind",
    );
    assert_bad(
        &replace(&full(), "state-store.engine", "\"none\""),
        "state-store.engine",
    );
    assert_bad(
        &replace(&full(), "state-store.engine", "\"postgres\""),
        "state-store.engine",
    );
}

#[test]
fn every_table_is_closed_and_toml_aliases_do_not_change_wire_spelling() {
    for table in [
        "state-management",
        "state-management.flow",
        "state-management.flow.gate-wait",
        "state-management.flow.backlog",
        "state-management.flow.scheme",
        "state-management.start",
        "state-management.persistence",
        "state-management.reads",
        "state-management.ste100",
        "state-management.ste100.model-binding",
        "state-management.ste100.repair-cap",
        "state-management.tools",
        "state-management.placement",
    ] {
        let key = format!("{table}.unknown");
        let error = parse_serving_draft(&format!("{}\n{key} = true\n", full())).unwrap_err();
        assert_eq!(error.kind, ConfigErrorKind::UnknownField, "{key}: {error}");
        assert_eq!(error.field, Some(FieldName(key)));
    }
    for key in [
        "state-management",
        "gate-wait",
        "observation-interval",
        "max-pending-bytes",
        "dump-sha256",
        "max-result-bytes",
        "model-binding",
        "model-artifact-sha256",
        "repair-cap",
        "decision-wait",
        "clock-ms",
        "c-head",
    ] {
        let bad = full().replace(key, &key.replace('-', "_"));
        assert!(parse_serving_draft(&bad).is_err(), "{key}");
    }
    let parsed = parse_serving_declaration(&full()).unwrap();
    let json = serde_json::to_string(parsed.state_management()).unwrap();
    assert!(json.contains("\"gate_wait\""));
    assert!(!json.contains("\"gate-wait\""));
    assert!(
        serde_json::from_str::<StateManagementConfig>(&json.replace("gate_wait", "gate-wait"))
            .is_err()
    );
}

#[test]
fn duplicate_keys_and_wrong_table_shapes_refuse() {
    for extra in [
        "session = \"again\"",
        "state-management.flow.version = 1",
        "state-management.start.dump-sha256 = \"bad\"",
    ] {
        assert!(parse_serving_draft(&format!("{}\n{extra}\n", full())).is_err());
    }
    for line in [
        "state-management.flow.partition = [{ id = \"a\", id = \"b\" }]",
        "state-management.flow.partition = 1",
    ] {
        let source = without(&full(), "state-management.flow.partition");
        assert!(parse_serving_draft(&format!("{source}\n{line}\n")).is_err());
    }
    for key in ["flow", "start", "reads", "ste100", "tools", "placement"] {
        let prefix = format!("state-management.{key}.");
        let source = full()
            .lines()
            .filter(|line| !line.starts_with(&prefix))
            .collect::<Vec<_>>()
            .join("\n");
        let error =
            parse_serving_draft(&format!("{source}\nstate-management.{key} = 1\n")).unwrap_err();
        assert_eq!(
            error.field,
            Some(FieldName(format!("state-management.{key}")))
        );
    }
}

#[test]
fn partition_projection_is_whole_and_has_no_hidden_default() {
    let source = replace(&full(), "state-management.flow.partition", "[]");
    assert!(
        parse_serving_draft(&source)
            .unwrap()
            .state_management()
            .flow
            .partition
            .is_empty()
    );
    let rule = |blocks: &str, tail: &str| {
        format!(
            "[{{ id = \"one\", kind = \"unknown-to-floor\", channels = [\"input\"], origins = [\"operator\"], blocks = {blocks}{tail} }}]"
        )
    };
    for blocks in [
        "[\"content.text[0]\"]",
        "[\"message.content.text[*]\"]",
        "[\"pair\"]",
    ] {
        parse_serving_draft(&replace(
            &full(),
            "state-management.flow.partition",
            &rule(blocks, ""),
        ))
        .unwrap();
    }
    for blocks in [
        "[]",
        "[\"content.text[01]\"]",
        "[\"content.text[-1]\"]",
        "[\"content.text[18446744073709551616]\"]",
        "[\"content.tool_call[*]\"]",
        "[\"return\"]",
        "[\"content.text[0].text\"]",
        "[\"content.text[*]\", \"content.text[0]\"]",
        "[\"pair\", \"content.text[0]\"]",
    ] {
        assert_bad(
            &replace(
                &full(),
                "state-management.flow.partition",
                &rule(blocks, ""),
            ),
            "state-management.flow.partition.0.blocks",
        );
    }
    let context = rule("[\"message.content.text[*]\"]", ", context = \"context\"");
    assert_eq!(
        parse_serving_draft(&replace(
            &full(),
            "state-management.flow.partition",
            &context
        ))
        .unwrap()
        .state_management()
        .flow
        .partition[0]
            .context
            .as_deref(),
        Some("context")
    );
    assert_bad(
        &replace(
            &full(),
            "state-management.flow.partition",
            &rule("[\"pair\"]", ", context = \"context\""),
        ),
        "state-management.flow.partition.0.blocks",
    );
    let single = rule("[\"pair\"]", "");
    let duplicate = format!(
        "[{}, {}]",
        &single[1..single.len() - 1],
        &single[1..single.len() - 1]
    );
    assert_bad(
        &replace(&full(), "state-management.flow.partition", &duplicate),
        "state-management.flow.partition.1.id",
    );
}

#[test]
fn tool_policy_presence_matches_the_existing_tool_election() {
    assert_bad(
        &replace(&full(), "tool-set", "[]"),
        "state-management.tools",
    );
    assert_bad(
        &replace(&full(), "tool-set", "[\"sh\", \"sh\"]"),
        "state-management.tools",
    );
    let no_policy = without(
        &without(&full(), "state-management.tools.sh.clock-ms"),
        "state-management.tools.sh.measurement-ref",
    ) + "state-management.tools = {}\n";
    assert_bad(&no_policy, "state-management.tools");
    let no_tools = replace(&no_policy, "tool-set", "[]");
    assert!(
        parse_serving_draft(&no_tools)
            .unwrap()
            .state_management()
            .tools
            .sh
            .is_none()
    );
}

#[test]
fn common_operational_checks_and_inventory_boundaries_are_unchanged() {
    for (key, value, field) in [
        (
            "spu-instruction.decoder.model-binding.devices",
            "[]",
            "model-binding",
        ),
        (
            "spu-instruction.decoder.tunable-values",
            "{ temperature = nan }",
            "tunable-values.temperature",
        ),
        ("trace-sink.path", r#""/tmp/trace\n""#, "trace-sink.path"),
        ("lifecycle.drain-bound", "0", "lifecycle.drain-bound"),
        (
            "lifecycle.wind-down-bound",
            "0.5",
            "lifecycle.wind-down-bound",
        ),
        ("tool-set", "[\"unknown\"]", "tool-set"),
    ] {
        assert_bad(&replace(&full(), key, value), field);
    }
    let source = format!("{}\ntrace-sink.extra = true\n", full());
    assert_eq!(
        parse_serving_draft(&source).unwrap_err().field,
        Some(FieldName("trace-sink.extra".into()))
    );
    let source = without(&without(&full(), "binding-kind"), "state-store.engine");
    let source = without(
        &without(
            &without(&source, "gate-instruction.access-rule.allowed-uids"),
            "gate-instruction.access-rule.allowed-gids",
        ),
        "gate-instruction.access-rule.denied-uids",
    );
    let parsed = parse_serving_draft(&source).unwrap();
    assert!(parsed.agent().binding_kind.is_none());
    assert!(parsed.agent().state_store.is_none());
    assert!(parsed.agent().gate_instruction.is_none());
    assert!(super::super::parse(&full()).is_err());
    assert!(super::super::parse(COMMON).is_ok());
}
