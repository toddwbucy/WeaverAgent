//! Private, read-backed canonical material. Inspection records are not grants.
//! Every unit retains the exact released/Recorded read evidence that built it.

use std::sync::Arc;

use weaver_trace::Sequence;
use weaver_traits::{ContentBlock, Message, Role, ToolCall, ToolResultBlock};
use weaver_types::state_flow::{
    CallId, ContentRef, DecimalU64, EventSource, MaterialRef, PairEnded, PairFate, PairReadSource,
    PairRequest, QueryResult, ReadEntry, ReadPair, ReadSelector, ReadSource, SourceBlockId, Span,
    SpanRef, StatementRef, StepQueryRef, UnitId, decode_strict,
};

use super::context::{RecordedRead, RecordingContext, RunFailure};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum MaterialFailure {
    /// Inspection-only selectors have no material constructor.
    NotMaterial,
    InvalidSpan,
    IncompleteCandidates,
    ForeignContext,
    Invalid(&'static str),
    /// Lifecycle refused admission; the exact inputs remain in MaterialError.
    Unavailable,
}

#[derive(Debug)]
pub(super) struct MaterialError {
    pub kind: MaterialFailure,
    pub reads: Vec<RecordedRead>,
    pub failure: Option<RunFailure>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SourceMapEntry {
    pub material: MaterialRef,
    pub message: DecimalU64,
    pub block: DecimalU64,
    pub start: Option<DecimalU64>,
    pub end: Option<DecimalU64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct StepQueryOccurrence {
    pub reference: StepQueryRef,
    pub message: DecimalU64,
}

#[derive(Debug)]
struct Evidence {
    reads: Vec<RecordedRead>,
}

/// Immutable source-backed unit. No Deserialize, Clone, raw constructor or
/// conversion into a tool, input, filing, placement or dispatch capability.
#[derive(Debug)]
pub(super) struct SelectedUnit {
    evidence: Arc<Evidence>,
    unit: Unit,
}

impl SelectedUnit {
    pub(super) fn origin(&self) -> &Arc<()> {
        // All constructors retain at least one read and check a common owner.
        self.evidence.reads[0].origin()
    }

    pub(super) fn unit_id(&self) -> &UnitId {
        &self.unit.id
    }
    pub(super) fn sources(&self) -> &[MaterialRef] {
        &self.unit.sources
    }
    pub(super) fn source_map(&self) -> &[SourceMapEntry] {
        &self.unit.map
    }
    pub(super) fn messages(&self) -> &[Message] {
        &self.unit.messages
    }
    pub(super) fn spans(&self) -> &[SpanRef] {
        &self.unit.spans
    }
    pub(super) fn step_queries(&self) -> &[StepQueryOccurrence] {
        &self.unit.step_queries
    }
    pub(super) fn reads(&self) -> &[RecordedRead] {
        &self.evidence.reads
    }
    pub(super) fn belongs_to(&self, context: &RecordingContext) -> bool {
        self.evidence
            .reads
            .iter()
            .all(|read| read.belongs_to(context))
    }
}

/// One admitted read conversion. Each resulting unit independently retains its
/// evidence when this container is consumed, without cloning a read capability.
#[derive(Debug)]
pub(super) struct HeldMaterial {
    units: Vec<SelectedUnit>,
    evidence: Arc<Evidence>,
}

impl HeldMaterial {
    pub(super) fn units(&self) -> &[SelectedUnit] {
        &self.units
    }
    pub(super) fn into_units(self) -> Vec<SelectedUnit> {
        self.units
    }
    pub(super) fn recall_sequences(&self) -> impl Iterator<Item = Sequence> + '_ {
        self.evidence
            .reads
            .iter()
            .map(RecordedRead::recall_sequence)
    }
    pub(super) fn belongs_to(&self, context: &RecordingContext) -> bool {
        self.evidence
            .reads
            .iter()
            .all(|read| read.belongs_to(context))
    }
}

#[derive(Debug)]
struct Unit {
    id: UnitId,
    sources: Vec<MaterialRef>,
    map: Vec<SourceMapEntry>,
    messages: Vec<Message>,
    spans: Vec<SpanRef>,
    step_queries: Vec<StepQueryOccurrence>,
    pairs: Vec<CallId>,
}

impl Unit {
    fn new(id: UnitId) -> Self {
        Self {
            id,
            sources: vec![],
            map: vec![],
            messages: vec![],
            spans: vec![],
            step_queries: vec![],
            pairs: vec![],
        }
    }

    fn map(&mut self, material: MaterialRef, message: usize, offsets: Option<(u64, u64)>) {
        if !self.sources.contains(&material) {
            self.sources.push(material.clone());
        }
        self.map.push(SourceMapEntry {
            material,
            message: (message as u64).into(),
            block: 0.into(),
            start: offsets.map(|(start, _)| start.into()),
            end: offsets.map(|(_, end)| end.into()),
        });
    }

    fn text(
        &mut self,
        entry: &ReadEntry,
        interval: Span,
        pairs: &[&ReadPair],
    ) -> Result<(), MaterialFailure> {
        let ReadSource::Event(source) = &entry.source else {
            return Err(MaterialFailure::InvalidSpan);
        };
        let role = match source.envelope.kind.as_str() {
            "message.assistant" => Role::Assistant,
            "message.user" | "loop.instruction" | "plan.event" | "step.outcome"
            | "sort.decision" | "state.mark" => Role::User,
            _ => return Err(MaterialFailure::Invalid("unknown material source kind")),
        };
        let text = interval
            .slice(&entry.text)
            .map_err(|_| MaterialFailure::Invalid("invalid material Text interval"))?;
        if text.is_empty() {
            return Err(MaterialFailure::Invalid("empty material Text interval"));
        }
        let message = self.messages.len();
        self.messages.push(Message {
            role,
            content: vec![ContentBlock::Text {
                text: text.to_owned(),
            }],
        });
        self.map(
            MaterialRef::Text {
                source: ContentRef::Event {
                    block_id: source.block_id.clone(),
                },
                span: interval,
            },
            message,
            Some((0, text.len() as u64)),
        );
        if let Some(reference) = &entry.step_query {
            if source.envelope.kind != "loop.instruction"
                || reference.instruction.block_id != source.block_id
                || entry.bindings.len() != 1
                || entry.bindings[0].ordinal != reference.instruction.binding
                || entry.bindings[0].value.statement_span != interval
                || !pairs.is_empty()
            {
                return Err(MaterialFailure::Invalid(
                    "step query is not its exact instruction",
                ));
            }
            self.step_queries.push(StepQueryOccurrence {
                reference: reference.clone(),
                message: (message as u64).into(),
            });
        }
        for pair in pairs {
            self.pair(entry, interval, message, pair)?;
        }
        Ok(())
    }

    fn pair(
        &mut self,
        entry: &ReadEntry,
        interval: Span,
        text_message: usize,
        pair: &ReadPair,
    ) -> Result<(), MaterialFailure> {
        if pair.fate == PairFate::Opened {
            return Err(MaterialFailure::IncompleteCandidates);
        }
        let terminal_source = pair
            .terminal_source
            .as_ref()
            .ok_or(MaterialFailure::Invalid(
                "terminal pair has no terminal source",
            ))?;
        let terminal_raw = terminal_source.raw_block.get();
        if self.pairs.contains(&pair.call_id) {
            return Ok(());
        }
        let binding = entry
            .bindings
            .iter()
            .find(|binding| binding.ordinal == pair.statement.binding)
            .ok_or(MaterialFailure::Invalid("pair has no included binding"))?;
        if !matches!(&entry.source, ReadSource::Event(source)
            if source.block_id == pair.statement.block_id)
            || !interval.contains(&binding.value.statement_span)
        {
            return Err(MaterialFailure::Invalid(
                "pair statement is outside selected Text",
            ));
        }
        let request = MaterialRef::Request {
            call_id: pair.call_id.clone(),
        };
        match &pair.request {
            PairRequest::External { name, arguments } => {
                self.call(request, name.clone(), arguments.clone());
            }
            PairRequest::Review { command, .. } => {
                self.call(request, "sh".into(), command.clone());
            }
            PairRequest::Recall { .. } => {
                self.map(
                    request,
                    text_message,
                    Some((
                        binding.value.statement_span.start.0 - interval.start.0,
                        binding.value.statement_span.end.0 - interval.start.0,
                    )),
                );
            }
        }
        let terminal: PairEnded = decode_strict(terminal_raw)
            .map_err(|_| MaterialFailure::Invalid("invalid whole pair terminal"))?;
        let content = match &terminal.outcome {
            Some(outcome) => crate::tools::outcome_content(outcome),
            None => serde_json::to_string(&terminal)
                .map_err(|_| MaterialFailure::Invalid("pair terminal cannot be rendered"))?,
        };
        let message = self.messages.len();
        self.messages.push(Message {
            role: Role::ToolResult,
            content: vec![ContentBlock::ToolResult(ToolResultBlock { content })],
        });
        self.map(
            MaterialRef::Returned {
                call_id: pair.call_id.clone(),
            },
            message,
            None,
        );
        self.pairs.push(pair.call_id.clone());
        Ok(())
    }

    fn call(&mut self, material: MaterialRef, name: String, arguments: String) {
        let message = self.messages.len();
        self.messages.push(Message {
            role: Role::Assistant,
            content: vec![ContentBlock::ToolCall(ToolCall { name, arguments })],
        });
        self.map(material, message, None);
    }

    fn validate_map(&self) -> Result<(), MaterialFailure> {
        let mut distinct = vec![];
        for mapping in &self.map {
            let block = self
                .messages
                .get(mapping.message.0 as usize)
                .and_then(|message| message.content.get(mapping.block.0 as usize))
                .ok_or(MaterialFailure::Invalid("source map addresses no block"))?;
            match (block, mapping.start, mapping.end, &mapping.material) {
                (ContentBlock::Text { text }, Some(start), Some(end), material) => {
                    let local = Span { start, end };
                    if local.slice(text).is_err() || start >= end {
                        return Err(MaterialFailure::Invalid(
                            "source map has an invalid Text location",
                        ));
                    }
                    match material {
                        MaterialRef::Text { span, .. }
                            if span.end.0 - span.start.0 == end.0 - start.0 => {}
                        MaterialRef::Request { .. } => {}
                        _ => {
                            return Err(MaterialFailure::Invalid(
                                "source map changed Text coverage",
                            ));
                        }
                    }
                }
                (ContentBlock::ToolCall(_), None, None, MaterialRef::Request { .. })
                | (ContentBlock::ToolResult(_), None, None, MaterialRef::Returned { .. }) => {}
                _ => return Err(MaterialFailure::Invalid("source map changed a whole block")),
            }
            if !distinct.contains(&mapping.material) {
                distinct.push(mapping.material.clone());
            }
        }
        if distinct != self.sources {
            return Err(MaterialFailure::Invalid(
                "source map differs from ordered coverage",
            ));
        }
        Ok(())
    }
}

fn checked_entries(read: &RecordedRead) -> Result<&[ReadEntry], MaterialFailure> {
    super::read_validate::validate(read.selector(), read.result())
        .map_err(|error| MaterialFailure::Invalid(error.0))?;
    match (read.selector(), read.result()) {
        (
            ReadSelector::History { .. }
            | ReadSelector::Condition { .. }
            | ReadSelector::Statement { .. }
            | ReadSelector::Subjects { .. }
            | ReadSelector::Conversation { .. }
            | ReadSelector::Span { .. },
            QueryResult::Entries { entries },
        ) => {
            consistent_originals(entries)?;
            Ok(entries)
        }
        (ReadSelector::SystemPrompt {}, _) => Err(MaterialFailure::InvalidSpan),
        _ => Err(MaterialFailure::NotMaterial),
    }
}

/// One immutable read cannot give two originals for the same event block.
/// Filtered bindings and selected intervals may differ; the held source cannot.
fn consistent_originals(entries: &[ReadEntry]) -> Result<(), MaterialFailure> {
    let mut originals = std::collections::BTreeMap::new();
    let mut bindings = std::collections::BTreeMap::new();
    let mut pairs = std::collections::BTreeMap::new();
    for entry in entries {
        let ReadSource::Event(source) = &entry.source else {
            continue;
        };
        consistent_source(&mut originals, source, entry.raw_block.get())?;
        for pair in &entry.pairs {
            consistent_source(
                &mut originals,
                &pair.opening_source.source,
                pair.opening_source.raw_block.get(),
            )?;
            if let Some(terminal) = &pair.terminal_source {
                consistent_source(&mut originals, &terminal.source, terminal.raw_block.get())?;
            }
            let key = (&pair.call_id.run, pair.call_id.sequence, pair.call_id.call);
            if let Some(previous) = pairs.insert(key, pair)
                && !same_pair_sources(previous, pair)
            {
                return Err(MaterialFailure::Invalid(
                    "repeated pair changes its held sources",
                ));
            }
        }
        for binding in &entry.bindings {
            if let Some(previous) =
                bindings.insert((&source.block_id, binding.ordinal), &binding.value)
                && previous != &binding.value
            {
                return Err(MaterialFailure::Invalid(
                    "repeated binding ordinal changes its original value",
                ));
            }
        }
    }
    Ok(())
}

type Originals<'a> = std::collections::BTreeMap<&'a SourceBlockId, (&'a EventSource, &'a str)>;

fn consistent_source<'a>(
    originals: &mut Originals<'a>,
    source: &'a EventSource,
    raw: &'a str,
) -> Result<(), MaterialFailure> {
    if let Some((previous, previous_raw)) = originals.insert(&source.block_id, (source, raw))
        && (previous != source || previous_raw != raw)
    {
        return Err(MaterialFailure::Invalid(
            "one source identity has conflicting held content",
        ));
    }
    Ok(())
}

fn same_pair_source(left: &PairReadSource, right: &PairReadSource) -> bool {
    left.source == right.source && left.raw_block.get() == right.raw_block.get()
}

fn same_pair_sources(left: &ReadPair, right: &ReadPair) -> bool {
    // Local read validation has already bound every typed field to these exact
    // whole payloads. Null terminals remain legal for a valid opened pair.
    same_pair_source(&left.opening_source, &right.opening_source)
        && match (&left.terminal_source, &right.terminal_source) {
            (None, None) => true,
            (Some(left), Some(right)) => same_pair_source(left, right),
            _ => false,
        }
}

fn units(read: &RecordedRead) -> Result<Vec<Unit>, MaterialFailure> {
    let entries = checked_entries(read)?;
    if let ReadSelector::Span {
        seed_id,
        local_id,
        revision,
    } = read.selector()
    {
        let span = SpanRef {
            seed_id: seed_id.clone(),
            local_id: *local_id,
            revision: *revision,
        };
        if local_id.0 == 0 {
            return Err(MaterialFailure::InvalidSpan);
        }
        let mut unit = Unit::new(UnitId::Span { span: span.clone() });
        unit.spans.push(span);
        for entry in entries {
            let selected = entry
                .selected_span
                .as_ref()
                .ok_or(MaterialFailure::Invalid("span lacks its selected interval"))?;
            unit.text(
                entry,
                Span {
                    start: selected.start,
                    end: selected.end,
                },
                &entry.pairs.iter().collect::<Vec<_>>(),
            )?;
        }
        unit.validate_map()?;
        return Ok(vec![unit]);
    }
    let mut units = vec![];
    for entry in entries {
        let ReadSource::Event(source) = &entry.source else {
            return Err(MaterialFailure::InvalidSpan);
        };
        let (id, interval) = match read.selector() {
            ReadSelector::Subjects { .. } | ReadSelector::Conversation { .. } => (
                UnitId::Block {
                    source: ContentRef::Event {
                        block_id: source.block_id.clone(),
                    },
                },
                Span {
                    start: 0.into(),
                    end: (entry.text.len() as u64).into(),
                },
            ),
            _ => {
                let [binding] = entry.bindings.as_slice() else {
                    return Err(MaterialFailure::Invalid(
                        "statement unit lacks one original binding",
                    ));
                };
                (
                    UnitId::Statement {
                        statement: StatementRef {
                            block_id: source.block_id.clone(),
                            binding: binding.ordinal,
                        },
                    },
                    binding.value.statement_span,
                )
            }
        };
        let mut unit = Unit::new(id);
        unit.text(entry, interval, &entry.pairs.iter().collect::<Vec<_>>())?;
        unit.validate_map()?;
        units.push(unit);
    }
    Ok(units)
}

fn pair_unit(
    read: &RecordedRead,
    entry_index: usize,
    pair_index: usize,
    statement: &RecordedRead,
) -> Result<Vec<Unit>, MaterialFailure> {
    let entries = checked_entries(read)?;
    let entry = entries.get(entry_index).ok_or(MaterialFailure::Invalid(
        "pair selection names no read entry",
    ))?;
    let pair = entry.pairs.get(pair_index).ok_or(MaterialFailure::Invalid(
        "pair selection names no attachment",
    ))?;
    if pair.fate == PairFate::Opened {
        return Err(MaterialFailure::IncompleteCandidates);
    }
    let statement_entries = checked_entries(statement)?;
    if !matches!(statement.selector(), ReadSelector::Statement { statement } if statement == &pair.statement)
    {
        return Err(MaterialFailure::Invalid(
            "pair lacks its matching statement query",
        ));
    }
    let [held] = statement_entries else {
        return Err(MaterialFailure::Invalid(
            "pair statement query is not one entry",
        ));
    };
    let binding = &held.bindings[0]; // checked statement validation requires one.
    if held.source != entry.source
        || held.raw_block.get() != entry.raw_block.get()
        || held.text != entry.text
        || !entry.bindings.contains(binding)
    {
        return Err(MaterialFailure::Invalid(
            "pair statement differs between held snapshots",
        ));
    }
    let mut unit = Unit::new(UnitId::Pair {
        call_id: pair.call_id.clone(),
    });
    unit.text(held, binding.value.statement_span, &[pair])?;
    unit.validate_map()?;
    Ok(vec![unit])
}

pub(super) fn from_read(read: RecordedRead) -> Result<HeldMaterial, MaterialError> {
    let result = units(&read);
    finish(vec![read], result)
}

pub(super) fn from_pair(
    read: RecordedRead,
    entry: usize,
    pair: usize,
    statement: RecordedRead,
) -> Result<HeldMaterial, MaterialError> {
    let result = pair_unit(&read, entry, pair, &statement);
    finish(vec![read, statement], result)
}

fn finish(
    reads: Vec<RecordedRead>,
    units: Result<Vec<Unit>, MaterialFailure>,
) -> Result<HeldMaterial, MaterialError> {
    match units {
        Ok(units) => {
            let evidence = Arc::new(Evidence { reads });
            Ok(HeldMaterial {
                units: units
                    .into_iter()
                    .map(|unit| SelectedUnit {
                        evidence: evidence.clone(),
                        unit,
                    })
                    .collect(),
                evidence,
            })
        }
        Err(kind) => Err(MaterialError {
            kind,
            reads,
            failure: None,
        }),
    }
}
