//! Dormant admission-to-lifecycle value adapter. No recorder, channel, deadline,
//! readiness or rollback owner is created here. The caller retains the original
//! checked admission and exchange; only its later Recorded author permits send.
use crate::failure::ChannelFault;
use weaver_types::state_flow::{
    StateManagementCode, StateManagementStage, Ste100AdmissionCode, Ste100AdmissionRefused,
    ValidationError,
};
use weaver_types::{
    ExchangeId, LifecycleRefusal, MAX_ENVELOPE_BYTES, OrganEnvelope, Payload, Position,
};

// Keep the actual oversized envelope inline; a failing preflight must not
// require a second allocation merely to return its original.
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub(super) enum AdmissionRefusalError {
    InvalidSource(ValidationError),
    /// The actual whole envelope is returned intact, including correlation and
    /// all original detail. The borrowed checker evidence remains at the caller.
    Envelope {
        envelope: OrganEnvelope,
        fault: ChannelFault,
    },
}

/// Prepare the exact close envelope without authoring or sending it. It is a
/// value, not a Recorded proof. Existing transport staging must still enforce
/// its own whole-envelope bound when the owning caller later sends it.
#[allow(clippy::result_large_err)]
pub(super) fn prepare_admission_refusal(
    exchange: &ExchangeId,
    source: &Ste100AdmissionRefused,
) -> Result<OrganEnvelope, AdmissionRefusalError> {
    source
        .validate()
        .map_err(AdmissionRefusalError::InvalidSource)?;
    let (code, checker_code) = match source.code {
        Ste100AdmissionCode::SchemeMismatch => {
            (StateManagementCode::SchemeMismatch, "scheme_mismatch")
        }
        Ste100AdmissionCode::UnsupportedArtifact => {
            (StateManagementCode::TemplateFailed, "unsupported_artifact")
        }
        Ste100AdmissionCode::InvalidGrammar => {
            (StateManagementCode::TemplateFailed, "invalid_grammar")
        }
        Ste100AdmissionCode::InsufficientCapacity => {
            (StateManagementCode::TemplateFailed, "insufficient_capacity")
        }
        Ste100AdmissionCode::InvalidInstruction => {
            (StateManagementCode::TemplateFailed, "invalid_instruction")
        }
    };
    // This readable prefix is an implementation rendering of DECLARATION's
    // required code plus exact detail. It adds no parsed sub-protocol: the
    // source detail remains byte-for-byte unchanged, including failed paths.
    let detail = format!("{checker_code}: {}", source.detail);
    let envelope = OrganEnvelope {
        exchange: exchange.clone(),
        position: Position::Close,
        payload: Payload::Refusal(LifecycleRefusal::StateManagementInvalid {
            stage: StateManagementStage::Template,
            code,
            rule_id: None,
            path: None,
            detail,
        }),
    };
    let bytes = match serde_json::to_vec(&envelope) {
        Ok(bytes) => bytes,
        Err(_) => {
            return Err(AdmissionRefusalError::Envelope {
                envelope,
                fault: ChannelFault::Undecodable,
            });
        }
    };
    if bytes.len() > MAX_ENVELOPE_BYTES {
        return Err(AdmissionRefusalError::Envelope {
            envelope,
            fault: ChannelFault::Truncated {
                bound: MAX_ENVELOPE_BYTES,
            },
        });
    }
    Ok(envelope)
}

#[cfg(test)]
#[path = "lifecycle_refusal_tests.rs"]
mod tests;
