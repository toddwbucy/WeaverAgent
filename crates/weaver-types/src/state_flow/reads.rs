//! Store-owned read and control vocabulary, from TYPES-READ and STATE-PAGES.

use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

use super::{
    Binding, CaptureRefusalCode, DecimalU64, Digest, MemberObservationIdentity, PlanFlagSnapshot,
    PlanRef, PlanSnapshot, ReadPair, SourceBlockId, SourceEnvelope, SourcePathPart, SourcePosition,
    SpanInfo, SpanRef, StatementRef, StepId, StepQueryRef, StoredOperationObservation,
    TaskScopeRef, ValidationError,
};

/// Pre-agent content attribution, with no invented operating event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeedSource {
    pub seed_id: String,
    pub ordinal: DecimalU64,
}

/// Original event attribution of one admitted content block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventSource {
    pub block_id: SourceBlockId,
    pub envelope: SourceEnvelope,
    pub source_path: Vec<SourcePathPart>,
    pub partition_rule_id: String,
}

/// Exactly one seed or event source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ReadSource {
    Seed(SeedSource),
    Event(Box<EventSource>),
}

/// The selected interval of the original block, not rewritten content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectedSpan {
    pub span_ref: SpanRef,
    pub start: DecimalU64,
    pub end: DecimalU64,
}

/// A binding retains its original ordinal even after filtering.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadBinding {
    pub ordinal: DecimalU64,
    pub value: Binding,
}

/// One retained statement block and all requested whole-pair attachments.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadEntry {
    pub source: ReadSource,
    pub raw_block: Box<RawValue>,
    pub text: String,
    pub bindings: Vec<ReadBinding>,
    #[serde(deserialize_with = "super::required_option")]
    pub selected_span: Option<SelectedSpan>,
    #[serde(deserialize_with = "super::required_option")]
    pub step_query: Option<StepQueryRef>,
    pub pairs: Vec<ReadPair>,
}

/// Closed structured queries. No selector carries SQL or a mutation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ReadSelector {
    History {
        noun: String,
        #[serde(deserialize_with = "super::required_option")]
        verb: Option<String>,
        #[serde(deserialize_with = "super::required_option")]
        condition: Option<String>,
    },
    Subjects {
        nouns: Vec<String>,
    },
    Statement {
        statement: StatementRef,
    },
    Condition {
        condition: String,
    },
    Span {
        seed_id: String,
        local_id: DecimalU64,
        revision: DecimalU64,
    },
    SystemPrompt {},
    Conversation {
        last_turns: DecimalU64,
    },
    OpenPairs {},
    Marks {},
    SpanPreview {
        span: SpanRef,
        through: SourcePosition,
    },
    PlanCurrent {},
    Plan {
        plan: PlanRef,
    },
    StepSuccessor {
        plan: PlanRef,
        #[serde(deserialize_with = "super::required_option")]
        previous_step: Option<StepId>,
    },
    PlanFlags {
        plan: PlanRef,
        after: SourceBlockId,
    },
    Observations {
        run: String,
        after: DecimalU64,
        #[serde(deserialize_with = "super::required_option")]
        through: Option<DecimalU64>,
    },
}

/// Complete tagged result, exposed only after all pages have been checked.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum QueryResult {
    Entries {
        entries: Vec<ReadEntry>,
    },
    OpenPairs {
        pairs: Vec<ReadPair>,
    },
    Marks {
        next_span_id: DecimalU64,
        #[serde(deserialize_with = "super::required_option")]
        current_task: Option<TaskScopeRef>,
        open_scopes: Vec<SpanInfo>,
    },
    SpanPreview {
        span: SpanRef,
        through: SourcePosition,
        entries: Vec<ReadEntry>,
    },
    PlanCurrent {
        #[serde(deserialize_with = "super::required_option")]
        current: Option<PlanSnapshot>,
        next_plan_id: DecimalU64,
        next_step_id: DecimalU64,
    },
    Plan {
        snapshot: PlanSnapshot,
    },
    StepSuccessor {
        reference: StepQueryRef,
        instruction: Box<ReadEntry>,
    },
    PlanFlags {
        snapshot: PlanFlagSnapshot,
    },
    Observations {
        run: String,
        after: DecimalU64,
        through: DecimalU64,
        member_observation: MemberObservationIdentity,
        records: Vec<StoredOperationObservation>,
    },
}

/// Base64 bytes from one immutable complete result. A page is not model content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadPage {
    pub snapshot_id: DecimalU64,
    pub offset: DecimalU64,
    pub total_bytes: DecimalU64,
    pub octets: String,
    pub done: bool,
}

impl ReadPage {
    /// Check the declared result bound before allocating and decode one bounded page.
    ///
    /// A page may split UTF-8. Its bytes cannot become query data until the complete
    /// result has been reassembled and validated. The caller also checks snapshot
    /// identity, the exact expected offset, and the unchanged total across pages.
    pub fn decode_octets(&self, max_result_bytes: DecimalU64) -> Result<Vec<u8>, ValidationError> {
        const PAGE_BYTES: usize = 524_288;
        const PAGE_BASE64: usize = PAGE_BYTES.div_ceil(3) * 4;
        let encoded = self.octets.as_bytes();
        if self.total_bytes.0 == 0
            || self.total_bytes > max_result_bytes
            || self.offset >= self.total_bytes
            || encoded.is_empty()
            || encoded.len() > PAGE_BASE64
            || !encoded.len().is_multiple_of(4)
        {
            return Err(ValidationError("invalid page size, result bound or offset"));
        }
        let padding = usize::from(encoded.ends_with(b"=")) + usize::from(encoded.ends_with(b"=="));
        let decoded_len = encoded.len() / 4 * 3 - padding;
        let end = self.offset.0.checked_add(decoded_len as u64);
        if decoded_len == 0
            || decoded_len > PAGE_BYTES
            || end.is_none_or(|end| {
                end > self.total_bytes.0 || self.done != (end == self.total_bytes.0)
            })
        {
            return Err(ValidationError(
                "page end or completion flag is inconsistent",
            ));
        }
        let digit = |byte: u8| match byte {
            b'A'..=b'Z' => Ok(byte - b'A'),
            b'a'..=b'z' => Ok(byte - b'a' + 26),
            b'0'..=b'9' => Ok(byte - b'0' + 52),
            b'+' => Ok(62),
            b'/' => Ok(63),
            _ => Err(ValidationError("invalid base64 byte")),
        };
        let mut decoded = Vec::with_capacity(decoded_len);
        for (index, group) in encoded.chunks_exact(4).enumerate() {
            let last = index + 1 == encoded.len() / 4;
            let a = digit(group[0])?;
            let b = digit(group[1])?;
            decoded.push(a << 2 | b >> 4);
            if group[2] == b'=' {
                if !last || group[3] != b'=' || b & 15 != 0 {
                    return Err(ValidationError("noncanonical base64 padding"));
                }
                continue;
            }
            let c = digit(group[2])?;
            decoded.push(b << 4 | c >> 2);
            if group[3] == b'=' {
                if !last || c & 3 != 0 {
                    return Err(ValidationError("noncanonical base64 padding"));
                }
                continue;
            }
            decoded.push(c << 6 | digit(group[3])?);
        }
        Ok(decoded)
    }
}

/// The closed read refusal set, including the common complete-answer bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadRefusalCode {
    UnsupportedOperation,
    InvalidSelector,
    WrongSeed,
    NotFound,
    UnknownSpan,
    SpanOpen,
    SpanPending,
    PartialPair,
    CounterExhausted,
    ReadBusy,
    UnknownSnapshot,
    WrongOffset,
    ResultTooLarge,
    ResultUnavailable,
    AnswerTooLarge,
}

/// Every control refusal belongs to exactly one of these disjoint code sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ControlRefusalCode {
    Read(ReadRefusalCode),
    Capture(CaptureRefusalCode),
}

/// One operation on the sole state exchange. Capture continuations keep its ID.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum StateControl {
    Read {
        selector: ReadSelector,
    },
    Next {
        snapshot_id: DecimalU64,
        offset: DecimalU64,
    },
    Release {
        snapshot_id: DecimalU64,
    },
    Shape {},
    Held {},
    Position {},
    Grants {},
    Snapshot {
        capture_id: DecimalU64,
    },
    Acknowledge {
        capture_id: DecimalU64,
        dump_sha256: Digest,
    },
    Abort {
        capture_id: DecimalU64,
    },
    ReleaseCapture {
        capture_id: DecimalU64,
    },
}

/// Content counts grouped by source run and kind in first-commit order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShapeRow {
    pub run: String,
    pub kind: String,
    pub count: DecimalU64,
}

/// The corresponding typed control answer, wrapped with correlation and observation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum StateControlAnswer {
    ReadPage(ReadPage),
    Released {
        snapshot_id: DecimalU64,
    },
    Shape(Vec<ShapeRow>),
    Held {
        count: DecimalU64,
    },
    Position(Option<SourcePosition>),
    Grants {
        surface: Vec<String>,
    },
    Snapshot {
        capture_id: DecimalU64,
        room_name: String,
        size_bytes: DecimalU64,
        #[serde(deserialize_with = "super::required_option")]
        position: Option<SourcePosition>,
    },
    Finished {
        capture_id: DecimalU64,
        room_name: String,
    },
    Aborted {
        capture_id: DecimalU64,
    },
    CaptureReleased {
        capture_id: DecimalU64,
    },
    ControlRefused {
        code: ControlRefusalCode,
        detail: String,
    },
}

impl ReadSelector {
    /// Check context-free selector constraints before any store lookup.
    ///
    /// Invalid selector values remain decodable so the store can return its
    /// correlated operation refusal. Identity and holding checks are store duties.
    pub fn validate_shape(&self) -> Result<(), ReadRefusalCode> {
        match self {
            Self::Subjects { nouns }
                if nouns.is_empty()
                    || nouns.iter().any(String::is_empty)
                    || nouns
                        .windows(2)
                        .any(|pair| pair[0].as_bytes() >= pair[1].as_bytes()) =>
            {
                Err(ReadRefusalCode::InvalidSelector)
            }
            Self::Conversation { last_turns } if last_turns.0 == 0 => {
                Err(ReadRefusalCode::InvalidSelector)
            }
            Self::Observations {
                after,
                through: Some(through),
                ..
            } if after.0 > through.0 => Err(ReadRefusalCode::InvalidSelector),
            _ => Ok(()),
        }
    }
}

impl QueryResult {
    /// Tag association is checked before query data can be exposed to a caller.
    ///
    /// This check grants no content or placement authority. Selector-specific
    /// identity echoes and entry invariants must also be checked by the consumer.
    pub fn matches_selector(&self, selector: &ReadSelector) -> bool {
        matches!(
            (self, selector),
            (
                Self::Entries { .. },
                ReadSelector::History { .. }
                    | ReadSelector::Subjects { .. }
                    | ReadSelector::Statement { .. }
                    | ReadSelector::Condition { .. }
                    | ReadSelector::Span { .. }
                    | ReadSelector::SystemPrompt {}
                    | ReadSelector::Conversation { .. }
            ) | (Self::OpenPairs { .. }, ReadSelector::OpenPairs {})
                | (Self::Marks { .. }, ReadSelector::Marks {})
                | (Self::SpanPreview { .. }, ReadSelector::SpanPreview { .. })
                | (Self::PlanCurrent { .. }, ReadSelector::PlanCurrent {})
                | (Self::Plan { .. }, ReadSelector::Plan { .. })
                | (
                    Self::StepSuccessor { .. },
                    ReadSelector::StepSuccessor { .. }
                )
                | (Self::PlanFlags { .. }, ReadSelector::PlanFlags { .. })
                | (Self::Observations { .. }, ReadSelector::Observations { .. })
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_pages_validate_progress_completion_and_canonical_base64() {
        let page = ReadPage {
            snapshot_id: 1.into(),
            offset: 0.into(),
            total_bytes: 3.into(),
            octets: "YWJj".into(),
            done: true,
        };
        assert_eq!(page.decode_octets(3.into()).unwrap(), b"abc");
        assert!(page.decode_octets(2.into()).is_err());
        let mut changed = page.clone();
        changed.done = false;
        assert!(changed.decode_octets(3.into()).is_err());
        changed = page.clone();
        changed.total_bytes = 4.into();
        assert!(changed.decode_octets(4.into()).is_err());
        changed.done = false;
        assert_eq!(changed.decode_octets(4.into()).unwrap(), b"abc");
        changed = page.clone();
        changed.offset = (u64::MAX - 1).into();
        changed.total_bytes = u64::MAX.into();
        assert!(changed.decode_octets(u64::MAX.into()).is_err());
        for (octets, count) in [
            ("", 1_u64),
            ("YQ", 1),
            ("YQ=", 1),
            ("YR==", 1),
            ("YWJ=", 2),
            ("YQ==YQ==", 4),
            ("YW J", 3),
            ("====", 1),
        ] {
            changed = page.clone();
            changed.octets = octets.into();
            changed.total_bytes = count.into();
            assert!(changed.decode_octets(10.into()).is_err(), "{octets}");
        }
        // The first byte of a multibyte character is a valid page, not UTF-8 yet.
        changed = page;
        changed.octets = "ww==".into();
        changed.total_bytes = 2.into();
        changed.done = false;
        assert_eq!(changed.decode_octets(2.into()).unwrap(), [0xc3]);
    }

    #[test]
    fn read_pages_accept_the_fixed_page_limit_and_refuse_one_byte_more() {
        let mut page = ReadPage {
            snapshot_id: 1.into(),
            offset: 0.into(),
            total_bytes: 524_288.into(),
            octets: format!("{}AAA=", "AAAA".repeat(174_762)),
            done: true,
        };
        assert_eq!(page.decode_octets(524_288.into()).unwrap().len(), 524_288);
        page.octets = "AAAA".repeat(174_763);
        page.total_bytes = 524_289.into();
        assert!(page.decode_octets(524_289.into()).is_err());
    }

    #[test]
    fn read_selectors_are_closed_and_explicit_nulls_are_required() {
        assert!(
            serde_json::from_str::<ReadSelector>(
                r#"{"history":{"noun":"record","verb":null,"condition":null}}"#
            )
            .is_ok()
        );
        for json in [
            r#"{"history":{"noun":"record","verb":null}}"#,
            r#"{"system_prompt":{"sql":"SELECT 1"}}"#,
            r#"{"conversation":{"last_turns":2}}"#,
            r#"{"step_successor":{"plan":null,"previous_step":null}}"#,
            r#"{"observations":{"run":"r","after":"0"}}"#,
            r#"{"subjects":{"nouns":[],"order":"recent"}}"#,
        ] {
            assert!(
                serde_json::from_str::<ReadSelector>(json).is_err(),
                "{json}"
            );
        }
    }

    #[test]
    fn invalid_selector_values_can_produce_typed_refusals() {
        for nouns in [
            vec![],
            vec!["".to_owned()],
            vec!["a".into(), "a".into()],
            vec!["b".into(), "a".into()],
        ] {
            assert_eq!(
                ReadSelector::Subjects { nouns }.validate_shape(),
                Err(ReadRefusalCode::InvalidSelector)
            );
        }
        assert!(
            ReadSelector::Subjects {
                nouns: vec!["A".into(), "a".into()]
            }
            .validate_shape()
            .is_ok()
        );
        assert_eq!(
            ReadSelector::Conversation {
                last_turns: DecimalU64(0)
            }
            .validate_shape(),
            Err(ReadRefusalCode::InvalidSelector)
        );
    }

    #[test]
    fn metadata_result_cannot_be_consumed_as_content_entries() {
        let entries: QueryResult = serde_json::from_str(r#"{"entries":{"entries":[]}}"#).unwrap();
        let pairs: QueryResult = serde_json::from_str(r#"{"open_pairs":{"pairs":[]}}"#).unwrap();
        assert!(entries.matches_selector(&ReadSelector::SystemPrompt {}));
        assert!(!pairs.matches_selector(&ReadSelector::SystemPrompt {}));
        assert!(!entries.matches_selector(&ReadSelector::OpenPairs {}));
        assert!(!entries.matches_selector(&ReadSelector::Observations {
            run: "r".into(),
            after: DecimalU64(0),
            through: None,
        }));
    }

    #[test]
    fn tagged_read_keeps_the_original_raw_block_spelling() {
        let raw = r#"{"type":"text","text":"Read\u0020the record."}"#;
        let json = format!(
            r#"{{"entries":{{"entries":[{{"source":{{"seed":{{"seed_id":"seed","ordinal":"0"}}}},"raw_block":{raw},"text":"Read the record.","bindings":[],"selected_span":null,"step_query":null,"pairs":[]}}]}}}}"#
        );
        let result: QueryResult = serde_json::from_str(&json).unwrap();
        let QueryResult::Entries { entries } = &result else {
            panic!("entries tag");
        };
        assert_eq!(entries[0].raw_block.get(), raw);
        assert!(serde_json::to_string(&result).unwrap().contains(raw));
        assert!(
            serde_json::from_str::<QueryResult>(&json.replace(",\"selected_span\":null", ""))
                .is_err()
        );
    }

    #[test]
    fn control_frames_keep_capture_identity_and_refuse_unknown_commands() {
        let abort: StateControl = serde_json::from_str(r#"{"abort":{"capture_id":"7"}}"#).unwrap();
        assert_eq!(
            serde_json::to_string(&abort).unwrap(),
            r#"{"abort":{"capture_id":"7"}}"#
        );
        for json in [
            r#"{"abort":{"capture_id":"7","request_id":"8"}}"#,
            r#"{"restore":{}}"#,
            r#"{"read":{"selector":{"marks":{}},"sql":"SELECT 1"}}"#,
        ] {
            assert!(
                serde_json::from_str::<StateControl>(json).is_err(),
                "{json}"
            );
        }
        assert!(serde_json::from_str::<StateControlAnswer>(r#"{"position":null}"#).is_ok());
        assert!(
            serde_json::from_str::<StateControlAnswer>(
                r#"{"control_refused":{"code":"invented","detail":"no"}}"#
            )
            .is_err()
        );
    }
}
