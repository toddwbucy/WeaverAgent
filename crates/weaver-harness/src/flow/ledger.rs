//! Whole-copy installation and source-ordered filing under HARNESS-LEDGER.
//!
//! The coordinator supplies recorded port results. This owner keeps every source
//! obligation until its terminal flow record has crossed the trace barrier.

use serde_json::value::RawValue;
use weaver_trace::serving::{
    AdmittedCopy, ContentSchema, Kind, PathPart, SelectedBlock, content_schema,
};
use weaver_types::state_flow::{
    Admission, Common, MarkEffect, PairEnded, PairFailure, PairFile, PairLane, PairRecord,
    PairStarted, PlanEffect, PlanInputContext, SchemeDeclaration, SourceBlockId, SourceContext,
    SourceEnvelope, SourcePathPart, StateFrame, StateOperation, StatementEffect, StatementFile,
    Ste100Reply, Ste100Request, Ste100Result, Ste100Target, decode_strict,
};

use super::RecordedTerminal;
use super::channel::{CommitEvidence, ExchangeProgress, StateRequest};
use super::intake::RetentionBudget;

#[path = "receipts.rs"]
mod receipts;
pub(super) use receipts::{
    CommittedSource, ReceiptRegistrationFailure, ReceiptSet, RejectedSource, SourceDisposition,
};
#[path = "source_leases.rs"]
mod source_leases;
use source_leases::SourceLeases;
pub(super) use source_leases::{LeaseSlotStatus, SourceLease, SourceLeaseRegistrationFailure};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct LedgerError(pub(super) &'static str);

impl std::fmt::Display for LedgerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

impl std::error::Error for LedgerError {}

pub(super) use super::classifier::AcceptedReuse;

#[derive(Debug)]
pub(super) struct InstallFailure {
    pub(super) copy: AdmittedCopy,
    pub(super) reuse: Option<AcceptedReuse>,
    pub(super) error: LedgerError,
}

#[derive(Debug)]
pub(super) struct InstallReport {
    pub(super) source_sequence: u64,
    pub(super) sources: Vec<SourceBlockId>,
}

#[derive(Debug, Clone)]
pub(super) enum LedgerFile {
    Statement(Box<StatementFile>),
    Pair(Box<PairFile>),
}

impl LedgerFile {
    pub(super) fn source(&self) -> &SourceBlockId {
        match self {
            Self::Statement(file) => &file.block_id,
            Self::Pair(file) => &file.block_id,
        }
    }

    pub(super) fn into_request(self) -> StateRequest {
        match self {
            Self::Statement(file) => StateRequest::File(*file),
            Self::Pair(file) => StateRequest::FilePair(*file),
        }
    }

    fn envelope(&self) -> &SourceEnvelope {
        match self {
            Self::Statement(file) => &file.source,
            Self::Pair(file) => &file.source,
        }
    }

    fn operation(&self) -> StateOperation {
        match self {
            Self::Statement(_) => StateOperation::File,
            Self::Pair(_) => StateOperation::FilePair,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LedgerState {
    AwaitingClassification,
    ReadyToFile,
    Sending,
    AwaitingReceipt,
    FiledHeld,
    CorrectlyRejectedHeld,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TerminalOutcome {
    Filed,
    CorrectlyRejected,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TerminalCandidate {
    pub(super) source: SourceBlockId,
    pub(super) turn: Option<String>,
    pub(super) outcome: TerminalOutcome,
    pub(super) bytes: u64,
    pub(super) first_sent_ns: Option<u64>,
    pub(super) intake_ns: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ClassificationCandidate {
    pub(super) source: SourceBlockId,
    pub(super) text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Classified {
    Accepted(SourceBlockId),
    CorrectlyRejected(TerminalCandidate),
}

#[derive(Debug)]
pub(super) struct BlockFacts<'a> {
    pub(super) source: &'a SourceBlockId,
    pub(super) turn: Option<&'a str>,
    pub(super) raw_bytes: u64,
    pub(super) context_bytes: u64,
    pub(super) intake_ns: u64,
    pub(super) state: LedgerState,
    pub(super) content_type: weaver_trace::serving::payloads::CopyContentType,
    /// Disposition records can wait after a valid port result. The halt account
    /// retains the state whose result is held, with commit evidence separately.
    pub(super) prior_state: LedgerState,
    pub(super) progress: Option<&'a ExchangeProgress>,
    pub(super) first_sent_ns: Option<u64>,
}

struct Entry {
    file: LedgerFile,
    state: LedgerState,
    classification: Option<Ste100Request>,
    exchange: Option<Common>,
    progress: Option<ExchangeProgress>,
    raw_bytes: u64,
    context_bytes: u64,
    intake_ns: u64,
    first_sent_ns: Option<u64>,
    receipt: Option<StateFrame>,
    rejection: Option<Ste100Reply>,
}

impl Entry {
    fn terminal(&self) -> Option<TerminalCandidate> {
        let outcome = match self.state {
            LedgerState::FiledHeld => TerminalOutcome::Filed,
            LedgerState::CorrectlyRejectedHeld => TerminalOutcome::CorrectlyRejected,
            _ => return None,
        };
        Some(TerminalCandidate {
            source: self.file.source().clone(),
            turn: self.file.envelope().turn.clone(),
            outcome,
            // Installation checked this sum before transferring the copy.
            bytes: self.raw_bytes + self.context_bytes,
            first_sent_ns: self.first_sent_ns,
            intake_ns: self.intake_ns,
        })
    }
}

/// Source history retains ordinals, not disposed content. A mere sequence
/// watermark cannot distinguish a known selected block from an unknown one.
struct SourceHistory {
    sequence: u64,
    blocks: Vec<u64>,
}

pub(super) struct Ledger {
    run: String,
    session: String,
    scheme: SchemeDeclaration,
    entries: Vec<Entry>,
    history: Vec<SourceHistory>,
    frozen: bool,
    receipts: Option<ReceiptSet>,
    source_leases: SourceLeases,
}

impl Ledger {
    pub(super) fn new(
        run: String,
        session: String,
        scheme: SchemeDeclaration,
    ) -> Result<Self, LedgerError> {
        scheme
            .validate()
            .map_err(|_| LedgerError("invalid loaded scheme"))?;
        Self::with_scheme(run, session, scheme)
    }

    fn with_scheme(
        run: String,
        session: String,
        scheme: SchemeDeclaration,
    ) -> Result<Self, LedgerError> {
        if run.is_empty() || session.is_empty() {
            return Err(LedgerError("empty ledger run or session"));
        }
        Ok(Self {
            run,
            session,
            scheme,
            entries: Vec::new(),
            history: Vec::new(),
            frozen: false,
            receipts: None,
            source_leases: SourceLeases::new(),
        })
    }

    #[cfg(test)]
    pub(super) fn fixture(run: String, session: String) -> Self {
        Self::with_scheme(run, session, super::classifier::scripted::fixture_scheme()).unwrap()
    }

    pub(super) fn matches_identity(
        &self,
        run: &str,
        session: &str,
        scheme: &SchemeDeclaration,
    ) -> bool {
        self.run == run && self.session == session && self.scheme == *scheme
    }

    pub(super) fn install(
        &mut self,
        copy: AdmittedCopy,
        intake_ns: u64,
        reuse: Option<AcceptedReuse>,
    ) -> Result<InstallReport, Box<InstallFailure>> {
        self.source_leases.prune_abandoned();
        match self.prepare_copy(&copy, intake_ns, reuse.as_ref()) {
            Ok((prepared, history, sources)) => {
                // Reserve every growing collection before any block transfers.
                if self.entries.try_reserve(prepared.len()).is_err()
                    || self.history.try_reserve(1).is_err()
                {
                    return Err(Box::new(InstallFailure {
                        copy,
                        reuse,
                        error: LedgerError("whole-copy ledger reservation failed"),
                    }));
                }
                let source_sequence = history.sequence;
                self.entries.extend(prepared);
                self.history.push(history);
                Ok(InstallReport {
                    source_sequence,
                    sources,
                })
            }
            Err(error) => Err(Box::new(InstallFailure { copy, reuse, error })),
        }
    }

    pub(super) fn next_classification(&self) -> Option<ClassificationCandidate> {
        if self.frozen {
            return None;
        }
        self.entries.iter().find_map(|entry| {
            if entry.state != LedgerState::AwaitingClassification || entry.classification.is_some()
            {
                return None;
            }
            let LedgerFile::Statement(file) = &entry.file else {
                return None;
            };
            Some(ClassificationCandidate {
                source: file.block_id.clone(),
                text: file.text.clone(),
            })
        })
    }

    pub(super) fn classification_submitted(
        &mut self,
        source: &SourceBlockId,
        request: &Ste100Request,
    ) -> Result<(), LedgerError> {
        self.live()?;
        let scheme = &self.scheme.scheme_id;
        let entry = self
            .entries
            .iter_mut()
            .find(|entry| entry.file.source() == source)
            .ok_or(LedgerError(
                "classification names an unknown selected source",
            ))?;
        let LedgerFile::Statement(file) = &entry.file else {
            return Err(LedgerError("a pair record cannot be classified"));
        };
        if entry.state != LedgerState::AwaitingClassification
            || entry.classification.is_some()
            || request.validate().is_err()
            || request.scheme_id != *scheme
            || request.target != Ste100Target::Source(source.clone())
            || request.text != file.text
        {
            return Err(LedgerError(
                "classifier request differs from the selected statement",
            ));
        }
        entry.classification = Some(request.clone());
        Ok(())
    }

    /// The caller has released this exact reply through Classifier::recorded.
    pub(super) fn classify_recorded(
        &mut self,
        reply: &Ste100Reply,
    ) -> Result<Classified, LedgerError> {
        self.live()?;
        let Ste100Target::Source(source) = &reply.target else {
            return Err(LedgerError(
                "ledger classification requires a source target",
            ));
        };
        let entry = self
            .entries
            .iter_mut()
            .find(|entry| entry.file.source() == source)
            .ok_or(LedgerError(
                "classifier reply names an unknown selected source",
            ))?;
        let request = entry
            .classification
            .as_ref()
            .ok_or(LedgerError("source has no outstanding classifier request"))?;
        if entry.state != LedgerState::AwaitingClassification {
            return Err(LedgerError("source classification was already reduced"));
        }
        reply
            .validate_for(request, &self.scheme.verbs)
            .map_err(|_| LedgerError("classifier reply differs from its exact request"))?;
        match &reply.result {
            Ste100Result::Accepted { bindings } => {
                let LedgerFile::Statement(file) = &entry.file else {
                    return Err(LedgerError("accepted source is not a statement"));
                };
                let mut ready = (**file).clone();
                ready.bindings = bindings.clone();
                resolve_effect(&mut ready)?;
                ready
                    .validate_structure(&self.scheme.verbs)
                    .map_err(|_| LedgerError("accepted filing failed structural validation"))?;
                entry.file = LedgerFile::Statement(Box::new(ready));
                entry.state = LedgerState::ReadyToFile;
                Ok(Classified::Accepted(source.clone()))
            }
            Ste100Result::NotSte100 { .. } => {
                entry.rejection = Some(reply.clone());
                entry.state = LedgerState::CorrectlyRejectedHeld;
                Ok(Classified::CorrectlyRejected(
                    entry.terminal().expect("terminal state"),
                ))
            }
            Ste100Result::ComponentFailed { .. } => Err(LedgerError("classifier component failed")),
        }
    }

    pub(super) fn next_file(&self) -> Option<LedgerFile> {
        if self.frozen {
            return None;
        }
        self.entries
            .first()
            .filter(|entry| entry.state == LedgerState::ReadyToFile)
            .map(|entry| entry.file.clone())
    }

    pub(super) fn begin_file(
        &mut self,
        source: &SourceBlockId,
        common: &Common,
    ) -> Result<(), LedgerError> {
        self.live()?;
        if common.validate().is_err()
            || common.run_id != self.run
            || common.session_id != self.session
        {
            return Err(LedgerError("filing exchange belongs to another session"));
        }
        let entry = self
            .entries
            .first_mut()
            .ok_or(LedgerError("no selected filing is owed"))?;
        if entry.file.source() != source || entry.state != LedgerState::ReadyToFile {
            return Err(LedgerError("filing overtakes an earlier source obligation"));
        }
        entry.exchange = Some(common.clone());
        entry.state = LedgerState::Sending;
        Ok(())
    }

    pub(super) fn observe_progress(
        &mut self,
        progress: &ExchangeProgress,
        first_sent_ns: Option<u64>,
    ) -> Result<(), LedgerError> {
        self.live()?;
        let entry = self
            .entries
            .first_mut()
            .ok_or(LedgerError("no filing owns transport progress"))?;
        if !matches!(
            entry.state,
            LedgerState::Sending | LedgerState::AwaitingReceipt
        ) || entry.exchange.as_ref() != Some(&progress.common)
            || progress.source.as_ref() != Some(entry.file.source())
            || progress.operation != entry.file.operation()
            || progress.frame_bytes == 0
            || progress.bytes_sent > progress.frame_bytes
            || (progress.bytes_sent != 0) != first_sent_ns.is_some()
            || first_sent_ns.is_some_and(|sent| sent < entry.intake_ns)
            || entry
                .first_sent_ns
                .is_some_and(|sent| Some(sent) != first_sent_ns)
            || entry.progress.as_ref().is_some_and(|old| {
                old.frame_bytes != progress.frame_bytes || old.bytes_sent > progress.bytes_sent
            })
        {
            return Err(LedgerError(
                "transport progress differs from the frozen filing",
            ));
        }
        entry.progress = Some(progress.clone());
        entry.first_sent_ns = first_sent_ns;
        entry.state = if progress.bytes_sent == progress.frame_bytes {
            LedgerState::AwaitingReceipt
        } else {
            LedgerState::Sending
        };
        Ok(())
    }

    /// The sole channel has already released this answer after its measurement.
    pub(super) fn receipt_recorded(
        &mut self,
        answer: &StateFrame,
    ) -> Result<TerminalCandidate, LedgerError> {
        self.live()?;
        let StateFrame::Filed {
            common, block_id, ..
        } = answer
        else {
            return Err(LedgerError("a non-filed answer cannot finish a filing"));
        };
        let entry = self
            .entries
            .first_mut()
            .ok_or(LedgerError("receipt has no selected obligation"))?;
        if entry.state != LedgerState::AwaitingReceipt
            || entry.file.source() != block_id
            || entry.exchange.as_ref() != Some(common)
            || !entry.progress.as_ref().is_some_and(|progress| {
                progress.bytes_sent == progress.frame_bytes
                    && progress.commit == CommitEvidence::Committed
            })
        {
            return Err(LedgerError(
                "receipt differs from the complete committed filing",
            ));
        }
        entry.state = LedgerState::FiledHeld;
        entry.receipt = Some(answer.clone());
        Ok(entry.terminal().expect("terminal state"))
    }

    pub(super) fn terminal_candidate(&self, source: &SourceBlockId) -> Option<TerminalCandidate> {
        self.entries
            .iter()
            .find(|entry| entry.file.source() == source)
            .and_then(Entry::terminal)
    }

    pub(super) fn terminal_candidates(&self) -> impl Iterator<Item = TerminalCandidate> + '_ {
        self.entries.iter().filter_map(Entry::terminal)
    }

    pub(super) fn terminal_recorded(
        &mut self,
        proof: RecordedTerminal,
        budget: &mut RetentionBudget,
    ) -> Result<TerminalCandidate, LedgerError> {
        self.source_leases.prune_abandoned();
        self.live()?;
        let index = self
            .entries
            .iter()
            .position(|entry| entry.file.source() == proof.source())
            .ok_or(LedgerError("terminal proof names no selected obligation"))?;
        let terminal = self.entries[index].terminal().ok_or(LedgerError(
            "terminal proof preceded a held terminal disposition",
        ))?;
        let expected = match terminal.outcome {
            TerminalOutcome::Filed => weaver_trace::serving::payloads::FlowPhase::Filed,
            TerminalOutcome::CorrectlyRejected => {
                weaver_trace::serving::payloads::FlowPhase::CorrectlyRejected
            }
        };
        if proof.phase() != expected {
            return Err(LedgerError(
                "terminal proof reports a different source disposition",
            ));
        }
        let watched = self
            .receipts
            .as_ref()
            .and_then(|set| set.position(&terminal.source));
        let leased = self.source_leases.watching(&terminal.source);
        if watched.is_some() && leased.is_some() {
            return Err(LedgerError("source receipt has overlapping consumers"));
        }
        if let Some(slot) = watched
            && !self.receipts.as_ref().unwrap().slot_available(slot)
        {
            return Err(LedgerError("terminal receipt slot is already occupied"));
        }
        if leased.is_some_and(|slot| !self.source_leases.slot_available(slot)) {
            return Err(LedgerError("terminal lease slot is already occupied"));
        }
        if watched.is_some() || leased.is_some() {
            let entry = &self.entries[index];
            match terminal.outcome {
                TerminalOutcome::Filed => {
                    if !matches!(&entry.receipt, Some(StateFrame::Filed { common, block_id, .. })
                        if block_id == entry.file.source() && entry.exchange.as_ref() == Some(common))
                        || entry.rejection.is_some()
                    {
                        return Err(LedgerError("committed source lost its exact filed answer"));
                    }
                }
                TerminalOutcome::CorrectlyRejected => {
                    if !matches!(&entry.file, LedgerFile::Statement(_))
                        || !matches!(&entry.rejection, Some(Ste100Reply {
                            target: Ste100Target::Source(source),
                            result: Ste100Result::NotSte100 { .. }, ..
                        }) if source == entry.file.source())
                        || entry.receipt.is_some()
                    {
                        return Err(LedgerError(
                            "rejected source lost its exact classifier result",
                        ));
                    }
                }
            }
        }
        budget
            .release(proof)
            .map_err(|_| LedgerError("terminal retention release failed"))?;
        let entry = self.entries.remove(index);
        if watched.is_some() || leased.is_some() {
            let disposition = match terminal.outcome {
                TerminalOutcome::Filed => SourceDisposition::Committed(CommittedSource::new(
                    entry.file,
                    entry.receipt.expect("checked exact committed answer"),
                    terminal.clone(),
                )),
                TerminalOutcome::CorrectlyRejected => {
                    let LedgerFile::Statement(file) = entry.file else {
                        unreachable!("checked rejected statement")
                    };
                    SourceDisposition::CorrectlyRejected(RejectedSource::new(
                        file,
                        entry.rejection.expect("checked exact rejection"),
                        terminal.clone(),
                    ))
                }
            };
            if let Some(slot) = watched {
                self.receipts.as_mut().unwrap().put(slot, disposition);
            } else {
                self.source_leases
                    .put(leased.expect("checked lease consumer"), disposition);
            }
        }
        Ok(terminal)
    }

    /// Register before these exact retained sources can dispose. The context
    /// reserves its author operation earlier, then supplies actual installed IDs.
    /// History alone cannot register a source that has already been removed.
    pub(super) fn register_receipts(
        &mut self,
        sources: Vec<SourceBlockId>,
    ) -> Result<(), ReceiptRegistrationFailure> {
        self.source_leases.prune_abandoned();
        let validation = (|| {
            self.live()?;
            if self.receipts.is_some() {
                return Err(LedgerError("another source receipt operation owns custody"));
            }
            u64::try_from(sources.len())
                .map_err(|_| LedgerError("receipt source count overflow"))?;
            let mut total = 0_u64;
            let mut previous = None;
            for source in &sources {
                let position = (source.sequence.0, source.block.0);
                if source.run != self.run || previous.is_some_and(|old| old >= position) {
                    return Err(LedgerError(
                        "receipt sources are foreign, duplicate or out of order",
                    ));
                }
                if self.source_leases.watches(source) {
                    return Err(LedgerError("receipt source already has a lease consumer"));
                }
                let entry = self
                    .entries
                    .iter()
                    .find(|entry| entry.file.source() == source)
                    .ok_or(LedgerError("receipt source is unknown or already disposed"))?;
                total = total
                    .checked_add(entry.raw_bytes)
                    .and_then(|bytes| bytes.checked_add(entry.context_bytes))
                    .ok_or(LedgerError("receipt source byte count overflow"))?;
                previous = Some(position);
            }
            Ok(())
        })();
        if let Err(error) = validation {
            return Err(ReceiptRegistrationFailure { sources, error });
        }
        self.receipts = Some(ReceiptSet::new(sources)?);
        Ok(())
    }

    /// Register exact currently retained sources without imposing a group wait.
    /// Each slot transfers separately after the same terminal recording proof.
    pub(super) fn register_source_lease(
        &mut self,
        sources: Vec<SourceBlockId>,
    ) -> Result<SourceLease, SourceLeaseRegistrationFailure> {
        self.source_leases.prune_abandoned();
        let validation = (|| {
            self.live()?;
            u64::try_from(sources.len()).map_err(|_| LedgerError("source lease count overflow"))?;
            let mut previous = None;
            let mut total = 0_u64;
            for source in &sources {
                let position = (source.sequence.0, source.block.0);
                if source.run != self.run || previous.is_some_and(|old| old >= position) {
                    return Err(LedgerError(
                        "lease sources are foreign, duplicate or out of order",
                    ));
                }
                if self.source_leases.watches(source)
                    || self
                        .receipts
                        .as_ref()
                        .is_some_and(|set| set.position(source).is_some())
                {
                    return Err(LedgerError("lease source already has a receipt consumer"));
                }
                let entry = self
                    .entries
                    .iter()
                    .find(|entry| entry.file.source() == source)
                    .ok_or(LedgerError("lease source is unknown or already disposed"))?;
                total = total
                    .checked_add(entry.raw_bytes)
                    .and_then(|bytes| bytes.checked_add(entry.context_bytes))
                    .ok_or(LedgerError("source lease byte count overflow"))?;
                previous = Some(position);
            }
            Ok(())
        })();
        if let Err(error) = validation {
            return Err(SourceLeaseRegistrationFailure { sources, error });
        }
        self.source_leases.register(sources)
    }

    pub(super) fn source_lease_sources<'a>(
        &'a self,
        lease: &SourceLease,
    ) -> Result<impl Iterator<Item = &'a SourceBlockId>, LedgerError> {
        self.live()?;
        self.source_leases.sources(lease)
    }

    pub(super) fn source_lease_status(
        &self,
        lease: &SourceLease,
        source: &SourceBlockId,
    ) -> Result<LeaseSlotStatus, LedgerError> {
        self.live()?;
        self.source_leases.status(lease, source)
    }

    pub(super) fn take_source_receipt(
        &mut self,
        lease: &SourceLease,
        source: &SourceBlockId,
    ) -> Result<Option<SourceDisposition>, LedgerError> {
        self.live()?;
        self.source_leases.take(lease, source)
    }

    /// Relinquishing consumer custody never removes a selected filing obligation.
    /// Cleanup is also permitted after freeze and returns no receipt capability.
    pub(super) fn release_source_receipt(
        &mut self,
        lease: &SourceLease,
        source: &SourceBlockId,
    ) -> Result<(), LedgerError> {
        self.source_leases.release_source(lease, source)
    }

    pub(super) fn release_source_lease(
        &mut self,
        lease: SourceLease,
    ) -> Result<(), (SourceLease, LedgerError)> {
        self.source_leases.release(lease)
    }

    #[cfg(test)]
    pub(super) fn review_set_taken_source_status(
        &mut self,
        lease: &SourceLease,
        source: &SourceBlockId,
        status: LeaseSlotStatus,
    ) {
        self.source_leases
            .review_set_taken_status(lease, source, status);
    }

    pub(super) fn release_source_leases(
        &mut self,
        leases: &[&SourceLease],
    ) -> Result<(), LedgerError> {
        self.source_leases.release_batch(leases)
    }

    /// Retained consumer facts are separate from owed filing gauges and barriers.
    pub(super) fn has_source_leases(&self) -> bool {
        !self.source_leases.is_empty()
    }

    pub(super) fn source_lease_count(&self) -> usize {
        self.source_leases.len()
    }

    pub(super) fn source_lease_retained(
        &self,
        lease: &SourceLease,
    ) -> Result<(u64, u64), LedgerError> {
        self.source_leases.retained(lease)
    }

    pub(super) fn receipts(&self) -> Option<&ReceiptSet> {
        self.receipts.as_ref()
    }

    pub(super) fn take_recorded_pair_terminal(
        &mut self,
        source: &SourceBlockId,
    ) -> Option<CommittedSource> {
        let receipt = self
            .receipts
            .as_mut()?
            .take_recorded_pair_terminal(source)?;
        self.receipts = None;
        Some(receipt)
    }

    /// Retain actual completed evidence without releasing a consumer grant.
    pub(super) fn retain_boot_filing_receipts(&mut self) -> Result<ReceiptSet, LedgerError> {
        self.live()?;
        if !self.receipts.as_ref().is_some_and(ReceiptSet::is_complete) {
            return Err(LedgerError(
                "boot rollback requires complete recorded receipts",
            ));
        }
        Ok(self.receipts.take().expect("complete receipt set"))
    }

    /// No completion crosses this port on failure or before every subscribed
    /// disposition was recorded. Taking it relinquishes the one subscription.
    pub(super) fn take_receipts(&mut self) -> Result<Option<ReceiptSet>, LedgerError> {
        self.live()?;
        if self.receipts.as_ref().is_some_and(ReceiptSet::is_complete) {
            let mut set = self.receipts.take().expect("complete receipt set");
            set.release();
            Ok(Some(set))
        } else {
            Ok(None)
        }
    }

    /// Cancellation removes only the consumer subscription. Already filed facts
    /// remain filed, pending obligations retain their original charge, and the
    /// returned account cannot yield committed grants, including on halt.
    pub(super) fn cancel_receipts(&mut self) -> Option<ReceiptSet> {
        self.receipts.take().map(|mut set| {
            set.cancel();
            set
        })
    }

    pub(super) fn split_source_lease(
        &mut self,
        lease: &SourceLease,
        source: &SourceBlockId,
    ) -> Result<SourceLease, LedgerError> {
        self.source_leases.split(lease, source)
    }

    pub(super) fn owned_pending_text(&self, lease: &SourceLease, source: &SourceBlockId) -> bool {
        (self
            .source_leases
            .status(lease, source)
            .is_ok_and(|status| status == LeaseSlotStatus::Pending)
            && self.entries.iter().any(|entry| {
                entry.file.source() == source
                    && matches!(&entry.file,LedgerFile::Statement(file) if !file.text.is_empty())
            }))
            || self.source_leases.retained_whole_text(lease, source)
    }

    pub(super) fn barrier(&self, sources: &[SourceBlockId]) -> Result<bool, LedgerError> {
        self.live()?;
        let mut complete = true;
        for source in sources {
            if source.run != self.run
                || !self.history.iter().any(|known| {
                    known.sequence == source.sequence.0 && known.blocks.contains(&source.block.0)
                })
            {
                return Err(LedgerError("barrier names an unknown selected source"));
            }
            complete &= !self
                .entries
                .iter()
                .any(|entry| entry.file.source() == source);
        }
        Ok(complete)
    }

    pub(super) fn barrier_through(&self, sequence: u64) -> bool {
        !self.frozen
            && !self
                .entries
                .iter()
                .any(|entry| entry.file.source().sequence.0 <= sequence)
    }

    pub(super) fn facts(&self) -> impl Iterator<Item = BlockFacts<'_>> {
        self.entries.iter().map(|entry| BlockFacts {
            source: entry.file.source(),
            turn: entry.file.envelope().turn.as_deref(),
            raw_bytes: entry.raw_bytes,
            context_bytes: entry.context_bytes,
            intake_ns: entry.intake_ns,
            state: entry.state,
            content_type: match entry.file {
                LedgerFile::Statement(_) => {
                    weaver_trace::serving::payloads::CopyContentType::Statement
                }
                LedgerFile::Pair(_) => weaver_trace::serving::payloads::CopyContentType::Pair,
            },
            prior_state: match entry.state {
                LedgerState::FiledHeld => LedgerState::AwaitingReceipt,
                LedgerState::CorrectlyRejectedHeld => LedgerState::AwaitingClassification,
                state => state,
            },
            progress: entry.progress.as_ref(),
            first_sent_ns: entry.first_sent_ns,
        })
    }

    pub(super) fn freeze(&mut self) {
        self.frozen = true;
    }

    pub(super) fn boot_filing_matches(
        &self,
        source: Option<&SourceBlockId>,
        expected: &PairEnded,
    ) -> bool {
        let matches_file = |file: &LedgerFile| {
            source == Some(file.source())
                && matches!(file, LedgerFile::Pair(file)
                    if file.source.turn.is_none()
                        && matches!(&file.pair, PairRecord::Ended(ended) if ended == expected))
        };
        self.entries.iter().all(|entry| matches_file(&entry.file))
            && self.entries.len() <= 1
            && self.receipts.as_ref().is_none_or(|receipts| {
                receipts.sources().all(|held| Some(held) == source)
                    && receipts.sources().count() == 1
                    && receipts
                        .recorded_dispositions()
                        .all(|receipt| match receipt {
                            SourceDisposition::Committed(receipt) => matches_file(receipt.file()),
                            SourceDisposition::CorrectlyRejected(_) => false,
                        })
            })
    }

    #[cfg(test)]
    pub(super) fn test_boot_filing_evidence(&self, retained: Option<&ReceiptSet>) -> String {
        format!(
            "{:?}",
            (
                &self.run,
                &self.session,
                &self.scheme,
                self.entries
                    .iter()
                    .map(|entry| (
                        &entry.file,
                        &entry.classification,
                        &entry.exchange,
                        entry.raw_bytes,
                        entry.context_bytes,
                        entry.intake_ns,
                        &entry.receipt,
                        &entry.rejection,
                    ))
                    .collect::<Vec<_>>(),
                self.receipts.as_ref().or(retained),
            )
        )
    }

    fn live(&self) -> Result<(), LedgerError> {
        if self.frozen {
            Err(LedgerError("ledger is frozen for halt"))
        } else {
            Ok(())
        }
    }

    fn prepare_copy(
        &self,
        copy: &AdmittedCopy,
        intake_ns: u64,
        reuse: Option<&AcceptedReuse>,
    ) -> Result<(Vec<Entry>, SourceHistory, Vec<SourceBlockId>), LedgerError> {
        self.live()?;
        let (raw_source, rule, blocks) = match copy {
            AdmittedCopy::Statement(copy) => (
                &copy.source_envelope,
                &copy.partition_rule_id,
                copy.blocks.as_slice(),
            ),
            AdmittedCopy::Pair(copy) => (
                &copy.source_envelope,
                &copy.partition_rule_id,
                std::slice::from_ref(&copy.block),
            ),
        };
        let source: SourceEnvelope =
            decode_strict(raw_source).map_err(|_| LedgerError("invalid copied source envelope"))?;
        if source.run != self.run
            || source.session != self.session
            || source.sequence.0 == 0
            || rule.is_empty()
            || self
                .history
                .last()
                .is_some_and(|old| source.sequence.0 <= old.sequence)
        {
            return Err(LedgerError(
                "copied source is foreign, repeated, or out of order",
            ));
        }
        let kind = Kind::from_name(&source.kind).ok_or(LedgerError("unknown copied event kind"))?;
        let schema = content_schema(kind);
        if !licensed_source(kind, &source) {
            return Err(LedgerError("copied source envelope is not licensed"));
        }
        let mut entries = Vec::new();
        let mut ordinals = Vec::new();
        let mut sources = Vec::new();
        entries
            .try_reserve(blocks.len())
            .map_err(|_| LedgerError("whole-copy allocation failed"))?;
        ordinals
            .try_reserve(blocks.len())
            .map_err(|_| LedgerError("whole-copy allocation failed"))?;
        sources
            .try_reserve(blocks.len())
            .map_err(|_| LedgerError("whole-copy allocation failed"))?;
        if reuse.is_some() && (blocks.len() != 1 || !matches!(copy, AdmittedCopy::Statement(_))) {
            return Err(LedgerError("input reuse requires one selected Text block"));
        }
        if let AdmittedCopy::Statement(statement) = copy {
            if !matches!(
                schema,
                ContentSchema::Message | ContentSchema::OneText { .. } | ContentSchema::Plan
            ) || statement.context.is_some()
                && blocks.first().is_none_or(|block| block.source.block != 0)
            {
                return Err(LedgerError(
                    "statement copy is not licensed by the content registry",
                ));
            }
        } else if schema != ContentSchema::Pair {
            return Err(LedgerError(
                "pair copy is not licensed by the content registry",
            ));
        }
        for block in blocks {
            let id = block_id(block);
            if id.run != source.run
                || id.sequence != source.sequence
                || ordinals
                    .last()
                    .is_some_and(|previous| id.block.0 <= *previous)
            {
                return Err(LedgerError("selected original block identities disagree"));
            }
            let raw_bytes = u64::try_from(block.raw.len())
                .map_err(|_| LedgerError("raw byte count overflow"))?;
            let mut context_bytes = 0;
            let (file, state) = match copy {
                AdmittedCopy::Statement(statement) => {
                    let text = decode_text(&block.raw)?;
                    if !valid_text_path(&block.source_path, schema, id.block.0) {
                        return Err(LedgerError(
                            "selected block is not a complete original Text block",
                        ));
                    }
                    let context = if id.block.0 == 0 {
                        statement
                            .context
                            .as_ref()
                            .map(|context| {
                                if context.source_path != [PathPart::Key("context".into())] {
                                    return Err(LedgerError(
                                        "context is not the original block-zero sidecar",
                                    ));
                                }
                                context_bytes = u64::try_from(context.raw.len())
                                    .map_err(|_| LedgerError("context byte count overflow"))?;
                                Ok(SourceContext {
                                    source_path: path(&context.source_path),
                                    raw: raw(&context.raw)?,
                                })
                            })
                            .transpose()?
                    } else {
                        None
                    };
                    let mut filing = StatementFile {
                        block_id: id.clone(),
                        source: source.clone(),
                        partition_rule_id: rule.clone(),
                        source_path: path(&block.source_path),
                        admission: Admission::Admitted,
                        raw_block: raw(&block.raw)?,
                        text,
                        scheme_id: self.scheme.scheme_id.clone(),
                        bindings: Vec::new(),
                        context,
                        effect: None,
                    };
                    let state = if let Some(reuse) = reuse {
                        if source.kind != "message.user"
                            || source.channel != "input"
                            || !matches!(source.origin.as_str(), "operator" | "external")
                            || id.block.0 != 0
                            || reuse.run() != self.run
                            || source.causal_parent != Some(reuse.offered_sequence())
                            || reuse.offered_sequence() >= source.sequence
                            || filing.text != reuse.text()
                            || filing.scheme_id != reuse.scheme_id()
                        {
                            return Err(LedgerError(
                                "accepted input differs from its checked offer",
                            ));
                        }
                        filing.bindings = reuse.bindings().to_vec();
                        resolve_effect(&mut filing)?;
                        filing
                            .validate_structure(&self.scheme.verbs)
                            .map_err(|_| LedgerError("reused input bindings are invalid"))?;
                        LedgerState::ReadyToFile
                    } else {
                        LedgerState::AwaitingClassification
                    };
                    (LedgerFile::Statement(Box::new(filing)), state)
                }
                AdmittedCopy::Pair(_) => {
                    let pair = match kind {
                        Kind::ToolCallStarted => PairRecord::Started(
                            decode_strict::<PairStarted>(&block.raw)
                                .map_err(|_| LedgerError("invalid whole pair opening"))?,
                        ),
                        Kind::ToolCallCompleted => PairRecord::Ended(
                            decode_strict::<PairEnded>(&block.raw)
                                .map_err(|_| LedgerError("invalid whole pair terminal"))?,
                        ),
                        _ => return Err(LedgerError("wrong pair event kind")),
                    };
                    let licensed = match &pair {
                        PairRecord::Started(record) => {
                            source.turn.is_some()
                                && match record.lane {
                                    PairLane::Review => source.origin == "loop",
                                    PairLane::External | PairLane::Recall => {
                                        source.origin == "model"
                                    }
                                }
                        }
                        PairRecord::Ended(record) => {
                            if matches!(record.reason, Some(PairFailure::AbandonedAtLoad {})) {
                                source.turn.is_none() && source.origin == "loop"
                            } else {
                                source.turn.is_some()
                            }
                        }
                    };
                    if !licensed {
                        return Err(LedgerError(
                            "pair payload and envelope attribution disagree",
                        ));
                    }
                    let filing = PairFile {
                        block_id: id.clone(),
                        source: source.clone(),
                        partition_rule_id: rule.clone(),
                        source_path: path(&block.source_path),
                        admission: Admission::Admitted,
                        raw_block: raw(&block.raw)?,
                        scheme_id: self.scheme.scheme_id.clone(),
                        pair,
                    };
                    filing
                        .validate_structure()
                        .map_err(|_| LedgerError("pair filing differs from its whole copy"))?;
                    (LedgerFile::Pair(Box::new(filing)), LedgerState::ReadyToFile)
                }
            };
            raw_bytes
                .checked_add(context_bytes)
                .ok_or(LedgerError("block traffic byte count overflow"))?;
            entries.push(Entry {
                file,
                state,
                classification: None,
                exchange: None,
                progress: None,
                raw_bytes,
                context_bytes,
                intake_ns,
                first_sent_ns: None,
                receipt: None,
                rejection: None,
            });
            ordinals.push(id.block.0);
            sources.push(id);
        }
        Ok((
            entries,
            SourceHistory {
                sequence: source.sequence.0,
                blocks: ordinals,
            },
            sources,
        ))
    }
}

fn decode_text(raw: &str) -> Result<String, LedgerError> {
    let value: serde_json::Value =
        decode_strict(raw).map_err(|_| LedgerError("selected block is not exact Text"))?;
    let object = value
        .as_object()
        .ok_or(LedgerError("selected block is not a Text object"))?;
    if object.len() != 2 || object.get("type").and_then(serde_json::Value::as_str) != Some("text") {
        return Err(LedgerError("selected block is not exact Text"));
    }
    object
        .get("text")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .ok_or(LedgerError("selected Text block lacks its exact string"))
}

fn block_id(block: &SelectedBlock) -> SourceBlockId {
    SourceBlockId {
        run: block.source.run.clone(),
        sequence: block.source.sequence.0.into(),
        block: block.source.block.into(),
    }
}

fn path(source: &[PathPart]) -> Vec<SourcePathPart> {
    source
        .iter()
        .map(|part| match part {
            PathPart::Key(key) => SourcePathPart::Key(key.clone()),
            PathPart::Index(index) => SourcePathPart::Index {
                index: (*index).into(),
            },
        })
        .collect()
}

fn valid_text_path(path: &[PathPart], schema: ContentSchema, index: u64) -> bool {
    match (schema, path) {
        (ContentSchema::Message, [PathPart::Key(content), PathPart::Index(original)]) => {
            content == "content" && *original == index
        }
        (
            ContentSchema::OneText { .. } | ContentSchema::Plan,
            [
                PathPart::Key(message),
                PathPart::Key(content),
                PathPart::Index(original),
            ],
        ) => message == "message" && content == "content" && *original == index,
        _ => false,
    }
}

fn licensed_source(kind: Kind, source: &SourceEnvelope) -> bool {
    match kind {
        Kind::MessageUser => {
            source.channel == "input"
                && matches!(source.origin.as_str(), "operator" | "external")
                && source.turn.is_some()
        }
        Kind::MessageAssistant => {
            source.channel == "output" && source.origin == "model" && source.turn.is_some()
        }
        Kind::LoopInstruction | Kind::SortDecision | Kind::StepOutcome | Kind::StateMark => {
            source.channel == "internal"
                && source.origin == "loop"
                && (!matches!(kind, Kind::StepOutcome | Kind::StateMark) || source.turn.is_some())
        }
        Kind::PlanEvent => {
            (source.channel == "input" && source.origin == "operator")
                || (source.channel == "internal" && source.origin == "loop")
        }
        Kind::ToolCallStarted => {
            source.channel == "tool" && matches!(source.origin.as_str(), "model" | "loop")
        }
        Kind::ToolCallCompleted => {
            source.channel == "tool" && matches!(source.origin.as_str(), "tool" | "loop")
        }
        _ => false,
    }
}

fn raw(text: &str) -> Result<Box<RawValue>, LedgerError> {
    RawValue::from_string(text.to_owned()).map_err(|_| LedgerError("invalid copied raw JSON"))
}

fn resolve_effect(file: &mut StatementFile) -> Result<(), LedgerError> {
    let source = &file.source;
    let block_zero = file.block_id.block.0 == 0;
    if !block_zero {
        if file.context.is_some() {
            return Err(LedgerError("context belongs only to block zero"));
        }
        return Ok(());
    }
    if source.kind == "plan.event" && source.channel == "input" && source.origin == "operator" {
        let context = file
            .context
            .as_ref()
            .ok_or(LedgerError("operator plan input lacks context"))?;
        let _: PlanInputContext = decode_strict(context.raw.get())
            .map_err(|_| LedgerError("invalid operator plan provenance"))?;
        return Ok(());
    }
    if source.channel == "internal" && source.origin == "loop" {
        match source.kind.as_str() {
            "state.mark" => {
                let context = file
                    .context
                    .as_ref()
                    .ok_or(LedgerError("mark statement lacks context"))?;
                let effect: MarkEffect = decode_strict(context.raw.get())
                    .map_err(|_| LedgerError("invalid mark context"))?;
                if !matches!(effect, MarkEffect::BeginTask { .. }) {
                    return Err(LedgerError("mark effect is not directly licensed"));
                }
                file.effect = Some(StatementEffect::Mark(effect));
                return Ok(());
            }
            "plan.event" | "step.outcome" => {
                let context = file
                    .context
                    .as_ref()
                    .ok_or(LedgerError("mechanical statement lacks context"))?;
                if source.kind == "step.outcome" && context.raw.get().trim() == "null" {
                    file.context = None;
                    return Ok(());
                }
                let effect: PlanEffect = decode_strict(context.raw.get())
                    .map_err(|_| LedgerError("invalid plan effect context"))?;
                if (source.kind == "step.outcome") != matches!(effect, PlanEffect::Close { .. }) {
                    return Err(LedgerError("plan effect belongs to another event kind"));
                }
                file.effect = Some(StatementEffect::Plan(Box::new(effect)));
                return Ok(());
            }
            _ => {}
        }
    }
    if file.context.is_some()
        || matches!(
            source.kind.as_str(),
            "state.mark" | "plan.event" | "step.outcome"
        )
    {
        return Err(LedgerError("unexpected or missing licensed context"));
    }
    Ok(())
}

#[cfg(test)]
#[path = "ledger_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "source_leases_tests.rs"]
mod source_leases_tests;
