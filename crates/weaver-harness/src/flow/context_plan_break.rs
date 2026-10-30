//! A break report follows the retained effect receipt and mandatory summary.
//! With no locked plan, the same author owner records only the report.
use super::*;
use crate::flow::author::AuthoredSource;

impl RecordingContext {
    pub(super) fn prepare_plan_break(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<PlanPortProgress, RunFailure> {
        let work = self.plan_port_work().unwrap();
        let prepared = checked::break_plan::prepare(
            &work.request,
            self,
            work.current.as_ref().unwrap(),
            work.turn.clone(),
            work.bound,
        );
        let prepared = match prepared {
            Ok(prepared) => prepared,
            Err(failure) => {
                self.retain_plan_check_failure(failure, io)?;
                return Ok(PlanPortProgress::Progress);
            }
        };
        let work = self.plan_port_work_mut().unwrap();
        let PlanRequest::RecordBreak {
            expected,
            step_id,
            case,
            proposed_source,
        } = &work.request
        else {
            unreachable!()
        };
        work.break_report = Some(s::PlanBreakReport {
            plan: prepared.as_ref().map(|_| expected.clone()),
            step_id: step_id.clone(),
            proposed_source: proposed_source.clone(),
            case: *case,
        });
        work.effect = prepared.map(PlanEffectWork::new);
        work.stage = if work.effect.is_some() {
            Stage::Effect
        } else {
            Stage::BreakReport
        };
        Ok(PlanPortProgress::Progress)
    }

    pub(super) fn finish_break_summary(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<PlanPortProgress, RunFailure> {
        let Some(PlanSummaryCompletion::Effect(proof)) = &self.plan_effect().summary else {
            unreachable!("checked effect summary")
        };
        let s::PlanEffect::Break {
            expected,
            step_id,
            case,
            proposed_source,
        } = self.plan_effect().prepared.effect()
        else {
            unreachable!("private break continuation")
        };
        let checked = matches!(proof.read().result(),
            s::QueryResult::PlanCurrent { current: Some(snapshot), .. }
                if &snapshot.plan == expected && snapshot.status == s::PlanStatus::Broken
                    && snapshot.broken.as_ref().is_some_and(|broken|
                        &broken.step_id == step_id && &broken.case == case
                            && &broken.proposed_source == proposed_source
                            && broken.source == snapshot.last_source.block_id));
        if !checked {
            return Err(self.plan_port_fault(io, "break refresh changed its committed break"));
        }
        let parent = proof.filing().sequence();
        let current = Arc::clone(self.plan_cache.committed.as_ref().unwrap());
        let work = self.plan_port_work_mut().unwrap();
        work.parent = Some(parent);
        work.current = Some(current);
        // Broken keeps the open marks, but the native cache must still adopt
        // the exact new plan summary before the caller can use it again.
        work.stage = Stage::Native;
        Ok(PlanPortProgress::Progress)
    }

    pub(super) fn advance_plan_break_report<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<PlanPortProgress, RunFailure> {
        let work = self.plan_port_work().unwrap();
        if work.record.is_none() {
            let event = AuthoredEvent {
                kind: Kind::PlanBreak,
                subsystem: weaver_trace::Subsystem::Harness,
                turn: work.turn.clone(),
                source: AuthoredSource::Loop,
                causal_parent: work.parent,
                payload: Some(Payload::PlanBreak(Box::new(
                    crate::flow::mirror::plan_break(work.break_report.as_ref().unwrap()),
                ))),
            };
            self.start_plan_port_record(event);
        }
        if !self.advance_plan_port_record(ports)? {
            return Ok(PlanPortProgress::Pending);
        }
        self.live(ports.io)?;
        self.plan_port_work_mut().unwrap().stage = Stage::Complete;
        Ok(PlanPortProgress::Complete)
    }
}
