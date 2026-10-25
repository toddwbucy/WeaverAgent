//! Roll back completed classifier admission before any SPU child was created.
//! The actual Leave licenses a new Grants comparison, never another boot step.
use super::*;
use crate::flow::context::startup_composition::refusal::response::listener::{
    RetiredRollbackDials, RollbackDials,
};
use crate::flow::context::*;
use weaver_trace::GrantSurface;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    Enter,
    Grants,
    Release,
    Classifier,
    State,
    Terminal,
}

pub(super) struct ExpiredClassifierRollback {
    stage: Stage,
    dials: Option<RollbackDials>,
    retired_dials: Option<RetiredRollbackDials>,
    inspecting: bool,
    pub(super) current: Option<InspectionCompletion>,
    surface: Option<GrantSurface>,
}

impl ExpiredClassifierRollback {
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
                        let HeldStartup::Residency(held) = owner.handoff.held.as_ref() else {
                            unreachable!("constructor retained the original classifier startup")
                        };
                        self.surface = match (&held.startup.initial_grants, &reading) {
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
                        self.stage = Stage::Release;
                    } else if progress == InspectionProgress::Pending {
                        context.pump_once(&mut ports)?;
                    }
                }
                Stage::Release => {
                    context.begin_releasing(leave, io)?;
                    self.stage = Stage::Classifier;
                }
                Stage::Classifier => {
                    if context.retire_classifier(io)? == Advance::Progress {
                        // Terminal authoring precedes any child wait. The
                        // shared terminal still requires its actual reap for Left.
                        owner.run.resources.poll_classifier_child();
                        self.stage = Stage::State;
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
            ));
        }
        Ok(())
    }
}

impl FoundationExpiredRollback {
    fn classifier_rollback(&mut self) -> Option<ExpiredClassifierRollback> {
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
            || self.run.resources.spu_child().is_some()
            || self.run.resources.classifier_child().is_none()
            || !matches!(self.handoff.held.as_ref(), HeldStartup::Residency(_))
        {
            return None;
        }
        Some(ExpiredClassifierRollback {
            stage: Stage::Enter,
            dials: Some(RollbackDials::new(self.handoff.listener.take()?)),
            retired_dials: None,
            inspecting: false,
            current: None,
            surface: None,
        })
    }

    pub(super) fn advance_classifier(&mut self, io: &mut impl PumpIo) -> Result<(), String> {
        if self.classifier.is_none() {
            self.classifier = self.classifier_rollback();
        }
        let Some(mut classifier) = self.classifier.take() else {
            return Ok(());
        };
        let result = classifier.step(self, io);
        self.classifier = Some(classifier);
        result
    }
}
