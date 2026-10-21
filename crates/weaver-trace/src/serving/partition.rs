use std::collections::BTreeSet;

use serde::Serialize;

use super::{Channel, ContentSchema, Kind, Origin, content_schema};

/// The caller converts the floor's loaded rule list field for field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Partition {
    pub rules: Vec<PartitionRule>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PartitionRule {
    pub id: String,
    pub kind: String,
    pub channels: Vec<String>,
    pub origins: Vec<String>,
    pub blocks: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
}

/// A proof of grammar, finite vocabulary, tuple separation and serving coverage.
/// Only `validate_partition` constructs this value.
#[derive(Debug)]
pub struct ValidatedPartition {
    pub(super) rules: Vec<Rule>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartitionErrorCode {
    UnknownVocabulary,
    InvalidProjection,
    Overlap,
    ForbiddenProjection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartitionError {
    pub code: PartitionErrorCode,
    pub rule_id: Option<String>,
    pub path: Option<String>,
}

impl std::fmt::Display for PartitionError {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            out,
            "partition {:?} at {:?}: {:?}",
            self.code, self.rule_id, self.path
        )
    }
}

impl std::error::Error for PartitionError {}

#[derive(Debug)]
pub(super) struct Rule {
    pub id: String,
    pub kind: Kind,
    pub channels: Vec<Channel>,
    pub origins: Vec<Origin>,
    pub projections: Vec<Projection>,
    pub context: bool,
}

impl Rule {
    pub fn matches(&self, kind: Kind, channel: Channel, origin: Origin) -> bool {
        self.kind == kind && self.channels.contains(&channel) && self.origins.contains(&origin)
    }

    pub fn all_text(&self) -> bool {
        self.projections
            .iter()
            .any(|p| matches!(p, Projection::Text { index: None, .. }))
    }

    pub fn block_zero(&self) -> bool {
        self.all_text()
            || self
                .projections
                .iter()
                .any(|p| matches!(p, Projection::Text { index: Some(0), .. }))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Projection {
    Text { nested: bool, index: Option<u64> },
    Pair,
}

impl Projection {
    fn parse(value: &str) -> Option<Self> {
        if value == "pair" {
            return Some(Self::Pair);
        }
        let (nested, index) = if let Some(index) = value.strip_prefix("content.text[") {
            (false, index)
        } else if let Some(index) = value.strip_prefix("message.content.text[") {
            (true, index)
        } else {
            return None;
        };
        let index = index.strip_suffix(']')?;
        let index = if index == "*" {
            None
        } else {
            Some(super::record::decimal(index)?)
        };
        Some(Self::Text { nested, index })
    }

    fn overlaps(self, other: Self) -> bool {
        match (self, other) {
            (Self::Pair, _) | (_, Self::Pair) => true,
            (
                Self::Text {
                    nested: a,
                    index: x,
                },
                Self::Text {
                    nested: b,
                    index: y,
                },
            ) => a == b && (x.is_none() || y.is_none() || x == y),
        }
    }

    fn licensed(self, schema: ContentSchema, internal_plan: bool) -> bool {
        match (schema, self) {
            (ContentSchema::Message, Self::Text { nested: false, .. }) => true,
            (ContentSchema::Pair, Self::Pair) => true,
            (
                ContentSchema::OneText { .. },
                Self::Text {
                    nested: true,
                    index,
                },
            ) => index.is_none() || index == Some(0),
            (
                ContentSchema::Plan,
                Self::Text {
                    nested: true,
                    index,
                },
            ) => !internal_plan || index.is_none() || index == Some(0),
            _ => false,
        }
    }
}

/// Validate a complete serving partition, without inspecting any message content.
///
/// An empty declaration is syntactically representable. It cannot prove the
/// serving capability coverage required before activation.
pub fn validate_partition(partition: &Partition) -> Result<ValidatedPartition, PartitionError> {
    let mut ids = BTreeSet::new();
    let mut tuples = BTreeSet::new();
    let mut rules = Vec::new();
    for source in &partition.rules {
        let error = |code, path: Option<String>| PartitionError {
            code,
            rule_id: Some(source.id.clone()),
            path,
        };
        if source.id.is_empty() || !ids.insert(&source.id) {
            return Err(error(
                PartitionErrorCode::InvalidProjection,
                Some("id".into()),
            ));
        }
        let kind = Kind::from_name(&source.kind)
            .ok_or_else(|| error(PartitionErrorCode::UnknownVocabulary, Some("kind".into())))?;
        let schema = content_schema(kind);
        if matches!(schema, ContentSchema::Head | ContentSchema::None) {
            return Err(error(
                PartitionErrorCode::ForbiddenProjection,
                Some("kind".into()),
            ));
        }
        let channels = vocabulary_list(&source.channels, Channel::from_name)
            .map_err(|code| error(code, Some("channels".into())))?;
        let origins = vocabulary_list(&source.origins, Origin::from_name)
            .map_err(|code| error(code, Some("origins".into())))?;
        let needs_context = matches!(
            schema,
            ContentSchema::OneText { context: true } | ContentSchema::Plan
        );
        if source.context.as_deref() != needs_context.then_some("context") {
            return Err(error(
                PartitionErrorCode::InvalidProjection,
                Some("context".into()),
            ));
        }
        if source.blocks.is_empty() {
            return Err(error(
                PartitionErrorCode::InvalidProjection,
                Some("blocks".into()),
            ));
        }
        let internal_plan = kind == Kind::PlanEvent && channels.contains(&Channel::Internal);
        let mut projections = Vec::new();
        for path in &source.blocks {
            let projection = Projection::parse(path)
                .filter(|p| p.licensed(schema, internal_plan))
                .ok_or_else(|| error(PartitionErrorCode::InvalidProjection, Some(path.clone())))?;
            if projections
                .iter()
                .any(|previous: &Projection| previous.overlaps(projection))
            {
                return Err(error(PartitionErrorCode::Overlap, Some(path.clone())));
            }
            projections.push(projection);
        }
        // Context belongs to the source block at original index zero. Coverage
        // of mandatory serving tuples does not constrain additional rules.
        if needs_context
            && !projections.iter().any(|projection| {
                matches!(
                    projection,
                    Projection::Text {
                        index: None | Some(0),
                        ..
                    }
                )
            })
        {
            return Err(error(
                PartitionErrorCode::InvalidProjection,
                Some("blocks".into()),
            ));
        }
        for channel in &channels {
            for origin in &origins {
                if !tuples.insert((kind, *channel, *origin)) {
                    return Err(error(PartitionErrorCode::Overlap, None));
                }
            }
        }
        rules.push(Rule {
            id: source.id.clone(),
            kind,
            channels,
            origins,
            projections,
            context: needs_context,
        });
    }
    validate_coverage(&rules)?;
    Ok(ValidatedPartition { rules })
}

fn vocabulary_list<T: Copy + Ord>(
    source: &[String],
    parse: impl Fn(&str) -> Option<T>,
) -> Result<Vec<T>, PartitionErrorCode> {
    if source.is_empty() {
        return Err(PartitionErrorCode::InvalidProjection);
    }
    let mut seen = BTreeSet::new();
    source
        .iter()
        .map(|name| {
            let value = parse(name).ok_or(PartitionErrorCode::UnknownVocabulary)?;
            if !seen.insert(value) {
                return Err(PartitionErrorCode::Overlap);
            }
            Ok(value)
        })
        .collect()
}

fn validate_coverage(rules: &[Rule]) -> Result<(), PartitionError> {
    use Channel as C;
    use Kind as K;
    use Origin as O;
    // Every serving call lane, including available review, remains possible after
    // an operator plan update. This matrix is TRACE-VALIDATION, not a default.
    for (kind, channel, origin, all) in [
        (K::MessageUser, C::Input, O::Operator, true),
        (K::MessageUser, C::Input, O::External, true),
        (K::MessageAssistant, C::Output, O::Model, true),
        (K::LoopInstruction, C::Internal, O::Loop, false),
        (K::ToolCallStarted, C::Tool, O::Model, false),
        (K::ToolCallStarted, C::Tool, O::Loop, false),
        (K::ToolCallCompleted, C::Tool, O::Tool, false),
        (K::ToolCallCompleted, C::Tool, O::Loop, false),
        (K::StateMark, C::Internal, O::Loop, false),
        (K::PlanEvent, C::Input, O::Operator, true),
        (K::PlanEvent, C::Internal, O::Loop, false),
        (K::StepOutcome, C::Internal, O::Loop, false),
    ] {
        let matched = rules
            .iter()
            .find(|rule| rule.matches(kind, channel, origin));
        if !matched.is_some_and(|rule| {
            if content_schema(kind) == ContentSchema::Pair {
                rule.projections == [Projection::Pair]
            } else if all {
                rule.all_text()
            } else {
                rule.block_zero()
            }
        }) {
            return Err(PartitionError {
                code: PartitionErrorCode::InvalidProjection,
                rule_id: matched.map(|rule| rule.id.clone()),
                path: Some(format!(
                    "{}/{}/{}",
                    kind.as_str(),
                    channel.as_str(),
                    origin.as_str()
                )),
            });
        }
    }
    Ok(())
}
