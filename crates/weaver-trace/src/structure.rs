//! conforms: trace-structure-holds-rendered-lines
//! conforms: trace-structure-no-mutation-surface
//! conforms: trace-index-envelope-fields-only
//! conforms: trace-one-rendering-two-holders
//!
//! The working structure, per `weaver-trace-Spec` section 4: an append-only
//! log of rendered lines with an index over the envelope fields a reader
//! selects on. The stored value is the rendered line itself and not a second
//! representation of it.

use crate::canonical::Sequence;
use crate::event::{Kind, Line, TurnRef};

/// The run's record so far, in RAM, in the canonical form the stream carries.
///
/// Append-only is structural: there is no `remove`, no `truncate`, no
/// `get_mut`, no method returning a mutable record, and the append itself is
/// crate-private - only the emit path can extend the structure, so a caller
/// holding a reference has no expressible way to alter what landed. The named
/// mutators are pinned by compile-fail doctests at the crate root; the
/// signature half is pinned by the compiling accessor doctest beside them.
#[derive(Debug)]
pub struct WorkingStructure {
    records: Vec<Record>,
}

/// One landed event: the envelope fields a reader selects on, lifted out of
/// the line so selection needs no parse, and the line itself. Nothing else is
/// lifted, because every further field would be a second copy of data the line
/// already holds.
#[derive(Debug, Clone)]
pub struct Record {
    pub sequence: Sequence,
    pub kind: Kind,
    pub turn: Option<TurnRef>,
    pub line: Line,
}

impl WorkingStructure {
    pub fn new() -> Self {
        WorkingStructure {
            records: Vec::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Record> {
        self.records.iter()
    }

    pub fn by_kind(&self, kind: Kind) -> impl Iterator<Item = &Record> {
        self.records.iter().filter(move |r| r.kind == kind)
    }

    pub fn in_turn<'a>(&'a self, turn: &'a TurnRef) -> impl Iterator<Item = &'a Record> {
        self.records
            .iter()
            .filter(move |r| r.turn.as_ref() == Some(turn))
    }

    /// The one append, reachable only from this crate's emit path.
    pub(crate) fn append(&mut self, record: Record) {
        self.records.push(record);
    }
}

impl Default for WorkingStructure {
    fn default() -> Self {
        Self::new()
    }
}
