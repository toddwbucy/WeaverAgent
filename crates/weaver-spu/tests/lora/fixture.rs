//! The arm runner the two LORA-7 accelerator targets share, per
//! `weaver-spu-lora-loading-Spec-amendment` sections 6 and 7.
//!
//! **One arm is one process.** A residency admits once per process and the
//! LORA-3 observer installs once per process, so a comparison of a base-only
//! control with an adapted run cannot be made inside one test process. The
//! parent test re-runs its own binary once per arm, with the arm named in
//! [`ARM`]. The child admits the fixture once, decodes every requested entry
//! greedily from its frozen input ids, writes its outputs and its observations
//! under the report directory, releases, and exits. The parent compares the
//! files.
//!
//! **Every input is named by the environment**, so no machine's paths enter
//! the tree, and a missing input fails by name (LORA-7: a missing artifact,
//! backend or device is not a pass). The report directory must be fresh and
//! outside this repository (LORA-6).
//!
//! **The records this reads are the pinned engine's debug log lines**, through
//! the LORA-3 capture: `load_tensors: layer N assigned to device D` for the
//! division, `llama_kv_cache: layer N: dev = D` for the per-layer KV cache,
//! and the adapter placement lines that `lora::observation::judge` already
//! reads. A change to their wording in the engine pin breaks this parse, and
//! the parse fails rather than reporting an empty division.
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use serde_json::{Value, json};
use sha2::Digest as _;
use weaver_spu::decoder::backend::TokenId;
use weaver_spu::decoder::session::{
    NeverCancels, PositionedSinks, SamplerBuild, StopCondition, Stopped,
};
use weaver_spu::lora::{Fixture, Request};
use weaver_spu::readout::ReadoutElection;
use weaver_spu::residency::{Headroom, LoadedModel};
use weaver_spu::sampling::EffectiveKnobs;
use weaver_types::{ArtifactRef, DeviceOrdinal, ModelBinding};

/// The child marker: the arm a child process runs. Not a fixture, so it
/// carries no `WEAVER_` prefix and stays out of Spec section 1.2's table.
pub const ARM: &str = "LORA_PROOF_ARM";
/// The child's entry ids, comma-separated, chosen by the parent.
const IDS: &str = "LORA_PROOF_IDS";

/// The session capacity, the reference run's own sequence limit. The longest
/// held-out prompt and its reply budget fit well inside it.
pub const CAPACITY: u32 = 32768;

/// No admission headroom. Every arm runs with the same value, so it moves no
/// comparison, and the report names it.
pub const HEADROOM_BYTES: u64 = 0;

/// Greedy decoding: the reference sampled with `do_sample = false`.
pub const KNOBS: EffectiveKnobs = EffectiveKnobs {
    temperature: 0.0,
    top_k: 1,
    top_p: 1.0,
    repetition_penalty: 1.0,
    repetition_window: 0,
    seed: 0,
};

/// The fixture inputs, read once from the environment.
pub struct Inputs {
    pub base: PathBuf,
    pub adapter: PathBuf,
    pub adapter_sha256: String,
    pub base_blake3: String,
    pub scale: f32,
    pub entries: PathBuf,
    pub report: PathBuf,
    pub devices: Vec<DeviceOrdinal>,
}

fn required(variable: &str) -> String {
    std::env::var(variable)
        .unwrap_or_else(|_| panic!("{variable} is not set, so the fixture did not run"))
}

fn existing_file(variable: &str) -> PathBuf {
    let path = PathBuf::from(required(variable));
    assert!(
        path.is_file(),
        "{variable} names no regular file at {}",
        path.display()
    );
    path
}

impl Inputs {
    pub fn from_env() -> Inputs {
        let scale: f32 = required("WEAVER_LORA_SCALE")
            .parse()
            .expect("WEAVER_LORA_SCALE is a number");
        let devices = required("WEAVER_LORA_DEVICES")
            .split(',')
            .map(|d| {
                DeviceOrdinal(
                    d.trim()
                        .parse()
                        .expect("WEAVER_LORA_DEVICES lists device ordinals"),
                )
            })
            .collect();
        Inputs {
            base: existing_file("WEAVER_LORA_BASE"),
            adapter: existing_file("WEAVER_LORA_ADAPTER"),
            adapter_sha256: required("WEAVER_LORA_ADAPTER_SHA256"),
            base_blake3: required("WEAVER_LORA_BASE_BLAKE3"),
            scale,
            entries: existing_file("WEAVER_LORA_ENTRIES"),
            report: PathBuf::from(required("WEAVER_LORA_REPORT")),
            devices,
        }
    }

    fn binding(&self) -> ModelBinding {
        ModelBinding {
            artifact: ArtifactRef(self.base.to_string_lossy().into_owned()),
            devices: self.devices.clone(),
        }
    }

    fn request(&self) -> Request {
        Request {
            artifact: self.adapter.clone(),
            sha256: self.adapter_sha256.clone(),
            expected_base_weights_hash: self.base_blake3.clone(),
            scale: self.scale,
        }
    }
}

/// One held-out entry: its frozen prompt ids and reply budget.
pub struct Entry {
    pub id: String,
    pub input_ids: Vec<TokenId>,
    pub max_new_tokens: usize,
}

/// A token id from JSON, refused rather than wrapped when it does not fit.
fn token(value: &Value) -> u32 {
    value
        .as_u64()
        .and_then(|t| u32::try_from(t).ok())
        .unwrap_or_else(|| panic!("{value} is not a token id"))
}

/// The entries file, in its own order. A repeated id refuses, since the
/// comparison is keyed by id.
pub fn entries(path: &Path) -> Vec<Entry> {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("{} is unreadable: {e}", path.display()));
    let entries: Vec<Entry> = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let row: Value = serde_json::from_str(line)
                .unwrap_or_else(|e| panic!("{} holds a bad line: {e}", path.display()));
            Entry {
                id: row["id"].as_str().expect("an entry carries its id").into(),
                input_ids: row["input_ids"]
                    .as_array()
                    .expect("an entry carries its input ids")
                    .iter()
                    .map(|t| TokenId(token(t)))
                    .collect(),
                max_new_tokens: row["max_new_tokens"]
                    .as_u64()
                    .expect("an entry carries its reply budget")
                    as usize,
            }
        })
        .collect();
    let mut seen = std::collections::BTreeSet::new();
    for entry in &entries {
        assert!(
            seen.insert(entry.id.as_str()),
            "{} repeats the id {}",
            path.display(),
            entry.id
        );
    }
    entries
}

/// The report directory, made fresh and refused inside this repository.
pub fn fresh_report(report: &Path) {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the repository root resolves");
    assert!(
        report.is_absolute(),
        "WEAVER_LORA_REPORT names {}, which is not an absolute path",
        report.display()
    );
    assert!(
        !report.exists(),
        "WEAVER_LORA_REPORT names {}, which already exists; a report is never reused",
        report.display()
    );
    // Judged on the nearest existing ancestor, before anything is created.
    let mut ancestor = report.to_path_buf();
    while !ancestor.exists() {
        ancestor = ancestor
            .parent()
            .expect("an absolute path has an existing ancestor")
            .to_path_buf();
    }
    let resolved = ancestor
        .canonicalize()
        .expect("the report's ancestor resolves");
    assert!(
        !resolved.starts_with(&repository),
        "the report directory {} is inside the repository; LORA-6 keeps it outside",
        report.display()
    );
    std::fs::create_dir_all(report)
        .unwrap_or_else(|e| panic!("the report directory {}: {e}", report.display()));
}

/// One arm's outcome as the parent reads it back.
pub struct Arm {
    pub name: String,
    pub outputs: BTreeMap<String, Vec<u32>>,
    pub observation: Value,
}

/// Run one arm in a child process of this test binary, and read its files.
/// `ids` of `None` decodes every entry, which the child reads itself.
pub fn run_arm(
    test: &str,
    inputs: &Inputs,
    name: &str,
    adapted: bool,
    ids: Option<&[String]>,
) -> Arm {
    let exe = std::env::current_exe().expect("the test binary names itself");
    let mut command = Command::new(exe);
    command
        .args([
            test,
            "--exact",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(
            ARM,
            format!("{name}:{}", if adapted { "adapted" } else { "base" }),
        );
    if let Some(ids) = ids {
        command.env(IDS, ids.join("\n"));
    }
    let status = command.status().expect("the arm process starts");
    if !status.success() {
        // LORA-6 keeps a failed attempt's facts: the stage the child reached
        // is in its stage file, and the parent records the exit.
        let _ = std::fs::write(
            inputs.report.join(format!("{name}.failed")),
            format!("{status}\n"),
        );
        panic!("arm {name} failed: {status}");
    }
    read_arm(&inputs.report, name)
}

fn read_arm(report: &Path, name: &str) -> Arm {
    let outputs_path = report.join(format!("{name}.jsonl"));
    let mut outputs = BTreeMap::new();
    for line in std::fs::read_to_string(&outputs_path)
        .unwrap_or_else(|e| panic!("arm {name} wrote no outputs: {e}"))
        .lines()
    {
        let row: Value = serde_json::from_str(line).expect("an output line is JSON");
        let ids = row["token_ids"]
            .as_array()
            .expect("an output carries token ids")
            .iter()
            .map(token)
            .collect();
        outputs.insert(row["id"].as_str().expect("an id").to_string(), ids);
    }
    let observation = serde_json::from_str(
        &std::fs::read_to_string(report.join(format!("{name}.json")))
            .unwrap_or_else(|e| panic!("arm {name} wrote no observation: {e}")),
    )
    .expect("the observation is JSON");
    Arm {
        name: name.into(),
        outputs,
        observation,
    }
}

/// When this process is an arm's child, run the arm and return true.
pub fn child() -> bool {
    let Ok(arm) = std::env::var(ARM) else {
        return false;
    };
    let (name, kind) = arm.split_once(':').expect("the arm marker is name:kind");
    let adapted = kind == "adapted";
    let inputs = Inputs::from_env();
    let all = entries(&inputs.entries);
    let wanted: Vec<String> = match std::env::var(IDS) {
        Ok(ids) => ids.lines().map(str::to_string).collect(),
        Err(_) => all.iter().map(|e| e.id.clone()).collect(),
    };
    let selected: Vec<&Entry> = wanted
        .iter()
        .map(|id| {
            all.iter()
                .find(|e| &e.id == id)
                .unwrap_or_else(|| panic!("no entry {id} in WEAVER_LORA_ENTRIES"))
        })
        .collect();

    let stage = |reached: &str| {
        let _ = std::fs::write(
            inputs.report.join(format!("{name}.stage")),
            format!("{reached}\n"),
        );
    };
    stage("observer");
    let mut fixture = Fixture::new().expect("the LORA-3 observer installs first");
    let request = inputs.request();
    stage("admit");
    let admit_started = Instant::now();
    let resident = match fixture.admit(
        &inputs.binding(),
        Headroom(HEADROOM_BYTES),
        ReadoutElection(false),
        false,
        adapted.then_some(&request),
    ) {
        Ok(resident) => resident,
        Err(refusal) => {
            stage(&format!("admit refused: {refusal:?}"));
            dump_records(&inputs.report, name, &fixture.records().unwrap_or_default());
            panic!("arm {name} admission refused: {refusal:?}")
        }
    };
    let admit_seconds = admit_started.elapsed().as_secs_f64();
    stage("decode");
    let family = (resident.declaration.renderer)();
    let stops = resident
        .stop_set(family)
        .expect("the family's stop set promotes");
    // **The arm stops on the turn's close alone**, which is what the
    // reference's generation stopped on (its end-of-turn token as the only
    // eos). The serving stop set adds the artifact's declared end of sequence
    // as a backstop, and the report records it beside the one used.
    let terminator = stops.terminator;
    let serving_stops: Vec<u32> = stops.tokens.iter().map(|t| t.0).collect();
    let LoadedModel::Gguf(model) = resident.model() else {
        panic!("the base is not served by the GGUF engine");
    };
    let identity = model.adapter_identity().cloned();
    let placement = model.adapter_placement().cloned();
    let weights_hash = resident.weights_hash.0.clone();
    let family_name = resident.declaration.family;
    // The layer count is the artifact's own (`block_count`), not one read back
    // from the capture, so a capture that lost a suffix cannot vouch for itself.
    let n_layer = u32::try_from(
        resident
            .header
            .layer_count
            .expect("the base declares its block count"),
    )
    .expect("a layer count fits u32");
    let template_sha256 = resident
        .header
        .chat_template
        .as_ref()
        .map(|t| format!("{:x}", sha2::Sha256::digest(t.as_bytes())));
    assert_eq!(
        identity.is_some(),
        adapted,
        "arm {name}: adapter residency does not match the arm"
    );

    let mut lines = String::new();
    // LORA-6: the exact inputs this arm decoded, archived beside its outputs.
    let mut consumed = String::new();
    for entry in &selected {
        let row = json!({
            "id": entry.id,
            "input_ids": entry.input_ids.iter().map(|t| t.0).collect::<Vec<u32>>(),
            "max_new_tokens": entry.max_new_tokens,
        });
        writeln!(consumed, "{row}").expect("a string takes a line");
    }
    std::fs::write(inputs.report.join(format!("{name}.inputs.jsonl")), consumed)
        .expect("the consumed inputs write");
    for entry in &selected {
        let started = Instant::now();
        let mut session = resident
            .open_session(&KNOBS, CAPACITY)
            .expect("a session opens");
        session
            .open(&entry.input_ids)
            .expect("the frozen prompt decodes");
        let generated = session
            .append_and_generate(
                &[],
                &StopCondition {
                    stop_tokens: vec![terminator],
                    terminator,
                    max_tokens: entry.max_new_tokens,
                },
                &mut NeverCancels,
                &mut |_| {},
                PositionedSinks {
                    field: None,
                    on_column: &mut |_, _| {},
                },
                SamplerBuild {
                    seed: 0,
                    penalty_window: 0,
                },
            )
            .expect("the entry generates");
        let text = resident
            .detokenize(&generated.tokens)
            .expect("the output detokenizes");
        let mut ids: Vec<u32> = generated.tokens.iter().map(|t| t.0).collect();
        let finish = match generated.stopped {
            Stopped::Complete => {
                ids.push(terminator.0);
                "stop"
            }
            Stopped::LimitReached => "length",
            other => panic!("entry {} ended {other:?}", entry.id),
        };
        drop(session);
        let row = json!({
            "id": entry.id,
            "output": text,
            "token_ids": ids,
            "finish_reason": finish,
            "wall_seconds": started.elapsed().as_secs_f64(),
        });
        writeln!(lines, "{row}").expect("a string takes a line");
        eprintln!("arm {name}: {} {} tokens, {finish}", entry.id, ids.len());
    }

    stage("observe");
    let records = fixture.records().expect("the capture holds every record");
    dump_records(&inputs.report, name, &records);
    let (division, kv) = division(&records, n_layer);
    let observation = json!({
        "arm": name,
        "adapted": adapted,
        "base": inputs.base,
        "base_weights_hash": weights_hash,
        "devices": inputs.devices.iter().map(|d| d.0).collect::<Vec<_>>(),
        "family": family_name,
        "n_layer": n_layer,
        "chat_template_sha256": template_sha256,
        "entries_sha256": format!("{:x}", sha2::Sha256::digest(
            std::fs::read(&inputs.entries).expect("the entries file reads again"),
        )),
        "terminator": terminator.0,
        "stop_tokens": [terminator.0],
        "serving_stop_tokens": serving_stops,
        "capacity": CAPACITY,
        "headroom_bytes": HEADROOM_BYTES,
        "knobs": {
            "temperature": KNOBS.temperature,
            "top_k": KNOBS.top_k,
            "top_p": KNOBS.top_p,
            "repetition_penalty": KNOBS.repetition_penalty,
            "repetition_window": KNOBS.repetition_window,
            "seed": KNOBS.seed,
        },
        "admit_seconds": admit_seconds,
        "source": source(),
        "adapter": identity.map(|i| json!({
            "sha256": i.sha256,
            "base_weights_hash": i.base_weights_hash,
            "scale": i.scale,
            "alpha": i.alpha,
            "targets": i.targets,
            "tensor_bytes": i.tensor_bytes,
        })),
        "adapter_placement": placement,
        "division": division,
        "kv_cache": kv,
        "input_layer": "token_embd stays in host memory under every assignment; the pinned engine emits no assignment record for it",
        "libllama": libllama(),
    });
    let report = &inputs.report;
    std::fs::write(report.join(format!("{name}.jsonl")), lines).expect("the outputs write");
    std::fs::write(
        report.join(format!("{name}.json")),
        serde_json::to_string_pretty(&observation).expect("the observation serializes"),
    )
    .expect("the observation writes");
    fixture.release().expect("the arm releases");
    stage("released");
    true
}

/// Every captured engine record, verbatim, for LORA-6's raw placement evidence.
fn dump_records(report: &Path, name: &str, records: &[weaver_spu::lora::observation::Record]) {
    let mut text = String::new();
    for record in records {
        let _ = writeln!(
            text,
            "{}\t{}",
            record.level,
            String::from_utf8_lossy(&record.bytes).trim_end()
        );
    }
    let _ = std::fs::write(report.join(format!("{name}.records.txt")), text);
}

/// The build and environment facts LORA-6 names, read at run time.
fn source() -> Value {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let git = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .current_dir(&root)
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
    };
    let lock = std::fs::read_to_string(root.join("Cargo.lock")).unwrap_or_default();
    let engine = lock
        .lines()
        .find(|line| line.contains("llama-cpp-rs") && line.contains("rev="))
        .map(str::to_string);
    json!({
        "commit": git(&["rev-parse", "HEAD"]),
        "dirty": git(&["status", "--porcelain"]).map(|s| !s.is_empty()),
        "engine_pin": engine,
        "host": std::fs::read_to_string("/proc/sys/kernel/hostname").ok().map(|h| h.trim().to_string()),
        "cuda_visible_devices": std::env::var("CUDA_VISIBLE_DEVICES").ok(),
        "cuda_device_order": std::env::var("CUDA_DEVICE_ORDER").ok(),
    })
}

/// The division and the per-layer KV cache devices, from the engine's records.
/// Every KV record for one layer must name one device across the arm's
/// sessions; the division must be non-empty.
pub fn division(
    records: &[weaver_spu::lora::observation::Record],
    n_layer: u32,
) -> (BTreeMap<u32, String>, BTreeMap<u32, String>) {
    let mut division = BTreeMap::new();
    let mut kv = BTreeMap::new();
    for record in records {
        let Ok(text) = std::str::from_utf8(&record.bytes) else {
            continue;
        };
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("load_tensors: layer ") {
                let (layer, device) = rest
                    .split_once(" assigned to device ")
                    .expect("an assignment record names its device");
                let device = device.split(',').next().unwrap().to_string();
                let layer: u32 = layer.trim().parse().expect("an assignment names its layer");
                if let Some(seen) = division.insert(layer, device.clone()) {
                    assert_eq!(seen, device, "layer {layer} was assigned to two devices");
                }
            } else if let Some((_, rest)) = line.split_once(": layer ")
                && let Some((layer, device)) = rest.split_once(": dev = ")
                && line.contains("kv_cache")
            {
                let layer: u32 = layer.trim().parse().expect("a KV record names its layer");
                let device = device.trim().to_string();
                if let Some(seen) = kv.insert(layer, device.clone()) {
                    assert_eq!(
                        seen, device,
                        "layer {layer}'s KV cache moved between sessions"
                    );
                }
            }
        }
    }
    // LORA-7: an assignment for every repeating layer and for the output
    // layer, which the engine counts as n_layer, and a KV record for every
    // repeating layer. A partial or empty capture fails rather than comparing
    // equal to another partial one. `n_layer` is the artifact's own count.
    let last = n_layer;
    assert!(
        division.keys().copied().eq(0..=last),
        "the assignment records are not one per layer 0..={last}: {:?}",
        division.keys().collect::<Vec<_>>()
    );
    assert!(
        kv.keys().copied().eq(0..last),
        "the KV records are not one per repeating layer 0..{last}: {:?}",
        kv.keys().collect::<Vec<_>>()
    );
    // A layer's KV cache lives with its layer: membership in the binding is
    // not enough, the device must be the one the layer is assigned to.
    for (layer, device) in &kv {
        assert_eq!(
            &division[layer], device,
            "layer {layer}'s KV cache is on {device}, and the layer is assigned to {}",
            division[layer]
        );
    }
    (division, kv)
}

/// The libllama this process mapped, so the report names the library it ran.
fn libllama() -> Vec<String> {
    let maps = std::fs::read_to_string("/proc/self/maps").unwrap_or_default();
    let mut found: Vec<String> = maps
        .lines()
        .filter_map(|line| line.split_whitespace().nth(5))
        .filter(|path| path.contains("libllama") || path.contains("libggml"))
        .map(str::to_string)
        .collect();
    found.sort();
    found.dedup();
    found
}

/// The first position where two token lists differ, or None when equal.
pub fn first_difference(a: &[u32], b: &[u32]) -> Option<usize> {
    if a == b {
        return None;
    }
    Some(
        a.iter()
            .zip(b)
            .position(|(x, y)| x != y)
            .unwrap_or(a.len().min(b.len())),
    )
}

/// The division and KV records of two arms must agree, per LORA-7.
pub fn assert_same_division(control: &Arm, other: &Arm) {
    for key in ["division", "kv_cache"] {
        assert_eq!(
            control.observation[key], other.observation[key],
            "{key} differs between {} and {}; the comparison fails",
            control.name, other.name
        );
    }
}

/// Every assignment record names a device in the binding.
pub fn assert_division_on_binding(arm: &Arm, devices: &[DeviceOrdinal]) {
    let names: Vec<String> = devices.iter().map(|d| format!("CUDA{}", d.0)).collect();
    for key in ["division", "kv_cache"] {
        let map = arm.observation[key].as_object().expect("a per-layer map");
        for (layer, device) in map {
            let device = device.as_str().expect("a device name");
            assert!(
                names.iter().any(|n| n == device),
                "arm {}: {key} puts layer {layer} on {device}, outside the binding {names:?}",
                arm.name
            );
        }
    }
}

/// The layer an adapter target belongs to: `blk.N.` is repeating layer N, and
/// `output.` is the output layer, which the engine numbers `n_layer`. A target
/// in neither is refused rather than judged against no layer.
pub fn adapter_layer(target: &str, n_layer: u32) -> u32 {
    if let Some(rest) = target.strip_prefix("blk.") {
        let layer = rest.split('.').next().unwrap_or_default();
        return layer
            .parse()
            .unwrap_or_else(|_| panic!("adapter target {target} names no layer"));
    }
    if target.starts_with("output.") {
        return n_layer;
    }
    panic!("adapter target {target} is outside every assigned layer");
}

/// LORA-3: each adapter target's buffer is the device its layer is assigned
/// to, not merely a device of the binding.
pub fn assert_adapter_on_its_layers(arm: &Arm) {
    let Some(placement) = arm.observation["adapter_placement"].as_object() else {
        panic!("arm {} carries no adapter placement", arm.name);
    };
    let n_layer = arm.observation["n_layer"].as_u64().expect("n_layer") as u32;
    let division = &arm.observation["division"];
    assert!(
        !placement.is_empty(),
        "arm {} placed no adapter target",
        arm.name
    );
    for (target, buffer) in placement {
        let layer = adapter_layer(target, n_layer);
        assert_eq!(
            division[layer.to_string()],
            *buffer,
            "arm {}: adapter target {target} is on {buffer}, and its layer {layer} is assigned to {}",
            arm.name,
            division[layer.to_string()]
        );
    }
}

/// An arm answered every id it was asked for, and no other.
pub fn assert_covers(arm: &Arm, ids: &[String]) {
    let asked: std::collections::BTreeSet<&str> = ids.iter().map(String::as_str).collect();
    let answered: std::collections::BTreeSet<&str> =
        arm.outputs.keys().map(String::as_str).collect();
    assert_eq!(
        answered, asked,
        "arm {} did not answer exactly the ids asked",
        arm.name
    );
}
