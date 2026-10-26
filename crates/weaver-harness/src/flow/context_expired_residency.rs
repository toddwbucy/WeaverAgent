//! One Release under an accepted rollback. The expired Admit is retained and
//! never resumes its write leg or grants a boot capability.
use super::*;
use crate::flow::context::worker_startup::expired::ExpiredReleasePermit;

pub(in crate::flow::context) struct ExpiredResidencyRelease {
    permit: ExpiredReleasePermit,
    prior: Option<ResidencyRequest>,
    prior_reply: Option<ReceivedOrgan>,
    request: Option<ResidencyRequest>,
    reply: Option<ReceivedOrgan>,
    refused: Option<LifecycleDirective>,
    outcome: Option<ReleaseOutcome>,
}

impl FailedEnterResources {
    #[allow(clippy::result_large_err)] // Return the actual prior reply on rejection.
    pub(in crate::flow::context) fn begin_expired_release(
        &mut self,
        context: &RecordingContext,
        permit: ExpiredReleasePermit,
        reply: Option<ReceivedOrgan>,
    ) -> Result<ExpiredResidencyRelease, (Option<ReceivedOrgan>, String)> {
        if !permit.allows(context)
            || !self.matches(context)
            || !self.original_ports()
            || !matches!(self.classifier_child(), Some(ChildState::Reaped(_)))
            || !context.flow.classifier_retired()
            || !self.spu.as_ref().is_some_and(|spu| {
                spu.retired_decode.is_none()
                    && spu.retired_residency.is_none()
                    && spu
                        .decode
                        .as_ref()
                        .is_some_and(|port| context.can_retire_boot_decode(port))
            })
        {
            return Err((
                reply,
                "expired Release requires settled original resources and actual classifier reap"
                    .into(),
            ));
        }
        let spu = self.spu.as_mut().unwrap();
        let prior = spu.residency_request.take();
        // Keep framing, reservation and send evidence after closing the seam.
        // The admission predicate above forbids this while Open is still owed.
        spu.retired_decode = spu.decode.take().map(DecodePort::retire_failed);
        Ok(ExpiredResidencyRelease {
            permit,
            prior,
            prior_reply: reply,
            request: None,
            reply: None,
            refused: None,
            outcome: None,
        })
    }

    pub(in crate::flow::context) fn poll_spu_child(&mut self) -> Option<ChildState> {
        self.spu.as_mut().map(|spu| spu.child.poll())
    }
}

impl ExpiredResidencyRelease {
    #[cfg(test)]
    pub(in crate::flow::context) fn received_reply(&self) -> Option<&OrganEnvelope> {
        self.reply.as_ref().map(ReceivedOrgan::envelope)
    }
    #[cfg(test)]
    pub(in crate::flow::context) fn release_sent(&self) -> bool {
        self.request
            .as_ref()
            .is_some_and(|request| request.send.sent())
    }
    #[cfg(test)]
    pub(in crate::flow::context) fn prior_reply(&self) -> Option<&OrganEnvelope> {
        self.prior_reply.as_ref().map(ReceivedOrgan::envelope)
    }
    pub(in crate::flow::context) fn outcome(&self) -> Option<ReleaseOutcome> {
        self.outcome
    }

    fn finish(&mut self, resources: &mut FailedEnterResources, outcome: ReleaseOutcome) {
        let spu = resources.spu.as_mut().unwrap();
        spu.retired_residency = spu.residency.take().map(OrganPort::retire_failed);
        self.outcome = Some(outcome);
    }

    pub(in crate::flow::context) fn advance(
        &mut self,
        resources: &mut FailedEnterResources,
        context: &mut RecordingContext,
        io: &mut impl PumpIo,
    ) -> Result<bool, RunFailure> {
        context.live(io)?;
        if !self.permit.allows(context) || !resources.matches(context) {
            return Err(context.fail(
                io,
                FlowHaltReason::ProtocolFailed,
                HaltPoint::Barrier,
                "expired Release lost its original accepted rollback",
            ));
        }
        if self.outcome.is_some() {
            return Ok(true);
        }
        let spu = resources.spu.as_ref().unwrap();
        let port = spu.residency.as_ref().unwrap();
        if self.prior.as_ref().is_some_and(|prior| !prior.send.sent()) {
            // Leave cannot complete any portion of an expired Admit. Closing
            // preserves its original bytes and reports no Release confirmation.
            self.finish(resources, ReleaseOutcome::Unconfirmed);
            return Ok(true);
        }
        if self.prior.as_ref().is_some_and(|prior| {
            !prior.send.belongs_to(port)
                || !prior
                    .send
                    .sent_at()
                    .is_some_and(|at| self.permit.original_send(at))
                || !matches!(
                    prior.envelope.payload,
                    weaver_types::Payload::Directive(LifecycleDirective::Admit { .. })
                )
        }) || self.prior_reply.as_ref().is_some_and(|reply| {
            !reply.belongs_to(port)
                || !self
                    .prior
                    .as_ref()
                    .is_some_and(|prior| prior.send.matches_exchange(&reply.envelope().exchange))
                || reply.envelope().position != Position::Close
                || !admit_reply_is_terminal(reply.envelope())
        }) {
            return Err(context.fail(
                io,
                FlowHaltReason::ProtocolFailed,
                HaltPoint::Barrier,
                "expired Release has invalid retained Admit evidence",
            ));
        }
        if self.request.is_none() {
            match resources.stage_residency_inner(LifecycleDirective::Release) {
                Ok(()) => self.request = resources.spu.as_mut().unwrap().residency_request.take(),
                Err((directive, detail)) => {
                    self.refused = Some(directive);
                    return Err(context.fail(
                        io,
                        FlowHaltReason::ProtocolFailed,
                        HaltPoint::Barrier,
                        format!("expired Release stage: {detail}"),
                    ));
                }
            }
        }
        let port = resources.spu.as_mut().unwrap().residency.as_mut().unwrap();
        // Never let a later write make an earlier reply a valid confirmation.
        let receive_failure = if port.held().is_none() {
            port.try_receive().err()
        } else {
            None
        };
        match port.take_received() {
            Ok(Some(reply)) => {
                let envelope = reply.envelope();
                if self.prior_reply.is_none()
                    && self.prior.as_ref().is_some_and(|prior| {
                        reply.belongs_to(port)
                            && prior.send.matches_exchange(&envelope.exchange)
                            && envelope.position == Position::Close
                    })
                {
                    let valid = admit_reply_is_terminal(envelope);
                    self.prior_reply = Some(reply);
                    if !valid {
                        return Err(context.fail(
                            io,
                            FlowHaltReason::ProtocolFailed,
                            HaltPoint::Barrier,
                            "expired Admit reply was neither Admitted nor an Admit refusal",
                        ));
                    }
                    return Ok(false);
                }
                let request = self.request.as_ref().unwrap();
                let exact = reply.belongs_to(port)
                    && request.send.belongs_to(port)
                    && request.send.sent()
                    && request
                        .send
                        .sent_at()
                        .is_some_and(|at| self.permit.release_send(at))
                    && request.send.matches_exchange(&envelope.exchange)
                    && envelope.position == Position::Close;
                let outcome = if exact {
                    match envelope.payload {
                        weaver_types::Payload::Answer(LifecycleAnswer::Released) => {
                            Some(ReleaseOutcome::Confirmed)
                        }
                        weaver_types::Payload::Refusal(
                            weaver_types::LifecycleRefusal::NoResidency,
                        ) => Some(ReleaseOutcome::Unconfirmed),
                        _ => None,
                    }
                } else {
                    None
                };
                self.reply = Some(reply);
                let Some(outcome) = outcome else {
                    return Err(context.fail(io, FlowHaltReason::ProtocolFailed, HaltPoint::Barrier,
                        "expired Release reply lacks the exact whole-send witness or terminal answer"));
                };
                self.finish(resources, outcome);
                Ok(true)
            }
            Ok(None) => {
                if let Some(failure) = receive_failure {
                    return Err(context.fail(
                        io,
                        FlowHaltReason::ProtocolFailed,
                        HaltPoint::Barrier,
                        format!("expired Release receive: {failure:?}"),
                    ));
                }
                context.live(io)?;
                port.try_send_observed(io).map_err(|failure| {
                    let unclocked = self.request.as_ref().is_some_and(|request| {
                        request.send.sent() && request.send.sent_at().is_none()
                    });
                    context.fail(
                        io,
                        if unclocked {
                            FlowHaltReason::TraceFailed
                        } else {
                            FlowHaltReason::ProtocolFailed
                        },
                        if unclocked {
                            HaltPoint::Author
                        } else {
                            HaltPoint::Barrier
                        },
                        format!("expired Release send: {failure:?}"),
                    )
                })?;
                Ok(false)
            }
            Err(failure) => Err(context.fail(
                io,
                FlowHaltReason::ProtocolFailed,
                HaltPoint::Barrier,
                format!("expired Release completion: {failure:?}"),
            )),
        }
    }

    pub(in crate::flow::context) fn wait(
        &self,
        resources: &FailedEnterResources,
        context: &mut RecordingContext,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        context.live(io)?;
        context.barrier.drain_capacity_wake().map_err(|error| {
            context.fail(
                io,
                FlowHaltReason::TraceFailed,
                HaltPoint::Author,
                format!("expired Release trace wake: {error:?}"),
            )
        })?;
        let port = resources.spu.as_ref().unwrap().residency.as_ref().unwrap();
        let interest = port.interest();
        let mut events = PollFlags::POLLHUP | PollFlags::POLLERR | PollFlags::POLLNVAL;
        if interest.readable {
            events |= PollFlags::POLLIN;
        }
        if interest.writable {
            events |= PollFlags::POLLOUT;
        }
        io.wait(
            &[
                WaitInterest {
                    fd: port.fd(),
                    events,
                },
                WaitInterest {
                    fd: context.barrier.capacity_fd(),
                    events: PollFlags::POLLIN,
                },
            ],
            Some(self.permit.deadline()),
        )
        .map_err(|error| {
            context.fail(
                io,
                FlowHaltReason::TraceFailed,
                HaltPoint::Author,
                format!("expired Release wait: {error}"),
            )
        })?;
        context.live(io)
    }
}
