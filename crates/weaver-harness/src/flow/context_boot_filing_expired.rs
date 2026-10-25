//! Custody of an expired boot repair's original author and receipt subscription.
//! This account cannot be resumed or converted into a successful filing grant.
use super::*;
use std::sync::Arc;
use weaver_types::state_flow::{PairEnded, SourceBlockId};

pub(in crate::flow) struct BootFilingRollbackPermit {
    barrier: Arc<()>,
    pub(in crate::flow) expected: PairEnded,
    pub(in crate::flow) source: Option<SourceBlockId>,
    pub(in crate::flow) bound: AbsoluteBound,
    pub(in crate::flow) author_done: bool,
    pub(in crate::flow) unstarted: bool,
}

impl BootFilingRollbackPermit {
    pub(in crate::flow) fn matches_record(&self, barrier: &RecordBarrier) -> bool {
        Arc::ptr_eq(&self.barrier, &barrier.author_identity())
    }
    pub(in crate::flow) fn matches_event(&self, event: &AuthoredEvent) -> bool {
        event.kind == Kind::ToolCallCompleted
            && event.turn.is_none()
            && event.causal_parent.is_none()
            && matches!(&event.payload, Some(Payload::PairEnded(ended))
                if ended.as_ref() == &crate::flow::mirror::pair_ended(&self.expected))
    }
}

pub(in crate::flow::context) struct StoppedBootFilingAccount {
    _work: Box<FilingTransaction>,
    _waiting: Option<WaitingAuthor>,
    _completed: Option<Sequence>,
    _author_turn: Option<(Kind, Option<weaver_trace::TurnRef>)>,
    _admission_pending: bool,
    pub(in crate::flow::context) parked: bool,
    pub(in crate::flow::context) complete: bool,
}

#[cfg(test)]
fn evidence(
    work: &FilingTransaction,
    waiting: Option<&WaitingAuthor>,
    completed: Option<Sequence>,
    author_turn: &Option<(Kind, Option<weaver_trace::TurnRef>)>,
    admission_pending: bool,
) -> String {
    format!(
        "{:?}",
        (
            (
                Arc::as_ptr(&work.receipt_identity),
                work.owner,
                work.key,
                work.bound,
                work.sequence,
                work.author_done
            ),
            (
                &work.returned_author,
                work.cancelled,
                &work.cancelled_event,
                &work.cancelled_receipts
            ),
            waiting.map(|waiting| (&waiting.event, waiting.claim.is_some(), &waiting.withdrawn)),
            completed,
            author_turn,
            admission_pending,
        )
    )
}

#[cfg(test)]
impl StoppedBootFilingAccount {
    pub(in crate::flow::context) fn test_evidence(&self) -> String {
        evidence(
            &self._work,
            self._waiting.as_ref(),
            self._completed,
            &self._author_turn,
            self._admission_pending,
        )
    }
}

impl RecordingContext {
    #[cfg(test)]
    pub(in crate::flow::context) fn test_boot_filing_identity_guards(&mut self) {
        let expected = self
            .boot_pairs
            .as_ref()
            .unwrap()
            .expired_test_expected()
            .unwrap()
            .clone();
        let bound = self.filing.as_ref().unwrap().bound;
        assert!(self.boot_filing_rollback_permit(&expected, bound).is_some());
        let altered =
            AbsoluteBound::from_absolute(bound.began_ns() + 1, bound.deadline_ns()).unwrap();
        assert!(
            self.boot_filing_rollback_permit(&expected, altered)
                .is_none(),
            "filing bound must stay exact"
        );
        let mut foreign = expected.clone();
        foreign.call_id.call = (foreign.call_id.call.0 + 1).into();
        assert!(
            self.boot_filing_rollback_permit(&foreign, bound).is_none(),
            "another CallId cannot claim the filing"
        );
        let original = self.filing.as_ref().unwrap().owner;
        self.filing.as_mut().unwrap().owner = FilingOwner::Ordinary;
        assert!(
            self.boot_filing_rollback_permit(&expected, bound).is_none(),
            "ordinary filing is not boot repair"
        );
        self.filing.as_mut().unwrap().owner = original;
        let mut permit = self.boot_filing_rollback_permit(&expected, bound).unwrap();
        permit.barrier = Arc::new(());
        assert!(
            !self.flow.boot_filing_rollback_matches(
                &self.barrier,
                &self.inbox,
                &self.budget,
                &permit
            ),
            "another record barrier cannot claim custody"
        );
        let mut permit = self.boot_filing_rollback_permit(&expected, bound).unwrap();
        if let Some(source) = &mut permit.source {
            source.sequence = (source.sequence.0 + 1).into();
            assert!(
                !self.flow.boot_filing_rollback_matches(
                    &self.barrier,
                    &self.inbox,
                    &self.budget,
                    &permit
                ),
                "receipt reservation must name the original source"
            );
        }
    }
    #[cfg(test)]
    pub(in crate::flow::context) fn test_boot_filing_trace(&self) -> Option<(Sequence, PairEnded)> {
        Some((
            self.filing.as_ref()?.sequence?,
            self.boot_pairs.as_ref()?.expired_test_expected()?.clone(),
        ))
    }
    #[cfg(test)]
    pub(in crate::flow::context) fn test_boot_filing_author(&self) -> Option<String> {
        Some(evidence(
            self.filing.as_ref()?,
            self.waiting_content.as_ref(),
            self.completed_author,
            &self.author_turn,
            self.author_admission_pending,
        ))
    }
    pub(in crate::flow::context) fn boot_filing_rollback_permit(
        &self,
        expected: &PairEnded,
        bound: AbsoluteBound,
    ) -> Option<BootFilingRollbackPermit> {
        let work = self.filing.as_ref()?;
        if work.owner != FilingOwner::BootPairs
            || work.key != PumpDeadline::Decision
            || work.bound != bound
            || work.plan_effect
            || work.outbound.is_some()
            || self.release_author.is_some()
            || self
                .completed_author
                .is_some_and(|sequence| Some(sequence) != work.sequence)
            || self
                .author_turn
                .as_ref()
                .is_some_and(|(kind, turn)| *kind != Kind::ToolCallCompleted || turn.is_some())
        {
            return None;
        }
        let permit = BootFilingRollbackPermit {
            barrier: self.barrier.author_identity(),
            expected: expected.clone(),
            source: work.sequence.map(|sequence| SourceBlockId {
                run: self.flow.run().to_owned(),
                sequence: sequence.0.into(),
                block: 0.into(),
            }),
            bound,
            author_done: work.author_done,
            unstarted: work.sequence.is_none()
                && !work.author_done
                && self.waiting_content.is_some()
                && self.barrier.pending().is_none()
                && self.barrier.pending_author().is_none(),
        };
        if self.waiting_content.as_ref().is_some_and(|waiting| {
            waiting.claim.is_some()
                || waiting.withdrawn.is_some()
                || !permit.matches_event(&waiting.event)
        }) || work
            .returned_author
            .failure
            .as_ref()
            .and_then(|failure| failure.original.as_ref())
            .is_some_and(|event| !permit.matches_event(event))
        {
            return None;
        }
        self.flow
            .boot_filing_rollback_matches(&self.barrier, &self.inbox, &self.budget, &permit)
            .then_some(permit)
    }

    pub(in crate::flow::context) fn stop_boot_filing_author(
        &mut self,
        parked: bool,
        complete: bool,
    ) -> StoppedBootFilingAccount {
        StoppedBootFilingAccount {
            _work: Box::new(self.filing.take().expect("validated boot filing")),
            _waiting: self.waiting_content.take(),
            _completed: self.completed_author.take(),
            _author_turn: self.author_turn.take(),
            _admission_pending: std::mem::take(&mut self.author_admission_pending),
            parked,
            complete,
        }
    }
}
