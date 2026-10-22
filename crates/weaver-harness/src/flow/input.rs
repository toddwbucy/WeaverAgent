//! The run-local input transaction under HARNESS-INPUT and Spec 02 INPUT/REUSE.
//! Recording, the one clock and socket progress belong to the recording context.

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use weaver_trace::serving::{Kind, Payload, payloads::InputOfferedRecord};
use weaver_trace::{Sequence, Subsystem};
use weaver_traits::{ContentBlock, Message, Role};
use weaver_types::state_flow::{
    DecimalU64, InputRefusalCase, Ste100Reply, Ste100Result, Ste100Target,
};
use weaver_types::{InputRefusal, TurnFrame, TurnRequest};

use super::author::{AuthoredEvent, AuthoredSource};
use super::classifier::AcceptedReuse;
use super::clock::AbsoluteBound;
use super::coordinator::CheckOperation;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum InputState {
    Offered,
    ReadyToCheck,
    Checking,
    NeedRefusal,
    Refused,
    Accepted,
    Consumed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct InputSnapshot {
    pub input_id: DecimalU64,
    pub offered_sequence: Option<DecimalU64>,
    pub request_id: Option<DecimalU64>,
    pub bound: AbsoluteBound,
    pub state: InputState,
    pub cancelled: bool,
    pub halted: bool,
    pub result_recorded: bool,
    pub response_sent: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum InputError {
    InvalidIdentity,
    Busy,
    MissingDialer,
    CounterExhausted,
    InvalidTransition,
    Correlation,
    MissingAcceptedProof,
    ForeignIdentity,
    Revoked,
    Halted,
}

impl fmt::Display for InputError {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        out.write_str(match self {
            Self::InvalidIdentity => "input owner identity is empty",
            Self::Busy => "another input operation still owns service",
            Self::MissingDialer => "inbound frame has no kernel-authenticated dialer",
            Self::CounterExhausted => "input identifier counter exhausted",
            Self::InvalidTransition => "input transition is not legal in this state",
            Self::Correlation => "input completion differs from its outstanding check",
            Self::MissingAcceptedProof => "accepted input lacks its exact recorded capability",
            Self::ForeignIdentity => "accepted input belongs to another run or scheme",
            Self::Revoked => "accepted input was revoked or already consumed",
            Self::Halted => "input owner is halted",
        })
    }
}

#[derive(Debug)]
pub(super) struct BeginFailure {
    pub frame: TurnFrame,
    pub bound: AbsoluteBound,
    pub error: InputError,
}

#[derive(Debug)]
pub(super) struct CompletionFailure {
    pub operation: CheckOperation,
    pub reply: Ste100Reply,
    pub reuse: Option<AcceptedReuse>,
    pub error: InputError,
}

/// This capability is deliberately not Clone. The owner keeps a revocation
/// handle after transfer, so leave or halt can still prevent consumption.
#[derive(Debug)]
pub(super) struct AcceptedInput {
    reuse: AcceptedReuse,
    dialer: u32,
    revoked: Arc<AtomicBool>,
}

#[derive(Debug)]
pub(super) struct ConsumeFailure {
    pub token: AcceptedInput,
    pub error: InputError,
}

#[derive(Debug)]
pub(super) struct ConsumedInput {
    pub input_id: DecimalU64,
    pub offered_sequence: DecimalU64,
    pub dialer: u32,
    pub message: Message,
    pub reuse: Option<AcceptedReuse>,
}

struct Current {
    snapshot: InputSnapshot,
    frame: TurnFrame,
    dialer: u32,
    text: Option<String>,
    reply: Option<Ste100Reply>,
    refusal: Option<InputRefusal>,
    refusal_sequence: Option<Sequence>,
    cancellation_disposed: bool,
    revoked: Arc<AtomicBool>,
    accepted: Option<AcceptedInput>,
}

pub(super) struct InputSlot {
    run: String,
    scheme_id: String,
    next_id: Option<u64>,
    current: Option<Current>,
    halted: bool,
}

impl InputSlot {
    pub(super) fn new(run: String, scheme_id: String) -> Result<Self, InputError> {
        if run.is_empty() || scheme_id.is_empty() {
            return Err(InputError::InvalidIdentity);
        }
        Ok(Self {
            run,
            scheme_id,
            next_id: Some(1),
            current: None,
            halted: false,
        })
    }

    /// The caller invokes begin only when the frame becomes eligible. Queuing
    /// behind an earlier turn must not allocate an ID or start this bound.
    pub(super) fn begin(
        &mut self,
        frame: TurnFrame,
        bound: AbsoluteBound,
    ) -> Result<(), BeginFailure> {
        let error = if self.halted {
            Some(InputError::Halted)
        } else if self.current.is_some() {
            Some(InputError::Busy)
        } else if frame.dialer.is_none() {
            Some(InputError::MissingDialer)
        } else if self.next_id.is_none() {
            Some(InputError::CounterExhausted)
        } else {
            None
        };
        if let Some(error) = error {
            return Err(BeginFailure {
                frame,
                bound,
                error,
            });
        }
        let id = self.next_id.expect("checked counter");
        self.next_id = id.checked_add(1);
        self.current = Some(Current {
            snapshot: InputSnapshot {
                input_id: id.into(),
                offered_sequence: None,
                request_id: None,
                bound,
                state: InputState::Offered,
                cancelled: false,
                halted: false,
                result_recorded: false,
                response_sent: false,
            },
            dialer: frame.dialer.expect("checked boundary identity"),
            frame,
            text: None,
            reply: None,
            refusal: None,
            refusal_sequence: None,
            cancellation_disposed: false,
            revoked: Arc::new(AtomicBool::new(false)),
            accepted: None,
        });
        Ok(())
    }

    pub(super) fn frame(&self) -> Option<&TurnFrame> {
        self.current.as_ref().map(|current| &current.frame)
    }

    pub(super) fn snapshot(&self) -> Option<InputSnapshot> {
        self.current
            .as_ref()
            .map(|current| current.snapshot.clone())
    }

    pub(super) fn offered_event(&self) -> Option<AuthoredEvent> {
        let current = self.current.as_ref()?;
        if current.snapshot.offered_sequence.is_some() || self.halted {
            return None;
        }
        Some(AuthoredEvent {
            kind: Kind::InputOffered,
            subsystem: Subsystem::Harness,
            turn: None,
            source: AuthoredSource::BoundaryInput(current.dialer),
            causal_parent: None,
            payload: Some(Payload::InputOffered(Box::new(InputOfferedRecord {
                input_id: current.snapshot.input_id.0.into(),
                octets: current.frame.octets.clone(),
            }))),
        })
    }

    /// Called only after the exact offered event is Recorded. No octet decoding
    /// or request parsing occurs at begin or while offered authoring is pending.
    pub(super) fn offered_recorded(&mut self, sequence: Sequence) -> Result<(), InputError> {
        let current = self.live_current()?;
        if sequence.0 == 0
            || current.snapshot.offered_sequence.is_some()
            || !matches!(
                current.snapshot.state,
                InputState::Offered | InputState::Cancelled
            )
        {
            return Err(InputError::InvalidTransition);
        }
        current.snapshot.offered_sequence = Some(sequence.0.into());
        if current.snapshot.cancelled {
            return Ok(());
        }
        let parsed = current
            .frame
            .octets()
            .ok_or_else(|| "the frame's carriage is not the canonical encoding".to_owned())
            .and_then(|octets| TurnRequest::parse(&octets))
            .and_then(|request| {
                if request.role.is_none() {
                    Ok(request.text)
                } else {
                    Err("a serving input must omit the role member".into())
                }
            });
        match parsed {
            Ok(text) => {
                current.text = Some(text);
                current.snapshot.state = InputState::ReadyToCheck;
            }
            Err(detail) => {
                current.refusal = Some(InputRefusal {
                    input_id: current.snapshot.input_id,
                    case: InputRefusalCase::InvalidInputShape,
                    detail,
                });
                current.snapshot.state = InputState::NeedRefusal;
            }
        }
        Ok(())
    }

    pub(super) fn check_operation(&self) -> Option<CheckOperation> {
        let current = self.current.as_ref()?;
        if self.halted || current.snapshot.state != InputState::ReadyToCheck {
            return None;
        }
        Some(CheckOperation::Input {
            input_id: current.snapshot.input_id,
            offered_sequence: current
                .snapshot
                .offered_sequence
                .expect("recorded offered source"),
            text: current.text.as_ref().expect("parsed input").clone(),
        })
    }

    pub(super) fn submitted(&mut self, request_id: DecimalU64) -> Result<(), InputError> {
        let current = self.live_current()?;
        if request_id.0 == 0
            || current.snapshot.state != InputState::ReadyToCheck
            || current.snapshot.request_id.is_some()
        {
            return Err(InputError::InvalidTransition);
        }
        current.snapshot.request_id = Some(request_id);
        current.snapshot.state = InputState::Checking;
        Ok(())
    }

    #[allow(clippy::result_large_err)]
    pub(super) fn accept_completion(
        &mut self,
        operation: CheckOperation,
        reply: Ste100Reply,
        reuse: Option<AcceptedReuse>,
    ) -> Result<(), CompletionFailure> {
        if let Err(error) = self.validate_completion(&operation, &reply, reuse.as_ref()) {
            return Err(CompletionFailure {
                operation,
                reply,
                reuse,
                error,
            });
        }
        let current = self.current.as_mut().expect("validated current input");
        current.snapshot.result_recorded = true;
        if current.snapshot.cancelled {
            // The late result remains evidence, but even a valid recorded
            // accepted capability cannot mint or revive a cancelled token.
            current.snapshot.state = InputState::Cancelled;
        } else {
            match &reply.result {
                Ste100Result::Accepted { .. } => {
                    current.accepted = Some(AcceptedInput {
                        reuse: reuse.expect("validated recorded capability"),
                        dialer: current.dialer,
                        revoked: current.revoked.clone(),
                    });
                    current.snapshot.state = InputState::Accepted;
                }
                Ste100Result::NotSte100 { detail, .. } => {
                    current.refusal = Some(InputRefusal {
                        input_id: current.snapshot.input_id,
                        case: InputRefusalCase::NonSte100,
                        detail: detail.clone(),
                    });
                    current.snapshot.state = InputState::NeedRefusal;
                }
                Ste100Result::ComponentFailed { .. } => unreachable!("component result refused"),
            }
        }
        current.reply = Some(reply);
        Ok(())
    }

    fn validate_completion(
        &self,
        operation: &CheckOperation,
        reply: &Ste100Reply,
        reuse: Option<&AcceptedReuse>,
    ) -> Result<(), InputError> {
        if self.halted {
            return Err(InputError::Halted);
        }
        let current = self.current.as_ref().ok_or(InputError::InvalidTransition)?;
        if !matches!(
            current.snapshot.state,
            InputState::Checking | InputState::Cancelled
        ) || current.snapshot.request_id.is_none()
            || current.snapshot.result_recorded
        {
            return Err(InputError::InvalidTransition);
        }
        let CheckOperation::Input {
            input_id,
            offered_sequence,
            text,
        } = operation
        else {
            return Err(InputError::Correlation);
        };
        if *input_id != current.snapshot.input_id
            || Some(*offered_sequence) != current.snapshot.offered_sequence
            || current.text.as_ref() != Some(text)
            || Some(reply.request_id) != current.snapshot.request_id
            || reply.scheme_id != self.scheme_id
            || reply.target
                != (Ste100Target::Input {
                    input_id: *input_id,
                    block: 0.into(),
                })
        {
            return Err(InputError::Correlation);
        }
        match (&reply.result, reuse) {
            (Ste100Result::Accepted { .. }, Some(reuse))
                if reuse.run() == self.run
                    && reuse.offered_sequence() == *offered_sequence
                    && reuse.text() == text
                    && reuse.scheme_id() == self.scheme_id
                    && reuse.reply() == reply
                    && reuse.request().request_id == reply.request_id
                    && reuse.request().target == reply.target =>
            {
                Ok(())
            }
            (Ste100Result::NotSte100 { .. }, None) => Ok(()),
            _ => Err(InputError::MissingAcceptedProof),
        }
    }

    pub(super) fn refusal(&self) -> Option<InputRefusal> {
        let current = self.current.as_ref()?;
        if self.halted || current.snapshot.state != InputState::NeedRefusal {
            return None;
        }
        current.refusal.clone()
    }

    pub(super) fn refusal_recorded(&mut self, sequence: Sequence) -> Result<(), InputError> {
        let current = self.live_current()?;
        let owed_refusal = current.snapshot.state == InputState::NeedRefusal
            || (current.snapshot.state == InputState::Cancelled && current.refusal.is_some());
        if !owed_refusal
            || current.refusal_sequence.is_some()
            || current
                .snapshot
                .offered_sequence
                .is_none_or(|offered| sequence.0 <= offered.0)
        {
            return Err(InputError::InvalidTransition);
        }
        current.refusal_sequence = Some(sequence);
        if !current.snapshot.cancelled {
            current.snapshot.state = InputState::Refused;
        }
        Ok(())
    }

    pub(super) fn cancel(&mut self) -> Result<(), InputError> {
        let current = self.live_current()?;
        if current.snapshot.state == InputState::Consumed {
            return Err(InputError::InvalidTransition);
        }
        current.revoked.store(true, Ordering::Release);
        current.snapshot.cancelled = true;
        current.snapshot.state = InputState::Cancelled;
        Ok(())
    }

    /// The driver calls this after the existing transport owner accepts the
    /// unnamed stopped close. It adds no cancellation event or new wire kind.
    pub(super) fn cancellation_disposed(&mut self) -> Result<(), InputError> {
        let current = self.live_current()?;
        if !current.snapshot.cancelled {
            return Err(InputError::InvalidTransition);
        }
        current.cancellation_disposed = true;
        current.snapshot.response_sent = true;
        Ok(())
    }

    pub(super) fn halt(&mut self) {
        self.halted = true;
        if let Some(current) = &mut self.current {
            current.snapshot.halted = true;
            current.revoked.store(true, Ordering::Release);
        }
    }

    pub(super) fn take_accepted(&mut self) -> Option<AcceptedInput> {
        if self.halted {
            return None;
        }
        let current = self.current.as_mut()?;
        if current.snapshot.state != InputState::Accepted {
            return None;
        }
        current.accepted.take()
    }

    /// Read-only pre-admission check for the explicit mark owner. Consumption
    /// below repeats this check and remains the sole revoking transfer.
    pub(super) fn validate_accepted(
        &self,
        token: &AcceptedInput,
        run: &str,
        scheme_id: &str,
    ) -> Result<(), InputError> {
        let error = if run != self.run
            || scheme_id != self.scheme_id
            || token.reuse.run() != self.run
            || token.reuse.scheme_id() != self.scheme_id
        {
            Some(InputError::ForeignIdentity)
        } else if self.halted || token.revoked.load(Ordering::Acquire) {
            Some(InputError::Revoked)
        } else if self.current.as_ref().is_none_or(|current| {
            current.snapshot.state != InputState::Accepted
                || current.accepted.is_some()
                || !Arc::ptr_eq(&current.revoked, &token.revoked)
                || current.snapshot.offered_sequence != Some(token.reuse.offered_sequence())
                || current.dialer != token.dialer
        }) {
            Some(InputError::InvalidTransition)
        } else {
            None
        };
        error.map_or(Ok(()), Err)
    }

    pub(super) fn consume_input(
        &mut self,
        token: AcceptedInput,
        run: &str,
        scheme_id: &str,
    ) -> Result<ConsumedInput, ConsumeFailure> {
        if let Err(error) = self.validate_accepted(&token, run, scheme_id) {
            return Err(ConsumeFailure { token, error });
        }
        let current = self.current.as_mut().expect("checked owning operation");
        current.snapshot.state = InputState::Consumed;
        current.revoked.store(true, Ordering::Release);
        Ok(ConsumedInput {
            input_id: current.snapshot.input_id,
            offered_sequence: token.reuse.offered_sequence(),
            dialer: token.dialer,
            message: Message {
                role: Role::User,
                content: vec![ContentBlock::Text {
                    text: token.reuse.text().to_owned(),
                }],
            },
            reuse: Some(token.reuse),
        })
    }

    pub(super) fn gate_may_disarm(&self) -> bool {
        if self.halted {
            return false;
        }
        self.current
            .as_ref()
            .is_some_and(|current| match current.snapshot.state {
                InputState::Accepted | InputState::Consumed | InputState::Refused => true,
                InputState::Cancelled => {
                    current.snapshot.offered_sequence.is_some()
                        && if current.snapshot.request_id.is_some() {
                            current.snapshot.result_recorded
                        } else {
                            current.cancellation_disposed
                        }
                }
                _ => false,
            })
    }

    pub(super) fn close_frame(&self) -> Option<TurnFrame> {
        let current = self.current.as_ref()?;
        if current.snapshot.state == InputState::Consumed
            || current.snapshot.response_sent
            || (!self.halted && current.snapshot.offered_sequence.is_none())
        {
            return None;
        }
        let bytes: &[u8] = if self.halted {
            br#"{"kind":"stopped","reason":"The agent stopped because state flow failed."}"#
        } else if current.snapshot.cancelled {
            br#"{"kind":"stopped","reason":"The agent is leaving."}"#
        } else if current.snapshot.state == InputState::Refused {
            br#"{"kind":"refused","reason":"Restate the input in STE100."}"#
        } else {
            return None;
        };
        Some(TurnFrame::carry(bytes))
    }

    pub(super) fn response_sent(&mut self) -> Result<(), InputError> {
        if self.halted {
            let current = self.current.as_mut().ok_or(InputError::InvalidTransition)?;
            if current.snapshot.state == InputState::Consumed {
                return Err(InputError::InvalidTransition);
            }
            current.snapshot.response_sent = true;
            return Ok(());
        }
        if self
            .current
            .as_ref()
            .is_some_and(|current| current.snapshot.cancelled)
        {
            return self.cancellation_disposed();
        }
        let current = self.live_current()?;
        if current.snapshot.state != InputState::Refused {
            return Err(InputError::InvalidTransition);
        }
        current.snapshot.response_sent = true;
        Ok(())
    }

    pub(super) fn finish(&mut self) -> Result<(), InputError> {
        if self.halted {
            return Err(InputError::Halted);
        }
        let current = self.current.as_ref().ok_or(InputError::InvalidTransition)?;
        if current.snapshot.state != InputState::Consumed
            && !(current.snapshot.response_sent && self.gate_may_disarm())
        {
            return Err(InputError::InvalidTransition);
        }
        self.current = None;
        Ok(())
    }

    fn live_current(&mut self) -> Result<&mut Current, InputError> {
        if self.halted {
            return Err(InputError::Halted);
        }
        self.current.as_mut().ok_or(InputError::InvalidTransition)
    }
}

#[cfg(test)]
#[path = "input_tests.rs"]
mod tests;
