//! A terminal result releases the one allocator only after exact trace/read/file
//! duties settle. The original root packet and all evidence move with that result.
use super::*;
use crate::flow::author::AuthoredSource;
use std::sync::Arc;
use weaver_types::{LifecycleAnswer, LifecycleAsk, RefusalRecord};

pub(super) struct RootPlanRecord {
    identity: Arc<()>,
    original: Option<AuthoredEvent>,
    ticket: Option<u64>,
    sequence: Option<Sequence>,
    consumed: bool,
    returned: StoppedAuthor,
}

pub(in crate::flow::context) struct RootPlanAccount {
    work: Box<RootPlanTransaction>,
}
impl RootPlanAccount {
    pub(in crate::flow::context) fn packet(&self) -> &RootPlanPacket {
        match &self.work.input {
            RootPlanInput::Accepted(input) => input.packet(),
            RootPlanInput::Refused { original, .. } => original,
        }
    }
    pub(in crate::flow::context) fn work(&self) -> &RootPlanTransaction {
        &self.work
    }
}
pub(in crate::flow::context) struct RootPlanUpdated {
    account: RootPlanAccount,
}
impl RootPlanUpdated {
    pub(in crate::flow::context) fn account(&self) -> &RootPlanAccount {
        &self.account
    }
    pub(in crate::flow::context) fn answer(&self) -> LifecycleAnswer {
        let summary = self.account.work.summary().unwrap().summary().unwrap();
        LifecycleAnswer::PlanUpdated {
            plan: summary.plan.clone(),
            source: summary.last_source.clone(),
        }
    }
}
pub(in crate::flow::context) enum RootPlanCompletion {
    Updated(RootPlanUpdated),
    Refused {
        refusal: LifecycleRefusal,
        account: RootPlanAccount,
    },
    Cancelled(RootPlanAccount),
}

fn wire_refusal(value: &PlanUpdateRefused) -> LifecycleRefusal {
    LifecycleRefusal::PlanUpdateRefused {
        case: value.case,
        detail: value.detail.clone(),
        rejected_blocks: value.rejected_blocks.clone(),
        read_code: value.read_code,
    }
}

impl RootPlanTransaction {
    pub(in crate::flow::context) fn refusal_sequence(&self) -> Option<Sequence> {
        self.refusal_record.as_ref()?.sequence
    }
    pub(in crate::flow::context) fn refusal_ticket(&self) -> Option<u64> {
        self.refusal_record.as_ref()?.ticket
    }
    fn refusal_parent(&self) -> Option<Sequence> {
        self.refused_read
            .as_ref()
            .map(|proof| proof.measurement_sequence())
            .or(self.fault_sequence)
            .or_else(|| self.input_filing.as_ref().map(FilingCompletion::sequence))
    }
}

impl RecordingContext {
    pub(in crate::flow::context) fn root_plan_author_claim(&self) -> Option<AuthorClaim> {
        let record = self.root_plan_work()?.refusal_record.as_ref()?;
        (record.original.is_none() && !record.consumed)
            .then(|| AuthorClaim::RootPlan(Arc::clone(&record.identity)))
    }
    pub(in crate::flow::context) fn claim_root_plan_author_ticket(
        &mut self,
        identity: &Arc<()>,
        ticket: u64,
    ) -> bool {
        let Some(work) = self.root_plan_work_mut() else {
            return false;
        };
        let Some(record) = work.refusal_record.as_mut() else {
            return false;
        };
        if !work.driving
            || work.stage != Stage::Refusal
            || !Arc::ptr_eq(&record.identity, identity)
            || record.ticket.is_some()
        {
            return false;
        }
        record.ticket = Some(ticket);
        true
    }
    pub(in crate::flow::context) fn bind_root_plan_author(&mut self, sequence: Sequence) {
        let ticket = self.barrier.recorded_author_ticket(sequence);
        if let Some(record) = self
            .root_plan_work_mut()
            .and_then(|work| work.refusal_record.as_mut())
            && record.ticket.is_some()
            && record.ticket == ticket
            && record.sequence.is_none()
        {
            record.sequence = Some(sequence);
        }
    }

    pub(in crate::flow::context) fn settle_root_plan_update<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<RootPlanProgress, RunFailure> {
        self.live(ports.io)?;
        let Some(work) = self.root_plan_work() else {
            return Err(RunFailure::MarksOwned);
        };
        if work.settled {
            return Ok(RootPlanProgress::Complete);
        }
        if !matches!(
            work.stage,
            Stage::Refusal | Stage::Cancelled | Stage::SummaryReady
        ) {
            return Err(RunFailure::MarksOwned);
        }
        if self.deadlines.get(PumpDeadline::Decision) != Some(work.bound) {
            return Err(
                self.root_plan_fault(ports.io, "root settlement lost its original Decision")
            );
        }
        self.root_plan_work_mut().unwrap().driving = true;
        let result = self.settle_root_plan_owned(ports);
        self.root_plan_work_mut().unwrap().driving = false;
        result
    }

    fn settle_root_plan_owned<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<RootPlanProgress, RunFailure> {
        if self.root_plan_work().unwrap().stage == Stage::Refusal
            && !self.record_root_plan_refusal(ports)?
        {
            return Ok(RootPlanProgress::Pending);
        }
        if self.read.is_some()
            || self.filing.is_some()
            || self.plan_summary.is_some()
            || self.plan_cache.required.is_some()
            || self.flow.has_filing_consumer()
            || self.waiting_content.is_some()
            || self.completed_author.is_some()
            || self.barrier.pending_author().is_some()
            || self.flow.has_pending_record()
            || self.flow.pending_control().is_some()
            || self.flow.pending_check().is_some()
            || !self.flow.state().outstanding().is_empty()
            || self.flow.state().held_measurement().is_some()
        {
            return Ok(RootPlanProgress::Pending);
        }
        let work = self.root_plan_work().unwrap();
        if work.stage == Stage::SummaryReady
            && work
                .summary()
                .and_then(EffectPlanSummary::summary)
                .is_none()
        {
            return Err(
                self.root_plan_fault(ports.io, "root settlement lost its committed summary")
            );
        }
        // The last live check and exact completed owners bound this terminal
        // transition. Later retrieval cannot expire or renew the finished work.
        self.deadlines.disarm(PumpDeadline::Decision);
        let work = self.root_plan_work_mut().unwrap();
        work.settled = true;
        work.stage = Stage::Complete;
        Ok(RootPlanProgress::Complete)
    }

    fn record_root_plan_refusal<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<bool, RunFailure> {
        if self.root_plan_work().unwrap().refusal_record.is_none() {
            let work = self.root_plan_work().unwrap();
            let refusal = work.refusal().ok_or(RunFailure::MarksOwned)?;
            let event = AuthoredEvent {
                kind: Kind::Refusal,
                subsystem: weaver_trace::Subsystem::Harness,
                turn: None,
                source: AuthoredSource::Runtime,
                causal_parent: work.refusal_parent(),
                payload: Some(Payload::Refusal(
                    serde_json::value::to_raw_value(&RefusalRecord::Lifecycle {
                        asked: LifecycleAsk::PlanUpdate,
                        refusal: wire_refusal(refusal),
                    })
                    .expect("typed root refusal serializes"),
                )),
            };
            self.root_plan_work_mut().unwrap().refusal_record = Some(RootPlanRecord {
                identity: Arc::new(()),
                original: Some(event),
                ticket: None,
                sequence: None,
                consumed: false,
                returned: StoppedAuthor::default(),
            });
        }
        let record = self
            .root_plan_work()
            .unwrap()
            .refusal_record
            .as_ref()
            .unwrap();
        record.returned.check()?;
        let claim = AuthorClaim::RootPlan(Arc::clone(&record.identity));
        if record.original.is_some() {
            if !self.serving_observation_author_available() {
                return Ok(false);
            }
            let event = self
                .root_plan_work_mut()
                .unwrap()
                .refusal_record
                .as_mut()
                .unwrap()
                .original
                .take()
                .unwrap();
            self.waiting_content = Some(WaitingAuthor::claimed(event, claim.clone()));
        }
        let record = self
            .root_plan_work()
            .unwrap()
            .refusal_record
            .as_ref()
            .unwrap();
        let waiting = self
            .waiting_content
            .as_ref()
            .is_some_and(|waiting| waiting.matches(Some(&claim)));
        let pending = record.ticket.is_some_and(|ticket| {
            self.barrier
                .pending_author()
                .is_some_and(|pending| pending.ticket == ticket)
        });
        let completed = record.sequence.is_some() && self.completed_author == record.sequence;
        if record.consumed && !waiting && !pending && !completed {
            // Only this owner's successful return can consume its completion.
            return Ok(true);
        }
        if !waiting && !pending && !completed {
            return Err(self.root_plan_fault(ports.io, "root refusal lost its exact author ticket"));
        }
        let sequence = self
            .resume_claimed_author_and_wait(ports, Some(claim))
            .map_err(|failure| {
                self.root_plan_work_mut()
                    .unwrap()
                    .refusal_record
                    .as_mut()
                    .unwrap()
                    .returned
                    .retain(failure)
            })?;
        if self.root_plan_work().unwrap().refusal_sequence() != Some(sequence) {
            return Err(self.root_plan_fault(
                ports.io,
                "root refusal completion differs from its Recorded ticket",
            ));
        }
        self.root_plan_work_mut()
            .unwrap()
            .refusal_record
            .as_mut()
            .unwrap()
            .consumed = true;
        Ok(true)
    }

    pub(in crate::flow::context) fn take_root_plan_update(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<Option<RootPlanCompletion>, RunFailure> {
        self.live(io)?;
        let Some(work) = self.root_plan_work() else {
            return Ok(None);
        };
        if !work.settled {
            return Ok(None);
        }
        if work.stage != Stage::Complete || self.deadlines.get(PumpDeadline::Decision).is_some() {
            return Err(self.root_plan_fault(io, "root terminal account has another Decision"));
        }
        let Some(MarkOperation::Plan(work)) = self.marks.take() else {
            unreachable!("checked root owner")
        };
        let account = RootPlanAccount { work };
        if account.work.cancelled {
            return Ok(Some(RootPlanCompletion::Cancelled(account)));
        }
        if let Some(refusal) = account.work.refusal() {
            let refusal = wire_refusal(refusal);
            return Ok(Some(RootPlanCompletion::Refused { refusal, account }));
        }
        Ok(Some(RootPlanCompletion::Updated(RootPlanUpdated {
            account,
        })))
    }
}
