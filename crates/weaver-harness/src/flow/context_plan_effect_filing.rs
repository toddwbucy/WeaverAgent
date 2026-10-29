//! The retained root grant uses the ordinary author/classifier/member pipeline.
//! A receipt still owes the checked summary and operation settlement.
use super::*;
use crate::flow::author::AuthoredSource;
use crate::flow::ledger::SourceDisposition;
use crate::flow::templates::{ExpansionIdentity, TemplateCertificate};

impl RecordingContext {
    pub(in crate::flow::context) fn advance_root_plan_summary<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<RootPlanProgress, RunFailure> {
        if self.root_plan_work().unwrap().stage == Stage::EffectFiled {
            let filing = self
                .root_plan_work_mut()
                .unwrap()
                .effect_filing
                .take()
                .expect("retained root commit");
            if let Err(error) = self.begin_effect_plan_summary(filing, ports.io) {
                self.root_plan_work_mut().unwrap().effect_filing = Some(error.prerequisite);
                if let Some(failure) = error.failure {
                    return Err(failure);
                }
                if error.refusal == PlanSummaryBeginRefusal::Busy {
                    self.root_plan_work_mut().unwrap().wait_ready = true;
                    return Ok(RootPlanProgress::Pending);
                }
                return Err(
                    self.root_plan_fault(ports.io, "root summary lost its exact committed effect")
                );
            }
            self.root_plan_work_mut().unwrap().stage = Stage::Summary;
        }
        let progress = self.advance_plan_summary(ports)?;
        self.root_plan_work_mut().unwrap().wait_ready = progress == PlanSummaryProgress::Pending;
        let Some(summary) = self.take_plan_summary(ports.io)? else {
            return Ok(RootPlanProgress::Pending);
        };
        self.root_plan_work_mut().unwrap().summary = Some(summary);
        let work = self.root_plan_work_mut().unwrap();
        match work.summary.as_ref().unwrap() {
            PlanSummaryCompletion::Effect(_) => {
                work.stage = Stage::SummaryReady;
                Ok(if work.cancelled {
                    RootPlanProgress::Cancelled
                } else {
                    RootPlanProgress::SummaryReady
                })
            }
            PlanSummaryCompletion::Cancelled(_) => {
                work.cancelled = true;
                work.stage = Stage::Cancelled;
                Ok(RootPlanProgress::Cancelled)
            }
            PlanSummaryCompletion::Boot(_) | PlanSummaryCompletion::Refused(_) => {
                Err(self
                    .root_plan_fault(ports.io, "root summary has no valid committed disposition"))
            }
        }
    }

    pub(in crate::flow::context) fn begin_root_plan_effect(
        &mut self,
        certificate: &TemplateCertificate,
        io: &mut impl PumpIo,
    ) -> Result<RootPlanProgress, RunFailure> {
        self.live(io)?;
        let Some(work) = self.root_plan_work() else {
            return Err(RunFailure::MarksOwned);
        };
        if work.stage != Stage::Effect
            || self.deadlines.get(PumpDeadline::Decision) != Some(work.bound)
        {
            return Err(
                self.root_plan_fault(io, "root effect lost its preparation or original wait")
            );
        }
        if work.cancelled {
            self.root_plan_work_mut().unwrap().stage = Stage::Cancelled;
            return Ok(RootPlanProgress::Cancelled);
        }
        if !self.repair_certificate_matches(certificate) {
            return Err(self.root_plan_fault(io, "root effect has a foreign template certificate"));
        }
        let family = self
            .root_plan_work()
            .unwrap()
            .effect
            .as_ref()
            .unwrap()
            .template()
            .map_err(|detail| self.root_plan_fault(io, detail))?;
        let expansion = family.expand(
            certificate,
            ExpansionIdentity {
                run_id: self.flow.run(),
                scheme: &self.declaration.scheme,
                checker_artifact_sha256: certificate.checker_artifact_sha256(),
                inventory_sha256: certificate.inventory_sha256(),
            },
        );
        match expansion {
            Ok(expansion) => {
                let event = self
                    .root_plan_work()
                    .unwrap()
                    .effect
                    .as_ref()
                    .unwrap()
                    .event(self, &expansion)
                    .map_err(|detail| self.root_plan_fault(io, detail))?;
                let work = self.root_plan_work_mut().unwrap();
                work.expansion = Some(expansion);
                work.effect_event = Some(event);
                work.stage = Stage::EffectFiling;
            }
            Err(reason) => {
                let work = self.root_plan_work_mut().unwrap();
                work.fault_account = Some(
                    serde_json::value::to_raw_value(&serde_json::json!({
                        "template_id": family.id(), "reason": format!("{reason:?}")
                    }))
                    .expect("template defect serializes"),
                );
                work.stage = Stage::TemplateFault;
            }
        }
        Ok(RootPlanProgress::Pending)
    }

    pub(in crate::flow::context) fn advance_root_plan_effect_filing<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<RootPlanProgress, RunFailure> {
        if self.root_plan_work().unwrap().cancelled
            && let Some(event) = self.withdraw_unstarted_root_effect()
        {
            let work = self.root_plan_work_mut().unwrap();
            work.effect_event = Some(event);
            work.stage = Stage::Cancelled;
            return Ok(RootPlanProgress::Cancelled);
        }
        if self.filing.is_none() {
            if self.root_plan_work().unwrap().cancelled {
                self.root_plan_work_mut().unwrap().stage = Stage::Cancelled;
                return Ok(RootPlanProgress::Cancelled);
            }
            let event = self
                .root_plan_work_mut()
                .unwrap()
                .effect_event
                .take()
                .expect("root effect retains its unadmitted original");
            if let Err(error) = self.begin_owned_filing(
                event,
                PumpDeadline::Decision,
                FilingOwner::RootPlan,
                ports.io,
            ) {
                self.root_plan_work_mut().unwrap().effect_event = Some(error.event);
                if let Some(failure) = error.failure {
                    return Err(failure);
                }
                if error.refusal == FilingBeginRefusal::Busy {
                    self.root_plan_work_mut().unwrap().wait_ready = true;
                    return Ok(RootPlanProgress::Pending);
                }
                return Err(self.root_plan_fault(ports.io, "root effect filing lost its admission"));
            }
        }
        // An admitted effect cannot be forgotten at cancellation. Its actual
        // disposition and any committed refresh duty remain with this owner.
        let progress = self.advance_owned_filing(FilingOwner::RootPlan, ports)?;
        self.root_plan_work_mut().unwrap().wait_ready = progress == FilingProgress::Pending;
        let Some(filing) = self.take_owned_filing(FilingOwner::RootPlan, ports.io)? else {
            return Ok(RootPlanProgress::Pending);
        };
        self.root_plan_work_mut().unwrap().effect_filing = Some(filing);
        let work = self.root_plan_work().unwrap();
        let rejected = work
            .effect
            .as_ref()
            .unwrap()
            .check_filing(
                self,
                work.expansion.as_ref().unwrap(),
                work.effect_filing.as_ref().unwrap(),
                work.bound,
            )
            .map_err(|detail| self.root_plan_fault(ports.io, detail))?;
        if rejected {
            let work = self.root_plan_work_mut().unwrap();
            let SourceDisposition::CorrectlyRejected(source) = work
                .effect_filing
                .as_ref()
                .unwrap()
                .receipts()
                .dispositions()
                .next()
                .unwrap()
            else {
                unreachable!("checked root rejection")
            };
            work.fault_account = Some(
                serde_json::value::to_raw_value(&serde_json::json!({
                    "template_id": work.expansion.as_ref().unwrap().template_id(),
                    "source_id": source.source(), "result": source.reply(),
                }))
                .expect("template defect serializes"),
            );
            work.stage = Stage::TemplateFault;
            Ok(RootPlanProgress::Pending)
        } else {
            self.root_plan_work_mut().unwrap().stage = Stage::EffectFiled;
            Ok(RootPlanProgress::EffectFiled)
        }
    }

    pub(in crate::flow::context) fn advance_root_plan_template_fault<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<RootPlanProgress, RunFailure> {
        let work = self.root_plan_work().unwrap();
        work.returned_author.check()?;
        if !work.fault_started {
            let fault = weaver_types::FaultReport {
                case: weaver_types::FaultCase::StateTemplateRejected,
                account: work.fault_account.as_ref().unwrap().clone(),
            };
            let event = AuthoredEvent {
                kind: Kind::Fault,
                subsystem: weaver_trace::Subsystem::Harness,
                turn: None,
                source: AuthoredSource::Runtime,
                causal_parent: Some(work.effect_filing.as_ref().map_or_else(
                    || work.input_filing.as_ref().unwrap().sequence(),
                    FilingCompletion::sequence,
                )),
                payload: Some(Payload::Fault(
                    serde_json::value::to_raw_value(&fault).expect("fault serializes"),
                )),
            };
            self.root_plan_work_mut().unwrap().fault_started = true;
            let sequence = self.author_and_wait(ports, event).map_err(|failure| {
                let work = self.root_plan_work_mut().unwrap();
                work.returned_author
                    .retain_started(failure, &mut work.fault_started)
            })?;
            self.root_plan_work_mut().unwrap().fault_sequence = Some(sequence);
        } else if self.completed_author.is_some()
            || self.waiting_content.is_some()
            || self.barrier.pending_author().is_some()
        {
            let sequence = self.resume_author_and_wait(ports).map_err(|failure| {
                let work = self.root_plan_work_mut().unwrap();
                work.returned_author
                    .retain_started(failure, &mut work.fault_started)
            })?;
            self.root_plan_work_mut().unwrap().fault_sequence = Some(sequence);
        }
        let work = self.root_plan_work_mut().unwrap();
        assert!(
            work.fault_sequence.is_some(),
            "Recorded template fault retained"
        );
        work.refusal = Some(PlanUpdateRefused {
            case: weaver_types::state_flow::PlanUpdateCase::TemplateRejected,
            detail: "certified plan template was rejected".into(),
            rejected_blocks: vec![],
            read_code: None,
        });
        work.stage = Stage::Refusal;
        Ok(RootPlanProgress::RefusalReady)
    }
}
