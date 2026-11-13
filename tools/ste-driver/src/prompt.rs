//! Workflow 4.07 and 6.01. Read the pinned source bundle for Judge's examples.

use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::Path;

pub const SYSTEM_SHA256: &str = "0a42709ebe47ef8f17d728921b8bd802e0bf1466fced8d020b0b3c6dec6aa701";
pub const BUNDLE_SHA256: &str = "e21370fcd140a4bb5e7a0d2dd415c58d8ecead9333a9e730adb66ab96e7a851c";
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub fn source_examples(path: &Path) -> Result<Vec<String>> {
    decode(&std::fs::read(path)?, SYSTEM_SHA256, BUNDLE_SHA256)
}

fn decode(bytes: &[u8], system_sha: &str, bundle_sha: &str) -> Result<Vec<String>> {
    let value: Value = serde_json::from_slice(bytes)?;
    let system = value["system"]
        .as_str()
        .ok_or("missing source system text")?;
    if format!("{:x}", Sha256::digest(system.as_bytes())) != system_sha {
        return Err("source system hash mismatch".into());
    }
    if value["meta"]["limit_instruction"] != 20 || value["meta"]["limit_description"] != 25 {
        return Err("source prompt limits are not 20 and 25".into());
    }
    let messages = value["examples"]
        .as_array()
        .ok_or("missing source examples")?;
    if messages.len() != 8 {
        return Err("source bundle must have four example pairs".into());
    }
    let mut records = Vec::new();
    let mut examples = Vec::new();
    for (index, message) in messages.iter().enumerate() {
        let role = if index % 2 == 0 { "user" } else { "assistant" };
        if message["role"] != role {
            return Err("source example roles are not in paired order".into());
        }
        let content = message["content"]
            .as_str()
            .ok_or("source example lacks text")?;
        records.push(format!(
            "{{\"role\": {}, \"content\": {}}}",
            quoted(role),
            quoted(content)
        ));
        if role == "assistant" {
            examples.push(content.to_owned());
        }
    }
    // ste_prompt.main hashes Python json.dumps([system, examples]).
    let rendered = format!("[{}, [{}]]", quoted(system), records.join(", "));
    if format!("{:x}", Sha256::digest(rendered.as_bytes())) != bundle_sha {
        return Err("source bundle hash mismatch".into());
    }
    Ok(examples)
}

/// Python's default JSON string spelling, including UTF-16 escape pairs.
fn quoted(text: &str) -> String {
    let mut result = String::from("\"");
    for ch in text.chars() {
        match ch {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\x08' => result.push_str("\\b"),
            '\x0c' => result.push_str("\\f"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            ' '..='~' => result.push(ch),
            other => {
                for unit in other.encode_utf16(&mut [0; 2]) {
                    use std::fmt::Write;
                    write!(&mut result, "\\u{unit:04x}").expect("String write");
                }
            }
        }
    }
    result.push('"');
    result
}

#[cfg(test)]
mod tests;
