//! Drain the standing gate without opening a turn or replacing its owner.
//! GateQuiesced is only one witness: every earlier frame still owes its actual
//! refusal record and complete world send, and a cancelled input check drains.
use super::*;
use crate::flow::context::RecordingContext;
use crate::flow::input::InputState;
use crate::flow::pump_transport::OrganPort;
use weaver_trace::Sequence;
use weaver_types::{
    ExchangeId, LifecycleAnswer, LifecycleAsk, LifecycleDirective, LifecycleRefusal, RefusalRecord,
    TurnFrame,
};

#[path = "context_worker_graceful_quiesce_lower.rs"]
mod lower;

pub(in crate::flow::context::worker_startup) struct Quiesce {
    port: crate::flow::pump_transport::PortIdentity,
    request: OrganEnvelope,
    send: Option<OrganSend>,
    reply: Option<ReceivedOrgan>,
    prepared: bool,
    input_done: bool,
    input_responded: bool,
    pending: Option<GateRecord>,
    completed: Vec<GateRecord>,
    complete: bool,
    gate_failure: Option<crate::flow::pump_transport::Failure>,
}

struct GateRecord {
    // An active input keeps its original receipt in FoundationInput. All
    // other originals, including malformed protocol replies, stay here.
    received: Option<ReceivedOrgan>,
    active_input: bool,
    event: Option<AuthoredEvent>,
    started: bool,
    stopped: StoppedAuthor,
    recorded: Option<Sequence>,
    close: Option<OrganEnvelope>,
    send: Option<OrganSend>,
}

impl Quiesce {
    pub(super) fn settled(&self) -> bool {
        self.complete
    }
}

impl FoundationInput<'_> {
    /// A retained EOF belongs to this exact gate until the forced tail reaps
    /// it. Polling it again must not preempt the secondary management dial.
    pub(in crate::flow::context::worker_startup::input) fn retained_quiesce_gate_end(
        &self,
    ) -> bool {
        self.serving
            .graceful
            .as_ref()
            .and_then(|leave| leave.quiesce.as_ref())
            .is_some_and(|work| {
                work.gate_failure.is_some() && work.port.matches(&self.serving.owner.gate)
            })
    }

    /// Failed-run cleanup consults the actual ticket, including a send that
    /// crossed just before its post-send clock or input accounting failed.
    pub(in crate::flow::context::worker_startup::input) fn quiesce_input_send(
        &self,
    ) -> Option<&OrganSend> {
        let work = self.serving.graceful.as_ref()?.quiesce.as_ref()?;
        work.pending
            .iter()
            .chain(&work.completed)
            .find(|record| record.active_input)
            .and_then(|record| record.send.as_ref())
    }

    pub(in crate::flow::context::worker_startup::input) fn quiesce_send_clock_failed(
        &self,
    ) -> bool {
        let Some(work) = self
            .serving
            .graceful
            .as_ref()
            .and_then(|leave| leave.quiesce.as_ref())
        else {
            return false;
        };
        work.send
            .iter()
            .chain(
                work.pending
                    .iter()
                    .chain(&work.completed)
                    .filter_map(|record| record.send.as_ref()),
            )
            .any(|send| send.sent() && send.sent_at().is_none())
    }

    /// Elect Quiesce only after a real turn has finished its world send, or
    /// before any turn began. This grants neither wind-down nor Lower/save.
    pub fn begin_graceful_quiesce(&mut self) -> bool {
        self.begin_graceful_quiesce_with_io(&mut MonotonicIo)
    }

    fn begin_graceful_quiesce_with_io(&mut self, io: &mut impl PumpIo) -> bool {
        if self.progress() != FoundationInputProgress::LeavePending
            || self
                .serving
                .graceful
                .as_ref()
                .is_none_or(|leave| leave.forced || leave.quiesce.is_some())
            || self.serving.owner.context.open_turn.is_some()
            || self.serving.owner.context.input_turn.is_some()
            || self.serving.owner.context.marks.is_some()
            || (self.turn.is_some()
                && !matches!(
                    self.stage,
                    Stage::Completed | Stage::Refused | Stage::Aborted
                ))
        {
            return false;
        }
        if let Err(failure) = self.serving.owner.context.live(io) {
            if !self.retain_graceful_expiry(&failure) {
                self.failure = Some(failure);
                self.stage = Stage::Stopped;
            }
            return false;
        }
        let Some(ordinal) = self.serving.gate_outgoing.checked_add(1) else {
            self.failure = Some(self.serving.owner.context.fail(
                io,
                FlowHaltReason::ProtocolFailed,
                HaltPoint::Barrier,
                "Quiesce exchange exhausted",
            ));
            self.stage = Stage::Stopped;
            return false;
        };
        self.serving.gate_outgoing = ordinal;
        self.serving.graceful.as_mut().unwrap().quiesce = Some(Box::new(Quiesce {
            port: self.serving.owner.gate.port_identity(),
            request: OrganEnvelope {
                exchange: ExchangeId {
                    opener: Opener::Harness,
                    ordinal,
                },
                position: Position::Open,
                payload: Payload::Directive(LifecycleDirective::Quiesce),
            },
            send: None,
            reply: None,
            prepared: false,
            input_done: self.serving.owner.context.input.snapshot().is_none(),
            input_responded: false,
            pending: None,
            completed: Vec::new(),
            complete: false,
            gate_failure: None,
        }));
        true
    }

    pub(in crate::flow::context::worker_startup::input) fn quiesce_progress(
        &self,
    ) -> Option<FoundationInputProgress> {
        let leave = self.serving.graceful.as_ref()?;
        let work = leave.quiesce.as_ref()?;
        Some(if leave.forced {
            FoundationInputProgress::LeavePending
        } else if work.complete {
            FoundationInputProgress::Quiesced
        } else {
            FoundationInputProgress::Pending
        })
    }

    pub(in crate::flow::context::worker_startup::input) fn retain_quiesce_gate_end(
        &mut self,
        failure: &RunFailure,
    ) -> bool {
        let RunFailure::Transport {
            source: PumpSource::Gate,
            failure,
        } = failure
        else {
            return false;
        };
        if failure.fault != crate::failure::ChannelFault::Closed
            || self.serving.owner.context.failure().is_some()
        {
            return false;
        }
        let Some(leave) = &mut self.serving.graceful else {
            return false;
        };
        let Some(work) = &mut leave.quiesce else {
            return false;
        };
        if work.complete
            && leave
                .wind_down
                .as_ref()
                .is_none_or(|wind_down| !wind_down.active())
        {
            return false;
        }
        // The forced tail must retain the actual failed port and every owed
        // record/send. This is neither a GateQuiesced nor a Lower proof.
        work.gate_failure.get_or_insert_with(|| failure.clone());
        leave.forced = true;
        self.serving
            .owner
            .context
            .complete_wait(PumpDeadline::GracefulDrain);
        self.serving
            .owner
            .context
            .complete_wait(PumpDeadline::WindDown);
        true
    }

    pub(in crate::flow::context::worker_startup::input) fn advance_quiesce(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        if self.advance_report(io)? {
            return Ok(());
        }
        let serving = &mut self.serving;
        let owner = &mut serving.owner;
        let work = serving.graceful.as_mut().unwrap().quiesce.as_mut().unwrap();
        let context = &mut *owner.context;
        context.live(io)?;
        let mut ports = PumpPorts {
            lifecycle: LifecyclePorts {
                listener: Some(&mut owner.listener),
                residency: Some(owner.resident.residency),
                ..Default::default()
            },
            gate: Some(&mut owner.gate),
            decode: Some(owner.decode),
            label: None,
            io,
        };
        if !work.prepared {
            // A prior refusal may have crossed just before Leave was heard.
            // Never answer it twice or call an already-sent datagram withdrawn.
            if let Some(send) = &self.send {
                let original = self
                    .received
                    .as_ref()
                    .expect("input close retains original");
                checked_send(
                    context,
                    ports.gate.as_deref().unwrap(),
                    send,
                    &original.envelope().exchange,
                    ports.io,
                )?;
                if send.sent() {
                    if context
                        .input
                        .snapshot()
                        .is_some_and(|input| !input.response_sent)
                    {
                        context.input_response_sent(ports.io)?;
                    }
                    work.input_responded = true;
                } else if !send.withdrawn() {
                    match ports
                        .gate
                        .as_deref_mut()
                        .unwrap()
                        .withdraw_pending(&original.envelope().exchange)
                        .map_err(gate_failure)?
                    {
                        Some(close) => self.close = Some(close),
                        None => {
                            context.pump_once(&mut ports)?;
                            return Ok(());
                        }
                    }
                }
            }
            if let Some(input) = context.input.snapshot() {
                if input.state == InputState::Consumed {
                    return Err(protocol(
                        context,
                        ports.io,
                        "Quiesce still owns an unfinished turn input",
                    ));
                }
                if !input.response_sent && !input.cancelled {
                    context.cancel_input(ports.io)?;
                }
            } else if self.stage == Stage::Waiting
                && let Some(received) = self.received.take()
            {
                serving.queued_inputs.push_front(received);
            }
            work.prepared = true;
        }
        if work.send.is_none() {
            match ports
                .gate
                .as_deref_mut()
                .unwrap()
                .stage_tracked(work.request.clone())
            {
                Ok(TrackedStage::Queued(send)) => work.send = Some(send),
                Ok(TrackedStage::Busy(original)) => {
                    debug_assert_eq!(original, work.request);
                }
                Err(error) => return Err(gate_failure(error.failure)),
            }
        }
        let Some(send) = &work.send else {
            context.pump_once(&mut ports)?;
            return Ok(());
        };
        checked_send(
            context,
            ports.gate.as_deref().unwrap(),
            send,
            &work.request.exchange,
            ports.io,
        )?;
        if !send.sent() {
            context.pump_once(&mut ports)?;
            return Ok(());
        }
        if let Some(record) = &mut work.pending {
            if record.advance(
                context,
                &mut ports,
                self.received.as_ref(),
                &mut serving.gate_exchanges,
            )? {
                if record.active_input {
                    context.input_response_sent(ports.io)?;
                    work.input_responded = true;
                }
                work.completed.push(work.pending.take().unwrap());
            }
            return Ok(());
        }
        if !work.input_done {
            // Cancellation revokes acceptance but never discards a started
            // classifier exchange, its original offered event or measurement.
            let progress = context.advance_input(&mut ports)?;
            if work.input_responded && progress == InputProgress::Finished {
                context.finish_input(ports.io)?;
                work.input_done = true;
            } else if !work.input_responded
                && context
                    .input
                    .snapshot()
                    .is_some_and(|input| input.offered_sequence.is_some())
                && context.input_author.is_none()
                && !context.flow.has_pending_record()
                && context.barrier.pending().is_none()
                && context.waiting_content.is_none()
                && context.completed_author.is_none()
            {
                work.pending = Some(GateRecord::new(None, true));
            } else if progress != InputProgress::Progress {
                context.pump_once(&mut ports)?;
            }
            return Ok(());
        }
        if let Some(received) = serving.queued_inputs.pop_front() {
            work.pending = Some(GateRecord::new(Some(received), false));
            return Ok(());
        }
        if let Some(received) = ports
            .gate
            .as_deref_mut()
            .unwrap()
            .take_received()
            .map_err(gate_failure)?
        {
            if matches!(
                received.envelope().payload,
                Payload::Frame(_) | Payload::Fault(_)
            ) {
                work.pending = Some(GateRecord::new(Some(received), false));
                return Ok(());
            }
            work.reply = Some(received);
            let reply = work.reply.as_ref().unwrap();
            if !reply.belongs_to(ports.gate.as_deref().unwrap())
                || reply.envelope().exchange != work.request.exchange
                || reply.envelope().position != Position::Close
                || !matches!(
                    reply.envelope().payload,
                    Payload::Answer(LifecycleAnswer::GateQuiesced)
                )
            {
                return Err(protocol(
                    context,
                    ports.io,
                    "Quiesce received a foreign or invalid acknowledgement",
                ));
            }
            // The gate's ordering promise and this owner's empty refusal/input
            // accounts jointly establish drain completion, before wind-down.
            context.complete_wait(PumpDeadline::GracefulDrain);
            work.complete = true;
            return Ok(());
        }
        context.pump_once(&mut ports)?;
        Ok(())
    }
}

impl GateRecord {
    fn new(received: Option<ReceivedOrgan>, active_input: bool) -> Self {
        Self {
            received,
            active_input,
            event: None,
            started: false,
            stopped: StoppedAuthor::default(),
            recorded: None,
            close: None,
            send: None,
        }
    }

    fn prepare_event(
        &mut self,
        context: &mut RecordingContext,
        io: &mut impl PumpIo,
        input: Option<&ReceivedOrgan>,
        seen: &mut std::collections::HashSet<u64>,
    ) -> Result<(), RunFailure> {
        let Some(received) = self
            .received
            .as_ref()
            .or(input.filter(|_| self.active_input))
        else {
            return Err(protocol(
                context,
                io,
                "Quiesce refusal lost its original input",
            ));
        };
        let envelope = received.envelope();
        if envelope.exchange.opener != Opener::Gate || envelope.position != Position::Open {
            return Err(protocol(
                context,
                io,
                "Quiesce input has a foreign exchange position",
            ));
        }
        if self.event.is_none() && !self.started && self.recorded.is_none() {
            let (kind, subsystem, payload) = match &envelope.payload {
                Payload::Frame(_) => {
                    if self.active_input {
                        if !seen.contains(&envelope.exchange.ordinal) {
                            return Err(protocol(
                                context,
                                io,
                                "Quiesce active input has no admitted gate exchange",
                            ));
                        }
                    } else if !seen.insert(envelope.exchange.ordinal) {
                        return Err(protocol(
                            context,
                            io,
                            "Quiesce input repeated an earlier exchange",
                        ));
                    }
                    self.close = Some(OrganEnvelope {
                        exchange: envelope.exchange.clone(),
                        position: Position::Close,
                        payload: Payload::Frame(TurnFrame::carry(
                            br#"{"kind":"refused","reason":"the agent is unloading"}"#,
                        )),
                    });
                    let refusal = RefusalRecord::Lifecycle {
                        asked: LifecycleAsk::Leave,
                        refusal: LifecycleRefusal::Unloading,
                    };
                    (
                        serving::Kind::Refusal,
                        Subsystem::Harness,
                        serving::Payload::Refusal(
                            serde_json::value::to_raw_value(&refusal)
                                .expect("typed lifecycle refusal"),
                        ),
                    )
                }
                Payload::Fault(fault) if !self.active_input => (
                    serving::Kind::Fault,
                    Subsystem::Gate,
                    serving::Payload::Fault(
                        serde_json::value::to_raw_value(fault).expect("typed gate fault"),
                    ),
                ),
                _ => {
                    return Err(protocol(context, io, "Quiesce received no frame or fault"));
                }
            };
            self.event = Some(AuthoredEvent {
                kind,
                subsystem,
                turn: None,
                source: AuthoredSource::Runtime,
                causal_parent: None,
                payload: Some(payload),
            });
        }
        Ok(())
    }

    fn advance<I: PumpIo>(
        &mut self,
        context: &mut RecordingContext,
        ports: &mut PumpPorts<'_, I>,
        input: Option<&ReceivedOrgan>,
        seen: &mut std::collections::HashSet<u64>,
    ) -> Result<bool, RunFailure> {
        self.stopped.check()?;
        let Some(received) = self
            .received
            .as_ref()
            .or(input.filter(|_| self.active_input))
        else {
            return Err(protocol(
                context,
                ports.io,
                "Quiesce refusal lost its original input",
            ));
        };
        let envelope = received.envelope();
        if !received.belongs_to(ports.gate.as_deref().unwrap())
            || envelope.exchange.opener != Opener::Gate
            || envelope.position != Position::Open
        {
            return Err(protocol(
                context,
                ports.io,
                "Quiesce input came from another gate or exchange position",
            ));
        }
        let exchange = envelope.exchange.clone();
        self.prepare_event(context, ports.io, input, seen)?;
        if self.recorded.is_none() {
            let result = if self.started {
                context.resume_author_and_wait(ports)
            } else {
                self.started = true;
                context.author_and_wait(ports, self.event.take().unwrap())
            };
            self.recorded = Some(
                result
                    .map_err(|failure| self.stopped.retain_started(failure, &mut self.started))?,
            );
        }
        if let Some(close) = self.close.take() {
            match ports.gate.as_deref_mut().unwrap().stage_tracked(close) {
                Ok(TrackedStage::Queued(send)) => self.send = Some(send),
                Ok(TrackedStage::Busy(close)) => self.close = Some(close),
                Err(error) => {
                    self.close = Some(error.input);
                    return Err(gate_failure(error.failure));
                }
            }
        }
        if let Some(send) = &self.send {
            checked_send(
                context,
                ports.gate.as_deref().unwrap(),
                send,
                &exchange,
                ports.io,
            )?;
            if send.sent() {
                return Ok(true);
            }
        } else if self.close.is_none() {
            return Ok(true); // A gate fault has no world response.
        }
        context.pump_once(ports)?;
        Ok(false)
    }
}

fn checked_send(
    context: &mut RecordingContext,
    gate: &OrganPort,
    send: &OrganSend,
    exchange: &ExchangeId,
    io: &mut impl PumpIo,
) -> Result<(), RunFailure> {
    if !send.belongs_to(gate) || !send.matches_exchange(exchange) {
        return Err(protocol(
            context,
            io,
            "Quiesce send proof belongs to another port or exchange",
        ));
    }
    if send.sent() && send.sent_at().is_none() {
        return Err(context.fail(
            io,
            FlowHaltReason::TraceFailed,
            HaltPoint::Author,
            "Quiesce send has no clock observation",
        ));
    }
    Ok(())
}

fn protocol(context: &mut RecordingContext, io: &mut impl PumpIo, detail: &str) -> RunFailure {
    context.fail(
        io,
        FlowHaltReason::ProtocolFailed,
        HaltPoint::Barrier,
        detail,
    )
}
fn gate_failure(failure: crate::flow::pump_transport::Failure) -> RunFailure {
    RunFailure::Transport {
        source: PumpSource::Gate,
        failure,
    }
}
