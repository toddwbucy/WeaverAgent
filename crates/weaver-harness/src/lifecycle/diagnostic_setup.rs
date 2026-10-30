//! Diagnostic setup retains management and its first rollback interval while
//! the existing typed channel owners wait for each elected arm.
use super::*;
use crate::channel::diagnostic::DiagnosticRequest;
use nix::poll::{PollFd, PollFlags, poll};
use std::collections::VecDeque;
use std::os::fd::{AsFd, BorrowedFd};
use std::time::{Duration, Instant};

#[cfg(test)]
#[path = "diagnostic_setup_tests.rs"]
mod tests;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Phase {
    Setup,
    Admit,
    Cleanup,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Admission {
    Unsent,
    Awaiting,
    Admitted,
    Refused,
    Lost,
}

struct Reply {
    bytes: Vec<u8>,
    enter: bool,
}

struct Peer {
    channel: Option<OrganChannel>,
    replies: VecDeque<Reply>,
    first_request_deadline: Option<Instant>,
    first_request_pending: bool,
}

pub(super) struct Leaving {
    peer: usize,
    exchange: ExchangeId,
    pub(super) cause: weaver_types::Cause,
    pub(super) forced: bool,
    rollback: bool,
    pub(super) deadline: Instant,
    joins: Vec<(usize, ExchangeId, weaver_types::Cause)>,
}

pub(super) struct Setup<'a> {
    listener: &'a CoordinationListener,
    peers: Vec<Peer>,
    exchange: ExchangeId,
    enter_selected: bool,
    enter_done: bool,
    left: Option<LifecycleAnswer>,
    pub(super) ready: bool,
    pub(super) phase: Phase,
    pub(super) leaving: Option<Leaving>,
    admission: Admission,
    pub(super) load: Option<Box<weaver_types::LoadFacts>>,
}

impl<'a> Setup<'a> {
    pub(super) fn new(
        listener: &'a CoordinationListener,
        connection: &OrganChannel,
        exchange: ExchangeId,
    ) -> Result<Self, ChannelFault> {
        Ok(Self {
            listener,
            peers: vec![Peer {
                channel: Some(connection.try_clone()?),
                replies: VecDeque::new(),
                first_request_deadline: None,
                first_request_pending: false,
            }],
            exchange,
            enter_selected: false,
            enter_done: false,
            left: None,
            ready: false,
            phase: Phase::Setup,
            leaving: None,
            admission: Admission::Unsent,
            load: None,
        })
    }

    fn retire(&mut self, peer: usize) {
        self.peers[peer].channel = None;
        self.peers[peer].replies.clear();
        if peer == 0 {
            self.enter_done = true;
        }
    }

    fn queue(
        &mut self,
        peer: usize,
        envelope: OrganEnvelope,
        enter: bool,
    ) -> Result<(), ChannelFault> {
        let Some(channel) = &self.peers[peer].channel else {
            return Ok(());
        };
        let reply = Reply {
            bytes: channel.encode(&envelope)?,
            enter,
        };
        if enter {
            // In particular, a same-peer rollback cannot overtake its Enter.
            self.peers[peer].replies.push_front(reply);
        } else {
            self.peers[peer].replies.push_back(reply);
        }
        Ok(())
    }

    pub(super) fn enter_outcome(
        &mut self,
        outcome: Result<LifecycleAnswer, LifecycleRefusal>,
    ) -> Result<(), ChannelFault> {
        if self.enter_selected {
            return Ok(());
        }
        self.enter_selected = true;
        self.ready = matches!(outcome, Ok(LifecycleAnswer::Ready));
        self.queue(
            0,
            OrganEnvelope {
                exchange: self.exchange.clone(),
                position: Position::Close,
                payload: match outcome {
                    Ok(answer) => weaver_types::Payload::Answer(answer),
                    Err(reason) => weaver_types::Payload::Refusal(reason),
                },
            },
            true,
        )
    }

    fn hear(&mut self, peer: usize) -> Result<(), ChannelFault> {
        self.peers[peer].first_request_deadline = None;
        self.peers[peer].first_request_pending = false;
        let channel = self.peers[peer].channel.as_ref().unwrap();
        let incoming = if channel.is_diagnostic_wire() {
            channel.recv_diagnostic()
        } else {
            channel
                .recv_with_descriptor()
                .map(|(request, sink, state)| {
                    if request.exchange.opener != Opener::Admin
                        || request.position != Position::Open
                        || !matches!(request.payload, weaver_types::Payload::Directive(_))
                        || sink.is_some()
                        || state.is_some()
                    {
                        DiagnosticRequest::Refused {
                            exchange: request.exchange,
                            reason: LifecycleRefusal::Malformed,
                        }
                    } else {
                        DiagnosticRequest::Ordinary(request)
                    }
                })
        };
        let incoming = match incoming {
            Ok(value) => value,
            Err(_) => {
                self.retire(peer);
                return Ok(());
            }
        };
        let (exchange, result) = match incoming {
            DiagnosticRequest::Refused { exchange, reason } => (exchange, Err(reason)),
            DiagnosticRequest::Ordinary(envelope) => {
                let weaver_types::Payload::Directive(directive) = envelope.payload else {
                    unreachable!("validated management directive")
                };
                let result = match directive {
                    LifecycleDirective::Observe => Ok(if !self.ready || self.leaving.is_some() {
                        LifecycleAnswer::InTransition {
                            constituents: Vec::new(),
                        }
                    } else {
                        LifecycleAnswer::State {
                            plan: None,
                            state: weaver_types::AgentState::Idle,
                            load: self.load.clone(),
                            constituents: Vec::new(),
                        }
                    }),
                    LifecycleDirective::Stop { .. } if self.ready && self.leaving.is_none() => {
                        Ok(LifecycleAnswer::AtRest)
                    }
                    LifecycleDirective::Leave {
                        cause,
                        forced,
                        rollback,
                    } if self.leaving.is_none() && (self.ready || forced && rollback) => {
                        let began = Instant::now();
                        let deadline = shared_deadline(began).ok_or(ChannelFault::Closed)?;
                        self.leaving = Some(Leaving {
                            peer,
                            exchange: envelope.exchange,
                            cause,
                            forced: forced || rollback,
                            rollback,
                            deadline,
                            joins: Vec::new(),
                        });
                        if !self.ready {
                            self.enter_outcome(Err(LifecycleRefusal::NoResidency))?;
                        }
                        return Ok(());
                    }
                    LifecycleDirective::JoinLeave { cause }
                        if self.ready
                            && self.leaving.as_ref().is_some_and(|leave| !leave.rollback) =>
                    {
                        if let Some(answer) = &self.left {
                            return self.queue(
                                peer,
                                OrganEnvelope {
                                    exchange: envelope.exchange,
                                    position: Position::Close,
                                    payload: weaver_types::Payload::Answer(answer.clone()),
                                },
                                false,
                            );
                        }
                        let leave = self.leaving.as_mut().unwrap();
                        leave.forced = true;
                        leave.joins.push((peer, envelope.exchange, cause));
                        return Ok(());
                    }
                    _ => Err(LifecycleRefusal::OutOfOrder),
                };
                (envelope.exchange, result)
            }
        };
        self.queue(
            peer,
            OrganEnvelope {
                exchange,
                position: Position::Close,
                payload: match result {
                    Ok(answer) => weaver_types::Payload::Answer(answer),
                    Err(reason) => weaver_types::Payload::Refusal(reason),
                },
            },
            false,
        )
    }

    pub(super) fn deadline(&self, phase: Option<Instant>) -> Option<Instant> {
        match (phase, self.leaving.as_ref().map(|leave| leave.deadline)) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    fn live(&self, deadline: Option<Instant>) -> Result<(), ChannelFault> {
        if self
            .deadline(deadline)
            .is_some_and(|end| Instant::now() >= end)
        {
            Err(ChannelFault::Closed)
        } else {
            Ok(())
        }
    }

    /// Returns readiness of the extra work channel and whether management
    /// progressed. Each peer keeps its own reply queue and cannot hold the
    /// listener or another peer's rollback hostage to its receive pressure.
    fn poll(
        &mut self,
        extra: Option<(BorrowedFd<'_>, PollFlags)>,
        deadline: Option<Instant>,
        maximum: Option<Duration>,
    ) -> Result<(bool, bool), ChannelFault> {
        self.live(deadline)?;
        let deadline = self.deadline(deadline);
        let first_request = self
            .peers
            .iter()
            .filter(|peer| peer.channel.is_some())
            .filter_map(|peer| peer.first_request_deadline)
            .min();
        // A silent dial heard during Leave has the existing answer bound.
        // Its expiry retires that peer, never the operation or its deadline.
        let wake = match (deadline, first_request) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        let remaining = wake.map(|end| end.saturating_duration_since(Instant::now()));
        let duration = match (remaining, maximum) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        let timeout = duration.map(|duration| {
            duration
                .as_nanos()
                .div_ceil(1_000_000)
                .min(u128::from(u16::MAX)) as u16
        });
        let mut fds = vec![PollFd::new(self.listener.as_fd(), PollFlags::POLLIN)];
        let mut indices = vec![None];
        for (index, peer) in self.peers.iter().enumerate() {
            if let Some(channel) = &peer.channel {
                let write = if peer.replies.is_empty() {
                    PollFlags::empty()
                } else {
                    PollFlags::POLLOUT
                };
                fds.push(PollFd::new(channel.as_fd(), PollFlags::POLLIN | write));
                indices.push(Some(index));
            }
        }
        let extra_index = extra.map(|(fd, events)| {
            fds.push(PollFd::new(fd, events));
            fds.len() - 1
        });
        match poll(&mut fds, timeout) {
            Ok(_) => (),
            Err(nix::errno::Errno::EINTR) => return Ok((false, true)),
            Err(_) => return Err(ChannelFault::Closed),
        }
        let ready: Vec<_> = fds
            .iter()
            .map(|fd| fd.revents().unwrap_or(PollFlags::empty()))
            .collect();
        drop(fds);
        self.live(deadline)?;
        let mut progress = false;
        if !ready[0].is_empty() {
            match self.listener.try_accept_root() {
                Ok(Some(channel)) => {
                    self.peers.push(Peer {
                        channel: Some(channel),
                        replies: VecDeque::new(),
                        first_request_deadline: Some(
                            Instant::now() + Duration::from_millis(crate::state::ANSWER_BOUND_MS),
                        ),
                        first_request_pending: true,
                    });
                    progress = true;
                }
                Ok(None) | Err(ChannelFault::WrongPeer { .. }) => (),
                Err(fault) => return Err(fault),
            }
        }
        for (slot, peer) in indices.into_iter().enumerate().skip(1) {
            let peer = peer.unwrap();
            if self.peers[peer]
                .first_request_deadline
                .is_some_and(|end| Instant::now() >= end)
            {
                self.retire(peer);
                progress = true;
                continue;
            }
            if ready[slot].intersects(PollFlags::POLLIN | PollFlags::POLLHUP | PollFlags::POLLERR) {
                self.hear(peer)?;
                progress = true;
            }
            self.live(deadline)?;
            if ready[slot].contains(PollFlags::POLLOUT)
                && let Some(channel) = &self.peers[peer].channel
                && let Some(reply) = self.peers[peer].replies.front()
            {
                match crate::channel::managed::try_send(channel.as_fd(), &reply.bytes) {
                    Ok(true) => {
                        let reply = self.peers[peer].replies.pop_front().unwrap();
                        if reply.enter {
                            self.enter_done = true;
                        }
                        progress = true;
                    }
                    Ok(false) => (),
                    Err(_) => self.retire(peer),
                }
            }
        }
        self.live(deadline)?;
        Ok((
            extra_index.is_some_and(|index| !ready[index].is_empty()),
            progress,
        ))
    }

    pub(super) fn checkpoint(&mut self) -> Result<(), ChannelFault> {
        while self.poll(None, None, Some(Duration::ZERO))?.1 {}
        if self.leaving.is_some() && self.phase == Phase::Setup {
            Err(ChannelFault::Closed)
        } else {
            Ok(())
        }
    }

    pub(super) fn wait(
        &mut self,
        fd: BorrowedFd<'_>,
        events: PollFlags,
        deadline: Option<Instant>,
    ) -> Result<(), ChannelFault> {
        loop {
            if self.leaving.is_some() && self.phase == Phase::Setup {
                return Err(ChannelFault::Closed);
            }
            let (ready, _) = self.poll(Some((fd, events)), deadline, None)?;
            if self.leaving.is_some() && self.phase == Phase::Setup {
                return Err(ChannelFault::Closed);
            }
            if ready {
                return Ok(());
            }
        }
    }

    pub(super) fn admit(
        &mut self,
        channel: &OrganChannel,
        ordinal: u64,
        instruction: weaver_types::SpuInstruction,
    ) -> Result<(), LifecycleRefusal> {
        let exchange = ExchangeId {
            opener: Opener::Harness,
            ordinal,
        };
        channel
            .send_managed(
                &OrganEnvelope {
                    exchange: exchange.clone(),
                    position: Position::Open,
                    payload: weaver_types::Payload::Directive(LifecycleDirective::Admit {
                        instruction,
                    }),
                },
                &mut |fd, events| self.wait(fd, events, None),
            )
            .map_err(|_| LifecycleRefusal::NoResidency)?;
        self.admission = Admission::Awaiting;
        // A fully sent Admit has an owed terminal. Rollback stops later
        // work but must settle this exchange before deciding whether Release
        // is permitted. The wait uses the original rollback interval.
        self.phase = Phase::Admit;
        let answer = channel.recv_managed(&mut |fd, events| self.wait(fd, events, None));
        self.phase = Phase::Setup;
        self.admission = Admission::Lost;
        let answer = answer.map_err(|_| LifecycleRefusal::NoResidency)?;
        if answer.exchange != exchange || answer.position != Position::Close {
            return Err(LifecycleRefusal::Malformed);
        }
        match answer.payload {
            weaver_types::Payload::Answer(LifecycleAnswer::Admitted) => {
                self.admission = Admission::Admitted;
                Ok(())
            }
            weaver_types::Payload::Refusal(reason) => {
                self.admission = Admission::Refused;
                Err(LifecycleRefusal::OrganRefused {
                    organ: weaver_types::RefusingOrgan::Spu,
                    reason: Box::new(reason),
                })
            }
            _ => Err(LifecycleRefusal::Malformed),
        }
    }

    pub(super) fn open(
        &mut self,
        decode: &DecodeChannel,
        session: SessionId,
        identity: Vec<weaver_traits::Message>,
        column_ask: bool,
    ) -> Result<(), LifecycleRefusal> {
        decode
            .send_managed(
                &TokenDirective::Open {
                    session: session.clone(),
                    messages: identity,
                    column_ask,
                },
                &mut |fd, events| self.wait(fd, events, None),
            )
            .map_err(|_| LifecycleRefusal::NoResidency)?;
        let reply = decode.recv_managed(&mut |fd, events| self.wait(fd, events, None));
        judge_open_reply(reply, &session, &mut std::io::stderr())
    }

    fn replies_done(&self) -> bool {
        self.enter_done
            && self.peers.iter().all(|peer| {
                peer.replies.is_empty() && (peer.channel.is_none() || !peer.first_request_pending)
            })
    }

    fn await_disposition(&mut self, run: &mut Run) -> Result<bool, ChannelFault> {
        loop {
            if run.recorder.failed() {
                return Err(ChannelFault::Closed);
            }
            if self.leaving.is_some() {
                self.cleanup(run)?;
                self.queue_left()?;
                while !self.replies_done() {
                    self.poll(None, None, None)?;
                }
                self.live(None)?;
                return Ok(true);
            }
            if self.ready && self.replies_done() {
                return Ok(false);
            }
            self.poll(None, None, None)?;
        }
    }

    fn release(&mut self, channel: &OrganChannel, ordinal: u64) -> Result<(), ChannelFault> {
        let exchange = ExchangeId {
            opener: Opener::Harness,
            ordinal,
        };
        channel.send_managed(
            &OrganEnvelope {
                exchange: exchange.clone(),
                position: Position::Open,
                payload: weaver_types::Payload::Directive(LifecycleDirective::Release),
            },
            &mut |fd, events| self.wait(fd, events, None),
        )?;
        let answer = channel.recv_managed(&mut |fd, events| self.wait(fd, events, None))?;
        if answer.exchange == exchange
            && answer.position == Position::Close
            && answer.payload == weaver_types::Payload::Answer(LifecycleAnswer::Released)
        {
            Ok(())
        } else {
            Err(ChannelFault::Undecodable)
        }
    }

    fn cleanup(&mut self, run: &mut Run) -> Result<(), ChannelFault> {
        self.phase = Phase::Cleanup;
        // No state answer after acceptance can start a new setup step.
        drop(run.state.take());
        if self.ready {
            // Ready commits at selection, even if its reply cannot yet cross.
            // A later rollback is the entered run's Leave: an unconfirmed SPU
            // release still closes/reaps the child and does not undo residency.
            let leave = self.leaving.as_ref().expect("accepted Leave");
            return leave_after(
                run,
                Some(leave.cause),
                leave.forced,
                None,
                None,
                Some(leave.deadline),
            )
            .map_err(|_| ChannelFault::Closed);
        }
        let mut children = Children::default();
        if let Some(arm) = run.classify.take() {
            children.0.push(arm.pid);
            drop(arm.channel);
        }
        let release = if let Some(spu) = run.spu.take() {
            children.0.push(spu.pid);
            drop(spu.decode);
            let released = if self.admission == Admission::Admitted {
                run.spu_ordinal += 1;
                self.release(&spu.lifecycle, run.spu_ordinal)
            } else {
                Ok(())
            };
            drop(spu.lifecycle);
            released
        } else {
            Ok(())
        };
        if release.is_err() {
            children.kill();
        }
        let reaped = children.reap(self);
        release?;
        reaped?;
        self.live(None)?;
        let deadline = self
            .leaving
            .as_ref()
            .expect("cleanup has an accepted leave")
            .deadline;
        if run.recorder.failed()
            || !matches!(
                run.recorder.drain_by(deadline),
                Ok(weaver_trace::Drained::Emptied)
            )
        {
            return Err(ChannelFault::Closed);
        }
        self.live(None)
    }

    fn queue_left(&mut self) -> Result<(), ChannelFault> {
        let leave = self
            .leaving
            .as_mut()
            .expect("cleanup has an accepted leave");
        let mut replies = vec![(leave.peer, leave.exchange.clone(), leave.cause)];
        replies.append(&mut leave.joins);
        let forced = leave.forced;
        let answer = LifecycleAnswer::Left {
            save_point: None,
            forced,
            no_state: true,
        };
        self.left = Some(answer.clone());
        for (peer, exchange, _cause) in replies {
            self.queue(
                peer,
                OrganEnvelope {
                    exchange,
                    position: Position::Close,
                    payload: weaver_types::Payload::Answer(answer.clone()),
                },
                false,
            )?;
        }
        Ok(())
    }

    fn fail(&mut self, run: &mut Run) {
        // Before ReplayOpened there is no diagnostic bracket in which a Fault
        // could be authored. Close the actual work ends and kill only children
        // this worker created. Failure never produces Left or a replay grant.
        drop(run.state.take());
        let mut children = Children::default();
        if let Some(spu) = run.spu.take() {
            children.0.push(spu.pid);
            drop((spu.lifecycle, spu.decode));
        }
        if let Some(arm) = run.classify.take() {
            children.0.push(arm.pid);
            drop(arm.channel);
        }
        children.kill();
        self.phase = Phase::Cleanup;
        // An established rollback keeps its interval even on this road.
        let deadline = self.deadline(shared_deadline(Instant::now()));
        let _ = children.reap_by(self, deadline);
    }
}

pub(super) fn classify_death(pid: nix::unistd::Pid) -> Option<LifecycleRefusal> {
    use nix::sys::wait::{Id, WaitPidFlag, waitid};
    // The setup cleanup still owns the child. Reading its cause must not
    // release the PID for reuse before that owner closes and reaps it.
    let status = waitid(
        Id::Pid(pid),
        WaitPidFlag::WEXITED | WaitPidFlag::WNOWAIT | WaitPidFlag::WNOHANG,
    )
    .ok()?;
    organ_death_refusal(status)
}

/// A failure still owns all children whose reap has not been observed. Drop
/// cannot grant a successful leave after failure. At that terminal boundary
/// the existing kill-then-reap rule applies, with admin retaining containment.
#[derive(Default)]
struct Children(Vec<nix::unistd::Pid>);

impl Children {
    fn kill(&self) {
        for pid in &self.0 {
            let _ = nix::sys::signal::kill(*pid, nix::sys::signal::Signal::SIGKILL);
        }
    }

    fn reap(&mut self, setup: &mut Setup<'_>) -> Result<(), ChannelFault> {
        self.reap_by(setup, None)
    }

    fn reap_by(
        &mut self,
        setup: &mut Setup<'_>,
        deadline: Option<Instant>,
    ) -> Result<(), ChannelFault> {
        use nix::sys::wait::{WaitPidFlag, WaitStatus, waitpid};
        while let Some(pid) = self.0.last().copied() {
            setup.live(deadline)?;
            match waitpid(pid, Some(WaitPidFlag::WNOHANG)) {
                Ok(WaitStatus::Exited(..) | WaitStatus::Signaled(..))
                | Err(nix::errno::Errno::ECHILD) => {
                    self.0.pop();
                }
                Ok(WaitStatus::StillAlive) | Err(nix::errno::Errno::EINTR) => {
                    setup.poll(None, deadline, Some(Duration::from_millis(10)))?;
                }
                _ => return Err(ChannelFault::Closed),
            }
        }
        setup.live(deadline)
    }
}

impl Drop for Children {
    fn drop(&mut self) {
        self.kill();
        for pid in &self.0 {
            // As in the existing terminal reap, SIGKILL precedes the wait.
            // An expired rollback cannot grant Left, but still owns its child
            // until the actual reap. EINTR does not discharge that custody.
            while matches!(
                nix::sys::wait::waitpid(*pid, None),
                Err(nix::errno::Errno::EINTR)
            ) {}
        }
    }
}

impl Harness {
    pub(super) fn enter_diagnostic_managed(
        &mut self,
        connection: &OrganChannel,
        exchange: ExchangeId,
        payload: weaver_types::EnterPayload,
        sink: Option<OwnedFd>,
        state_end: Option<OwnedFd>,
    ) -> Result<Option<Outcome>, ChannelFault> {
        let mut setup = Setup::new(&self.coordination, connection, exchange.clone())?;
        let entered = self.enter_with_setup(payload, sink, state_end, Some(&mut setup));
        let (mut run, refusal) = match entered {
            Ok(run) => (run, None),
            Err(EnterFailure::AfterLoad(run, refusal)) => (*run, Some(refusal)),
            Err(EnterFailure::BeforeLoad(refusal)) => {
                // No adopted run or child exists. This remains the established
                // BeforeEnter path, including its Unloaded observation.
                drop(setup);
                self.refuse(connection, &exchange, refusal)?;
                return Ok(None);
            }
        };
        let driven = (|| {
            if let Some(refusal) = refusal {
                setup.enter_outcome(Err(refusal))?;
            } else {
                // Service queued rollback before committing Ready. Once Ready
                // is selected, delayed delivery cannot revise that outcome.
                if let Err(fault) = setup.checkpoint()
                    && setup.leaving.is_none()
                {
                    return Err(fault);
                }
                if run.recorder.failed() {
                    return Err(ChannelFault::Closed);
                }
                if setup.leaving.is_none() {
                    setup.enter_outcome(Ok(LifecycleAnswer::Ready))?;
                }
            }
            setup.await_disposition(&mut run)
        })();
        if driven.is_err() {
            setup.fail(&mut run);
        }
        drop(setup);
        match driven {
            Ok(false) => {
                run.diagnostic.as_mut().expect("Diagnostic run").admitted = true;
                self.state = ChannelState::Entered(Box::new(run));
                Ok(None)
            }
            Ok(true) => {
                self.coordination.unlink();
                self.state = ChannelState::Left;
                Ok(Some(Outcome::Left))
            }
            Err(fault) => {
                self.state = ChannelState::Left;
                Err(fault)
            }
        }
    }
}
