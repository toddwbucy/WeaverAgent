use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("ste-command-{}-{stamp}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

fn repo() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
}

fn invoke(config: &Path, output: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_weaver-ste-driver"))
        .args(["evaluate", "--config"])
        .arg(config)
        .arg("--output")
        .arg(output)
        .output()
        .unwrap()
}

fn fixture(directory: &Directory) -> (Value, PathBuf) {
    let kit = directory.0.join("kit.jsonl");
    let bytes = b"{\"id\":\"a\",\"kind\":\"narration\",\"text\":\"Read [[C1]].\"}\n";
    std::fs::write(&kit, bytes).unwrap();
    let config = json!({
        "repo":repo(), "agent":"ste-fixture", "config_root":directory.0.join("admin"),
        "train_root":directory.0.join("train"),
        "approved_words":directory.0.join("words.json"),
        "prompt_bundle":directory.0.join("bundle.json"),
        "kit":kit, "kit_sha256":format!("{:x}",Sha256::digest(bytes)),
        "driver_repair_pass_cap":2, "offer_pass_cap":1, "transport_stop_bound":0,
        "gate_call_deadline_seconds":600
    });
    (config, directory.0.join("config.json"))
}

fn refused(directory: &Directory, config: &Value, path: &Path, reason: &str) {
    std::fs::write(path, serde_json::to_vec(config).unwrap()).unwrap();
    let output = directory.0.join("result");
    let result = invoke(path, &output);
    assert_eq!(result.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&result.stderr).contains(reason),
        "{result:?}"
    );
    assert!(!output.exists(), "preflight must not create a run");
}

#[test]
fn command_help_and_argument_refusals_do_not_need_runtime_files() {
    let binary = env!("CARGO_BIN_EXE_weaver-ste-driver");
    let help = Command::new(binary).arg("--help").output().unwrap();
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("evaluate --config"));
    for args in [
        vec![],
        vec!["seed"],
        vec!["evaluate"],
        vec!["evaluate", "--config", "a", "--config", "b"],
        vec!["evaluate", "--config", "a", "--output"],
        vec![
            "evaluate",
            "--config",
            "a",
            "--output",
            "b",
            "--unknown",
            "x",
        ],
    ] {
        assert_eq!(
            Command::new(binary)
                .args(args)
                .output()
                .unwrap()
                .status
                .code(),
            Some(1)
        );
    }
}

#[test]
fn command_argument_guards_report_their_own_refusal_before_file_access() {
    let directory = Directory::new();
    let cases = [
        (
            vec![
                "evaluate", "--config", "a", "--output", "b", "--config", "c",
            ],
            "repeated command option",
        ),
        (
            vec![
                "evaluate", "--config", "a", "--output", "b", "--output", "c",
            ],
            "repeated command option",
        ),
        (
            vec!["evaluate", "--config", "--output", "run"],
            "missing command option value",
        ),
        (
            vec!["evaluate", "--config", "a", "--output", "--help"],
            "missing command option value",
        ),
    ];
    for (args, reason) in cases {
        let result = Command::new(env!("CARGO_BIN_EXE_weaver-ste-driver"))
            .args(args)
            .current_dir(&directory.0)
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(1));
        assert_eq!(
            String::from_utf8(result.stderr).unwrap(),
            format!("STE driver: {reason}\n")
        );
        assert!(result.stdout.is_empty());
        assert_eq!(std::fs::read_dir(&directory.0).unwrap().count(), 0);
    }
}

#[test]
fn command_requires_every_field_and_the_exact_r0_elections() {
    let directory = Directory::new();
    let (base, path) = fixture(&directory);
    for key in base.as_object().unwrap().keys() {
        let mut changed = base.clone();
        changed.as_object_mut().unwrap().remove(key);
        refused(&directory, &changed, &path, "missing field");
        changed = base.clone();
        changed[key] = Value::Null;
        refused(&directory, &changed, &path, "invalid type");
    }
    for (key, value) in [
        ("driver_repair_pass_cap", 3),
        ("offer_pass_cap", 0),
        ("transport_stop_bound", 1),
    ] {
        let mut changed = base.clone();
        changed[key] = json!(value);
        refused(&directory, &changed, &path, "R0 requires");
    }
    for key in [
        "repo",
        "config_root",
        "train_root",
        "approved_words",
        "prompt_bundle",
        "kit",
    ] {
        let mut changed = base.clone();
        changed[key] = json!("");
        refused(&directory, &changed, &path, "an input path is empty");
    }
    for (value, reason) in [
        (
            json!(0),
            "the gate-call deadline must be at least one second",
        ),
        (
            json!(u64::MAX),
            "the gate-call deadline is too large to measure",
        ),
        (json!(-1), "invalid value"),
        (json!(1.5), "invalid type"),
        (
            json!("REPLACE_WITH_THE_ELECTED_WHOLE_SECONDS"),
            "invalid type",
        ),
    ] {
        let mut changed = base.clone();
        changed["gate_call_deadline_seconds"] = value;
        refused(&directory, &changed, &path, reason);
    }
    // The deadline is refused with the other elections, before any path is read.
    let mut changed = base.clone();
    changed["gate_call_deadline_seconds"] = json!(0);
    changed["repo"] = json!("");
    refused(
        &directory,
        &changed,
        &path,
        "the gate-call deadline must be at least one second",
    );
    let mut changed = base.clone();
    changed["extra"] = json!(true);
    refused(&directory, &changed, &path, "unknown field");
    let bytes = serde_json::to_string(&base).unwrap();
    let duplicate = format!("{{\"offer_pass_cap\":1,{}", &bytes[1..]);
    std::fs::write(&path, duplicate).unwrap();
    let result = invoke(&path, &directory.0.join("result"));
    assert!(String::from_utf8_lossy(&result.stderr).contains("duplicate field"));
    assert!(!directory.0.join("result").exists());
}

#[test]
fn command_refuses_changed_kit_fold_and_bundle_before_a_gate_call() {
    let directory = Directory::new();
    let (base, path) = fixture(&directory);
    let mut changed = base.clone();
    changed["kit_sha256"] = json!("0".repeat(64));
    refused(&directory, &changed, &path, "frozen kit hash mismatch");
    changed = base.clone();
    changed["agent"] = json!("../invalid");
    refused(&directory, &changed, &path, "invalid agent name");
    let fake_repo = directory.0.join("repo");
    std::fs::create_dir_all(fake_repo.join("deploy")).unwrap();
    std::fs::write(
        fake_repo.join("deploy/turn.py"),
        "raise RuntimeError('must not run')",
    )
    .unwrap();
    let fold = fake_repo.join("docs/crates/weaver-harness/Loops/ste-conversion-prompt.txt");
    std::fs::create_dir_all(fold.parent().unwrap()).unwrap();
    std::fs::write(fold, "wrong prompt").unwrap();
    changed = base.clone();
    changed["repo"] = json!(fake_repo);
    refused(&directory, &changed, &path, "folded prompt hash mismatch");
    std::fs::write(directory.0.join("bundle.json"), "{}").unwrap();
    refused(&directory, &base, &path, "missing source system text");
}

/// The files are operator-provided pinned data, never silently substituted.
#[test]
#[ignore = "requires STE_TEST_TRAIN_ROOT, STE_TEST_APPROVED_WORDS and STE_TEST_PROMPT_BUNDLE"]
fn command_runs_the_pinned_checker_through_a_synthetic_gate() {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixListener;
    let directory = Directory::new();
    let (mut config, path) = fixture(&directory);
    config["kit"] = json!("kit.jsonl");
    config["config_root"] = json!("admin");
    for (field, variable) in [
        ("train_root", "STE_TEST_TRAIN_ROOT"),
        ("approved_words", "STE_TEST_APPROVED_WORDS"),
        ("prompt_bundle", "STE_TEST_PROMPT_BUNDLE"),
    ] {
        config[field] = json!(std::env::var(variable).expect(variable));
    }
    let coord = directory.0.join("coord");
    std::fs::create_dir_all(coord.join("weaver-ste-fixture")).unwrap();
    let admin = directory.0.join("admin/ste-fixture");
    std::fs::create_dir_all(&admin).unwrap();
    std::fs::write(admin.join("coordination-root"), coord.to_str().unwrap()).unwrap();
    let config_bytes = serde_json::to_vec(&config).unwrap();
    let config_sha256 = format!("{:x}", Sha256::digest(&config_bytes));
    std::fs::write(&path, config_bytes).unwrap();
    let listener = UnixListener::bind(coord.join("weaver-ste-fixture/gate.sock")).unwrap();
    listener.set_nonblocking(true).unwrap();
    let output = directory.0.join("pass-1");
    let run = output.clone();
    let server = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        let (mut peer, _) = loop {
            match listener.accept() {
                Ok(peer) => break peer,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(std::time::Instant::now() < deadline, "no gate request");
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                Err(error) => panic!("{error}"),
            }
        };
        peer.set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let record: Value =
            serde_json::from_slice(&std::fs::read(run.join("run.json")).unwrap()).unwrap();
        assert_eq!(
            record["provenance"]["source_system_sha256"],
            weaver_ste_driver::prompt::SYSTEM_SHA256
        );
        assert_eq!(record["provenance"]["config"]["offer_pass_cap"], 1);
        assert_eq!(record["provenance"]["config_sha256"], config_sha256);
        assert_eq!(
            record["provenance"]["source_bundle_sha256"],
            weaver_ste_driver::prompt::BUNDLE_SHA256
        );
        assert_eq!(
            record["provenance"]["approved_words_sha256"],
            weaver_ste_driver::source::APPROVED_WORDS_SHA256
        );
        let mut line = String::new();
        BufReader::new(peer.try_clone().unwrap())
            .read_line(&mut line)
            .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&line).unwrap(),
            json!({"text":"<<<TEXT\nRead [[C1]].\nTEXT>>>"})
        );
        peer.write_all(b"{\"kind\":\"answered\",\"run\":\"fixture\",\"turn\":\"one\",\"text\":\"Read [[C1]].\"}\n").unwrap();
    });
    let result = invoke(&path, &output);
    assert!(server.join().is_ok(), "gate fixture failed: {result:?}");
    assert!(result.status.success(), "{result:?}");
    let summary: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(summary["accepted_count"], 1);
    assert_eq!(summary["void"], false);
    assert!(output.join("output.jsonl").is_file());
    assert!(!invoke(&path, &output).status.success());
    let void = directory.0.join("pass-2");
    // The closed fixture socket causes a transport stop. No live agent is used.
    let stopped = invoke(&path, &void);
    assert_eq!(stopped.status.code(), Some(2));
    let summary: Value = serde_json::from_slice(&stopped.stdout).unwrap();
    assert_eq!(summary["void"], true);
    assert_eq!(summary["transport_stopped_count"], 1);
    assert!(!void.join("output.jsonl").exists());
    assert!(void.join("summary.json").is_file());
}
