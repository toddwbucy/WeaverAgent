//! Healthy release authoring uses ordinary capacity with the same stopped tap.
//! This seal is private to the context; Failed retains its separate halt owner.

use std::sync::Arc;

use super::*;
use crate::flow::recording::AuthorTicket;
use weaver_trace::serving::Event;

pub(in crate::flow) fn shutdown_kind(kind: Kind) -> bool {
    serving::content_schema(kind) == ContentSchema::None
        && matches!(
            kind,
            Kind::Unload | Kind::SessionClosed | Kind::Fault | Kind::Refusal
        )
}

/// A context-owned attempt has no Clone, Default or scalar constructor. A fresh
/// attempt cannot resume another attempt, even with the same run and event kind.
pub(super) struct ReleaseAttempt {
    identity: Arc<()>,
    recorded: Option<Sequence>,
}

/// Borrow the issuing context's actual phase and failure slots for this call.
/// Holding a scope prevents changing those slots while the barrier uses it.
pub(in crate::flow) struct ReleaseScope<'a> {
    barrier: Arc<()>,
    phase: &'a RecordingPhase,
    failure: &'a Option<RunFailure>,
    attempt: &'a Option<ReleaseAttempt>,
}

/// The admission seal owns its event. There is no separate caller-supplied Event
/// which could borrow a shutdown permit issued for another original.
pub(in crate::flow) struct ReleaseAuthor<'a> {
    scope: ReleaseScope<'a>,
    event: AuthoredEvent,
}

pub(in crate::flow) struct ReleaseTicket {
    barrier: Arc<()>,
    attempt: Arc<()>,
    ticket: u64,
    kind: Kind,
}

impl<'a> ReleaseAuthor<'a> {
    pub(in crate::flow) fn into_parts(self) -> (ReleaseScope<'a>, AuthoredEvent) {
        (self.scope, self.event)
    }
}
impl ReleaseScope<'_> {
    pub(in crate::flow) fn allows(&self, barrier: &Arc<()>) -> bool {
        *self.phase == RecordingPhase::Releasing
            && self.failure.is_none()
            && Arc::ptr_eq(&self.barrier, barrier)
            && self
                .attempt
                .as_ref()
                .is_some_and(|attempt| attempt.recorded.is_none())
    }
    pub(in crate::flow) fn ticket(&self, ticket: u64, kind: Kind) -> ReleaseTicket {
        ReleaseTicket {
            barrier: Arc::clone(&self.barrier),
            attempt: Arc::clone(
                &self
                    .attempt
                    .as_ref()
                    .expect("checked release attempt")
                    .identity,
            ),
            ticket,
            kind,
        }
    }
}
impl ReleaseTicket {
    pub(in crate::flow) fn matches(
        &self,
        scope: &ReleaseScope<'_>,
        barrier: &Arc<()>,
        original: Option<&AuthorTicket>,
        event: &Event,
        purpose: &RecordPurpose,
    ) -> bool {
        scope.allows(barrier)
            && Arc::ptr_eq(&self.barrier, barrier)
            && scope
                .attempt
                .as_ref()
                .is_some_and(|attempt| Arc::ptr_eq(&self.attempt, &attempt.identity))
            && original
                .is_some_and(|held| held.ticket == self.ticket && held.event.kind == self.kind)
            && event.envelope.kind == self.kind
            && shutdown_kind(self.kind)
            && matches!(purpose, RecordPurpose::Observation)
    }
}

impl RecordingContext {
    pub(super) fn submit_context_author<I: PumpIo>(
        &mut self,
        event: AuthoredEvent,
        purpose: RecordPurpose,
        wall_ms: u64,
        io: &mut I,
    ) -> RecordProgress {
        if self.phase != RecordingPhase::Releasing {
            return self.barrier.submit_authored_with_clock(
                event,
                purpose,
                &self.author,
                wall_ms,
                &mut self.inbox,
                &mut self.budget,
                &mut || io.now(),
            );
        }
        self.release_author = Some(ReleaseAttempt {
            identity: Arc::new(()),
            recorded: None,
        });
        let scope = ReleaseScope {
            barrier: self.barrier.author_identity(),
            phase: &self.phase,
            failure: &self.failure,
            attempt: &self.release_author,
        };
        self.barrier.submit_release_authored_with_clock(
            ReleaseAuthor { scope, event },
            purpose,
            &self.author,
            wall_ms,
            &mut self.inbox,
            &mut self.budget,
            &mut || io.now(),
        )
    }

    pub(super) fn resume_context_author<I: PumpIo>(&mut self, io: &mut I) -> RecordProgress {
        if self.phase == RecordingPhase::Releasing || self.release_author.is_some() {
            let scope = ReleaseScope {
                barrier: self.barrier.author_identity(),
                phase: &self.phase,
                failure: &self.failure,
                attempt: &self.release_author,
            };
            self.barrier.resume_release_with_clock(
                &scope,
                &mut self.inbox,
                &mut self.budget,
                &mut || io.now(),
            )
        } else {
            self.barrier
                .resume_with_clock(&mut self.inbox, &mut self.budget, &mut || io.now())
        }
    }

    /// Called at the Recorded boundary, before any fallible observer. A later
    /// clock/intake failure retains the real sequence and cannot resubmit it.
    pub(super) fn retain_release_recorded(&mut self, sequence: Sequence) {
        if let Some(attempt) = &mut self.release_author {
            attempt.recorded = Some(sequence);
            self.last_author_sequence = Some(sequence);
            self.completed_author = Some(sequence);
        }
    }

    pub(super) fn finish_release_author(&mut self) {
        self.release_author = None;
    }
}

#[cfg(test)]
#[path = "context_release_tests.rs"]
mod tests;
