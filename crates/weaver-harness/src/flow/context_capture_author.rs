//! A queued save records only its own requested/refused observation. An active
//! generation or read keeps its owner; any earlier author keeps its exact ticket.
use super::*;
use std::sync::{Arc, OnceLock};
use weaver_trace::serving::payloads as trace;

#[derive(Debug)]
pub(super) struct QueuedSaveAuthor {
    origin: Arc<()>,
    capture_id: trace::DecimalU64,
    window: trace::SaveWindow,
    refusal: Option<String>,
    ticket: OnceLock<u64>,
    sequence: OnceLock<Sequence>,
}

impl QueuedSaveAuthor {
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
            capture_id,
            window,
            refusal,
            ticket: OnceLock::new(),
            sequence: OnceLock::new(),
        }
    }

    fn allows(&self, context: &RecordingContext) -> bool {
        Arc::ptr_eq(&self.origin, &context.read_origin)
            && context.phase == RecordingPhase::Entered
            && context.failure.is_none()
            && context.flow.halt().is_none()
    }

    fn allows_event(&self, event: &AuthoredEvent) -> bool {
        if event.kind != Kind::SavePoint
            || event.subsystem != weaver_trace::Subsystem::Harness
            || event.turn.is_some()
            || event.causal_parent.is_some()
            || !matches!(event.source, super::super::author::AuthoredSource::Runtime)
        {
            return false;
        }
        let Some(Payload::SavePointRecord(payload)) = &event.payload else {
            return false;
        };
        match (&**payload, &self.refusal) {
            (
                trace::SavePointRecord::Requested {
                    capture_id,
                    window,
                    tail_window: None,
                    cause: trace::PersistenceCause::Requested,
                },
                None,
            ) => *capture_id == self.capture_id && *window == self.window,
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
                *capture_id == self.capture_id && *window == self.window && refusal_code == expected
            }
            _ => false,
        }
    }

    pub(super) fn claim(&self, ticket: u64) -> bool {
        self.ticket.set(ticket).is_ok()
    }
    fn ticket(&self) -> Option<u64> {
        self.ticket.get().copied()
    }
    pub(super) fn sequence(&self) -> Option<Sequence> {
        self.sequence.get().copied()
    }
}

impl RecordingContext {
    pub(super) fn queued_save_author_available(&self) -> bool {
        self.phase == RecordingPhase::Entered
            && self.failure.is_none()
            && self.flow.halt().is_none()
            && self.rollback_gate_author.is_none()
            && self.queued_save_author.is_none()
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

    pub(super) fn bind_queued_save_author(&mut self, sequence: Sequence) {
        if let Some(owner) = &self.queued_save_author
            && owner.ticket().is_some()
            && owner.ticket() == self.barrier.recorded_author_ticket(sequence)
        {
            let _ = owner.sequence.set(sequence);
        }
    }

    #[allow(clippy::result_large_err)]
    pub(super) fn author_queued_save<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
        owner: &Arc<QueuedSaveAuthor>,
        event: AuthoredEvent,
    ) -> Result<Sequence, AuthorFailure> {
        if !owner.allows(self) || !owner.allows_event(&event) || owner.ticket().is_some() {
            return Err(AuthorFailure {
                reason: RunFailure::Closed,
                original: Some(event),
                held: None,
            });
        }
        if !self.queued_save_author_available() {
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
            ) if bound.began_ns() == owner.window.began_ns.0
                && bound.deadline_ns() == owner.window.deadline_ns.0 =>
            {
                Some(reason)
            }
            Err(reason) => {
                return Err(AuthorFailure {
                    reason,
                    original: Some(event),
                    held: None,
                });
            }
        };
        self.queued_save_author = Some(Arc::clone(owner));
        self.waiting_content = Some(WaitingAuthor::claimed(
            event,
            AuthorClaim::QueuedSave(Arc::clone(owner)),
        ));
        // Expiry can race the preflight. Preserve this exact waiting original
        // while the capture owner reduces its queued timeout; do not make it a
        // rejected attempt or turn a retry into a second author.
        if let Some(reason) = queued_expiry {
            return Err(reason.into());
        }
        let result =
            self.continue_claimed_content(ports, Some(AuthorClaim::QueuedSave(Arc::clone(owner))));
        self.finish_queued_save_author(owner, result)
    }

    #[allow(clippy::result_large_err)]
    pub(super) fn resume_queued_save<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
        owner: &Arc<QueuedSaveAuthor>,
    ) -> Result<Sequence, AuthorFailure> {
        if !owner.allows(self)
            || !self
                .queued_save_author
                .as_ref()
                .is_some_and(|actual| Arc::ptr_eq(actual, owner))
        {
            return Err(RunFailure::Closed.into());
        }
        self.live(ports.io)?;
        let waiting = self.waiting_content.as_ref().is_some_and(|waiting| {
            waiting.matches(Some(&AuthorClaim::QueuedSave(Arc::clone(owner))))
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
            Some(AuthorClaim::QueuedSave(Arc::clone(owner))),
        );
        self.finish_queued_save_author(owner, result)
    }

    #[allow(clippy::result_large_err)]
    fn finish_queued_save_author(
        &mut self,
        owner: &Arc<QueuedSaveAuthor>,
        result: Result<Sequence, AuthorFailure>,
    ) -> Result<Sequence, AuthorFailure> {
        match &result {
            Ok(sequence) if owner.sequence() == Some(*sequence) => {
                self.queued_save_author.take();
            }
            Ok(_) => return Err(RunFailure::Closed.into()),
            Err(error) if error.original.is_some() && owner.ticket().is_none() => {
                self.queued_save_author.take();
            }
            Err(_) => (),
        }
        result
    }
}
