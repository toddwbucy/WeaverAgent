use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::process::Command;
use weaver_ste_driver::trace_report::report;

fn event(sequence: u64, kind: &str, payload: Value) -> Value {
    let (channel, origin) = match kind {
        "model.output" | "message.assistant" => ("output", "model"),
        "model.measurement" => ("measurement", "runtime"),
        "loop.instruction" | "emission.change" | "model.request" => ("internal", "loop"),
        "message.user" => ("input", "operator"),
        _ => ("internal", "runtime"),
    };
    let mut value = json!({"session":"session", "run":"run", "turn":"turn",
        "sequence":sequence.to_string(), "kind":kind, "channel":channel,
        "origin":origin, "subsystem":"harness", "wall_ms":1,
        "monotonic_ns":sequence.to_string()});
    // context_place_generation::model_event authors these as spu_decoder.
    if kind.starts_with("model.") {
        value["subsystem"] = json!("spu_decoder");
    }
    if kind == "load" {
        value.as_object_mut().unwrap().remove("turn");
    }
    if !payload.is_null() {
        value["payload"] = payload;
    }
    value
}

fn fixture() -> Vec<Value> {
    let original = json!({"role":"assistant","content":[
        {"type":"think","text":"Keep this thought intact."},
        {"type":"text","text":"Use coal."}]});
    let mut change: Value = serde_json::from_str(include_str!(
        "../../../crates/weaver-trace/tests/fixtures/ste-emission-offer.json"
    ))
    .unwrap();
    change["matched"]["source"] = json!({"run":"run","sequence":"6","block":"1"});
    change["method"]["offer"]["instruction"]["sequence"] = json!("7");
    change["method"]["offer"]["reemission"] = json!({"run":"run","sequence":"11","block":"0"});
    let measurement = json!({"model":"synthetic", "weights_hash":"test",
        "input_tokens":[10,11,12], "output_tokens":[20,21],
        "blocks":[{"label":"turn-delta","start":0,"end":4}],
        "timings":{"prefill_ns":"12","decode_ns":"34"}});
    let mut events = vec![
        event(0, "load", json!({})),
        event(1, "turn.started", Value::Null),
        event(
            2,
            "message.user",
            json!({"role":"user","content":[{"type":"text","text":"source"}]}),
        ),
        event(3, "model.request", json!({"rendered":"original request"})),
        event(
            4,
            "model.output",
            json!({"emission":"thought\nUse coal.","finish":"completed","resident":30,"capacity":100}),
        ),
        event(5, "model.measurement", measurement.clone()),
        event(6, "message.assistant", original),
        event(
            7,
            "loop.instruction",
            json!({"template_id":"thesaurus-offer/1",
            "message":{"role":"user","content":[{"type":"text","text":"Offer"}]},
            "instruction":{"offer":{"matched":change["matched"]}}}),
        ),
        event(8, "model.request", json!({"rendered":"offer request"})),
        event(
            9,
            "model.output",
            json!({"emission":"Use fuel.","finish":"length","resident":40,"capacity":100}),
        ),
        event(10, "model.measurement", measurement),
        event(
            11,
            "message.assistant",
            json!({"role":"assistant","content":[{"type":"text","text":"Use fuel."}]}),
        ),
        event(12, "emission.change", change),
        event(13, "turn.closed", json!({"close":"clean"})),
    ];
    events[7]["causal_parent"] = json!("6");
    events[11]["causal_parent"] = json!("7");
    events[12]["causal_parent"] = json!("11");
    events
}

fn lines(values: &[Value]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for value in values {
        serde_json::to_writer(&mut bytes, value).unwrap();
        bytes.push(b'\n');
    }
    bytes
}

fn journal() -> Vec<Value> {
    vec![
        json!({"kind":"request","id":"row-7-final",
        "request":{"ordinal":1,"kind":"initial","text":"source"}}),
        json!({"kind":"checked_attempt","id":"row-7-final","attempt":{
            "ordinal":1,"kind":"initial","text":"source",
            "result":{"class":"checked","output":"Use fuel.",
                "response":{"run":"run","turn":"turn"}}}}),
    ]
}

fn turn(report: &Value) -> &Value {
    &report["requests"][0]["trace"]
}

fn assert_incomplete(events: &[Value], reason: &str) {
    let result = report(&lines(events), &lines(&journal())).unwrap();
    assert_eq!(result["complete"], false);
    let trace = turn(&result);
    assert_eq!(trace["trace_complete"], false);
    assert!(trace["recorded_generation_count"].is_null());
    assert!(
        trace["incomplete_reasons"]
            .as_array()
            .unwrap()
            .contains(&json!(reason)),
        "expected {reason}, got {}",
        trace["incomplete_reasons"]
    );
}

#[test]
fn gapless_run_requires_a_load_before_its_turn() {
    let mut events = fixture();
    events[0]["kind"] = json!("fault");
    assert_incomplete(&events, "run has no load at its start");
}

#[test]
fn later_assistant_requires_an_offer_even_with_complete_measurements() {
    let mut events = fixture();
    events[7]["kind"] = json!("fault");
    events[11].as_object_mut().unwrap().remove("causal_parent");
    assert_incomplete(&events, "later assistant message has no offer parent");
}

#[test]
fn sequence_decimal_requires_canonical_spelling() {
    let mut events = fixture();
    events[1]["sequence"] = json!("01");
    assert_incomplete(&events, "trace line 2: invalid sequence");
}

#[test]
fn trace_subsystem_requires_a_declared_name() {
    let mut events = fixture();
    events[2]["subsystem"] = json!("unknown");
    assert_incomplete(&events, "trace line 3: invalid trace identity or counter");
}

#[test]
fn measurement_requires_its_channel_and_origin() {
    for (field, value) in [("channel", "output"), ("origin", "model")] {
        let mut events = fixture();
        events[5][field] = json!(value);
        assert_incomplete(&events, "generation measurement evidence is incomplete");
    }
}

#[test]
fn measurement_timings_require_canonical_decimal_strings() {
    for field in ["prefill_ns", "decode_ns"] {
        for value in ["1.5", "-1", "01"] {
            let mut events = fixture();
            events[5]["payload"]["timings"][field] = json!(value);
            assert_incomplete(&events, "generation measurement evidence is incomplete");
        }
    }
}

#[test]
fn trace_report_counts_outcomes_and_keeps_producer_evidence_and_offer_links() {
    let events = fixture();
    let trace = lines(&events);
    let journal = lines(&journal());
    let result = report(&trace, &journal).unwrap();
    let turn = turn(&result);
    assert_eq!(result["complete"], true);
    assert_eq!(turn["recorded_generation_count"], 2);
    assert_eq!(turn["events"].as_array().unwrap(), &events[1..]);
    assert_eq!(turn["first_model_output"], events[4]);
    assert_eq!(
        turn["recorded_token_use"],
        json!([
        {"sequence":"5","input_tokens":3,"output_tokens":2,
         "timings":{"prefill_ns":"12","decode_ns":"34"}},
        {"sequence":"10","input_tokens":3,"output_tokens":2,
         "timings":{"prefill_ns":"12","decode_ns":"34"}}])
    );
    assert_eq!(
        turn["offers"],
        json!([{"instruction_sequence":"7",
        "source":{"run":"run","sequence":"6","block":"1"},"response_sequences":["11"]}])
    );
    assert_eq!(
        result["trace_sha256"],
        format!("{:x}", Sha256::digest(&trace))
    );
    assert_eq!(
        result["journal_sha256"],
        format!("{:x}", Sha256::digest(&journal))
    );
}

#[test]
fn trace_report_selects_run_and_turn_without_counting_neighbors() {
    let mut events = fixture();
    let mut later = fixture();
    later.remove(0);
    for (index, e) in later.iter_mut().enumerate() {
        e["turn"] = json!("other-turn");
        e["sequence"] = json!((index + 14).to_string());
    }
    events.extend(later);
    let mut other = fixture();
    for e in &mut other {
        e["run"] = json!("other-run");
    }
    events.extend(other);
    let result = report(&lines(&events), &lines(&journal())).unwrap();
    assert_eq!(result["complete"], true);
    assert_eq!(turn(&result)["recorded_generation_count"], 2);
    assert_eq!(turn(&result)["events"].as_array().unwrap().len(), 13);
}

#[test]
fn gaps_duplicates_missing_brackets_and_bad_output_never_get_a_guessed_count() {
    for case in 0..11 {
        let mut events = fixture();
        match case {
            0 => {
                events.remove(5);
            }
            1 => {
                events.insert(4, events[4].clone());
            }
            2 => {
                events.remove(0);
            }
            3 => {
                events.pop();
            }
            4 => {
                events[13]["payload"]["close"] = json!("unknown");
            }
            5 => {
                events[4]["payload"]["finish"] = json!("unknown");
            }
            6 => {
                events[4]["sequence"] = json!(4);
            }
            7 => {
                events[4]["session"] = json!("foreign");
            }
            8 => {
                events.remove(3);
            }
            9 => {
                events.insert(3, events[3].clone());
            }
            10 => {
                events[1]["kind"] = json!("fault");
            }
            _ => unreachable!(),
        }
        let result = report(&lines(&events), &lines(&journal())).unwrap();
        assert_eq!(result["complete"], false, "case {case}");
        assert!(
            turn(&result)["recorded_generation_count"].is_null(),
            "case {case}"
        );
        assert!(
            !turn(&result)["incomplete_reasons"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }
    for suffix in [b"garbage\n".as_slice(), b"{\"run\":".as_slice()] {
        let mut trace = lines(&fixture());
        trace.extend(suffix);
        let result = report(&trace, &lines(&journal())).unwrap();
        assert!(turn(&result)["recorded_generation_count"].is_null());
    }
    let mut trace = lines(&fixture());
    trace.pop();
    assert!(
        turn(&report(&trace, &lines(&journal())).unwrap())["recorded_generation_count"].is_null()
    );
}

#[test]
fn missing_measurements_or_bad_offer_links_keep_the_report_incomplete() {
    for case in 0..11 {
        let mut events = fixture();
        match case {
            0 => {
                events[10]["kind"] = json!("fault");
            }
            1 => {
                events[10]["payload"]["output_tokens"] = json!(["bad"]);
            }
            2 => {
                events[11].as_object_mut().unwrap().remove("causal_parent");
            }
            3 => {
                events[11]["causal_parent"] = json!("6");
            }
            4 => {
                events[7]["causal_parent"] = json!("5");
            }
            5 => {
                events[7]["payload"]["instruction"]["offer"]["matched"]["source"]["run"] =
                    json!("foreign");
            }
            6 => {
                events[7]["payload"]["instruction"]["offer"]["matched"]["source"]["block"] =
                    json!("0");
            }
            7 => {
                events[7]["payload"]["template_id"] = json!("other");
            }
            8 => {
                events[11]["kind"] = json!("fault");
            }
            9 => {
                events[10]["payload"]["timings"]["decode_ns"] = Value::Null;
            }
            10 => {
                events[11]["channel"] = json!("internal");
            }
            _ => unreachable!(),
        }
        let result = report(&lines(&events), &lines(&journal())).unwrap();
        assert_eq!(result["complete"], false, "case {case}");
        assert!(
            turn(&result)["recorded_generation_count"].is_null(),
            "case {case}"
        );
        assert_eq!(turn(&result)["events"].as_array().unwrap(), &events[1..]);
    }
}

#[test]
fn unfinished_requests_and_unknown_turns_remain_in_the_report() {
    let mut journal = journal();
    journal.truncate(1);
    let result = report(&lines(&fixture()), &lines(&journal)).unwrap();
    assert_eq!(result["complete"], false);
    assert_eq!(result["requests"][0]["id"], "row-7-final");
    assert!(turn(&result)["recorded_generation_count"].is_null());
    let mut journal = self::journal();
    journal[1]["attempt"]["result"]["response"]["turn"] = json!("absent");
    let result = report(&lines(&fixture()), &lines(&journal)).unwrap();
    assert!(turn(&result)["recorded_generation_count"].is_null());
}

#[test]
fn ambiguous_or_mismatched_journals_refuse_before_collection() {
    for case in 0..5 {
        let mut journal = journal();
        match case {
            0 => {
                journal.insert(1, journal[0].clone());
            }
            1 => {
                journal.push(journal[1].clone());
            }
            2 => {
                journal[1]["attempt"]["text"] = json!("different");
            }
            3 => {
                journal.remove(0);
            }
            4 => {
                let mut second = journal.clone();
                for e in &mut second {
                    e["id"] = json!("row-8-final");
                }
                journal.extend(second);
            }
            _ => unreachable!(),
        }
        assert!(
            report(&lines(&fixture()), &lines(&journal)).is_err(),
            "case {case}"
        );
    }
}

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("ste-trace-{}-{stamp}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn trace_report_consumes_the_evaluation_journal_without_rewriting_it() {
    use weaver_ste_driver::{
        check::Checker,
        gate::{Capture, Transport},
        guard::Judge,
        lexicon::Lexicon,
        run::{Kit, R0Parameters, evaluate},
    };
    struct Answer;
    impl Transport for Answer {
        fn send(&mut self, text: &str) -> Capture {
            assert_eq!(text, "<<<TEXT\nUse coal.\nTEXT>>>");
            Capture {
                stdout: serde_json::to_vec(&json!({"kind":"answered",
                "run":"run","turn":"turn","text":"Use fuel."}))
                .unwrap(),
                stderr: vec![],
                exit_code: Some(0),
                error: None,
            }
        }
    }
    let directory = Directory::new();
    let input = directory.0.join("input");
    std::fs::write(
        &input,
        lines(&[json!({"id":"actual-journal","kind":"final","text":"Use coal."})]),
    )
    .unwrap();
    let kit = Kit::read(&input).unwrap();
    let lexicon = Lexicon::build(
        serde_json::from_value(json!({
        "approved":{"use":["v"],"fuel":["n"]},"unapproved":{},"unapproved_verbs":{},
        "technical_verbs":[],"technical_nouns":[],"approved_verbs":["use"],
        "caps_names":[],"fixed":{}}))
        .unwrap(),
    )
    .unwrap();
    let judge = Judge::new(Checker::new(lexicon, 20, 25).unwrap(), &[]).unwrap();
    let run = directory.0.join("evaluation");
    let evaluated = evaluate(
        &kit,
        &R0Parameters::new(2, 1, 0).unwrap(),
        &judge,
        &mut Answer,
        &run,
    )
    .unwrap();
    assert!(evaluated.r0_score_allowed());
    let journal = std::fs::read(run.join("journal.jsonl")).unwrap();
    let result = report(&lines(&fixture()), &journal).unwrap();
    assert_eq!(result["complete"], true);
    assert_eq!(result["requests"].as_array().unwrap().len(), 1);
    assert_eq!(result["requests"][0]["id"], "actual-journal");
    assert_eq!(turn(&result)["recorded_generation_count"], 2);
    assert_eq!(std::fs::read(run.join("journal.jsonl")).unwrap(), journal);
}

#[test]
fn offline_command_keeps_exact_sources_and_refuses_output_reuse() {
    let directory = Directory::new();
    let trace = lines(&fixture());
    let journal = lines(&journal());
    std::fs::write(directory.0.join("source-trace"), &trace).unwrap();
    std::fs::write(directory.0.join("source-journal"), &journal).unwrap();
    let run = |output: &str| {
        Command::new(env!("CARGO_BIN_EXE_weaver-ste-driver"))
            .current_dir(&directory.0)
            .args([
                "trace-report",
                "--trace",
                "source-trace",
                "--journal",
                "source-journal",
                "--output",
                output,
            ])
            .output()
            .unwrap()
    };
    assert!(run("report").status.success());
    let saved = directory.0.join("report");
    assert_eq!(std::fs::read(saved.join("trace.jsonl")).unwrap(), trace);
    assert_eq!(std::fs::read(saved.join("journal.jsonl")).unwrap(), journal);
    let report: Value =
        serde_json::from_slice(&std::fs::read(saved.join("trace-report.json")).unwrap()).unwrap();
    assert_eq!(report["complete"], true);
    assert_eq!(run("report").status.code(), Some(1));
    std::fs::write(directory.0.join("source-trace"), b"incomplete\n").unwrap();
    assert_eq!(run("incomplete").status.code(), Some(2));
    assert_eq!(std::fs::read(saved.join("trace.jsonl")).unwrap(), trace);
}

#[test]
fn trace_command_rejects_missing_repeated_and_foreign_options() {
    for (args, reason) in [
        (vec!["trace-report"], "missing --trace"),
        (vec!["trace-report", "--trace", "t"], "missing --journal"),
        (
            vec!["trace-report", "--trace", "t", "--journal", "j"],
            "missing --output",
        ),
        (
            vec![
                "trace-report",
                "--trace",
                "t",
                "--journal",
                "j",
                "--output",
                "o",
                "--trace",
                "u",
            ],
            "repeated command option",
        ),
        (
            vec!["trace-report", "--trace", "--help"],
            "missing command option value",
        ),
        (
            vec!["trace-report", "--config", "c"],
            "unknown command option",
        ),
        (vec!["evaluate", "--trace", "t"], "unknown command option"),
    ] {
        let result = Command::new(env!("CARGO_BIN_EXE_weaver-ste-driver"))
            .args(args)
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(1));
        assert_eq!(
            String::from_utf8(result.stderr).unwrap(),
            format!("STE driver: {reason}\n")
        );
        assert!(result.stdout.is_empty());
    }
}
