//! Consume actual stopped residency and its matching owned run into teardown.
use super::*;
use crate::flow::context::failed_enter_driver::{
    FailedEnterCleanup, FailedEnterFinished, FailedEnterProgress, FailedEnterRefusal,
};

#[derive(Debug)]
pub(in crate::flow) enum CleanupJoinRefusal {
    ForeignContext,
    NoSpu,
    AcceptedLeave,
    Cleanup(FailedEnterRefusal),
}

pub(in crate::flow) struct CleanupJoinRejected {
    pub account: StoppedResidency,
    pub context: RecordingContext,
    pub resources: FailedEnterResources,
    pub refusal: CleanupJoinRefusal,
}

/// Both accounts remain move-owned. No live context escapes this wrapper.
pub(in crate::flow) struct ResidencyCleanup {
    cleanup: FailedEnterCleanup,
    account: StoppedResidency,
}

pub(in crate::flow) struct ResidencyFinished {
    pub cleanup: FailedEnterFinished,
    pub account: StoppedResidency,
}

impl StoppedResidency {
    /// The borrowed composer first returns its account; the outer root can then
    /// move the original owned context and bag here. Identity and the full Failed
    /// domain are rechecked, and every rejected input is returned untouched.
    #[allow(clippy::result_large_err)]
    pub(in crate::flow) fn into_cleanup(
        self,
        context: RecordingContext,
        resources: FailedEnterResources,
    ) -> Result<ResidencyCleanup, CleanupJoinRejected> {
        let refusal = if !std::sync::Arc::ptr_eq(&self.origin, &context.read_origin) {
            Some(CleanupJoinRefusal::ForeignContext)
        } else if resources.spu_child().is_none() {
            // No-SPU terminal representation remains held for #285's merge.
            Some(CleanupJoinRefusal::NoSpu)
        } else if context.deadlines.get(PumpDeadline::Leave).is_some() || context.place_leaving {
            Some(CleanupJoinRefusal::AcceptedLeave)
        } else {
            None
        };
        if let Some(refusal) = refusal {
            return Err(CleanupJoinRejected {
                account: self,
                context,
                resources,
                refusal,
            });
        }
        match context.into_failed_enter_cleanup(resources) {
            Ok(mut cleanup) => {
                let mut account = self;
                if let Some(reply) = account.reply.take() {
                    cleanup.retain_residency_completion(reply);
                }
                Ok(ResidencyCleanup { cleanup, account })
            }
            Err(rejected) => Err(CleanupJoinRejected {
                account: self,
                context: rejected.context,
                resources: rejected.resources,
                refusal: CleanupJoinRefusal::Cleanup(rejected.refusal),
            }),
        }
    }
}

impl ResidencyCleanup {
    pub(in crate::flow) fn failure(&self) -> &RunFailure {
        self.cleanup.failure()
    }
    pub(in crate::flow) fn deadline(&self) -> Option<u64> {
        self.cleanup.deadline()
    }
    pub(in crate::flow) fn advance(&mut self, io: &mut impl PumpIo) -> FailedEnterProgress {
        self.cleanup.advance(io)
    }
    pub(in crate::flow) fn wait_for_progress(
        &mut self,
        io: &mut impl PumpIo,
    ) -> FailedEnterProgress {
        self.cleanup.wait_for_progress(io)
    }
    #[allow(clippy::result_large_err)]
    pub(in crate::flow) fn finish(self) -> Result<ResidencyFinished, Self> {
        match self.cleanup.finish() {
            Ok(cleanup) => Ok(ResidencyFinished {
                cleanup,
                account: self.account,
            }),
            Err(cleanup) => Err(Self {
                cleanup,
                account: self.account,
            }),
        }
    }
}

#[cfg(test)]
#[path = "context_residency_cleanup_tests.rs"]
pub(in crate::flow::context) mod tests;
