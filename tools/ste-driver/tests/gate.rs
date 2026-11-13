use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use weaver_ste_driver::{
    check::Checker,
    conversion::check_answer,
    gate::{Capture, Transport, TurnClient},
    guard::Judge,
    lexicon::Lexicon,
    policy::AttemptResult,
    run::{Kit, R0Parameters, evaluate},
};

const DEADLINE: Duration = Duration::from_secs(30);

/// A temporary directory holding a stand-in checkout whose deploy/turn.py is the
/// given Python, and an admin root the stand-in reads as WEAVER_ADMIN_CONFIG.
struct FakeClient(PathBuf);
impl FakeClient {
    fn new(name: &str, script: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("ste-{name}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(root.join("repo/deploy")).unwrap();
        std::fs::create_dir_all(root.join("admin")).unwrap();
        std::fs::write(root.join("repo/deploy/turn.py"), script).unwrap();
        Self(root)
    }
    fn client(&self, deadline: Duration) -> TurnClient {
        TurnClient::new(
            &self.0.join("repo"),
            "ste-fixture",
            &self.0.join("admin"),
            deadline,
        )
        .unwrap()
    }
}
impl Drop for FakeClient {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

fn judge() -> Judge {
    let cases: Value = serde_json::from_str(include_str!("language-reference.json")).unwrap();
    let lex =
        Lexicon::build(serde_json::from_value(cases["lexicons"][0].clone()).unwrap()).unwrap();
    Judge::new(Checker::new(lex, 20, 25).unwrap(), &[]).unwrap()
}

const SLEEPER: &str = r#"import os, sys, time
open(os.path.join(os.environ["WEAVER_ADMIN_CONFIG"], "pid"), "w").write(str(os.getpid()))
sys.stdout.write("partial")
sys.stdout.flush()
sys.stderr.write("waiting for the gate\n")
sys.stderr.flush()
time.sleep(30)
"#;

fn capture(value: serde_json::Value) -> Capture {
    Capture {
        stdout: serde_json::to_vec(&value).unwrap(),
        stderr: b"retained diagnostic".to_vec(),
        exit_code: Some(0),
        error: None,
    }
}

#[test]
fn completed_answers_keep_the_exact_text_and_finish() {
    for finish in [None, Some("length")] {
        let mut value = json!({"kind":"answered","run":"r-1","turn":"t-2",
            "text":"  Read [[C1]].\r\n"});
        if let Some(finish) = finish {
            value["finish"] = json!(finish);
        }
        let capture = capture(value);
        let answer = capture.answer().unwrap();
        assert_eq!(answer.run, "r-1");
        assert_eq!(answer.turn, "t-2");
        assert_eq!(answer.text.as_deref(), Some("  Read [[C1]].\r\n"));
        assert_eq!(answer.truncated, finish.is_some());
        assert_eq!(capture.to_json()["stderr_bytes"][0], b'r');
    }
}

#[test]
fn only_a_completed_answer_can_have_a_missing_output_shape_fault() {
    for text in [serde_json::Value::Null, json!(7), json!("")] {
        let capture = capture(json!({"kind":"answered","run":"r","turn":"t","text":text}));
        let answer = capture.answer().unwrap();
        assert!(answer.text.is_none() || answer.text.as_deref() == Some(""));
    }
    for value in [
        json!({"kind":"stopped","run":"r","turn":"t","reason":"unload"}),
        json!({"kind":"refused","reason":"not ready"}),
        json!({"kind":"answered","text":"answer"}),
        json!({"kind":"answered","run":"r","turn":"","text":"answer"}),
        json!({"kind":"answered","run":"r","turn":"t","finish":"stop"}),
        json!(null),
    ] {
        assert!(capture(value).answer().is_err());
    }
    let mut failure = capture(json!({"kind":"answered","run":"r","turn":"t","text":"x"}));
    failure.exit_code = Some(1);
    assert!(failure.answer().is_err());
    failure.exit_code = Some(0);
    failure.error = Some("start failed".into());
    assert!(failure.answer().is_err());
    failure.error = None;
    failure.stdout = b"not JSON".to_vec();
    assert!(failure.answer().is_err());
}

#[test]
fn the_real_turn_client_sends_one_user_block_and_keeps_raw_evidence() {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("ste-gate-{}-{nanos}", std::process::id()));
    let agent = "ste-fixture";
    let admin = root.join("admin");
    let coordination = root.join("coord");
    std::fs::create_dir_all(admin.join(agent)).unwrap();
    std::fs::create_dir_all(coordination.join(format!("weaver-{agent}"))).unwrap();
    std::fs::write(
        admin.join(agent).join("coordination-root"),
        coordination.to_str().unwrap(),
    )
    .unwrap();
    let socket = coordination.join(format!("weaver-{agent}/gate.sock"));
    let listener = UnixListener::bind(&socket).unwrap();
    listener.set_nonblocking(true).unwrap();
    let request = "<<<TEXT\nRead `data` and $(literal).\nTEXT>>>";
    let response =
        b"{\"kind\":\"answered\",\"run\":\"r\",\"turn\":\"t\",\"text\":\"Read the file.\"}\n";
    let server = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let (mut peer, _) = loop {
            match listener.accept() {
                Ok(peer) => break peer,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(
                        std::time::Instant::now() < deadline,
                        "client did not connect"
                    );
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                Err(error) => panic!("accept: {error}"),
            }
        };
        peer.set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut line = String::new();
        BufReader::new(peer.try_clone().unwrap())
            .read_line(&mut line)
            .unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&line).unwrap(),
            json!({"text":request})
        );
        peer.write_all(response).unwrap();
    });
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap();
    let mut client = TurnClient::new(repo, agent, &admin, DEADLINE).unwrap();
    let result = client.send(request);
    server.join().unwrap();
    assert_eq!(result.stdout, response);
    assert_eq!(
        result.answer().unwrap().text.as_deref(),
        Some("Read the file.")
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn the_real_client_retains_its_preconnection_failure() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap();
    let missing = std::env::temp_dir().join(format!("ste-no-config-{}", std::process::id()));
    assert!(!missing.exists());
    let mut client = TurnClient::new(repo, "ste-fixture", &missing, DEADLINE).unwrap();
    let result = client.send("<<<TEXT\nRead the file.\nTEXT>>>");
    assert_eq!(result.exit_code, Some(1));
    assert!(result.stdout.is_empty());
    assert!(String::from_utf8_lossy(&result.stderr).contains("cannot read"));
    assert!(result.answer().is_err());
}

#[test]
fn a_client_past_its_deadline_is_killed_reaped_and_recorded_as_a_transport_stop() {
    let fake = FakeClient::new("sleeper", SLEEPER);
    let started = Instant::now();
    let capture = fake
        .client(Duration::from_secs(1))
        .send("<<<TEXT\nRead the file.\nTEXT>>>");
    let elapsed = started.elapsed();
    assert!(elapsed >= Duration::from_secs(1), "{elapsed:?}");
    assert!(
        elapsed < Duration::from_secs(10),
        "the client was not ended: {elapsed:?}"
    );
    assert_eq!(
        capture.error.as_deref(),
        Some("gate client passed the gate-call deadline of 1 s and was killed")
    );
    assert_eq!(capture.exit_code, None);
    assert_eq!(capture.stdout, b"partial");
    assert_eq!(capture.stderr, b"waiting for the gate\n");
    let pid = std::fs::read_to_string(fake.0.join("admin/pid")).unwrap();
    assert!(
        !Path::new("/proc").join(&pid).exists(),
        "the client {pid} is still a process"
    );
    assert_eq!(capture.to_json()["error"], json!(capture.error));
    match check_answer(capture, &judge(), "Read the file.").unwrap() {
        AttemptResult::TransportStopped { detail, .. } => assert!(detail.contains("deadline")),
        other => panic!("not a transport stop: {other:?}"),
    }

    // Through the evaluation: the request is journaled first, the stop voids the
    // round at R0's zero bound, and no scorer output is published.
    let kit_path = fake.0.join("kit.jsonl");
    std::fs::write(
        &kit_path,
        "{\"id\":\"a\",\"kind\":\"narration\",\"text\":\"Read the file.\"}\n\
         {\"id\":\"b\",\"kind\":\"narration\",\"text\":\"Read the file.\"}\n",
    )
    .unwrap();
    let run = fake.0.join("run");
    let report = evaluate(
        &Kit::read(&kit_path).unwrap(),
        &R0Parameters::new(2, 1, 0).unwrap(),
        &judge(),
        &mut fake.client(Duration::from_secs(1)),
        &run,
    )
    .unwrap();
    assert!(report.void);
    assert_eq!(report.transport_stopped_count, 1);
    assert_eq!(report.never_attempted, vec!["b"]);
    assert!(!run.join("output.jsonl").exists());
    let journal = std::fs::read_to_string(run.join("journal.jsonl")).unwrap();
    let kinds: Vec<Value> = journal
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap()["kind"].clone())
        .collect();
    assert_eq!(
        kinds,
        [
            json!("request"),
            json!("response"),
            json!("checked_attempt")
        ]
    );
    let response: Value = serde_json::from_str(journal.lines().nth(1).unwrap()).unwrap();
    assert!(
        response["transport"]["error"]
            .as_str()
            .unwrap()
            .contains("gate-call deadline of 1 s")
    );
}

#[test]
fn a_client_that_writes_past_a_pipe_buffer_on_both_streams_is_not_blocked() {
    let fake = FakeClient::new(
        "flood",
        r#"import json, sys
sys.stderr.write("e" * 262144)
sys.stderr.flush()
sys.stdout.write(json.dumps({"kind": "answered", "run": "r", "turn": "t", "text": "a" * 131072}))
"#,
    );
    let started = Instant::now();
    let capture = fake
        .client(Duration::from_secs(5))
        .send("<<<TEXT\nRead the file.\nTEXT>>>");
    assert!(started.elapsed() < Duration::from_secs(5));
    assert_eq!(capture.error, None);
    assert_eq!(capture.exit_code, Some(0));
    assert_eq!(capture.stderr.len(), 262144);
    assert_eq!(capture.answer().unwrap().text.unwrap().len(), 131072);
}

#[test]
fn a_client_that_ends_at_once_is_recorded_as_before() {
    let fake = FakeClient::new(
        "prompt",
        r#"import sys
print('{"kind":"answered","run":"r","turn":"t","text":"Read the file."}', end="")
sys.stderr.write("diagnostic")
"#,
    );
    let started = Instant::now();
    let capture = fake
        .client(DEADLINE)
        .send("<<<TEXT\nRead the file.\nTEXT>>>");
    assert!(started.elapsed() < Duration::from_secs(5));
    assert_eq!(
        capture.stdout,
        b"{\"kind\":\"answered\",\"run\":\"r\",\"turn\":\"t\",\"text\":\"Read the file.\"}"
    );
    assert_eq!(capture.stderr, b"diagnostic");
    assert_eq!(capture.exit_code, Some(0));
    assert_eq!(capture.error, None);
    assert_eq!(
        capture.answer().unwrap().text.as_deref(),
        Some("Read the file.")
    );

    // The client's own failure is its exit status, not a deadline.
    let failing = FakeClient::new("failing", "import sys\nsys.exit(3)\n");
    let capture = failing
        .client(DEADLINE)
        .send("<<<TEXT\nRead the file.\nTEXT>>>");
    assert_eq!(capture.exit_code, Some(3));
    assert_eq!(capture.error, None);
    assert!(capture.stdout.is_empty());
    assert!(capture.answer().is_err());
}

#[test]
fn the_client_refuses_a_zero_or_unmeasurable_deadline() {
    let fake = FakeClient::new("refusal", "raise RuntimeError('must not run')\n");
    let repo = fake.0.join("repo");
    let admin = fake.0.join("admin");
    for (deadline, reason) in [
        (
            Duration::ZERO,
            "the gate-call deadline must be at least one second",
        ),
        (
            Duration::MAX,
            "the gate-call deadline is too large to measure",
        ),
    ] {
        match TurnClient::new(&repo, "ste-fixture", &admin, deadline) {
            Err(error) => assert_eq!(error, reason),
            Ok(_) => panic!("{deadline:?} was accepted"),
        }
    }
    assert!(TurnClient::new(&repo, "ste-fixture", &admin, Duration::from_secs(1)).is_ok());
}
