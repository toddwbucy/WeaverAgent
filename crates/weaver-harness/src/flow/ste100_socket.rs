//! Harness near side of Spec 02 CHANNEL, PORT and PORT-SERIALIZATION.
//!
//! | State | Input | Result |
//! | --- | --- | --- |
//! | Starting | Complete, matching Ready after bootstrap send | Idle, Ready once |
//! | Starting | Complete valid admission refusal | Refused, refusal once |
//! | Idle | Valid request | InFlight, whole request queued |
//! | InFlight | Another submit | Busy, second request returned unchanged |
//! | InFlight | Valid language result after complete send | Idle, Completed once |
//! | InFlight | Valid component result after complete send | Failed, Completed once |
//! | Live | Transport, framing or result fault | Failed, first cause retained |
//!
//! The caller supplies the exact scheme bytes and declaration from its judged
//! inventory. Admin and the SPU verify those artifact bytes and their digest.
//! This adapter neither loads artifacts nor claims to perform that judgment.
//! It checks the shared declarations and exact admission echo, owns one socket
//! cursor, and has no clock. The classifier coordinator owns request numbering
//! and the Recorded barrier after a completed result.

use std::os::fd::{BorrowedFd, OwnedFd};

use weaver_types::state_flow::{
    SchemeDeclaration, Ste100AdmissionRefused, Ste100Bootstrap, Ste100ComponentCode,
    Ste100Instruction, Ste100Ready, Ste100Reply, Ste100Request, Ste100Result, decode_strict,
};

use super::classifier::{PortFailure, ResultPoll, Ste100Port, Submit, validate_reply};
use super::ste100_channel::{Channel, Fault, Message, Receive, Tag};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AdmissionPoll {
    Pending,
    Ready(Ste100Ready),
    Refused(Ste100AdmissionRefused),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Starting,
    Idle,
    InFlight,
    Refused,
    Failed,
}

pub(crate) struct SocketPort {
    channel: Channel,
    scheme: SchemeDeclaration,
    instruction: Ste100Instruction,
    phase: Phase,
    outstanding: Option<Ste100Request>,
    failure: Option<PortFailure>,
}

/// A closed endpoint's original admission/request/framing evidence. No port
/// trait is implemented, so it cannot regain classifier or transport authority.
pub(super) struct RetiredSocketPort {
    _channel: Channel<()>,
    _scheme: SchemeDeclaration,
    _instruction: Ste100Instruction,
    _phase: Phase,
    _outstanding: Option<Ste100Request>,
    _failure: Option<PortFailure>,
}

impl SocketPort {
    #[cfg(test)]
    pub(super) fn test_evidence(&self) -> String {
        format!(
            "{:?}",
            (
                &self.scheme,
                &self.instruction,
                self.phase,
                &self.outstanding,
                &self.failure,
                self.channel.test_evidence()
            )
        )
    }
    #[cfg(test)]
    pub(super) fn test_partial_bytes(&self) -> usize {
        self.channel.test_partial_bytes()
    }
    /// Side-effect-free ownership check. It never polls or completes admission.
    pub(super) fn matches_admission(
        &self,
        scheme: &SchemeDeclaration,
        completion: Option<&AdmissionPoll>,
    ) -> bool {
        &self.scheme == scheme
            && self.failure.is_none()
            && self.outstanding.is_none()
            && match (self.phase, completion) {
                (Phase::Starting, None) => true,
                (Phase::Idle, Some(AdmissionPoll::Ready(ready))) => {
                    ready.validate_for(scheme, &self.instruction).is_ok()
                }
                (Phase::Refused, Some(AdmissionPoll::Refused(refused))) => {
                    refused.validate().is_ok()
                }
                _ => false,
            }
    }

    pub(super) fn retire_failed(self) -> RetiredSocketPort {
        let Self {
            channel,
            scheme,
            instruction,
            phase,
            outstanding,
            failure,
        } = self;
        RetiredSocketPort {
            _channel: channel.retire_failed(),
            _scheme: scheme,
            _instruction: instruction,
            _phase: phase,
            _outstanding: outstanding,
            _failure: failure,
        }
    }

    /// `bootstrap.scheme_text` and `scheme` must come from the same judged
    /// inventory. This port does not receive filesystem or digest authority.
    pub(crate) fn starting(
        end: OwnedFd,
        bootstrap: Ste100Bootstrap,
        scheme: SchemeDeclaration,
    ) -> Result<Self, PortFailure> {
        bootstrap
            .validate(bootstrap.instruction.max_wire_bytes)
            .map_err(|error| PortFailure::protocol(error.to_string()))?;
        scheme
            .validate()
            .map_err(|error| PortFailure::protocol(error.to_string()))?;
        let limit = usize::try_from(bootstrap.instruction.max_wire_bytes.0)
            .map_err(|_| PortFailure::protocol("STE100 wire limit does not fit this host"))?;
        let payload = serde_json::to_vec(&bootstrap)
            .map_err(|error| PortFailure::protocol(error.to_string()))?;
        if payload.len() > limit {
            return Err(PortFailure::protocol(
                "STE100 bootstrap exceeds its wire limit",
            ));
        }
        let mut channel = Channel::from_owned(end, limit).map_err(transport_failure)?;
        channel
            .begin_send(Tag::Bootstrap, 0, payload)
            .map_err(transport_failure)?;
        Ok(Self {
            channel,
            scheme,
            instruction: bootstrap.instruction,
            phase: Phase::Starting,
            outstanding: None,
            failure: None,
        })
    }

    pub(crate) fn readiness(&self) -> BorrowedFd<'_> {
        self.channel.readiness()
    }

    /// POLLOUT interest belongs to the concrete pump, not the Ste100Port trait.
    pub(crate) fn wants_write(&self) -> bool {
        matches!(self.phase, Phase::Starting | Phase::InFlight) && self.channel.has_pending_send()
    }

    /// EINTR can leave a receive probe or a consumed zero-length datagram's
    /// socket-status judgment pending. The pump must revisit that held work
    /// even when the descriptor no longer reports readable. This adds no clock.
    pub(crate) fn needs_progress(&self) -> bool {
        matches!(self.phase, Phase::Starting | Phase::Idle | Phase::InFlight)
            && self.channel.has_pending_receive_status()
    }

    /// Advance at most one outgoing and one incoming fragment. The enclosing
    /// load pump supplies its remaining deadline, never a fresh timeout here.
    pub(crate) fn advance_admission(&mut self) -> Result<AdmissionPoll, PortFailure> {
        self.ensure_live()?;
        if self.phase == Phase::Idle {
            return Ok(AdmissionPoll::Pending);
        }
        if self.phase != Phase::Starting {
            return Err(self.fail(PortFailure::protocol(
                "STE100 admission called after request submission",
            )));
        }
        let received = self.progress()?;
        let Receive::Complete(message) = received else {
            return Ok(AdmissionPoll::Pending);
        };
        if message.id != 0 {
            return Err(self.invalid("STE100 admission has a nonzero header ID"));
        }
        match message.tag {
            Tag::Ready => {
                let ready = payload_text(&message)
                    .and_then(|text| decode_strict::<Ste100Ready>(text).map_err(invalid_json))
                    .and_then(|ready| {
                        ready
                            .validate_for(&self.scheme, &self.instruction)
                            .map_err(|error| invalid_result(error.to_string()))?;
                        Ok(ready)
                    })
                    .map_err(|failure| self.fail(failure))?;
                self.phase = Phase::Idle;
                Ok(AdmissionPoll::Ready(ready))
            }
            Tag::AdmissionRefused => {
                let refused = payload_text(&message)
                    .and_then(|text| {
                        decode_strict::<Ste100AdmissionRefused>(text).map_err(invalid_json)
                    })
                    .and_then(|refused| {
                        refused
                            .validate()
                            .map_err(|error| invalid_result(error.to_string()))?;
                        Ok(refused)
                    })
                    .map_err(|failure| self.fail(failure))?;
                self.phase = Phase::Refused;
                self.channel.close();
                Ok(AdmissionPoll::Refused(refused))
            }
            _ => Err(self.invalid("STE100 admission has an unexpected message tag")),
        }
    }

    fn ensure_live(&mut self) -> Result<(), PortFailure> {
        if let Some(failure) = &self.failure {
            return Err(failure.clone());
        }
        if self.phase == Phase::Refused {
            return Err(self.fail(PortFailure::protocol("STE100 duty refused admission")));
        }
        Ok(())
    }

    fn fail(&mut self, failure: PortFailure) -> PortFailure {
        if self.failure.is_none() {
            self.failure = Some(failure);
            self.phase = Phase::Failed;
            self.channel.close();
        }
        self.failure
            .as_ref()
            .expect("first failure retained")
            .clone()
    }

    fn invalid(&mut self, detail: &str) -> PortFailure {
        self.fail(invalid_result(detail))
    }

    fn unexpected(&mut self, received: Receive, detail: &str) -> PortFailure {
        let mut failure = invalid_result(detail);
        if let Receive::Complete(message) = received
            && message.tag == Tag::Reply
            && let Ok(text) = payload_text(&message)
            && let Ok(reply) = decode_strict::<Ste100Reply>(text)
        {
            failure.received = Some(Box::new(reply));
        }
        self.fail(failure)
    }

    fn receive(&mut self) -> Result<Receive, PortFailure> {
        self.channel
            .try_receive()
            .map_err(|fault| self.fail(transport_failure(fault)))
    }

    fn progress(&mut self) -> Result<Receive, PortFailure> {
        if self.channel.has_pending_send() {
            // Inspect the pre-send state. A queued premature reply cannot
            // become timely just because this call sends the final fragment.
            let received = self.receive()?;
            if !matches!(received, Receive::Pending) {
                return Err(self.unexpected(
                    received,
                    "STE100 reply arrived before the whole send completed",
                ));
            }
            self.channel
                .try_send()
                .map_err(|fault| self.fail(transport_failure(fault)))?;
            return Ok(Receive::Pending);
        }
        self.receive()
    }
}

#[cfg(test)]
impl RetiredSocketPort {
    pub(super) fn test_evidence(&self) -> String {
        format!(
            "{:?}",
            (
                &self._scheme,
                &self._instruction,
                self._phase,
                &self._outstanding,
                &self._failure,
                self._channel.test_evidence()
            )
        )
    }
}

impl Ste100Port for SocketPort {
    fn readiness(&self) -> BorrowedFd<'_> {
        self.readiness()
    }

    fn submit(&mut self, request: Ste100Request) -> Result<Submit, PortFailure> {
        self.ensure_live()?;
        if self.phase == Phase::InFlight {
            return Ok(Submit::Busy(request));
        }
        if self.phase != Phase::Idle {
            return Err(self.fail(PortFailure::protocol(
                "STE100 request submitted before admission",
            )));
        }
        request
            .validate()
            .map_err(|error| self.fail(PortFailure::protocol(error.to_string())))?;
        if request.scheme_id != self.scheme.scheme_id {
            return Err(self.fail(PortFailure::protocol(
                "STE100 request differs from the admitted scheme",
            )));
        }
        // A queued stale reply must not acquire the identity of a new request.
        let received = self.receive()?;
        if !matches!(received, Receive::Pending) {
            return Err(self.unexpected(received, "unsolicited STE100 traffic before submission"));
        }
        let payload = serde_json::to_vec(&request)
            .map_err(|error| self.fail(PortFailure::protocol(error.to_string())))?;
        self.channel
            .begin_send(Tag::Request, request.request_id.0, payload)
            .map_err(|fault| self.fail(transport_failure(fault)))?;
        self.outstanding = Some(request);
        self.phase = Phase::InFlight;
        Ok(Submit::Submitted)
    }

    fn try_result(&mut self) -> Result<ResultPoll, PortFailure> {
        self.ensure_live()?;
        if self.phase == Phase::Starting {
            return Err(self.fail(PortFailure::protocol(
                "STE100 service polled before admission",
            )));
        }
        if self.phase == Phase::Idle {
            return match self.receive()? {
                Receive::Pending => Ok(ResultPoll::Pending),
                received => Err(self.unexpected(received, "unsolicited STE100 result")),
            };
        }
        let received = self.progress()?;
        let Receive::Complete(message) = received else {
            return Ok(ResultPoll::Pending);
        };
        if message.tag != Tag::Reply {
            return Err(self.invalid("STE100 result has an unexpected message tag"));
        }
        let reply = payload_text(&message)
            .and_then(|text| decode_strict::<Ste100Reply>(text).map_err(invalid_json))
            .map_err(|failure| self.fail(failure))?;
        if message.id != reply.request_id.0 {
            let mut failure = invalid_result("STE100 result header and JSON IDs disagree");
            failure.received = Some(Box::new(reply));
            return Err(self.fail(failure));
        }
        let request = self.outstanding.as_ref().expect("in-flight request held");
        validate_reply(&reply, request, &self.scheme).map_err(|failure| self.fail(failure))?;
        self.outstanding = None;
        match &reply.result {
            Ste100Result::ComponentFailed { code, detail } => {
                let mut failure = PortFailure::component(*code, detail.clone());
                failure.received = Some(Box::new(reply.clone()));
                self.fail(failure);
            }
            _ => self.phase = Phase::Idle,
        }
        Ok(ResultPoll::Completed(reply))
    }
}

fn invalid_result(detail: impl Into<String>) -> PortFailure {
    PortFailure::component(Ste100ComponentCode::InvalidResult, detail)
}

fn invalid_json(error: serde_json::Error) -> PortFailure {
    invalid_result(format!("invalid STE100 JSON: {error}"))
}

fn payload_text(message: &Message) -> Result<&str, PortFailure> {
    std::str::from_utf8(&message.payload)
        .map_err(|error| invalid_result(format!("invalid STE100 UTF-8: {error}")))
}

fn transport_failure(fault: Fault) -> PortFailure {
    let code = match fault {
        Fault::Closed => Ste100ComponentCode::Crashed,
        _ => Ste100ComponentCode::TransportFailed,
    };
    PortFailure::component(code, fault.to_string())
}

#[cfg(test)]
#[path = "ste100_socket_tests.rs"]
mod tests;
