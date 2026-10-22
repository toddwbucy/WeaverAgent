//! Recorded repair mechanics over passive repair, template and pair owners.
//! No decoder is called here. A restatement grant follows its actual instruction
//! record and any selected runtime check, never an ordinary filing receipt.

use super::repair::{
    CorrectionProgress, EffectiveEmission, RepairAccount, RepairError, RepairExhausted,
    RepairIntent, RepairLedger, RestatementRequest,
};
use super::*;
use crate::flow::author::AuthoredSource;
use crate::flow::coordinator::{
    AssociationRefusal, CheckOperation, CheckedGeneration, InstructionCheck, OriginalRequests,
    OutputCancellation, RequestScheme,
};
use crate::flow::observation::Transition;
use crate::flow::templates::{
    ExpansionIdentity, RestatementTemplate, TemplateCertificate, TemplateExpansion,
};
use weaver_trace::serving::payloads as trace;
use weaver_types::state_flow::{
    CallId, Digest, NonDispatchCause, ProtocolRepairCase, SourceBlockId, SourcePosition,
    Ste100Result,
};
use weaver_types::{FaultCase, FaultReport, RefusalRecord};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum RepairBeginRefusal {
    WrongPhase,
    Busy,
    WrongGeneration,
    LostCustody,
    Certificate,
    MissingWait,
}
#[derive(Debug)]
pub(in crate::flow) struct RepairBeginError {
    original: OriginalRequests,
    certificate: TemplateCertificate,
    pub refusal: RepairBeginRefusal,
    pub failure: Option<RunFailure>,
    instruction_refusal: Option<RefusalRecord>,
}
impl RepairBeginError {
    pub(in crate::flow) fn original(&self) -> &OriginalRequests {
        &self.original
    }
    pub(in crate::flow) fn certificate(&self) -> &TemplateCertificate {
        &self.certificate
    }
    /// The loop owns the returned event and must record it before reporting the
    /// refusal. An ordinary pending author resumes the same event. Consuming
    /// this error prevents replay or replacement of the retained refusal.
    pub(in crate::flow) fn into_parts(
        self,
    ) -> (OriginalRequests, TemplateCertificate, Option<AuthoredEvent>) {
        let event = self.instruction_refusal.map(|record| AuthoredEvent {
            kind: Kind::Refusal,
            subsystem: weaver_trace::Subsystem::Harness,
            turn: Some(self.original.generation().original().turn.clone()),
            source: AuthoredSource::Runtime,
            causal_parent: Some(Sequence(
                self.original.generation().source().block_id.sequence.0,
            )),
            payload: Some(Payload::Refusal(
                serde_json::value::to_raw_value(&record)
                    .expect("typed instruction refusal serializes"),
            )),
        });
        (self.original, self.certificate, event)
    }
}
#[derive(Debug)]
pub(in crate::flow) struct RepairObserveError {
    pub correction: OriginalRequests,
    pub error: RepairError,
    pub failure: Option<RunFailure>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum RepairProgress {
    Progress,
    Pending,
    Restatement,
    AwaitingCorrection,
    Complete,
    Exhausted,
    Refused,
    SpecGap,
    Cancelled,
}

#[derive(Debug)]
pub(in crate::flow) struct RepairCancelled {
    account: RepairAccount,
    pending: Option<OriginalRequests>,
    incomplete_output: Option<OutputCancellation>,
}
impl RepairCancelled {
    pub(in crate::flow) fn account(&self) -> &RepairAccount {
        &self.account
    }
    pub(in crate::flow) fn pending(&self) -> Option<&OriginalRequests> {
        self.pending.as_ref()
    }
    pub(in crate::flow) fn incomplete_output(&self) -> Option<&OutputCancellation> {
        self.incomplete_output.as_ref()
    }
}

/// A recorded operation refusal returns custody, never success, exhaustion or
/// evidence that accepted sends have reached a terminal state. The loop chooses
/// an actual disposition before passing the retained account to pair accounting.
#[derive(Debug)]
pub(in crate::flow) struct RepairRefused {
    account: RepairAccount,
    pending: Option<OriginalRequests>,
    evidence: RepairRefusalEvidence,
}
#[derive(Debug)]
pub(in crate::flow) enum RepairRefusalEvidence {
    RuntimeTemplate {
        fault: SourcePosition,
        check: crate::flow::coordinator::RecordedInstructionCheck,
    },
    Instruction {
        source: SourcePosition,
        record: RefusalRecord,
    },
}
impl RepairRefused {
    pub(in crate::flow) fn account(&self) -> &RepairAccount {
        &self.account
    }
    pub(in crate::flow) fn pending(&self) -> Option<&OriginalRequests> {
        self.pending.as_ref()
    }
    pub(in crate::flow) fn evidence(&self) -> &RepairRefusalEvidence {
        &self.evidence
    }
    pub(in crate::flow) fn into_parts(
        self,
    ) -> (
        RepairAccount,
        Option<OriginalRequests>,
        RepairRefusalEvidence,
    ) {
        (self.account, self.pending, self.evidence)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AuthorPurpose {
    Instruction,
    ProtocolRefusal,
    EmptyCorrectionRefusal,
    Association,
    TemplateFault,
    InstructionRefusal,
}
struct RepairAuthor {
    purpose: AuthorPurpose,
    event: Option<AuthoredEvent>,
    expansion: Option<TemplateExpansion>,
    source: Option<SourcePosition>,
    started: bool,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Accounting {
    Correction,
    Exhausted,
    CancelCorrection,
    CancelOriginal,
}

pub(super) struct RepairTransaction {
    ledger: RepairLedger,
    certificate: TemplateCertificate,
    template: &'static RestatementTemplate,
    internal: bool,
    bound: Option<AbsoluteBound>,
    author: Option<RepairAuthor>,
    measurement: bool,
    restatement: Option<RestatementRequest>,
    issued: bool,
    output_started: bool,
    issued_source: Option<SourceBlockId>,
    expected_correction: Option<SourcePosition>,
    held_output: Option<CheckedGeneration>,
    associations: Vec<RefusalRecord>,
    association_next: usize,
    association_last: Option<SourcePosition>,
    accounting: Option<Accounting>,
    pair_started: bool,
    correction: Option<OriginalRequests>,
    account: Option<RepairAccount>,
    calls: Vec<CallId>,
    refused: bool,
    rejection: Option<crate::flow::coordinator::RecordedInstructionCheck>,
    refusal_record: Option<RefusalRecord>,
    refusal_source: Option<SourcePosition>,
    cancel: Option<NonDispatchCause>,
    cancelled: Option<RepairCancelled>,
    incomplete_output: Option<OutputCancellation>,
}

impl RecordingContext {
    #[allow(clippy::result_large_err)]
    pub(in crate::flow) fn begin_repair(
        &mut self,
        original: OriginalRequests,
        certificate: TemplateCertificate,
        template: &'static RestatementTemplate,
        io: &mut impl PumpIo,
    ) -> Result<(), RepairBeginError> {
        if let Err(failure) = self.live(io) {
            return Err(RepairBeginError {
                original,
                certificate,
                refusal: RepairBeginRefusal::WrongPhase,
                failure: Some(failure),
                instruction_refusal: None,
            });
        }
        let refusal = if self.phase != RecordingPhase::Entered
            || self.flow.phase() != CoordinatorPhase::Active
            || !self.barrier.is_active()
        {
            Some(RepairBeginRefusal::WrongPhase)
        } else if self.marks.is_some()
            || self.repair.is_some()
            || self.freshness.is_some()
            || self.repair_certificate.is_some()
            || self.output.is_some()
            || self.filing.is_some()
            || self.pair_work.is_some()
            || self.input.snapshot().is_some()
            || self.input_turn.is_some()
            || self.input_author.is_some()
            || self.member_capture.is_some()
            || self.read.is_some()
            || self.inspection.is_some()
            || self.measure_blocks_external()
            || self.waiting_content.is_some()
            || self.completed_author.is_some()
            || (self.barrier.pending_author().is_some() && !self.flow.has_pending_record())
        {
            Some(RepairBeginRefusal::Busy)
        } else if original.generation().scheme() != &self.declaration.scheme
            || original.generation().source().block_id.run != self.barrier.run()
            || self.open_turn.as_ref() != Some(&original.generation().original().turn)
        {
            Some(RepairBeginRefusal::WrongGeneration)
        } else if !self.pair_custody_matches(&original) {
            Some(RepairBeginRefusal::LostCustody)
        } else if !self.repair_certificate_matches(&certificate) {
            Some(RepairBeginRefusal::Certificate)
        } else {
            None
        };
        if let Some(refusal) = refusal {
            let instruction_refusal = (refusal == RepairBeginRefusal::Certificate).then(|| RefusalRecord::Instruction {
                template_id: template.template_id().into(), case: weaver_types::state_flow::InstructionRefusal::InvalidContext,
                detail: "restatement certificate differs from this run's recorded load and acceptance transcript".into(),
            });
            return Err(RepairBeginError {
                original,
                certificate,
                refusal,
                failure: None,
                instruction_refusal,
            });
        }
        // Validate the compiled family before transferring the original. The
        // expansion itself is created anew only when its instruction is owed.
        if template
            .expand(
                &certificate,
                ExpansionIdentity {
                    run_id: certificate.run_id(),
                    scheme: certificate.scheme(),
                    checker_artifact_sha256: certificate.checker_artifact_sha256(),
                    inventory_sha256: certificate.inventory_sha256(),
                },
            )
            .is_err()
        {
            return Err(RepairBeginError {
                original,
                certificate,
                refusal: RepairBeginRefusal::Certificate,
                failure: None,
                instruction_refusal: Some(RefusalRecord::Instruction {
                    template_id: template.template_id().into(),
                    case: weaver_types::state_flow::InstructionRefusal::InvalidTemplate,
                    detail: "compiled restatement family has no usable certified expansion".into(),
                }),
            });
        }
        let cap = self
            .load_duty
            .as_ref()
            .unwrap()
            .instruction
            .repair_cap
            .attempts;
        let ledger = match RepairLedger::begin(original, cap) {
            Ok(ledger) => ledger,
            Err((original, _)) => {
                return Err(RepairBeginError {
                    original,
                    certificate,
                    refusal: RepairBeginRefusal::Certificate,
                    failure: None,
                    instruction_refusal: None,
                });
            }
        };
        self.repair = Some(RepairTransaction {
            ledger,
            certificate,
            template,
            internal: false,
            bound: self.deadlines.get(PumpDeadline::Decision),
            author: None,
            measurement: false,
            restatement: None,
            issued: false,
            output_started: false,
            issued_source: None,
            expected_correction: None,
            held_output: None,
            associations: vec![],
            association_next: 0,
            association_last: None,
            accounting: None,
            pair_started: false,
            correction: None,
            account: None,
            calls: vec![],
            refused: false,
            rejection: None,
            refusal_record: None,
            refusal_source: None,
            cancel: None,
            cancelled: None,
            incomplete_output: None,
        });
        Ok(())
    }

    pub(super) fn repair_certificate_matches(&self, certificate: &TemplateCertificate) -> bool {
        let Some(duty) = &self.load_duty else {
            return false;
        };
        let Some(transcript) = self.template_transcript() else {
            return false;
        };
        let Ok(checker) = Digest::new(duty.instruction.model_artifact_sha256.clone()) else {
            return false;
        };
        certificate.run_id() == self.barrier.run()
            && certificate.scheme() == &self.declaration.scheme
            && certificate.checker_artifact_sha256() == &checker
            && certificate.inventory().matches_load(&duty.template_inventory, &self.declaration.scheme, &checker).is_ok()
            && transcript.is_complete() && transcript.failure().is_none()
            && certificate.accepted().len() == transcript.records().len()
            && certificate.accepted().iter().zip(transcript.records()).all(|(accepted, recorded)| {
                matches!(recorded.operation(), CheckOperation::Template { template_id, .. } if template_id == accepted.template_id())
                    && recorded.reply().request_id == accepted.request_id()
                    && recorded.result_source() == accepted.result_source()
                    && matches!(recorded.reply().result, Ste100Result::Accepted { .. })
            })
    }

    pub(super) fn repair_blocks_external(&self) -> bool {
        self.repair.as_ref().is_some_and(|work| !work.internal)
    }
    pub(super) fn repair_owns_pairs(&self) -> bool {
        self.repair.is_some()
    }
    pub(super) fn repair_allows_output(&self) -> bool {
        self.repair.as_ref().is_none_or(|work| {
            work.internal
                || (work.cancel.is_none()
                    && !work.refused
                    && work.issued
                    && !work.output_started
                    && matches!(
                        work.ledger.intent(),
                        RepairIntent::AwaitingCorrection { .. }
                    ))
        })
    }
    pub(super) fn repair_note_output_started(&mut self) {
        if let Some(work) = self.repair.as_mut() {
            work.output_started = true;
            work.bound = self.deadlines.get(PumpDeadline::Decision);
        }
    }
    pub(super) fn repair_wait_still_owned(&self, key: PumpDeadline) -> bool {
        key == PumpDeadline::Decision
            && self
                .repair
                .as_ref()
                .is_some_and(|work| work.bound.is_some())
    }
    pub(super) fn freeze_repair(&mut self) {
        if let Some(work) = self.repair.as_mut() {
            work.ledger.freeze();
        }
    }
    pub(in crate::flow) fn repair_refused(&self) -> bool {
        self.repair.as_ref().is_some_and(|work| work.refused)
    }

    pub(super) fn bind_repair_author(
        &mut self,
        sequence: Sequence,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        let Some(work) = self.repair.as_mut() else {
            return Ok(());
        };
        let Some(author) = work.author.as_mut() else {
            // This hook sees the actual output record before its copy drains.
            // Keep the loan identity even when take_output moves the proof out.
            if work.output_started && self.output.is_some() && work.expected_correction.is_none() {
                work.expected_correction = Some(SourcePosition {
                    block_id: SourceBlockId {
                        run: self.barrier.run().into(),
                        sequence: sequence.0.into(),
                        block: 0.into(),
                    },
                    turn: self.open_turn.as_ref().map(|turn| turn.0.clone()),
                });
            }
            return Ok(());
        };
        if !author.started || author.source.is_some() {
            return Ok(());
        }
        let original = work
            .ledger
            .original()
            .or_else(|| work.account.as_ref().map(RepairAccount::original))
            .expect("author retains its original account");
        let source = SourcePosition {
            block_id: SourceBlockId {
                run: self.barrier.run().into(),
                sequence: sequence.0.into(),
                block: 0.into(),
            },
            turn: original.generation().source().turn.clone(),
        };
        author.source = Some(source);
        if author.purpose == AuthorPurpose::Instruction {
            let result = self.flow.bind_instruction_check(
                &mut FlowRecording {
                    barrier: &mut self.barrier,
                    inbox: &mut self.inbox,
                    budget: &mut self.budget,
                    author: &self.author,
                },
                sequence,
            );
            result.map_err(|error| self.flow_failed(io, error))?;
        }
        Ok(())
    }

    pub(in crate::flow) fn advance_repair<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<RepairProgress, RunFailure> {
        self.live(ports.io)?;
        if self.repair.is_none() {
            return Err(RunFailure::RepairOwned);
        }
        if self.repair.as_ref().unwrap().cancel.is_some() {
            return self.advance_cancelled_repair(ports);
        }
        if self.repair.as_ref().unwrap().refused {
            return Ok(RepairProgress::Refused);
        }
        if self.repair.as_ref().unwrap().author.is_some() {
            return self.advance_repair_author(ports);
        }
        if self.repair.as_ref().unwrap().accounting.is_some() {
            return self.advance_repair_accounting(ports);
        }
        if self.repair.as_ref().unwrap().measurement {
            return self.advance_repair_measurement(ports.io);
        }
        match self.repair.as_ref().unwrap().ledger.intent() {
            RepairIntent::Requested { source, retry_cap } => self.start_repair_measurement(
                source,
                0,
                retry_cap.0,
                trace::FlowPhase::RepairRequested,
                ports.io,
            ),
            RepairIntent::Instruction { source, attempt } => {
                self.ensure_repair_wait(ports.io)?;
                let work = self.repair.as_ref().unwrap();
                let expanded = match work.template.expand(
                    &work.certificate,
                    ExpansionIdentity {
                        run_id: work.certificate.run_id(),
                        scheme: work.certificate.scheme(),
                        checker_artifact_sha256: work.certificate.checker_artifact_sha256(),
                        inventory_sha256: work.certificate.inventory_sha256(),
                    },
                ) {
                    Ok(expanded) => expanded,
                    Err(error) => {
                        let refusal = RefusalRecord::Instruction {
                            template_id: work.template.template_id().into(),
                            case: weaver_types::state_flow::InstructionRefusal::InvalidTemplate,
                            detail: format!("certified restatement expansion refused: {error:?}"),
                        };
                        self.stage_repair_refusal(
                            refusal,
                            AuthorPurpose::InstructionRefusal,
                            ports.io,
                        )?;
                        return Ok(RepairProgress::Progress);
                    }
                };
                let message = serde_json::value::to_raw_value(expanded.message())
                    .map_err(|error| self.repair_fault(ports.io, error.to_string()))?;
                let payload = Payload::LoopInstruction(Box::new(trace::LoopInstructionRecord {
                    template_id: expanded.template_id().into(),
                    message,
                    instruction: trace::LoopInstructionMeta::Restatement {
                        source: crate::flow::mirror::source(&source),
                        attempt: trace::DecimalU64(attempt.0),
                    },
                }));
                let turn = self.repair_turn();
                let result = self.flow.reserve_instruction_check(
                    &mut FlowRecording {
                        barrier: &mut self.barrier,
                        inbox: &mut self.inbox,
                        budget: &mut self.budget,
                        author: &self.author,
                    },
                    &expanded,
                    &turn,
                );
                result.map_err(|error| self.flow_failed(ports.io, error))?;
                self.stage_repair_author(
                    AuthorPurpose::Instruction,
                    Kind::LoopInstruction,
                    AuthoredSource::Loop,
                    Some(Sequence(source.sequence.0)),
                    payload,
                    Some(expanded),
                );
                Ok(RepairProgress::Progress)
            }
            RepairIntent::AwaitingCorrection { .. } => {
                Ok(if self.repair.as_ref().unwrap().restatement.is_some() {
                    RepairProgress::Restatement
                } else {
                    RepairProgress::AwaitingCorrection
                })
            }
            RepairIntent::ProtocolRefusal { correction, .. } => {
                let refusal = RefusalRecord::ProtocolRepair {
                    source_id: correction.block_id,
                    case: ProtocolRepairCase::CorrectionCall,
                    detail: "a restatement pass attempted a call".into(),
                };
                self.stage_repair_refusal(refusal, AuthorPurpose::ProtocolRefusal, ports.io)?;
                Ok(RepairProgress::Progress)
            }
            RepairIntent::EmptyCorrectionRefusal { source, correction } => {
                self.ensure_repair_wait(ports.io)?;
                let refusal = RefusalRecord::ProtocolRepair {
                    source_id: source,
                    case: ProtocolRepairCase::EmptyCorrection,
                    detail: ProtocolRepairCase::EMPTY_CORRECTION_DETAIL.into(),
                };
                let payload = serde_json::value::to_raw_value(&refusal)
                    .map_err(|error| self.repair_fault(ports.io, error.to_string()))?;
                self.stage_repair_author(
                    AuthorPurpose::EmptyCorrectionRefusal,
                    Kind::Refusal,
                    AuthoredSource::Runtime,
                    Some(Sequence(correction.block_id.sequence.0)),
                    Payload::Refusal(payload),
                    None,
                );
                Ok(RepairProgress::Progress)
            }
            RepairIntent::CorrectionAccounting => {
                let correction = self
                    .repair
                    .as_mut()
                    .unwrap()
                    .ledger
                    .take_correction_accounting()
                    .ok_or_else(|| {
                        self.repair_fault(ports.io, "correction account has no retained loan")
                    })?;
                let work = self.repair.as_mut().unwrap();
                work.calls = correction
                    .requests()
                    .iter()
                    .map(|request| request.opening().call_id.clone())
                    .collect();
                work.correction = Some(correction);
                work.accounting = Some(Accounting::Correction);
                Ok(RepairProgress::Progress)
            }
            RepairIntent::AssociationRefusals => self.start_association_refusals(ports.io),
            RepairIntent::ExhaustedRecord {
                source,
                failed_corrections,
                retry_cap,
            } => self.start_repair_measurement(
                source,
                failed_corrections.0,
                retry_cap.0,
                trace::FlowPhase::RepairExhausted,
                ports.io,
            ),
            RepairIntent::ExhaustedAccounting => {
                let account = self
                    .repair
                    .as_mut()
                    .unwrap()
                    .ledger
                    .take_exhausted_account()
                    .ok_or_else(|| {
                        self.repair_fault(ports.io, "exhaustion account has no retained loan")
                    })?;
                let work = self.repair.as_mut().unwrap();
                work.calls = account
                    .requests()
                    .iter()
                    .map(|request| request.call_id.clone())
                    .collect();
                work.account = Some(account);
                work.accounting = Some(Accounting::Exhausted);
                Ok(RepairProgress::Progress)
            }
            RepairIntent::Complete => {
                self.finish_repair_wait();
                Ok(RepairProgress::Complete)
            }
            RepairIntent::Exhausted => {
                self.finish_repair_wait();
                Ok(RepairProgress::Exhausted)
            }
            RepairIntent::InvalidRecallGap { .. } => {
                self.finish_repair_wait();
                Ok(RepairProgress::SpecGap)
            }
            RepairIntent::Cancelled => Ok(RepairProgress::Cancelled),
            RepairIntent::Frozen | RepairIntent::Transferred => Err(RunFailure::RepairOwned),
        }
    }

    fn repair_turn(&self) -> weaver_trace::TurnRef {
        let work = self.repair.as_ref().unwrap();
        work.ledger
            .original()
            .map(|original| original.generation().original().turn.clone())
            .or_else(|| work.account.as_ref().map(|account| account.turn().clone()))
            .or_else(|| self.open_turn.clone())
            .expect("repair retains attributed turn")
    }
    fn ensure_repair_wait(&mut self, io: &mut impl PumpIo) -> Result<(), RunFailure> {
        if let Some(bound) = self.repair.as_ref().unwrap().bound {
            if self.deadlines.get(PumpDeadline::Decision) != Some(bound) {
                return Err(self.repair_fault(io, "repair lost its original wait"));
            }
            return Ok(());
        }
        let bound = if let Some(bound) = self.deadlines.get(PumpDeadline::Decision) {
            bound
        } else {
            let now = io
                .now()
                .map_err(|error| self.repair_fault(io, error.to_string()))?;
            let elapsed = self
                .load_duty
                .as_ref()
                .unwrap()
                .instruction
                .decision_wait
                .elapsed_ns
                .0;
            let bound = AbsoluteBound::after(now, elapsed)
                .map_err(|error| self.repair_fault(io, error.to_string()))?;
            self.arm(PumpDeadline::Decision, bound, io)?;
            bound
        };
        self.repair.as_mut().unwrap().bound = Some(bound);
        Ok(())
    }
    fn finish_repair_wait(&mut self) {
        if let Some(work) = self.repair.as_mut() {
            work.bound = None;
        }
        self.complete_wait(PumpDeadline::Decision);
    }
    fn start_repair_measurement(
        &mut self,
        source: SourceBlockId,
        failed: u64,
        cap: u64,
        phase: trace::FlowPhase,
        io: &mut impl PumpIo,
    ) -> Result<RepairProgress, RunFailure> {
        self.ensure_repair_wait(io)?;
        let turn = self.repair_turn().0;
        let mut transition = Transition::new(phase);
        transition.source_id = Some(source);
        transition.failed_corrections = Some(failed);
        transition.retry_cap = Some(cap);
        let result = self.flow.begin_repair_measurement(
            &mut FlowRecording {
                barrier: &mut self.barrier,
                inbox: &mut self.inbox,
                budget: &mut self.budget,
                author: &self.author,
            },
            io,
            transition,
            turn,
        );
        if result.map_err(|error| self.flow_failed(io, error))? {
            self.repair.as_mut().unwrap().measurement = true;
        }
        Ok(RepairProgress::Progress)
    }
    fn advance_repair_measurement(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<RepairProgress, RunFailure> {
        self.reduce_flow(io)?;
        let result = self.flow.take_repair_measurement(&mut FlowRecording {
            barrier: &mut self.barrier,
            inbox: &mut self.inbox,
            budget: &mut self.budget,
            author: &self.author,
        });
        let Some(recorded) = result.map_err(|error| self.flow_failed(io, error))? else {
            return Ok(RepairProgress::Pending);
        };
        let work = self.repair.as_mut().unwrap();
        work.measurement = false;
        if work.cancel.is_some() {
            return Ok(RepairProgress::Progress);
        }
        let accepted = if recorded.sample.phase == trace::FlowPhase::RepairRequested {
            work.ledger.recorded_requested(recorded.source)
        } else {
            work.ledger.recorded_exhausted(recorded.source)
        };
        accepted
            .map_err(|error| self.repair_fault(io, format!("repair measurement: {error:?}")))?;
        Ok(RepairProgress::Progress)
    }
    fn stage_repair_author(
        &mut self,
        purpose: AuthorPurpose,
        kind: Kind,
        source: AuthoredSource,
        causal_parent: Option<Sequence>,
        payload: Payload,
        expansion: Option<TemplateExpansion>,
    ) {
        let turn = self.repair_turn();
        self.repair.as_mut().unwrap().author = Some(RepairAuthor {
            purpose,
            event: Some(AuthoredEvent {
                kind,
                subsystem: weaver_trace::Subsystem::Harness,
                turn: Some(turn),
                source,
                causal_parent,
                payload: Some(payload),
            }),
            expansion,
            source: None,
            started: false,
        });
    }
    fn stage_repair_refusal(
        &mut self,
        refusal: RefusalRecord,
        purpose: AuthorPurpose,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        let payload = serde_json::value::to_raw_value(&refusal)
            .map_err(|error| self.repair_fault(io, error.to_string()))?;
        if purpose == AuthorPurpose::InstructionRefusal {
            self.repair.as_mut().unwrap().refusal_record = Some(refusal);
        }
        self.stage_repair_author(
            purpose,
            Kind::Refusal,
            AuthoredSource::Runtime,
            None,
            Payload::Refusal(payload),
            None,
        );
        Ok(())
    }

    fn advance_repair_author<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<RepairProgress, RunFailure> {
        if self
            .repair
            .as_ref()
            .unwrap()
            .author
            .as_ref()
            .unwrap()
            .source
            .is_none()
        {
            let started = self
                .repair
                .as_ref()
                .unwrap()
                .author
                .as_ref()
                .unwrap()
                .started;
            if !started
                && (self.flow.has_pending_record()
                    || self.barrier.pending().is_some()
                    || self.barrier.pending_author().is_some()
                    || self.waiting_content.is_some()
                    || self.completed_author.is_some())
            {
                self.reduce_flow(ports.io)?;
                return Ok(RepairProgress::Pending);
            }
            self.repair.as_mut().unwrap().internal = true;
            let result = if started {
                self.resume_author_and_wait(ports)
            } else {
                let author = self.repair.as_mut().unwrap().author.as_mut().unwrap();
                author.started = true;
                let event = author.event.take().expect("staged repair event");
                self.author_and_wait(ports, event)
            };
            self.repair.as_mut().unwrap().internal = false;
            result?;
        }
        let purpose = self
            .repair
            .as_ref()
            .unwrap()
            .author
            .as_ref()
            .unwrap()
            .purpose;
        if purpose == AuthorPurpose::Instruction {
            self.reduce_flow(ports.io)?;
            let result = self.flow.take_instruction_check(&mut FlowRecording {
                barrier: &mut self.barrier,
                inbox: &mut self.inbox,
                budget: &mut self.budget,
                author: &self.author,
            });
            let Some(check) = result.map_err(|error| self.flow_failed(ports.io, error))? else {
                return Ok(RepairProgress::Pending);
            };
            let expected = self
                .repair
                .as_ref()
                .unwrap()
                .author
                .as_ref()
                .unwrap()
                .source
                .as_ref()
                .unwrap();
            let instruction = match &check {
                InstructionCheck::Unselected { source } => source,
                InstructionCheck::Checked(check) => check.instruction(),
            };
            if instruction != expected {
                return Err(self.repair_fault(ports.io, "instruction check names another author"));
            }
            if let InstructionCheck::Checked(check) = check
                && matches!(check.reply().result, Ste100Result::NotSte100 { .. })
            {
                let account = serde_json::value::to_raw_value(&serde_json::json!({
                    "template_id": self.repair.as_ref().unwrap().template.template_id(),
                    "source_id": check.instruction().block_id,
                    "result_source": check.source(), "result": check.reply(),
                }))
                .map_err(|error| self.repair_fault(ports.io, error.to_string()))?;
                let fault = FaultReport {
                    case: FaultCase::StateTemplateRejected,
                    account,
                };
                let payload = serde_json::value::to_raw_value(&fault)
                    .map_err(|error| self.repair_fault(ports.io, error.to_string()))?;
                self.repair.as_mut().unwrap().rejection = Some(check);
                self.stage_repair_author(
                    AuthorPurpose::TemplateFault,
                    Kind::Fault,
                    AuthoredSource::Runtime,
                    None,
                    Payload::Fault(payload),
                    None,
                );
                return Ok(RepairProgress::Progress);
            }
        }
        // A recorded instruction is still ordinary selected content. Even
        // cancellation must observe its exact runtime check and report a
        // template defect. It needs no File receipt and grants no restatement.
        if self.repair.as_ref().unwrap().cancel.is_some() {
            self.repair.as_mut().unwrap().author = None;
            return Ok(RepairProgress::Progress);
        }
        let author = self.repair.as_mut().unwrap().author.take().unwrap();
        let source = author.source.expect("actual recorded author");
        let result = match purpose {
            AuthorPurpose::Instruction => {
                let expanded = author.expansion.expect("certified instruction retained");
                let result = self
                    .repair
                    .as_mut()
                    .unwrap()
                    .ledger
                    .recorded_instruction(expanded, source);
                match result {
                    Ok(request) => {
                        self.repair.as_mut().unwrap().restatement = Some(request);
                        self.finish_repair_wait();
                        return Ok(RepairProgress::Restatement);
                    }
                    Err((expanded, error)) => {
                        self.repair.as_mut().unwrap().author = Some(RepairAuthor {
                            purpose,
                            event: None,
                            expansion: Some(expanded),
                            source: None,
                            started: true,
                        });
                        Err(error)
                    }
                }
            }
            AuthorPurpose::ProtocolRefusal => self
                .repair
                .as_mut()
                .unwrap()
                .ledger
                .recorded_protocol_refusal(source),
            AuthorPurpose::EmptyCorrectionRefusal => self
                .repair
                .as_mut()
                .unwrap()
                .ledger
                .recorded_empty_correction_refusal(source),
            AuthorPurpose::Association => {
                let work = self.repair.as_mut().unwrap();
                work.association_next += 1;
                work.association_last = Some(source);
                if work.association_next == work.associations.len() {
                    work.ledger
                        .recorded_association_refusals(work.association_last.take().unwrap())
                } else {
                    Ok(())
                }
            }
            AuthorPurpose::TemplateFault | AuthorPurpose::InstructionRefusal => {
                let work = self.repair.as_mut().unwrap();
                work.refused = true;
                work.refusal_source = Some(source);
                self.finish_repair_wait();
                return Ok(RepairProgress::Refused);
            }
        };
        result.map_err(|error| {
            self.repair_fault(ports.io, format!("recorded repair event: {error:?}"))
        })?;
        Ok(RepairProgress::Progress)
    }

    fn start_association_refusals(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<RepairProgress, RunFailure> {
        let work = self.repair.as_mut().unwrap();
        if work.associations.is_empty() {
            for refusal in work.ledger.original_refusals() {
                if let AssociationRefusal::External {
                    source,
                    case,
                    detail,
                    ..
                } = refusal
                {
                    work.associations.push(RefusalRecord::ProtocolRepair {
                        source_id: source.clone(),
                        case: *case,
                        detail: (*detail).into(),
                    });
                }
            }
            for refusal in work.ledger.associations() {
                work.associations.push(RefusalRecord::ProtocolRepair {
                    source_id: refusal.source().clone(),
                    case: refusal.case(),
                    detail: "repaired external send requires exactly one non-recall statement"
                        .into(),
                });
            }
        }
        let Some(refusal) = work.associations.get(work.association_next).cloned() else {
            return Err(self.repair_fault(io, "association frontier has no typed refusal"));
        };
        self.stage_repair_refusal(refusal, AuthorPurpose::Association, io)?;
        Ok(RepairProgress::Progress)
    }

    fn advance_repair_accounting<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<RepairProgress, RunFailure> {
        self.ensure_repair_wait(ports.io)?;
        let kind = self.repair.as_ref().unwrap().accounting.unwrap();
        let cause = self
            .repair
            .as_ref()
            .unwrap()
            .cancel
            .unwrap_or_else(|| match kind {
                Accounting::Correction => NonDispatchCause::PassPurpose,
                Accounting::Exhausted => NonDispatchCause::RepairExhausted,
                Accounting::CancelCorrection | Accounting::CancelOriginal => {
                    unreachable!("cancellation cause retained")
                }
            });
        if !self.repair.as_ref().unwrap().pair_started {
            self.repair.as_mut().unwrap().internal = true;
            let result = if matches!(kind, Accounting::Correction | Accounting::CancelCorrection) {
                let correction = self.repair.as_mut().unwrap().correction.take().unwrap();
                match self.begin_not_dispatched(correction, cause, ports.io) {
                    Ok(()) => Ok(()),
                    Err(error) => {
                        self.repair.as_mut().unwrap().correction = Some(error.original);
                        if error.failure.is_none() && error.refusal == PairBeginRefusal::Busy {
                            Err(None)
                        } else {
                            Err(Some(error.failure.unwrap_or_else(|| {
                                self.repair_fault(
                                    ports.io,
                                    format!("correction pair intake: {:?}", error.refusal),
                                )
                            })))
                        }
                    }
                }
            } else {
                let account = self.repair.as_mut().unwrap().account.take().unwrap();
                match self.begin_repair_not_dispatched(account, cause, ports.io) {
                    Ok(()) => Ok(()),
                    Err(error) => {
                        self.repair.as_mut().unwrap().account = Some(error.account);
                        if error.failure.is_none() && error.refusal == PairBeginRefusal::Busy {
                            Err(None)
                        } else {
                            Err(Some(error.failure.unwrap_or_else(|| {
                                self.repair_fault(
                                    ports.io,
                                    format!("repair pair intake: {:?}", error.refusal),
                                )
                            })))
                        }
                    }
                }
            };
            self.repair.as_mut().unwrap().internal = false;
            match result {
                Ok(()) => {
                    self.repair.as_mut().unwrap().pair_started = true;
                    return Ok(RepairProgress::Progress);
                }
                Err(Some(error)) => return Err(error),
                Err(None) => {
                    self.reduce_flow(ports.io)?;
                    return Ok(RepairProgress::Pending);
                }
            }
        }
        self.repair.as_mut().unwrap().internal = true;
        if self.repair.as_ref().unwrap().cancel.is_some() {
            let result = self.bar_pending_pairs(cause, ports.io);
            if let Err(error) = result {
                self.repair.as_mut().unwrap().internal = false;
                return Err(error);
            }
        }
        let result = self.advance_pairs(ports);
        self.repair.as_mut().unwrap().internal = false;
        if result? != PairProgress::Complete {
            return Ok(RepairProgress::Pending);
        }
        self.repair.as_mut().unwrap().internal = true;
        let correction = if matches!(kind, Accounting::Correction | Accounting::CancelCorrection) {
            let result = self.take_pair_accounting(ports.io);
            self.repair.as_mut().unwrap().internal = false;
            Some(result?.expect("completed original pair account"))
        } else {
            None
        };
        let account = if correction.is_none() {
            let result = self.take_repair_accounting(ports.io);
            self.repair.as_mut().unwrap().internal = false;
            Some(result?.expect("completed repair pair account"))
        } else {
            None
        };
        // CompletedPair capabilities remain in the pair owner. The repair owner
        // checks exact identities and causes before accepting its custody back.
        let calls = self.repair.as_ref().unwrap().calls.clone();
        let proofs = calls
            .iter()
            .map(|call| self.pairs.completed(call).ok().flatten())
            .collect::<Option<Vec<_>>>();
        let Some(proofs) = proofs else {
            self.repair.as_mut().unwrap().correction = correction;
            self.repair.as_mut().unwrap().account = account;
            return Err(self.repair_fault(ports.io, "finished pair account lacks a terminal proof"));
        };
        let work = self.repair.as_mut().unwrap();
        let result = match kind {
            Accounting::Correction => match work
                .ledger
                .finish_correction_accounting(correction.unwrap(), &proofs)
            {
                Ok(_) => Ok(()),
                Err((correction, error)) => {
                    work.correction = Some(correction);
                    Err(error)
                }
            },
            Accounting::Exhausted => match work
                .ledger
                .finish_exhausted_account(account.unwrap(), &proofs)
            {
                Ok(()) => Ok(()),
                Err((account, error)) => {
                    work.account = Some(account);
                    Err(error)
                }
            },
            Accounting::CancelCorrection => {
                work.correction = correction;
                Ok(())
            }
            Accounting::CancelOriginal => {
                work.account = account;
                Ok(())
            }
        };
        work.accounting = None;
        work.pair_started = false;
        work.calls.clear();
        result.map_err(|error| {
            self.repair_fault(ports.io, format!("completed repair accounting: {error:?}"))
        })?;
        Ok(RepairProgress::Progress)
    }

    pub(in crate::flow) fn take_restatement(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<Option<RestatementRequest>, RunFailure> {
        self.live(io)?;
        let Some(work) = self.repair.as_mut() else {
            return Ok(None);
        };
        if work.cancel.is_some() || work.refused {
            return Ok(None);
        }
        let request = work.restatement.take();
        if let Some(request) = &request {
            work.issued = true;
            work.output_started = false;
            work.issued_source = Some(request.source().clone());
            work.expected_correction = None;
        }
        Ok(request)
    }

    #[allow(clippy::result_large_err)]
    pub(in crate::flow) fn observe_correction(
        &mut self,
        source: &SourceBlockId,
        correction: OriginalRequests,
        io: &mut impl PumpIo,
    ) -> Result<CorrectionProgress, RepairObserveError> {
        if let Err(failure) = self.live(io) {
            return Err(RepairObserveError {
                correction,
                error: RepairError::Frozen,
                failure: Some(failure),
            });
        }
        let valid = self.repair.as_ref().is_some_and(|work| {
            work.issued
                && work.output_started
                && !work.refused
                && work.issued_source.as_ref() == Some(source)
                && work.expected_correction.as_ref() == Some(correction.generation().source())
        }) && self.output.is_none()
            && self.pair_custody_matches(&correction)
            && correction.generation().scheme() == &self.declaration.scheme
            && correction.generation().source().block_id.run == self.barrier.run()
            && self.open_turn.as_ref() == Some(&correction.generation().original().turn);
        if !valid {
            return Err(RepairObserveError {
                correction,
                error: RepairError::WrongPhase,
                failure: None,
            });
        }
        if self.repair.as_ref().unwrap().cancel.is_some() {
            let work = self.repair.as_mut().unwrap();
            work.correction = Some(correction);
            work.issued = false;
            work.output_started = false;
            work.issued_source = None;
            work.expected_correction = None;
            return Ok(CorrectionProgress::Cancelled);
        }
        let result = self
            .repair
            .as_mut()
            .unwrap()
            .ledger
            .observe_correction(source, correction);
        match result {
            Ok(progress) => {
                let work = self.repair.as_mut().unwrap();
                work.issued = false;
                work.output_started = false;
                work.issued_source = None;
                work.expected_correction = None;
                work.bound = self.deadlines.get(PumpDeadline::Decision);
                Ok(progress)
            }
            Err((correction, error)) => Err(RepairObserveError {
                correction,
                error,
                failure: None,
            }),
        }
    }

    pub(in crate::flow) fn take_effective_emission(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<Option<EffectiveEmission>, RunFailure> {
        self.live(io)?;
        let Some(work) = self.repair.as_mut() else {
            return Ok(None);
        };
        if work.cancel.is_some() || work.refused {
            return Ok(None);
        }
        let result = work
            .ledger
            .take_effective()
            .map_err(|error| self.repair_fault(io, format!("effective emission: {error:?}")))?;
        if result.is_some() {
            self.finish_repair_wait();
            self.retire_repair_owner();
        }
        Ok(result)
    }
    pub(in crate::flow) fn take_repair_exhausted(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<Option<RepairExhausted>, RunFailure> {
        self.live(io)?;
        let Some(work) = self.repair.as_mut() else {
            return Ok(None);
        };
        if work.cancel.is_some() || work.refused {
            return Ok(None);
        }
        let result = work.ledger.take_exhausted();
        if result.is_some() {
            self.finish_repair_wait();
            self.retire_repair_owner();
        }
        Ok(result)
    }

    pub(in crate::flow) fn take_refused_repair(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<Option<RepairRefused>, RunFailure> {
        self.live(io)?;
        let Some(work) = self.repair.as_mut() else {
            return Ok(None);
        };
        if !work.refused || work.cancel.is_some() {
            return Ok(None);
        }
        // This state has no outstanding correction or pair-accounting loan.
        // Cancelling the passive repair owner only transfers its custody. It
        // does not choose any pair disposition or emit a terminal pair record.
        work.ledger.cancel();
        let Some(cancelled) = work.ledger.take_cancelled() else {
            return Ok(None);
        };
        let (account, pending) = cancelled.into_parts();
        let source = work
            .refusal_source
            .take()
            .expect("refusal follows its actual record");
        let evidence = if let Some(check) = work.rejection.take() {
            RepairRefusalEvidence::RuntimeTemplate {
                fault: source,
                check,
            }
        } else {
            RepairRefusalEvidence::Instruction {
                source,
                record: work
                    .refusal_record
                    .take()
                    .expect("structural refusal retains its exact record"),
            }
        };
        let refused = RepairRefused {
            account,
            pending,
            evidence,
        };
        self.finish_repair_wait();
        self.retire_repair_owner();
        Ok(Some(refused))
    }

    pub(in crate::flow) fn cancel_repair(
        &mut self,
        cause: NonDispatchCause,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        self.live(io)?;
        if !matches!(cause, NonDispatchCause::Stop | NonDispatchCause::Unload) {
            return Err(RunFailure::RepairOwned);
        }
        let Some(work) = self.repair.as_mut() else {
            return Err(RunFailure::RepairOwned);
        };
        if work.cancel != Some(NonDispatchCause::Unload) {
            work.cancel = Some(cause);
        }
        work.ledger.cancel();
        work.restatement = None;
        if work
            .author
            .as_ref()
            .is_some_and(|author| author.purpose == AuthorPurpose::Instruction && !author.started)
        {
            work.author = None;
            self.flow.cancel_instruction_check();
        }
        Ok(())
    }

    fn advance_cancelled_repair<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<RepairProgress, RunFailure> {
        if self.repair.as_ref().unwrap().cancelled.is_some() {
            return Ok(RepairProgress::Cancelled);
        }
        if self.output.is_some() {
            // The decoder already supplied this entire Message. Finish its
            // checks, retain its lease and account its calls. Cancelling the
            // output observer here would erase known accepted sends.
            self.repair.as_mut().unwrap().internal = true;
            let result = self.advance_output(ports);
            self.repair.as_mut().unwrap().internal = false;
            if result? != OutputProgress::Complete {
                return Ok(RepairProgress::Pending);
            }
            let Some(checked) = self.take_output(ports.io)? else {
                return Ok(RepairProgress::Pending);
            };
            self.repair.as_mut().unwrap().held_output = Some(checked);
            let scheme = RequestScheme::from_exact_scheme(
                &self.load_duty.as_ref().unwrap().scheme_text,
                &self.declaration.scheme,
            )
            .map_err(|error| {
                self.repair_fault(ports.io, format!("loaded request scheme: {error:?}"))
            })?;
            let checked = self.repair.as_mut().unwrap().held_output.take().unwrap();
            let correction = match OriginalRequests::from_checked(checked, &scheme) {
                Ok(correction) => correction,
                Err(failure) => {
                    self.repair.as_mut().unwrap().held_output = Some(failure.generation);
                    return Err(self.repair_fault(
                        ports.io,
                        format!("cancelled correction association: {:?}", failure.error),
                    ));
                }
            };
            let source = self
                .repair
                .as_ref()
                .unwrap()
                .issued_source
                .clone()
                .expect("correction loan has a source");
            if let Err(error) = self.observe_correction(&source, correction, ports.io) {
                self.repair.as_mut().unwrap().correction = Some(error.correction);
                return Err(error.failure.unwrap_or_else(|| {
                    self.repair_fault(
                        ports.io,
                        format!("cancelled correction custody: {:?}", error.error),
                    )
                }));
            }
        }
        // A caller may hold the complete checked proof between take_output and
        // observe_correction. Cancellation cannot report complete before that
        // exact loan returns, and keeps the same Decision bound while it waits.
        if self.repair.as_ref().unwrap().output_started {
            return Ok(RepairProgress::Pending);
        }
        if self.repair.as_ref().unwrap().author.is_some() {
            return self.advance_repair_author(ports);
        }
        if self.repair.as_ref().unwrap().measurement {
            return self.advance_repair_measurement(ports.io);
        }
        if self.repair.as_ref().unwrap().accounting.is_some() {
            return self.advance_repair_accounting(ports);
        }
        if self.repair.as_ref().unwrap().account.is_none() {
            let cancelled = self
                .repair
                .as_mut()
                .unwrap()
                .ledger
                .take_cancelled()
                .ok_or_else(|| {
                    self.repair_fault(ports.io, "cancelled repair lost its original account")
                })?;
            let (account, pending) = cancelled.into_parts();
            let work = self.repair.as_mut().unwrap();
            work.account = Some(account);
            if let Some(pending) = pending {
                debug_assert!(work.correction.is_none());
                work.correction = Some(pending);
            }
        }
        if let Some(correction) = self.repair.as_ref().unwrap().correction.as_ref() {
            // A pending forbidden pass has not yet entered pair accounting.
            // Once returned from cancellation accounting, keep it as evidence.
            let unsettled = correction.requests().iter().any(|request| {
                self.pairs
                    .completed(&request.opening().call_id)
                    .ok()
                    .flatten()
                    .is_none()
            });
            if unsettled {
                let work = self.repair.as_mut().unwrap();
                work.calls = work
                    .correction
                    .as_ref()
                    .unwrap()
                    .requests()
                    .iter()
                    .map(|request| request.opening().call_id.clone())
                    .collect();
                work.accounting = Some(Accounting::CancelCorrection);
                return Ok(RepairProgress::Progress);
            }
        }
        let unsettled = self
            .repair
            .as_ref()
            .unwrap()
            .account
            .as_ref()
            .unwrap()
            .requests()
            .iter()
            .any(|request| {
                self.pairs
                    .completed(&request.call_id)
                    .ok()
                    .flatten()
                    .is_none()
            });
        if unsettled {
            let work = self.repair.as_mut().unwrap();
            work.calls = work
                .account
                .as_ref()
                .unwrap()
                .requests()
                .iter()
                .map(|request| request.call_id.clone())
                .collect();
            work.accounting = Some(Accounting::CancelOriginal);
            return Ok(RepairProgress::Progress);
        }
        let work = self.repair.as_mut().unwrap();
        work.cancelled = Some(RepairCancelled {
            account: work.account.take().unwrap(),
            pending: work.correction.take(),
            incomplete_output: work.incomplete_output.take(),
        });
        self.finish_repair_wait();
        Ok(RepairProgress::Cancelled)
    }

    pub(in crate::flow) fn take_cancelled_repair(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<Option<RepairCancelled>, RunFailure> {
        self.live(io)?;
        let result = self.repair.as_mut().and_then(|work| work.cancelled.take());
        if result.is_some() {
            self.finish_repair_wait();
            self.retire_repair_owner();
        }
        Ok(result)
    }
    fn retire_repair_owner(&mut self) {
        let owner = self.repair.take().expect("completed repair still owned");
        debug_assert!(self.repair_certificate.is_none());
        self.repair_certificate = Some(owner.certificate);
    }
    pub(in crate::flow) fn take_repair_certificate(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<Option<TemplateCertificate>, RunFailure> {
        self.live(io)?;
        Ok(self.repair_certificate.take())
    }
    fn repair_fault(&mut self, io: &mut impl PumpIo, detail: impl Into<String>) -> RunFailure {
        self.fail(
            io,
            FlowHaltReason::ProtocolFailed,
            HaltPoint::Barrier,
            detail,
        )
    }
}
