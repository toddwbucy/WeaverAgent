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

/// **The numerical trace's calculation, as `weaver-spu-Spec` section 6 states
/// it**, written into every report beside the values it names.
pub const CALCULATION: &str = "per retained step, from the step's logits over the full \
vocabulary (llama_get_logits_ith), before temperature and every sampling transform: \
entropy_bits = -sum(p * ln p) / ln 2 and surprisal_bits = -ln p(drawn) / ln 2, in f32, in \
the log-sum-exp-stable form (logits shifted by their maximum); the drawn stop token is not \
retained and carries no step";

/// How the trace writes a value: the IEEE 754 binary32 bit pattern
/// (`f32::to_bits`), eight lowercase hex digits.
pub const REPRESENTATION: &str =
    "IEEE 754 binary32 bit pattern (f32::to_bits), eight lowercase hex digits";

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
    pub traces: BTreeMap<String, Trace>,
    pub observation: Value,
}

/// **One entry's numerical trace**: per retained step, the token and the f32
/// bit patterns of its entropy and surprisal (see [`CALCULATION`]), then the
/// termination. Positions are the retained tokens' own; the drawn stop is in
/// `stop`, not in the steps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trace {
    pub tokens: Vec<u32>,
    pub entropy: Vec<u32>,
    pub surprisal: Vec<u32>,
    pub finish: String,
    pub stop: Option<u32>,
}

impl Trace {
    /// The trace as one report line, values in [`REPRESENTATION`].
    pub fn to_json(&self, id: &str) -> Value {
        let hex = |values: &[u32]| {
            values
                .iter()
                .map(|v| format!("{v:08x}"))
                .collect::<Vec<_>>()
        };
        json!({
            "id": id,
            "tokens": self.tokens,
            "entropy_bits_f32": hex(&self.entropy),
            "surprisal_bits_f32": hex(&self.surprisal),
            "finish_reason": self.finish,
            "stop_token": self.stop,
        })
    }

    /// The trace read back from its report line.
    pub fn from_json(row: &Value) -> Trace {
        let bits = |key: &str| -> Vec<u32> {
            row[key]
                .as_array()
                .unwrap_or_else(|| panic!("a trace carries {key}"))
                .iter()
                .map(|v| {
                    let text = v.as_str().expect("a bit pattern is a string");
                    assert_eq!(text.len(), 8, "a bit pattern is eight hex digits: {text}");
                    u32::from_str_radix(text, 16).expect("a bit pattern is hex")
                })
                .collect()
        };
        Trace {
            tokens: row["tokens"]
                .as_array()
                .expect("a trace carries its tokens")
                .iter()
                .map(token)
                .collect(),
            entropy: bits("entropy_bits_f32"),
            surprisal: bits("surprisal_bits_f32"),
            finish: row["finish_reason"]
                .as_str()
                .expect("a finish reason")
                .into(),
            stop: row["stop_token"]
                .as_u64()
                .map(|t| u32::try_from(t).expect("a token id")),
        }
    }
}

/// **The first place two traces differ, bit for bit**, or `None` when they
/// are bitwise identical. Read position by position (token, then entropy,
/// then surprisal), then the termination. The account names the position and
/// both values, as bit patterns and as the f32 they encode.
pub fn first_numerical_difference(a: &Trace, b: &Trace) -> Option<String> {
    let value = |bits: Option<&u32>| match bits {
        Some(bits) => format!("{bits:08x} ({})", f32::from_bits(*bits)),
        None => "absent".into(),
    };
    let steps = a.tokens.len().max(b.tokens.len());
    for position in 0..steps {
        if a.tokens.get(position) != b.tokens.get(position) {
            return Some(format!(
                "position {position}: token {:?} against {:?}",
                a.tokens.get(position),
                b.tokens.get(position)
            ));
        }
        for (name, x, y) in [
            ("entropy_bits", &a.entropy, &b.entropy),
            ("surprisal_bits", &a.surprisal, &b.surprisal),
        ] {
            if x.get(position) != y.get(position) {
                return Some(format!(
                    "position {position}: {name} {} against {}",
                    value(x.get(position)),
                    value(y.get(position))
                ));
            }
        }
    }
    if (a.entropy.len(), a.surprisal.len()) != (b.entropy.len(), b.surprisal.len()) {
        return Some("the signal vectors differ in length past the tokens".into());
    }
    if (&a.finish, a.stop) != (&b.finish, b.stop) {
        return Some(format!(
            "termination: {} with stop {:?} against {} with stop {:?}",
            a.finish, a.stop, b.finish, b.stop
        ));
    }
    None
}

/// **Bitwise numerical replay of one entry across a repeat pair** (the
/// operator's ruling of 2026-11-08): identical token ids, bit-identical
/// entropy and surprisal at every position, and the same termination. A
/// departure fails naming the first differing position and both values.
pub fn assert_bitwise_replay(first: &Arm, second: &Arm, id: &str) {
    if let Some(difference) = first_numerical_difference(&first.traces[id], &second.traces[id]) {
        panic!(
            "{} and {} do not replay bitwise on {id}: {difference}",
            first.name, second.name
        );
    }
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
        // The entries it completed are on disk already (see [`EntryLog`]),
        // and the record names how far it got. The trace file can hold one
        // entry more than the outputs: an entry whose trace landed and whose
        // measurement check or text step then failed.
        let (completed, last) = completed_entries(&inputs.report, name);
        let traces = std::fs::read_to_string(inputs.report.join(format!("{name}.trace.jsonl")))
            .map(|text| text.lines().count())
            .unwrap_or(0);
        let _ = std::fs::write(
            inputs.report.join(format!("{name}.failed")),
            format!(
                "{status}\nentries completed: {completed}\nlast completed: {}\n\
                 trace lines on disk: {traces}\n",
                last.as_deref().unwrap_or("none")
            ),
        );
        panic!(
            "arm {name} failed: {status} after {completed} entries (last {})",
            last.as_deref().unwrap_or("none")
        );
    }
    read_arm(&inputs.report, name)
}

/// **Each entry's output and numerical trace, on disk as the entry
/// completes** (LORA-6: a failed attempt keeps its evidence). Both files are
/// created fresh, and every append is written, flushed and synced before the
/// next session opens, so an arm that fails on a later entry leaves every
/// completed entry behind.
pub struct EntryLog {
    outputs: std::fs::File,
    traces: std::fs::File,
}

impl EntryLog {
    pub fn create(report: &Path, name: &str) -> EntryLog {
        let open = |file: String| {
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(report.join(&file))
                .unwrap_or_else(|e| panic!("{file} in {}: {e}", report.display()))
        };
        EntryLog {
            outputs: open(format!("{name}.jsonl")),
            traces: open(format!("{name}.trace.jsonl")),
        }
    }

    pub fn append_trace(&mut self, trace: &Value) {
        Self::line(&mut self.traces, trace);
    }

    pub fn append_output(&mut self, output: &Value) {
        Self::line(&mut self.outputs, output);
    }

    fn line(file: &mut std::fs::File, row: &Value) {
        use std::io::Write as _;
        writeln!(file, "{row}").expect("an entry line writes");
        file.flush().expect("an entry line flushes");
        file.sync_data().expect("an entry line reaches the disk");
    }
}

/// **One entry onto the log, its ids and numerical trace first** (LORA-6):
/// the trace line lands before the measurement check and any text step, so a
/// failure in either cannot lose what was decoded. The check then requires
/// one entropy and one surprisal per retained token, or none where nothing
/// was retained: anything else is a measurement this run cannot replay, so
/// the arm fails with no output row and the entry is not counted completed.
/// The output row's text is [`emission_text`] over the emission's bytes,
/// flagged where they are not UTF-8. Returns the fault, `None` where the
/// bytes were UTF-8.
pub fn record_entry(
    log: &mut EntryLog,
    id: &str,
    trace: &Trace,
    mut row: Value,
    emission: impl FnOnce() -> Vec<u8>,
) -> Option<String> {
    log.append_trace(&trace.to_json(id));
    assert!(
        trace.entropy.len() == trace.tokens.len() && trace.surprisal.len() == trace.tokens.len(),
        "entry {id} measured {} entropies and {} surprisals for {} tokens",
        trace.entropy.len(),
        trace.surprisal.len(),
        trace.tokens.len()
    );
    let (text, fault) = emission_text(emission());
    row["output"] = Value::String(text);
    row["output_utf8_valid"] = Value::Bool(fault.is_none());
    if let Some(fault) = &fault {
        row["output_utf8_fault"] = Value::String(fault.clone());
    }
    log.append_output(&row);
    fault
}

/// **The emission's text as the reference recorded it.** The reference
/// decoded with replacement: a model can emit bytes that are not UTF-8 (base
/// Granite does on held-out t004), and each invalid sequence became U+FFFD.
/// Valid bytes come back unchanged with no fault; invalid ones come back
/// replaced, with the strict conversion's first fault.
pub fn emission_text(bytes: Vec<u8>) -> (String, Option<String>) {
    match String::from_utf8(bytes) {
        Ok(text) => (text, None),
        Err(error) => {
            let fault = error.utf8_error().to_string();
            (
                String::from_utf8_lossy(error.as_bytes()).into_owned(),
                Some(fault),
            )
        }
    }
}

/// How many entries an arm left on disk, and the last one's id.
pub fn completed_entries(report: &Path, name: &str) -> (usize, Option<String>) {
    let text = std::fs::read_to_string(report.join(format!("{name}.jsonl"))).unwrap_or_default();
    let ids: Vec<String> = text
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter_map(|row| row["id"].as_str().map(str::to_string))
        .collect();
    (ids.len(), ids.last().cloned())
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
    let mut traces = BTreeMap::new();
    for line in std::fs::read_to_string(report.join(format!("{name}.trace.jsonl")))
        .unwrap_or_else(|e| panic!("arm {name} wrote no numerical trace: {e}"))
        .lines()
    {
        let row: Value = serde_json::from_str(line).expect("a trace line is JSON");
        let id = row["id"]
            .as_str()
            .expect("a trace names its entry")
            .to_string();
        traces.insert(id, Trace::from_json(&row));
    }
    let observation = serde_json::from_str(
        &std::fs::read_to_string(report.join(format!("{name}.json")))
            .unwrap_or_else(|e| panic!("arm {name} wrote no observation: {e}")),
    )
    .expect("the observation is JSON");
    Arm {
        name: name.into(),
        outputs,
        traces,
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
    match fixture.admit(
        &inputs.binding(),
        Headroom(HEADROOM_BYTES),
        CAPACITY,
        ReadoutElection(false),
        // The surprisal is elected so the numerical trace carries it. Every
        // arm elects it, so the election moves no comparison.
        true,
        adapted.then_some(&request),
    ) {
        Ok(_) => {}
        Err(refusal) => {
            stage(&format!("admit refused: {refusal:?}"));
            dump_records(&inputs.report, name, &fixture.records().unwrap_or_default());
            panic!("arm {name} admission refused: {refusal:?}")
        }
    };
    // Shared from here, so the capture reads between sessions.
    let resident = fixture.resident().expect("the admission stands");
    // The load's records end here: the assignment and adapter records. Each
    // session's context then writes the cache records again.
    let load_end = fixture.records().expect("the capture reads").len();
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
    // LORA-3's per-target evidence: an adapted arm placed every target, which
    // the load's judge already matched to its layer's device, and a base arm
    // placed none.
    assert_eq!(
        placement.as_ref().is_some_and(|p| !p.is_empty()),
        adapted,
        "arm {name}: adapter placement does not match the arm"
    );

    let mut log = EntryLog::create(&inputs.report, name);
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
    // **Every session's division is judged when it closes**, against the
    // load's records and that session's own, so a departure fails the arm at
    // the session that shows it rather than after the whole run.
    let mut session_start = load_end;
    let mut observed = None;
    for entry in &selected {
        stage(&format!("decode {}", entry.id));
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
        let mut ids: Vec<u32> = generated.tokens.iter().map(|t| t.0).collect();
        let finish = match generated.stopped {
            Stopped::Complete => {
                ids.push(terminator.0);
                "stop"
            }
            Stopped::LimitReached => "length",
            other => panic!("entry {} ended {other:?}", entry.id),
        };
        // The numerical trace, checked by `record_entry` once it is on disk.
        let bits = |values: &Option<weaver_spu::measurement::NonEmpty<f32>>| -> Vec<u32> {
            values
                .as_ref()
                .map(|v| v.iter().map(|x| x.to_bits()).collect())
                .unwrap_or_default()
        };
        let trace = Trace {
            tokens: generated.tokens.iter().map(|t| t.0).collect(),
            entropy: bits(&generated.signals.entropy_bits),
            surprisal: bits(&generated.signals.surprisal_bits),
            finish: finish.into(),
            stop: generated.drawn_stop.map(|t| t.0),
        };
        // On disk before the text step, the judgment and the next session.
        let row = json!({
            "id": entry.id,
            "token_ids": ids,
            "finish_reason": finish,
            "wall_seconds": started.elapsed().as_secs_f64(),
        });
        let utf8_fault = record_entry(&mut log, &entry.id, &trace, row, || {
            resident
                .emission_bytes(&generated.tokens)
                .expect("the emission's pieces read")
        });
        if let Some(fault) = &utf8_fault {
            eprintln!(
                "arm {name}: {} emitted bytes that are not UTF-8 ({fault}); recorded replaced",
                entry.id
            );
        }
        drop(session);
        let captured = fixture.records().expect("the capture reads");
        match judge_session(
            &captured,
            load_end,
            session_start..captured.len(),
            &inputs.devices,
            &division,
            &caches,
        ) {
            Ok(this) => match &observed {
                None => observed = Some(this),
                Some(first) => assert_eq!(
                    first, &this,
                    "arm {name}: entry {} observed another division",
                    entry.id
                ),
            },
            Err(faults) => {
                stage(&format!("division refused at {}: {faults}", entry.id));
                dump_records(&inputs.report, name, &captured);
                panic!(
                    "arm {name}: the observed division departs from PAIR-3 at {}: {faults}",
                    entry.id
                )
            }
        }
        session_start = captured.len();
        // The raw records so far, so a later failure keeps every judged
        // session's placement evidence too.
        dump_records(&inputs.report, name, &captured);
        eprintln!("arm {name}: {} {} tokens, {finish}", entry.id, ids.len());
    }

    stage("observe");
    let records = fixture.records().expect("the capture holds every record");
    dump_records(&inputs.report, name, &records);
    let observed = observed.expect("an arm decodes at least one entry");
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
        "sessions_judged": selected.len(),
        "records_of_load": load_end,
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
        "input_layer": "token_embd stays in host memory under every assignment; the pinned engine emits no assignment record for it (placement.input_bytes is its size)",
        "interconnect": interconnect(),
        "numerical_trace": {
            "file": format!("{name}.trace.jsonl"),
            "calculation": CALCULATION,
            "representation": REPRESENTATION,
            "comparison": "bitwise between arms of this deployment (the operator's ruling of \
    2026-11-08, bitwise numerical replay); a Python reference is another deployment, so it is \
    compared by token identity only and its numbers are not compared",
        },
        "libllama": libllama(),
    });
    let report = &inputs.report;
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

/// **One session's observed division**, per PAIR-5: the load's records
/// (`..load_end`, the assignment and adapter records) with the session's own
/// (`session`, its context's cache records), judged by
/// `lora::observation::judge_division`. The boundaries are record counts the
/// driver took as it opened and closed each session, not a reading of the
/// log, so a cache that prints a layer more than once per session (a
/// sliding-window model's two caches) does not move them.
pub fn judge_session(
    records: &[Record],
    load_end: usize,
    session: std::ops::Range<usize>,
    devices: &[DeviceOrdinal],
    division: &weaver_spu::division::Division,
    caches: &[weaver_spu::room::Caches],
) -> Result<weaver_spu::lora::observation::ObservedDivision, String> {
    let judged: Vec<Record> = records[..load_end]
        .iter()
        .chain(&records[session])
        .cloned()
        .collect();
    judge_division(&judged, devices, division, caches)
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
