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
use weaver_types::state_flow::{SourceBlockId, SourceEnvelope, Ste100Request, Ste100Target};

use super::author::{AuthoredEvent, AuthoredSource, ServingAuthor};
use super::channel::StateChannel;
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

    /// Read/write at most one bounded unit on each ready owner. No ledger or
    /// counter transition, authoring, or completion release can occur here.
    pub(super) fn transport(
        &mut self,
        cx: &mut FlowRecording<'_>,
        io: &mut impl PumpIo,
        state_ready: bool,
        classifier_ready: bool,
    ) -> Result<(), FlowFailure> {
        self.observe_failures(cx)?;
        if state_ready || self.state.ready_buffered() {
            self.state
                .progress(&mut || io.now())
                .map_err(|e| self.fail(cx, "state_transport", e))?;
        }
        if classifier_ready {
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
        if let Some(measurement) = self.state.held_measurement() {
            let measurement = measurement.clone();
            let turn = measurement
                .request_source
                .as_ref()
                .and_then(|source| self.turn(source));
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
                Ste100Target::Source(source) => self.turn(source),
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
        if self.state.can_begin_filing()
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
        if self.classifier.state() == ClassifierState::Idle
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
        Ok(if gauges.work_pending() {
            Advance::Pending
        } else {
            Advance::Idle
        })
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
        let now = io.now().map_err(|e| self.fail(cx, "clock", e))?;
        let event = cx
            .author
            .prepare(&event, now, wall_ms)
            .map_err(|e| self.fail(cx, "author", format!("{e:?}")))?;
        self.pending_record = Some(pending);
        let progress =
            cx.barrier
                .submit_with_clock(event, purpose, cx.inbox, cx.budget, &mut || io.now());
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
            RecordProgress::Busy { .. }
            | RecordProgress::Failed
            | RecordProgress::Recorded { proof: None, .. } => {
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
                let timing = self
                    .state
                    .held_measurement()
                    .map(|m| (m.harness_received_ns.0, m.harness_sent_ns.0));
                let completion = self
                    .state
                    .release(proof)
                    .map_err(|e| self.fail(cx, "state_reply", e))?;
                let terminal = self
                    .ledger
                    .receipt_recorded(&completion.answer)
                    .map_err(|e| self.fail(cx, "ledger", e))?;
                let elapsed = match timing {
                    Some((received, sent)) => Some(
                        received
                            .checked_sub(sent)
                            .ok_or_else(|| self.fail(cx, "clock", "filing interval reversed"))?,
                    ),
                    None => {
                        return Err(self.fail(cx, "state_reply", "filing receipt lost its timing"));
                    }
                };
                self.terminal(
                    cx,
                    io,
                    terminal,
                    elapsed,
                    Some(completion.answer.common().request_id.0),
                )?;
            }
            _ => return Err(self.fail(cx, "author", "record proof and pending transition differ")),
        }
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
