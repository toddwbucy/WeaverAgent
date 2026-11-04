//! Workflow 4.06, 6.07 and 7.01. Read external evidence without changing a run.
//! Spec 09 RECORD-OWNERS and TRACE-INSTRUCTION define the measured sources.

use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Deserialize)]
struct Event {
    session: String,
    run: String,
    turn: Option<String>,
    sequence: String,
    kind: String,
    channel: String,
    origin: String,
    subsystem: String,
    causal_parent: Option<String>,
    #[serde(rename = "wall_ms")]
    _wall_ms: u64,
    monotonic_ns: String,
}

struct Record {
    event: Event,
    value: Value,
    sequence: u64,
}

fn decimal(text: &str) -> Option<u64> {
    let value: u64 = text.parse().ok()?;
    (value.to_string() == text).then_some(value)
}

#[derive(Default)]
struct Trace {
    runs: BTreeMap<String, Vec<Record>>,
    defects: BTreeSet<String>,
}

impl Trace {
    fn read(bytes: &[u8]) -> Self {
        let mut trace = Self::default();
        if !bytes.ends_with(b"\n") {
            trace.defects.insert("trace has no final LF".into());
        }
        for (index, line) in bytes.split_inclusive(|b| *b == b'\n').enumerate() {
            let parsed = (|| -> Result<Record> {
                let event: Event = serde_json::from_slice(line)?;
                let sequence = decimal(&event.sequence).ok_or("invalid sequence")?;
                if event.run.is_empty()
                    || event.session.is_empty()
                    || !matches!(
                        event.subsystem.as_str(),
                        "admin" | "harness" | "spu" | "spu_decoder" | "gate" | "tool"
                    )
                    || event.turn.as_deref() == Some("")
                    || decimal(&event.monotonic_ns).is_none()
                    || event
                        .causal_parent
                        .as_deref()
                        .is_some_and(|s| decimal(s).is_none())
                {
                    return Err("invalid trace identity or counter".into());
                }
                Ok(Record {
                    event,
                    value: serde_json::from_slice(line)?,
                    sequence,
                })
            })();
            match parsed {
                Ok(record) => {
                    trace
                        .runs
                        .entry(record.event.run.clone())
                        .or_default()
                        .push(record);
                }
                Err(error) => {
                    trace
                        .defects
                        .insert(format!("trace line {}: {error}", index + 1));
                }
            }
        }
        trace
    }

    fn turn(&self, run: &str, turn: &str) -> Value {
        let mut defects = self.defects.clone();
        let all = self.runs.get(run).map(Vec::as_slice).unwrap_or_default();
        let mut expected = Some(0);
        for record in all {
            if Some(record.sequence) != expected {
                defects.insert("run sequence is not gapless from zero".into());
            }
            expected = record.sequence.checked_add(1);
            if record.event.session != all[0].event.session {
                defects.insert("run changes session identity".into());
            }
        }
        if !all
            .first()
            .is_some_and(|r| r.event.kind == "load" && r.event.turn.is_none())
        {
            defects.insert("run has no load at its start".into());
        }
        let records: Vec<_> = all
            .iter()
            .filter(|r| r.event.turn.as_deref() == Some(turn))
            .collect();
        let starts: Vec<_> = records
            .iter()
            .filter(|r| r.event.kind == "turn.started")
            .collect();
        let closes: Vec<_> = records
            .iter()
            .filter(|r| r.event.kind == "turn.closed")
            .collect();
        if starts.len() != 1
            || closes.len() != 1
            || records
                .first()
                .is_none_or(|r| r.event.kind != "turn.started")
            || records.last().is_none_or(|r| r.event.kind != "turn.closed")
        {
            defects.insert("turn lacks one complete bracket".into());
        }
        if closes.first().is_some_and(|r| {
            !matches!(
                r.value["payload"]["close"].as_str(),
                Some("clean" | "stopped")
            )
        }) {
            defects.insert("turn has an unknown close".into());
        }
        let outputs: Vec<_> = records
            .iter()
            .filter(|r| r.event.kind == "model.output")
            .collect();
        for output in &outputs {
            let event = &output.event;
            let p = &output.value["payload"];
            if event.channel != "output"
                || event.origin != "model"
                || p["emission"].as_str().is_none()
                || !matches!(
                    p["finish"].as_str(),
                    Some("completed" | "stopped" | "length")
                )
                || p["resident"].as_u64().is_none()
                || p["capacity"].as_u64().is_none()
            {
                defects.insert("invalid model.output record".into());
            }
        }
        let measurements: Vec<_> = records
            .iter()
            .filter(|r| r.event.kind == "model.measurement")
            .collect();
        let token_use: Vec<_> = measurements
            .iter()
            .map(|r| {
                let p = &r.value["payload"];
                let count_tokens = |value: &Value| {
                    value
                        .as_array()
                        .filter(|tokens| {
                            tokens
                                .iter()
                                .all(|t| t.as_u64().is_some_and(|n| u32::try_from(n).is_ok()))
                        })
                        .map(Vec::len)
                };
                json!({"sequence":r.event.sequence,
                "input_tokens":count_tokens(&p["input_tokens"]),
                "output_tokens":count_tokens(&p["output_tokens"]),
                "timings":p["timings"]})
            })
            .collect();
        if measurements.len() != outputs.len()
            || measurements.iter().any(|r| {
                r.event.channel != "measurement"
                    || r.event.origin != "runtime"
                    || ["prefill_ns", "decode_ns"].iter().any(|key| {
                        r.value["payload"]["timings"][key]
                            .as_str()
                            .and_then(decimal)
                            .is_none()
                    })
            })
            || token_use
                .iter()
                .any(|v| v["input_tokens"].is_null() || v["output_tokens"].is_null())
        {
            defects.insert("generation measurement evidence is incomplete".into());
        }
        let mut offers = Vec::new();
        for instruction in records.iter().filter(|r| {
            r.event.kind == "loop.instruction"
                && r.value["payload"]["instruction"].get("offer").is_some()
        }) {
            let p = &instruction.value["payload"];
            let source = &p["instruction"]["offer"]["matched"]["source"];
            let source_sequence = source["sequence"].as_str().and_then(decimal);
            let source_record = records.iter().find(|r| Some(r.sequence) == source_sequence);
            let source_block = source["block"]
                .as_str()
                .and_then(decimal)
                .and_then(|b| usize::try_from(b).ok());
            let source_valid = source_record.is_some_and(|r| {
                r.event.kind == "message.assistant"
                    && r.sequence < instruction.sequence
                    && source_block
                        .and_then(|b| r.value["payload"]["content"].get(b))
                        .is_some_and(|b| b["type"] == "text" && b["text"].is_string())
            });
            if p["template_id"] != "thesaurus-offer/1"
                || source["run"] != run
                || instruction.event.channel != "internal"
                || instruction.event.origin != "loop"
                || instruction.event.subsystem != "harness"
                || instruction.event.causal_parent.as_deref().and_then(decimal) != source_sequence
                || !source_valid
            {
                defects.insert("offer instruction source link is invalid".into());
            }
            let responses: Vec<_> = records
                .iter()
                .filter(|r| {
                    r.event.kind == "message.assistant"
                        && r.event.causal_parent.as_deref()
                            == Some(instruction.event.sequence.as_str())
                })
                .collect();
            if responses.len() != 1
                || responses.iter().any(|r| {
                    r.sequence <= instruction.sequence
                        || r.event.channel != "output"
                        || r.event.origin != "model"
                        || r.event.subsystem != "harness"
                })
            {
                defects.insert("offer response link is absent or invalid".into());
            }
            offers.push(json!({"instruction_sequence":instruction.event.sequence,
                "source":source,
                "response_sequences":responses.iter().map(|r| &r.event.sequence).collect::<Vec<_>>()}));
        }
        // A later assistant message needs a recorded offer parent in this loop.
        for response in records
            .iter()
            .filter(|r| r.event.kind == "message.assistant")
            .skip(1)
        {
            if !offers.iter().any(|o| {
                o["response_sequences"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|s| s == &response.event.sequence)
            }) {
                defects.insert("later assistant message has no offer parent".into());
            }
        }
        let count = defects.is_empty().then_some(outputs.len());
        json!({"run":run, "turn":turn, "trace_complete":defects.is_empty(),
            "recorded_generation_count":count, "incomplete_reasons":defects,
            "recorded_token_use":token_use, "offers":offers,
            "first_model_output":outputs.first().map(|r| &r.value),
            "events":records.iter().map(|r| &r.value).collect::<Vec<_>>()})
    }
}

/// Collect only recorded facts. Completeness does not certify the loaded agent.
/// The original files remain the authority for exact bytes and opaque content.
pub fn report(trace_bytes: &[u8], journal_bytes: &[u8]) -> Result<Value> {
    let trace = Trace::read(trace_bytes);
    let mut requests = Vec::new();
    let mut positions = BTreeMap::new();
    if !journal_bytes.ends_with(b"\n") {
        return Err("journal has no final LF".into());
    }
    for line in journal_bytes.split_inclusive(|b| *b == b'\n') {
        let record: Value = serde_json::from_slice(line)?;
        let id = record["id"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or("journal has no id")?;
        match record["kind"].as_str() {
            Some("request") => {
                let ordinal = record["request"]["ordinal"]
                    .as_u64()
                    .filter(|n| *n > 0)
                    .ok_or("invalid request ordinal")?;
                if positions
                    .insert((id.to_owned(), ordinal), requests.len())
                    .is_some()
                {
                    return Err("repeated journal request".into());
                }
                requests.push(json!({"id":id,"ordinal":ordinal,
                    "request":record["request"],"checked_attempt":null}));
            }
            Some("checked_attempt") => {
                let attempt = &record["attempt"];
                let ordinal = attempt["ordinal"]
                    .as_u64()
                    .ok_or("invalid checked ordinal")?;
                let index = positions
                    .get(&(id.to_owned(), ordinal))
                    .ok_or("check has no request")?;
                let request = &mut requests[*index];
                if !request["checked_attempt"].is_null() {
                    return Err("repeated checked attempt".into());
                }
                for field in ["ordinal", "kind", "text"] {
                    if request["request"][field] != attempt[field] {
                        return Err("checked attempt differs from its request".into());
                    }
                }
                request["checked_attempt"] = attempt.clone();
            }
            Some("response" | "checker_failure") => {}
            _ => return Err("unknown journal record kind".into()),
        }
    }
    if requests.is_empty() {
        return Err("journal contains no requests".into());
    }
    let mut identities = BTreeSet::new();
    for request in &mut requests {
        let response = &request["checked_attempt"]["result"]["response"];
        let evidence = match (response["run"].as_str(), response["turn"].as_str()) {
            (Some(run), Some(turn)) if !run.is_empty() && !turn.is_empty() => {
                if !identities.insert((run.to_owned(), turn.to_owned())) {
                    return Err("gate requests share a trace turn identity".into());
                }
                trace.turn(run, turn)
            }
            _ => json!({"trace_complete":false, "recorded_generation_count":null,
                "incomplete_reasons":["request has no checked run and turn identity"]}),
        };
        request["trace"] = evidence;
    }
    Ok(json!({"kind":"ste-trace-report",
        "trace_sha256":format!("{:x}",Sha256::digest(trace_bytes)),
        "journal_sha256":format!("{:x}",Sha256::digest(journal_bytes)),
        "complete":requests.iter().all(|r| r["trace"]["trace_complete"] == true),
        "requests":requests}))
}

/// Write a new offline evidence deposit. Never rewrite the evaluation or trace.
pub fn collect(trace: &Path, journal: &Path, output: &Path) -> Result<bool> {
    let trace_bytes = std::fs::read(trace)?;
    let journal_bytes = std::fs::read(journal)?;
    let report = report(&trace_bytes, &journal_bytes)?;
    let mut encoded = serde_json::to_vec_pretty(&report)?;
    encoded.push(b'\n');
    std::fs::create_dir(output)?;
    for (name, bytes) in [
        ("trace.jsonl", trace_bytes),
        ("journal.jsonl", journal_bytes),
        ("trace-report.json", encoded),
    ] {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output.join(name))?;
        file.write_all(&bytes)?;
        file.sync_all()?;
    }
    File::open(output)?.sync_all()?;
    Ok(report["complete"] == true)
}
