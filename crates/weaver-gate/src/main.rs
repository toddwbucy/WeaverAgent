//! conforms: gate-cancel-past-the-answer-is-dropped
//! conforms: gate-out-of-order-refused
//! conforms: gate-channel-state-three-positions
//! conforms: gate-closure-is-death
//!
//! Entry, the serve loop, and the dispatch, per `weaver-gate-Spec` section 1.
//! The two hygiene sets are `channel.rs`'s, performed inside `adopt`, which
//! this file calls and does not repeat.
//!
//! **The exchange service is a serial loop over the channel.** A directive out
//! of order for the channel's state answers `OutOfOrder`, per
//! `weaver-harness-gate-contract` section 3, and the state has four positions,
//! before-raise, raised, draining and lowered, the last terminal
//! (`weaver-gate-Spec` section 4).

use std::process::ExitCode;

use nix::poll::{PollFd, PollFlags, PollTimeout, poll};

use weaver_gate::channel::{self, Channel, ChannelFault, EntryFault};
use weaver_gate::hook::{AcceptOutcome, Hook, RaiseRefusal};
use weaver_gate::relay::{self, Relay};
use weaver_types::{
    ExchangeId, LifecycleAnswer, LifecycleDirective, LifecycleRefusal, Opener, OrganEnvelope,
    Payload, Position,
};

/// The hook's state, as a type rather than a flag.
///
/// Named for the hook rather than the channel so it cannot be misread beside
/// `weaver_types::Position`, which is an envelope's place in its exchange and
/// a different thing entirely.
///
/// **Four positions, the last terminal** (`weaver-gate-Spec` section 4). A directive against a lowered hook
/// is refused by a match arm rather than by a flag check, which is what makes
/// the ordering the compiler's business: adding a position breaks every match
/// that judges one.
///
/// The hook lives inside `Raised` rather than beside it, so a raised state
/// without a listener and a lowered state still holding one are both
/// unrepresentable.
enum HookState {
    /// No raise has arrived. Only a raise is in order.
    BeforeRaise,
    /// The hook stands. A quiesce or a lower is in order, and tool executions
    /// open here alone.
    ///
    /// The relay rides with it, because its connections exist only while it
    /// does: the lower drops the hook and them together, which is what makes
    /// `weaver-gate-world-contract` section 5's "a request while the hook is
    /// lowered finds no listener" true of a connection that was already open.
    Raised(Hook, Box<Relay>),
    /// The graceful unload's, between `Quiesce` and `Lower`: the listener is
    /// closed, so only the relay rides, and its connections are written and
    /// never read, each leaving once what it is owed is delivered. A `Lower`
    /// heard here waits as `Some`: `GateStopped` is answered once nothing is
    /// owed, or at the lower bound with each lost delivery named.
    Draining(Box<Relay>, Option<Stopping>),
    /// Terminal. A directive of any kind arriving here answers `OutOfOrder`.
    Lowered,
}

fn main() -> ExitCode {
    // Entry performs its two hygiene sets and one election before the first
    // read.
    match channel::adopt() {
        Ok(channel) => serve(channel),
        Err(EntryFault::Unusable) => {
            // Mute by construction: with no usable channel the only refusal
            // available is the exit the harness observes.
            eprintln!("{}", serde_json::json!({"refusal": "descriptors_unusable"}));
            ExitCode::FAILURE
        }
        Err(EntryFault::HygieneFailed(channel)) => {
            // **The channel is usable, so this refuses in words rather than by
            // dying.** Exiting here would reach the harness as a closed channel
            // and be aggregated as a no-residency, sending the operator to
            // debug the wrong subsystem for what the floor already names.
            eprintln!("{}", serde_json::json!({"refusal": "boundary_unverified"}));
            refuse_everything(channel)
        }
    }
}

/// A process that failed its hygiene sets serves nothing and answers every
/// directive with the floor's case for exactly this: the boundary could not be
/// established. It runs the same loop shape so the harness reads a typed reason
/// on the exchange it opened.
fn refuse_everything(channel: Channel) -> ExitCode {
    loop {
        let envelope = match channel.recv() {
            Ok(envelope) => envelope,
            Err(ChannelFault::Closed) => return ExitCode::FAILURE,
            Err(_) => return ExitCode::FAILURE,
        };
        let answered = channel.send(&OrganEnvelope {
            exchange: envelope.exchange,
            position: Position::Close,
            payload: Payload::Refusal(LifecycleRefusal::BoundaryUnverified),
        });
        if answered.is_err() {
            return ExitCode::FAILURE;
        }
    }
}

/// A `Lower` heard while draining, answered last: the exchange its
/// `GateStopped` closes, and the settle instant, one margin inside the lower
/// bound.
struct Stopping {
    exchange: ExchangeId,
    deadline: std::time::Instant,
}

/// The serial loop.
///
/// **Closure is death.** A read that returns closure means the interior is
/// gone: this crate closes its listener if one stands and exits, never treating
/// closure as an answer. The listener's close is the drop of the position,
/// which happens on the way out of this function.
fn serve(channel: Channel) -> ExitCode {
    let mut state = HookState::BeforeRaise;

    /// What one poll slot waits for, carried beside its `PollFd` so a wake
    /// names its origin without re-deriving it from a position.
    #[derive(Clone, Copy)]
    enum Tag {
        Channel,
        Listener,
        Conn(usize),
    }

    loop {
        // **A lower from draining answers last** (`weaver-gate-Spec` section
        // 4): once nothing is owed, or at the lower bound, each connection
        // still owed then named on standard error with its dialer, its
        // delivery lost, and the relay dropped before `GateStopped`.
        let mut timeout = PollTimeout::NONE;
        if let HookState::Draining(relay, Some(stopping)) = &state {
            let now = std::time::Instant::now();
            match relay.settle(now >= stopping.deadline) {
                relay::Settled::Open => {
                    // Rounded up to the next millisecond: poll counts whole
                    // milliseconds, and a truncated timeout of zero in the
                    // last fraction would spin until the instant.
                    let remaining = (stopping.deadline - now)
                        .saturating_add(std::time::Duration::from_micros(999));
                    timeout = PollTimeout::try_from(remaining).unwrap_or(PollTimeout::MAX);
                }
                relay::Settled::Done { lost } => {
                    for dialer in lost {
                        eprintln!(
                            "{}",
                            serde_json::json!({"fault": "lost_delivery", "dialer": dialer, "at": "the lower bound"})
                        );
                    }
                    let HookState::Draining(_, Some(stopping)) =
                        std::mem::replace(&mut state, HookState::Lowered)
                    else {
                        unreachable!("matched above");
                    };
                    match channel.send(&OrganEnvelope {
                        exchange: stopping.exchange,
                        position: Position::Close,
                        payload: Payload::Answer(LifecycleAnswer::GateStopped),
                    }) {
                        Ok(()) => {}
                        Err(ChannelFault::Closed) => return ExitCode::SUCCESS,
                        Err(fault) => {
                            eprintln!("{}", fault_line(&fault));
                            return ExitCode::FAILURE;
                        }
                    }
                    continue;
                }
            }
        }

        // **Wait where a landing can arrive**, per Spec section 4: the
        // channel; while raised, the listener; and, raised or draining,
        // every served connection by what it wants, readable while no
        // exchange is open (never while draining) and writable while a
        // response stands undelivered. The channel
        // adds writability exactly while pending envelopes wait on it, and
        // the loop blocks on none of them.
        let mut waiting = Vec::with_capacity(4);
        let mut tags = Vec::with_capacity(4);
        let channel_flags = match relay_ref(&state) {
            Some(relay) if !relay.pending.is_empty() => PollFlags::POLLIN | PollFlags::POLLOUT,
            _ => PollFlags::POLLIN,
        };
        waiting.push(PollFd::new(channel.as_fd(), channel_flags));
        tags.push(Tag::Channel);
        if let HookState::Raised(hook, _) = &state {
            waiting.push(PollFd::new(hook.listener(), PollFlags::POLLIN));
            tags.push(Tag::Listener);
        }
        if let Some(relay) = relay_ref(&state) {
            for (index, served) in relay.served.iter().enumerate() {
                let mut flags = PollFlags::empty();
                if served.wants_read() {
                    flags |= PollFlags::POLLIN;
                }
                if served.wants_write() {
                    flags |= PollFlags::POLLOUT;
                }
                if flags.is_empty() {
                    continue;
                }
                waiting.push(PollFd::new(served.as_fd(), flags));
                tags.push(Tag::Conn(index));
            }
        }
        match poll(&mut waiting, timeout) {
            Ok(_) => {}
            Err(nix::errno::Errno::EINTR) => continue,
            Err(errno) => {
                // Named on the way out, per issue #221: a gate that dies
                // silently leaves the worker's fault event as the only
                // account of which party fell first.
                eprintln!(
                    "{}",
                    serde_json::json!({"gate_death": "poll failed", "errno": errno as i32})
                );
                drop(state);
                return ExitCode::FAILURE;
            }
        }

        // One wake handled per round, the channel first: a lower waiting
        // behind a queue of dials or writes would let a caller hold the
        // hook open by talking to it.
        let mut wake: Option<(Tag, PollFlags)> = None;
        for (fd, tag) in waiting.iter().zip(&tags) {
            let revents = fd.revents().unwrap_or(PollFlags::empty());
            // An invalid descriptor never becomes ready, so treating
            // POLLNVAL as anything but an ending would spin this loop.
            if revents.contains(PollFlags::POLLNVAL) {
                eprintln!(
                    "{}",
                    serde_json::json!({"gate_death": "a polled descriptor went invalid"})
                );
                drop(state);
                return ExitCode::FAILURE;
            }
            if !revents.is_empty() {
                wake = Some((*tag, revents));
                break;
            }
        }
        drop(waiting);
        let Some((tag, revents)) = wake else {
            continue;
        };

        match tag {
            Tag::Channel
                if revents
                    .intersects(PollFlags::POLLIN | PollFlags::POLLHUP | PollFlags::POLLERR) =>
            {
                if let Err(code) = serve_channel_event(&channel, &mut state) {
                    drop(state);
                    return code;
                }
            }
            Tag::Channel => {
                // Writable alone: the pending envelopes drain, in order.
                if let Some(relay) = relay_of(&mut state) {
                    while let Some(front) = relay.pending.front() {
                        match channel.try_send(front) {
                            Ok(true) => {
                                relay.pending.pop_front();
                            }
                            Ok(false) => break,
                            Err(fault) => {
                                eprintln!("{}", fault_line(&fault));
                                drop(state);
                                return ExitCode::FAILURE;
                            }
                        }
                    }
                }
            }
            Tag::Listener => {
                // An errored listener never accepts again, and judging it
                // would spin: the boundary is gone, which ends service.
                if revents.contains(PollFlags::POLLERR) {
                    eprintln!(
                        "{}",
                        serde_json::json!({"gate_death": "the client listener errored"})
                    );
                    drop(state);
                    return ExitCode::FAILURE;
                }
                if let HookState::Raised(hook, relay) = &mut state {
                    judge_one(hook, relay);
                }
            }
            Tag::Conn(at) => {
                if let Some(relay) = relay_of(&mut state) {
                    // **Where an answer is owed, the write comes first**
                    // (self-check on B1): a hang-up wakes as POLLOUT with
                    // POLLHUP, and taking the read for it spun the loop
                    // without ever trying the write whose failure removes
                    // the connection. A connection is read only while it
                    // wants a read, so a draining one never is.
                    if relay.served.get(at).is_some_and(relay::Served::wants_write)
                        && revents.intersects(
                            PollFlags::POLLOUT | PollFlags::POLLHUP | PollFlags::POLLERR,
                        )
                        && let Some(served) = relay.served.get_mut(at)
                    {
                        match served.on_writable() {
                            Ok(()) => {
                                // The drain finished, so the scan resumes
                                // over the residual, the cap admitting the
                                // next line only now, and a spent
                                // connection leaves quietly.
                                match relay.frame_one(at) {
                                    Ok(relay::Framed::Opened(next)) => {
                                        if let Err(code) = send_or_pend(&channel, relay, *next) {
                                            drop(state);
                                            return code;
                                        }
                                    }
                                    Ok(relay::Framed::Waiting) => {
                                        if relay.served.get(at).is_some_and(relay::Served::spent) {
                                            relay.served.swap_remove(at);
                                        }
                                    }
                                    Err(gone) => remove(relay, at, &gone),
                                }
                            }
                            Err(gone) => remove(relay, at, &gone),
                        }
                    } else if revents
                        .intersects(PollFlags::POLLIN | PollFlags::POLLHUP | PollFlags::POLLERR)
                    {
                        // An errored connection surfaces through its own
                        // read, costing the connection and never the gate.
                        match relay.read_one(at) {
                            Ok(relay::Framed::Opened(envelope)) => {
                                if let Err(code) = send_or_pend(&channel, relay, *envelope) {
                                    drop(state);
                                    return code;
                                }
                            }
                            Ok(relay::Framed::Waiting) => {
                                // A half-closed peer with nothing left to
                                // serve leaves quietly, its conversation
                                // finished.
                                if relay.served.get(at).is_some_and(relay::Served::spent) {
                                    relay.served.swap_remove(at);
                                }
                            }
                            Err(gone) => remove(relay, at, &gone),
                        }
                    }
                }
            }
        }
    }
}

/// One channel event: a response frame routed to the connection its
/// exchange names, or a directive judged and answered.
fn serve_channel_event(channel: &Channel, state: &mut HookState) -> Result<(), ExitCode> {
    let envelope = match channel.recv() {
        Ok(envelope) => envelope,
        Err(ChannelFault::Closed) => {
            // The interior is gone. Returning drops the state, closing the
            // listener and every served connection, and this exits rather
            // than answering.
            return Err(ExitCode::SUCCESS);
        }
        Err(fault) => {
            eprintln!("{}", fault_line(&fault));
            return Err(ExitCode::FAILURE);
        }
    };
    // **The lower's clock starts when it is read** (Codex on #114): every
    // wait the lower then takes, the flush and the deferral, runs against the
    // settle instant measured from here, never from after a blocking step.
    let heard = std::time::Instant::now();

    // A frame is a response and routes by identity. Everything else is the
    // lifecycle's, judged against the position and answered.
    if let Payload::Frame(frame) = &envelope.payload {
        route_response(state, &envelope, frame);
        return Ok(());
    }

    // A cancel or the unload's interrupt may cross the answer in flight:
    // the result already sent stands (the first outcome wins), and neither
    // has an answer of its own.
    if envelope.exchange.opener == Opener::Harness
        && envelope.position == Position::Continue
        && matches!(
            envelope.payload,
            Payload::ToolCancel | Payload::ToolInterrupt
        )
    {
        return Ok(());
    }
    let payload = if let Payload::Tool(execution) = &envelope.payload
        && envelope.exchange.opener == Opener::Harness
        && envelope.position == Position::Open
        && matches!(state, HookState::Raised(_, _))
    {
        match weaver_gate::tools::execute(execution, channel, &envelope.exchange) {
            Ok(outcome) => Payload::ToolAnswer(outcome),
            Err(ChannelFault::Closed) => return Err(ExitCode::SUCCESS),
            Err(fault) => {
                eprintln!("{}", fault_line(&fault));
                return Err(ExitCode::FAILURE);
            }
        }
    } else {
        // **The quiesce and the lower answer after every admitted frame**
        // (`weaver-harness-gate-contract` section 2): what waits on the
        // channel's writability is sent first, so the harness meets each
        // request ahead of the answer and refuses it `Unloading`. Nothing is
        // accepted or read while this runs, so flushing before the position
        // changes is the Spec's order.
        // The quiesce's flush is the drain's wait, which only a declared
        // `drain-bound` bounds (`weaver-harness-Spec` section 6); the lower's
        // stops at the settle instant, each frame it could not send named
        // lost.
        let flushed = match (&envelope.payload, relay_of(state)) {
            (Payload::Directive(LifecycleDirective::Quiesce), Some(relay)) => {
                relay.flush_pending(channel).map(|()| Vec::new())
            }
            (Payload::Directive(LifecycleDirective::Lower), Some(relay)) => {
                relay.flush_pending_until(channel, settle_instant(heard))
            }
            _ => Ok(Vec::new()),
        };
        match flushed {
            Ok(lost) => {
                for dialer in lost {
                    eprintln!(
                        "{}",
                        serde_json::json!({"fault": "lost_delivery", "dialer": dialer, "at": "the lower's flush"})
                    );
                }
            }
            Err(ChannelFault::Closed) => return Err(ExitCode::SUCCESS),
            Err(fault) => {
                eprintln!("{}", fault_line(&fault));
                return Err(ExitCode::FAILURE);
            }
        }
        match dispatch_at(state, &envelope, heard) {
            Dispatched::Answer(payload) => payload,
            // A lower from draining is answered last, by the serve loop.
            Dispatched::Deferred => return Ok(()),
        }
    };
    match channel.send(&OrganEnvelope {
        exchange: envelope.exchange,
        position: Position::Close,
        payload,
    }) {
        Ok(()) => Ok(()),
        // **The interior went away between the directive and its answer,
        // which is the ordinary teardown rather than a failure here.** The
        // harness's leave sends Lower, drops the channel, and reaps without
        // reading the answer, so this is the path every clean session
        // takes: treating it as a failure made the gate exit non-zero on
        // every orderly unload, and any supervisor reading exit status
        // recorded a fault per session.
        Err(ChannelFault::Closed) => Err(ExitCode::SUCCESS),
        // The other two mean this crate built something it cannot send,
        // which is this crate's fault and is reported as one.
        Err(fault) => {
            eprintln!("{}", fault_line(&fault));
            Err(ExitCode::FAILURE)
        }
    }
}

/// A response frame from the harness: the exchange's identity names the
/// connection owed it, the line queues with its delimiter, and the scan
/// resumes over the residual so a waiting line is served in its turn. A
/// response owed to a connection that already left is a lost delivery and
/// not a lost turn, per the world contract, and it is reported as one.
fn route_response(
    state: &mut HookState,
    envelope: &OrganEnvelope,
    frame: &weaver_types::TurnFrame,
) {
    // A draining relay still routes the responses it owes.
    let Some(relay) = relay_of(state) else {
        eprintln!(
            "{}",
            serde_json::json!({"fault": "a response arrived with no hook raised"})
        );
        return;
    };
    if envelope.exchange.opener != Opener::Gate || envelope.position != Position::Close {
        eprintln!(
            "{}",
            serde_json::json!({"fault": "a frame outside this crate's exchanges"})
        );
        return;
    }
    let ordinal = envelope.exchange.ordinal;
    let Some(at) = relay.owed(ordinal) else {
        eprintln!(
            "{}",
            serde_json::json!({"fault": "lost_delivery", "exchange": ordinal})
        );
        return;
    };
    let routed = relay
        .served
        .get_mut(at)
        .expect("owed proved the index")
        .on_response(frame);
    match routed {
        // The response queues and the scan waits: the cap admits the next
        // line only after the drain finishes, so the outbound buffer holds
        // at most one response and the residual is scanned again from the
        // writable wake that empties it.
        Ok(()) => {}
        Err(gone) => remove(relay, at, &gone),
    }
}

/// Sends a frame's envelope without blocking, the envelope waiting in the
/// relay when the channel cannot take it yet, draining under the poll, per
/// Spec section 4.
fn send_or_pend(
    channel: &Channel,
    relay: &mut Relay,
    envelope: OrganEnvelope,
) -> Result<(), ExitCode> {
    match channel.try_send(&envelope) {
        Ok(true) => Ok(()),
        Ok(false) => {
            relay.pending.push_back(envelope);
            Ok(())
        }
        Err(fault) => {
            eprintln!("{}", fault_line(&fault));
            Err(ExitCode::FAILURE)
        }
    }
}

/// A connection leaves the relay, its reason to standard error and its
/// stream closed by the drop, never a word to the peer: the peer left the
/// protocol or the conversation, and the boundary answers by closure.
fn remove(relay: &mut Relay, at: usize, gone: &relay::Gone) {
    relay.served.swap_remove(at);
    eprintln!(
        "{}",
        serde_json::json!({"connection": "gone", "reason": format!("{gone:?}")})
    );
}

/// How many connections the raised window serves before it refuses further
/// ones by closure.
///
/// **A bound rather than a capacity**: it keeps a peer that dials in a loop
/// from consuming the gate's descriptor table. Past it the accept still
/// runs and the connection drops, refused by capacity rather than by
/// identity. Sizing it against real traffic is issue 102's baseline
/// measurement, taken once a live turn exists to measure.
const RETAINED_LIMIT: usize = 64;

/// Accept one dialing peer, judge it, and admit it into the relay.
///
/// A peer that fails the predicate is refused at accept, before any content
/// is read, by closure with nothing written back, per
/// `weaver-gate-world-contract` section 5: `Hook::accept` returns no stream
/// for a refused peer, so the closure is the drop inside it. An admitted
/// peer is served, nonblocking end to end, per Spec section 4.
///
/// **Past the bound the peer is closed, and the accept still runs.**
/// Declining to accept would leave the listener readable and the loop
/// spinning, so the accept always happens and what changes at the bound is
/// whether the connection is kept: refused by capacity, not by identity.
fn judge_one(hook: &Hook, relay: &mut Relay) {
    match hook.accept() {
        Ok(peer) => {
            if relay.served.len() < RETAINED_LIMIT {
                match relay::Served::admit(peer) {
                    Ok(served) => relay.served.push(served),
                    Err(error) => {
                        eprintln!(
                            "{}",
                            serde_json::json!({"fault": "admit_failed", "detail": error.to_string()})
                        );
                    }
                }
            }
        }
        // Refused, and already closed by the accept that judged it.
        Err(AcceptOutcome::Denied) => {}
        Err(AcceptOutcome::Unusable { detail }) => {
            eprintln!(
                "{}",
                serde_json::json!({"fault": "accept_failed", "detail": detail})
            );
        }
    }
}

/// The relay a position carries, to read: raised, or draining after the
/// quiesce.
fn relay_ref(state: &HookState) -> Option<&Relay> {
    match state {
        HookState::Raised(_, relay) | HookState::Draining(relay, _) => Some(relay),
        HookState::BeforeRaise | HookState::Lowered => None,
    }
}

/// The relay a position carries, to change.
fn relay_of(state: &mut HookState) -> Option<&mut Relay> {
    match state {
        HookState::Raised(_, relay) | HookState::Draining(relay, _) => Some(relay),
        HookState::BeforeRaise | HookState::Lowered => None,
    }
}

fn fault_line(fault: &ChannelFault) -> String {
    match fault {
        ChannelFault::Truncated { bound } => {
            serde_json::json!({"fault": "truncated", "bound": bound}).to_string()
        }
        ChannelFault::Undecodable => serde_json::json!({"fault": "undecodable"}).to_string(),
        ChannelFault::Closed => serde_json::json!({"fault": "closed"}).to_string(),
    }
}

/// What a directive's judgment gives: its answer now, or, for a lower from
/// draining, an answer the serve loop sends last.
#[derive(Debug)]
enum Dispatched {
    Answer(Payload),
    Deferred,
}

// Test-only hook kept here (#118): a trait impl beside the type it compares.
#[cfg(test)]
impl PartialEq<Payload> for Dispatched {
    fn eq(&self, other: &Payload) -> bool {
        matches!(self, Dispatched::Answer(payload) if payload == other)
    }
}

/// **The exchanges of `weaver-harness-gate-contract`, judged against the
/// channel's position first.**
///
/// The contract draws raise, quiesce and lower. Everything else, including a well-formed
/// directive at the wrong position, a directive that does not open its
/// exchange, and an exchange claiming an opener that is not the harness,
/// answers `OutOfOrder` before the hook is touched.
///
/// The match carries no wildcard arm over the directive, so a case added to
/// loop 0 breaks this crate loudly in the act that edits the floor.
/// The instant a lower heard at `heard` answers by: one settle margin inside
/// the lower bound, so the answer reaches a harness still waiting the full
/// bound.
fn settle_instant(heard: std::time::Instant) -> std::time::Instant {
    heard
        + std::time::Duration::from_millis(
            weaver_types::LOWER_BOUND_MS.saturating_sub(weaver_types::LOWER_SETTLE_MARGIN_MS),
        )
}

/// `dispatch_at` with the directive heard now, for the position tests.
// Test-only hook kept here (#118): the position tests' entry beside `dispatch_at`.
#[cfg(test)]
fn dispatch(state: &mut HookState, envelope: &OrganEnvelope) -> Dispatched {
    dispatch_at(state, envelope, std::time::Instant::now())
}

fn dispatch_at(
    state: &mut HookState,
    envelope: &OrganEnvelope,
    heard: std::time::Instant,
) -> Dispatched {
    if envelope.position != Position::Open || envelope.exchange.opener != Opener::Harness {
        return Dispatched::Answer(Payload::Refusal(LifecycleRefusal::OutOfOrder));
    }
    let Payload::Directive(directive) = &envelope.payload else {
        return Dispatched::Answer(Payload::Refusal(LifecycleRefusal::OutOfOrder));
    };

    let answer = match (&*state, directive) {
        (
            HookState::BeforeRaise,
            LifecycleDirective::Raise {
                instruction,
                socket,
            },
        ) => {
            match Hook::raise(instruction, socket) {
                Ok(hook) => {
                    // Ready is answered only after the bind and listen have
                    // returned, which is what makes ready a fact about the
                    // listener rather than a statement of intent.
                    *state = HookState::Raised(hook, Box::new(Relay::new()));
                    Payload::Answer(LifecycleAnswer::GateReady)
                }
                Err(RaiseRefusal::BindFailed { detail }) => {
                    // The reason travels to standard error, because the floor's
                    // closed refusal set carries no detail field and the
                    // operator who must clear the path needs to know which one.
                    // A refusal leaves nothing held: the position is unchanged,
                    // so no listener and no half-bound socket stand, and the
                    // aggregate's rollback has nothing of this crate's to
                    // unwind.
                    eprintln!(
                        "{}",
                        serde_json::json!({"refusal": "bind_failed", "detail": detail})
                    );
                    Payload::Refusal(LifecycleRefusal::BindFailed)
                }
            }
        }
        (HookState::Raised(_, _), LifecycleDirective::Quiesce) => {
            // The listener closes first and the answer confirms after: no new
            // connection is accepted from here, every connection owed nothing
            // closes, and the rest drain, read no more.
            let previous = std::mem::replace(state, HookState::Lowered);
            if let HookState::Raised(hook, mut relay) = previous {
                hook.lower();
                relay.quiesce();
                *state = HookState::Draining(relay, None);
            }
            Payload::Answer(LifecycleAnswer::GateQuiesced)
        }
        // **A lower from draining answers last** (`weaver-gate-Spec` section
        // 4): the position holds its exchange and the settle instant, one
        // margin inside the lower bound so the answer reaches a harness still
        // waiting the full bound; a request never answered closes now; and the
        // serve loop answers once every response the harness sent is written,
        // or at that instant.
        (HookState::Draining(_, None), LifecycleDirective::Lower) => {
            if let HookState::Draining(relay, stopping) = state {
                relay.lower();
                *stopping = Some(Stopping {
                    exchange: envelope.exchange.clone(),
                    deadline: settle_instant(heard),
                });
            }
            return Dispatched::Deferred;
        }
        // One `Lower` per leave: a second while the first waits is out of
        // order, the first's exchange standing.
        (HookState::Draining(_, Some(_)), LifecycleDirective::Lower) => {
            Payload::Refusal(LifecycleRefusal::OutOfOrder)
        }
        (HookState::Raised(_, _), LifecycleDirective::Lower) => {
            // The close happens here, before the answer is formed, so nothing
            // new can arrive once the harness reads stopped.
            let previous = std::mem::replace(state, HookState::Lowered);
            if let HookState::Raised(hook, relay) = previous {
                // **The forced unload's close** (`weaver-gate-Spec` section
                // 4): the served connections close with the listener, a turn
                // in flight among them, its connection closing unanswered, the
                // operator having chosen no time to finish. Every frame this
                // crate admitted was flushed to the channel first, so the
                // harness records each refused with `Unloading`.
                drop(relay);
                hook.lower();
            }
            Payload::Answer(LifecycleAnswer::GateStopped)
        }

        // Every other pairing is out of order: a lower before any raise, a
        // second raise, anything after a lower, and every directive the
        // contract does not draw. Refused before the hook is touched, which is
        // what not-queued means.
        (
            _,
            LifecycleDirective::Raise { .. }
            | LifecycleDirective::Lower
            | LifecycleDirective::Enter { .. }
            | LifecycleDirective::Leave { .. }
            | LifecycleDirective::Stop { .. }
            | LifecycleDirective::Observe
            | LifecycleDirective::Admit { .. }
            | LifecycleDirective::Release
            | LifecycleDirective::Load { .. }
            | LifecycleDirective::Unload { .. }
            | LifecycleDirective::Validate { .. }
            | LifecycleDirective::Show { .. }
            | LifecycleDirective::SavePoint { .. }
            | LifecycleDirective::SavePointVerb { .. }
            | LifecycleDirective::Restore { .. }
            | LifecycleDirective::ForceUnload { .. }
            | LifecycleDirective::JoinLeave { .. }
            | LifecycleDirective::Quiesce,
        ) => Payload::Refusal(LifecycleRefusal::OutOfOrder),
    };
    Dispatched::Answer(answer)
}

#[cfg(test)]
#[path = "../tests/common/mod.rs"]
mod test_common;

#[cfg(test)]
mod tests;
