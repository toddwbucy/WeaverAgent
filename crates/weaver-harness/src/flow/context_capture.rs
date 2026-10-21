//! Harness/member half of PERSIST-SAVE. The lifecycle caller owns Beginning,
//! publication and final save records. This owner freezes the actual recording
//! frontier and never publishes an image or grants a new turn on its own.

use super::*;
use crate::flow::coordinator::{CapturePhase, CaptureSnapshot, OperationStart};
use weaver_types::state_flow::{
    DecimalU64, Digest, SaveWindow, StateControl, StateControlAnswer, StateFrame,
};

const SMALL_CONTROL_NS: u64 = 2_000_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Barrier,
    Snapshot,
    Held,
    Acknowledge,
    Finished,
    Aborting,
    Releasing,
    ReleaseCleanup,
    Complete,
}

pub(super) struct MemberCapture {
    pub(super) id: DecimalU64,
    window: AbsoluteBound,
    snapshot_wait: u64,
    phase: Phase,
    leg_bound: Option<AbsoluteBound>,
    answer: Option<CaptureAnswer>,
    refusal: Option<CaptureAnswer>,
    timeout: Option<CaptureTimeout>,
}

#[derive(Debug)]
pub(in crate::flow) struct CaptureAnswer {
    pub request: StateControl,
    pub answer: StateFrame,
    pub recorded_ns: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) struct CaptureTimeout {
    pub phase: CapturePhase,
    pub bound: AbsoluteBound,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum CaptureProgress {
    Progress,
    Pending,
    ImageReady,
    FinishedReady,
    Complete,
}

impl RecordingContext {
    /// Called only after the parent carries SavePointBeginning completely. The
    /// caller cannot choose a smaller sequence and thereby omit owed content.
    /// SavePoint records themselves are untapped observations.
    pub(super) fn prepare_member_capture(
        &mut self,
        id: DecimalU64,
        window: SaveWindow,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        self.live(io)?;
        if self.phase != RecordingPhase::Entered
            || self.member_capture.is_some()
            || self.input.snapshot().is_some()
            || self.input_turn.is_some()
            || self.open_turn.is_some()
            || self.waiting_content.is_some()
            || self.completed_author.is_some()
            || (self.barrier.pending_author().is_some() && !self.flow.has_pending_record())
            || self.deadlines.get(PumpDeadline::Gate).is_some()
            || self.deadlines.get(PumpDeadline::Cut).is_some()
            || self.deadlines.get(PumpDeadline::StateControl).is_some()
        {
            return Err(self.capture_fault(io, "capture requires a free between-turn boundary"));
        }
        let Some(duty) = self.load_duty.as_ref() else {
            return Err(self.capture_fault(io, "capture has no recorded persistence declaration"));
        };
        let save_wait = duty.persistence.save_wait.elapsed_ns.0;
        let snapshot_wait = duty.persistence.snapshot_wait.elapsed_ns.0;
        if save_wait == 0
            || snapshot_wait == 0
            || duty.persistence.save_wait.measurement_ref.trim().is_empty()
            || duty
                .persistence
                .snapshot_wait
                .measurement_ref
                .trim()
                .is_empty()
        {
            return Err(self.capture_fault(io, "capture declaration lacks measured bounds"));
        }
        window
            .validate()
            .map_err(|error| self.capture_fault(io, error.to_string()))?;
        let declared_end = window
            .began_ns
            .0
            .checked_add(save_wait)
            .ok_or_else(|| self.capture_fault(io, "capture window overflow"))?;
        let allowed_end = self
            .deadlines
            .get(PumpDeadline::Leave)
            .map_or(declared_end, |leave| declared_end.min(leave.deadline_ns()));
        // An on-demand window keeps its full declared duration, even if a later
        // Leave is now earlier. A leave capture is capped at its shared tail.
        if window.began_ns.0 > self.last_ns
            || (window.deadline_ns.0 != declared_end && window.deadline_ns.0 != allowed_end)
        {
            return Err(self.capture_fault(io, "capture window differs from its declaration"));
        }
        let window = AbsoluteBound::from_absolute(window.began_ns.0, window.deadline_ns.0)
            .map_err(|error| self.capture_fault(io, error.to_string()))?;
        if window.expired(self.last_ns) {
            return Err(RunFailure::Wait {
                key: PumpDeadline::CaptureWindow,
                bound: window,
            });
        }
        let frontier = self
            .last_author_sequence
            .ok_or_else(|| self.capture_fault(io, "capture has no recorded frontier"))?
            .0;
        let result = self.flow.register_capture_barrier(
            &mut FlowRecording {
                barrier: &mut self.barrier,
                inbox: &mut self.inbox,
                budget: &mut self.budget,
                author: &self.author,
            },
            id,
            frontier,
        );
        result.map_err(|error| self.flow_failed(io, error))?;
        self.member_capture = Some(MemberCapture {
            id,
            window,
            snapshot_wait,
            phase: Phase::Barrier,
            leg_bound: None,
            answer: None,
            refusal: None,
            timeout: None,
        });
        self.arm_capture_wait(PumpDeadline::CaptureWindow, window, io)?;
        let cut = AbsoluteBound::after(self.last_ns, self.declaration.cut_wait.elapsed_ns.0)
            .map_err(|error| self.capture_fault(io, error.to_string()))?;
        self.arm_capture_wait(PumpDeadline::Cut, cut, io)
    }

    pub(super) fn capture_snapshot(&self) -> Option<CaptureSnapshot> {
        self.flow.capture_snapshot()
    }

    pub(super) fn capture_timeout(&self) -> Option<CaptureTimeout> {
        self.member_capture.as_ref().and_then(|owner| owner.timeout)
    }

    /// A timely explicit refusal and a missing result remain different causes.
    /// Neither licenses the parent to resume before recorded Aborted.
    pub(super) fn capture_refusal(&self) -> Option<&CaptureAnswer> {
        self.member_capture.as_ref().and_then(|owner| {
            (owner.phase == Phase::Complete)
                .then_some(owner.refusal.as_ref())
                .flatten()
        })
    }

    pub(super) fn advance_member_capture(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<CaptureProgress, RunFailure> {
        self.settle_recorded_capture(io)?;
        self.capture_live(io)?;
        let phase = self
            .member_capture
            .as_ref()
            .map(|owner| owner.phase)
            .ok_or_else(|| self.capture_fault(io, "capture progress has no owner"))?;
        if phase == Phase::Barrier {
            let ready = self
                .flow
                .capture_barrier_ready(&mut FlowRecording {
                    barrier: &mut self.barrier,
                    inbox: &mut self.inbox,
                    budget: &mut self.budget,
                    author: &self.author,
                })
                .map_err(|error| self.flow_failed(io, error))?;
            if ready {
                self.deadlines.disarm(PumpDeadline::Cut);
                let (id, duration) = self
                    .member_capture
                    .as_ref()
                    .map(|owner| (owner.id, owner.snapshot_wait))
                    .expect("capture owner");
                let result = self
                    .flow
                    .begin_capture_snapshot(
                        &mut FlowRecording {
                            barrier: &mut self.barrier,
                            inbox: &mut self.inbox,
                            budget: &mut self.budget,
                            author: &self.author,
                        },
                        io,
                        id,
                    )
                    .map_err(|error| self.flow_failed(io, error))?;
                if !matches!(result, OperationStart::Started { .. }) {
                    return Err(self.capture_fault(io, "settled capture refused snapshot entry"));
                }
                self.begin_capture_wait(Phase::Snapshot, duration, io)?;
                return Ok(CaptureProgress::Progress);
            }
        }
        let advance = match self.reduce_flow(io) {
            Ok(advance) => advance,
            Err(RunFailure::Wait {
                key: PumpDeadline::CaptureResult | PumpDeadline::CaptureWindow,
                ..
            }) => {
                self.capture_live(io)?;
                Advance::Progress
            }
            Err(error) => return Err(error),
        };
        let owner = self
            .member_capture
            .as_ref()
            .expect("capture owner retained");
        Ok(match owner.phase {
            Phase::Held => CaptureProgress::ImageReady,
            Phase::Finished => CaptureProgress::FinishedReady,
            Phase::Complete => CaptureProgress::Complete,
            _ if advance == Advance::Progress => CaptureProgress::Progress,
            _ => CaptureProgress::Pending,
        })
    }

    pub(super) fn take_capture_answer(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<Option<CaptureAnswer>, RunFailure> {
        self.settle_recorded_capture(io)?;
        self.capture_live(io)?;
        if self.barrier.pending().is_some() || self.barrier.pending_author().is_some() {
            return Ok(None);
        }
        Ok(self
            .member_capture
            .as_mut()
            .and_then(|owner| owner.answer.take()))
    }

    pub(super) fn acknowledge_member_capture(
        &mut self,
        id: DecimalU64,
        digest: Digest,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        self.require_member_capture(id, Phase::Held, io)?;
        let result = self
            .flow
            .acknowledge_capture(
                &mut FlowRecording {
                    barrier: &mut self.barrier,
                    inbox: &mut self.inbox,
                    budget: &mut self.budget,
                    author: &self.author,
                },
                io,
                id,
                digest,
            )
            .map_err(|error| self.flow_failed(io, error))?;
        if !matches!(result, OperationStart::Started { .. }) {
            return Err(self.capture_fault(io, "capture acknowledge has an unsettled predecessor"));
        }
        self.begin_capture_wait(Phase::Acknowledge, SMALL_CONTROL_NS, io)
    }

    pub(super) fn release_member_capture(
        &mut self,
        id: DecimalU64,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        self.require_member_capture(id, Phase::Finished, io)?;
        let result = self
            .flow
            .release_capture(
                &mut FlowRecording {
                    barrier: &mut self.barrier,
                    inbox: &mut self.inbox,
                    budget: &mut self.budget,
                    author: &self.author,
                },
                io,
                id,
            )
            .map_err(|error| self.flow_failed(io, error))?;
        if !matches!(result, OperationStart::Started { .. }) {
            return Err(self.capture_fault(io, "capture release has an unsettled predecessor"));
        }
        self.begin_capture_wait(Phase::Releasing, SMALL_CONTROL_NS, io)
    }

    /// Parent cancellation is allowed only before Commit. This member owner
    /// cannot infer that lifecycle fact from a store reply. Barrier cancellation
    /// requires the parent to record refusal first and starts no member request.
    pub(super) fn cancel_member_capture(
        &mut self,
        id: DecimalU64,
        io: &mut impl PumpIo,
    ) -> Result<Advance, RunFailure> {
        self.settle_recorded_capture(io)?;
        if self
            .member_capture
            .as_ref()
            .is_none_or(|owner| owner.id != id)
        {
            return Err(self.capture_fault(io, "cancellation names another capture"));
        }
        self.allow_capture_cleanup(io)?;
        if self.member_capture.as_ref().expect("checked capture").phase == Phase::Barrier {
            self.flow
                .cancel_capture_barrier(
                    &mut FlowRecording {
                        barrier: &mut self.barrier,
                        inbox: &mut self.inbox,
                        budget: &mut self.budget,
                        author: &self.author,
                    },
                    id,
                )
                .map_err(|error| self.flow_failed(io, error))?;
            self.deadlines.disarm(PumpDeadline::Cut);
            self.deadlines.disarm(PumpDeadline::CaptureWindow);
            self.member_capture = None;
            return Ok(Advance::Progress);
        }
        let phase = self.member_capture.as_ref().expect("capture owner").phase;
        if matches!(phase, Phase::Aborting | Phase::ReleaseCleanup) {
            return Ok(Advance::Pending);
        }
        if phase == Phase::Complete {
            return Ok(Advance::Progress);
        }
        if phase == Phase::Finished
            || (phase == Phase::Acknowledge
                && self.deadlines.get(PumpDeadline::CaptureResult).is_none())
        {
            let accepted = self
                .flow
                .enqueue_capture_release(
                    &mut FlowRecording {
                        barrier: &mut self.barrier,
                        inbox: &mut self.inbox,
                        budget: &mut self.budget,
                        author: &self.author,
                    },
                    io,
                    id,
                )
                .map_err(|error| self.flow_failed(io, error))?;
            if accepted {
                self.deadlines.disarm(PumpDeadline::CaptureWindow);
                self.member_capture.as_mut().expect("capture owner").answer = None;
                self.begin_capture_wait(Phase::ReleaseCleanup, SMALL_CONTROL_NS, io)?;
                return Ok(Advance::Progress);
            }
        }
        self.enqueue_capture_abort(io)?;
        Ok(Advance::Progress)
    }

    /// Parent has recorded the final disposition and consumed the exact cleanup
    /// or release answer. Only this explicit finish removes the admission hold.
    pub(super) fn finish_member_capture(
        &mut self,
        id: DecimalU64,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        self.require_member_capture(id, Phase::Complete, io)?;
        self.flow
            .finish_capture(
                &mut FlowRecording {
                    barrier: &mut self.barrier,
                    inbox: &mut self.inbox,
                    budget: &mut self.budget,
                    author: &self.author,
                },
                id,
            )
            .map_err(|error| self.flow_failed(io, error))?;
        self.deadlines.disarm(PumpDeadline::CaptureWindow);
        self.member_capture = None;
        Ok(())
    }

    pub(super) fn settle_recorded_capture(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<bool, RunFailure> {
        if matches!(self.flow.operation_completion(),
            Some(OperationCompletion::Capture { completion, .. }) if completion.discard_capture_result)
        {
            if self
                .member_capture
                .as_ref()
                .is_none_or(|owner| !matches!(owner.phase, Phase::Aborting | Phase::ReleaseCleanup))
            {
                return Err(self.capture_fault(io, "discarded capture result has no cleanup owner"));
            }
            // This original leg was already Recorded before another author
            // blocked its consumer. Abort retains that exact result until the
            // author's proof permits reduction. It cannot end the cleanup wait.
            let result = self
                .flow
                .take_operation_completion(&mut FlowRecording {
                    barrier: &mut self.barrier,
                    inbox: &mut self.inbox,
                    budget: &mut self.budget,
                    author: &self.author,
                })
                .map_err(|error| self.flow_failed(io, error))?;
            return Ok(result.is_some());
        }
        let Some(OperationCompletion::Capture { recorded_ns, .. }) =
            self.flow.operation_completion()
        else {
            // The proof may already exist while the channel retries an
            // interrupted release. Later consumer scheduling cannot expire it.
            if let Some(recorded_ns) = self.flow.capture_recorded_ns()
                && let Some(owner) = &self.member_capture
                && let Some(bound) = owner.leg_bound
                && !bound.expired(recorded_ns)
            {
                let key = if matches!(
                    owner.phase,
                    Phase::Aborting | Phase::Releasing | Phase::ReleaseCleanup
                ) {
                    PumpDeadline::StateControl
                } else {
                    PumpDeadline::CaptureResult
                };
                self.deadlines.disarm(key);
            }
            return Ok(false);
        };
        let recorded_ns = *recorded_ns;
        let (phase, bound) = self
            .member_capture
            .as_ref()
            .and_then(|owner| owner.leg_bound.map(|bound| (owner.phase, bound)))
            .ok_or_else(|| self.capture_fault(io, "capture result has no owned wait"))?;
        let key = if matches!(
            phase,
            Phase::Aborting | Phase::Releasing | Phase::ReleaseCleanup
        ) {
            PumpDeadline::StateControl
        } else {
            PumpDeadline::CaptureResult
        };
        if !bound.expired(recorded_ns) {
            self.deadlines.disarm(key);
        }
        let completion = self
            .flow
            .take_operation_completion(&mut FlowRecording {
                barrier: &mut self.barrier,
                inbox: &mut self.inbox,
                budget: &mut self.budget,
                author: &self.author,
            })
            .map_err(|error| self.flow_failed(io, error))?;
        if completion.is_none() {
            return Ok(false);
        }
        let Some(OperationCompletion::Capture {
            request,
            completion,
            ..
        }) = completion
        else {
            return Err(self.capture_fault(io, "capture completion changed owner"));
        };
        let result = CaptureAnswer {
            request,
            answer: completion.answer,
            recorded_ns,
        };
        if bound.expired(recorded_ns) {
            if matches!(phase, Phase::Snapshot | Phase::Acknowledge) {
                self.note_capture_timeout(phase, bound);
                self.enqueue_capture_abort(io)?;
                return Ok(true);
            }
            // Retain the exact late terminal result in the failed owner.
            self.member_capture.as_mut().expect("capture owner").answer = Some(result);
            return Err(self.expired(io, PumpDeadline::StateControl, bound));
        }
        self.deadlines.disarm(key);
        let StateFrame::ControlAnswer { frame, .. } = &result.answer else {
            return Err(self.capture_fault(io, "capture result is not a control answer"));
        };
        let next = match (phase, frame) {
            (Phase::Snapshot, StateControlAnswer::Snapshot { .. }) => Phase::Held,
            (Phase::Acknowledge, StateControlAnswer::Finished { .. }) => Phase::Finished,
            (Phase::Aborting, StateControlAnswer::Aborted { .. })
            | (
                Phase::Releasing | Phase::ReleaseCleanup,
                StateControlAnswer::CaptureReleased { .. },
            ) => Phase::Complete,
            (Phase::Snapshot | Phase::Acknowledge, StateControlAnswer::ControlRefused { .. }) => {
                self.member_capture.as_mut().expect("capture owner").refusal = Some(result);
                self.enqueue_capture_abort(io)?;
                return Ok(true);
            }
            (
                Phase::Releasing | Phase::ReleaseCleanup,
                StateControlAnswer::ControlRefused { .. },
            ) => {
                self.member_capture.as_mut().expect("capture owner").answer = Some(result);
                return Err(self.fail(
                    io,
                    FlowHaltReason::ProtocolFailed,
                    HaltPoint::StateReply,
                    "member refused capture cleanup",
                ));
            }
            _ => {
                self.member_capture.as_mut().expect("capture owner").answer = Some(result);
                return Err(self.capture_fault(io, "capture completion differs from the owned leg"));
            }
        };
        let owner = self.member_capture.as_mut().expect("capture owner");
        owner.phase = next;
        owner.leg_bound = None;
        owner.answer = Some(result);
        Ok(true)
    }

    fn require_member_capture(
        &mut self,
        id: DecimalU64,
        phase: Phase,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        self.settle_recorded_capture(io)?;
        self.capture_live(io)?;
        if self
            .member_capture
            .as_ref()
            .is_none_or(|owner| owner.id != id || owner.phase != phase || owner.answer.is_some())
        {
            return Err(
                self.capture_fault(io, "capture continuation lacks its consumed predecessor")
            );
        }
        Ok(())
    }

    fn capture_live(&mut self, io: &mut impl PumpIo) -> Result<(), RunFailure> {
        match self.live(io) {
            Err(RunFailure::Wait {
                key: PumpDeadline::CaptureResult | PumpDeadline::CaptureWindow,
                bound,
            }) if self.member_capture.as_ref().is_some_and(|owner| {
                matches!(owner.phase, Phase::Snapshot | Phase::Acknowledge)
            }) && self.deadlines.get(PumpDeadline::CaptureResult).is_some() =>
            {
                let phase = self.member_capture.as_ref().expect("capture owner").phase;
                let bound = self
                    .member_capture
                    .as_ref()
                    .and_then(|owner| owner.leg_bound)
                    .unwrap_or(bound);
                self.note_capture_timeout(phase, bound);
                self.enqueue_capture_abort(io)
            }
            Err(
                error @ RunFailure::Wait {
                    key: PumpDeadline::CaptureWindow,
                    ..
                },
            ) if self
                .member_capture
                .as_ref()
                .is_some_and(|owner| owner.phase == Phase::Releasing) =>
            {
                // Release is a small-control exchange, not one of the two
                // recoverable original legs. The equal outer deadline cannot
                // hide its missing acknowledgement or an earlier Leave.
                self.allow_capture_cleanup(io)?;
                Err(error)
            }
            result => result,
        }
    }

    fn note_capture_timeout(&mut self, phase: Phase, bound: AbsoluteBound) {
        self.member_capture.as_mut().expect("capture owner").timeout = Some(CaptureTimeout {
            phase: if phase == Phase::Snapshot {
                CapturePhase::SnapshotPending
            } else {
                CapturePhase::AcknowledgePending
            },
            bound,
        });
    }

    fn allow_capture_cleanup(&mut self, io: &mut impl PumpIo) -> Result<(), RunFailure> {
        // PERSIST-CLEANUP's own small-control wait can run after save expiry.
        // The original window is retained as publication permission, never
        // extended. An earlier lifecycle wait and a broken component still win.
        match self.live(io) {
            Err(RunFailure::Wait {
                key: PumpDeadline::CaptureResult | PumpDeadline::CaptureWindow,
                ..
            }) => {
                let expired = self
                    .deadlines
                    .expired(self.last_ns)
                    .filter(|(key, _)| {
                        !matches!(
                            key,
                            PumpDeadline::CaptureResult | PumpDeadline::CaptureWindow
                        )
                    })
                    .min_by_key(|(_, bound)| bound.deadline_ns());
                match expired {
                    Some((key, bound)) => Err(self.expired(io, key, bound)),
                    None => Ok(()),
                }
            }
            result => result,
        }
    }

    fn enqueue_capture_abort(&mut self, io: &mut impl PumpIo) -> Result<(), RunFailure> {
        self.allow_capture_cleanup(io)?;
        let id = self.member_capture.as_ref().expect("capture owner").id;
        let result = self
            .flow
            .abort_capture(
                &mut FlowRecording {
                    barrier: &mut self.barrier,
                    inbox: &mut self.inbox,
                    budget: &mut self.budget,
                    author: &self.author,
                },
                io,
                id,
            )
            .map_err(|error| self.flow_failed(io, error))?;
        if !matches!(result, OperationStart::Started { .. }) {
            return Err(self.capture_fault(io, "capture cleanup has an incompatible state slot"));
        }
        self.deadlines.disarm(PumpDeadline::CaptureResult);
        self.deadlines.disarm(PumpDeadline::CaptureWindow);
        self.member_capture.as_mut().expect("capture owner").answer = None;
        self.begin_capture_wait(Phase::Aborting, SMALL_CONTROL_NS, io)
    }

    fn begin_capture_wait(
        &mut self,
        phase: Phase,
        duration: u64,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        let began = self
            .flow
            .capture_queued_ns()
            .ok_or_else(|| self.capture_fault(io, "capture lost its enqueue anchor"))?;
        let mut bound = AbsoluteBound::after(began, duration)
            .map_err(|error| self.capture_fault(io, error.to_string()))?;
        if !matches!(phase, Phase::Aborting | Phase::ReleaseCleanup) {
            let window = self.member_capture.as_ref().expect("capture owner").window;
            let deadline = bound.deadline_ns().min(window.deadline_ns());
            let deadline = self
                .deadlines
                .get(PumpDeadline::Leave)
                .map_or(deadline, |leave| deadline.min(leave.deadline_ns()));
            // The common clock may cross the outer bound during enqueue.
            // Retain that request with an exhausted leg. The next progress
            // starts correlated cleanup, never mislabels expiry as bad input.
            bound = AbsoluteBound::from_absolute(began, deadline.max(began))
                .map_err(|error| self.capture_fault(io, error.to_string()))?;
        }
        let owner = self.member_capture.as_mut().expect("capture owner");
        owner.phase = phase;
        owner.leg_bound = Some(bound);
        let key = if matches!(
            phase,
            Phase::Aborting | Phase::Releasing | Phase::ReleaseCleanup
        ) {
            PumpDeadline::StateControl
        } else {
            PumpDeadline::CaptureResult
        };
        self.arm_capture_wait(key, bound, io)
    }

    fn arm_capture_wait(
        &mut self,
        key: PumpDeadline,
        bound: AbsoluteBound,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        self.deadlines
            .arm(key, bound)
            .map_err(|error| self.capture_fault(io, error.to_string()))
    }

    fn capture_fault(&mut self, io: &mut impl PumpIo, detail: impl Into<String>) -> RunFailure {
        self.fail(
            io,
            FlowHaltReason::ProtocolFailed,
            HaltPoint::Barrier,
            detail,
        )
    }
}
