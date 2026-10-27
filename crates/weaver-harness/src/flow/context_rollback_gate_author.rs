//! A rollback gate may file only its exact noncontent cleanup observation.
//! Enclosing input/engine accounts remain owned; an actual author never yields
//! its original, ticket or completion to this narrowly scoped continuation.
use super::worker_startup::expired::gate::RollbackGateAuthor;
use super::*;
use std::sync::Arc;

impl RecordingContext {
    pub(in crate::flow::context) fn rollback_gate_author_available(&self) -> bool {
        self.phase == RecordingPhase::Entered
            && self.failure.is_none()
            && self.flow.halt().is_none()
            && self.rollback_gate_author.is_none()
            && self.queued_save_author.is_none()
            && !self.caller_author_held()
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

    pub(super) fn rollback_gate_recorded(&self, sequence: Sequence) -> bool {
        self.rollback_gate_author
            .as_ref()
            .is_some_and(|owner| owner.sequence() == Some(sequence))
    }

    pub(in crate::flow::context) fn bind_rollback_gate_author(&mut self, sequence: Sequence) {
        if let Some(owner) = &self.rollback_gate_author {
            owner.bind(self.barrier.recorded_author_ticket(sequence), sequence);
        }
    }

    #[allow(clippy::result_large_err)]
    pub(in crate::flow::context) fn author_rollback_gate<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
        owner: &Arc<RollbackGateAuthor>,
        event: AuthoredEvent,
    ) -> Result<Sequence, AuthorFailure> {
        if !owner.allows(self) || !owner.allows_event(&event) || owner.ticket().is_some() {
            return Err(AuthorFailure {
                reason: RunFailure::Closed,
                original: Some(event),
                held: None,
            });
        }
        if !self.rollback_gate_author_available() {
            return Err(AuthorFailure {
                reason: RunFailure::Busy(HeldInput::Caller),
                original: Some(event),
                held: None,
            });
        }
        if let Err(reason) = self.live(ports.io) {
            return Err(AuthorFailure {
                reason,
                original: Some(event),
                held: None,
            });
        }
        if !owner.allows(self) {
            return Err(AuthorFailure {
                reason: RunFailure::Closed,
                original: Some(event),
                held: None,
            });
        }
        self.rollback_gate_author = Some(Arc::clone(owner));
        self.waiting_content = Some(WaitingAuthor::claimed(
            event,
            AuthorClaim::RollbackGate(Arc::clone(owner)),
        ));
        let result = self
            .continue_claimed_content(ports, Some(AuthorClaim::RollbackGate(Arc::clone(owner))));
        self.finish_rollback_gate_author(owner, result)
    }

    #[allow(clippy::result_large_err)]
    pub(in crate::flow::context) fn resume_rollback_gate_author<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
        owner: &Arc<RollbackGateAuthor>,
    ) -> Result<Sequence, AuthorFailure> {
        if !owner.allows(self)
            || !self
                .rollback_gate_author
                .as_ref()
                .is_some_and(|actual| Arc::ptr_eq(actual, owner))
        {
            return Err(RunFailure::Closed.into());
        }
        self.live(ports.io)?;
        let waiting = self.waiting_content.as_ref().is_some_and(|waiting| {
            waiting.matches(Some(&AuthorClaim::RollbackGate(Arc::clone(owner))))
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
            Some(AuthorClaim::RollbackGate(Arc::clone(owner))),
        );
        self.finish_rollback_gate_author(owner, result)
    }

    #[allow(clippy::result_large_err)]
    fn finish_rollback_gate_author(
        &mut self,
        owner: &Arc<RollbackGateAuthor>,
        result: Result<Sequence, AuthorFailure>,
    ) -> Result<Sequence, AuthorFailure> {
        match &result {
            Ok(sequence) if owner.sequence() == Some(*sequence) => {
                self.rollback_gate_author.take();
            }
            Ok(_) => return Err(RunFailure::Closed.into()),
            Err(error) if error.original.is_some() && owner.ticket().is_none() => {
                self.rollback_gate_author.take();
            }
            Err(_) => (),
        }
        result
    }
}
