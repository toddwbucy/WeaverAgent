//! Consume stopped rollback custody into its exact run's terminal cleanup.
use super::*;
use crate::flow::context::failed_enter_driver::FailedEnterResources;
use crate::flow::context::{RecordingContext, RecordingPhase, RunFailure};
use weaver_trace::Sequence;

/// Borrowed facts from an actual rollback stage, never free-standing caller input.
pub(in crate::flow::context) struct RollbackView<'a> {
    pub context: &'a RecordingContext,
    pub resources: &'a FailedEnterResources,
    pub leave: Option<AbsoluteBound>,
    pub recorded: Option<Sequence>,
    pub cause: Option<weaver_types::Cause>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExitRoute {
    Recorded,
    Halt,
    Expired,
}

fn route(stopped: bool, recorded: bool, halted: bool, leave_expired: bool) -> Option<ExitRoute> {
    match (stopped, recorded, halted, leave_expired) {
        (false, _, _, _) => None,
        (true, true, _, _) => Some(ExitRoute::Recorded),
        (true, false, true, _) => Some(ExitRoute::Halt),
        (true, false, false, true) => Some(ExitRoute::Expired),
        (true, false, false, false) => None,
    }
}

#[cfg(test)]
mod decision_tests {
    use super::*;

    #[test]
    fn rollback_terminal_receipt_precedes_later_fault_and_expiry() {
        for (halt, expired) in [(false, false), (false, true), (true, false), (true, true)] {
            assert_eq!(route(true, true, halt, expired), Some(ExitRoute::Recorded));
        }
    }
    #[test]
    fn rollback_unrecorded_first_fault_keeps_its_failure_route_at_the_cap() {
        assert_eq!(route(true, false, true, false), Some(ExitRoute::Halt));
        assert_eq!(route(true, false, true, true), Some(ExitRoute::Halt));
    }
    #[test]
    fn rollback_expiry_without_a_component_fault_does_not_invent_one() {
        assert_eq!(route(true, false, false, true), Some(ExitRoute::Expired));
        assert_eq!(route(true, false, false, false), None);
    }
    #[test]
    fn progressing_or_successful_rollback_cannot_be_consumed_as_failure() {
        for (recorded, halt, expired) in [
            (false, false, false),
            (false, false, true),
            (false, true, false),
            (false, true, true),
            (true, false, false),
            (true, false, true),
            (true, true, false),
            (true, true, true),
        ] {
            assert_eq!(route(false, recorded, halt, expired), None);
        }
    }
}

use crate::flow::clock::{PumpIo, WaitInterest};
use crate::flow::context::failed_enter_driver::{FailedEnterProgress, FailedRunCleanup};
use crate::flow::context::startup_composition::refusal::RetiredRefusal;
use crate::flow::context::startup_composition::refusal::response::{
    RetiredPreparedRollback, RetiredRefusalResponse, RetiredRollback,
    terminal::RetiredRollbackTerminal,
};
use crate::flow::coordinator::FailedFlowResources;
use crate::flow::pump::{
    PumpDeadline, RetiredLifecycleListener, RetiredLifecycleMessage, RetiredLifecyclePeer,
};
use std::sync::Arc;

impl RollbackState<'_> {
    pub(super) fn failure_view(&self) -> Option<RollbackView<'_>> {
        Some(match self {
            Self::Record(owner) => owner.failure_view(),
            Self::Recorded(owner) => owner.failure_view(),
            Self::Response(owner) | Self::Await(owner) => owner.failure_view(),
            Self::Rollback(owner) => owner.failure_view(),
            Self::Prepared(owner) => owner.failure_view(),
            Self::Terminal(owner) => owner.failure_view(),
            Self::Closed(_) => return None,
        })
    }
    fn retire_failure(self) -> HeldRollback {
        match self {
            Self::Record(owner) => HeldRollback::Record(Box::new(owner.retire_failure())),
            Self::Recorded(owner) => HeldRollback::Recorded(Box::new(owner.retire_failure())),
            Self::Response(owner) => HeldRollback::Response(Box::new(owner.retire_failure())),
            Self::Await(owner) => HeldRollback::Await(Box::new(owner.retire_failure())),
            Self::Rollback(owner) => HeldRollback::Rollback(Box::new(owner.retire_failure())),
            Self::Prepared(owner) => HeldRollback::Prepared(Box::new(owner.retire_failure())),
            Self::Terminal(owner) => HeldRollback::Terminal(Box::new(owner.retire_failure())),
            Self::Closed(_) => unreachable!("a completed rollback has no failure handoff"),
        }
    }
}

enum HeldRollback {
    Record(Box<RetiredRefusal>),
    Recorded(Box<RetiredRefusal>),
    Response(Box<RetiredRefusalResponse>),
    Await(Box<RetiredRefusalResponse>),
    Rollback(Box<RetiredRollback>),
    Prepared(Box<RetiredPreparedRollback>),
    Terminal(Box<RetiredRollbackTerminal>),
}
struct RetiredAwaitDial {
    peer: RetiredLifecyclePeer,
    bound: AbsoluteBound,
    message: Option<RetiredLifecycleMessage>,
    send: Option<OrganSend>,
}
struct RollbackIdentity {
    origin: Arc<()>,
    phase: RecordingPhase,
    failure: Option<RunFailure>,
    registered_leave: Option<AbsoluteBound>,
    accepted_leave: Option<AbsoluteBound>,
    deadline: Option<u64>,
    route: ExitRoute,
    terminal: Option<Sequence>,
    cause: Option<weaver_types::Cause>,
}
impl RollbackIdentity {
    fn capture(view: RollbackView<'_>, stopped: bool) -> Option<Self> {
        let context = view.context;
        if !view.resources.matches(context)
            || !view.resources.original_ports()
            || view.resources.spu_child().is_some()
            || context.ever_started_turn
            || context.open_turn.is_some()
            || context.input.snapshot().is_some()
            || context.member_capture.is_some()
            || !context.flow.preinput_custody()
            || view.leave.is_some() && view.cause.is_none()
        {
            return None;
        }
        let registered_leave = context.deadlines.get(PumpDeadline::Leave);
        if registered_leave.is_some() && registered_leave != view.leave {
            return None;
        }
        let halted = context.phase == RecordingPhase::Failed
            && context.failure.is_some()
            && context.flow.preinput_failed_owner();
        let expired = view
            .leave
            .is_some_and(|leave| leave.expired(context.last_ns));
        let route = route(stopped, view.recorded.is_some(), halted, expired)?;
        if route != ExitRoute::Halt && view.leave.is_none() {
            return None;
        }
        if view.recorded.is_some() && !context.barrier.is_closed() {
            return None;
        }
        let deadline = match context.flow.halt() {
            Some(halt) => halt.deadline().ok().map(|deadline| {
                view.leave
                    .map_or(deadline, |leave| deadline.min(leave.deadline_ns()))
            }),
            None => view.leave.map(AbsoluteBound::deadline_ns),
        };
        Some(Self {
            origin: Arc::clone(&context.read_origin),
            phase: context.phase,
            failure: context.failure.clone(),
            registered_leave,
            accepted_leave: view.leave,
            deadline,
            route,
            terminal: view.recorded,
            cause: view.cause,
        })
    }
    fn matches(&self, run: &FoundationRun) -> bool {
        Arc::ptr_eq(&self.origin, &run.context.read_origin)
            && run.resources.matches(&run.context)
            && run.resources.original_ports()
            && run.resources.spu_child().is_none()
            && run.context.phase == self.phase
            && run.context.failure == self.failure
            && run.context.deadlines.get(PumpDeadline::Leave) == self.registered_leave
            && run.context.flow.preinput_custody()
            && run.original.is_none()
            && run.listener.is_none()
            && match run.context.flow.halt() {
                Some(halt) => {
                    halt.deadline().ok().map(|deadline| {
                        self.accepted_leave
                            .map_or(deadline, |leave| deadline.min(leave.deadline_ns()))
                    }) == self.deadline
                }
                None => self.accepted_leave.map(AbsoluteBound::deadline_ns) == self.deadline,
            }
    }
}

/// Consumed rollback custody. No caller can replace the original run, actual
/// Leave, first fault, Recorded terminal receipt or cleanup deadline.
///
/// ```compile_fail,E0277
/// fn clone_required<T: Clone>() {}
/// clone_required::<weaver_harness::FoundationRollbackHandoff>();
/// ```
pub struct FoundationRollbackHandoff {
    identity: RollbackIdentity,
    held: HeldRollback,
    listener: Option<RetiredLifecycleListener>,
    dial: Option<RetiredAwaitDial>,
    enter: AbsoluteBound,
    leave: Option<AbsoluteBound>,
    refusal_sequence: Option<u64>,
    unload_sequence: Option<u64>,
    detail: Option<String>,
}
impl FoundationRollback<'_> {
    /// Return progressing, successful and healthy unaccepted rollback owners
    /// unchanged. Only a checked stopped route releases its actual borrows.
    pub fn failed_handoff(
        mut self: Box<Self>,
    ) -> Result<Box<FoundationRollbackHandoff>, Box<Self>> {
        let Some(view) = self.state.as_ref().and_then(RollbackState::failure_view) else {
            return Err(self);
        };
        let Some(identity) =
            RollbackIdentity::capture(view, self.progress() == FoundationRollbackProgress::Stopped)
        else {
            return Err(self);
        };
        let held = self.state.take().unwrap().retire_failure();
        Ok(Box::new(FoundationRollbackHandoff {
            identity,
            held,
            listener: self
                .listener
                .take()
                .map(|listener| listener.retire_failed()),
            dial: self.dial.take().map(|dial| RetiredAwaitDial {
                peer: dial.peer.retire_failed(),
                bound: dial.bound,
                message: dial.message.map(LifecycleMessage::retire_failed),
                send: dial.send,
            }),
            enter: self.enter,
            leave: self.leave,
            refusal_sequence: self.refusal_sequence,
            unload_sequence: self.unload_sequence,
            detail: self.detail,
        }))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoundationRollbackExitProgress {
    Progress,
    Pending,
    Closed,
}

/// Failed terminal custody; this owner exposes neither a live context nor Left.
pub struct FoundationRollbackExit {
    handoff: Box<FoundationRollbackHandoff>,
    driver: RollbackDriver,
    progress: FoundationRollbackExitProgress,
}
enum RollbackDriver {
    Halt(Box<FailedRunCleanup>),
    NoRecord(Box<NoRecordExit>),
}
pub(in crate::flow::context) struct NoRecordExit {
    context: RecordingContext,
    resources: FailedEnterResources,
    retired: FailedFlowResources,
    deadline: Option<u64>,
    expired: bool,
    drained: bool,
    abandoned: bool,
    detail: Option<String>,
}
impl FoundationRun {
    #[allow(clippy::result_large_err)]
    pub fn failed_rollback_exit(
        self: Box<Self>,
        handoff: Box<FoundationRollbackHandoff>,
    ) -> Result<Box<FoundationRollbackExit>, (Box<Self>, Box<FoundationRollbackHandoff>)> {
        if !handoff.identity.matches(&self) {
            return Err((self, handoff));
        }
        let FoundationRun {
            context,
            resources,
            listener: _,
            original: _,
        } = *self;
        let identity = &handoff.identity;
        let driver = if identity.route == ExitRoute::Halt {
            RollbackDriver::Halt(Box::new(FailedRunCleanup::after_rollback(
                context,
                resources,
                identity.accepted_leave,
                identity.accepted_leave.and(identity.cause),
            )))
        } else {
            // These routes cannot append anything: Unload is already Recorded,
            // or the actual healthy Leave has expired. Retain all framing bytes.
            RollbackDriver::NoRecord(Box::new(NoRecordExit::new(
                context,
                resources,
                identity.deadline,
                identity.route == ExitRoute::Expired,
            )))
        };
        Ok(Box::new(FoundationRollbackExit {
            handoff,
            driver,
            progress: FoundationRollbackExitProgress::Progress,
        }))
    }
}

#[cfg(test)]
#[path = "context_worker_rollback_failure_tests.rs"]
pub(in crate::flow::context::worker_startup) mod tests;
impl NoRecordExit {
    /// The caller has retained an actual terminal or expired-Leave proof.
    /// No ordinary author or terminal retry is exposed by this driver.
    pub(in crate::flow::context) fn new(
        mut context: RecordingContext,
        resources: FailedEnterResources,
        deadline: Option<u64>,
        expired: bool,
    ) -> Self {
        let retired = context
            .flow
            .retire_abandoned_rollback_ports()
            .expect("checked pre-input custody");
        Self {
            context,
            resources,
            retired,
            deadline,
            expired,
            drained: false,
            abandoned: false,
            detail: None,
        }
    }
    pub(in crate::flow::context) fn abandoned(&self) -> bool {
        self.abandoned
    }
    #[cfg(test)]
    pub(in crate::flow::context) fn reap_composition_test_children(&mut self) {
        self.resources.reap_startup_test_children();
        self.context.barrier.abandon();
    }
    fn abort(&mut self) {
        self.resources.close_at_cap();
        self.context.barrier.abandon();
        self.context.phase = RecordingPhase::Closed;
        self.abandoned = true;
    }
    fn clock(&mut self, io: &mut impl PumpIo) -> bool {
        match io.now() {
            Ok(now)
                if now >= self.context.last_ns && self.deadline.is_some_and(|end| now < end) =>
            {
                self.context.last_ns = now;
                true
            }
            reading => {
                self.detail.get_or_insert_with(|| {
                    format!("stopped rollback clock or original cap: {reading:?}")
                });
                self.abort();
                false
            }
        }
    }
    pub(in crate::flow::context) fn advance(
        &mut self,
        io: &mut impl PumpIo,
    ) -> FoundationRollbackExitProgress {
        use FoundationRollbackExitProgress as Progress;
        if self.expired {
            self.abort();
            return Progress::Closed;
        }
        if !self.clock(io) {
            return Progress::Closed;
        }
        let children = self.resources.poll_children();
        match self.context.barrier.drain_status(&mut self.context.budget) {
            Ok(weaver_trace::serving::DrainStatus::Emptied) => self.drained = true,
            Ok(weaver_trace::serving::DrainStatus::Pending { .. }) => {}
            Err(error) => {
                self.detail = Some(format!("recorded rollback trace drain: {error:?}"));
                self.abort();
                return Progress::Closed;
            }
        }
        if !self.clock(io) {
            return Progress::Closed;
        }
        if self.drained && children {
            self.context.barrier.abandon();
            self.context.phase = RecordingPhase::Closed;
            Progress::Closed
        } else {
            Progress::Pending
        }
    }
    pub(in crate::flow::context) fn wait(
        &mut self,
        io: &mut impl PumpIo,
    ) -> FoundationRollbackExitProgress {
        use FoundationRollbackExitProgress as Progress;
        if !self.clock(io) {
            return Progress::Closed;
        }
        let wake = self
            .context
            .last_ns
            .saturating_add(10_000_000)
            .min(self.deadline.unwrap());
        if let Err(error) = self.context.barrier.drain_capacity_wake() {
            self.detail = Some(format!("recorded rollback capacity wake: {error:?}"));
            self.abort();
            return Progress::Closed;
        }
        let interests = [WaitInterest {
            fd: self.context.barrier.capacity_fd(),
            events: nix::poll::PollFlags::POLLIN,
        }];
        if let Err(error) = io.wait(&interests, Some(wake)) {
            self.detail = Some(format!("recorded rollback wait: {error}"));
            self.abort();
            return Progress::Closed;
        }
        if self.clock(io) {
            Progress::Pending
        } else {
            Progress::Closed
        }
    }
}
impl FoundationRollbackExit {
    pub fn progress(&self) -> FoundationRollbackExitProgress {
        self.progress
    }
    pub fn process_exit(&self) -> Option<crate::FoundationProcessExit> {
        (self.progress == FoundationRollbackExitProgress::Closed)
            .then_some(crate::FoundationProcessExit::Failure)
    }
    pub fn channel_fault(&self) -> Option<crate::ChannelFault> {
        (self.progress == FoundationRollbackExitProgress::Closed)
            .then_some(crate::ChannelFault::Closed)
    }
    pub fn unload_sequence(&self) -> Option<u64> {
        match &self.driver {
            RollbackDriver::Halt(tail) => tail.unload_sequence().map(|sequence| sequence.0),
            RollbackDriver::NoRecord(_) => {
                self.handoff.identity.terminal.map(|sequence| sequence.0)
            }
        }
    }
    pub fn abandoned(&self) -> bool {
        match &self.driver {
            RollbackDriver::Halt(tail) => tail.abandoned(),
            RollbackDriver::NoRecord(owner) => owner.abandoned,
        }
    }
    pub fn advance(&mut self) -> FoundationRollbackExitProgress {
        self.advance_with_io(&mut MonotonicIo)
    }
    pub fn wait_for_progress(&mut self) -> FoundationRollbackExitProgress {
        self.wait_with_io(&mut MonotonicIo)
    }
    fn advance_with_io(&mut self, io: &mut impl PumpIo) -> FoundationRollbackExitProgress {
        if self.progress == FoundationRollbackExitProgress::Closed {
            return self.progress;
        }
        self.progress = match &mut self.driver {
            RollbackDriver::Halt(tail) => match tail.advance(io) {
                FailedEnterProgress::Progress => FoundationRollbackExitProgress::Progress,
                FailedEnterProgress::Pending => FoundationRollbackExitProgress::Pending,
                FailedEnterProgress::Closed => FoundationRollbackExitProgress::Closed,
            },
            RollbackDriver::NoRecord(owner) => owner.advance(io),
        };
        self.progress
    }
    fn wait_with_io(&mut self, io: &mut impl PumpIo) -> FoundationRollbackExitProgress {
        if self.progress != FoundationRollbackExitProgress::Pending {
            return self.progress;
        }
        self.progress = match &mut self.driver {
            RollbackDriver::Halt(tail) => match tail.wait_for_progress(io) {
                FailedEnterProgress::Progress => FoundationRollbackExitProgress::Progress,
                FailedEnterProgress::Pending => FoundationRollbackExitProgress::Pending,
                FailedEnterProgress::Closed => FoundationRollbackExitProgress::Closed,
            },
            RollbackDriver::NoRecord(owner) => owner.wait(io),
        };
        self.progress
    }
}
