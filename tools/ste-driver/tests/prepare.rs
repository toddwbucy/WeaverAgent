use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Command;
use weaver_ste_driver::{mask::unmask, prepare::prepare, run::Kit};

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("ste-prepare-{}-{stamp}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn row(id: &str, raw: &str) -> Value {
    json!({"id":id,"kind":"narration","uuid":format!("uuid-{id}"),
        "loc":{"msg":7,"field":"content"},"raw":raw,"from":"k019",
        "dataset_line":42})
}

fn source(directory: &Directory, rows: &[Value]) -> (PathBuf, Vec<u8>) {
    let mut bytes = b"\n".to_vec();
    for row in rows {
        serde_json::to_writer(&mut bytes, row).unwrap();
        bytes.extend_from_slice(b"\n\n");
    }
    let path = directory.0.join("source.jsonl");
    std::fs::write(&path, &bytes).unwrap();
    (path, bytes)
}

fn lines(path: &Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn preparation_preserves_source_order_locators_and_exact_nested_codes() {
    let directory = Directory::new();
    let raw = "  Read `file_name.py` and https://example.com/file.txt.\r\n";
    let mut final_row = row("first", "> Keep `this` unchanged.\n\nRead the file.");
    final_row["kind"] = json!("final");
    final_row["loc"] = json!({"msg":9,"field":"finish","call":0});
    let rows = [row("second", raw), final_row];
    let (path, bytes) = source(&directory, &rows);
    let output = directory.0.join("prepared");
    let report = prepare(&path, &hash(&bytes), &output).unwrap();
    assert_eq!(std::fs::read(output.join("source.jsonl")).unwrap(), bytes);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    let kit = Kit::read(&output.join("input.jsonl")).unwrap();
    assert_eq!(kit.segments().len(), 2);
    assert_eq!(kit.segments()[0].id, "second");
    assert_eq!(kit.segments()[1].id, "first");
    let codes = lines(&output.join("codes.jsonl"));
    for (index, segment) in kit.segments().iter().enumerate() {
        assert_eq!(segment.locators["uuid"], rows[index]["uuid"]);
        assert_eq!(segment.locators["loc"], rows[index]["loc"]);
        assert_eq!(segment.locators["from"], rows[index]["from"]);
        assert_eq!(segment.locators["dataset_line"], 42);
        assert!(!segment.locators.contains_key("raw"));
        assert_eq!(codes[index]["id"], segment.id);
        let spans: Vec<String> = serde_json::from_value(codes[index]["codes"].clone()).unwrap();
        assert_eq!(
            unmask(&segment.text, &spans),
            rows[index]["raw"].as_str().unwrap()
        );
    }
    assert_eq!(kit.segments()[0].text, "  Read [[C1]] and [[C2]]\r\n");
    assert_eq!(
        codes[0]["codes"],
        json!(["`file_name.py`", "https://example.com/file.txt."])
    );
    assert_eq!(report["segments"], 2);
    assert_eq!(report["kinds"], json!({"narration":1,"final":1}));
    assert_eq!(report["source_sha256"], hash(&bytes));
    assert_eq!(report["input_sha256"], kit.sha256());
    assert_eq!(
        report["codes_sha256"],
        hash(&std::fs::read(output.join("codes.jsonl")).unwrap())
    );
    assert_eq!(report["qualification"], "not assessed");
    assert_eq!(lines(&output.join("preparation.json")), vec![report]);
}

#[test]
fn preparation_measures_masked_character_bins_without_changing_the_roster() {
    let directory = Directory::new();
    let rows: Vec<_> = [0, 199, 200, 499, 500, 6000]
        .into_iter()
        .enumerate()
        .map(|(i, n)| row(&i.to_string(), &"\u{e9}".repeat(n)))
        .collect();
    let (path, bytes) = source(&directory, &rows);
    let report = prepare(&path, &hash(&bytes), &directory.0.join("out")).unwrap();
    assert_eq!(report["masked_character_bins"], json!([2, 2, 2]));
    assert_eq!(report["max_masked_characters"], 6000);
}

#[test]
fn preparation_limits_the_masked_text_and_resolves_relative_output_from_cwd() {
    let directory = Directory::new();
    let raw = format!("`{}`", "x".repeat(7000));
    let (source, bytes) = source(&directory, &[row("a", &raw)]);
    let config_dir = directory.0.join("config");
    std::fs::create_dir(&config_dir).unwrap();
    let config = config_dir.join("prepare.json");
    std::fs::write(
        &config,
        json!({
            "source":source,"source_sha256":hash(&bytes)
        })
        .to_string(),
    )
    .unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_weaver-ste-driver"))
        .args(["prepare-kit", "--config"])
        .arg(&config)
        .args(["--output", "out"])
        .current_dir(&directory.0)
        .output()
        .unwrap();
    assert!(result.status.success(), "{result:?}");
    assert!(!config_dir.join("out").exists());
    let out = directory.0.join("out");
    let report: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["max_masked_characters"], 6);
    assert_eq!(report["masked_character_bins"], json!([1, 0, 0]));
    assert_eq!(
        Kit::read(&out.join("input.jsonl")).unwrap().segments()[0].text,
        "[[C1]]"
    );
    assert_eq!(lines(&out.join("codes.jsonl"))[0]["codes"], json!([raw]));
}

#[test]
fn preparation_refuses_bad_identity_and_source_shapes_before_creating_output() {
    let directory = Directory::new();
    let cases = [
        ("id", json!(""), "empty or repeated segment id"),
        (
            "uuid",
            json!(""),
            "empty trajectory uuid or repeated segment locator",
        ),
        ("kind", json!("think"), "ineligible segment kind"),
        (
            "loc",
            json!({"msg":1,"field":"finish","call":0}),
            "invalid segment locator",
        ),
        (
            "loc",
            json!({"msg":-1,"field":"content"}),
            "invalid segment locator",
        ),
        ("loc", json!({"field":"content"}), "invalid segment locator"),
        ("loc", json!(null), "invalid segment locator"),
        (
            "text",
            json!("shadow input"),
            "source contains reserved text field",
        ),
        ("raw", json!("Use [[C1]]."), "marker_collision"),
        (
            "raw",
            json!("x".repeat(6001)),
            "masked segment exceeds 6000 characters",
        ),
    ];
    for (key, value, reason) in cases {
        let mut record = row("a", "Read.");
        record[key] = value;
        let (path, bytes) = source(&directory, &[record]);
        let out = directory.0.join("out");
        assert_eq!(
            prepare(&path, &hash(&bytes), &out).unwrap_err().to_string(),
            reason
        );
        assert!(!out.exists());
    }
    for key in ["id", "uuid", "kind", "raw", "loc"] {
        let mut record = row("a", "Read.");
        record.as_object_mut().unwrap().remove(key);
        let (path, bytes) = source(&directory, &[record]);
        let out = directory.0.join("out");
        assert!(
            prepare(&path, &hash(&bytes), &out)
                .unwrap_err()
                .to_string()
                .contains("missing field")
        );
        assert!(!out.exists());
    }
}

#[test]
fn preparation_refuses_repeated_ids_and_trajectories_independently() {
    let directory = Directory::new();
    for (field, reason) in [
        ("id", "empty or repeated segment id"),
        ("uuid", "empty trajectory uuid or repeated segment locator"),
    ] {
        let first = row("a", "Read.");
        let mut second = row("b", "Stop.");
        second[field] = first[field].clone();
        let (path, bytes) = source(&directory, &[first, second]);
        let out = directory.0.join("out");
        assert_eq!(
            prepare(&path, &hash(&bytes), &out).unwrap_err().to_string(),
            reason
        );
        assert!(!out.exists());
    }
}

// One trajectory's segments share its UUID at different locators.
#[test]
fn preparation_accepts_one_trajectory_at_several_locators() {
    let directory = Directory::new();
    let first = row("a", "Read.");
    let mut second = row("b", "Stop.");
    second["uuid"] = first["uuid"].clone();
    second["loc"] = json!({"msg":9,"field":"content"});
    let (path, bytes) = source(&directory, &[first, second]);
    let out = directory.0.join("out");
    let report = prepare(&path, &hash(&bytes), &out).unwrap();
    assert_eq!(report["segments"], 2);
}

#[test]
fn final_locator_requires_a_call_index() {
    let directory = Directory::new();
    for locator in [
        json!({"msg":1,"field":"finish","call":null}),
        json!({"msg":1,"field":"finish","call":-1}),
        json!({"msg":1,"field":"finish","call":"0"}),
        json!({"msg":1,"field":"finish"}),
        json!({"msg":1,"field":"content","call":0}),
    ] {
        let mut record = row("a", "Done.");
        record["kind"] = json!("final");
        record["loc"] = locator;
        let (path, bytes) = source(&directory, &[record]);
        let out = directory.0.join("out");
        assert_eq!(
            prepare(&path, &hash(&bytes), &out).unwrap_err().to_string(),
            "invalid segment locator"
        );
        assert!(!out.exists());
    }
}

#[test]
fn preparation_refuses_bad_bytes_digest_and_empty_source_before_output() {
    let directory = Directory::new();
    let (path, bytes) = source(&directory, &[row("a", "Read.")]);
    let out = directory.0.join("out");
    assert_eq!(
        prepare(&path, &"0".repeat(64), &out)
            .unwrap_err()
            .to_string(),
        "frozen source hash mismatch"
    );
    assert!(!out.exists());
    for (bad, reason) in [
        (b"\xff".as_slice(), "invalid utf-8"),
        (b"not JSON", "expected ident"),
        (b"\n \n", "empty source kit"),
    ] {
        std::fs::write(&path, bad).unwrap();
        let error = prepare(&path, &hash(bad), &out).unwrap_err().to_string();
        assert!(error.contains(reason), "{error}");
        assert!(!out.exists());
    }
    std::fs::write(&path, &bytes).unwrap();
    std::fs::create_dir(&out).unwrap();
    std::fs::write(out.join("keep"), b"untouched").unwrap();
    assert!(prepare(&path, &hash(&bytes), &out).is_err());
    assert_eq!(std::fs::read(out.join("keep")).unwrap(), b"untouched");
    assert_eq!(std::fs::read_dir(&out).unwrap().count(), 1);
}

#[test]
fn preparation_command_resolves_source_from_config_without_runtime_inputs() {
    let directory = Directory::new();
    let (source, bytes) = source(&directory, &[row("a", "Read `file.rs`.")]);
    let config = directory.0.join("prepare.json");
    std::fs::write(
        &config,
        serde_json::to_vec(&json!({
            "source":source.file_name().unwrap().to_str().unwrap(),"source_sha256":hash(&bytes)
        }))
        .unwrap(),
    )
    .unwrap();
    let out = directory.0.join("out");
    let result = Command::new(env!("CARGO_BIN_EXE_weaver-ste-driver"))
        .args(["prepare-kit", "--config"])
        .arg(&config)
        .arg("--output")
        .arg(&out)
        .current_dir("/")
        .output()
        .unwrap();
    assert!(result.status.success(), "{result:?}");
    let report: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(
        report["input_sha256"],
        Kit::read(&out.join("input.jsonl")).unwrap().sha256()
    );
}

#[test]
fn preparation_command_refuses_invalid_config_without_output() {
    let directory = Directory::new();
    let (source, bytes) = source(&directory, &[row("a", "Read.")]);
    let valid = json!({"source":source,"source_sha256":hash(&bytes)});
    let config = directory.0.join("prepare.json");
    let out = directory.0.join("out");
    let mut cases = Vec::new();
    for key in ["source", "source_sha256"] {
        let mut missing = valid.clone();
        missing.as_object_mut().unwrap().remove(key);
        cases.push((missing.to_string(), "missing field"));
        let mut null = valid.clone();
        null[key] = Value::Null;
        cases.push((null.to_string(), "invalid type"));
        let mut repeated = valid.to_string();
        repeated.pop();
        repeated.push_str(&format!(",{key:?}:null}}"));
        cases.push((repeated, "duplicate field"));
    }
    let mut unknown = valid.clone();
    unknown["agent"] = json!("not-used");
    cases.push((unknown.to_string(), "unknown field"));
    let mut empty = valid.clone();
    empty["source"] = json!("");
    cases.push((empty.to_string(), "source path is empty"));
    let mut mismatch = valid;
    mismatch["source_sha256"] = json!("wrong");
    cases.push((mismatch.to_string(), "frozen source hash mismatch"));
    for (body, reason) in cases {
        std::fs::write(&config, body).unwrap();
        let result = Command::new(env!("CARGO_BIN_EXE_weaver-ste-driver"))
            .args(["prepare-kit", "--config"])
            .arg(&config)
            .arg("--output")
            .arg(&out)
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(1));
        assert!(
            String::from_utf8_lossy(&result.stderr).contains(reason),
            "{result:?}"
        );
        assert!(!out.exists());
    }
}

#[test]
fn preparation_command_refuses_missing_repeated_and_foreign_options() {
    let cases: &[(&[&str], &str)] = &[
        (&[], "missing --config"),
        (&["--config", "config.json"], "missing --output"),
        (&["--config"], "missing command option value"),
        (&["--config", ""], "missing command option value"),
        (&["--config", "--output"], "missing command option value"),
        (
            &["--config", "a", "--config", "b"],
            "repeated command option",
        ),
        (
            &["--output", "a", "--output", "b"],
            "repeated command option",
        ),
        (&["--trace", "trace.jsonl"], "unknown command option"),
        (&["--journal", "journal.jsonl"], "unknown command option"),
    ];
    for (args, reason) in cases {
        let result = Command::new(env!("CARGO_BIN_EXE_weaver-ste-driver"))
            .arg("prepare-kit")
            .args(*args)
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(1));
        assert!(
            String::from_utf8_lossy(&result.stderr).contains(reason),
            "{result:?}"
        );
    }
}
