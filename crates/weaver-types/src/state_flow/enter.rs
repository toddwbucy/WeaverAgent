//! The exact replacement Enter carrier, held for FOUNDATION-TRANSITION.
//! Its closed codec accepts only TYPES-ENTER's shape. The active legacy lifecycle
//! parser does not select it; coupled admin/harness cutover must replace that path.
use super::{Digest, StartIdentity, StateManagementConfig};
use crate::{ExchangeId, Lineage, Opener, Position, Reset, RunId, SessionId, SpuInstruction};
use serde::{Deserialize, Serialize};

super::codec::closed_object! {
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EnterPayload {
    pub session: SessionId,
    pub run: RunId,
    pub spu_instruction: SpuInstruction,
    /// The kind resolved, with what the kind requires riding inside it.
    pub binding: EnterBinding,
    /// The store the member stands on, resolved the same way: the embedded
    /// engine where the declaration is silent, per `weaver-types-Spec`
    /// section 4, with no database or role, which the inventory refuses for
    /// every engine. The harness names its engine on the load event.
    pub state_store: crate::config::StateStore,
    /// The declaration file's digest as admin read it at the inventory, per
    /// `weaver-types-Spec` section 4 as of 2026-09-04, so the run and the
    /// record both name what they were built from and the harness holds no
    /// file.
    pub declaration: String,
    /// The lineage of the save point the load restores, resolved by admin
    /// from the save point's stamp and never its path, per `weaver-types-Spec`
    /// section 4 on the operator's rulings of 2026-10-02 on #58: the harness
    /// names it on the load event, compares it with what the member answers
    /// to the `restored` ask, and numbers the run's turns from one whatever
    /// it carries. Absent where the load stands from nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restore: Option<Lineage>,
    /// The reset the load records, present where the agent's last run did
    /// not end in a clean unload, whether or not a save point stands, per the
    /// same section: admin resolves it from its clean-unload marker and the
    /// harness copies it onto the load event. Absent otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reset: Option<Reset>,
    /// The digests of the organ binaries admin started, keyed by the
    /// binary's name, so the load event names the stack that ran it, per the
    /// same section. Admin's fact, authored by the harness as it authors the
    /// store's.
    pub stack: std::collections::BTreeMap<String, String>,
    /// The sha256 hex of the agent's `roles.toml` as admin read it at the
    /// inventory, per `weaver-types-Spec` section 4, which the harness copies
    /// onto the load event as boundary and never constitution.
    pub boundary: String,
    /// Who asked for this load, section 3.1's `Cause`, recorded on the load
    /// event.
    pub cause: crate::Cause,
    /// The operator's uid, the value the agent root's `operator` key names,
    /// per `weaver-types-Spec` section 4 on the operator's ruling of
    /// 2026-10-06: what the harness admits the seeding line from, and the
    /// only uid it does.
    pub operator: u32,
    /// The engine libraries' directory where the agent's root names one,
    /// judged by admin and recorded on the load event beside the stack.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub library_path: Option<String>,
    /// **The graceful unload's drain bound, in seconds**, as admin parsed the
    /// declaration's `[lifecycle]` table, the harness's only source for timing
    /// S4; absent where the declaration is silent, meaning unbounded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drain_bound: Option<u64>,
    /// The wind-down's bound, in seconds, the same way, for S5.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wind_down_bound: Option<u64>,
}
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum EnterBinding {
    Serving {
        gate_instruction: crate::GateInstruction,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        shell: Option<crate::ShellGrant>,
        state_management: StateManagementConfig,
        start: StartIdentity,
        ste100_scheme: String,
    },
    Diagnostic {
        state_election: crate::StateElection,
    },
}

/// The show/load facts counterpart from TYPES-ENTER, with no legacy election alias.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum LoadedStateFacts {
    Serving {
        state_management: StateManagementConfig,
        start: StartIdentity,
        ste100_scheme: String,
        ste100_binary_sha256: Digest,
    },
    Diagnostic {
        state_election: crate::StateElection,
    },
}

super::codec::closed_object! {
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EnterEnvelope {
    pub exchange: ExchangeId,
    pub position: Position,
    pub payload: EnterContent,
}
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "body",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum EnterContent {
    Directive(EnterDirective),
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EnterDirective {
    Enter { payload: Box<EnterPayload> },
}

#[derive(Debug)]
pub enum CodecError {
    TooLarge,
    Malformed(serde_json::Error),
    WrongExchange,
}
impl EnterEnvelope {
    pub fn new(ordinal: u64, payload: EnterPayload) -> Self {
        Self {
            exchange: ExchangeId {
                opener: Opener::Admin,
                ordinal,
            },
            position: Position::Open,
            payload: EnterContent::Directive(EnterDirective::Enter {
                payload: Box::new(payload),
            }),
        }
    }
    pub fn enter(&self) -> &EnterPayload {
        let EnterContent::Directive(EnterDirective::Enter { payload }) = &self.payload;
        payload
    }
    fn check_exchange(&self) -> Result<(), CodecError> {
        if self.exchange.opener != Opener::Admin || self.position != Position::Open {
            return Err(CodecError::WrongExchange);
        }
        Ok(())
    }
    /// Check the entire escaped envelope before a descriptor can be sent.
    pub fn encode(&self) -> Result<Vec<u8>, CodecError> {
        self.check_exchange()?;
        let bytes = serde_json::to_vec(self).map_err(CodecError::Malformed)?;
        if bytes.len() > crate::MAX_ENVELOPE_BYTES {
            return Err(CodecError::TooLarge);
        }
        Ok(bytes)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, CodecError> {
        if bytes.len() > crate::MAX_ENVELOPE_BYTES {
            return Err(CodecError::TooLarge);
        }
        let text = std::str::from_utf8(bytes).map_err(|error| {
            CodecError::Malformed(serde_json::Error::io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                error,
            )))
        })?;
        let value: Self = super::decode_strict(text).map_err(CodecError::Malformed)?;
        value.check_exchange()?;
        Ok(value)
    }
}

#[cfg(test)]
#[path = "enter_tests.rs"]
mod tests;
