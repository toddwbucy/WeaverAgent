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
//! `weaver-harness-gate-contract` section 3, and the state has three positions,
//! before-raise, raised, and lowered, the last terminal.

use std::process::ExitCode;

use nix::poll::{PollFd, PollFlags, PollTimeout, poll};

use weaver_gate::channel::{self, Channel, ChannelFault, EntryFault};
use weaver_gate::hook::{AcceptOutcome, Hook, RaiseRefusal};
use weaver_gate::relay::{self, Relay};
use weaver_types::{
    LifecycleAnswer, LifecycleDirective, LifecycleRefusal, Opener, OrganEnvelope, Payload, Position,
};

/// The hook's state, as a type rather than a flag.
///
/// Named for the hook rather than the channel so it cannot be misread beside
/// `weaver_types::Position`, which is an envelope's place in its exchange and
/// a different thing entirely.
///
/// **Three positions, the last terminal.** A directive against a lowered hook
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
    /// The hook stands. Only a lower is in order.
    ///
    /// The relay rides with it, because its connections exist only while it
    /// does: the lower drops the hook and them together, which is what makes
    /// `weaver-gate-world-contract` section 5's "a request while the hook is
    /// lowered finds no listener" true of a connection that was already open.
    Raised(Hook, Box<Relay>),
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
        // **Wait where a landing can arrive**, per Spec section 4: the
        // channel, and while raised the listener and every served
        // connection by what it wants, readable while no exchange is open
        // and writable while a response stands undelivered. The channel
        // adds writability exactly while pending envelopes wait on it, and
        // the loop blocks on none of them.
        let mut waiting = Vec::with_capacity(4);
        let mut tags = Vec::with_capacity(4);
        let channel_flags = match &state {
            HookState::Raised(_, relay) if !relay.pending.is_empty() => {
                PollFlags::POLLIN | PollFlags::POLLOUT
            }
            _ => PollFlags::POLLIN,
        };
        waiting.push(PollFd::new(channel.as_fd(), channel_flags));
        tags.push(Tag::Channel);
        if let HookState::Raised(hook, relay) = &state {
            waiting.push(PollFd::new(hook.listener(), PollFlags::POLLIN));
            tags.push(Tag::Listener);
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
        match poll(&mut waiting, PollTimeout::NONE) {
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
                if let HookState::Raised(_, relay) = &mut state {
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
                if let HookState::Raised(_, relay) = &mut state {
                    // An errored connection surfaces through its own read,
                    // costing the connection and never the gate.
                    if revents
                        .intersects(PollFlags::POLLIN | PollFlags::POLLHUP | PollFlags::POLLERR)
                    {
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
                    } else if revents.contains(PollFlags::POLLOUT)
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

    // A frame is a response and routes by identity. Everything else is the
    // lifecycle's, judged against the position and answered.
    if let Payload::Frame(frame) = &envelope.payload {
        route_response(state, &envelope, frame);
        return Ok(());
    }

    // A cancel may cross the answer in flight. It has no answer of its own.
    if envelope.exchange.opener == Opener::Harness
        && envelope.position == Position::Continue
        && matches!(envelope.payload, Payload::ToolCancel)
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
        dispatch(state, &envelope)
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
    let HookState::Raised(_, relay) = state else {
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

fn fault_line(fault: &ChannelFault) -> String {
    match fault {
        ChannelFault::Truncated { bound } => {
            serde_json::json!({"fault": "truncated", "bound": bound}).to_string()
        }
        ChannelFault::Undecodable => serde_json::json!({"fault": "undecodable"}).to_string(),
        ChannelFault::Closed => serde_json::json!({"fault": "closed"}).to_string(),
    }
}

/// **The two exchanges of `weaver-harness-gate-contract`, judged against the
/// channel's position first.**
///
/// The contract draws raise and lower. Everything else, including a well-formed
/// directive at the wrong position, a directive that does not open its
/// exchange, and an exchange claiming an opener that is not the harness,
/// answers `OutOfOrder` before the hook is touched.
///
/// The match carries no wildcard arm over the directive, so a case added to
/// loop 0 breaks this crate loudly in the act that edits the floor.
fn dispatch(state: &mut HookState, envelope: &OrganEnvelope) -> Payload {
    if envelope.position != Position::Open || envelope.exchange.opener != Opener::Harness {
        return Payload::Refusal(LifecycleRefusal::OutOfOrder);
    }
    let Payload::Directive(directive) = &envelope.payload else {
        return Payload::Refusal(LifecycleRefusal::OutOfOrder);
    };

    match (&*state, directive) {
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
        (HookState::Raised(_, _), LifecycleDirective::Lower) => {
            // The close happens here, before the answer is formed, so nothing
            // new can arrive once the harness reads stopped.
            let previous = std::mem::replace(state, HookState::Lowered);
            if let HookState::Raised(hook, relay) = previous {
                // The served connections close with the listener, whatever
                // their buffers still held undelivered, which is what makes
                // a lowered hook find nothing standing: no turn is in flight
                // at a lower, so what the closes drop is deliveries at most.
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
            | LifecycleDirective::Leave
            | LifecycleDirective::Stop
            | LifecycleDirective::Observe
            | LifecycleDirective::Admit { .. }
            | LifecycleDirective::Release
            | LifecycleDirective::Load { .. }
            | LifecycleDirective::Unload { .. }
            | LifecycleDirective::Validate { .. }
            | LifecycleDirective::List
            | LifecycleDirective::Show { .. },
        ) => Payload::Refusal(LifecycleRefusal::OutOfOrder),
    }
}

#[cfg(test)]
#[path = "../tests/common/mod.rs"]
mod test_common;

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use weaver_types::{AccessRule, AgentName, ExchangeId, GateInstruction};

    /// conforms: gate-cancel-past-the-answer-is-dropped
    /// The real served channel must not insert a refusal before the next
    /// execution's answer. Removing only the late-cancel drop fails this.
    #[test]
    fn a_cancel_past_the_answer_is_dropped() {
        use nix::sys::socket::{MsgFlags, recv, send};
        use std::os::fd::AsRawFd;
        if std::env::var_os("WEAVER_LATE_CANCEL_CHILD").is_some() {
            let channel = channel::adopt().expect("inherited channel");
            assert_eq!(serve(channel), ExitCode::SUCCESS);
            return;
        }
        let (harness, child_end) = test_common::seqpacket_pair();
        test_common::bound_receives(&harness, 5);
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "tests::a_cancel_past_the_answer_is_dropped",
                "--nocapture",
            ])
            .env("WEAVER_LATE_CANCEL_CHILD", "1");
        test_common::place_inherited(&mut command, &[child_end.as_raw_fd()]);
        let mut child = command.spawn().unwrap();
        drop(child_end);
        let late_cancel = scratch("late-cancel");
        let ready = test_common::ask(
            &harness,
            1,
            LifecycleDirective::Raise {
                instruction: instruction(),
                socket: late_cancel.to_path_buf(),
            },
        );
        assert_eq!(ready.payload, Payload::Answer(LifecycleAnswer::GateReady));
        let write = |envelope: &OrganEnvelope| {
            send(
                harness.as_raw_fd(),
                &serde_json::to_vec(envelope).unwrap(),
                MsgFlags::empty(),
            )
            .unwrap();
        };
        let read = || {
            let mut buffer = vec![0; weaver_types::MAX_ENVELOPE_BYTES];
            let n = recv(harness.as_raw_fd(), &mut buffer, MsgFlags::empty()).unwrap();
            serde_json::from_slice::<OrganEnvelope>(&buffer[..n]).unwrap()
        };
        for ordinal in [2, 3] {
            let exchange = weaver_types::ExchangeId {
                opener: Opener::Harness,
                ordinal,
            };
            write(&OrganEnvelope {
                exchange: exchange.clone(),
                position: Position::Open,
                payload: Payload::Tool(weaver_types::ToolExecution {
                    name: weaver_types::ToolName("bash".into()),
                    arguments: r#"{"command":"echo completed"}"#.into(),
                    clock_ms: 1_000,
                }),
            });
            let answer = read();
            assert_eq!(answer.exchange, exchange);
            assert!(
                matches!(
                    answer.payload,
                    Payload::ToolAnswer(weaver_types::ToolOutcome::Result { .. })
                ),
                "{answer:?}"
            );
            if ordinal == 2 {
                write(&OrganEnvelope {
                    exchange,
                    position: Position::Continue,
                    payload: Payload::ToolCancel,
                });
            }
        }
        // Other misplaced payloads and positions still refuse.
        for (position, payload) in [
            (
                Position::Continue,
                Payload::Directive(LifecycleDirective::Stop),
            ),
            (Position::Open, Payload::ToolCancel),
        ] {
            write(&OrganEnvelope {
                exchange: weaver_types::ExchangeId {
                    opener: Opener::Harness,
                    ordinal: 4,
                },
                position,
                payload,
            });
            assert_eq!(
                read().payload,
                Payload::Refusal(LifecycleRefusal::OutOfOrder)
            );
        }
        drop(harness);
        assert!(test_common::wait_bounded(&mut child, 5, "late-cancel child").success());
    }

    fn opened(directive: LifecycleDirective) -> OrganEnvelope {
        OrganEnvelope {
            exchange: ExchangeId {
                opener: Opener::Harness,
                ordinal: 1,
            },
            position: Position::Open,
            payload: Payload::Directive(directive),
        }
    }

    fn instruction() -> GateInstruction {
        GateInstruction {
            access_rule: AccessRule {
                allowed_uids: BTreeSet::new(),
                allowed_gids: BTreeSet::new(),
                denied_uids: BTreeSet::new(),
            },
        }
    }

    /// A socket path inside a directory of its own, the directory removed
    /// when the test ends, pass or fail: the guard drops on the unwind a
    /// failed assertion takes as on a clean return (#690 item C2.9). It reads
    /// as the socket's path, and a caller handing the path to a directive
    /// holds the guard for as long as the socket is wanted.
    struct Scratch {
        dir: std::path::PathBuf,
        file: std::path::PathBuf,
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    impl std::ops::Deref for Scratch {
        type Target = std::path::Path;
        fn deref(&self) -> &std::path::Path {
            &self.file
        }
    }

    impl AsRef<std::path::Path> for Scratch {
        fn as_ref(&self) -> &std::path::Path {
            &self.file
        }
    }

    /// A scratch socket path, pre-cleaned, matching the boundary and entry
    /// helpers so a previous run's leftover cannot make a raise refuse.
    fn scratch(name: &str) -> Scratch {
        let dir = std::env::temp_dir().join(format!("weaver-gate-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a scratch dir");
        let path = dir.join("gate.sock");
        std::fs::remove_file(&path).ok();
        Scratch { dir, file: path }
    }

    /// **A lower before any raise answers `OutOfOrder`.** The order is judged
    /// against the position before the hook is touched.
    ///
    /// Perturbation: give `(BeforeRaise, Lower)` an arm that answers
    /// `GateStopped` and this test fails. Watched under exactly that addition.
    #[test]
    fn a_lower_before_any_raise_answers_out_of_order() {
        let mut state = HookState::BeforeRaise;
        assert_eq!(
            dispatch(&mut state, &opened(LifecycleDirective::Lower)),
            Payload::Refusal(LifecycleRefusal::OutOfOrder)
        );
        assert!(matches!(state, HookState::BeforeRaise), "and is not queued");
    }

    /// **The lowered position is terminal.** A directive of any kind arriving
    /// after a lower answers `OutOfOrder`.
    #[test]
    fn any_directive_after_a_lower_answers_out_of_order() {
        let mut state = HookState::Lowered;
        let terminal = scratch("terminal");
        for case in [
            LifecycleDirective::Raise {
                instruction: instruction(),
                socket: terminal.to_path_buf(),
            },
            LifecycleDirective::Lower,
            LifecycleDirective::List,
        ] {
            assert_eq!(
                dispatch(&mut state, &opened(case)),
                Payload::Refusal(LifecycleRefusal::OutOfOrder)
            );
            assert!(
                matches!(state, HookState::Lowered),
                "terminal stays terminal"
            );
        }
    }

    /// **A raise answers ready, and a lower after it answers stopped**, each
    /// once, in order.
    #[test]
    fn raise_then_lower_walks_the_three_positions() {
        let path = scratch("walk");
        let mut state = HookState::BeforeRaise;

        let ready = dispatch(
            &mut state,
            &opened(LifecycleDirective::Raise {
                instruction: instruction(),
                socket: path.to_path_buf(),
            }),
        );
        assert_eq!(ready, Payload::Answer(LifecycleAnswer::GateReady));
        assert!(matches!(state, HookState::Raised(_, _)));
        assert!(path.exists(), "ready is a fact about a bound listener");

        // A second raise is out of order even though the first succeeded.
        assert_eq!(
            dispatch(
                &mut state,
                &opened(LifecycleDirective::Raise {
                    instruction: instruction(),
                    socket: path.to_path_buf(),
                }),
            ),
            Payload::Refusal(LifecycleRefusal::OutOfOrder)
        );

        let stopped = dispatch(&mut state, &opened(LifecycleDirective::Lower));
        assert_eq!(stopped, Payload::Answer(LifecycleAnswer::GateStopped));
        assert!(matches!(state, HookState::Lowered));
        // **This crate unlinks nothing**: the path is the operator's artifact
        // and survives the lower.
        assert!(
            path.exists(),
            "the lower closes the listener and unlinks nothing"
        );
        std::fs::remove_file(&path).ok();
    }

    /// **A refused raise leaves nothing held.** The position is unchanged, so
    /// the aggregate's rollback has nothing of this crate's to unwind, and the
    /// refusal is answered rather than exited on.
    ///
    /// Perturbation: move `*position = HookState::Raised(..)` before the bind's
    /// result is judged and this test fails. Watched under exactly that move.
    #[test]
    fn a_refused_raise_holds_nothing_and_is_answered() {
        // A path inside a directory that does not exist cannot be bound. Built
        // from this test's own scratch directory with a missing component in
        // the middle, so the fixture does not rest on `/nonexistent` being
        // absent on whatever machine runs it.
        let unbindable = scratch("refused")
            .parent()
            .expect("the scratch dir")
            .join("no-such-directory")
            .join("gate.sock");
        let mut state = HookState::BeforeRaise;
        assert_eq!(
            dispatch(
                &mut state,
                &opened(LifecycleDirective::Raise {
                    instruction: instruction(),
                    socket: unbindable.clone(),
                }),
            ),
            Payload::Refusal(LifecycleRefusal::BindFailed),
            "the refusal is answered, never exited on"
        );
        assert!(
            matches!(state, HookState::BeforeRaise),
            "and nothing is held: no listener, no half-bound socket"
        );
    }

    /// Everything outside the drawn vocabulary refuses as out of order.
    #[test]
    fn a_directive_outside_the_drawn_vocabulary_refuses() {
        let mut state = HookState::BeforeRaise;
        for case in [
            LifecycleDirective::Leave,
            LifecycleDirective::Stop,
            LifecycleDirective::Release,
            LifecycleDirective::List,
            LifecycleDirective::Show {
                agent: AgentName("alpha".into()),
            },
        ] {
            assert_eq!(
                dispatch(&mut state, &opened(case)),
                Payload::Refusal(LifecycleRefusal::OutOfOrder)
            );
        }
    }

    /// A mis-shapen exchange is refused before the directive's case is read.
    #[test]
    fn a_mis_shapen_exchange_refuses_before_the_directive_runs() {
        let mut state = HookState::BeforeRaise;
        let misshapen = scratch("misshapen");
        let mut closing = opened(LifecycleDirective::Raise {
            instruction: instruction(),
            socket: misshapen.to_path_buf(),
        });
        closing.position = Position::Close;
        assert_eq!(
            dispatch(&mut state, &closing),
            Payload::Refusal(LifecycleRefusal::OutOfOrder)
        );
        assert!(matches!(state, HookState::BeforeRaise), "nothing was bound");
    }
}
