//! Private journal arithmetic, without a production initializer or live grants.
//! Boot/Open, terminal correlation and recorded mark/source authority are owed.
//! Only tests can currently construct the journal or the confirmation inputs.

use std::sync::Arc;

use weaver_traits::{Message, Role};
use weaver_types::state_flow::{
    ArgumentCursor, CallId, DecimalU64, MaterialRef, SpanRef, UnitOccurrenceId,
};

use super::material::SelectedUnit;

mod layout;
use layout::{PlacedSource, RetainedMessage};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum JournalError {
    Busy,
    Stale,
    InvalidCursor,
    InvalidMap,
    InvalidPair,
    InvalidFacts,
    ForeignMaterial,
    Exhausted,
}
type Result<T> = std::result::Result<T, JournalError>;

#[derive(Debug)]
pub(super) struct Journal {
    identity: Arc<()>,
    origin: Arc<()>,
    run: String,
    revision: u64,
    next_message: u64,
    next_assembly: u64,
    next_mark: u64,
    prefix: u64,
    resident: u64,
    capacity: u64,
    layout: Vec<RetainedMessage>,
    boundaries: Vec<Boundary>,
    pairs: Vec<PairLink>,
    marks: Vec<Mark>,
    pending: Option<u64>,
    rebuild_pending: bool,
}

#[derive(Debug, Clone)]
struct Boundary {
    end: ArgumentCursor,
    resident: u64,
}
#[derive(Debug, Clone)]
struct PairLink {
    call: CallId,
    request: PlacedSource,
    returned: PlacedSource,
}
#[derive(Debug, Clone)]
struct Mark {
    ordinal: u64,
    span: SpanRef,
    end: ArgumentCursor,
}

/// A projection is arithmetic evidence only, never a PreparedPrefix or a grant.
#[derive(Debug)]
pub(super) struct PrefixProjection {
    identity: Arc<()>,
    revision: u64,
    end: ArgumentCursor,
    n: u64,
    resident_before: u64,
    kept: Vec<RetainedMessage>,
    rebuild: Vec<RetainedMessage>,
    coverage: Vec<MaterialRef>,
}

#[derive(Debug)]
pub(super) struct PendingAppend {
    identity: Arc<()>,
    revision: u64,
    assembly: u64,
    assistant_id: DecimalU64,
    new_messages: Vec<RetainedMessage>,
    new_pairs: Vec<PairLink>,
    prefix: PrefixProjection,
}

// No production constructors. The later owner must bind the exact terminal to
// its reservation, request, trace and admitted facts before making these inputs.
struct ConfirmedAppend {
    assembly: u64,
    assistant: Message,
    resident: u64,
    capacity: u64,
}
struct ConfirmedCut {
    before: u64,
    after: u64,
}
struct ConfirmedMark {
    span: SpanRef,
    end: ArgumentCursor,
}
struct ConfirmedSources {
    sources: Vec<PlacedSource>,
}

impl Journal {
    fn next_revision(&self) -> Result<u64> {
        self.revision.checked_add(1).ok_or(JournalError::Exhausted)
    }

    fn check_projection(&self, prefix: &PrefixProjection) -> Result<()> {
        if !Arc::ptr_eq(&self.identity, &prefix.identity)
            || prefix.revision != self.revision
            || prefix.resident_before != self.resident
        {
            return Err(JournalError::Stale);
        }
        Ok(())
    }

    /// Cursor input is internal arithmetic, not a public cut address. The future
    /// mark owner must resolve a current CutReference before invoking this core.
    fn project(&self, end: Option<&ArgumentCursor>) -> Result<PrefixProjection> {
        let end = end.cloned().unwrap_or_else(|| layout::end(&self.layout));
        let position = layout::position(&self.layout, &end)?;
        for pair in &self.pairs {
            let request = layout::location_kept(&self.layout, &pair.request, position)?;
            let returned = layout::location_kept(&self.layout, &pair.returned, position)?;
            if request != returned {
                return Err(JournalError::InvalidPair);
            }
        }
        let kept = layout::prefix(&self.layout, &end)?;
        let (n, rebuild_start) = if end == layout::end(&self.layout) && !self.rebuild_pending {
            (self.resident, kept.len())
        } else {
            let boundary = self.boundaries.iter().rev().find(|boundary| {
                layout::position(&self.layout, &boundary.end).is_ok_and(|end| end <= position)
            });
            match boundary {
                Some(boundary) => (
                    boundary.resident,
                    layout::position(&self.layout, &boundary.end)?.message,
                ),
                None => (self.prefix, 0),
            }
        };
        if self.prefix > n || n > self.resident || self.resident > self.capacity {
            return Err(JournalError::InvalidFacts);
        }
        let coverage = layout::coverage(&kept);
        let rebuild = kept[rebuild_start..].to_vec();
        Ok(PrefixProjection {
            identity: self.identity.clone(),
            revision: self.revision,
            end,
            n,
            resident_before: self.resident,
            kept,
            rebuild,
            coverage,
        })
    }

    /// Burn prospective IDs once. No message, source or boundary becomes active.
    fn reserve(
        &mut self,
        prefix: PrefixProjection,
        units: &[Arc<SelectedUnit>],
    ) -> Result<PendingAppend> {
        if self.pending.is_some() {
            return Err(JournalError::Busy);
        }
        self.check_projection(&prefix)?;
        if prefix.n != self.resident
            || layout::compare(&self.layout, &prefix.end, &layout::end(&self.layout))?
                != std::cmp::Ordering::Equal
        {
            return Err(JournalError::InvalidFacts);
        }
        if units
            .iter()
            .any(|unit| !Arc::ptr_eq(&self.origin, unit.origin()))
        {
            return Err(JournalError::ForeignMaterial);
        }
        let assembly = self.next_assembly;
        let next_assembly = assembly.checked_add(1).ok_or(JournalError::Exhausted)?;
        let mut next_message = self.next_message;
        let mut new_messages = vec![];
        let mut new_pairs = vec![];
        for (index, unit) in units.iter().enumerate() {
            let occurrence = UnitOccurrenceId {
                run: self.run.clone(),
                assembly_id: assembly.into(),
                unit_index: u64::try_from(index)
                    .map_err(|_| JournalError::Exhausted)?
                    .into(),
            };
            let start = new_messages.len();
            for message in unit.messages() {
                let id = next_message;
                next_message = id.checked_add(1).ok_or(JournalError::Exhausted)?;
                if message.role == Role::System {
                    return Err(JournalError::InvalidMap);
                }
                new_messages.push(RetainedMessage {
                    id: id.into(),
                    message: message.clone(),
                    sources: vec![],
                    material: Some(unit.clone()),
                    occurrence: Some(occurrence.clone()),
                    step_queries: vec![],
                });
            }
            if start == new_messages.len() {
                return Err(JournalError::InvalidMap);
            }
            for mapping in unit.source_map() {
                let index =
                    usize::try_from(mapping.message.0).map_err(|_| JournalError::InvalidMap)?;
                let target = new_messages
                    .get_mut(start.checked_add(index).ok_or(JournalError::Exhausted)?)
                    .ok_or(JournalError::InvalidMap)?;
                let source = PlacedSource {
                    material: mapping.material.clone(),
                    message_id: target.id,
                    block: mapping.block,
                    start: mapping.start,
                    end: mapping.end,
                };
                layout::validate_source(target, &source)?;
                target.sources.push(source);
            }
            for query in unit.step_queries() {
                let index =
                    usize::try_from(query.message.0).map_err(|_| JournalError::InvalidMap)?;
                let target = new_messages
                    .get_mut(start.checked_add(index).ok_or(JournalError::Exhausted)?)
                    .ok_or(JournalError::InvalidMap)?;
                target.step_queries.push(query.reference.clone());
            }
            if layout::coverage(&new_messages[start..]) != unit.sources() {
                return Err(JournalError::InvalidMap);
            }
            let sources: Vec<_> = new_messages[start..]
                .iter()
                .flat_map(|m| &m.sources)
                .collect();
            for request in &sources {
                if let MaterialRef::Request { call_id } = &request.material {
                    let mut returns = sources.iter().filter(|source|
                        matches!(&source.material, MaterialRef::Returned { call_id: found } if found == call_id));
                    let returned = returns.next().ok_or(JournalError::InvalidPair)?;
                    if returns.next().is_some() {
                        return Err(JournalError::InvalidPair);
                    }
                    new_pairs.push(PairLink {
                        call: call_id.clone(),
                        request: (*request).clone(),
                        returned: (**returned).clone(),
                    });
                }
            }
            for returned in sources {
                if let MaterialRef::Returned { call_id } = &returned.material
                    && !new_pairs
                        .iter()
                        .any(|pair| pair.call == *call_id && pair.returned == *returned)
                {
                    // Fresh result-only authority is deliberately not implemented.
                    return Err(JournalError::InvalidPair);
                }
            }
        }
        let assistant_id = next_message.into();
        next_message = next_message.checked_add(1).ok_or(JournalError::Exhausted)?;
        self.next_message = next_message;
        self.next_assembly = next_assembly;
        self.pending = Some(assembly);
        Ok(PendingAppend {
            identity: self.identity.clone(),
            revision: self.revision,
            assembly,
            assistant_id,
            new_messages,
            new_pairs,
            prefix,
        })
    }

    fn check_pending(&self, pending: &PendingAppend) -> Result<()> {
        if !Arc::ptr_eq(&self.identity, &pending.identity)
            || self.pending != Some(pending.assembly)
            || pending.revision != self.revision
        {
            return Err(JournalError::Stale);
        }
        self.check_projection(&pending.prefix)
    }

    fn cancel(&mut self, pending: &PendingAppend) -> Result<()> {
        self.check_pending(pending)?;
        self.pending = None;
        Ok(())
    }

    fn complete(&mut self, pending: &PendingAppend, confirmed: ConfirmedAppend) -> Result<()> {
        self.check_pending(pending)?;
        if confirmed.assembly != pending.assembly
            || confirmed.assistant.role != Role::Assistant
            || confirmed.assistant.content.iter().any(|block| {
                !matches!(
                    block,
                    weaver_traits::ContentBlock::Text { .. }
                        | weaver_traits::ContentBlock::ToolCall(_)
                        | weaver_traits::ContentBlock::UnrecoveredCall { .. }
                )
            })
            || confirmed.capacity != self.capacity
            || confirmed.resident <= pending.prefix.n
            || confirmed.resident > self.capacity
        {
            return Err(JournalError::InvalidFacts);
        }
        let revision = self.next_revision()?;
        let mut layout = pending.prefix.kept.clone();
        layout.extend(pending.new_messages.iter().cloned());
        layout.push(RetainedMessage {
            id: pending.assistant_id,
            message: confirmed.assistant,
            sources: vec![],
            material: None,
            occurrence: None,
            step_queries: vec![],
        });
        let end = layout::end(&layout);
        let old_end = layout::position(&self.layout, &pending.prefix.end)?;
        self.boundaries
            .retain(|boundary| boundary.resident <= pending.prefix.n);
        self.pairs
            .retain(|pair| layout::location_kept(&self.layout, &pair.request, old_end) == Ok(true));
        self.marks
            .retain(|mark| layout::position(&self.layout, &mark.end).is_ok_and(|p| p <= old_end));
        self.layout = layout;
        self.pairs.extend(pending.new_pairs.iter().cloned());
        self.boundaries.push(Boundary {
            end,
            resident: confirmed.resident,
        });
        self.resident = confirmed.resident;
        self.rebuild_pending = false;
        self.pending = None;
        self.revision = revision;
        Ok(())
    }

    fn cut(&mut self, prefix: &PrefixProjection, confirmed: ConfirmedCut) -> Result<()> {
        if self.pending.is_some() {
            return Err(JournalError::Busy);
        }
        self.check_projection(prefix)?;
        if confirmed.before != self.resident || confirmed.after != prefix.n {
            return Err(JournalError::InvalidFacts);
        }
        let revision = self.next_revision()?;
        let end = layout::position(&self.layout, &prefix.end)?;
        self.boundaries
            .retain(|boundary| boundary.resident <= prefix.n);
        self.pairs
            .retain(|pair| layout::location_kept(&self.layout, &pair.request, end) == Ok(true));
        self.marks
            .retain(|mark| layout::position(&self.layout, &mark.end).is_ok_and(|p| p <= end));
        self.layout = prefix.kept.clone();
        self.resident = prefix.n;
        self.rebuild_pending = !prefix.rebuild.is_empty();
        self.revision = revision;
        Ok(())
    }

    fn project_mark(&self, run: &str, ordinal: u64, span: &SpanRef) -> Result<PrefixProjection> {
        if run != self.run {
            return Err(JournalError::InvalidCursor);
        }
        let mark = self
            .marks
            .iter()
            .find(|mark| mark.ordinal == ordinal && mark.span == *span)
            .ok_or(JournalError::InvalidCursor)?;
        self.project(Some(&mark.end))
    }

    fn mark(&mut self, confirmed: ConfirmedMark) -> Result<u64> {
        if self.pending.is_some() {
            return Err(JournalError::Busy);
        }
        layout::position(&self.layout, &confirmed.end)?;
        let revision = self.next_revision()?;
        let ordinal = self.next_mark;
        let next = ordinal.checked_add(1).ok_or(JournalError::Exhausted)?;
        self.marks.push(Mark {
            ordinal,
            span: confirmed.span,
            end: confirmed.end,
        });
        self.next_mark = next;
        self.revision = revision;
        Ok(ordinal)
    }

    fn annotate(&mut self, confirmed: ConfirmedSources) -> Result<()> {
        if self.pending.is_some() {
            return Err(JournalError::Busy);
        }
        let revision = self.next_revision()?;
        for source in &confirmed.sources {
            if matches!(source.material, MaterialRef::Returned { .. }) {
                return Err(JournalError::InvalidPair);
            }
            let message = self
                .layout
                .iter()
                .find(|message| message.id == source.message_id)
                .ok_or(JournalError::InvalidMap)?;
            layout::validate_source(message, source)?;
        }
        for source in confirmed.sources {
            let message = self
                .layout
                .iter_mut()
                .find(|message| message.id == source.message_id)
                .expect("validated source location");
            if !message.sources.contains(&source) {
                message.sources.push(source);
            }
        }
        self.revision = revision;
        Ok(())
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(super) fn exercise_material(units: Vec<SelectedUnit>, foreign: SelectedUnit) {
    tests::exercise_material(units, foreign);
}
