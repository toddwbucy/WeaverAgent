//! Join the actual stopped startup to its one after-load refusal record.
//! The exclusive owner keeps all startup facts; no second admission read occurs.
use super::*;
use crate::flow::author::AuthoredSource;
use crate::flow::lifecycle_refusal::{AdmissionRefusalError, prepare_admission_refusal};
use std::sync::Arc;
use weaver_trace::Subsystem;
use weaver_types::{ExchangeId, LifecycleAsk, Opener, OrganEnvelope, RefusalRecord};

#[derive(Debug)]
pub(in crate::flow) enum RefusalJoinError {
    WrongStage,
    Busy,
    WrongExchange,
    WrongWindow,
    Mapping(AdmissionRefusalError),
    TraceEncoding(String),
}

/// This wrapper consumes the startup owner, rather than accepting a free-standing
/// code/detail or an account which a caller could attach to a different context.
pub(in crate::flow) struct EnterRefusalComposition<'a> {
    startup: StartupComposition<'a>,
    bound: AbsoluteBound,
    envelope: OrganEnvelope,
    original: Option<AuthoredEvent>,
}

/// Recorded is a retained fact. Sending the answer and accepting admin's actual
/// rollback still belong to the outer lifecycle; this owner grants neither Ready
/// nor Left and supplies no mutable context escape.
pub(in crate::flow) struct RecordedStartupRefusal<'a> {
    owner: EnterRefusalComposition<'a>,
}

pub(in crate::flow::context) struct RetiredRefusal {
    startup: StoppedStartupAccount,
    original_enter: Option<crate::flow::pump::enter::RetiredEnter>,
    bound: AbsoluteBound,
    envelope: OrganEnvelope,
    original: Option<AuthoredEvent>,
}
impl EnterRefusalComposition<'_> {
    #[cfg(test)]
    pub(in crate::flow::context) fn failure_test_context(&mut self) -> &mut RecordingContext {
        self.startup.context
    }

    pub(in crate::flow::context) fn failure_view(
        &self,
    ) -> crate::flow::context::worker_startup::rollback::failed::RollbackView<'_> {
        use crate::flow::context::worker_startup::rollback::failed::RollbackView;
        RollbackView {
            context: self.startup.context,
            resources: self.startup.resources,
            leave: None,
            recorded: None,
            cause: self
                .startup
                .original_enter
                .as_ref()
                .map(|original| original.envelope().enter().cause),
        }
    }
    pub(in crate::flow::context) fn retire_failure(self) -> RetiredRefusal {
        let Self {
            startup,
            bound,
            envelope,
            original,
        } = self;
        let (_, _, mut startup) = startup
            .into_stopped()
            .ok()
            .expect("retained classifier refusal");
        let original_enter = startup
            .original_enter
            .take()
            .map(|original| original.retire_failed());
        RetiredRefusal {
            startup,
            original_enter,
            bound,
            envelope,
            original,
        }
    }
}
impl RecordedStartupRefusal<'_> {
    #[cfg(test)]
    pub(in crate::flow::context) fn failure_test_context(&mut self) -> &mut RecordingContext {
        self.owner.failure_test_context()
    }

    pub(in crate::flow::context) fn failure_view(
        &self,
    ) -> crate::flow::context::worker_startup::rollback::failed::RollbackView<'_> {
        self.owner.failure_view()
    }
    pub(in crate::flow::context) fn retire_failure(self) -> RetiredRefusal {
        self.owner.retire_failure()
    }
}

impl<'a> StartupComposition<'a> {
    #[allow(clippy::result_large_err)]
    pub(in crate::flow) fn into_original_refusal(
        self,
    ) -> Result<EnterRefusalComposition<'a>, (Self, RefusalJoinError)> {
        let Some(original) = self
            .original_enter
            .as_ref()
            .filter(|o| o.adopted_by(self.context))
        else {
            return Err((self, RefusalJoinError::WrongExchange));
        };
        let exchange = original.envelope().exchange.clone();
        self.into_refusal(exchange)
    }
    #[allow(clippy::result_large_err)]
    pub(in crate::flow) fn into_refusal(
        self,
        exchange: ExchangeId,
    ) -> Result<EnterRefusalComposition<'a>, (Self, RefusalJoinError)> {
        if self.stage != StartupStage::Stopped
            || !matches!(self.stop, Some(StartupStop::ClassifierRefused(_)))
            || self.load_sequence.is_none()
            || self.opened.is_none()
            || !self.resources.matches(self.context)
        {
            return Err((self, RefusalJoinError::WrongStage));
        }
        if exchange.opener != Opener::Admin {
            return Err((self, RefusalJoinError::WrongExchange));
        }
        let Some(bound) = self.enter_bound else {
            return Err((self, RefusalJoinError::WrongWindow));
        };
        if self.context.deadlines.get(PumpDeadline::Enter) != Some(bound) {
            return Err((self, RefusalJoinError::WrongWindow));
        }
        if self.context.enter_refusal_record.is_some()
            || self.context.waiting_content.is_some()
            || self.context.barrier.pending_author().is_some()
            || self.context.completed_author.is_some()
            || self.context.author_admission_pending
            || self.context.caller_author_held()
        {
            return Err((self, RefusalJoinError::Busy));
        }
        let Some(StartupStop::ClassifierRefused(source)) = self.stop.as_ref() else {
            unreachable!("checked actual startup refusal")
        };
        let envelope = match prepare_admission_refusal(&exchange, source) {
            Ok(envelope) => envelope,
            Err(error) => return Err((self, RefusalJoinError::Mapping(error))),
        };
        let weaver_types::Payload::Refusal(refusal) = &envelope.payload else {
            unreachable!("mapped lifecycle refusal")
        };
        let payload = match serde_json::value::to_raw_value(&RefusalRecord::Lifecycle {
            asked: LifecycleAsk::Enter,
            refusal: refusal.clone(),
        }) {
            Ok(payload) => payload,
            Err(error) => return Err((self, RefusalJoinError::TraceEncoding(error.to_string()))),
        };
        self.context.enter_refusal_record = Some(AuthorRecord::new());
        Ok(EnterRefusalComposition {
            startup: self,
            bound,
            envelope,
            original: Some(AuthoredEvent {
                kind: Kind::Refusal,
                subsystem: Subsystem::Harness,
                turn: None,
                source: AuthoredSource::Runtime,
                causal_parent: None,
                payload: Some(Payload::Refusal(payload)),
            }),
        })
    }
}

impl<'a> EnterRefusalComposition<'a> {
    pub(in crate::flow) fn advance_original_record(
        &mut self,
        listener: &mut crate::flow::pump::LifecycleListener,
        io: &mut impl PumpIo,
    ) -> Result<Sequence, RunFailure> {
        let mut original = self.startup.original_enter.take();
        let result = crate::flow::pump::enter::with_enter(
            original.as_mut(),
            LifecyclePorts {
                listener: Some(listener),
                ..Default::default()
            },
            |lifecycle, _| {
                self.advance_record(&mut PumpPorts {
                    lifecycle,
                    gate: None,
                    decode: None,
                    label: None,
                    io,
                })
            },
        );
        self.startup.original_enter = original;
        result.map_err(|failure| RunFailure::Transport {
            source: crate::flow::pump::PumpSource::Admin,
            failure,
        })?
    }

    pub(in crate::flow) fn original_bound(&self) -> AbsoluteBound {
        self.bound
    }

    /// Recorded custody survives a later expired Enter. It still authorizes no
    /// response after expiry; the response owner keeps it for actual rollback.
    #[allow(clippy::result_large_err)]
    pub(in crate::flow) fn finish_original(self) -> Result<RecordedStartupRefusal<'a>, Self> {
        if self.recorded_sequence().is_none()
            || self.startup.context.failure().is_some()
            || !self
                .startup
                .original_enter
                .as_ref()
                .is_some_and(|original| original.adopted_by(self.startup.context))
            || self.startup.context.phase != RecordingPhase::Entered
            || self.startup.context.deadlines.get(PumpDeadline::Enter) != Some(self.bound)
            || self
                .startup
                .context
                .deadlines
                .get(PumpDeadline::Leave)
                .is_some()
            || self.startup.context.boot_enter_cancelled
            || self.startup.context.place_admission_stopped()
        {
            return Err(self);
        }
        Ok(RecordedStartupRefusal { owner: self })
    }
    pub(in crate::flow) fn source(&self) -> &weaver_types::state_flow::Ste100AdmissionRefused {
        let Some(StartupStop::ClassifierRefused(source)) = self.startup.stop.as_ref() else {
            unreachable!("owner retains actual startup refusal")
        };
        source
    }

    pub(in crate::flow) fn recorded_sequence(&self) -> Option<Sequence> {
        self.startup.context.enter_refusal_record.as_ref()?.sequence
    }

    fn check(&mut self, ports: &mut PumpPorts<'_, impl PumpIo>) -> Result<(), RunFailure> {
        let context = &mut self.startup.context;
        if let Some(failure) = context.failure().cloned() {
            return Err(failure);
        }
        if context.phase != RecordingPhase::Entered
            || context.deadlines.get(PumpDeadline::Enter) != Some(self.bound)
            || context.deadlines.get(PumpDeadline::Leave).is_some()
        {
            return Err(RunFailure::EnterRefusalOwned);
        }
        context.observe_boot_lifecycle(&ports.lifecycle);
        context.observe_place_lifecycle(&ports.lifecycle);
        if context.boot_enter_cancelled || context.place_admission_stopped() {
            return Err(RunFailure::EnterRefusalOwned);
        }
        context.live(ports.io)
    }

    /// No new read, Open, child or admission. Only this exact original author may
    /// progress. The record's identity binds its allocated ticket before pumping,
    /// and its actual Recorded sequence before any fallible observer.
    pub(in crate::flow) fn advance_record(
        &mut self,
        ports: &mut PumpPorts<'_, impl PumpIo>,
    ) -> Result<Sequence, RunFailure> {
        self.check(ports)?;
        if let Some(sequence) = self.recorded_sequence() {
            if self.startup.context.completed_author == Some(sequence) {
                self.startup.context.completed_author.take();
            }
            return Ok(sequence);
        }
        let context = &mut self.startup.context;
        let record = context
            .enter_refusal_record
            .as_ref()
            .expect("exclusive owner");
        record.returned.check()?;
        let claim = AuthorClaim::EnterRefusal(Arc::clone(&record.identity));
        if let Some(original) = self.original.take() {
            if context.waiting_content.is_some() || context.barrier.pending_author().is_some() {
                self.original = Some(original);
                return Err(RunFailure::EnterRefusalOwned);
            }
            context.waiting_content = Some(WaitingAuthor::claimed(original, claim.clone()));
        }
        if context.waiting_content.is_none()
            && (record.ticket.is_none()
                || context.barrier.pending_author().map(|author| author.ticket) != record.ticket)
        {
            return Err(context.fail(
                ports.io,
                FlowHaltReason::ProtocolFailed,
                HaltPoint::Author,
                "Enter refusal lost its exact queued author",
            ));
        }
        let sequence = context
            .resume_claimed_author_and_wait(ports, Some(claim))
            .map_err(|failure| {
                context
                    .enter_refusal_record
                    .as_mut()
                    .unwrap()
                    .retain_failure(failure)
            })?;
        if context.enter_refusal_record.as_ref().unwrap().sequence != Some(sequence) {
            return Err(context.fail(
                ports.io,
                FlowHaltReason::ProtocolFailed,
                HaltPoint::Author,
                "Enter refusal completion differs from its actual Recorded ticket",
            ));
        }
        Ok(sequence)
    }
}

impl<'a> EnterRefusalComposition<'a> {
    #[allow(clippy::result_large_err)]
    pub(in crate::flow) fn finish(
        mut self,
        ports: &mut PumpPorts<'_, impl PumpIo>,
    ) -> Result<RecordedStartupRefusal<'a>, Self> {
        if self.recorded_sequence().is_none() || self.check(ports).is_err() {
            return Err(self);
        }
        Ok(RecordedStartupRefusal { owner: self })
    }
}

impl RecordedStartupRefusal<'_> {
    pub(in crate::flow) fn envelope(&self) -> &OrganEnvelope {
        &self.owner.envelope
    }
    pub(in crate::flow) fn sequence(&self) -> Sequence {
        self.owner
            .recorded_sequence()
            .expect("retained actual Recorded witness")
    }
    pub(in crate::flow) fn source(&self) -> &weaver_types::state_flow::Ste100AdmissionRefused {
        self.owner.source()
    }
}

impl RecordingContext {
    pub(in crate::flow::context) fn bind_enter_refusal_author(&mut self, sequence: Sequence) {
        let ticket = self.barrier.recorded_author_ticket(sequence);
        if let Some(record) = self.enter_refusal_record.as_mut() {
            record.bind(ticket, sequence);
        }
    }
}

#[cfg(test)]
#[path = "context_enter_refusal_composition_tests.rs"]
pub(super) mod tests;

#[path = "context_rollback_composition.rs"]
pub(in crate::flow) mod response;

#[cfg(test)]
impl RetiredRefusal {
    pub(in crate::flow::context) fn retained_enter_wire(&self) -> &[u8] {
        self.original_enter.as_ref().unwrap().retained_wire()
    }
}
