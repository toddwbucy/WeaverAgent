use std::sync::Arc;

use serde_json::value::RawValue;

use crate::Sequence;

use super::partition::{Projection, Rule};
use super::record::{Object, Record};
use super::{ContentSchema, Kind, ValidatedPartition, content_schema};

/// Identity already assigned by the recorder before the tap. Supplying it also
/// attributes a failure when damaged canonical bytes cannot supply an envelope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceEvent {
    pub run: String,
    pub sequence: Sequence,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceBlock {
    pub run: String,
    pub sequence: Sequence,
    pub block: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathPart {
    Key(String),
    Index(u64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedBlock {
    pub source: SourceBlock,
    pub source_path: Vec<PathPart>,
    pub raw: Arc<str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextBlock {
    pub source_path: Vec<PathPart>,
    pub raw: Arc<str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatementCopy {
    pub source_envelope: Arc<str>,
    pub partition_rule_id: String,
    pub blocks: Vec<SelectedBlock>,
    pub context: Option<ContextBlock>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairCopy {
    pub source_envelope: Arc<str>,
    pub partition_rule_id: String,
    pub block: SelectedBlock,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdmittedCopy {
    Statement(StatementCopy),
    Pair(PairCopy),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TapFailureCode {
    InvalidCanonicalRecord,
    MissingProjection,
    InvalidProjection,
    CopyAllocationFailed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TapFailure {
    pub source: SourceEvent,
    pub code: TapFailureCode,
    pub source_path: Option<Vec<PathPart>>,
}

#[must_use = "the caller must account for a selected copy or a tap failure"]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TapOutcome {
    HeadUntapped,
    Unselected,
    Selected(AdmittedCopy),
    Failed(TapFailure),
    Stopped,
}

/// The pure tap has only a validated declaration and a terminal stop flag.
/// It owns no transport, filing frame, writer, classifier or retention policy.
pub struct Tee {
    partition: ValidatedPartition,
    stopped: bool,
}

impl Tee {
    pub fn new(partition: ValidatedPartition) -> Self {
        Self {
            partition,
            stopped: false,
        }
    }

    /// Terminal, including when called more than once. There is no restart API.
    pub fn stop(&mut self) {
        self.stopped = true;
    }

    /// Read one canonical record without mutating it. The future serving recorder
    /// calls this once after assigning identity and rendering, before enqueue.
    ///
    /// A zero-block StatementCopy is an explicit selected outcome. Its caller
    /// records that observation and owes no filing for a nonexistent Text block.
    /// Event payload licensing and conditional turn rules belong to authoring,
    /// not this projection boundary. A pair's typed codec belongs to its consumer.
    pub fn tap(&mut self, source: &SourceEvent, canonical_record: &str) -> TapOutcome {
        self.tap_with(source, canonical_record, &mut Copies::default())
    }

    fn tap_with(&mut self, source: &SourceEvent, line: &str, copies: &mut Copies) -> TapOutcome {
        if self.stopped {
            return TapOutcome::Stopped;
        }
        match self.copy(source, line, copies) {
            Ok(outcome) => outcome,
            Err(error) => {
                self.stop();
                TapOutcome::Failed(TapFailure {
                    source: source.clone(),
                    code: error.code,
                    source_path: error.path,
                })
            }
        }
    }

    fn copy(
        &self,
        source: &SourceEvent,
        line: &str,
        copies: &mut Copies,
    ) -> Result<TapOutcome, ProjectionFailure> {
        let record = Record::parse(source, line)
            .map_err(|_| ProjectionFailure::new(TapFailureCode::InvalidCanonicalRecord, None))?;
        if record.kind == Kind::Load {
            return Ok(TapOutcome::HeadUntapped);
        }
        let Some(rule) = self
            .partition
            .rules
            .iter()
            .find(|rule| rule.matches(record.kind, record.channel, record.origin))
        else {
            return Ok(TapOutcome::Unselected);
        };
        // Only the envelope selected the rule. Nothing in the payload below can
        // turn a selected event into Unselected.
        let payload = record
            .payload
            .ok_or_else(|| ProjectionFailure::missing(Vec::new()))?;
        let source_envelope = copies.arc(&record.envelope)?;
        let partition_rule_id = copies.string(&rule.id)?;
        if rule.projections == [Projection::Pair] {
            return Ok(TapOutcome::Selected(AdmittedCopy::Pair(PairCopy {
                source_envelope,
                partition_rule_id,
                block: SelectedBlock {
                    source: SourceBlock {
                        run: source.run.clone(),
                        sequence: source.sequence,
                        block: 0,
                    },
                    source_path: Vec::new(),
                    raw: copies.arc(payload.get())?,
                },
            })));
        }
        let (blocks, context) = project_statement(source, &record, rule, payload, copies)?;
        Ok(TapOutcome::Selected(AdmittedCopy::Statement(
            StatementCopy {
                source_envelope,
                partition_rule_id,
                blocks,
                context,
            },
        )))
    }
}

struct ProjectionFailure {
    code: TapFailureCode,
    path: Option<Vec<PathPart>>,
}

impl ProjectionFailure {
    fn new(code: TapFailureCode, path: Option<Vec<PathPart>>) -> Self {
        Self { code, path }
    }
    fn missing(path: Vec<PathPart>) -> Self {
        Self::new(TapFailureCode::MissingProjection, Some(path))
    }
    fn invalid(path: Vec<PathPart>) -> Self {
        Self::new(TapFailureCode::InvalidProjection, Some(path))
    }
    fn allocation() -> Self {
        Self::new(TapFailureCode::CopyAllocationFailed, None)
    }
}

fn project_statement(
    source: &SourceEvent,
    record: &Record<'_>,
    rule: &Rule,
    payload: &RawValue,
    copies: &mut Copies,
) -> Result<(Vec<SelectedBlock>, Option<ContextBlock>), ProjectionFailure> {
    let payload =
        Object::parse(payload.get()).map_err(|_| ProjectionFailure::invalid(Vec::new()))?;
    let nested = !matches!(content_schema(record.kind), ContentSchema::Message);
    let mut path = Vec::new();
    let nested_message;
    let message = if nested {
        path.push(PathPart::Key("message".into()));
        let raw = payload
            .get("message")
            .ok_or_else(|| ProjectionFailure::missing(path.clone()))?;
        nested_message =
            Object::parse(raw.get()).map_err(|_| ProjectionFailure::invalid(path.clone()))?;
        &nested_message
    } else {
        &payload
    };
    if !message.exact(&["role", "content"])
        || !message.string("role").is_ok_and(|role| {
            matches!(
                role.as_str(),
                "system" | "user" | "assistant" | "tool_result"
            )
        })
    {
        // Preserve MissingProjection for a missing content container.
        if message.get("content").is_none() {
            path.push(PathPart::Key("content".into()));
            return Err(ProjectionFailure::missing(path));
        }
        return Err(ProjectionFailure::invalid(path));
    }
    path.push(PathPart::Key("content".into()));
    let array: Vec<&RawValue> =
        serde_json::from_str(message.get("content").expect("checked content").get())
            .map_err(|_| ProjectionFailure::invalid(path.clone()))?;
    for projection in &rule.projections {
        if let Projection::Text {
            index: Some(index), ..
        } = projection
            && usize::try_from(*index)
                .ok()
                .is_none_or(|index| index >= array.len())
        {
            let mut missing = path.clone();
            missing.push(PathPart::Index(*index));
            return Err(ProjectionFailure::missing(missing));
        }
    }
    let mut blocks = Vec::new();
    for (index, raw) in array.into_iter().enumerate() {
        let index = u64::try_from(index).map_err(|_| ProjectionFailure::allocation())?;
        if !rule.projections.iter().any(|projection| {
            matches!(projection, Projection::Text { index: selected, .. } if selected.is_none() || *selected == Some(index))
        }) {
            continue;
        }
        let mut block_path = path.clone();
        block_path.push(PathPart::Index(index));
        if text_block(raw).map_err(|_| ProjectionFailure::invalid(block_path.clone()))? {
            blocks
                .try_reserve(1)
                .map_err(|_| ProjectionFailure::allocation())?;
            blocks.push(SelectedBlock {
                source: SourceBlock {
                    run: copies.string(&source.run)?,
                    sequence: source.sequence,
                    block: index,
                },
                source_path: block_path,
                raw: copies.arc(raw.get())?,
            });
        }
    }
    let context = if rule.context {
        let path = vec![PathPart::Key("context".into())];
        let raw = payload
            .get("context")
            .ok_or_else(|| ProjectionFailure::missing(path.clone()))?;
        Some(ContextBlock {
            source_path: path,
            raw: copies.arc(raw.get())?,
        })
    } else {
        None
    };
    Ok((blocks, context))
}

fn text_block(raw: &RawValue) -> Result<bool, ()> {
    let block = Object::parse(raw.get())?;
    let (fields, is_text) = match block.string("type")?.as_str() {
        "text" => (&["text"][..], true),
        "tool_call" => (&["name", "arguments"][..], false),
        "tool_result" => (&["content"][..], false),
        "unrecovered_call" => (&["fragment"][..], false),
        _ => return Err(()),
    };
    if block.0.len() != fields.len() + 1 {
        return Err(());
    }
    for field in fields {
        block.string(field)?;
    }
    Ok(is_text)
}

/// Fallible reservations report capacity errors that Rust exposes. As elsewhere
/// in this crate, allocator aborts during serde or Arc allocation cannot be caught.
#[derive(Default)]
struct Copies {
    #[cfg(test)]
    fail_at: Option<usize>,
    #[cfg(test)]
    copied: usize,
}

impl Copies {
    fn string(&mut self, value: &str) -> Result<String, ProjectionFailure> {
        #[cfg(test)]
        {
            let copied = self.copied;
            self.copied += 1;
            if self.fail_at == Some(copied) {
                return Err(ProjectionFailure::allocation());
            }
        }
        let mut result = String::new();
        result
            .try_reserve_exact(value.len())
            .map_err(|_| ProjectionFailure::allocation())?;
        result.push_str(value);
        Ok(result)
    }

    fn arc(&mut self, value: &str) -> Result<Arc<str>, ProjectionFailure> {
        self.string(value).map(Arc::from)
    }
}

#[cfg(test)]
#[test]
fn every_reportable_copy_allocation_failure_is_terminal_without_partial_selection() {
    use super::tests::{line, partition, source};
    use super::{Channel, Origin, validate_partition};
    let raw = line(
        Kind::MessageUser,
        Channel::Input,
        Origin::Operator,
        r#"{"role":"user","content":[{"type":"text","text":"Read x."}]}"#,
    );
    // Envelope, rule ID, source run, and complete Text are separately owned.
    for fail_at in 0..4 {
        let mut tee = Tee::new(validate_partition(&partition()).unwrap());
        let mut copies = Copies {
            fail_at: Some(fail_at),
            copied: 0,
        };
        let outcome = tee.tap_with(&source(), &raw, &mut copies);
        assert!(
            matches!(
                outcome,
                TapOutcome::Failed(TapFailure {
                    code: TapFailureCode::CopyAllocationFailed,
                    ..
                })
            ),
            "copy {fail_at}: {outcome:?}"
        );
        assert_eq!(tee.tap(&source(), &raw), TapOutcome::Stopped);
    }
}
