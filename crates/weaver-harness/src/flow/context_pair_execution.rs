//! External execution custody. The caller elects which ready call to run and
//! supplies its exchange identity. This owner neither retries nor schedules
//! tools, and never creates a harness tool clock or a placement grant.

use super::pair_driver::PairTransaction;
use super::pairs::GateObservation;
use super::*;
use crate::flow::author::AuthoredSource;
use crate::flow::pump_transport::{OrganPort, TrackedStage};
use weaver_trace::Subsystem;
use weaver_types::state_flow::{CallId, NonDispatchCause, PairRequest, SourcePosition};
use weaver_types::{ExchangeId, Payload as OrganPayload};

#[derive(Debug)]
pub(in crate::flow) struct ExecutionBeginError {
    pub effective: EffectiveEmission,
    pub refusal: PairBeginRefusal,
    pub failure: Option<RunFailure>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum ExecutionRefusal {
    Busy,
    NotReady,
    InwardLane,
    NoGate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum ExecutionProgress {
    Pending,
    Sent,
    Terminal,
    Rejected,
    OtherTraffic,
}

impl RecordingContext {
    /// Only the successful repair owner's sealed result permits execution.
    #[allow(clippy::result_large_err)]
    pub(in crate::flow) fn begin_pair_execution(
        &mut self,
        effective: EffectiveEmission,
        io: &mut impl PumpIo,
    ) -> Result<(), ExecutionBeginError> {
        if let Err(failure) = self.live(io) {
            return Err(ExecutionBeginError {
                effective,
                refusal: PairBeginRefusal::WrongPhase,
                failure: Some(failure),
            });
        }
        let refusal = self.pair_begin_refusal().or_else(|| {
            if !effective
                .account()
                .generations()
                .all(|generation| self.pair_generation_matches(generation))
            {
                Some(PairBeginRefusal::WrongGeneration)
            } else if !self.repair_pair_custody_matches(effective.account()) {
                Some(PairBeginRefusal::LostCustody)
            } else {
                None
            }
        });
        if let Some(refusal) = refusal {
            return Err(ExecutionBeginError {
                effective,
                refusal,
                failure: None,
            });
        }
        if let Err((effective, _)) = self.pairs.accept_effective(effective) {
            return Err(ExecutionBeginError {
                effective,
                refusal: PairBeginRefusal::InvalidBatch,
                failure: None,
            });
        }
        self.pair_work = Some(PairTransaction::new(
            self.deadlines
                .get(PumpDeadline::Decision)
                .expect("checked wait"),
        ));
        Ok(())
    }

    pub(in crate::flow) fn next_pair_execution(
        &self,
    ) -> Option<&weaver_types::state_flow::PairStarted> {
        if self.failure.is_some()
            || self
                .pair_work
                .as_ref()
                .is_none_or(|work| work.leg.is_some() || work.inward.is_some())
        {
            return None;
        }
        self.pairs.next_ready()
    }

    /// Stage one exact call using the recorded load's tool policy. The pair
    /// owns the outgoing slot until advance_pair_send reports Sent or withdrawal
    /// accounts it. A busy slot never binds an execution identity.
    pub(in crate::flow) fn stage_pair_execution(
        &mut self,
        call: &CallId,
        exchange: ExchangeId,
        gate: Option<&mut OrganPort>,
        io: &mut impl PumpIo,
    ) -> Result<Result<ExecutionProgress, ExecutionRefusal>, RunFailure> {
        self.live(io)?;
        if let Some(port) = gate.as_ref() {
            self.check_pair_gate(port)?;
        }
        self.pair_wait_matches(io)?;
        let Some(ready) = self
            .next_pair_execution()
            .filter(|ready| &ready.call_id == call)
        else {
            return Ok(Err(ExecutionRefusal::NotReady));
        };
        let PairRequest::External { name, .. } = &ready.request else {
            return Ok(Err(ExecutionRefusal::InwardLane));
        };
        let policy = self
            .load_duty
            .as_ref()
            .and_then(|load| (name == "sh").then_some(load.tools.sh.as_ref()).flatten());
        let Some(policy) = policy else {
            self.pairs
                .refuse_ungranted(call, format!("tool {name} is not granted"))
                .map_err(|error| self.pair_fault(io, format!("tool refusal: {error:?}")))?;
            return Ok(Ok(ExecutionProgress::Terminal));
        };
        let clock_ms = policy.clock_ms;
        let Some(gate) = gate else {
            return Ok(Err(ExecutionRefusal::NoGate));
        };
        if gate.send_pending() {
            return Ok(Err(ExecutionRefusal::Busy));
        }
        if let Some(failure) = gate.failure() {
            return Err(RunFailure::Transport {
                source: PumpSource::Gate,
                failure: failure.clone(),
            });
        }
        let frame = self
            .pairs
            .bind_execution(call, exchange, clock_ms)
            .map_err(|error| self.pair_fault(io, format!("pair dispatch: {error:?}")))?;
        match gate.stage_tracked(frame) {
            Ok(TrackedStage::Queued(ticket)) => {
                self.pair_gate_identity = Some(ticket.port_identity());
                self.pair_work.as_mut().unwrap().send = Some(ticket);
            }
            Ok(TrackedStage::Busy(frame)) => {
                self.pair_work.as_mut().unwrap().refused_send = Some(frame);
                return Err(self.pair_fault(io, "exclusive gate send became busy"));
            }
            Err(error) => {
                self.pair_work.as_mut().unwrap().refused_send = Some(error.input);
                return Err(RunFailure::Transport {
                    source: PumpSource::Gate,
                    failure: error.failure,
                });
            }
        }
        // The existing filing window bounds getting the packet onto the wire.
        // It retires at successful send, before the gate's execution wait.
        Ok(Ok(ExecutionProgress::Pending))
    }

    /// One nonblocking send attempt. The common pump may have sent the owned
    /// packet already. Its empty slot is evidence only for our retained exchange.
    pub(in crate::flow) fn advance_pair_send(
        &mut self,
        gate: &mut OrganPort,
        io: &mut impl PumpIo,
    ) -> Result<ExecutionProgress, RunFailure> {
        self.check_pair_gate(gate)?;
        self.live(io)?;
        let Some((_, _, sent)) = self.pairs.executing() else {
            return Err(RunFailure::PairOwned);
        };
        if sent {
            return Ok(ExecutionProgress::Sent);
        }
        let Some(ticket) = self.pair_work.as_ref().and_then(|work| work.send.as_ref()) else {
            return Err(RunFailure::PairOwned);
        };
        if ticket.withdrawn() {
            return Err(RunFailure::PairOwned);
        }
        let result = gate.try_send_observed(io);
        self.reconcile_pair_send(io)?;
        result.map_err(|failure| RunFailure::Transport {
            source: PumpSource::Gate,
            failure,
        })?;
        Ok(if self.pairs.executing().is_some_and(|(_, _, sent)| sent) {
            ExecutionProgress::Sent
        } else {
            ExecutionProgress::Pending
        })
    }

    fn check_pair_gate(&self, gate: &OrganPort) -> Result<(), RunFailure> {
        if self
            .pair_gate_identity
            .as_ref()
            .is_some_and(|identity| !identity.matches(gate))
        {
            return Err(RunFailure::PairOwned);
        }
        Ok(())
    }

    // This hook runs before deadline reduction and after each common pump step.
    // Only the exact socket's successful atomic send can publish this ticket.
    pub(super) fn reconcile_pair_send(&mut self, io: &mut impl PumpIo) -> Result<bool, RunFailure> {
        if self.failure.is_some() {
            return Ok(false);
        }
        let Some((call, exchange, false)) = self.pairs.executing() else {
            return Ok(false);
        };
        if !self
            .pair_work
            .as_ref()
            .and_then(|work| work.send.as_ref())
            .is_some_and(|ticket| ticket.sent())
        {
            return Ok(false);
        }
        let work = self.pair_work.as_ref().expect("tracked send owner");
        let bound = work.bound.expect("dispatch wait retained through send");
        let sent_at = work.send.as_ref().unwrap().sent_at();
        let (call, exchange) = (call.clone(), exchange.clone());
        self.pairs
            .mark_sent(&call, &exchange)
            .map_err(|error| self.pair_fault(io, format!("pair send proof: {error:?}")))?;
        let Some(sent_at) = sent_at.filter(|at| *at >= bound.began_ns() && *at >= self.last_ns)
        else {
            return Err(self.fail(
                io,
                FlowHaltReason::TraceFailed,
                HaltPoint::Author,
                "successful gate send lacks a valid common-clock observation",
            ));
        };
        if bound.expired(sent_at) {
            return Err(self.expired(io, PumpDeadline::Decision, bound));
        }
        if self.pair_rejection_wait.is_none() {
            self.deadlines.disarm(PumpDeadline::Decision);
        }
        self.pair_work.as_mut().unwrap().bound = None;
        Ok(true)
    }

    /// Accept only a socket-owned ToolAnswer. Other traffic remains with its
    /// existing owner. A wrong reply is retained until its own Recorded event.
    pub(in crate::flow) fn observe_pair_reply(
        &mut self,
        gate: &mut OrganPort,
        io: &mut impl PumpIo,
    ) -> Result<ExecutionProgress, RunFailure> {
        self.check_pair_gate(gate)?;
        self.live(io)?;
        if self
            .pair_work
            .as_ref()
            .is_some_and(|work| work.leg.is_some() || work.inward.is_some())
            || self.pair_rejected_authoring
            || self.pair_refused_gate.is_some()
            || self.pairs.pending_rejection().is_some()
        {
            return Err(RunFailure::PairOwned);
        }
        let Some(held) = gate.held() else {
            if let Some(failure) = gate.failure() {
                return Err(RunFailure::Transport {
                    source: PumpSource::Gate,
                    failure: failure.clone(),
                });
            }
            return Ok(ExecutionProgress::Pending);
        };
        if !matches!(held.payload, OrganPayload::ToolAnswer(_))
            && self
                .pairs
                .executing()
                .is_none_or(|(_, exchange, _)| exchange != &held.exchange)
        {
            return Ok(ExecutionProgress::OtherTraffic);
        }
        let received = gate
            .take_received()
            .map_err(|failure| RunFailure::Transport {
                source: PumpSource::Gate,
                failure,
            })?
            .expect("held received envelope");
        if self.pair_gate_identity.is_none() {
            self.pair_gate_identity = Some(received.port_identity());
        }
        match self.pairs.observe_gate(received) {
            Ok(GateObservation::Matched(_)) => {
                self.arm_pair_terminal_wait(io)?;
                Ok(ExecutionProgress::Terminal)
            }
            Ok(GateObservation::Rejected) => Ok(ExecutionProgress::Rejected),
            Ok(GateObservation::Halted(_)) => {
                Err(self.pair_fault(io, "healthy pair acquired a halted result"))
            }
            Err((received, error)) => {
                self.pair_refused_gate = Some(received);
                Err(RunFailure::Transport {
                    source: PumpSource::Gate,
                    failure: crate::flow::pump_transport::Failure {
                        fault: crate::failure::ChannelFault::Undecodable,
                        detail: format!("invalid gate answer: {error:?}"),
                        errno: None,
                    },
                })
            }
        }
    }

    pub(super) fn arm_pair_terminal_wait(&mut self, io: &mut impl PumpIo) -> Result<(), RunFailure> {
        if self
            .pair_work
            .as_ref()
            .is_some_and(|work| work.bound.is_some())
        {
            return Ok(());
        }
        let elapsed = self
            .load_duty
            .as_ref()
            .expect("recorded load")
            .instruction
            .decision_wait
            .elapsed_ns;
        let bound = AbsoluteBound::after(self.last_ns, elapsed.0)
            .map_err(|error| self.pair_fault(io, format!("terminal wait: {error}")))?;
        // The caller checked live before consuming the answer or withdrawing
        // the send. Install its terminal wait without yielding after custody
        // has changed; the next live check still honors an earlier Leave.
        self.deadlines
            .arm(PumpDeadline::Decision, bound)
            .map_err(|error| self.pair_fault(io, format!("terminal wait: {error}")))?;
        self.pair_work.as_mut().unwrap().bound = Some(bound);
        Ok(())
    }

    /// The previous terminal receipt has completed its barrier. A later call's
    /// opening/send now blocks the next decision and starts a distinct measured
    /// window. Partial progress within either barrier never renews its bound.
    pub(super) fn begin_next_pair_send_wait(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        // take_owned_filing checked this boundary before consuming the terminal
        // receipt. Its installation and the next wait are one owner transition:
        // another live/arm check here could yield Leave after losing that
        // continuation, or after disarming the previous wait.
        self.pair_wait_matches(io)?;
        let elapsed = self
            .load_duty
            .as_ref()
            .expect("recorded load")
            .instruction
            .decision_wait
            .elapsed_ns
            .0;
        let bound = AbsoluteBound::after(self.last_ns, elapsed)
            .map_err(|error| self.pair_fault(io, format!("next send wait: {error}")))?;
        self.deadlines.disarm(PumpDeadline::Decision);
        self.deadlines
            .arm(PumpDeadline::Decision, bound)
            .map_err(|error| self.pair_fault(io, format!("next send wait: {error}")))?;
        self.pair_work.as_mut().unwrap().bound = Some(bound);
        Ok(())
    }

    /// The private author ticket belongs to this exact rejection until Recorded.
    /// Transport interruptions resume it, never construct a replacement event.
    pub(in crate::flow) fn record_pair_rejection<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<Sequence, RunFailure> {
        self.live(ports.io)?;
        if self
            .pair_work
            .as_ref()
            .is_some_and(|work| work.leg.is_some() || work.inward.is_some())
            || self.pairs.pending_rejection().is_none()
            || self.repair_blocks_external()
            || self.output.is_some()
            || self.measure_blocks_external()
        {
            return Err(RunFailure::PairOwned);
        }
        let sequence = if self.pair_rejected_authoring {
            self.resume_owned_author_and_wait(ports)?
        } else {
            if self.filing.is_some()
                || self.waiting_content.is_some()
                || self.completed_author.is_some()
                || self.barrier.pending().is_some()
                || self.barrier.pending_author().is_some()
                || self.flow.has_pending_record()
            {
                return Err(RunFailure::PairOwned);
            }
            let inherited = self.deadlines.get(PumpDeadline::Decision);
            let bound = match inherited {
                Some(bound) => bound,
                None => {
                    let elapsed = self
                        .load_duty
                        .as_ref()
                        .expect("recorded load")
                        .instruction
                        .decision_wait
                        .elapsed_ns
                        .0;
                    let bound = AbsoluteBound::after(self.last_ns, elapsed).map_err(|error| {
                        self.pair_fault(ports.io, format!("rejection wait: {error}"))
                    })?;
                    self.arm(PumpDeadline::Decision, bound, ports.io)?;
                    bound
                }
            };
            self.pair_rejection_wait = Some((bound, inherited.is_none()));
            let record = self.pairs.pending_rejection().unwrap().record().clone();
            self.waiting_content = Some(AuthoredEvent {
                kind: Kind::ToolReturnRejected,
                subsystem: Subsystem::Gate,
                turn: self.open_turn.clone(),
                source: AuthoredSource::Tool,
                causal_parent: record
                    .call_id
                    .as_ref()
                    .map(|call| Sequence(call.sequence.0)),
                payload: Some(Payload::RejectedReturn(Box::new(record))),
            });
            self.pair_rejected_authoring = true;
            self.continue_waiting_content(ports)?
        };
        let source = SourcePosition {
            block_id: weaver_types::state_flow::SourceBlockId {
                run: self.flow.state().identity().1.to_owned(),
                sequence: sequence.0.into(),
                block: 0.into(),
            },
            turn: self.open_turn.as_ref().map(|turn| turn.0.clone()),
        };
        self.pairs.ack_rejection_recorded(source).map_err(|error| {
            self.pair_fault(ports.io, format!("recorded rejected reply: {error:?}"))
        })?;
        self.pair_rejected_authoring = false;
        if let Some((_, created)) = self.pair_rejection_wait.take()
            && (created
                || self
                    .pair_work
                    .as_ref()
                    .is_some_and(|work| work.bound.is_none()))
        {
            self.deadlines.disarm(PumpDeadline::Decision);
        }
        Ok(sequence)
    }

    /// Observe whether the exact queued packet crossed the socket before
    /// accepting a stop or leave. An unsent packet is withdrawn before the
    /// ledger records not_dispatched. The caller handles sent-call controls
    /// through the gate's existing cancel/interrupt protocol.
    pub(in crate::flow) fn bar_executing_pairs(
        &mut self,
        cause: NonDispatchCause,
        gate: &mut OrganPort,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        self.check_pair_gate(gate)?;
        self.live(io)?;
        if self.repair_blocks_external() && self.repair_owns_pairs() {
            return Err(RunFailure::RepairOwned);
        }
        let unsent = self
            .pairs
            .executing()
            .filter(|(_, _, sent)| !*sent)
            .map(|(call, exchange, _)| (call.clone(), exchange.clone()));
        if let Some((call, exchange)) = unsent {
            let withdrawn =
                gate.withdraw_pending(&exchange)
                    .map_err(|failure| RunFailure::Transport {
                        source: PumpSource::Gate,
                        failure,
                    })?;
            let Some(frame) = withdrawn else {
                return Err(self.pair_fault(io, "pair lost its reserved unsent packet"));
            };
            self.pair_work.as_mut().unwrap().refused_send = Some(frame);
            self.pairs
                .set_not_dispatched(cause)
                .map_err(|error| self.pair_fault(io, format!("dispatch bar: {error:?}")))?;
            self.pairs
                .cancel_unsent(&call, &exchange)
                .map_err(|error| self.pair_fault(io, format!("unsent withdrawal: {error:?}")))?;
            self.arm_pair_terminal_wait(io)?;
            self.pair_work.as_mut().unwrap().refused_send = None;
        } else {
            self.pairs
                .set_not_dispatched(cause)
                .map_err(|error| self.pair_fault(io, format!("dispatch bar: {error:?}")))?;
        }
        Ok(())
    }

    pub(in crate::flow) fn set_pair_retry(
        &mut self,
        call: &CallId,
        predecessor: &CallId,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        self.live(io)?;
        self.pairs
            .set_retry_of(call, predecessor)
            .map_err(|error| self.pair_fault(io, format!("retry lineage: {error:?}")))
    }

    pub(in crate::flow) fn take_executed_pairs(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<Option<EffectiveEmission>, RunFailure> {
        self.live(io)?;
        if !self.pairs.has_effective() {
            return Err(RunFailure::PairOwned);
        }
        self.pair_wait_matches(io)?;
        if !self.pairs.is_complete() {
            return Ok(None);
        }
        let effective = self
            .pairs
            .finish_effective()
            .map_err(|error| self.pair_fault(io, format!("execution completion: {error:?}")))?;
        self.pair_work = None;
        Ok(Some(effective))
    }
}
