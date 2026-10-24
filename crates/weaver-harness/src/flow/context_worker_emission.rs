//! Join the retained checked generation to repair and recorded outbound sort.
//! The worker elects each operation, including correction placement and decode.
//! Neither the passive repair ledger nor sort calls the decoder.
use super::*;
use crate::flow::context::{
    EffectiveEmission, OutboundCompletion, PairBeginRefusal, RepairAccount, RepairBeginRefusal,
    RepairCancelled, RepairExhausted, RepairProgress, RepairRefused, RestatementRequest,
};
use crate::flow::coordinator::{OriginalRequests, RequestScheme};
use crate::flow::sort::{OutboundInput, noun_keys};
use crate::flow::templates::{ExpansionIdentity, RestatementTemplate, TemplateCertificate};
use weaver_types::state_flow::{NonDispatchCause, SortAct, SourcePosition, StatementRef};

#[cfg(test)]
#[path = "context_worker_emission_tests.rs"]
pub(in crate::flow::context) mod tests;

#[path = "context_worker_correction.rs"]
pub(super) mod correction;
use correction::CorrectionWork;

#[path = "context_worker_pairs.rs"]
pub(in crate::flow::context::worker_startup) mod execution;
use execution::WorkerPairs;

/// A compiled worker choice of restatement family. The actual entered run's
/// certificate, not this name, proves that the family can be used.
pub struct FoundationRestatement {
    template: RestatementTemplate,
}
impl FoundationRestatement {
    pub const fn new(template_id: &'static str) -> Self {
        Self {
            template: RestatementTemplate::new(template_id),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoundationRepairAdmission {
    WrongPhase,
    UncertifiedFamily,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EmissionStage {
    Begin,
    Repair,
    Restatement,
    Correction,
    PlacementRefusal,
    RefusalClose,
    Effective,
    BeginOutbound,
    Outbound,
    Sorted,
    Execution,
    Refused,
    RefusalAuthor,
    CancelAccount,
    Pairs,
    Cancelled,
}

pub(super) struct EmissionWork {
    template: Option<&'static RestatementTemplate>,
    stage: EmissionStage,
    cancelled: bool,
    original: Option<OriginalRequests>,
    restatement: Option<RestatementRequest>,
    corrections: Vec<CorrectionWork>,
    closing_correction: bool,
    correction_close: Option<weaver_types::TurnFrame>,
    effective: Option<EffectiveEmission>,
    refused: Option<RepairRefused>,
    exhausted: Option<RepairExhausted>,
    cancelled_repair: Option<RepairCancelled>,
    outbound: Option<OutboundInput>,
    result: Option<OutboundCompletion>,
    execution: Option<WorkerPairs>,
    account: Option<RepairAccount>,
    refusal_evidence: Option<crate::flow::context::RepairRefusalEvidence>,
    pending_correction: Option<OriginalRequests>,
    sort_record: Option<crate::flow::context::FilingCompletion>,
    sort_fault: Option<weaver_trace::Sequence>,
    selected: Option<(SourcePosition, Vec<SortAct>)>,
    refusal_author: Option<AuthoredEvent>,
    refusal_started: bool,
    returned_refusal: crate::flow::context::StoppedAuthor,
    refusal_record: Option<weaver_trace::Sequence>,
    unexpected_refusal: Option<AuthoredEvent>,
    detail: Option<String>,
}

impl EmissionWork {
    pub(super) fn selected_answer(&self) -> Option<(String, weaver_types::Finish)> {
        let effective = self.execution.as_ref()?.completed_effective()?;
        if *effective.finish() == weaver_types::Finish::Stopped {
            return None;
        }
        let (_, acts) = self.selected.as_ref()?;
        Some((effective.answer_text(acts).ok()?, *effective.finish()))
    }
    fn new(template: Option<&'static RestatementTemplate>) -> Self {
        Self {
            template,
            stage: EmissionStage::Begin,
            cancelled: false,
            original: None,
            restatement: None,
            corrections: vec![],
            closing_correction: false,
            correction_close: None,
            effective: None,
            refused: None,
            exhausted: None,
            cancelled_repair: None,
            outbound: None,
            result: None,
            execution: None,
            account: None,
            refusal_evidence: None,
            pending_correction: None,
            sort_record: None,
            sort_fault: None,
            selected: None,
            detail: None,
            refusal_author: None,
            refusal_started: false,
            returned_refusal: crate::flow::context::StoppedAuthor::default(),
            refusal_record: None,
            unexpected_refusal: None,
        }
    }
    pub(super) fn stopped() -> Self {
        let mut work = Self::new(None);
        work.cancelled = true;
        work
    }
    pub(super) fn progress(&self) -> FoundationInputProgress {
        match self.stage {
            EmissionStage::Restatement => FoundationInputProgress::Restatement,
            EmissionStage::Correction => self.corrections.last().unwrap().progress(),
            EmissionStage::Effective => FoundationInputProgress::Effective,
            EmissionStage::Sorted => FoundationInputProgress::Sorted,
            EmissionStage::Execution => self.execution.as_ref().unwrap().progress(),
            EmissionStage::Refused | EmissionStage::Cancelled => {
                FoundationInputProgress::OperationRefused
            }
            _ => FoundationInputProgress::Pending,
        }
    }
    pub(super) fn settled(&self) -> bool {
        self.stage == EmissionStage::Cancelled
    }
    pub(super) fn detail(&self) -> Option<&str> {
        self.detail.as_deref()
    }
    pub(super) fn closing_correction(&self) -> bool {
        self.closing_correction
    }
    pub(super) fn correction_close(&self) -> Option<&weaver_types::TurnFrame> {
        self.correction_close.as_ref()
    }

    pub(super) fn cancel(
        &mut self,
        context: &mut RecordingContext,
        decode: Option<&mut crate::flow::pump_transport::DecodePort>,
        gate: Option<&mut crate::flow::pump_transport::OrganPort>,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        self.cancelled = true;
        if self.stage == EmissionStage::Execution {
            self.execution.as_mut().unwrap().cancel(context, gate, io)?;
        } else if context.repair.is_some() {
            context.cancel_repair(NonDispatchCause::Stop, io)?;
            if self.stage == EmissionStage::Correction {
                self.corrections
                    .last_mut()
                    .unwrap()
                    .cancel(context, decode, io)?;
            } else {
                self.stage = EmissionStage::Repair;
            }
        } else if self.stage == EmissionStage::Outbound {
            context.cancel_outbound();
        } else if !matches!(
            self.stage,
            EmissionStage::Begin
                | EmissionStage::Pairs
                | EmissionStage::Cancelled
                | EmissionStage::RefusalAuthor
        ) {
            self.stage = EmissionStage::CancelAccount;
        }
        Ok(())
    }

    pub(super) fn advance<I: PumpIo>(
        &mut self,
        generation: &mut Option<GenerationCompletion>,
        certificate: &mut Option<TemplateCertificate>,
        context: &mut RecordingContext,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<(), RunFailure> {
        match self.stage {
            EmissionStage::Execution => {
                let work = self.execution.as_mut().unwrap();
                work.advance(&self.selected.as_ref().unwrap().1, context, ports)?;
                if self.cancelled && work.progress() == FoundationInputProgress::PairsComplete {
                    self.stage = EmissionStage::Cancelled;
                }
            }
            EmissionStage::PlacementRefusal => {
                let correction = self.corrections.last().unwrap();
                self.account = Some(correction.finish_refusal(context, ports.io)?);
                *certificate = context.take_repair_certificate(ports.io)?;
                self.stage = EmissionStage::CancelAccount;
            }
            EmissionStage::RefusalClose => {
                self.returned_refusal.check()?;
                if self.refusal_author.is_none() && !self.refusal_started {
                    self.refusal_author = Some(AuthoredEvent {
                        kind: serving::Kind::TurnClosed,
                        subsystem: Subsystem::Harness,
                        turn: context.open_turn.clone(),
                        source: AuthoredSource::Runtime,
                        causal_parent: None,
                        payload: Some(serving::Payload::TurnClosed(
                            weaver_trace::TurnClose::Stopped {
                                reason: weaver_trace::StopReason::Refused,
                                cause: None,
                            },
                        )),
                    });
                }
                let turn = self.account.as_ref().unwrap().turn().clone();
                let sequence = if self.refusal_started {
                    context
                        .resume_author_and_wait(ports)
                        .map_err(|failure| self.returned_refusal.retain(failure))?
                } else {
                    self.refusal_started = true;
                    context
                        .author_and_wait(ports, self.refusal_author.take().unwrap())
                        .map_err(|failure| {
                            self.returned_refusal
                                .retain_started(failure, &mut self.refusal_started)
                        })?
                };
                self.refusal_record = Some(sequence);
                let frame = crate::lifecycle::render_close(
                    "stopped",
                    "reason",
                    self.detail.as_deref().expect("placement refusal retained"),
                    Some((
                        &weaver_types::TurnKey(turn.0),
                        &weaver_types::RunId(context.barrier.run().to_owned()),
                    )),
                );
                self.correction_close = Some(weaver_types::TurnFrame::carry(frame.as_bytes()));
                self.stage = EmissionStage::Refused;
            }
            EmissionStage::Correction => {
                let correction = self.corrections.last_mut().unwrap();
                if correction.advance(context, ports)? {
                    self.stage = if correction.refused() && !self.cancelled {
                        self.detail = correction.detail().map(str::to_owned);
                        EmissionStage::Refused
                    } else {
                        EmissionStage::Repair
                    };
                }
            }
            EmissionStage::Begin => {
                if self.original.is_none() {
                    let scheme = RequestScheme::from_exact_scheme(
                        &context.load_duty.as_ref().unwrap().scheme_text,
                        &context.declaration.scheme,
                    )
                    .map_err(|e| fault(context, ports.io, format!("emission scheme: {e:?}")))?;
                    let Some(GenerationCompletion::Generated(proof)) = generation.as_mut() else {
                        unreachable!("emission requires the retained generation")
                    };
                    let checked = proof.take_output().expect("one checked OUTPUT loan");
                    match OriginalRequests::from_checked(checked, &scheme) {
                        Ok(original) => self.original = Some(original),
                        Err(error) => {
                            proof.output = Some(error.generation);
                            return Err(fault(
                                context,
                                ports.io,
                                format!("emission associations: {:?}", error.error),
                            ));
                        }
                    }
                }
                let original = self.original.take().unwrap();
                let certified = certificate.take().unwrap();
                let begin = match self.template {
                    Some(template) => context.begin_repair(original, certified, template, ports.io),
                    None => context.begin_stopped_emission(original, certified, ports.io),
                };
                match begin {
                    Ok(()) => {
                        self.stage = EmissionStage::Repair;
                        if self.cancelled {
                            context.cancel_repair(NonDispatchCause::Stop, ports.io)?;
                        }
                    }
                    Err(error) => {
                        let refusal = error.refusal;
                        let failure = error.failure.clone();
                        let (original, returned, owed) = error.into_parts();
                        self.original = Some(original);
                        *certificate = Some(returned);
                        // The compiled family was checked before transfer against
                        // this same immutable certificate. Any new mismatch is a
                        // custody fault. Keep its author with the failed owner.
                        if let Some(event) = owed {
                            self.unexpected_refusal = Some(event);
                        }
                        if let Some(failure) = failure {
                            return Err(failure);
                        }
                        if refusal != RepairBeginRefusal::Busy {
                            return Err(fault(
                                context,
                                ports.io,
                                format!("emission admission: {refusal:?}"),
                            ));
                        }
                        context.pump_once(ports)?;
                    }
                }
            }
            EmissionStage::Repair => {
                let progress = context.advance_repair(ports)?;
                match progress {
                    RepairProgress::Complete => {
                        if let Some(effective) = context.take_effective_emission(ports.io)? {
                            self.effective = Some(effective);
                            self.stage = EmissionStage::Effective;
                        }
                    }
                    RepairProgress::Restatement => {
                        self.restatement = context.take_restatement(ports.io)?;
                        if self.restatement.is_some() {
                            self.stage = EmissionStage::Restatement;
                        }
                    }
                    RepairProgress::Exhausted => {
                        self.exhausted = context.take_repair_exhausted(ports.io)?;
                        if self.exhausted.is_some() {
                            self.detail = Some("repair exhausted".into());
                            self.stage = EmissionStage::Refused;
                        }
                    }
                    RepairProgress::Refused => {
                        self.refused = context.take_refused_repair(ports.io)?;
                        if self.refused.is_some() {
                            self.detail = Some("repair instruction refused".into());
                            self.stage = EmissionStage::Refused;
                        }
                    }
                    RepairProgress::Cancelled => {
                        self.cancelled_repair = context.take_cancelled_repair(ports.io)?;
                        if self.cancelled_repair.is_some() {
                            self.stage = EmissionStage::Cancelled;
                        }
                    }
                    RepairProgress::Pending => {
                        context.pump_once(ports)?;
                    }
                    RepairProgress::Progress => {}
                    RepairProgress::AwaitingCorrection => {
                        return Err(fault(
                            context,
                            ports.io,
                            "restatement loan lost at worker boundary".into(),
                        ));
                    }
                }
                if context.repair.is_none() && certificate.is_none() {
                    *certificate = context.take_repair_certificate(ports.io)?;
                    if certificate.is_none() {
                        return Err(RunFailure::RepairOwned);
                    }
                }
            }
            EmissionStage::BeginOutbound => {
                decision(context, ports.io)?;
                match context.begin_outbound(
                    self.outbound.take().unwrap(),
                    certificate.as_ref().unwrap(),
                    ports.io,
                ) {
                    Ok(()) => self.stage = EmissionStage::Outbound,
                    Err(error) => {
                        self.detail = Some(format!(
                            "outbound: {:?} / {:?} / {:?}",
                            error.invalid_candidate, error.template, error.refusal
                        ));
                        let busy = error.refusal == Some(PairBeginRefusal::Busy);
                        let failure = error.failure.clone();
                        let (input, owed) = error.into_parts();
                        self.outbound = Some(input);
                        if let Some(event) = owed {
                            self.refusal_author = Some(event);
                            self.stage = EmissionStage::RefusalAuthor;
                        } else if !busy {
                            context.complete_wait(PumpDeadline::Decision);
                            self.stage = EmissionStage::Refused;
                        }
                        if let Some(failure) = failure {
                            return Err(failure);
                        }
                        if busy {
                            context.pump_once(ports)?;
                        }
                    }
                }
            }
            EmissionStage::Outbound => {
                let progress = context.advance_outbound(ports)?;
                if let Some(result) = context.take_outbound(ports.io)? {
                    self.stage = match &result {
                        OutboundCompletion::Sorted(_) => {
                            context.complete_wait(PumpDeadline::Decision);
                            EmissionStage::Sorted
                        }
                        OutboundCompletion::Refused { .. } => {
                            context.complete_wait(PumpDeadline::Decision);
                            self.detail = Some("outbound instruction refused".into());
                            EmissionStage::Refused
                        }
                        OutboundCompletion::Cancelled { .. } => EmissionStage::CancelAccount,
                    };
                    self.result = Some(result);
                } else if progress == crate::flow::context::OutboundProgress::Pending {
                    context.pump_once(ports)?;
                }
            }
            EmissionStage::RefusalAuthor => {
                self.returned_refusal.check()?;
                let sequence = if self.refusal_started {
                    context.resume_author_and_wait(ports).map_err(|failure| {
                        self.returned_refusal
                            .retain_started(failure, &mut self.refusal_started)
                    })?
                } else {
                    self.refusal_started = true;
                    context
                        .author_and_wait(ports, self.refusal_author.take().unwrap())
                        .map_err(|failure| {
                            self.returned_refusal
                                .retain_started(failure, &mut self.refusal_started)
                        })?
                };
                self.refusal_record = Some(sequence);
                context.complete_wait(PumpDeadline::Decision);
                self.stage = if self.cancelled {
                    EmissionStage::CancelAccount
                } else {
                    EmissionStage::Refused
                };
            }
            EmissionStage::CancelAccount => {
                if let Some(effective) = self.effective.take() {
                    self.account = Some(effective.into_account());
                }
                if let Some(result) = self.result.take() {
                    let (input, record, fault) = match result {
                        OutboundCompletion::Sorted(sorted) => {
                            let (input, record) = sorted.into_parts();
                            (input, record, None)
                        }
                        OutboundCompletion::Refused {
                            input,
                            record,
                            fault,
                        } => (input, record, Some(fault)),
                        OutboundCompletion::Cancelled {
                            input,
                            record,
                            fault,
                        } => (input, record, fault),
                    };
                    self.outbound = Some(input);
                    self.sort_record = Some(record);
                    self.sort_fault = fault;
                }
                if let Some(input) = self.outbound.take() {
                    self.selected = Some((input.source, input.acts));
                    self.account = Some(input.effective.into_account());
                }
                if let Some(refused) = self.refused.take() {
                    let (account, pending, evidence) = refused.into_parts();
                    self.account = Some(account);
                    self.pending_correction = pending;
                    self.refusal_evidence = Some(evidence);
                }
                if let Some(exhausted) = self.exhausted.take() {
                    self.account = Some(exhausted.into_account());
                    self.stage = EmissionStage::Cancelled; // Exhaustion already settled every pair.
                    return Ok(());
                }
                decision(context, ports.io)?;
                let account = self
                    .account
                    .take()
                    .expect("completed emission retains its account");
                let cause = if self.closing_correction && !self.cancelled {
                    NonDispatchCause::PassPurpose
                } else {
                    NonDispatchCause::Stop
                };
                match context.begin_repair_not_dispatched(account, cause, ports.io) {
                    Ok(()) => self.stage = EmissionStage::Pairs,
                    Err(error) => {
                        self.account = Some(error.account);
                        if let Some(failure) = error.failure {
                            return Err(failure);
                        }
                        if error.refusal != PairBeginRefusal::Busy {
                            return Err(fault(
                                context,
                                ports.io,
                                format!("outbound cancellation: {:?}", error.refusal),
                            ));
                        }
                        context.pump_once(ports)?;
                    }
                }
            }
            EmissionStage::Pairs => {
                let progress = context.advance_pairs(ports)?;
                if let Some(account) = context.take_repair_accounting(ports.io)? {
                    self.account = Some(account);
                    context.complete_wait(PumpDeadline::Decision);
                    self.stage = if self.closing_correction && !self.cancelled {
                        EmissionStage::RefusalClose
                    } else {
                        EmissionStage::Cancelled
                    };
                } else if progress == crate::flow::context::PairProgress::Pending {
                    context.pump_once(ports)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
}

impl FoundationInput<'_> {
    /// Elect repair of the actual checked generation. A bad compiled family is
    /// rejected before ownership transfer, leaving Generated available intact.
    pub fn repair(
        &mut self,
        family: &'static FoundationRestatement,
    ) -> Result<(), FoundationRepairAdmission> {
        if self.progress() != FoundationInputProgress::Generated {
            return Err(FoundationRepairAdmission::WrongPhase);
        }
        let certificate = self.serving.owner.certificate.as_ref().unwrap();
        family
            .template
            .expand(
                certificate,
                ExpansionIdentity {
                    run_id: certificate.run_id(),
                    scheme: certificate.scheme(),
                    checker_artifact_sha256: certificate.checker_artifact_sha256(),
                    inventory_sha256: certificate.inventory_sha256(),
                },
            )
            .map_err(|_| FoundationRepairAdmission::UncertifiedFamily)?;
        let work = self.engine.as_mut().unwrap();
        work.emission = Some(Box::new(EmissionWork::new(Some(&family.template))));
        work.stage = EngineStage::Emission;
        Ok(())
    }

    pub fn effective_emission(&self) -> Option<FoundationEmission<'_>> {
        if self.progress() != FoundationInputProgress::Effective {
            return None;
        }
        Some(FoundationEmission {
            effective: self
                .engine
                .as_ref()?
                .emission
                .as_ref()?
                .effective
                .as_ref()?,
        })
    }
    /// Identity validation and its Recorded decision precede release of acts.
    /// The caller supplies their complete order and whether any Answer exists.
    pub fn sort_outbound(&mut self, acts: Vec<SortAct>) -> Result<(), Vec<SortAct>> {
        if self.progress() != FoundationInputProgress::Effective {
            return Err(acts);
        }
        let work = self.engine.as_mut().unwrap().emission.as_mut().unwrap();
        let effective = work.effective.take().unwrap();
        work.outbound = Some(OutboundInput {
            source: effective.source().clone(),
            effective,
            acts,
        });
        work.stage = EmissionStage::BeginOutbound;
        Ok(())
    }
    pub fn sorted_acts(&self) -> Option<&[SortAct]> {
        if self.progress() != FoundationInputProgress::Sorted {
            return None;
        }
        match self.engine.as_ref()?.emission.as_ref()?.result.as_ref()? {
            OutboundCompletion::Sorted(sorted) => Some(sorted.acts()),
            _ => None,
        }
    }
    /// Inspection only: the instruction, certification and filing lease stay
    /// with the owner through correction placement and decode.
    pub fn restatement_source(&self) -> Option<&weaver_types::state_flow::SourceBlockId> {
        if self.progress() != FoundationInputProgress::Restatement {
            return None;
        }
        Some(
            self.engine
                .as_ref()?
                .emission
                .as_ref()?
                .restatement
                .as_ref()?
                .source(),
        )
    }
}

/// Read-only workflow facts. Copies of these identities cannot construct the
/// private effective proof, filing receipt or execution grant.
pub struct FoundationEmission<'a> {
    effective: &'a EffectiveEmission,
}
impl FoundationEmission<'_> {
    pub fn source(&self) -> &SourcePosition {
        self.effective.source()
    }
    pub fn statements(&self) -> &[StatementRef] {
        self.effective.statements()
    }
    pub fn requests(&self) -> &[weaver_types::state_flow::PairStarted] {
        self.effective.requests()
    }
    pub fn text(&self) -> &str {
        self.effective.text()
    }
    pub fn finish(&self) -> &weaver_types::Finish {
        self.effective.finish()
    }
    pub fn attempted_call(&self) -> bool {
        self.effective.attempted_call()
    }
    /// SUBJECTS reads only checked effective bindings, using each original
    /// binding ordinal. This query-free projection preserves exact noun bytes.
    pub fn subjects(&self) -> Vec<String> {
        let mut keys = std::collections::BTreeSet::new();
        for statement in self.effective.statements() {
            let block = self
                .effective
                .account()
                .generations()
                .flat_map(|g| g.blocks())
                .find(|block| {
                    block.request().target
                        == weaver_types::state_flow::Ste100Target::Source(
                            statement.block_id.clone(),
                        )
                })
                .expect("effective statement has a checked source");
            let weaver_types::state_flow::Ste100Result::Accepted { bindings } =
                &block.reply().result
            else {
                unreachable!("effective statement is accepted")
            };
            let binding = &bindings[usize::try_from(statement.binding.0).expect("checked ordinal")];
            keys.extend(noun_keys(std::slice::from_ref(binding)));
        }
        keys.into_iter().collect()
    }
}

/// Move-owned sort completion, retaining the complete serving and source owners.
/// This proves a recorded identity sort; execution remains a separate operation.
///
/// ```compile_fail,E0616
/// fn raw(output: weaver_harness::FoundationOutbound<'_>) { let _ = output.input; }
/// ```
/// ```compile_fail,E0277
/// fn cloned<T: Clone>() {}
/// cloned::<weaver_harness::FoundationOutbound<'static>>();
/// ```
pub struct FoundationOutbound<'a> {
    pub(in crate::flow::context::worker_startup::input) input: Box<FoundationInput<'a>>,
}
impl<'a> FoundationOutbound<'a> {
    pub fn into_service(self) -> Box<FoundationInput<'a>> {
        self.input
    }
    pub fn acts(&self) -> &[SortAct] {
        self.input.sorted_acts().expect("sorted handoff")
    }
}
