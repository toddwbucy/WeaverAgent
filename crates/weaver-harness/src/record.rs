//! The run's record, one shape over two mechanisms, per `weaver-harness-Spec`
//! section 9's settled election and `weaver-agent-PRD` section 6 as ruled
//! 2026-08-24: a serving run authors the trace through `weaver-trace` and a
//! diagnostic run authors a diagnostic-trace through `weaver-diagnostic`,
//! this crate the sole writer under either kind and the kind selecting the
//! arm. The two writers mirror each other's receive and submit and share no
//! type, per `weaver-diagnostic-Spec` section 5, so the fork lives here, at
//! the one submit road, and the authorship path does not fork per site.

use weaver_trace::{Event, Kind, Pressure, Sequence, Subsystem, WorkingStructure};

// The replacement lifetime has one home even while the legacy path remains
// active until the qualified foundation cutover.
#[allow(unused_imports)]
pub(crate) use crate::flow::context::RecordingContext;

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
/// one closed set into the other. The converter also uses the diagnostic
/// submission-refusal vocabulary for a malformed compatible event before writing.
#[derive(Debug)]
pub enum RecordFailure {
    Serving(weaver_trace::Failure),
    Diagnostic(weaver_diagnostic::Failure),
    /// The event's kind, payload or subsystem has no diagnostic counterpart. The
    /// diagnostic record holds twenty kinds, per `weaver-diagnostic-Spec`
    /// section 3, and an authoring that reaches this arm with a kind outside
    /// them is a defect in the caller: the run brackets are the replay's
    /// own, tools do not execute, and nothing classifies under a replay. An
    /// elision does map since 2026-09-26, the replay reproducing a source's.
    OutsideDiagnosticVocabulary,
    /// A diagnostic-only entry was called on the run's fixed serving arm.
    /// Neither writer was called and the run's record mechanism did not change.
    DiagnosticDoorOnServingRecord,
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
    /// composes one form. The diagnostic arm checks the legacy kind/payload
    /// pairing before copying its envelope and splicing its exact payload.
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

    /// Shared authoring in the replacement trace vocabulary, admitted only
    /// to a diagnostic binding. DIAGNOSTIC-CONVERT keeps its legacy schema:
    /// new kinds and payloads refuse before the diagnostic writer is called.
    /// This door neither selects the replacement serving recorder nor sends
    /// a serving event through the legacy recorder or around its barrier.
    pub fn submit_shared_diagnostic(
        &mut self,
        event: weaver_trace::serving::Event,
    ) -> Result<Sequence, RecordFailure> {
        let submitted = match &mut self.arm {
            Arm::Serving(_) => Err(RecordFailure::DiagnosticDoorOnServingRecord),
            Arm::Diagnostic(recorder) => convert_shared(event).and_then(|converted| {
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
                peak: 0,
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
            Arm::Serving(_) => Err(RecordFailure::DiagnosticDoorOnServingRecord),
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

/// The legacy event rendered into the diagnostic one. The sixteen shared
/// kinds map by spelling and explicitly licensed payload variant. The
/// converter preserves the supplied session, run and sequence. On submission the
/// diagnostic writer replaces those three with its own identity and next sequence,
/// which are the values carried by the emitted record.
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
    if !weaver_trace::payload_pairing_licensed(event.envelope.kind, event.payload.as_ref()) {
        let refusal = match event.payload {
            None => weaver_diagnostic::SubmitRefusal::RequiredFieldAbsent {
                field: weaver_diagnostic::FieldName("payload".into()),
            },
            Some(_) => weaver_diagnostic::SubmitRefusal::PayloadKindMismatch,
        };
        return Err(diagnostic_refusal(refusal));
    }
    let subsystem = match event.envelope.subsystem {
        Subsystem::Harness => weaver_diagnostic::Subsystem::Harness,
        Subsystem::Spu => weaver_diagnostic::Subsystem::Spu,
        Subsystem::SpuDecoder => weaver_diagnostic::Subsystem::SpuDecoder,
        _ => return Err(RecordFailure::OutsideDiagnosticVocabulary),
    };
    let payload = match event.payload {
        None => None,
        Some(payload) => {
            use weaver_trace::Payload as P;
            let raw = match payload {
                P::Message(raw)
                | P::Fault(raw)
                | P::ModelRequest(raw)
                | P::ModelMeasurement(raw)
                | P::Refusal(raw)
                | P::UnrecoveredCall(raw) => raw,
                structured => serde_json::value::to_raw_value(&structured).map_err(|_| {
                    diagnostic_refusal(weaver_diagnostic::SubmitRefusal::PayloadMalformed)
                })?,
            };
            Some(weaver_diagnostic::Payload::Spliced(raw))
        }
    };
    Ok(weaver_diagnostic::Event {
        envelope: weaver_diagnostic::Envelope {
            session: weaver_diagnostic::SessionRef(event.envelope.session.0),
            run: weaver_diagnostic::RunRef(event.envelope.run.0),
            turn: event.envelope.turn.map(|t| weaver_diagnostic::TurnRef(t.0)),
            sequence: weaver_diagnostic::Sequence(event.envelope.sequence.0),
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

fn diagnostic_refusal(refusal: weaver_diagnostic::SubmitRefusal) -> RecordFailure {
    RecordFailure::Diagnostic(weaver_diagnostic::Failure::SubmitRefused { refusal })
}

/// DIAGNOSTIC-CONVERT's envelope mapping discards only channel and origin.
/// Keep this destructure exhaustive so a new envelope field needs a judgment.
/// Payloads cross through typed legacy counterparts, never a schema-erasing
/// JSON conversion. The shared legacy checker then verifies their pairing.
fn convert_shared(
    event: weaver_trace::serving::Event,
) -> Result<weaver_diagnostic::Event, RecordFailure> {
    use weaver_trace::serving::{Kind as K, Payload as P};
    let weaver_trace::serving::Envelope {
        session,
        run,
        turn,
        sequence,
        kind,
        channel: _,
        origin: _,
        subsystem,
        causal_parent,
        wall_ms,
        monotonic_ns,
    } = event.envelope;
    let kind = match kind {
        K::TurnStarted => Kind::TurnStarted,
        K::TurnClosed => Kind::TurnClosed,
        K::MessageSystem => Kind::MessageSystem,
        K::MessageUser => Kind::MessageUser,
        K::MessageAssistant => Kind::MessageAssistant,
        K::MessageToolResult => Kind::MessageToolResult,
        K::ModelRequest => Kind::ModelRequest,
        K::ModelOutput => Kind::ModelOutput,
        K::ModelMeasurement => Kind::ModelMeasurement,
        K::ModelField => Kind::ModelField,
        K::Flush => Kind::Flush,
        K::Refusal => Kind::Refusal,
        K::Fault => Kind::Fault,
        K::Elision => Kind::Elision,
        K::ModelUnrecoveredCall => Kind::ModelUnrecoveredCall,
        // These kinds never belonged to the diagnostic vocabulary. All
        // unloads refuse, including either or both TRACE-HALT extensions.
        K::Load
        | K::Unload
        | K::SessionClosed
        | K::MessageRestored
        | K::Score
        | K::SavePoint
        | K::ToolCallStarted
        | K::ToolCallCompleted
        | K::ClassifyRequest
        | K::ClassifyOutput
        // Serving recall has no legacy RecallAccount payload variant.
        | K::Recall
        | K::ToolReturnRejected
        | K::LoopInstruction
        | K::SortDecision
        | K::StateSortMeasurement
        | K::StepOutcome
        | K::PlanEvent
        | K::PlanBreak
        | K::StatePlanMeasurement
        | K::StateMark
        | K::StateMarkPosition
        | K::StatePlacement
        | K::InputOffered
        | K::StateFlow
        | K::StateStoreMeasurement
        | K::Ste100Result => return Err(RecordFailure::OutsideDiagnosticVocabulary),
    };
    let payload = match event.payload {
        None => None,
        Some(payload) => Some(match payload {
            P::Message(value) => weaver_trace::Payload::Message(value),
            P::TurnClosed(value) => weaver_trace::Payload::TurnClosed(value),
            P::Fault(value) => weaver_trace::Payload::Fault(value),
            P::ModelRequest(value) => weaver_trace::Payload::ModelRequest(value),
            P::ModelOutput(value) => weaver_trace::Payload::ModelOutput(value),
            P::ModelField(value) => weaver_trace::Payload::ModelField(value),
            P::Flush(value) => weaver_trace::Payload::Flush(value),
            P::Elision(value) => weaver_trace::Payload::Elision(value),
            P::Refusal(value) => weaver_trace::Payload::Refusal(value),
            P::ModelMeasurement(value) => weaver_trace::Payload::ModelMeasurement(value),
            P::UnrecoveredCall(value) => weaver_trace::Payload::UnrecoveredCall(value),
            P::Unload(_)
            | P::Elections(_)
            | P::ClassifyRequest(_)
            | P::ClassifyOutput(_)
            | P::Score(_)
            | P::PairStarted(_)
            | P::PairEnded(_)
            | P::RejectedReturn(_)
            | P::LoopInstruction(_)
            | P::SortDecision(_)
            | P::SortMeasurement(_)
            | P::StepOutcome(_)
            | P::PlanInput(_)
            | P::PlanEffect(_)
            | P::PlanBreak(_)
            | P::PlanMeasurement(_)
            | P::StateMark(_)
            | P::MarkPosition(_)
            | P::Placement(_)
            | P::InputOffered(_)
            | P::FlowObservation(_)
            | P::StateStoreMeasurement(_)
            | P::Ste100Result(_)
            | P::SavePointRecord(_)
            | P::RecallRecord(_) => return Err(RecordFailure::OutsideDiagnosticVocabulary),
        }),
    };
    convert(Event {
        envelope: weaver_trace::Envelope {
            session,
            run,
            turn,
            sequence,
            kind,
            subsystem,
            causal_parent,
            wall_ms,
            monotonic_ns,
        },
        payload,
    })
}

#[cfg(test)]
mod tests;
