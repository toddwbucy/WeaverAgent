//! Passive custody for conforming sends barred from execution. The context owns
//! every author ticket and socket operation. This owner grants no execution,
//! tool result, repair success, placement, or new write path.

use std::sync::Arc;

use weaver_traits::ContentBlock;
use weaver_types::state_flow::{
    CallId, NonDispatchCause, PairEnded, PairFailure, PairRecord, PairStarted, SchemeDeclaration,
    SourceBlockId, StateFrame, Ste100Result, Ste100Target, TerminalPairFate,
};

use super::repair::RepairAccount;
use crate::flow::coordinator::{CheckedGeneration, OriginalRequests};
use crate::flow::ledger::{CommittedSource, LedgerFile, SourceLease, TerminalOutcome};
use crate::flow::report::{OutstandingPair, PairState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PairError {
    Frozen,
    InvalidIdentity,
    WrongScheme,
    Busy,
    UnknownCall,
    WrongPhase,
    DuplicateCall,
    DuplicateSource,
    WrongReceipt,
    WrongStatement,
    Allocation,
}

impl std::fmt::Display for PairError {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(out, "{self:?}")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PairRecordPhase {
    Opening,
    Terminal,
}

/// A private view of the exact terminal commit, not another wire frame.
#[derive(Debug)]
pub(in crate::flow) struct FiledReceipt {
    request_id: weaver_types::state_flow::DecimalU64,
    block_id: SourceBlockId,
}

impl FiledReceipt {
    pub(in crate::flow) fn request_id(&self) -> weaver_types::state_flow::DecimalU64 {
        self.request_id
    }
    pub(in crate::flow) fn block_id(&self) -> &SourceBlockId {
        &self.block_id
    }
}

/// Only a matched opening and terminal commit construct this retained fact.
/// It is not a delivery acknowledgement or a permission to execute a request.
#[derive(Debug)]
pub(in crate::flow) struct CompletedPair {
    started: PairStarted,
    ended: PairEnded,
    statement: Arc<CommittedSource>,
    opening: CommittedSource,
    terminal: CommittedSource,
    receipt: FiledReceipt,
    result: crate::tools::ToolResult,
}

impl CompletedPair {
    pub(in crate::flow) fn started(&self) -> &PairStarted {
        &self.started
    }
    pub(in crate::flow) fn ended(&self) -> &PairEnded {
        &self.ended
    }
    pub(in crate::flow) fn statement(&self) -> &CommittedSource {
        &self.statement
    }
    pub(in crate::flow) fn opening(&self) -> &CommittedSource {
        &self.opening
    }
    pub(in crate::flow) fn terminal(&self) -> &CommittedSource {
        &self.terminal
    }
    pub(in crate::flow) fn terminal_source(&self) -> &SourceBlockId {
        self.terminal.source()
    }
    pub(in crate::flow) fn receipt(&self) -> &FiledReceipt {
        &self.receipt
    }
    /// A notice of a committed non-dispatch, never proof of execution. Actual
    /// returned placement coverage is still required to acknowledge delivery.
    pub(in crate::flow) fn result(&self) -> &crate::tools::ToolResult {
        &self.result
    }
}

struct Entry {
    started: PairStarted,
    opening_authoring: bool,
    opening_source: Option<SourceBlockId>,
    opening: Option<CommittedSource>,
    ending: Option<PairEnded>,
    terminal_authoring: bool,
    terminal_source: Option<SourceBlockId>,
    completed: Option<CompletedPair>,
}

/// Only the association or repair owner can construct either proof. A public
/// PairStarted vector is never sufficient to enter the accounting ledger.
enum BatchProof {
    Original(OriginalRequests),
    Repair(RepairAccount),
}

impl BatchProof {
    fn original(&self) -> &OriginalRequests {
        match self {
            Self::Original(original) => original,
            Self::Repair(account) => account.original(),
        }
    }

    fn requests(&self) -> impl Iterator<Item = &PairStarted> {
        let (original, repair) = match self {
            Self::Original(original) => (Some(original), None),
            Self::Repair(account) => (None, Some(account)),
        };
        original
            .into_iter()
            .flat_map(|original| original.requests().iter().map(|request| request.opening()))
            .chain(repair.into_iter().flat_map(|account| account.requests()))
    }

    fn generations(&self) -> impl Iterator<Item = &CheckedGeneration> {
        let (original, repair) = match self {
            Self::Original(original) => (Some(original.generation()), None),
            Self::Repair(account) => (None, Some(account)),
        };
        original
            .into_iter()
            .chain(repair.into_iter().flat_map(|account| account.generations()))
    }

    fn generation_for(&self, source: &SourceBlockId) -> Option<&CheckedGeneration> {
        self.generations().find(|generation| {
            generation
                .blocks()
                .iter()
                .any(|block| block.request().target == Ste100Target::Source(source.clone()))
        })
    }
}

struct Batch {
    proof: BatchProof,
    entries: Vec<Entry>,
    statements: Vec<Arc<CommittedSource>>,
    cause: Option<NonDispatchCause>,
}

pub(super) struct PairLedger {
    session: String,
    run: String,
    scheme: SchemeDeclaration,
    active: Option<Batch>,
    closed: Vec<CompletedPair>,
    frozen: bool,
}

impl PairLedger {
    pub(super) fn new(
        session: String,
        run: String,
        scheme: SchemeDeclaration,
    ) -> Result<Self, PairError> {
        if session.is_empty() || run.is_empty() {
            return Err(PairError::InvalidIdentity);
        }
        scheme.validate().map_err(|_| PairError::WrongScheme)?;
        Ok(Self {
            session,
            run,
            scheme,
            active: None,
            closed: Vec::new(),
            frozen: false,
        })
    }

    /// The context verifies that the generation's private lease belongs to its
    /// live ledger before calling this. The original remains owned for repair.
    #[allow(clippy::result_large_err)]
    pub(super) fn accept(
        &mut self,
        original: OriginalRequests,
    ) -> Result<(), (OriginalRequests, PairError)> {
        self.accept_proof(BatchProof::Original(original))
            .map_err(|(proof, error)| match proof {
                BatchProof::Original(original) => (original, error),
                BatchProof::Repair(_) => unreachable!("original proof returned unchanged"),
            })
    }

    #[allow(clippy::result_large_err)]
    pub(super) fn accept_repair(
        &mut self,
        account: RepairAccount,
    ) -> Result<(), (RepairAccount, PairError)> {
        self.accept_proof(BatchProof::Repair(account))
            .map_err(|(proof, error)| match proof {
                BatchProof::Repair(account) => (account, error),
                BatchProof::Original(_) => unreachable!("repair proof returned unchanged"),
            })
    }

    #[allow(clippy::result_large_err)]
    fn accept_proof(&mut self, proof: BatchProof) -> Result<(), (BatchProof, PairError)> {
        let prepare = (|| {
            self.live()?;
            if self.active.is_some() {
                return Err(PairError::Busy);
            }
            let generation = proof.original().generation();
            if generation.source().block_id.run != self.run
                || generation.source().block_id.sequence.0 == 0
                || generation.source().turn.as_deref()
                    != Some(generation.original().turn.0.as_str())
            {
                return Err(PairError::InvalidIdentity);
            }
            if generation.scheme() != &self.scheme {
                return Err(PairError::WrongScheme);
            }
            for held in proof.generations() {
                if held.scheme() != &self.scheme {
                    return Err(PairError::WrongScheme);
                }
                if held.source().block_id.run != self.run
                    || held.source().turn != generation.source().turn
                    || held.original().turn != generation.original().turn
                    || held.source().block_id.sequence < generation.source().block_id.sequence
                {
                    return Err(PairError::InvalidIdentity);
                }
            }
            let count = proof.requests().count();
            let mut entries = Vec::new();
            entries
                .try_reserve_exact(count)
                .map_err(|_| PairError::Allocation)?;
            let mut statements = Vec::new();
            statements
                .try_reserve_exact(count)
                .map_err(|_| PairError::Allocation)?;
            self.closed
                .try_reserve(count)
                .map_err(|_| PairError::Allocation)?;
            for started in proof.requests() {
                if self
                    .closed
                    .iter()
                    .any(|pair| pair.started.call_id == started.call_id)
                    || entries
                        .iter()
                        .any(|entry: &Entry| entry.started.call_id == started.call_id)
                {
                    return Err(PairError::DuplicateCall);
                }
                if started.call_id.run != self.run
                    || started.call_id.sequence != generation.source().block_id.sequence
                    || started.retry_of.is_some()
                    || started.validate_structure().is_err()
                    || proof.generation_for(&started.statement.block_id).is_none()
                {
                    return Err(PairError::InvalidIdentity);
                }
                entries.push(Entry {
                    started: started.clone(),
                    opening_authoring: false,
                    opening_source: None,
                    opening: None,
                    ending: None,
                    terminal_authoring: false,
                    terminal_source: None,
                    completed: None,
                });
            }
            Ok((entries, statements))
        })();
        let (entries, statements) = match prepare {
            Ok(prepared) => prepared,
            Err(error) => return Err((proof, error)),
        };
        self.active = Some(Batch {
            proof,
            entries,
            statements,
            cause: None,
        });
        Ok(())
    }

    pub(super) fn original(&self) -> Option<&OriginalRequests> {
        self.active.as_ref().map(|batch| batch.proof.original())
    }

    pub(super) fn has_repair_account(&self) -> bool {
        self.active
            .as_ref()
            .is_some_and(|batch| matches!(batch.proof, BatchProof::Repair(_)))
    }

    pub(super) fn receipt_lease(&self, source: &SourceBlockId) -> Option<&SourceLease> {
        self.active
            .as_ref()?
            .proof
            .generation_for(source)?
            .receipt_lease()
    }

    /// Multiple bindings in one source use the same owned committed fact.
    pub(super) fn next_statement(&self) -> Option<&SourceBlockId> {
        if self.frozen {
            return None;
        }
        let batch = self.active.as_ref()?;
        batch
            .entries
            .iter()
            .map(|entry| &entry.started.statement.block_id)
            .find(|source| !batch.statements.iter().any(|held| held.source() == *source))
    }

    #[allow(clippy::result_large_err)]
    pub(super) fn install_statement(
        &mut self,
        receipt: CommittedSource,
    ) -> Result<(), (CommittedSource, PairError)> {
        let validate = (|| {
            self.live()?;
            self.validate_receipt(&receipt)?;
            let batch = self.active.as_ref().ok_or(PairError::WrongPhase)?;
            if batch
                .statements
                .iter()
                .any(|held| held.source() == receipt.source())
            {
                return Err(PairError::DuplicateSource);
            }
            if !batch
                .entries
                .iter()
                .any(|entry| &entry.started.statement.block_id == receipt.source())
            {
                return Err(PairError::WrongStatement);
            }
            let LedgerFile::Statement(file) = receipt.file() else {
                return Err(PairError::WrongStatement);
            };
            let generation = batch
                .proof
                .generation_for(receipt.source())
                .ok_or(PairError::WrongStatement)?;
            let check = generation
                .blocks()
                .iter()
                .find(|block| {
                    block.request().target == Ste100Target::Source(receipt.source().clone())
                })
                .ok_or(PairError::WrongStatement)?;
            let Ste100Result::Accepted { bindings } = &check.reply().result else {
                return Err(PairError::WrongStatement);
            };
            let index =
                usize::try_from(receipt.source().block.0).map_err(|_| PairError::WrongStatement)?;
            let Some(ContentBlock::Text { text }) =
                generation.original().message.content.get(index)
            else {
                return Err(PairError::WrongStatement);
            };
            if file.source.session != self.session
                || file.source.kind != "message.assistant"
                || file.source.channel != "output"
                || file.source.origin != "model"
                || file.source.subsystem != "harness"
                || file.source.turn.as_deref() != Some(generation.original().turn.0.as_str())
                || file.scheme_id != self.scheme.scheme_id
                || file.text != *text
                || file.bindings != *bindings
                || file.context.is_some()
                || file.effect.is_some()
                || file.validate_structure(&self.scheme.verbs).is_err()
                || batch
                    .entries
                    .iter()
                    .filter(|entry| &entry.started.statement.block_id == receipt.source())
                    .any(|entry| {
                        usize::try_from(entry.started.statement.binding.0)
                            .ok()
                            .is_none_or(|ordinal| ordinal >= file.bindings.len())
                    })
            {
                return Err(PairError::WrongStatement);
            }
            Ok(())
        })();
        if let Err(error) = validate {
            return Err((receipt, error));
        }
        self.active
            .as_mut()
            .unwrap()
            .statements
            .push(Arc::new(receipt));
        Ok(())
    }

    pub(super) fn next_opening(&self) -> Option<(&CallId, &PairStarted)> {
        if self.frozen || self.author_pending() {
            return None;
        }
        let entry = self
            .active
            .as_ref()?
            .entries
            .iter()
            .find(|entry| !entry.opening_authoring)?;
        Some((&entry.started.call_id, &entry.started))
    }

    pub(super) fn next_terminal(&self) -> Option<(&CallId, &PairEnded)> {
        if self.frozen || self.author_pending() {
            return None;
        }
        let batch = self.active.as_ref()?;
        let entry = batch
            .entries
            .iter()
            .find(|entry| entry.completed.is_none())?;
        if entry.terminal_authoring
            || entry.opening.is_none()
            || !batch
                .statements
                .iter()
                .any(|held| held.source() == &entry.started.statement.block_id)
        {
            return None;
        }
        Some((&entry.started.call_id, entry.ending.as_ref()?))
    }

    /// Claim before handing the record to the recording context. Once claimed,
    /// its terminal intent stays immutable even while its author ticket waits.
    pub(super) fn mark_authoring(
        &mut self,
        call: &CallId,
        phase: PairRecordPhase,
    ) -> Result<(), PairError> {
        self.live()?;
        let expected = match phase {
            PairRecordPhase::Opening => self.next_opening().map(|(call, _)| call),
            PairRecordPhase::Terminal => self.next_terminal().map(|(call, _)| call),
        };
        if expected != Some(call) {
            return Err(PairError::WrongPhase);
        }
        let entry = self.entry_mut(call)?;
        match phase {
            PairRecordPhase::Opening => entry.opening_authoring = true,
            PairRecordPhase::Terminal => entry.terminal_authoring = true,
        }
        Ok(())
    }

    pub(super) fn mark_recorded(
        &mut self,
        call: &CallId,
        phase: PairRecordPhase,
        source: SourceBlockId,
    ) -> Result<(), PairError> {
        self.live()?;
        if source.run != self.run || source.block.0 != 0 || source.sequence <= call.sequence {
            return Err(PairError::InvalidIdentity);
        }
        if self.source_used(&source) {
            return Err(PairError::DuplicateSource);
        }
        let entry = self.entry_mut(call)?;
        match phase {
            PairRecordPhase::Opening => {
                if !entry.opening_authoring || entry.opening_source.is_some() {
                    return Err(PairError::WrongPhase);
                }
                if source.sequence <= entry.started.statement.block_id.sequence {
                    return Err(PairError::InvalidIdentity);
                }
                entry.opening_source = Some(source);
            }
            PairRecordPhase::Terminal => {
                if !entry.terminal_authoring
                    || entry.terminal_source.is_some()
                    || entry
                        .opening_source
                        .as_ref()
                        .is_none_or(|opening| source.sequence <= opening.sequence)
                {
                    return Err(PairError::WrongPhase);
                }
                entry.terminal_source = Some(source);
            }
        }
        Ok(())
    }

    #[allow(clippy::result_large_err)]
    pub(super) fn install_opening(
        &mut self,
        call: &CallId,
        receipt: CommittedSource,
    ) -> Result<(), (CommittedSource, PairError)> {
        let validate = (|| {
            self.live()?;
            let entry = self.entry(call)?;
            if entry.opening.is_some() || entry.completed.is_some() {
                return Err(PairError::WrongPhase);
            }
            self.validate_pair_receipt(entry, PairRecordPhase::Opening, &receipt)
        })();
        if let Err(error) = validate {
            return Err((receipt, error));
        }
        self.entry_mut(call).unwrap().opening = Some(receipt);
        Ok(())
    }

    /// Operator leave dominates stop. An accepted stop dominates ordinary
    /// caller barriers. No change rewrites a record already given to authoring.
    pub(super) fn set_not_dispatched(&mut self, cause: NonDispatchCause) -> Result<(), PairError> {
        self.live()?;
        let batch = self.active.as_mut().ok_or(PairError::WrongPhase)?;
        let cause = dominant(batch.cause, cause);
        batch.cause = Some(cause);
        for entry in &mut batch.entries {
            if !entry.terminal_authoring && entry.completed.is_none() {
                entry.ending = Some(PairEnded {
                    call_id: entry.started.call_id.clone(),
                    execution: None,
                    fate: TerminalPairFate::Failed,
                    reason: Some(PairFailure::NotDispatched { cause }),
                    outcome: None,
                    loaded_from: None,
                });
            }
        }
        Ok(())
    }

    #[allow(clippy::result_large_err)]
    pub(super) fn install_terminal(
        &mut self,
        call: &CallId,
        receipt: CommittedSource,
    ) -> Result<(), (CommittedSource, PairError)> {
        let validate = (|| {
            self.live()?;
            let entry = self.entry(call)?;
            if entry.completed.is_some() || entry.opening.is_none() {
                return Err(PairError::WrongPhase);
            }
            self.validate_pair_receipt(entry, PairRecordPhase::Terminal, &receipt)?;
            self.active
                .as_ref()
                .unwrap()
                .statements
                .iter()
                .find(|held| held.source() == &entry.started.statement.block_id)
                .cloned()
                .ok_or(PairError::WrongStatement)
        })();
        let statement = match validate {
            Ok(statement) => statement,
            Err(error) => return Err((receipt, error)),
        };
        let entry = self.entry_mut(call).unwrap();
        let filed = FiledReceipt {
            request_id: receipt.common().request_id,
            block_id: receipt.source().clone(),
        };
        let Some(PairFailure::NotDispatched { cause }) = &entry.ending.as_ref().unwrap().reason
        else {
            unreachable!("this owner creates only non-dispatch terminals")
        };
        let cause = *cause;
        entry.completed = Some(CompletedPair {
            started: entry.started.clone(),
            ended: entry.ending.as_ref().unwrap().clone(),
            statement,
            opening: entry.opening.take().unwrap(),
            terminal: receipt,
            receipt: filed,
            result: crate::tools::ToolResult::not_dispatched(cause),
        });
        Ok(())
    }

    pub(super) fn completed(&self, call: &CallId) -> Result<Option<&CompletedPair>, PairError> {
        self.live()?;
        if let Some(entry) = self.active.as_ref().and_then(|batch| {
            batch
                .entries
                .iter()
                .find(|entry| &entry.started.call_id == call)
        }) {
            return Ok(entry.completed.as_ref());
        }
        self.closed
            .iter()
            .find(|pair| &pair.started.call_id == call)
            .map(Some)
            .ok_or(PairError::UnknownCall)
    }

    pub(super) fn is_complete(&self) -> bool {
        !self.frozen
            && self
                .active
                .as_ref()
                .is_some_and(|batch| batch.entries.iter().all(|entry| entry.completed.is_some()))
    }

    pub(super) fn finish_batch(&mut self) -> Result<OriginalRequests, PairError> {
        self.live()?;
        if self.has_repair_account() {
            return Err(PairError::WrongPhase);
        }
        match self.finish_proof()? {
            BatchProof::Original(original) => Ok(original),
            BatchProof::Repair(_) => unreachable!("checked original batch"),
        }
    }

    pub(super) fn finish_repair_batch(&mut self) -> Result<RepairAccount, PairError> {
        self.live()?;
        if !self.has_repair_account() {
            return Err(PairError::WrongPhase);
        }
        match self.finish_proof()? {
            BatchProof::Repair(account) => Ok(account),
            BatchProof::Original(_) => unreachable!("checked repair batch"),
        }
    }

    fn finish_proof(&mut self) -> Result<BatchProof, PairError> {
        self.live()?;
        if self
            .active
            .as_ref()
            .is_none_or(|batch| batch.entries.iter().any(|entry| entry.completed.is_none()))
        {
            return Err(PairError::WrongPhase);
        }
        let batch = self.active.take().unwrap();
        self.closed.extend(
            batch
                .entries
                .into_iter()
                .map(|entry| entry.completed.unwrap()),
        );
        Ok(batch.proof)
    }

    /// Unrecorded starts remain in the context's pending-author account. A
    /// source is never guessed merely to fill the outstanding-pair structure.
    pub(super) fn outstanding(&self) -> Vec<OutstandingPair> {
        let Some(batch) = &self.active else {
            return Vec::new();
        };
        batch
            .entries
            .iter()
            .filter_map(|entry| {
                if entry.completed.is_some() {
                    return None;
                }
                let opening_source = entry.opening_source.clone()?;
                let has_statement = batch
                    .statements
                    .iter()
                    .any(|held| held.source() == &entry.started.statement.block_id);
                Some(OutstandingPair {
                    call_id: entry.started.call_id.clone(),
                    state: if entry.terminal_authoring {
                        PairState::Ending
                    } else if entry.opening.is_some() && has_statement {
                        PairState::Opened
                    } else {
                        PairState::Opening
                    },
                    execution: None,
                    opening_source,
                    opening_filed: entry.opening.is_some(),
                    terminal_source: entry.terminal_source.clone(),
                    terminal_filed: false,
                })
            })
            .collect()
    }

    pub(super) fn freeze(&mut self) {
        self.frozen = true;
    }

    fn live(&self) -> Result<(), PairError> {
        if self.frozen {
            Err(PairError::Frozen)
        } else {
            Ok(())
        }
    }

    fn entry(&self, call: &CallId) -> Result<&Entry, PairError> {
        self.active
            .as_ref()
            .and_then(|batch| {
                batch
                    .entries
                    .iter()
                    .find(|entry| &entry.started.call_id == call)
            })
            .ok_or(PairError::UnknownCall)
    }

    fn entry_mut(&mut self, call: &CallId) -> Result<&mut Entry, PairError> {
        self.active
            .as_mut()
            .and_then(|batch| {
                batch
                    .entries
                    .iter_mut()
                    .find(|entry| &entry.started.call_id == call)
            })
            .ok_or(PairError::UnknownCall)
    }

    fn author_pending(&self) -> bool {
        self.active.as_ref().is_some_and(|batch| {
            batch.entries.iter().any(|entry| {
                (entry.opening_authoring && entry.opening_source.is_none())
                    || (entry.terminal_authoring && entry.terminal_source.is_none())
            })
        })
    }

    fn source_used(&self, source: &SourceBlockId) -> bool {
        self.closed
            .iter()
            .any(|pair| pair.opening.source() == source || pair.terminal.source() == source)
            || self.active.as_ref().is_some_and(|batch| {
                batch.entries.iter().any(|entry| {
                    entry.opening_source.as_ref() == Some(source)
                        || entry.terminal_source.as_ref() == Some(source)
                })
            })
    }

    fn validate_receipt(&self, receipt: &CommittedSource) -> Result<(), PairError> {
        if receipt.common().session_id != self.session
            || receipt.common().run_id != self.run
            || receipt.common().validate().is_err()
            || receipt.source().run != self.run
            || receipt.terminal().source != *receipt.source()
            || receipt.terminal().outcome != TerminalOutcome::Filed
            || !matches!(receipt.receipt(), StateFrame::Filed { block_id, .. } if block_id == receipt.source())
        {
            return Err(PairError::WrongReceipt);
        }
        Ok(())
    }

    fn validate_pair_receipt(
        &self,
        entry: &Entry,
        phase: PairRecordPhase,
        receipt: &CommittedSource,
    ) -> Result<(), PairError> {
        self.validate_receipt(receipt)?;
        let LedgerFile::Pair(file) = receipt.file() else {
            return Err(PairError::WrongReceipt);
        };
        let batch = self.active.as_ref().ok_or(PairError::WrongPhase)?;
        if file.scheme_id != self.scheme.scheme_id
            || file.source.session != self.session
            || file.source.turn.as_deref()
                != Some(
                    batch
                        .proof
                        .original()
                        .generation()
                        .original()
                        .turn
                        .0
                        .as_str(),
                )
            || file.source.channel != "tool"
            || file.source.subsystem != "harness"
            || file.validate_structure().is_err()
        {
            return Err(PairError::WrongReceipt);
        }
        let valid = match phase {
            PairRecordPhase::Opening => {
                entry.opening_authoring
                    && entry.opening_source.as_ref() == Some(receipt.source())
                    && file.source.kind == "tool.call.started"
                    && file.source.origin == "model"
                    && matches!(&file.pair, PairRecord::Started(started) if started == &entry.started)
            }
            PairRecordPhase::Terminal => {
                entry.terminal_authoring
                    && entry.terminal_source.as_ref() == Some(receipt.source())
                    && file.source.kind == "tool.call.completed"
                    && file.source.origin == "loop"
                    && matches!(&file.pair, PairRecord::Ended(ended) if Some(ended) == entry.ending.as_ref())
            }
        };
        if !valid {
            return Err(PairError::WrongReceipt);
        }
        Ok(())
    }
}

fn dominant(old: Option<NonDispatchCause>, new: NonDispatchCause) -> NonDispatchCause {
    match (old, new) {
        (Some(NonDispatchCause::Unload), _) | (_, NonDispatchCause::Unload) => {
            NonDispatchCause::Unload
        }
        (Some(NonDispatchCause::Stop), _) | (_, NonDispatchCause::Stop) => NonDispatchCause::Stop,
        (Some(old), _) => old,
        (None, new) => new,
    }
}

#[cfg(test)]
#[path = "pairs_tests.rs"]
mod tests;
