//! Passive ownership of a complete generation and its recorded repair attempts.
//! The context records intents and supplies checked corrections. This owner never
//! calls a decoder, opens a socket, dispatches a request, or fabricates a receipt.

use std::sync::Arc;

use weaver_trace::TurnRef;
use weaver_types::state_flow::{
    CallId, DecimalU64, NonDispatchCause, PairFailure, PairLane, PairStarted, ProtocolRepairCase,
    RecallRequestRefusal, SchemeDeclaration, SourceBlockId, SourcePosition, StatementRef,
    Ste100Result, Ste100Target, TerminalPairFate,
};
use weaver_types::{Finish, RefusalRecord};

use super::pairs::CompletedPair;
use crate::flow::coordinator::{AssociationRefusal, CheckedGeneration, OriginalRequests};
use crate::flow::templates::TemplateExpansion;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum RepairError {
    InvalidCap,
    WrongPhase,
    Frozen,
    WrongSource,
    WrongScheme,
    WrongRun,
    WrongTurn,
    OutOfOrder,
    InvalidExpansion,
    WrongAccounting,
    InvalidStatement,
    OutstandingCorrectionLoan,
    OutstandingAccountLoan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum RepairIntent {
    Requested {
        source: SourceBlockId,
        retry_cap: DecimalU64,
    },
    Instruction {
        source: SourceBlockId,
        attempt: DecimalU64,
    },
    AwaitingCorrection {
        source: SourceBlockId,
        attempt: DecimalU64,
    },
    ProtocolRefusal {
        source: SourceBlockId,
        correction: SourcePosition,
    },
    CorrectionAccounting,
    AssociationRefusals,
    ExhaustedRecord {
        source: SourceBlockId,
        failed_corrections: DecimalU64,
        retry_cap: DecimalU64,
    },
    ExhaustedAccounting,
    Complete,
    Exhausted,
    EmptyCorrectionRefusal {
        source: SourceBlockId,
        correction: SourcePosition,
    },
    RecallRefusal(RecallRequestRefusal),
    Cancelled,
    Frozen,
    Transferred,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum CorrectionProgress {
    Resolved,
    Retry,
    ProtocolRefusal,
    Exhausted,
    EmptyCorrectionRefusal,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::flow) struct RepairAssociationRefusal {
    call_id: CallId,
    call_block: u64,
    source: SourceBlockId,
    case: ProtocolRepairCase,
}
impl RepairAssociationRefusal {
    pub(in crate::flow) fn call_id(&self) -> &CallId {
        &self.call_id
    }
    pub(in crate::flow) fn call_block(&self) -> u64 {
        self.call_block
    }
    pub(in crate::flow) fn source(&self) -> &SourceBlockId {
        &self.source
    }
    pub(in crate::flow) fn case(&self) -> ProtocolRepairCase {
        self.case
    }
}

/// Read-only evidence installed by the owner at its exact Recorded boundary.
#[derive(Debug)]
pub(in crate::flow) struct RecordedRecallRefusal {
    refusal: RecallRequestRefusal,
    sequence: DecimalU64,
}
impl RecordedRecallRefusal {
    pub(in crate::flow) fn refusal(&self) -> &RecallRequestRefusal {
        &self.refusal
    }
    pub(in crate::flow) fn sequence(&self) -> DecimalU64 {
        self.sequence
    }
}

/// Actual association records survive a partial batch and cancellation.
#[derive(Debug)]
pub(in crate::flow) struct RecordedAssociationRefusal {
    refusal: RefusalRecord,
    source: SourcePosition,
}
impl RecordedAssociationRefusal {
    pub(in crate::flow) fn refusal(&self) -> &RefusalRecord {
        &self.refusal
    }
    pub(in crate::flow) fn source(&self) -> &SourcePosition {
        &self.source
    }
}

/// An empty pass counts only after its attributed refusal is Recorded.
#[derive(Debug)]
pub(in crate::flow) struct RecordedEmptyCorrection {
    correction: SourcePosition,
    source: SourcePosition,
    failed_corrections: u32,
}
impl RecordedEmptyCorrection {
    pub(in crate::flow) fn correction(&self) -> &SourcePosition {
        &self.correction
    }
    pub(in crate::flow) fn source(&self) -> &SourcePosition {
        &self.source
    }
    pub(in crate::flow) fn failed_corrections(&self) -> u32 {
        self.failed_corrections
    }
}

/// One charged pass, attributed to its rejected source and exact correction.
#[derive(Debug)]
pub(in crate::flow) struct RecordedFailedCorrection {
    source: SourceBlockId,
    correction: SourcePosition,
    failed_corrections: u32,
}
impl RecordedFailedCorrection {
    pub(in crate::flow) fn source(&self) -> &SourceBlockId {
        &self.source
    }
    pub(in crate::flow) fn correction(&self) -> &SourcePosition {
        &self.correction
    }
    pub(in crate::flow) fn failed_corrections(&self) -> u32 {
        self.failed_corrections
    }
}

/// The original and every correction keep their exact source leases alive.
/// Requests are derived by this owner only, never accepted from a caller vector.
/// Failed-pass identities/counts survive cancelled, exhausted and successful exits.
#[derive(Debug)]
pub(in crate::flow) struct RepairAccount {
    original: OriginalRequests,
    corrections: Vec<OriginalRequests>,
    requests: Vec<PairStarted>,
    recall_refusals: Vec<RecordedRecallRefusal>,
    association_refusals: Vec<RecordedAssociationRefusal>,
    empty_corrections: Vec<RecordedEmptyCorrection>,
    failed_corrections: Vec<RecordedFailedCorrection>,
    nonce: Arc<()>,
}
impl RepairAccount {
    pub(in crate::flow) fn original(&self) -> &OriginalRequests {
        &self.original
    }
    pub(in crate::flow) fn generations(&self) -> impl Iterator<Item = &CheckedGeneration> {
        std::iter::once(self.original.generation())
            .chain(self.corrections.iter().map(OriginalRequests::generation))
    }
    pub(in crate::flow) fn requests(&self) -> &[PairStarted] {
        &self.requests
    }
    pub(in crate::flow) fn source(&self) -> &SourcePosition {
        self.original.generation().source()
    }
    pub(in crate::flow) fn scheme(&self) -> &SchemeDeclaration {
        self.original.generation().scheme()
    }
    pub(in crate::flow) fn turn(&self) -> &TurnRef {
        &self.original.generation().original().turn
    }
    pub(in crate::flow) fn corrections(&self) -> &[OriginalRequests] {
        &self.corrections
    }
    pub(in crate::flow) fn recall_refusals(&self) -> &[RecordedRecallRefusal] {
        &self.recall_refusals
    }
    pub(in crate::flow) fn association_refusals(&self) -> &[RecordedAssociationRefusal] {
        &self.association_refusals
    }
    pub(in crate::flow) fn empty_corrections(&self) -> &[RecordedEmptyCorrection] {
        &self.empty_corrections
    }
    pub(in crate::flow) fn failed_corrections(&self) -> &[RecordedFailedCorrection] {
        &self.failed_corrections
    }
}

#[derive(Debug)]
pub(in crate::flow) struct RestatementRequest {
    source: SourceBlockId,
    attempt: DecimalU64,
    instruction: TemplateExpansion,
    recorded: SourcePosition,
}
impl RestatementRequest {
    pub(in crate::flow) fn source(&self) -> &SourceBlockId {
        &self.source
    }
    pub(in crate::flow) fn attempt(&self) -> DecimalU64 {
        self.attempt
    }
    pub(in crate::flow) fn instruction(&self) -> &TemplateExpansion {
        &self.instruction
    }
    pub(in crate::flow) fn recorded(&self) -> &SourcePosition {
        &self.recorded
    }
}

#[derive(Debug)]
pub(in crate::flow) struct EffectiveEmission {
    source: SourcePosition,
    finish: Finish,
    statements: Vec<StatementRef>,
    requests: Vec<PairStarted>,
    text: String,
    attempted_call: bool,
    account: RepairAccount,
}
impl EffectiveEmission {
    pub(super) fn account(&self) -> &RepairAccount {
        &self.account
    }
    pub(in crate::flow) fn source(&self) -> &SourcePosition {
        &self.source
    }
    pub(in crate::flow) fn finish(&self) -> &Finish {
        &self.finish
    }
    pub(in crate::flow) fn statements(&self) -> &[StatementRef] {
        &self.statements
    }
    pub(in crate::flow) fn requests(&self) -> &[PairStarted] {
        &self.requests
    }
    pub(in crate::flow) fn text(&self) -> &str {
        &self.text
    }
    pub(in crate::flow) fn attempted_call(&self) -> bool {
        self.attempted_call
    }
    pub(in crate::flow) fn into_account(self) -> RepairAccount {
        self.account
    }
}

#[derive(Debug)]
pub(in crate::flow) struct RepairExhausted {
    source: SourceBlockId,
    failed_corrections: DecimalU64,
    retry_cap: DecimalU64,
    account: RepairAccount,
}
impl RepairExhausted {
    pub(in crate::flow) fn source(&self) -> &SourceBlockId {
        &self.source
    }
    pub(in crate::flow) fn failed_corrections(&self) -> DecimalU64 {
        self.failed_corrections
    }
    pub(in crate::flow) fn retry_cap(&self) -> DecimalU64 {
        self.retry_cap
    }
    pub(in crate::flow) fn into_account(self) -> RepairAccount {
        self.account
    }
}

#[derive(Debug)]
pub(in crate::flow) struct RepairCancellation {
    account: RepairAccount,
    pending: Option<OriginalRequests>,
}
impl RepairCancellation {
    pub(super) fn account(&self) -> &RepairAccount {
        &self.account
    }
    pub(super) fn into_parts(self) -> (RepairAccount, Option<OriginalRequests>) {
        (self.account, self.pending)
    }
    pub(in crate::flow) fn pending(&self) -> Option<&OriginalRequests> {
        self.pending.as_ref()
    }
}

#[derive(Debug)]
struct RepairContext {
    source: SourceBlockId,
    failed: u32,
    success: Option<usize>,
    instructions: Vec<SourcePosition>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Requested,
    Instruction,
    Awaiting,
    ProtocolRefusal,
    CorrectionAccounting,
    CorrectionOut,
    Association,
    ExhaustedRecord,
    ExhaustedAccounting,
    ExhaustedOut,
    Complete,
    Exhausted,
    EmptyRefusal,
    Cancelled,
    Frozen,
    Transferred,
}

#[derive(Debug)]
pub(super) struct RepairLedger {
    account: Option<RepairAccount>,
    contexts: Vec<RepairContext>,
    current: usize,
    cap: u32,
    stage: Stage,
    frontier: u64,
    pending: Option<OriginalRequests>,
    pending_protocol_recorded: bool,
    correction_out: Option<SourcePosition>,
    account_out: Option<Arc<()>>,
    associations: Vec<RepairAssociationRefusal>,
    association_recorded: bool,
    original_refusals_recorded: bool,
}

impl RepairLedger {
    #[allow(clippy::result_large_err)]
    pub(super) fn begin(
        original: OriginalRequests,
        cap: u32,
    ) -> Result<Self, (OriginalRequests, RepairError)> {
        if cap == 0 {
            return Err((original, RepairError::InvalidCap));
        }
        let contexts = original
            .generation()
            .blocks()
            .iter()
            .filter_map(|check| {
                if !matches!(check.reply().result, Ste100Result::NotSte100 { .. }) {
                    return None;
                }
                let Ste100Target::Source(source) = &check.request().target else {
                    unreachable!("checked model source");
                };
                Some(RepairContext {
                    source: source.clone(),
                    failed: 0,
                    success: None,
                    instructions: vec![],
                })
            })
            .collect::<Vec<_>>();
        let frontier = generation_frontier(original.generation());
        let requests = original
            .requests()
            .iter()
            .map(|candidate| candidate.opening().clone())
            .collect();
        let original_refusals_recorded = !original
            .refusals()
            .iter()
            .any(|refusal| matches!(refusal, AssociationRefusal::External { .. }));
        let stage = if contexts.is_empty() {
            if !original_refusals_recorded {
                Stage::Association
            } else {
                Stage::Complete
            }
        } else {
            Stage::Requested
        };
        Ok(Self {
            account: Some(RepairAccount {
                original,
                corrections: vec![],
                requests,
                recall_refusals: vec![],
                association_refusals: vec![],
                empty_corrections: vec![],
                failed_corrections: vec![],
                nonce: Arc::new(()),
            }),
            contexts,
            current: 0,
            cap,
            stage,
            frontier,
            pending: None,
            pending_protocol_recorded: false,
            correction_out: None,
            account_out: None,
            associations: vec![],
            association_recorded: true,
            original_refusals_recorded,
        })
    }

    pub(super) fn intent(&self) -> RepairIntent {
        if !matches!(
            self.stage,
            Stage::Frozen | Stage::Transferred | Stage::Cancelled
        ) && let Some(refusal) = self.next_recall_refusal()
        {
            return RepairIntent::RecallRefusal(refusal);
        }
        match self.stage {
            Stage::Requested => RepairIntent::Requested {
                source: self.context().source.clone(),
                retry_cap: u64::from(self.cap).into(),
            },
            Stage::Instruction => RepairIntent::Instruction {
                source: self.context().source.clone(),
                attempt: self.attempt(),
            },
            Stage::Awaiting => RepairIntent::AwaitingCorrection {
                source: self.context().source.clone(),
                attempt: self.attempt(),
            },
            Stage::ProtocolRefusal => RepairIntent::ProtocolRefusal {
                source: self.context().source.clone(),
                correction: self.pending.as_ref().unwrap().generation().source().clone(),
            },
            Stage::CorrectionAccounting | Stage::CorrectionOut => {
                RepairIntent::CorrectionAccounting
            }
            Stage::Association => RepairIntent::AssociationRefusals,
            Stage::ExhaustedRecord => RepairIntent::ExhaustedRecord {
                source: self.context().source.clone(),
                failed_corrections: u64::from(self.context().failed).into(),
                retry_cap: u64::from(self.cap).into(),
            },
            Stage::ExhaustedAccounting | Stage::ExhaustedOut => RepairIntent::ExhaustedAccounting,
            Stage::Complete => RepairIntent::Complete,
            Stage::Exhausted => RepairIntent::Exhausted,
            Stage::EmptyRefusal => RepairIntent::EmptyCorrectionRefusal {
                source: self.context().source.clone(),
                correction: self.pending.as_ref().unwrap().generation().source().clone(),
            },
            Stage::Cancelled => RepairIntent::Cancelled,
            Stage::Frozen => RepairIntent::Frozen,
            Stage::Transferred => RepairIntent::Transferred,
        }
    }

    pub(super) fn original(&self) -> Option<&OriginalRequests> {
        self.account.as_ref().map(|account| &account.original)
    }
    #[cfg(test)]
    pub(super) fn associations(&self) -> &[RepairAssociationRefusal] {
        &self.associations
    }
    #[cfg(test)]
    pub(super) fn original_refusals(&self) -> &[AssociationRefusal] {
        self.account
            .as_ref()
            .map_or(&[], |account| account.original.refusals())
    }
    pub(super) fn pending_correction(&self) -> Option<&OriginalRequests> {
        self.pending.as_ref()
    }
    #[cfg(test)]
    pub(super) fn recorded_refusals(&self) -> &[RecordedRecallRefusal] {
        self.account
            .as_ref()
            .map_or(&[], RepairAccount::recall_refusals)
    }

    #[cfg(test)]
    pub(super) fn account_for_test(&self) -> &RepairAccount {
        self.account.as_ref().unwrap()
    }

    /// Only checked association evidence supplies a refusal. The account's
    /// private Recorded entries retire each exact request once, in record order.
    pub(super) fn next_recall_refusal(&self) -> Option<RecallRequestRefusal> {
        let account = self.account.as_ref()?;
        std::iter::once(&account.original)
            .chain(account.corrections.iter())
            .chain(self.pending.iter())
            .flat_map(|generation| generation.refusals())
            .filter_map(AssociationRefusal::recall_refusal)
            .find(|refusal| {
                !account
                    .recall_refusals
                    .iter()
                    .any(|recorded| recorded.refusal.call_id == refusal.call_id)
            })
    }

    pub(super) fn recorded_recall_refusal(
        &mut self,
        expected: &RecallRequestRefusal,
        source: SourcePosition,
    ) -> Result<(), RepairError> {
        if self.stage == Stage::Frozen {
            return Err(RepairError::Frozen);
        }
        let refusal = self.next_recall_refusal().ok_or(RepairError::WrongPhase)?;
        if &refusal != expected {
            return Err(RepairError::WrongAccounting);
        }
        self.record(source.clone())?;
        self.account
            .as_mut()
            .unwrap()
            .recall_refusals
            .push(RecordedRecallRefusal {
                refusal,
                sequence: source.block_id.sequence,
            });
        Ok(())
    }

    pub(super) fn cancelled_protocol_refusal(&self) -> Option<SourcePosition> {
        let pending = self.pending.as_ref()?;
        (self.stage == Stage::Cancelled
            && pending.attempted_call()
            && !self.pending_protocol_recorded)
            .then(|| pending.generation().source().clone())
    }

    pub(super) fn cancelled_empty_refusal(&self) -> Option<(SourceBlockId, SourcePosition)> {
        let pending = self.pending.as_ref()?;
        (self.stage == Stage::Cancelled
            && !pending.attempted_call()
            && pending.generation().blocks().is_empty())
        .then(|| {
            (
                self.context().source.clone(),
                pending.generation().source().clone(),
            )
        })
    }

    /// The context calls this only for the exact issued output after every
    /// Text check is Recorded and the loaded scheme excludes all attempted calls.
    /// It changes no stage and grants no retry, including at the cap.
    pub(super) fn recorded_text_failure(
        &mut self,
        source: &SourceBlockId,
        correction: &SourcePosition,
    ) -> Result<(), RepairError> {
        let account = self.account.as_ref().ok_or(RepairError::WrongPhase)?;
        let context = self
            .contexts
            .get(self.current)
            .ok_or(RepairError::WrongPhase)?;
        if &context.source != source
            || correction.block_id.run != account.source().block_id.run
            || correction.turn != account.source().turn
            || correction.block_id.block.0 != 0
            || context.instructions.last().is_none_or(|instruction| {
                correction.block_id.sequence <= instruction.block_id.sequence
            })
        {
            return Err(RepairError::WrongSource);
        }
        if let Some(last) = account.failed_corrections.last() {
            if &last.source == source && &last.correction == correction {
                return Ok(());
            }
            if last.correction.block_id.sequence >= correction.block_id.sequence {
                return Err(RepairError::OutOfOrder);
            }
        }
        if !matches!(self.stage, Stage::Awaiting | Stage::Cancelled) {
            return Err(RepairError::WrongPhase);
        }
        if context.failed >= self.cap {
            return Err(RepairError::WrongPhase);
        }
        self.retain_failed_correction(correction.clone());
        Ok(())
    }

    #[allow(clippy::result_large_err)]
    pub(super) fn observe_cancelled_correction(
        &mut self,
        correction: OriginalRequests,
    ) -> Result<(), (OriginalRequests, RepairError)> {
        let validation = self.require(Stage::Cancelled).and_then(|()| {
            if self.pending.is_some() {
                return Err(RepairError::WrongPhase);
            }
            self.validate_correction(&correction)
        });
        if let Err(error) = validation {
            return Err((correction, error));
        }
        self.frontier = generation_frontier(correction.generation());
        if !correction.attempted_call()
            && correction
                .generation()
                .blocks()
                .iter()
                .any(|check| matches!(check.reply().result, Ste100Result::NotSte100 { .. }))
        {
            self.retain_failed_correction(correction.generation().source().clone());
        }
        self.pending = Some(correction);
        self.pending_protocol_recorded = false;
        Ok(())
    }
    pub(super) fn failed_corrections(&self) -> Option<u32> {
        self.contexts
            .get(self.current)
            .map(|context| context.failed)
    }

    pub(super) fn recorded_requested(&mut self, source: SourcePosition) -> Result<(), RepairError> {
        self.require(Stage::Requested)?;
        self.record(source)?;
        self.stage = Stage::Instruction;
        Ok(())
    }

    #[allow(clippy::result_large_err)]
    pub(super) fn recorded_instruction(
        &mut self,
        instruction: TemplateExpansion,
        recorded: SourcePosition,
    ) -> Result<RestatementRequest, (TemplateExpansion, RepairError)> {
        let validation = self.require(Stage::Instruction).and_then(|()| {
            let account = self.account.as_ref().unwrap();
            if instruction.run_id() != account.source().block_id.run
                || instruction.scheme() != account.scheme()
            {
                return Err(RepairError::InvalidExpansion);
            }
            self.validate_record(&recorded)
        });
        if let Err(error) = validation {
            return Err((instruction, error));
        }
        self.frontier = recorded.block_id.sequence.0;
        self.contexts[self.current]
            .instructions
            .push(recorded.clone());
        self.stage = Stage::Awaiting;
        Ok(RestatementRequest {
            source: self.context().source.clone(),
            attempt: self.attempt(),
            instruction,
            recorded,
        })
    }

    #[allow(clippy::result_large_err)]
    pub(super) fn observe_correction(
        &mut self,
        source: &SourceBlockId,
        correction: OriginalRequests,
    ) -> Result<CorrectionProgress, (OriginalRequests, RepairError)> {
        let validation = self.require(Stage::Awaiting).and_then(|()| {
            if source != &self.context().source {
                return Err(RepairError::WrongSource);
            }
            self.validate_correction(&correction)
        });
        if let Err(error) = validation {
            return Err((correction, error));
        }
        self.frontier = generation_frontier(correction.generation());
        if correction.attempted_call() {
            self.pending = Some(correction);
            self.pending_protocol_recorded = false;
            self.stage = Stage::ProtocolRefusal;
            return Ok(CorrectionProgress::ProtocolRefusal);
        }
        if correction.generation().blocks().is_empty() {
            self.pending = Some(correction);
            self.pending_protocol_recorded = false;
            self.stage = Stage::EmptyRefusal;
            return Ok(CorrectionProgress::EmptyCorrectionRefusal);
        }
        let failed = correction
            .generation()
            .blocks()
            .iter()
            .any(|check| matches!(check.reply().result, Ste100Result::NotSte100 { .. }));
        let account = self.account.as_mut().unwrap();
        let index = account.corrections.len();
        let correction_source = correction.generation().source().clone();
        account.corrections.push(correction);
        if failed {
            return Ok(self.failed_attempt(correction_source));
        }
        self.contexts[self.current].success = Some(index);
        self.resolve_external(index);
        self.current += 1;
        self.stage = if self.current < self.contexts.len() {
            Stage::Requested
        } else {
            self.finished_stage()
        };
        Ok(CorrectionProgress::Resolved)
    }

    pub(super) fn recorded_protocol_refusal(
        &mut self,
        recorded: SourcePosition,
    ) -> Result<(), RepairError> {
        if self.stage != Stage::Cancelled {
            self.require(Stage::ProtocolRefusal)?;
        }
        if self.pending_protocol_recorded
            || self.pending.as_ref().is_none_or(|p| !p.attempted_call())
        {
            return Err(RepairError::WrongPhase);
        }
        self.record(recorded)?;
        self.pending_protocol_recorded = true;
        if self.stage != Stage::Cancelled {
            self.stage = Stage::CorrectionAccounting;
        }
        Ok(())
    }

    pub(super) fn recorded_empty_correction_refusal(
        &mut self,
        recorded: SourcePosition,
    ) -> Result<(), RepairError> {
        let cancelled = self.stage == Stage::Cancelled;
        if !cancelled {
            self.require(Stage::EmptyRefusal)?;
        }
        let correction = self.pending.as_ref().ok_or(RepairError::WrongPhase)?;
        if correction.attempted_call() || !correction.generation().blocks().is_empty() {
            return Err(RepairError::WrongPhase);
        }
        self.record(recorded.clone())?;
        let correction = self.pending.take().expect("validated empty correction");
        self.pending_protocol_recorded = false;
        let correction_source = correction.generation().source().clone();
        self.account.as_mut().unwrap().corrections.push(correction);
        // Count at Recorded even when cancellation wins the completion race.
        // A late receipt must never reopen a cancelled retry/exhaustion path.
        self.failed_attempt(correction_source.clone());
        if cancelled {
            self.stage = Stage::Cancelled;
        }
        let failed_corrections = self.contexts[self.current].failed;
        self.account
            .as_mut()
            .unwrap()
            .empty_corrections
            .push(RecordedEmptyCorrection {
                correction: correction_source,
                source: recorded,
                failed_corrections,
            });
        Ok(())
    }

    pub(super) fn take_correction_accounting(&mut self) -> Option<OriginalRequests> {
        if self.stage != Stage::CorrectionAccounting || self.next_recall_refusal().is_some() {
            return None;
        }
        let correction = self.pending.take()?;
        self.pending_protocol_recorded = false;
        self.correction_out = Some(correction.generation().source().clone());
        self.stage = Stage::CorrectionOut;
        Some(correction)
    }

    #[allow(clippy::result_large_err)]
    pub(super) fn finish_correction_accounting(
        &mut self,
        correction: OriginalRequests,
        completed: &[&CompletedPair],
    ) -> Result<CorrectionProgress, (OriginalRequests, RepairError)> {
        let validation = (|| {
            if !matches!(self.stage, Stage::CorrectionOut | Stage::Cancelled) {
                return self.require(Stage::CorrectionOut);
            }
            if self.correction_out.as_ref() != Some(correction.generation().source()) {
                return Err(RepairError::WrongAccounting);
            }
            if correction.generation().scheme() != self.account.as_ref().unwrap().scheme() {
                return Err(RepairError::WrongScheme);
            }
            if self.stage == Stage::Cancelled {
                return Ok(());
            }
            validate_accounted(
                correction
                    .requests()
                    .iter()
                    .map(|request| request.opening()),
                completed,
                NonDispatchCause::PassPurpose,
            )
        })();
        if let Err(error) = validation {
            return Err((correction, error));
        }
        let correction_source = correction.generation().source().clone();
        self.account.as_mut().unwrap().corrections.push(correction);
        self.correction_out = None;
        if self.stage == Stage::Cancelled {
            return Ok(CorrectionProgress::Cancelled);
        }
        Ok(self.failed_attempt(correction_source))
    }

    /// The original checked requests and repaired associations are the only
    /// authority for this frontier; no caller supplies a refusal or final flag.
    pub(super) fn next_association_refusal(&self) -> Option<(usize, RefusalRecord)> {
        let account = self.account.as_ref()?;
        let index = account.association_refusals.len();
        // Traverse borrowed descriptions first. Only the selected frontier
        // allocates an owned refusal, including on repeated pending polls.
        let (source, case, detail) = account
            .original
            .refusals()
            .iter()
            .filter_map(|refusal| {
                let AssociationRefusal::External {
                    source,
                    case,
                    detail,
                    ..
                } = refusal
                else {
                    return None;
                };
                Some((source, *case, *detail))
            })
            .chain(self.associations.iter().map(|refusal| {
                (
                    refusal.source(),
                    refusal.case(),
                    "repaired external send requires exactly one non-recall statement",
                )
            }))
            .nth(index)?;
        let refusal = RefusalRecord::ProtocolRepair {
            source_id: source.clone(),
            case,
            detail: detail.into(),
        };
        Some((index, refusal))
    }

    pub(super) fn recorded_association_refusal(
        &mut self,
        expected_index: usize,
        source: SourcePosition,
    ) -> Result<(), RepairError> {
        let cancelled = self.stage == Stage::Cancelled;
        if !cancelled {
            self.require(Stage::Association)?;
        }
        let (index, refusal) = self
            .next_association_refusal()
            .ok_or(RepairError::WrongPhase)?;
        if index != expected_index {
            return Err(RepairError::WrongAccounting);
        }
        self.record(source.clone())?;
        self.account
            .as_mut()
            .unwrap()
            .association_refusals
            .push(RecordedAssociationRefusal { refusal, source });
        if self.next_association_refusal().is_none() {
            self.association_recorded = true;
            self.original_refusals_recorded = true;
            if !cancelled {
                self.stage = self.finished_stage();
            }
        }
        Ok(())
    }

    pub(super) fn recorded_exhausted(&mut self, source: SourcePosition) -> Result<(), RepairError> {
        self.require(Stage::ExhaustedRecord)?;
        self.record(source)?;
        self.stage = Stage::ExhaustedAccounting;
        Ok(())
    }

    pub(super) fn take_exhausted_account(&mut self) -> Option<RepairAccount> {
        if self.stage != Stage::ExhaustedAccounting || self.next_recall_refusal().is_some() {
            return None;
        }
        let account = self.account.take()?;
        self.account_out = Some(Arc::clone(&account.nonce));
        self.stage = Stage::ExhaustedOut;
        Some(account)
    }

    #[allow(clippy::result_large_err)]
    pub(super) fn finish_exhausted_account(
        &mut self,
        account: RepairAccount,
        completed: &[&CompletedPair],
    ) -> Result<(), (RepairAccount, RepairError)> {
        let validation = (|| {
            if !matches!(self.stage, Stage::ExhaustedOut | Stage::Cancelled) {
                return self.require(Stage::ExhaustedOut);
            }
            if self
                .account_out
                .as_ref()
                .is_none_or(|nonce| !Arc::ptr_eq(nonce, &account.nonce))
            {
                return Err(RepairError::WrongAccounting);
            }
            if self.stage == Stage::Cancelled {
                return Ok(());
            }
            validate_accounted(
                account.requests.iter(),
                completed,
                NonDispatchCause::RepairExhausted,
            )
        })();
        if let Err(error) = validation {
            return Err((account, error));
        }
        self.account = Some(account);
        self.account_out = None;
        if self.stage != Stage::Cancelled {
            self.stage = Stage::Exhausted;
        }
        Ok(())
    }

    pub(super) fn take_effective(&mut self) -> Result<Option<EffectiveEmission>, RepairError> {
        if self.stage == Stage::Frozen {
            return Err(RepairError::Frozen);
        }
        if self.stage != Stage::Complete || self.next_recall_refusal().is_some() {
            return Ok(None);
        }
        let account = self.account.as_ref().unwrap();
        let mut statements = vec![];
        let mut text = vec![];
        for check in account.original.generation().blocks() {
            let Ste100Target::Source(source) = &check.request().target else {
                return Err(RepairError::InvalidStatement);
            };
            if matches!(check.reply().result, Ste100Result::Accepted { .. }) {
                append_statements(check, &mut statements, &mut text)?;
            } else {
                let context = self
                    .contexts
                    .iter()
                    .find(|context| &context.source == source)
                    .ok_or(RepairError::InvalidStatement)?;
                let correction =
                    &account.corrections[context.success.ok_or(RepairError::InvalidStatement)?];
                for check in correction.generation().blocks() {
                    append_statements(check, &mut statements, &mut text)?;
                }
            }
        }
        let account = self.account.take().unwrap();
        let effective = EffectiveEmission {
            source: account.source().clone(),
            finish: account.original.generation().original().finish,
            statements,
            requests: account.requests.clone(),
            text: text.join("\n"),
            attempted_call: account.original.attempted_call(),
            account,
        };
        self.stage = Stage::Transferred;
        Ok(Some(effective))
    }

    pub(super) fn take_exhausted(&mut self) -> Option<RepairExhausted> {
        if self.stage != Stage::Exhausted {
            return None;
        }
        let source = self.context().source.clone();
        let failed_corrections = u64::from(self.context().failed).into();
        let account = self.account.take()?;
        self.stage = Stage::Transferred;
        Some(RepairExhausted {
            source,
            failed_corrections,
            retry_cap: u64::from(self.cap).into(),
            account,
        })
    }

    pub(super) fn cancel(&mut self) {
        if !matches!(self.stage, Stage::Frozen | Stage::Transferred) {
            self.stage = Stage::Cancelled;
        }
    }
    pub(super) fn outstanding_loan(&self) -> Option<RepairError> {
        if self.correction_out.is_some() {
            Some(RepairError::OutstandingCorrectionLoan)
        } else if self.account_out.is_some() {
            Some(RepairError::OutstandingAccountLoan)
        } else {
            None
        }
    }

    pub(super) fn take_cancelled(&mut self) -> Option<RepairCancellation> {
        if self.stage != Stage::Cancelled
            || self.correction_out.is_some()
            || self.account_out.is_some()
            || self.next_recall_refusal().is_some()
            || self.cancelled_protocol_refusal().is_some()
            || self.cancelled_empty_refusal().is_some()
        {
            return None;
        }
        let account = self.account.take()?;
        self.stage = Stage::Transferred;
        self.pending_protocol_recorded = false;
        Some(RepairCancellation {
            account,
            pending: self.pending.take(),
        })
    }
    pub(super) fn freeze(&mut self) {
        if self.stage != Stage::Transferred {
            self.stage = Stage::Frozen;
        }
    }

    fn finished_stage(&self) -> Stage {
        if !self.association_recorded || !self.original_refusals_recorded {
            Stage::Association
        } else {
            Stage::Complete
        }
    }
    fn context(&self) -> &RepairContext {
        &self.contexts[self.current]
    }
    fn attempt(&self) -> DecimalU64 {
        // A final Recorded check can charge the current pass before its loan
        // returns. Its already issued ordinal must not change with that count.
        if self.stage == Stage::Awaiting {
            u64::try_from(self.context().instructions.len())
                .expect("instruction count fits u64")
                .into()
        } else {
            (u64::from(self.context().failed) + 1).into()
        }
    }
    fn require(&self, stage: Stage) -> Result<(), RepairError> {
        if self.stage == Stage::Frozen {
            Err(RepairError::Frozen)
        } else if self.stage != stage {
            Err(RepairError::WrongPhase)
        } else {
            Ok(())
        }
    }
    fn validate_record(&self, source: &SourcePosition) -> Result<(), RepairError> {
        let account = self.account.as_ref().ok_or(RepairError::WrongPhase)?;
        if source.block_id.run != account.source().block_id.run {
            return Err(RepairError::WrongRun);
        }
        if source.turn != account.source().turn {
            return Err(RepairError::WrongTurn);
        }
        if source.block_id.block.0 != 0 || source.block_id.sequence.0 <= self.frontier {
            return Err(RepairError::OutOfOrder);
        }
        Ok(())
    }
    fn record(&mut self, source: SourcePosition) -> Result<(), RepairError> {
        self.validate_record(&source)?;
        self.frontier = source.block_id.sequence.0;
        Ok(())
    }
    fn validate_correction(&self, correction: &OriginalRequests) -> Result<(), RepairError> {
        let account = self.account.as_ref().unwrap();
        if correction.generation().scheme() != account.scheme() {
            return Err(RepairError::WrongScheme);
        }
        self.validate_record(correction.generation().source())
    }
    fn retain_failed_correction(&mut self, correction: SourcePosition) {
        let source = &self.contexts[self.current].source;
        let account = self.account.as_mut().expect("counting retains its account");
        if account
            .failed_corrections
            .last()
            .is_some_and(|last| &last.source == source && last.correction == correction)
        {
            return;
        }
        let context = &mut self.contexts[self.current];
        context.failed += 1;
        account.failed_corrections.push(RecordedFailedCorrection {
            source: context.source.clone(),
            correction,
            failed_corrections: context.failed,
        });
    }

    fn failed_attempt(&mut self, correction: SourcePosition) -> CorrectionProgress {
        self.retain_failed_correction(correction);
        let context = &self.contexts[self.current];
        if context.failed == self.cap {
            self.stage = Stage::ExhaustedRecord;
            CorrectionProgress::Exhausted
        } else {
            self.stage = Stage::Instruction;
            CorrectionProgress::Retry
        }
    }
    fn resolve_external(&mut self, correction_index: usize) {
        let account = self.account.as_mut().unwrap();
        let source = &self.contexts[self.current].source;
        let correction = &account.corrections[correction_index];
        let bindings: Vec<_> = correction
            .generation()
            .blocks()
            .iter()
            .flat_map(|check| {
                let Ste100Result::Accepted { bindings } = &check.reply().result else {
                    unreachable!("successful correction");
                };
                let Ste100Target::Source(source) = &check.request().target else {
                    unreachable!("model source");
                };
                bindings
                    .iter()
                    .enumerate()
                    .map(move |(index, _)| StatementRef {
                        block_id: source.clone(),
                        binding: (index as u64).into(),
                    })
            })
            .collect();
        for pending in account
            .original
            .unresolved()
            .iter()
            .filter(|pending| pending.rejected_source() == source)
        {
            if bindings.len() == 1 {
                account.requests.push(PairStarted {
                    call_id: pending.call_id().clone(),
                    call_block: pending.call_block().into(),
                    statement: bindings[0].clone(),
                    lane: PairLane::External,
                    request: pending.request().clone(),
                    retry_of: None,
                });
            } else {
                self.associations.push(RepairAssociationRefusal {
                    call_id: pending.call_id().clone(),
                    call_block: pending.call_block(),
                    source: source.clone(),
                    case: ProtocolRepairCase::AmbiguousToolStatement,
                });
                self.association_recorded = false;
            }
        }
        account
            .requests
            .sort_by_key(|request| (request.call_block, request.statement.binding));
    }
}

fn generation_frontier(generation: &CheckedGeneration) -> u64 {
    generation
        .blocks()
        .iter()
        .map(|check| check.result_source().block_id.sequence.0)
        .chain(std::iter::once(generation.source().block_id.sequence.0))
        .max()
        .unwrap()
}

fn append_statements(
    check: &crate::flow::coordinator::RecordedOutputBlock,
    statements: &mut Vec<StatementRef>,
    text: &mut Vec<String>,
) -> Result<(), RepairError> {
    let Ste100Result::Accepted { bindings } = &check.reply().result else {
        return Err(RepairError::InvalidStatement);
    };
    let Ste100Target::Source(source) = &check.request().target else {
        return Err(RepairError::InvalidStatement);
    };
    for (index, binding) in bindings.iter().enumerate() {
        let span = binding
            .statement_span
            .slice(&check.request().text)
            .map_err(|_| RepairError::InvalidStatement)?;
        statements.push(StatementRef {
            block_id: source.clone(),
            binding: (index as u64).into(),
        });
        text.push(span.to_owned());
    }
    Ok(())
}

pub(super) fn validate_accounted<'a>(
    requests: impl Iterator<Item = &'a PairStarted>,
    completed: &[&CompletedPair],
    cause: NonDispatchCause,
) -> Result<(), RepairError> {
    let requests: Vec<_> = requests.collect();
    if requests.len() != completed.len() {
        return Err(RepairError::WrongAccounting);
    }
    for (request, completed) in requests.into_iter().zip(completed) {
        if completed.started() != request
            || completed.ended().call_id != request.call_id
            || completed.ended().fate != TerminalPairFate::Failed
            || completed.ended().reason != Some(PairFailure::NotDispatched { cause })
            || completed.ended().execution.is_some()
            || completed.ended().outcome.is_some()
            || completed.ended().loaded_from.is_some()
        {
            return Err(RepairError::WrongAccounting);
        }
    }
    Ok(())
}

/// A terminal already admitted by one purpose remains immutable when Stop and
/// then Unload claim the remaining suffix. Public pair ownership cannot reorder
/// that prefix; this check also refuses corrupted or wrongly composed proofs.
pub(super) fn validate_refused_accounted<'a>(
    requests: impl ExactSizeIterator<Item = &'a PairStarted>,
    completed: &[&CompletedPair],
    cancel: Option<NonDispatchCause>,
) -> Result<(), RepairError> {
    let ceiling = match cancel {
        None => 0,
        Some(NonDispatchCause::Stop) => 1,
        Some(NonDispatchCause::Unload) => 2,
        Some(_) => return Err(RepairError::WrongAccounting),
    };
    if requests.len() != completed.len() {
        return Err(RepairError::WrongAccounting);
    }
    let mut previous = 0;
    for (request, proof) in requests.zip(completed) {
        let Some(PairFailure::NotDispatched { cause }) = &proof.ended().reason else {
            return Err(RepairError::WrongAccounting);
        };
        let rank = match cause {
            NonDispatchCause::PassPurpose => 0,
            NonDispatchCause::Stop => 1,
            NonDispatchCause::Unload => 2,
            _ => return Err(RepairError::WrongAccounting),
        };
        if rank < previous || rank > ceiling {
            return Err(RepairError::WrongAccounting);
        }
        validate_accounted(std::iter::once(request), &[*proof], *cause)?;
        previous = rank;
    }
    Ok(())
}

#[cfg(test)]
#[path = "repair_tests.rs"]
mod tests;

/// Fixture release goes through the real successful repair transition. A
/// rejected or unresolved generation cannot manufacture an effective emission.
#[cfg(test)]
pub(super) fn effective_fixture(original: OriginalRequests) -> EffectiveEmission {
    RepairLedger::begin(original, 2)
        .unwrap()
        .take_effective()
        .unwrap()
        .expect("fixture generation has no unresolved repair")
}

/// Pair-owner fixture. The actual-context repair tests cover the recorder side.
/// This runs the real repair transitions and derives starts from checked spans,
/// rather than minting a caller-supplied request vector.
#[cfg(test)]
pub(super) fn account_fixture(
    original: OriginalRequests,
    successful: Vec<(SourceBlockId, OriginalRequests)>,
) -> RepairAccount {
    let mut ledger = RepairLedger::begin(original, 2).unwrap();
    for (source, correction) in successful {
        let account = ledger.account.as_ref().unwrap();
        let source_record = SourcePosition {
            block_id: SourceBlockId {
                run: account.source().block_id.run.clone(),
                sequence: (ledger.frontier + 1).into(),
                block: 0.into(),
            },
            turn: account.source().turn.clone(),
        };
        let expanded = tests::expansion(&account.source().block_id.run, account.scheme());
        ledger.recorded_requested(source_record.clone()).unwrap();
        let instruction = SourcePosition {
            block_id: SourceBlockId {
                sequence: (source_record.block_id.sequence.0 + 1).into(),
                ..source_record.block_id
            },
            turn: source_record.turn,
        };
        ledger.recorded_instruction(expanded, instruction).unwrap();
        assert_eq!(
            ledger.observe_correction(&source, correction).unwrap(),
            CorrectionProgress::Resolved
        );
    }
    ledger.account.take().unwrap()
}
