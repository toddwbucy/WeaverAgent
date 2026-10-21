//! Ordered trace-copy filing, composed from the sole transport owners.
//!
//! The recording context lends its author, barrier, inbox and retention budget.
//! Transport progress is separate from reduction: it can fill a held completion
//! while one trace ticket waits, but only reduction consumes a recording proof.
//! Lifecycle/gate/decode/legacy-label cursors join this owner at coupled cutover.

use std::collections::VecDeque;
use std::io;

use weaver_trace::serving::{Kind, Payload, payloads as trace};
use weaver_trace::{Subsystem, TurnRef};
use weaver_types::state_flow::{
    DecimalU64, SourceBlockId, SourceEnvelope, StateControl, Ste100Reply, Ste100Request,
    Ste100Target,
};

use super::author::{AuthoredEvent, AuthoredSource, ServingAuthor};
use super::channel::{StateChannel, StateCompletion, StateRequest};
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
/// selected-copy admission or manufactures a ledger entry. Gate input is
/// deliberately absent until its intake, cancellation and counter owner exists.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum CheckOperation {
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
            Self::Template { template_id, .. } => Ste100Target::Template {
                template_id: template_id.clone(),
            },
            Self::UnselectedModel { source, .. } => Ste100Target::Source(source.clone()),
        }
    }

    fn text(&self) -> &str {
        match self {
            Self::Template { text, .. } | Self::UnselectedModel { text, .. } => text,
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
    State {
        request: StateControl,
        turn: Option<TurnRef>,
        completion: StateCompletion,
    },
    Check {
        operation: CheckOperation,
        reply: Ste100Reply,
    },
}

#[derive(Clone)]
struct Notice {
    point: SamplePoint,
    turn: Option<String>,
}

#[derive(Clone, Copy)]
enum PendingRecord {
    Classifier,
    State,
    Flow,
    StateRelease,
}

/// No socket, mutable ledger or completion escapes through a caller-owned clone.
/// The later run-wide latch consumes failure() before granting a decision. This
/// owner freezes immediately and preserves all current source/transport evidence.
pub(super) struct FlowCoordinator<P> {
    run: String,
    ledger: Ledger,
    classifier: Classifier<P>,
    state: StateChannel,
    observations: Observation,
    notices: VecDeque<Notice>,
    pending_record: Option<PendingRecord>,
    failure: Option<FlowFailure>,
    backlog_reported: bool,
    started_file: Option<SourceBlockId>,
    reuse: Option<AcceptedReuse>,
    state_operation: Option<(StateControl, Option<TurnRef>)>,
    check_operation: Option<CheckOperation>,
    operation_completion: Option<OperationCompletion>,
    state_release_timing: Option<(u64, u64)>,
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
            classifier,
            state,
            observations,
            notices: VecDeque::new(),
            pending_record: None,
            failure: None,
            backlog_reported: false,
            started_file: None,
            reuse: None,
            state_operation: None,
            check_operation: None,
            operation_completion: None,
            state_release_timing: None,
            refused_author: None,
        })
    }

    pub(super) fn failure(&self) -> Option<&FlowFailure> {
        self.failure.as_ref()
    }
    pub(super) fn ledger(&self) -> &Ledger {
        &self.ledger
    }
    pub(super) fn state(&self) -> &StateChannel {
        &self.state
    }
    pub(super) fn classifier(&self) -> &Classifier<P> {
        &self.classifier
    }

    /// Begin a nonfiling exchange through the same request counter and socket.
    /// A busy owner returns the entire control unchanged and assigns no identity.
    pub(super) fn begin_control(
        &mut self,
        cx: &mut FlowRecording<'_>,
        io: &mut impl PumpIo,
        request: ControlOperation,
    ) -> Result<OperationStart<ControlOperation>, FlowFailure> {
        self.observe_failures(cx)?;
        if self.state_operation.is_some()
            || self.operation_completion.is_some()
            || !self.state.outstanding().is_empty()
            || self.state.held_measurement().is_some()
        {
            return Ok(OperationStart::Busy(request));
        }
        let now = io.now().map_err(|e| self.fail(cx, "clock", e))?;
        let (request, turn) = request.into_parts();
        self.state_operation = Some((request.clone(), turn));
        let common = self
            .state
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
        self.observe_failures(cx)?;
        if self.check_operation.is_some()
            || self.operation_completion.is_some()
            || self.classifier.state() != ClassifierState::Idle
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
            .next_request_id()
            .map_err(|e| self.fail(cx, "classifier", e.detail))?;
        let request = Ste100Request {
            request_id,
            target: operation.target(),
            text: operation.text().to_owned(),
            scheme_id: self.classifier.scheme().scheme_id.clone(),
        };
        self.check_operation = Some(operation);
        match self
            .classifier
            .submit(request)
            .map_err(|e| self.fail(cx, "classifier", e.detail))?
        {
            Submit::Submitted => Ok(OperationStart::Started { request_id }),
            Submit::Busy(_) => Ok(OperationStart::Busy(
                self.check_operation.take().expect("owned check"),
            )),
        }
    }

    pub(super) fn take_operation_completion(
        &mut self,
        cx: &mut FlowRecording<'_>,
    ) -> Result<Option<OperationCompletion>, FlowFailure> {
        self.observe_failures(cx)?;
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
        if self.failure.is_some() || self.reuse.is_some() {
            return Err(reuse);
        }
        self.reuse = Some(reuse);
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
            self.failure = Some(FlowFailure {
                point,
                detail: detail.to_string(),
            });
        }
        // Preserve the last transport fact even when the work unit failed.
        // This updates evidence only and never releases a completion or retries.
        if let Some(progress) = self
            .state
            .outstanding()
            .first()
            .filter(|p| p.source.is_some())
        {
            let _ = self
                .ledger
                .observe_progress(progress, self.state.first_sent_ns());
        }
        self.ledger.freeze();
        self.classifier.fail();
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
        let point = self
            .observations
            .capture(transition, gauges, now)
            .map_err(|e| self.fail(cx, "observation", e))?;
        self.notices.push_back(Notice { point, turn });
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
        let mut copied = Vec::new();
        while let Some((copy, intake_ns)) = cx.inbox.pop_timed(cx.budget) {
            let Some(intake_ns) = intake_ns else {
                let _ = cx.inbox.preserve_failure(IntakeFailure {
                    copy,
                    detail: IntakeError::ClockReadFailed,
                });
                return Err(self.fail(cx, "copy_intake", "copy has no intake clock"));
            };
            match self.ledger.install(copy, intake_ns, self.reuse.take()) {
                Ok(installed) => {
                    copied.try_reserve(1).map_err(|_| {
                        self.fail(cx, "counter", "copied transition reservation failed")
                    })?;
                    copied.push(installed.sources);
                }
                Err(failure) => {
                    let detail = failure.error.to_string();
                    self.reuse = failure.reuse;
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
            TransportSource::State => self
                .state
                .progress(&mut || io.now())
                .map_err(|e| self.fail(cx, "state_transport", e))?,
            TransportSource::Classifier => {
                if self.classifier.state() == ClassifierState::ResultHeld {
                    self.classifier
                        .check_held_terminal()
                        .map_err(|e| self.fail(cx, "classifier", e.detail))?;
                } else {
                    self.classifier
                        .try_result()
                        .map_err(|e| self.fail(cx, "classifier", e.detail))?;
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
        if state_ready || self.state.ready_buffered() {
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
        if matches!(self.pending_record, Some(PendingRecord::StateRelease)) {
            // The measurement is recorded, but its interrupted final probe must
            // not release a result across a later outstanding ordinary author.
            if cx.barrier.pending().is_some() {
                return Ok(Advance::Pending);
            }
            let completion = self
                .state
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
        if let Some(progress) = self
            .state
            .outstanding()
            .first()
            .filter(|p| p.source.is_some())
        {
            self.ledger
                .observe_progress(progress, self.state.first_sent_ns())
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
            if self.observations.pending().is_none() {
                self.observations
                    .prepare_captured(&notice.point)
                    .map_err(|e| self.fail(cx, "observation", e))?;
            }
            let sample = self
                .observations
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
        if let Some(measurement) = self.state.held_measurement() {
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
        if let Some(reply) = self.classifier.held() {
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
        if self.state_operation.is_none()
            && self.state.can_begin_filing()
            && let Some(filing) = self.ledger.next_file()
        {
            let source = filing.source().clone();
            let common = self
                .state
                .begin(filing.into_request(), now)
                .map_err(|e| self.fail(cx, "state_send", e))?;
            self.ledger
                .begin_file(&source, &common)
                .map_err(|e| self.fail(cx, "ledger", e))?;
            self.started_file = None;
            return Ok(Advance::Progress);
        }
        if self.check_operation.is_none()
            && self.classifier.state() == ClassifierState::Idle
            && let Some(candidate) = self.ledger.next_classification()
        {
            let request = Ste100Request {
                request_id: self
                    .classifier
                    .next_request_id()
                    .map_err(|e| self.fail(cx, "classifier", e.detail))?,
                target: Ste100Target::Source(candidate.source.clone()),
                text: candidate.text,
                scheme_id: self.classifier.scheme().scheme_id.clone(),
            };
            match self
                .classifier
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
                    .recorded(proof)
                    .map_err(|e| self.fail(cx, "observation", e))?;
                self.notices.pop_front();
                if let Some(terminal) = terminal {
                    self.ledger
                        .terminal_recorded(terminal, cx.budget)
                        .map_err(|e| self.fail(cx, "ledger", e))?;
                    if let Some(IntakeOutcome::IntakeFailed { copy, detail }) =
                        cx.inbox.resume_waiting(cx.budget)
                    {
                        let _ = cx.inbox.preserve_failure(IntakeFailure { copy, detail });
                    }
                    self.drain(cx, io)?;
                }
            }
            (Some(PendingRecord::Classifier), RecordProof::Classifier(proof)) => {
                let reply = self
                    .classifier
                    .recorded(proof)
                    .map_err(|e| self.fail(cx, "classifier", e.detail))?;
                if self.classifier.state() == ClassifierState::Failed {
                    return Err(self.fail(
                        cx,
                        "classifier",
                        "classifier reported component failure",
                    ));
                }
                if let Some(owner) = self.check_operation.take() {
                    self.operation_completion = Some(OperationCompletion::Check {
                        operation: owner,
                        reply,
                    });
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
                self.state_release_timing = self
                    .state
                    .held_measurement()
                    .map(|m| (m.harness_received_ns.0, m.harness_sent_ns.0));
                let completion = self
                    .state
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
                    .note_filed(terminal.bytes)
                    .map_err(|e| self.fail(cx, "counter", e))?;
                trace::FlowPhase::Filed
            }
            TerminalOutcome::CorrectlyRejected => {
                self.observations
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
                }),
            0,
            cx.budget.gauges(),
            waiting.as_ref().map(|(copy, turn)| WaitingSource {
                copy,
                turn: turn.as_deref(),
            }),
        );
        result.map_err(|e| self.fail(cx, "observation", e))
    }
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
mod tests;
