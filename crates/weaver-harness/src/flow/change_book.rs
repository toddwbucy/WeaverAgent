//! Workflow 5.01, 5.09, 5.12 and 5.13, OUTBOUND-CHANGE chain accounting.
//! This private component holds text and recorder progress, not operation grants.
//! The recording context must verify lookup, artifact and offer authority first.
//! It must drive callbacks from the matching author and its actual Recorded result.
//! Neither public record data nor a text snapshot is an EffectiveEmission proof.

use std::sync::Arc;

use weaver_trace::Sequence;
use weaver_traits::{ContentBlock, Role};
use weaver_types::state_flow::{DecimalU64, EmissionChangeRecord, SourceBlockId, SourcePosition};

use crate::flow::coordinator::OutputOriginal;

/// One proposal identity, private to its owner and never reconstructed from IDs.
#[derive(Debug)]
#[must_use]
pub(super) struct ChangeReservation(Arc<()>);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ChangeError {
    Stopped,
    WrongSource,
    StaleSource,
    PendingChange,
    InvalidText,
    UnknownReservation,
    NotAdmitted,
    AlreadyAdmitted,
    InvalidSequence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Cancellation {
    Released,
    AwaitingRecord,
}

/// These are accounting outcomes, with no permission to send or place text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RecordedChange {
    Applied,
    AccountedAfterCancellation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Reserved,
    Admitted,
    Cancelled,
}

struct Pending {
    identity: Arc<()>,
    record: EmissionChangeRecord,
    next_text: String,
    phase: Phase,
}

struct TextVersion {
    text: String,
    head: Option<DecimalU64>,
    pending: Option<Pending>,
}

/// One original generation, with original content indices and finish retained.
pub(super) struct ChangeBook {
    original: OutputOriginal,
    source: SourcePosition,
    texts: Vec<Option<TextVersion>>,
    last_recorded: Sequence,
    stopped: bool,
}

impl ChangeBook {
    /// Retain the original on invalid attribution. Construction grants no operation.
    pub(super) fn new(
        original: OutputOriginal,
        source: SourcePosition,
    ) -> Result<Self, Box<OutputOriginal>> {
        if source.block_id.run.is_empty()
            || source.block_id.block.0 != 0
            || original.turn.0.is_empty()
            || source.turn.as_deref() != Some(original.turn.0.as_str())
            || original.message.role != Role::Assistant
            || original.message.content.iter().any(|block| {
                !matches!(
                    block,
                    ContentBlock::Text { .. }
                        | ContentBlock::ToolCall(_)
                        | ContentBlock::UnrecoveredCall { .. }
                )
            })
        {
            return Err(Box::new(original));
        }
        let texts = original
            .message
            .content
            .iter()
            .map(|block| match block {
                ContentBlock::Text { text } => Some(TextVersion {
                    text: text.clone(),
                    head: None,
                    pending: None,
                }),
                _ => None,
            })
            .collect();
        Ok(Self {
            last_recorded: Sequence(source.block_id.sequence.0),
            source,
            original,
            texts,
            stopped: false,
        })
    }

    pub(super) fn original(&self) -> (&OutputOriginal, &SourcePosition) {
        (&self.original, &self.source)
    }

    fn index(&self, source: &SourceBlockId) -> Result<usize, ChangeError> {
        if source.run != self.source.block_id.run
            || source.sequence != self.source.block_id.sequence
        {
            return Err(ChangeError::WrongSource);
        }
        let index = usize::try_from(source.block.0).map_err(|_| ChangeError::WrongSource)?;
        self.texts
            .get(index)
            .and_then(Option::as_ref)
            .ok_or(ChangeError::WrongSource)?;
        Ok(index)
    }

    pub(super) fn current(
        &self,
        source: &SourceBlockId,
    ) -> Result<(&str, Option<DecimalU64>), ChangeError> {
        let version = self.texts[self.index(source)?].as_ref().unwrap();
        Ok((&version.text, version.head))
    }

    /// Data only. The enclosing owner still owes its recorded release proof.
    pub(super) fn text_snapshot(&self) -> String {
        self.texts
            .iter()
            .flatten()
            .map(|v| v.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Reserve after authority checks. This checks the chain and exact bytes only.
    pub(super) fn reserve(
        &mut self,
        record: EmissionChangeRecord,
        reemission_text: Option<&str>,
    ) -> Result<ChangeReservation, ChangeError> {
        if self.stopped {
            return Err(ChangeError::Stopped);
        }
        let index = self.index(&record.matched.source)?;
        let version = self.texts[index].as_mut().unwrap();
        if record.matched.previous_change != version.head {
            return Err(ChangeError::StaleSource);
        }
        if version.pending.is_some() {
            return Err(ChangeError::PendingChange);
        }
        record
            .validate_texts(&version.text, reemission_text)
            .map_err(|_| ChangeError::InvalidText)?;
        let start = usize::try_from(record.span.start.0).map_err(|_| ChangeError::InvalidText)?;
        let end = usize::try_from(record.span.end.0).map_err(|_| ChangeError::InvalidText)?;
        let mut next_text = version.text.clone();
        next_text.replace_range(start..end, &record.replacement);
        let identity = Arc::new(());
        version.pending = Some(Pending {
            identity: identity.clone(),
            record,
            next_text,
            phase: Phase::Reserved,
        });
        Ok(ChangeReservation(identity))
    }

    fn reservation_index(&self, reservation: &ChangeReservation) -> Result<usize, ChangeError> {
        self.texts
            .iter()
            .position(|version| {
                version
                    .as_ref()
                    .and_then(|v| v.pending.as_ref())
                    .is_some_and(|p| Arc::ptr_eq(&p.identity, &reservation.0))
            })
            .ok_or(ChangeError::UnknownReservation)
    }

    /// Writer pressure retains this exact record and leaves current Y unchanged.
    pub(super) fn pending_record(
        &self,
        reservation: &ChangeReservation,
    ) -> Result<&EmissionChangeRecord, ChangeError> {
        let index = self.reservation_index(reservation)?;
        Ok(&self.texts[index]
            .as_ref()
            .unwrap()
            .pending
            .as_ref()
            .unwrap()
            .record)
    }

    pub(super) fn admitted(&mut self, reservation: &ChangeReservation) -> Result<(), ChangeError> {
        let index = self.reservation_index(reservation)?;
        let pending = self.texts[index]
            .as_mut()
            .unwrap()
            .pending
            .as_mut()
            .unwrap();
        if pending.phase != Phase::Reserved {
            return Err(ChangeError::AlreadyAdmitted);
        }
        pending.phase = Phase::Admitted;
        Ok(())
    }

    /// Call only from this proposal's actual Recorded callback, once per record.
    pub(super) fn recorded(
        &mut self,
        reservation: &ChangeReservation,
        sequence: Sequence,
    ) -> Result<RecordedChange, ChangeError> {
        let index = self.reservation_index(reservation)?;
        let version = self.texts[index].as_mut().unwrap();
        if version.pending.as_ref().unwrap().phase == Phase::Reserved {
            return Err(ChangeError::NotAdmitted);
        }
        if sequence.0 <= self.last_recorded.0 {
            return Err(ChangeError::InvalidSequence);
        }
        let pending = version.pending.take().unwrap();
        self.last_recorded = sequence;
        if self.stopped || pending.phase == Phase::Cancelled {
            return Ok(RecordedChange::AccountedAfterCancellation);
        }
        version.text = pending.next_text;
        version.head = Some(sequence.0.into());
        Ok(RecordedChange::Applied)
    }

    /// Cancelling admitted work ends this answer's continuation, as in teardown.
    pub(super) fn cancel(
        &mut self,
        reservation: &ChangeReservation,
    ) -> Result<Cancellation, ChangeError> {
        let index = self.reservation_index(reservation)?;
        let version = self.texts[index].as_mut().unwrap();
        match version.pending.as_ref().unwrap().phase {
            Phase::Reserved => {
                version.pending = None;
                Ok(Cancellation::Released)
            }
            Phase::Admitted | Phase::Cancelled => {
                self.cancel_all();
                Ok(Cancellation::AwaitingRecord)
            }
        }
    }

    /// Stop new proposals and retain admitted records for teardown accounting.
    pub(super) fn cancel_all(&mut self) {
        self.stopped = true;
        for version in self.texts.iter_mut().flatten() {
            match version.pending.as_mut() {
                Some(pending) if pending.phase == Phase::Reserved => version.pending = None,
                Some(pending) => pending.phase = Phase::Cancelled,
                None => {}
            }
        }
    }
}

#[cfg(test)]
#[path = "change_book_tests.rs"]
mod tests;
