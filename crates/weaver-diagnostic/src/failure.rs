//! conforms: diagnostic-failure-enum-exhaustive
//!
//! The failure vocabulary, per `weaver-diagnostic-Spec` section 6:
//! exhaustive, so a new case reaches every caller. A refusal has no effect
//! on the sink and a write failure is terminal for the record, per the
//! contract's section 5. What the harness does with either is its own: this
//! crate reports and never decides, which is the no-policy half of the
//! charter's section 1.

/// What a submission can come back with instead of a sequence.
#[derive(Debug, Clone, PartialEq)]
pub enum Failure {
    SubmitRefused { refusal: SubmitRefusal },
    WriteFailed { error: WriteError },
}

/// Why an event was refused before the sink was touched. A refused
/// submission consumes no sequence, so a gap in a record is a lost write
/// and never a refusal.
#[derive(Debug, Clone, PartialEq)]
pub enum SubmitRefusal {
    UnknownKind,
    PayloadMalformed,
    PayloadKindMismatch,
    RequiredFieldAbsent { field: FieldName },
}

/// The write's own account of failing, terminal for the record: every
/// later submission answers with the failure that ended it rather than
/// pretending a sink that lost a line still holds a record.
#[derive(Debug, Clone, PartialEq)]
pub struct WriteError(pub String);

/// The member a refusal names, per section 8's satellite election.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldName(pub String);
