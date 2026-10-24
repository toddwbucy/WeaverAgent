//! HARNESS-OPEN's closed SPU evidence check, shared with diagnostic Open.
//! A successful check establishes wire consistency only. It does not prove the
//! producer used these bytes, confer boot provenance, or license placement.

use serde_json::{Map, Value};
use weaver_types::state_flow::decode_strict;
use weaver_types::{Opened, SessionId};

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum OpenEvidenceError {
    Session,
    Counts,
    Request,
    Measurement,
    TokenCount,
    IdentityBlock,
}

/// Require the named object shape without adding a dependency or a floor
/// measurement mirror. decode_strict has already rejected duplicate keys.
fn object<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a Map<String, Value>> {
    let object = value.as_object()?;
    (object.len() == keys.len() && keys.iter().all(|key| object.contains_key(*key)))
        .then_some(object)
}

pub(crate) fn validate(opened: &Opened, session: &SessionId) -> Result<(), OpenEvidenceError> {
    if &opened.session != session {
        return Err(OpenEvidenceError::Session);
    }
    if opened.prefix_tokens > opened.capacity || opened.terminal_tokens != 1 {
        return Err(OpenEvidenceError::Counts);
    }
    // These temporary parse trees are dropped before returning; no token-vector
    // cache or normalized replacement of the SPU's raw boxes is retained.
    let request: Value =
        decode_strict(opened.request.get()).map_err(|_| OpenEvidenceError::Request)?;
    let request = object(&request, &["rendered", "template"]).ok_or(OpenEvidenceError::Request)?;
    let rendered = request["rendered"]
        .as_str()
        .ok_or(OpenEvidenceError::Request)?;
    if !request["template"].is_string() {
        return Err(OpenEvidenceError::Request);
    }
    let measurement: Value =
        decode_strict(opened.measurement.get()).map_err(|_| OpenEvidenceError::Measurement)?;
    let measurement = object(
        &measurement,
        &["model", "weights_hash", "prefix_token_ids", "blocks"],
    )
    .ok_or(OpenEvidenceError::Measurement)?;
    if !measurement["model"].is_string() || !measurement["weights_hash"].is_string() {
        return Err(OpenEvidenceError::Measurement);
    }
    let tokens = measurement["prefix_token_ids"]
        .as_array()
        .ok_or(OpenEvidenceError::TokenCount)?;
    let count = tokens.iter().try_fold(0_u64, |count, token| {
        let token = token.as_u64().ok_or(OpenEvidenceError::TokenCount)?;
        u32::try_from(token).map_err(|_| OpenEvidenceError::TokenCount)?;
        count.checked_add(1).ok_or(OpenEvidenceError::TokenCount)
    })?;
    if count != opened.prefix_tokens {
        return Err(OpenEvidenceError::TokenCount);
    }
    let blocks = measurement["blocks"]
        .as_array()
        .ok_or(OpenEvidenceError::IdentityBlock)?;
    let [block] = blocks.as_slice() else {
        return Err(OpenEvidenceError::IdentityBlock);
    };
    let block =
        object(block, &["label", "start", "end"]).ok_or(OpenEvidenceError::IdentityBlock)?;
    let end = u64::try_from(rendered.len()).map_err(|_| OpenEvidenceError::IdentityBlock)?;
    if block["label"].as_str() != Some("identity-prefix")
        || block["start"].as_u64() != Some(0)
        || block["end"].as_u64() != Some(end)
    {
        return Err(OpenEvidenceError::IdentityBlock);
    }
    Ok(())
}

#[cfg(test)]
#[path = "open_evidence_tests.rs"]
mod tests;
