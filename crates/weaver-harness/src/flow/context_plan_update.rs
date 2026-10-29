//! Root input retains the one mark allocator through catalog reads and filing.
//! Filed input grants no effect; only a later effect receipt and checked summary
//! can establish PlanUpdated. This module has no lifecycle send port.
use super::*;
use crate::flow::plan::{
    AcceptedPlanUpdate, PreparedRootEffect, RootPlanEvidence, preflight_read_refusal,
};
use crate::flow::pump::plan::RootPlanPacket;
use crate::flow::templates::TemplateExpansion;
use marks_driver::MarkOperation;
use receipt_driver::FilingOwner;
use weaver_types::LifecycleRefusal;
use weaver_types::state_flow::{PlanUpdateRefused, ReadSelector};

#[path = "context_plan_effect_filing.rs"]
mod effect_filing;
#[path = "context_plan_settlement.rs"]
pub(super) mod settlement;
use settlement::RootPlanRecord;

#[allow(clippy::large_enum_variant)] // The allocator boxes this entire retained account.
pub(super) enum RootPlanInput {
    Accepted(AcceptedPlanUpdate),
    Refused {
        original: RootPlanPacket,
        refusal: PlanUpdateRefused,
    },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RootPlanProgress {
    Pending,
    InputReady,
    InputFiled,
    EffectReady,
    EffectFiled,
    SummaryReady,
    RefusalReady,
    Cancelled,
    Complete,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Current,
    Marks,
    Input,
    InputFiled,
    Closure,
    Effect,
    EffectFiling,
    EffectFiled,
    Summary,
    SummaryReady,
    TemplateFault,
    Refusal,
    Cancelled,
    Complete,
}

pub(super) struct RootPlanTransaction {
    pub(super) driving: bool,
    bound: AbsoluteBound,
    input: RootPlanInput,
    stage: Stage,
    current: Option<RecordedRead>,
    catalog: Option<RecordedRead>,
    refused_read: Option<Box<RecordedReadRefusal>>,
    refusal: Option<PlanUpdateRefused>,
    cancelled: bool,
    cancelled_read: Option<read_driver::RetainedReadCompletion>,
    input_event: Option<AuthoredEvent>,
    input_filing: Option<FilingCompletion>,
    closures: Vec<RecordedRead>,
    effect: Option<PreparedRootEffect>,
    expansion: Option<TemplateExpansion>,
    effect_event: Option<AuthoredEvent>,
    effect_filing: Option<FilingCompletion>,
    summary: Option<PlanSummaryCompletion>,
    fault_account: Option<Box<serde_json::value::RawValue>>,
    fault_started: bool,
    fault_sequence: Option<Sequence>,
    returned_author: StoppedAuthor,
    refusal_record: Option<RootPlanRecord>,
    settled: bool,
    wait_ready: bool,
}
pub(super) struct RootPlanBeginError {
    pub original: Option<RootPlanPacket>,
    pub refusal: LifecycleRefusal,
    pub failure: Option<RunFailure>,
}
impl RootPlanTransaction {
    #[cfg(test)]
    pub(super) fn is_cancelled(&self) -> bool {
        self.cancelled
    }
    pub(super) fn input(&self) -> &RootPlanInput {
        &self.input
    }
    pub(super) fn refusal(&self) -> Option<&PlanUpdateRefused> {
        match &self.input {
            RootPlanInput::Refused { refusal, .. } => Some(refusal),
            _ => self.refusal.as_ref(),
        }
    }
    pub(super) fn read_refusal(&self) -> Option<&RecordedReadRefusal> {
        self.refused_read.as_deref()
    }
    pub(super) fn input_filing(&self) -> Option<&FilingCompletion> {
        self.input_filing.as_ref()
    }
    pub(super) fn effect(&self) -> Option<&PreparedRootEffect> {
        self.effect.as_ref()
    }
    pub(super) fn effect_filing(&self) -> Option<&FilingCompletion> {
        self.effect_filing
            .as_ref()
            .or_else(|| match self.summary.as_ref()? {
                PlanSummaryCompletion::Effect(proof) => Some(proof.filing()),
                PlanSummaryCompletion::Refused(account)
                | PlanSummaryCompletion::Cancelled(account) => account.filing(),
                PlanSummaryCompletion::Boot(_) => None,
            })
    }
    pub(super) fn summary(&self) -> Option<&EffectPlanSummary> {
        match self.summary.as_ref()? {
            PlanSummaryCompletion::Effect(proof) => Some(proof),
            _ => None,
        }
    }
    pub(super) fn closures(&self) -> &[RecordedRead] {
        &self.closures
    }
    fn closure_selector(
        &self,
        context: &RecordingContext,
    ) -> Result<Option<ReadSelector>, &'static str> {
        let RootPlanInput::Accepted(input) = &self.input else {
            return Err("closure read has no accepted root input");
        };
        input.closure_selector(
            context,
            self.current
                .as_ref()
                .ok_or("closure read has no plan catalog")?,
            self.closures.len(),
        )
    }
}

impl RecordingContext {
    pub(super) fn root_plan_summary_allowed(&self, key: PumpDeadline) -> bool {
        self.root_plan_work().is_some_and(|work| {
            work.driving
                && matches!(work.stage, Stage::EffectFiled | Stage::Summary)
                && key == PumpDeadline::Decision
                && self.deadlines.get(key) == Some(work.bound)
        })
    }
    pub(in crate::flow::context) fn root_plan_wait_ready(&self) -> bool {
        self.root_plan_work().is_some_and(|work| work.wait_ready)
    }
    pub(super) fn root_plan_work(&self) -> Option<&RootPlanTransaction> {
        match self.marks.as_ref()? {
            MarkOperation::Plan(work) => Some(work),
            _ => None,
        }
    }
    fn root_plan_work_mut(&mut self) -> Option<&mut RootPlanTransaction> {
        match self.marks.as_mut()? {
            MarkOperation::Plan(work) => Some(work),
            _ => None,
        }
    }
    pub(super) fn root_plan_read_allowed(&self, selector: &ReadSelector) -> bool {
        self.root_plan_work().is_some_and(|work| {
            work.driving
                && !work.cancelled
                && (matches!(
                    (work.stage, selector),
                    (Stage::Current, ReadSelector::PlanCurrent {})
                        | (Stage::Marks, ReadSelector::Marks {})
                ) || (work.stage == Stage::Closure
                    && work.closure_selector(self).ok().flatten().as_ref() == Some(selector)))
        })
    }
    pub(super) fn root_plan_bound(&self) -> Option<AbsoluteBound> {
        self.root_plan_work().map(|work| work.bound)
    }
    pub(in crate::flow) fn root_plan_effect_identity(
        &self,
        input: &AcceptedPlanUpdate,
    ) -> Option<(
        &StartIdentity,
        &weaver_types::state_flow::SchemeDeclaration,
        &str,
        AbsoluteBound,
    )> {
        let work = self.root_plan_work()?;
        if !work.driving
            || work.cancelled
            || work.stage != Stage::InputFiled
            || self.deadlines.get(PumpDeadline::Decision) != Some(work.bound)
            || !matches!(&work.input, RootPlanInput::Accepted(owned) if std::ptr::eq(owned, input))
        {
            return None;
        }
        Some((
            self.open_start.as_ref()?,
            &self.declaration.scheme,
            self.flow.run(),
            work.bound,
        ))
    }
    pub(super) fn root_plan_filing_allowed(&self, key: PumpDeadline) -> bool {
        self.root_plan_work().is_some_and(|work| {
            work.driving
                && matches!(work.stage, Stage::Input | Stage::EffectFiling)
                && key == PumpDeadline::Decision
                && self.deadlines.get(key) == Some(work.bound)
                && matches!(&work.input, RootPlanInput::Accepted(input) if input.belongs_to(self))
        })
    }
    #[allow(clippy::result_large_err)] // Return the exact authenticated packet without allocating on refusal.
    pub(super) fn begin_root_plan_update(
        &mut self,
        packet: RootPlanPacket,
        io: &mut impl PumpIo,
    ) -> Result<(), RootPlanBeginError> {
        if let Err(failure) = self.live(io) {
            return Err(RootPlanBeginError {
                original: Some(packet),
                refusal: LifecycleRefusal::OutOfOrder,
                failure: Some(failure),
            });
        }
        if self.phase != RecordingPhase::Entered
            || self.flow.phase() != CoordinatorPhase::Active
            || !self.barrier.is_active()
            || self.marks.is_some()
            || self.load_duty.is_none()
            || self.open_turn.is_some()
            || self.input.snapshot().is_some()
            || self.input_turn.is_some()
            || self.input_author.is_some()
            || self.member_capture.is_some()
            || self.boot_pairs.is_some()
            || self.boot_open_blocks_external()
            || self.place_blocks_external()
            || self.plan_summary_blocks_external()
            || self.measure_blocks_external()
            || self.freshness.is_some()
            || self.output.is_some()
            || self.filing.is_some()
            || self.pair_work.is_some()
            || self.repair.is_some()
            || self.read.is_some()
            || self.inspection.is_some()
            || self.templates_block_content()
            || self.caller_author_held()
            || self.waiting_content.is_some()
            || self.completed_author.is_some()
            || self.barrier.pending().is_some()
            || self.barrier.pending_author().is_some()
            || self.flow.has_pending_record()
            || self.flow.has_filing_consumer()
            || self.flow.has_output_consumer()
            || self.flow.pending_check().is_some()
            || self.flow.pending_control().is_some()
            || self.flow.operation_completion().is_some()
            || !self.flow.state().outstanding().is_empty()
            || self.flow.state().held_measurement().is_some()
            || [
                PumpDeadline::Enter,
                PumpDeadline::Leave,
                PumpDeadline::Decision,
                PumpDeadline::Cut,
                PumpDeadline::Gate,
                PumpDeadline::StateControl,
                PumpDeadline::CaptureWindow,
            ]
            .into_iter()
            .any(|key| self.deadlines.get(key).is_some())
            || self.committed_plan_summary().is_err()
        {
            return Err(RootPlanBeginError {
                original: Some(packet),
                refusal: LifecycleRefusal::OutOfOrder,
                failure: None,
            });
        }
        // Parsing and hashing spend this same acceptance window. Store original
        // custody before its first post-parse clock check can report expiry.
        let bound = match AbsoluteBound::after(
            self.last_ns,
            self.load_duty
                .as_ref()
                .unwrap()
                .instruction
                .decision_wait
                .elapsed_ns
                .0,
        ) {
            Ok(bound) => bound,
            Err(error) => {
                return Err(RootPlanBeginError {
                    original: Some(packet),
                    refusal: LifecycleRefusal::OutOfOrder,
                    failure: Some(self.root_plan_fault(io, error.to_string())),
                });
            }
        };
        let (input, stage) = match AcceptedPlanUpdate::from_received(packet) {
            Ok(mut input) => {
                input.bind_to(self);
                (RootPlanInput::Accepted(input), Stage::Current)
            }
            Err((original, refusal)) => {
                (RootPlanInput::Refused { original, refusal }, Stage::Refusal)
            }
        };
        self.marks = Some(MarkOperation::Plan(Box::new(RootPlanTransaction {
            driving: false,
            bound,
            input,
            stage,
            current: None,
            catalog: None,
            refused_read: None,
            refusal: None,
            cancelled: false,
            cancelled_read: None,
            input_event: None,
            input_filing: None,
            closures: Vec::new(),
            effect: None,
            expansion: None,
            effect_event: None,
            effect_filing: None,
            summary: None,
            fault_account: None,
            fault_started: false,
            fault_sequence: None,
            returned_author: StoppedAuthor::default(),
            refusal_record: None,
            settled: false,
            wait_ready: false,
        })));
        self.arm(PumpDeadline::Decision, bound, io)
            .map_err(|failure| RootPlanBeginError {
                original: None,
                refusal: LifecycleRefusal::OutOfOrder,
                failure: Some(failure),
            })
    }

    pub(super) fn advance_root_plan_update<I: PumpIo>(
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
        if self.deadlines.get(PumpDeadline::Decision) != Some(work.bound) {
            return Err(
                self.root_plan_fault(ports.io, "root update lost its original Decision wait")
            );
        }
        self.root_plan_work_mut().unwrap().driving = true;
        self.root_plan_work_mut().unwrap().wait_ready = false;
        let result = match self.root_plan_work().unwrap().stage {
            Stage::Input => self.advance_root_plan_input(ports),
            Stage::InputFiled => self.advance_root_plan_effect_preflight(ports.io),
            Stage::EffectFiling => self.advance_root_plan_effect_filing(ports),
            Stage::TemplateFault => self.advance_root_plan_template_fault(ports),
            Stage::EffectFiled | Stage::Summary => self.advance_root_plan_summary(ports),
            Stage::SummaryReady => Ok(if self.root_plan_work().unwrap().cancelled {
                RootPlanProgress::Cancelled
            } else {
                RootPlanProgress::SummaryReady
            }),
            _ => self.advance_root_plan_read(ports),
        };
        self.root_plan_work_mut().unwrap().driving = false;
        result
    }
    fn advance_root_plan_read<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<RootPlanProgress, RunFailure> {
        let work = self.root_plan_work().unwrap();
        if work.cancelled && self.read.is_none() && work.stage != Stage::Refusal {
            self.root_plan_work_mut().unwrap().stage = Stage::Cancelled;
            return Ok(RootPlanProgress::Cancelled);
        }
        let selector = match work.stage {
            Stage::Current => ReadSelector::PlanCurrent {},
            Stage::Marks => ReadSelector::Marks {},
            Stage::Input => return Ok(RootPlanProgress::InputReady),
            Stage::InputFiled => return Ok(RootPlanProgress::InputFiled),
            Stage::Closure => work
                .closure_selector(self)
                .map_err(|detail| self.root_plan_fault(ports.io, detail))?
                .ok_or_else(|| self.root_plan_fault(ports.io, "closure read lost its selector"))?,
            Stage::Effect => return Ok(RootPlanProgress::EffectReady),
            Stage::EffectFiling
            | Stage::EffectFiled
            | Stage::Summary
            | Stage::SummaryReady
            | Stage::TemplateFault => {
                unreachable!("root effect driver")
            }
            Stage::Refusal => return Ok(RootPlanProgress::RefusalReady),
            Stage::Cancelled => return Ok(RootPlanProgress::Cancelled),
            Stage::Complete => return Ok(RootPlanProgress::Complete),
        };
        if self.read.is_none() {
            match self.begin_root_plan_read(selector.clone(), ports.io) {
                Ok(()) => (),
                Err(error) => {
                    if let Some(failure) = error.failure {
                        return Err(failure);
                    }
                    if error.refusal == ReadBeginRefusal::Busy {
                        self.root_plan_work_mut().unwrap().wait_ready = true;
                        return Ok(RootPlanProgress::Pending);
                    }
                    return Err(self.root_plan_fault(ports.io, "root plan read lost its admission"));
                }
            }
        }
        if self.root_plan_work().unwrap().cancelled {
            self.cancel_read(ports.io)?;
        }
        let progress = self.advance_read(ports)?;
        self.root_plan_work_mut().unwrap().wait_ready = progress == ReadProgress::Pending;
        let Some(done) = self.take_read_retained(ports.io)? else {
            return Ok(RootPlanProgress::Pending);
        };
        // Keep the completed original with its owner before any fallible check.
        self.root_plan_work_mut().unwrap().cancelled_read = Some(done);
        if self.root_plan_work().unwrap().cancelled {
            self.root_plan_work_mut().unwrap().stage = Stage::Cancelled;
            return Ok(RootPlanProgress::Cancelled);
        }
        let retained = self
            .root_plan_work()
            .unwrap()
            .cancelled_read
            .as_ref()
            .unwrap();
        if let Some(proof) = &retained.refusal {
            let refusal = preflight_read_refusal(self, &selector, proof)
                .map_err(|detail| self.root_plan_fault(ports.io, detail))?;
            let work = self.root_plan_work_mut().unwrap();
            work.refusal = Some(refusal);
            work.refused_read = work.cancelled_read.take().unwrap().refusal;
            work.stage = Stage::Refusal;
            return Ok(RootPlanProgress::RefusalReady);
        }
        if !matches!(&retained.completion, ReadCompletion::Recorded(proof) if proof.belongs_to(self) && proof.selector() == &selector)
        {
            return Err(self.root_plan_fault(
                ports.io,
                "root plan read returned foreign or cancelled evidence",
            ));
        }
        let work = self.root_plan_work_mut().unwrap();
        let ReadCompletion::Recorded(proof) = work.cancelled_read.take().unwrap().completion else {
            unreachable!("judged read")
        };
        match work.stage {
            Stage::Current => {
                work.current = Some(proof);
                work.stage = Stage::Marks;
            }
            Stage::Marks => {
                work.catalog = Some(proof);
                work.stage = Stage::Input;
            }
            Stage::Closure => {
                work.closures.push(proof);
                work.stage = Stage::InputFiled;
            }
            _ => unreachable!("pending root read stage"),
        }
        if work.stage == Stage::Input {
            self.check_root_plan_snapshots(ports.io)
        } else {
            Ok(RootPlanProgress::Pending)
        }
    }
    fn advance_root_plan_effect_preflight(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<RootPlanProgress, RunFailure> {
        let work = self.root_plan_work().unwrap();
        if work.cancelled {
            self.root_plan_work_mut().unwrap().stage = Stage::Cancelled;
            return Ok(RootPlanProgress::Cancelled);
        }
        if work
            .closure_selector(self)
            .map_err(|detail| self.root_plan_fault(io, detail))?
            .is_some()
        {
            self.root_plan_work_mut().unwrap().stage = Stage::Closure;
            return Ok(RootPlanProgress::Pending);
        }
        let work = self.root_plan_work().unwrap();
        let RootPlanInput::Accepted(input) = &work.input else {
            unreachable!("filed root input")
        };
        let effect = input
            .prepare_effect(RootPlanEvidence {
                context: self,
                current: work.current.as_ref().unwrap(),
                marks: work.catalog.as_ref().unwrap(),
                input: work.input_filing.as_ref().unwrap(),
                closures: &work.closures,
            })
            .map_err(|detail| self.root_plan_fault(io, detail))?;
        let work = self.root_plan_work_mut().unwrap();
        match effect {
            Ok(effect) => {
                work.effect = Some(effect);
                work.stage = Stage::Effect;
                Ok(RootPlanProgress::EffectReady)
            }
            Err(refusal) => {
                work.refusal = Some(refusal);
                work.stage = Stage::Refusal;
                Ok(RootPlanProgress::RefusalReady)
            }
        }
    }
    fn check_root_plan_snapshots(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<RootPlanProgress, RunFailure> {
        let scheme =
            weaver_types::state_flow::decode_strict::<weaver_types::state_flow::Ste100Scheme>(
                &self
                    .load_duty
                    .as_ref()
                    .expect("retained loaded duty")
                    .scheme_text,
            )
            .map_err(|_| self.root_plan_fault(io, "root preflight lost its loaded scheme"))?;
        let work = self.root_plan_work().unwrap();
        let RootPlanInput::Accepted(input) = &work.input else {
            unreachable!("accepted preflight")
        };
        let refusal = input
            .check_snapshots(
                self,
                work.current.as_ref().unwrap(),
                work.catalog.as_ref().unwrap(),
                &self.open_start.as_ref().expect("entered store").seed_id,
                &scheme,
            )
            .map_err(|detail| self.root_plan_fault(io, detail))?;
        if let Some(refusal) = refusal {
            let work = self.root_plan_work_mut().unwrap();
            work.refusal = Some(refusal);
            work.stage = Stage::Refusal;
            Ok(RootPlanProgress::RefusalReady)
        } else {
            Ok(RootPlanProgress::InputReady)
        }
    }
    fn advance_root_plan_input<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<RootPlanProgress, RunFailure> {
        if self.filing.is_none() {
            if self.root_plan_work().unwrap().cancelled {
                self.root_plan_work_mut().unwrap().stage = Stage::Cancelled;
                return Ok(RootPlanProgress::Cancelled);
            }
            if self.root_plan_work().unwrap().input_event.is_none() {
                let RootPlanInput::Accepted(input) = &self.root_plan_work().unwrap().input else {
                    return Err(
                        self.root_plan_fault(ports.io, "root filing lost its accepted input")
                    );
                };
                let event = input
                    .event(self)
                    .map_err(|detail| self.root_plan_fault(ports.io, detail))?;
                self.root_plan_work_mut().unwrap().input_event = Some(event);
            }
            let event = self
                .root_plan_work_mut()
                .unwrap()
                .input_event
                .take()
                .unwrap();
            if let Err(error) = self.begin_owned_filing(
                event,
                PumpDeadline::Decision,
                FilingOwner::RootPlan,
                ports.io,
            ) {
                self.root_plan_work_mut().unwrap().input_event = Some(error.event);
                if let Some(failure) = error.failure {
                    return Err(failure);
                }
                if error.refusal == FilingBeginRefusal::Busy {
                    self.root_plan_work_mut().unwrap().wait_ready = true;
                    return Ok(RootPlanProgress::Pending);
                }
                return Err(self.root_plan_fault(ports.io, "root input filing lost its admission"));
            }
        }
        // Once admitted, Stop/Leave drains every classification and receipt.
        // It does not cancel the subscription and lose the remaining proposals.
        let progress = self.advance_owned_filing(FilingOwner::RootPlan, ports)?;
        self.root_plan_work_mut().unwrap().wait_ready = progress == FilingProgress::Pending;
        let Some(filing) = self.take_owned_filing(FilingOwner::RootPlan, ports.io)? else {
            return Ok(RootPlanProgress::Pending);
        };
        self.root_plan_work_mut().unwrap().input_filing = Some(filing);
        if self.root_plan_work().unwrap().cancelled {
            self.root_plan_work_mut().unwrap().stage = Stage::Cancelled;
            return Ok(RootPlanProgress::Cancelled);
        }
        let work = self.root_plan_work().unwrap();
        let RootPlanInput::Accepted(input) = &work.input else {
            unreachable!("accepted root filing")
        };
        let refusal = input
            .check_filing(
                self,
                work.input_filing.as_ref().unwrap(),
                self.flow.run(),
                &self.declaration.scheme.scheme_id,
                work.bound,
            )
            .map_err(|detail| self.root_plan_fault(ports.io, detail))?;
        let work = self.root_plan_work_mut().unwrap();
        work.refusal = refusal;
        if work.refusal.is_some() {
            work.stage = Stage::Refusal;
            Ok(RootPlanProgress::RefusalReady)
        } else {
            work.stage = Stage::InputFiled;
            Ok(RootPlanProgress::InputFiled)
        }
    }
    pub(super) fn cancel_root_plan_update(&mut self) {
        if let Some(work) = self.root_plan_work_mut()
            && !work.settled
        {
            work.cancelled = true;
        }
    }
    fn root_plan_fault(&mut self, io: &mut impl PumpIo, detail: impl Into<String>) -> RunFailure {
        self.fail(
            io,
            FlowHaltReason::ProtocolFailed,
            HaltPoint::Barrier,
            detail,
        )
    }
}
