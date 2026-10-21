//! A single consumer's custody of exact recorded source dispositions.
//!
//! This is a private child of ledger. Only its parent can construct committed
//! evidence, after the real state and terminal recording barriers. It grants no
//! tool dispatch, pair completion, placement, or store-writing authority.

use weaver_types::state_flow::{Common, SourceBlockId, StateFrame, StatementFile, Ste100Reply};

use super::{LedgerError, LedgerFile, TerminalCandidate};

/// A committed fact, not permission to perform a dependent operation. The exact
/// immutable filing and correlated answer move out of the ledger together.
#[derive(Debug)]
pub(in crate::flow) struct CommittedSource {
    file: LedgerFile,
    receipt: StateFrame,
    terminal: TerminalCandidate,
}

impl CommittedSource {
    pub(super) fn new(file: LedgerFile, receipt: StateFrame, terminal: TerminalCandidate) -> Self {
        Self {
            file,
            receipt,
            terminal,
        }
    }

    pub(in crate::flow) fn source(&self) -> &SourceBlockId {
        self.file.source()
    }

    pub(in crate::flow) fn file(&self) -> &LedgerFile {
        &self.file
    }

    pub(in crate::flow) fn receipt(&self) -> &StateFrame {
        &self.receipt
    }

    pub(in crate::flow) fn common(&self) -> &Common {
        self.receipt.common()
    }

    pub(in crate::flow) fn terminal(&self) -> &TerminalCandidate {
        &self.terminal
    }
}

/// A recorded correct rejection remains available without becoming acceptance.
#[derive(Debug)]
pub(in crate::flow) struct RejectedSource {
    file: Box<StatementFile>,
    reply: Ste100Reply,
    terminal: TerminalCandidate,
}

impl RejectedSource {
    pub(super) fn new(
        file: Box<StatementFile>,
        reply: Ste100Reply,
        terminal: TerminalCandidate,
    ) -> Self {
        Self {
            file,
            reply,
            terminal,
        }
    }

    pub(in crate::flow) fn source(&self) -> &SourceBlockId {
        &self.file.block_id
    }

    pub(in crate::flow) fn file(&self) -> &StatementFile {
        &self.file
    }

    pub(in crate::flow) fn reply(&self) -> &Ste100Reply {
        &self.reply
    }

    pub(in crate::flow) fn terminal(&self) -> &TerminalCandidate {
        &self.terminal
    }
}

#[derive(Debug)]
// Registration reserves each complete slot before progress. This avoids adding
// a new receipt box allocation after the terminal transfer releases the charge.
#[allow(clippy::large_enum_variant)]
pub(in crate::flow) enum SourceDisposition {
    Committed(CommittedSource),
    CorrectlyRejected(RejectedSource),
}

impl SourceDisposition {
    pub(in crate::flow) fn source(&self) -> &SourceBlockId {
        match self {
            Self::Committed(source) => source.source(),
            Self::CorrectlyRejected(source) => source.source(),
        }
    }

    fn bytes(&self) -> u64 {
        match self {
            Self::Committed(source) => source.terminal.bytes,
            Self::CorrectlyRejected(source) => source.terminal.bytes,
        }
    }
}

#[derive(Debug)]
struct Slot {
    source: SourceBlockId,
    disposition: Option<SourceDisposition>,
}

/// One finite, caller-registered source set. It contains no unrelated history.
/// Grants remain private until the complete subscription transfers to its caller.
/// Cancellation preserves facts but makes borrowed and owned grants unavailable.
#[derive(Debug)]
pub(in crate::flow) struct ReceiptSet {
    slots: Vec<Slot>,
    cancelled: bool,
    released: bool,
}

#[derive(Debug)]
pub(in crate::flow) struct ReceiptRegistrationFailure {
    pub sources: Vec<SourceBlockId>,
    pub error: LedgerError,
}

impl ReceiptSet {
    pub(super) fn new(sources: Vec<SourceBlockId>) -> Result<Self, ReceiptRegistrationFailure> {
        let mut slots = Vec::new();
        if slots.try_reserve_exact(sources.len()).is_err() {
            return Err(ReceiptRegistrationFailure {
                sources,
                error: LedgerError("source receipt reservation failed"),
            });
        }
        slots.extend(sources.into_iter().map(|source| Slot {
            source,
            disposition: None,
        }));
        Ok(Self {
            slots,
            cancelled: false,
            released: false,
        })
    }

    pub(in crate::flow) fn sources(&self) -> impl Iterator<Item = &SourceBlockId> {
        self.slots.iter().map(|slot| &slot.source)
    }

    pub(in crate::flow) fn pending(&self) -> impl Iterator<Item = &SourceBlockId> {
        self.slots
            .iter()
            .filter(|slot| slot.disposition.is_none())
            .map(|slot| &slot.source)
    }

    pub(in crate::flow) fn dispositions(&self) -> impl Iterator<Item = &SourceDisposition> {
        self.slots.iter().filter_map(|slot| {
            (self.released && !self.cancelled)
                .then_some(slot.disposition.as_ref())
                .flatten()
        })
    }

    pub(in crate::flow) fn is_complete(&self) -> bool {
        self.slots.iter().all(|slot| slot.disposition.is_some())
    }

    pub(in crate::flow) fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    /// Consumer custody after terminal Recorded, separate from owed-flow charge.
    /// Bytes are original raw block plus context bytes, not an RSS estimate or
    /// parsed-structure allocation size. Registration checks the possible sum.
    pub(in crate::flow) fn retained(&self) -> (u64, u64) {
        self.slots
            .iter()
            .filter_map(|slot| slot.disposition.as_ref())
            .fold((0, 0), |(blocks, bytes), disposition| {
                (blocks + 1, bytes + disposition.bytes())
            })
    }

    pub(in crate::flow) fn into_dispositions(
        self,
    ) -> Result<impl Iterator<Item = SourceDisposition>, Self> {
        if !self.released || self.cancelled || !self.is_complete() {
            return Err(self);
        }
        Ok(self
            .slots
            .into_iter()
            .map(|slot| slot.disposition.expect("complete receipt set")))
    }

    pub(super) fn position(&self, source: &SourceBlockId) -> Option<usize> {
        self.slots.iter().position(|slot| &slot.source == source)
    }

    pub(super) fn slot_available(&self, index: usize) -> bool {
        !self.cancelled && self.slots[index].disposition.is_none()
    }

    pub(super) fn put(&mut self, index: usize, disposition: SourceDisposition) {
        assert!(
            self.slot_available(index),
            "terminal cannot replace receipt custody"
        );
        assert_eq!(self.slots[index].source, *disposition.source());
        self.slots[index].disposition = Some(disposition);
    }

    pub(super) fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub(super) fn release(&mut self) {
        assert!(!self.cancelled && self.is_complete());
        self.released = true;
    }
}
