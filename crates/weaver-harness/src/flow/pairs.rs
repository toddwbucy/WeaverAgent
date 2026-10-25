//! Passive pair custody. The context owns every author ticket and socket operation.
//! Dispatch requires a sealed effective emission and both real commits. A gate
//! answer remains owned until its exact terminal commits, never itself a grant.

use std::collections::HashMap;
use std::sync::Arc;

use weaver_traits::ContentBlock;
use weaver_types::state_flow::{
    CallId, NonDispatchCause, PairEnded, PairFailure, PairLane, PairRecord, PairRequest,
    PairStarted, SHELL_MAX_CLOCK_MS, SchemeDeclaration, SourceBlockId, SourcePosition, StateFrame,
    Ste100Result, Ste100Target, TerminalPairFate,
};

use super::repair::{EffectiveEmission, RepairAccount};
use crate::flow::coordinator::{CheckedGeneration, OriginalRequests};
use crate::flow::ledger::{CommittedSource, LedgerFile, SourceLease, TerminalOutcome};
use crate::flow::pump_transport::{OrganPort, OrganSend, ReceivedOrgan};
use crate::flow::report::{OutstandingPair, PairState};
use weaver_types::{
    ExchangeId, KillCause, Opener, OrganEnvelope, Position, ToolExecution, ToolName, ToolOutcome,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PairError {
    Frozen,
    InvalidIdentity,
    WrongScheme,
    Busy,
    UnknownCall,
    NotReady,
    AlreadyDelivered,
    WrongPhase,
    DuplicateCall,
    DuplicateSource,
    WrongReceipt,
    WrongStatement,
    Allocation,
    WrongExchange,
    InvalidClock,
    UnsupportedLane,
    InvalidLineage,
    Protocol,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum GateObservation {
    Matched(CallId),
    Rejected,
    Halted(CallId),
}

/// Raw received custody remains here until the context records this disposition.
pub(super) struct RejectedGateReturn {
    record: weaver_trace::serving::payloads::RejectedReturn,
    received: ReceivedOrgan,
}

impl RejectedGateReturn {
    pub(super) fn record(&self) -> &weaver_trace::serving::payloads::RejectedReturn {
        &self.record
    }
    pub(super) fn raw_bytes(&self) -> &[u8] {
        self.received.raw_bytes()
    }
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
    received: Option<ReceivedOrgan>,
    inward: Option<super::inward_recall::InwardEvidence>,
}

impl CompletedPair {
    pub(in crate::flow) fn raw_answer(&self) -> Option<&[u8]> {
        self.received.as_ref().map(ReceivedOrgan::raw_bytes)
    }
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
    pub(in crate::flow) fn recalled(&self) -> Option<&super::RecordedRead> {
        let super::inward_recall::InwardEvidence::Read(completion) = self.inward.as_ref()? else {
            return None;
        };
        match completion.as_ref() {
            super::ReadCompletion::Recorded(read) => Some(read),
            _ => None,
        }
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
    execution: Option<ExchangeId>,
    sent: bool,
    received: Option<ReceivedOrgan>,
    gate_terminal: bool,
    internal_started: bool,
    inward: Option<super::inward_recall::InwardEvidence>,
}

/// Only the association or repair owner can construct either proof. A public
/// PairStarted vector is never sufficient to enter the accounting ledger.
enum BatchProof {
    Original(OriginalRequests),
    Repair(RepairAccount),
    Effective(EffectiveEmission),
}

impl BatchProof {
    fn original(&self) -> &OriginalRequests {
        match self {
            Self::Original(original) => original,
            Self::Repair(account) => account.original(),
            Self::Effective(effective) => effective.account().original(),
        }
    }

    fn requests(&self) -> impl Iterator<Item = &PairStarted> {
        let (original, repair) = match self {
            Self::Original(original) => (Some(original), None),
            Self::Repair(account) => (None, Some(account)),
            Self::Effective(effective) => (None, Some(effective.account())),
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
            Self::Effective(effective) => (None, Some(effective.account())),
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

fn call_key(call: &CallId) -> (String, u64, u64) {
    (call.run.clone(), call.sequence.0, call.call.0)
}

struct RetainedPair {
    pair: Arc<CompletedPair>,
    result_record: Option<weaver_trace::Sequence>,
    delivered: Option<weaver_trace::Sequence>,
}
impl std::ops::Deref for RetainedPair {
    type Target = CompletedPair;
    fn deref(&self) -> &CompletedPair {
        &self.pair
    }
}

pub(super) struct PairLedger {
    session: String,
    run: String,
    scheme: SchemeDeclaration,
    active: Option<Batch>,
    closed: Vec<RetainedPair>,
    closed_by_call: HashMap<(String, u64, u64), usize>,
    frozen: bool,
    gate_failed: bool,
    rejected: Option<RejectedGateReturn>,
    rejected_sources: Vec<SourceBlockId>,
    cancelled_exchanges: Vec<(ExchangeId, CallId)>,
}

impl PairLedger {
    /// Only successful repair release authorizes execution. Accounting owners
    /// obtained on rejection or exhaustion cannot enter this arm.
    #[allow(clippy::result_large_err)]
    pub(super) fn accept_effective(
        &mut self,
        effective: EffectiveEmission,
    ) -> Result<(), (EffectiveEmission, PairError)> {
        self.accept_proof(BatchProof::Effective(effective))
            .map_err(|(proof, error)| match proof {
                BatchProof::Effective(effective) => (effective, error),
                _ => unreachable!("effective proof returned unchanged"),
            })
    }

    pub(super) fn has_effective(&self) -> bool {
        self.active
            .as_ref()
            .is_some_and(|batch| matches!(batch.proof, BatchProof::Effective(_)))
    }

    /// The first unfinished request is a barrier, including an inward lane.
    /// Never skip a recall to execute a later external call.
    pub(super) fn next_ready(&self) -> Option<&PairStarted> {
        if self.frozen || self.gate_failed || self.rejected.is_some() || self.author_pending() {
            return None;
        }
        let batch = self.active.as_ref()?;
        if !matches!(batch.proof, BatchProof::Effective(_)) || batch.cause.is_some() {
            return None;
        }
        let entry = batch
            .entries
            .iter()
            .find(|entry| entry.completed.is_none())?;
        if entry.opening.is_none()
            || entry.execution.is_some()
            || entry.internal_started
            || entry.ending.is_some()
            || !batch
                .statements
                .iter()
                .any(|receipt| receipt.source() == &entry.started.statement.block_id)
        {
            return None;
        }
        Some(&entry.started)
    }

    /// Exercise the helper's defensive empty-input boundary after real receipts.
    /// This deliberately differs from the immutable opening evidence and cannot
    /// be produced by checked noun spans. It creates no production proof door.
    #[cfg(test)]
    pub(super) fn empty_ready_inward_subject_for_test(&mut self, call: &CallId) {
        assert_eq!(self.next_ready().map(|ready| &ready.call_id), Some(call));
        let PairRequest::Recall { subject } = &mut self.entry_mut(call).unwrap().started.request
        else {
            panic!("fixture requires a ready recall");
        };
        subject.clear();
    }

    /// Isolate a defensive delivery gate after real pair and append receipts.
    /// This corruption instrument is absent from production builds.
    #[cfg(test)]
    pub(super) fn remove_result_record_for_test(
        &mut self,
        call: &CallId,
    ) -> weaver_trace::Sequence {
        self.closed[*self.closed_by_call.get(&call_key(call)).unwrap()]
            .result_record
            .take()
            .unwrap()
    }

    pub(super) fn start_inward(&mut self, call: &CallId) -> Result<(), PairError> {
        self.live()?;
        let ready = self
            .next_ready()
            .filter(|ready| &ready.call_id == call)
            .ok_or(PairError::WrongPhase)?;
        if ready.lane != PairLane::Recall || !matches!(ready.request, PairRequest::Recall { .. }) {
            return Err(PairError::UnsupportedLane);
        }
        self.entry_mut(call)?.internal_started = true;
        Ok(())
    }

    pub(super) fn finish_inward(
        &mut self,
        call: &CallId,
        evidence: super::inward_recall::InwardEvidence,
    ) -> Result<(), (super::inward_recall::InwardEvidence, PairError)> {
        let checked = (|| {
            self.live()?;
            let entry = self.entry(call)?;
            if !entry.internal_started || entry.ending.is_some() || entry.completed.is_some() {
                return Err(PairError::WrongPhase);
            }
            let PairRequest::Recall { subject } = &entry.started.request else {
                return Err(PairError::UnsupportedLane);
            };
            evidence.outcome(subject).ok_or(PairError::Protocol)
        })();
        let outcome = match checked {
            Ok(outcome) => outcome,
            Err(error) => return Err((evidence, error)),
        };
        let entry = self.entry_mut(call).expect("checked inward owner");
        entry.ending = Some(PairEnded {
            call_id: call.clone(),
            execution: None,
            fate: TerminalPairFate::Completed,
            reason: None,
            outcome: Some(outcome),
            loaded_from: None,
        });
        entry.inward = Some(evidence);
        Ok(())
    }

    /// Bind before staging bytes. The context supplies the clock from its exact
    /// recorded load policy and retains this returned frame until fully sent.
    pub(super) fn bind_execution(
        &mut self,
        call: &CallId,
        exchange: ExchangeId,
        clock_ms: u64,
    ) -> Result<OrganEnvelope, PairError> {
        self.live()?;
        if self.next_ready().map(|ready| &ready.call_id) != Some(call) {
            return Err(PairError::WrongPhase);
        }
        if exchange.opener != Opener::Harness
            || exchange.ordinal == 0
            || self.exchange_known(&exchange)
        {
            return Err(PairError::WrongExchange);
        }
        if !(1..=SHELL_MAX_CLOCK_MS).contains(&clock_ms) {
            return Err(PairError::InvalidClock);
        }
        let entry = self.entry_mut(call)?;
        let PairRequest::External { name, arguments } = &entry.started.request else {
            return Err(PairError::UnsupportedLane);
        };
        if entry.started.lane != PairLane::External {
            return Err(PairError::UnsupportedLane);
        }
        let frame = OrganEnvelope {
            exchange: exchange.clone(),
            position: Position::Open,
            payload: weaver_types::Payload::Tool(ToolExecution {
                name: ToolName(name.clone()),
                arguments: arguments.clone(),
                clock_ms,
            }),
        };
        entry.execution = Some(exchange);
        Ok(frame)
    }

    pub(super) fn mark_sent(
        &mut self,
        call: &CallId,
        exchange: &ExchangeId,
    ) -> Result<(), PairError> {
        self.live()?;
        if self
            .active
            .as_ref()
            .is_none_or(|batch| batch.cause.is_some())
        {
            return Err(PairError::WrongPhase);
        }
        let entry = self.entry_mut(call)?;
        if entry.execution.as_ref() != Some(exchange) || entry.sent || entry.ending.is_some() {
            return Err(PairError::WrongPhase);
        }
        entry.sent = true;
        Ok(())
    }

    /// The context must first withdraw the exact unsent packet from transport.
    /// Its reservation is retired permanently, without claiming an execution.
    pub(super) fn cancel_unsent(
        &mut self,
        call: &CallId,
        exchange: &ExchangeId,
    ) -> Result<(), PairError> {
        self.live()?;
        let cause = self
            .active
            .as_ref()
            .and_then(|batch| batch.cause)
            .ok_or(PairError::WrongPhase)?;
        let entry = self.entry(call)?;
        if entry.execution.as_ref() != Some(exchange)
            || entry.sent
            || entry.ending.is_some()
            || entry.terminal_authoring
            || entry.completed.is_some()
        {
            return Err(PairError::WrongPhase);
        }
        self.cancelled_exchanges
            .try_reserve(1)
            .map_err(|_| PairError::Allocation)?;
        self.cancelled_exchanges
            .push((exchange.clone(), call.clone()));
        let entry = self.entry_mut(call).expect("validated unsent call");
        entry.execution = None;
        entry.ending = Some(PairEnded {
            call_id: call.clone(),
            execution: None,
            fate: TerminalPairFate::Failed,
            reason: Some(PairFailure::NotDispatched { cause }),
            outcome: None,
            loaded_from: None,
        });
        Ok(())
    }

    pub(super) fn executing(&self) -> Option<(&CallId, &ExchangeId, bool)> {
        self.active.as_ref()?.entries.iter().find_map(|entry| {
            (entry.ending.is_none()).then_some(())?;
            Some((
                &entry.started.call_id,
                entry.execution.as_ref()?,
                entry.sent,
            ))
        })
    }

    /// Frozen inspection keeps an already received terminal attached to its
    /// execution. It grants no new dispatch or ordinary terminal filing.
    pub(super) fn halt_execution(&self) -> Option<(&CallId, &ExchangeId, bool)> {
        if !self.frozen {
            return None;
        }
        self.active.as_ref()?.entries.iter().find_map(|entry| {
            if entry.completed.is_some() || (entry.ending.is_some() && entry.received.is_none()) {
                return None;
            }
            Some((
                &entry.started.call_id,
                entry.execution.as_ref()?,
                entry.sent,
            ))
        })
    }

    /// Only the same context's retained whole-send ticket can settle a send
    /// whose healthy reducer did not run before failure. No deadline retires.
    pub(super) fn reconcile_halt_send(
        &mut self,
        gate: &OrganPort,
        ticket: &OrganSend,
    ) -> Result<(), PairError> {
        if !self.frozen || !ticket.belongs_to(gate) || !ticket.sent() {
            return Err(PairError::WrongPhase);
        }
        let (call, exchange, _) = self.halt_execution().ok_or(PairError::WrongPhase)?;
        if !ticket.matches_exchange(exchange) {
            return Err(PairError::WrongPhase);
        }
        let call = call.clone();
        self.entry_mut(&call)?.sent = true;
        Ok(())
    }

    pub(super) fn halt_gate_terminal(&self, call: &CallId) -> Option<(&PairEnded, &[u8])> {
        if !self.frozen {
            return None;
        }
        let entry = self.entry(call).ok()?;
        Some((entry.ending.as_ref()?, entry.received.as_ref()?.raw_bytes()))
    }

    /// A frozen, actually received gate terminal can use the halt's reserved
    /// record. A healthy author already owns its original attempt if started.
    pub(super) fn unclaimed_halt_terminal(&self) -> Option<(&PairEnded, &weaver_trace::TurnRef)> {
        if !self.frozen {
            return None;
        }
        let batch = self.active.as_ref()?;
        let entry = batch.entries.iter().find(|entry| {
            entry.received.is_some()
                && entry.gate_terminal
                && !entry.terminal_authoring
                && entry.terminal_source.is_none()
                && entry.completed.is_none()
        })?;
        Some((
            entry.ending.as_ref()?,
            &batch.proof.original().generation().original().turn,
        ))
    }

    pub(super) fn active_turn(&self) -> Option<&weaver_trace::TurnRef> {
        Some(
            &self
                .active
                .as_ref()?
                .proof
                .original()
                .generation()
                .original()
                .turn,
        )
    }

    /// No terminal can be attributed to the gate without the owned packet.
    /// A frozen flow may retain its trace-only outcome, but cannot file it.
    #[allow(clippy::result_large_err)]
    pub(super) fn observe_gate(
        &mut self,
        received: ReceivedOrgan,
    ) -> Result<GateObservation, (ReceivedOrgan, PairError)> {
        if self.rejected.is_some() && !self.frozen {
            return Err((received, PairError::Busy));
        }
        let envelope = received.envelope();
        let weaver_types::Payload::ToolAnswer(outcome) = &envelope.payload else {
            return Err((received, PairError::Protocol));
        };
        if envelope.position != Position::Close {
            return Err((received, PairError::Protocol));
        }
        if matches!(
            outcome,
            ToolOutcome::Killed {
                by: KillCause::Fault,
                ..
            }
        ) || (!self.frozen
            && matches!(
                outcome,
                ToolOutcome::Killed {
                    by: KillCause::FlowFailure,
                    ..
                }
            ))
        {
            return Err((received, PairError::Protocol));
        }
        let exchange = envelope.exchange.clone();
        let mapped = self.active.as_ref().and_then(|batch| {
            batch
                .entries
                .iter()
                .find(|entry| entry.execution.as_ref() == Some(&exchange))
        });
        let live_call = mapped
            .filter(|entry| entry.ending.is_none() && entry.sent)
            .map(|entry| entry.started.call_id.clone());
        if let Some(call) = live_call {
            let (fate, reason) = match outcome {
                ToolOutcome::Result { .. }
                | ToolOutcome::Refused { .. }
                | ToolOutcome::Errored { .. } => (TerminalPairFate::Completed, None),
                ToolOutcome::Killed { by, .. } => (
                    TerminalPairFate::Failed,
                    Some(match by {
                        KillCause::Clock => PairFailure::Timeout {},
                        KillCause::Cancel => PairFailure::Cancelled {},
                        KillCause::Unload => PairFailure::Unload {},
                        KillCause::FlowFailure => PairFailure::FlowFailure {},
                        KillCause::Fault => unreachable!("gate cannot mint a harness fault"),
                    }),
                ),
            };
            let ending = PairEnded {
                call_id: call.clone(),
                execution: Some(exchange),
                fate,
                reason,
                outcome: Some(outcome.clone()),
                loaded_from: None,
            };
            let frozen = self.frozen;
            let entry = self.entry_mut(&call).expect("live mapped entry");
            entry.ending = Some(ending);
            entry.gate_terminal = true;
            entry.received = Some(received);
            return Ok(if frozen {
                GateObservation::Halted(call)
            } else {
                GateObservation::Matched(call)
            });
        }
        // A frozen, already-sent execution may retain its own terminal while
        // an older rejection remains owed. A second rejection never replaces it.
        if self.rejected.is_some() {
            return Err((received, PairError::Busy));
        }
        let known = mapped
            .map(|entry| (&entry.started.call_id, entry.ending.as_ref()))
            .or_else(|| {
                self.closed
                    .iter()
                    .find(|pair| pair.ended.execution.as_ref() == Some(&exchange))
                    .map(|pair| (&pair.started.call_id, Some(&pair.ended)))
            })
            .or_else(|| {
                self.cancelled_exchanges
                    .iter()
                    .find(|(reserved, _)| reserved == &exchange)
                    .map(|(_, call)| (call, None))
            });
        let reason = match known {
            Some((_, Some(end))) if matches!(end.reason, Some(PairFailure::GateFault {})) => {
                "late_return"
            }
            Some((_, Some(_))) => "duplicate_return",
            _ => "unexpected_exchange",
        };
        let call_id = known.map(|(call, _)| call.clone());
        // ReceivedOrgan already owns a decoded envelope. Copy its complete
        // payload text, including surrounding whitespace and original escapes;
        // parsing it again would normalize the evidence or trim its boundary.
        let raw = match std::str::from_utf8(received.raw_bytes()) {
            Ok(text) => text.to_owned(),
            Err(_) => return Err((received, PairError::Protocol)),
        };
        use weaver_trace::serving::payloads as trace;
        let record = trace::RejectedReturn {
            call_id: call_id.map(|call| trace::CallId {
                run: call.run,
                sequence: trace::DecimalU64(call.sequence.0),
                call: trace::DecimalU64(call.call.0),
            }),
            execution: trace::ExchangeId {
                opener: match exchange.opener {
                    Opener::Harness => trace::Opener::Harness,
                    Opener::Admin => trace::Opener::Admin,
                    Opener::Spu => trace::Opener::Spu,
                    Opener::Gate => trace::Opener::Gate,
                },
                ordinal: exchange.ordinal,
            },
            reason: reason.into(),
            raw_envelope: raw,
        };
        self.rejected = Some(RejectedGateReturn { record, received });
        Ok(GateObservation::Rejected)
    }

    pub(super) fn pending_rejection(&self) -> Option<&RejectedGateReturn> {
        self.rejected.as_ref()
    }

    /// Called only by the context's exact Recorded hook, never when authoring
    /// merely starts. Return the raw custody after its rejection is on trace.
    pub(super) fn ack_rejection_recorded(
        &mut self,
        source: SourcePosition,
    ) -> Result<ReceivedOrgan, PairError> {
        let pending = self.rejected.as_ref().ok_or(PairError::WrongPhase)?;
        if source.block_id.run != self.run
            || source.block_id.block.0 != 0
            || source.block_id.sequence.0 == 0
            || self.source_used(&source.block_id)
            || self.rejected_sources.contains(&source.block_id)
            || pending
                .record
                .call_id
                .as_ref()
                .is_some_and(|call| source.block_id.sequence.0 <= call.sequence.0)
            || self.active.as_ref().is_some_and(|batch| {
                source.turn.as_deref()
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
            })
        {
            return Err(PairError::InvalidIdentity);
        }
        self.rejected_sources
            .try_reserve(1)
            .map_err(|_| PairError::Allocation)?;
        self.rejected_sources.push(source.block_id);
        Ok(self.rejected.take().unwrap().received)
    }

    pub(super) fn terminal_is_gate(&self, call: &CallId) -> Result<bool, PairError> {
        Ok(self.entry(call)?.gate_terminal)
    }

    pub(super) fn halted_terminal(&self) -> Option<(&CallId, &PairEnded)> {
        if !self.frozen {
            return None;
        }
        self.active.as_ref()?.entries.iter().find_map(|entry| {
            if entry.completed.is_some() {
                return None;
            }
            Some((&entry.started.call_id, entry.ending.as_ref()?))
        })
    }

    /// The context calls this only after its gate death/containment evidence.
    /// A received result already owns the fate and cannot be overwritten.
    pub(super) fn close_gate_fault(&mut self, call: &CallId) -> Result<(), PairError> {
        let entry = self.entry_mut(call)?;
        if entry.execution.is_none()
            || !entry.sent
            || entry.ending.is_some()
            || entry.completed.is_some()
        {
            return Err(PairError::WrongPhase);
        }
        entry.ending = Some(PairEnded {
            call_id: call.clone(),
            execution: entry.execution.clone(),
            fate: TerminalPairFate::Failed,
            reason: Some(PairFailure::GateFault {}),
            outcome: Some(ToolOutcome::Killed {
                by: KillCause::Fault,
                partial: None,
            }),
            loaded_from: None,
        });
        self.gate_failed = true;
        Ok(())
    }

    pub(super) fn refuse_ungranted(
        &mut self,
        call: &CallId,
        reason: String,
    ) -> Result<(), PairError> {
        self.live()?;
        if self.next_ready().map(|ready| &ready.call_id) != Some(call) {
            return Err(PairError::WrongPhase);
        }
        let entry = self.entry_mut(call)?;
        if entry.started.lane != PairLane::External {
            return Err(PairError::UnsupportedLane);
        }
        entry.ending = Some(PairEnded {
            call_id: call.clone(),
            execution: None,
            fate: TerminalPairFate::Failed,
            reason: Some(PairFailure::NotGranted {}),
            outcome: Some(ToolOutcome::Refused { reason }),
            loaded_from: None,
        });
        Ok(())
    }

    /// Bar only the next fully opened request. Outbound selection does not
    /// erase omitted calls or bar a selected suffix. Later cancellation may
    /// still replace this cause until the terminal author owns it.
    pub(super) fn refuse_dispatch(
        &mut self,
        call: &CallId,
        cause: NonDispatchCause,
    ) -> Result<(), PairError> {
        self.live()?;
        if self.next_ready().map(|ready| &ready.call_id) != Some(call) {
            return Err(PairError::WrongPhase);
        }
        self.entry_mut(call)?.ending = Some(PairEnded {
            call_id: call.clone(),
            execution: None,
            fate: TerminalPairFate::Failed,
            reason: Some(PairFailure::NotDispatched { cause }),
            outcome: None,
            loaded_from: None,
        });
        Ok(())
    }

    pub(super) fn set_retry_of(
        &mut self,
        call: &CallId,
        predecessor: &CallId,
    ) -> Result<(), PairError> {
        self.live()?;
        if !self.has_effective() {
            return Err(PairError::WrongPhase);
        }
        let entry = self.entry(call)?;
        let prior = self
            .active
            .as_ref()
            .into_iter()
            .flat_map(|batch| &batch.entries)
            .filter_map(|entry| entry.completed.as_ref())
            .chain(self.closed.iter().map(|pair| pair.pair.as_ref()))
            .find(|pair| &pair.started.call_id == predecessor)
            .ok_or(PairError::InvalidLineage)?;
        if entry.opening_authoring
            || entry.started.retry_of.is_some()
            || call == predecessor
            || call.run != predecessor.run
            || entry.started.lane != PairLane::External
            || prior.started.lane != PairLane::External
            || entry.started.request != prior.started.request
        {
            return Err(PairError::InvalidLineage);
        }
        self.entry_mut(call)?.started.retry_of = Some(predecessor.clone());
        Ok(())
    }

    pub(super) fn finish_effective(&mut self) -> Result<EffectiveEmission, PairError> {
        self.live()?;
        if !self.has_effective() || self.rejected.is_some() {
            return Err(PairError::WrongPhase);
        }
        match self.finish_proof()? {
            BatchProof::Effective(effective) => Ok(effective),
            _ => unreachable!("checked effective owner"),
        }
    }

    fn exchange_known(&self, exchange: &ExchangeId) -> bool {
        self.closed
            .iter()
            .any(|pair| pair.ended.execution.as_ref() == Some(exchange))
            || self
                .cancelled_exchanges
                .iter()
                .any(|(reserved, _)| reserved == exchange)
            || self.active.as_ref().is_some_and(|batch| {
                batch
                    .entries
                    .iter()
                    .any(|entry| entry.execution.as_ref() == Some(exchange))
            })
    }
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
            closed_by_call: HashMap::new(),
            frozen: false,
            gate_failed: false,
            rejected: None,
            rejected_sources: Vec::new(),
            cancelled_exchanges: Vec::new(),
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
                _ => unreachable!("original proof returned unchanged"),
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
                _ => unreachable!("repair proof returned unchanged"),
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
            self.closed_by_call
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
                    execution: None,
                    sent: false,
                    received: None,
                    gate_terminal: false,
                    internal_started: false,
                    inward: None,
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

    /// The execution context shares one receipt wait, so it opens only the
    /// first unfinished pair. The general ledger still permits overlapping
    /// openings for callers that retain independent receipt ownership.
    pub(super) fn next_execution_opening(&self) -> Option<(&CallId, &PairStarted)> {
        let next = self.next_opening()?;
        let first = self
            .active
            .as_ref()?
            .entries
            .iter()
            .find(|entry| entry.completed.is_none())?;
        (next.0 == &first.started.call_id).then_some(next)
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

    pub(super) fn barred_for(&self, cause: NonDispatchCause) -> bool {
        self.active
            .as_ref()
            .is_some_and(|batch| batch.cause == Some(cause))
    }

    /// Operator leave dominates stop. An accepted stop dominates ordinary
    /// caller barriers. No change rewrites a record already given to authoring.
    pub(super) fn set_not_dispatched(&mut self, cause: NonDispatchCause) -> Result<(), PairError> {
        self.live()?;
        let batch = self.active.as_mut().ok_or(PairError::WrongPhase)?;
        let cause = dominant(batch.cause, cause);
        batch.cause = Some(cause);
        for entry in &mut batch.entries {
            if !entry.terminal_authoring
                && entry.completed.is_none()
                && entry.execution.is_none()
                && !entry.internal_started
                && (entry.ending.is_none()
                    || matches!(
                        entry.ending.as_ref().and_then(|end| end.reason.as_ref()),
                        Some(PairFailure::NotDispatched { .. })
                    ))
            {
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
        let ending = entry.ending.as_ref().unwrap();
        let result = match (&ending.reason, &ending.outcome) {
            (Some(PairFailure::NotDispatched { cause }), None) => {
                crate::tools::ToolResult::not_dispatched(*cause)
            }
            (_, Some(outcome)) => crate::tools::ToolResult::granted(outcome),
            _ => unreachable!("all live terminal constructors retain their outcome"),
        };
        entry.completed = Some(CompletedPair {
            started: entry.started.clone(),
            ended: entry.ending.as_ref().unwrap().clone(),
            statement,
            opening: entry.opening.take().unwrap(),
            terminal: receipt,
            receipt: filed,
            result,
            received: entry.received.take(),
            inward: entry.inward.take(),
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
            .map(|pair| Some(pair.pair.as_ref()))
            .ok_or(PairError::UnknownCall)
    }

    pub(super) fn is_complete(&self) -> bool {
        !self.frozen
            && self.rejected.is_none()
            && self
                .active
                .as_ref()
                .is_some_and(|batch| batch.entries.iter().all(|entry| entry.completed.is_some()))
    }

    pub(super) fn finish_batch(&mut self) -> Result<OriginalRequests, PairError> {
        self.live()?;
        if !self
            .active
            .as_ref()
            .is_some_and(|batch| matches!(batch.proof, BatchProof::Original(_)))
        {
            return Err(PairError::WrongPhase);
        }
        match self.finish_proof()? {
            BatchProof::Original(original) => Ok(original),
            _ => unreachable!("checked original batch"),
        }
    }

    pub(super) fn finish_repair_batch(&mut self) -> Result<RepairAccount, PairError> {
        self.live()?;
        if !self.has_repair_account() {
            return Err(PairError::WrongPhase);
        }
        match self.finish_proof()? {
            BatchProof::Repair(account) => Ok(account),
            _ => unreachable!("checked repair batch"),
        }
    }

    fn finish_proof(&mut self) -> Result<BatchProof, PairError> {
        self.live()?;
        if self.rejected.is_some() {
            return Err(PairError::Busy);
        }
        if self
            .active
            .as_ref()
            .is_none_or(|batch| batch.entries.iter().any(|entry| entry.completed.is_none()))
        {
            return Err(PairError::WrongPhase);
        }
        let batch = self.active.take().unwrap();
        for entry in batch.entries {
            self.closed_by_call
                .insert(call_key(&entry.started.call_id), self.closed.len());
            self.closed.push(RetainedPair {
                pair: Arc::new(entry.completed.unwrap()),
                result_record: None,
                delivered: None,
            });
        }
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
                    state: if entry.terminal_authoring
                        || ((entry.execution.is_some() || entry.internal_started)
                            && entry.ending.is_some())
                    {
                        PairState::Ending
                    } else if entry.internal_started {
                        PairState::ExecutingInternal
                    } else if entry.execution.is_some() {
                        PairState::Executing
                    } else if entry.opening.is_some() && has_statement {
                        PairState::Opened
                    } else {
                        PairState::Opening
                    },
                    execution: entry.execution.clone(),
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
                    && file.source.subsystem == "harness"
                    && matches!(&file.pair, PairRecord::Started(started) if started == &entry.started)
            }
            PairRecordPhase::Terminal => {
                entry.terminal_authoring
                    && entry.terminal_source.as_ref() == Some(receipt.source())
                    && file.source.kind == "tool.call.completed"
                    && file.source.origin == if entry.gate_terminal { "tool" } else { "loop" }
                    && file.source.subsystem
                        == if entry.gate_terminal {
                            "gate"
                        } else {
                            "harness"
                        }
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

#[cfg(test)]
#[path = "pairs_execution_tests.rs"]
mod execution_tests;

impl PairLedger {
    pub(super) fn opening_for_material(
        &self,
        call: &CallId,
    ) -> Option<(&PairStarted, &CommittedSource, &OriginalRequests)> {
        let batch = self.active.as_ref()?;
        let entry = batch.entries.iter().find(|e| &e.started.call_id == call)?;
        Some((
            &entry.started,
            entry.opening.as_ref()?,
            batch.proof.original(),
        ))
    }
    pub(super) fn closed_for_material(
        &self,
        call: &CallId,
    ) -> Result<Arc<CompletedPair>, PairError> {
        self.live()?;
        if let Some(index) = self.closed_by_call.get(&call_key(call)) {
            let retained = &self.closed[*index];
            if retained.delivered.is_some() {
                return Err(PairError::AlreadyDelivered);
            }
            return Ok(Arc::clone(&retained.pair));
        }
        self.completed(call)?;
        Err(PairError::NotReady)
    }

    /// The complete grant is borrowed. Delivery changes no terminal evidence.
    pub(super) fn take_result(&self, call: &CallId) -> Result<&CompletedPair, PairError> {
        self.live()?;
        if let Some(pair) = self.closed.iter().find(|p| &p.started.call_id == call) {
            if pair.delivered.is_some() {
                return Err(PairError::AlreadyDelivered);
            }
            return Ok(&pair.pair);
        }
        self.completed(call)?.ok_or(PairError::NotReady)
    }

    pub(super) fn result_record(
        &self,
        pair: &CompletedPair,
    ) -> Result<Option<weaver_trace::Sequence>, PairError> {
        self.live()?;
        let retained = self
            .closed_by_call
            .get(&call_key(&pair.started.call_id))
            .map(|index| &self.closed[*index])
            .filter(|p| std::ptr::eq(p.pair.as_ref(), pair))
            .ok_or(PairError::WrongReceipt)?;
        if retained.delivered.is_some() {
            return Err(PairError::AlreadyDelivered);
        }
        Ok(retained.result_record)
    }

    pub(super) fn record_result(
        &mut self,
        call: &CallId,
        sequence: weaver_trace::Sequence,
    ) -> Result<(), PairError> {
        // Retaining an already-authored receipt is owed even after fail()
        // freezes the ledger. This grants neither another author nor delivery.
        let index = *self
            .closed_by_call
            .get(&call_key(call))
            .ok_or(PairError::UnknownCall)?;
        let pair = &mut self.closed[index];
        if pair.delivered.is_some() || pair.result_record.is_some_and(|old| old != sequence) {
            return Err(PairError::WrongReceipt);
        }
        pair.result_record = Some(sequence);
        Ok(())
    }

    pub(super) fn acknowledge_delivery(
        &mut self,
        call: &CallId,
        proof: &crate::flow::place::ConfirmedGeneration,
    ) -> Result<(), PairError> {
        self.live()?;
        let index = *self
            .closed_by_call
            .get(&call_key(call))
            .ok_or(PairError::UnknownCall)?;
        let pair = &mut self.closed[index];
        if pair.delivered.is_some() {
            return Err(PairError::AlreadyDelivered);
        }
        if pair.result_record.is_none()
            || !proof
                .append_receipt
                .as_ref()
                .is_some_and(|receipt| receipt.matches(&proof.grant.pending))
            || !proof.grant.units.iter().any(|unit| {
                unit.completed_pair()
                    .is_some_and(|p| std::ptr::eq(p, pair.pair.as_ref()))
                    && unit
                        .sources()
                        .contains(&weaver_types::state_flow::MaterialRef::Returned {
                            call_id: call.clone(),
                        })
            })
        {
            return Err(PairError::WrongReceipt);
        }
        pair.delivered = Some(proof.placement_sequence());
        Ok(())
    }
}
