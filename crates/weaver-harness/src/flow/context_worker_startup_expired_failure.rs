//! Consume a stopped expired rollback without retrying its Release or Unload.
use super::*;
use crate::flow::context::failed_enter_driver::{FailedEnterProgress, FailedRunCleanup};
use crate::flow::context::worker_startup::rollback::failed::{
    FoundationRollbackExitProgress, NoRecordExit,
};

/// Terminal-only custody; it never returns a context, successful Enter or Left.
pub struct FoundationExpiredRollbackExit {
    held: RetainedExpiredRollback,
    driver: Driver,
    progress: FoundationRollbackExitProgress,
}

enum Driver {
    Halt(Box<FailedRunCleanup>),
    Recorded(Box<NoRecordExit>),
}

struct RetainedExpiredRollback {
    handoff: Box<FoundationExpiredEnter>,
    leave: Option<AbsoluteBound>,
    original: Option<RetiredEnter>,
    retired: Option<crate::flow::coordinator::FailedFlowResources>,
    detail: Option<String>,
    stopped_record: Option<crate::flow::recording::StoppedStartupRecord>,
    listener: Option<RetiredLifecycleListener>,
    prior_listener: Option<RetiredLifecycleListener>,
    dial: Option<RetiredDial>,
    prior_dial: Option<RetiredDial>,
    gate: Option<PacketPort<OrganEnvelope, ()>>,
    preflight: Option<preflight::ExpiredPreflightRollback>,
    active: Option<active::ExpiredActiveRollback>,
    recorded: Option<u64>,
}

impl FoundationExpiredRollback {
    #[allow(clippy::result_large_err)]
    pub fn failed_exit(
        mut self: Box<Self>,
    ) -> Result<Box<FoundationExpiredRollbackExit>, Box<Self>> {
        let context = &self.run.context;
        let recorded = self.unload_sequence();
        let halted = context.phase == RecordingPhase::Failed
            && context.failure.is_some()
            && context.flow.preinput_failed_owner();
        if self.progress() != FoundationExpiredRollbackProgress::Stopped
            || self.before_load
            || self.handoff.gate.is_some()
            || !self.run.resources.matches(context)
            || !Arc::ptr_eq(&self.handoff.identity.origin, &context.read_origin)
            || !context.flow.preinput_custody()
            || context.ever_started_turn
            || context.open_turn.is_some()
            || context.member_capture.is_some()
            || context.input.snapshot().is_some()
            || context.deadlines.get(PumpDeadline::Leave) != self.leave
            || (self.leave.is_some() && self.original.is_none())
            || (recorded.is_some() && (!context.barrier.is_closed() || self.leave.is_none()))
            || (recorded.is_none() && !halted)
        {
            return Err(self);
        }
        let deadline = match context.flow.halt() {
            Some(halt) => halt
                .deadline()
                .ok()
                .map(|end| self.leave.map_or(end, |leave| end.min(leave.deadline_ns()))),
            None => self.leave.map(AbsoluteBound::deadline_ns),
        };
        // Receipt precedence: after Recorded Unload even a later real fault
        // cannot authorize a second terminal or append a fault afterward.
        let release = if recorded.is_none() {
            self.active
                .as_mut()
                .and_then(|active| active.release.take())
        } else {
            None
        };
        let admit_reply = if recorded.is_none() && release.is_none() {
            self.active
                .as_mut()
                .and_then(|active| active.returned_admit.take())
                .or_else(|| self.handoff.held.take_admit_reply())
        } else {
            None
        };
        if let Some(active) = &mut self.active {
            active.retire_accounts();
            active.retire_dials();
        }
        if let Some(preflight) = &mut self.preflight {
            preflight.retire_dials();
        }
        let listener = self
            .handoff
            .listener
            .take()
            .map(LifecycleListener::retire_failed);
        let dial = self.dial.take().map(|dial| RetiredDial {
            _peer: dial.peer.retire_failed(),
            _read: dial.read,
            _message: dial.message,
            _answer: dial.answer,
            _send: dial.send,
        });
        let Self {
            run,
            handoff,
            leave,
            original,
            retired,
            detail,
            stopped_record,
            retired_listener,
            retired_dial,
            retired_gate,
            preflight,
            active,
            ..
        } = *self;
        let FoundationRun {
            context,
            resources,
            listener: _,
            original: _,
        } = *run;
        let cause = leave.and(original.as_ref().map(|enter| enter.payload().cause));
        let driver = if recorded.is_some() {
            Driver::Recorded(Box::new(NoRecordExit::new(
                context, resources, deadline, false,
            )))
        } else {
            Driver::Halt(Box::new(FailedRunCleanup::after_expired_rollback(
                context,
                resources,
                leave,
                cause,
                release,
                admit_reply,
            )))
        };
        Ok(Box::new(FoundationExpiredRollbackExit {
            held: RetainedExpiredRollback {
                handoff,
                leave,
                original,
                retired,
                detail,
                stopped_record,
                listener,
                prior_listener: retired_listener,
                dial,
                prior_dial: retired_dial,
                gate: retired_gate,
                preflight,
                active,
                recorded,
            },
            driver,
            progress: FoundationRollbackExitProgress::Progress,
        }))
    }
}

impl FoundationExpiredRollbackExit {
    #[cfg(test)]
    pub(super) fn test_templates(
        &self,
    ) -> Option<&crate::flow::context::template_driver::StoppedTemplateAccount> {
        self.held.active.as_ref()?.templates.as_ref()
    }
    #[cfg(test)]
    pub(super) fn test_read(&self) -> Option<&crate::flow::context::read_driver::ReadTransaction> {
        Some(self.held.active.as_ref()?.read.as_ref()?.evidence())
    }
    #[cfg(test)]
    pub(super) fn test_open(
        &self,
    ) -> Option<&crate::flow::context::boot_open_driver::StoppedBootOpenAccount> {
        self.held.active.as_ref()?.open.as_ref()
    }
    pub fn progress(&self) -> FoundationRollbackExitProgress {
        self.progress
    }
    pub fn channel_fault(&self) -> Option<crate::ChannelFault> {
        (self.progress == FoundationRollbackExitProgress::Closed)
            .then_some(crate::ChannelFault::Closed)
    }
    pub fn unload_sequence(&self) -> Option<u64> {
        match &self.driver {
            Driver::Halt(tail) => tail.unload_sequence().map(|sequence| sequence.0),
            Driver::Recorded(_) => self.held.recorded,
        }
    }
    pub fn abandoned(&self) -> bool {
        match &self.driver {
            Driver::Halt(tail) => tail.abandoned(),
            Driver::Recorded(tail) => tail.abandoned(),
        }
    }
    pub fn advance(&mut self) -> FoundationRollbackExitProgress {
        self.advance_with_io(&mut MonotonicIo)
    }
    pub fn wait_for_progress(&mut self) -> FoundationRollbackExitProgress {
        self.wait_with_io(&mut MonotonicIo)
    }
    pub(super) fn advance_with_io(
        &mut self,
        io: &mut impl PumpIo,
    ) -> FoundationRollbackExitProgress {
        if self.progress == FoundationRollbackExitProgress::Closed {
            return self.progress;
        }
        self.progress = match &mut self.driver {
            Driver::Halt(tail) => convert(tail.advance(io)),
            Driver::Recorded(tail) => tail.advance(io),
        };
        self.progress
    }
    pub(super) fn wait_with_io(&mut self, io: &mut impl PumpIo) -> FoundationRollbackExitProgress {
        if self.progress != FoundationRollbackExitProgress::Pending {
            return self.progress;
        }
        self.progress = match &mut self.driver {
            Driver::Halt(tail) => convert(tail.wait_for_progress(io)),
            Driver::Recorded(tail) => tail.wait(io),
        };
        self.progress
    }
    #[cfg(test)]
    pub(super) fn reap_composition_test_children(&mut self) {
        match &mut self.driver {
            Driver::Halt(tail) => tail.reap_composition_test_children(),
            Driver::Recorded(tail) => tail.reap_composition_test_children(),
        }
    }
    #[cfg(test)]
    pub(super) fn failure(&self) -> Option<&RunFailure> {
        match &self.driver {
            Driver::Halt(tail) => Some(tail.failure()),
            Driver::Recorded(_) => None,
        }
    }
    #[cfg(test)]
    pub(super) fn deadline(&self) -> Option<u64> {
        match &self.driver {
            Driver::Halt(tail) => tail.deadline(),
            Driver::Recorded(_) => self.held.leave.map(AbsoluteBound::deadline_ns),
        }
    }
    #[cfg(test)]
    pub(super) fn test_spu_child(
        &self,
    ) -> Option<crate::flow::context::failed_enter_driver::ChildState> {
        match &self.driver {
            Driver::Halt(tail) => tail.resources().spu_child(),
            Driver::Recorded(_) => None,
        }
    }
}
fn convert(progress: FailedEnterProgress) -> FoundationRollbackExitProgress {
    match progress {
        FailedEnterProgress::Progress => FoundationRollbackExitProgress::Progress,
        FailedEnterProgress::Pending => FoundationRollbackExitProgress::Pending,
        FailedEnterProgress::Closed => FoundationRollbackExitProgress::Closed,
    }
}
