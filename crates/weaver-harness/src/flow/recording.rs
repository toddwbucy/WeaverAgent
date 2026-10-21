//! A record barrier mints release proofs only after real trace admission.
//! Capacity pressure retains one exact event. It does not reduce a port reply,
//! recursively author a measurement, or allow a second content event to pass.

use weaver_trace::Sequence;
use weaver_trace::serving::{
    Event, Kind, Payload, Recorder, SubmitAttempt, TapFailure, TapOutcome,
};
use weaver_types::state_flow::{SourceBlockId, StateStoreMeasurement, Ste100Reply};

use super::intake::{FlowInbox, IntakeFailure, IntakeOutcome, RetentionBudget};

/// None of these receipts is Clone or publicly constructible. In production,
/// only RecordBarrier's successful Recorded arm constructs one.
#[derive(Debug)]
pub(super) struct RecordedSte100 {
    run: String,
    reply: Ste100Reply,
}

impl RecordedSte100 {
    pub(super) fn run(&self) -> &str {
        &self.run
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
            run: run.into(),
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
}

impl RecordedTerminal {
    pub(super) fn into_source(self) -> SourceBlockId {
        self.source
    }
    #[cfg(test)]
    pub(super) fn for_test(source: SourceBlockId) -> Self {
        Self { source }
    }
}

#[derive(Debug)]
pub(super) enum RecordPurpose {
    Content,
    Observation,
    Classifier(Ste100Reply),
    State(StateStoreMeasurement),
    Terminal(SourceBlockId),
}

#[derive(Debug)]
pub(super) enum RecordProof {
    Classifier(RecordedSte100),
    State(RecordedStateMeasurement),
    Terminal(RecordedTerminal),
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
    Failed,
}

#[derive(Debug)]
pub(super) enum RecordFailure {
    Protocol(String),
    Trace(weaver_trace::Failure),
    Tap(TapFailure),
    Intake(super::intake::IntakeError),
}

/// This object owns the only ordinary serving-recorder handle. The later
/// recording context supplies author attribution, drains the inbox and owns
/// the halt latch. This component never turns a failure into a success token.
pub(super) struct RecordBarrier {
    recorder: Recorder,
    run: String,
    pending: Option<(Event, RecordPurpose)>,
    failure: Option<RecordFailure>,
}

impl RecordBarrier {
    pub(super) fn new(recorder: Recorder, run: String) -> Self {
        Self {
            recorder,
            run,
            pending: None,
            failure: None,
        }
    }

    pub(super) fn submit(
        &mut self,
        event: Event,
        purpose: RecordPurpose,
        inbox: &mut FlowInbox,
        budget: &mut RetentionBudget,
    ) -> RecordProgress {
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
        self.resume(inbox, budget)
    }

    pub(super) fn resume(
        &mut self,
        inbox: &mut FlowInbox,
        budget: &mut RetentionBudget,
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
                let (_, purpose) = self.pending.take().expect("pending event");
                match submission.tap {
                    TapOutcome::Selected(copy) => match inbox.accept(copy, budget) {
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
                            run: self.run.clone(),
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
                        Some(RecordProof::Terminal(RecordedTerminal { source }))
                    }
                };
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
    pub(super) fn pending(&self) -> Option<&Event> {
        self.pending.as_ref().map(|p| &p.0)
    }
    pub(super) fn recorder(&self) -> &Recorder {
        &self.recorder
    }
    pub(super) fn capacity_fd(&self) -> std::os::fd::BorrowedFd<'_> {
        self.recorder.capacity_fd()
    }
    pub(super) fn drain_capacity_wake(&mut self) -> Result<(), weaver_trace::Failure> {
        self.recorder.drain_capacity_wake()
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
