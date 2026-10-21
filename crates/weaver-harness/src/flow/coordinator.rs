//! Ordered trace-copy filing, composed from the sole transport owners.
//!
//! The recording context lends its author, barrier, inbox and retention budget.
//! Transport progress is separate from reduction: it can fill a held completion
//! while one trace ticket waits, but only reduction consumes a recording proof.
//! Lifecycle/gate/decode/legacy-label cursors join this owner at coupled cutover.

use std::collections::VecDeque;
use std::io;

use weaver_trace::serving::{Kind, Payload, payloads as trace};
use weaver_trace::{Sequence, Subsystem, TurnRef};
use weaver_types::state_flow::{
    DecimalU64, Digest, MeasuredInterval, ReadLimits, STATE_CONTENT_SCHEMA, SchemeDeclaration,
    SourceBlockId, SourceEnvelope, SourcePosition, StartIdentity, StateControl, StateControlAnswer,
    StateFrame, Ste100Reply, Ste100Request, Ste100Result, Ste100Target,
};

use super::author::{AuthoredEvent, AuthoredSource, ServingAuthor};
use super::channel::{StateChannel, StateCompletion, StateRequest, UnusedOpen};
use super::classifier::{Classifier, ClassifierState, Ste100Port, Submit};
use super::clock::PumpIo;
use super::intake::{FlowInbox, IntakeError, IntakeFailure, IntakeOutcome, RetentionBudget};
use super::ledger::{
    AcceptedReuse, Classified, Ledger, LedgerState, TerminalCandidate, TerminalOutcome,
};
use super::observation::{
    GaugeSnapshot, Observation, PendingSource, SamplePoint, Transition, WaitingSource,
};
use super::recording::{RecordBarrier, RecordProgress, RecordProof, RecordPurpose};
use super::report::{HaltLatch, SecondaryFailure};

pub(super) struct FlowRecording<'a> {
    pub barrier: &'a mut RecordBarrier,
    pub inbox: &'a mut FlowInbox,
    pub budget: &'a mut RetentionBudget,
    pub author: &'a ServingAuthor,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FlowFailure {
    pub point: &'static str,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Advance {
    Progress,
    Pending,
    Idle,
}

/// One invocation progresses only this owner. The common pump checks its
/// absolute deadlines before choosing the next transport unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TransportSource {
    State,
    Classifier,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum OperationStart<T> {
    Started { request_id: DecimalU64 },
    Busy(T),
}

/// Only ordinary reads and inspections use this completion route. Persistence
/// needs a separate paired cleanup owner for Snapshot/Finished plus Aborted.
#[derive(Debug, PartialEq)]
pub(super) struct ControlOperation {
    request: StateControl,
    turn: Option<TurnRef>,
}

impl TryFrom<(StateControl, Option<TurnRef>)> for ControlOperation {
    type Error = (StateControl, Option<TurnRef>);

    fn try_from((request, turn): Self::Error) -> Result<Self, Self::Error> {
        match request {
            StateControl::Read { .. }
            | StateControl::Next { .. }
            | StateControl::Release { .. }
            | StateControl::Shape {}
            | StateControl::Held {}
            | StateControl::Position {}
            | StateControl::Grants {} => Ok(Self { request, turn }),
            request => Err((request, turn)),
        }
    }
}

impl ControlOperation {
    pub(super) fn into_parts(self) -> (StateControl, Option<TurnRef>) {
        (self.request, self.turn)
    }
}

/// The caller owns the source of this work. None of these operations grants
/// selected-copy admission or manufactures a ledger entry. Input submission is
/// licensed only through its registered intake and measured submission method.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum CheckOperation {
    Input {
        input_id: DecimalU64,
        offered_sequence: DecimalU64,
        text: String,
    },
    Template {
        template_id: String,
        text: String,
    },
    UnselectedModel {
        source: SourceBlockId,
        text: String,
        turn: TurnRef,
    },
}

impl CheckOperation {
    fn target(&self) -> Ste100Target {
        match self {
            Self::Input { input_id, .. } => Ste100Target::Input {
                input_id: *input_id,
                block: 0.into(),
            },
            Self::Template { template_id, .. } => Ste100Target::Template {
                template_id: template_id.clone(),
            },
            Self::UnselectedModel { source, .. } => Ste100Target::Source(source.clone()),
        }
    }

    fn text(&self) -> &str {
        match self {
            Self::Input { text, .. }
            | Self::Template { text, .. }
            | Self::UnselectedModel { text, .. } => text,
        }
    }
}

/// A returned author remains owned even after its misuse latches failure.
#[derive(Debug)]
pub(super) enum RefusedAuthor {
    Prepared {
        event: weaver_trace::serving::Event,
        purpose: RecordPurpose,
    },
    Authored {
        event: AuthoredEvent,
        purpose: RecordPurpose,
    },
}

/// A single owned operation completion, available only after the exact trace
/// proof. A read adapter still owes its recall record before placing content.
#[derive(Debug)]
// One fixed slot holds either result. Keep the value inline so consuming a
// recording proof never needs a second allocation to publish its completion.
#[allow(clippy::large_enum_variant)]
pub(super) enum OperationCompletion {
    Capture {
        request: StateControl,
        completion: StateCompletion,
        recorded_ns: u64,
    },
    Input {
        operation: CheckOperation,
        reply: Ste100Reply,
        reuse: Option<AcceptedReuse>,
        recorded_ns: u64,
    },
    Opened(StateCompletion),
    State {
        request: StateControl,
        turn: Option<TurnRef>,
        completion: StateCompletion,
    },
    Check(RecordedCheck),
}

/// An operation result and its exact recorded event, held as one capability.
/// Only the coordinator's successful classifier-proof reduction constructs it.
/// It cannot be cloned or reconstructed from a reply or a caller-supplied source.
#[derive(Debug)]
pub(super) struct RecordedCheck {
    operation: CheckOperation,
    reply: Ste100Reply,
    source: SourcePosition,
}

impl RecordedCheck {
    pub(super) fn operation(&self) -> &CheckOperation {
        &self.operation
    }

    pub(super) fn reply(&self) -> &Ste100Reply {
        &self.reply
    }

    pub(super) fn source(&self) -> &SourcePosition {
        &self.source
    }

    pub(super) fn into_parts(self) -> (CheckOperation, Ste100Reply, SourcePosition) {
        (self.operation, self.reply, self.source)
    }

    #[cfg(test)]
    pub(super) fn for_test(
        operation: CheckOperation,
        reply: Ste100Reply,
        source: SourcePosition,
    ) -> Self {
        Self {
            operation,
            reply,
            source,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CapturePhase {
    Barrier,
    SnapshotPending,
    Held,
    AcknowledgePending,
    Finished,
    ReleaseQueued,
    Aborting,
    Complete,
}

/// Read-only custody facts. The context owns save, leg and cleanup deadlines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct CaptureSnapshot {
    pub capture_id: DecimalU64,
    pub frontier: u64,
    pub request_id: Option<DecimalU64>,
    pub phase: CapturePhase,
}

struct CaptureOwner {
    snapshot: CaptureSnapshot,
    leg: Option<StateControl>,
    // The original leg remains owned until its exact observation is recorded.
    // Abort is the channel's sole permitted second exchange.
    cleanup: Option<StateControl>,
    release_queued: Option<StateControl>,
    refused_snapshot: bool,
    queued_ns: Option<u64>,
}

#[derive(Clone)]
struct Notice {
    input_id: Option<u64>,
    point: SamplePoint,
    turn: Option<String>,
}

struct InputAccount {
    id: DecimalU64,
    began_ns: u64,
    request_id: Option<DecimalU64>,
}

struct InputReuse {
    grant: AcceptedReuse,
    source_sequence: Option<u64>,
}

#[derive(Clone, Copy)]
enum PendingRecord {
    Classifier,
    State,
    Flow,
    StateRelease,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AdmissionAdvance {
    Pending,
    Held,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CoordinatorPhase {
    Preflight,
    Opening,
    Active,
    Releasing,
    Closed,
    Failed,
}

/// One consumer reserves before authoring. Its source is bound only by the
/// recorder's actual completion, before any classification can be reduced.
struct ReceiptReservation {
    sequence: Option<u64>,
    expected: Vec<SourceBlockId>,
    registered: bool,
    author_recorded_ns: Option<u64>,
    recorded_ns: Option<u64>,
}

/// No socket, mutable ledger or completion escapes through a caller-owned clone.
/// The sole run-wide latch freezes this owner before reporting. Component
/// notifications preserve every current source and transport fact for that latch.
pub(super) struct FlowCoordinator<P> {
    run: String,
    ledger: Ledger,
    receipt_reservation: Option<ReceiptReservation>,
    classifier: Option<Classifier<P>>,
    starting_classifier: Option<P>,
    admission_completion: Option<super::ste100_socket::AdmissionPoll>,
    admission_refused: bool,
    admission_probe_pending: bool,
    classifier_retired: bool,
    state: Option<StateChannel>,
    state_retired: bool,
    unused_open: Option<UnusedOpen>,
    observations: Option<Observation>,
    observation_config: Option<(MeasuredInterval, u64)>,
    scheme: SchemeDeclaration,
    phase: CoordinatorPhase,
    open_pending: bool,
    notices: VecDeque<Notice>,
    pending_record: Option<PendingRecord>,
    failure: Option<FlowFailure>,
    halt: Option<HaltLatch>,
    frozen_sample: Option<trace::FlowObservation>,
    snapshot_failure: Option<FlowFailure>,
    backlog_reported: bool,
    started_file: Option<SourceBlockId>,
    reuse: Option<InputReuse>,
    input_account: Option<InputAccount>,
    last_input_id: u64,
    state_operation: Option<(StateControl, Option<TurnRef>)>,
    capture: Option<CaptureOwner>,
    check_operation: Option<CheckOperation>,
    operation_completion: Option<OperationCompletion>,
    state_release_timing: Option<(u64, u64)>,
    state_recorded_ns: Option<u64>,
    refused_author: Option<RefusedAuthor>,
}

impl<P: Ste100Port> FlowCoordinator<P> {
    pub(super) fn new(
        run: String,
        ledger: Ledger,
        classifier: Classifier<P>,
        state: StateChannel,
        observations: Observation,
    ) -> Result<Self, FlowFailure> {
        let (session, state_run, state_scheme) = state.identity();
        if run.is_empty()
            || !state.can_begin_filing()
            || state_run != run
            || classifier.run() != run
            || observations.run() != run
            || state_scheme != Some(classifier.scheme().scheme_id.as_str())
            || !ledger.matches_identity(&run, session, classifier.scheme())
        {
            return Err(FlowFailure {
                point: "open",
                detail: "filing requires recorded open confirmation".into(),
            });
        }
        Ok(Self {
            run,
            ledger,
            receipt_reservation: None,
            scheme: classifier.scheme().clone(),
            classifier: Some(classifier),
            starting_classifier: None,
            admission_completion: None,
            admission_refused: false,
            admission_probe_pending: false,
            classifier_retired: false,
            state: Some(state),
            state_retired: false,
            unused_open: None,
            observations: Some(observations),
            observation_config: None,
            phase: CoordinatorPhase::Active,
            open_pending: false,
            notices: VecDeque::new(),
            pending_record: None,
            failure: None,
            halt: None,
            frozen_sample: None,
            snapshot_failure: None,
            backlog_reported: false,
            started_file: None,
            reuse: None,
            input_account: None,
            last_input_id: 0,
            state_operation: None,
            capture: None,
            check_operation: None,
            operation_completion: None,
            state_release_timing: None,
            state_recorded_ns: None,
            refused_author: None,
        })
    }

    pub(super) fn run(&self) -> &str {
        &self.run
    }

    pub(super) fn classifier_retired(&self) -> bool {
        self.classifier_retired
    }

    pub(super) fn failure(&self) -> Option<&FlowFailure> {
        self.failure.as_ref()
    }
    pub(super) fn halt(&self) -> Option<&HaltLatch> {
        self.halt.as_ref()
    }

    pub(super) fn halt_mut(&mut self) -> Option<&mut HaltLatch> {
        self.halt.as_mut()
    }

    pub(super) fn has_pending_record(&self) -> bool {
        self.pending_record.is_some()
    }

    /// First-latch facts are captured before any teardown transport is probed.
    /// A pre-open failure has no sample because no observation baseline exists.
    pub(super) fn final_sample(&self) -> Result<Option<&trace::FlowObservation>, &FlowFailure> {
        match &self.snapshot_failure {
            Some(failure) => Err(failure),
            None => Ok(self.frozen_sample.as_ref()),
        }
    }

    pub(super) fn latch(
        &mut self,
        first: SecondaryFailure,
        leave_deadline: Option<u64>,
        cx: &mut FlowRecording<'_>,
    ) {
        if self.halt.is_some() {
            return;
        }
        let observed_ns = first.observed_ns;
        let detail = first.detail.clone();
        self.halt = Some(HaltLatch::new(first, leave_deadline));
        self.fail(cx, "halt", detail);
        if self.observations.is_none() {
            return;
        }
        let result = (|| {
            let now = observed_ns.ok_or_else(|| FlowFailure {
                point: "clock",
                detail: "first halt has no observation timestamp".into(),
            })?;
            let gauges = self.gauges(cx)?;
            self.observations
                .as_mut()
                .expect("checked observation owner")
                .final_sample(gauges, now.0)
                .map_err(|error| FlowFailure {
                    point: "observation",
                    detail: error.to_string(),
                })
        })();
        match result {
            Ok(sample) => self.frozen_sample = Some(sample),
            Err(failure) => self.snapshot_failure = Some(failure),
        }
    }

    pub(super) fn ledger(&self) -> &Ledger {
        &self.ledger
    }
    /// Reserve the only consumer before its original author is submitted.
    /// This is not a source lookup or a way to recover a disposed receipt.
    pub(super) fn reserve_filing_receipts(
        &mut self,
        cx: &mut FlowRecording<'_>,
    ) -> Result<(), FlowFailure> {
        self.observe_failures(cx)?;
        if self.phase != CoordinatorPhase::Active
            || self.receipt_reservation.is_some()
            || self.ledger.receipts().is_some()
        {
            return Err(self.fail(cx, "receipt", "filing receipt consumer is already owned"));
        }
        self.receipt_reservation = Some(ReceiptReservation {
            sequence: None,
            expected: Vec::new(),
            registered: false,
            author_recorded_ns: None,
            recorded_ns: None,
        });
        Ok(())
    }

    pub(super) fn has_filing_consumer(&self) -> bool {
        self.receipt_reservation.is_some()
    }

    /// Called only at the actual ordinary Recorded boundary. Intake is already
    /// drained, but a whole copy may still wait for retention room.
    pub(super) fn bind_filing_receipts(
        &mut self,
        cx: &mut FlowRecording<'_>,
        sequence: Sequence,
        recorded_ns: u64,
    ) -> Result<(), FlowFailure> {
        self.observe_failures(cx)?;
        if self
            .receipt_reservation
            .as_ref()
            .is_none_or(|r| r.sequence.is_some())
        {
            return Err(self.fail(cx, "receipt", "receipt consumer has no unbound author"));
        }
        let installed: Vec<_> = self
            .ledger
            .facts()
            .filter(|fact| fact.source.run == self.run && fact.source.sequence.0 == sequence.0)
            .map(|fact| fact.source.clone())
            .collect();
        let pending = pending_receipt_sources(cx, &self.run, sequence, installed.is_empty())
            .map_err(|error| self.fail(cx, "receipt", error))?;
        let deferred = pending.is_some();
        let expected = pending.unwrap_or(installed);
        if !deferred {
            self.ledger
                .register_receipts(expected.clone())
                .map_err(|error| self.fail(cx, "receipt", error.error))?;
        }
        let owner = self
            .receipt_reservation
            .as_mut()
            .expect("unbound reservation");
        owner.sequence = Some(sequence.0);
        owner.author_recorded_ns = Some(recorded_ns);
        owner.expected = expected;
        owner.registered = !deferred;
        if !deferred && owner.expected.is_empty() {
            owner.recorded_ns = Some(recorded_ns);
        }
        Ok(())
    }

    fn register_deferred_receipts(
        &mut self,
        cx: &mut FlowRecording<'_>,
        sequence: u64,
        sources: &[SourceBlockId],
    ) -> Result<(), FlowFailure> {
        let Some(owner) = self.receipt_reservation.as_ref() else {
            return Ok(());
        };
        if owner.sequence != Some(sequence) {
            return Ok(());
        }
        if owner.registered || owner.expected != sources {
            return Err(self.fail(
                cx,
                "receipt",
                "installed source differs from reserved author",
            ));
        }
        self.ledger
            .register_receipts(sources.to_vec())
            .map_err(|error| self.fail(cx, "receipt", error.error))?;
        let owner = self
            .receipt_reservation
            .as_mut()
            .expect("matching reservation");
        owner.registered = true;
        if sources.is_empty() {
            owner.recorded_ns = owner.author_recorded_ns;
        }
        Ok(())
    }

    pub(super) fn filing_receipts(&self) -> Option<&super::ledger::ReceiptSet> {
        self.ledger.receipts()
    }

    pub(super) fn filing_recorded_ns(&self) -> Option<u64> {
        self.receipt_reservation
            .as_ref()
            .and_then(|r| r.recorded_ns)
    }

    pub(super) fn take_filing_receipts(
        &mut self,
        cx: &mut FlowRecording<'_>,
    ) -> Result<Option<super::ledger::ReceiptSet>, FlowFailure> {
        self.observe_failures(cx)?;
        if cx.barrier.pending().is_some() || self.pending_record.is_some() {
            return Ok(None);
        }
        let result = self
            .ledger
            .take_receipts()
            .map_err(|error| self.fail(cx, "receipt", error))?;
        if result.is_some() {
            self.receipt_reservation = None;
        }
        Ok(result)
    }

    /// Subscription cancellation never cancels a selected filing or its trace
    /// disposition. The partial set is marked unusable by the ledger itself.
    pub(super) fn cancel_filing_receipts(&mut self) -> Option<super::ledger::ReceiptSet> {
        self.receipt_reservation = None;
        self.ledger.cancel_receipts()
    }

    pub(super) fn state(&self) -> &StateChannel {
        self.state.as_ref().expect("standing state channel")
    }
    pub(super) fn classifier(&self) -> &Classifier<P> {
        self.classifier.as_ref().expect("standing classifier")
    }
    pub(super) fn standing_state(&self) -> Option<&StateChannel> {
        self.state.as_ref()
    }
    pub(super) fn standing_classifier(&self) -> Option<&Classifier<P>> {
        self.classifier.as_ref()
    }
    pub(super) fn starting_classifier(&self) -> Option<&P> {
        self.starting_classifier.as_ref()
    }
    pub(super) fn phase(&self) -> CoordinatorPhase {
        self.phase
    }

    /// The unopened channel and declared identity become this run's sole owners.
    pub(super) fn preflight(
        state: StateChannel,
        scheme: SchemeDeclaration,
        observation_interval: MeasuredInterval,
        lag_alarm_turns: u64,
    ) -> Result<Self, FlowFailure> {
        let (session, run, opened_scheme) = state.identity();
        if opened_scheme.is_some()
            || !state.outstanding().is_empty()
            || state.held_measurement().is_some()
            || state.failure().is_some()
            || scheme.validate().is_err()
            || observation_interval.validate().is_err()
            || lag_alarm_turns == 0
        {
            return Err(FlowFailure {
                point: "open",
                detail: "preflight requires an unused channel and valid declaration".into(),
            });
        }
        let run = run.to_owned();
        let ledger =
            Ledger::new(run.clone(), session.to_owned(), scheme.clone()).map_err(|error| {
                FlowFailure {
                    point: "open",
                    detail: error.to_string(),
                }
            })?;
        Ok(Self {
            run,
            ledger,
            receipt_reservation: None,
            classifier: None,
            starting_classifier: None,
            admission_completion: None,
            admission_refused: false,
            admission_probe_pending: false,
            classifier_retired: false,
            state: Some(state),
            state_retired: false,
            unused_open: None,
            observations: None,
            observation_config: Some((observation_interval, lag_alarm_turns)),
            scheme,
            phase: CoordinatorPhase::Preflight,
            open_pending: false,
            notices: VecDeque::new(),
            pending_record: None,
            failure: None,
            halt: None,
            frozen_sample: None,
            snapshot_failure: None,
            backlog_reported: false,
            started_file: None,
            reuse: None,
            input_account: None,
            last_input_id: 0,
            state_operation: None,
            capture: None,
            check_operation: None,
            operation_completion: None,
            state_release_timing: None,
            state_recorded_ns: None,
            refused_author: None,
        })
    }

    pub(super) fn begin_open(
        &mut self,
        cx: &mut FlowRecording<'_>,
        io: &mut impl PumpIo,
        start: StartIdentity,
        read_limits: ReadLimits,
    ) -> Result<(), FlowFailure> {
        self.observe_failures(cx)?;
        if self.phase != CoordinatorPhase::Preflight || self.open_pending {
            return Err(self.fail(cx, "open", "state open is permitted once during preflight"));
        }
        let now = io.now().map_err(|error| self.fail(cx, "clock", error))?;
        self.open_pending = true;
        let request = StateRequest::Open {
            schema_id: STATE_CONTENT_SCHEMA.into(),
            scheme_id: self.scheme.scheme_id.clone(),
            start,
            read_limits,
        };
        self.state
            .as_mut()
            .expect("preflight channel")
            .begin(request, now)
            .map_err(|error| self.fail(cx, "open", error))?;
        Ok(())
    }

    pub(super) fn opened_identity(
        &self,
    ) -> Option<&weaver_types::state_flow::MemberObservationIdentity> {
        self.state.as_ref().and_then(StateChannel::opened_identity)
    }

    pub(super) fn open_refusal(
        &self,
    ) -> Option<(weaver_types::state_flow::StateRefusalCode, &str)> {
        self.state.as_ref().and_then(StateChannel::open_refusal)
    }

    /// The barrier itself proves load admission and semantic activation. The
    /// held open answer still owes its own exact measurement before release.
    pub(super) fn activate_after_load(
        &mut self,
        cx: &mut FlowRecording<'_>,
    ) -> Result<(), FlowFailure> {
        self.observe_failures(cx)?;
        if self.phase != CoordinatorPhase::Preflight
            || !cx.barrier.is_active()
            || !self.open_pending
            || self.opened_identity().is_none()
        {
            return Err(self.fail(
                cx,
                "open",
                "activation requires recorded load and held matching open",
            ));
        }
        self.phase = CoordinatorPhase::Opening;
        Ok(())
    }

    fn require_active(&mut self, cx: &mut FlowRecording<'_>) -> Result<(), FlowFailure> {
        self.observe_failures(cx)?;
        if self.phase != CoordinatorPhase::Active {
            return Err(self.fail(cx, "barrier", "operation requires an active flow"));
        }
        Ok(())
    }

    fn clean_obligations(&self, cx: &FlowRecording<'_>) -> bool {
        self.capture.is_none() && self.settled_obligations(cx)
    }

    fn settled_obligations(&self, cx: &FlowRecording<'_>) -> bool {
        self.receipt_reservation.is_none()
            && self.ledger.receipts().is_none()
            && self.ledger.facts().next().is_none()
            && cx.inbox.queued().next().is_none()
            && cx.inbox.failure().is_none()
            && cx.budget.waiting_copy().is_none()
            && cx.budget.gauges() == (0, 0)
            && self.notices.is_empty()
            && self.pending_record.is_none()
            && self.operation_completion.is_none()
            && self.input_account.is_none()
            && self.reuse.is_none()
            && self.state_operation.is_none()
            && self.check_operation.is_none()
            && self.admission_completion.is_none()
            && cx.barrier.pending().is_none()
    }

    pub(super) fn unused_open(&self) -> Option<&UnusedOpen> {
        self.unused_open.as_ref()
    }

    /// Existing admin rollback, after an actual preactivation enter refusal.
    /// No tee was installed and this Open can never authorize boot or filing.
    pub(super) fn begin_enter_rollback(
        &mut self,
        cx: &mut FlowRecording<'_>,
        _io: &mut impl PumpIo,
    ) -> Result<Advance, FlowFailure> {
        self.observe_failures(cx)?;
        if self.phase != CoordinatorPhase::Preflight
            || !cx.barrier.is_refused()
            || cx.barrier.is_active()
            || !self.open_pending
            || self.observations.is_some()
            || self.state_retired
            || self.classifier_retired
            || self.unused_open.is_some()
            || self.classifier.is_some()
            || self.starting_classifier.is_some()
            || !self.clean_obligations(cx)
        {
            return Err(self.fail(
                cx,
                "barrier",
                "enter rollback requires a recorded preactivation refusal",
            ));
        }
        let Some(owner) = &mut self.state else {
            return Err(self.fail(cx, "barrier", "enter rollback lost its state owner"));
        };
        let unused = owner
            .abandon_unactivated_open()
            .map_err(|error| self.fail(cx, "state_reply", error))?;
        let Some(unused) = unused else {
            return Ok(Advance::Pending);
        };
        self.unused_open = Some(unused);
        self.state.take();
        self.state_retired = true;
        self.classifier_retired = true;
        self.open_pending = false;
        self.phase = CoordinatorPhase::Releasing;
        Ok(Advance::Progress)
    }

    pub(super) fn begin_releasing(
        &mut self,
        cx: &mut FlowRecording<'_>,
    ) -> Result<(), FlowFailure> {
        self.require_active(cx)?;
        if !self.clean_obligations(cx)
            || !self.state().can_begin_filing()
            || self
                .classifier
                .as_ref()
                .is_some_and(|owner| owner.state() != ClassifierState::Idle)
            || self.starting_classifier.is_some()
        {
            return Err(self.fail(
                cx,
                "barrier",
                "release requires all owned operations and filings settled",
            ));
        }
        self.phase = CoordinatorPhase::Releasing;
        Ok(())
    }

    pub(super) fn retire_state(
        &mut self,
        cx: &mut FlowRecording<'_>,
        io: &mut impl PumpIo,
    ) -> Result<Advance, FlowFailure> {
        self.observe_failures(cx)?;
        if self.phase != CoordinatorPhase::Releasing
            || self.state_retired
            || !self.clean_obligations(cx)
            || !self
                .state
                .as_ref()
                .is_some_and(StateChannel::can_begin_filing)
        {
            return Err(self.fail(
                cx,
                "barrier",
                "state retirement requires a clean releasing owner",
            ));
        }
        self.state
            .as_mut()
            .expect("checked standing state")
            .progress(&mut || io.now())
            .map_err(|error| self.fail(cx, "state_transport", error))?;
        if self.state().needs_progress() {
            return Ok(Advance::Pending);
        }
        self.state.take();
        self.state_retired = true;
        Ok(Advance::Progress)
    }

    pub(super) fn retire_classifier(
        &mut self,
        cx: &mut FlowRecording<'_>,
    ) -> Result<Advance, FlowFailure> {
        self.observe_failures(cx)?;
        if self.phase != CoordinatorPhase::Releasing
            || self.classifier_retired
            || self.starting_classifier.is_some()
            || !self.clean_obligations(cx)
            || self
                .classifier
                .as_ref()
                .is_some_and(|owner| owner.state() != ClassifierState::Idle)
        {
            return Err(self.fail(
                cx,
                "classifier",
                "classifier retirement requires a clean releasing owner",
            ));
        }
        if let Some(owner) = &self.classifier {
            match retirement_status(owner.readiness()) {
                Ok(false) => return Ok(Advance::Pending),
                Ok(true) => {}
                Err(error) => return Err(self.fail(cx, "classifier", error)),
            }
        }
        if let Some(mut owner) = self.classifier.take() {
            owner.close();
        }
        self.classifier_retired = true;
        Ok(Advance::Progress)
    }

    pub(super) fn close(&mut self, cx: &mut FlowRecording<'_>) -> Result<(), FlowFailure> {
        self.observe_failures(cx)?;
        if self.phase != CoordinatorPhase::Releasing
            || !self.state_retired
            || !self.classifier_retired
            || !self.clean_obligations(cx)
        {
            return Err(self.fail(
                cx,
                "barrier",
                "close requires retired ports and settled evidence",
            ));
        }
        self.phase = CoordinatorPhase::Closed;
        Ok(())
    }

    pub(super) fn capture_snapshot(&self) -> Option<CaptureSnapshot> {
        self.capture.as_ref().map(|owner| owner.snapshot)
    }

    pub(super) fn capture_queued_ns(&self) -> Option<u64> {
        self.capture.as_ref().and_then(|owner| owner.queued_ns)
    }

    /// A Recorded leg may still owe the nonblocking final release probe. Its
    /// deadline ends at this preserved boundary, not at a later EINTR retry.
    pub(super) fn capture_recorded_ns(&self) -> Option<u64> {
        self.capture.as_ref().and_then(|owner| {
            owner.leg.as_ref()?;
            // A late original's proof cannot end the cleanup leg's deadline.
            if owner.cleanup.is_some() || owner.release_queued.is_some() {
                return None;
            }
            matches!(self.pending_record, Some(PendingRecord::StateRelease))
                .then_some(self.state_recorded_ns)
                .flatten()
        })
    }

    /// The context has frozen ordinary admission at a turn boundary. Existing
    /// owed copies still drain through their actual classification and receipts.
    pub(super) fn register_capture_barrier(
        &mut self,
        cx: &mut FlowRecording<'_>,
        capture_id: DecimalU64,
        frontier: u64,
    ) -> Result<(), FlowFailure> {
        self.require_active(cx)?;
        if capture_id.0 == 0 || self.capture.is_some() {
            return Err(self.fail(cx, "barrier", "invalid or repeated capture barrier"));
        }
        self.check_capture_frontier(cx, frontier)?;
        self.capture = Some(CaptureOwner {
            snapshot: CaptureSnapshot {
                capture_id,
                frontier,
                request_id: None,
                phase: CapturePhase::Barrier,
            },
            leg: None,
            cleanup: None,
            release_queued: None,
            refused_snapshot: false,
            queued_ns: None,
        });
        Ok(())
    }

    fn check_capture_frontier(
        &mut self,
        cx: &mut FlowRecording<'_>,
        frontier: u64,
    ) -> Result<(), FlowFailure> {
        let later = self
            .ledger
            .facts()
            .any(|fact| fact.source.sequence.0 > frontier)
            || cx
                .inbox
                .queued()
                .chain(cx.budget.waiting_copy())
                .any(|copy| {
                    super::intake::selected_blocks(copy)
                        .iter()
                        .any(|block| block.source.sequence.0 > frontier)
                });
        if later {
            return Err(self.fail(cx, "barrier", "capture frontier precedes owned content"));
        }
        Ok(())
    }

    /// A maximum sequence alone is not a disposition. This checks every owned
    /// ledger row, copy, proof, operation and observation, including later rows.
    pub(super) fn capture_barrier_ready(
        &mut self,
        cx: &mut FlowRecording<'_>,
    ) -> Result<bool, FlowFailure> {
        self.require_active(cx)?;
        let Some(owner) = self.capture.as_ref() else {
            return Err(self.fail(cx, "barrier", "capture has no registered barrier"));
        };
        let snapshot = owner.snapshot;
        if snapshot.phase != CapturePhase::Barrier {
            return Ok(false);
        }
        self.check_capture_frontier(cx, snapshot.frontier)?;
        Ok(self.settled_obligations(cx)
            && self.state().can_begin_filing()
            && self
                .classifier
                .as_ref()
                .is_none_or(|port| port.state() == ClassifierState::Idle)
            && self.starting_classifier.is_none())
    }

    pub(super) fn begin_capture_snapshot(
        &mut self,
        cx: &mut FlowRecording<'_>,
        io: &mut impl PumpIo,
        capture_id: DecimalU64,
    ) -> Result<OperationStart<DecimalU64>, FlowFailure> {
        self.require_capture(cx, capture_id)?;
        if !self.capture_barrier_ready(cx)? {
            return Ok(OperationStart::Busy(capture_id));
        }
        let request_id = self.start_capture_leg(
            cx,
            io,
            StateControl::Snapshot { capture_id },
            CapturePhase::SnapshotPending,
        )?;
        self.capture
            .as_mut()
            .expect("registered capture")
            .snapshot
            .request_id = Some(request_id);
        Ok(OperationStart::Started { request_id })
    }

    pub(super) fn acknowledge_capture(
        &mut self,
        cx: &mut FlowRecording<'_>,
        io: &mut impl PumpIo,
        capture_id: DecimalU64,
        dump_sha256: Digest,
    ) -> Result<OperationStart<(DecimalU64, Digest)>, FlowFailure> {
        self.require_capture(cx, capture_id)?;
        if !self.capture_leg_ready(cx, CapturePhase::Held) {
            return Ok(OperationStart::Busy((capture_id, dump_sha256)));
        }
        let request_id = self.start_capture_leg(
            cx,
            io,
            StateControl::Acknowledge {
                capture_id,
                dump_sha256,
            },
            CapturePhase::AcknowledgePending,
        )?;
        Ok(OperationStart::Started { request_id })
    }

    pub(super) fn release_capture(
        &mut self,
        cx: &mut FlowRecording<'_>,
        io: &mut impl PumpIo,
        capture_id: DecimalU64,
    ) -> Result<OperationStart<DecimalU64>, FlowFailure> {
        self.require_capture(cx, capture_id)?;
        if !self.capture_leg_ready(cx, CapturePhase::Finished) {
            return Ok(OperationStart::Busy(capture_id));
        }
        let request_id = self.start_capture_leg(
            cx,
            io,
            StateControl::ReleaseCapture { capture_id },
            CapturePhase::Finished,
        )?;
        Ok(OperationStart::Started { request_id })
    }

    /// Accept a fresh release intent for a Recorded Finished. The
    /// context decides whether that original result was timely. Acceptance is
    /// the queue-time anchor even if the original proof or a later trace author
    /// still prevents the channel from beginning this new exchange.
    pub(super) fn enqueue_capture_release(
        &mut self,
        cx: &mut FlowRecording<'_>,
        io: &mut impl PumpIo,
        capture_id: DecimalU64,
    ) -> Result<bool, FlowFailure> {
        self.require_capture(cx, capture_id)?;
        let owner = self.capture.as_ref().expect("checked capture");
        if owner.release_queued.is_some() || owner.cleanup.is_some() {
            return Ok(false);
        }
        let recorded_completion = matches!(
            &self.operation_completion,
            Some(OperationCompletion::Capture {
                request: StateControl::Acknowledge { capture_id: id, .. },
                completion: StateCompletion {
                    answer: StateFrame::ControlAnswer {
                        frame: StateControlAnswer::Finished { capture_id: returned, .. }, ..
                    },
                    discard_capture_result: false,
                }, ..
            }) if *id == capture_id && *returned == capture_id
        );
        let finished = match owner.snapshot.phase {
            CapturePhase::Finished => {
                owner.leg.is_none() && (recorded_completion || self.operation_completion.is_none())
            }
            CapturePhase::AcknowledgePending => {
                self.operation_completion.is_none()
                    && matches!(self.pending_record, Some(PendingRecord::StateRelease))
                    && self.state_recorded_ns.is_some()
                    && self.state().recorded_finished(capture_id)
            }
            _ => false,
        };
        if !finished {
            return Ok(false);
        }
        let now = io.now().map_err(|error| self.fail(cx, "clock", error))?;
        let owner = self.capture.as_mut().expect("checked capture");
        owner.release_queued = Some(StateControl::ReleaseCapture { capture_id });
        owner.snapshot.phase = CapturePhase::ReleaseQueued;
        owner.queued_ns = Some(now);
        if recorded_completion
            && let Some(OperationCompletion::Capture { completion, .. }) =
                &mut self.operation_completion
        {
            completion.discard_capture_result = true;
        }
        self.start_queued_capture_release(cx)?;
        Ok(true)
    }

    fn start_queued_capture_release(
        &mut self,
        cx: &mut FlowRecording<'_>,
    ) -> Result<bool, FlowFailure> {
        if !self
            .capture
            .as_ref()
            .is_some_and(|owner| owner.release_queued.is_some() && owner.leg.is_none())
            || self.operation_completion.is_some()
            || self.pending_record.is_some()
            || cx.barrier.pending().is_some()
            || !self.state().outstanding().is_empty()
            || self.state().held_measurement().is_some()
        {
            return Ok(false);
        }
        let owner = self.capture.as_mut().expect("queued release owner");
        let request = owner.release_queued.take().expect("queued release request");
        let queued_ns = owner.queued_ns.expect("accepted release anchor");
        owner.leg = Some(request.clone());
        owner.snapshot.phase = CapturePhase::Finished;
        self.state
            .as_mut()
            .expect("active state")
            .begin(StateRequest::Control(request), queued_ns)
            .map_err(|error| self.fail(cx, "state_send", error))?;
        Ok(true)
    }

    /// Queue the one cleanup continuation without replacing an outstanding
    /// Snapshot/Acknowledge or consuming its held measurement. The channel keeps
    /// both partial-send offsets and identifies the original result as discard.
    pub(super) fn abort_capture(
        &mut self,
        cx: &mut FlowRecording<'_>,
        io: &mut impl PumpIo,
        capture_id: DecimalU64,
    ) -> Result<OperationStart<DecimalU64>, FlowFailure> {
        self.require_capture(cx, capture_id)?;
        let owner = self.capture.as_ref().expect("checked capture");
        let recorded_original = matches!(
            &self.operation_completion,
            Some(OperationCompletion::Capture {
                request: StateControl::Snapshot { capture_id: id }
                    | StateControl::Acknowledge { capture_id: id, .. },
                completion,
                ..
            }) if *id == capture_id && !completion.discard_capture_result
        );
        if (self.operation_completion.is_some() && !recorded_original)
            || owner.cleanup.is_some()
            || owner.release_queued.is_some()
            || !(matches!(
                owner.snapshot.phase,
                CapturePhase::SnapshotPending
                    | CapturePhase::Held
                    | CapturePhase::AcknowledgePending
                    | CapturePhase::Finished
            ) || (owner.snapshot.phase == CapturePhase::Complete && owner.refused_snapshot))
            || matches!(owner.leg, Some(StateControl::ReleaseCapture { .. }))
        {
            return Ok(OperationStart::Busy(capture_id));
        }
        let now = io.now().map_err(|e| self.fail(cx, "clock", e))?;
        let owner = self.capture.as_mut().expect("checked capture");
        let request = StateControl::Abort { capture_id };
        if owner.leg.is_some() {
            owner.cleanup = Some(request);
        } else {
            owner.leg = Some(request);
        }
        owner.snapshot.phase = CapturePhase::Aborting;
        owner.queued_ns = Some(now);
        if recorded_original
            && let Some(OperationCompletion::Capture { completion, .. }) =
                &mut self.operation_completion
        {
            // The original proof already exists, but a later author may still
            // block its consumer. Preserve that one result as explicit discard
            // while the channel carries cleanup on the same capture identity.
            completion.discard_capture_result = true;
        }
        let common = self
            .state
            .as_mut()
            .expect("active state")
            .abort_capture(now)
            .map_err(|e| self.fail(cx, "state_send", e))?;
        Ok(OperationStart::Started {
            request_id: common.request_id,
        })
    }

    /// The caller records the final save outcome before relinquishing this hold.
    /// A refused capture retains its identity until Abort proves cleanup.
    pub(super) fn finish_capture(
        &mut self,
        cx: &mut FlowRecording<'_>,
        capture_id: DecimalU64,
    ) -> Result<(), FlowFailure> {
        self.require_capture(cx, capture_id)?;
        let owner = self.capture.as_ref().expect("checked capture");
        if owner.snapshot.phase != CapturePhase::Complete
            || owner.refused_snapshot
            || owner.leg.is_some()
            || owner.cleanup.is_some()
            || owner.release_queued.is_some()
            || !self.settled_obligations(cx)
            || !self.state().can_begin_filing()
        {
            return Err(self.fail(cx, "barrier", "capture still owes result or cleanup"));
        }
        self.capture.take();
        Ok(())
    }

    /// Expiry before Snapshot needs no member cleanup. The caller records the
    /// refusal before dropping only this hold. Existing filing remains owned.
    pub(super) fn cancel_capture_barrier(
        &mut self,
        cx: &mut FlowRecording<'_>,
        capture_id: DecimalU64,
    ) -> Result<(), FlowFailure> {
        self.require_capture(cx, capture_id)?;
        let owner = self.capture.as_ref().expect("checked capture");
        if owner.snapshot.phase != CapturePhase::Barrier
            || owner.leg.is_some()
            || owner.cleanup.is_some()
            || cx.barrier.pending().is_some()
        {
            return Err(self.fail(
                cx,
                "barrier",
                "capture cancellation already owes a member result",
            ));
        }
        self.capture.take();
        Ok(())
    }

    fn require_capture(
        &mut self,
        cx: &mut FlowRecording<'_>,
        capture_id: DecimalU64,
    ) -> Result<(), FlowFailure> {
        self.require_active(cx)?;
        if self
            .capture
            .as_ref()
            .is_none_or(|owner| owner.snapshot.capture_id != capture_id)
        {
            return Err(self.fail(cx, "barrier", "capture identity has no registered owner"));
        }
        Ok(())
    }

    fn capture_leg_ready(&self, cx: &FlowRecording<'_>, phase: CapturePhase) -> bool {
        self.capture.as_ref().is_some_and(|owner| {
            owner.snapshot.phase == phase
                && owner.leg.is_none()
                && owner.cleanup.is_none()
                && owner.release_queued.is_none()
        }) && self.operation_completion.is_none()
            && self.pending_record.is_none()
            && cx.barrier.pending().is_none()
            && self.state().outstanding().is_empty()
            && self.state().held_measurement().is_none()
    }

    fn start_capture_leg(
        &mut self,
        cx: &mut FlowRecording<'_>,
        io: &mut impl PumpIo,
        request: StateControl,
        phase: CapturePhase,
    ) -> Result<DecimalU64, FlowFailure> {
        let now = io.now().map_err(|e| self.fail(cx, "clock", e))?;
        let owner = self.capture.as_mut().expect("registered capture");
        owner.leg = Some(request.clone());
        owner.snapshot.phase = phase;
        owner.queued_ns = Some(now);
        let common = self
            .state
            .as_mut()
            .expect("active state")
            .begin(StateRequest::Control(request), now)
            .map_err(|e| self.fail(cx, "state_send", e))?;
        Ok(common.request_id)
    }

    /// Begin a nonfiling exchange through the same request counter and socket.
    /// A busy owner returns the entire control unchanged and assigns no identity.
    pub(super) fn begin_control(
        &mut self,
        cx: &mut FlowRecording<'_>,
        io: &mut impl PumpIo,
        request: ControlOperation,
    ) -> Result<OperationStart<ControlOperation>, FlowFailure> {
        self.require_active(cx)?;
        if self.capture.is_some()
            || self.state_operation.is_some()
            || self.operation_completion.is_some()
            || !self.state().outstanding().is_empty()
            || self.state().held_measurement().is_some()
        {
            return Ok(OperationStart::Busy(request));
        }
        let now = io.now().map_err(|e| self.fail(cx, "clock", e))?;
        let (request, turn) = request.into_parts();
        self.state_operation = Some((request.clone(), turn));
        let common = self
            .state
            .as_mut()
            .expect("active state")
            .begin(StateRequest::Control(request), now)
            .map_err(|e| self.fail(cx, "state_send", e))?;
        Ok(OperationStart::Started {
            request_id: common.request_id,
        })
    }

    /// Submit a template or unselected model block without adding a
    /// source obligation. The operation and exact request stay owned until proof.
    pub(super) fn begin_check(
        &mut self,
        cx: &mut FlowRecording<'_>,
        operation: CheckOperation,
    ) -> Result<OperationStart<CheckOperation>, FlowFailure> {
        if matches!(operation, CheckOperation::Input { .. }) {
            return Err(self.fail(cx, "classifier", "input requires measured submission"));
        }
        self.begin_check_inner(cx, operation)
    }

    fn begin_check_inner(
        &mut self,
        cx: &mut FlowRecording<'_>,
        operation: CheckOperation,
    ) -> Result<OperationStart<CheckOperation>, FlowFailure> {
        self.require_active(cx)?;
        if self.classifier.is_none() || self.capture.is_some() {
            return Ok(OperationStart::Busy(operation));
        }
        if self.check_operation.is_some()
            || self.operation_completion.is_some()
            || self.classifier().state() != ClassifierState::Idle
        {
            return Ok(OperationStart::Busy(operation));
        }
        if matches!(&operation, CheckOperation::UnselectedModel { source, .. }
                if source.run != self.run || source.sequence.0 == 0
                    || self.ledger.barrier(std::slice::from_ref(source)).is_ok()
                    || cx.inbox.queued().any(|copy| copy_contains(copy, source))
                    || cx.budget.waiting_copy().is_some_and(|copy| copy_contains(copy, source)))
        {
            return Err(self.fail(cx, "classifier", "invalid nonfiling check source"));
        }
        let request_id = self
            .classifier
            .as_mut()
            .expect("active classifier")
            .next_request_id()
            .map_err(|e| self.fail(cx, "classifier", e.detail))?;
        let request = Ste100Request {
            request_id,
            target: operation.target(),
            text: operation.text().to_owned(),
            scheme_id: self.classifier().scheme().scheme_id.clone(),
        };
        self.check_operation = Some(operation);
        match self
            .classifier
            .as_mut()
            .expect("active classifier")
            .submit(request)
            .map_err(|e| self.fail(cx, "classifier", e.detail))?
        {
            Submit::Submitted => Ok(OperationStart::Started { request_id }),
            Submit::Busy(_) => Ok(OperationStart::Busy(
                self.check_operation.take().expect("owned check"),
            )),
        }
    }

    /// The frame owner registers serialized intake before authoring input.offered.
    /// The supplied start is the gate bound's original sample of this same clock.
    /// Queued frames have no account and therefore cannot start a check clock here.
    pub(super) fn input_started(
        &mut self,
        cx: &mut FlowRecording<'_>,
        io: &mut impl PumpIo,
        input_id: DecimalU64,
        began_ns: u64,
    ) -> Result<(), FlowFailure> {
        // The frame owner already transferred serialized intake. Mirror that
        // ownership before a clock or asynchronously failed trace can latch.
        // An existing failure is immutable and cannot acquire later work.
        self.ensure_live()?;
        if self.phase != CoordinatorPhase::Active {
            return Err(self.fail(cx, "barrier", "input requires an active flow"));
        }
        if self.capture.is_some() {
            return Err(self.fail(
                cx,
                "barrier",
                "input cannot enter a registered save barrier",
            ));
        }
        if self.input_account.is_some() || input_id.0 <= self.last_input_id {
            return Err(self.fail(
                cx,
                "counter",
                "input intake is repeated or already occupied",
            ));
        }
        self.last_input_id = input_id.0;
        self.input_account = Some(InputAccount {
            id: input_id,
            began_ns,
            request_id: None,
        });
        self.observations
            .as_mut()
            .expect("active observation owner")
            .note_input_started()
            .map_err(|error| self.fail(cx, "counter", error))?;
        self.observe_failures(cx)?;
        let now = io.now().map_err(|error| self.fail(cx, "clock", error))?;
        if began_ns > now {
            return Err(self.fail(cx, "clock", "input intake is after registration"));
        }
        Ok(())
    }

    /// This is the sole input submission path. Busy retains the complete operation
    /// and the original intake account without counting a second submission.
    pub(super) fn begin_input_check(
        &mut self,
        cx: &mut FlowRecording<'_>,
        io: &mut impl PumpIo,
        operation: CheckOperation,
    ) -> Result<OperationStart<CheckOperation>, FlowFailure> {
        self.require_active(cx)?;
        let CheckOperation::Input {
            input_id,
            offered_sequence,
            ..
        } = &operation
        else {
            return Err(self.fail(
                cx,
                "classifier",
                "input submission requires an input target",
            ));
        };
        let input_id = *input_id;
        if input_id.0 == 0
            || offered_sequence.0 == 0
            || !self
                .input_account
                .as_ref()
                .is_some_and(|input| input.id == input_id && input.request_id.is_none())
        {
            return Err(self.fail(
                cx,
                "classifier",
                "input submission lacks its original intake",
            ));
        }
        let result = self.begin_check_inner(cx, operation)?;
        if let OperationStart::Started { request_id } = &result {
            self.input_account
                .as_mut()
                .expect("registered input")
                .request_id = Some(*request_id);
            let mut transition = Transition::new(trace::FlowPhase::ClassificationStarted);
            transition.input_id = Some(input_id.0);
            transition.request_id = Some(request_id.0);
            self.notice(cx, io, transition, None)?;
        }
        Ok(result)
    }

    /// The frame owner calls this only after its required result/refusal or
    /// cancellation disposition. A still-submitted check cannot be forgotten.
    pub(super) fn input_finished(
        &mut self,
        cx: &mut FlowRecording<'_>,
        io: &mut impl PumpIo,
        input_id: DecimalU64,
    ) -> Result<(), FlowFailure> {
        let ended_ns = io.now().map_err(|error| self.fail(cx, "clock", error))?;
        self.input_finished_at(cx, io, input_id, ended_ns)
    }

    /// The completion owner supplies its exact Recorded/response-ack sample.
    /// Time spent reducing or publishing later measurements is not gate wait.
    pub(super) fn input_finished_at(
        &mut self,
        cx: &mut FlowRecording<'_>,
        io: &mut impl PumpIo,
        input_id: DecimalU64,
        ended_ns: u64,
    ) -> Result<(), FlowFailure> {
        self.require_active(cx)?;
        if self
            .input_account
            .as_ref()
            .is_none_or(|input| input.id != input_id)
            || matches!(self.check_operation, Some(CheckOperation::Input { .. }))
            || matches!(
                self.operation_completion,
                Some(OperationCompletion::Input { .. })
            )
        {
            return Err(self.fail(
                cx,
                "counter",
                "input disposition is absent or still outstanding",
            ));
        }
        let now = io.now().map_err(|error| self.fail(cx, "clock", error))?;
        let account = self.input_account.as_ref().expect("matched input");
        let elapsed = ended_ns
            .checked_sub(account.began_ns)
            .filter(|_| ended_ns <= now)
            .ok_or_else(|| {
                self.fail(
                    cx,
                    "clock",
                    "input disposition is outside its clock interval",
                )
            })?;
        let request_id = self
            .input_account
            .as_ref()
            .expect("matched input")
            .request_id;
        self.observations
            .as_mut()
            .expect("active observation owner")
            .note_input_completed()
            .map_err(|error| self.fail(cx, "counter", error))?;
        self.input_account.take();
        let mut transition = Transition::new(trace::FlowPhase::WaitFinished);
        transition.input_id = Some(input_id.0);
        transition.request_id = request_id.map(|id| id.0);
        transition.blocked_stage = Some(trace::BlockedStage::Gate);
        transition.elapsed_ns = Some(elapsed);
        self.notice(cx, io, transition, None)
    }

    pub(super) fn pending_input_id(&self) -> Option<DecimalU64> {
        self.input_account.as_ref().map(|account| account.id)
    }

    pub(super) fn input_measurements_pending(&self) -> bool {
        self.notices.iter().any(|notice| notice.input_id.is_some())
    }

    pub(super) fn take_operation_completion(
        &mut self,
        cx: &mut FlowRecording<'_>,
    ) -> Result<Option<OperationCompletion>, FlowFailure> {
        self.observe_failures(cx)?;
        // HARNESS-RECORD-LIFETIME permits transport progress while an author
        // waits, but completion reduction waits for that author's Recorded.
        if cx.barrier.pending().is_some() {
            return Ok(None);
        }
        Ok(self.operation_completion.take())
    }

    pub(super) fn pending_control(&self) -> Option<(&StateControl, Option<&TurnRef>)> {
        self.state_operation
            .as_ref()
            .map(|(request, turn)| (request, turn.as_ref()))
    }

    pub(super) fn pending_check(&self) -> Option<&CheckOperation> {
        self.check_operation.as_ref()
    }

    pub(super) fn operation_completion(&self) -> Option<&OperationCompletion> {
        self.operation_completion.as_ref()
    }

    pub(super) fn refused_author(&self) -> Option<&RefusedAuthor> {
        self.refused_author.as_ref()
    }

    /// The frame/engine boundary supplies this single-use check immediately
    /// before authoring its accepted message. The ledger still compares every
    /// byte, scheme and offered-sequence link before it can bypass classification.
    pub(super) fn reuse_next_input(&mut self, reuse: AcceptedReuse) -> Result<(), AcceptedReuse> {
        if self.failure.is_some() || self.phase != CoordinatorPhase::Active || self.reuse.is_some()
        {
            return Err(reuse);
        }
        self.reuse = Some(InputReuse {
            grant: reuse,
            source_sequence: None,
        });
        Ok(())
    }

    /// A Recorded accepted message pins its actual source. An unselected or
    /// zero-block copy owes no filing and cannot donate labels to a later event.
    pub(super) fn finish_input_author(
        &mut self,
        cx: &mut FlowRecording<'_>,
        sequence: weaver_trace::Sequence,
    ) -> Result<(), FlowFailure> {
        self.require_active(cx)?;
        let Some(reuse) = &mut self.reuse else {
            return Ok(());
        };
        if sequence.0 <= reuse.grant.offered_sequence().0
            || reuse
                .source_sequence
                .is_some_and(|source| source != sequence.0)
        {
            return Err(self.fail(
                cx,
                "copy_intake",
                "accepted input source does not follow its offer",
            ));
        }
        reuse.source_sequence = Some(sequence.0);
        let owed = cx
            .inbox
            .queued()
            .chain(cx.budget.waiting_copy())
            .try_fold(false, |owed, copy| {
                matches_input_copy(copy, reuse).map(|matches| owed || matches)
            });
        let owed = owed.map_err(|error| self.fail(cx, "copy_intake", error))?;
        if !owed {
            self.reuse.take();
        }
        Ok(())
    }

    fn ensure_live(&self) -> Result<(), FlowFailure> {
        self.failure.clone().map_or(Ok(()), Err)
    }

    fn fail(
        &mut self,
        cx: &mut FlowRecording<'_>,
        point: &'static str,
        detail: impl ToString,
    ) -> FlowFailure {
        if self.failure.is_none() {
            self.phase = CoordinatorPhase::Failed;
            self.failure = Some(FlowFailure {
                point,
                detail: detail.to_string(),
            });
        }
        // Once the semantic latch exists, no subsequent failure probe may move
        // first-failure evidence. latch() performs this freeze before sampling.
        let first_snapshot = self.frozen_sample.is_some() || self.snapshot_failure.is_some();
        // Preserve the last transport fact even when the work unit failed.
        // This updates evidence only and never releases a completion or retries.
        if !first_snapshot
            && let Some(progress) = self
                .state
                .as_ref()
                .map(StateChannel::outstanding)
                .unwrap_or_default()
                .first()
                .filter(|p| p.source.is_some())
        {
            let _ = self
                .ledger
                .observe_progress(progress, self.state().first_sent_ns());
        }
        self.ledger.freeze();
        if let Some(owner) = &mut self.classifier {
            owner.fail();
        }
        cx.budget.freeze();
        cx.barrier.stop_for_flow_failure();
        self.failure.clone().expect("failure is held")
    }

    fn notice(
        &mut self,
        cx: &mut FlowRecording<'_>,
        io: &mut impl PumpIo,
        transition: Transition,
        turn: Option<String>,
    ) -> Result<(), FlowFailure> {
        self.notices.try_reserve(1).map_err(|_| FlowFailure {
            point: "counter",
            detail: "flow transition reservation failed".into(),
        })?;
        let gauges = self.gauges(cx)?;
        let now = io.now().map_err(|e| self.fail(cx, "clock", e))?;
        let input_id = transition.input_id;
        let point = self
            .observations
            .as_mut()
            .expect("recorded open observation")
            .capture(transition, gauges, now)
            .map_err(|e| self.fail(cx, "observation", e))?;
        self.notices.push_back(Notice {
            input_id,
            point,
            turn,
        });
        Ok(())
    }

    fn observe_failures(&mut self, cx: &mut FlowRecording<'_>) -> Result<(), FlowFailure> {
        self.ensure_live()?;
        if let Some(failure) = cx.barrier.failure() {
            return Err(self.fail(cx, "author", format!("{failure:?}")));
        }
        if let Some(failure) = cx.inbox.failure() {
            return Err(self.fail(cx, "copy_intake", failure.detail));
        }
        if cx.budget.is_frozen() {
            return Err(self.fail(cx, "barrier", "retention already froze before progress"));
        }
        Ok(())
    }

    /// Only called after actual trace intake. No classifier answer is reduced here.
    pub(super) fn drain(
        &mut self,
        cx: &mut FlowRecording<'_>,
        io: &mut impl PumpIo,
    ) -> Result<(), FlowFailure> {
        self.observe_failures(cx)?;
        if self.phase != CoordinatorPhase::Active {
            if cx.inbox.queued().next().is_some() || cx.budget.waiting_copy().is_some() {
                return Err(self.fail(cx, "copy_intake", "content reached a non-active flow"));
            }
            return Ok(());
        }
        if let Some(owner) = &self.capture {
            let snapshot = owner.snapshot;
            self.check_capture_frontier(cx, snapshot.frontier)?;
            if snapshot.phase != CapturePhase::Barrier
                && (cx.inbox.queued().next().is_some() || cx.budget.waiting_copy().is_some())
            {
                return Err(self.fail(cx, "barrier", "content entered a held capture"));
            }
        }
        let mut copied = Vec::new();
        while let Some((copy, intake_ns)) = cx.inbox.pop_timed(cx.budget) {
            let Some(intake_ns) = intake_ns else {
                let _ = cx.inbox.preserve_failure(IntakeFailure {
                    copy,
                    detail: IntakeError::ClockReadFailed,
                });
                return Err(self.fail(cx, "copy_intake", "copy has no intake clock"));
            };
            let use_reuse = match self
                .reuse
                .as_ref()
                .map(|reuse| matches_input_copy(&copy, reuse))
                .transpose()
            {
                Ok(matched) => matched.unwrap_or(false),
                Err(error) => {
                    let _ = cx.inbox.preserve_failure(IntakeFailure {
                        copy,
                        detail: IntakeError::InvalidIdentity,
                    });
                    return Err(self.fail(cx, "copy_intake", error));
                }
            };
            let selected_reuse = if use_reuse { self.reuse.take() } else { None };
            let reuse_sequence = selected_reuse
                .as_ref()
                .and_then(|reuse| reuse.source_sequence);
            match self
                .ledger
                .install(copy, intake_ns, selected_reuse.map(|reuse| reuse.grant))
            {
                Ok(installed) => {
                    self.register_deferred_receipts(
                        cx,
                        installed.source_sequence,
                        &installed.sources,
                    )?;
                    copied.try_reserve(1).map_err(|_| {
                        self.fail(cx, "counter", "copied transition reservation failed")
                    })?;
                    copied.push(installed.sources);
                }
                Err(failure) => {
                    let detail = failure.error.to_string();
                    if let Some(grant) = failure.reuse {
                        self.reuse = Some(InputReuse {
                            grant,
                            source_sequence: reuse_sequence,
                        });
                    }
                    let _ = cx.inbox.preserve_failure(IntakeFailure {
                        copy: failure.copy,
                        detail: IntakeError::InvalidIdentity,
                    });
                    return Err(self.fail(cx, "copy_intake", detail));
                }
            }
        }
        // Complete the inbox-to-ledger transfer before freezing gauges. Every
        // queued reservation is then visible, without a second facts owner.
        // This drain performs no disposition or port-completion reduction.
        for sources in copied {
            if sources.is_empty() {
                self.notice(cx, io, Transition::new(trace::FlowPhase::Copied), None)
                    .map_err(|e| self.fail(cx, e.point, e.detail))?;
            }
            for source in sources {
                let turn = self
                    .ledger
                    .facts()
                    .find(|fact| fact.source == &source)
                    .and_then(|fact| fact.turn.map(str::to_owned));
                let mut transition = Transition::new(trace::FlowPhase::Copied);
                transition.source_id = Some(source);
                self.notice(cx, io, transition, turn)
                    .map_err(|e| self.fail(cx, e.point, e.detail))?;
            }
        }
        let waiting = cx.budget.waiting_copy().is_some();
        if waiting != self.backlog_reported {
            let phase = if waiting {
                trace::FlowPhase::BacklogWait
            } else {
                trace::FlowPhase::BacklogResumed
            };
            self.notice(cx, io, Transition::new(phase), None)
                .map_err(|e| self.fail(cx, e.point, e.detail))?;
            self.backlog_reported = waiting;
        }
        Ok(())
    }

    /// Progress exactly one owner. No authoring or completion reduction occurs.
    pub(super) fn transport_one(
        &mut self,
        cx: &mut FlowRecording<'_>,
        io: &mut impl PumpIo,
        source: TransportSource,
    ) -> Result<(), FlowFailure> {
        self.observe_failures(cx)?;
        match source {
            TransportSource::State => {
                let Some(owner) = &mut self.state else {
                    return Err(self.fail(
                        cx,
                        "state_transport",
                        "state port is absent or retired",
                    ));
                };
                owner
                    .progress(&mut || io.now())
                    .map_err(|error| self.fail(cx, "state_transport", error))?;
            }
            TransportSource::Classifier => {
                let Some(owner) = &mut self.classifier else {
                    return Err(self.fail(
                        cx,
                        "classifier",
                        "classifier needs admission or is retired",
                    ));
                };
                if owner.state() == ClassifierState::ResultHeld {
                    owner
                        .check_held_terminal()
                        .map_err(|error| self.fail(cx, "classifier", error.detail))?;
                } else {
                    owner
                        .try_result()
                        .map_err(|error| self.fail(cx, "classifier", error.detail))?;
                }
            }
        }
        Ok(())
    }

    /// Compatibility for the existing component fixtures. The run-wide pump
    /// calls transport_one so deadlines can be checked between distinct owners.
    pub(super) fn transport(
        &mut self,
        cx: &mut FlowRecording<'_>,
        io: &mut impl PumpIo,
        state_ready: bool,
        classifier_ready: bool,
    ) -> Result<(), FlowFailure> {
        self.observe_failures(cx)?;
        if state_ready
            || self
                .state
                .as_ref()
                .is_some_and(StateChannel::ready_buffered)
        {
            self.transport_one(cx, io, TransportSource::State)?;
        }
        if classifier_ready {
            self.transport_one(cx, io, TransportSource::Classifier)?;
        }
        Ok(())
    }

    /// One reduction/author step. The caller pumps the existing sources between
    /// Pending steps, including when an excluded completion record lacks capacity.
    pub(super) fn advance(
        &mut self,
        cx: &mut FlowRecording<'_>,
        io: &mut impl PumpIo,
        wall_ms: u64,
    ) -> Result<Advance, FlowFailure> {
        self.ensure_live()?;
        self.drain(cx, io)?;
        if matches!(self.phase, CoordinatorPhase::Preflight) {
            return Ok(Advance::Pending);
        }
        if matches!(
            self.phase,
            CoordinatorPhase::Releasing | CoordinatorPhase::Closed
        ) {
            return Ok(Advance::Idle);
        }
        if matches!(self.pending_record, Some(PendingRecord::StateRelease)) {
            // The measurement is recorded, but its interrupted final probe must
            // not release a result across a later outstanding ordinary author.
            if cx.barrier.pending().is_some() {
                return Ok(Advance::Pending);
            }
            let completion = self
                .state
                .as_mut()
                .expect("pending state release")
                .resume_release()
                .map_err(|e| self.fail(cx, "state_reply", e))?;
            return self.finish_state_release(cx, io, completion);
        }
        if self.pending_record.is_some() {
            let progress = cx
                .barrier
                .resume_with_clock(cx.inbox, cx.budget, &mut || io.now());
            self.drain(cx, io)?;
            return self.recorded(cx, io, progress);
        }
        // A content author owned by the surrounding context takes precedence.
        if cx.barrier.pending().is_some() {
            return Ok(Advance::Pending);
        }
        let now = io.now().map_err(|e| self.fail(cx, "clock", e))?;
        if self.open_pending {
            let measurement =
                self.state().held_measurement().cloned().ok_or_else(|| {
                    self.fail(cx, "open", "activated open lost its held measurement")
                })?;
            return self.author(
                cx,
                io,
                wall_ms,
                Kind::StateStoreMeasurement,
                None,
                Payload::StateStoreMeasurement(Box::new(super::mirror::measurement(&measurement))),
                RecordPurpose::State(measurement),
                PendingRecord::State,
            );
        }
        if let Some(progress) = self
            .state
            .as_ref()
            .expect("active state")
            .outstanding()
            .first()
            .filter(|p| p.source.is_some())
        {
            self.ledger
                .observe_progress(progress, self.state().first_sent_ns())
                .map_err(|e| self.fail(cx, "ledger", e))?;
            if progress.bytes_sent > 0 && self.started_file.as_ref() != progress.source.as_ref() {
                let source = progress.source.clone().expect("filtered filing");
                let mut transition = Transition::new(trace::FlowPhase::FileStarted);
                transition.source_id = Some(source.clone());
                transition.request_id = Some(progress.common.request_id.0);
                self.notice(cx, io, transition, self.turn(&source))
                    .map_err(|e| self.fail(cx, e.point, e.detail))?;
                self.started_file = Some(source);
            }
        }
        if let Some(notice) = self.notices.front().cloned() {
            if self
                .observations
                .as_mut()
                .expect("recorded open observation")
                .pending()
                .is_none()
            {
                self.observations
                    .as_mut()
                    .expect("recorded open observation")
                    .prepare_captured(&notice.point)
                    .map_err(|e| self.fail(cx, "observation", e))?;
            }
            let sample = self
                .observations
                .as_mut()
                .expect("recorded open observation")
                .pending()
                .expect("prepared sample")
                .clone();
            return self.author(
                cx,
                io,
                wall_ms,
                Kind::StateFlow,
                notice.turn.clone(),
                Payload::FlowObservation(Box::new(sample.clone())),
                RecordPurpose::Flow(sample),
                PendingRecord::Flow,
            );
        }
        if self.operation_completion.is_some() {
            return Ok(Advance::Pending);
        }
        if self.start_queued_capture_release(cx)? {
            return Ok(Advance::Progress);
        }
        if let Some(measurement) = self.state().held_measurement() {
            let measurement = measurement.clone();
            let turn = measurement
                .request_source
                .as_ref()
                .and_then(|source| self.turn(source))
                .or_else(|| {
                    self.state_operation
                        .as_ref()
                        .and_then(|(_, turn)| turn.as_ref().map(|t| t.0.clone()))
                });
            let mirror = super::mirror::measurement(&measurement);
            return self.author(
                cx,
                io,
                wall_ms,
                Kind::StateStoreMeasurement,
                turn,
                Payload::StateStoreMeasurement(Box::new(mirror)),
                RecordPurpose::State(measurement),
                PendingRecord::State,
            );
        }
        if let Some(reply) = self.classifier.as_ref().and_then(Classifier::held) {
            let reply = reply.clone();
            let turn = match &reply.target {
                Ste100Target::Source(source) => {
                    self.turn(source).or_else(|| match &self.check_operation {
                        Some(CheckOperation::UnselectedModel { turn, .. }) => Some(turn.0.clone()),
                        _ => None,
                    })
                }
                _ => None,
            };
            let mirror = super::mirror::ste100(&reply);
            return self.author(
                cx,
                io,
                wall_ms,
                Kind::Ste100Result,
                turn,
                Payload::Ste100Result(Box::new(mirror)),
                RecordPurpose::Classifier(reply),
                PendingRecord::Classifier,
            );
        }
        if self
            .capture
            .as_ref()
            .is_none_or(|owner| owner.snapshot.phase == CapturePhase::Barrier)
            && self.state_operation.is_none()
            && self.state().can_begin_filing()
            && let Some(filing) = self.ledger.next_file()
        {
            let source = filing.source().clone();
            let common = self
                .state
                .as_mut()
                .expect("active state")
                .begin(filing.into_request(), now)
                .map_err(|e| self.fail(cx, "state_send", e))?;
            self.ledger
                .begin_file(&source, &common)
                .map_err(|e| self.fail(cx, "ledger", e))?;
            self.started_file = None;
            return Ok(Advance::Progress);
        }
        if self
            .capture
            .as_ref()
            .is_none_or(|owner| owner.snapshot.phase == CapturePhase::Barrier)
            && self.check_operation.is_none()
            && self.classifier.is_some()
            && self.classifier().state() == ClassifierState::Idle
            && let Some(candidate) = self.ledger.next_classification()
        {
            let request = Ste100Request {
                request_id: self
                    .classifier
                    .as_mut()
                    .expect("active classifier")
                    .next_request_id()
                    .map_err(|e| self.fail(cx, "classifier", e.detail))?,
                target: Ste100Target::Source(candidate.source.clone()),
                text: candidate.text,
                scheme_id: self.classifier().scheme().scheme_id.clone(),
            };
            match self
                .classifier
                .as_mut()
                .expect("active classifier")
                .submit(request.clone())
                .map_err(|e| self.fail(cx, "classifier", e.detail))?
            {
                Submit::Submitted => {
                    self.ledger
                        .classification_submitted(&candidate.source, &request)
                        .map_err(|e| self.fail(cx, "ledger", e))?;
                    let mut transition = Transition::new(trace::FlowPhase::ClassificationStarted);
                    transition.source_id = Some(candidate.source.clone());
                    transition.request_id = Some(request.request_id.0);
                    self.notice(cx, io, transition, self.turn(&candidate.source))
                        .map_err(|e| self.fail(cx, e.point, e.detail))?;
                    return Ok(Advance::Progress);
                }
                Submit::Busy(_) => return Ok(Advance::Pending),
            }
        }
        let gauges = self.gauges(cx)?;
        if self
            .observations
            .as_mut()
            .expect("recorded open observation")
            .tick_due(now, gauges.work_pending())
            .map_err(|e| self.fail(cx, "observation", e))?
        {
            self.notice(cx, io, Transition::new(trace::FlowPhase::Sampled), None)
                .map_err(|e| self.fail(cx, e.point, e.detail))?;
            return Ok(Advance::Progress);
        }
        Ok(
            if gauges.work_pending()
                || self.state_operation.is_some()
                || self.check_operation.is_some()
                || self.capture.is_some()
            {
                Advance::Pending
            } else {
                Advance::Idle
            },
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn author(
        &mut self,
        cx: &mut FlowRecording<'_>,
        io: &mut impl PumpIo,
        wall_ms: u64,
        kind: Kind,
        turn: Option<String>,
        payload: Payload,
        purpose: RecordPurpose,
        pending: PendingRecord,
    ) -> Result<Advance, FlowFailure> {
        let event = AuthoredEvent {
            kind,
            subsystem: Subsystem::Harness,
            turn: turn.map(TurnRef),
            source: AuthoredSource::Runtime,
            causal_parent: None,
            payload: Some(payload),
        };
        self.pending_record = Some(pending);
        let progress = cx.barrier.submit_authored_with_clock(
            event,
            purpose,
            cx.author,
            wall_ms,
            cx.inbox,
            cx.budget,
            &mut || io.now(),
        );
        if matches!(
            &progress,
            RecordProgress::Busy { .. } | RecordProgress::BusyAuthored { .. }
        ) {
            return self.recorded(cx, io, progress);
        }
        self.drain(cx, io)?;
        self.recorded(cx, io, progress)
    }

    fn recorded(
        &mut self,
        cx: &mut FlowRecording<'_>,
        io: &mut impl PumpIo,
        progress: RecordProgress,
    ) -> Result<Advance, FlowFailure> {
        let proof = match progress {
            RecordProgress::Pending => return Ok(Advance::Pending),
            RecordProgress::Recorded {
                proof: Some(proof), ..
            } => proof,
            RecordProgress::Busy { event, purpose } => {
                self.refused_author = Some(RefusedAuthor::Prepared { event, purpose });
                return Err(self.fail(cx, "author", "flow author was returned busy"));
            }
            RecordProgress::BusyAuthored { event, purpose } => {
                self.refused_author = Some(RefusedAuthor::Authored { event, purpose });
                return Err(self.fail(cx, "author", "flow author was returned busy"));
            }
            RecordProgress::Failed | RecordProgress::Recorded { proof: None, .. } => {
                return Err(self.fail(cx, "author", "required flow record did not complete"));
            }
        };
        match (self.pending_record.take(), proof) {
            (Some(PendingRecord::Flow), RecordProof::Flow(proof)) => {
                let terminal = self
                    .observations
                    .as_mut()
                    .expect("recorded open observation")
                    .recorded(proof)
                    .map_err(|e| self.fail(cx, "observation", e))?;
                self.notices.pop_front();
                if let Some(terminal) = terminal {
                    self.ledger
                        .terminal_recorded(terminal, cx.budget)
                        .map_err(|e| self.fail(cx, "ledger", e))?;
                    if self.ledger.receipts().is_some_and(|set| set.is_complete())
                        && self
                            .receipt_reservation
                            .as_ref()
                            .is_some_and(|r| r.recorded_ns.is_none())
                    {
                        let recorded_ns =
                            io.now().map_err(|error| self.fail(cx, "clock", error))?;
                        self.receipt_reservation
                            .as_mut()
                            .expect("registered consumer")
                            .recorded_ns = Some(recorded_ns);
                    }
                    if let Some(IntakeOutcome::IntakeFailed { copy, detail }) =
                        cx.inbox.resume_waiting(cx.budget)
                    {
                        let _ = cx.inbox.preserve_failure(IntakeFailure { copy, detail });
                    }
                    self.drain(cx, io)?;
                }
            }
            (Some(PendingRecord::Classifier), RecordProof::Classifier(proof)) => {
                if let Some(CheckOperation::Input {
                    input_id,
                    offered_sequence,
                    ..
                }) = &self.check_operation
                {
                    let input_id = *input_id;
                    let offered_sequence = *offered_sequence;
                    // The completed proof is the gate's logical recording
                    // boundary. Sample before releasing or authoring any notice.
                    let recorded_ns = io.now().map_err(|error| self.fail(cx, "clock", error))?;
                    let accepted = matches!(
                        self.classifier().held().map(|reply| &reply.result),
                        Some(Ste100Result::Accepted { .. })
                    );
                    let (reply, reuse) = if accepted {
                        let reuse = self
                            .classifier
                            .as_mut()
                            .expect("held classifier")
                            .recorded_input(proof, offered_sequence)
                            .map_err(|error| self.fail(cx, "classifier", error.detail))?;
                        (reuse.reply().clone(), Some(reuse))
                    } else {
                        let reply = self
                            .classifier
                            .as_mut()
                            .expect("held classifier")
                            .recorded(proof)
                            .map_err(|error| self.fail(cx, "classifier", error.detail))?;
                        (reply, None)
                    };
                    if self.classifier().state() == ClassifierState::Failed {
                        return Err(self.fail(
                            cx,
                            "classifier",
                            "classifier reported component failure",
                        ));
                    }
                    if accepted {
                        let mut transition = Transition::new(trace::FlowPhase::Accepted);
                        transition.input_id = Some(input_id.0);
                        transition.request_id = Some(reply.request_id.0);
                        self.notice(cx, io, transition, None)?;
                    }
                    self.operation_completion = Some(OperationCompletion::Input {
                        operation: self.check_operation.take().expect("owned input operation"),
                        reply,
                        reuse,
                        recorded_ns,
                    });
                    return Ok(Advance::Progress);
                }
                let result_source = proof.source().clone();
                let reply = self
                    .classifier
                    .as_mut()
                    .expect("held classifier")
                    .recorded(proof)
                    .map_err(|e| self.fail(cx, "classifier", e.detail))?;
                if self.classifier().state() == ClassifierState::Failed {
                    return Err(self.fail(
                        cx,
                        "classifier",
                        "classifier reported component failure",
                    ));
                }
                if let Some(owner) = self.check_operation.take() {
                    self.operation_completion = Some(OperationCompletion::Check(RecordedCheck {
                        operation: owner,
                        reply,
                        source: result_source,
                    }));
                    return Ok(Advance::Progress);
                }
                match self
                    .ledger
                    .classify_recorded(&reply)
                    .map_err(|e| self.fail(cx, "ledger", e))?
                {
                    Classified::Accepted(source) => {
                        let mut transition = Transition::new(trace::FlowPhase::Accepted);
                        transition.source_id = Some(source.clone());
                        transition.request_id = Some(reply.request_id.0);
                        self.notice(cx, io, transition, self.turn(&source))
                            .map_err(|e| self.fail(cx, e.point, e.detail))?;
                    }
                    Classified::CorrectlyRejected(terminal) => {
                        self.terminal(cx, io, terminal, None, Some(reply.request_id.0))?
                    }
                }
            }
            (Some(PendingRecord::State), RecordProof::State(proof)) => {
                // One sample at the Recorded boundary survives a pending final
                // release probe and all later consumer delay.
                self.state_recorded_ns = Some(io.now().map_err(|e| self.fail(cx, "clock", e))?);
                self.state_release_timing = self
                    .state
                    .as_ref()
                    .expect("held state")
                    .held_measurement()
                    .map(|m| (m.harness_received_ns.0, m.harness_sent_ns.0));
                let completion = self
                    .state
                    .as_mut()
                    .expect("held state")
                    .release(proof)
                    .map_err(|e| self.fail(cx, "state_reply", e))?;
                return self.finish_state_release(cx, io, completion);
            }
            _ => return Err(self.fail(cx, "author", "record proof and pending transition differ")),
        }
        Ok(Advance::Progress)
    }

    fn finish_state_release(
        &mut self,
        cx: &mut FlowRecording<'_>,
        io: &mut impl PumpIo,
        completion: Option<StateCompletion>,
    ) -> Result<Advance, FlowFailure> {
        let Some(completion) = completion else {
            self.pending_record = Some(PendingRecord::StateRelease);
            return Ok(Advance::Pending);
        };
        self.pending_record = None;
        let timing = self.state_release_timing.take();
        let recorded_ns = self.state_recorded_ns.take();
        if self.open_pending {
            if !matches!(completion.answer, StateFrame::Opened { .. }) {
                return Err(self.fail(cx, "open", "open released a non-open completion"));
            }
            let (interval, alarm) = self
                .observation_config
                .take()
                .expect("preflight observation declaration");
            let confirmed = timing
                .ok_or_else(|| self.fail(cx, "open", "open measurement lost its timestamp"))?
                .0;
            self.observations = Some(
                Observation::new(self.run.clone(), confirmed, interval, alarm)
                    .map_err(|error| self.fail(cx, "observation", error))?,
            );
            self.open_pending = false;
            self.phase = CoordinatorPhase::Active;
            self.operation_completion = Some(OperationCompletion::Opened(completion));
            return Ok(Advance::Progress);
        }
        if self
            .capture
            .as_ref()
            .is_some_and(|owner| owner.leg.is_some())
        {
            let recorded_ns = recorded_ns
                .ok_or_else(|| self.fail(cx, "state_reply", "capture lost its recording time"))?;
            return self.finish_capture_release(cx, completion, recorded_ns);
        }
        if let Some((request, turn)) = self.state_operation.take() {
            self.operation_completion = Some(OperationCompletion::State {
                request,
                turn,
                completion,
            });
            return Ok(Advance::Progress);
        }
        let terminal = self
            .ledger
            .receipt_recorded(&completion.answer)
            .map_err(|e| self.fail(cx, "ledger", e))?;
        let elapsed = match timing {
            Some((received, sent)) => received
                .checked_sub(sent)
                .ok_or_else(|| self.fail(cx, "clock", "filing interval reversed"))?,
            None => return Err(self.fail(cx, "state_reply", "filing receipt lost its timing")),
        };
        self.terminal(
            cx,
            io,
            terminal,
            Some(elapsed),
            Some(completion.answer.common().request_id.0),
        )?;
        Ok(Advance::Progress)
    }

    fn finish_capture_release(
        &mut self,
        cx: &mut FlowRecording<'_>,
        mut completion: StateCompletion,
        recorded_ns: u64,
    ) -> Result<Advance, FlowFailure> {
        let owner = self.capture.as_ref().expect("owned capture leg");
        let request = owner.leg.as_ref().expect("owned capture request");
        if completion.discard_capture_result {
            if owner.snapshot.phase != CapturePhase::Aborting
                || owner.cleanup.is_none()
                || !matches!(
                    request,
                    StateControl::Snapshot { .. } | StateControl::Acknowledge { .. }
                )
            {
                return Err(self.fail(
                    cx,
                    "state_reply",
                    "discard does not name the cleanup's original leg",
                ));
            }
            let owner = self.capture.as_mut().expect("owned capture leg");
            owner.leg = owner.cleanup.take();
            return Ok(Advance::Progress);
        }
        let StateFrame::ControlAnswer { frame, .. } = &completion.answer else {
            return Err(self.fail(cx, "state_reply", "capture released a non-control answer"));
        };
        if owner.release_queued.is_some() {
            if !matches!((request, frame),
                (StateControl::Acknowledge { capture_id, .. }, StateControlAnswer::Finished { capture_id: id, .. })
                    if capture_id == id)
            {
                return Err(self.fail(
                    cx,
                    "state_reply",
                    "queued release lost its Recorded Finished",
                ));
            }
            completion.discard_capture_result = true;
            let owner = self.capture.as_mut().expect("queued release owner");
            self.operation_completion = Some(OperationCompletion::Capture {
                request: owner.leg.take().expect("recorded original request"),
                completion,
                recorded_ns,
            });
            return Ok(Advance::Progress);
        }
        let phase = match (request, frame) {
            (StateControl::Snapshot { .. }, StateControlAnswer::Snapshot { .. }) => {
                CapturePhase::Held
            }
            (StateControl::Acknowledge { .. }, StateControlAnswer::Finished { .. }) => {
                CapturePhase::Finished
            }
            (StateControl::Abort { .. }, StateControlAnswer::Aborted { .. })
            | (StateControl::ReleaseCapture { .. }, StateControlAnswer::CaptureReleased { .. })
            | (StateControl::Snapshot { .. }, StateControlAnswer::ControlRefused { .. }) => {
                CapturePhase::Complete
            }
            (StateControl::Acknowledge { .. }, StateControlAnswer::ControlRefused { .. }) => {
                CapturePhase::Held
            }
            (StateControl::ReleaseCapture { .. }, StateControlAnswer::ControlRefused { .. }) => {
                CapturePhase::Finished
            }
            _ => {
                return Err(self.fail(
                    cx,
                    "state_reply",
                    "capture answer differs from its owned leg",
                ));
            }
        };
        let owner = self.capture.as_mut().expect("owned capture leg");
        owner.refused_snapshot = matches!(
            (&owner.leg, frame),
            (
                Some(StateControl::Snapshot { .. }),
                StateControlAnswer::ControlRefused { .. }
            )
        );
        owner.snapshot.phase = phase;
        self.operation_completion = Some(OperationCompletion::Capture {
            request: owner.leg.take().expect("owned capture request"),
            completion,
            recorded_ns,
        });
        Ok(Advance::Progress)
    }

    fn terminal(
        &mut self,
        cx: &mut FlowRecording<'_>,
        io: &mut impl PumpIo,
        terminal: TerminalCandidate,
        elapsed_ns: Option<u64>,
        request_id: Option<u64>,
    ) -> Result<(), FlowFailure> {
        let phase = match terminal.outcome {
            TerminalOutcome::Filed => {
                self.observations
                    .as_mut()
                    .expect("recorded open observation")
                    .note_filed(terminal.bytes)
                    .map_err(|e| self.fail(cx, "counter", e))?;
                trace::FlowPhase::Filed
            }
            TerminalOutcome::CorrectlyRejected => {
                self.observations
                    .as_mut()
                    .expect("recorded open observation")
                    .note_rejected()
                    .map_err(|e| self.fail(cx, "counter", e))?;
                trace::FlowPhase::CorrectlyRejected
            }
        };
        let mut transition = Transition::new(phase);
        transition.source_id = Some(terminal.source);
        transition.elapsed_ns = elapsed_ns;
        transition.request_id = request_id;
        self.notice(cx, io, transition, terminal.turn)
            .map_err(|e| self.fail(cx, e.point, e.detail))
    }

    fn turn(&self, source: &SourceBlockId) -> Option<String> {
        self.ledger
            .facts()
            .find(|fact| fact.source == source)
            .and_then(|fact| fact.turn.map(str::to_owned))
    }

    fn gauges(&mut self, cx: &mut FlowRecording<'_>) -> Result<GaugeSnapshot, FlowFailure> {
        let waiting = cx
            .budget
            .waiting_copy()
            .map(copy_identity)
            .transpose()
            .map_err(|e| self.fail(cx, "copy_intake", e))?;
        let queued =
            queued_sources(cx.inbox).map_err(|error| self.fail(cx, "copy_intake", error))?;
        let result = GaugeSnapshot::from_sources(
            &self.run,
            self.ledger
                .facts()
                .filter(|fact| {
                    !matches!(
                        fact.state,
                        LedgerState::FiledHeld | LedgerState::CorrectlyRejectedHeld
                    )
                })
                .map(|fact| PendingSource {
                    source: fact.source,
                    turn: fact.turn,
                    raw_bytes: fact.raw_bytes,
                    context_bytes: fact.context_bytes,
                    intake_ns: fact.intake_ns,
                })
                .chain(queued.iter().map(|fact| PendingSource {
                    source: &fact.source,
                    turn: fact.turn.as_deref(),
                    raw_bytes: fact.raw_bytes,
                    context_bytes: fact.context_bytes,
                    intake_ns: fact.intake_ns,
                })),
            u64::from(self.input_account.is_some()),
            cx.budget.gauges(),
            waiting.as_ref().map(|(copy, turn)| WaitingSource {
                copy,
                turn: turn.as_deref(),
            }),
        );
        result.map_err(|e| self.fail(cx, "observation", e))
    }
}

impl FlowCoordinator<super::ste100_socket::SocketPort> {
    pub(super) fn begin_classifier_admission(
        &mut self,
        cx: &mut FlowRecording<'_>,
        descriptor: std::os::fd::OwnedFd,
        bootstrap: weaver_types::state_flow::Ste100Bootstrap,
    ) -> Result<(), FlowFailure> {
        self.observe_failures(cx)?;
        if !matches!(
            self.phase,
            CoordinatorPhase::Opening | CoordinatorPhase::Active
        ) || !cx.barrier.is_active()
            || self.classifier.is_some()
            || self.starting_classifier.is_some()
            || self.classifier_retired
        {
            return Err(self.fail(
                cx,
                "classifier",
                "classifier admission is once after trace activation",
            ));
        }
        let port =
            super::ste100_socket::SocketPort::starting(descriptor, bootstrap, self.scheme.clone())
                .map_err(|error| self.fail(cx, "classifier", error.detail))?;
        self.starting_classifier = Some(port);
        Ok(())
    }

    pub(super) fn admission_refused(&self) -> bool {
        self.admission_refused
    }

    pub(super) fn admission_needs_progress(&self) -> bool {
        self.admission_probe_pending
    }

    /// A conforming admission refusal owes intentional disposal, not a failure
    /// probe. Its peer must close after refusal, so EOF here is expected.
    pub(super) fn retire_refused_classifier(
        &mut self,
        cx: &mut FlowRecording<'_>,
    ) -> Result<(), FlowFailure> {
        self.observe_failures(cx)?;
        if !matches!(
            self.phase,
            CoordinatorPhase::Opening | CoordinatorPhase::Active
        ) || !self.admission_refused
            || self.admission_completion.is_some()
            || self.classifier_retired
            || self.classifier.is_some()
            || self.starting_classifier.is_none()
            || !self.clean_obligations(cx)
        {
            return Err(self.fail(
                cx,
                "classifier",
                "refused classifier disposal requires its consumed refusal",
            ));
        }
        self.starting_classifier.take();
        self.admission_probe_pending = false;
        self.classifier_retired = true;
        Ok(())
    }

    pub(super) fn held_admission(&self) -> Option<&super::ste100_socket::AdmissionPoll> {
        self.admission_completion.as_ref()
    }

    /// Transport retains the completion. Only take_admission_completion grants
    /// use of the admitted port. A valid refusal expects the peer to retire.
    pub(super) fn advance_classifier_admission(
        &mut self,
        cx: &mut FlowRecording<'_>,
    ) -> Result<AdmissionAdvance, FlowFailure> {
        self.observe_failures(cx)?;
        let Some(port) = &mut self.starting_classifier else {
            return Err(self.fail(cx, "classifier", "no classifier admission is outstanding"));
        };
        if self.admission_refused {
            return Ok(AdmissionAdvance::Pending);
        }
        if let Some(held) = &self.admission_completion {
            if matches!(held, super::ste100_socket::AdmissionPoll::Ready(_)) {
                match retirement_status(port.readiness()) {
                    Ok(false) => {
                        self.admission_probe_pending = true;
                        return Ok(AdmissionAdvance::Pending);
                    }
                    Ok(true) => self.admission_probe_pending = false,
                    Err(error) => return Err(self.fail(cx, "classifier", error)),
                }
            }
            return Ok(AdmissionAdvance::Held);
        }
        let result = port
            .advance_admission()
            .map_err(|error| self.fail(cx, "classifier", error.detail))?;
        if result == super::ste100_socket::AdmissionPoll::Pending {
            return Ok(AdmissionAdvance::Pending);
        }
        self.admission_completion = Some(result);
        Ok(AdmissionAdvance::Held)
    }

    pub(super) fn take_admission_completion(
        &mut self,
        cx: &mut FlowRecording<'_>,
    ) -> Result<Option<super::ste100_socket::AdmissionPoll>, FlowFailure> {
        self.observe_failures(cx)?;
        if cx.barrier.pending().is_some() {
            return Ok(None);
        }
        if matches!(
            self.admission_completion,
            Some(super::ste100_socket::AdmissionPoll::Ready(_))
        ) {
            let fd = self
                .starting_classifier
                .as_ref()
                .expect("admission owns its port")
                .readiness();
            match retirement_status(fd) {
                Ok(false) => {
                    self.admission_probe_pending = true;
                    return Ok(None);
                }
                Ok(true) => self.admission_probe_pending = false,
                Err(error) => return Err(self.fail(cx, "classifier", error)),
            }
        }
        let Some(result) = self.admission_completion.take() else {
            return Ok(None);
        };
        if matches!(result, super::ste100_socket::AdmissionPoll::Ready(_)) {
            let port = self
                .starting_classifier
                .take()
                .expect("admission owns its port");
            self.classifier = Some(
                Classifier::new(port, self.scheme.clone(), self.run.clone())
                    .map_err(|error| self.fail(cx, "classifier", error.detail))?,
            );
        }
        if matches!(result, super::ste100_socket::AdmissionPoll::Refused(_)) {
            self.admission_refused = true;
        }
        Ok(Some(result))
    }
}

/// Select reuse by its real offer correlation, never by inbox position.
fn matches_input_copy(
    copy: &weaver_trace::serving::AdmittedCopy,
    reuse: &InputReuse,
) -> io::Result<bool> {
    let weaver_trace::serving::AdmittedCopy::Statement(statement) = copy else {
        return Ok(false);
    };
    if statement.blocks.is_empty() {
        return Ok(false);
    }
    let source: SourceEnvelope =
        weaver_types::state_flow::decode_strict(&statement.source_envelope)
            .map_err(io::Error::other)?;
    if source.run != reuse.grant.run()
        || source.kind != "message.user"
        || source.causal_parent != Some(reuse.grant.offered_sequence())
    {
        return Ok(false);
    }
    if reuse
        .source_sequence
        .is_some_and(|sequence| sequence != source.sequence.0)
    {
        return Err(io::Error::other(
            "input copy differs from its recorded source",
        ));
    }
    Ok(true)
}

struct QueuedSource {
    source: SourceBlockId,
    turn: Option<String>,
    raw_bytes: u64,
    context_bytes: u64,
    intake_ns: u64,
}

/// A failure can occur after admission and before the ordinary inbox drain.
/// These are the same owed sources, with their actual original intake clocks.
fn queued_sources(inbox: &FlowInbox) -> io::Result<Vec<QueuedSource>> {
    use weaver_trace::serving::AdmittedCopy;
    let mut facts = Vec::new();
    for (copy, intake_ns) in inbox.queued_timed() {
        let (envelope, blocks, context) = match copy {
            AdmittedCopy::Statement(copy) => (
                &copy.source_envelope,
                copy.blocks.as_slice(),
                copy.context.as_ref(),
            ),
            AdmittedCopy::Pair(copy) => (
                &copy.source_envelope,
                std::slice::from_ref(&copy.block),
                None,
            ),
        };
        if blocks.is_empty() {
            continue;
        }
        let intake_ns =
            intake_ns.ok_or_else(|| io::Error::other("queued copy has no intake clock"))?;
        let source: SourceEnvelope =
            weaver_types::state_flow::decode_strict(envelope).map_err(io::Error::other)?;
        facts.try_reserve(blocks.len()).map_err(io::Error::other)?;
        for block in blocks {
            if block.source.run != source.run || block.source.sequence.0 != source.sequence.0 {
                return Err(io::Error::other("queued block differs from its envelope"));
            }
            let context_bytes = if block.source.block == 0 {
                context
                    .map_or(Ok(0), |context| u64::try_from(context.raw.len()))
                    .map_err(io::Error::other)?
            } else {
                0
            };
            facts.push(QueuedSource {
                source: SourceBlockId {
                    run: block.source.run.clone(),
                    sequence: block.source.sequence.0.into(),
                    block: block.source.block.into(),
                },
                turn: source.turn.clone(),
                raw_bytes: u64::try_from(block.raw.len()).map_err(io::Error::other)?,
                context_bytes,
                intake_ns,
            });
        }
    }
    Ok(facts)
}

/// A clean retirement still observes an already queued terminal/extra frame.
/// EINTR grants no permission to close and adds no wait or retry loop.
fn retirement_status(fd: std::os::fd::BorrowedFd<'_>) -> io::Result<bool> {
    use nix::libc;
    use std::os::fd::AsRawFd;
    let mut entry = libc::pollfd {
        fd: fd.as_raw_fd(),
        events: libc::POLLIN | libc::POLLRDHUP,
        revents: 0,
    };
    // SAFETY: entry is one writable pollfd and the owned descriptor stays borrowed.
    if unsafe { libc::poll(&mut entry, 1, 0) } < 0 {
        let error = io::Error::last_os_error();
        return if error.kind() == io::ErrorKind::Interrupted {
            Ok(false)
        } else {
            Err(error)
        };
    }
    if entry.revents != 0 {
        return Err(io::Error::other(
            "classifier has terminal or unsolicited work before retirement",
        ));
    }
    Ok(true)
}

fn pending_receipt_sources(
    cx: &FlowRecording<'_>,
    run: &str,
    sequence: Sequence,
    installed_empty: bool,
) -> io::Result<Option<Vec<SourceBlockId>>> {
    let mut pending = None;
    for copy in cx.inbox.queued().chain(cx.budget.waiting_copy()) {
        let (identity, _) = copy_identity(copy)?;
        if identity.source_event.run != run || identity.source_event.sequence.0 != sequence.0 {
            continue;
        }
        if pending.is_some() || !installed_empty {
            return Err(io::Error::other("author copy has duplicate custody"));
        }
        pending = Some(
            identity
                .sources
                .into_iter()
                .map(|source| SourceBlockId {
                    run: source.run,
                    sequence: source.sequence.0.into(),
                    block: source.block.0.into(),
                })
                .collect(),
        );
    }
    Ok(pending)
}

fn copy_contains(copy: &weaver_trace::serving::AdmittedCopy, source: &SourceBlockId) -> bool {
    super::intake::selected_blocks(copy).iter().any(|block| {
        block.source.run == source.run
            && block.source.sequence.0 == source.sequence.0
            && block.source.block == source.block.0
    })
}

fn copy_identity(
    copy: &weaver_trace::serving::AdmittedCopy,
) -> io::Result<(trace::CopyAtHalt, Option<String>)> {
    use weaver_trace::serving::AdmittedCopy;
    let (envelope, rule, content_type, blocks) = match copy {
        AdmittedCopy::Statement(copy) => (
            &copy.source_envelope,
            &copy.partition_rule_id,
            trace::CopyContentType::Statement,
            copy.blocks.as_slice(),
        ),
        AdmittedCopy::Pair(copy) => (
            &copy.source_envelope,
            &copy.partition_rule_id,
            trace::CopyContentType::Pair,
            std::slice::from_ref(&copy.block),
        ),
    };
    let source: SourceEnvelope = serde_json::from_str(envelope).map_err(io::Error::other)?;
    Ok((
        trace::CopyAtHalt {
            source_event: trace::CopySourceEvent {
                run: source.run,
                sequence: source.sequence.0.into(),
            },
            partition_rule_id: rule.clone(),
            content_type,
            sources: blocks
                .iter()
                .map(|block| trace::SourceBlockId {
                    run: block.source.run.clone(),
                    sequence: block.source.sequence.0.into(),
                    block: block.source.block.into(),
                })
                .collect(),
        },
        source.turn,
    ))
}

#[cfg(test)]
#[path = "coordinator_tests.rs"]
pub(in crate::flow) mod tests;

#[cfg(test)]
#[path = "coordinator_input_tests.rs"]
mod input_tests;

#[cfg(test)]
#[path = "coordinator_capture_tests.rs"]
mod capture_tests;
