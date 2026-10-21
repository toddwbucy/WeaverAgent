//! The one rendered line, shared by append-only memory and the writer.

use super::Kind;
use crate::{Failure, Line, Sequence, TurnRef};

#[derive(Debug, Clone)]
pub struct Record {
    pub sequence: Sequence,
    pub kind: Kind,
    pub turn: Option<TurnRef>,
    pub line: Line,
}

/// The canonical run record is read-only outside the recorder.
///
/// ```compile_fail
/// use weaver_trace::serving::WorkingStructure;
/// fn rewrite(trace: &mut WorkingStructure) {
///     trace.iter_mut();
/// }
/// ```
#[derive(Debug, Default)]
pub struct WorkingStructure {
    records: Vec<Record>,
    #[cfg(test)]
    pub(super) fail_append: bool,
}

impl WorkingStructure {
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
        self.records
            .iter()
            .filter(move |record| record.kind == kind)
    }
    pub fn in_turn<'a>(&'a self, turn: &'a TurnRef) -> impl Iterator<Item = &'a Record> {
        self.records
            .iter()
            .filter(move |record| record.turn.as_ref() == Some(turn))
    }
    pub(super) fn append(&mut self, record: Record) -> Result<(), Failure> {
        #[cfg(test)]
        if self.fail_append {
            return Err(Failure::AppendFailed {
                sequence: record.sequence,
            });
        }
        self.records
            .try_reserve(1)
            .map_err(|_| Failure::AppendFailed {
                sequence: record.sequence,
            })?;
        self.records.push(record);
        Ok(())
    }
}
