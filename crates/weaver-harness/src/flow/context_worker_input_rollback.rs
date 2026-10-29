//! Move the real input account out of its serving loans. Rollback borrows the
//! original run's ports for one reduction; it never reconstructs a Ready seat.
use super::*;
use crate::flow::context::RecordingContext;
use crate::flow::context::worker_startup::expired::gate::RollbackGateClosed;
use crate::flow::input::InputState;
use crate::flow::pump_transport::{DecodePort, OrganPort};
use crate::flow::templates::TemplateCertificate;
use weaver_types::state_flow::NonDispatchCause;
use weaver_types::{Cause, TurnFrame};

pub(in crate::flow::context::worker_startup) struct RollbackInputWork {
    stage: Stage,
    received: Option<ReceivedOrgan>,
    _token: Option<AcceptedInput>,
    _turn: Option<TurnKey>,
    _seated: Option<SeatedInput>,
    close: Option<OrganEnvelope>,
    send: Option<OrganSend>,
    report: Option<report::Report>,
    _failure: Option<RunFailure>,
    engine: Option<Box<generation::EngineWork>>,
    world: Option<close::WorldClose>,
    _displaced_world: Option<close::WorldClose>,
    _recorded_report: Option<report::Report>,
    _placement_close: Option<TurnFrame>,
    complete: bool,
}

impl<'a> FoundationInput<'a> {
    pub(in crate::flow::context::worker_startup) fn rollback_admissible(&self) -> bool {
        self.failure.is_none() && self.stage != Stage::Stopped
    }

    pub(in crate::flow::context::worker_startup) fn into_rollback_parts(
        self: Box<Self>,
    ) -> (FoundationServing<'a>, RollbackInputWork) {
        let Self {
            serving,
            stage,
            received,
            token,
            turn,
            seated,
            close,
            send,
            report,
            failure,
            engine,
        } = *self;
        (
            serving,
            RollbackInputWork {
                stage,
                received,
                _token: token,
                _turn: turn,
                _seated: seated,
                close,
                send,
                report,
                _failure: failure,
                engine,
                world: None,
                _displaced_world: None,
                _recorded_report: None,
                _placement_close: None,
                complete: false,
            },
        )
    }
}

impl RollbackInputWork {
    pub(in crate::flow::context::worker_startup) fn original_input(
        &self,
    ) -> Option<&ReceivedOrgan> {
        self.received.as_ref()
    }

    pub(in crate::flow::context::worker_startup) fn retire_failed_placement_grants(
        &mut self,
        context: &mut RecordingContext,
    ) -> Result<(), RunFailure> {
        if let Some(engine) = &mut self.engine {
            engine.retire_failed_placement_grants(context)?;
        }
        Ok(())
    }

    #[cfg(test)]
    pub(in crate::flow::context::worker_startup) fn test_checked_source(
        &self,
    ) -> Option<(&TurnKey, &weaver_types::state_flow::SourceBlockId)> {
        Some((self._turn.as_ref()?, &self._seated.as_ref()?.source))
    }

    #[cfg(test)]
    pub(in crate::flow::context::worker_startup) fn test_received(&self) -> Option<&ReceivedOrgan> {
        self.received.as_ref()
    }
    #[cfg(test)]
    pub(in crate::flow::context::worker_startup) fn test_engine_identity(&self) -> Option<usize> {
        self.engine
            .as_deref()
            .map(|engine| std::ptr::from_ref(engine) as usize)
    }
    #[cfg(test)]
    pub(in crate::flow::context::worker_startup) fn test_token_revoked(&self) -> bool {
        self._token.as_ref().is_none_or(AcceptedInput::test_revoked)
    }
    #[cfg(test)]
    pub(in crate::flow::context::worker_startup) fn test_world(
        &self,
    ) -> Option<(Option<weaver_trace::Sequence>, bool, bool)> {
        self.world.as_ref().map(|world| {
            (
                world.recorded(),
                world.send().is_some_and(OrganSend::sent),
                world.send().is_some_and(OrganSend::withdrawn),
            )
        })
    }

    pub(in crate::flow::context::worker_startup) fn take_unoffered(
        &mut self,
    ) -> Option<ReceivedOrgan> {
        (self.stage == Stage::Waiting)
            .then(|| self.received.take())
            .flatten()
    }

    pub(in crate::flow::context::worker_startup) fn cancel_control(
        &self,
    ) -> Option<(&OrganEnvelope, &OrganSend)> {
        self.engine
            .as_ref()
            .and_then(|engine| engine.cancel_control())
    }

    pub(in crate::flow::context::worker_startup) fn cancel(
        &mut self,
        context: &mut RecordingContext,
        decode: Option<&mut DecodePort>,
        gate: Option<&mut OrganPort>,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        if let Some(engine) = &mut self.engine {
            if self.world.is_none()
                && let Some(close) = engine.take_close()
            {
                if close.started() {
                    self.world = Some(close);
                } else {
                    self._displaced_world = Some(close);
                }
            }
            engine.cancel_for(NonDispatchCause::Unload, context, decode, gate, io)?;
        }
        // A task may still hold the exact accepted token. Its cancellation
        // settles that account first; no token is consumed by this adapter.
        if context.marks.is_none()
            && context
                .input
                .snapshot()
                .is_some_and(|input| input.state != InputState::Consumed && !input.cancelled)
        {
            context.cancel_input(io)?;
        }
        Ok(())
    }

    pub(in crate::flow::context::worker_startup) fn prepare_control(
        &mut self,
        context: &mut RecordingContext,
        gate: &mut OrganPort,
        io: &mut impl PumpIo,
    ) -> Result<bool, RunFailure> {
        if self
            .world
            .as_ref()
            .and_then(|world| world.send())
            .is_some_and(|send| send.sent() && send.sent_at().is_none())
            || self
                .send
                .as_ref()
                .is_some_and(|send| send.sent() && send.sent_at().is_none())
        {
            return Err(context.fail(
                io,
                FlowHaltReason::TraceFailed,
                HaltPoint::Author,
                "rollback retained a world send without its clock observation",
            ));
        }
        if let Some(world) = &mut self.world
            && !world.prepare_rollback_transport(gate)?
        {
            return Ok(false);
        }
        if let Some(send) = &self.send {
            let Some(request) = &self.received else {
                return Err(RunFailure::PlaceOwned);
            };
            if !send.belongs_to(gate) || !send.matches_exchange(&request.envelope().exchange) {
                return Err(RunFailure::PlaceOwned);
            }
            if send.sent()
                && context
                    .input
                    .snapshot()
                    .is_some_and(|input| !input.response_sent)
            {
                context.input_response_sent(io)?;
            }
            if !send.sent() && !send.withdrawn() {
                match gate.withdraw_pending(&request.envelope().exchange) {
                    Ok(Some(close)) => self.close = Some(close),
                    Ok(None) => return Ok(false),
                    Err(failure) => {
                        return Err(RunFailure::Transport {
                            source: PumpSource::Gate,
                            failure,
                        });
                    }
                }
            }
        }
        if let Some(engine) = &mut self.engine {
            if let Some((control, send)) = engine.cancel_control()
                && send.sent()
            {
                if !send.belongs_to(gate) || !send.matches_exchange(&control.exchange) {
                    return Err(RunFailure::PairOwned);
                }
                if send.sent_at().is_none() {
                    return Err(context.fail(
                        io,
                        FlowHaltReason::TraceFailed,
                        HaltPoint::Author,
                        "rollback retained an execution control without its clock observation",
                    ));
                }
                return Ok(true);
            }
            engine.advance_cancel_control(context, gate, io)
        } else {
            Ok(true)
        }
    }

    pub(in crate::flow::context::worker_startup) fn route_reply(
        &mut self,
        context: &mut RecordingContext,
        gate: &mut OrganPort,
        io: &mut impl PumpIo,
    ) -> Result<bool, RunFailure> {
        if gate.held().is_none() {
            return Ok(false);
        }
        match &mut self.engine {
            Some(engine) => engine.route_cancel_reply(context, gate, io),
            None => Ok(false),
        }
    }

    pub(in crate::flow::context::worker_startup) fn advance<I: PumpIo>(
        &mut self,
        certificate: &mut Option<TemplateCertificate>,
        context: &mut RecordingContext,
        ports: &mut PumpPorts<'_, I>,
        lowered: Option<&RollbackGateClosed>,
        cause: Cause,
    ) -> Result<bool, RunFailure> {
        if self.complete {
            return Ok(true);
        }
        if let Some(report) = &mut self.report
            && report.advance(context, ports)?
        {
            self._recorded_report = self.report.take();
            return Ok(false);
        }
        if let Some(world) = &mut self.world {
            world.prepare_rollback_record(context, ports)?;
        }
        if let Some(engine) = &mut self.engine
            && !engine.advance_cancelled(certificate, context, ports)?
        {
            return Ok(false);
        }
        // Source authoring already started before Leave remains the same owed
        // operation. An elected but unconsumed token never enters this branch.
        if context.input_turn.is_some() {
            if let Some(seated) = context.resume_input_turn(ports)? {
                self._seated = Some(seated);
            } else {
                context.pump_once(ports)?;
            }
            return Ok(false);
        }
        if let Some(seated) = &self._seated {
            // Direct source authoring has no task receipt consumer. Keep its
            // actual seated source while the coordinator files the selected
            // copy using the already accepted input's single-use check. A
            // retention charge retires only with the actual Recorded terminal;
            // recording MessageUser alone is not this filing disposition.
            let progress = context.reduce_flow(ports.io)?;
            let source = crate::flow::intake::SourceEventKey {
                run: seated.source.run.clone(),
                sequence: seated.source.sequence.0,
            };
            if context.budget.reservation(&source).is_some()
                || context.budget.waiting_copy().is_some()
                || context.inbox.queued().next().is_some()
                || !context
                    .flow
                    .ledger()
                    .barrier_through(seated.source.sequence.0)
                || context.flow.has_pending_observations()
            {
                if progress != crate::flow::coordinator::Advance::Progress {
                    context.pump_once(ports)?;
                }
                return Ok(false);
            }
        }
        if context
            .input
            .snapshot()
            .is_some_and(|input| input.state == InputState::Consumed)
        {
            context.finish_input(ports.io)?;
        }
        if context.input.snapshot().is_some() {
            if !context.input.snapshot().unwrap().cancelled {
                context.cancel_input(ports.io)?;
            }
            if let Some(proof) = lowered {
                self.check_lower_source(proof, context, ports.io)?;
                context.input_closed_by_lower(proof, ports.io)?;
            }
            match context.advance_input(ports)? {
                InputProgress::Finished => {
                    context.finish_input(ports.io)?;
                }
                InputProgress::CloseReady => {
                    if self.close.is_none() {
                        let request = self.received.as_ref().expect("owned input source");
                        self.close = Some(OrganEnvelope {
                            exchange: request.envelope().exchange.clone(),
                            position: Position::Close,
                            payload: Payload::Frame(
                                context.input_close().expect("cancelled input close"),
                            ),
                        });
                    }
                    return Ok(false);
                }
                InputProgress::Pending => {
                    context.pump_once(ports)?;
                    return Ok(false);
                }
                _ => return Ok(false),
            }
        }
        if self.report.is_some() {
            return Ok(false);
        }
        let Some(proof) = lowered else {
            return Ok(false);
        };
        if let Some(request) = &self.received
            && !proof.owns(request)
        {
            return Err(RunFailure::PlaceOwned);
        }
        // A placement refusal already closed its own turn. Retain its original
        // response as lost to Lower, never call its successful-send hook.
        if context.place_close().is_some() {
            self._placement_close = context.place_close.take();
        }
        if self.world.is_none()
            && let Some(turn) = context.open_turn.clone()
        {
            let request = self
                .received
                .as_ref()
                .expect("open turn has its original input");
            self.world = Some(close::WorldClose::rollback(
                turn,
                request.envelope().exchange.clone(),
                cause,
            ));
        }
        if let Some(world) = &mut self.world {
            world.advance_after_lower(context, ports, proof)?;
        }
        self.complete = true;
        Ok(true)
    }

    fn check_lower_source(
        &self,
        proof: &RollbackGateClosed,
        context: &mut RecordingContext,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        if self
            .received
            .as_ref()
            .is_some_and(|source| proof.owns(source))
        {
            Ok(())
        } else {
            Err(context.fail(
                io,
                FlowHaltReason::ProtocolFailed,
                HaltPoint::Barrier,
                "cancelled input is not owned by this lowered gate",
            ))
        }
    }
}
