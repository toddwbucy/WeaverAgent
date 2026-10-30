//! A private operation claim keeps refusal attribution and the consumed proof's
//! parent through recorder pressure, cancellation and trace-only failure drain.
use super::*;
use crate::flow::author::AuthoredSource;
use weaver_types::RefusalRecord;

pub(super) struct PlanPortRecord {
    identity: Arc<()>,
    original: Option<AuthoredEvent>,
    ticket: Option<u64>,
    pub(super) sequence: Option<Sequence>,
    consumed: bool,
    returned: StoppedAuthor,
}

#[cfg(test)]
impl PlanPortTransaction {
    pub(in crate::flow::context) fn test_request(&self) -> &PlanRequest {
        &self.request
    }
    pub(in crate::flow::context) fn test_cancelled(&self) -> bool {
        self.cancelled
    }
    pub(in crate::flow::context) fn test_stage(&self) -> String {
        format!(
            "{:?}, request {:?}, cancelled {}",
            self.stage,
            self.request.operation(),
            self.cancelled
        )
    }
    pub(in crate::flow::context) fn test_refusal_account(
        &self,
    ) -> (&PlanRequest, Option<u64>, Option<Sequence>, bool) {
        let record = self.record.as_ref().expect("chosen refusal record");
        (
            &self.request,
            record.ticket,
            record.sequence,
            record.consumed,
        )
    }
}

impl RecordingContext {
    pub(in crate::flow::context) fn plan_port_author_claim(&self) -> Option<AuthorClaim> {
        let record = self.plan_port_work()?.record.as_ref()?;
        (record.original.is_none() && !record.consumed)
            .then(|| AuthorClaim::PlanPort(Arc::clone(&record.identity)))
    }

    pub(in crate::flow::context) fn claim_plan_port_author_ticket(
        &mut self,
        identity: &Arc<()>,
        ticket: u64,
    ) -> bool {
        let Some(work) = self.plan_port_work_mut() else {
            return false;
        };
        let Some(record) = work.record.as_mut() else {
            return false;
        };
        if !work.driving
            || !matches!(
                work.stage,
                Stage::Refusal | Stage::TemplateFault | Stage::BreakReport
            )
            || !Arc::ptr_eq(&record.identity, identity)
            || record.ticket.is_some()
        {
            return false;
        }
        record.ticket = Some(ticket);
        true
    }

    pub(in crate::flow::context) fn bind_plan_port_author(&mut self, sequence: Sequence) {
        let ticket = self.barrier.recorded_author_ticket(sequence);
        if let Some(record) = self
            .plan_port_work_mut()
            .and_then(|work| work.record.as_mut())
            && record.ticket.is_some()
            && record.ticket == ticket
            && record.sequence.is_none()
        {
            record.sequence = Some(sequence);
        }
    }

    pub(super) fn record_plan_port_refusal<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<bool, RunFailure> {
        let work = self.plan_port_work().unwrap();
        if work.record.is_none() {
            let refusal = work.refusal.as_ref().ok_or(RunFailure::MarksOwned)?;
            let event = AuthoredEvent {
                kind: Kind::Refusal,
                subsystem: weaver_trace::Subsystem::Harness,
                turn: work.turn.clone(),
                source: AuthoredSource::Runtime,
                causal_parent: work.parent,
                payload: Some(Payload::Refusal(
                    serde_json::value::to_raw_value(&RefusalRecord::PlanOperation {
                        operation: work
                            .request
                            .operation()
                            .expect("operation refusal, not STEP-QUERY"),
                        expected: work.request.expected().clone(),
                        step_id: work.request.step_id().cloned(),
                        refusal: refusal.clone(),
                    })
                    .expect("typed plan operation refusal serializes"),
                )),
            };
            self.start_plan_port_record(event);
        }
        self.advance_plan_port_record(ports)
    }

    pub(super) fn start_plan_port_record(&mut self, event: AuthoredEvent) {
        assert!(self.plan_port_work().unwrap().record.is_none());
        self.plan_port_work_mut().unwrap().record = Some(PlanPortRecord {
            identity: Arc::new(()),
            original: Some(event),
            ticket: None,
            sequence: None,
            consumed: false,
            returned: StoppedAuthor::default(),
        });
    }

    pub(super) fn advance_plan_port_record<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<bool, RunFailure> {
        let record = self.plan_port_work().unwrap().record.as_ref().unwrap();
        record.returned.check()?;
        let claim = AuthorClaim::PlanPort(Arc::clone(&record.identity));
        if record.original.is_some() {
            if !self.serving_observation_author_available() {
                return Ok(false);
            }
            let event = self
                .plan_port_work_mut()
                .unwrap()
                .record
                .as_mut()
                .unwrap()
                .original
                .take()
                .unwrap();
            self.waiting_content = Some(WaitingAuthor::claimed(event, claim.clone()));
        }
        let record = self.plan_port_work().unwrap().record.as_ref().unwrap();
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
            return Ok(true);
        }
        if !waiting && !pending && !completed {
            return Err(self.plan_port_fault(ports.io, "plan refusal lost its exact author ticket"));
        }
        let sequence = self
            .resume_claimed_author_and_wait(ports, Some(claim))
            .map_err(|error| {
                self.plan_port_work_mut()
                    .unwrap()
                    .record
                    .as_mut()
                    .unwrap()
                    .returned
                    .retain(error)
            })?;
        let record = self.plan_port_work_mut().unwrap().record.as_mut().unwrap();
        if record.sequence != Some(sequence) {
            return Err(self.plan_port_fault(
                ports.io,
                "plan refusal consumed another Recorded completion",
            ));
        }
        record.consumed = true;
        Ok(true)
    }
}
