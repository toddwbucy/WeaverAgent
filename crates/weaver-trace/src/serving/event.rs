//! Serving envelope and typed payload licensing. The harness supplies all facts.

use super::payloads::*;
use super::{Channel, Kind, Origin, TurnRule, turn_rule};
use crate::{
    Failure, FieldName, MonotonicNs, RunRef, Sequence, SessionRef, SubmitRefusal, Subsystem,
    TurnRef,
};
use serde::Serialize;
use serde_json::value::RawValue;

#[derive(Debug, Clone, Serialize)]
pub struct Event {
    #[serde(flatten)]
    pub envelope: Envelope,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<Payload>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Envelope {
    pub session: SessionRef,
    pub run: RunRef,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn: Option<TurnRef>,
    pub sequence: Sequence,
    pub kind: Kind,
    pub channel: Channel,
    pub origin: Origin,
    pub subsystem: Subsystem,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub causal_parent: Option<Sequence>,
    pub wall_ms: u64,
    pub monotonic_ns: MonotonicNs,
}

/// Untagged by design. The envelope kind is the discriminant. Serving recall
/// keeps its read and boot wrappers separate from the legacy RecallAccount.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum Payload {
    Message(Box<RawValue>),
    TurnClosed(crate::TurnClose),
    Unload(Box<UnloadClose>),
    Fault(Box<RawValue>),
    ModelRequest(Box<RawValue>),
    ModelOutput(crate::ModelOutput),
    ModelField(crate::ModelField),
    Elections(Box<LoadRecord>),
    Flush(crate::FlushCounts),
    Elision(crate::ElisionSpan),
    Refusal(Box<RawValue>),
    ModelMeasurement(Box<RawValue>),
    ClassifyRequest(crate::ClassifyAsk),
    ClassifyOutput(crate::ClassifyScored),
    Score(crate::TaskScore),
    UnrecoveredCall(Box<RawValue>),
    PairStarted(Box<PairStarted>),
    PairEnded(Box<PairEnded>),
    RejectedReturn(Box<RejectedReturn>),
    LoopInstruction(Box<LoopInstructionRecord>),
    SortDecision(Box<SortDecisionRecord>),
    SortMeasurement(Box<SortMeasurement>),
    StepOutcome(Box<StepOutcomeRecord>),
    PlanInput(Box<PlanInputRecord>),
    PlanEffect(Box<PlanEffectRecord>),
    PlanBreak(Box<PlanBreakReport>),
    PlanMeasurement(Box<PlanMeasurement>),
    StateMark(Box<StateMarkRecord>),
    MarkPosition(Box<MarkPosition>),
    Placement(Box<PlacementRecord>),
    InputOffered(Box<InputOfferedRecord>),
    FlowObservation(Box<FlowObservation>),
    StateStoreMeasurement(Box<StateStoreMeasurement>),
    Ste100Result(Box<Ste100ResultRecord>),
    SavePointRecord(Box<SavePointRecord>),
    RecallRecord(Box<RecallRecord>),
}

pub(super) fn malformed() -> Failure {
    Failure::RefusedOnSubmit {
        reason: SubmitRefusal::PayloadMalformed,
    }
}
fn absent(field: &str) -> Failure {
    Failure::RefusedOnSubmit {
        reason: SubmitRefusal::RequiredFieldAbsent {
            field: FieldName(field.into()),
        },
    }
}

pub(super) fn admit(event: &Event, session: &SessionRef, run: &RunRef) -> Result<(), Failure> {
    let envelope = &event.envelope;
    if envelope.session != *session {
        return Err(absent("session"));
    }
    if envelope.run != *run {
        return Err(absent("run"));
    }
    match (turn_rule(envelope.kind), envelope.turn.is_some()) {
        (TurnRule::Required, false) => return Err(absent("turn")),
        (TurnRule::Forbidden, true) => return Err(malformed()),
        _ => {}
    }
    if !channel_origin_licensed(envelope) {
        return Err(malformed());
    }
    let Some(payload) = event.payload.as_ref() else {
        return if matches!(envelope.kind, Kind::TurnStarted | Kind::SessionClosed) {
            Ok(())
        } else {
            Err(absent("payload"))
        };
    };
    if !payload_licensed(envelope, payload) {
        return Err(malformed());
    }
    Ok(())
}

fn channel_origin_licensed(envelope: &Envelope) -> bool {
    use Channel as C;
    use Kind as K;
    use Origin as O;
    let (channel, origin) = (envelope.channel, envelope.origin);
    match envelope.kind {
        K::InputOffered | K::MessageUser => {
            channel == C::Input && matches!(origin, O::Operator | O::External)
        }
        K::PlanEvent => {
            (channel, origin) == (C::Input, O::Operator)
                || (channel, origin) == (C::Internal, O::Loop)
        }
        K::PlanBreak
        | K::StatePlanMeasurement
        | K::LoopInstruction
        | K::SortDecision
        | K::StateSortMeasurement
        | K::StepOutcome
        | K::StateMark
        | K::StateMarkPosition
        | K::StatePlacement
        | K::ModelRequest => (channel, origin) == (C::Internal, O::Loop),
        K::MessageAssistant | K::ModelOutput | K::ModelUnrecoveredCall => {
            (channel, origin) == (C::Output, O::Model)
        }
        K::MessageToolResult | K::ToolReturnRejected => (channel, origin) == (C::Tool, O::Tool),
        K::ToolCallStarted => channel == C::Tool && matches!(origin, O::Model | O::Loop),
        K::ToolCallCompleted => channel == C::Tool && matches!(origin, O::Tool | O::Loop),
        K::ModelMeasurement
        | K::ModelField
        | K::Score
        | K::StateFlow
        | K::StateStoreMeasurement => (channel, origin) == (C::Measurement, O::Runtime),
        K::ClassifyRequest
        | K::ClassifyOutput
        | K::Ste100Result
        | K::Load
        | K::Unload
        | K::SessionClosed
        | K::TurnStarted
        | K::TurnClosed
        | K::Fault
        | K::Elision
        | K::Refusal
        | K::Flush
        | K::Recall
        | K::SavePoint
        | K::MessageRestored
        | K::MessageSystem => (channel, origin) == (C::Internal, O::Runtime),
    }
}

fn payload_licensed(envelope: &Envelope, payload: &Payload) -> bool {
    use Kind as K;
    use Payload as P;
    let turn = envelope.turn.is_some();
    match (envelope.kind, payload) {
        (
            K::MessageSystem
            | K::MessageUser
            | K::MessageAssistant
            | K::MessageToolResult
            | K::MessageRestored,
            P::Message(_),
        ) => envelope.subsystem == Subsystem::Harness,
        (K::TurnClosed, P::TurnClosed(_))
        | (K::Fault, P::Fault(_))
        | (K::Load, P::Elections(_))
        | (K::Flush, P::Flush(_))
        | (K::Elision, P::Elision(_))
        | (K::Refusal, P::Refusal(_))
        | (K::ModelRequest, P::ModelRequest(_))
        | (K::ModelOutput, P::ModelOutput(_))
        | (K::ModelMeasurement, P::ModelMeasurement(_))
        | (K::ModelField, P::ModelField(_))
        | (K::ClassifyRequest, P::ClassifyRequest(_))
        | (K::ClassifyOutput, P::ClassifyOutput(_))
        | (K::Score, P::Score(_))
        | (K::ModelUnrecoveredCall, P::UnrecoveredCall(_)) => true,
        (K::Unload, P::Unload(record)) => {
            record.end_reason.is_some() == record.state_flow_failure.is_some()
        }
        (K::ToolCallStarted, P::PairStarted(record)) => {
            matches!(
                (&record.lane, &record.request, envelope.origin),
                (PairLane::Review, PairRequest::Review { .. }, Origin::Loop)
                    | (
                        PairLane::External,
                        PairRequest::External { .. },
                        Origin::Model
                    )
                    | (PairLane::Recall, PairRequest::Recall { .. }, Origin::Model)
            )
        }
        (K::ToolCallCompleted, P::PairEnded(record)) => {
            let abandoned = matches!(record.reason, Some(PairFailure::AbandonedAtLoad {}));
            if abandoned {
                !turn && envelope.origin == Origin::Loop
            } else {
                turn
            }
        }
        (K::ToolReturnRejected, P::RejectedReturn(_)) => true,
        (K::LoopInstruction, P::LoopInstruction(record)) => {
            user_text(&record.message, true)
                && (turn || matches!(record.instruction, LoopInstructionMeta::WindDown {}))
        }
        (K::SortDecision, P::SortDecision(record)) => user_text(&record.message, true),
        (K::StateSortMeasurement, P::SortMeasurement(_)) => true,
        (K::StepOutcome, P::StepOutcome(record)) => {
            user_text(&record.message, true) && outcome_context(record)
        }
        (K::PlanEvent, P::PlanInput(record)) => {
            envelope.channel == Channel::Input
                && envelope.origin == Origin::Operator
                && !turn
                && user_text(&record.message, false)
        }
        (K::PlanEvent, P::PlanEffect(record)) => {
            envelope.channel == Channel::Internal
                && envelope.origin == Origin::Loop
                && user_text(&record.message, true)
                && match &record.context {
                    PlanEffect::Authorize { .. }
                    | PlanEffect::Replace { .. }
                    | PlanEffect::Lock {
                        source: PlanLockSource::Operator { .. },
                        ..
                    } => !turn,
                    PlanEffect::Lock {
                        source: PlanLockSource::Model { .. },
                        ..
                    }
                    | PlanEffect::Activate { .. }
                    | PlanEffect::Break { .. } => turn,
                    PlanEffect::Close { .. } => false,
                }
        }
        (K::PlanBreak, P::PlanBreak(_)) => true,
        (K::StatePlanMeasurement, P::PlanMeasurement(record)) => {
            !matches!(record.event, PlanMeasureEvent::NextReady { .. }) || turn
        }
        (K::StateMark, P::StateMark(record)) => {
            user_text(&record.message, true)
                && matches!(record.context, MarkEffect::BeginTask { .. })
        }
        (K::StateMarkPosition, P::MarkPosition(_)) | (K::InputOffered, P::InputOffered(_)) => true,
        (K::StatePlacement, P::Placement(record)) => match record.phase {
            PlacementPhase::Boot => {
                !turn
                    && envelope.causal_parent.is_some()
                    && record.context.is_none()
                    && record.cut.is_none()
                    && record.method.is_none()
                    && record.assistant_message_id.is_none()
                    && record.assembly_id.0 == 0
                    && record.n == record.prefix_tokens
                    && record.resident_before == record.prefix_tokens
                    && record.input_tokens == 0
                    && record.prefix_tokens <= record.capacity
                    && record.terminal_tokens == 1
                    && record.unit_map.is_empty()
                    && record.message_ids.is_empty()
                    && record.recomputed_messages.0 == 0
                    && record
                        .open_evidence
                        .as_ref()
                        .is_some_and(OpenEvidenceRecord::has_object_boxes)
            }
            PlacementPhase::Cut => {
                record.context.is_none()
                    && record.method.is_some()
                    && record.assistant_message_id.is_none()
                    && record.assembly_id.0 != 0
                    && record.open_evidence.is_none()
            }
            PlacementPhase::Append => {
                record.context.is_some()
                    && record.method.is_some()
                    && record.assembly_id.0 != 0
                    && record.open_evidence.is_none()
            }
        },
        (K::StateFlow, P::FlowObservation(record)) => {
            let repair = matches!(
                record.phase,
                FlowPhase::RepairRequested | FlowPhase::RepairExhausted
            );
            record.failed_corrections.is_some() == repair
                && record.retry_cap.is_some() == repair
                && (!repair || record.source_id.is_some())
        }
        (K::StateStoreMeasurement, P::StateStoreMeasurement(_)) => {
            envelope.subsystem == Subsystem::Harness
        }
        (K::Ste100Result, P::Ste100Result(record)) => {
            !matches!(record.target, Ste100Target::Template { .. }) || !turn
        }
        (K::SavePoint, P::SavePointRecord(record)) => {
            let cause = match record.as_ref() {
                SavePointRecord::Requested { cause, .. }
                | SavePointRecord::Confirmed { cause, .. }
                | SavePointRecord::Published { cause, .. }
                | SavePointRecord::Refused { cause, .. } => cause,
            };
            !matches!(cause, PersistenceCause::Seed | PersistenceCause::Rebind)
        }
        (K::Recall, P::RecallRecord(record)) => {
            envelope.subsystem == Subsystem::Harness && record.valid()
        }
        _ => false,
    }
}

fn outcome_context(record: &StepOutcomeRecord) -> bool {
    match (&record.outcome, &record.context) {
        (ReviewOutcome::Failed { .. }, None) => true,
        (
            ReviewOutcome::Closed {},
            Some(PlanEffect::Close {
                expected,
                step_id,
                review_call,
                work,
                ..
            }),
        ) => {
            expected == &record.plan
                && step_id == &record.step_id
                && review_call == &record.review_call
                && work == &record.work
        }
        _ => false,
    }
}

/// Only the new loop-authored structural records are inspected here. Ordinary
/// conversation Message bytes stay under the existing opaque-message contract.
fn user_text(raw: &RawValue, single: bool) -> bool {
    use super::record::Object;
    let Ok(message) = Object::parse(raw.get()) else {
        return false;
    };
    if !message.exact(&["role", "content"]) || message.string("role").as_deref() != Ok("user") {
        return false;
    }
    let Some(content) = message.get("content") else {
        return false;
    };
    let Ok(blocks) = serde_json::from_str::<Vec<&RawValue>>(content.get()) else {
        return false;
    };
    if blocks.is_empty() || (single && blocks.len() != 1) {
        return false;
    }
    blocks.iter().all(|raw| {
        let Ok(block) = Object::parse(raw.get()) else {
            return false;
        };
        block.exact(&["type", "text"])
            && block.string("type").as_deref() == Ok("text")
            && block.string("text").is_ok()
    })
}

#[cfg(test)]
#[path = "event_tests.rs"]
mod tests;
