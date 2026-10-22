//! The replacement store's typed content ports, under STATE-FILE and STATE-READ.
//!
//! A receipt accounts for a completed RAM transaction. It is not a socket write
//! or a grant to dispatch work. The legacy Store remains selected until cutover.

use std::fmt;

use weaver_types::state_flow::{
    PairFile, QueryResult, ReadRefusalCode, ReadSelector, SourceBlockId, StateRefusalCode,
    StatementFile,
};

/// Evidence that one complete source and its effects committed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileCommit {
    pub block_id: SourceBlockId,
}

/// A typed filing refusal with an operator-facing diagnostic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileFault {
    pub code: StateRefusalCode,
    pub detail: String,
}

impl FileFault {
    #[cfg(feature = "sqlite")]
    pub(crate) fn new(code: StateRefusalCode, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }
}

impl fmt::Display for FileFault {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.code, self.detail)
    }
}

impl std::error::Error for FileFault {}

#[cfg(feature = "sqlite")]
impl From<rusqlite::Error> for FileFault {
    fn from(error: rusqlite::Error) -> Self {
        Self::new(StateRefusalCode::WriteFailed, error.to_string())
    }
}

/// Ordinary query refusals remain distinct from a broken member. The latter has
/// no refusal frame and must close the channel at the process boundary.
#[derive(Debug)]
pub enum ReadFault {
    Refused(ReadRefusalCode),
    ComponentFailure { detail: String },
}

impl std::fmt::Display for ReadFault {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Refused(code) => write!(formatter, "query refused: {code:?}"),
            Self::ComponentFailure { detail } => formatter.write_str(detail),
        }
    }
}

impl std::error::Error for ReadFault {}

/// The outcome and exact SQL transaction interval of a member filing call.
/// Pre-transaction validation refusals have no transaction phase. A failure to
/// measure the required clock is a component failure outside this result.
#[derive(Debug)]
pub struct ObservedFiling {
    pub result: Result<FileCommit, FileFault>,
    pub transaction: Option<weaver_types::state_flow::OperationPhase>,
}

/// No caller SQL or direct plan/mark mutation crosses this interface.
pub trait ServingStore {
    fn file(&mut self, filing: &StatementFile) -> Result<FileCommit, FileFault>;
    fn file_pair(&mut self, filing: &PairFile) -> Result<FileCommit, FileFault>;
    fn query(&self, selector: &ReadSelector) -> Result<QueryResult, ReadFault>;
}
