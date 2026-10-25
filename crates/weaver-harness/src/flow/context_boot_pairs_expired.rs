//! Expired boot repair retains its original Open, reads and filed terminals.
//! Moving this account never certifies repair or renews its work allowance.
use super::*;
use crate::flow::context::read_driver::ReadTransaction;
use crate::flow::context::worker_startup::expired::ExpiredStartupAccountPermit;

pub(in crate::flow::context) struct StoppedBootPairsAccount {
    work: Box<BootPairsTransaction>,
    read: Option<Box<ReadTransaction>>,
    filing: Option<Box<super::super::receipt_driver::StoppedBootFilingAccount>>,
}

impl StoppedBootPairsAccount {
    #[cfg(test)]
    pub(in crate::flow::context) fn test_filing(&self) -> Option<String> {
        self.filing.as_ref().map(|filing| filing.test_evidence())
    }
    pub(in crate::flow::context) fn parked_filing(&self) -> bool {
        self.filing.as_ref().is_some_and(|filing| filing.parked)
    }
    pub(in crate::flow::context) fn account_complete(&self) -> bool {
        self.filing.as_ref().is_none_or(|filing| filing.complete)
    }
    pub(in crate::flow::context) fn read_evidence(&self) -> Option<&ReadTransaction> {
        self.read.as_deref()
    }
    #[cfg(test)]
    pub(in crate::flow::context) fn test_evidence(&self) -> String {
        self.work.expired_test_evidence()
    }
}

impl BootPairsTransaction {
    #[cfg(test)]
    pub(in crate::flow::context) fn expired_test_expected(&self) -> Option<&PairEnded> {
        self.expected.as_ref()
    }
    #[cfg(test)]
    pub(in crate::flow::context) fn expired_test_terminals(&self) -> &[FilingCompletion] {
        &self.terminals
    }

    fn expired_owner_matches(&self, context: &RecordingContext) -> bool {
        self.actual_boot_owner(context)
            && !self.driving
            && context.open_start.as_ref() == Some(&self.start)
            && context.deadlines.get(PumpDeadline::Enter) == Some(self.prior.enter)
            && context.boot_head.as_ref().is_some_and(|head| {
                head.belongs_to(context)
                    && head.recall_sequence() == self.prior.head
                    && head.enter_bound() == self.prior.enter
            })
            && context.deadlines.get(PumpDeadline::Decision)
                == if self.receipt_settled {
                    None
                } else {
                    self.bound
                }
    }
    #[cfg(test)]
    pub(in crate::flow::context) fn expired_test_evidence(&self) -> String {
        format!(
            "{:?}",
            (
                &self.prior,
                &self.start,
                self.stage,
                &self.initial,
                &self.recheck,
                &self.terminals,
                &self.expected,
                self.bound,
                self.receipt_settled,
                (&self.cancelled_event, self.cancelled, self.refusal),
            )
        )
    }
}

impl RecordingContext {
    pub(in crate::flow::context) fn can_detach_expired_boot_pairs(
        &self,
        permit: &ExpiredStartupAccountPermit,
    ) -> bool {
        permit.allows(self)
            && self.boot_pairs.as_ref().is_some_and(|work| {
                work.expired_owner_matches(self)
                    && self.read.as_ref().is_none_or(|read| {
                        matches!(work.stage, Stage::Initial | Stage::Recheck)
                            && read.is_boot_pairs()
                            && read.plan_bound().expired(self.last_ns)
                            && read.rollback_wait_matches(self)
                    })
            })
            && self.filing.as_ref().is_none_or(|_| {
                let work = self.boot_pairs.as_ref().unwrap();
                work.stage == Stage::Terminals
                    && work
                        .expected
                        .as_ref()
                        .zip(work.bound)
                        .is_some_and(|(expected, bound)| {
                            self.boot_filing_rollback_permit(expected, bound).is_some()
                        })
            })
            && (self.read.is_some() || self.deadlines.get(PumpDeadline::StateControl).is_none())
            && (self.filing.is_some() || self.read_rollback_permit().is_ok())
    }

    pub(in crate::flow::context) fn detach_expired_boot_pairs(
        &mut self,
        permit: &ExpiredStartupAccountPermit,
        io: &mut impl PumpIo,
    ) -> Result<StoppedBootPairsAccount, RunFailure> {
        self.release_live(io)?;
        if !self.can_detach_expired_boot_pairs(permit)
            || self.plan_summary.is_some()
            || self.repair.is_some()
            || self.output.is_some()
            || self.pair_work.is_some()
            || self.marks.is_some()
            || self.member_capture.is_some()
            || self.freshness.is_some()
            || self.inspection.is_some()
            || self.measure_blocks_external()
            || self.boot_open_blocks_external()
            || self.place_owed()
            || self.open_turn.is_some()
            || self.input.snapshot().is_some()
            || self.input_turn.is_some()
            || self.input_author.is_some()
            || (self.filing.is_none()
                && (self.waiting_content.is_some() || self.completed_author.is_some()))
        {
            return Err(RunFailure::BootPairsOwned);
        }
        self.check_held_input()?;
        let filing_permit = self.filing.as_ref().map(|_| {
            let work = self.boot_pairs.as_ref().unwrap();
            self.boot_filing_rollback_permit(work.expected.as_ref().unwrap(), work.bound.unwrap())
                .unwrap()
        });
        let read_permit = self
            .filing
            .is_none()
            .then(|| self.read_rollback_permit())
            .transpose()?;
        let mut cx = FlowRecording {
            barrier: &mut self.barrier,
            inbox: &mut self.inbox,
            budget: &mut self.budget,
            author: &self.author,
        };
        // Pending reads retain their actual member endpoint until classifier
        // reap. Between reads, ordinary cleanup can obtain a fresh Grants
        // comparison under Leave after this old owner is transferred.
        let result = if let Some(permit) = filing_permit {
            self.flow
                .park_boot_filing_rollback(&mut cx, permit)
                .map(Some)
        } else if self.read.is_some() {
            self.flow
                .park_boot_read_rollback(&mut cx, read_permit.as_ref().unwrap())
                .map(|()| None)
        } else {
            self.flow.require_releasing(&mut cx).map(|()| None)
        };
        let filing = result
            .map_err(|failure| self.flow_failed(io, failure))?
            .map(|(parked, complete)| Box::new(self.stop_boot_filing_author(parked, complete)));
        let work = Box::new(self.boot_pairs.take().unwrap());
        let read = self.read.take().map(|mut read| {
            read.stop_expired_boot_read();
            Box::new(read)
        });
        if work.bound.is_some() {
            self.deadlines.disarm(PumpDeadline::Decision);
        }
        if read.is_some() || filing.as_ref().is_some_and(|filing| filing.parked) {
            self.phase = RecordingPhase::Releasing;
            self.deadlines.disarm(PumpDeadline::StateControl);
            self.disarm_wait(PumpDeadline::Enter);
        }
        Ok(StoppedBootPairsAccount { work, read, filing })
    }

    pub(in crate::flow::context) fn retire_expired_boot_filing(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<weaver_trace::MemberRelease, RunFailure> {
        self.live(io)?;
        let result = self.flow.retire_boot_filing_state(&mut FlowRecording {
            barrier: &mut self.barrier,
            inbox: &mut self.inbox,
            budget: &mut self.budget,
            author: &self.author,
        });
        result.map_err(|failure| self.flow_failed(io, failure))
    }
}
