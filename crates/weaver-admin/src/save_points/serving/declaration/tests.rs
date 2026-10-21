use sha2::{Digest as _, Sha256};
use weaver_types::state_flow::Digest;
use weaver_types::{parse_serving_declaration, parse_serving_draft};

use super::*;

const GOLDEN: &str = r#"session = "fixture-session"
spu-instruction.decoder.model-binding.artifact = "/models/decoder.gguf"
spu-instruction.decoder.model-binding.devices = [0]
spu-instruction.decoder.residual-readout-election = false
spu-instruction.decoder.surprisal-election = false
spu-instruction.decoder.refeed-permission = false
spu-instruction.decoder.column-permission = false
spu-instruction.decoder.tunable-values = {}
tool-set = []
trace-sink.kind = "file"
trace-sink.path = "/fixture/trace.jsonl"
trace-sink.create = true
state-management.flow.version = 1
state-management.flow.partition = []
state-management.flow.gate-wait.elapsed-ns = "11"
state-management.flow.gate-wait.measurement-ref = "fixture-gate"
state-management.flow.cut-wait.elapsed-ns = "12"
state-management.flow.cut-wait.measurement-ref = "fixture-cut"
state-management.flow.observation-interval.elapsed-ns = "13"
state-management.flow.observation-interval.measurement-ref = "fixture-observation"
state-management.flow.backlog.max-pending-blocks = "14"
state-management.flow.backlog.max-pending-bytes = "15"
state-management.flow.backlog.lag-alarm-turns = "16"
state-management.flow.backlog.measurement-ref = "fixture-backlog"
state-management.flow.scheme.scheme-id = "ste100:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
state-management.flow.scheme.artifact-sha256 = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
state-management.flow.scheme.verbs = ["hold", "recall"]
state-management.start.dump-sha256 = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
state-management.persistence.save-wait.elapsed-ns = "17"
state-management.persistence.save-wait.measurement-ref = "fixture-save"
state-management.persistence.snapshot-wait.elapsed-ns = "120000000000"
state-management.persistence.snapshot-wait.measurement-ref = "fixture-snapshot"
state-management.reads.max-result-bytes = "18"
state-management.reads.measurement-ref = "fixture-read"
state-management.ste100.model-binding.artifact = "/models/checker.json"
state-management.ste100.model-binding.devices = [1]
state-management.ste100.model-artifact-sha256 = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
state-management.ste100.instances = 2
state-management.ste100.max-wire-bytes = "19"
state-management.ste100.measurement-ref = "fixture-checker"
state-management.ste100.repair-cap.attempts = 3
state-management.ste100.repair-cap.measurement-ref = "fixture-repair"
state-management.ste100.decision-wait.elapsed-ns = "20"
state-management.ste100.decision-wait.measurement-ref = "fixture-decision"
state-management.tools = {}
state-management.placement.c-head = 21
state-management.placement.c-tail = 22
state-management.placement.measurement-ref = "fixture-placement"
"#;

pub(in crate::save_points::serving) fn fixture() -> ServingDeclaration {
    parse_serving_declaration(GOLDEN).unwrap()
}

fn round_trip(source: &str) -> (ServingDeclaration, String) {
    let bound = parse_serving_declaration(source).unwrap();
    let rendered = render(&bound).unwrap();
    let parsed = parse_serving_declaration(&rendered).unwrap();
    assert_eq!(parsed, bound, "rendered:\n{rendered}");
    assert_eq!(render(&parsed).unwrap(), rendered);
    (parsed, rendered)
}

#[test]
fn canonical_bytes_follow_parser_order_and_have_one_hash() {
    // Authoring layout, comments and field order do not reach the bound bytes.
    let source = GOLDEN
        .lines()
        .filter(|line| !line.starts_with("state-management.start."))
        .rev()
        .map(|line| format!("  {line}  # authoring comment\n"))
        .collect::<String>();
    let draft = parse_serving_draft(&source).unwrap();
    assert!(draft.state_management().start.is_none());
    let bound = draft.bind_start(Digest::new("a".repeat(64)).unwrap());
    assert_eq!(bound, fixture());
    let rendered = render(&bound).unwrap();
    assert_eq!(rendered, GOLDEN);
    assert_eq!(rendered.matches("dump-sha256").count(), 1);
    assert!(!rendered.contains('#'));
    assert_eq!(parse_serving_declaration(&rendered).unwrap(), bound);
    assert_eq!(
        render(&parse_serving_declaration(GOLDEN).unwrap()).unwrap(),
        rendered
    );
    assert_eq!(
        format!("{:x}", Sha256::digest(rendered.as_bytes())),
        "77baddbc836abba6c5d10f96b41726f55320bc99027a66b4834a9733778376c4"
    );
}

#[test]
fn rebinding_changes_only_start_and_canonical_hash() {
    let original = fixture();
    let replaced = original.with_start(Digest::new("d".repeat(64)).unwrap());
    assert_eq!(replaced.agent(), original.agent());
    let before = render(&original).unwrap();
    let after = render(&replaced).unwrap();
    assert_eq!(after, before.replace(&"a".repeat(64), &"d".repeat(64)));
    assert_ne!(
        Sha256::digest(before.as_bytes()),
        Sha256::digest(after.as_bytes())
    );
    assert_eq!(parse_serving_declaration(&after).unwrap(), replaced);
    assert_eq!(render(&original).unwrap(), before);
}

#[test]
fn every_operational_field_survives_with_ordered_arrays_and_empty_options() {
    let source = GOLDEN
        .replace("devices = [0]", "devices = [4294967295, 0, 3]")
        .replace("residual-readout-election = false", "residual-readout-election = true")
        .replace("surprisal-election = false", "surprisal-election = true")
        .replace("refeed-permission = false", "refeed-permission = true")
        .replace("column-permission = false", "column-permission = true")
        .replace(
            "tool-set = []",
            "tool-set = [\"sh\"]\n\
             binding-kind = \"serving\"\n\
             gate-instruction.access-rule.allowed-uids = [23, 1]\n\
             gate-instruction.access-rule.allowed-gids = [24, 2]\n\
             gate-instruction.access-rule.denied-uids = [25, 3]\n\
             state-store.engine = \"sqlite\"\n\
             lifecycle.drain-bound = 9223372036854775807\n\
             lifecycle.wind-down-bound = 26\n\
             spu-instruction.decoder.field-election.depth = 4294967295\n\
             spu-instruction.classify.model-binding.artifact = \"/models/scorer\"\n\
             spu-instruction.classify.model-binding.devices = [4, 2]",
        )
        .replace(
            "state-management.flow.partition = []",
            r#"state-management.flow.partition = [{id="user",kind="message.user",channels=["input"],origins=["external","operator"],blocks=["content.text[2]","content.text[0]"]}, {id="instruction",kind="loop.instruction",channels=["internal"],origins=["loop"],blocks=["message.content.text[*]"],context="context"}]"#,
        )
        .replace(
            "state-management.tools = {}",
            "state-management.tools.sh.clock-ms = 60000\n\
             state-management.tools.sh.measurement-ref = \"fixture-shell\"",
        );
    let (bound, rendered) = round_trip(&source);
    let agent = bound.agent();
    assert_eq!(agent.binding_kind, Some(BindingKind::Serving));
    assert_eq!(
        agent.state_store.as_ref().unwrap().engine,
        StoreEngine::Sqlite
    );
    assert_eq!(
        agent
            .spu_instruction
            .decoder
            .field_election
            .as_ref()
            .unwrap()
            .depth,
        u32::MAX
    );
    assert_eq!(
        agent.lifecycle.as_ref().unwrap().drain_bound,
        Some(i64::MAX as u64)
    );
    assert!(rendered.contains("devices = [4294967295, 0, 3]"));
    assert!(rendered.contains("allowed-uids = [1, 23]"));
    assert!(rendered.contains("origins = [\"external\", \"operator\"]"));
    assert!(rendered.contains("blocks = [\"content.text[2]\", \"content.text[0]\"]"));
    assert!(rendered.contains("context = \"context\""));
    assert_eq!(rendered.matches("context =").count(), 1);
    assert!(
        rendered.find("spu-instruction.classify").unwrap() < rendered.find("tool-set").unwrap()
    );
    assert!(rendered.find("state-store").unwrap() < rendered.find("lifecycle").unwrap());
    assert!(rendered.find("lifecycle").unwrap() < rendered.find("state-management").unwrap());

    for lifecycle in [
        "lifecycle = {}",
        "lifecycle.drain-bound = 1",
        "lifecycle.wind-down-bound = 2",
    ] {
        let (_, rendered) = round_trip(&format!("{GOLDEN}{lifecycle}\n"));
        assert!(rendered.contains(lifecycle));
    }
}

#[test]
fn optional_absences_stay_absent_and_every_sink_variant_round_trips() {
    let (_, rendered) = round_trip(GOLDEN);
    for absent in [
        "binding-kind",
        "gate-instruction",
        "state-store",
        "lifecycle",
        "spu-instruction.classify",
        "field-election",
        "state-election",
        "restore",
    ] {
        assert!(!rendered.contains(absent), "{absent}");
    }
    for kind in ["file", "pipe", "socket"] {
        let mut source = GOLDEN.replace("kind = \"file\"", &format!("kind = \"{kind}\""));
        if kind == "socket" {
            source = source.replace("trace-sink.create = true\n", "");
        } else {
            source = source.replace("trace-sink.create = true", "trace-sink.create = false");
        }
        let (_, rendered) = round_trip(&source);
        assert!(rendered.contains(&format!("trace-sink.kind = \"{kind}\"")));
        assert_eq!(rendered.contains("trace-sink.create"), kind != "socket");
    }
}

#[test]
fn toml_strings_escape_every_control_and_retain_unicode_and_parameter_keys() {
    assert_eq!(
        quote("\0\u{8}\t\n\u{c}\r\u{1f}\u{7f}\"\\").unwrap(),
        r#""\u0000\b\t\n\f\r\u001f\u007f\"\\""#
    );
    let text = "\"\\\n\r\t\u{8}\u{c}\0\u{1f}\u{7f}\u{85}\u{2028}\u{e9}\u{1f680}";
    let key = format!("a.quoted-{text}");
    let source = GOLDEN
        .replace("\"fixture-session\"", &quote(text).unwrap())
        .replace("\"fixture-gate\"", &quote(text).unwrap())
        .replace(
            "spu-instruction.decoder.tunable-values = {}",
            &format!(
                "spu-instruction.decoder.tunable-values.{} = -0.0",
                quote(&key).unwrap()
            ),
        );
    let (bound, rendered) = round_trip(&source);
    assert_eq!(bound.agent().session.0, text);
    assert_eq!(
        bound.state_management().flow.gate_wait.measurement_ref,
        text
    );
    assert_eq!(
        bound.agent().spu_instruction.decoder.tunable_values[&key].to_bits(),
        (-0.0f64).to_bits()
    );
    assert!(!rendered.contains('\u{7f}'));
    assert!(!rendered.contains('\0'));
    assert!(rendered.contains("\\u007f"));
    assert!(rendered.contains('\u{e9}'));
    assert!(rendered.contains('\u{1f680}'));
}

#[test]
fn finite_float_extremes_and_full_width_decimal_strings_keep_their_types() {
    let values = [
        0.0f64,
        -0.0,
        f64::MIN_POSITIVE,
        f64::MAX,
        f64::MIN,
        f64::from_bits(1),
    ];
    let tunables = values
        .iter()
        .enumerate()
        .map(|(at, value)| {
            format!(
                "spu-instruction.decoder.tunable-values.v{at} = {}\n",
                finite_float(*value).unwrap()
            )
        })
        .collect::<String>();
    let source = GOLDEN
        .replace("spu-instruction.decoder.tunable-values = {}\n", &tunables)
        .replace(
            "elapsed-ns = \"11\"",
            "elapsed-ns = \"18446744073709551615\"",
        )
        .replace(
            "lag-alarm-turns = \"16\"",
            "lag-alarm-turns = \"18446744073709551615\"",
        );
    let (bound, rendered) = round_trip(&source);
    for (at, value) in values.into_iter().enumerate() {
        assert_eq!(
            bound.agent().spu_instruction.decoder.tunable_values[&format!("v{at}")].to_bits(),
            value.to_bits()
        );
    }
    assert_eq!(
        bound.state_management().flow.gate_wait.elapsed_ns.0,
        u64::MAX
    );
    assert!(rendered.contains("elapsed-ns = \"18446744073709551615\""));
}

#[test]
fn impossible_native_values_refuse_without_reinterpreting_their_type() {
    assert_eq!(
        native_integer(i64::MAX as u64).unwrap(),
        i64::MAX.to_string()
    );
    assert!(matches!(
        native_integer(i64::MAX as u64 + 1),
        Err(RenderError::Unsupported(_))
    ));
    assert!(matches!(
        native_integer(u64::MAX),
        Err(RenderError::Unsupported(_))
    ));
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(matches!(
            finite_float(value),
            Err(RenderError::Unsupported("tunable-values"))
        ));
    }
    assert!(
        parse_serving_declaration(&GOLDEN.replace("c-head = 21", "c-head = 18446744073709551615"))
            .is_err()
    );
    assert!(
        parse_serving_declaration(
            &GOLDEN.replace("tunable-values = {}", "tunable-values = { bad = nan }")
        )
        .is_err()
    );
}
