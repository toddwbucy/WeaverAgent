//! The run's record, one shape over two mechanisms, per `weaver-harness-Spec`
//! section 9's settled election and `weaver-agent-PRD` section 6 as ruled
//! 2026-08-24: a serving run authors the trace through `weaver-trace` and a
//! diagnostic run authors a diagnostic-trace through `weaver-diagnostic`,
//! this crate the sole writer under either kind and the kind selecting the
//! arm. The two writers mirror each other's receive and submit and share no
//! type, per `weaver-diagnostic-Spec` section 5, so the fork lives here, at
//! the one submit road, and the authorship path does not fork per site.

use weaver_trace::{Event, Kind, Pressure, Sequence, Subsystem, WorkingStructure};

/// The run's record: its arm, constructed once at the enter from the
/// binding's kind and never re-selected, a run's record being one mechanism
/// for its whole residency, and the latch of a failed authoring.
pub struct Record {
    arm: Arm,
    failed: bool,
}

/// The record's arm.
enum Arm {
    Serving(weaver_trace::Recorder),
    Diagnostic(weaver_diagnostic::Recorder),
}

/// Why a submission failed, in the arm's own vocabulary. The two writers'
/// failure sets are each their own, per the shared-shape-unshared-types
/// election, so this carries whichever arm answered rather than translating
/// one closed set into the other.
#[derive(Debug)]
pub enum RecordFailure {
    Serving(weaver_trace::Failure),
    Diagnostic(weaver_diagnostic::Failure),
    /// The event's kind or subsystem has no diagnostic counterpart. The
    /// diagnostic record holds nineteen kinds, per `weaver-diagnostic-Spec`
    /// section 3, and an authoring that reaches this arm with a kind outside
    /// them is a defect in the caller: the run brackets are the replay's
    /// own, tools do not execute, and nothing classifies under a replay. An
    /// elision does map since 2026-09-26, the replay reproducing a source's.
    OutsideDiagnosticVocabulary,
}

/// A serving run's record, through `weaver-trace`.
impl From<weaver_trace::Recorder> for Record {
    fn from(recorder: weaver_trace::Recorder) -> Self {
        Record {
            arm: Arm::Serving(recorder),
            failed: false,
        }
    }
}

/// A diagnostic run's record, through `weaver-diagnostic`.
impl From<weaver_diagnostic::Recorder> for Record {
    fn from(recorder: weaver_diagnostic::Recorder) -> Self {
        Record {
            arm: Arm::Diagnostic(recorder),
            failed: false,
        }
    }
}

impl Record {
    /// One event submitted through whichever mechanism the run holds. The
    /// event arrives in the serving vocabulary because the authorship path
    /// composes one form, and the diagnostic arm converts at this one site:
    /// the envelope member for member, the kind by its shared spelling, and
    /// the payload as the rendered bytes it would have carried, spliced,
    /// which the mirror's field-for-field claim licenses.
    pub fn submit(&mut self, event: Event) -> Result<Sequence, RecordFailure> {
        let submitted = match &mut self.arm {
            Arm::Serving(recorder) => recorder.submit(event).map_err(RecordFailure::Serving),
            Arm::Diagnostic(recorder) => convert(event).and_then(|converted| {
                recorder
                    .submit(converted)
                    .map(|sequence| Sequence(sequence.0))
                    .map_err(RecordFailure::Diagnostic)
            }),
        };
        self.latch(submitted)
    }

    /// **Whether an authoring has failed on this record** (#116; the
    /// Planner's ruling of 2026-10-09, `weaver-harness-Spec` section 4): set
    /// by any submission or drain that answered `Err` and never lowered. A
    /// record the run cannot serve on without is authored or service ends, so
    /// the serve loop reads this at every wake and at the enter, and a turn's
    /// close reads it before its answer is announced. It covers what the
    /// writer's own standing failure does not: a submission refused on its
    /// own (`RefusedOnSubmit`, `OutsideDiagnosticVocabulary`), which leaves
    /// the writer sound and the record missing that event, the diagnostic
    /// arm, whose drain is vacuous, and an event whose payload would not
    /// render ([`Record::latch_unrendered`]). A serving write that fails
    /// behind the queue is met at the next submission or drain, nothing on a
    /// turn path waiting on the sink. A refusal of that kind is this crate's
    /// authoring defect, and the run ending on it, naming it, is the rule
    /// working: the record's ending is the evidence.
    pub fn failed(&self) -> bool {
        self.failed
    }

    /// The latch set on a failed answer, which passes through unchanged.
    fn latch<T>(&mut self, answer: Result<T, RecordFailure>) -> Result<T, RecordFailure> {
        if answer.is_err() {
            self.failed = true;
        }
        answer
    }

    /// The working structure, the serving mechanism's alone: the diagnostic
    /// writer holds no RAM copy by contract, a replay's present being the
    /// holdings the replay ask answers, per `weaver-diagnostic-Spec`
    /// section 5. Callers on the frame path may expect it, no frame
    /// arriving under a diagnostic binding.
    pub fn structure(&self) -> Option<&WorkingStructure> {
        match &self.arm {
            Arm::Serving(recorder) => Some(recorder.structure()),
            Arm::Diagnostic(_) => None,
        }
    }

    /// The commit queue's pressure. The diagnostic writer is synchronous
    /// and queues nothing, so its arm is never over the mark.
    pub fn pressure(&self) -> Pressure {
        match &self.arm {
            Arm::Serving(recorder) => recorder.pressure(),
            Arm::Diagnostic(_) => Pressure {
                queued: 0,
                over_mark: false,
            },
        }
    }

    /// One diagnostic-native event submitted, the replay's own kinds:
    /// composed in the diagnostic vocabulary because they exist in no
    /// serving one. The serving arm refuses - a serving record has no
    /// replay bracket to hold.
    pub fn submit_diagnostic(
        &mut self,
        event: weaver_diagnostic::Event,
    ) -> Result<weaver_diagnostic::Sequence, RecordFailure> {
        let submitted = match &mut self.arm {
            Arm::Serving(_) => Err(RecordFailure::OutsideDiagnosticVocabulary),
            Arm::Diagnostic(recorder) => recorder.submit(event).map_err(RecordFailure::Diagnostic),
        };
        self.latch(submitted)
    }

    /// The commit queue's drain at the leave. The diagnostic writer is
    /// synchronous, every admitted event on the stream at its submit's
    /// return, so its arm has nothing to wait on and drains vacuously.
    pub fn drain(&mut self) -> Result<(), RecordFailure> {
        let drained = match &mut self.arm {
            Arm::Serving(recorder) => recorder.drain().map_err(RecordFailure::Serving),
            Arm::Diagnostic(_) => Ok(()),
        };
        self.latch(drained)
    }

    /// [`Record::drain`] by a deadline, the leave's shared one
    /// (`weaver-harness-Spec` section 6): `Expired` names what the writer had
    /// not taken by it. The diagnostic arm drains vacuously.
    pub fn drain_by(
        &mut self,
        deadline: std::time::Instant,
    ) -> Result<weaver_trace::Drained, RecordFailure> {
        let drained = match &mut self.arm {
            Arm::Serving(recorder) => recorder.drain_by(deadline).map_err(RecordFailure::Serving),
            Arm::Diagnostic(_) => Ok(weaver_trace::Drained::Emptied),
        };
        self.latch(drained)
    }

    /// **Abandon the writer** (#117; `weaver-trace-Spec` section 7), after
    /// the leave's last bounded drain expired: its queued records are
    /// forfeited and the recorder's drop detaches the writer. The diagnostic
    /// writer is synchronous and has nothing to abandon.
    pub fn abandon(&mut self) {
        if let Arm::Serving(recorder) = &mut self.arm {
            recorder.abandon();
        }
    }

    /// The serving recorder where that is the arm, read-only, for the tests
    /// that read the structure back.
    pub fn serving(&self) -> Option<&weaver_trace::Recorder> {
        match &self.arm {
            Arm::Serving(recorder) => Some(recorder),
            Arm::Diagnostic(_) => None,
        }
    }

    /// **The tee attached to the serving arm**, the one thing this crate does
    /// to the recorder outside its submit road; answers whether it was
    /// attached, the diagnostic arm holding no tee. No `&mut` recorder is
    /// handed out (#116), so no submission or drain can pass the latch.
    pub fn attach_tee(&mut self, tee: weaver_trace::Tee) -> bool {
        match &mut self.arm {
            Arm::Serving(recorder) => {
                recorder.attach_tee(tee);
                true
            }
            Arm::Diagnostic(_) => false,
        }
    }

    /// **An event dropped before it reached the submit road is latched as
    /// failed** (#116): an authoring whose payload would not render is a
    /// record missing that event as surely as one the recorder refused.
    pub fn latch_unrendered(&mut self) {
        self.failed = true;
    }
}

/// The serving-vocabulary event rendered into the diagnostic one, member
/// for member. The sixteen shared kinds map by spelling, per
/// `weaver-diagnostic-Spec` section 3.2, and the payload crosses as the
/// bytes it would have carried, spliced whole.
fn convert(event: Event) -> Result<weaver_diagnostic::Event, RecordFailure> {
    let kind = match event.envelope.kind {
        Kind::TurnStarted => weaver_diagnostic::Kind::TurnStarted,
        Kind::TurnClosed => weaver_diagnostic::Kind::TurnClosed,
        Kind::MessageSystem => weaver_diagnostic::Kind::MessageSystem,
        Kind::MessageUser => weaver_diagnostic::Kind::MessageUser,
        Kind::MessageAssistant => weaver_diagnostic::Kind::MessageAssistant,
        Kind::MessageToolResult => weaver_diagnostic::Kind::MessageToolResult,
        Kind::ModelRequest => weaver_diagnostic::Kind::ModelRequest,
        Kind::ModelOutput => weaver_diagnostic::Kind::ModelOutput,
        Kind::ModelMeasurement => weaver_diagnostic::Kind::ModelMeasurement,
        Kind::ModelField => weaver_diagnostic::Kind::ModelField,
        Kind::Flush => weaver_diagnostic::Kind::Flush,
        Kind::Refusal => weaver_diagnostic::Kind::Refusal,
        Kind::Fault => weaver_diagnostic::Kind::Fault,
        Kind::Recall => weaver_diagnostic::Kind::Recall,
        Kind::Elision => weaver_diagnostic::Kind::Elision,
        // A re-fed path's unrecovered call, the diagnostic record's twentieth
        // kind, per `weaver-diagnostic-Spec` section 3.2 (#31).
        Kind::ModelUnrecoveredCall => weaver_diagnostic::Kind::ModelUnrecoveredCall,
        // A restored message is seated by a serving enter, and a diagnostic
        // enter seats no prefix, per `weaver-diagnostic-Spec` section 3.2.
        Kind::Load
        | Kind::Unload
        | Kind::SessionClosed
        | Kind::MessageRestored
        // A replay scores nothing: the task's verdict is the serving run's.
        | Kind::Score
        // A diagnostic binding takes no save point, per `weaver-harness-Spec`
        // section 6.
        | Kind::SavePoint
        | Kind::ToolCallStarted
        | Kind::ToolCallCompleted
        | Kind::ClassifyRequest
        | Kind::ClassifyOutput => {
            return Err(RecordFailure::OutsideDiagnosticVocabulary);
        }
    };
    let subsystem = match event.envelope.subsystem {
        Subsystem::Harness => weaver_diagnostic::Subsystem::Harness,
        Subsystem::Spu => weaver_diagnostic::Subsystem::Spu,
        Subsystem::SpuDecoder => weaver_diagnostic::Subsystem::SpuDecoder,
        _ => return Err(RecordFailure::OutsideDiagnosticVocabulary),
    };
    let payload = match event.payload {
        None => None,
        Some(payload) => {
            let rendered = serde_json::to_string(&payload)
                .map_err(|_| RecordFailure::OutsideDiagnosticVocabulary)?;
            let raw = serde_json::value::RawValue::from_string(rendered)
                .map_err(|_| RecordFailure::OutsideDiagnosticVocabulary)?;
            Some(weaver_diagnostic::Payload::Spliced(raw))
        }
    };
    Ok(weaver_diagnostic::Event {
        envelope: weaver_diagnostic::Envelope {
            session: weaver_diagnostic::SessionRef(event.envelope.session.0),
            run: weaver_diagnostic::RunRef(event.envelope.run.0),
            turn: event.envelope.turn.map(|t| weaver_diagnostic::TurnRef(t.0)),
            // The recorder assigns the run-scoped sequence, as the sibling
            // does.
            sequence: weaver_diagnostic::Sequence(0),
            kind,
            subsystem,
            causal_parent: event
                .envelope
                .causal_parent
                .map(|s| weaver_diagnostic::Sequence(s.0)),
            wall_ms: event.envelope.wall_ms,
            monotonic_ns: weaver_diagnostic::MonotonicNs(event.envelope.monotonic_ns.0),
        },
        payload,
    })
}

#[cfg(test)]
mod tests;
