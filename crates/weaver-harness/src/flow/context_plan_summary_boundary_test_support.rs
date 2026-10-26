//! Read-only observations of the actual summary settlement boundary.
use super::*;

impl PlanSummaryTransaction {
    pub(in crate::flow::context) fn expired_test_settlement(&self) -> (bool, bool) {
        (self.rollback.read_settled, self.rollback.account_settled)
    }

    pub(in crate::flow::context) fn expired_test_recorded_read(&self) -> Option<&RecordedRead> {
        self.recorded.as_ref().map(|recorded| recorded.read())
    }

    pub(in crate::flow::context) fn expired_test_refusal(&self) -> Option<&ReadFailure> {
        self.rollback.refusal.as_ref()
    }
}
