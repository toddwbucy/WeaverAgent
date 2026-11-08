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

/// The entries file, in its own order.
pub fn entries(path: &Path) -> Vec<Entry> {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("{} is unreadable: {e}", path.display()));
    text.lines()
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
                    .map(|t| TokenId(t.as_u64().expect("a token id is unsigned") as u32))
                    .collect(),
                max_new_tokens: row["max_new_tokens"]
                    .as_u64()
                    .expect("an entry carries its reply budget")
                    as usize,
            }
        })
        .collect()
}

/// The report directory, made fresh and refused inside this repository.
pub fn fresh_report(report: &Path) {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the repository root resolves");
    assert!(
        !report.exists(),
        "WEAVER_LORA_REPORT names {}, which already exists; a report is never reused",
        report.display()
    );
    std::fs::create_dir_all(report)
        .unwrap_or_else(|e| panic!("the report directory {}: {e}", report.display()));
    let resolved = report
        .canonicalize()
        .expect("the report directory resolves");
    assert!(
        !resolved.starts_with(&repository),
        "the report directory {} is inside the repository; LORA-6 keeps it outside",
        resolved.display()
    );
}

/// One arm's outcome as the parent reads it back.
pub struct Arm {
    pub name: String,
    pub outputs: BTreeMap<String, Vec<u32>>,
    pub observation: Value,
}

/// Run one arm in a child process of this test binary, and read its files.
pub fn run_arm(test: &str, inputs: &Inputs, name: &str, adapted: bool, ids: &[String]) -> Arm {
    let exe = std::env::current_exe().expect("the test binary names itself");
    let status = Command::new(exe)
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
        )
        .env(IDS, ids.join(","))
        .status()
        .expect("the arm process starts");
    assert!(status.success(), "arm {name} failed: {status}");
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
            .map(|t| t.as_u64().expect("a token id") as u32)
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
    let wanted: Vec<String> = std::env::var(IDS)
        .expect("the parent names the arm's ids")
        .split(',')
        .map(str::to_string)
        .collect();
    let all = entries(&inputs.entries);
    let selected: Vec<&Entry> = wanted
        .iter()
        .map(|id| {
            all.iter()
                .find(|e| &e.id == id)
                .unwrap_or_else(|| panic!("no entry {id} in WEAVER_LORA_ENTRIES"))
        })
        .collect();

    let mut fixture = Fixture::new().expect("the LORA-3 observer installs first");
    let request = inputs.request();
    let resident = match fixture.admit(
        &inputs.binding(),
        Headroom(HEADROOM_BYTES),
        ReadoutElection(false),
        false,
        adapted.then_some(&request),
    ) {
        Ok(resident) => resident,
        Err(refusal) => panic!("arm {name} admission refused: {refusal:?}"),
    };
    let family = (resident.declaration.renderer)();
    let stops = resident
        .stop_set(family)
        .expect("the family's stop set promotes");
    let terminator = stops.terminator;
    let LoadedModel::Gguf(model) = resident.model() else {
        panic!("the base is not served by the GGUF engine");
    };
    let identity = model.adapter_identity().cloned();
    let placement = model.adapter_placement().cloned();
    let weights_hash = resident.weights_hash.0.clone();
    let family_name = resident.declaration.family;
    assert_eq!(
        identity.is_some(),
        adapted,
        "arm {name}: adapter residency does not match the arm"
    );

    let mut lines = String::new();
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

    let records = fixture.records().expect("the capture holds every record");
    let (division, kv) = division(&records);
    let observation = json!({
        "arm": name,
        "adapted": adapted,
        "base": inputs.base,
        "base_weights_hash": weights_hash,
        "devices": inputs.devices.iter().map(|d| d.0).collect::<Vec<_>>(),
        "family": family_name,
        "terminator": terminator.0,
        "stop_tokens": [terminator.0],
        "capacity": CAPACITY,
        "headroom_bytes": HEADROOM_BYTES,
        "knobs": {"temperature": 0.0, "top_k": 1, "top_p": 1.0, "repetition_penalty": 1.0},
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
    true
}

/// The division and the per-layer KV cache devices, from the engine's records.
/// Every KV record for one layer must name one device across the arm's
/// sessions; the division must be non-empty.
fn division(
    records: &[weaver_spu::lora::observation::Record],
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
                assert!(
                    division.insert(layer, device).is_none(),
                    "layer {layer} was assigned twice in one admission"
                );
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
    assert!(!division.is_empty(), "no assignment record was captured");
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
    let division = arm.observation["division"]
        .as_object()
        .expect("the division is a map");
    for (layer, device) in division {
        let device = device.as_str().expect("a device name");
        assert!(
            names.iter().any(|n| n == device),
            "arm {}: layer {layer} is assigned to {device}, outside the binding {names:?}",
            arm.name
        );
    }
}
