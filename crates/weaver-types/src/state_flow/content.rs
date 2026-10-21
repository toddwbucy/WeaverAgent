use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

use super::{DecimalU64, MarkEffect, PairRecord, PlanEffect, ValidationError};

/// An event block's identity before any partition decision.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceBlockId {
    pub run: String,
    pub sequence: DecimalU64,
    pub block: DecimalU64,
}

/// A held source frontier, including its exact nullable turn attribution.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourcePosition {
    pub block_id: SourceBlockId,
    #[serde(deserialize_with = "super::required_option")]
    pub turn: Option<String>,
}

/// The complete source envelope. Vocabulary judgment belongs to the trace.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceEnvelope {
    pub session: String,
    pub run: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_option"
    )]
    pub turn: Option<String>,
    pub sequence: DecimalU64,
    pub kind: String,
    pub channel: String,
    pub origin: String,
    pub subsystem: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_option"
    )]
    pub causal_parent: Option<DecimalU64>,
    pub wall_ms: u64,
    pub monotonic_ns: DecimalU64,
}

fn present_option<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

/// A real JSON source path, not the partition's structural selector spelling.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub enum SourcePathPart {
    Key(String),
    Index { index: DecimalU64 },
}

/// A half-open UTF-8 byte interval in decoded source text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Span {
    pub start: DecimalU64,
    pub end: DecimalU64,
}

impl Span {
    pub fn slice<'a>(&self, text: &'a str) -> Result<&'a str, ValidationError> {
        let start = usize::try_from(self.start.0)
            .map_err(|_| ValidationError("span start exceeds the address space"))?;
        let end = usize::try_from(self.end.0)
            .map_err(|_| ValidationError("span end exceeds the address space"))?;
        if start >= end {
            return Err(ValidationError("span must be nonempty and forward"));
        }
        text.get(start..end)
            .ok_or(ValidationError("span is outside UTF-8 source boundaries"))
    }

    pub fn contains(&self, other: &Self) -> bool {
        self.start <= other.start && other.end <= self.end
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NounBinding {
    pub value: String,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub statement_span: Span,
    pub verb: String,
    pub verb_span: Span,
    #[serde(deserialize_with = "super::required_option")]
    pub condition: Option<String>,
    #[serde(deserialize_with = "super::required_option")]
    pub condition_span: Option<Span>,
    pub nouns: Vec<NounBinding>,
}

impl Binding {
    /// Check references and dictionary membership, never classify the statement.
    pub fn validate(&self, text: &str, verbs: &[String]) -> Result<(), ValidationError> {
        self.statement_span.slice(text)?;
        self.verb_span.slice(text)?;
        if self.verb.is_empty() || !verbs.contains(&self.verb) {
            return Err(ValidationError("verb is not in the closed dictionary"));
        }
        if !self.statement_span.contains(&self.verb_span) {
            return Err(ValidationError("verb lies outside its statement"));
        }
        if self.nouns.is_empty() {
            return Err(ValidationError("binding requires at least one noun"));
        }
        for noun in &self.nouns {
            if !self.statement_span.contains(&noun.span) || noun.span.slice(text)? != noun.value {
                return Err(ValidationError(
                    "noun does not name its exact source substring",
                ));
            }
        }
        match (&self.condition, &self.condition_span) {
            (None, None) => {}
            (Some(value), Some(span)) => {
                let source = span.slice(text)?;
                if !self.statement_span.contains(span) {
                    return Err(ValidationError("condition lies outside its statement"));
                }
                // A scheme-recognized forward flag has a normalized step address.
                // The checker owns recognition. This port checks its canonical ID.
                let step = value
                    .strip_prefix("step:")
                    .and_then(|v| v.parse::<DecimalU64>().ok());
                if value != source && step.is_none_or(|id| id.0 == 0) {
                    return Err(ValidationError(
                        "condition is neither its source nor a step address",
                    ));
                }
            }
            _ => {
                return Err(ValidationError(
                    "condition and condition_span must be null together",
                ));
            }
        }
        Ok(())
    }
}

/// Validate the ordered bindings of an accepted non-template block.
pub fn validate_bindings(
    bindings: &[Binding],
    text: &str,
    verbs: &[String],
) -> Result<(), ValidationError> {
    if text.is_empty() || bindings.is_empty() {
        return Err(ValidationError(
            "accepted text and bindings must be nonempty",
        ));
    }
    let mut previous = None;
    for binding in bindings {
        binding.validate(text, verbs)?;
        let start = binding.statement_span.start;
        if previous.is_some_and(|prior| start < prior) {
            return Err(ValidationError(
                "bindings are not in source statement order",
            ));
        }
        previous = Some(start);
    }
    Ok(())
}

/// Copied context stays byte-identical until its licensed effect is checked.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceContext {
    pub source_path: Vec<SourcePathPart>,
    pub raw: Box<RawValue>,
}

impl PartialEq for SourceContext {
    fn eq(&self, other: &Self) -> bool {
        self.source_path == other.source_path && self.raw.get() == other.raw.get()
    }
}

impl Eq for SourceContext {}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatementRef {
    pub block_id: SourceBlockId,
    pub binding: DecimalU64,
}

/// The only admission value carried by an event filing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Admission {
    Admitted,
}

/// A mechanical update filed atomically with its licensed source statement.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum StatementEffect {
    Mark(MarkEffect),
    Plan(Box<PlanEffect>),
}

/// An admitted complete Text block. The member owns acceptance and commit.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatementFile {
    pub block_id: SourceBlockId,
    pub source: SourceEnvelope,
    pub partition_rule_id: String,
    pub source_path: Vec<SourcePathPart>,
    pub admission: Admission,
    pub raw_block: Box<RawValue>,
    pub text: String,
    pub scheme_id: String,
    pub bindings: Vec<Binding>,
    #[serde(deserialize_with = "super::required_option")]
    pub context: Option<SourceContext>,
    #[serde(deserialize_with = "super::required_option")]
    pub effect: Option<StatementEffect>,
}

impl StatementFile {
    /// Check source structure and spans, without authorizing an effect or commit.
    ///
    /// The store must also check its opened run, session, scheme, source uniqueness
    /// and the registry's context/effect license against its current state.
    pub fn validate_structure(&self, verbs: &[String]) -> Result<(), ValidationError> {
        validate_source(
            &self.block_id,
            &self.source,
            &self.partition_rule_id,
            &self.scheme_id,
        )?;
        if !is_text_path(&self.source_path, self.block_id.block) {
            return Err(ValidationError(
                "source path must address the complete original Text block",
            ));
        }
        validate_text_block(&self.raw_block, &self.text)?;
        validate_bindings(&self.bindings, &self.text, verbs)?;
        if let Some(context) = &self.context {
            if self.block_id.block.0 != 0
                || context.source_path != [SourcePathPart::Key("context".into())]
            {
                return Err(ValidationError(
                    "context belongs only to the original block zero",
                ));
            }
        } else if self.effect.is_some() {
            return Err(ValidationError(
                "an effect requires its copied source context",
            ));
        }
        Ok(())
    }
}

/// An admitted whole pair record, never classified as a new statement.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PairFile {
    pub block_id: SourceBlockId,
    pub source: SourceEnvelope,
    pub partition_rule_id: String,
    pub source_path: Vec<SourcePathPart>,
    pub admission: Admission,
    pub raw_block: Box<RawValue>,
    pub scheme_id: String,
    pub pair: PairRecord,
}

impl PairFile {
    /// Check copied identity and complete payload equality. Pair lifecycle guards
    /// and the opened run, session and scheme remain obligations of the member.
    pub fn validate_structure(&self) -> Result<(), ValidationError> {
        validate_source(
            &self.block_id,
            &self.source,
            &self.partition_rule_id,
            &self.scheme_id,
        )?;
        if self.block_id.block.0 != 0 || !self.source_path.is_empty() {
            return Err(ValidationError(
                "a pair filing addresses the whole payload at block zero",
            ));
        }
        // The trace payload is the record inside the floor's external pair tag.
        let equal = match &self.pair {
            PairRecord::Started(record) => {
                serde_json::from_str::<super::PairStarted>(self.raw_block.get())
                    .is_ok_and(|raw| raw == *record)
            }
            PairRecord::Ended(record) => {
                serde_json::from_str::<super::PairEnded>(self.raw_block.get())
                    .is_ok_and(|raw| raw == *record)
            }
        };
        if !equal {
            return Err(ValidationError(
                "pair record differs from its complete copied payload",
            ));
        }
        Ok(())
    }
}

fn validate_source(
    id: &SourceBlockId,
    source: &SourceEnvelope,
    rule: &str,
    scheme: &str,
) -> Result<(), ValidationError> {
    if id.run != source.run || id.sequence != source.sequence {
        return Err(ValidationError(
            "block identity differs from the copied source envelope",
        ));
    }
    if rule.is_empty() || scheme.is_empty() {
        return Err(ValidationError(
            "filing requires a rule and scheme identity",
        ));
    }
    Ok(())
}

fn is_text_path(path: &[SourcePathPart], original_index: DecimalU64) -> bool {
    use SourcePathPart::{Index, Key};
    match path {
        [Key(content), Index { index }] => content == "content" && *index == original_index,
        [Key(message), Key(content), Index { index }] => {
            message == "message" && content == "content" && *index == original_index
        }
        _ => false,
    }
}

/// Decode only a complete Text block, without normalizing the retained JSON bytes.
pub(crate) fn validate_text_block(raw: &RawValue, text: &str) -> Result<(), ValidationError> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct TextBlock {
        #[serde(rename = "type")]
        kind: String,
        text: String,
    }
    let block: TextBlock = serde_json::from_str(raw.get())
        .map_err(|_| ValidationError("raw block is not an exact Text ContentBlock"))?;
    if block.kind != "text" || block.text != text {
        return Err(ValidationError(
            "text differs from the complete copied Text block",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(start: u64, end: u64) -> Span {
        Span {
            start: start.into(),
            end: end.into(),
        }
    }

    fn binding() -> Binding {
        Binding {
            statement_span: span(0, 12),
            verb: "read".into(),
            verb_span: span(0, 4),
            condition: None,
            condition_span: None,
            nouns: vec![NounBinding {
                value: "café".into(),
                span: span(5, 10),
            }],
        }
    }

    #[test]
    fn binding_checks_decoded_utf8_bytes_without_normalizing_nouns() {
        let text = "Read café. ";
        let mut b = binding();
        b.statement_span = span(0, text.len() as u64);
        assert!(b.validate(text, &["read".into()]).is_ok());
        let mut bad = b.clone();
        bad.nouns[0].span.end = 9.into();
        assert!(bad.validate(text, &["read".into()]).is_err());
        bad = b.clone();
        bad.nouns[0].value = "CAFÉ".into();
        assert!(bad.validate(text, &["read".into()]).is_err());
        bad = b.clone();
        bad.nouns.clear();
        assert!(bad.validate(text, &["read".into()]).is_err());
        assert!(b.validate(text, &["write".into()]).is_err());
        bad = b.clone();
        bad.statement_span = span(5, text.len() as u64);
        assert!(bad.validate(text, &["read".into()]).is_err());
        bad = b;
        bad.condition = Some("step:1".into());
        assert!(bad.validate(text, &["read".into()]).is_err());
    }

    #[test]
    fn nullable_binding_fields_are_required_and_duplicates_refuse() {
        let encoded = serde_json::to_string(&binding()).unwrap();
        assert!(encoded.contains("\"condition\":null,\"condition_span\":null"));
        for missing in ["\"condition\":null,", "\"condition_span\":null,"] {
            assert!(serde_json::from_str::<Binding>(&encoded.replace(missing, "")).is_err());
        }
        assert!(
            serde_json::from_str::<Binding>(
                &encoded.replace("\"verb\":", "\"verb\":\"read\",\"verb\":")
            )
            .is_err()
        );
    }

    #[test]
    fn envelope_golden_bytes_keep_attribution_and_existing_omissions() {
        let source = SourceEnvelope {
            session: "s".into(),
            run: "r".into(),
            turn: None,
            sequence: DecimalU64(u64::MAX),
            kind: "message.assistant".into(),
            channel: "output".into(),
            origin: "model".into(),
            subsystem: "harness".into(),
            causal_parent: None,
            wall_ms: 1,
            monotonic_ns: DecimalU64(u64::MAX),
        };
        let json = serde_json::to_string(&source).unwrap();
        assert_eq!(
            json,
            r#"{"session":"s","run":"r","sequence":"18446744073709551615","kind":"message.assistant","channel":"output","origin":"model","subsystem":"harness","wall_ms":1,"monotonic_ns":"18446744073709551615"}"#
        );
        assert_eq!(
            serde_json::from_str::<SourceEnvelope>(&json).unwrap(),
            source
        );
        assert!(
            serde_json::from_str::<SourceEnvelope>(&json.replace("\"origin\":\"model\",", ""))
                .is_err()
        );
        for field in ["turn", "causal_parent"] {
            let with_null = format!("{{\"{field}\":null,{}", &json[1..]);
            assert!(serde_json::from_str::<SourceEnvelope>(&with_null).is_err());
        }
    }

    #[test]
    fn source_paths_keep_original_array_indices_and_strict_index_objects() {
        let path = vec![
            SourcePathPart::Key("content".into()),
            SourcePathPart::Index { index: 7.into() },
        ];
        assert_eq!(
            serde_json::to_string(&path).unwrap(),
            r#"["content",{"index":"7"}]"#
        );
        for bad in [
            r#"{"index":7}"#,
            r#"{"index":"07"}"#,
            r#"{"index":"7","extra":0}"#,
            r#"{"index":"7","index":"8"}"#,
        ] {
            assert!(
                serde_json::from_str::<SourcePathPart>(bad).is_err(),
                "{bad}"
            );
        }
    }

    fn statement() -> StatementFile {
        let text = "Read café. ";
        let mut b = binding();
        b.statement_span = span(0, text.len() as u64);
        StatementFile {
            block_id: SourceBlockId {
                run: "r".into(),
                sequence: 1.into(),
                block: 3.into(),
            },
            source: SourceEnvelope {
                session: "s".into(),
                run: "r".into(),
                turn: Some("t".into()),
                sequence: 1.into(),
                kind: "message.assistant".into(),
                channel: "output".into(),
                origin: "model".into(),
                subsystem: "harness".into(),
                causal_parent: None,
                wall_ms: 1,
                monotonic_ns: 2.into(),
            },
            partition_rule_id: "output".into(),
            source_path: vec![
                SourcePathPart::Key("content".into()),
                SourcePathPart::Index { index: 3.into() },
            ],
            admission: Admission::Admitted,
            raw_block: RawValue::from_string(r#"{"type":"text","text":"Read caf\u00e9. "}"#.into())
                .unwrap(),
            text: text.into(),
            scheme_id: "test-scheme".into(),
            bindings: vec![b],
            context: None,
            effect: None,
        }
    }

    #[test]
    fn filing_preserves_raw_spelling_and_original_block_identity() {
        let filing = statement();
        filing.validate_structure(&["read".into()]).unwrap();
        let json = serde_json::to_string(&filing).unwrap();
        assert!(json.contains(r#""raw_block":{"type":"text","text":"Read caf\u00e9. "}"#));
        let decoded: StatementFile = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.raw_block.get(), filing.raw_block.get());
        assert_eq!(decoded.source_path, filing.source_path);
        assert_eq!(decoded.text, "Read café. ");
        for field in ["\"context\":null,", ",\"effect\":null"] {
            assert!(serde_json::from_str::<StatementFile>(&json.replace(field, "")).is_err());
        }

        let mut bad = filing.clone();
        bad.block_id.block = 0.into();
        assert!(bad.validate_structure(&["read".into()]).is_err());
        bad = filing.clone();
        bad.source.run = "another-run".into();
        assert!(bad.validate_structure(&["read".into()]).is_err());
        bad = filing.clone();
        bad.source_path.push(SourcePathPart::Key("text".into()));
        assert!(bad.validate_structure(&["read".into()]).is_err());
        bad = filing;
        bad.context = Some(SourceContext {
            source_path: vec![SourcePathPart::Key("context".into())],
            raw: RawValue::from_string("{}".into()).unwrap(),
        });
        assert!(bad.validate_structure(&["read".into()]).is_err());
    }

    #[test]
    fn filing_cannot_project_tool_blocks_or_change_copied_text() {
        let mut filing = statement();
        for raw in [
            r#"{"type":"tool_call","text":"Read café. "}"#,
            r#"{"type":"text","text":"Read changed text."}"#,
            r#"{"type":"text","text":"Read café. ","arguments":"hidden"}"#,
            r#"{"type":"text","text":"Read café. ","text":"Read café. "}"#,
        ] {
            filing.raw_block = RawValue::from_string(raw.into()).unwrap();
            assert!(
                filing.validate_structure(&["read".into()]).is_err(),
                "{raw}"
            );
        }
    }

    #[test]
    fn pair_filing_matches_whole_payload_without_classification() {
        let statement = statement();
        let record = super::super::PairStarted {
            call_id: super::super::CallId {
                run: "r".into(),
                sequence: 1.into(),
                call: 0.into(),
            },
            call_block: 4.into(),
            statement: StatementRef {
                block_id: statement.block_id.clone(),
                binding: 0.into(),
            },
            lane: super::super::PairLane::External,
            request: super::super::PairRequest::External {
                name: "sh".into(),
                arguments: "ls".into(),
            },
            retry_of: None,
        };
        let mut filing = PairFile {
            block_id: SourceBlockId {
                run: "r".into(),
                sequence: 2.into(),
                block: 0.into(),
            },
            source: SourceEnvelope {
                sequence: 2.into(),
                kind: "tool.call.started".into(),
                channel: "tool".into(),
                ..statement.source
            },
            partition_rule_id: "pairs".into(),
            source_path: vec![],
            admission: Admission::Admitted,
            raw_block: serde_json::value::to_raw_value(&record).unwrap(),
            scheme_id: "test-scheme".into(),
            pair: PairRecord::Started(record.clone()),
        };
        filing.validate_structure().unwrap();
        filing.pair = PairRecord::Started(super::super::PairStarted {
            call_block: 5.into(),
            ..record
        });
        assert!(filing.validate_structure().is_err());
        filing.block_id.block = 1.into();
        assert!(filing.validate_structure().is_err());
    }
}
