//! A record barrier mints release proofs only after real trace admission.
//! Capacity pressure retains one exact event. It does not reduce a port reply,
//! recursively author a measurement, or allow a second content event to pass.

use std::sync::Arc;

use weaver_trace::Sequence;
use weaver_trace::serving::{
    Event, Kind, Payload, Recorder, SubmitAttempt, TapFailure, TapOutcome,
};
use weaver_types::state_flow::{SourceBlockId, SourcePosition, StateStoreMeasurement, Ste100Reply};

use super::author::{AuthorError, AuthoredEvent, ServingAuthor};
use super::context::release_driver::{ReleaseAuthor, ReleaseScope, ReleaseTicket, shutdown_kind};
use super::intake::{FlowInbox, IntakeFailure, IntakeOutcome, RetentionBudget};
use weaver_trace::serving::payloads::{FlowObservation, FlowPhase};

/// None of these receipts is Clone or publicly constructible. In production,
/// only RecordBarrier's successful Recorded arm constructs one.
#[derive(Debug)]
pub(super) struct RecordedSte100 {
    source: SourcePosition,
    reply: Ste100Reply,
}

impl RecordedSte100 {
    pub(super) fn run(&self) -> &str {
        &self.source.block_id.run
    }
    pub(super) fn source(&self) -> &SourcePosition {
        &self.source
    }
    pub(super) fn matches(&self, reply: &Ste100Reply) -> bool {
        self.reply == *reply
    }
    #[cfg(test)]
    pub(super) fn for_test(reply: &Ste100Reply) -> Self {
        Self::for_test_in("run", reply)
    }
    #[cfg(test)]
    pub(super) fn for_test_in(run: &str, reply: &Ste100Reply) -> Self {
        Self {
            source: SourcePosition {
                block_id: SourceBlockId {
                    run: run.into(),
                    sequence: 1.into(),
                    block: 0.into(),
                },
                turn: None,
            },
            reply: reply.clone(),
        }
    }
}

#[derive(Debug)]
pub(super) struct RecordedStateMeasurement {
    run: String,
    measurement: StateStoreMeasurement,
}

impl RecordedStateMeasurement {
    pub(super) fn run(&self) -> &str {
        &self.run
    }
    pub(super) fn measurement(&self) -> &StateStoreMeasurement {
        &self.measurement
    }
    #[cfg(test)]
    pub(super) fn for_test(measurement: &StateStoreMeasurement) -> Self {
        Self::for_test_in("run", measurement)
    }
    #[cfg(test)]
    pub(super) fn for_test_in(run: &str, measurement: &StateStoreMeasurement) -> Self {
        Self {
            run: run.into(),
            measurement: measurement.clone(),
        }
    }
}

#[derive(Debug)]
pub(super) struct RecordedTerminal {
    source: SourceBlockId,
    phase: FlowPhase,
}

impl RecordedTerminal {
    pub(super) fn phase(&self) -> FlowPhase {
        self.phase
    }
    pub(super) fn source(&self) -> &SourceBlockId {
        &self.source
    }
    pub(super) fn into_source(self) -> SourceBlockId {
        self.source
    }
    #[cfg(test)]
    pub(super) fn for_test(source: SourceBlockId) -> Self {
        Self::for_test_phase(source, FlowPhase::Filed)
    }
    #[cfg(test)]
    pub(super) fn for_test_phase(source: SourceBlockId, phase: FlowPhase) -> Self {
        Self { source, phase }
    }
}

/// One exact flow sample crossed the trace barrier. The accounting owner must
/// match this sample before advancing its interval or releasing its charge.
#[derive(Debug)]
pub(super) struct RecordedFlow {
    run: String,
    sample: FlowObservation,
}

impl RecordedFlow {
    pub(super) fn run(&self) -> &str {
        &self.run
    }

    pub(super) fn sample(&self) -> &FlowObservation {
        &self.sample
    }

    pub(super) fn into_terminal(self) -> Option<RecordedTerminal> {
        if !matches!(
            self.sample.phase,
            FlowPhase::Filed | FlowPhase::CorrectlyRejected
        ) {
            return None;
        }
        self.sample.source_id.map(|source| RecordedTerminal {
            phase: self.sample.phase,
            source: SourceBlockId {
                run: source.run,
                sequence: source.sequence.0.into(),
                block: source.block.0.into(),
            },
        })
    }

    #[cfg(test)]
    pub(super) fn for_test(run: &str, sample: &FlowObservation) -> Self {
        Self {
            run: run.into(),
            sample: sample.clone(),
        }
    }
}

#[derive(Debug)]
pub(super) enum RecordPurpose {
    Content,
    Observation,
    Classifier(Ste100Reply),
    State(StateStoreMeasurement),
    Terminal(SourceBlockId),
    Flow(FlowObservation),
}

#[derive(Debug)]
pub(super) enum RecordProof {
    Classifier(RecordedSte100),
    State(RecordedStateMeasurement),
    Terminal(RecordedTerminal),
    Flow(RecordedFlow),
}

#[must_use]
#[derive(Debug)]
pub(super) enum RecordProgress {
    Recorded {
        sequence: Sequence,
        proof: Option<RecordProof>,
    },
    Pending,
    Busy {
        event: Event,
        purpose: RecordPurpose,
    },
    BusyAuthored {
        event: AuthoredEvent,
        purpose: RecordPurpose,
        owed: Option<OwedAuthor>,
    },
    Failed,
}

/// An existing account owns release admission; this is not a trace failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OwedAuthor {
    BootOpen,
    Placement,
}

#[derive(Debug)]
pub(super) enum RecordFailure {
    Protocol(String),
    Author(AuthorError),
    Clock(String),
    Trace(weaver_trace::Failure),
    Tap(TapFailure),
    Intake(super::intake::IntakeError),
}

/// Identity belongs to the author attempt, not the trace. The original source
/// includes facts (such as the peer UID) which preparation deliberately reduces.
#[derive(Debug)]
pub(super) struct AuthorTicket {
    pub ticket: u64,
    pub event: AuthoredEvent,
    // The release mark cannot outlive the original author it authorizes.
    release: Option<ReleaseTicket>,
}

#[derive(Debug)]
pub(super) struct TerminalAuthorFailure {
    pub event: AuthoredEvent,
    pub detail: String,
}

/// This object owns the only ordinary serving-recorder handle. The later
/// recording context supplies author attribution, drains the inbox and owns
/// the halt latch. This component never turns a failure into a success token.
pub(super) struct RecordBarrier {
    recorder: super::record_owner::RunRecorder,
    run: String,
    pending: Option<(Event, RecordPurpose)>,
    pending_author: Option<AuthorTicket>,
    author_identity: Arc<()>,
    boot_open_owed: bool,
    place_owed: bool,
    next_ticket: Option<u64>,
    admitted_before_failure: Option<Sequence>,
    last_trace_only_sequence: Option<Sequence>,
    failure: Option<RecordFailure>,
}

impl RecordBarrier {
    pub(super) fn new(recorder: Recorder, run: String) -> Self {
        Self::with_owner(super::record_owner::RunRecorder::active(recorder), run)
    }

    pub(super) fn preflight(
        recorder: weaver_trace::serving::PreflightRecorder,
        run: String,
    ) -> Self {
        Self::with_owner(super::record_owner::RunRecorder::preflight(recorder), run)
    }

    fn with_owner(recorder: super::record_owner::RunRecorder, run: String) -> Self {
        Self {
            recorder,
            run,
            pending: None,
            pending_author: None,
            author_identity: Arc::new(()),
            boot_open_owed: false,
            place_owed: false,
            next_ticket: Some(0),
            admitted_before_failure: None,
            last_trace_only_sequence: None,
            failure: None,
        }
    }

    pub(super) fn is_loaded(&self) -> bool {
        self.recorder.is_loaded()
    }
    pub(super) fn run(&self) -> &str {
        &self.run
    }
    pub(super) fn is_active(&self) -> bool {
        self.recorder.is_active()
    }

    pub(super) fn is_refused(&self) -> bool {
        self.recorder.is_refused()
    }
    pub(super) fn is_closed(&self) -> bool {
        self.recorder.is_closed()
    }

    pub(super) fn activate(
        &mut self,
        partition: weaver_trace::serving::ValidatedPartition,
    ) -> Result<(), weaver_trace::Failure> {
        if self.failure.is_some() || self.pending.is_some() || self.pending_author.is_some() {
            return Err(weaver_trace::Failure::RefusedOnSubmit {
                reason: weaver_trace::SubmitRefusal::PayloadMalformed,
            });
        }
        self.recorder.activate(partition)
    }

    pub(super) fn standing_failure(&self) -> Option<weaver_trace::Failure> {
        self.recorder.standing_failure()
    }

    pub(super) fn abandon(&mut self) {
        self.recorder.abandon();
    }

    /// Prepare once and retain the complete original until actual admission.
    /// Busy returns the new original intact and never replaces the first ticket.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn submit_authored_with_clock(
        &mut self,
        event: AuthoredEvent,
        purpose: RecordPurpose,
        author: &ServingAuthor,
        wall_ms: u64,
        inbox: &mut FlowInbox,
        budget: &mut RetentionBudget,
        now: &mut impl FnMut() -> std::io::Result<u64>,
    ) -> RecordProgress {
        self.submit_authored_inner(event, purpose, None, author, wall_ms, inbox, budget, now)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn submit_release_authored_with_clock(
        &mut self,
        permit: ReleaseAuthor<'_>,
        purpose: RecordPurpose,
        author: &ServingAuthor,
        wall_ms: u64,
        inbox: &mut FlowInbox,
        budget: &mut RetentionBudget,
        now: &mut impl FnMut() -> std::io::Result<u64>,
    ) -> RecordProgress {
        let (scope, event) = permit.into_parts();
        self.submit_authored_inner(
            event,
            purpose,
            Some(&scope),
            author,
            wall_ms,
            inbox,
            budget,
            now,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn submit_authored_inner(
        &mut self,
        event: AuthoredEvent,
        purpose: RecordPurpose,
        release: Option<&ReleaseScope<'_>>,
        author: &ServingAuthor,
        wall_ms: u64,
        inbox: &mut FlowInbox,
        budget: &mut RetentionBudget,
        now: &mut impl FnMut() -> std::io::Result<u64>,
    ) -> RecordProgress {
        if let Some(scope) = release {
            if let Some(owed) = self.owed_author() {
                return RecordProgress::BusyAuthored {
                    event,
                    purpose,
                    owed: Some(owed),
                };
            }
            if !scope.allows(&self.author_identity)
                || !shutdown_kind(event.kind)
                || !matches!(purpose, RecordPurpose::Observation)
            {
                self.remember_failure(
                    RecordFailure::Protocol("invalid release author scope".into()),
                    budget,
                );
                return RecordProgress::BusyAuthored {
                    event,
                    purpose,
                    owed: None,
                };
            }
        }
        // Unload is held by an owed boot or placement account, not a competing author.
        // Return its original intact without poisoning the still-live barrier.
        if event.kind == Kind::Unload
            && let Some(owed) = self.owed_author()
        {
            return RecordProgress::BusyAuthored {
                event,
                purpose,
                owed: Some(owed),
            };
        }
        if self.pending.is_some() || self.pending_author.is_some() {
            self.remember_failure(
                RecordFailure::Protocol("second author while a ticket is pending".into()),
                budget,
            );
            return RecordProgress::BusyAuthored {
                event,
                purpose,
                owed: None,
            };
        }
        if let Some(failure) = inbox.failure() {
            self.remember_failure(RecordFailure::Intake(failure.detail), budget);
        }
        if self.failure.is_some()
            || budget.is_frozen()
            || (budget.waiting_copy().is_some() && matches!(purpose, RecordPurpose::Content))
        {
            return RecordProgress::BusyAuthored {
                event,
                purpose,
                owed: None,
            };
        }
        let Some(ticket) = self.next_ticket else {
            self.remember_failure(
                RecordFailure::Protocol("author ticket overflow".into()),
                budget,
            );
            return RecordProgress::BusyAuthored {
                event,
                purpose,
                owed: None,
            };
        };
        self.next_ticket = ticket.checked_add(1);
        let release_mark = release.map(|scope| scope.ticket(ticket, event.kind));
        self.pending_author = Some(AuthorTicket {
            ticket,
            event,
            release: release_mark,
        });
        let raw_ns = match now() {
            Ok(value) => value,
            Err(error) => return self.fail(RecordFailure::Clock(error.to_string()), budget),
        };
        let original = &self.pending_author.as_ref().expect("owned original").event;
        let prepared = match author.prepare(original, raw_ns, wall_ms) {
            Ok(event) => event,
            Err(error) => return self.fail(RecordFailure::Author(error), budget),
        };
        self.pending = Some((prepared, purpose));
        self.resume_inner(release, inbox, budget, now)
    }

    pub(super) fn submit(
        &mut self,
        event: Event,
        purpose: RecordPurpose,
        inbox: &mut FlowInbox,
        budget: &mut RetentionBudget,
    ) -> RecordProgress {
        self.submit_with_clock(
            event,
            purpose,
            inbox,
            budget,
            &mut super::clock::monotonic_ns,
        )
    }

    pub(super) fn submit_with_clock(
        &mut self,
        event: Event,
        purpose: RecordPurpose,
        inbox: &mut FlowInbox,
        budget: &mut RetentionBudget,
        now: &mut impl FnMut() -> std::io::Result<u64>,
    ) -> RecordProgress {
        if event.envelope.kind == Kind::Unload && self.owed_author().is_some() {
            return RecordProgress::Busy { event, purpose };
        }
        // Busy returns ownership before admission. Observation progress remains
        // possible while a whole selected copy waits for its source allowance.
        if self.failure.is_none()
            && let Some(failure) = inbox.failure()
        {
            self.remember_failure(RecordFailure::Intake(failure.detail), budget);
        }
        // A second author during a pending ticket is misuse, not backpressure.
        // Keep both events: the first in pending, the second in the return.
        if self.pending.is_some() && self.failure.is_none() {
            self.remember_failure(
                RecordFailure::Protocol("second author while a ticket is pending".into()),
                budget,
            );
            return RecordProgress::Busy { event, purpose };
        }
        if self.failure.is_some()
            || inbox.failure().is_some()
            || budget.is_frozen()
            || (budget.waiting_copy().is_some() && matches!(purpose, RecordPurpose::Content))
        {
            return RecordProgress::Busy { event, purpose };
        }
        self.pending = Some((event, purpose));
        self.resume_with_clock(inbox, budget, now)
    }

    pub(super) fn resume(
        &mut self,
        inbox: &mut FlowInbox,
        budget: &mut RetentionBudget,
    ) -> RecordProgress {
        self.resume_with_clock(inbox, budget, &mut super::clock::monotonic_ns)
    }

    pub(super) fn resume_with_clock(
        &mut self,
        inbox: &mut FlowInbox,
        budget: &mut RetentionBudget,
        now: &mut impl FnMut() -> std::io::Result<u64>,
    ) -> RecordProgress {
        self.resume_inner(None, inbox, budget, now)
    }

    pub(super) fn resume_release_with_clock(
        &mut self,
        scope: &ReleaseScope<'_>,
        inbox: &mut FlowInbox,
        budget: &mut RetentionBudget,
        now: &mut impl FnMut() -> std::io::Result<u64>,
    ) -> RecordProgress {
        self.resume_inner(Some(scope), inbox, budget, now)
    }

    fn resume_inner(
        &mut self,
        release: Option<&ReleaseScope<'_>>,
        inbox: &mut FlowInbox,
        budget: &mut RetentionBudget,
        now: &mut impl FnMut() -> std::io::Result<u64>,
    ) -> RecordProgress {
        if self.failure.is_some() {
            return RecordProgress::Failed;
        }
        if let Some(failure) = inbox.failure() {
            return self.fail(RecordFailure::Intake(failure.detail), budget);
        }
        if budget.is_frozen() {
            return self.fail(
                RecordFailure::Protocol("author resumed after retention froze".into()),
                budget,
            );
        }
        let Some((event, purpose)) = self.pending.as_ref() else {
            return self.fail(
                RecordFailure::Protocol("resume has no pending author".into()),
                budget,
            );
        };
        if event.envelope.run.0 != self.run || !matches_purpose(event, purpose) {
            return self.fail(
                RecordFailure::Protocol("trace event differs from its release obligation".into()),
                budget,
            );
        }
        let release_mark = self
            .pending_author
            .as_ref()
            .and_then(|held| held.release.as_ref());
        let released = match (release_mark, release) {
            (Some(ticket), Some(scope))
                if ticket.matches(
                    scope,
                    &self.author_identity,
                    self.pending_author.as_ref(),
                    event,
                    purpose,
                ) =>
            {
                true
            }
            (None, None) => false,
            _ => {
                return self.fail(
                    RecordFailure::Protocol("release author lost its exact scope or ticket".into()),
                    budget,
                );
            }
        };
        if released && let Some(owed) = self.owed_author() {
            // The exact scope and ticket were validated first. CapacityPending
            // never admitted a sequence; return both preparations' ownership
            // together instead of retaining a detached release mark or renewing
            // a wait. Public owner admission currently prevents this debt rise.
            let (_, purpose) = self.pending.take().expect("validated pending event");
            let original = self
                .pending_author
                .take()
                .expect("validated release ticket");
            return RecordProgress::BusyAuthored {
                event: original.event,
                purpose,
                owed: Some(owed),
            };
        }
        if self.recorder.tap_stopped() && !released {
            return self.fail(
                RecordFailure::Protocol("ordinary author has no live release scope".into()),
                budget,
            );
        }
        // Retain the exact pending author for the halt account even if the
        // recorder rejects it. The clone shares raw text where its type permits.
        let attempt = self.recorder.submit(event.clone());
        match attempt {
            Err(failure) => self.fail(RecordFailure::Trace(failure), budget),
            Ok(SubmitAttempt::CapacityPending(event)) => {
                self.pending.as_mut().expect("pending event").0 = event;
                RecordProgress::Pending
            }
            Ok(SubmitAttempt::Recorded(submission)) => {
                let (recorded_event, purpose) = self.pending.take().expect("pending event");
                self.pending_author = None;
                // Failure below is after trace admission. It must not be
                // reported as an unadmitted author in the halt account.
                self.admitted_before_failure = Some(submission.sequence);
                match submission.tap {
                    TapOutcome::Selected(copy) => match match now() {
                        Ok(intake_ns) => inbox.accept_at(copy, budget, Some(intake_ns)),
                        Err(_) => IntakeOutcome::IntakeFailed {
                            copy,
                            detail: super::intake::IntakeError::ClockReadFailed,
                        },
                    } {
                        IntakeOutcome::Accepted | IntakeOutcome::Deferred => {}
                        IntakeOutcome::IntakeFailed { copy, detail } => {
                            // A failure is terminal, so this slot cannot already
                            // hold another failure produced by this recorder.
                            let kept = inbox.preserve_failure(IntakeFailure { copy, detail });
                            assert!(
                                kept.is_ok(),
                                "record owner must stop after first intake failure"
                            );
                            return self.fail(RecordFailure::Intake(detail), budget);
                        }
                    },
                    TapOutcome::Failed(failure) => {
                        return self.fail(RecordFailure::Tap(failure), budget);
                    }
                    TapOutcome::HeadUntapped if !self.recorder.is_active() => {}
                    TapOutcome::Stopped if self.recorder.is_preflight_shutdown() => {}
                    TapOutcome::Stopped if released => {}
                    TapOutcome::Stopped | TapOutcome::HeadUntapped => {
                        return self.fail(
                            RecordFailure::Protocol("ordinary record has no live tap".into()),
                            budget,
                        );
                    }
                    TapOutcome::Unselected => {}
                }
                let proof = match purpose {
                    RecordPurpose::Content | RecordPurpose::Observation => None,
                    RecordPurpose::Classifier(reply) => {
                        Some(RecordProof::Classifier(RecordedSte100 {
                            source: SourcePosition {
                                block_id: SourceBlockId {
                                    run: recorded_event.envelope.run.0.clone(),
                                    sequence: submission.sequence.0.into(),
                                    block: 0.into(),
                                },
                                turn: recorded_event
                                    .envelope
                                    .turn
                                    .as_ref()
                                    .map(|turn| turn.0.clone()),
                            },
                            reply,
                        }))
                    }
                    RecordPurpose::State(measurement) => {
                        Some(RecordProof::State(RecordedStateMeasurement {
                            run: self.run.clone(),
                            measurement,
                        }))
                    }
                    RecordPurpose::Terminal(source) => {
                        let Some(Payload::FlowObservation(sample)) = recorded_event.payload else {
                            unreachable!("validated terminal payload")
                        };
                        Some(RecordProof::Terminal(RecordedTerminal {
                            source,
                            phase: sample.phase,
                        }))
                    }
                    RecordPurpose::Flow(sample) => Some(RecordProof::Flow(RecordedFlow {
                        run: self.run.clone(),
                        sample,
                    })),
                };
                self.admitted_before_failure = None;
                RecordProgress::Recorded {
                    sequence: submission.sequence,
                    proof,
                }
            }
        }
    }

    fn fail(&mut self, failure: RecordFailure, budget: &mut RetentionBudget) -> RecordProgress {
        self.remember_failure(failure, budget);
        RecordProgress::Failed
    }

    fn remember_failure(&mut self, failure: RecordFailure, budget: &mut RetentionBudget) {
        if self.failure.is_none() {
            self.failure = Some(failure);
        }
        self.recorder.stop_tap();
        budget.freeze();
    }

    pub(super) fn failure(&self) -> Option<&RecordFailure> {
        self.failure.as_ref()
    }
    pub(super) fn set_place_owed(&mut self, owed: bool) {
        self.place_owed = owed;
    }
    // Match context admission's precedence if defensive debt flags overlap.
    fn owed_author(&self) -> Option<OwedAuthor> {
        if self.place_owed {
            Some(OwedAuthor::Placement)
        } else if self.boot_open_owed {
            Some(OwedAuthor::BootOpen)
        } else {
            None
        }
    }
    pub(super) fn set_boot_open_owed(&mut self, owed: bool) {
        self.boot_open_owed = owed;
    }

    pub(super) fn boot_open_owed(&self) -> bool {
        self.boot_open_owed
    }

    pub(super) fn stop_for_flow_failure(&mut self) {
        self.recorder.stop_tap();
    }
    pub(super) fn pending(&self) -> Option<&Event> {
        self.pending.as_ref().map(|p| &p.0)
    }
    pub(super) fn pending_author(&self) -> Option<&AuthorTicket> {
        self.pending_author.as_ref()
    }
    pub(super) fn author_identity(&self) -> Arc<()> {
        Arc::clone(&self.author_identity)
    }
    #[cfg(test)]
    pub(super) fn replace_pending_author_ticket_for_test(&mut self) -> u64 {
        let ticket = self.next_ticket.expect("fixture ticket capacity");
        self.next_ticket = ticket.checked_add(1);
        self.pending_author
            .as_mut()
            .expect("real pending author")
            .ticket = ticket;
        ticket
    }
    pub(super) fn admitted_before_failure(&self) -> Option<Sequence> {
        self.admitted_before_failure
    }
    /// Latest ordinary admission after tap stop, excluding terminal reserves.
    pub(super) fn last_trace_only_sequence(&self) -> Option<Sequence> {
        self.last_trace_only_sequence
    }

    /// Only the halt owner calls this after stopping the tap. Reserved evidence
    /// bypasses an unadmitted ordinary author without taking or replacing it.
    // Return the original event inline on failure, without requiring another
    // allocation on the terminal reporting path.
    #[allow(clippy::result_large_err)]
    pub(super) fn submit_terminal_authored(
        &mut self,
        event: AuthoredEvent,
        author: &ServingAuthor,
        raw_ns: u64,
        wall_ms: u64,
    ) -> Result<Sequence, TerminalAuthorFailure> {
        self.submit_terminal_authored_ref(&event, author, raw_ns, wall_ms)
            .map_err(|detail| TerminalAuthorFailure { event, detail })
    }

    /// Borrow an original which its halt owner already retains on both outcomes.
    pub(super) fn submit_terminal_authored_ref(
        &mut self,
        event: &AuthoredEvent,
        author: &ServingAuthor,
        raw_ns: u64,
        wall_ms: u64,
    ) -> Result<Sequence, String> {
        if event.kind == Kind::Unload
            && let Some(owed) = self.owed_author()
        {
            return Err(match owed {
                OwedAuthor::BootOpen => "boot Open account must settle before unload",
                OwedAuthor::Placement => "placement account must settle before unload",
            }
            .into());
        }
        author
            .prepare(event, raw_ns, wall_ms)
            .map_err(|e| format!("terminal author: {e:?}"))
            .and_then(|prepared| {
                self.recorder
                    .submit_terminal(prepared)
                    .map_err(|e| format!("terminal trace: {e:?}"))
            })
            .map(|submission| submission.sequence)
    }

    /// A refusal already observed when serving stopped still owes a trace
    /// record. Queue its original and prepared event without a copy, a release
    /// proof or a reserved terminal permit. Only trace-only drain may resume it.
    #[allow(clippy::result_large_err)]
    pub(super) fn queue_trace_only_authored(
        &mut self,
        event: AuthoredEvent,
        author: &ServingAuthor,
        raw_ns: u64,
        wall_ms: u64,
    ) -> Result<(), TerminalAuthorFailure> {
        if !self.recorder.tap_stopped()
            || self.recorder.is_closed()
            || self.pending.is_some()
            || self.pending_author.is_some()
            || event.kind != Kind::Refusal
            || !matches!(event.source, super::author::AuthoredSource::Runtime)
            || !matches!(event.payload, Some(Payload::Refusal(_)))
        {
            return Err(TerminalAuthorFailure {
                event,
                detail: "trace-only author requires a stopped tap and an empty refusal slot".into(),
            });
        }
        let Some(ticket) = self.next_ticket else {
            return Err(TerminalAuthorFailure {
                event,
                detail: "author ticket overflow".into(),
            });
        };
        let prepared = match author.prepare(&event, raw_ns, wall_ms) {
            Ok(prepared) => prepared,
            Err(error) => {
                return Err(TerminalAuthorFailure {
                    event,
                    detail: format!("trace-only author: {error:?}"),
                });
            }
        };
        self.next_ticket = ticket.checked_add(1);
        self.pending_author = Some(AuthorTicket {
            ticket,
            event,
            release: None,
        });
        self.pending = Some((prepared, RecordPurpose::Observation));
        Ok(())
    }

    #[allow(clippy::result_large_err)] // Keep the sole original on preparation refusal.
    pub(super) fn queue_boot_open_account(
        &mut self,
        permit: super::context::boot_open_driver::OwedBootAuthor,
        author: &ServingAuthor,
        raw_ns: u64,
        wall_ms: u64,
    ) -> Result<(), TerminalAuthorFailure> {
        let event = permit.into_event();
        if !self.boot_open_owed
            || !self.recorder.tap_stopped()
            || self.recorder.is_closed()
            || self.pending.is_some()
            || self.pending_author.is_some()
        {
            return Err(TerminalAuthorFailure {
                event,
                detail: "boot account requires its stopped, empty ordinary slot".into(),
            });
        }
        let Some(ticket) = self.next_ticket else {
            return Err(TerminalAuthorFailure {
                event,
                detail: "author ticket overflow".into(),
            });
        };
        let prepared = match author.prepare(&event, raw_ns, wall_ms) {
            Ok(prepared) => prepared,
            Err(error) => {
                return Err(TerminalAuthorFailure {
                    event,
                    detail: format!("boot account author: {error:?}"),
                });
            }
        };
        self.next_ticket = ticket.checked_add(1);
        self.pending_author = Some(AuthorTicket {
            ticket,
            event,
            release: None,
        });
        self.pending = Some((prepared, RecordPurpose::Observation));
        Ok(())
    }

    pub(super) fn queue_plan_recall_account(
        &mut self,
        permit: &super::context::plan_summary_driver::OwedPlanRecall,
        author: &ServingAuthor,
        raw_ns: u64,
        wall_ms: u64,
    ) -> Result<(), String> {
        if !self.recorder.tap_stopped()
            || self.recorder.is_closed()
            || self.pending.is_some()
            || self.pending_author.is_some()
        {
            return Err("summary account requires its stopped, empty ordinary slot".into());
        }
        let event = permit.event();
        let ticket = self.next_ticket.ok_or("author ticket overflow")?;
        let prepared = author
            .prepare(event, raw_ns, wall_ms)
            .map_err(|error| format!("summary recall author: {error:?}"))?;
        self.next_ticket = ticket.checked_add(1);
        self.pending_author = Some(AuthorTicket {
            ticket,
            event: event.clone(),
            release: None,
        });
        self.pending = Some((prepared, RecordPurpose::Observation));
        Ok(())
    }

    pub(super) fn retain_summary_rollback_record(
        &mut self,
        permit: &super::context::plan_summary_driver::PlanReadRollbackPermit,
    ) -> Result<StoppedReadRecord, &'static str> {
        if !permit.matches_record(self) {
            return Err("summary rollback lost its exact record");
        }
        Ok(StoppedReadRecord {
            _pending: self.pending.take(),
            _author: self.pending_author.take(),
        })
    }

    /// Release this owner's unadmitted ticket on bounded incomplete teardown.
    pub(super) fn abandon_boot_author(&mut self, ticket: u64) -> bool {
        if self
            .pending_author
            .as_ref()
            .is_some_and(|held| held.ticket == ticket)
        {
            self.pending = None;
            self.pending_author = None;
            true
        } else {
            false
        }
    }

    /// An ordinary event already owed to the trace may land after the halt.
    /// It receives no completion proof, reservation or state copy.
    pub(super) fn resume_trace_only(&mut self) -> Result<Option<Sequence>, weaver_trace::Failure> {
        if !self.recorder.tap_stopped() {
            return Err(weaver_trace::Failure::RefusedOnSubmit {
                reason: weaver_trace::SubmitRefusal::PayloadMalformed,
            });
        }
        let Some((event, purpose)) = self.pending.as_ref() else {
            return Ok(None);
        };
        if event.envelope.run.0 != self.run || !matches_purpose(event, purpose) {
            return Err(weaver_trace::Failure::RefusedOnSubmit {
                reason: weaver_trace::SubmitRefusal::PayloadMalformed,
            });
        }
        match self.recorder.submit(event.clone())? {
            SubmitAttempt::CapacityPending(_) => Ok(None),
            SubmitAttempt::Recorded(submission) => {
                self.pending = None;
                self.pending_author = None;
                self.last_trace_only_sequence = Some(submission.sequence);
                Ok(Some(submission.sequence))
            }
        }
    }
    #[cfg(test)]
    pub(super) fn recorder(&self) -> &Recorder {
        self.recorder.active_recorder()
    }
    pub(super) fn capacity_fd(&self) -> std::os::fd::BorrowedFd<'_> {
        self.recorder.capacity_fd()
    }
    pub(super) fn drain_capacity_wake(&mut self) -> Result<(), weaver_trace::Failure> {
        self.recorder.drain_capacity_wake()
    }

    pub(super) fn try_capacity_wake(
        &mut self,
        budget: &mut RetentionBudget,
    ) -> Result<weaver_trace::serving::CapacityWake, weaver_trace::Failure> {
        self.recorder.try_capacity_wake().inspect_err(|failure| {
            self.remember_failure(RecordFailure::Trace(failure.clone()), budget);
        })
    }

    pub(super) fn drain_status(
        &mut self,
        budget: &mut RetentionBudget,
    ) -> Result<weaver_trace::serving::DrainStatus, weaver_trace::Failure> {
        self.recorder.drain_status().inspect_err(|failure| {
            self.remember_failure(RecordFailure::Trace(failure.clone()), budget);
        })
    }
}

fn matches_purpose(event: &Event, purpose: &RecordPurpose) -> bool {
    use weaver_trace::serving::payloads::FlowPhase;
    match (purpose, &event.payload) {
        (RecordPurpose::Content, _) => !is_completion(event.envelope.kind),
        (RecordPurpose::Observation, _) => is_completion(event.envelope.kind),
        (RecordPurpose::Classifier(reply), Some(Payload::Ste100Result(record))) => {
            event.envelope.kind == Kind::Ste100Result
                && matches!((serde_json::to_value(record), serde_json::to_value(reply)),
                    (Ok(record), Ok(reply)) if record == reply)
        }
        (RecordPurpose::State(measurement), Some(Payload::StateStoreMeasurement(record))) => {
            event.envelope.kind == Kind::StateStoreMeasurement
                && measurement.validate().is_ok()
                && matches!((serde_json::to_value(record), serde_json::to_value(measurement)),
                    (Ok(record), Ok(measurement)) if record == measurement)
        }
        (RecordPurpose::Terminal(source), Some(Payload::FlowObservation(record))) => {
            event.envelope.kind == Kind::StateFlow
                && source.run == event.envelope.run.0
                && matches!(
                    record.phase,
                    FlowPhase::Filed | FlowPhase::CorrectlyRejected
                )
                && record.source_id.as_ref().is_some_and(|id| {
                    id.run == source.run
                        && id.sequence.0 == source.sequence.0
                        && id.block.0 == source.block.0
                })
        }
        (RecordPurpose::Flow(sample), Some(Payload::FlowObservation(record))) => {
            event.envelope.kind == Kind::StateFlow
                && record.as_ref() == sample
                && sample
                    .source_id
                    .as_ref()
                    .is_none_or(|source| source.run == event.envelope.run.0)
        }
        _ => false,
    }
}

fn is_completion(kind: Kind) -> bool {
    // The declaration loader forbids the registry's non-content kinds. This
    // permits all excluded completion evidence, not just the three proof kinds.
    weaver_trace::serving::content_schema(kind) == weaver_trace::serving::ContentSchema::None
}

#[cfg(test)]
#[path = "recording_tests.rs"]
mod tests;

/// Incomplete trace evidence retains prepared bytes and author identity. It has
/// no admission sequence and cannot be submitted again after rollback.
pub(super) struct StoppedReadRecord {
    _pending: Option<(Event, RecordPurpose)>,
    _author: Option<AuthorTicket>,
}

impl RecordBarrier {
    #[allow(clippy::result_large_err)]
    pub(super) fn queue_place_account(
        &mut self,
        permit: super::context::place_driver::OwedPlaceAuthor,
        author: &ServingAuthor,
        raw_ns: u64,
        wall_ms: u64,
    ) -> Result<(), TerminalAuthorFailure> {
        let event = permit.into_event();
        if !self.place_owed
            || !self.recorder.tap_stopped()
            || self.recorder.is_closed()
            || self.pending.is_some()
            || self.pending_author.is_some()
        {
            return Err(TerminalAuthorFailure {
                event,
                detail: "place account requires its stopped empty author slot".into(),
            });
        }
        let Some(ticket) = self.next_ticket else {
            return Err(TerminalAuthorFailure {
                event,
                detail: "author ticket overflow".into(),
            });
        };
        let prepared = match author.prepare(&event, raw_ns, wall_ms) {
            Ok(p) => p,
            Err(error) => {
                return Err(TerminalAuthorFailure {
                    event,
                    detail: format!("place account author: {error:?}"),
                });
            }
        };
        self.next_ticket = ticket.checked_add(1);
        self.pending_author = Some(AuthorTicket {
            ticket,
            event,
            release: None,
        });
        self.pending = Some((prepared, RecordPurpose::Observation));
        Ok(())
    }
}
