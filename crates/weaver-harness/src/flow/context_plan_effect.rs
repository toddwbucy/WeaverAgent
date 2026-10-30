//! The allocator retains a plan effect through its admitted filing, mandatory
//! summary refresh, native-scope adoption and any template-refusal records.
use super::*;
use crate::flow::author::AuthoredSource;
use crate::flow::context::receipt_driver::FilingOwner;
use crate::flow::place::PlaceError;
use crate::flow::plan::ports::effect::PreparedPortEffect;
use crate::flow::templates::{
    ExpansionIdentity, TemplateCertificate, TemplateExpansion, expand_activate, expand_break,
    expand_step_closed,
};

pub(super) struct PlanEffectWork {
    pub(super) prepared: PreparedPortEffect,
    expansion: Option<TemplateExpansion>,
    event: Option<AuthoredEvent>,
    pub(super) filing: Option<FilingCompletion>,
    receipt: Option<Arc<()>>,
    pub(super) summary: Option<PlanSummaryCompletion>,
    native: Option<NativeMarkCompletion>,
    cancelled_native: Option<PlaceAccount>,
    fault: Option<Box<serde_json::value::RawValue>>,
    fault_record: Option<PlanPortRecord>,
    pub(super) active: Option<s::PlanStep>,
}
impl PlanEffectWork {
    pub(super) fn new(prepared: PreparedPortEffect) -> Self {
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
    pub(in crate::flow::context) fn test_plan_effect(&self) -> Option<&s::PlanEffect> {
        self.plan_port_work()?
            .effect
            .as_ref()
            .map(|work| work.prepared.effect())
    }
    pub(super) fn plan_effect(&self) -> &PlanEffectWork {
        self.plan_port_work().unwrap().effect.as_ref().unwrap()
    }
    pub(super) fn plan_effect_mut(&mut self) -> &mut PlanEffectWork {
        self.plan_port_work_mut().unwrap().effect.as_mut().unwrap()
    }
    pub(in crate::flow::context) fn plan_port_filing_allowed(&self, key: PumpDeadline) -> bool {
        self.plan_port_work().is_some_and(|work| {
            work.driving
                && work.stage == Stage::EffectFiling
                && work.effect.is_some()
                && key == PumpDeadline::Decision
                && self.deadlines.get(key) == Some(work.bound)
        })
    }
    pub(in crate::flow::context) fn plan_port_summary_allowed(&self, key: PumpDeadline) -> bool {
        self.plan_port_work().is_some_and(|work| {
            work.driving
                && matches!(work.stage, Stage::EffectFiled | Stage::Summary)
                && work.effect.is_some()
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
                && work.effect.as_ref().is_some_and(|activation| {
                    activation.receipt.as_ref().is_some_and(|receipt| {
                        Arc::ptr_eq(receipt, proof.filing().receipt_identity())
                    })
                })
        })
    }

    pub(in crate::flow) fn begin_plan_effect(
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
                self.plan_port_fault(io, "plan effect lost preparation or its original wait")
            );
        }
        if work.cancelled {
            self.plan_port_work_mut().unwrap().stage = Stage::Complete;
            return Ok(());
        }
        if !self.repair_certificate_matches(certificate) {
            return Err(self.plan_port_fault(io, "plan effect has a foreign template certificate"));
        }
        let template_id = self.plan_effect().prepared.template_id();
        let expand = match self.plan_effect().prepared.effect() {
            s::PlanEffect::Activate { .. } => expand_activate,
            s::PlanEffect::Break { .. } => expand_break,
            s::PlanEffect::Close { .. } => expand_step_closed,
            _ => unreachable!("private plan effect"),
        };
        match expand(
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
                    .plan_effect()
                    .prepared
                    .event(self, &expansion)
                    .map_err(|detail| self.plan_port_fault(io, detail))?;
                let work = self.plan_effect_mut();
                work.expansion = Some(expansion);
                work.event = Some(event);
                self.plan_port_work_mut().unwrap().stage = Stage::EffectFiling;
            }
            Err(reason) => {
                self.plan_effect_mut().fault = Some(serde_json::value::to_raw_value(
                    &serde_json::json!({"template_id":template_id, "reason":format!("{reason:?}")}))
                    .expect("template fault serializes"));
                self.plan_port_work_mut().unwrap().stage = Stage::TemplateFault;
            }
        }
        Ok(())
    }

    pub(super) fn advance_plan_effect_filing<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<PlanPortProgress, RunFailure> {
        if self.plan_port_work().unwrap().cancelled
            && let Some(event) = self.withdraw_unstarted_owned(FilingOwner::PlanPort)
        {
            self.plan_effect_mut().event = Some(event);
            self.plan_port_work_mut().unwrap().stage = Stage::Complete;
            return Ok(PlanPortProgress::Complete);
        }
        if self.filing.is_none() {
            if self.plan_port_work().unwrap().cancelled {
                self.plan_port_work_mut().unwrap().stage = Stage::Complete;
                return Ok(PlanPortProgress::Complete);
            }
            let event = self
                .plan_effect_mut()
                .event
                .take()
                .expect("unadmitted plan effect source");
            if let Err(error) = self.begin_owned_filing(
                event,
                PumpDeadline::Decision,
                FilingOwner::PlanPort,
                ports.io,
            ) {
                self.plan_effect_mut().event = Some(error.event);
                if let Some(failure) = error.failure {
                    return Err(failure);
                }
                if error.refusal == FilingBeginRefusal::Busy {
                    return Ok(PlanPortProgress::Pending);
                }
                return Err(self.plan_port_fault(ports.io, "plan effect lost filing admission"));
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
        self.plan_effect_mut().receipt = Some(Arc::clone(filing.receipt_identity()));
        self.plan_effect_mut().filing = Some(filing);
        let work = self.plan_effect();
        let rejected = work
            .prepared
            .check_filing(
                self,
                work.expansion.as_ref().unwrap(),
                work.filing.as_ref().unwrap(),
            )
            .map_err(|detail| self.plan_port_fault(ports.io, detail))?;
        if rejected {
            let work = self.plan_effect_mut();
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
                "template_id":work.prepared.template_id(), "source_id":source.source(), "result":source.reply(),
            })).expect("template fault serializes"));
            self.plan_port_work_mut().unwrap().stage = Stage::TemplateFault;
        } else {
            self.plan_port_work_mut().unwrap().stage = Stage::EffectFiled;
        }
        Ok(PlanPortProgress::Progress)
    }

    pub(super) fn advance_plan_effect_summary<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<PlanPortProgress, RunFailure> {
        if self.plan_port_work().unwrap().stage == Stage::EffectFiled {
            let filing = self.plan_effect_mut().filing.take().unwrap();
            if let Err(error) = self.begin_effect_plan_summary(filing, ports.io) {
                self.plan_effect_mut().filing = Some(error.prerequisite);
                if let Some(failure) = error.failure {
                    return Err(failure);
                }
                if error.refusal == PlanSummaryBeginRefusal::Busy {
                    return Ok(PlanPortProgress::Pending);
                }
                return Err(
                    self.plan_port_fault(ports.io, "plan effect lost its effect refresh duty")
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
        self.plan_effect_mut().summary = Some(summary);
        if matches!(
            self.plan_effect().summary,
            Some(PlanSummaryCompletion::Cancelled(_))
        ) {
            let work = self.plan_port_work_mut().unwrap();
            work.cancelled = true;
            work.stage = Stage::Complete;
            return Ok(PlanPortProgress::Complete);
        }
        let Some(PlanSummaryCompletion::Effect(proof)) = &self.plan_effect().summary else {
            return Err(self.plan_port_fault(ports.io, "plan effect has no committed summary"));
        };
        if matches!(
            self.plan_effect().prepared.effect(),
            s::PlanEffect::Break { .. }
        ) {
            return self.finish_break_summary(ports.io);
        }
        if matches!(
            self.plan_effect().prepared.effect(),
            s::PlanEffect::Close { .. }
        ) {
            return self.finish_close_summary(ports.io);
        }
        let s::PlanEffect::Activate {
            expected,
            step_id,
            step_span,
            group_span,
            ..
        } = self.plan_effect().prepared.effect()
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
        self.plan_effect_mut().active = Some(active);
        let current = Arc::clone(self.plan_cache.committed.as_ref().unwrap());
        let work = self.plan_port_work_mut().unwrap();
        work.current = Some(current);
        // Cancellation revokes the caller's grant, not the committed effect.
        // Adopt its native scopes through the exact receipt before releasing
        // the allocator. Otherwise later work sees a permanently stale engine.
        work.stage = Stage::Native;
        Ok(PlanPortProgress::Progress)
    }

    /// The refreshed revision must hold this exact closure: the step closed
    /// with this outcome span, WorkRef and null review, and this source as its
    /// closure and the plan's last source. Ancestor closure is the store's.
    fn finish_close_summary(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<PlanPortProgress, RunFailure> {
        let Some(PlanSummaryCompletion::Effect(proof)) = &self.plan_effect().summary else {
            unreachable!("checked effect summary")
        };
        let s::PlanEffect::Close {
            expected,
            step_id,
            review_call,
            work,
            outcome_span,
        } = self.plan_effect().prepared.effect()
        else {
            unreachable!("private close continuation")
        };
        let checked = matches!(proof.read().result(),
            s::QueryResult::PlanCurrent { current: Some(snapshot), .. }
                if &snapshot.plan == expected
                    && matches!(snapshot.status, s::PlanStatus::Locked | s::PlanStatus::Complete)
                    && (snapshot.status == s::PlanStatus::Complete)
                        == snapshot.steps.iter().all(|step| step.status == s::StepStatus::Closed)
                    && snapshot.steps.iter().any(|step| &step.step_id == step_id
                        && step.status == s::StepStatus::Closed
                        && step.outcome_span.as_ref() == Some(outcome_span)
                        && step.review_call.as_ref() == review_call.as_ref()
                        && step.work.as_ref() == Some(work)
                        && step.closure_source.as_ref() == Some(&snapshot.last_source.block_id)));
        if !checked {
            return Err(self.plan_port_fault(io, "close refresh changed its committed closure"));
        }
        let current = Arc::clone(self.plan_cache.committed.as_ref().unwrap());
        let work = self.plan_port_work_mut().unwrap();
        work.current = Some(current);
        // As for activation, the committed closing scopes are adopted before
        // the allocator is released, even if the caller has cancelled.
        work.stage = Stage::Native;
        Ok(PlanPortProgress::Progress)
    }

    pub(super) fn advance_plan_effect_native<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<PlanPortProgress, RunFailure> {
        if self.native_marks.is_none() {
            let Some(PlanSummaryCompletion::Effect(proof)) = self.plan_effect_mut().summary.take()
            else {
                unreachable!("checked plan effect summary")
            };
            if let Err(error) = self.begin_place_effect_marks(proof, ports.io) {
                self.plan_effect_mut().summary =
                    Some(PlanSummaryCompletion::Effect(error.prerequisite));
                if let Some(failure) = error.failure {
                    return Err(failure);
                }
                if matches!(error.refusal, PlaceError::Busy(_)) {
                    return Ok(PlanPortProgress::Pending);
                }
                return Err(
                    self.plan_port_fault(ports.io, "plan effect lost native scope adoption")
                );
            }
        }
        if self.plan_port_work().unwrap().cancelled {
            self.cancel_place_marks()?;
        }
        let progress = self.advance_place_marks(ports)?;
        if let Some(done) = self.take_place_marks(ports.io)? {
            self.plan_effect_mut().native = Some(done);
        } else if let Some(done) = self.take_cancelled_place_marks(ports.io)? {
            self.plan_effect_mut().cancelled_native = Some(done);
            self.plan_port_work_mut().unwrap().cancelled = true;
        } else {
            return Ok(if progress == PlaceProgress::Pending {
                PlanPortProgress::Pending
            } else {
                PlanPortProgress::Progress
            });
        }
        if matches!(
            self.plan_effect().prepared.effect(),
            s::PlanEffect::Break { .. }
        ) {
            self.plan_port_work_mut().unwrap().stage = Stage::BreakReport;
            Ok(PlanPortProgress::Progress)
        } else {
            self.plan_port_work_mut().unwrap().stage = Stage::Complete;
            Ok(PlanPortProgress::Complete)
        }
    }

    pub(super) fn advance_plan_effect_fault<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<PlanPortProgress, RunFailure> {
        if self.plan_port_work().unwrap().record.is_none() {
            let work = self.plan_port_work().unwrap();
            let fault = weaver_types::FaultReport {
                case: weaver_types::FaultCase::StateTemplateRejected,
                account: self.plan_effect().fault.as_ref().unwrap().clone(),
            };
            let event = AuthoredEvent {
                kind: Kind::Fault,
                subsystem: weaver_trace::Subsystem::Harness,
                turn: work.turn.clone(),
                source: AuthoredSource::Runtime,
                causal_parent: self
                    .plan_effect()
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
        work.effect.as_mut().unwrap().fault_record = Some(record);
        work.refusal = Some(s::PlanOperationRefused {
            case: s::PlanOperationCase::TemplateRejected,
            detail: "certified plan effect template was rejected".into(),
            read_code: None,
        });
        work.stage = Stage::Refusal;
        Ok(PlanPortProgress::Progress)
    }
}
