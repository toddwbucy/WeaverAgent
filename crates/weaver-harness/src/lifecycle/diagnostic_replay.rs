//! The entered Diagnostic owner keeps management, decode and record duties
//! together until the pass stops and its original Leave completes.
use super::*;
use crate::channel::DecodeReply;
use crate::engine::TurnError;
use crate::replay::ReplaySeat;
use weaver_trace::{Kind, Payload, StopReason, Subsystem, TurnClose};
use weaver_types::{TokenAnswer, TokenDirective, TurnKey};

#[derive(Clone, Copy)]
enum Abandon {
    Never,
    AtRest,
    Unsent,
}

struct Stop {
    peer: usize,
    exchange: ExchangeId,
    cause: weaver_types::Cause,
    aborted: bool,
}

pub(super) struct Control {
    pub(super) opened: bool,
    turn: Option<TurnKey>,
    closing: bool,
    interrupted: bool,
    stop: Option<Stop>,
    active: bool,
    cancel_sent: bool,
    decode_settled: bool,
    decode_busy: bool,
    decode_dead: bool,
    // One report can be received while an earlier record owns the writer.
    // Do not consume another until that report is Recorded.
    idle_report: Option<weaver_types::FaultReport>,
    filing_report: bool,
    // The terminal remains owned through the turn-close disposition. It is
    // never converted into a Generation when the ReFeed was interrupted.
    terminal: Option<TokenAnswer>,
    pub(super) failure_deadline: Option<Instant>,
    pub(super) cancel_deadline: Option<Instant>,
    drain_bound: Option<u64>,
    pub(super) drain_deadline: Option<Instant>,
}

impl Control {
    fn new(drain_bound: Option<u64>) -> Self {
        Self {
            opened: false,
            turn: None,
            closing: false,
            interrupted: false,
            stop: None,
            active: false,
            cancel_sent: false,
            decode_settled: false,
            decode_busy: false,
            decode_dead: false,
            idle_report: None,
            filing_report: false,
            terminal: None,
            failure_deadline: None,
            cancel_deadline: None,
            drain_bound,
            drain_deadline: None,
        }
    }

    fn cancel(&mut self) -> Result<(), ChannelFault> {
        self.interrupted = true;
        if self.turn.is_some() && self.cancel_deadline.is_none() {
            self.cancel_deadline = Some(
                Instant::now()
                    .checked_add(Duration::from_millis(
                        weaver_types::DIAGNOSTIC_CANCEL_BOUND_MS,
                    ))
                    .ok_or(ChannelFault::Closed)?,
            );
        }
        Ok(())
    }
}

impl Setup<'_> {
    fn watch_idle_decode(&self) -> bool {
        let control = self.replay.as_ref().unwrap();
        (control.opened || control.decode_settled)
            && !control.decode_busy
            && !control.decode_dead
            && control.idle_report.is_none()
            && !control.filing_report
    }

    fn retain_idle_decode(
        &mut self,
        decode: &DecodeChannel,
        deadline: Option<Instant>,
    ) -> Result<(), ChannelFault> {
        let mut wait_failed = false;
        let answer = decode.recv_managed(&mut |fd, flags| {
            let waited = self.wait(fd, flags, deadline);
            wait_failed |= waited.is_err();
            waited
        });
        if wait_failed {
            return Err(answer.expect_err("a failed wait ends the receive"));
        }
        let control = self.replay.as_mut().unwrap();
        match answer {
            Ok(DecodeReply::Answer(TokenAnswer::Fault(report))) => {
                control.idle_report = Some(report);
            }
            Err(ChannelFault::Closed) => {
                control.decode_dead = true;
                control.interrupted = true;
                if control.turn.is_some() {
                    control.failure_deadline.get_or_insert(
                        shared_deadline(Instant::now()).ok_or(ChannelFault::Closed)?,
                    );
                }
            }
            Err(fault) => return Err(fault),
            Ok(_) => return Err(ChannelFault::Undecodable),
        }
        Ok(())
    }

    pub(super) fn replay_tick(&mut self) -> Result<(), ChannelFault> {
        let Some(control) = self.replay.as_mut() else {
            return Ok(());
        };
        if control
            .drain_deadline
            .is_some_and(|end| Instant::now() >= end)
        {
            control.drain_deadline = None;
            self.leaving.as_mut().expect("drain has a Leave").forced = true;
            control.cancel()?;
        }
        Ok(())
    }

    pub(super) fn replay_directive(
        &mut self,
        peer: usize,
        exchange: ExchangeId,
        directive: LifecycleDirective,
    ) -> Result<(), ChannelFault> {
        let control = self.replay.as_mut().expect("entered replay owner");
        let outcome = match directive {
            LifecycleDirective::Observe => Ok(if self.leaving.is_some() {
                LifecycleAnswer::InTransition {
                    constituents: Vec::new(),
                }
            } else {
                LifecycleAnswer::State {
                    state: if control.turn.is_some() {
                        weaver_types::AgentState::Active
                    } else {
                        weaver_types::AgentState::Idle
                    },
                    load: self.load.clone(),
                    plan: None,
                    constituents: Vec::new(),
                }
            }),
            LifecycleDirective::Stop { cause }
                if self.leaving.is_none() && control.stop.is_none() =>
            {
                if control.turn.is_none() {
                    Ok(LifecycleAnswer::AtRest)
                } else if control.closing {
                    // Spec section 6.2: a Stop between turns answers AtRest
                    // and does not cancel the pass. The close in flight keeps
                    // its own wait, and the answer follows its Recorded.
                    control.stop = Some(Stop {
                        peer,
                        exchange,
                        cause,
                        aborted: false,
                    });
                    return Ok(());
                } else {
                    control.cancel()?;
                    control.stop = Some(Stop {
                        peer,
                        exchange,
                        cause,
                        aborted: true,
                    });
                    return Ok(());
                }
            }
            LifecycleDirective::Leave {
                cause,
                forced,
                rollback,
            } if self.leaving.is_none() => {
                let forced_now = forced || rollback;
                control.interrupted = true;
                let deadline = if control.turn.is_none() {
                    Some(shared_deadline(Instant::now()).ok_or(ChannelFault::Closed)?)
                } else {
                    None
                };
                self.leaving = Some(Leaving {
                    peer,
                    exchange,
                    cause,
                    forced: forced_now,
                    rollback,
                    deadline,
                    joinable: !forced_now,
                    joins: Vec::new(),
                });
                if forced_now {
                    control.cancel()?;
                } else if control.turn.is_some() {
                    control.drain_deadline = control
                        .drain_bound
                        .map(|seconds| {
                            Instant::now()
                                .checked_add(Duration::from_secs(seconds))
                                .ok_or(ChannelFault::Closed)
                        })
                        .transpose()?;
                }
                return Ok(());
            }
            LifecycleDirective::JoinLeave { cause }
                if self
                    .leaving
                    .as_ref()
                    .is_some_and(|leave| leave.joinable && !leave.rollback) =>
            {
                let leave = self.leaving.as_mut().unwrap();
                leave.forced = true;
                leave.joins.push((peer, exchange, cause));
                control.drain_deadline = None;
                control.cancel()?;
                return Ok(());
            }
            _ => Err(LifecycleRefusal::OutOfOrder),
        };
        self.queue(
            peer,
            OrganEnvelope {
                exchange,
                position: Position::Close,
                payload: match outcome {
                    Ok(answer) => weaver_types::Payload::Answer(answer),
                    Err(reason) => weaver_types::Payload::Refusal(reason),
                },
            },
            false,
        )
    }

    fn abandon_replay_wait(&self, abandon: Abandon) -> bool {
        let control = self.replay.as_ref().unwrap();
        match abandon {
            Abandon::Never => false,
            Abandon::AtRest => {
                control.decode_dead || self.leaving.is_some() && control.turn.is_none()
            }
            Abandon::Unsent => {
                control.cancel_deadline.is_some()
                    || self.leaving.is_some() && control.turn.is_none()
            }
        }
    }

    /// Cancel is a retained atomic packet, sent only after the original
    /// ReFeed has crossed whole. A record wait can drive this send too.
    fn replay_wait(
        &mut self,
        decode: &DecodeChannel,
        fd: BorrowedFd<'_>,
        events: PollFlags,
        deadline: Option<Instant>,
        abandon: Abandon,
    ) -> Result<bool, ChannelFault> {
        loop {
            self.checkpoint()?;
            let control = self.replay.as_ref().unwrap();
            if self.abandon_replay_wait(abandon) {
                return Ok(false);
            }
            if control.active && control.cancel_deadline.is_some() && !control.cancel_sent {
                let bytes = serde_json::to_vec(&TokenDirective::Cancel {
                    turn: control.turn.clone().expect("active turn"),
                })
                .map_err(|_| ChannelFault::Undecodable)?;
                if crate::channel::managed::try_send(decode.as_fd(), &bytes)? {
                    self.replay.as_mut().unwrap().cancel_sent = true;
                    continue;
                }
                self.poll(Some((decode.as_fd(), PollFlags::POLLOUT)), deadline, None)?;
                continue;
            }
            let idle = self.watch_idle_decode();
            let (ready, _, unexpected) = self.poll_with_observer(
                Some((fd, events)),
                idle.then(|| decode.as_fd()),
                deadline,
                None,
            )?;
            if unexpected {
                self.retain_idle_decode(decode, deadline)?;
                // Wake the parked caller to file the report, or let a writer
                // finish its earlier ticket. A spurious work wake is harmless:
                // the nonblocking transport retries under the same deadline.
                return Ok(!self.abandon_replay_wait(abandon));
            }
            if ready {
                // A directive heard in this poll takes effect before the work
                // is used, even when work and management were ready together.
                let control = self.replay.as_ref().unwrap();
                if control.active && control.cancel_deadline.is_some() && !control.cancel_sent {
                    continue;
                }
                if self.abandon_replay_wait(abandon) {
                    return Ok(false);
                }
                return Ok(true);
            }
        }
    }

    pub(super) fn run_replay(&mut self, run: &mut Run) -> Result<(), ChannelFault> {
        self.phase = Phase::Replay;
        self.replay = Some(Control::new(run.drain_bound));
        let seat = run.diagnostic.as_mut().expect("Diagnostic binding");
        seat.admitted = true;
        seat.replayed = true;
        let reader_elected = seat.reader_elected;
        let artifact = seat.artifact.clone();
        let spu = run.spu.as_ref().ok_or(ChannelFault::Closed)?;
        let driven = {
            let mut ports = ReplayPorts {
                management: self,
                decode: &spu.decode,
                author: &run.author,
                recorder: &mut run.recorder,
                state: run.state.as_mut(),
                failed: false,
            };
            crate::replay::drive(&mut ports, reader_elected, &artifact)
                .map_err(|_| ChannelFault::Closed)
        };
        if self.replay.as_ref().unwrap().decode_dead {
            if driven.is_err() {
                // Failure still owes the observed death under the original
                // failure bound. The outer failed-run owner kills and reaps.
                self.record_spu_death(run, "channel closed during replay")?;
            } else {
                self.idle_spu_died(run)?;
                self.replay.as_mut().unwrap().failure_deadline = None;
            }
        }
        driven?;
        // Completed and interrupted passes are both single-use. Keep hearing
        // management on every retained peer until the operator leaves.
        self.await_replay_leave(run)?;
        let leave = self.leaving.as_mut().unwrap();
        // Admin Spec 3 (A): an SPU lost while idle makes the later graceful
        // Leave forced, preserving the original request and cause.
        leave.forced |= run.spu.is_none();
        if leave.deadline.is_none() {
            leave.deadline = Some(shared_deadline(Instant::now()).ok_or(ChannelFault::Closed)?);
        }
        self.replay.as_mut().unwrap().drain_deadline = None;
        self.cleanup(run)?;
        self.queue_left()?;
        self.deliver_left()
    }
}

struct ReplayPorts<'a, 'b> {
    management: &'a mut Setup<'b>,
    decode: &'a DecodeChannel,
    author: &'a Author,
    recorder: &'a mut crate::record::Record,
    state: Option<&'a mut crate::state::StateSeam>,
    failed: bool,
}

impl ReplayPorts<'_, '_> {
    fn wait_record(&mut self, event: weaver_trace::Event) -> Result<(), TurnError> {
        self.wait_record_until(event, None)
    }

    fn wait_record_until(
        &mut self,
        event: weaver_trace::Event,
        deadline: Option<Instant>,
    ) -> Result<(), TurnError> {
        let management = &mut self.management;
        let decode = self.decode;
        self.recorder
            .submit_managed_diagnostic(event, &mut |fd| {
                record_nonblocking(fd)?;
                management
                    .replay_wait(decode, fd, PollFlags::POLLOUT, deadline, Abandon::Never)
                    .map(|_| ())
                    .map_err(|_| std::io::Error::from(std::io::ErrorKind::BrokenPipe))
            })
            .map_err(|_| TurnError::ChannelLost)?;
        Ok(())
    }

    fn settle_idle_report(&mut self, deadline: Option<Instant>) -> Result<(), TurnError> {
        let Some(report) = self.management.replay.as_mut().unwrap().idle_report.take() else {
            return Ok(());
        };
        let raw = serde_json::to_string(&report)
            .ok()
            .and_then(|text| weaver_trace::raw_payload(&text))
            .ok_or(TurnError::ChannelLost)?;
        self.management.replay.as_mut().unwrap().filing_report = true;
        let recorded = self.wait_record_until(
            self.author.event(
                Kind::Fault,
                Subsystem::SpuDecoder,
                None,
                Some(Payload::Fault(raw)),
            ),
            deadline,
        );
        self.management.replay.as_mut().unwrap().filing_report = false;
        recorded
    }

    fn hear_idle_decode(&mut self) -> Result<(), TurnError> {
        if self.management.watch_idle_decode() {
            let (_, _, ready) = self
                .management
                .poll_with_observer(None, Some(self.decode.as_fd()), None, Some(Duration::ZERO))
                .map_err(|_| TurnError::ChannelLost)?;
            if ready {
                self.management
                    .retain_idle_decode(self.decode, None)
                    .map_err(|_| TurnError::ChannelLost)?;
            }
        }
        self.settle_idle_report(None)
    }

    fn event(
        &mut self,
        kind: Kind,
        subsystem: Subsystem,
        turn: Option<&TurnKey>,
        payload: Option<Payload>,
    ) -> Result<(), TurnError> {
        self.wait_record(self.author.event(kind, subsystem, turn, payload))?;
        self.settle_idle_report(None)
    }

    fn native(
        &mut self,
        kind: weaver_diagnostic::Kind,
        turn: Option<&TurnKey>,
        payload: weaver_diagnostic::Payload,
    ) -> Result<(), TurnError> {
        let event = self.author.diagnostic_event(kind, turn, Some(payload));
        let management = &mut self.management;
        let decode = self.decode;
        self.recorder
            .submit_native_managed(event, &mut |fd| {
                record_nonblocking(fd)?;
                management
                    .replay_wait(decode, fd, PollFlags::POLLOUT, None, Abandon::Never)
                    .map(|_| ())
                    .map_err(|_| std::io::Error::from(std::io::ErrorKind::BrokenPipe))
            })
            .map_err(|_| TurnError::ChannelLost)?;
        if kind == weaver_diagnostic::Kind::ReplayOpened {
            management.replay.as_mut().unwrap().opened = true;
        }
        self.settle_idle_report(None)
    }

    fn receive(&mut self) -> Result<Option<DecodeReply>, TurnError> {
        let decode = self.decode;
        let mut abandoned = false;
        let mut wait_failed = false;
        let received = decode.recv_managed(&mut |fd, flags| {
            let waited = self
                .management
                .replay_wait(decode, fd, flags, None, Abandon::AtRest);
            wait_failed |= waited.is_err();
            if waited? {
                Ok(())
            } else {
                abandoned = true;
                Err(ChannelFault::Closed)
            }
        });
        if abandoned {
            return Ok(None);
        }
        if !wait_failed && matches!(received, Err(ChannelFault::Closed)) {
            self.management.replay.as_mut().unwrap().decode_dead = true;
        }
        received.map(Some).map_err(|_| TurnError::ChannelLost)
    }

    fn send(&mut self, directive: &TokenDirective) -> Result<bool, TurnError> {
        if !self.checkpoint()? {
            return Ok(false);
        }
        self.management.replay.as_mut().unwrap().decode_busy = true;
        let decode = self.decode;
        let mut abandoned = false;
        let sent = decode.send_replay_managed(directive, &mut |fd, flags, started| {
            let abandon = if started {
                Abandon::Never
            } else {
                Abandon::Unsent
            };
            if self
                .management
                .replay_wait(decode, fd, flags, None, abandon)?
            {
                Ok(())
            } else {
                abandoned = true;
                Err(ChannelFault::Closed)
            }
        });
        if abandoned {
            self.management.replay.as_mut().unwrap().decode_busy = false;
            return Ok(false);
        }
        sent.map_err(|_| TurnError::ChannelLost)?;
        Ok(true)
    }

    fn close(&mut self, turn: &TurnKey, fallback: TurnClose) -> Result<(), TurnError> {
        if self.management.replay.as_ref().unwrap().turn.is_none() {
            return Ok(());
        }
        self.management
            .checkpoint()
            .map_err(|_| TurnError::ChannelLost)?;
        let control = self.management.replay.as_mut().unwrap();
        let close = if self.failed {
            fallback
        } else if control.cancel_deadline.is_some() {
            if let Some(leave) = &self.management.leaving
                && leave.forced
            {
                TurnClose::Stopped {
                    reason: StopReason::Unload,
                    cause: Some(crate::engine::trace_cause(leave.cause)),
                }
            } else {
                TurnClose::Stopped {
                    reason: StopReason::Directive,
                    cause: control
                        .stop
                        .as_ref()
                        .map(|s| crate::engine::trace_cause(s.cause)),
                }
            }
        } else {
            fallback
        };
        control.closing = true;
        self.event(
            Kind::TurnClosed,
            Subsystem::Harness,
            Some(turn),
            Some(Payload::TurnClosed(close)),
        )?;
        let control = self.management.replay.as_mut().unwrap();
        control.turn = None;
        control.closing = false;
        control.cancel_deadline = None;
        control.drain_deadline = None;
        control.decode_settled = false;
        control.terminal = None;
        if self.failed {
            return Err(TurnError::ChannelLost);
        }
        if let Some(stop) = control.stop.take() {
            self.management
                .queue(
                    stop.peer,
                    OrganEnvelope {
                        exchange: stop.exchange,
                        position: Position::Close,
                        payload: weaver_types::Payload::Answer(if stop.aborted {
                            LifecycleAnswer::TurnAborted { turn: turn.clone() }
                        } else {
                            LifecycleAnswer::AtRest
                        }),
                    },
                    false,
                )
                .map_err(|_| TurnError::ChannelLost)?;
        }
        Ok(())
    }

    fn generation(
        &mut self,
        turn: &TurnKey,
        generation: &weaver_types::Generation,
    ) -> Result<(), TurnError> {
        self.event(
            Kind::ModelRequest,
            Subsystem::SpuDecoder,
            Some(turn),
            Some(Payload::ModelRequest(generation.request.clone())),
        )?;
        self.event(
            Kind::ModelOutput,
            Subsystem::SpuDecoder,
            Some(turn),
            Some(Payload::ModelOutput(weaver_trace::ModelOutput {
                emission: generation.emission.clone(),
                finish: match generation.finish {
                    weaver_types::Finish::Completed => weaver_trace::Finish::Completed,
                    weaver_types::Finish::Stopped => weaver_trace::Finish::Stopped,
                    weaver_types::Finish::Length => weaver_trace::Finish::Length,
                },
                resident: generation.resident,
                capacity: generation.capacity,
            })),
        )?;
        self.event(
            Kind::ModelMeasurement,
            Subsystem::SpuDecoder,
            Some(turn),
            Some(Payload::ModelMeasurement(generation.measurement.clone())),
        )?;
        for report in &generation.unrecovered {
            let payload = serde_json::to_string(report)
                .ok()
                .and_then(|text| weaver_trace::raw_payload(&text))
                .ok_or(TurnError::ChannelLost)?;
            self.event(
                Kind::ModelUnrecoveredCall,
                Subsystem::SpuDecoder,
                Some(turn),
                Some(Payload::UnrecoveredCall(payload)),
            )?;
        }
        let message = weaver_traits::Message {
            role: weaver_traits::Role::Assistant,
            content: generation.content.clone(),
        };
        crate::authorship::licensed(&message)
            .map_err(|_| TurnError::Unlicensed { turn: turn.clone() })?;
        let payload = serde_json::to_string(&message)
            .ok()
            .and_then(|text| weaver_trace::raw_payload(&text))
            .ok_or(TurnError::ChannelLost)?;
        self.event(
            Kind::MessageAssistant,
            Subsystem::Harness,
            Some(turn),
            Some(Payload::Message(payload)),
        )
    }

    fn refeed_inner(
        &mut self,
        turn: &TurnKey,
        rendered: String,
        path: Vec<u32>,
    ) -> Result<Option<weaver_types::Generation>, TurnError> {
        let path_len = path.len() as u64;
        if !self.send(&TokenDirective::ReFeed {
            turn: turn.clone(),
            rendered,
            path,
        })? {
            return Ok(None);
        }
        let control = self.management.replay.as_mut().unwrap();
        control.active = true;
        control.cancel_sent = false;
        control.decode_settled = false;
        let mut terminal = None;
        // A refusal that crosses a sent Cancel is still the ReFeed's owed
        // answer. It closes the ReFeed, and the Cancel's AtRest follows.
        let mut refused = None;
        loop {
            let open = terminal.is_none() && refused.is_none();
            match self.receive()?.ok_or(TurnError::ChannelLost)? {
                DecodeReply::Answer(TokenAnswer::Field {
                    position,
                    ranked,
                    realized,
                }) if open => {
                    self.event(
                        Kind::ModelField,
                        Subsystem::SpuDecoder,
                        Some(turn),
                        Some(Payload::ModelField(weaver_trace::ModelField {
                            position,
                            ranked: ranked
                                .into_iter()
                                .map(|candidate| weaver_trace::Candidate {
                                    token: candidate.token,
                                    probability: candidate.probability,
                                })
                                .collect(),
                            realized,
                        })),
                    )?;
                }
                DecodeReply::Answer(TokenAnswer::Column { position, layers }) if open => {
                    let column = weaver_diagnostic::ResidualColumn {
                        position,
                        layers: layers.len() as u32,
                        width: layers.first().map_or(0, |l| l.len()) as u32,
                        values: layers,
                    };
                    self.native(
                        weaver_diagnostic::Kind::ResidualColumn,
                        Some(turn),
                        weaver_diagnostic::Payload::ResidualColumn(column),
                    )?;
                }
                DecodeReply::Answer(TokenAnswer::ReFed(generation)) if open => {
                    self.management.replay.as_mut().unwrap().active = false;
                    terminal = Some(TokenAnswer::ReFed(generation));
                }
                DecodeReply::Answer(TokenAnswer::RefeedInterrupted {
                    turn: received,
                    consumed,
                    resident,
                    capacity,
                }) if open && self.management.replay.as_ref().unwrap().cancel_sent => {
                    if &received != turn
                        || consumed >= path_len
                        || resident > capacity
                        || consumed
                            .checked_add(1)
                            .is_none_or(|minimum| minimum > resident)
                    {
                        return Err(TurnError::ChannelLost);
                    }
                    self.management.replay.as_mut().unwrap().active = false;
                    terminal = Some(TokenAnswer::RefeedInterrupted {
                        turn: received,
                        consumed,
                        resident,
                        capacity,
                    });
                }
                DecodeReply::Answer(TokenAnswer::AtRest)
                    if !open && self.management.replay.as_ref().unwrap().cancel_sent =>
                {
                    break;
                }
                DecodeReply::Refusal(refusal) if open => {
                    self.management.replay.as_mut().unwrap().active = false;
                    if self.management.replay.as_ref().unwrap().cancel_sent {
                        self.refusal(weaver_types::TokenAsk::ReFeed, refusal.clone(), Some(turn))?;
                        refused = Some(refusal);
                        continue;
                    }
                    self.management.replay.as_mut().unwrap().decode_busy = false;
                    self.refusal(weaver_types::TokenAsk::ReFeed, refusal.clone(), Some(turn))?;
                    return Err(TurnError::Refused {
                        turn: turn.clone(),
                        refusal,
                    });
                }
                DecodeReply::Answer(TokenAnswer::Fault(report)) => {
                    // Bound best-effort fault accounting before touching a
                    // possibly blocked sink. Keep an earlier interval intact.
                    // The ReFeed has ended: a Stop landing during the record
                    // must not send a Cancel to it.
                    let control = self.management.replay.as_mut().unwrap();
                    control.active = false;
                    let earlier = control.failure_deadline.is_some();
                    control.failure_deadline.get_or_insert_with(|| {
                        shared_deadline(Instant::now()).expect("representable cleanup bound")
                    });
                    let raw = serde_json::to_string(&report)
                        .ok()
                        .and_then(|text| weaver_trace::raw_payload(&text))
                        .ok_or(TurnError::ChannelLost)?;
                    self.event(
                        Kind::Fault,
                        Subsystem::SpuDecoder,
                        Some(turn),
                        Some(Payload::Fault(raw)),
                    )?;
                    let control = self.management.replay.as_mut().unwrap();
                    if control.cancel_sent {
                        return Err(TurnError::ChannelLost);
                    }
                    // An uninterrupted fault ends the pass, not the run: the
                    // bracket closes stopped and the run stands for Leave. The
                    // bound covered only this fault's record.
                    control.decode_busy = false;
                    if !earlier {
                        control.failure_deadline = None;
                    }
                    return Err(TurnError::Faulted {
                        turn: turn.clone(),
                        report,
                    });
                }
                _ => return Err(TurnError::ChannelLost),
            }
            if terminal.is_some() && !self.management.replay.as_ref().unwrap().cancel_sent {
                break;
            }
        }
        self.management.replay.as_mut().unwrap().decode_settled = true;
        self.management.replay.as_mut().unwrap().decode_busy = false;
        if let Some(refusal) = refused {
            return Err(TurnError::Refused {
                turn: turn.clone(),
                refusal,
            });
        }
        let terminal = terminal.expect("a terminal precedes completion");
        let recorded = match &terminal {
            TokenAnswer::ReFed(generation) => self.generation(turn, generation),
            TokenAnswer::RefeedInterrupted { .. } => Ok(()),
            _ => unreachable!("only a ReFeed terminal reaches settlement"),
        };
        if recorded.is_err()
            || self
                .management
                .replay
                .as_ref()
                .unwrap()
                .cancel_deadline
                .is_some()
        {
            self.management.replay.as_mut().unwrap().terminal = Some(terminal);
            recorded?;
            Ok(None)
        } else if let TokenAnswer::ReFed(generation) = terminal {
            Ok(Some(generation))
        } else {
            Err(TurnError::ChannelLost)
        }
    }

    fn refusal(
        &mut self,
        ask: weaver_types::TokenAsk,
        refusal: weaver_types::TokenRefusal,
        turn: Option<&TurnKey>,
    ) -> Result<(), TurnError> {
        let record = weaver_types::RefusalRecord::Decode {
            asked: ask,
            refusal,
        };
        let raw = serde_json::to_string(&record)
            .ok()
            .and_then(|text| weaver_trace::raw_payload(&text))
            .ok_or(TurnError::ChannelLost)?;
        self.event(
            Kind::Refusal,
            Subsystem::Harness,
            turn,
            Some(Payload::Refusal(raw)),
        )
    }
}

pub(super) fn record_nonblocking(fd: BorrowedFd<'_>) -> std::io::Result<()> {
    use nix::fcntl::{FcntlArg, OFlag, fcntl};
    let flags = OFlag::from_bits_truncate(fcntl(fd, FcntlArg::F_GETFL)?);
    if !flags.contains(OFlag::O_NONBLOCK) {
        fcntl(fd, FcntlArg::F_SETFL(flags | OFlag::O_NONBLOCK))?;
    }
    Ok(())
}

impl ReplaySeat for ReplayPorts<'_, '_> {
    fn checkpoint(&mut self) -> Result<bool, TurnError> {
        if self.failed || self.recorder.failed() {
            return Err(TurnError::ChannelLost);
        }
        self.management
            .checkpoint()
            .map_err(|_| TurnError::ChannelLost)?;
        self.hear_idle_decode()?;
        let control = self.management.replay.as_ref().unwrap();
        if control.decode_dead && control.turn.is_some() {
            return Err(TurnError::ChannelLost);
        }
        Ok(!control.interrupted || control.turn.is_some() && control.cancel_deadline.is_none())
    }

    fn replay(&mut self, bound_ms: u64) -> Option<Vec<crate::state::Recalled>> {
        let decode = self.decode;
        let mut failed = false;
        let state = self.state.take()?;
        let events = state.ask_replay_managed(bound_ms, &mut |fd, deadline| match self
            .management
            .replay_wait(
                decode,
                fd,
                PollFlags::POLLIN,
                Some(deadline),
                Abandon::AtRest,
            ) {
            Ok(ready) => {
                if self.settle_idle_report(Some(deadline)).is_err() {
                    failed = true;
                    false
                } else {
                    ready
                }
            }
            Err(_) => {
                // Only the parked ask's own timeout is its ordinary
                // unanswered outcome. A shorter lifecycle interval or
                // transport failure keeps its failure disposition.
                failed = self.management.live(None).is_err() || Instant::now() < deadline;
                false
            }
        });
        self.state = Some(state);
        self.failed |= failed;
        if !self.checkpoint().ok()? {
            return None;
        }
        let events = events?;
        let identity = |e: &crate::state::Recalled| weaver_trace::RecalledIdentity {
            run: e.run.clone(),
            turn: e.turn.clone(),
            sequence: e.sequence.clone(),
            kind: e.kind.clone(),
        };
        let returned = match events.as_slice() {
            [] => Vec::new(),
            [only] => vec![identity(only)],
            [first, .., last] => vec![identity(first), identity(last)],
        };
        self.event(
            Kind::Recall,
            Subsystem::Harness,
            None,
            Some(Payload::Recall(weaver_trace::RecallAccount {
                ask: weaver_trace::RecallAsk {
                    verb: weaver_trace::RecallVerb::Replay,
                    last_turns: None,
                },
                returned,
                count: Some(events.len() as u64),
            })),
        )
        .ok()?;
        Some(events)
    }

    fn session_name(&self) -> String {
        self.author.session().to_string()
    }

    fn author_replay(
        &mut self,
        kind: weaver_diagnostic::Kind,
        payload: weaver_diagnostic::Payload,
    ) -> Result<(), TurnError> {
        if !self.checkpoint()? {
            return Ok(());
        }
        self.native(kind, None, payload)
    }

    fn replay_turn_started(&mut self, turn: &TurnKey) -> Result<bool, TurnError> {
        if !self.checkpoint()? {
            return Ok(false);
        }
        self.management.replay.as_mut().unwrap().turn = Some(turn.clone());
        self.event(Kind::TurnStarted, Subsystem::Harness, Some(turn), None)?;
        Ok(true)
    }

    fn replay_turn_closed(&mut self, turn: &TurnKey) -> Result<(), TurnError> {
        self.close(turn, TurnClose::Clean)
    }

    fn replay_turn_stopped(&mut self, turn: &TurnKey, reason: StopReason) -> Result<(), TurnError> {
        self.close(
            turn,
            TurnClose::Stopped {
                reason,
                cause: None,
            },
        )
    }

    fn refeed(
        &mut self,
        turn: &TurnKey,
        rendered: String,
        path: Vec<u32>,
    ) -> Result<Option<weaver_types::Generation>, TurnError> {
        let result = self.refeed_inner(turn, rendered, path);
        if matches!(result, Err(TurnError::ChannelLost)) {
            self.failed = true;
            let control = self.management.replay.as_mut().unwrap();
            control.failure_deadline.get_or_insert_with(|| {
                shared_deadline(Instant::now()).expect("representable cleanup bound")
            });
        }
        result
    }

    fn flush(&mut self, keep: u64) -> Result<Option<(u64, u64)>, TurnError> {
        if !self.send(&TokenDirective::Flush { keep })? {
            return Ok(None);
        }
        let Some(reply) = self.receive()? else {
            return Ok(None);
        };
        self.management.replay.as_mut().unwrap().decode_busy = false;
        match reply {
            DecodeReply::Answer(TokenAnswer::Flushed {
                resident_before,
                resident_after,
            }) => {
                self.event(
                    Kind::Flush,
                    Subsystem::Harness,
                    None,
                    Some(Payload::Flush(weaver_trace::FlushCounts {
                        resident_before,
                        resident_after,
                    })),
                )?;
                Ok(Some((resident_before, resident_after)))
            }
            DecodeReply::Refusal(refusal) => {
                self.refusal(weaver_types::TokenAsk::Flush { keep }, refusal, None)?;
                Ok(None)
            }
            _ => Err(TurnError::ChannelLost),
        }
    }

    fn elide(&mut self, from: u64, to: u64) -> Result<Option<(u64, u64)>, TurnError> {
        if !self.send(&TokenDirective::Elide { from, to })? {
            return Ok(None);
        }
        let Some(reply) = self.receive()? else {
            return Ok(None);
        };
        self.management.replay.as_mut().unwrap().decode_busy = false;
        match reply {
            DecodeReply::Answer(TokenAnswer::Elided {
                resident_before,
                resident_after,
            }) => {
                self.event(
                    Kind::Elision,
                    Subsystem::Harness,
                    None,
                    Some(Payload::Elision(weaver_trace::ElisionSpan {
                        from,
                        to,
                        resident_before,
                        resident_after,
                    })),
                )?;
                Ok(Some((resident_before, resident_after)))
            }
            DecodeReply::Refusal(refusal) => {
                self.refusal(weaver_types::TokenAsk::Elide { from, to }, refusal, None)?;
                Ok(None)
            }
            _ => Err(TurnError::ChannelLost),
        }
    }
}

#[cfg(test)]
#[path = "diagnostic_replay_tests.rs"]
mod tests;
