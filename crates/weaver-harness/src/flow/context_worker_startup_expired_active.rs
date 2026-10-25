//! Roll back settled startup accounts and release their actual organ resources.
//! The actual Leave licenses a new Grants comparison, never another boot step.
use super::*;
use crate::flow::context::failed_enter_driver::ChildState;
use crate::flow::context::failed_enter_driver::expired_residency::ExpiredResidencyRelease;
use crate::flow::context::startup_composition::refusal::response::listener::{
    RetiredRollbackDials, RollbackDials,
};
use crate::flow::context::*;
use crate::flow::pump_transport::ReceivedOrgan;
use weaver_trace::GrantSurface;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    Enter,
    Grants,
    Seal,
    Release,
    Classifier,
    Child,
    Spu,
    State,
    Terminal,
}

pub(super) struct ExpiredActiveRollback {
    stage: Stage,
    dials: Option<RollbackDials>,
    retired_dials: Option<RetiredRollbackDials>,
    inspecting: bool,
    pub(super) current: Option<InspectionCompletion>,
    surface: Option<GrantSurface>,
    pub(super) release: Option<ExpiredResidencyRelease>,
    pub(super) returned_admit: Option<ReceivedOrgan>,
}

impl ExpiredActiveRollback {
    pub(super) fn retire_dials(&mut self) {
        if let Some(dials) = self.dials.take() {
            self.retired_dials = Some(dials.retire_failed());
        }
    }

    fn step(
        &mut self,
        owner: &mut FoundationExpiredRollback,
        io: &mut impl PumpIo,
    ) -> Result<(), String> {
        let context = &mut owner.run.context;
        let leave = owner.leave.unwrap();
        if self.stage == Stage::Enter {
            // This stage owns a completed startup, including both the Open
            // measurement and initial Grants. No old request borrows Leave.
            if context.deadlines.get(PumpDeadline::StateControl).is_some()
                || context.inspection.is_some()
                || context.read.is_some()
                || context.barrier.pending().is_some()
                || context.barrier.pending_author().is_some()
                || context.flow.pending_control().is_some()
                || context.flow.operation_completion().is_some()
                || !context.flow.state().outstanding().is_empty()
            {
                return Err("expired classifier rollback retains unfinished startup work".into());
            }
            if context.complete_wait(PumpDeadline::Enter) != Some(owner.handoff.identity.enter) {
                return Err("expired classifier rollback lost its original Enter".into());
            }
            self.stage = Stage::Grants;
            return Ok(());
        }
        if self.stage == Stage::Terminal {
            return Ok(());
        }
        let dials = self.dials.as_mut().expect("retained rollback dials");
        let result = (|| -> Result<(), RunFailure> {
            dials.prepare(context, leave, io)?;
            match self.stage {
                Stage::Grants => {
                    if !self.inspecting {
                        match context.begin_inspection(InspectionKind::Grants, None, io) {
                            Ok(()) => self.inspecting = true,
                            Err(error) => {
                                self.inspecting = error.retained;
                                return Err(error.failure.unwrap_or(RunFailure::InspectionOwned));
                            }
                        }
                    }
                    let mut ports = crate::flow::pump::PumpPorts {
                        lifecycle: dials.ports(&mut owner.dial.as_mut().unwrap().peer),
                        gate: None,
                        decode: None,
                        label: None,
                        io,
                    };
                    let progress = context.advance_inspection(&mut ports)?;
                    if let Some(reading) = context.take_inspection(ports.io)? {
                        self.surface =
                            match (owner.handoff.held.initial_grants().unwrap(), &reading) {
                                (
                                    InspectionCompletion::Grants {
                                        surface: initial, ..
                                    },
                                    InspectionCompletion::Grants {
                                        surface: current, ..
                                    },
                                ) => Some(if initial == current {
                                    GrantSurface::Unchanged
                                } else {
                                    GrantSurface::Varied
                                }),
                                (
                                    InspectionCompletion::Grants { .. }
                                    | InspectionCompletion::Refused { .. },
                                    InspectionCompletion::Refused { .. },
                                )
                                | (
                                    InspectionCompletion::Refused { .. },
                                    InspectionCompletion::Grants { .. },
                                ) => Some(GrantSurface::Unreadable),
                                _ => None,
                            };
                        self.current = Some(reading);
                        if self.surface.is_none() {
                            return Err(RunFailure::InspectionOwned);
                        }
                        self.stage = Stage::Seal;
                    } else if progress == InspectionProgress::Pending {
                        context.pump_once(&mut ports)?;
                    }
                }
                Stage::Seal => {
                    if dials.seal_step(context, leave, io)? {
                        self.stage = Stage::Release;
                    }
                }
                Stage::Release => {
                    context.begin_releasing(leave, io)?;
                    self.stage = Stage::Classifier;
                }
                Stage::Classifier => {
                    if context.retire_classifier(io)? == Advance::Progress {
                        self.stage = Stage::Child;
                    }
                }
                Stage::Child => match owner.run.resources.poll_classifier_child() {
                    Some(ChildState::Reaped(_)) => self.stage = Stage::Spu,
                    Some(ChildState::Running) => {
                        let wake = context
                            .last_ns
                            .saturating_add(10_000_000)
                            .min(leave.deadline_ns());
                        if let Err(error) = io.wait(&[], Some(wake)) {
                            return Err(context.fail(
                                io,
                                FlowHaltReason::TraceFailed,
                                HaltPoint::Author,
                                format!("expired classifier child wait: {error}"),
                            ));
                        }
                    }
                    _ => return Err(RunFailure::InspectionOwned),
                },
                Stage::Spu => {
                    if owner.run.resources.spu_child().is_none() {
                        self.stage = Stage::State;
                    } else {
                        if self.release.is_none() {
                            let permit = ExpiredReleasePermit {
                                origin: Arc::clone(&context.read_origin),
                                enter: owner.handoff.identity.enter,
                                leave,
                            };
                            let reply = owner.handoff.held.take_admit_reply();
                            match owner
                                .run
                                .resources
                                .begin_expired_release(context, permit, reply)
                            {
                                Ok(release) => self.release = Some(release),
                                Err((reply, detail)) => {
                                    self.returned_admit = reply;
                                    return Err(context.fail(
                                        io,
                                        FlowHaltReason::ProtocolFailed,
                                        HaltPoint::Barrier,
                                        detail,
                                    ));
                                }
                            }
                        }
                        let release = self.release.as_mut().unwrap();
                        if release.advance(&mut owner.run.resources, context, io)? {
                            self.stage = Stage::State;
                        } else {
                            release.wait(&owner.run.resources, context, io)?;
                        }
                    }
                }
                Stage::State => {
                    if context.retire_state(io)? == Advance::Progress {
                        self.stage = Stage::Terminal;
                    }
                }
                Stage::Enter | Stage::Terminal => unreachable!(),
            }
            Ok(())
        })();
        if let Err(error) = result {
            let result = if matches!(
                error,
                RunFailure::Transport {
                    source: crate::flow::pump::PumpSource::LeavePeer,
                    ..
                }
            ) {
                // The actual accepted directive stays authoritative after its
                // answer peer departs. Cleanup must not become a new Enter.
                context.live(io)
            } else {
                dials.recover(context, error, io)
            };
            result.map_err(|error| format!("expired classifier rollback: {error:?}"))?;
        }
        if self.stage == Stage::Terminal {
            owner.preflight = Some(preflight::ExpiredPreflightRollback::released(
                owner,
                self.dials.take().unwrap(),
                self.surface.unwrap(),
                self.release
                    .as_ref()
                    .and_then(ExpiredResidencyRelease::outcome),
            ));
        }
        Ok(())
    }
}

impl FoundationExpiredRollback {
    fn active_rollback(&mut self) -> Option<ExpiredActiveRollback> {
        let context = &self.run.context;
        if self.before_load
            || context.phase != RecordingPhase::Entered
            || context.flow.phase() != CoordinatorPhase::Active
            || !context.barrier.is_active()
            || context.failure.is_some()
            || context.flow.halt().is_some()
            || context.deadlines.get(PumpDeadline::Enter) != Some(self.handoff.identity.enter)
            || context.deadlines.get(PumpDeadline::Leave) != self.leave
            || self.leave?.expired(context.last_ns)
            || self.run.resources.classifier_child().is_none()
            || self.handoff.gate.is_some()
            || matches!(self.handoff.held.as_ref(), HeldStartup::Initial(_))
            || self.handoff.held.initial_grants().is_none()
            || context.deadlines.get(PumpDeadline::StateControl).is_some()
            || context.boot_open_blocks_external()
            || context.place_owed()
            || context.plan_summary.is_some()
            || context.repair.is_some()
            || context.boot_pairs.is_some()
            || context.read.is_some()
            || context.inspection.is_some()
            || context.barrier.pending().is_some()
            || context.barrier.pending_author().is_some()
            || context.flow.pending_control().is_some()
            || context.flow.operation_completion().is_some()
            || !context.flow.state().outstanding().is_empty()
        {
            return None;
        }
        Some(ExpiredActiveRollback {
            stage: Stage::Enter,
            dials: Some(RollbackDials::new(self.handoff.listener.take()?)),
            retired_dials: None,
            inspecting: false,
            current: None,
            surface: None,
            release: None,
            returned_admit: None,
        })
    }

    pub(super) fn advance_active(&mut self, io: &mut impl PumpIo) -> Result<(), String> {
        if self.active.is_none() {
            self.active = self.active_rollback();
        }
        let Some(mut active) = self.active.take() else {
            return Ok(());
        };
        let result = active.step(self, io);
        self.active = Some(active);
        result
    }
}
