//! Private canonical material from completed reads or filed task input.
//! Every unit retains its actual source owner. Inspection records are not grants.

use std::sync::Arc;

use weaver_trace::Sequence;
use weaver_traits::{ContentBlock, Message, Role, ToolCall, ToolResultBlock};
use weaver_types::state_flow::{
    CallId, ContentRef, DecimalU64, EventSource, MaterialRef, PairEnded, PairFate, PairReadSource,
    PairRequest, QueryResult, ReadEntry, ReadPair, ReadSelector, ReadSource, SourceBlockId, Span,
    SpanRef, StatementRef, StepQueryRef, UnitId, decode_strict,
};

use super::context::{BegunTask, RecordedRead, RecordingContext, RunFailure};

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

#[derive(Debug)]
pub(super) struct TaskMaterialError {
    pub kind: MaterialFailure,
    pub task: Box<BegunTask>,
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
enum Evidence {
    Reads(Vec<RecordedRead>),
    Pair {
        origin: Arc<()>,
        pair: Arc<super::context::CompletedPair>,
    },
    FiledTask(Box<BegunTask>),
    FiledSource {
        origin: Arc<()>,
        source: Box<super::ledger::CommittedSource>,
    },
    /// Spec 08's instruction unit: the filed loop.instruction and the
    /// certified expansion grant whose exact Text it filed.
    FiledInstruction {
        origin: Arc<()>,
        source: Box<super::ledger::CommittedSource>,
        expansion: Box<super::templates::TemplateExpansion>,
    },
}

impl Evidence {
    fn origin(&self) -> &Arc<()> {
        match self {
            // Read constructors retain at least one read and check every owner.
            Self::Reads(reads) => reads[0].origin(),
            Self::FiledTask(task) => task.origin(),
            Self::FiledSource { origin, .. }
            | Self::FiledInstruction { origin, .. }
            | Self::Pair { origin, .. } => origin,
        }
    }

    fn reads(&self) -> &[RecordedRead] {
        match self {
            Self::Reads(reads) => reads,
            // The retained marks catalog is not candidate-producing content.
            Self::FiledTask(_)
            | Self::FiledSource { .. }
            | Self::FiledInstruction { .. }
            | Self::Pair { .. } => &[],
        }
    }

    fn task(&self) -> Option<&BegunTask> {
        match self {
            Self::Reads(_)
            | Self::FiledSource { .. }
            | Self::FiledInstruction { .. }
            | Self::Pair { .. } => None,
            Self::FiledTask(task) => Some(task),
        }
    }

    fn belongs_to(&self, context: &RecordingContext) -> bool {
        match self {
            Self::Reads(reads) => reads.iter().all(|read| read.belongs_to(context)),
            Self::FiledTask(task) => task.belongs_to(context),
            Self::FiledSource { origin, .. }
            | Self::FiledInstruction { origin, .. }
            | Self::Pair { origin, .. } => Arc::ptr_eq(origin, context.material_origin()),
        }
    }
}

/// Immutable source-backed unit. No Deserialize, Clone, raw constructor or
/// conversion into a tool, input, filing, placement or dispatch capability.
#[derive(Debug)]
pub(super) struct SelectedUnit {
    evidence: Arc<Evidence>,
    unit: Unit,
}

impl SelectedUnit {
    /// Corrupt only coverage after real source and append evidence exists.
    /// No production constructor or proof capability is exposed.
    #[cfg(test)]
    pub(super) fn without_return_coverage_for_test(&self) -> Self {
        let mut unit = self.unit.clone();
        unit.sources.clear();
        Self {
            evidence: Arc::clone(&self.evidence),
            unit,
        }
    }
    pub(super) fn origin(&self) -> &Arc<()> {
        self.evidence.origin()
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
    /// Candidate-producing content reads only. This slice is empty for fresh
    /// task input; callers must inspect it before indexing. That material has
    /// no read rank and is mandatory-only under SORT-PORT.
    pub(super) fn reads(&self) -> &[RecordedRead] {
        self.evidence.reads()
    }
    pub(super) fn task(&self) -> Option<&BegunTask> {
        self.evidence.task()
    }
    pub(super) fn belongs_to(&self, context: &RecordingContext) -> bool {
        self.evidence.belongs_to(context)
    }
}

/// One admitted source conversion. Each resulting unit independently retains
/// its evidence when this container is consumed, without cloning a capability.
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
            .reads()
            .iter()
            .map(RecordedRead::recall_sequence)
    }
    pub(super) fn task(&self) -> Option<&BegunTask> {
        self.evidence.task()
    }
    pub(super) fn belongs_to(&self, context: &RecordingContext) -> bool {
        self.evidence.belongs_to(context)
    }
}

#[derive(Debug)]
#[cfg_attr(test, derive(Clone))]
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
            if !matches!(&self.id, UnitId::Span { .. })
                || source.envelope.kind != "loop.instruction"
                || reference.instruction.block_id != source.block_id
                || entry.bindings.len() != 1
                || entry.bindings[0].ordinal != reference.instruction.binding
                || entry.bindings[0].value.statement_span != interval
            {
                return Err(MaterialFailure::Invalid(
                    "step query is not its exact instruction",
                ));
            }
            self.step_queries.push(StepQueryOccurrence {
                reference: reference.clone(),
                message: (message as u64).into(),
            });
            // This occurrence is only the previous-step query's instruction.
            // The checked read retains its attachments as evidence, but they
            // do not expand prior work or add request/returned coverage here.
            return Ok(());
        }
        let mut incomplete = false;
        for pair in pairs {
            match self.pair(entry, interval, message, pair) {
                Err(MaterialFailure::IncompleteCandidates) => incomplete = true,
                result => result?,
            }
        }
        if incomplete {
            return Err(MaterialFailure::IncompleteCandidates);
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
        if local_id.0 == 0 || entries.is_empty() {
            return Err(MaterialFailure::InvalidSpan);
        }
        let mut unit = Unit::new(UnitId::Span { span: span.clone() });
        unit.spans.push(span);
        let mut incomplete = false;
        for entry in entries {
            let selected = entry
                .selected_span
                .as_ref()
                .ok_or(MaterialFailure::Invalid("span lacks its selected interval"))?;
            match unit.text(
                entry,
                Span {
                    start: selected.start,
                    end: selected.end,
                },
                &entry.pairs.iter().collect::<Vec<_>>(),
            ) {
                Err(MaterialFailure::IncompleteCandidates) => incomplete = true,
                result => result?,
            }
        }
        unit.validate_map()?;
        if incomplete {
            return Err(MaterialFailure::IncompleteCandidates);
        }
        return Ok(vec![unit]);
    }
    let mut units = vec![];
    let mut incomplete = false;
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
        match unit.text(entry, interval, &entry.pairs.iter().collect::<Vec<_>>()) {
            Err(MaterialFailure::IncompleteCandidates) => incomplete = true,
            result => result?,
        }
        unit.validate_map()?;
        units.push(unit);
    }
    if incomplete {
        return Err(MaterialFailure::IncompleteCandidates);
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
    for attached in &held.pairs {
        if attached.call_id == pair.call_id && !same_pair_sources(attached, pair) {
            return Err(MaterialFailure::Invalid(
                "pair attachment differs between held snapshots",
            ));
        }
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

fn task_unit(task: &BegunTask) -> Result<Unit, MaterialFailure> {
    let message = task.input().message();
    let [ContentBlock::Text { text }] = message.content.as_slice() else {
        return Err(MaterialFailure::Invalid("filed task input is not one Text"));
    };
    if text.is_empty() {
        return Err(MaterialFailure::Invalid("filed task input Text is empty"));
    }
    let source = ContentRef::Event {
        block_id: task.input().source().clone(),
    };
    let mut unit = Unit::new(UnitId::Block {
        source: source.clone(),
    });
    // FiledTaskInput proves message.user. The source kind determines the
    // canonical role, rather than a role supplied on an inspection Message.
    unit.messages.push(Message {
        role: Role::User,
        content: vec![ContentBlock::Text { text: text.clone() }],
    });
    unit.map(
        MaterialRef::Text {
            source,
            span: Span {
                start: 0.into(),
                end: (text.len() as u64).into(),
            },
        },
        0,
        Some((0, text.len() as u64)),
    );
    unit.validate_map()?;
    Ok(unit)
}

pub(super) fn from_task(task: BegunTask) -> Result<HeldMaterial, TaskMaterialError> {
    match task_unit(&task) {
        Ok(unit) => {
            // Keep the complete task owner, including both receipts and the
            // marks catalog. None of those becomes content-read provenance.
            let evidence = Arc::new(Evidence::FiledTask(Box::new(task)));
            Ok(HeldMaterial {
                units: vec![SelectedUnit {
                    evidence: evidence.clone(),
                    unit,
                }],
                evidence,
            })
        }
        Err(kind) => Err(TaskMaterialError {
            kind,
            task: Box::new(task),
            failure: None,
        }),
    }
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
            let evidence = Arc::new(Evidence::Reads(reads));
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

/// Only a real live-ledger committed whole Text can enter the reload UNIT path.
/// The original immutable receipt remains owned by every resulting unit.
#[allow(clippy::result_large_err)] // Return the original receipt intact on a refused conversion.
pub(super) fn from_filed(
    origin: Arc<()>,
    source: super::ledger::CommittedSource,
) -> Result<SelectedUnit, (super::ledger::CommittedSource, MaterialFailure)> {
    let super::ledger::LedgerFile::Statement(file) = source.file() else {
        return Err((
            source,
            MaterialFailure::Invalid("reload source is not Text"),
        ));
    };
    let role = match file.source.kind.as_str() {
        "message.assistant" => Role::Assistant,
        "message.user" | "loop.instruction" | "plan.event" | "step.outcome" | "sort.decision"
        | "state.mark" => Role::User,
        _ => return Err((source, MaterialFailure::Invalid("unknown reload Text kind"))),
    };
    if file.text.is_empty() {
        return Err((source, MaterialFailure::InvalidSpan));
    }
    let content = ContentRef::Event {
        block_id: file.block_id.clone(),
    };
    let mut unit = Unit::new(UnitId::Block {
        source: content.clone(),
    });
    unit.messages.push(Message {
        role,
        content: vec![ContentBlock::Text {
            text: file.text.clone(),
        }],
    });
    unit.map(
        MaterialRef::Text {
            source: content,
            span: Span {
                start: 0.into(),
                end: (file.text.len() as u64).into(),
            },
        },
        0,
        Some((0, file.text.len() as u64)),
    );
    let evidence = Arc::new(Evidence::FiledSource {
        origin,
        source: Box::new(source),
    });
    Ok(SelectedUnit { evidence, unit })
}

/// Spec 08's instruction unit: a live-ledger committed loop.instruction whose
/// whole Text is exactly the certified expansion it was filed from. The unit
/// retains both, so neither copied text nor a source ID can construct it.
#[allow(clippy::result_large_err)] // Return the original receipt intact on a refused conversion.
pub(super) fn from_filed_instruction(
    origin: Arc<()>,
    source: super::ledger::CommittedSource,
    expansion: super::templates::TemplateExpansion,
) -> Result<SelectedUnit, (super::ledger::CommittedSource, MaterialFailure)> {
    let super::ledger::LedgerFile::Statement(file) = source.file() else {
        return Err((
            source,
            MaterialFailure::Invalid("instruction source is not Text"),
        ));
    };
    if file.source.kind != "loop.instruction"
        || file.source.origin != "loop"
        || file.text.is_empty()
        || file.text != expansion.text()
    {
        return Err((
            source,
            MaterialFailure::Invalid("instruction source is not its certified expansion"),
        ));
    }
    let content = ContentRef::Event {
        block_id: file.block_id.clone(),
    };
    let mut unit = Unit::new(UnitId::Instruction {
        source: content.clone(),
    });
    unit.messages.push(Message {
        role: Role::User,
        content: vec![ContentBlock::Text {
            text: file.text.clone(),
        }],
    });
    unit.map(
        MaterialRef::Text {
            source: content,
            span: Span {
                start: 0.into(),
                end: (file.text.len() as u64).into(),
            },
        },
        0,
        Some((0, file.text.len() as u64)),
    );
    let evidence = Arc::new(Evidence::FiledInstruction {
        origin,
        source: Box::new(source),
        expansion: Box::new(expansion),
    });
    Ok(SelectedUnit { evidence, unit })
}

/// Spec 08's instruction unit for exactly this filed source, owned by this run.
pub(super) fn is_instruction_unit(
    unit: &SelectedUnit,
    source: &SourceBlockId,
    context: &RecordingContext,
) -> bool {
    unit.belongs_to(context)
        && matches!(unit.unit_id(), UnitId::Instruction {
            source: ContentRef::Event { block_id },
        } if block_id == source)
        && matches!(unit.evidence.as_ref(), Evidence::FiledInstruction { source: filed, .. }
            if filed.source() == source)
}

pub(super) fn from_completed_pair(
    origin: Arc<()>,
    pair: Arc<super::context::CompletedPair>,
) -> SelectedUnit {
    let call_id = pair.started().call_id.clone();
    let mut unit = Unit::new(UnitId::Result {
        call_id: call_id.clone(),
    });
    unit.messages.push(Message {
        role: Role::ToolResult,
        content: vec![ContentBlock::ToolResult(pair.result().block())],
    });
    unit.map(MaterialRef::Returned { call_id }, 0, None);
    SelectedUnit {
        unit,
        evidence: Arc::new(Evidence::Pair { origin, pair }),
    }
}
impl SelectedUnit {
    pub(super) fn filed_source(&self) -> Option<&super::ledger::CommittedSource> {
        match self.evidence.as_ref() {
            Evidence::FiledSource { source, .. } | Evidence::FiledInstruction { source, .. } => {
                Some(source)
            }
            _ => None,
        }
    }
    pub(super) fn completed_pair(&self) -> Option<&super::context::CompletedPair> {
        match self.evidence.as_ref() {
            Evidence::Pair { pair, .. } => Some(pair),
            _ => None,
        }
    }
}
