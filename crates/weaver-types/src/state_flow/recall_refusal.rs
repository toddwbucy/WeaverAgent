//! Trace-only operation refusal for a checked recall binding, TYPES-RECALL-REFUSAL.

use serde::{Deserialize, Deserializer, Serialize};

use super::codec::deserialize_object as named_object;
use super::{CallId, DecimalU64, SourceBlockId, StatementRef, ValidationError};

/// The original request and its checked statement remain separate identities.
/// This evidence grants no pair, query, subject selection or model-facing result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RecallRequestRefusal {
    pub call_id: CallId,
    pub call_block: DecimalU64,
    pub statement: StatementRef,
    pub noun_count: DecimalU64,
    pub has_condition: bool,
    pub detail: String,
}

impl RecallRequestRefusal {
    pub const DETAIL: &'static str = "recall requires exactly one noun and no governing condition";

    /// Validate this carrier without inferring association or classifier meaning.
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.noun_count.0 == 0 || (self.noun_count.0 == 1 && !self.has_condition) {
            return Err(ValidationError(
                "recall refusal requires an invalid operation shape",
            ));
        }
        if self.detail != Self::DETAIL {
            return Err(ValidationError(
                "recall refusal detail differs from RECALL-REFUSAL",
            ));
        }
        if self.call_id.run.is_empty()
            || self.call_id.sequence.0 == 0
            || self.statement.block_id.run.is_empty()
            || self.statement.block_id.sequence.0 == 0
        {
            return Err(ValidationError("recall refusal requires source identities"));
        }
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RecallRequestRefusalWire {
    #[serde(deserialize_with = "named_object")]
    call_id: CallId,
    call_block: DecimalU64,
    #[serde(deserialize_with = "statement_object")]
    statement: StatementRef,
    noun_count: DecimalU64,
    has_condition: bool,
    detail: String,
}

impl<'de> Deserialize<'de> for RecallRequestRefusal {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = named_object::<D, RecallRequestRefusalWire>(deserializer)?;
        Self::try_from(wire).map_err(serde::de::Error::custom)
    }
}

fn statement_object<'de, D: Deserializer<'de>>(deserializer: D) -> Result<StatementRef, D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Statement {
        block_id: SourceBlockId,
        binding: DecimalU64,
    }
    let statement = named_object::<D, Statement>(deserializer)?;
    Ok(StatementRef {
        block_id: statement.block_id,
        binding: statement.binding,
    })
}

impl TryFrom<RecallRequestRefusalWire> for RecallRequestRefusal {
    type Error = ValidationError;

    fn try_from(wire: RecallRequestRefusalWire) -> Result<Self, Self::Error> {
        let value = Self {
            call_id: wire.call_id,
            call_block: wire.call_block,
            statement: wire.statement,
            noun_count: wire.noun_count,
            has_condition: wire.has_condition,
            detail: wire.detail,
        };
        value.validate()?;
        Ok(value)
    }
}
