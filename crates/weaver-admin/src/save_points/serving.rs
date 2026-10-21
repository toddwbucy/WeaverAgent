//! Bound serving artifacts, per the foundation Spec sections 5.1 and 5.3.
//!
//! This module judges and publishes indexed three-file artifacts through held
//! descriptors. Publication owns filesystem steps, never lifecycle outcomes.
//! It does not start a member, take a run lock, or change the running admin verbs. The later cutover must call it only after root inventory and
//! the existing invocation/run-lock checks. Schema adoption remains the member's.

pub(crate) mod codec;
pub(crate) mod declaration;
mod fs_ops;
pub(crate) mod publication;
pub(crate) mod repository;

use std::fmt;
use std::path::Path;
use weaver_types::TraceSink;
use weaver_types::state_flow::PersistenceCase;

/// The declared sink's exact path is part of the bound serving identity.
pub(crate) fn trace_path(sink: &TraceSink) -> &Path {
    match sink {
        TraceSink::File { path, .. }
        | TraceSink::Pipe { path, .. }
        | TraceSink::Socket { path } => path,
    }
}

/// An artifact judgment is not a worker outcome or an automatic recovery.
#[derive(Debug)]
pub(crate) struct Fault {
    pub(crate) case: PersistenceCase,
    pub(crate) detail: String,
}

impl Fault {
    pub(super) fn new(case: PersistenceCase, detail: impl Into<String>) -> Self {
        Self {
            case,
            detail: detail.into(),
        }
    }
}

impl fmt::Display for Fault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.case, self.detail)
    }
}

impl std::error::Error for Fault {}
