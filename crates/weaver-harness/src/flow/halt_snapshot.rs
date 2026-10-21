//! Read-only projection of frozen owners under HARNESS-HALT-ACCOUNT.
//!
//! This helper never reads a socket, drains an inbox, or assigns a trace identity.
//! External owners supply their complete pair, capture, input and wait facts.

use weaver_trace::serving::{AdmittedCopy, payloads as trace};
use weaver_types::state_flow::{DecimalU64, SourceEnvelope, Ste100Target};

use super::channel::{CommitEvidence, ExchangeProgress};
use super::classifier::Ste100Port;
use super::coordinator::{CoordinatorPhase, FlowCoordinator};
use super::intake::{FlowInbox, RetentionBudget};
use super::ledger::LedgerState;
use super::recording::{RecordBarrier, RecordFailure};
use super::report::{
    AuthorAtHalt, BlockAtHalt, BlockState, CaptureAtHalt, FlowHaltAccount, FlowHaltReason,
    HaltPoint, OutstandingPair, ReportError, RequestAtHalt, RequestDomain, WaitAtHalt,
};

/// No Default: an owner must explicitly supply every fact outside the flow.
#[derive(Debug)]
pub(super) struct ExternalFacts {
    pub outstanding_pairs: Vec<OutstandingPair>,
    pub capture: Option<CaptureAtHalt>,
    pub wait: Option<WaitAtHalt>,
    pub input_id: Option<DecimalU64>,
}

/// On failure the caller still owns all facts supplied by external owners.
#[derive(Debug)]
pub(super) struct SnapshotFailure {
    pub external: ExternalFacts,
    pub error: ReportError,
}

#[allow(clippy::result_large_err)]
pub(super) fn snapshot<P: Ste100Port>(
    flow: &FlowCoordinator<P>,
    barrier: &RecordBarrier,
    inbox: &FlowInbox,
    budget: &RetentionBudget,
    external: ExternalFacts,
) -> Result<FlowHaltAccount, SnapshotFailure> {
    match prepare(flow, barrier, inbox, budget, external.input_id) {
        Ok(mut account) => {
            account.outstanding_pairs = external.outstanding_pairs;
            account.capture = external.capture;
            account.wait = external.wait;
            match account.validate() {
                Ok(()) => Ok(account),
                Err(error) => Err(SnapshotFailure {
                    external: ExternalFacts {
                        outstanding_pairs: account.outstanding_pairs,
                        capture: account.capture,
                        wait: account.wait,
                        input_id: external.input_id,
                    },
                    error,
                }),
            }
        }
        Err(error) => Err(SnapshotFailure { external, error }),
    }
}

fn prepare<P: Ste100Port>(
    flow: &FlowCoordinator<P>,
    barrier: &RecordBarrier,
    inbox: &FlowInbox,
    budget: &RetentionBudget,
    external_input: Option<DecimalU64>,
) -> Result<FlowHaltAccount, ReportError> {
    let halt = flow.halt().ok_or(ReportError::SnapshotMissing)?;
    if flow.phase() != CoordinatorPhase::Failed || !budget.is_frozen() {
        return Err(ReportError::Invalid("halt sources are not frozen"));
    }
    let first = halt.first();
    let sample = flow
        .final_sample()
        .map_err(|_| ReportError::Invalid("final flow sample failed"))?
        .ok_or(ReportError::SnapshotMissing)?;
    let progress = flow.standing_state().map(|state| state.outstanding());
    let mut outstanding_blocks = Vec::new();
    for fact in flow.ledger().facts() {
        let current = progress.as_ref().and_then(|all| {
            all.iter()
                .find(|item| item.source.as_ref() == Some(fact.source))
        });
        let transport = match (fact.progress, current) {
            (Some(prior), Some(current)) => {
                if prior.common != current.common
                    || prior.operation != current.operation
                    || prior.frame_bytes != current.frame_bytes
                    || prior.bytes_sent > current.bytes_sent
                {
                    return Err(ReportError::Invalid("ledger and channel progress disagree"));
                }
                Some(current)
            }
            (prior, current) => current.or(prior),
        };
        let state = match fact.prior_state {
            LedgerState::AwaitingClassification => BlockState::AwaitingClassification,
            LedgerState::ReadyToFile => BlockState::ReadyToFile,
            LedgerState::Sending => BlockState::Sending,
            LedgerState::AwaitingReceipt => BlockState::AwaitingReceipt,
            LedgerState::FiledHeld | LedgerState::CorrectlyRejectedHeld => {
                return Err(ReportError::Invalid("ledger omitted its prior state"));
            }
        };
        let (request_id, bytes_sent, frame_bytes, commit) = match transport {
            Some(progress) => (
                Some(progress.common.request_id),
                decimal_size(progress.bytes_sent)?,
                Some(decimal_size(progress.frame_bytes)?),
                progress.commit,
            ),
            None => (None, DecimalU64(0), None, CommitEvidence::NotRequested),
        };
        push(
            &mut outstanding_blocks,
            BlockAtHalt {
                block_id: fact.source.clone(),
                content_type: fact.content_type,
                state,
                request_id,
                bytes_sent,
                frame_bytes,
                commit,
            },
        )?;
    }
    let mut queued_copies = Vec::new();
    for copy in inbox.queued() {
        push(&mut queued_copies, copy_identity(copy)?)?;
    }
    let failed_copy = inbox.failure().map(|failure| &failure.copy);
    let waiting_copy = budget.waiting_copy();
    let unheld = if first.reason == FlowHaltReason::QueueLimit {
        if failed_copy.is_some() {
            return Err(ReportError::Invalid(
                "queue bound conflicts with failed copy intake",
            ));
        }
        waiting_copy
    } else {
        failed_copy.or(waiting_copy)
    };
    let unheld_copy = unheld.map(copy_identity).transpose()?;
    let actual_waiting = waiting_copy.map(copy_identity).transpose()?;
    if actual_waiting != sample.waiting_copy {
        return Err(ReportError::Invalid(
            "waiting copy differs from frozen sample",
        ));
    }

    let mut source_event = None;
    let mut source_id = None;
    let mut input_id = external_input;
    let mut request = None;
    let mut received_request = None;
    match first.point {
        HaltPoint::Tap => {
            if let Some(RecordFailure::Tap(failure)) = barrier.failure() {
                source_event = Some(trace::CopySourceEvent {
                    run: failure.source.run.clone(),
                    sequence: failure.source.sequence.0.into(),
                });
            }
        }
        HaltPoint::CopyIntake => {
            source_event = failed_copy
                .map(copy_identity)
                .transpose()?
                .map(|copy| copy.source_event);
        }
        HaltPoint::StateSend | HaltPoint::StateReply => {
            if let Some(state) = flow.standing_state() {
                let expected = match state.failure() {
                    Some(failure) => {
                        received_request = failure.received.as_ref().map(|common| RequestAtHalt {
                            domain: RequestDomain::State,
                            id: common.request_id,
                        });
                        failure.outstanding.as_deref()
                    }
                    None => progress.as_ref().and_then(|all| all.first()),
                };
                if let Some(expected) = expected {
                    request = Some(state_request(expected));
                    source_id = expected.source.clone();
                }
            }
        }
        HaltPoint::Classifier => {
            if let Some(classifier) = flow.standing_classifier() {
                received_request = classifier.failure().and_then(|failure| {
                    failure.received.as_ref().map(|reply| RequestAtHalt {
                        domain: RequestDomain::Ste100,
                        id: reply.request_id,
                    })
                });
                if let Some(expected) = classifier.outstanding() {
                    request = Some(RequestAtHalt {
                        domain: RequestDomain::Ste100,
                        id: expected.request_id,
                    });
                    match &expected.target {
                        Ste100Target::Source(source) => {
                            // A queued input may be waiting behind this source's
                            // check. Both identities are real concurrent facts.
                            source_id = Some(source.clone());
                        }
                        Ste100Target::Input {
                            input_id: actual, ..
                        } => {
                            if input_id.is_some_and(|supplied| supplied != *actual) {
                                return Err(ReportError::Invalid(
                                    "input differs from classifier target",
                                ));
                            }
                            input_id = Some(*actual);
                        }
                        Ste100Target::Template { .. } => {}
                    }
                }
            }
        }
        HaltPoint::Author | HaltPoint::Counter | HaltPoint::Barrier | HaltPoint::Teardown => {}
    }
    if source_event.is_none()
        && matches!(
            first.point,
            HaltPoint::Tap | HaltPoint::CopyIntake | HaltPoint::Author
        )
        && let Some(sequence) = barrier.admitted_before_failure()
    {
        source_event = Some(trace::CopySourceEvent {
            run: barrier.run().to_owned(),
            sequence: sequence.0.into(),
        });
    }
    if let Some(source) = &source_id {
        source_event = Some(trace::CopySourceEvent {
            run: source.run.clone(),
            sequence: source.sequence.0.into(),
        });
    }
    if first.reason == FlowHaltReason::QueueLimit {
        source_event = unheld_copy.as_ref().map(|copy| copy.source_event.clone());
    }
    let pending_author = barrier.pending_author().map(|pending| AuthorAtHalt {
        ticket: pending.ticket.into(),
        event: pending.event.clone(),
    });
    if barrier.pending().is_some() && pending_author.is_none() {
        return Err(ReportError::Invalid(
            "pending event has no original author ticket",
        ));
    }
    let account = FlowHaltAccount {
        reason: first.reason,
        point: first.point,
        detail: first.detail.clone(),
        observed_ns: first.observed_ns,
        source_event,
        source_id,
        input_id,
        request,
        received_request,
        // snapshot() moves the external obligations into these members before
        // validating or returning the account. No external fact is cloned away.
        wait: None,
        outstanding_blocks,
        queued_copies,
        unheld_copy,
        pending_author,
        outstanding_pairs: Vec::new(),
        capture: None,
        flow_sample: sample.clone(),
        secondary: Vec::new(),
    };
    Ok(account)
}

fn state_request(progress: &ExchangeProgress) -> RequestAtHalt {
    RequestAtHalt {
        domain: RequestDomain::State,
        id: progress.common.request_id,
    }
}

fn decimal_size(value: usize) -> Result<DecimalU64, ReportError> {
    u64::try_from(value)
        .map(DecimalU64)
        .map_err(|_| ReportError::CounterOverflow)
}

fn push<T>(values: &mut Vec<T>, value: T) -> Result<(), ReportError> {
    values
        .try_reserve(1)
        .map_err(|_| ReportError::AllocationFailed)?;
    values.push(value);
    Ok(())
}

fn copy_identity(copy: &AdmittedCopy) -> Result<trace::CopyAtHalt, ReportError> {
    let (envelope, rule, content_type, blocks) = match copy {
        AdmittedCopy::Statement(copy) => (
            &copy.source_envelope,
            &copy.partition_rule_id,
            trace::CopyContentType::Statement,
            copy.blocks.as_slice(),
        ),
        AdmittedCopy::Pair(copy) => (
            &copy.source_envelope,
            &copy.partition_rule_id,
            trace::CopyContentType::Pair,
            std::slice::from_ref(&copy.block),
        ),
    };
    let event: SourceEnvelope = serde_json::from_str(envelope)
        .map_err(|_| ReportError::Invalid("held copy has no decodable source envelope"))?;
    if event.run.is_empty() || rule.is_empty() {
        return Err(ReportError::Invalid(
            "held copy has an empty source identity",
        ));
    }
    let mut sources = Vec::new();
    let mut previous = None;
    for block in blocks {
        if block.source.run != event.run
            || block.source.sequence.0 != event.sequence.0
            || previous.is_some_and(|prior| block.source.block <= prior)
        {
            return Err(ReportError::Invalid("held copy source identities disagree"));
        }
        previous = Some(block.source.block);
        push(
            &mut sources,
            trace::SourceBlockId {
                run: block.source.run.clone(),
                sequence: block.source.sequence.0.into(),
                block: block.source.block.into(),
            },
        )?;
    }
    Ok(trace::CopyAtHalt {
        source_event: trace::CopySourceEvent {
            run: event.run,
            sequence: event.sequence.0.into(),
        },
        partition_rule_id: rule.clone(),
        content_type,
        sources,
    })
}

#[cfg(test)]
#[path = "halt_snapshot_tests.rs"]
mod tests;
