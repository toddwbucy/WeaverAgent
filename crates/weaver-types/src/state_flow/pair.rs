//! Tool-call communication records, from Spec 05 and TYPES-PAIRS.

use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

use super::{
    DecimalU64, EventSource, ReviewSetupRef, StartIdentity, StatementRef, ValidationError, WorkRef,
};

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

impl PairStarted {
    /// Check the lane before a directly constructed opening enters a filing.
    /// Referential and lifecycle checks remain the store's obligations.
    /// Decoding preserves a mismatched lane for the member's InvalidPair refusal.
    pub fn validate_structure(&self) -> Result<(), ValidationError> {
        validate_lane(self.lane, &self.request)
    }
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
#[derive(Debug, Clone, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReadPair {
    pub call_id: CallId,
    pub statement: StatementRef,
    pub lane: PairLane,
    pub request: PairRequest,
    pub fate: PairFate,
    pub reason: Option<PairFailure>,
    pub opening_source: PairReadSource,
    pub terminal_source: Option<PairReadSource>,
}

impl ReadPair {
    /// Check the request's declared lane before accepting a pair attachment.
    /// The store still owns source reconstruction and terminal-state checks.
    pub fn validate_structure(&self) -> Result<(), ValidationError> {
        validate_lane(self.lane, &self.request)
    }
}

impl<'de> Deserialize<'de> for ReadPair {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            call_id: CallId,
            statement: StatementRef,
            lane: PairLane,
            request: PairRequest,
            fate: PairFate,
            #[serde(deserialize_with = "super::required_option")]
            reason: Option<PairFailure>,
            opening_source: PairReadSource,
            #[serde(deserialize_with = "super::required_option")]
            terminal_source: Option<PairReadSource>,
        }
        let fields = Fields::deserialize(deserializer)?;
        let record = Self {
            call_id: fields.call_id,
            statement: fields.statement,
            lane: fields.lane,
            request: fields.request,
            fate: fields.fate,
            reason: fields.reason,
            opening_source: fields.opening_source,
            terminal_source: fields.terminal_source,
        };
        record
            .validate_structure()
            .map_err(serde::de::Error::custom)?;
        Ok(record)
    }
}

fn validate_lane(lane: PairLane, request: &PairRequest) -> Result<(), ValidationError> {
    if lane != request.lane() {
        return Err(ValidationError("pair lane differs from its request kind"));
    }
    Ok(())
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

    fn requests() -> [PairRequest; 3] {
        let digest = super::super::Digest::new("a".repeat(64)).unwrap();
        [
            PairRequest::External {
                name: "sh".into(),
                arguments: "ls".into(),
            },
            PairRequest::Recall {
                subject: "record".into(),
            },
            PairRequest::Review {
                setup: ReviewSetupRef {
                    setup_id: "checks".into(),
                    sha256: digest.clone(),
                },
                work: Box::new(WorkRef {
                    plan: super::super::PlanRef {
                        plan_id: super::super::PlanId {
                            seed_id: "seed".into(),
                            local_id: 1.into(),
                        },
                        revision: 1.into(),
                    },
                    step_id: super::super::StepId {
                        seed_id: "seed".into(),
                        local_id: 1.into(),
                    },
                    span: super::super::SpanRef {
                        seed_id: "seed".into(),
                        local_id: 2.into(),
                        revision: 1.into(),
                    },
                    through: super::super::SourcePosition {
                        block_id: super::super::SourceBlockId {
                            run: "r".into(),
                            sequence: 5.into(),
                            block: 0.into(),
                        },
                        turn: Some("t".into()),
                    },
                    content_sha256: digest,
                }),
                command: "check".into(),
            },
        ]
    }

    fn started(request: PairRequest) -> PairStarted {
        PairStarted {
            call_id: CallId {
                run: "r".into(),
                sequence: 1.into(),
                call: 0.into(),
            },
            call_block: 0.into(),
            statement: StatementRef {
                block_id: super::super::SourceBlockId {
                    run: "r".into(),
                    sequence: 1.into(),
                    block: 0.into(),
                },
                binding: 0.into(),
            },
            lane: request.lane(),
            request,
            retry_of: None,
        }
    }

    fn read_pair(opening: &PairStarted) -> ReadPair {
        ReadPair {
            call_id: opening.call_id.clone(),
            statement: opening.statement.clone(),
            lane: opening.lane,
            request: opening.request.clone(),
            fate: PairFate::Opened,
            reason: None,
            opening_source: PairReadSource {
                source: EventSource {
                    block_id: super::super::SourceBlockId {
                        run: "r".into(),
                        sequence: 2.into(),
                        block: 0.into(),
                    },
                    envelope: super::super::SourceEnvelope {
                        session: "s".into(),
                        run: "r".into(),
                        turn: Some("t".into()),
                        sequence: 2.into(),
                        kind: "tool.call.started".into(),
                        channel: "tool".into(),
                        origin: "loop".into(),
                        subsystem: "harness".into(),
                        causal_parent: None,
                        wall_ms: 0,
                        monotonic_ns: 0.into(),
                    },
                    source_path: vec![],
                    partition_rule_id: "pairs".into(),
                },
                raw_block: serde_json::value::to_raw_value(opening).unwrap(),
            },
            terminal_source: None,
        }
    }

    #[test]
    fn opening_lane_is_validated_at_filing_without_losing_the_typed_refusal() {
        for request in requests() {
            let opening = started(request);
            for lane in [PairLane::External, PairLane::Recall, PairLane::Review] {
                let record = PairStarted {
                    lane,
                    ..opening.clone()
                };
                let valid = lane == record.request.lane();
                assert_eq!(record.validate_structure().is_ok(), valid);
                let raw = serde_json::to_string(&record).unwrap();
                assert!(serde_json::from_str::<PairStarted>(&raw).is_ok());
                assert!(
                    serde_json::from_str::<PairRecord>(&format!(r#"{{"started":{raw}}}"#)).is_ok()
                );
                let source = read_pair(&record).opening_source.source;
                let filing = super::super::PairFile {
                    block_id: source.block_id,
                    source: source.envelope,
                    partition_rule_id: source.partition_rule_id,
                    source_path: source.source_path,
                    admission: super::super::Admission::Admitted,
                    raw_block: RawValue::from_string(raw).unwrap(),
                    scheme_id: "scheme".into(),
                    pair: PairRecord::Started(record),
                };
                assert_eq!(filing.validate_structure().is_ok(), valid);
            }
        }
    }

    #[test]
    fn read_lane_must_match_through_entries_and_tagged_query_results() {
        use super::super::{QueryResult, ReadEntry, ReadSource, StepQueryRef};
        for request in requests() {
            let opening = started(request);
            for lane in [PairLane::External, PairLane::Recall, PairLane::Review] {
                let pair = ReadPair {
                    lane,
                    ..read_pair(&opening)
                };
                let valid = lane == pair.request.lane();
                assert_eq!(pair.validate_structure().is_ok(), valid);
                assert_eq!(
                    serde_json::from_str::<ReadPair>(&serde_json::to_string(&pair).unwrap())
                        .is_ok(),
                    valid
                );
                let entry = ReadEntry {
                    source: ReadSource::Event(Box::new(pair.opening_source.source.clone())),
                    raw_block: RawValue::from_string(
                        r#"{"type":"text","text":"Read the record."}"#.into(),
                    )
                    .unwrap(),
                    text: "Read the record.".into(),
                    bindings: vec![],
                    selected_span: None,
                    step_query: None,
                    pairs: vec![pair.clone()],
                };
                assert_eq!(
                    serde_json::from_str::<ReadEntry>(&serde_json::to_string(&entry).unwrap())
                        .is_ok(),
                    valid
                );
                let PairRequest::Review { work, .. } = &requests()[2] else {
                    unreachable!()
                };
                for result in [
                    QueryResult::Entries {
                        entries: vec![entry.clone()],
                    },
                    QueryResult::OpenPairs { pairs: vec![pair] },
                    QueryResult::SpanPreview {
                        span: work.span.clone(),
                        through: work.through.clone(),
                        entries: vec![entry.clone()],
                    },
                    QueryResult::StepSuccessor {
                        reference: StepQueryRef {
                            plan: work.plan.clone(),
                            previous_step: None,
                            step_id: work.step_id.clone(),
                            instruction: opening.statement.clone(),
                        },
                        instruction: Box::new(entry),
                    },
                ] {
                    assert_eq!(
                        serde_json::from_str::<QueryResult>(
                            &serde_json::to_string(&result).unwrap()
                        )
                        .is_ok(),
                        valid
                    );
                }
            }
        }
    }

    #[test]
    fn validated_pair_decoders_keep_required_nulls_and_strict_fields() {
        let opening = started(requests()[0].clone());
        let open_json = serde_json::to_string(&opening).unwrap();
        for forbidden in [
            open_json.replace(",\"retry_of\":null", ""),
            open_json.replacen("\"lane\":", "\"extra\":null,\"lane\":", 1),
            open_json.replacen("\"lane\":", "\"lane\":\"external\",\"lane\":", 1),
        ] {
            assert!(serde_json::from_str::<PairStarted>(&forbidden).is_err());
        }
        let mut pair = read_pair(&opening);
        let raw = open_json.replacen('{', "{ ", 1);
        pair.opening_source.raw_block = RawValue::from_string(raw.clone()).unwrap();
        let read_json = serde_json::to_string(&pair).unwrap();
        let decoded: ReadPair = serde_json::from_str(&read_json).unwrap();
        assert_eq!(decoded.opening_source.raw_block.get(), raw);
        for forbidden in [
            read_json.replacen(",\"reason\":null", "", 1),
            read_json.replacen(",\"terminal_source\":null", "", 1),
            read_json.replacen("\"lane\":", "\"extra\":null,\"lane\":", 1),
            read_json.replacen("\"lane\":", "\"lane\":\"external\",\"lane\":", 1),
        ] {
            assert!(serde_json::from_str::<ReadPair>(&forbidden).is_err());
        }
    }

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
