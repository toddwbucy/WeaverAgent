//! Workflow 6.06, 7.07, 7.08 and 7.09. Split a frozen kit into a chain of batches,
//! and check a batch evaluation's place in its chain.
//!
//! The splitter cuts only between trajectories, so a trajectory is never in two
//! batches. Each batch is a consecutive run of the kit's own lines, so the batch
//! files joined in index order are the kit's exact bytes. The manifest is written
//! last, through a pending file and a rename, so a directory that holds
//! `chain.json` holds every batch it names.

use crate::run::{Kit, Segment};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// The manifest's file name in a split directory.
pub const MANIFEST: &str = "chain.json";

/// One batch of a chain, as the manifest names it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Batch {
    pub index: usize,
    pub file: String,
    pub sha256: String,
    pub segments: usize,
    pub first_segment: String,
    pub last_segment: String,
    pub trajectories: Vec<String>,
}

/// The chain manifest: the parent kit's hash and every batch in index order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub kit_sha256: String,
    pub segments: usize,
    pub trajectories: usize,
    pub batch_trajectories: usize,
    pub batches: Vec<Batch>,
}

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// One batch while the kit is read: its byte range and what it holds.
struct Cut {
    start: usize,
    end: usize,
    segments: usize,
    first_segment: String,
    last_segment: String,
    trajectories: Vec<String>,
}

/// The trajectory a kit segment belongs to: its `uuid` locator, a nonempty string.
fn trajectory(segment: &Segment) -> Result<String> {
    segment
        .locators
        .get("uuid")
        .and_then(Value::as_str)
        .filter(|uuid| !uuid.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| "kit segment has no trajectory uuid".into())
}

/// Cut the kit's bytes between trajectories, `batch_trajectories` to a batch.
/// The lines are the ones `Kit::read` parsed, met in its order and under its blank
/// rule, so each nonblank line is the kit's next segment. A blank line stays with
/// the batch of the line before it, so no byte is lost.
fn cuts(kit: &Kit, batch_trajectories: usize) -> Result<(Vec<Cut>, usize)> {
    let mut cuts: Vec<Cut> = Vec::new();
    let mut seen = BTreeSet::new();
    let mut current: Option<String> = None;
    let mut segments = kit.segments().iter();
    let mut offset = 0;
    for raw in kit.bytes().split_inclusive(|byte| *byte == b'\n') {
        let start = offset;
        offset += raw.len();
        if std::str::from_utf8(raw)?.trim().is_empty() {
            if let Some(cut) = cuts.last_mut() {
                cut.end = offset;
            }
            continue;
        }
        let segment = segments
            .next()
            .ok_or("the kit has more lines than segments")?;
        let (id, uuid) = (segment.id.clone(), trajectory(segment)?);
        if current.as_deref() != Some(uuid.as_str()) {
            if !seen.insert(uuid.clone()) {
                return Err(
                    format!("trajectory {uuid} is not one consecutive run of the kit").into(),
                );
            }
            current = Some(uuid.clone());
            let full = cuts
                .last()
                .is_none_or(|cut| cut.trajectories.len() == batch_trajectories);
            if full {
                // The first batch also holds any blank lines before its first segment.
                let start = if cuts.is_empty() { 0 } else { start };
                cuts.push(Cut {
                    start,
                    end: offset,
                    segments: 0,
                    first_segment: id.clone(),
                    last_segment: id.clone(),
                    trajectories: Vec::new(),
                });
            }
            let cut = cuts.last_mut().ok_or("no batch was started")?;
            cut.trajectories.push(uuid);
        }
        let cut = cuts.last_mut().ok_or("no batch was started")?;
        cut.end = offset;
        cut.segments += 1;
        cut.last_segment = id;
    }
    if segments.next().is_some() {
        return Err("the kit has more segments than lines".into());
    }
    Ok((cuts, seen.len()))
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_data()?;
    Ok(())
}

/// Workflow 7.09. Split the frozen kit at `kit` into batches of whole trajectories in
/// a new directory `output`, and write the manifest last. Returns the manifest and
/// the digest of the bytes written as `chain.json`.
/// Every check of the kit finishes before the directory is created. A failure after
/// that leaves the directory without `chain.json`, which marks it incomplete.
pub fn split(
    kit: &Path,
    expected_sha256: &str,
    batch_trajectories: usize,
    output: &Path,
) -> Result<(Manifest, String)> {
    if batch_trajectories == 0 {
        return Err("a batch must hold at least one trajectory".into());
    }
    let parent = Kit::read(kit)?;
    if parent.sha256() != expected_sha256 {
        return Err("frozen kit hash mismatch".into());
    }
    let bytes = parent.bytes();
    let (cuts, trajectories) = cuts(&parent, batch_trajectories)?;
    if cuts.first().map(|cut| cut.start) != Some(0)
        || cuts.last().map(|cut| cut.end) != Some(bytes.len())
        || cuts.windows(2).any(|pair| pair[0].end != pair[1].start)
    {
        return Err("the batches do not cover the kit's bytes exactly".into());
    }
    let width = cuts.len().saturating_sub(1).to_string().len().max(4);
    let batches: Vec<Batch> = cuts
        .iter()
        .enumerate()
        .map(|(index, cut)| Batch {
            index,
            file: format!("batch-{index:0width$}.jsonl"),
            sha256: hash(&bytes[cut.start..cut.end]),
            segments: cut.segments,
            first_segment: cut.first_segment.clone(),
            last_segment: cut.last_segment.clone(),
            trajectories: cut.trajectories.clone(),
        })
        .collect();
    let manifest = Manifest {
        kit_sha256: parent.sha256().to_owned(),
        segments: parent.segments().len(),
        trajectories,
        batch_trajectories,
        batches,
    };
    std::fs::create_dir(output)?;
    for (batch, cut) in manifest.batches.iter().zip(&cuts) {
        let path = output.join(&batch.file);
        write_new(&path, &bytes[cut.start..cut.end])?;
        let written = Kit::read(&path)?;
        if written.sha256() != batch.sha256 || written.segments().len() != batch.segments {
            return Err(format!("batch {} does not read back as written", batch.index).into());
        }
    }
    let mut text = serde_json::to_vec_pretty(&manifest)?;
    text.push(b'\n');
    let pending = output.join(format!("{MANIFEST}.pending"));
    write_new(&pending, &text)?;
    std::fs::rename(&pending, output.join(MANIFEST))?;
    File::open(output)?.sync_all()?;
    Ok((manifest, hash(&text)))
}

/// Workflow 7.09. Check a batch evaluation's place in its chain before it starts:
/// the manifest's hash, its shape, and that the batch at `index` is this kit.
/// Returns the manifest's entry for that batch, for the evaluation's record.
pub fn check(
    manifest: &Path,
    expected_sha256: &str,
    index: usize,
    kit_sha256: &str,
) -> Result<Batch> {
    let bytes = std::fs::read(manifest)?;
    if hash(&bytes) != expected_sha256 {
        return Err("chain manifest hash mismatch".into());
    }
    let chain: Manifest = serde_json::from_slice(&bytes)?;
    let indexed = chain
        .batches
        .iter()
        .enumerate()
        .all(|(place, batch)| batch.index == place);
    let mut files = BTreeSet::new();
    let mut seen = BTreeSet::new();
    let shaped = chain.batches.iter().all(|batch| {
        batch.segments > 0
            && !batch.trajectories.is_empty()
            && batch.trajectories.len() <= chain.batch_trajectories
            && files.insert(batch.file.as_str())
            && batch
                .trajectories
                .iter()
                .all(|uuid| seen.insert(uuid.as_str()))
    });
    let counted = chain
        .batches
        .iter()
        .map(|batch| batch.segments)
        .sum::<usize>()
        == chain.segments
        && seen.len() == chain.trajectories;
    let full = chain.batches.split_last().is_some_and(|(_, earlier)| {
        earlier
            .iter()
            .all(|batch| batch.trajectories.len() == chain.batch_trajectories)
    });
    if chain.batches.is_empty() || !indexed || !shaped || !counted || !full {
        return Err("chain manifest is not a chain of whole trajectories".into());
    }
    let batch = chain
        .batches
        .get(index)
        .ok_or("chain batch index is not in the manifest")?;
    if batch.sha256 != kit_sha256 {
        return Err("kit is not the chain's batch at that index".into());
    }
    Ok(batch.clone())
}
