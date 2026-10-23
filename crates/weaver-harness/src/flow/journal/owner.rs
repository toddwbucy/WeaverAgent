//! Production-facing journal operations require the owning private proofs.
use super::*;
use crate::flow::place::mirror;
use crate::flow::place::{ConfirmedGeneration, PreparedPrefix};
use weaver_trace::serving::payloads as trace;
use weaver_types::state_flow::{CutMethod, CutReference};

impl PrefixProjection {
    pub(in crate::flow) fn revision(&self) -> u64 {
        self.revision
    }
    pub(in crate::flow) fn coverage(&self) -> &[MaterialRef] {
        &self.coverage
    }
    pub(in crate::flow) fn n(&self) -> u64 {
        self.n
    }
    pub(in crate::flow) fn resident_before(&self) -> u64 {
        self.resident_before
    }
    pub(in crate::flow) fn rebuild_messages(&self) -> Vec<Message> {
        self.rebuild.iter().map(|m| m.message.clone()).collect()
    }
}
impl PendingAppend {
    pub(in crate::flow) fn assembly(&self) -> u64 {
        self.assembly
    }
    pub(in crate::flow) fn assistant_id(&self) -> DecimalU64 {
        self.assistant_id
    }
    pub(in crate::flow) fn delta(&self) -> Vec<Message> {
        self.prefix
            .rebuild
            .iter()
            .chain(&self.new_messages)
            .map(|m| m.message.clone())
            .collect()
    }
    pub(in crate::flow) fn ids(&self) -> Vec<trace::DecimalU64> {
        self.prefix
            .rebuild
            .iter()
            .chain(&self.new_messages)
            .map(|m| m.id.0.into())
            .collect()
    }
    pub(in crate::flow) fn recomputed(&self) -> usize {
        self.prefix.rebuild.len()
    }
    pub(in crate::flow) fn map(&self, units: &[Arc<SelectedUnit>]) -> Vec<trace::UnitMapEntry> {
        let mut start = self.prefix.rebuild.len();
        let mut offset = 0;
        units
            .iter()
            .enumerate()
            .map(|(index, unit)| {
                let end = start + unit.messages().len();
                let messages = &self.new_messages[offset..offset + unit.messages().len()];
                let map = unit
                    .source_map()
                    .iter()
                    .map(|entry| trace::PlacedSource {
                        material: mirror::material(&entry.material),
                        message_id: messages[entry.message.0 as usize].id.0.into(),
                        block: entry.block.0.into(),
                        start: entry.start.map(|n| n.0.into()),
                        end: entry.end.map(|n| n.0.into()),
                    })
                    .collect();
                let result = trace::UnitMapEntry {
                    unit_id: mirror::unit(unit.unit_id()),
                    occurrence_id: trace::UnitOccurrenceId {
                        run: messages[0].occurrence.as_ref().unwrap().run.clone(),
                        assembly_id: self.assembly.into(),
                        unit_index: (index as u64).into(),
                    },
                    message_start: (start as u64).into(),
                    message_end: (end as u64).into(),
                    spans: unit
                        .spans()
                        .iter()
                        .map(crate::flow::mirror::span_ref)
                        .collect(),
                    source_map: map,
                    step_queries: unit
                        .step_queries()
                        .iter()
                        .map(|q| trace::PlacedStepQuery {
                            reference: mirror::query(&q.reference),
                            message_id: messages[q.message.0 as usize].id.0.into(),
                        })
                        .collect(),
                };
                start = end;
                offset += messages.len();
                result
            })
            .collect()
    }
}
impl Journal {
    pub(in crate::flow) fn revision(&self) -> u64 {
        self.revision
    }
    pub(in crate::flow) fn resident(&self) -> u64 {
        self.resident
    }
    pub(in crate::flow) fn prefix_tokens(&self) -> u64 {
        self.prefix
    }
    pub(in crate::flow) fn capacity(&self) -> u64 {
        self.capacity
    }
    pub(in crate::flow) fn next_assembly(&self) -> u64 {
        self.next_assembly
    }
    pub(in crate::flow) fn current_end(&self) -> ArgumentCursor {
        layout::end(&self.layout)
    }
    pub(in crate::flow) fn prepare(
        &self,
        cut: Option<CutReference>,
        method: CutMethod,
    ) -> Result<PreparedPrefix> {
        if self.pending.is_some() {
            return Err(JournalError::Busy);
        }
        let projection = match &cut {
            Some(cut) => self.project_mark(
                &cut.occurrence_id.run,
                cut.occurrence_id.ordinal.0,
                &cut.span,
            )?,
            None => self.project(None)?,
        };
        Ok(PreparedPrefix {
            projection,
            assembly: self.next_assembly.into(),
            cut,
            method: Some(method),
        })
    }
    pub(in crate::flow) fn owns_prefix(&self, prefix: &PreparedPrefix) -> bool {
        Arc::ptr_eq(&self.identity, &prefix.projection.identity)
    }
    /// QueueCut may outlive the turn that supplied this address. A named mark
    /// resolves the same occurrence; an unmarked private prefix keeps its exact
    /// original logical endpoint, never the later current end.
    pub(in crate::flow) fn reprepare_queued(
        &self,
        prefix: &PreparedPrefix,
    ) -> Result<PreparedPrefix> {
        if !self.owns_prefix(prefix) {
            return Err(JournalError::Stale);
        }
        if self.pending.is_some() {
            return Err(JournalError::Busy);
        }
        let projection = match &prefix.cut {
            Some(cut) => self.project_mark(
                &cut.occurrence_id.run,
                cut.occurrence_id.ordinal.0,
                &cut.span,
            )?,
            None => self.project(Some(&prefix.projection.end))?,
        };
        Ok(PreparedPrefix {
            projection,
            assembly: self.next_assembly.into(),
            cut: prefix.cut.clone(),
            method: prefix.method,
        })
    }
    /// Closure may add actual marks/source annotations while the cut drains.
    /// It cannot move this requested logical boundary, change its physical N,
    /// replace a held Message or remove any already covered source interval.
    pub(in crate::flow) fn refresh_drained_prefix(
        &self,
        prefix: &PreparedPrefix,
    ) -> Result<PreparedPrefix> {
        let fresh = self.reprepare_queued(prefix)?;
        let before = &prefix.projection;
        let after = &fresh.projection;
        if before.end != after.end
            || before.n != after.n
            || before.resident_before != after.resident_before
            || prefix.assembly != fresh.assembly
            || before.kept.len() != after.kept.len()
            || before.kept.iter().zip(&after.kept).any(|(a, b)| {
                a.id != b.id
                    || a.message != b.message
                    || a.sources.iter().any(|source| !b.sources.contains(source))
            })
        {
            return Err(JournalError::Stale);
        }
        Ok(fresh)
    }
    pub(in crate::flow) fn complete_generation(
        &mut self,
        proof: &ConfirmedGeneration,
    ) -> Result<()> {
        let generation = proof.generation();
        self.complete(
            &proof.grant.pending,
            ConfirmedAppend {
                assembly: proof.grant.pending.assembly,
                assistant: Message {
                    role: Role::Assistant,
                    content: generation.content.clone(),
                },
                resident: generation.resident,
                capacity: generation.capacity,
            },
        )
    }
    /// This call is private to a checked cut terminal held by the context owner.
    /// Counts are rechecked here before the immutable projected prefix is installed.
    pub(in crate::flow) fn confirm_cut(
        &mut self,
        proof: &crate::flow::context::CheckedCut<'_>,
    ) -> Result<()> {
        let prefix = proof.prefix();
        let before = proof.before();
        let after = proof.after();
        if prefix.assembly.0 != self.next_assembly {
            return Err(JournalError::Stale);
        }
        let next = self
            .next_assembly
            .checked_add(1)
            .ok_or(JournalError::Exhausted)?;
        self.cut(&prefix.projection, ConfirmedCut { before, after })?;
        self.next_assembly = next;
        Ok(())
    }
    pub(in crate::flow) fn placed_marks(
        &mut self,
        proof: &ConfirmedGeneration,
    ) -> Result<Vec<trace::MarkPosition>> {
        let mut candidates = vec![];
        for unit in &proof.grant.units {
            let Some(message) = self.layout.iter().rev().find(|m| {
                m.material
                    .as_ref()
                    .is_some_and(|held| Arc::ptr_eq(held, unit))
                    && m.occurrence
                        .as_ref()
                        .is_some_and(|o| o.assembly_id.0 == proof.grant.pending.assembly)
            }) else {
                return Err(JournalError::InvalidMap);
            };
            let end = ArgumentCursor::Message {
                message_id: message.id,
                block_end: (message.message.content.len() as u64).into(),
                byte_end: None,
            };
            let occurrence = message.occurrence.clone().unwrap();
            for span in unit.spans() {
                candidates.push((span.clone(), end.clone(), occurrence.clone()));
            }
        }
        let mut records = vec![];
        for (span, end, unit) in candidates {
            let ordinal = self.mark(ConfirmedMark {
                span: span.clone(),
                end: end.clone(),
            })?;
            records.push(trace::MarkPosition {
                id: trace::MarkOccurrenceId {
                    run: self.run.clone(),
                    ordinal: ordinal.into(),
                },
                span: crate::flow::mirror::span_ref(&span),
                end: mirror::cursor(&end),
                source: trace::MarkPositionSource::Placed {
                    unit: trace::UnitOccurrenceId {
                        run: unit.run,
                        assembly_id: unit.assembly_id.0.into(),
                        unit_index: unit.unit_index.0.into(),
                    },
                },
            });
        }
        Ok(records)
    }
}

impl Journal {
    pub(in crate::flow) fn annotate_output(
        &mut self,
        proof: &ConfirmedGeneration,
        output: &crate::flow::coordinator::CheckedGeneration,
    ) -> Result<()> {
        if output.original().turn != proof.grant.turn
            || output.original().message
                != (Message {
                    role: Role::Assistant,
                    content: proof.generation().content.clone(),
                })
            || output.source().block_id.run != self.run
        {
            return Err(JournalError::InvalidMap);
        }
        let mut sources = vec![];
        for (index, block) in output.original().message.content.iter().enumerate() {
            if let weaver_traits::ContentBlock::Text { text } = block {
                if text.is_empty() {
                    continue;
                }
                let mut block_id = output.source().block_id.clone();
                block_id.block = (index as u64).into();
                sources.push(PlacedSource {
                    material: MaterialRef::Text {
                        source: weaver_types::state_flow::ContentRef::Event { block_id },
                        span: weaver_types::state_flow::Span {
                            start: 0.into(),
                            end: (text.len() as u64).into(),
                        },
                    },
                    message_id: proof.assistant_id(),
                    block: (index as u64).into(),
                    start: Some(0.into()),
                    end: Some((text.len() as u64).into()),
                });
            }
        }
        self.annotate(ConfirmedSources { sources })?;
        self.emissions
            .push((output.source().clone(), proof.assistant_id()));
        Ok(())
    }
    pub(in crate::flow) fn annotate_pair(
        &mut self,
        started: &weaver_types::state_flow::PairStarted,
        opening: &crate::flow::ledger::CommittedSource,
        original: &crate::flow::coordinator::OriginalRequests,
    ) -> Result<()> {
        if opening.source().run != self.run || started.call_id.run != self.run {
            return Err(JournalError::InvalidMap);
        }
        let generation = original.generation();
        if generation.source().block_id.sequence != started.call_id.sequence {
            return Err(JournalError::InvalidMap);
        }
        let Some((_, message_id)) = self.emissions.iter().find(|(source, _)| {
            source.block_id.run == started.call_id.run
                && source.block_id.sequence == started.call_id.sequence
        }) else {
            return Ok(());
        };
        let Some(message) = self.layout.iter().find(|message| message.id == *message_id) else {
            return Ok(());
        };
        let block = started.call_block;
        let content = message
            .message
            .content
            .get(block.0 as usize)
            .ok_or(JournalError::InvalidMap)?;
        let (start, end) = match content {
            weaver_traits::ContentBlock::ToolCall(_) => (None, None),
            weaver_traits::ContentBlock::Text { .. } => {
                let request = original
                    .requests()
                    .iter()
                    .find(|r| r.opening().call_id == started.call_id)
                    .ok_or(JournalError::InvalidMap)?;
                let statement = &request.opening().statement;
                let check=generation.blocks().iter().find(|b|matches!(&b.request().target,weaver_types::state_flow::Ste100Target::Source(s) if s==&statement.block_id)).ok_or(JournalError::InvalidMap)?;
                let weaver_types::state_flow::Ste100Result::Accepted { bindings } =
                    &check.reply().result
                else {
                    return Err(JournalError::InvalidMap);
                };
                let span = &bindings
                    .get(statement.binding.0 as usize)
                    .ok_or(JournalError::InvalidMap)?
                    .statement_span;
                (Some(span.start), Some(span.end))
            }
            _ => return Err(JournalError::InvalidMap),
        };
        self.annotate(ConfirmedSources {
            sources: vec![PlacedSource {
                material: MaterialRef::Request {
                    call_id: started.call_id.clone(),
                },
                message_id: message.id,
                block,
                start,
                end,
            }],
        })
    }
    pub(in crate::flow) fn native_mark(
        &mut self,
        position: &crate::flow::context::NativePosition,
    ) -> Result<trace::MarkPosition> {
        if !Arc::ptr_eq(position.origin(), &self.origin) || !position.receipt_matches() {
            return Err(JournalError::ForeignMaterial);
        }
        let span = position.span().clone();
        let end = position.end().clone();
        let event = position.event().clone();
        let ordinal = self.mark(ConfirmedMark {
            span: span.clone(),
            end: end.clone(),
        })?;
        Ok(trace::MarkPosition {
            id: trace::MarkOccurrenceId {
                run: self.run.clone(),
                ordinal: ordinal.into(),
            },
            span: crate::flow::mirror::span_ref(&span),
            end: mirror::cursor(&end),
            source: trace::MarkPositionSource::Native {
                event: crate::flow::mirror::source(&event),
            },
        })
    }
    pub(in crate::flow) fn held_units(&self) -> impl Iterator<Item = &Arc<SelectedUnit>> {
        self.layout
            .iter()
            .filter_map(|message| message.material.as_ref())
    }
    pub(in crate::flow) fn full_text_end(
        &self,
        source: &weaver_types::state_flow::SourceBlockId,
        text: &str,
    ) -> Option<ArgumentCursor> {
        self.layout.iter().rev().find_map(|message| message.sources.iter().find_map(|placed| {
            if !matches!(&placed.material,MaterialRef::Text{source:weaver_types::state_flow::ContentRef::Event{block_id},span} if block_id==source&&span.start.0==0&&span.end.0==text.len() as u64){return None}
            let weaver_traits::ContentBlock::Text{text:resident}=message.message.content.get(placed.block.0 as usize)? else{return None};
            let start=placed.start?.0 as usize;let end=placed.end?.0 as usize;
            if resident.get(start..end)!=Some(text){return None}
            Some(ArgumentCursor::Message{message_id:message.id,block_end:(placed.block.0+1).into(),byte_end:if end==resident.len(){None}else{Some((end as u64).into())}})
        }))
    }
}

impl Journal {
    pub(in crate::flow) fn validate_selection<'a>(
        &self,
        prefix: &PrefixProjection,
        units: impl IntoIterator<Item = &'a SelectedUnit>,
    ) -> Result<()> {
        self.check_projection(prefix)?;
        for unit in units {
            if !Arc::ptr_eq(&self.origin, unit.origin()) {
                return Err(JournalError::ForeignMaterial);
            }
            layout::validate_coverage(unit.source_map(), unit.sources())?;
            for message in unit.messages() {
                validate_delta_message(message)?;
            }
            if let Some(pair) = unit.completed_pair() {
                if !prefix.coverage.contains(&MaterialRef::Request {
                    call_id: pair.started().call_id.clone(),
                }) {
                    return Err(JournalError::InvalidPair);
                }
                let crate::flow::ledger::LedgerFile::Statement(file) = pair.statement().file()
                else {
                    return Err(JournalError::InvalidPair);
                };
                let reference = &pair.started().statement;
                let binding = file
                    .bindings
                    .get(reference.binding.0 as usize)
                    .ok_or(JournalError::InvalidPair)?;
                if !prefix.coverage.iter().any(|m|matches!(m,MaterialRef::Text{source:weaver_types::state_flow::ContentRef::Event{block_id},span} if block_id==&reference.block_id&&span.start<=binding.statement_span.start&&span.end>=binding.statement_span.end)){return Err(JournalError::InvalidPair)}
            }
        }
        Ok(())
    }
}
