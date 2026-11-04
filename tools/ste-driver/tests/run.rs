use serde_json::{Value, json};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use weaver_ste_driver::{
    check::Checker,
    gate::{Capture, Transport},
    guard::Judge,
    lexicon::Lexicon,
    mask::Masker,
    run::{Kit, PROMPT_SHA256, R0Parameters, evaluate, prepare_text, verify_folded_prompt},
};

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("ste-run-{}-{nanos}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn kit(&self, rows: &[Value]) -> Kit {
        let path = self.0.join("kit.jsonl");
        std::fs::write(
            &path,
            rows.iter().map(|v| format!("{v}\n")).collect::<String>(),
        )
        .unwrap();
        Kit::read(&path).unwrap()
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

fn read(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

fn judge() -> Judge {
    let cases: Value = serde_json::from_str(include_str!("language-reference.json")).unwrap();
    let lex =
        Lexicon::build(serde_json::from_value(cases["lexicons"][0].clone()).unwrap()).unwrap();
    Judge::new(Checker::new(lex, 20, 25).unwrap(), &[]).unwrap()
}

struct Fixture {
    run: PathBuf,
    answers: VecDeque<Value>,
    sent: usize,
}
impl Transport for Fixture {
    fn send(&mut self, text: &str) -> Capture {
        let parameters = read(&self.run.join("run.json"));
        assert_eq!(parameters["driver_repair_pass_cap"], 2);
        assert_eq!(parameters["offer_pass_cap"], 1);
        assert_eq!(parameters["transport_stop_bound"], 0);
        let journal = std::fs::read_to_string(self.run.join("journal.jsonl")).unwrap();
        let record: Value = serde_json::from_str(journal.lines().last().unwrap()).unwrap();
        assert_eq!(record["kind"], "request");
        assert_eq!(record["request"]["text"], text);
        assert_eq!(record["prompt_sha256"], PROMPT_SHA256);
        self.sent += 1;
        Capture {
            stdout: serde_json::to_vec(&self.answers.pop_front().expect("no extra gate ask"))
                .unwrap(),
            stderr: Vec::new(),
            exit_code: Some(0),
            error: None,
        }
    }
}

fn answered(text: &str) -> Value {
    json!({"kind":"answered","run":"r","turn":"t","text":text})
}

#[test]
fn a_void_run_retains_attempts_and_never_publishes_scorer_output() {
    let directory = Directory::new();
    let kit = directory.kit(&[
        json!({"id":"a","kind":"narration","text":"Read [[C1]].","message":3,"field":"content"}),
        json!({"id":"b","kind":"narration","text":"Read the file."}),
        json!({"id":"c","kind":"final","text":"Read the file."}),
    ]);
    let run = directory.0.join("void");
    let mut transport = Fixture {
        run: run.clone(),
        answers: VecDeque::from([
            answered("Read [[C1]]\x3b"),
            json!({"kind":"stopped","reason":"unload"}),
        ]),
        sent: 0,
    };
    let report = evaluate(
        &kit,
        &R0Parameters::new(2, 1, 0).unwrap(),
        &judge(),
        &mut transport,
        &run,
    )
    .unwrap();
    assert_eq!(transport.sent, 2);
    assert!(report.void);
    assert_eq!(report.attempted_text_count, 1);
    assert_eq!(report.transport_stopped_count, 1);
    assert_eq!(report.conversion_loss_count, 0);
    assert_eq!(report.never_attempted, vec!["b", "c"]);
    assert!(!run.join("output.jsonl").exists());
    let attempt = read(&run.join("attempts.jsonl"));
    assert!(attempt["output"].is_null());
    assert_eq!(
        attempt["attempts"][0]["result"]["output"],
        "Read [[C1]]\x3b"
    );
    assert_eq!(attempt["segment"]["message"], 3);
    assert_eq!(attempt["segment"]["field"], "content");
    assert_eq!(
        read(&run.join("first-gate-answers.jsonl"))["output"],
        "Read [[C1]]\x3b"
    );
    assert_eq!(read(&run.join("summary.json"))["r0_score_allowed"], false);
    assert_eq!(read(&run.join("run.json"))["input_sha256"], kit.sha256());
    assert!(
        evaluate(
            &kit,
            &R0Parameters::new(2, 1, 0).unwrap(),
            &judge(),
            &mut transport,
            &run
        )
        .is_err()
    );
    assert_eq!(transport.sent, 2);
}

#[test]
fn a_complete_run_counts_both_loss_classes_and_keeps_the_last_outputs() {
    let directory = Directory::new();
    let kit = directory.kit(&[
        json!({"id":"shape","kind":"narration","text":"Read [[C1]]."}),
        json!({"id":"language","kind":"narration","text":"Read [[C1]]."}),
        json!({"id":"pass","kind":"final","text":"Read the file."}),
    ]);
    let run = directory.0.join("complete");
    let mut transport = Fixture {
        run: run.clone(),
        answers: VecDeque::from([
            answered("Read."),
            answered("Read."),
            answered("Read."),
            answered("Read [[C1]] [[C1]]."),
            answered("Read [[C1]] [[C1]]."),
            answered("Read [[C1]] [[C1]]."),
            answered("Read the file."),
        ]),
        sent: 0,
    };
    let report = evaluate(
        &kit,
        &R0Parameters::new(2, 1, 0).unwrap(),
        &judge(),
        &mut transport,
        &run,
    )
    .unwrap();
    assert_eq!(transport.sent, 7);
    assert_eq!(report.shape_drop_count, 1);
    assert_eq!(report.language_unresolved_count, 1);
    assert_eq!(report.accepted_count, 1);
    assert_eq!(report.attempted_text_count, 3);
    assert_eq!(report.conversion_loss_count, 2);
    assert!(report.r0_score_allowed());
    assert_eq!(
        read(&run.join("summary.json"))["kill_line_verdict"],
        Value::Null
    );
    let outputs = std::fs::read_to_string(run.join("output.jsonl")).unwrap();
    let rows: Vec<Value> = outputs
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(
        rows,
        vec![
            json!({"id":"shape","output":"Read."}),
            json!({"id":"language","output":"Read [[C1]] [[C1]]."}),
            json!({"id":"pass","output":"Read the file."})
        ]
    );
    assert!(!run.join("output.pending.jsonl").exists());
}

#[test]
fn kit_and_parameters_are_validated_before_the_run_can_start() {
    for parameters in [(0, 1, 0), (2, 0, 0), (2, 1, 1), (3, 1, 0)] {
        assert!(R0Parameters::new(parameters.0, parameters.1, parameters.2).is_err());
    }
    let directory = Directory::new();
    let file = directory.0.join("bad.jsonl");
    for bytes in [
        String::new(),
        "not JSON\n".into(),
        format!(
            "{}\n{}\n",
            json!({"id":"same","kind":"final","text":"x"}),
            json!({"id":"same","kind":"final","text":"x"})
        ),
        json!({"id":"bad","kind":"issue","text":"x"}).to_string(),
        json!({"id":"bad","kind":"final","text":"x".repeat(6001)}).to_string(),
    ] {
        std::fs::write(&file, bytes).unwrap();
        assert!(Kit::read(&file).is_err());
    }
}

#[test]
fn source_marker_collisions_refuse_before_masking_and_the_fold_is_pinned() {
    let masker = Masker::new().unwrap();
    for text in ["Read [[C1]].", "Literal [[Ctext is not a generated marker."] {
        assert!(
            prepare_text(&masker, text)
                .unwrap_err()
                .to_string()
                .contains("marker_collision")
        );
    }
    let masked = prepare_text(&masker, "Read `data`.").unwrap();
    assert_eq!(masked.text, "Read [[C1]].");
    assert_eq!(masked.codes, vec!["`data`"]);
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap();
    let path = repo.join("docs/crates/weaver-harness/Loops/ste-conversion-prompt.txt");
    verify_folded_prompt(&path).unwrap();
    let directory = Directory::new();
    let changed = directory.0.join("changed-prompt");
    let mut bytes = std::fs::read(path).unwrap();
    bytes.push(b' ');
    std::fs::write(&changed, bytes).unwrap();
    assert!(
        verify_folded_prompt(&changed)
            .unwrap_err()
            .to_string()
            .contains("hash mismatch")
    );
}
