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
//! **Each arm runs under its binding's device mask**, per the device pair
//! amendment PAIR-1: the admission refuses a process whose
//! `CUDA_DEVICE_ORDER` and `CUDA_VISIBLE_DEVICES` do not name the binding's
//! ordinals in order. [`run_arm`] sets both on the arm's command, as the
//! harness's fork does. The arms keep this runner rather than
//! `common::run_masked`, which collects a child's output when it exits: an
//! arm of the held-out run decodes for about an hour, and its per-entry
//! progress lines reach the console as they happen only through an inherited
//! stderr.
//!
//! **The division is judged by the SPU's own instrument**,
//! `lora::observation::judge_division` (PAIR-5), over the LORA-3 capture,
//! against the division the admission computed (`resident.placement`) and the
//! cache records the artifact's PAIR-4 classification predicts. Each target
//! placement is judged at load by `lora::observation::judge`, which refuses a
//! target off its layer's device, so this runner records the placement and
//! does not judge it again.
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use serde_json::{Value, json};
use sha2::Digest as _;
use weaver_spu::artifact;
use weaver_spu::decoder::backend::TokenId;
use weaver_spu::decoder::session::{
    NeverCancels, PositionedSinks, SamplerBuild, StopCondition, Stopped,
};
use weaver_spu::lora::observation::{Record, judge_division};
use weaver_spu::lora::{Fixture, Request};
use weaver_spu::readout::ReadoutElection;
use weaver_spu::residency::{Headroom, LoadedModel};
use weaver_spu::room::Layout;
use weaver_spu::sampling::EffectiveKnobs;
use weaver_types::{ArtifactRef, DeviceOrdinal, ModelBinding};

/// The child marker: the arm a child process runs. Not a fixture, so it
/// carries no `WEAVER_` prefix and stays out of Spec section 1.2's table.
pub const ARM: &str = "LORA_PROOF_ARM";
/// The child's entry ids, comma-separated, chosen by the parent.
const IDS: &str = "LORA_PROOF_IDS";

/// The session capacity, the reference run's own sequence limit. The longest
/// held-out prompt and its reply budget fit well inside it. The admission
/// charges each device the KV cache of this many positions (PAIR-4).
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

    /// The binding's ordinals, as the leases and the PAIR-1 mask name them.
    pub fn ordinals(&self) -> Vec<u32> {
        self.devices.iter().map(|d| d.0).collect()
    }

    /// The PAIR-1 mask: the binding's ordinals in order, comma-separated.
    fn mask(&self) -> String {
        self.ordinals()
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(",")
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
        )
        // PAIR-1: the binding's numbering, as the harness's fork sets it.
        .env("CUDA_DEVICE_ORDER", "PCI_BUS_ID")
        .env("CUDA_VISIBLE_DEVICES", inputs.mask());
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
    // The cache records the judgment predicts, from the artifact's own
    // directory through the pin the admission takes. File reads only.
    stage("classify");
    let caches = {
        let reference = ArtifactRef(inputs.base.to_string_lossy().into_owned());
        let resolved = artifact::resolve(&reference).expect("the base resolves");
        let pinned = artifact::pin(&resolved).expect("the base pins");
        let layout = Layout::read(&pinned.paths()).expect("the base's layout reads");
        layout.caches().unwrap_or_else(|| {
            panic!(
                "{} is recurrent or hybrid, whose cache classification is the device \
                 pair work's second child act, so its records are not predicted here",
                layout.architecture()
            )
        })
    };
    stage("observer");
    let mut fixture = Fixture::new().expect("the LORA-3 observer installs first");
    let request = inputs.request();
    stage("admit");
    let admit_started = Instant::now();
    let resident = match fixture.admit(
        &inputs.binding(),
        Headroom(HEADROOM_BYTES),
        CAPACITY,
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
    // What the admission judged: the visible set, the PAIR-3 division and each
    // device's PAIR-4 share (LORA-6 as PAIR-5 extends it).
    let placement_judged = resident.placement.clone();
    let division = placement_judged
        .division
        .clone()
        .expect("a GGUF residency carries its division");
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
    // An arm of one entry opened one session and judges every record. An arm
    // of several opened one session per entry, each writing the cache records
    // again, so it judges the load and its first session.
    let judged = if selected.len() == 1 {
        &records[..]
    } else {
        first_session(&records)
    };
    let observed = match judge_division(judged, &inputs.devices, &division, &caches) {
        Ok(observed) => observed,
        Err(faults) => {
            stage(&format!("division refused: {faults}"));
            panic!("arm {name}: the observed division departs from PAIR-3: {faults}")
        }
    };
    let ordinals = |devices: &[Option<DeviceOrdinal>]| -> Vec<Option<u32>> {
        devices.iter().map(|d| d.as_ref().map(|d| d.0)).collect()
    };
    let observation = json!({
        "arm": name,
        "adapted": adapted,
        "base": inputs.base,
        "base_weights_hash": weights_hash,
        "devices": inputs.devices.iter().map(|d| d.0).collect::<Vec<_>>(),
        "family": family_name,
        "n_layer": division.layers(),
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
        "adapter_placement": placement.map(|p| p
            .into_iter()
            .map(|(target, device)| (target, device.0))
            .collect::<BTreeMap<String, u32>>()),
        "computed_division": {
            "layers": division.layers(),
            "counts": division.counts(),
            "split_values": division.split_values(),
        },
        "observed_division": {
            "slots": observed.slots.iter().map(|d| d.0).collect::<Vec<_>>(),
            "attention": ordinals(&observed.attention),
            "recurrent": ordinals(&observed.recurrent),
        },
        "records_judged": judged.len(),
        "records_captured": records.len(),
        "placement": {
            "visible": placement_judged.visible,
            "needs": placement_judged.needs,
            "shares": placement_judged.shares.as_ref().map(|shares| shares
                .iter()
                .map(|s| json!({
                    "weights": s.weights,
                    "output": s.output,
                    "every_device": s.every_device,
                    "cache": s.cache,
                    "adapter": s.adapter,
                }))
                .collect::<Vec<_>>()),
            "input_bytes": placement_judged.input_bytes,
        },
        "interconnect": interconnect(),
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

/// **The records through an arm's first session.** Each session's context
/// writes the KV cache records again, so an arm that opens one session per
/// entry repeats them, and `judge_division` refuses a repeat. The cut falls at
/// the second attention cache record of layer 0, where the second session's
/// records begin. A layer 0 record repeated inside the first session is cut
/// rather than caught here; the gate's arms open one session each and judge
/// every record, so they catch it.
///
/// **A record is read lossily**, as `lora::observation::judge` and
/// `judge_division` read it: a record that is not UTF-8 still opens a session
/// when it carries a layer 0 cache line, and whether an invalid record that
/// carries placement evidence refuses is the judgment's rule, applied to the
/// records this keeps.
pub fn first_session(records: &[Record]) -> &[Record] {
    let opens_a_cache = |record: &Record| {
        String::from_utf8_lossy(&record.bytes).lines().any(|line| {
            line.strip_prefix("llama_kv_cache: layer ")
                .and_then(|rest| rest.split_once(':'))
                .is_some_and(|(layer, _)| layer.trim() == "0")
        })
    };
    match records
        .iter()
        .enumerate()
        .filter(|(_, record)| opens_a_cache(record))
        .nth(1)
    {
        Some((index, _)) => &records[..index],
        None => records,
    }
}

/// The topology between the devices, evidence only (PAIR-5): admission does
/// not judge it. Read through `nvidia-smi`, which ignores the process mask
/// and lists every device in its own numbering.
fn interconnect() -> Value {
    match Command::new("nvidia-smi").args(["topo", "-m"]).output() {
        Ok(output) if output.status.success() => {
            Value::String(String::from_utf8_lossy(&output.stdout).into_owned())
        }
        Ok(output) => json!({"unread": String::from_utf8_lossy(&output.stderr)}),
        Err(error) => json!({"unread": error.to_string()}),
    }
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

/// The computed and observed divisions of two arms must agree, per LORA-7.
pub fn assert_same_division(control: &Arm, other: &Arm) {
    for key in ["computed_division", "observed_division"] {
        assert_eq!(
            control.observation[key], other.observation[key],
            "{key} differs between {} and {}; the comparison fails",
            control.name, other.name
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
