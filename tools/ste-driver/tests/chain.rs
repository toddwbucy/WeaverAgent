use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use weaver_ste_driver::chain::{self, Manifest};
use weaver_ste_driver::run::Kit;

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("ste-chain-{}-{stamp}", std::process::id()));
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

fn segment(id: &str, uuid: &str) -> String {
    json!({"id":id,"kind":"narration","text":"Read [[C1]].","uuid":uuid,"msg":1,"field":"content"})
        .to_string()
}

/// Five trajectories of 2, 1, 3, 1 and 2 segments, with a leading blank line, a blank
/// line inside a trajectory, a blank line between two, and one CRLF line.
fn kit_bytes() -> Vec<u8> {
    let mut text = String::from("\n");
    for (id, uuid) in [("a1", "A"), ("a2", "A"), ("b1", "B")] {
        text += &segment(id, uuid);
        text += "\n";
    }
    text += &segment("c1", "C");
    text += "\r\n";
    text += "\n";
    for (id, uuid) in [("c2", "C"), ("c3", "C"), ("d1", "D")] {
        text += &segment(id, uuid);
        text += "\n";
    }
    text += "\n";
    for (id, uuid) in [("e1", "E"), ("e2", "E")] {
        text += &segment(id, uuid);
        text += "\n";
    }
    text.into_bytes()
}

fn write_kit(directory: &Directory, bytes: &[u8]) -> (PathBuf, String) {
    let kit = directory.0.join("kit.jsonl");
    std::fs::write(&kit, bytes).unwrap();
    (kit, hash(bytes))
}

fn joined(output: &Path, manifest: &Manifest) -> Vec<u8> {
    manifest
        .batches
        .iter()
        .flat_map(|batch| std::fs::read(output.join(&batch.file)).unwrap())
        .collect()
}

#[test]
fn a_kit_splits_between_trajectories_and_its_batches_join_to_its_bytes() {
    let directory = Directory::new();
    let bytes = kit_bytes();
    let (kit, sha256) = write_kit(&directory, &bytes);
    let output = directory.0.join("chain");
    let (manifest, manifest_sha256) = chain::split(&kit, &sha256, 2, &output).unwrap();
    assert_eq!(
        manifest_sha256,
        hash(&std::fs::read(output.join(chain::MANIFEST)).unwrap())
    );
    assert_eq!(manifest.kit_sha256, sha256);
    assert_eq!((manifest.segments, manifest.trajectories), (9, 5));
    assert_eq!(manifest.batch_trajectories, 2);
    let shape: Vec<_> = manifest
        .batches
        .iter()
        .map(|batch| {
            (
                batch.index,
                batch.file.as_str(),
                batch.segments,
                batch.first_segment.as_str(),
                batch.last_segment.as_str(),
                batch.trajectories.join(","),
            )
        })
        .collect();
    assert_eq!(
        shape,
        [
            (0, "batch-0000.jsonl", 3, "a1", "b1", "A,B".to_owned()),
            (1, "batch-0001.jsonl", 4, "c1", "d1", "C,D".to_owned()),
            (2, "batch-0002.jsonl", 2, "e1", "e2", "E".to_owned()),
        ]
    );
    assert_eq!(joined(&output, &manifest), bytes);
    for batch in &manifest.batches {
        let read = Kit::read(&output.join(&batch.file)).unwrap();
        assert_eq!(read.sha256(), batch.sha256);
        assert_eq!(read.segments().len(), batch.segments);
    }
    let written: Manifest =
        serde_json::from_slice(&std::fs::read(output.join(chain::MANIFEST)).unwrap()).unwrap();
    assert_eq!(written, manifest);
    assert!(!output.join("chain.json.pending").exists());
}

#[test]
fn every_batch_size_keeps_each_trajectory_in_one_batch() {
    let directory = Directory::new();
    let bytes = kit_bytes();
    let (kit, sha256) = write_kit(&directory, &bytes);
    for size in 1..=6 {
        let output = directory.0.join(format!("chain-{size}"));
        let (manifest, _) = chain::split(&kit, &sha256, size, &output).unwrap();
        assert_eq!(
            manifest.batches.len(),
            5_usize.div_ceil(size),
            "size {size}"
        );
        assert_eq!(joined(&output, &manifest), bytes, "size {size}");
        for batch in &manifest.batches {
            let read = Kit::read(&output.join(&batch.file)).unwrap();
            let in_batch: Vec<&str> = read
                .segments()
                .iter()
                .map(|segment| segment.locators["uuid"].as_str().unwrap())
                .collect();
            for uuid in &in_batch {
                assert!(batch.trajectories.iter().any(|named| named == uuid));
            }
            assert!(batch.trajectories.len() <= size);
        }
        let all: Vec<String> = manifest
            .batches
            .iter()
            .flat_map(|batch| batch.trajectories.clone())
            .collect();
        assert_eq!(all, ["A", "B", "C", "D", "E"], "size {size}");
    }
}

#[test]
fn the_splitter_refuses_before_it_creates_the_directory() {
    let directory = Directory::new();
    let bytes = kit_bytes();
    let (kit, sha256) = write_kit(&directory, &bytes);
    let output = directory.0.join("chain");
    let refused = |kit: &Path, sha256: &str, size: usize, reason: &str| {
        let error = chain::split(kit, sha256, size, &output).unwrap_err();
        assert!(error.to_string().contains(reason), "{error}");
        assert!(!output.exists(), "{reason}");
    };
    refused(&kit, &sha256, 0, "at least one trajectory");
    refused(&kit, &"0".repeat(64), 2, "frozen kit hash mismatch");
    let split_run = [("a1", "A"), ("b1", "B"), ("a2", "A")]
        .map(|(id, uuid)| segment(id, uuid) + "\n")
        .concat();
    let other = directory.0.join("split-run.jsonl");
    std::fs::write(&other, &split_run).unwrap();
    refused(
        &other,
        &hash(split_run.as_bytes()),
        2,
        "trajectory A is not one consecutive run",
    );
    let no_uuid = json!({"id":"x","kind":"narration","text":"Read."}).to_string() + "\n";
    std::fs::write(&other, &no_uuid).unwrap();
    refused(
        &other,
        &hash(no_uuid.as_bytes()),
        2,
        "has no trajectory uuid",
    );
    std::fs::create_dir(&output).unwrap();
    assert!(chain::split(&kit, &sha256, 2, &output).is_err());
    assert_eq!(std::fs::read_dir(&output).unwrap().count(), 0);
}

fn manifest_at(directory: &Directory) -> (PathBuf, String, Manifest) {
    let bytes = kit_bytes();
    let (kit, sha256) = write_kit(directory, &bytes);
    let output = directory.0.join("chain");
    let (manifest, manifest_sha256) = chain::split(&kit, &sha256, 2, &output).unwrap();
    let path = output.join(chain::MANIFEST);
    (path, manifest_sha256, manifest)
}

#[test]
fn a_batch_is_checked_against_its_pinned_manifest_and_index() {
    let directory = Directory::new();
    let (path, manifest_sha256, manifest) = manifest_at(&directory);
    let second = &manifest.batches[1];
    assert_eq!(
        &chain::check(&path, &manifest_sha256, 1, &second.sha256).unwrap(),
        second
    );
    let refused = |path: &Path, sha256: &str, index: usize, kit: &str, reason: &str| {
        let error = chain::check(path, sha256, index, kit).unwrap_err();
        assert!(error.to_string().contains(reason), "{error}");
    };
    refused(
        &path,
        &"0".repeat(64),
        1,
        &second.sha256,
        "chain manifest hash mismatch",
    );
    refused(
        &path,
        &manifest_sha256,
        3,
        &second.sha256,
        "index is not in the manifest",
    );
    refused(
        &path,
        &manifest_sha256,
        0,
        &second.sha256,
        "not the chain's batch at that index",
    );
    let tampered = directory.0.join("tampered.json");
    let rewrite = |change: &dyn Fn(&mut Value)| {
        let mut value: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        change(&mut value);
        let bytes = serde_json::to_vec(&value).unwrap();
        std::fs::write(&tampered, &bytes).unwrap();
        hash(&bytes)
    };
    for change in [
        &(|value: &mut Value| value["batches"][2]["trajectories"] = json!(["A"]))
            as &dyn Fn(&mut Value),
        // A trajectory in two batches, with every count still matching.
        &|value: &mut Value| value["batches"][2]["trajectories"] = json!(["A", "E"]),
        // A batch before the last that is not full.
        &|value: &mut Value| value["batch_trajectories"] = json!(3),
        &|value: &mut Value| value["batches"][1]["index"] = json!(2),
        &|value: &mut Value| value["batches"][1]["trajectories"] = json!(["C", "D", "X"]),
        &|value: &mut Value| value["segments"] = json!(8),
        &|value: &mut Value| value["batches"] = json!([]),
        &|value: &mut Value| value["batches"][2]["file"] = json!("batch-0000.jsonl"),
    ] {
        let sha256 = rewrite(change);
        refused(
            &tampered,
            &sha256,
            1,
            &second.sha256,
            "not a chain of whole trajectories",
        );
    }
    let sha256 = rewrite(&|value: &mut Value| value["parent"] = json!("kit.jsonl"));
    refused(&tampered, &sha256, 1, &second.sha256, "unknown field");
}

fn driver(args: &[&str], config: &Path, output: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_weaver-ste-driver"))
        .args(args)
        .arg("--config")
        .arg(config)
        .arg("--output")
        .arg(output)
        .output()
        .unwrap()
}

#[test]
fn split_kit_names_the_manifest_it_wrote() {
    let directory = Directory::new();
    let bytes = kit_bytes();
    let (_, sha256) = write_kit(&directory, &bytes);
    let config = directory.0.join("split.json");
    let output = directory.0.join("chain");
    let write =
        |value: &Value| std::fs::write(&config, serde_json::to_vec(value).unwrap()).unwrap();
    write(&json!({"kit":"kit.jsonl","kit_sha256":sha256,"batch_trajectories":0}));
    let result = driver(&["split-kit"], &config, &output);
    assert_eq!(result.status.code(), Some(1), "{result:?}");
    assert!(!output.exists());
    write(&json!({"kit":"kit.jsonl","kit_sha256":sha256,"batch_trajectories":2,"extra":1}));
    let result = driver(&["split-kit"], &config, &output);
    assert_eq!(result.status.code(), Some(1), "{result:?}");
    assert!(String::from_utf8_lossy(&result.stderr).contains("unknown field"));
    for size in [
        json!("REPLACE_WITH_THE_ELECTED_COUNT"),
        json!(null),
        json!(-1),
        json!(1.5),
    ] {
        write(&json!({"kit":"kit.jsonl","kit_sha256":sha256,"batch_trajectories":size}));
        let result = driver(&["split-kit"], &config, &output);
        assert_eq!(result.status.code(), Some(1), "{size}: {result:?}");
        assert!(!output.exists(), "{size}");
    }
    for field in ["kit", "kit_sha256", "batch_trajectories"] {
        let mut value = json!({"kit":"kit.jsonl","kit_sha256":sha256,"batch_trajectories":2});
        value.as_object_mut().unwrap().remove(field);
        write(&value);
        let result = driver(&["split-kit"], &config, &output);
        assert!(
            String::from_utf8_lossy(&result.stderr).contains("missing field"),
            "{field}"
        );
        assert!(!output.exists(), "{field}");
    }
    write(&json!({"kit":"kit.jsonl","kit_sha256":sha256,"batch_trajectories":2}));
    let result = driver(&["split-kit"], &config, &output);
    assert!(result.status.success(), "{result:?}");
    let summary: Value = serde_json::from_slice(&result.stdout).unwrap();
    let manifest = std::fs::read(output.join(chain::MANIFEST)).unwrap();
    assert_eq!(summary["manifest_sha256"], hash(&manifest));
    assert_eq!(summary["kit_sha256"], sha256);
    assert_eq!(
        (summary["batches"].as_u64(), summary["segments"].as_u64()),
        (Some(3), Some(9))
    );
    assert_eq!(
        driver(&["split-kit"], &config, &output).status.code(),
        Some(1)
    );
}

fn repo() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
}

/// An evaluation of the chain's batch 1, with no pinned checker data, so a config
/// that passes the chain check stops later, at the prompt bundle.
fn batch_config(directory: &Directory) -> (Value, PathBuf) {
    let (path, manifest_sha256, manifest) = manifest_at(directory);
    let second = &manifest.batches[1];
    let config = json!({
        "repo":repo(), "agent":"ste-fixture", "config_root":directory.0.join("admin"),
        "train_root":directory.0.join("train"),
        "approved_words":directory.0.join("words.json"),
        "prompt_bundle":directory.0.join("bundle.json"),
        "kit":path.parent().unwrap().join(&second.file), "kit_sha256":second.sha256,
        "driver_repair_pass_cap":2, "offer_pass_cap":1, "transport_stop_bound":0,
        "gate_call_deadline_seconds":600,
        "chain":{"manifest":"chain/chain.json","manifest_sha256":manifest_sha256,"index":1}
    });
    (config, directory.0.join("config.json"))
}

fn evaluated(directory: &Directory, config: &Value, path: &Path) -> String {
    std::fs::write(path, serde_json::to_vec(config).unwrap()).unwrap();
    let output = directory.0.join("result");
    let result = driver(&["evaluate"], path, &output);
    assert_eq!(result.status.code(), Some(1), "{result:?}");
    assert!(
        !output.exists(),
        "a refused evaluation must not create a run"
    );
    String::from_utf8_lossy(&result.stderr).into_owned()
}

#[test]
fn evaluate_checks_a_batch_against_its_chain_before_a_run_exists() {
    let directory = Directory::new();
    let (base, path) = batch_config(&directory);
    // The chain check passes, and the run stops later, at a prompt bundle with no
    // source system text, the check after the kit's and the chain's.
    std::fs::write(directory.0.join("bundle.json"), "{}").unwrap();
    let accepted = evaluated(&directory, &base, &path);
    assert!(
        accepted.contains("missing source system text"),
        "{accepted}"
    );
    let mut changed = base.clone();
    changed["chain"]["index"] = json!(0);
    assert!(evaluated(&directory, &changed, &path).contains("kit is not the chain's batch"));
    changed["chain"]["index"] = json!(3);
    assert!(evaluated(&directory, &changed, &path).contains("index is not in the manifest"));
    changed = base.clone();
    changed["chain"]["manifest_sha256"] = json!("0".repeat(64));
    assert!(evaluated(&directory, &changed, &path).contains("chain manifest hash mismatch"));
    changed = base.clone();
    changed["chain"]["run"] = json!(1);
    assert!(evaluated(&directory, &changed, &path).contains("unknown field"));
    for field in ["manifest", "manifest_sha256", "index"] {
        changed = base.clone();
        changed["chain"].as_object_mut().unwrap().remove(field);
        assert!(
            evaluated(&directory, &changed, &path).contains("missing field"),
            "{field}"
        );
    }
    changed = base.clone();
    changed["chain"]["manifest"] = json!("");
    assert!(evaluated(&directory, &changed, &path).contains("an input path is empty"));
}

/// The files are operator-provided pinned data, never silently substituted. No gate
/// listens, so the first call is a transport stop and the evaluation is void, after
/// run.json has recorded the batch's place in its chain.
#[test]
#[ignore = "requires STE_TEST_TRAIN_ROOT, STE_TEST_APPROVED_WORDS and STE_TEST_PROMPT_BUNDLE"]
fn a_batch_evaluation_records_its_place_in_the_chain_before_its_first_call() {
    let directory = Directory::new();
    let (mut config, path) = batch_config(&directory);
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
    std::fs::write(&path, serde_json::to_vec(&config).unwrap()).unwrap();
    let output = directory.0.join("batch-1");
    let result = driver(&["evaluate"], &path, &output);
    assert_eq!(result.status.code(), Some(2), "{result:?}");
    let record: Value =
        serde_json::from_slice(&std::fs::read(output.join("run.json")).unwrap()).unwrap();
    let provenance = &record["provenance"];
    let manifest = directory.0.join("chain/chain.json");
    assert_eq!(provenance["config"]["chain"]["manifest"], json!(manifest));
    assert_eq!(provenance["config"]["chain"]["index"], 1);
    assert_eq!(
        provenance["config"]["chain"]["manifest_sha256"],
        hash(&std::fs::read(&manifest).unwrap())
    );
    let written: Manifest = serde_json::from_slice(&std::fs::read(&manifest).unwrap()).unwrap();
    assert_eq!(
        provenance["chain_batch"],
        serde_json::to_value(&written.batches[1]).unwrap()
    );
    assert_eq!(
        provenance["config"]["kit_sha256"],
        written.batches[1].sha256
    );
}
