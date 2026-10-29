//! The allocator retains activation through its admitted effect, mandatory
//! summary refresh, native-scope adoption and any template-refusal records.
use super::*;
use crate::flow::author::AuthoredSource;
use crate::flow::context::receipt_driver::FilingOwner;
use crate::flow::place::PlaceError;
use crate::flow::plan::ports::activation::PreparedActivation;
use crate::flow::templates::{
    ExpansionIdentity, TemplateCertificate, TemplateExpansion, expand_activate,
};

pub(super) struct ActivationWork {
    prepared: PreparedActivation,
    expansion: Option<TemplateExpansion>,
    event: Option<AuthoredEvent>,
    filing: Option<FilingCompletion>,
    receipt: Option<Arc<()>>,
    summary: Option<PlanSummaryCompletion>,
    native: Option<NativeMarkCompletion>,
    cancelled_native: Option<PlaceAccount>,
    fault: Option<Box<serde_json::value::RawValue>>,
    fault_record: Option<PlanPortRecord>,
    pub(super) active: Option<s::PlanStep>,
}
impl ActivationWork {
    pub(super) fn new(prepared: PreparedActivation) -> Self {
        Self {
            prepared,
            expansion: None,
            event: None,
            filing: None,
            receipt: None,
            summary: None,
            native: None,
            cancelled_native: None,
            fault: None,
            fault_record: None,
            active: None,
        }
    }
}

impl RecordingContext {
    #[cfg(test)]
    pub(in crate::flow::context) fn test_activation_effect(&self) -> Option<&s::PlanEffect> {
        self.plan_port_work()?
            .activation
            .as_ref()
            .map(|work| work.prepared.effect())
    }
    fn activation(&self) -> &ActivationWork {
        self.plan_port_work().unwrap().activation.as_ref().unwrap()
    }
    fn activation_mut(&mut self) -> &mut ActivationWork {
        self.plan_port_work_mut()
            .unwrap()
            .activation
            .as_mut()
            .unwrap()
    }
    pub(in crate::flow::context) fn plan_port_filing_allowed(&self, key: PumpDeadline) -> bool {
        self.plan_port_work().is_some_and(|work| {
            work.driving
                && work.stage == Stage::EffectFiling
                && work.activation.is_some()
                && key == PumpDeadline::Decision
                && self.deadlines.get(key) == Some(work.bound)
        })
    }
    pub(in crate::flow::context) fn plan_port_summary_allowed(&self, key: PumpDeadline) -> bool {
        self.plan_port_work().is_some_and(|work| {
            work.driving
                && matches!(work.stage, Stage::EffectFiled | Stage::Summary)
                && work.activation.is_some()
                && key == PumpDeadline::Decision
                && self.deadlines.get(key) == Some(work.bound)
        })
    }

    pub(in crate::flow::context) fn plan_port_native_allowed(
        &self,
        proof: &EffectPlanSummary,
    ) -> bool {
        self.plan_port_work().is_some_and(|work| {
            work.driving
                && work.stage == Stage::Native
                && proof.filing().wait_key() == PumpDeadline::Decision
                && self.deadlines.get(PumpDeadline::Decision) == Some(work.bound)
                && proof.filing().wait_bound() == work.bound
                && work.activation.as_ref().is_some_and(|activation| {
                    activation.receipt.as_ref().is_some_and(|receipt| {
                        Arc::ptr_eq(receipt, proof.filing().receipt_identity())
                    })
                })
        })
    }

    pub(in crate::flow) fn begin_activation_effect(
        &mut self,
        certificate: &TemplateCertificate,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        self.live(io)?;
        let work = self.plan_port_work().ok_or(RunFailure::MarksOwned)?;
        if work.stage != Stage::Effect
            || self.deadlines.get(PumpDeadline::Decision) != Some(work.bound)
        {
            return Err(
                self.plan_port_fault(io, "activation lost preparation or its original wait")
            );
        }
        if work.cancelled {
            self.plan_port_work_mut().unwrap().stage = Stage::Complete;
            return Ok(());
        }
        if !self.repair_certificate_matches(certificate) {
            return Err(self.plan_port_fault(io, "activation has a foreign template certificate"));
        }
        match expand_activate(
            certificate,
            ExpansionIdentity {
                run_id: self.flow.run(),
                scheme: &self.declaration.scheme,
                checker_artifact_sha256: certificate.checker_artifact_sha256(),
                inventory_sha256: certificate.inventory_sha256(),
            },
        ) {
            Ok(expansion) => {
                let event = self
                    .activation()
                    .prepared
                    .event(self, &expansion)
                    .map_err(|detail| self.plan_port_fault(io, detail))?;
                let work = self.activation_mut();
                work.expansion = Some(expansion);
                work.event = Some(event);
                self.plan_port_work_mut().unwrap().stage = Stage::EffectFiling;
            }
            Err(reason) => {
                self.activation_mut().fault = Some(serde_json::value::to_raw_value(
                    &serde_json::json!({"template_id":"plan.activate", "reason":format!("{reason:?}")}))
                    .expect("template fault serializes"));
                self.plan_port_work_mut().unwrap().stage = Stage::TemplateFault;
            }
        }
        Ok(())
    }

    pub(super) fn advance_activation_filing<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<PlanPortProgress, RunFailure> {
        if self.plan_port_work().unwrap().cancelled
            && let Some(event) = self.withdraw_unstarted_owned(FilingOwner::PlanPort)
        {
            self.activation_mut().event = Some(event);
            self.plan_port_work_mut().unwrap().stage = Stage::Complete;
            return Ok(PlanPortProgress::Complete);
        }
        if self.filing.is_none() {
            if self.plan_port_work().unwrap().cancelled {
                self.plan_port_work_mut().unwrap().stage = Stage::Complete;
                return Ok(PlanPortProgress::Complete);
            }
            let event = self
                .activation_mut()
                .event
                .take()
                .expect("unadmitted activation source");
            if let Err(error) = self.begin_owned_filing(
                event,
                PumpDeadline::Decision,
                FilingOwner::PlanPort,
                ports.io,
            ) {
                self.activation_mut().event = Some(error.event);
                if let Some(failure) = error.failure {
                    return Err(failure);
                }
                if error.refusal == FilingBeginRefusal::Busy {
                    return Ok(PlanPortProgress::Pending);
                }
                return Err(self.plan_port_fault(ports.io, "activation lost filing admission"));
            }
        }
        let progress = self.advance_owned_filing(FilingOwner::PlanPort, ports)?;
        let Some(filing) = self.take_owned_filing(FilingOwner::PlanPort, ports.io)? else {
            return Ok(if progress == FilingProgress::Pending {
                PlanPortProgress::Pending
            } else {
                PlanPortProgress::Progress
            });
        };
        self.activation_mut().receipt = Some(Arc::clone(filing.receipt_identity()));
        self.activation_mut().filing = Some(filing);
        let work = self.activation();
        let rejected = work
            .prepared
            .check_filing(
                self,
                work.expansion.as_ref().unwrap(),
                work.filing.as_ref().unwrap(),
            )
            .map_err(|detail| self.plan_port_fault(ports.io, detail))?;
        if rejected {
            let work = self.activation_mut();
            let crate::flow::ledger::SourceDisposition::CorrectlyRejected(source) = work
                .filing
                .as_ref()
                .unwrap()
                .receipts()
                .dispositions()
                .next()
                .unwrap()
            else {
                unreachable!()
            };
            work.fault = Some(serde_json::value::to_raw_value(&serde_json::json!({
                "template_id":"plan.activate", "source_id":source.source(), "result":source.reply(),
            })).expect("template fault serializes"));
            self.plan_port_work_mut().unwrap().stage = Stage::TemplateFault;
        } else {
            self.plan_port_work_mut().unwrap().stage = Stage::EffectFiled;
        }
        Ok(PlanPortProgress::Progress)
    }

    pub(super) fn advance_activation_summary<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<PlanPortProgress, RunFailure> {
        if self.plan_port_work().unwrap().stage == Stage::EffectFiled {
            let filing = self.activation_mut().filing.take().unwrap();
            if let Err(error) = self.begin_effect_plan_summary(filing, ports.io) {
                self.activation_mut().filing = Some(error.prerequisite);
                if let Some(failure) = error.failure {
                    return Err(failure);
                }
                if error.refusal == PlanSummaryBeginRefusal::Busy {
                    return Ok(PlanPortProgress::Pending);
                }
                return Err(
                    self.plan_port_fault(ports.io, "activation lost its effect refresh duty")
                );
            }
            self.plan_port_work_mut().unwrap().stage = Stage::Summary;
        }
        let progress = self.advance_plan_summary(ports)?;
        let Some(summary) = self.take_plan_summary(ports.io)? else {
            return Ok(if progress == PlanSummaryProgress::Pending {
                PlanPortProgress::Pending
            } else {
                PlanPortProgress::Progress
            });
        };
        self.activation_mut().summary = Some(summary);
        if matches!(
            self.activation().summary,
            Some(PlanSummaryCompletion::Cancelled(_))
        ) {
            let work = self.plan_port_work_mut().unwrap();
            work.cancelled = true;
            work.stage = Stage::Complete;
            return Ok(PlanPortProgress::Complete);
        }
        let Some(PlanSummaryCompletion::Effect(proof)) = &self.activation().summary else {
            return Err(self.plan_port_fault(ports.io, "activation has no committed summary"));
        };
        let s::PlanEffect::Activate {
            expected,
            step_id,
            step_span,
            group_span,
            ..
        } = self.activation().prepared.effect()
        else {
            unreachable!()
        };
        let active = match proof.read().result() {
            s::QueryResult::PlanCurrent {
                current: Some(plan),
                ..
            } if &plan.plan == expected && plan.status == s::PlanStatus::Locked => plan
                .steps
                .iter()
                .find(|step| {
                    &step.step_id == step_id
                        && step.status == s::StepStatus::Active
                        && step.span.as_ref() == Some(step_span)
                        && match &step.group {
                            None => group_span.is_none(),
                            Some(id) => plan.groups.iter().any(|group| {
                                &group.id == id && group.span.as_ref() == group_span.as_ref()
                            }),
                        }
                })
                .cloned(),
            _ => None,
        };
        let Some(active) = active else {
            return Err(
                self.plan_port_fault(ports.io, "activation refresh changed its committed step")
            );
        };
        self.activation_mut().active = Some(active);
        let current = Arc::clone(self.plan_cache.committed.as_ref().unwrap());
        let work = self.plan_port_work_mut().unwrap();
        work.current = Some(current);
        // Cancellation revokes the caller's grant, not the committed effect.
        // Adopt its native scopes through the exact receipt before releasing
        // the allocator. Otherwise later work sees a permanently stale engine.
        work.stage = Stage::Native;
        Ok(PlanPortProgress::Progress)
    }

    pub(super) fn advance_activation_native<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<PlanPortProgress, RunFailure> {
        if self.native_marks.is_none() {
            let Some(PlanSummaryCompletion::Effect(proof)) = self.activation_mut().summary.take()
            else {
                unreachable!("checked activation summary")
            };
            if let Err(error) = self.begin_place_effect_marks(proof, ports.io) {
                self.activation_mut().summary =
                    Some(PlanSummaryCompletion::Effect(error.prerequisite));
                if let Some(failure) = error.failure {
                    return Err(failure);
                }
                if matches!(error.refusal, PlaceError::Busy(_)) {
                    return Ok(PlanPortProgress::Pending);
                }
                return Err(self.plan_port_fault(ports.io, "activation lost native scope adoption"));
            }
        }
        if self.plan_port_work().unwrap().cancelled {
            self.cancel_place_marks()?;
        }
        let progress = self.advance_place_marks(ports)?;
        if let Some(done) = self.take_place_marks(ports.io)? {
            self.activation_mut().native = Some(done);
        } else if let Some(done) = self.take_cancelled_place_marks(ports.io)? {
            self.activation_mut().cancelled_native = Some(done);
            self.plan_port_work_mut().unwrap().cancelled = true;
        } else {
            return Ok(if progress == PlaceProgress::Pending {
                PlanPortProgress::Pending
            } else {
                PlanPortProgress::Progress
            });
        }
        self.plan_port_work_mut().unwrap().stage = Stage::Complete;
        Ok(PlanPortProgress::Complete)
    }

    pub(super) fn advance_activation_fault<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<PlanPortProgress, RunFailure> {
        if self.plan_port_work().unwrap().record.is_none() {
            let work = self.plan_port_work().unwrap();
            let fault = weaver_types::FaultReport {
                case: weaver_types::FaultCase::StateTemplateRejected,
                account: self.activation().fault.as_ref().unwrap().clone(),
            };
            let event = AuthoredEvent {
                kind: Kind::Fault,
                subsystem: weaver_trace::Subsystem::Harness,
                turn: work.turn.clone(),
                source: AuthoredSource::Runtime,
                causal_parent: self
                    .activation()
                    .filing
                    .as_ref()
                    .map(FilingCompletion::sequence)
                    .or(work.parent),
                payload: Some(Payload::Fault(
                    serde_json::value::to_raw_value(&fault).expect("template fault serializes"),
                )),
            };
            self.start_plan_port_record(event);
        }
        if !self.advance_plan_port_record(ports)? {
            return Ok(PlanPortProgress::Pending);
        }
        let work = self.plan_port_work_mut().unwrap();
        let record = work.record.take().unwrap();
        work.parent = record.sequence;
        work.activation.as_mut().unwrap().fault_record = Some(record);
        work.refusal = Some(s::PlanOperationRefused {
            case: s::PlanOperationCase::TemplateRejected,
            detail: "certified activation template was rejected".into(),
            read_code: None,
        });
        work.stage = Stage::Refusal;
        Ok(PlanPortProgress::Progress)
    }
}
