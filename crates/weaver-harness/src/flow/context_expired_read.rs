//! Retain expired boot reads without closing the member ahead of classifier reap.
//! The original read gets no new request, Release, recall or success proof.
use super::read_driver::ReadTransaction;
use super::worker_startup::expired::ExpiredStartupAccountPermit;
use super::*;

pub(super) enum ExpiredReadAccount {
    Prompt(Box<ReadTransaction>),
    Summary(PlanSummaryAccount),
    BootPairs(super::boot_pair_driver::StoppedBootPairsAccount),
}

impl ExpiredReadAccount {
    pub(super) fn evidence(&self) -> &ReadTransaction {
        match self {
            Self::Prompt(read) => read,
            Self::Summary(account) => account.read_evidence().expect("original summary read"),
            Self::BootPairs(account) => account.read_evidence().expect("original boot pair read"),
        }
    }
}

impl RecordingContext {
    pub(super) fn can_park_expired_startup_read(
        &self,
        permit: &ExpiredStartupAccountPermit,
    ) -> bool {
        permit.allows(self)
            && self.read.as_ref().is_some_and(|read| {
                read.plan_bound().expired(self.last_ns)
                    && read.rollback_wait_matches(self)
                    && (read.boot_prompt_enter() == self.deadlines.get(PumpDeadline::Enter)
                        || (read.is_boot_pairs() && self.can_detach_expired_boot_pairs(permit))
                        || (read.is_summary()
                            && self.plan_summary.is_some()
                            && self.plan_summary_is_boot_cleanup()))
            })
            && self.read_rollback_permit().is_ok()
    }

    pub(super) fn park_expired_startup_read(
        &mut self,
        permit: &ExpiredStartupAccountPermit,
        io: &mut impl PumpIo,
    ) -> Result<ExpiredReadAccount, RunFailure> {
        self.release_live(io)?;
        if !self.can_park_expired_startup_read(permit) {
            return Err(RunFailure::ReadOwned);
        }
        if self.read.as_ref().unwrap().is_boot_pairs() {
            return self
                .detach_expired_boot_pairs(permit, io)
                .map(ExpiredReadAccount::BootPairs);
        }
        if self.read.as_ref().unwrap().is_summary() {
            return self
                .park_expired_boot_plan_summary(permit, io)
                .map(ExpiredReadAccount::Summary);
        }
        if self.plan_summary.is_some()
            || self.boot_pairs.is_some()
            || self.repair.is_some()
            || self.filing.is_some()
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
            || self.waiting_content.is_some()
            || self.completed_author.is_some()
        {
            return Err(RunFailure::ReadOwned);
        }
        self.check_held_input()?;
        let read_permit = self.read_rollback_permit()?;
        self.flow
            .park_boot_read_rollback(
                &mut FlowRecording {
                    barrier: &mut self.barrier,
                    inbox: &mut self.inbox,
                    budget: &mut self.budget,
                    author: &self.author,
                },
                &read_permit,
            )
            .map_err(|failure| self.flow_failed(io, failure))?;
        self.phase = RecordingPhase::Releasing;
        let mut read = self.read.take().unwrap();
        read.stop_expired_boot_read();
        // The original Enter remains in both the read purpose and retired_enter;
        // the read bound remains in this account and is never replaced by Leave.
        self.deadlines.disarm(PumpDeadline::StateControl);
        self.disarm_wait(PumpDeadline::Enter);
        Ok(ExpiredReadAccount::Prompt(Box::new(read)))
    }

    pub(super) fn retire_expired_startup_read(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<weaver_trace::MemberRelease, RunFailure> {
        self.live(io)?;
        self.flow
            .retire_parked_boot_read(&mut FlowRecording {
                barrier: &mut self.barrier,
                inbox: &mut self.inbox,
                budget: &mut self.budget,
                author: &self.author,
            })
            .map_err(|failure| self.flow_failed(io, failure))
    }
}
