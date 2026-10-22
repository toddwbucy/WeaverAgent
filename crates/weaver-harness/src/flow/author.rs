//! Serving author attribution, with the same numeric origin as the flow pump.
//! This prepares one event and performs no submission or nested authoring.

use weaver_trace::serving::{Channel, Envelope, Event, Kind, Origin, Payload};
use weaver_trace::{Cause, MonotonicNs, RunRef, Sequence, SessionRef, Subsystem, TurnRef};

#[derive(Debug, Clone)]
pub(super) enum AuthoredSource {
    BoundaryInput(u32),
    ManagementInput(Cause),
    Model,
    Tool,
    Loop,
    Runtime,
}

/// The six original author members supplied to event preparation. The run-wide
/// recording context must retain these alongside its prepared pending ticket.
#[derive(Debug, Clone)]
pub(super) struct AuthoredEvent {
    pub kind: Kind,
    pub subsystem: Subsystem,
    pub turn: Option<TurnRef>,
    pub source: AuthoredSource,
    pub causal_parent: Option<Sequence>,
    pub payload: Option<Payload>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AuthorError {
    EmptyIdentity,
    UnlicensedSource,
    SystemPrompt,
    ClockBeforeOrigin,
}

pub(super) struct ServingAuthor {
    session: String,
    run: String,
    operator: u32,
    origin_ns: u64,
}

impl ServingAuthor {
    pub(super) fn new(
        session: String,
        run: String,
        operator: u32,
        origin_ns: u64,
    ) -> Result<Self, AuthorError> {
        if session.is_empty() || run.is_empty() {
            return Err(AuthorError::EmptyIdentity);
        }
        Ok(Self {
            session,
            run,
            operator,
            origin_ns,
        })
    }

    pub(super) fn prepare(
        &self,
        authored: &AuthoredEvent,
        raw_ns: u64,
        wall_ms: u64,
    ) -> Result<Event, AuthorError> {
        if authored.kind == Kind::MessageSystem {
            return Err(AuthorError::SystemPrompt);
        }
        let (channel, origin) = attribution(authored.kind, &authored.source, self.operator)?;
        let elapsed = raw_ns
            .checked_sub(self.origin_ns)
            .ok_or(AuthorError::ClockBeforeOrigin)?;
        Ok(Event {
            envelope: Envelope {
                session: SessionRef(self.session.clone()),
                run: RunRef(self.run.clone()),
                turn: authored.turn.clone(),
                sequence: Sequence(0),
                kind: authored.kind,
                channel,
                origin,
                subsystem: authored.subsystem,
                causal_parent: authored.causal_parent,
                wall_ms,
                monotonic_ns: MonotonicNs(elapsed),
            },
            payload: authored.payload.clone(),
        })
    }
}

fn attribution(
    kind: Kind,
    source: &AuthoredSource,
    operator: u32,
) -> Result<(Channel, Origin), AuthorError> {
    use AuthoredSource as S;
    use Channel as C;
    use Kind as K;
    use Origin as O;
    // Exhaustive over Kind. A registry addition cannot acquire a default origin.
    let result = match kind {
        K::InputOffered | K::MessageUser => match source {
            S::BoundaryInput(uid) => Some((
                C::Input,
                if *uid == operator {
                    O::Operator
                } else {
                    O::External
                },
            )),
            _ => None,
        },
        K::PlanEvent => match source {
            S::ManagementInput(_) => Some((C::Input, O::Operator)),
            S::Loop => Some((C::Internal, O::Loop)),
            _ => None,
        },
        K::MessageAssistant | K::ModelOutput | K::ModelUnrecoveredCall => match source {
            S::Model => Some((C::Output, O::Model)),
            _ => None,
        },
        K::MessageToolResult | K::ToolReturnRejected => match source {
            S::Tool => Some((C::Tool, O::Tool)),
            _ => None,
        },
        K::ToolCallStarted => match source {
            S::Model => Some((C::Tool, O::Model)),
            S::Loop => Some((C::Tool, O::Loop)),
            _ => None,
        },
        K::ToolCallCompleted => match source {
            S::Tool => Some((C::Tool, O::Tool)),
            S::Loop => Some((C::Tool, O::Loop)),
            _ => None,
        },
        K::PlanBreak
        | K::StatePlanMeasurement
        | K::LoopInstruction
        | K::SortDecision
        | K::StateSortMeasurement
        | K::StepOutcome
        | K::StateMark
        | K::StateMarkPosition
        | K::StatePlacement
        | K::ModelRequest => match source {
            S::Loop => Some((C::Internal, O::Loop)),
            _ => None,
        },
        K::ModelMeasurement
        | K::ModelField
        | K::Score
        | K::StateFlow
        | K::StateStoreMeasurement => match source {
            S::Runtime => Some((C::Measurement, O::Runtime)),
            _ => None,
        },
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
        | K::MessageSystem => match source {
            S::Runtime => Some((C::Internal, O::Runtime)),
            _ => None,
        },
    };
    result.ok_or(AuthorError::UnlicensedSource)
}

#[cfg(test)]
#[path = "author_tests.rs"]
mod tests;
