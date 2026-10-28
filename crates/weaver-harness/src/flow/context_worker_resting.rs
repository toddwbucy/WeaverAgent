//! A refused graceful Leave ends at S9; a new command owns its own interval.
use super::*;
use crate::flow::context::startup_composition::refusal::response::listener::{
    RestingCommand, RestingRequest,
};
use crate::flow::context::worker_startup::input::save::{
    FailedSaveRequest, PendingSave, SaveProgress,
};
use weaver_types::state_flow::capture::{CaptureDirective, CapturePayload};

#[derive(Default)]
pub(super) struct RestingWork {
    // Keep the last completed refusal, including raw Aborted and lost delivery,
    // while this run stands. A later completed refusal can replace this account.
    pub(super) completed_capture: Option<(ServingCapture, RetiredLifecyclePeer)>,
    pub(super) command: Option<RestingCommand>,
    pub(super) retired_command: Option<crate::flow::context::startup_composition::refusal::response::listener::RetiredRestingCommand>,
    pub(super) save: Option<Box<PendingSave>>,
    pub(super) failed_save: Option<FailedSaveRequest>,
}

impl FoundationEnteredRollback {
    #[cfg(test)]
    pub(in crate::flow::context::worker_startup) fn test_resting_reply_held(&mut self) -> bool {
        self.resting
            .save
            .as_mut()
            .and_then(|save| save.port())
            .is_some_and(|peer| peer.capture_held().is_some())
    }
    pub(super) fn retain_resting_interrupt(
        &mut self,
        failure: &RunFailure,
        io: &mut impl PumpIo,
    ) -> Result<bool, RunFailure> {
        let run = self.run.as_mut().unwrap();
        if self
            .resting
            .save
            .as_mut()
            .is_some_and(|save| save.retain_interrupt(&mut run.context, failure))
        {
            return Ok(true);
        }
        let RunFailure::Transport { source, failure } = failure else {
            return Ok(false);
        };
        let (residency, decode) = run.resources.spu_ports_mut().expect("S9 retains SPU ports");
        run.context
            .observe_idle_spu_end(*source, failure, decode, residency, true, io)
    }
    pub(super) fn step_resting(&mut self, io: &mut impl PumpIo) -> Result<(), RunFailure> {
        let run = self.run.as_mut().unwrap();
        let dials = self.dials.as_mut().unwrap();
        assert!(self.gate_done && self.handoff.arms.port.is_none());
        assert!(run.context.deadlines.get(PumpDeadline::Leave).is_none());
        run.resources.refresh_spu_observation(&mut run.context);
        if let Some(save) = &mut self.resting.save {
            save.prepare_expiry(&mut run.context, io)?;
        }
        let (residency, decode) = run
            .resources
            .spu_ports_mut()
            .expect("S9 retains the admitted SPU");
        let mut lifecycle = dials.resting_ports();
        lifecycle.residency = Some(residency);
        match run.context.advance_leaving_organ_faults(
            &mut PumpPorts {
                lifecycle,
                gate: None,
                decode: Some(decode),
                label: None,
                io,
            },
            self.gate.as_ref().and_then(RollbackGate::world_closed),
        )? {
            serving_organ_loss::ObservationProgress::Complete => (),
            _ => {
                self.progress = FoundationEnteredRollbackProgress::Pending;
                return Ok(());
            }
        }
        let mut save_progress = false;
        if let Some(save) = &mut self.resting.save {
            let (residency, decode) = run.resources.spu_ports_mut().unwrap();
            let mut lifecycle = dials.resting_ports();
            lifecycle.residency = Some(residency);
            match save.advance(
                &mut run.context,
                PumpPorts {
                    lifecycle,
                    gate: None,
                    decode: Some(decode),
                    label: None,
                    io,
                },
                true,
            )? {
                SaveProgress::Complete => {
                    self.resting.save = None;
                    save_progress = true;
                }
                SaveProgress::Progress => save_progress = true,
                SaveProgress::Pending => (),
            }
        }
        let load = self
            .handoff
            .startup
            .loaded
            .as_deref()
            .expect("actual Ready load facts");
        if self.resting.command.is_none() {
            self.resting.command =
                dials.prepare_resting(&mut run.context, load, self.resting.save.is_none(), io)?;
        }
        let Some(command) = &mut self.resting.command else {
            if !save_progress {
                self.progress = FoundationEnteredRollbackProgress::Pending;
            }
            return Ok(());
        };
        match &command.request {
            RestingRequest::Leave(message) => {
                let weaver_types::Payload::Directive(weaver_types::LifecycleDirective::Leave {
                    forced,
                    rollback: false,
                    ..
                }) = message.envelope.payload
                else {
                    unreachable!("checked S9 Leave")
                };
                let leave = AbsoluteBound::after(
                    command.heard_ns,
                    weaver_types::LEAVE_BOUND_MS * 1_000_000,
                )
                .map_err(|error| {
                    run.context.fail(
                        io,
                        FlowHaltReason::ProtocolFailed,
                        HaltPoint::Barrier,
                        error.to_string(),
                    )
                })?;
                command
                    .peer
                    .enable_capture()
                    .map_err(|failure| RunFailure::Transport {
                        source: PumpSource::LeavePeer,
                        failure,
                    })?;
                run.context.arm(PumpDeadline::Leave, leave, io)?;
                let command = self.resting.command.take().unwrap();
                let RestingRequest::Leave(message) = command.request else {
                    unreachable!()
                };
                self.handoff.message = message;
                self.handoff.peer = Some(command.peer);
                self.handoff.peer_unavailable = false;
                self.handoff.leave = leave;
                self.handoff.kind = if forced {
                    LeaveKind::Forced
                } else {
                    LeaveKind::Graceful
                };
                dials.restart_leave(forced, run.context.serving_organs.no_residency());
                self.stage = CleanupStage::Capture;
                self.progress = FoundationEnteredRollbackProgress::Progress;
            }
            RestingRequest::Save(request) => {
                let CapturePayload::Directive(CaptureDirective::SavePoint { window, .. }) =
                    request.message.envelope.payload
                else {
                    unreachable!("checked S9 SavePoint")
                };
                command
                    .peer
                    .enable_capture()
                    .map_err(|failure| RunFailure::Transport {
                        source: PumpSource::LeavePeer,
                        failure,
                    })?;
                let capture = ServingCapture::queue_demand(
                    &mut run.context,
                    &command.peer,
                    request.message.envelope.exchange.clone(),
                    window,
                    command.heard_ns,
                    io,
                )?;
                let command = self.resting.command.take().unwrap();
                let RestingRequest::Save(request) = command.request else {
                    unreachable!()
                };
                self.resting.save =
                    Some(Box::new(PendingSave::new(command.peer, request, capture)));
                self.progress = FoundationEnteredRollbackProgress::Progress;
            }
        }
        Ok(())
    }

    pub(super) fn wait_resting(&mut self, io: &mut impl PumpIo) {
        let run = self.run.as_mut().unwrap();
        let (residency, decode) = run
            .resources
            .spu_ports_mut()
            .expect("S9 retains the admitted SPU");
        let mut lifecycle = self.dials.as_mut().unwrap().resting_ports();
        lifecycle.residency = Some(residency);
        lifecycle.leave_peer = self.resting.save.as_mut().and_then(|save| save.port());
        let result = run.context.pump_once(&mut PumpPorts {
            lifecycle,
            gate: None,
            decode: Some(decode),
            label: None,
            io,
        });
        if let Err(error) = result {
            match self.retain_resting_interrupt(&error, io) {
                Ok(true) => self.progress = FoundationEnteredRollbackProgress::Progress,
                Ok(false) => self.handle_error(error, io),
                Err(error) => self.handle_error(error, io),
            }
        }
    }
}
