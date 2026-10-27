//! A serving observation records its exact private runtime fact. An active
//! generation or read keeps its owner; any earlier author keeps its exact ticket.
use super::*;
use std::sync::{Arc, OnceLock};
use weaver_trace::serving::payloads as trace;

#[cfg(test)]
#[path = "context_serving_observation_tests.rs"]
mod tests;

#[derive(Debug)]
pub(super) struct ServingObservationAuthor {
    origin: Arc<()>,
    observation: Observation,
    ticket: OnceLock<u64>,
    sequence: OnceLock<Sequence>,
}

#[derive(Debug)]
enum Observation {
    Save {
        capture_id: trace::DecimalU64,
        window: trace::SaveWindow,
        refusal: Option<String>,
    },
    OrganLoss {
        payload: String,
    },
    OrganReport {
        payload: String,
        subsystem: weaver_trace::Subsystem,
    },
}

impl ServingObservationAuthor {
    pub(super) fn new(context: &RecordingContext, payload: &trace::SavePointRecord) -> Self {
        let (capture_id, window, refusal) = match payload {
            trace::SavePointRecord::Requested {
                capture_id,
                window,
                tail_window: None,
                cause: trace::PersistenceCause::Requested,
            } => (*capture_id, *window, None),
            trace::SavePointRecord::Refused {
                capture_id,
                window,
                tail_window: None,
                cause: trace::PersistenceCause::Requested,
                refusal_code,
            } => (*capture_id, *window, Some(refusal_code.clone())),
            _ => unreachable!("queued demand owns only its requested/refused observation"),
        };
        Self {
            origin: Arc::clone(&context.read_origin),
            observation: Observation::Save {
                capture_id,
                window,
                refusal,
            },
            ticket: OnceLock::new(),
            sequence: OnceLock::new(),
        }
    }

    pub(super) fn organ_loss(
        context: &RecordingContext,
        report: &weaver_types::FaultReport,
    ) -> Self {
        assert_eq!(report.case, weaver_types::FaultCase::OrganDeathObserved);
        Self {
            origin: Arc::clone(&context.read_origin),
            observation: Observation::OrganLoss {
                payload: serde_json::to_string(report).expect("typed fault serializes"),
            },
            ticket: OnceLock::new(),
            sequence: OnceLock::new(),
        }
    }

    pub(super) fn organ_report(
        context: &RecordingContext,
        report: &weaver_types::FaultReport,
        subsystem: weaver_trace::Subsystem,
    ) -> Self {
        assert!(matches!(
            subsystem,
            weaver_trace::Subsystem::Gate | weaver_trace::Subsystem::Spu
        ));
        Self {
            origin: Arc::clone(&context.read_origin),
            observation: Observation::OrganReport {
                payload: serde_json::to_string(report).expect("typed fault serializes"),
                subsystem,
            },
            ticket: OnceLock::new(),
            sequence: OnceLock::new(),
        }
    }

    fn allows(&self, context: &RecordingContext) -> bool {
        Arc::ptr_eq(&self.origin, &context.read_origin)
            && (context.phase == RecordingPhase::Entered
                || (matches!(
                    self.observation,
                    Observation::OrganLoss { .. } | Observation::OrganReport { .. }
                ) && context.phase == RecordingPhase::Releasing))
            && context.failure.is_none()
            && context.flow.halt().is_none()
    }

    fn allows_event(&self, event: &AuthoredEvent) -> bool {
        let subsystem = match &self.observation {
            Observation::OrganReport { subsystem, .. } => *subsystem,
            _ => weaver_trace::Subsystem::Harness,
        };
        if event.subsystem != subsystem
            || event.turn.is_some()
            || event.causal_parent.is_some()
            || !matches!(event.source, super::super::author::AuthoredSource::Runtime)
        {
            return false;
        }
        let (expected_id, expected_window, expected_refusal) = match &self.observation {
            Observation::OrganLoss { payload } | Observation::OrganReport { payload, .. } => {
                return event.kind == Kind::Fault
                    && matches!(&event.payload, Some(Payload::Fault(raw)) if raw.get() == payload);
            }
            Observation::Save {
                capture_id,
                window,
                refusal,
            } => (*capture_id, *window, refusal),
        };
        if event.kind != Kind::SavePoint {
            return false;
        }
        let Some(Payload::SavePointRecord(payload)) = &event.payload else {
            return false;
        };
        match (&**payload, expected_refusal) {
            (
                trace::SavePointRecord::Requested {
                    capture_id,
                    window,
                    tail_window: None,
                    cause: trace::PersistenceCause::Requested,
                },
                None,
            ) => *capture_id == expected_id && *window == expected_window,
            (
                trace::SavePointRecord::Refused {
                    capture_id,
                    window,
                    tail_window: None,
                    cause: trace::PersistenceCause::Requested,
                    refusal_code,
                },
                Some(expected),
            ) => {
                *capture_id == expected_id && *window == expected_window && refusal_code == expected
            }
            _ => false,
        }
    }

    fn queued_expiry(&self, bound: crate::flow::clock::AbsoluteBound) -> bool {
        matches!(&self.observation, Observation::Save { window, .. }
            if bound.began_ns() == window.began_ns.0
                && bound.deadline_ns() == window.deadline_ns.0)
    }

    pub(super) fn claim(&self, ticket: u64) -> bool {
        self.ticket.set(ticket).is_ok()
    }
    pub(super) fn ticket(&self) -> Option<u64> {
        self.ticket.get().copied()
    }
    pub(super) fn sequence(&self) -> Option<Sequence> {
        self.sequence.get().copied()
    }
}

impl RecordingContext {
    pub(super) fn serving_observation_author_available(&self) -> bool {
        matches!(
            self.phase,
            RecordingPhase::Entered | RecordingPhase::Releasing
        ) && self.failure.is_none()
            && self.flow.halt().is_none()
            && self.rollback_gate_author.is_none()
            && self.serving_observation_author.is_none()
            && self.held_input().is_none()
            && self.refused_author.is_none()
            && self.waiting_content.is_none()
            && self.completed_author.is_none()
            && self.barrier.pending().is_none()
            && self.barrier.pending_author().is_none()
            && !self.flow.has_pending_record()
            && self.filing.is_none()
            && self
                .pair_work
                .as_ref()
                .is_none_or(|work| !work.holds_filing_original())
            && self.freshness.is_none()
            && !self.pair_rejected_authoring
            && !self.author_admission_pending
    }

    pub(super) fn bind_serving_observation_author(&mut self, sequence: Sequence) {
        if let Some(owner) = &self.serving_observation_author
            && owner.ticket().is_some()
            && owner.ticket() == self.barrier.recorded_author_ticket(sequence)
        {
            let _ = owner.sequence.set(sequence);
        }
    }

    pub(super) fn serving_observation_recorded(&self, sequence: Sequence) -> bool {
        self.serving_observation_author
            .as_ref()
            .is_some_and(|owner| owner.sequence() == Some(sequence))
    }

    #[allow(clippy::result_large_err)]
    pub(super) fn author_serving_observation<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
        owner: &Arc<ServingObservationAuthor>,
        event: AuthoredEvent,
    ) -> Result<Sequence, AuthorFailure> {
        if !owner.allows(self) || !owner.allows_event(&event) || owner.ticket().is_some() {
            return Err(AuthorFailure {
                reason: RunFailure::Closed,
                original: Some(event),
                held: None,
            });
        }
        if !self.serving_observation_author_available() {
            return Err(AuthorFailure {
                reason: RunFailure::Busy(HeldInput::Caller),
                original: Some(event),
                held: None,
            });
        }
        let queued_expiry = match self.live(ports.io) {
            Ok(()) => None,
            Err(
                reason @ RunFailure::Wait {
                    key: PumpDeadline::SaveQueued,
                    bound,
                },
            ) if owner.queued_expiry(bound) => Some(reason),
            Err(reason) => {
                return Err(AuthorFailure {
                    reason,
                    original: Some(event),
                    held: None,
                });
            }
        };
        self.serving_observation_author = Some(Arc::clone(owner));
        self.waiting_content = Some(WaitingAuthor::claimed(
            event,
            AuthorClaim::ServingObservation(Arc::clone(owner)),
        ));
        // Expiry can race the preflight. Preserve this exact waiting original
        // while the capture owner reduces its queued timeout; do not make it a
        // rejected attempt or turn a retry into a second author.
        if let Some(reason) = queued_expiry {
            return Err(reason.into());
        }
        let result = self.continue_claimed_content(
            ports,
            Some(AuthorClaim::ServingObservation(Arc::clone(owner))),
        );
        self.finish_serving_observation_author(owner, result)
    }

    #[allow(clippy::result_large_err)]
    pub(super) fn resume_serving_observation<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
        owner: &Arc<ServingObservationAuthor>,
    ) -> Result<Sequence, AuthorFailure> {
        if !owner.allows(self)
            || !self
                .serving_observation_author
                .as_ref()
                .is_some_and(|actual| Arc::ptr_eq(actual, owner))
        {
            return Err(RunFailure::Closed.into());
        }
        self.live(ports.io)?;
        let waiting = self.waiting_content.as_ref().is_some_and(|waiting| {
            waiting.matches(Some(&AuthorClaim::ServingObservation(Arc::clone(owner))))
        });
        let pending = owner.ticket().is_some_and(|ticket| {
            self.barrier
                .pending_author()
                .is_some_and(|pending| pending.ticket == ticket)
        });
        let recorded = self
            .completed_author
            .is_some_and(|sequence| owner.sequence() == Some(sequence));
        if !waiting && !pending && !recorded {
            return Err(RunFailure::Busy(HeldInput::Caller).into());
        }
        let result = self.resume_claimed_author_and_wait(
            ports,
            Some(AuthorClaim::ServingObservation(Arc::clone(owner))),
        );
        self.finish_serving_observation_author(owner, result)
    }

    #[allow(clippy::result_large_err)]
    fn finish_serving_observation_author(
        &mut self,
        owner: &Arc<ServingObservationAuthor>,
        result: Result<Sequence, AuthorFailure>,
    ) -> Result<Sequence, AuthorFailure> {
        match &result {
            Ok(sequence) if owner.sequence() == Some(*sequence) => {
                self.serving_observation_author.take();
            }
            Ok(_) => return Err(RunFailure::Closed.into()),
            Err(error) if error.original.is_some() && owner.ticket().is_none() => {
                self.serving_observation_author.take();
            }
            Err(_) => (),
        }
        result
    }
}
