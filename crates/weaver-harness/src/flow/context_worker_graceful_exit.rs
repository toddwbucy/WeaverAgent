//! The original graceful Leave consumes its phase accounts and actual Ready.
//! Shared Lower, capture and release retain its original peer and fixed outcome.
//! Converted drain, Quiesce and wind-down retain their accounts through cancellation.
//! The refused-capture policy is pending #348; that owner remains retained.
use super::*;

/// The original graceful holder, distinct from both rollback and sole force.
///
/// ```compile_fail,E0277
/// fn clone_required<T: Clone>() {}
/// clone_required::<weaver_harness::FoundationGracefulLeaveHandoff>();
/// ```
pub struct FoundationGracefulLeaveHandoff {
    pub(in crate::flow::context::worker_startup) inner: Box<FoundationEnteredRollbackHandoff>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoundationGracefulLeaveProgress {
    Progress,
    Pending,
    /// Internal stop only. No final outcome is elected before #348 merges.
    CaptureRefused,
    Closed,
}

/// The consumed run, original directive and capture stay under one owner.
///
/// ```compile_fail,E0277
/// fn clone_required<T: Clone>() {}
/// clone_required::<weaver_harness::FoundationGracefulLeave>();
/// ```
pub struct FoundationGracefulLeave {
    inner: Box<FoundationEnteredRollback>,
}

impl FoundationRun {
    #[allow(clippy::result_large_err)]
    pub fn graceful_leave(
        self: Box<Self>,
        mut handoff: Box<FoundationGracefulLeaveHandoff>,
    ) -> Result<Box<FoundationGracefulLeave>, (Box<Self>, Box<FoundationGracefulLeaveHandoff>)>
    {
        if handoff.inner.kind != LeaveKind::Graceful || !handoff.inner.matches(&self) {
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
        Ok(Box::new(FoundationGracefulLeave {
            inner: self.begin_leave_cleanup(handoff.inner),
        }))
    }
}

impl FoundationGracefulLeave {
    pub fn progress(&self) -> FoundationGracefulLeaveProgress {
        if self.inner.stage == CleanupStage::CaptureRefused && self.inner.failed.is_none() {
            return FoundationGracefulLeaveProgress::CaptureRefused;
        }
        match self.inner.progress() {
            FoundationEnteredRollbackProgress::Progress => {
                FoundationGracefulLeaveProgress::Progress
            }
            FoundationEnteredRollbackProgress::Pending => FoundationGracefulLeaveProgress::Pending,
            FoundationEnteredRollbackProgress::Closed => FoundationGracefulLeaveProgress::Closed,
        }
    }
    pub fn advance(&mut self) -> FoundationGracefulLeaveProgress {
        if self.progress() != FoundationGracefulLeaveProgress::CaptureRefused {
            self.inner.advance();
        }
        self.progress()
    }
    pub fn wait_for_progress(&mut self) -> FoundationGracefulLeaveProgress {
        if self.progress() == FoundationGracefulLeaveProgress::Pending {
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
    #[cfg(test)]
    pub(in crate::flow::context::worker_startup) fn test_inner(
        &mut self,
    ) -> &mut FoundationEnteredRollback {
        &mut self.inner
    }
}
