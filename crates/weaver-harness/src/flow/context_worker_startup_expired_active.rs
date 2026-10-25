//! Roll back settled startup accounts and release their actual organ resources.
//! The actual Leave licenses a new Grants comparison, never another boot step.
use super::*;
use crate::flow::context::boot_open_driver::StoppedBootOpenAccount;
use crate::flow::context::expired_read_driver::ExpiredReadAccount;
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
    Admission,
    BootPairs,
    Templates,
    Read,
    Open,
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
    pub(super) open: Option<StoppedBootOpenAccount>,
    pub(super) read: Option<ExpiredReadAccount>,
    pub(super) templates: Option<crate::flow::context::template_driver::StoppedTemplateAccount>,
    pub(super) admission:
        Option<crate::flow::context::expired_admission_driver::StoppedAdmissionAccount>,
    classifier_started: bool,
    pub(super) boot_pairs: Option<crate::flow::context::boot_pair_driver::StoppedBootPairsAccount>,
    member: weaver_trace::MemberRelease,
}

impl ExpiredActiveRollback {
    pub(super) fn retire_accounts(&mut self) {
        if let Some(account) = &mut self.admission {
            account.retire_transport();
        }
        if let Some(account) = &mut self.templates {
            account.retire_transport();
        }
    }
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
        if self.stage == Stage::BootPairs {
            let permit = ExpiredStartupAccountPermit {
                origin: Arc::clone(&context.read_origin),
                enter: owner.handoff.identity.enter,
                leave,
            };
            self.boot_pairs = Some(
                context
                    .detach_expired_boot_pairs(&permit, io)
                    .map_err(|error| format!("expired boot repair: {error:?}"))?,
            );
            self.stage = Stage::Enter;
            return Ok(());
        }
        if self.stage == Stage::Admission {
            let permit = ExpiredStartupAccountPermit {
                origin: Arc::clone(&context.read_origin),
                enter: owner.handoff.identity.enter,
                leave,
            };
            self.admission = Some(
                context
                    .detach_expired_admission(&permit, io)
                    .map_err(|error| format!("expired classifier admission: {error:?}"))?,
            );
            self.stage = Stage::Enter;
            return Ok(());
        }
        if self.stage == Stage::Templates {
            let permit = ExpiredStartupAccountPermit {
                origin: Arc::clone(&context.read_origin),
                enter: owner.handoff.identity.enter,
                leave,
            };
            self.templates = Some(
                context
                    .detach_expired_templates(&permit, io)
                    .map_err(|error| format!("expired template account: {error:?}"))?,
            );
            self.stage = Stage::Enter;
            return Ok(());
        }
        if self.stage == Stage::Read {
            let permit = ExpiredStartupAccountPermit {
                origin: Arc::clone(&context.read_origin),
                enter: owner.handoff.identity.enter,
                leave,
            };
            match context.park_expired_startup_read(&permit, io) {
                Ok(read) => {
                    self.read = Some(read);
                    // The expired read still owns the serial member exchange.
                    // No current Grants read can reuse or renew that channel.
                    self.surface = Some(GrantSurface::Unreadable);
                    self.stage = Stage::Seal;
                }
                Err(RunFailure::Busy(_) | RunFailure::ReadOwned | RunFailure::PlanSummaryOwned) => {
                }
                Err(error) => return Err(format!("expired startup read: {error:?}")),
            }
            return Ok(());
        }
        if self.stage == Stage::Open {
            let permit = ExpiredStartupAccountPermit {
                origin: Arc::clone(&context.read_origin),
                enter: owner.handoff.identity.enter,
                leave,
            };
            self.open = Some(
                context
                    .detach_expired_boot_open(
                        &permit,
                        owner
                            .run
                            .resources
                            .decode_mut()
                            .expect("original boot decode"),
                    )
                    .map_err(|error| format!("expired Open accounting: {error:?}"))?,
            );
            self.stage = Stage::Enter;
            return Ok(());
        }
        if self.stage == Stage::Enter {
            // The Open account has completed or is retained incomplete; no old
            // request borrows Leave for the new cleanup Grants comparison.
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
                        self.stage = if self.read.is_some() {
                            Stage::Classifier
                        } else {
                            Stage::Release
                        };
                    }
                }
                Stage::Release => {
                    context.begin_releasing(leave, io)?;
                    self.stage = Stage::Classifier;
                }
                Stage::Classifier => {
                    if self.templates.is_some() || self.admission.is_some() {
                        context.live(io)?;
                        let permit = ExpiredReleasePermit {
                            origin: Arc::clone(&context.read_origin),
                            enter: owner.handoff.identity.enter,
                            leave,
                        };
                        if !permit.allows(context) {
                            return Err(RunFailure::TemplateBlocked);
                        }
                        if let Some(account) = &mut self.templates {
                            account.retire_transport();
                        }
                        if let Some(account) = &mut self.admission {
                            account.retire_transport();
                        }
                    }
                    if context.retire_classifier(io)? == Advance::Progress {
                        self.stage = Stage::Child;
                    }
                }
                Stage::Child => match owner.run.resources.poll_classifier_child() {
                    None if !self.classifier_started => self.stage = Stage::Spu,
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
                    if self.read.is_some() {
                        if !matches!(
                            owner.run.resources.classifier_child(),
                            Some(ChildState::Reaped(_))
                        ) {
                            return Err(RunFailure::ReadOwned);
                        }
                        self.member = context.retire_expired_startup_read(io)?;
                        self.stage = Stage::Terminal;
                    } else if context.retire_state(io)? == Advance::Progress {
                        self.stage = Stage::Terminal;
                    }
                }
                Stage::Admission
                | Stage::BootPairs
                | Stage::Templates
                | Stage::Read
                | Stage::Open
                | Stage::Enter
                | Stage::Terminal => {
                    unreachable!()
                }
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
                self.open.as_ref().is_none_or(|open| !open.incomplete())
                    && self
                        .admission
                        .as_ref()
                        .is_none_or(|account| account.admission.account_complete()),
                self.member,
            ));
        }
        Ok(())
    }
}

impl FoundationExpiredRollback {
    fn active_rollback(&mut self) -> Option<ExpiredActiveRollback> {
        let context = &self.run.context;
        let permit = ExpiredStartupAccountPermit {
            origin: Arc::clone(&context.read_origin),
            enter: self.handoff.identity.enter,
            leave: self.leave?,
        };
        let detach_open = context.boot_open_blocks_external()
            && self
                .run
                .resources
                .decode_present_for_serving()
                .is_some_and(|decode| context.can_detach_expired_boot_open(&permit, decode));
        let park_read = context.can_park_expired_startup_read(&permit);
        let detach_templates = context.can_detach_expired_templates(&permit);
        let detach_admission = context.can_detach_expired_admission(&permit);
        let detach_boot_pairs = context.can_detach_expired_boot_pairs(&permit);
        let initial = matches!(self.handoff.held.as_ref(), HeldStartup::Initial(account)
            if account.load_sequence.is_some() && account.opened.is_some() && account.initial_grants.is_some());
        if self.before_load
            || context.phase != RecordingPhase::Entered
            || context.flow.phase() != CoordinatorPhase::Active
            || !context.barrier.is_active()
            || context.failure.is_some()
            || context.flow.halt().is_some()
            || context.deadlines.get(PumpDeadline::Enter) != Some(self.handoff.identity.enter)
            || context.deadlines.get(PumpDeadline::Leave) != self.leave
            || self.leave?.expired(context.last_ns)
            || (self.run.resources.classifier_child().is_none() && !initial)
            || self.handoff.gate.is_some()
            || (matches!(self.handoff.held.as_ref(), HeldStartup::Initial(_)) && !initial)
            || (context.flow.starting_classifier().is_some() && !detach_admission)
            || self.handoff.held.initial_grants().is_none()
            || (context.templates_block_content() && !detach_templates)
            || (context.flow.pending_check().is_some() && !detach_templates)
            || (context.deadlines.get(PumpDeadline::StateControl).is_some() && !park_read)
            || (context.boot_open_blocks_external() && !detach_open)
            || context.place_owed()
            || (context.plan_summary.is_some() && !park_read)
            || context.repair.is_some()
            || (context.boot_pairs.is_some() && !detach_boot_pairs)
            || (context.read.is_some() && !park_read)
            || context.inspection.is_some()
            || ((context.barrier.pending().is_some() || context.barrier.pending_author().is_some())
                && !detach_open
                && !park_read
                && !detach_templates)
            || (context.flow.pending_control().is_some() && !park_read)
            || (context.flow.operation_completion().is_some() && !park_read && !detach_templates)
            || (!context.flow.state().outstanding().is_empty() && !park_read)
        {
            return None;
        }
        Some(ExpiredActiveRollback {
            stage: if detach_admission {
                Stage::Admission
            } else if detach_templates {
                Stage::Templates
            } else if park_read {
                Stage::Read
            } else if detach_boot_pairs {
                Stage::BootPairs
            } else if detach_open {
                Stage::Open
            } else {
                Stage::Enter
            },
            dials: Some(RollbackDials::new(self.handoff.listener.take()?)),
            retired_dials: None,
            inspecting: false,
            current: None,
            surface: None,
            release: None,
            returned_admit: None,
            open: None,
            read: None,
            templates: None,
            admission: None,
            classifier_started: self.run.resources.classifier_child().is_some(),
            boot_pairs: None,
            member: weaver_trace::MemberRelease::Closed,
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
