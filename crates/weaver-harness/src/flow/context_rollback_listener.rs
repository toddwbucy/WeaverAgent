//! Secondary dials belong to the accepted Leave, never to boot cancellation.
use super::*;
use crate::flow::pump::{LifecycleListener, PumpSource};
use weaver_types::state_flow::capture::CaptureEnvelope;
use weaver_types::state_flow::enter::ObservationEnvelope;
use weaver_types::{LifecycleAnswer, LifecycleRefusal, Payload};

#[path = "context_rollback_resting.rs"]
mod resting;
pub(in crate::flow::context) use resting::{RestingCommand, RestingRequest, RetiredRestingCommand};

pub(in crate::flow::context) struct RollbackDials {
    pub(super) listener: LifecycleListener,
    pub(super) active: Option<PendingDial>,
    sealing: bool,
    graceful: Option<GracefulDials>,
    refusing: bool,
}

struct GracefulOutcome {
    forced: bool,
    forced_by: Option<weaver_types::Cause>,
    frozen: bool,
}
impl GracefulOutcome {
    fn join(&mut self, cause: weaver_types::Cause) {
        if !self.frozen && !self.forced {
            self.forced = true;
            self.forced_by = Some(cause);
        }
    }
}
struct GracefulDials {
    outcome: GracefulOutcome,
    joins: Vec<JoinedLeave>,
}

pub(in crate::flow::context) struct JoinedLeave {
    peer: LifecyclePeer,
    message: LifecycleMessage,
    answer: Option<CaptureEnvelope>,
    send: Option<OrganSend>,
    complete: bool,
    unavailable: bool,
    detail: Option<String>,
}
impl JoinedLeave {
    pub(in crate::flow::context) fn new(
        peer: LifecyclePeer,
        message: LifecycleMessage,
        unavailable: bool,
        detail: Option<String>,
    ) -> Self {
        Self {
            peer,
            message,
            answer: None,
            send: None,
            complete: false,
            unavailable,
            detail,
        }
    }
}

pub(super) struct PendingDial {
    pub(super) peer: LifecyclePeer,
    pub(super) read: Option<AbsoluteBound>,
    pub(super) message: Option<LifecycleMessage>,
    pub(super) answer: Option<OrganEnvelope>,
    observation: Option<ObservationEnvelope>,
    save: Option<crate::flow::pump::ReceivedCapture>,
    save_answer: Option<CaptureEnvelope>,
    serving_opener: bool,
    pub(super) send: Option<OrganSend>,
}

impl RollbackDials {
    pub(in crate::flow::context) fn new(listener: LifecycleListener) -> Self {
        Self {
            listener,
            active: None,
            sealing: false,
            graceful: None,
            refusing: false,
        }
    }

    pub(in crate::flow::context) fn adopt_graceful(
        &mut self,
        forced: bool,
        forced_by: Option<weaver_types::Cause>,
        joins: Vec<JoinedLeave>,
    ) {
        assert!(self.graceful.is_none() && !self.sealing);
        self.graceful = Some(GracefulDials {
            outcome: GracefulOutcome {
                forced,
                forced_by,
                frozen: false,
            },
            joins,
        });
    }

    pub(in crate::flow::context) fn outcome(&self) -> (bool, Option<weaver_types::Cause>) {
        self.graceful.as_ref().map_or((true, None), |held| {
            (held.outcome.forced, held.outcome.forced_by)
        })
    }

    pub(in crate::flow::context) fn freeze_outcome(&mut self) {
        if let Some(held) = &mut self.graceful {
            held.outcome.frozen = true;
        }
    }

    #[cfg(test)]
    pub(in crate::flow::context) fn test_join_count(&self) -> usize {
        self.graceful.as_ref().map_or(0, |held| held.joins.len())
    }

    fn drop_dial(&mut self, context: &mut RecordingContext, detail: &str) {
        if let Some(dial) = self.active.take() {
            if let Some(message) = dial.message.as_ref().or_else(|| dial.peer.held()) {
                eprintln!(
                    "weaver-harness: answer to exchange {} not delivered/completed ({detail}); accepted Leave continues",
                    message.envelope.exchange.ordinal
                );
            }
            if let Some(bound) = dial.read {
                // Never retire another owner's deadline on a malformed handoff.
                if context.deadlines.get(PumpDeadline::AdminRead) == Some(bound) {
                    context.complete_wait(PumpDeadline::AdminRead);
                }
            }
        }
    }

    fn prepare_dial(
        &mut self,
        context: &mut RecordingContext,
        leave: Option<AbsoluteBound>,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        context.live(io)?;
        // The listener may already hold the next connection while the prior
        // dial finishes. Retire that dial before adopting the held connection,
        // so the caller cannot sleep with neither an accept nor a read interest.
        if self
            .active
            .as_ref()
            .is_some_and(|dial| dial.peer.failure().is_some())
        {
            self.drop_dial(context, "dialer transport closed or failed");
        } else if self
            .active
            .as_ref()
            .is_some_and(|dial| dial.send.as_ref().is_some_and(OrganSend::sent))
        {
            self.active = None;
        }
        if self.active.is_none() && self.listener.held().is_some() {
            let Some(accepted) = self.listener.accepted_ns() else {
                return Err(context.fail(
                    io,
                    FlowHaltReason::ProtocolFailed,
                    HaltPoint::Barrier,
                    "rollback listener has an unclocked accepted connection",
                ));
            };
            if accepted > context.last_ns {
                return Err(context.fail(
                    io,
                    FlowHaltReason::TraceFailed,
                    HaltPoint::Author,
                    "rollback dial clock precedes its actual accept observation",
                ));
            }
            let end = accepted
                .saturating_add(crate::state::ANSWER_BOUND_MS * 1_000_000)
                .min(leave.map_or(u64::MAX, |bound| bound.deadline_ns()));
            let bound = AbsoluteBound::from_absolute(accepted, end).map_err(|error| {
                context.fail(
                    io,
                    FlowHaltReason::ProtocolFailed,
                    HaltPoint::Barrier,
                    error.to_string(),
                )
            })?;
            let (peer, observed) = self
                .listener
                .take_observed_connection()
                .expect("validated held listener observation remains owned");
            debug_assert_eq!(observed, accepted);
            self.active = Some(PendingDial {
                peer,
                read: Some(bound),
                message: None,
                answer: None,
                observation: None,
                save: None,
                save_answer: None,
                serving_opener: false,
                send: None,
            });
            context.arm(PumpDeadline::AdminRead, bound, io)?;
            // Acceptance may have preceded a delayed reduction. Do not grant a
            // late directive a fresh two seconds when the owner finally runs.
            context.live(io)?;
        }
        Ok(())
    }

    pub(in crate::flow::context) fn prepare(
        &mut self,
        context: &mut RecordingContext,
        leave: AbsoluteBound,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        self.prepare_dial(context, Some(leave), io)?;
        let Some(dial) = &mut self.active else {
            return Ok(());
        };
        if dial.message.is_none() && dial.peer.held().is_some() {
            dial.message =
                dial.peer
                    .take_completion()
                    .map_err(|failure| RunFailure::Transport {
                        source: PumpSource::AdminDial,
                        failure,
                    })?;
            if let Some(bound) = dial.read.take()
                && context.complete_wait(PumpDeadline::AdminRead) != Some(bound)
            {
                return Err(context.fail(
                    io,
                    FlowHaltReason::ProtocolFailed,
                    HaltPoint::Barrier,
                    "rollback dial lost its original read deadline",
                ));
            }
            let message = dial
                .message
                .as_ref()
                .expect("held directive was consumed once");
            if message.envelope.exchange.opener != Opener::Admin
                || message.envelope.position != Position::Open
                || ((message.sink.is_some() || message.state.is_some())
                    && !matches!(
                        message.envelope.payload,
                        Payload::Directive(LifecycleDirective::Enter { .. })
                    ))
            {
                self.drop_dial(context, "invalid lifecycle opener or handles");
                return Ok(());
            }
            if let Payload::Directive(LifecycleDirective::JoinLeave { cause }) =
                message.envelope.payload
                && !self.refusing
                && let Some(held) = &mut self.graceful
            {
                held.outcome.join(cause);
                let dial = self.active.take().unwrap();
                held.joins.push(JoinedLeave::new(
                    dial.peer,
                    dial.message.unwrap(),
                    false,
                    None,
                ));
                return Ok(());
            }
            let payload = match &message.envelope.payload {
                Payload::Directive(LifecycleDirective::Observe) if !self.sealing => {
                    Payload::Answer(LifecycleAnswer::InTransition {
                        constituents: Vec::new(),
                    })
                }
                // Only a retained graceful Leave permits the join above.
                // Another Leave never replaces any original holder.
                Payload::Directive(_) => Payload::Refusal(LifecycleRefusal::OutOfOrder),
                _ => {
                    self.drop_dial(context, "lifecycle dial did not open a directive");
                    return Ok(());
                }
            };
            dial.answer = Some(OrganEnvelope {
                exchange: message.envelope.exchange.clone(),
                position: Position::Close,
                payload,
            });
        }
        if dial.send.is_none()
            && let Some(answer) = &dial.answer
        {
            match dial.peer.stage_tracked(answer.clone()) {
                Ok(TrackedStage::Queued(send)) => dial.send = Some(send),
                Ok(TrackedStage::Busy(_)) => {
                    return Err(context.fail(
                        io,
                        FlowHaltReason::ProtocolFailed,
                        HaltPoint::Barrier,
                        "fresh rollback dial already has an outgoing answer",
                    ));
                }
                Err(error) => {
                    return Err(RunFailure::Transport {
                        source: PumpSource::AdminDial,
                        failure: error.failure,
                    });
                }
            }
        }
        Ok(())
    }

    /// Seal before organ release. Unlink once, exhaust the actual backlog and
    /// retain graceful joins and answer other directives OutOfOrder within
    /// the existing Leave.
    pub(in crate::flow::context) fn seal_step(
        &mut self,
        context: &mut RecordingContext,
        leave: AbsoluteBound,
        io: &mut impl PumpIo,
    ) -> Result<bool, RunFailure> {
        if !self.sealing {
            self.freeze_outcome();
            self.listener.unlink();
            self.sealing = true;
        }
        let result = (|| {
            context.live(io)?;
            let attempted = self.active.is_none();
            let accepted = if attempted {
                self.listener
                    .accept_observed(context.last_ns)
                    .map_err(|failure| RunFailure::Transport {
                        source: PumpSource::Listener,
                        failure,
                    })?
            } else {
                false
            };
            self.prepare(context, leave, io)?;
            if attempted && self.active.is_none() && self.listener.held().is_none() && !accepted {
                return Ok(true);
            }
            if self.active.is_none() {
                return Ok(false);
            }
            context.pump_once(&mut PumpPorts {
                lifecycle: LifecyclePorts {
                    dial: self.active.as_mut().map(|dial| &mut dial.peer),
                    ..Default::default()
                },
                gate: None,
                decode: None,
                label: None,
                io,
            })?;
            Ok(false)
        })();
        match result {
            Ok(done) => Ok(done),
            Err(error) => {
                self.recover(context, error, io)?;
                Ok(false)
            }
        }
    }

    /// Only after the original terminal record and drain, send the same fixed
    /// outcome on each original joining exchange. No new lifecycle clock runs.
    pub(in crate::flow::context) fn finish_joins(
        &mut self,
        answer: Option<&CaptureEnvelope>,
        now: u64,
        leave: AbsoluteBound,
        io: &mut impl PumpIo,
    ) -> Result<bool, String> {
        let Some(held) = &mut self.graceful else {
            return Ok(true);
        };
        if !held.outcome.frozen || !self.sealing || leave.expired(now) {
            return Err("graceful joins require the frozen live Leave tail".into());
        }
        let Some(answer) = answer else {
            return Err("graceful joins require the actual final capture outcome".into());
        };
        for join in &mut held.joins {
            if join.complete {
                continue;
            }
            if join.answer.is_none() {
                let mut response = answer.clone();
                response.exchange = join.message.envelope.exchange.clone();
                if let Err(failure) = join.peer.enable_capture() {
                    join.detail = Some(format!("joining Leave response: {failure:?}"));
                    join.unavailable = true;
                    join.complete = true;
                    continue;
                }
                join.answer = Some(response);
            }
            if join.send.is_none() {
                match join
                    .peer
                    .stage_capture(join.answer.as_ref().unwrap().clone())
                {
                    Ok(TrackedStage::Queued(send)) => join.send = Some(send),
                    Ok(TrackedStage::Busy(_)) => {
                        return Err("joining Leave has a competing response".into());
                    }
                    Err(error) => {
                        join.detail = Some(format!("joining Leave response: {:?}", error.failure));
                        join.unavailable = true;
                        join.complete = true;
                        continue;
                    }
                }
            }
            if let Err(failure) = join.peer.try_send_observed(io) {
                join.detail = Some(format!("joining Leave response: {failure:?}"));
                if join
                    .send
                    .as_ref()
                    .is_some_and(|send| send.sent() && send.sent_at().is_none())
                {
                    return Err("joining Leave response has no send clock witness".into());
                }
                join.unavailable = true;
                join.complete = true;
                continue;
            }
            let send = join.send.as_ref().unwrap();
            if send.sent() {
                if !join.peer.owns_send(send)
                    || !send.matches_exchange(&join.message.envelope.exchange)
                    || !send
                        .sent_at()
                        .is_some_and(|at| at >= leave.began_ns() && !leave.expired(at))
                {
                    return Err("joining Leave response lacks its timely whole-send witness".into());
                }
                join.complete = true;
            } else {
                io.wait(
                    &[crate::flow::clock::WaitInterest {
                        fd: join.peer.fd(),
                        events: nix::poll::PollFlags::POLLOUT
                            | nix::poll::PollFlags::POLLHUP
                            | nix::poll::PollFlags::POLLERR
                            | nix::poll::PollFlags::POLLNVAL,
                    }],
                    Some(leave.deadline_ns()),
                )
                .map_err(|error| format!("joining Leave wait: {error}"))?;
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub(in crate::flow::context) fn ports<'a>(
        &'a mut self,
        peer: &'a mut LifecyclePeer,
    ) -> LifecyclePorts<'a> {
        LifecyclePorts {
            listener: Some(&mut self.listener),
            leave_peer: peer.failure().is_none().then_some(peer),
            dial: self.active.as_mut().map(|dial| &mut dial.peer),
            ..Default::default()
        }
    }

    /// Capture owns a departed original peer until its cleanup reduces that
    /// departure. The ordinary rollback projection excludes failed transports.
    pub(in crate::flow::context) fn capture_ports<'a>(
        &'a mut self,
        peer: &'a mut LifecyclePeer,
    ) -> LifecyclePorts<'a> {
        LifecyclePorts {
            listener: Some(&mut self.listener),
            leave_peer: Some(peer),
            dial: self.active.as_mut().map(|dial| &mut dial.peer),
            ..Default::default()
        }
    }

    /// Only a secondary peer or its exact silent-read expiry is recoverable.
    /// Listener/state/trace/clock failure and the original Leave expiry survive.
    pub(in crate::flow::context) fn recover(
        &mut self,
        context: &mut RecordingContext,
        error: RunFailure,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        match &error {
            RunFailure::Wait {
                key: PumpDeadline::AdminRead,
                bound,
            } if self.active.as_ref().and_then(|dial| dial.read) == Some(*bound) => {
                self.drop_dial(context, "silent dial exceeded its original read allowance");
                context.live(io)
            }
            RunFailure::Transport {
                source: PumpSource::AdminDial,
                ..
            } if self.active.is_some() => {
                if self
                    .active
                    .as_ref()
                    .and_then(|dial| dial.send.as_ref())
                    .is_some_and(|send| send.sent() && send.sent_at().is_none())
                {
                    return Err(context.fail(
                        io,
                        FlowHaltReason::TraceFailed,
                        HaltPoint::Author,
                        format!("rollback dial send clock observation failed: {error:?}"),
                    ));
                }
                self.drop_dial(context, "dialer transport closed or failed");
                context.live(io)
            }
            _ => Err(error),
        }
    }
}

pub(in crate::flow::context) struct RetiredRollbackDials {
    pub(super) listener: crate::flow::pump::RetiredLifecycleListener,
    pub(super) active: Option<RetiredRollbackDial>,
    _graceful: Option<GracefulOutcome>,
    _joins: Vec<RetiredJoinedLeave>,
}
struct RetiredJoinedLeave {
    _peer: crate::flow::pump::RetiredLifecyclePeer,
    _message: crate::flow::pump::RetiredLifecycleMessage,
    _answer: Option<CaptureEnvelope>,
    _send: Option<OrganSend>,
    _complete: bool,
    _unavailable: bool,
    _detail: Option<String>,
}
pub(super) struct RetiredRollbackDial {
    pub(super) peer: crate::flow::pump::RetiredLifecyclePeer,
    pub(super) read: Option<AbsoluteBound>,
    pub(super) message: Option<crate::flow::pump::RetiredLifecycleMessage>,
    pub(super) answer: Option<OrganEnvelope>,
    observation: Option<ObservationEnvelope>,
    _save: Option<crate::flow::pump::RetiredReceivedCapture>,
    _save_answer: Option<CaptureEnvelope>,
    pub(super) send: Option<OrganSend>,
}
impl RollbackDials {
    pub(in crate::flow::context) fn retire_failed(self) -> RetiredRollbackDials {
        let (outcome, joins) = self.graceful.map_or((None, Vec::new()), |held| {
            (
                Some(held.outcome),
                held.joins
                    .into_iter()
                    .map(|join| RetiredJoinedLeave {
                        _peer: join.peer.retire_failed(),
                        _message: join.message.retire_failed(),
                        _answer: join.answer,
                        _send: join.send,
                        _complete: join.complete,
                        _unavailable: join.unavailable,
                        _detail: join.detail,
                    })
                    .collect(),
            )
        });
        RetiredRollbackDials {
            _graceful: outcome,
            _joins: joins,
            listener: self.listener.retire_failed(),
            active: self.active.map(|dial| RetiredRollbackDial {
                peer: dial.peer.retire_failed(),
                read: dial.read,
                message: dial.message.map(LifecycleMessage::retire_failed),
                answer: dial.answer,
                observation: dial.observation,
                _save: dial
                    .save
                    .map(crate::flow::pump::ReceivedCapture::retire_failed),
                _save_answer: dial.save_answer,
                send: dial.send,
            }),
        }
    }
}

#[cfg(test)]
mod graceful_outcome_tests {
    use super::*;

    #[test]
    fn graceful_join_preserves_first_force_and_the_frozen_s7_outcome() {
        // Spec 6: first join before S7 forces; later joins and every S7 join
        // receive the existing outcome without replacing its provenance.
        for (forced, first, frozen, expected, expected_first) in [
            (false, None, false, true, Some(81)),
            (true, None, false, true, None),
            (true, Some(74), false, true, Some(74)),
            (false, None, true, false, None),
            (true, None, true, true, None),
            (true, Some(74), true, true, Some(74)),
        ] {
            let mut outcome = GracefulOutcome {
                forced,
                forced_by: first.map(|uid| weaver_types::Cause { uid }),
                frozen,
            };
            outcome.join(weaver_types::Cause { uid: 81 });
            assert_eq!(outcome.forced, expected);
            assert_eq!(outcome.forced_by.map(|cause| cause.uid), expected_first);
            outcome.join(weaver_types::Cause { uid: 82 });
            assert_eq!(outcome.forced, expected);
            assert_eq!(outcome.forced_by.map(|cause| cause.uid), expected_first);
        }
    }
}
