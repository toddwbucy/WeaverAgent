//! Exact rejected-return authorship remains owned across the first halt.
use super::*;
use crate::flow::author::AuthoredSource;
use std::sync::Arc;
use weaver_trace::Subsystem;
use weaver_types::state_flow::SourcePosition;

pub(in crate::flow::context) struct RejectedAuthor {
    pub(super) identity: Arc<()>,
    pub(super) ticket: Option<u64>,
    pub(super) sequence: Option<Sequence>,
    event: Option<AuthoredEvent>,
    unavailable: Option<String>,
}
impl RejectedAuthor {
    pub(in crate::flow::context) fn new() -> Self {
        Self {
            identity: Arc::new(()),
            ticket: None,
            sequence: None,
            event: None,
            unavailable: None,
        }
    }
    pub(in crate::flow::context) fn claim(&mut self, identity: &Arc<()>, ticket: u64) -> bool {
        if !Arc::ptr_eq(identity, &self.identity)
            || self.ticket.is_some()
            || self.sequence.is_some()
        {
            return false;
        }
        self.ticket = Some(ticket);
        true
    }
}

/// Only the failed pair owner can mint this nonselectable ordinary-capacity act.
pub(in crate::flow) struct OwedRejectedReturn(AuthoredEvent);
impl OwedRejectedReturn {
    pub(in crate::flow) fn into_event(self) -> AuthoredEvent {
        self.0
    }
}

impl RecordingContext {
    fn rejection_turn(&self) -> Option<weaver_trace::TurnRef> {
        self.open_turn.clone().or_else(|| {
            (self.phase == RecordingPhase::Failed)
                .then(|| self.pairs.active_turn().cloned())
                .flatten()
        })
    }
    pub(in crate::flow::context) fn bind_rejected_author(&mut self, sequence: Sequence) {
        if let Some(owner) = &mut self.pair_rejected_claim
            && owner.ticket.is_some()
            && owner.ticket == self.barrier.recorded_author_ticket(sequence)
            && owner.sequence.is_none()
        {
            owner.sequence = Some(sequence);
        }
    }

    pub(in crate::flow::context) fn rejected_event(&self) -> AuthoredEvent {
        let record = self
            .pairs
            .pending_rejection()
            .expect("owned rejected packet")
            .record()
            .clone();
        AuthoredEvent {
            kind: Kind::ToolReturnRejected,
            subsystem: Subsystem::Gate,
            turn: self.rejection_turn(),
            source: AuthoredSource::Tool,
            causal_parent: record
                .call_id
                .as_ref()
                .map(|call| Sequence(call.sequence.0)),
            payload: Some(Payload::RejectedReturn(Box::new(record))),
        }
    }

    /// One exact ordinary ticket, no live reduction, new deadline or state copy.
    pub(in crate::flow::context) fn advance_halt_rejection(
        &mut self,
    ) -> Result<Option<Sequence>, String> {
        if self.phase != RecordingPhase::Failed || self.failure.is_none() {
            return Err("rejected-return halt author requires the original failure".into());
        }
        if self.pairs.pending_rejection().is_none() {
            return Ok(None);
        }
        if self.pair_rejected_return.failure.is_some() {
            return Err("returned rejected-return attempt remains with its stopped author".into());
        }
        if self.pair_rejected_claim.is_none() {
            let mut owner = RejectedAuthor::new();
            owner.event = Some(self.rejected_event());
            self.pair_rejected_claim = Some(owner);
        }
        if let Some(detail) = &self.pair_rejected_claim.as_ref().unwrap().unavailable {
            return Err(detail.clone());
        }
        if self
            .pair_rejected_claim
            .as_ref()
            .unwrap()
            .sequence
            .is_none()
        {
            if self.pair_rejected_claim.as_ref().unwrap().ticket.is_none() {
                if self.barrier.pending().is_some() || self.barrier.pending_author().is_some() {
                    return Ok(None);
                }
                if self.pair_rejected_claim.as_ref().unwrap().event.is_none() {
                    let claim = AuthorClaim::PairRejection(Arc::clone(
                        &self.pair_rejected_claim.as_ref().unwrap().identity,
                    ));
                    if !self.waiting_content.as_ref().is_some_and(|waiting| {
                        waiting.withdrawn.is_none()
                            && waiting
                                .claim
                                .as_ref()
                                .is_some_and(|actual| actual.matches(&claim))
                    }) {
                        return Err("rejected-return original has no exact waiting owner".into());
                    }
                    self.pair_rejected_claim.as_mut().unwrap().event =
                        Some(self.waiting_content.take().unwrap().event);
                }
                let wall = wall_ms()?;
                let event = self
                    .pair_rejected_claim
                    .as_mut()
                    .unwrap()
                    .event
                    .take()
                    .unwrap();
                if let Err(error) = self.barrier.queue_halt_rejected(
                    OwedRejectedReturn(event),
                    &self.author,
                    self.last_ns,
                    wall,
                ) {
                    let owner = self.pair_rejected_claim.as_mut().unwrap();
                    owner.event = Some(error.event);
                    owner.unavailable = Some(error.detail.clone());
                    return Err(error.detail);
                }
                self.pair_rejected_claim.as_mut().unwrap().ticket =
                    Some(self.barrier.pending_author().unwrap().ticket);
            }
            self.resume_trace_only_and_bind()
                .map_err(|error| format!("rejected-return trace: {error:?}"))?;
        }
        let Some(sequence) = self.pair_rejected_claim.as_ref().unwrap().sequence else {
            return Ok(None);
        };
        let source = SourcePosition {
            block_id: weaver_types::state_flow::SourceBlockId {
                run: self.flow.run().to_owned(),
                sequence: sequence.0.into(),
                block: 0.into(),
            },
            turn: self.rejection_turn().map(|turn| turn.0),
        };
        self.pairs
            .ack_rejection_recorded(source)
            .map_err(|error| format!("recorded halt rejection: {error:?}"))?;
        self.pair_rejected_claim = None;
        self.pair_rejected_authoring = false;
        Ok(Some(sequence))
    }
}
