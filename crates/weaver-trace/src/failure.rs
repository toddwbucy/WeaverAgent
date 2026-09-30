//! conforms: trace-failure-enum-exhaustive
//! conforms: trace-append-failed-no-recovery
//!
//! The failure vocabulary, per `weaver-trace-Spec` section 9: four cases
//! matching `weaver-harness-trace-contract` section 5 one to one. Nothing
//! returns a partial result with a success status, and every refusal names its
//! case rather than carrying a string, so a caller branches on a value.

use crate::canonical::Sequence;

/// What the recorder can report. Exhaustive, so a fifth case reaches every
/// caller at compile time in the act that adds it.
#[derive(Debug, Clone, PartialEq)]
pub enum Failure {
    RefusedOnSubmit {
        reason: SubmitRefusal,
    },
    CommitFailed {
        sequence: Sequence,
        source: WriteError,
    },
    AppendFailed {
        sequence: Sequence,
    },
    WriteTargetUnusable {
        source: WriteError,
    },
}

/// Why a submission was refused. A refusal has no effect on either sink.
#[derive(Debug, Clone, PartialEq)]
pub enum SubmitRefusal {
    UnknownKind,
    PayloadMalformed,
    RequiredFieldAbsent { field: FieldName },
}

/// The name of the field a refusal names. A satellite newtype per
/// `weaver-trace-Spec` section 12, this crate's own rather than the floor's,
/// per the no-internal-dependency rule.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldName(pub String);

/// What went wrong at the sink, carried as a value a caller can hold and
/// compare: the OS error code where one exists, and the rendered description.
/// A satellite shape per section 12.
#[derive(Debug, Clone, PartialEq)]
pub struct WriteError {
    pub os_error: Option<i32>,
    pub message: String,
}

impl From<&std::io::Error> for WriteError {
    fn from(err: &std::io::Error) -> Self {
        WriteError {
            os_error: err.raw_os_error(),
            message: err.to_string(),
        }
    }
}
