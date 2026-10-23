//! Whole-emission checks share the run recorder and classifier. This transaction
//! owns the original message and a borrowed Decision wait, never a decoder pass,
//! a filing deadline, a repair policy or permission to dispatch a call.

use super::*;
use crate::flow::author::AuthoredSource;
use crate::flow::coordinator::{CheckedGeneration, OutputCancellation, OutputOriginal};
use weaver_traits::{ContentBlock, Role};
use weaver_types::state_flow as s;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum OutputBeginRefusal {
    WrongPhase,
    Busy,
    MissingWait,
    InvalidMessage,
    WrongTurn,
}

#[derive(Debug)]
pub(in crate::flow) struct OutputBeginError {
    pub original: OutputOriginal,
    pub refusal: OutputBeginRefusal,
    pub failure: Option<RunFailure>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum OutputProgress {
    Progress,
    Pending,
    Complete,
    Cancelled,
}

pub(super) struct OutputTransaction {
    bound: AbsoluteBound,
    sequence: Option<Sequence>,
    author_done: bool,
    cancelled: bool,
}

impl RecordingContext {
    #[allow(clippy::result_large_err)]
    pub(in crate::flow) fn begin_output(
        &mut self,
        original: OutputOriginal,
        causal_parent: Option<Sequence>,
        io: &mut impl PumpIo,
    ) -> Result<(), OutputBeginError> {
        if let Err(failure) = self.live(io) {
            return Err(OutputBeginError {
                original,
                refusal: OutputBeginRefusal::WrongPhase,
                failure: Some(failure),
            });
        }
        if self.phase != RecordingPhase::Entered
            || !self.barrier.is_active()
            || self.flow.phase() != CoordinatorPhase::Active
        {
            return Err(OutputBeginError {
                original,
                refusal: OutputBeginRefusal::WrongPhase,
                failure: None,
            });
        }
        if original.message.role != Role::Assistant
            || original.message.content.iter().any(|block| {
                !matches!(
                    block,
                    ContentBlock::Text { .. }
                        | ContentBlock::ToolCall(_)
                        | ContentBlock::UnrecoveredCall { .. }
                )
            })
        {
            return Err(OutputBeginError {
                original,
                refusal: OutputBeginRefusal::InvalidMessage,
                failure: None,
            });
        }
        if original.turn.0.is_empty() || self.open_turn.as_ref() != Some(&original.turn) {
            return Err(OutputBeginError {
                original,
                refusal: OutputBeginRefusal::WrongTurn,
                failure: None,
            });
        }
        let Some(bound) = self.deadlines.get(PumpDeadline::Decision) else {
            return Err(OutputBeginError {
                original,
                refusal: OutputBeginRefusal::MissingWait,
                failure: None,
            });
        };
        if self.marks.is_some()
            || self.boot_pairs.is_some()
            || !self.repair_allows_output()
            || self.freshness.is_some()
            || self.output.is_some()
            || self.flow.has_output_consumer()
            || self.filing.is_some()
            || self.pair_work.is_some()
            || self.flow.has_filing_consumer()
            || self.templates_block_content()
            || self.template_blocked_author.is_some()
            || self.input.snapshot().is_some()
            || self.input_turn.is_some()
            || self.input_author.is_some()
            || self.member_capture.is_some()
            || self.read.is_some()
            || self.inspection.is_some()
            || self.boot_open_blocks_external()
            || self.measure_blocks_external()
            || self.waiting_content.is_some()
            || self.completed_author.is_some()
            || self.barrier.pending().is_some()
            || self.barrier.pending_author().is_some()
            || self.flow.has_pending_record()
            || self.flow.pending_check().is_some()
            || self.flow.pending_control().is_some()
            || self.flow.operation_completion().is_some()
        {
            return Err(OutputBeginError {
                original,
                refusal: OutputBeginRefusal::Busy,
                failure: None,
            });
        }
        let payload = match serde_json::value::to_raw_value(&original.message) {
            Ok(payload) => payload,
            Err(error) => {
                let failure = self.fail(
                    io,
                    FlowHaltReason::ProtocolFailed,
                    HaltPoint::Author,
                    error.to_string(),
                );
                return Err(OutputBeginError {
                    original,
                    refusal: OutputBeginRefusal::InvalidMessage,
                    failure: Some(failure),
                });
            }
        };
        let event = AuthoredEvent {
            kind: Kind::MessageAssistant,
            subsystem: weaver_trace::Subsystem::Harness,
            turn: Some(original.turn.clone()),
            source: AuthoredSource::Model,
            causal_parent,
            payload: Some(Payload::Message(payload)),
        };
        let reserved = self.flow.reserve_output_checks(
            &mut FlowRecording {
                barrier: &mut self.barrier,
                inbox: &mut self.inbox,
                budget: &mut self.budget,
                author: &self.author,
            },
            original,
        );
        if let Err((original, error)) = reserved {
            return Err(OutputBeginError {
                original,
                refusal: OutputBeginRefusal::WrongPhase,
                failure: Some(self.flow_failed(io, error)),
            });
        }
        self.output = Some(OutputTransaction {
            bound,
            sequence: None,
            author_done: false,
            cancelled: false,
        });
        self.waiting_content = Some(event);
        self.repair_note_output_started();
        Ok(())
    }

    pub(super) fn output_wait_still_owned(&self, key: PumpDeadline) -> bool {
        key == PumpDeadline::Decision
            && self.output.is_some()
            && !self.output_disposition_recorded()
    }

    /// The enclosing wait may end at the final Recorded result even while its
    /// terminal-status probe yields. This is timing evidence only: the original
    /// classifier owner must still release every check before any grant exists.
    fn output_disposition_recorded(&self) -> bool {
        let Some(owner) = &self.output else {
            return false;
        };
        if !owner.author_done {
            return false;
        }
        let Some(checks) = self.flow.output_checks() else {
            return false;
        };
        if owner.cancelled && checks.is_settled() {
            return true;
        }
        if checks
            .recorded_ns()
            .is_some_and(|at| !owner.bound.expired(at))
        {
            return true;
        }
        let Some((request, reply, recorded_ns)) = self.flow.pending_classifier_recorded_request()
        else {
            return false;
        };
        if owner.bound.expired(recorded_ns)
            || self.flow.failure().is_some()
            || matches!(reply.result, s::Ste100Result::ComponentFailed { .. })
            || !checks.observes(request)
        {
            return false;
        }
        if owner.cancelled {
            // Cancellation still drains an issued unselected check. Unissued
            // checks are cancelled; selected filing remains independently owed.
            return checks.owns_unselected(request);
        }
        let s::Ste100Target::Source(source) = &request.target else {
            return false;
        };
        let mut pending = checks.pending();
        pending.next().as_ref() == Some(source) && pending.next().is_none()
    }

    fn output_wait_matches(&mut self, io: &mut impl PumpIo) -> Result<(), RunFailure> {
        let owner = self.output.as_ref().ok_or(RunFailure::Closed)?;
        let current = self.deadlines.get(PumpDeadline::Decision);
        let settled = self.output_disposition_recorded();
        if current != Some(owner.bound) && !(current.is_none() && settled) {
            return Err(self.fail(
                io,
                FlowHaltReason::ProtocolFailed,
                HaltPoint::Barrier,
                "output consumer's enclosing wait was removed or replaced",
            ));
        }
        Ok(())
    }

    /// The recorder supplies this sequence before any selected result reduction.
    pub(super) fn bind_output_author(
        &mut self,
        sequence: Sequence,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        let Some(owner) = self.output.as_ref() else {
            return Ok(());
        };
        if owner.sequence.is_some() || owner.author_done {
            return Err(self.fail(
                io,
                FlowHaltReason::ProtocolFailed,
                HaltPoint::Author,
                "another author completed inside an output transaction",
            ));
        }
        self.output.as_mut().unwrap().sequence = Some(sequence);
        let now = io.now().map_err(|error| {
            self.fail(
                io,
                FlowHaltReason::ProtocolFailed,
                HaltPoint::Barrier,
                error.to_string(),
            )
        })?;
        let result = self.flow.bind_output_checks(
            &mut FlowRecording {
                barrier: &mut self.barrier,
                inbox: &mut self.inbox,
                budget: &mut self.budget,
                author: &self.author,
            },
            sequence,
            now,
        );
        result.map_err(|error| self.flow_failed(io, error))
    }

    pub(in crate::flow) fn advance_output<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<OutputProgress, RunFailure> {
        self.live(ports.io)?;
        self.output_wait_matches(ports.io)?;
        let owner = self.output.as_ref().expect("checked owner");
        if !owner.author_done {
            let sequence = if let Some(sequence) = self.completed_author.take() {
                sequence
            } else if self.waiting_content.is_some() {
                self.continue_waiting_content(ports)?
            } else if self.barrier.pending_author().is_some() && !self.flow.has_pending_record() {
                let progress =
                    self.barrier
                        .resume_with_clock(&mut self.inbox, &mut self.budget, &mut || {
                            ports.io.now()
                        });
                self.wait_recorded(ports, progress)?
            } else {
                return Err(self.fail(
                    ports.io,
                    FlowHaltReason::ProtocolFailed,
                    HaltPoint::Author,
                    "output transaction lost its original author",
                ));
            };
            let owner = self.output.as_mut().unwrap();
            if owner.sequence != Some(sequence) {
                return Err(self.fail(
                    ports.io,
                    FlowHaltReason::ProtocolFailed,
                    HaltPoint::Author,
                    "output author returned another source",
                ));
            }
            owner.author_done = true;
            return Ok(OutputProgress::Progress);
        }
        let checks = self.flow.output_checks().expect("retained output checks");
        if owner.cancelled && checks.is_settled() {
            return Ok(OutputProgress::Cancelled);
        }
        if !owner.cancelled && checks.is_complete() {
            return Ok(OutputProgress::Complete);
        }
        let advance = self.reduce_flow(ports.io)?;
        let checks = self.flow.output_checks().expect("retained output checks");
        if checks.is_cancelled() && checks.is_settled() {
            Ok(OutputProgress::Cancelled)
        } else if checks.is_complete() {
            Ok(OutputProgress::Complete)
        } else if advance == Advance::Progress {
            Ok(OutputProgress::Progress)
        } else {
            Ok(OutputProgress::Pending)
        }
    }

    pub(in crate::flow) fn take_output(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<Option<CheckedGeneration>, RunFailure> {
        self.live(io)?;
        if self.output.is_none() {
            return Ok(None);
        }
        self.output_wait_matches(io)?;
        let owner = self.output.as_ref().unwrap();
        if owner.cancelled || !owner.author_done {
            return Ok(None);
        }
        let result = self.flow.take_output_checks(&mut FlowRecording {
            barrier: &mut self.barrier,
            inbox: &mut self.inbox,
            budget: &mut self.budget,
            author: &self.author,
        });
        let result = result.map_err(|error| self.flow_failed(io, error))?;
        if result.is_some() {
            self.output = None;
        }
        Ok(result)
    }

    pub(in crate::flow) fn cancel_output(&mut self) {
        // Repair must account this emitted correction and owns cancellation.
        if self.repair_blocks_external() {
            return;
        }
        let Some(owner) = self.output.as_mut() else {
            return;
        };
        if owner.cancelled {
            return;
        }
        owner.cancelled = true;
        self.flow.cancel_output_checks();
        if owner.sequence.is_none() && self.waiting_content.is_some() {
            // No trace ticket or copy exists. The core returns the unchanged
            // original Message. There is nothing to file or to undo.
            self.waiting_content.take();
            owner.author_done = true;
        }
    }

    pub(in crate::flow) fn take_cancelled_output(&mut self) -> Option<OutputCancellation> {
        if self.repair_blocks_external() {
            return None;
        }
        if !self
            .output
            .as_ref()
            .is_some_and(|owner| owner.cancelled && owner.author_done)
        {
            return None;
        }
        let result = self.flow.take_cancelled_output_checks()?;
        self.output = None;
        Some(result)
    }
}

/// A wrong lease is a caller refusal. Failure of the run never becomes a ready
/// receipt. Reading a committed fact does not arm a clock or authorize dispatch.
#[derive(Debug)]
pub(in crate::flow) enum SourceReceiptError {
    Run(RunFailure),
    Refused(crate::flow::ledger::LedgerError),
}

impl RecordingContext {
    pub(in crate::flow) fn source_receipt_status(
        &mut self,
        lease: &crate::flow::ledger::SourceLease,
        source: &weaver_types::state_flow::SourceBlockId,
        io: &mut impl PumpIo,
    ) -> Result<crate::flow::ledger::LeaseSlotStatus, SourceReceiptError> {
        self.live(io).map_err(SourceReceiptError::Run)?;
        self.flow
            .source_receipt_status(lease, source)
            .map_err(SourceReceiptError::Refused)
    }

    pub(in crate::flow) fn take_source_receipt(
        &mut self,
        lease: &crate::flow::ledger::SourceLease,
        source: &weaver_types::state_flow::SourceBlockId,
        io: &mut impl PumpIo,
    ) -> Result<Option<crate::flow::ledger::SourceDisposition>, SourceReceiptError> {
        self.live(io).map_err(SourceReceiptError::Run)?;
        self.flow
            .take_source_receipt(lease, source)
            .map_err(SourceReceiptError::Refused)
    }

    pub(in crate::flow) fn release_source_receipt(
        &mut self,
        lease: &crate::flow::ledger::SourceLease,
        source: &weaver_types::state_flow::SourceBlockId,
    ) -> Result<(), crate::flow::ledger::LedgerError> {
        self.flow.release_source_receipt(lease, source)
    }

    pub(in crate::flow) fn release_source_lease(
        &mut self,
        lease: crate::flow::ledger::SourceLease,
    ) -> Result<
        (),
        (
            crate::flow::ledger::SourceLease,
            crate::flow::ledger::LedgerError,
        ),
    > {
        self.flow.release_source_lease(lease)
    }
}
