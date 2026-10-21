//! Tool-call communication records, from Spec 05 and TYPES-PAIRS.

use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

use super::{DecimalU64, EventSource, ReviewSetupRef, StartIdentity, StatementRef, WorkRef};

/// Stable source identity of one recovered request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CallId {
    pub run: String,
    pub sequence: DecimalU64,
    pub call: DecimalU64,
}

/// The three communication lanes. A review is never inferred from a tool reply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PairLane {
    External,
    Recall,
    Review,
}

/// Complete request information associated with its classified statement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum PairRequest {
    External {
        name: String,
        arguments: String,
    },
    Recall {
        subject: String,
    },
    Review {
        setup: ReviewSetupRef,
        work: Box<WorkRef>,
        command: String,
    },
}

impl PairRequest {
    /// The request tag determines the only matching lane.
    pub fn lane(&self) -> PairLane {
        match self {
            Self::External { .. } => PairLane::External,
            Self::Recall { .. } => PairLane::Recall,
            Self::Review { .. } => PairLane::Review,
        }
    }
}

/// Opening records account for conforming requests even if no tool is dispatched.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PairStarted {
    pub call_id: CallId,
    pub call_block: DecimalU64,
    pub statement: StatementRef,
    pub lane: PairLane,
    pub request: PairRequest,
    #[serde(deserialize_with = "super::required_option")]
    pub retry_of: Option<CallId>,
}

/// All retained pair states, including the open state in a read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PairFate {
    Opened,
    Completed,
    Failed,
}

/// A terminal record cannot claim that its communication remains open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalPairFate {
    Completed,
    Failed,
}

/// Reasons that a conforming send did not reach dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NonDispatchCause {
    PassPurpose,
    RepairExhausted,
    PlanBreak,
    RoundLimit,
    Stop,
    Unload,
    NoGate,
}

/// Closed metadata outcomes, never classifier input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum PairFailure {
    Timeout {},
    Cancelled {},
    Unload {},
    FlowFailure {},
    GateFault {},
    NotGranted {},
    NotDispatched { cause: NonDispatchCause },
    AbandonedAtLoad {},
}

/// The sole state terminal carrier. The existing ToolOutcome is kept whole.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PairEnded {
    pub call_id: CallId,
    #[serde(deserialize_with = "deserialize_execution")]
    pub execution: Option<crate::ExchangeId>,
    pub fate: TerminalPairFate,
    #[serde(deserialize_with = "super::required_option")]
    pub reason: Option<PairFailure>,
    #[serde(deserialize_with = "deserialize_outcome")]
    pub outcome: Option<crate::ToolOutcome>,
    #[serde(deserialize_with = "super::required_option")]
    pub loaded_from: Option<StartIdentity>,
}

/// A pair filing has exactly one of these payloads.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum PairRecord {
    Started(PairStarted),
    Ended(PairEnded),
}

/// Original canonical opening or terminal payload with its event attribution.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PairReadSource {
    pub source: EventSource,
    pub raw_block: Box<RawValue>,
}

/// A whole pair joined to its original request binding.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadPair {
    pub call_id: CallId,
    pub statement: StatementRef,
    pub lane: PairLane,
    pub request: PairRequest,
    pub fate: PairFate,
    #[serde(deserialize_with = "super::required_option")]
    pub reason: Option<PairFailure>,
    pub opening_source: PairReadSource,
    #[serde(deserialize_with = "super::required_option")]
    pub terminal_source: Option<PairReadSource>,
}

/// Read the existing shared types strictly at this new protocol boundary.
/// No alternative execution or tool-outcome enum is defined here.
struct RawFields(std::collections::BTreeMap<String, Box<RawValue>>);

impl<'de> Deserialize<'de> for RawFields {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct FieldsVisitor;
        impl<'de> serde::de::Visitor<'de> for FieldsVisitor {
            type Value = RawFields;
            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("an object with unique field names")
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(
                self,
                mut map: M,
            ) -> Result<Self::Value, M::Error> {
                let mut fields = std::collections::BTreeMap::new();
                while let Some((name, raw)) = map.next_entry::<String, Box<RawValue>>()? {
                    if fields.insert(name, raw).is_some() {
                        return Err(serde::de::Error::custom("duplicate field in shared value"));
                    }
                }
                Ok(RawFields(fields))
            }
        }
        deserializer.deserialize_map(FieldsVisitor)
    }
}

fn exact_keys(fields: &RawFields, keys: &[&str]) -> bool {
    fields.0.len() == keys.len() && keys.iter().all(|key| fields.0.contains_key(*key))
}

fn deserialize_execution<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<crate::ExchangeId>, D::Error> {
    let Some(raw) = Option::<Box<RawValue>>::deserialize(deserializer)? else {
        return Ok(None);
    };
    let fields: RawFields = serde_json::from_str(raw.get()).map_err(serde::de::Error::custom)?;
    if !exact_keys(&fields, &["opener", "ordinal"]) {
        return Err(serde::de::Error::custom(
            "execution requires exactly opener and ordinal",
        ));
    }
    serde_json::from_str(raw.get())
        .map(Some)
        .map_err(serde::de::Error::custom)
}

fn deserialize_outcome<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<crate::ToolOutcome>, D::Error> {
    let Some(raw) = Option::<Box<RawValue>>::deserialize(deserializer)? else {
        return Ok(None);
    };
    let fields: RawFields = serde_json::from_str(raw.get()).map_err(serde::de::Error::custom)?;
    let tag = fields
        .0
        .get("outcome")
        .ok_or_else(|| serde::de::Error::custom("missing outcome tag"))?;
    let tag: String = serde_json::from_str(tag.get()).map_err(serde::de::Error::custom)?;
    let keys: &[&str] = match tag.as_str() {
        "result" => &["outcome", "content"],
        "refused" => &["outcome", "reason"],
        "errored" => &["outcome", "detail"],
        "killed" => &["outcome", "partial", "by"],
        _ => return Err(serde::de::Error::custom("unknown tool outcome")),
    };
    if !exact_keys(&fields, keys) {
        return Err(serde::de::Error::custom(
            "tool outcome has missing or unknown fields",
        ));
    }
    serde_json::from_str(raw.get())
        .map(Some)
        .map_err(serde::de::Error::custom)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failure_metadata_keeps_empty_object_and_closed_cause_shapes() {
        for reason in [
            "timeout",
            "cancelled",
            "unload",
            "flow_failure",
            "gate_fault",
            "not_granted",
            "abandoned_at_load",
        ] {
            let json = format!(r#"{{"{reason}":{{}}}}"#);
            let value: PairFailure = serde_json::from_str(&json).unwrap();
            assert_eq!(serde_json::to_string(&value).unwrap(), json);
            assert!(
                serde_json::from_str::<PairFailure>(&format!(r#"{{"{reason}":{{"extra":true}}}}"#))
                    .is_err()
            );
        }
        assert!(
            serde_json::from_str::<PairFailure>(r#"{"not_dispatched":{"cause":"missing"}}"#)
                .is_err()
        );
        assert!(serde_json::from_str::<PairFailure>(r#""timeout""#).is_err());
    }

    #[test]
    fn terminal_record_requires_nulls_and_refuses_open_fate() {
        let json = r#"{"call_id":{"run":"r","sequence":"1","call":"0"},"execution":null,"fate":"failed","reason":{"not_dispatched":{"cause":"stop"}},"outcome":null,"loaded_from":null}"#;
        assert!(serde_json::from_str::<PairEnded>(json).is_ok());
        assert!(serde_json::from_str::<PairEnded>(&json.replace(",\"outcome\":null", "")).is_err());
        assert!(
            serde_json::from_str::<PairEnded>(
                &json.replace("\"fate\":\"failed\"", "\"fate\":\"opened\"")
            )
            .is_err()
        );
    }

    #[test]
    fn shared_execution_and_outcome_keep_the_new_boundary_strict() {
        let json = r#"{"call_id":{"run":"r","sequence":"1","call":"0"},"execution":{"opener":"harness","ordinal":1},"fate":"failed","reason":{"timeout":{}},"outcome":{"outcome":"killed","partial":null,"by":"clock"},"loaded_from":null}"#;
        assert!(serde_json::from_str::<PairEnded>(json).is_ok());
        for forbidden in [
            json.replace("\"ordinal\":1", "\"ordinal\":1,\"extra\":true"),
            json.replace(",\"partial\":null", ""),
            json.replace("\"by\":\"clock\"", "\"by\":\"clock\",\"extra\":null"),
            json.replace(
                "\"outcome\":\"killed\"",
                "\"outcome\":\"killed\",\"outcome\":\"killed\"",
            ),
        ] {
            assert!(
                serde_json::from_str::<PairEnded>(&forbidden).is_err(),
                "{forbidden}"
            );
        }
    }
}
