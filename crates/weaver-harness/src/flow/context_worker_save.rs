//! One on-demand request remains with the serving seat while its current turn
//! runs. The original peer, request bytes and capture identity move together.
use super::*;
use crate::flow::context::RecordingContext;
use crate::flow::context::leave_capture::{FailedCapture, ServingCapture, ServingCaptureProgress};
use crate::flow::pump::{LifecyclePeer, PumpDeadline, ReceivedCapture, RetiredReceivedCapture};
use weaver_types::state_flow::capture::{
    CaptureEnvelope, CapturePayload, CaptureRefusal, SaveRefusal,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::flow::context::worker_startup) enum SaveProgress {
    Complete,
    Progress,
    Pending,
}

pub(in crate::flow::context::worker_startup) struct PendingSave {
    pub(super) peer: LifecyclePeer,
    pub(super) request: ReceivedCapture,
    pub(super) capture: ServingCapture,
    refusal: Option<CaptureEnvelope>,
    send: Option<OrganSend>,
}

pub(in crate::flow::context::worker_startup) struct FailedSaveRequest {
    _request: RetiredReceivedCapture,
    _refusal: Option<CaptureEnvelope>,
    _send: Option<OrganSend>,
}

impl PendingSave {
    pub(in crate::flow::context::worker_startup) fn prepare_expiry(
        &mut self,
        context: &mut RecordingContext,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        self.capture
            .prepare_retained_window_expiry(context, &mut self.peer, io)
    }

    pub(in crate::flow::context::worker_startup) fn retain_interrupt(
        &mut self,
        context: &mut RecordingContext,
        failure: &RunFailure,
    ) -> bool {
        self.capture.retain_queued_expiry(context, failure)
            || self.capture.retain_window_expiry(context, failure)
            || matches!(
                failure,
                RunFailure::Wait {
                    key: PumpDeadline::CaptureResult,
                    ..
                }
            )
            || self.capture.retain_peer_end(failure)
    }
    pub(in crate::flow::context::worker_startup) fn new(
        peer: LifecyclePeer,
        request: ReceivedCapture,
        capture: ServingCapture,
    ) -> Self {
        Self {
            peer,
            request,
            capture,
            refusal: None,
            send: None,
        }
    }

    pub(in crate::flow::context::worker_startup) fn port(&mut self) -> Option<&mut LifecyclePeer> {
        self.capture.peer_available().then_some(&mut self.peer)
    }

    pub(super) fn blocks_turn(&self) -> bool {
        self.capture.blocks_turn()
    }

    #[cfg(test)]
    pub(in crate::flow::context::worker_startup) fn reply_held(&self) -> bool {
        self.peer.capture_held().is_some()
    }

    #[cfg(test)]
    pub(in crate::flow::context::worker_startup) fn test_boundary_pending(&self) -> bool {
        self.capture.boundary_pending()
    }

    /// Final refusal carry is deliberately nonblocking. The save has already
    /// been refused and the current turn may proceed. The same peer remains in
    /// the common pump; no new save or reply allowance replaces its old window.
    fn send_refusal(
        &mut self,
        context: &mut RecordingContext,
        io: &mut impl PumpIo,
    ) -> Result<SaveProgress, RunFailure> {
        context.live(io)?;
        if !self.capture.peer_available() {
            return Ok(SaveProgress::Complete);
        }
        if self.refusal.is_none() && self.send.is_none() {
            self.refusal = Some(CaptureEnvelope {
                exchange: self.request.message.envelope.exchange.clone(),
                position: Position::Close,
                payload: CapturePayload::Refusal(CaptureRefusal::Save(
                    SaveRefusal::SavePointNotTaken {
                        missed: self.capture.refusal_leg().expect("recorded refused save"),
                    },
                )),
            });
        }
        if let Some(envelope) = self.refusal.take() {
            match self.peer.stage_capture(envelope) {
                Ok(TrackedStage::Queued(send)) => self.send = Some(send),
                Ok(TrackedStage::Busy(envelope)) => self.refusal = Some(envelope),
                Err(error) => {
                    self.refusal = Some(error.input);
                    return Err(RunFailure::Transport {
                        source: PumpSource::LeavePeer,
                        failure: error.failure,
                    });
                }
            }
        }
        if let Err(failure) = self.peer.try_send_observed(io) {
            if self
                .send
                .as_ref()
                .is_some_and(|send| send.sent() && send.sent_at().is_none())
            {
                return Err(context.fail(
                    io,
                    FlowHaltReason::TraceFailed,
                    HaltPoint::Author,
                    format!("save refusal send clock observation failed: {failure:?}"),
                ));
            }
            return Err(RunFailure::Transport {
                source: PumpSource::LeavePeer,
                failure,
            });
        }
        let Some(send) = self.send.as_ref().filter(|send| send.sent()) else {
            return Ok(SaveProgress::Pending);
        };
        if !self.peer.owns_send(send)
            || !send.matches_exchange(&self.request.message.envelope.exchange)
            || send.sent_at().is_none()
        {
            return Err(context.fail(
                io,
                FlowHaltReason::TraceFailed,
                HaltPoint::Author,
                "save refusal lost its original send or clock observation",
            ));
        }
        context.live(io)?;
        Ok(SaveProgress::Complete)
    }

    pub(in crate::flow::context::worker_startup) fn advance<I: PumpIo>(
        &mut self,
        context: &mut RecordingContext,
        ports: PumpPorts<'_, I>,
        at_boundary: bool,
    ) -> Result<SaveProgress, RunFailure> {
        if self.capture.refusal_leg().is_some() {
            return self.send_refusal(context, ports.io);
        }
        let PumpPorts {
            lifecycle,
            gate,
            decode,
            label,
            io,
        } = ports;
        {
            let mut ports = PumpPorts {
                lifecycle: LifecyclePorts {
                    leave_peer: Some(&mut self.peer),
                    ..lifecycle
                },
                gate,
                decode,
                label,
                io: &mut *io,
            };
            loop {
                let progress = self.capture.advance(context, &mut ports)?;
                if progress == ServingCaptureProgress::Published {
                    return Ok(SaveProgress::Complete);
                }
                if progress == ServingCaptureProgress::Refused {
                    break;
                }
                if at_boundary && self.capture.boundary_pending() {
                    match self.capture.begin_demand_boundary(context, ports.io) {
                        Ok(true) => continue,
                        Ok(false) | Err(RunFailure::Busy(_)) => (),
                        Err(failure) => return Err(failure),
                    }
                }
                if progress == ServingCaptureProgress::Progress {
                    continue;
                }
                if self.capture.blocks_turn() {
                    if !self.capture.peer_available() {
                        ports.lifecycle.leave_peer = None;
                    }
                    match context.pump_once(&mut ports) {
                        // The capture owner must reduce its missing-result timeout
                        // before this outer pump may classify the operation's outcome.
                        Err(RunFailure::Wait {
                            key: PumpDeadline::CaptureResult | PumpDeadline::CaptureWindow,
                            ..
                        }) => (),
                        Err(failure) => return Err(failure),
                        Ok(_) => (),
                    }
                    // This pump may retain the very reply the capture needs.
                    // Return to its reduction before any outer owner waits again.
                    return Ok(SaveProgress::Progress);
                }
                return Ok(SaveProgress::Pending);
            }
        }
        self.send_refusal(context, io)
    }

    pub(in crate::flow::context::worker_startup) fn into_failed(
        self,
        context: &RecordingContext,
    ) -> (Box<FailedCapture>, FailedSaveRequest) {
        let Self {
            peer,
            request,
            capture,
            refusal,
            send,
        } = self;
        let capture = match capture.into_failed(context, peer) {
            Ok(capture) => capture,
            Err(_) => panic!("checked original failed demand capture"),
        };
        (
            capture,
            FailedSaveRequest {
                _request: request.retire_failed(),
                _refusal: refusal,
                _send: send,
            },
        )
    }
}

impl FoundationInput<'_> {
    pub(super) fn prepare_save_window_expiry(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        if let Some(save) = &mut self.serving.save {
            save.capture.prepare_retained_window_expiry(
                self.serving.owner.context,
                &mut save.peer,
                io,
            )?;
        }
        Ok(())
    }

    pub(super) fn save_blocks_turn(&self) -> bool {
        self.serving
            .save
            .as_ref()
            .is_some_and(|save| save.blocks_turn())
    }

    pub(super) fn retain_save_interrupt(&mut self, failure: &RunFailure) -> bool {
        let Some(save) = self.serving.save.as_mut() else {
            return false;
        };
        save.capture
            .retain_queued_expiry(self.serving.owner.context, failure)
            || save
                .capture
                .retain_window_expiry(self.serving.owner.context, failure)
            || save.capture.retain_peer_end(failure)
    }

    pub(super) fn advance_save(
        &mut self,
        io: &mut impl PumpIo,
        organs_settled: bool,
    ) -> Result<SaveProgress, RunFailure> {
        // Requested can become QueueBoundary in this same advance call, so
        // permission must reach the boundary election, not just entry here.
        let at_boundary = organs_settled
            && matches!(
                self.stage,
                Stage::Waiting | Stage::Completed | Stage::Refused | Stage::Aborted
            );
        let Some(save) = &mut self.serving.save else {
            return Ok(SaveProgress::Complete);
        };
        let owner = &mut self.serving.owner;
        let progress = save.advance(
            owner.context,
            PumpPorts {
                lifecycle: LifecyclePorts {
                    listener: Some(&mut owner.listener),
                    residency: Some(owner.resident.residency),
                    ..Default::default()
                },
                gate: Some(&mut owner.gate),
                decode: Some(owner.decode),
                label: None,
                io,
            },
            at_boundary,
        )?;
        if progress == SaveProgress::Complete {
            self.serving.save = None;
        }
        Ok(progress)
    }
}
