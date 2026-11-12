//! Workflow 4.02, 4.04, 4.11, 6.06 and 7.01. Retain source and mask tables.
//! This prepares supplied segments. It does not select or qualify dataset rows.

use crate::{mask::Masker, run::prepare_text};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Deserialize)]
struct SourceSegment {
    id: String,
    kind: String,
    uuid: String,
    loc: Value,
    raw: String,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn line(bytes: &mut Vec<u8>, value: &impl Serialize) -> Result<()> {
    serde_json::to_writer(&mut *bytes, value)?;
    bytes.push(b'\n');
    Ok(())
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_data()?;
    Ok(())
}

/// Verify the frozen source before masking. A failure never selects another row.
/// All source checks finish before a new output directory is created.
pub fn prepare(source: &Path, expected_sha256: &str, output: &Path) -> Result<Value> {
    let source_bytes = std::fs::read(source)?;
    let source_sha256 = hash(&source_bytes);
    if source_sha256 != expected_sha256 {
        return Err("frozen source hash mismatch".into());
    }
    let source_text = std::str::from_utf8(&source_bytes)?;
    let masker = Masker::new()?;
    let mut ids = BTreeSet::new();
    let mut uuids = BTreeSet::new();
    let mut input = Vec::new();
    let mut codes = Vec::new();
    let mut kinds = BTreeMap::<String, usize>::new();
    let mut bins = [0usize; 3];
    let mut max_characters = 0;
    for text in source_text.lines().filter(|text| !text.trim().is_empty()) {
        let row: SourceSegment = serde_json::from_str(text)?;
        if row.id.is_empty() || !ids.insert(row.id.clone()) {
            return Err("empty or repeated segment id".into());
        }
        // A trajectory has many prose segments, so its UUID repeats across them
        // (workflow 6.06 and 7.01). A segment's place, its UUID and locator, must
        // still be unique.
        if row.uuid.is_empty() || !uuids.insert((row.uuid.clone(), row.loc.to_string())) {
            return Err("empty trajectory uuid or repeated segment locator".into());
        }
        let field = match row.kind.as_str() {
            "narration" => "content",
            "final" => "finish",
            _ => return Err("ineligible segment kind".into()),
        };
        if row.loc.get("msg").and_then(Value::as_u64).is_none()
            || row.loc.get("field").and_then(Value::as_str) != Some(field)
            || (field == "finish" && row.loc.get("call").and_then(Value::as_u64).is_none())
        {
            return Err("invalid segment locator".into());
        }
        if row.extra.contains_key("text") {
            return Err("source contains reserved text field".into());
        }
        let masked = prepare_text(&masker, &row.raw)?;
        let characters = masked.text.chars().count();
        max_characters = max_characters.max(characters);
        bins[usize::from(characters >= 200) + usize::from(characters >= 500)] += 1;
        *kinds.entry(row.kind.clone()).or_default() += 1;
        line(&mut codes, &json!({"id":row.id,"codes":masked.codes}))?;
        let mut locators = row.extra;
        locators.insert("uuid".into(), json!(row.uuid));
        locators.insert("loc".into(), row.loc);
        let segment = crate::run::Segment {
            id: row.id,
            kind: row.kind,
            text: masked.text,
            locators,
        };
        line(&mut input, &segment)?;
    }
    if ids.is_empty() {
        return Err("empty source kit".into());
    }
    let report = json!({
        "source_revision":"9881da6cfbc92b08b31264883aea63b73aa7b694",
        "source_sha256":source_sha256,
        "input_sha256":hash(&input), "codes_sha256":hash(&codes),
        "segments":ids.len(), "kinds":kinds,
        "masked_character_bins":bins, "max_masked_characters":max_characters,
        "qualification":"not assessed"
    });
    let mut report_bytes = Vec::new();
    line(&mut report_bytes, &report)?;
    std::fs::create_dir(output)?;
    let deposit = || -> Result<()> {
        write_new(&output.join("source.jsonl"), &source_bytes)?;
        write_new(&output.join("input.jsonl"), &input)?;
        write_new(&output.join("codes.jsonl"), &codes)?;
        write_new(&output.join("preparation.json"), &report_bytes)?;
        Ok(())
    };
    if let Err(error) = deposit() {
        let mut failure = Vec::new();
        if line(
            &mut failure,
            &json!({"status":"incomplete","error":error.to_string()}),
        )
        .is_ok()
        {
            let _ = write_new(&output.join("failure.json"), &failure);
        }
        return Err(error);
    }
    Ok(report)
}
