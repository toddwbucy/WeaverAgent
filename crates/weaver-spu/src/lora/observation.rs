//! Process-lifetime fixture capture and placement qualification, per LORA-3.

use std::collections::BTreeMap;
use std::ffi::{CStr, c_char, c_void};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use weaver_types::DeviceOrdinal;

#[derive(Debug, Clone)]
pub struct Record {
    pub level: i32,
    pub bytes: Vec<u8>,
}

static RECORDS: OnceLock<Mutex<Vec<Record>>> = OnceLock::new();
static CAPTURE_FAILED: AtomicBool = AtomicBool::new(false);

/// Install once, before any admission, in an isolated fixture process.
/// The static allocation survives contexts, models and in-flight callbacks.
pub(super) fn install() -> Result<(), String> {
    if crate::decoder::gguf::backend_started() {
        return Err("fixture observation must precede backend admission".into());
    }
    RECORDS
        .set(Mutex::new(Vec::new()))
        .map_err(|_| "fixture capture already installed")?;
    // LORA-3: no callback replacement or teardown is permitted in this process.
    unsafe { llama_cpp_sys_2::llama_log_set(Some(capture), std::ptr::null_mut()) };
    Ok(())
}

unsafe extern "C" fn capture(level: u32, text: *const c_char, _data: *mut c_void) {
    // No backend calls or unwinding across the C boundary, per LORA-3.
    let result = std::panic::catch_unwind(|| {
        if text.is_null() {
            return;
        }
        let Some(records) = RECORDS.get() else {
            return;
        };
        // The backend owns a NUL-terminated string for this callback's duration.
        let bytes = unsafe { CStr::from_ptr(text) }.to_bytes().to_vec();
        records
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(Record {
                level: level as i32,
                bytes,
            });
    });
    if result.is_err() {
        CAPTURE_FAILED.store(true, Ordering::SeqCst);
    }
}

pub(crate) fn records() -> Result<Vec<Record>, String> {
    if CAPTURE_FAILED.load(Ordering::SeqCst) {
        return Err("fixture callback lost a record".into());
    }
    let records = RECORDS
        .get()
        .ok_or("fixture placement observer is absent")?;
    Ok(records
        .lock()
        .map_err(|_| "fixture capture failed")?
        .clone())
}

/// **Each target's buffer is matched to the device the division gives its
/// layer**, per LORA-3 as the device pair amendment completes it (PAIR-5):
/// a layer's target to that layer's device, an output target to the last
/// device. A target this crate does not classify to one device must sit on
/// some member. Under the PAIR-1 mask the engine names the device at binding
/// index i `CUDA{i}`, and the placement returned names the binding's own
/// ordinal.
///
/// **An adapter placement line that is not UTF-8 refuses**, read by the
/// rule [`judge_division`] states: a record that is not UTF-8 is read
/// lossily, and only a placement line in it is placement evidence. The
/// capture holds the loader's metadata dump, whose values are cut at 37
/// bytes and can end inside a character, so refusing every invalid record
/// would refuse a sound load over a vocabulary. A placement line whose
/// prefix is the corrupted part is not read, and its target then has no
/// observation, which refuses.
pub(crate) fn judge(
    records: &[Record],
    targets: &BTreeMap<String, u64>,
    devices: &[DeviceOrdinal],
    division: &crate::division::Division,
) -> Result<BTreeMap<String, DeviceOrdinal>, String> {
    use crate::room::{Placed, placed};
    let mut placement = BTreeMap::new();
    for record in records {
        let valid = std::str::from_utf8(&record.bytes).is_ok();
        let text = String::from_utf8_lossy(&record.bytes);
        for line in text.lines() {
            let Some(value) = line.strip_prefix("llama_adapter_lora_init_impl: lora for '") else {
                continue;
            };
            if !valid && line.contains(char::REPLACEMENT_CHARACTER) {
                return Err(format!("non-UTF-8 adapter placement record: {line}"));
            }
            if value.contains("cannot use buft") && value.contains("fallback to CPU") {
                return Err("adapter fell back to CPU despite successful load".into());
            }
            let Some((target, buffer)) = value.split_once("' -> '") else {
                return Err("unrecognized adapter placement record".into());
            };
            let Some(buffer) = buffer.strip_suffix('\'') else {
                return Err("malformed adapter placement record".into());
            };
            let observed = (0..devices.len()).find(|i| buffer == format!("CUDA{i}"));
            let matches = match (placed(target, division), observed) {
                (Placed::Device(expected), Some(index)) => expected == index,
                (Placed::Unclassified, Some(_)) => true,
                _ => false,
            };
            if record.level != llama_cpp_sys_2::GGML_LOG_LEVEL_DEBUG as i32
                || !targets.contains_key(target)
                || !matches
                || placement
                    .insert(target.into(), devices[observed.unwrap_or(0)].clone())
                    .is_some()
            {
                return Err(format!(
                    "ambiguous, undeclared or misplaced placement for {target}: {buffer}"
                ));
            }
        }
    }
    if placement.len() != targets.len() {
        return Err("a requested adapter target has no placement observation".into());
    }
    Ok(placement)
}

/// The device each assignment and cache record named, as the PAIR-5
/// observed division found it, by the binding's own ordinal (PAIR-1).
#[derive(Debug, Clone, PartialEq)]
pub struct ObservedDivision {
    /// The device of each `load_tensors` assignment record, slots 0 to L,
    /// slot L being the output layer.
    pub slots: Vec<DeviceOrdinal>,
    /// The device of each layer's attention cache record, `None` for a layer
    /// that holds no attention cache.
    pub attention: Vec<Option<DeviceOrdinal>>,
    /// The device of each layer's recurrent cache record, `None` for a layer
    /// that holds no recurrent cache.
    pub recurrent: Vec<Option<DeviceOrdinal>>,
}

/// The engine's three record shapes, per the pinned `llama-model.cpp`,
/// `llama-kv-cache.cpp` and `llama-memory-recurrent.cpp`. The KV cache's
/// sliding-window and full caches print the same prefix, each naming the
/// layers it holds and calling the others filtered.
const ASSIGNED: &str = "load_tensors: layer ";
const ATTENTION: &str = "llama_kv_cache: layer ";
const RECURRENT: &str = "llama_memory_recurrent, layer ";

/// **The observed division matches PAIR-3**, per the device pair amendment
/// PAIR-5. From the LORA-3 capture at debug level: exactly one assignment
/// record for each slot 0 to L, and for each layer exactly the cache records
/// `caches` predicts, every one naming `CUDA{i}` for the device i that
/// `division` gives the slot: under the PAIR-1 mask the engine names the
/// device at binding index i `CUDA{i}`, and the account names the binding's
/// ordinal `devices[i]`. A predicted record that reads "does not have
/// KV cache", "filtered" or "skipped" is missing. A `dev =` record the
/// classification does not predict, a second record from the same cache, a
/// record at another level, or a layer past the depth fails.
///
/// **A placement line that is not UTF-8 fails, and no record is skipped.**
/// A record that is not UTF-8 is read lossily, line by line: an assignment
/// or cache line in it fails, and any other line is not placement evidence.
/// [`judge`] reads adapter placement lines by the same rule. The loader cuts each metadata value it
/// prints at 37 bytes (`llama-model-loader.cpp`), which can split a
/// character, so an invalid record is not by that alone a placement fault.
///
/// Every fault is named, not only the first, because the instrument runs on
/// a leased device pair and one run should carry the whole account.
pub fn judge_division(
    records: &[Record],
    devices: &[DeviceOrdinal],
    division: &crate::division::Division,
    caches: &[crate::room::Caches],
) -> Result<ObservedDivision, String> {
    let layers = division.layers() as usize;
    if devices.len() != division.counts().len() {
        return Err(format!(
            "the binding holds {} devices where the division holds {}",
            devices.len(),
            division.counts().len()
        ));
    }
    if caches.len() != layers {
        return Err(format!(
            "the classification holds {} layers where the division holds {layers}",
            caches.len()
        ));
    }
    let mut faults = Vec::new();
    let mut slots: Vec<Vec<String>> = vec![Vec::new(); layers + 1];
    let mut attention: Vec<Vec<String>> = vec![Vec::new(); layers];
    let mut recurrent: Vec<Vec<String>> = vec![Vec::new(); layers];
    for record in records {
        let valid = std::str::from_utf8(&record.bytes).is_ok();
        let text = String::from_utf8_lossy(&record.bytes);
        for line in text.lines() {
            let (kind, slot, device) = if let Some(rest) = line.strip_prefix(ASSIGNED) {
                let Some((slot, rest)) = rest.split_once(" assigned to device ") else {
                    faults.push(format!("unrecognized assignment record: {line}"));
                    continue;
                };
                let device = rest.split_once(", is_swa = ").map_or(rest, |(d, _)| d);
                ("assignment", slot, Some(device))
            } else if let Some(rest) = line.strip_prefix(ATTENTION) {
                let Some((layer, rest)) = rest.split_once(": ") else {
                    faults.push(format!("unrecognized attention cache record: {line}"));
                    continue;
                };
                ("attention cache", layer, rest.strip_prefix("dev = "))
            } else if let Some(rest) = line.strip_prefix(RECURRENT) {
                let Some((layer, rest)) = rest.split_once(": ") else {
                    faults.push(format!("unrecognized recurrent cache record: {line}"));
                    continue;
                };
                ("recurrent cache", layer, rest.strip_prefix("dev = "))
            } else {
                continue;
            };
            if !valid && line.contains(char::REPLACEMENT_CHARACTER) {
                faults.push(format!("{kind} record is not UTF-8: {line}"));
                continue;
            }
            // "does not have KV cache", "filtered" and "skipped" place
            // nothing, so they count toward no layer and leave it missing.
            let Some(device) = device else {
                continue;
            };
            if record.level != llama_cpp_sys_2::GGML_LOG_LEVEL_DEBUG as i32 {
                faults.push(format!("{kind} record at level {}: {line}", record.level));
                continue;
            }
            let Ok(slot) = slot.trim().parse::<usize>() else {
                faults.push(format!("{kind} record names no layer: {line}"));
                continue;
            };
            let table = match kind {
                "assignment" => &mut slots,
                "attention cache" => &mut attention,
                _ => &mut recurrent,
            };
            match table.get_mut(slot) {
                Some(seen) => seen.push(device.to_string()),
                None => faults.push(format!("{kind} record past the depth: {line}")),
            }
        }
    }
    let mut judge =
        |kind: &str, slot: usize, seen: &[String], predicted: bool| -> Option<DeviceOrdinal> {
            let expected = division.index_of(slot as u32)?;
            let ordinal = devices[expected].0;
            match (predicted, seen) {
                (false, []) => None,
                (false, _) => {
                    faults.push(format!(
                        "layer {slot}: {kind} record not predicted: {seen:?}"
                    ));
                    None
                }
                (true, []) => {
                    faults.push(format!("layer {slot}: {kind} record missing"));
                    None
                }
                (true, [device]) if *device == format!("CUDA{expected}") => {
                    Some(devices[expected].clone())
                }
                (true, [device]) => {
                    faults.push(format!(
                        "layer {slot}: {kind} on {device}, where the division gives ordinal \
                     {ordinal} (CUDA{expected} under the mask)"
                    ));
                    None
                }
                (true, _) => {
                    faults.push(format!(
                        "layer {slot}: {kind} recorded {} times: {seen:?}",
                        seen.len()
                    ));
                    None
                }
            }
        };
    let observed = ObservedDivision {
        // A slot without its record is a fault, so on success every slot has
        // its ordinal and none is dropped.
        slots: slots
            .iter()
            .enumerate()
            .filter_map(|(slot, seen)| judge("assignment", slot, seen, true))
            .collect(),
        attention: attention
            .iter()
            .enumerate()
            .map(|(layer, seen)| judge("attention cache", layer, seen, caches[layer].attention))
            .collect(),
        recurrent: recurrent
            .iter()
            .enumerate()
            .map(|(layer, seen)| judge("recurrent cache", layer, seen, caches[layer].recurrent))
            .collect(),
    };
    if faults.is_empty() {
        Ok(observed)
    } else {
        Err(faults.join("; "))
    }
}
