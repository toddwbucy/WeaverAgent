//! One complete model-message check, independent of selected filing progress.
//!
//! Only the coordinator constructs recorded block evidence. The complete grant
//! is input to repair, never permission to answer, dispatch, or place content.

use weaver_trace::{Sequence, TurnRef};
use weaver_traits::{ContentBlock, Message, Role};
use weaver_types::Finish;
use weaver_types::state_flow::{
    SchemeDeclaration, SourceBlockId, SourcePosition, Ste100Reply, Ste100Request, Ste100Result,
    Ste100Target,
};

use crate::flow::ledger::SourceLease;

#[derive(Debug)]
pub(in crate::flow) struct OutputOriginal {
    pub message: Message,
    pub finish: Finish,
    pub turn: TurnRef,
}

#[derive(Debug)]
pub(in crate::flow) struct RecordedOutputBlock {
    request: Ste100Request,
    reply: Ste100Reply,
    result_source: SourcePosition,
    recorded_ns: u64,
    selected: bool,
}

impl RecordedOutputBlock {
    pub(in crate::flow) fn request(&self) -> &Ste100Request {
        &self.request
    }
    pub(in crate::flow) fn reply(&self) -> &Ste100Reply {
        &self.reply
    }
    pub(in crate::flow) fn result_source(&self) -> &SourcePosition {
        &self.result_source
    }
    pub(in crate::flow) fn recorded_ns(&self) -> u64 {
        self.recorded_ns
    }
    pub(in crate::flow) fn selected(&self) -> bool {
        self.selected
    }
}

#[derive(Debug)]
pub(in crate::flow) struct CheckedGeneration {
    scheme: SchemeDeclaration,
    original: OutputOriginal,
    source: SourcePosition,
    blocks: Vec<RecordedOutputBlock>,
    recorded_ns: u64,
    receipt_lease: Option<SourceLease>,
}

impl CheckedGeneration {
    pub(in crate::flow) fn scheme(&self) -> &SchemeDeclaration {
        &self.scheme
    }

    /// Real run transactions attach a lease before returning this check proof.
    /// A lease retains facts only. It grants no pair or dispatch authority.
    pub(in crate::flow) fn receipt_lease(&self) -> Option<&SourceLease> {
        self.receipt_lease.as_ref()
    }

    pub(in crate::flow) fn take_receipt_lease(&mut self) -> Option<SourceLease> {
        self.receipt_lease.take()
    }

    pub(super) fn attach_receipt_lease(&mut self, lease: SourceLease) {
        assert!(self.receipt_lease.is_none());
        self.receipt_lease = Some(lease);
    }

    pub(in crate::flow) fn original(&self) -> &OutputOriginal {
        &self.original
    }
    pub(in crate::flow) fn source(&self) -> &SourcePosition {
        &self.source
    }
    pub(in crate::flow) fn blocks(&self) -> &[RecordedOutputBlock] {
        &self.blocks
    }
    pub(in crate::flow) fn recorded_ns(&self) -> u64 {
        self.recorded_ns
    }
}

/// Cancellation retains evidence without carrying a complete-check capability.
#[derive(Debug)]
pub(in crate::flow) struct OutputCancellation {
    pub original: OutputOriginal,
    pub source: Option<SourcePosition>,
    pub recorded: Vec<RecordedOutputBlock>,
}

#[derive(Debug)]
struct Slot {
    index: u64,
    selected: bool,
    recorded: Option<RecordedOutputBlock>,
}

#[derive(Debug)]
pub(in crate::flow) struct OutputChecks {
    original: OutputOriginal,
    run: String,
    scheme: SchemeDeclaration,
    source: Option<SourcePosition>,
    slots: Vec<Slot>,
    in_flight: Option<Ste100Request>,
    author_recorded_ns: Option<u64>,
    cancelled: bool,
    frozen: bool,
}

impl OutputChecks {
    #[allow(clippy::result_large_err)]
    pub(super) fn new(
        run: &str,
        scheme: &SchemeDeclaration,
        original: OutputOriginal,
    ) -> Result<Self, (OutputOriginal, &'static str)> {
        if run.is_empty() || original.turn.0.is_empty() || original.message.role != Role::Assistant
        {
            return Err((original, "output requires an attributed assistant message"));
        }
        if original.message.content.iter().any(|block| {
            !matches!(
                block,
                ContentBlock::Text { .. }
                    | ContentBlock::ToolCall(_)
                    | ContentBlock::UnrecoveredCall { .. }
            )
        }) {
            return Err((original, "output contains an unlicensed assistant block"));
        }
        let mut slots = Vec::new();
        if slots
            .try_reserve_exact(original.message.content.len())
            .is_err()
        {
            return Err((original, "output check reservation failed"));
        }
        for (index, block) in original.message.content.iter().enumerate() {
            if matches!(block, ContentBlock::Text { .. }) {
                let Ok(index) = u64::try_from(index) else {
                    return Err((original, "output content index exceeds source counter"));
                };
                slots.push(Slot {
                    index,
                    selected: false,
                    recorded: None,
                });
            }
        }
        Ok(Self {
            original,
            run: run.to_owned(),
            scheme: scheme.clone(),
            source: None,
            slots,
            in_flight: None,
            author_recorded_ns: None,
            cancelled: false,
            frozen: false,
        })
    }

    pub(in crate::flow) fn original(&self) -> &OutputOriginal {
        &self.original
    }
    pub(in crate::flow) fn source(&self) -> Option<&SourcePosition> {
        self.source.as_ref()
    }
    pub(in crate::flow) fn is_bound(&self) -> bool {
        self.source.is_some()
    }
    pub(in crate::flow) fn is_complete(&self) -> bool {
        self.is_bound()
            && !self.cancelled
            && !self.frozen
            && self.in_flight.is_none()
            && self.slots.iter().all(|slot| slot.recorded.is_some())
    }
    pub(in crate::flow) fn is_cancelled(&self) -> bool {
        self.cancelled
    }
    pub(in crate::flow) fn is_settled(&self) -> bool {
        self.is_complete() || (self.cancelled && self.in_flight.is_none())
    }
    pub(in crate::flow) fn recorded_ns(&self) -> Option<u64> {
        self.is_complete().then(|| {
            self.slots
                .iter()
                .filter_map(|slot| slot.recorded.as_ref().map(|record| record.recorded_ns))
                .fold(self.author_recorded_ns.unwrap(), u64::max)
        })
    }
    pub(in crate::flow) fn pending(&self) -> impl Iterator<Item = SourceBlockId> + '_ {
        self.slots.iter().filter_map(|slot| {
            (slot.recorded.is_none())
                .then(|| self.block_source(slot.index))
                .flatten()
        })
    }

    pub(super) fn bind(
        &mut self,
        sequence: Sequence,
        recorded_ns: u64,
        selected: &[SourceBlockId],
    ) -> Result<(), &'static str> {
        if self.is_bound() || self.frozen || sequence.0 == 0 {
            return Err("output source cannot be rebound");
        }
        let mut previous = None;
        for source in selected {
            if source.run != self.run
                || source.sequence.0 != sequence.0
                || previous.is_some_and(|index| source.block.0 <= index)
                || !self.slots.iter().any(|slot| slot.index == source.block.0)
            {
                return Err("selected output source differs from original Text indices");
            }
            previous = Some(source.block.0);
        }
        for slot in &mut self.slots {
            slot.selected = selected.iter().any(|source| source.block.0 == slot.index);
        }
        self.source = Some(SourcePosition {
            block_id: SourceBlockId {
                run: self.run.clone(),
                sequence: sequence.0.into(),
                block: 0.into(),
            },
            turn: Some(self.original.turn.0.clone()),
        });
        self.author_recorded_ns = Some(recorded_ns);
        Ok(())
    }

    fn block_source(&self, index: u64) -> Option<SourceBlockId> {
        self.source.as_ref().map(|source| SourceBlockId {
            block: index.into(),
            ..source.block_id.clone()
        })
    }

    pub(super) fn next_unselected(&self) -> Option<(SourceBlockId, String, TurnRef)> {
        if !self.is_bound() || self.cancelled || self.frozen || self.in_flight.is_some() {
            return None;
        }
        let slot = self
            .slots
            .iter()
            .find(|slot| !slot.selected && slot.recorded.is_none())?;
        let ContentBlock::Text { text } = &self.original.message.content[slot.index as usize]
        else {
            unreachable!("slots enumerate original Text blocks");
        };
        Some((
            self.block_source(slot.index).unwrap(),
            text.clone(),
            self.original.turn.clone(),
        ))
    }

    pub(super) fn submitted(&mut self, request: Ste100Request) -> Result<(), &'static str> {
        let Some((source, text, _)) = self.next_unselected() else {
            return Err("output has no unsent unselected check");
        };
        if request.target != Ste100Target::Source(source)
            || request.text != text
            || request.scheme_id != self.scheme.scheme_id
        {
            return Err("submitted output check differs from reserved source");
        }
        self.in_flight = Some(request);
        Ok(())
    }

    pub(super) fn observes(&self, request: &Ste100Request) -> bool {
        let Ste100Target::Source(source) = &request.target else {
            return false;
        };
        self.source.as_ref().is_some_and(|original| {
            original.block_id.run == source.run && original.block_id.sequence == source.sequence
        })
    }

    pub(super) fn owns_unselected(&self, request: &Ste100Request) -> bool {
        self.in_flight.as_ref() == Some(request)
    }

    pub(super) fn record(
        &mut self,
        request: Ste100Request,
        reply: Ste100Reply,
        result_source: SourcePosition,
        recorded_ns: u64,
        selected: bool,
    ) -> Result<(), &'static str> {
        if self.frozen || !self.observes(&request) {
            return Err("recorded output result has no live source owner");
        }
        let Ste100Target::Source(source) = &request.target else {
            return Err("recorded output check has no source target");
        };
        let Some(slot) = self.slots.iter().find(|slot| slot.index == source.block.0) else {
            return Err("recorded output result does not name an original Text");
        };
        let ContentBlock::Text { text } = &self.original.message.content[slot.index as usize]
        else {
            unreachable!("slots enumerate original Text blocks");
        };
        if slot.recorded.is_some()
            || slot.selected != selected
            || request.text != *text
            || request.scheme_id != self.scheme.scheme_id
            || result_source.block_id.run != self.run
            || result_source.block_id.sequence <= source.sequence
            || result_source.block_id.block.0 != 0
            || result_source.turn.as_deref() != Some(self.original.turn.0.as_str())
            || recorded_ns < self.author_recorded_ns.unwrap()
            || (!selected && !self.owns_unselected(&request))
            || reply.validate_for(&request, &self.scheme.verbs).is_err()
            || matches!(reply.result, Ste100Result::ComponentFailed { .. })
        {
            return Err("recorded output check differs from its reserved exact source");
        }
        let index = slot.index;
        if !selected {
            self.in_flight = None;
        }
        self.slots
            .iter_mut()
            .find(|slot| slot.index == index)
            .unwrap()
            .recorded = Some(RecordedOutputBlock {
            request,
            reply,
            result_source,
            recorded_ns,
            selected,
        });
        Ok(())
    }

    pub(super) fn cancel(&mut self) {
        self.cancelled = true;
    }
    pub(super) fn freeze(&mut self) {
        self.frozen = true;
    }
    #[allow(clippy::result_large_err)]
    pub(super) fn complete(self) -> Result<CheckedGeneration, Self> {
        let Some(recorded_ns) = self.recorded_ns() else {
            return Err(self);
        };
        Ok(CheckedGeneration {
            scheme: self.scheme,
            original: self.original,
            source: self.source.unwrap(),
            blocks: self
                .slots
                .into_iter()
                .map(|slot| slot.recorded.unwrap())
                .collect(),
            recorded_ns,
            receipt_lease: None,
        })
    }

    #[allow(clippy::result_large_err)]
    pub(super) fn cancellation(self) -> Result<OutputCancellation, Self> {
        if !self.cancelled || !self.is_settled() {
            return Err(self);
        }
        Ok(OutputCancellation {
            original: self.original,
            source: self.source,
            recorded: self
                .slots
                .into_iter()
                .filter_map(|slot| slot.recorded)
                .collect(),
        })
    }
}

#[cfg(test)]
#[path = "output_tests.rs"]
mod tests;
