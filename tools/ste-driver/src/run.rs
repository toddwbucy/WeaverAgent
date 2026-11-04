//! Workflow 4.06, 4.07, 6.07, 7.01 and 7.07. Persist the external R0 run.

use crate::conversion::{attempt_json, check_answer, checker_failure, request_json, result_json};
use crate::gate::Transport;
use crate::guard::Judge;
use crate::literal::LiteralCheck;
use crate::mask::{MaskedText, Masker};
use crate::policy::{RoundReport, Terminal, TextConversion};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub const PROMPT_SHA256: &str = "0fe2ee35a59f6b1e30734af4d7a72e32bd6dbc0e728abc98e32e2c609dad07ed";

/// The judge reads the source examples. The fold is checked only as seed data.
pub fn verify_folded_prompt(path: &Path) -> Result<()> {
    let bytes = std::fs::read(path)?;
    if format!("{:x}", Sha256::digest(&bytes)) != PROMPT_SHA256 {
        return Err("folded prompt hash mismatch".into());
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Segment {
    pub id: String,
    pub kind: String,
    pub text: String,
    /// Keep the source's row, message and field locators without rewriting them.
    #[serde(flatten)]
    pub locators: BTreeMap<String, Value>,
}

pub struct Kit {
    segments: Vec<Segment>,
    sha256: String,
    bytes: Vec<u8>,
}

impl Kit {
    pub fn segments(&self) -> &[Segment] {
        &self.segments
    }

    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    /// Read an already-masked, frozen kit. Selection and masking are separate.
    pub fn read(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path)?;
        let text = std::str::from_utf8(&bytes)?;
        let mut segments = Vec::new();
        let mut ids = BTreeSet::new();
        for line in text.lines().filter(|s| !s.trim().is_empty()) {
            let segment: Segment = serde_json::from_str(line)?;
            if segment.id.is_empty() || !ids.insert(segment.id.clone()) {
                return Err("empty or repeated segment id".into());
            }
            if !matches!(segment.kind.as_str(), "narration" | "final") {
                return Err("kit contains an ineligible segment kind".into());
            }
            if segment.text.chars().count() > 6000 {
                return Err("masked segment exceeds 6000 characters".into());
            }
            segments.push(segment);
        }
        if segments.is_empty() {
            return Err("empty kit".into());
        }
        Ok(Self {
            segments,
            sha256: format!("{:x}", Sha256::digest(&bytes)),
            bytes,
        })
    }
}

/// Workflow 4.04 and 4.11. Reject source markers before the mask can confuse them.
pub fn prepare_text(masker: &Masker, source: &str) -> Result<MaskedText> {
    if source.contains("[[C") {
        return Err("marker_collision".into());
    }
    let masked = masker.mask(source)?;
    if masked.text.chars().count() > 6000 {
        return Err("masked segment exceeds 6000 characters".into());
    }
    Ok(masked)
}

/// No default. The caller must supply the three elected R0 values explicitly.
pub struct R0Parameters {
    repairs: u32,
    offers: u32,
    transport_bound: u64,
}

impl R0Parameters {
    pub fn new(repairs: u32, offers: u32, transport_bound: u64) -> Result<Self> {
        if (repairs, offers, transport_bound) != (2, 1, 0) {
            return Err("R0 requires repair cap 2, offer cap 1 and transport bound 0".into());
        }
        Ok(Self {
            repairs,
            offers,
            transport_bound,
        })
    }
}

fn create(path: &Path) -> std::io::Result<File> {
    OpenOptions::new().write(true).create_new(true).open(path)
}

fn append(file: &mut File, value: &Value) -> Result<()> {
    serde_json::to_writer(&mut *file, value)?;
    file.write_all(b"\n")?;
    file.sync_data()?;
    Ok(())
}

fn write_json(path: &Path, value: &Value) -> Result<()> {
    append(&mut create(path)?, value)
}

/// The transport is not called until the parameters and request are persisted.
/// A failed run keeps its journal. Reuse of the output directory is refused.
pub fn evaluate(
    kit: &Kit,
    parameters: &R0Parameters,
    judge: &Judge,
    transport: &mut impl Transport,
    directory: &Path,
) -> Result<RoundReport> {
    std::fs::create_dir(directory)?;
    let result = evaluate_inner(kit, parameters, judge, transport, directory);
    if let Err(error) = &result {
        let _ = write_json(
            &directory.join("failure.json"),
            &json!({
                "status":"incomplete", "error":error.to_string(), "r0_score_allowed":false
            }),
        );
    }
    result
}

fn evaluate_inner(
    kit: &Kit,
    parameters: &R0Parameters,
    judge: &Judge,
    transport: &mut impl Transport,
    directory: &Path,
) -> Result<RoundReport> {
    let mut input = create(&directory.join("input.jsonl"))?;
    input.write_all(&kit.bytes)?;
    input.sync_data()?;
    write_json(
        &directory.join("run.json"),
        &json!({
            "kind":"ste-kit-run", "engine":"weaver-gate", "status":"started",
            "started_unix_seconds":std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?.as_secs(),
            "input_sha256":kit.sha256, "segments":kit.segments.len(),
            "prompt_sha256":PROMPT_SHA256,
            "driver_repair_pass_cap":parameters.repairs, "offer_pass_cap":parameters.offers,
            "shared_request_limit":u64::from(parameters.repairs)+1,
            "transport_stop_bound":parameters.transport_bound, "kill_line_verdict":null,
            "layout_differences":["examples are in the system prompt with changed location wording",
                "repair is one user block"],
            "example_location_before":"The earlier messages are examples.",
            "example_location_after":"The worked examples are at the end of this system prompt.",
            "runtime_differences":["a stop or final missing output cannot reuse an earlier candidate"],
            "generation_accounting":"requires a complete trace read"
        }),
    )?;
    let mut journal = create(&directory.join("journal.jsonl"))?;
    let mut attempts = create(&directory.join("attempts.jsonl"))?;
    let mut first_answers = create(&directory.join("first-gate-answers.jsonl"))?;
    File::open(directory)?.sync_all()?;
    let literals = LiteralCheck::new()?;
    let mut conversions = Vec::new();
    for segment in &kit.segments {
        let mut conversion = TextConversion::new(segment.text.clone(), parameters.repairs);
        while conversion.terminal().is_none() {
            let request = conversion
                .begin_request()
                .map_err(|e| format!("request: {e:?}"))?;
            append(
                &mut journal,
                &json!({"kind":"request", "id":segment.id,
                "prompt_sha256":PROMPT_SHA256, "request":request_json(&request)}),
            )?;
            let capture = transport.send(&request.text);
            if request.ordinal == 1 {
                let answer = capture.answer().ok();
                append(
                    &mut first_answers,
                    &json!({"id":segment.id,
                    "output":answer.as_ref().and_then(|a|a.text.as_deref()),
                    "run":answer.as_ref().map(|a|a.run.as_str()),
                    "turn":answer.as_ref().map(|a|a.turn.as_str()),
                    "prompt_sha256":PROMPT_SHA256, "transport":capture.to_json()}),
                )?;
            }
            append(
                &mut journal,
                &json!({"kind":"response", "id":segment.id,
                "ordinal":request.ordinal, "transport":capture.to_json()}),
            )?;
            let result = match check_answer(capture, judge, &segment.text) {
                Ok(result) => result,
                Err(error) => {
                    append(
                        &mut journal,
                        &json!({"kind":"checker_failure", "id":segment.id,
                        "ordinal":request.ordinal, "check":checker_failure(&error)}),
                    )?;
                    return Err(error.into());
                }
            };
            conversion
                .record_result(request.ordinal, result)
                .map_err(|e| format!("record result: {e:?}"))?;
            append(
                &mut journal,
                &json!({"kind":"checked_attempt", "id":segment.id,
                "prompt_sha256":PROMPT_SHA256,
                "attempt":attempt_json(conversion.attempts().last().expect("recorded attempt"))}),
            )?;
        }
        let mut result = result_json(&segment.id, &segment.text, &conversion, &literals)?;
        result["segment"] = serde_json::to_value(segment)?;
        result["prompt_sha256"] = json!(PROMPT_SHA256);
        append(&mut attempts, &result)?;
        let stopped = conversion.terminal() == Some(Terminal::TransportStopped);
        conversions.push((segment.id.as_str(), conversion));
        // R0's explicit zero bound voids the evaluation at its first stop.
        if stopped {
            break;
        }
    }
    let planned: Vec<_> = kit.segments.iter().map(|s| s.id.as_str()).collect();
    let completed: Vec<_> = conversions.iter().map(|(id, text)| (*id, text)).collect();
    let report = RoundReport::collect(&planned, &completed, parameters.transport_bound)
        .map_err(|e| format!("round report: {e:?}"))?;
    write_json(&directory.join("summary.json"), &report.to_json())?;
    if report.r0_score_allowed() {
        let pending = directory.join("output.pending.jsonl");
        let mut outputs = create(&pending)?;
        for (id, conversion) in &conversions {
            if let Some(row) = conversion.scorer_output(id) {
                append(&mut outputs, &row)?;
            }
        }
        outputs.sync_all()?;
        std::fs::rename(pending, directory.join("output.jsonl"))?;
    }
    File::open(directory)?.sync_all()?;
    Ok(report)
}
