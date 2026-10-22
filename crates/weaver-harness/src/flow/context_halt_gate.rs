//! One frozen execution's interrupt and terminal custody. Gate death remains
//! cleanup-required until an owner with process/cgroup custody proves cleanup.

use super::pairs::GateObservation;
use super::*;
use crate::flow::pump_transport::{
    Failure, OrganPort, OrganSend, PortIdentity, ReceivedOrgan, TrackedStage,
};
use weaver_types::state_flow::{CallId, PairEnded};
use weaver_types::{ExchangeId, KillCause, OrganEnvelope, Payload as OrganPayload, Position};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum HaltGateRefusal {
    NotFailed,
    ReportNotAttempted,
    ReportAborted,
    CaptureOwned,
    AlreadyStarted,
    NotStarted,
    NoExecution,
    ForeignPort,
    MissingSend,
}

/// These are retained progress observations, never containment or filing grants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum HaltGateProgress {
    Pending,
    InterruptSent,
    Terminal,
    Withdrawn,
    Unavailable,
    AbortRequired,
}

/// No public constructor or transfer into a healthy execution exists. The
/// original execution ticket stays with PairTransaction throughout this owner.
pub(in crate::flow) struct HaltGate {
    call: CallId,
    exchange: ExchangeId,
    identity: PortIdentity,
    interrupt: Option<OrganEnvelope>,
    interrupt_send: Option<OrganSend>,
    withdrawn: Option<OrganEnvelope>,
    refused: Option<ReceivedOrgan>,
    failed_wire: Option<Vec<u8>>,
    transport_failure: Option<Failure>,
    unavailable: Option<String>,
    aborted: Option<String>,
    terminal: bool,
}

impl HaltGate {
    pub(in crate::flow) fn call(&self) -> &CallId {
        &self.call
    }
    pub(in crate::flow) fn exchange(&self) -> &ExchangeId {
        &self.exchange
    }
    pub(in crate::flow) fn interrupt(&self) -> Option<&OrganEnvelope> {
        self.interrupt.as_ref()
    }
    pub(in crate::flow) fn interrupt_sent(&self) -> bool {
        self.interrupt_send.as_ref().is_some_and(OrganSend::sent)
    }
    pub(in crate::flow) fn withdrawn(&self) -> Option<&OrganEnvelope> {
        self.withdrawn.as_ref()
    }
    pub(in crate::flow) fn refused(&self) -> Option<&ReceivedOrgan> {
        self.refused.as_ref()
    }
    pub(in crate::flow) fn failed_wire(&self) -> Option<&[u8]> {
        self.failed_wire.as_deref()
    }
    pub(in crate::flow) fn transport_failure(&self) -> Option<&Failure> {
        self.transport_failure.as_ref()
    }
    pub(in crate::flow) fn unavailable(&self) -> Option<&str> {
        self.unavailable.as_deref()
    }
    pub(in crate::flow) fn aborted(&self) -> Option<&str> {
        self.aborted.as_deref()
    }
}

impl RecordingContext {
    /// This primitive has no publication owner. An installed capture account
    /// therefore keeps its prior Reporting obligation and refuses this entry.
    pub(in crate::flow) fn begin_halt_gate(
        &mut self,
        gate: &OrganPort,
    ) -> Result<(), HaltGateRefusal> {
        if self.phase != RecordingPhase::Failed || self.failure.is_none() {
            return Err(HaltGateRefusal::NotFailed);
        }
        if self.halt_gate.is_some() {
            return Err(HaltGateRefusal::AlreadyStarted);
        }
        let Some(report) = self
            .halt_report
            .as_ref()
            .filter(|report| report.attempted())
        else {
            return Err(HaltGateRefusal::ReportNotAttempted);
        };
        if report.aborted().is_some() {
            return Err(HaltGateRefusal::ReportAborted);
        }
        if self
            .flow
            .halt()
            .unwrap()
            .account()
            .unwrap()
            .capture
            .is_some()
        {
            return Err(HaltGateRefusal::CaptureOwned);
        }
        if self
            .pair_gate_identity
            .as_ref()
            .is_none_or(|identity| !identity.matches(gate))
        {
            return Err(HaltGateRefusal::ForeignPort);
        }
        let (call, exchange, _) = self
            .pairs
            .halt_execution()
            .ok_or(HaltGateRefusal::NoExecution)?;
        let ticket = self
            .pair_work
            .as_ref()
            .and_then(|work| work.send.as_ref())
            .ok_or(HaltGateRefusal::MissingSend)?;
        if !ticket.belongs_to(gate) {
            return Err(HaltGateRefusal::ForeignPort);
        }
        self.halt_gate = Some(HaltGate {
            call: call.clone(),
            exchange: exchange.clone(),
            identity: ticket.port_identity(),
            interrupt: None,
            interrupt_send: None,
            withdrawn: None,
            refused: None,
            failed_wire: None,
            transport_failure: None,
            unavailable: None,
            aborted: None,
            terminal: false,
        });
        Ok(())
    }

    pub(in crate::flow) fn halt_gate(&self) -> Option<&HaltGate> {
        self.halt_gate.as_ref()
    }

    pub(in crate::flow) fn halt_gate_terminal(&self) -> Option<(&PairEnded, &[u8])> {
        self.pairs
            .halt_gate_terminal(&self.halt_gate.as_ref()?.call)
    }

    /// At most one receive and one send syscall, without a poll or an ordinary
    /// reduction. A caller can service other teardown work between every step.
    pub(in crate::flow) fn advance_halt_gate(
        &mut self,
        gate: &mut OrganPort,
        io: &mut impl PumpIo,
    ) -> Result<HaltGateProgress, HaltGateRefusal> {
        if self.phase != RecordingPhase::Failed || self.failure.is_none() {
            return Err(HaltGateRefusal::NotFailed);
        }
        let owner = self.halt_gate.as_ref().ok_or(HaltGateRefusal::NotStarted)?;
        if !owner.identity.matches(gate) {
            return Err(HaltGateRefusal::ForeignPort);
        }
        if !self.halt_gate_clock(io) {
            return Ok(HaltGateProgress::AbortRequired);
        }
        let progress = self.advance_halt_gate_owned(gate, io);
        // Every taken packet, successful send and withdrawal is retained first.
        if !self.halt_gate_clock(io) {
            return Ok(HaltGateProgress::AbortRequired);
        }
        Ok(progress)
    }

    fn advance_halt_gate_owned(
        &mut self,
        gate: &mut OrganPort,
        io: &mut impl PumpIo,
    ) -> HaltGateProgress {
        let owner = self.halt_gate.as_ref().unwrap();
        if owner.unavailable.is_some() {
            return HaltGateProgress::Unavailable;
        }
        if owner.terminal {
            return HaltGateProgress::Terminal;
        }
        if owner.withdrawn.is_some() {
            return HaltGateProgress::Withdrawn;
        }
        let sent = self
            .pair_work
            .as_ref()
            .and_then(|work| work.send.as_ref())
            .is_some_and(OrganSend::sent);
        if !sent {
            let exchange = self.halt_gate.as_ref().unwrap().exchange.clone();
            return match gate.withdraw_pending(&exchange) {
                Ok(Some(frame)) => {
                    self.halt_gate.as_mut().unwrap().withdrawn = Some(frame);
                    // The frozen ledger/account remains an account of the latch.
                    // No new NotDispatched cause is invented for flow failure.
                    HaltGateProgress::Withdrawn
                }
                Ok(None) => self.halt_gate_unavailable(
                    FlowHaltReason::ProtocolFailed,
                    "halt lost its exact unsent execution".into(),
                ),
                Err(failure) => self.halt_gate_transport_failed(gate, failure),
            };
        }
        let ticket = self.pair_work.as_ref().unwrap().send.as_ref().unwrap();
        if let Err(error) = self.pairs.reconcile_halt_send(gate, ticket) {
            return self.halt_gate_unavailable(
                FlowHaltReason::ProtocolFailed,
                format!("halt send proof: {error:?}"),
            );
        }
        if self.halt_gate_terminal().is_some() {
            self.halt_gate.as_mut().unwrap().terminal = true;
            return self.finish_halt_gate_terminal(gate);
        }
        // A held result wins even if a later terminal probe failed the socket.
        if gate.held().is_none()
            && gate.failure().is_none()
            && let Err(failure) = gate.try_receive()
        {
            return self.halt_gate_transport_failed(gate, failure);
        }
        if gate.held().is_some() {
            let received = match gate.take_received() {
                Ok(Some(received)) => received,
                Ok(None) => unreachable!("held gate result remains owned"),
                Err(failure) => return self.halt_gate_transport_failed(gate, failure),
            };
            let expected = &self.halt_gate.as_ref().unwrap().exchange;
            if received.envelope().exchange != *expected
                || received.envelope().position != Position::Close
                || !matches!(received.envelope().payload, OrganPayload::ToolAnswer(_))
            {
                self.halt_gate.as_mut().unwrap().refused = Some(received);
                return self.halt_gate_unavailable(
                    FlowHaltReason::ProtocolFailed,
                    "halt received a mismatched gate terminal".into(),
                );
            }
            match self.pairs.observe_gate(received) {
                Ok(GateObservation::Halted(call))
                    if call == self.halt_gate.as_ref().unwrap().call =>
                {
                    self.halt_gate.as_mut().unwrap().terminal = true;
                    return self.finish_halt_gate_terminal(gate);
                }
                Err((received, error)) => {
                    self.halt_gate.as_mut().unwrap().refused = Some(received);
                    return self.halt_gate_unavailable(
                        FlowHaltReason::ProtocolFailed,
                        format!("halt gate terminal: {error:?}"),
                    );
                }
                Ok(_) => {
                    return self.halt_gate_unavailable(
                        FlowHaltReason::ProtocolFailed,
                        "halt result lost its frozen execution".into(),
                    );
                }
            }
        }
        if let Some(failure) = gate.failure().cloned() {
            return self.halt_gate_transport_failed(gate, failure);
        }
        if self.halt_gate.as_ref().unwrap().interrupt_sent() {
            return HaltGateProgress::Pending;
        }
        if self.halt_gate.as_ref().unwrap().interrupt_send.is_none() {
            // A different lifecycle owner keeps its outgoing slot. This owner
            // cannot send or replace that packet under a new identity.
            if gate.send_pending() {
                return HaltGateProgress::Pending;
            }
            let owner = self.halt_gate.as_mut().unwrap();
            let frame = OrganEnvelope {
                exchange: owner.exchange.clone(),
                position: Position::Continue,
                payload: OrganPayload::ToolInterrupt {
                    by: KillCause::FlowFailure,
                },
            };
            owner.interrupt = Some(frame.clone());
            match gate.stage_tracked(frame) {
                Ok(TrackedStage::Queued(ticket)) => owner.interrupt_send = Some(ticket),
                Ok(TrackedStage::Busy(_)) => return HaltGateProgress::Pending,
                Err(failure) => return self.halt_gate_transport_failed(gate, failure.failure),
            }
        }
        match gate.try_send_observed(io) {
            Ok(_) if self.halt_gate.as_ref().unwrap().interrupt_sent() => {
                HaltGateProgress::InterruptSent
            }
            Ok(_) => HaltGateProgress::Pending,
            Err(failure) => self.halt_gate_transport_failed(gate, failure),
        }
    }

    fn finish_halt_gate_terminal(&mut self, gate: &mut OrganPort) -> HaltGateProgress {
        let owner = self.halt_gate.as_ref().unwrap();
        if owner
            .interrupt_send
            .as_ref()
            .is_some_and(|ticket| !ticket.sent())
        {
            let exchange = owner.exchange.clone();
            match gate.withdraw_pending(&exchange) {
                Ok(Some(_)) => {}
                Ok(None) => {
                    return self.halt_gate_unavailable(
                        FlowHaltReason::ProtocolFailed,
                        "halt lost its unsent interrupt after a terminal".into(),
                    );
                }
                Err(failure) => return self.halt_gate_transport_failed(gate, failure),
            }
        }
        HaltGateProgress::Terminal
    }

    fn halt_gate_transport_failed(
        &mut self,
        gate: &mut OrganPort,
        failure: Failure,
    ) -> HaltGateProgress {
        let owner = self.halt_gate.as_mut().unwrap();
        owner.failed_wire = gate.take_failed_wire();
        owner.transport_failure = Some(failure.clone());
        self.halt_gate_unavailable(
            FlowHaltReason::CarryFailed,
            format!("halt gate transport: {}", failure.detail),
        )
    }

    fn halt_gate_unavailable(
        &mut self,
        reason: FlowHaltReason,
        detail: String,
    ) -> HaltGateProgress {
        self.halt_gate.as_mut().unwrap().unavailable = Some(detail.clone());
        self.halt_gate_secondary(reason, Some(self.last_ns), detail);
        HaltGateProgress::Unavailable
    }

    fn halt_gate_clock(&mut self, io: &mut impl PumpIo) -> bool {
        if self.halt_gate.as_ref().unwrap().aborted.is_some() {
            return false;
        }
        if let Some(detail) = self.halt_report.as_ref().and_then(HaltReport::aborted) {
            self.halt_gate.as_mut().unwrap().aborted = Some(detail.into());
            return false;
        }
        let (observed, error) = match io.now() {
            Ok(now) if now < self.last_ns => (Some(now), Some("halt gate clock reversed".into())),
            Ok(now) => {
                self.last_ns = now;
                let error = match self.flow.halt().unwrap().deadline() {
                    Ok(deadline) if now < deadline => None,
                    Ok(_) => Some("halt gate reached its original deadline".into()),
                    Err(error) => Some(format!("halt gate has no deadline: {error}")),
                };
                (Some(now), error)
            }
            Err(error) => (None, Some(format!("halt gate clock: {error}"))),
        };
        if let Some(detail) = error {
            self.halt_gate.as_mut().unwrap().aborted = Some(detail.clone());
            self.halt_gate_secondary(FlowHaltReason::TraceFailed, observed, detail);
            return false;
        }
        true
    }

    fn halt_gate_secondary(
        &mut self,
        reason: FlowHaltReason,
        observed: Option<u64>,
        detail: String,
    ) {
        let _ = self
            .flow
            .halt_mut()
            .unwrap()
            .append_secondary(SecondaryFailure {
                reason,
                point: HaltPoint::Teardown,
                observed_ns: observed.map(Into::into),
                detail,
            });
    }
}
