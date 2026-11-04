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

pub(crate) fn judge(
    records: &[Record],
    targets: &BTreeMap<String, u64>,
    devices: &[DeviceOrdinal],
) -> Result<BTreeMap<String, String>, String> {
    let mut placement = BTreeMap::new();
    for record in records {
        let text = std::str::from_utf8(&record.bytes)
            .map_err(|_| "non-UTF-8 backend placement evidence")?;
        for line in text.lines() {
            let Some(value) = line.strip_prefix("llama_adapter_lora_init_impl: lora for '") else {
                continue;
            };
            if value.contains("cannot use buft") && value.contains("fallback to CPU") {
                return Err("adapter fell back to CPU despite successful load".into());
            }
            let Some((target, buffer)) = value.split_once("' -> '") else {
                return Err("unrecognized adapter placement record".into());
            };
            let Some(buffer) = buffer.strip_suffix('\'') else {
                return Err("malformed adapter placement record".into());
            };
            if record.level != llama_cpp_sys_2::GGML_LOG_LEVEL_DEBUG as i32
                || !targets.contains_key(target)
                || !devices.iter().any(|d| buffer == format!("CUDA{}", d.0))
                || placement.insert(target.into(), buffer.into()).is_some()
            {
                return Err(format!(
                    "ambiguous or undeclared placement for {target}: {buffer}"
                ));
            }
        }
    }
    if placement.len() != targets.len() {
        return Err("a requested adapter target has no placement observation".into());
    }
    Ok(placement)
}
