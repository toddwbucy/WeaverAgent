//! The same gate account after a component halt; no ordinary author is resumed.
use super::*;
use crate::flow::context::failed_enter_driver::FailedRunCleanup;

impl RollbackGate {
    /// Build custody only. Reporting still precedes every gate syscall. An
    /// existing driver is moved instead, retaining its earlier Lower bound.
    pub(in crate::flow::context::worker_startup) fn failed_new(
        gate: GateRollbackEvidence<'_>,
        arms: &StartupGate,
        leave: Option<AbsoluteBound>,
        deadline: Option<u64>,
        now: u64,
        allow_lower: bool,
    ) -> Self {
        let began = leave.map_or(now, AbsoluteBound::began_ns);
        let end = deadline
            .map(|end| end.min(began.saturating_add(weaver_types::LOWER_BOUND_MS * 1_000_000)));
        let bound = end.and_then(|end| AbsoluteBound::from_absolute(began, end).ok());
        let raised = arms.port.as_ref().is_some_and(|port| {
            gate.gate_enter == Some(gate.enter)
                && gate.send.as_ref().is_some_and(|send| {
                    send.sent()
                        && send.belongs_to(port)
                        && send.matches_exchange(&gate.raise.exchange)
                        && send.sent_at().is_some_and(|at| {
                            at >= gate.enter.began_ns() && !gate.enter.expired(at)
                        })
                })
        });
        let lower = (allow_lower && bound.is_some() && raised)
            .then(|| gate.raise.exchange.ordinal.checked_add(1))
            .flatten()
            .map(|ordinal| OrganEnvelope {
                exchange: ExchangeId {
                    opener: Opener::Harness,
                    ordinal,
                },
                position: Position::Open,
                payload: WirePayload::Directive(LifecycleDirective::Lower),
            });
        Self {
            // With no remaining cap, keep the original Enter interval as
            // evidence. Only nonblocking disposal can use that carrier.
            bound: bound.unwrap_or(gate.enter),
            lower,
            send: None,
            raise_reply: None,
            lower_reply: None,
            pending: None,
            received: None,
            retired: None,
            transport_done: false,
            lower_retired: true,
            sweep_done: false,
            containment: None,
            sweep_fault: None,
            death: None,
            refusal: None,
            last_recorded: None,
            complete: false,
        }
    }

    fn failed_retire(&mut self, arms: &mut StartupGate) {
        if let Some(port) = arms.port.take() {
            // The retired port itself retains any additional held frame and
            // malformed/partial bytes. Never overwrite the extracted slot.
            assert!(self.retired.is_none(), "one original gate transport");
            self.retired = Some(port.retire_failed());
        }
        self.transport_done = true;
    }

    fn failed_end(&mut self, arms: &mut StartupGate, kill: bool, died: bool) {
        self.failed_retire(arms);
        // This is the same one-shot Leave sweep as the healthy owner. It
        // delivers kill/fallback without waiting on tools inside actual Leave.
        self.sweep(arms, died);
        if kill && let Some(child) = &mut arms.child {
            child.stop_at_cap();
        }
    }

    pub(in crate::flow::context::worker_startup) fn failed_dispose(
        &mut self,
        arms: &mut StartupGate,
    ) {
        self.failed_end(arms, true, false);
    }

    /// Retain received evidence without new ordinary fault/refusal authors.
    /// The first halt stays primary; available later facts become secondary.
    fn failed_receive(
        &mut self,
        gate: GateRollbackEvidence<'_>,
        arms: &mut StartupGate,
        tail: &mut FailedRunCleanup,
    ) {
        let reply = self.received.as_ref().expect("exact extracted gate packet");
        let frame = reply.envelope();
        let port = arms.port.as_ref().expect("live received gate port");
        let prior_raise = self.raise_reply.is_none()
            && gate.reply.is_none()
            && reply.belongs_to(port)
            && frame.position == Position::Close
            && matches!(
                frame.payload,
                WirePayload::Answer(LifecycleAnswer::GateReady) | WirePayload::Refusal(_)
            )
            && gate.send.as_ref().is_some_and(|send| {
                send.sent()
                    && send.belongs_to(port)
                    && send.matches_exchange(&frame.exchange)
                    && send.sent_at().is_some_and(|at| {
                        gate.gate_enter
                            .is_some_and(|enter| at >= enter.began_ns() && !enter.expired(at))
                    })
            });
        if prior_raise {
            self.raise_reply = self.received.take();
            return;
        }
        if matches!(frame.payload, WirePayload::Fault(_))
            || (frame.exchange.opener == Opener::Gate
                && frame.position == Position::Open
                && matches!(frame.payload, WirePayload::Frame(_)))
        {
            tail.secondary(format!(
                "gate packet during failed rollback: {:?}",
                frame.payload
            ));
            // The first extra packet keeps the existing extracted slot. A
            // stopped owner cannot author its ordinary response, so retire
            // rather than accumulating a stream of unaccounted packets.
            self.failed_end(arms, true, false);
            return;
        }
        let exact = !self.bound.expired(tail.context.last_ns)
            && reply.belongs_to(port)
            && frame.position == Position::Close
            && self
                .lower
                .as_ref()
                .is_some_and(|lower| lower.exchange == frame.exchange)
            && self.send.as_ref().is_some_and(|send| {
                send.sent()
                    && send.belongs_to(port)
                    && send.matches_exchange(&frame.exchange)
                    && send
                        .sent_at()
                        .is_some_and(|at| at >= self.bound.began_ns() && !self.bound.expired(at))
            });
        let stopped = exact
            && matches!(
                frame.payload,
                WirePayload::Answer(LifecycleAnswer::GateStopped)
            );
        if !stopped {
            tail.secondary(format!(
                "failed rollback Lower lacks its exact timely GateStopped: {:?}",
                frame
            ));
        }
        if exact {
            assert!(self.lower_reply.is_none(), "one consumed Lower completion");
            self.lower_reply = self.received.take();
        }
        self.failed_end(arms, !stopped, false);
    }

    pub(in crate::flow::context::worker_startup) fn failed_step(
        &mut self,
        gate: GateRollbackEvidence<'_>,
        arms: &mut StartupGate,
        sweep_reported: &mut bool,
        tail: &mut FailedRunCleanup,
        io: &mut impl PumpIo,
    ) -> Result<bool, &'static str> {
        if !tail.clock(io) {
            return Err("failed gate original cap or clock");
        }
        if self.bound.expired(tail.context.last_ns)
            || (!self.transport_done && self.lower.is_none())
        {
            self.failed_end(arms, true, false);
        }
        if !self.transport_done {
            let port = arms.port.as_mut().expect("live failed Lower port");
            let receive_error = if self.received.is_none() && port.held().is_none() {
                port.try_receive().err()
            } else {
                None
            };
            if self.received.is_none() {
                match port.take_received() {
                    Ok(reply) => self.received = reply,
                    Err(error) => {
                        tail.secondary(format!("failed rollback gate receive: {error:?}"));
                        self.failed_end(arms, true, true);
                    }
                }
            }
            // Preserve a consumed packet before any post-receive clock failure.
            if !tail.clock(io) {
                return Err("failed gate receive clock or cap");
            }
            if self.received.is_some() && !self.transport_done {
                self.failed_receive(gate, arms, tail);
            }
            if let Some(error) = receive_error {
                tail.secondary(format!("failed rollback gate receive: {error:?}"));
                self.failed_end(arms, true, true);
            }
        }
        if !self.transport_done {
            if !tail.clock(io) {
                return Err("failed gate send clock or cap");
            }
            if self.bound.expired(tail.context.last_ns) {
                self.failed_end(arms, true, false);
            } else {
                let port = arms.port.as_mut().unwrap();
                if self.send.is_none() {
                    match port.stage_tracked(self.lower.as_ref().unwrap().clone()) {
                        Ok(TrackedStage::Queued(send)) => self.send = Some(send),
                        Ok(TrackedStage::Busy(_)) => {
                            tail.secondary("failed rollback Lower has another queued original");
                            self.failed_end(arms, true, false);
                        }
                        Err(error) => {
                            tail.secondary(format!(
                                "failed rollback Lower stage: {:?}",
                                error.failure
                            ));
                            self.failed_end(arms, true, true);
                        }
                    }
                }
                if !self.transport_done
                    && let Err(error) = arms.port.as_mut().unwrap().try_send_observed(io)
                {
                    tail.secondary(format!("failed rollback Lower send: {error:?}"));
                    self.failed_end(arms, true, true);
                }
            }
        }
        if !tail.clock(io) {
            return Err("failed gate post-work clock or cap");
        }
        // A timely GateStopped retires transport, not the original deadline
        // for its actual child. Kill at Lower expiry even after that reply.
        if self.bound.expired(tail.context.last_ns) {
            self.failed_end(arms, true, false);
        }
        if self.sweep_done && !*sweep_reported {
            *sweep_reported = true;
            if self.containment == Some(Containment::Undelivered) {
                tail.secondary("failed rollback gate tools sweep could not deliver containment");
            }
        }
        if !self.transport_done {
            return Ok(false);
        }
        match arms.child.as_mut().map(|child| child.poll()) {
            None | Some(ChildState::Reaped(_)) => {
                self.complete = true;
                Ok(true)
            }
            Some(ChildState::Running) => Ok(false),
            Some(ChildState::NotWaitable) => {
                tail.secondary("failed rollback gate lost its actual child reap");
                Err("failed gate child custody unavailable")
            }
        }
    }

    pub(in crate::flow::context::worker_startup) fn failed_wait(
        &self,
        arms: &StartupGate,
        tail: &mut FailedRunCleanup,
        io: &mut impl PumpIo,
    ) -> bool {
        if !tail.clock(io) {
            return false;
        }
        let mut wake = tail
            .context
            .last_ns
            .saturating_add(10_000_000)
            .min(tail.deadline().unwrap());
        if !self.bound.expired(tail.context.last_ns) {
            wake = wake.min(self.bound.deadline_ns());
        }
        if let Err(error) = tail.context.barrier.drain_capacity_wake() {
            tail.secondary(format!("failed rollback gate capacity wake: {error:?}"));
            return false;
        }
        let mut interests = vec![WaitInterest {
            fd: tail.context.barrier.capacity_fd(),
            events: PollFlags::POLLIN,
        }];
        if !self.transport_done
            && let Some(port) = arms.port.as_ref()
        {
            interests.push(WaitInterest {
                fd: port.fd(),
                events: port.interest().events(),
            });
        }
        if let Err(error) = io.wait(&interests, Some(wake)) {
            tail.secondary(format!("failed rollback gate wait: {error}"));
            return false;
        }
        tail.clock(io)
    }
}
