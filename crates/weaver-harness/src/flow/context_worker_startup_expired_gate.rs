//! One Lower and one unwind sweep for the original startup gate.
//! The accepted Leave owns cleanup without restarting the original Enter.
use super::*;
use crate::flow::author::AuthoredSource;
use crate::flow::context::failed_enter_driver::ChildState;
use crate::flow::context::startup_composition::refusal::response::listener::RollbackDials;
use crate::flow::context::worker_startup::failed::HeldGate;
use crate::flow::context::*;
use crate::flow::pump::{PumpPorts, PumpSource};
use crate::flow::pump_transport::PortIdentity;
use crate::flow::pump_transport::ReceivedOrgan;
use crate::tools_cgroup::{Containment, Context as ContainmentContext};
use std::sync::Arc;
use weaver_trace::{Sequence, Subsystem};
use weaver_types::{ExchangeId, LifecycleAsk, Payload as WirePayload, RefusalRecord};

#[path = "context_worker_startup_expired_gate_failed.rs"]
mod failed;

#[cfg(test)]
#[path = "context_worker_startup_expired_gate_test_access.rs"]
mod test_access;

struct GateRecord {
    // The exact received frame remains owned until its refusal/fault records.
    received: Option<ReceivedOrgan>,
    event: Option<AuthoredEvent>,
    started: bool,
    stopped: StoppedAuthor,
    claim: Option<Arc<RollbackGateAuthor>>,
}

impl GateRecord {
    fn unloading(received: ReceivedOrgan) -> Self {
        let payload = RefusalRecord::Lifecycle {
            asked: LifecycleAsk::Leave,
            refusal: LifecycleRefusal::Unloading,
        };
        Self {
            received: Some(received),
            event: Some(AuthoredEvent {
                kind: Kind::Refusal,
                subsystem: Subsystem::Harness,
                turn: None,
                source: AuthoredSource::Runtime,
                causal_parent: None,
                payload: Some(serving::Payload::Refusal(
                    serde_json::value::to_raw_value(&payload).expect("typed refusal serializes"),
                )),
            }),
            started: false,
            stopped: StoppedAuthor::default(),
            claim: None,
        }
    }
}

/// Minted only by this gate's actual cleanup record. The exact noncontent
/// event, root identity and accepted Leave cannot be widened by a caller.
#[derive(Debug)]
pub(in crate::flow::context) struct RollbackGateAuthor {
    origin: Arc<()>,
    leave: AbsoluteBound,
    kind: Kind,
    subsystem: Subsystem,
    payload: String,
    ticket: std::sync::OnceLock<u64>,
    sequence: std::sync::OnceLock<Sequence>,
}

impl RollbackGateAuthor {
    fn new(context: &RecordingContext, leave: AbsoluteBound, event: &AuthoredEvent) -> Self {
        let payload = match event.payload.as_ref() {
            Some(serving::Payload::Refusal(raw) | serving::Payload::Fault(raw)) => raw.get().into(),
            _ => unreachable!("gate cleanup only owns refusal or fault input"),
        };
        Self {
            origin: Arc::clone(&context.read_origin),
            leave,
            kind: event.kind,
            subsystem: event.subsystem,
            payload,
            ticket: std::sync::OnceLock::new(),
            sequence: std::sync::OnceLock::new(),
        }
    }

    pub(in crate::flow::context) fn allows(&self, context: &RecordingContext) -> bool {
        Arc::ptr_eq(&self.origin, &context.read_origin)
            && context.phase == RecordingPhase::Entered
            && context.deadlines.get(PumpDeadline::Leave) == Some(self.leave)
            && !self.leave.expired(context.last_ns)
            && context.failure.is_none()
            && context.flow.halt().is_none()
    }

    pub(in crate::flow::context) fn allows_event(&self, event: &AuthoredEvent) -> bool {
        if event.kind != self.kind
            || event.subsystem != self.subsystem
            || event.turn.is_some()
            || event.causal_parent.is_some()
            || !matches!(event.source, AuthoredSource::Runtime)
        {
            return false;
        }
        match (&event.kind, &event.payload) {
            (Kind::Refusal, Some(serving::Payload::Refusal(raw))) => {
                event.subsystem == Subsystem::Harness
                    && raw.get() == self.payload
                    && matches!(
                        serde_json::from_str::<RefusalRecord>(raw.get()),
                        Ok(RefusalRecord::Lifecycle {
                            asked: LifecycleAsk::Leave,
                            refusal: LifecycleRefusal::Unloading
                        })
                    )
            }
            (Kind::Fault, Some(serving::Payload::Fault(raw))) => {
                matches!(event.subsystem, Subsystem::Gate | Subsystem::Harness)
                    && raw.get() == self.payload
            }
            _ => false,
        }
    }

    pub(in crate::flow::context) fn claim(&self, ticket: u64) -> bool {
        self.ticket.set(ticket).is_ok()
    }
    pub(in crate::flow::context) fn ticket(&self) -> Option<u64> {
        self.ticket.get().copied()
    }
    pub(in crate::flow::context) fn sequence(&self) -> Option<Sequence> {
        self.sequence.get().copied()
    }
    pub(in crate::flow::context) fn bind(&self, ticket: Option<u64>, sequence: Sequence) {
        if self.ticket().is_some() && self.ticket() == ticket {
            let _ = self.sequence.set(sequence);
        }
    }
}

struct GateDeath {
    // Channel loss is retained before sampling the fallible clock. None is an
    // unanchored observation, never permission to invent another wait window.
    observed_ns: Option<u64>,
    deadline_ns: Option<u64>,
    snapshot: Option<crate::spawn::LastWordSnapshot>,
    settled: bool,
}

/// Borrow the original Raise witnesses without changing their owner or Enter.
#[derive(Clone, Copy)]
pub(in crate::flow::context::worker_startup) struct GateRollbackEvidence<'a> {
    pub(in crate::flow::context::worker_startup) raise: &'a OrganEnvelope,
    pub(in crate::flow::context::worker_startup) send: Option<&'a OrganSend>,
    pub(in crate::flow::context::worker_startup) reply: Option<&'a ReceivedOrgan>,
    pub(in crate::flow::context::worker_startup) gate_enter: Option<AbsoluteBound>,
    pub(in crate::flow::context::worker_startup) enter: AbsoluteBound,
    pub(in crate::flow::context::worker_startup) current_outgoing: u64,
    pub(in crate::flow::context::worker_startup) control:
        Option<(&'a OrganEnvelope, &'a OrganSend)>,
}

impl<'a> GateRollbackEvidence<'a> {
    pub(in crate::flow::context::worker_startup) fn from_held(
        gate: &'a HeldGate,
        enter: AbsoluteBound,
    ) -> Self {
        Self {
            raise: &gate.raise,
            send: gate.send.as_ref(),
            reply: gate.reply.as_ref(),
            gate_enter: gate.enter,
            enter,
            current_outgoing: gate.raise.exchange.ordinal,
            control: None,
        }
    }
}

fn preceding_quiesce_reply(
    original: GateRollbackEvidence<'_>,
    port: &crate::flow::pump_transport::OrganPort,
    received: &ReceivedOrgan,
) -> bool {
    let Some((request, send)) = original.control else {
        return false;
    };
    let reply = received.envelope();
    request.position == Position::Open
        && request.exchange.opener == Opener::Harness
        && matches!(
            request.payload,
            WirePayload::Directive(LifecycleDirective::Quiesce)
        )
        && reply.position == Position::Close
        && reply.exchange == request.exchange
        && matches!(
            reply.payload,
            WirePayload::Answer(LifecycleAnswer::GateQuiesced)
        )
        && received.belongs_to(port)
        && send.belongs_to(port)
        && send.matches_exchange(&request.exchange)
        && send.sent()
        && send.sent_at().is_some()
}

/// An accepted rollback Leave borrows the actual resources and retained Raise.
/// Successful Ready and expired Enter project their own retained accounts.
pub(in crate::flow::context::worker_startup) struct GateRollbackHost<'a> {
    pub(in crate::flow::context::worker_startup) context: &'a mut RecordingContext,
    pub(in crate::flow::context::worker_startup) arms: &'a mut StartupGate,
    pub(in crate::flow::context::worker_startup) original: GateRollbackEvidence<'a>,
    pub(in crate::flow::context::worker_startup) leave: AbsoluteBound,
    pub(in crate::flow::context::worker_startup) peer: &'a mut LifecyclePeer,
}

pub(in crate::flow::context::worker_startup) struct RollbackGate {
    closed: Option<RollbackGateClosed>,
    bound: AbsoluteBound,
    lower: Option<OrganEnvelope>,
    send: Option<OrganSend>,
    raise_reply: Option<ReceivedOrgan>,
    quiesce_reply: Option<ReceivedOrgan>,
    lower_reply: Option<ReceivedOrgan>,
    pending: Option<GateRecord>,
    // At most one further frame can be held by the pump during authoring.
    received: Option<ReceivedOrgan>,
    retired: Option<PacketPort<OrganEnvelope, ()>>,
    transport_done: bool,
    lower_retired: bool,
    sweep_done: bool,
    containment: Option<Containment>,
    sweep_fault: Option<AuthoredEvent>,
    death: Option<GateDeath>,
    refusal: Option<LifecycleRefusal>,
    last_recorded: Option<Sequence>,
    complete: bool,
    failed_pair_unavailable: bool,
    failed_retired_answer: Option<ReceivedOrgan>,
}

/// The actual gate transport has been retired and its Leave sweep performed.
/// This is a world-connection disposition, never a successful response receipt.
pub(in crate::flow) struct RollbackGateClosed {
    origin: Arc<()>,
    port: PortIdentity,
    leave: AbsoluteBound,
    disposed: bool,
    execution_reaped: bool,
    containment: Option<Containment>,
}

impl RollbackGateClosed {
    pub(in crate::flow::context) fn containment(&self) -> Option<Containment> {
        self.disposed.then_some(self.containment).flatten()
    }
    pub(in crate::flow::context) fn allows(&self, context: &RecordingContext) -> bool {
        self.disposed
            && Arc::ptr_eq(&self.origin, &context.read_origin)
            && context.deadlines.get(PumpDeadline::Leave) == Some(self.leave)
            && !self.leave.expired(context.last_ns)
            && context.failure().is_none()
    }

    pub(in crate::flow::context) fn owns(&self, received: &ReceivedOrgan) -> bool {
        self.disposed && self.port.matches_received(received)
    }

    pub(in crate::flow::context) fn owns_port(&self, port: &PortIdentity) -> bool {
        self.disposed && self.port.same(port)
    }

    pub(in crate::flow::context) fn execution_reaped(&self) -> bool {
        self.disposed && self.execution_reaped
    }
}

pub(super) type ExpiredGateRollback = RollbackGate;

impl RollbackGate {
    pub(in crate::flow::context::worker_startup) fn owns_author(&self) -> bool {
        self.pending.as_ref().is_some_and(|record| record.started)
    }
    pub(in crate::flow::context::worker_startup) fn world_closed(
        &self,
    ) -> Option<&RollbackGateClosed> {
        self.closed.as_ref().filter(|proof| proof.disposed)
    }

    pub(in crate::flow::context::worker_startup) fn route_retired_answer(
        &mut self,
        context: &mut RecordingContext,
        io: &mut impl PumpIo,
    ) -> Result<bool, RunFailure> {
        if !self.received.as_ref().is_some_and(|received| {
            matches!(received.envelope().payload, WirePayload::ToolAnswer(_))
        }) {
            return Ok(true);
        }
        let Some(proof) = self.closed.as_ref().filter(|proof| proof.disposed) else {
            return Ok(false);
        };
        let received = self.received.take().unwrap();
        match context.observe_rollback_pair_reply(proof, received, io) {
            Ok(_) => Ok(true),
            Err((original, error)) => {
                self.received = original;
                if matches!(error, RunFailure::PairOwned) {
                    Ok(false)
                } else {
                    Err(error)
                }
            }
        }
    }

    pub(in crate::flow::context::worker_startup) fn settle_retired_execution(
        &mut self,
        context: &mut RecordingContext,
        io: &mut impl PumpIo,
    ) -> Result<bool, RunFailure> {
        if let (Some(proof), Some(retired)) = (
            self.closed.as_ref().filter(|proof| proof.disposed),
            self.retired.as_mut(),
        ) {
            context.prepare_rollback_gate_execution(proof, retired, io)?;
        }
        if !self.route_retired_answer(context, io)? {
            return Ok(false);
        }
        if let (Some(proof), Some(retired)) = (
            self.closed.as_ref().filter(|proof| proof.disposed),
            self.retired.as_mut(),
        ) {
            return context.close_rollback_gate_execution(proof, retired, io);
        }
        Ok(true)
    }

    /// The serving seat already owns this FIFO. Take one exact original only
    /// when this gate's existing record slot is free; no overflow queue is made.
    #[allow(clippy::result_large_err)] // Refusal returns the move-owned original.
    pub(in crate::flow::context::worker_startup) fn retain_queued_input(
        &mut self,
        input: ReceivedOrgan,
    ) -> Result<(), ReceivedOrgan> {
        if self.pending.is_some() || self.received.is_some() {
            return Err(input);
        }
        self.pending = Some(GateRecord::unloading(input));
        self.complete = false;
        Ok(())
    }

    /// Progress Lower without competing for an ordinary author or waiting for
    /// the active turn's cancellation. Tool answers stay in the original port
    /// for its pair owner. The same Lower cap applies while another exact send
    /// is being reconciled by that owner.
    pub(in crate::flow::context::worker_startup) fn transport_step(
        &mut self,
        host: &mut GateRollbackHost<'_>,
        dials: &mut RollbackDials,
        allow_lower: bool,
        io: &mut impl PumpIo,
    ) -> Result<bool, RunFailure> {
        match host.context.live(io) {
            Ok(()) => (),
            Err(RunFailure::Wait {
                key: PumpDeadline::Lower,
                bound,
            }) if bound == self.bound => (),
            Err(
                error @ RunFailure::Wait {
                    key: PumpDeadline::AdminRead,
                    ..
                },
            ) => {
                dials.recover(host.context, error, io)?;
            }
            Err(error) => return Err(error),
        }
        if !self.lower_retired && self.bound.expired(host.context.last_ns) {
            self.end_transport(host.arms, true, false);
            self.retire_lower(host.context);
        }
        if self.transport_done {
            match host.arms.child.as_mut().map(|child| child.poll()) {
                Some(ChildState::Reaped(_)) => {
                    if let Some(proof) = &mut self.closed {
                        proof.execution_reaped = true;
                    }
                    self.retire_lower(host.context);
                }
                Some(ChildState::NotWaitable) => {
                    return Err(host.context.fail(
                        io,
                        FlowHaltReason::ProtocolFailed,
                        HaltPoint::Teardown,
                        "rollback gate lost its actual child reap",
                    ));
                }
                _ => (),
            }
            return Ok(self.complete);
        }
        if self.lower.is_none() {
            self.end_transport(host.arms, true, false);
            return Ok(false);
        }
        let result = (|| {
            if self.received.is_none() {
                let port = host
                    .arms
                    .port
                    .as_mut()
                    .expect("original live rollback gate");
                if port.held().is_none() {
                    port.try_receive()
                        .map_err(|failure| RunFailure::Transport {
                            source: PumpSource::Gate,
                            failure,
                        })?;
                }
                if port.held().is_some_and(|frame| {
                    !matches!(frame.payload, WirePayload::ToolAnswer(_))
                        && (self.pending.is_none()
                            || !matches!(
                                frame.payload,
                                WirePayload::Frame(_) | WirePayload::Fault(_)
                            ))
                }) {
                    self.received =
                        port.take_received()
                            .map_err(|failure| RunFailure::Transport {
                                source: PumpSource::Gate,
                                failure,
                            })?;
                    host.context.live(io)?;
                    if let Some(received) = self.received.take() {
                        self.receive(host, received, true);
                    }
                }
            }
            if allow_lower && !self.transport_done {
                self.send_lower(host, io)?;
            }
            Ok(false)
        })();
        match result {
            Err(RunFailure::Wait {
                key: PumpDeadline::Lower,
                bound,
            }) if bound == self.bound => {
                self.end_transport(host.arms, true, false);
                self.retire_lower(host.context);
                Ok(false)
            }
            Err(RunFailure::Transport {
                source: PumpSource::Gate,
                ..
            }) => {
                if self
                    .send
                    .as_ref()
                    .is_some_and(|send| send.sent() && send.sent_at().is_none())
                {
                    return Err(host.context.fail(
                        io,
                        FlowHaltReason::TraceFailed,
                        HaltPoint::Author,
                        "rollback Lower send clock failed",
                    ));
                }
                self.observe_death(host, dials, io)?;
                Ok(false)
            }
            other => other,
        }
    }
    pub(in crate::flow::context::worker_startup) fn bound(&self) -> AbsoluteBound {
        self.bound
    }
    pub(in crate::flow::context::worker_startup) fn refusal(&self) -> Option<&LifecycleRefusal> {
        self.refusal.as_ref()
    }
    pub(in crate::flow::context::worker_startup) fn swept(&self) -> bool {
        self.sweep_done
    }
    pub(in crate::flow::context::worker_startup) fn dispose(&mut self, arms: &mut StartupGate) {
        self.end_transport(arms, true, false);
    }

    pub(in crate::flow::context::worker_startup) fn new(
        host: &mut GateRollbackHost<'_>,
        io: &mut impl PumpIo,
    ) -> Result<Self, RunFailure> {
        let context = &mut *host.context;
        let leave = host.leave;
        context.live(io)?;
        let bound = AbsoluteBound::from_absolute(
            leave.began_ns(),
            leave
                .began_ns()
                .saturating_add(weaver_types::LOWER_BOUND_MS * 1_000_000)
                .min(leave.deadline_ns()),
        )
        .expect("Lower is inside actual Leave");
        context
            .deadlines
            .arm(PumpDeadline::Lower, bound)
            .map_err(|error| {
                context.fail(
                    io,
                    FlowHaltReason::ProtocolFailed,
                    HaltPoint::Author,
                    format!("rollback Lower bound: {error}"),
                )
            })?;
        let gate = host.original;
        let arms = &*host.arms;
        // A whole sent Raise needs disposal even when its reply arrived late.
        // Queued/EAGAIN Raise is unsent and must never be flushed by cleanup.
        let raised = arms.port.as_ref().is_some_and(|port| {
            gate.send.as_ref().is_some_and(|send| {
                send.sent()
                    && send.belongs_to(port)
                    && send.matches_exchange(&gate.raise.exchange)
                    && send.sent_at().is_some_and(|at| {
                        at >= host.original.enter.began_ns() && !host.original.enter.expired(at)
                    })
            })
        });
        let lower = if raised {
            gate.current_outgoing
                .checked_add(1)
                .map(|ordinal| OrganEnvelope {
                    exchange: ExchangeId {
                        opener: Opener::Harness,
                        ordinal,
                    },
                    position: Position::Open,
                    payload: WirePayload::Directive(LifecycleDirective::Lower),
                })
        } else {
            None
        };
        Ok(Self {
            closed: arms.port.as_ref().map(|port| RollbackGateClosed {
                origin: Arc::clone(&context.read_origin),
                port: port.port_identity(),
                leave,
                disposed: false,
                execution_reaped: false,
                containment: None,
            }),
            bound,
            lower,
            send: None,
            raise_reply: None,
            quiesce_reply: None,
            lower_reply: None,
            pending: None,
            received: None,
            retired: None,
            transport_done: false,
            lower_retired: false,
            sweep_done: false,
            containment: None,
            sweep_fault: None,
            death: None,
            refusal: (raised && gate.current_outgoing == u64::MAX)
                .then_some(LifecycleRefusal::Malformed),
            last_recorded: None,
            complete: false,
            failed_pair_unavailable: false,
            failed_retired_answer: None,
        })
    }

    /// Close transport first. A frame already received by a nested author wait
    /// still owes its record; unread socket bytes are not invented as receives.
    fn retire(&mut self, arms: &mut StartupGate) {
        if let Some(mut port) = arms.port.take() {
            if port.held().is_some() {
                assert!(self.received.is_none(), "one held receive slot");
                self.received = port.take_received().expect("held exact gate packet");
            }
            self.retired = Some(port.retire_failed());
        }
        self.transport_done = true;
    }

    fn sweep(&mut self, arms: &StartupGate, died: bool) {
        if self.sweep_done {
            return;
        }
        self.sweep_done = true;
        self.containment = arms.tools.as_ref().map(|tools| {
            crate::tools_cgroup::contain(
                tools,
                ContainmentContext::Leave,
                crate::tools_cgroup::CONTAINMENT_BOUND,
            )
        });
        // A healthy observed death owns a bounded stderr settlement first.
        // A failed owner retains its existing facts but never authors them.
        if self.death.is_none() && (died || self.containment == Some(Containment::Undelivered)) {
            self.sweep_fault = Some(self.sweep_event(died, None));
        }
    }

    fn sweep_event(&self, died: bool, last_word: Option<serde_json::Value>) -> AuthoredEvent {
        let mut account = serde_json::json!({"organ": "gate"});
        if let Some(containment) = self.containment {
            account["containment"] = serde_json::Value::String(match containment {
                Containment::Delivered => "tools/ killed, not waited on inside the leave",
                Containment::Undelivered => "containment failed: the write of cgroup.kill and the pinned fallback both failed, the kill not delivered",
                _ => unreachable!("Leave containment never waits"),
            }.into());
        }
        if died {
            account["death"] =
                serde_json::Value::String("its channel was lost at the lower".into());
        }
        if let Some(word) = last_word {
            account["last_word"] = word;
        }
        let report = crate::authorship::harness_report(
            if died {
                weaver_types::FaultCase::OrganDeathObserved
            } else {
                weaver_types::FaultCase::ToolsContainmentUndelivered
            },
            &account.to_string(),
        );
        AuthoredEvent {
            kind: Kind::Fault,
            subsystem: Subsystem::Harness,
            turn: None,
            source: AuthoredSource::Runtime,
            causal_parent: None,
            payload: Some(serving::Payload::Fault(
                serde_json::value::to_raw_value(&report).expect("typed fault serializes"),
            )),
        }
    }

    fn observe_death(
        &mut self,
        host: &mut GateRollbackHost<'_>,
        dials: &mut RollbackDials,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        assert!(self.death.is_none(), "one observed original gate death");
        if host
            .arms
            .port
            .as_ref()
            .is_some_and(|port| host.context.serving_organs.gate_matches(port))
        {
            // The serving owner retains this exact observation and its author.
            // Lower still closes and reaps the original child under its cap,
            // but cannot create a second death or a new last-word allowance.
            self.end_transport(host.arms, false, false);
            return Ok(());
        }
        self.death = Some(GateDeath {
            observed_ns: None,
            deadline_ns: None,
            snapshot: host
                .arms
                .child
                .as_ref()
                .and_then(|child| child.last_word_snapshot()),
            settled: false,
        });
        self.end_transport(host.arms, false, true);
        let clock = match host.context.live(io) {
            Err(
                error @ RunFailure::Wait {
                    key: PumpDeadline::AdminRead,
                    ..
                },
            ) => dials.recover(host.context, error, io),
            result => result,
        };
        match clock {
            Ok(()) => (),
            Err(RunFailure::Wait {
                key: PumpDeadline::Lower,
                bound,
            }) if bound == self.bound => {
                self.end_transport(host.arms, true, false);
                self.retire_lower(host.context);
            }
            Err(error) => return Err(error),
        }
        let observed = host.context.last_ns;
        let death = self.death.as_mut().unwrap();
        death.observed_ns = Some(observed);
        death.deadline_ns = Some(
            observed
                .saturating_add(crate::spawn::LAST_WORD_SETTLE_BOUND.as_nanos() as u64)
                .min(host.leave.deadline_ns()),
        );
        Ok(())
    }

    fn settle_death(
        &mut self,
        host: &mut GateRollbackHost<'_>,
        io: &mut impl PumpIo,
    ) -> Result<bool, RunFailure> {
        let death = self.death.as_mut().unwrap();
        assert!(!death.settled, "settled death event stays frozen");
        if let Some(snapshot) = host
            .arms
            .child
            .as_ref()
            .and_then(|child| child.last_word_snapshot())
        {
            death.snapshot = Some(snapshot);
        }
        let now = host.context.last_ns;
        let end = death
            .deadline_ns
            .expect("healthy death has one observed anchor");
        if death.snapshot.as_ref().is_some_and(|snapshot| snapshot.eof) || now >= end {
            death.settled = true;
            let word = death
                .snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.word.clone());
            assert!(self.sweep_fault.is_none(), "one owed sweep account");
            self.sweep_fault = Some(self.sweep_event(true, word));
            return Ok(true);
        }
        let mut wake = now
            .saturating_add(10_000_000)
            .min(end)
            .min(host.leave.deadline_ns());
        if !self.lower_retired {
            wake = wake.min(self.bound.deadline_ns());
        }
        io.wait(&[], Some(wake)).map_err(|error| {
            host.context.fail(
                io,
                FlowHaltReason::TraceFailed,
                HaltPoint::Author,
                format!("rollback gate last-word wait: {error}"),
            )
        })?;
        Ok(false)
    }

    fn end_transport(&mut self, arms: &mut StartupGate, kill: bool, died: bool) {
        self.retire(arms);
        self.sweep(arms, died);
        if kill && let Some(child) = &mut arms.child {
            child.stop_at_cap();
        }
        if let Some(proof) = &mut self.closed {
            proof.disposed = true;
            proof.containment = self.containment;
        }
    }

    fn retire_lower(&mut self, context: &mut RecordingContext) {
        if !self.lower_retired {
            assert_eq!(context.complete_wait(PumpDeadline::Lower), Some(self.bound));
            self.lower_retired = true;
        }
    }

    fn receive(&mut self, host: &mut GateRollbackHost<'_>, received: ReceivedOrgan, timely: bool) {
        if self.quiesce_reply.is_none()
            && host
                .arms
                .port
                .as_ref()
                .is_some_and(|port| preceding_quiesce_reply(host.original, port, &received))
        {
            // This is retained evidence of the preceding control. It grants
            // neither GateStopped nor a completed graceful phase.
            self.quiesce_reply = Some(received);
            return;
        }
        let frame = received.envelope();
        if frame.exchange.opener == Opener::Gate
            && frame.position == Position::Open
            && matches!(frame.payload, WirePayload::Frame(_))
        {
            self.pending = Some(GateRecord::unloading(received));
            return;
        }
        if let WirePayload::Fault(report) = &frame.payload {
            let event = AuthoredEvent {
                kind: Kind::Fault,
                subsystem: Subsystem::Gate,
                turn: None,
                source: AuthoredSource::Runtime,
                causal_parent: None,
                payload: Some(serving::Payload::Fault(
                    serde_json::value::to_raw_value(report).expect("typed fault serializes"),
                )),
            };
            self.pending = Some(GateRecord {
                received: Some(received),
                event: Some(event),
                started: false,
                stopped: StoppedAuthor::default(),
                claim: None,
            });
            return;
        }
        let gate = host.original;
        let arms = &mut *host.arms;
        if self.raise_reply.is_none()
            && gate.reply.is_none()
            && frame.position == Position::Close
            && gate
                .send
                .as_ref()
                .is_some_and(|send| send.sent() && send.matches_exchange(&frame.exchange))
            && matches!(
                frame.payload,
                WirePayload::Answer(LifecycleAnswer::GateReady) | WirePayload::Refusal(_)
            )
        {
            // Late evidence stays late. It grants no boot progress or Ready.
            self.raise_reply = Some(received);
            return;
        }
        let exact = timely
            && frame.position == Position::Close
            && arms.port.as_ref().is_some_and(|port| {
                received.belongs_to(port)
                    && self.send.as_ref().is_some_and(|send| {
                        send.sent()
                            && send.belongs_to(port)
                            && send.matches_exchange(&frame.exchange)
                            && send.sent_at().is_some_and(|at| {
                                at >= self.bound.began_ns() && !self.bound.expired(at)
                            })
                    })
            });
        if exact {
            match &frame.payload {
                WirePayload::Answer(LifecycleAnswer::GateStopped) => (),
                WirePayload::Refusal(reason) => {
                    self.refusal
                        .get_or_insert_with(|| LifecycleRefusal::OrganRefused {
                            organ: weaver_types::RefusingOrgan::Gate,
                            reason: Box::new(reason.clone()),
                        });
                }
                _ => {
                    self.refusal.get_or_insert(LifecycleRefusal::Malformed);
                }
            }
        } else if !self.transport_done {
            self.refusal.get_or_insert(LifecycleRefusal::Malformed);
        }
        self.lower_reply = Some(received);
        self.end_transport(arms, false, false);
    }

    fn record(
        &mut self,
        host: &mut GateRollbackHost<'_>,
        dials: &mut RollbackDials,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        let record = self.pending.as_mut().expect("owed gate record");
        record.stopped.check()?;
        if !record.started && !host.context.rollback_gate_author_available() {
            return Ok(());
        }
        if record.claim.is_none() {
            record.claim = Some(Arc::new(RollbackGateAuthor::new(
                host.context,
                host.leave,
                record.event.as_ref().expect("unstarted gate record"),
            )));
        }
        let claim = record.claim.as_ref().unwrap();
        let mut ports = PumpPorts {
            lifecycle: dials.ports(host.peer),
            gate: host.arms.port.as_mut().filter(|port| {
                // A nested serving wait already yielded this packet to its
                // owner. Keep the second Frame held while this exact record
                // resumes; the original Lower/Leave still bounds the wait.
                port.held().is_none() || host.context.rollback_gate_port.is_none()
            }),
            decode: None,
            label: None,
            io,
        };
        let result = if let Some(event) = record.event.take() {
            record.started = true;
            host.context.author_rollback_gate(&mut ports, claim, event)
        } else {
            host.context.resume_rollback_gate_author(&mut ports, claim)
        };
        match result {
            Ok(sequence) => {
                self.last_recorded = Some(sequence);
                self.pending.take();
                Ok(())
            }
            Err(error) => {
                // Recoverable cleanup interruptions retain the same original.
                // The owner below verifies the exact deadline/dial before it
                // permits continuation under the unchanged Leave.
                if matches!(
                    error.reason,
                    RunFailure::Busy(_)
                        | RunFailure::RollbackGateHeld
                        | RunFailure::Wait {
                            key: PumpDeadline::Lower | PumpDeadline::AdminRead,
                            ..
                        }
                        | RunFailure::Transport {
                            source: PumpSource::Gate
                                | PumpSource::LeavePeer
                                | PumpSource::AdminDial,
                            ..
                        }
                ) {
                    record.started = error.original.is_none();
                    record.event = error.original;
                    Err(error.reason)
                } else {
                    Err(record.stopped.retain_started(error, &mut record.started))
                }
            }
        }
    }

    pub(in crate::flow::context::worker_startup) fn step(
        &mut self,
        host: &mut GateRollbackHost<'_>,
        dials: &mut RollbackDials,
        allow_lower: bool,
        io: &mut impl PumpIo,
    ) -> Result<bool, RunFailure> {
        if self.complete {
            return Ok(true);
        }
        // A latched component fault wins over lifecycle timeout and all sends.
        let clock = match host.context.live(io) {
            Err(
                error @ RunFailure::Wait {
                    key: PumpDeadline::AdminRead,
                    ..
                },
            ) => dials.recover(host.context, error, io),
            result => result,
        };
        match clock {
            Ok(()) => (),
            Err(RunFailure::Wait {
                key: PumpDeadline::Lower,
                bound,
            }) if bound == self.bound => (),
            Err(error) => return Err(error),
        }
        if !self.lower_retired && self.bound.expired(host.context.last_ns) {
            self.end_transport(host.arms, true, false);
            self.retire_lower(host.context);
        }
        if !self.transport_done && self.lower.is_none() {
            self.end_transport(host.arms, true, false);
        }
        let result = match self.step_live(host, dials, allow_lower, io) {
            Err(RunFailure::Transport {
                source: PumpSource::LeavePeer,
                ..
            }) => host.context.live(io).map(|()| false),
            Err(
                error @ RunFailure::Transport {
                    source: PumpSource::Gate,
                    ..
                },
            ) => Err(error),
            Err(
                error @ RunFailure::Wait {
                    key: PumpDeadline::Lower,
                    ..
                },
            ) => Err(error),
            Err(error) => dials.recover(host.context, error, io).map(|()| false),
            result => result,
        };
        match result {
            Err(RunFailure::Wait {
                key: PumpDeadline::Lower,
                bound,
            }) if bound == self.bound => {
                self.end_transport(host.arms, true, false);
                self.retire_lower(host.context);
                Ok(false)
            }
            Err(RunFailure::Transport {
                source: PumpSource::Gate,
                ..
            }) => {
                if self
                    .send
                    .as_ref()
                    .is_some_and(|send| send.sent() && send.sent_at().is_none())
                {
                    return Err(host.context.fail(
                        io,
                        FlowHaltReason::TraceFailed,
                        HaltPoint::Author,
                        "rollback Lower send clock failed",
                    ));
                }
                self.observe_death(host, dials, io)?;
                Ok(false)
            }
            Err(error) => Err(error),
            Ok(done) => Ok(done),
        }
    }

    fn step_live(
        &mut self,
        host: &mut GateRollbackHost<'_>,
        dials: &mut RollbackDials,
        allow_lower: bool,
        io: &mut impl PumpIo,
    ) -> Result<bool, RunFailure> {
        let leave = host.leave;
        dials.prepare(host.context, leave, io)?;
        if !allow_lower && !self.transport_done {
            // Keep the already accepted Leave peer and secondary management
            // dials serviced without touching a held gate packet or author.
            host.context.observe_ready(&mut PumpPorts {
                lifecycle: dials.ports(host.peer),
                gate: None,
                decode: None,
                label: None,
                io: &mut *io,
            })?;
            // The exact interrupt still owns the outgoing slot. Wait only for
            // that socket's writability; neither a queued Frame's trace author
            // nor an execution result may stand between its send and Lower.
            // A short wake also returns to the existing lifecycle/receive
            // owners when a packet is already held. Every earlier bound wins.
            let wake = host.context.last_ns.saturating_add(10_000_000);
            let wake = host
                .context
                .deadlines
                .nearest()
                .map_or(wake, |end| wake.min(end));
            let port = host
                .arms
                .port
                .as_ref()
                .expect("live blocked rollback control");
            io.wait(
                &[crate::flow::clock::WaitInterest {
                    fd: port.fd(),
                    events: nix::poll::PollFlags::POLLOUT,
                }],
                Some(wake),
            )
            .map_err(|error| {
                host.context.fail(
                    io,
                    FlowHaltReason::TraceFailed,
                    HaltPoint::Author,
                    format!("rollback control wait: {error}"),
                )
            })?;
            return Ok(false);
        }
        if self.death.as_ref().is_some_and(|death| !death.settled)
            && !self.settle_death(host, io)?
        {
            return Ok(false);
        }
        if self.pending.is_some() {
            if allow_lower {
                self.send_lower(host, io)?;
            }
            self.record(host, dials, io)?;
            return Ok(false);
        }
        if let Some(received) = self.received.take() {
            self.receive(host, received, false);
            return Ok(false);
        }
        if let Some(event) = self.sweep_fault.take() {
            self.pending = Some(GateRecord {
                received: None,
                event: Some(event),
                started: false,
                stopped: StoppedAuthor::default(),
                claim: None,
            });
            return Ok(false);
        }
        if !self.transport_done {
            let port = host.arms.port.as_mut().expect("live Lower port");
            // Probe before every possible write: an early reply cannot acquire
            // a later send's witness through the pump's transport ordering.
            let receive_error = if port.held().is_none() {
                port.try_receive().err()
            } else {
                None
            };
            if port
                .held()
                .is_some_and(|held| matches!(held.payload, WirePayload::ToolAnswer(_)))
            {
                return Ok(false);
            }
            if let Some(received) =
                port.take_received()
                    .map_err(|failure| RunFailure::Transport {
                        source: PumpSource::Gate,
                        failure,
                    })?
            {
                self.received = Some(received);
                host.context.live(io)?;
                let received = self.received.take().unwrap();
                self.receive(host, received, true);
                if allow_lower {
                    self.send_lower(host, io)?;
                }
                return Ok(false);
            }
            if let Some(failure) = receive_error {
                return Err(RunFailure::Transport {
                    source: PumpSource::Gate,
                    failure,
                });
            }
            if allow_lower {
                self.send_lower(host, io)?;
            }
            if self.transport_done {
                return Ok(false);
            }
            let port = host.arms.port.as_mut().unwrap();
            host.context.pump_once(&mut PumpPorts {
                lifecycle: dials.ports(host.peer),
                gate: Some(port),
                decode: None,
                label: None,
                io,
            })?;
            return Ok(false);
        }
        let arms = &mut *host.arms;
        match arms.child.as_mut().map(|child| child.poll()) {
            state @ (None | Some(ChildState::Reaped(_))) => {
                if matches!(state, Some(ChildState::Reaped(_)))
                    && let Some(proof) = &mut self.closed
                {
                    proof.execution_reaped = true;
                }
                self.retire_lower(host.context);
                self.complete = true;
                Ok(true)
            }
            Some(ChildState::Running) => {
                let wake = host
                    .context
                    .last_ns
                    .saturating_add(10_000_000)
                    .min(leave.deadline_ns());
                let wake = if self.lower_retired {
                    wake
                } else {
                    wake.min(self.bound.deadline_ns())
                };
                io.wait(&[], Some(wake)).map_err(|error| {
                    host.context.fail(
                        io,
                        FlowHaltReason::TraceFailed,
                        HaltPoint::Author,
                        format!("rollback gate reap wait: {error}"),
                    )
                })?;
                Ok(false)
            }
            Some(ChildState::NotWaitable) => Err(host.context.fail(
                io,
                FlowHaltReason::ProtocolFailed,
                HaltPoint::Author,
                "rollback gate lost its actual child reap",
            )),
        }
    }

    fn send_lower(
        &mut self,
        host: &mut GateRollbackHost<'_>,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        if self.transport_done || self.send.as_ref().is_some_and(OrganSend::sent) {
            return Ok(());
        }
        host.context.live(io)?;
        let early = {
            let port = host.arms.port.as_mut().unwrap();
            if port.held().is_none() {
                port.try_receive()
                    .map_err(|failure| RunFailure::Transport {
                        source: PumpSource::Gate,
                        failure,
                    })?;
            }
            // Keep a second content frame in the pump's one receive slot while
            // the preceding record waits. A pre-send close has no Lower witness.
            if port.held().is_some_and(|frame| {
                !matches!(
                    frame.payload,
                    WirePayload::Frame(_) | WirePayload::Fault(_) | WirePayload::ToolAnswer(_)
                )
            }) {
                port.take_received()
                    .map_err(|failure| RunFailure::Transport {
                        source: PumpSource::Gate,
                        failure,
                    })?
            } else {
                None
            }
        };
        if let Some(received) = early {
            self.receive(host, received, false);
            if self.transport_done {
                return Ok(());
            }
        }
        host.context.live(io)?;
        let port = host.arms.port.as_mut().unwrap();
        if self.send.is_none() {
            match port.stage_tracked(self.lower.as_ref().unwrap().clone()) {
                Ok(TrackedStage::Queued(send)) => self.send = Some(send),
                Ok(TrackedStage::Busy(_)) => {
                    return Err(host.context.fail(
                        io,
                        FlowHaltReason::ProtocolFailed,
                        HaltPoint::Author,
                        "rollback Lower has a competing sender",
                    ));
                }
                Err(error) => {
                    return Err(RunFailure::Transport {
                        source: PumpSource::Gate,
                        failure: error.failure,
                    });
                }
            }
        }
        port.try_send_observed(io)
            .map_err(|failure| RunFailure::Transport {
                source: PumpSource::Gate,
                failure,
            })?;
        host.context.live(io)
    }
}
