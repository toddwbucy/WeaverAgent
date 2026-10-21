//! Observation of the real transaction scope, including rollback on return.

use super::super::ServingSqlite;
use crate::{FileCommit, FileFault, ObservedFiling, ReadFault, serving_clock::monotonic_ns};
use weaver_types::state_flow::{DecimalU64, OperationPhase, PhaseName, StateRefusalCode};

#[derive(Default)]
pub(super) struct TransactionTiming {
    began: Option<DecimalU64>,
    ended: Option<DecimalU64>,
    failure: Option<std::io::Error>,
    #[cfg(test)]
    fail_end: bool,
}

pub(super) struct Guard<'a>(&'a mut TransactionTiming);

impl TransactionTiming {
    pub(super) fn start(&mut self) -> Result<Guard<'_>, FileFault> {
        match monotonic_ns() {
            Ok(now) => self.began = Some(now),
            Err(error) => {
                self.failure = Some(error);
                return Err(FileFault::new(
                    StateRefusalCode::WriteFailed,
                    "transaction clock failed",
                ));
            }
        }
        Ok(Guard(self))
    }

    fn finish(self, result: Result<FileCommit, FileFault>) -> Result<ObservedFiling, ReadFault> {
        let failure = |detail: String| ReadFault::ComponentFailure { detail };
        if let Some(error) = self.failure {
            return Err(failure(format!("transaction observation failed: {error}")));
        }
        let transaction = match (self.began, self.ended) {
            (None, None) => None,
            (Some(began_ns), Some(ended_ns)) if began_ns <= ended_ns => Some(OperationPhase {
                name: PhaseName::Transaction,
                began_ns,
                ended_ns,
            }),
            _ => return Err(failure("incomplete transaction observation".into())),
        };
        Ok(ObservedFiling {
            result,
            transaction,
        })
    }
}

impl Drop for Guard<'_> {
    fn drop(&mut self) {
        // The guard is declared before the SQL transaction, so the latter's
        // rollback drop completes first on every early-return path.
        #[cfg(test)]
        if self.0.fail_end {
            self.0.failure = Some(std::io::Error::other("injected end clock failure"));
            return;
        }
        match monotonic_ns() {
            Ok(now) => self.0.ended = Some(now),
            Err(error) => self.0.failure = Some(error),
        }
    }
}

impl ServingSqlite {
    pub fn file_observed(
        &mut self,
        filing: &weaver_types::state_flow::StatementFile,
    ) -> Result<ObservedFiling, ReadFault> {
        let mut timing = TransactionTiming::default();
        let result = super::file_inner(self, filing, Some(&mut timing));
        timing.finish(result)
    }

    pub fn file_pair_observed(
        &mut self,
        filing: &weaver_types::state_flow::PairFile,
    ) -> Result<ObservedFiling, ReadFault> {
        let mut timing = TransactionTiming::default();
        let result = super::file_pair_inner(self, filing, Some(&mut timing));
        timing.finish(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ServingStore;

    #[test]
    fn transaction_phase_distinguishes_preflight_refusal_commit_and_rollback() {
        let mut store = super::super::tests::store();
        let filing = super::super::tests::statement(1, "message.user");
        let accepted = store.file_observed(&filing).unwrap();
        accepted.result.unwrap();
        let phase = accepted.transaction.unwrap();
        assert!(phase.began_ns <= phase.ended_ns);
        let duplicate = store.file_observed(&filing).unwrap();
        assert_eq!(
            duplicate.result.unwrap_err().code,
            StateRefusalCode::DuplicateSource
        );
        assert!(duplicate.transaction.is_none());
        store.connection.execute_batch("CREATE TEMP TRIGGER fail_write BEFORE INSERT ON statement_binding BEGIN SELECT RAISE(ABORT,'fixture'); END").unwrap();
        let failed = store
            .file_observed(&super::super::tests::statement(2, "message.user"))
            .unwrap();
        assert!(failed.result.is_err());
        assert!(failed.transaction.is_some());
        assert_eq!(store.held().unwrap().0, 2);
    }

    #[test]
    fn failed_end_clock_keeps_committed_content_uncertain_instead_of_a_refusal() {
        let mut store = super::super::tests::store();
        let filing = super::super::tests::statement(1, "message.user");
        let mut timing = TransactionTiming {
            fail_end: true,
            ..TransactionTiming::default()
        };
        let outcome = super::super::file_inner(&mut store, &filing, Some(&mut timing));
        assert!(outcome.is_ok());
        assert!(matches!(
            timing.finish(outcome),
            Err(ReadFault::ComponentFailure { .. })
        ));
        assert_eq!(
            store.file(&filing).unwrap_err().code,
            StateRefusalCode::DuplicateSource
        );
    }
}
