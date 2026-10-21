//! Frozen first-failure evidence under HARNESS-HALT-ACCOUNT and HALT-ACCOUNT-SIZE.
//!
//! The context freezes the real owners before building this snapshot. This module
//! owns neither a transport nor a dump operation. Its encoder has no input arm.

use std::collections::HashSet;
use std::io::{self, Write};

use serde_json::value::RawValue;
use weaver_trace::serving::payloads::{
    BlockedStage, CopyAtHalt, CopyContentType, CopySourceEvent, FlowObservation, FlowPhase,
};
use weaver_types::ExchangeId;
use weaver_types::state_flow::{CallId, DecimalU64, SourceBlockId, StartIdentity};

use super::author::{AuthoredEvent, AuthoredSource};
use super::channel::CommitEvidence;

macro_rules! spellings {
    ($name:ident { $($variant:ident => $text:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub(super) enum $name { $($variant),+ }

        impl $name {
            pub(super) fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $text),+ }
            }
        }
    };
}

spellings!(FlowHaltReason {
    TapFailed => "tap_failed",
    CarryFailed => "carry_failed",
    FilingRefused => "filing_refused",
    ClassifierFailed => "classifier_failed",
    GateBound => "gate_bound",
    CutBound => "cut_bound",
    DecisionBound => "decision_bound",
    ControlBound => "control_bound",
    ProtocolFailed => "protocol_failed",
    TraceFailed => "trace_failed",
    QueueLimit => "queue_limit",
});
spellings!(HaltPoint {
    Tap => "tap",
    CopyIntake => "copy_intake",
    Author => "author",
    StateSend => "state_send",
    StateReply => "state_reply",
    Classifier => "classifier",
    Barrier => "barrier",
    Counter => "counter",
    Teardown => "teardown",
});
spellings!(RequestDomain { State => "state", Ste100 => "ste100" });
spellings!(BlockState {
    AwaitingClassification => "awaiting_classification",
    ReadyToFile => "ready_to_file",
    Sending => "sending",
    AwaitingReceipt => "awaiting_receipt",
});
spellings!(PairState {
    Candidate => "candidate",
    Opening => "opening",
    Opened => "opened",
    Executing => "executing",
    ExecutingInternal => "executing_internal",
    Ending => "ending",
    Completed => "completed",
    Failed => "failed",
});
spellings!(CaptureState {
    QueueBoundary => "queue_boundary",
    Beginning => "beginning",
    Barrier => "barrier",
    AwaitImage => "await_image",
    AwaitChecked => "await_checked",
    AwaitFinished => "await_finished",
    AwaitPrepared => "await_prepared",
    AwaitRecorded => "await_recorded",
    AwaitPublished => "await_published",
    Completing => "completing",
    Aborting => "aborting",
});

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RequestAtHalt {
    pub domain: RequestDomain,
    pub id: DecimalU64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct WaitAtHalt {
    pub stage: BlockedStage,
    pub began_ns: DecimalU64,
    pub deadline_ns: DecimalU64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct BlockAtHalt {
    pub block_id: SourceBlockId,
    pub content_type: CopyContentType,
    pub state: BlockState,
    pub request_id: Option<DecimalU64>,
    pub bytes_sent: DecimalU64,
    pub frame_bytes: Option<DecimalU64>,
    pub commit: CommitEvidence,
}

#[derive(Debug, Clone)]
pub(super) struct AuthorAtHalt {
    pub ticket: DecimalU64,
    pub event: AuthoredEvent,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct OutstandingPair {
    pub call_id: CallId,
    pub state: PairState,
    pub execution: Option<ExchangeId>,
    pub opening_source: SourceBlockId,
    pub opening_filed: bool,
    pub terminal_source: Option<SourceBlockId>,
    pub terminal_filed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CaptureAtHalt {
    pub capture_id: DecimalU64,
    pub state: CaptureState,
    pub commit_sent: bool,
    pub published: Option<StartIdentity>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SecondaryFailure {
    pub reason: FlowHaltReason,
    pub point: HaltPoint,
    pub observed_ns: Option<DecimalU64>,
    pub detail: String,
}

/// Every field is supplied explicitly from a frozen owner. In particular, there
/// is no Default that can turn missing producer facts into empty obligations.
#[derive(Debug, Clone)]
pub(super) struct FlowHaltAccount {
    pub reason: FlowHaltReason,
    pub point: HaltPoint,
    pub detail: String,
    pub observed_ns: Option<DecimalU64>,
    pub source_event: Option<CopySourceEvent>,
    pub source_id: Option<SourceBlockId>,
    pub input_id: Option<DecimalU64>,
    pub request: Option<RequestAtHalt>,
    pub received_request: Option<RequestAtHalt>,
    pub wait: Option<WaitAtHalt>,
    pub outstanding_blocks: Vec<BlockAtHalt>,
    pub queued_copies: Vec<CopyAtHalt>,
    pub unheld_copy: Option<CopyAtHalt>,
    pub pending_author: Option<AuthorAtHalt>,
    pub outstanding_pairs: Vec<OutstandingPair>,
    pub capture: Option<CaptureAtHalt>,
    pub flow_sample: FlowObservation,
    pub secondary: Vec<SecondaryFailure>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ReportError {
    Invalid(&'static str),
    CounterOverflow,
    AllocationFailed,
    EncodingFailed,
    SizeChanged,
    ClockUnavailable,
    SnapshotMissing,
    SnapshotAlreadyInstalled,
    FirstCauseMismatch,
}

impl std::fmt::Display for ReportError {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(out, "halt report: {self:?}")
    }
}

impl std::error::Error for ReportError {}

impl FlowHaltAccount {
    pub(super) fn validate(&self) -> Result<(), ReportError> {
        if let (Some(event), Some(source)) = (&self.source_event, &self.source_id)
            && (event.run != source.run || event.sequence.0 != source.sequence.0)
        {
            return Err(ReportError::Invalid("source and event identities disagree"));
        }
        if let Some(wait) = &self.wait
            && wait.began_ns > wait.deadline_ns
        {
            return Err(ReportError::Invalid("wait begins after its deadline"));
        }
        let bound_stage = match self.reason {
            FlowHaltReason::GateBound => Some(BlockedStage::Gate),
            FlowHaltReason::CutBound => Some(BlockedStage::Cut),
            FlowHaltReason::DecisionBound | FlowHaltReason::QueueLimit => {
                Some(BlockedStage::Decision)
            }
            FlowHaltReason::ControlBound => Some(BlockedStage::Control),
            _ => None,
        };
        if let Some(stage) = bound_stage {
            let wait = self
                .wait
                .as_ref()
                .ok_or(ReportError::Invalid("bound has no wait"))?;
            if wait.stage != stage || self.observed_ns.is_none_or(|now| now < wait.deadline_ns) {
                return Err(ReportError::Invalid(
                    "bound has no observed matching expiry",
                ));
            }
        }
        if self.reason == FlowHaltReason::QueueLimit
            && (!self.flow_sample.backlog_waiting
                || self.unheld_copy.is_none()
                || self.unheld_copy != self.flow_sample.waiting_copy)
        {
            return Err(ReportError::Invalid(
                "queue limit has no matching waiting copy",
            ));
        }
        if self.flow_sample.phase != FlowPhase::Sampled
            || self.flow_sample.sample_start_ns > self.flow_sample.sampled_ns
            || self.flow_sample.failed_corrections.is_some()
            || self.flow_sample.retry_cap.is_some()
        {
            return Err(ReportError::Invalid(
                "halt requires its final sampled interval",
            ));
        }
        for block in &self.outstanding_blocks {
            let sent = block.bytes_sent.0;
            let full = block.frame_bytes.map(|bytes| bytes.0);
            let valid = match block.commit {
                CommitEvidence::NotRequested => sent == 0,
                CommitEvidence::NotCommitted => full.is_some_and(|full| sent > 0 && sent < full),
                CommitEvidence::Unknown | CommitEvidence::Committed | CommitEvidence::Refused => {
                    full.is_some_and(|full| sent > 0 && sent == full)
                }
            };
            if !valid || full.is_some_and(|full| sent > full) {
                return Err(ReportError::Invalid(
                    "commit evidence contradicts byte progress",
                ));
            }
        }
        if self
            .outstanding_pairs
            .iter()
            .any(|pair| pair.terminal_filed && pair.terminal_source.is_none())
        {
            return Err(ReportError::Invalid("filed pair terminal has no source"));
        }
        if self
            .capture
            .as_ref()
            .is_some_and(|capture| capture.published.is_some() && !capture.commit_sent)
        {
            return Err(ReportError::Invalid(
                "published capture has no completed Commit",
            ));
        }
        self.validate_disjoint()
    }

    fn validate_disjoint(&self) -> Result<(), ReportError> {
        let mut count = self.outstanding_blocks.len();
        for copy in self.queued_copies.iter().chain(self.unheld_copy.iter()) {
            count = count
                .checked_add(copy.sources.len())
                .ok_or(ReportError::CounterOverflow)?;
        }
        let mut sources = HashSet::new();
        sources
            .try_reserve(count)
            .map_err(|_| ReportError::AllocationFailed)?;
        for block in &self.outstanding_blocks {
            let id = &block.block_id;
            if !sources.insert((id.run.as_str(), id.sequence.0, id.block.0)) {
                return Err(ReportError::Invalid("a ledger obligation occurs twice"));
            }
        }
        for copy in self.queued_copies.iter().chain(self.unheld_copy.iter()) {
            for id in &copy.sources {
                if !sources.insert((id.run.as_str(), id.sequence.0, id.block.0)) {
                    return Err(ReportError::Invalid(
                        "ledger and held copy identities overlap",
                    ));
                }
            }
        }
        Ok(())
    }

    pub(super) fn account_bytes(&self) -> Result<usize, ReportError> {
        self.validate()?;
        self.count_with(&self.secondary)
    }

    pub(super) fn to_raw(&self) -> Result<Box<RawValue>, ReportError> {
        self.validate()?;
        self.raw_with(&self.secondary)
    }

    fn count_with(&self, secondary: &[SecondaryFailure]) -> Result<usize, ReportError> {
        let mut count = CountWriter {
            bytes: 0,
            overflow: false,
        };
        let result = self.write_account(&mut count, secondary);
        if count.overflow {
            return Err(ReportError::CounterOverflow);
        }
        result?;
        Ok(count.bytes)
    }

    fn raw_with(&self, secondary: &[SecondaryFailure]) -> Result<Box<RawValue>, ReportError> {
        let account_bytes = self.count_with(secondary)?;
        let mut out = ExactWriter::new(account_bytes)?;
        self.write_account(&mut out, secondary)?;
        if out.bytes.len() != account_bytes {
            return Err(ReportError::SizeChanged);
        }
        let json = String::from_utf8(out.bytes).map_err(|_| ReportError::EncodingFailed)?;
        RawValue::from_string(json).map_err(|_| ReportError::EncodingFailed)
    }
}

/// One first cause, one original deadline, and one later frozen snapshot. No
/// reporting error can remove this value or grant a decision. The context closes
/// failed when deadline() cannot be established, without inventing a clock anchor.
#[derive(Debug)]
pub(super) struct HaltLatch {
    first: SecondaryFailure,
    deadline: Result<u64, ReportError>,
    account: Option<FlowHaltAccount>,
    secondary: Vec<SecondaryFailure>,
}

#[derive(Debug)]
pub(super) struct InstallFailure {
    pub account: FlowHaltAccount,
    pub error: ReportError,
}

#[derive(Debug)]
pub(super) struct SecondaryRejected {
    pub failure: SecondaryFailure,
    pub error: ReportError,
}

impl HaltLatch {
    pub(super) fn new(first: SecondaryFailure, leave_deadline: Option<u64>) -> Self {
        // HARNESS-HALT-BOUND reuses the existing 150-second post-turn allowance.
        let deadline = first
            .observed_ns
            .ok_or(ReportError::ClockUnavailable)
            .and_then(|now| {
                now.0
                    .checked_add(150_000_000_000)
                    .ok_or(ReportError::CounterOverflow)
            })
            .map(|deadline| leave_deadline.map_or(deadline, |leave| leave.min(deadline)));
        Self {
            first,
            deadline,
            account: None,
            secondary: Vec::new(),
        }
    }

    pub(super) fn first(&self) -> &SecondaryFailure {
        &self.first
    }
    pub(super) fn deadline(&self) -> Result<u64, ReportError> {
        self.deadline
    }
    pub(super) fn account(&self) -> Option<&FlowHaltAccount> {
        self.account.as_ref()
    }
    pub(super) fn secondary(&self) -> &[SecondaryFailure] {
        &self.secondary
    }

    // Returning the owned snapshot must not require a new allocation on failure.
    #[allow(clippy::result_large_err)]
    pub(super) fn install_account(
        &mut self,
        account: FlowHaltAccount,
    ) -> Result<(), InstallFailure> {
        let result = if self.account.is_some() {
            Err(ReportError::SnapshotAlreadyInstalled)
        } else if account.reason != self.first.reason
            || account.point != self.first.point
            || account.observed_ns != self.first.observed_ns
            || account.detail != self.first.detail
            || !account.secondary.is_empty()
        {
            Err(ReportError::FirstCauseMismatch)
        } else {
            account.validate()
        };
        if let Err(error) = result {
            return Err(InstallFailure { account, error });
        }
        self.account = Some(account);
        Ok(())
    }

    pub(super) fn append_secondary(
        &mut self,
        failure: SecondaryFailure,
    ) -> Result<(), SecondaryRejected> {
        if self.secondary.try_reserve(1).is_err() {
            return Err(SecondaryRejected {
                failure,
                error: ReportError::AllocationFailed,
            });
        }
        self.secondary.push(failure);
        Ok(())
    }

    pub(super) fn fault_account(&self) -> Result<Box<RawValue>, ReportError> {
        self.account
            .as_ref()
            .ok_or(ReportError::SnapshotMissing)?
            .raw_with(&[])
    }

    pub(super) fn unload_account(&self) -> Result<Box<RawValue>, ReportError> {
        self.account
            .as_ref()
            .ok_or(ReportError::SnapshotMissing)?
            .raw_with(&self.secondary)
    }
}

struct CountWriter {
    bytes: usize,
    overflow: bool,
}

impl Write for CountWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let Some(next) = self.bytes.checked_add(bytes.len()) else {
            self.overflow = true;
            return Err(io::Error::other("halt account byte count overflow"));
        };
        self.bytes = next;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct ExactWriter {
    bytes: Vec<u8>,
    limit: usize,
}

impl ExactWriter {
    fn new(limit: usize) -> Result<Self, ReportError> {
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(limit)
            .map_err(|_| ReportError::AllocationFailed)?;
        Ok(Self { bytes, limit })
    }
}

impl Write for ExactWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let next = self
            .bytes
            .len()
            .checked_add(bytes.len())
            .ok_or_else(|| io::Error::other("halt account byte count overflow"))?;
        if next > self.limit {
            return Err(io::Error::other("halt account size changed"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn raw(out: &mut impl Write, bytes: &[u8]) -> Result<(), ReportError> {
    out.write_all(bytes)
        .map_err(|_| ReportError::EncodingFailed)
}

// Existing floor/trace serializers are invoked by concrete call sites. The
// harness acquires no serde dependency or deserialize implementation of its own.
macro_rules! value {
    ($out:expr, $value:expr) => {
        serde_json::to_writer(&mut *$out, $value).map_err(|_| ReportError::EncodingFailed)
    };
}
macro_rules! field {
    ($out:expr, $key:literal, $value:expr) => {{
        raw($out, concat!("\"", $key, "\":").as_bytes())?;
        value!($out, $value)?;
    }};
}

macro_rules! next {
    ($out:expr, $key:literal, $value:expr) => {{
        raw($out, b",")?;
        field!($out, $key, $value);
    }};
}

fn optional<T, W: Write>(
    out: &mut W,
    item: Option<&T>,
    write: impl FnOnce(&T, &mut W) -> Result<(), ReportError>,
) -> Result<(), ReportError> {
    match item {
        Some(item) => write(item, out),
        None => raw(out, b"null"),
    }
}

fn array<T, W: Write>(
    out: &mut W,
    items: &[T],
    write: impl Fn(&T, &mut W) -> Result<(), ReportError>,
) -> Result<(), ReportError> {
    raw(out, b"[")?;
    for (index, item) in items.iter().enumerate() {
        if index != 0 {
            raw(out, b",")?;
        }
        write(item, out)?;
    }
    raw(out, b"]")
}

impl FlowHaltAccount {
    fn write_account(
        &self,
        out: &mut impl Write,
        secondary: &[SecondaryFailure],
    ) -> Result<(), ReportError> {
        raw(out, b"{")?;
        field!(out, "reason", &self.reason.as_str());
        next!(out, "point", &self.point.as_str());
        next!(out, "detail", &self.detail);
        next!(out, "observed_ns", &self.observed_ns);
        next!(out, "source_event", &self.source_event);
        next!(out, "source_id", &self.source_id);
        next!(out, "input_id", &self.input_id);
        raw(out, b",\"request\":")?;
        optional(out, self.request.as_ref(), RequestAtHalt::write_json)?;
        raw(out, b",\"received_request\":")?;
        optional(
            out,
            self.received_request.as_ref(),
            RequestAtHalt::write_json,
        )?;
        raw(out, b",\"wait\":")?;
        optional(out, self.wait.as_ref(), WaitAtHalt::write_json)?;
        raw(out, b",\"outstanding_blocks\":")?;
        array(out, &self.outstanding_blocks, BlockAtHalt::write_json)?;
        next!(out, "queued_copies", &self.queued_copies);
        next!(out, "unheld_copy", &self.unheld_copy);
        raw(out, b",\"pending_author\":")?;
        optional(out, self.pending_author.as_ref(), AuthorAtHalt::write_json)?;
        raw(out, b",\"outstanding_pairs\":")?;
        array(out, &self.outstanding_pairs, OutstandingPair::write_json)?;
        raw(out, b",\"capture\":")?;
        optional(out, self.capture.as_ref(), CaptureAtHalt::write_json)?;
        next!(out, "flow_sample", &self.flow_sample);
        raw(out, b",\"secondary\":")?;
        array(out, secondary, SecondaryFailure::write_json)?;
        raw(out, b"}")
    }
}

impl RequestAtHalt {
    fn write_json(&self, out: &mut impl Write) -> Result<(), ReportError> {
        raw(out, b"{")?;
        field!(out, "domain", &self.domain.as_str());
        next!(out, "id", &self.id);
        raw(out, b"}")
    }
}

impl WaitAtHalt {
    fn write_json(&self, out: &mut impl Write) -> Result<(), ReportError> {
        raw(out, b"{")?;
        field!(out, "stage", &self.stage);
        next!(out, "began_ns", &self.began_ns);
        next!(out, "deadline_ns", &self.deadline_ns);
        raw(out, b"}")
    }
}

impl BlockAtHalt {
    fn write_json(&self, out: &mut impl Write) -> Result<(), ReportError> {
        let commit = match self.commit {
            CommitEvidence::NotRequested => "not_requested",
            CommitEvidence::NotCommitted => "not_committed",
            CommitEvidence::Unknown => "unknown",
            CommitEvidence::Committed => "committed",
            CommitEvidence::Refused => "refused",
        };
        raw(out, b"{")?;
        field!(out, "block_id", &self.block_id);
        next!(out, "content_type", &self.content_type);
        next!(out, "state", &self.state.as_str());
        next!(out, "request_id", &self.request_id);
        next!(out, "bytes_sent", &self.bytes_sent);
        next!(out, "frame_bytes", &self.frame_bytes);
        next!(out, "commit", &commit);
        raw(out, b"}")
    }
}

impl AuthorAtHalt {
    fn write_json(&self, out: &mut impl Write) -> Result<(), ReportError> {
        raw(out, b"{")?;
        field!(out, "ticket", &self.ticket);
        raw(out, b",\"event\":{")?;
        field!(out, "kind", &self.event.kind);
        next!(out, "subsystem", &self.event.subsystem);
        next!(out, "turn", &self.event.turn);
        raw(out, b",\"source\":")?;
        match &self.event.source {
            AuthoredSource::BoundaryInput(uid) => {
                raw(out, b"{")?;
                field!(out, "boundary_input", uid);
                raw(out, b"}")?;
            }
            AuthoredSource::ManagementInput(cause) => {
                raw(out, b"{")?;
                field!(out, "management_input", cause);
                raw(out, b"}")?;
            }
            AuthoredSource::Model => value!(out, &"model")?,
            AuthoredSource::Tool => value!(out, &"tool")?,
            AuthoredSource::Loop => value!(out, &"loop")?,
            AuthoredSource::Runtime => value!(out, &"runtime")?,
        }
        next!(out, "causal_parent", &self.event.causal_parent);
        next!(out, "payload", &self.event.payload);
        raw(out, b"}}")
    }
}

impl OutstandingPair {
    fn write_json(&self, out: &mut impl Write) -> Result<(), ReportError> {
        raw(out, b"{")?;
        field!(out, "call_id", &self.call_id);
        next!(out, "state", &self.state.as_str());
        next!(out, "execution", &self.execution);
        next!(out, "opening_source", &self.opening_source);
        next!(out, "opening_filed", &self.opening_filed);
        next!(out, "terminal_source", &self.terminal_source);
        next!(out, "terminal_filed", &self.terminal_filed);
        raw(out, b"}")
    }
}

impl CaptureAtHalt {
    fn write_json(&self, out: &mut impl Write) -> Result<(), ReportError> {
        raw(out, b"{")?;
        field!(out, "capture_id", &self.capture_id);
        next!(out, "state", &self.state.as_str());
        next!(out, "commit_sent", &self.commit_sent);
        next!(out, "published", &self.published);
        raw(out, b"}")
    }
}

impl SecondaryFailure {
    fn write_json(&self, out: &mut impl Write) -> Result<(), ReportError> {
        raw(out, b"{")?;
        field!(out, "reason", &self.reason.as_str());
        next!(out, "point", &self.point.as_str());
        next!(out, "observed_ns", &self.observed_ns);
        next!(out, "detail", &self.detail);
        raw(out, b"}")
    }
}

#[cfg(test)]
#[path = "report_tests.rs"]
mod tests;
