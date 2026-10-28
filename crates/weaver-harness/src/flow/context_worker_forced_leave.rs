//! Forced Leave consumes its own original directive and successful Ready.
//! It shares Lower, account settlement and release with entered rollback, but
//! must settle its capture before release. A refused capture carries its cleanup
//! acknowledgement, then finishes forced Left without a saved-state report.
use super::*;

/// A distinct move-owned handoff; a rollback cannot be used to request a save.
///
/// ```compile_fail,E0277
/// fn clone_required<T: Clone>() {}
/// clone_required::<weaver_harness::FoundationForcedLeaveHandoff>();
/// ```
pub struct FoundationForcedLeaveHandoff {
    pub(in crate::flow::context::worker_startup) inner: Box<FoundationEnteredRollbackHandoff>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoundationForcedLeaveProgress {
    Progress,
    Pending,
    Closed,
}

/// The consumed run, original directive and capture stay under one owner.
///
/// ```compile_fail,E0277
/// fn clone_required<T: Clone>() {}
/// clone_required::<weaver_harness::FoundationForcedLeave>();
/// ```
pub struct FoundationForcedLeave {
    inner: Box<FoundationEnteredRollback>,
}

impl FoundationRun {
    #[allow(clippy::result_large_err)]
    pub fn forced_leave(
        self: Box<Self>,
        mut handoff: Box<FoundationForcedLeaveHandoff>,
    ) -> Result<Box<FoundationForcedLeave>, (Box<Self>, Box<FoundationForcedLeaveHandoff>)> {
        if handoff.inner.kind != LeaveKind::Forced || !handoff.inner.matches(&self) {
            return Err((self, handoff));
        }
        if handoff
            .inner
            .peer
            .as_mut()
            .unwrap()
            .enable_capture()
            .is_err()
        {
            return Err((self, handoff));
        }
        Ok(Box::new(FoundationForcedLeave {
            inner: self.begin_leave_cleanup(handoff.inner),
        }))
    }
}

impl FoundationForcedLeave {
    pub fn progress(&self) -> FoundationForcedLeaveProgress {
        match self.inner.progress() {
            FoundationEnteredRollbackProgress::Progress => FoundationForcedLeaveProgress::Progress,
            FoundationEnteredRollbackProgress::Pending => FoundationForcedLeaveProgress::Pending,
            FoundationEnteredRollbackProgress::Closed => FoundationForcedLeaveProgress::Closed,
        }
    }
    pub fn advance(&mut self) -> FoundationForcedLeaveProgress {
        self.inner.advance();
        self.progress()
    }
    pub fn wait_for_progress(&mut self) -> FoundationForcedLeaveProgress {
        if self.progress() == FoundationForcedLeaveProgress::Pending {
            self.inner.wait_for_progress();
        }
        self.progress()
    }
    pub fn detail(&self) -> Option<&str> {
        self.inner.detail()
    }
    pub fn unload_sequence(&self) -> Option<u64> {
        self.inner.unload_sequence()
    }
    pub fn abandoned(&self) -> bool {
        self.inner.abandoned()
    }
    pub fn process_exit(&self) -> Option<crate::FoundationProcessExit> {
        self.inner.process_exit()
    }
    #[cfg(test)]
    pub(in crate::flow::context::worker_startup) fn test_inner(
        &mut self,
    ) -> &mut FoundationEnteredRollback {
        &mut self.inner
    }
}
