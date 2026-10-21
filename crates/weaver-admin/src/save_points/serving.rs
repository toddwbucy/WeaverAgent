//! Bound serving artifacts, per the foundation Spec sections 5.1 and 5.3.
//!
//! This module judges a held directory and its indexed three-file artifacts.
//! It does not publish, start a member, take a run lock, or change the running
//! admin verbs. The later cutover must call it only after root inventory and
//! the existing invocation/run-lock checks. Schema adoption remains the member's.

pub(crate) mod codec;
pub(crate) mod declaration;
pub(crate) mod repository;

use std::fmt;
use weaver_types::state_flow::PersistenceCase;

/// An artifact judgment is not a worker outcome or an automatic recovery.
#[derive(Debug)]
pub(crate) struct Fault {
    pub(crate) case: PersistenceCase,
    pub(crate) detail: String,
}

impl Fault {
    fn new(case: PersistenceCase, detail: impl Into<String>) -> Self {
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
