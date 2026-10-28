use super::*;
use crate::channel::bind_coordination;
use nix::sys::socket::{AddressFamily, SockFlag, SockType, UnixAddr, connect, socket};
use std::os::fd::{AsRawFd, OwnedFd};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

pub(super) struct Door {
    path: PathBuf,
    raw_listener: std::os::fd::RawFd,
    pub(super) early: Option<OwnedFd>,
    pub(super) accepted: Option<u64>,
}
impl Door {
    pub(super) fn new() -> (Self, LifecycleListener) {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "weaver-rollback-door-{}-{}.sock",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let listener = bind_coordination(&path).unwrap();
        let raw_listener = listener.as_fd().as_raw_fd();
        let listener = LifecycleListener::new(listener).unwrap();
        (
            Self {
                path,
                raw_listener,
                early: None,
                accepted: None,
            },
            listener,
        )
    }
    pub(super) fn connect(&self) -> OwnedFd {
        let end = socket(
            AddressFamily::Unix,
            SockType::SeqPacket,
            SockFlag::SOCK_CLOEXEC,
            None,
        )
        .unwrap();
        connect(end.as_raw_fd(), &UnixAddr::new(&self.path).unwrap()).unwrap();
        end
    }
}
impl Drop for Door {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
fn send_request(peer: &OwnedFd, envelope: &OrganEnvelope) {
    let wire = serde_json::to_vec(envelope).unwrap();
    assert_eq!(
        send(peer.as_raw_fd(), &wire, MsgFlags::MSG_NOSIGNAL),
        Ok(wire.len())
    );
}
fn enter() -> LifecycleDirective {
    LifecycleDirective::Enter {
        payload: Box::new(weaver_types::EnterPayload {
            session: weaver_types::SessionId("unused-secondary-session".into()),
            run: weaver_types::RunId("unused-secondary-run".into()),
            spu_instruction: weaver_types::SpuInstruction {
                classify: None,
                decoder: weaver_types::DecoderInstruction {
                    model_binding: weaver_types::ModelBinding {
                        artifact: weaver_types::ArtifactRef("must-not-open".into()),
                        devices: vec![],
                    },
                    residual_readout_election: false,
                    field_election: None,
                    surprisal_election: false,
                    refeed_permission: false,
                    column_permission: false,
                    tunable_values: Default::default(),
                },
            },
            binding: weaver_types::EnterBinding::Serving {
                gate_instruction: weaver_types::GateInstruction {
                    access_rule: weaver_types::AccessRule {
                        allowed_uids: Default::default(),
                        allowed_gids: Default::default(),
                        denied_uids: Default::default(),
                    },
                },
                shell: None,
            },
            state_election: weaver_types::StateElection {
                all_kinds: false,
                keys: vec![],
            },
            state_store: Default::default(),
            declaration: String::new(),
            restore: None,
            reset: None,
            stack: Default::default(),
            boundary: String::new(),
            cause: weaver_types::Cause { uid: 0 },
            operator: 0,
            library_path: None,
            drain_bound: None,
            wind_down_bound: None,
        }),
    }
}
fn fill(peer: &LifecyclePeer) -> Vec<Vec<u8>> {
    let mut packets = Vec::new();
    for size in [4096, 1] {
        let bytes = vec![b'x'; size];
        loop {
            match send(
                peer.fd().as_raw_fd(),
                &bytes,
                MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_NOSIGNAL,
            ) {
                Ok(n) => {
                    assert_eq!(n, size);
                    packets.push(bytes.clone());
                    assert!(packets.len() < 10000);
                }
                Err(Errno::EAGAIN) => break,
                other => panic!("fill secondary peer: {other:?}"),
            }
        }
    }
    assert!(!packets.is_empty());
    packets
}

fn take_answer(peer: &OwnedFd) -> Option<OrganEnvelope> {
    let mut bytes = vec![0; weaver_types::MAX_ENVELOPE_BYTES];
    match recv(peer.as_raw_fd(), &mut bytes, MsgFlags::MSG_DONTWAIT) {
        Ok(0) => None,
        Ok(n) => Some(serde_json::from_slice(&bytes[..n]).unwrap()),
        Err(Errno::EAGAIN) => None,
        Err(error) => panic!("dial answer: {error}"),
    }
}

#[test]
fn answered_dial_hands_a_retained_connection_to_its_reader_before_waiting() {
    if !root_test("answered_dial_hands_a_retained_connection_to_its_reader_before_waiting") {
        return;
    }
    use super::super::listener::RollbackDials;
    use crate::flow::context::tests::{Fixture, config};
    let mut fx = Fixture::new(config());
    fx.activate();
    let now = fx.clock.now().unwrap();
    let leave_bound = AbsoluteBound::after(now, 150_000_000_000).unwrap();
    let (door, listener) = Door::new();
    let mut dials = RollbackDials::new(listener);
    let peer = door.connect();
    let mut ask = leave();
    ask.payload = weaver_types::Payload::Directive(LifecycleDirective::Observe);
    send_request(&peer, &ask);
    assert!(dials.listener.accept_observed(now).unwrap());
    dials
        .prepare(&mut fx.context, leave_bound, &mut fx.clock)
        .unwrap();
    dials.active.as_mut().unwrap().peer.try_receive().unwrap();
    dials
        .prepare(&mut fx.context, leave_bound, &mut fx.clock)
        .unwrap();
    dials
        .active
        .as_mut()
        .unwrap()
        .peer
        .try_send_observed(&mut fx.clock)
        .unwrap();
    assert!(take_answer(&peer).is_some());
    let next = door.connect();
    send_request(&next, &ask);
    assert!(dials.listener.accept_observed(now).unwrap());
    assert!(dials.active.as_ref().unwrap().send.as_ref().unwrap().sent());
    dials
        .prepare(&mut fx.context, leave_bound, &mut fx.clock)
        .unwrap();
    assert!(
        dials.listener.held().is_none(),
        "accepted root dial must leave the listener slot before another wait"
    );
    assert!(
        dials.active.is_some(),
        "the next original dial needs its own read interest"
    );
}

#[test]
fn retiring_a_failed_dial_preserves_its_sent_fact_and_missing_clock_failure() {
    if !root_test("retiring_a_failed_dial_preserves_its_sent_fact_and_missing_clock_failure") {
        return;
    }
    use super::super::listener::RollbackDials;
    use crate::flow::context::tests::{Fixture, config};
    for recover_first in [false, true] {
        let mut fx = Fixture::new(config());
        fx.activate();
        let now = fx.clock.now().unwrap();
        let leave_bound = AbsoluteBound::after(now, 150_000_000_000).unwrap();
        let (door, listener) = Door::new();
        let mut dials = RollbackDials::new(listener);
        let peer = door.connect();
        let mut ask = leave();
        ask.payload = weaver_types::Payload::Directive(LifecycleDirective::Observe);
        send_request(&peer, &ask);
        assert!(dials.listener.accept_observed(now).unwrap());
        dials
            .prepare(&mut fx.context, leave_bound, &mut fx.clock)
            .unwrap();
        dials.active.as_mut().unwrap().peer.try_receive().unwrap();
        dials
            .prepare(&mut fx.context, leave_bound, &mut fx.clock)
            .unwrap();
        let mut failed = false;
        let error = dials
            .active
            .as_mut()
            .unwrap()
            .peer
            .try_send_observed(&mut DialSendClock {
                io: &mut fx.clock,
                peer: &peer,
                failed: &mut failed,
            })
            .unwrap_err();
        assert!(failed);
        let send = dials.active.as_ref().unwrap().send.as_ref().unwrap();
        assert!(send.sent() && send.sent_at().is_none());
        let heard = take_answer(&peer).unwrap();
        assert_eq!(heard.exchange, ask.exchange);
        let next = door.connect();
        send_request(&next, &ask);
        assert!(dials.listener.accept_observed(now).unwrap());
        let result = if recover_first {
            dials.recover(
                &mut fx.context,
                RunFailure::Transport {
                    source: crate::flow::pump::PumpSource::AdminDial,
                    failure: error,
                },
                &mut fx.clock,
            )
        } else {
            dials.prepare(&mut fx.context, leave_bound, &mut fx.clock)
        };
        assert!(
            result.is_err(),
            "missing send clock cannot be retired as a peer departure"
        );
        let first = fx.context.failure().unwrap().clone();
        assert!(matches!(
            first,
            RunFailure::Flow {
                reason: FlowHaltReason::TraceFailed,
                ..
            }
        ));
        let dial = dials.active.as_ref().unwrap();
        let send = dial.send.as_ref().unwrap();
        assert!(dial.peer.owns_send(send));
        assert!(send.sent() && send.sent_at().is_none());
        assert!(
            dials.listener.held().is_some(),
            "the next connection is retained during halt"
        );
        assert!(
            dials
                .prepare(&mut fx.context, leave_bound, &mut fx.clock)
                .is_err()
        );
        assert_eq!(fx.context.failure().unwrap(), &first);
    }
}

struct DialSendClock<'a> {
    io: &'a mut FakePumpIo,
    peer: &'a OwnedFd,
    failed: &'a mut bool,
}
impl PumpIo for DialSendClock<'_> {
    fn now(&mut self) -> std::io::Result<u64> {
        if !*self.failed
            && recv(
                self.peer.as_raw_fd(),
                &mut [0; 1],
                MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_PEEK,
            )
            .is_ok_and(|n| n > 0)
        {
            *self.failed = true;
            return Err(std::io::Error::other("actual dial post-send clock failed"));
        }
        self.io.now()
    }
    fn wait(
        &mut self,
        interests: &[WaitInterest<'_>],
        deadline: Option<u64>,
    ) -> std::io::Result<WaitOutcome> {
        CircuitIo(self.io).wait(interests, deadline)
    }
}
struct ReversedClock(u64);
impl PumpIo for ReversedClock {
    fn now(&mut self) -> std::io::Result<u64> {
        Ok(self.0)
    }
    fn wait(&mut self, _: &[WaitInterest<'_>], _: Option<u64>) -> std::io::Result<WaitOutcome> {
        unreachable!()
    }
}

pub(super) fn drive(
    mut rollback: RollbackComposition<'_>,
    door: &mut Door,
    io: &mut FakePumpIo,
    reader: &mut UnixStream,
    state: &mut UnixStream,
    mode: &str,
    original_peer: &OrganChannel,
) {
    let original = rollback.directive().envelope.clone();
    let leave_bound = rollback.leave();
    let extra = if mode == "response-listener-original-extra" {
        let mut envelope = leave();
        envelope.exchange.ordinal = 94;
        envelope.payload = weaver_types::Payload::Directive(LifecycleDirective::Stop {
            cause: weaver_types::Cause { uid: 777 },
        });
        let wire = serde_json::to_vec(&envelope).unwrap();
        assert_eq!(
            send(
                original_peer.as_fd().as_raw_fd(),
                &wire,
                MsgFlags::MSG_NOSIGNAL
            ),
            Ok(wire.len())
        );
        Some(envelope)
    } else {
        None
    };
    let mut ask = leave();
    ask.exchange.ordinal = 117;
    ask.payload = weaver_types::Payload::Directive(match mode {
        "response-listener-enter-handles" => enter(),
        "response-listener-join" => LifecycleDirective::JoinLeave {
            cause: weaver_types::Cause { uid: 888 },
        },
        "response-listener-second-leave" => LifecycleDirective::Leave {
            cause: weaver_types::Cause { uid: 999 },
            forced: false,
            rollback: false,
        },
        _ => LifecycleDirective::Observe,
    });
    let mut peer = door.early.take().unwrap_or_else(|| door.connect());
    let mut witnesses = Vec::new();
    if mode == "response-listener-enter-handles" {
        use nix::sys::socket::{ControlMessage, sendmsg};
        let (sink, sink_witness) = UnixStream::pair().unwrap();
        let (member, member_witness) = UnixStream::pair().unwrap();
        let wire = serde_json::to_vec(&ask).unwrap();
        assert_eq!(
            sendmsg::<()>(
                peer.as_raw_fd(),
                &[std::io::IoSlice::new(&wire)],
                &[ControlMessage::ScmRights(&[
                    sink.as_raw_fd(),
                    member.as_raw_fd()
                ])],
                MsgFlags::MSG_NOSIGNAL,
                None
            )
            .unwrap(),
            wire.len()
        );
        drop(sink);
        drop(member);
        witnesses.extend([sink_witness, member_witness]);
    } else if mode == "response-listener-malformed" {
        assert_eq!(send(peer.as_raw_fd(), b"[", MsgFlags::MSG_NOSIGNAL), Ok(1));
    } else if !matches!(
        mode,
        "response-listener-silent" | "response-listener-leave-expiry"
    ) {
        send_request(&peer, &ask);
    }
    if mode == "response-listener-loss" {
        nix::sys::socket::shutdown(door.raw_listener, nix::sys::socket::Shutdown::Both).unwrap();
        nix::sys::socket::shutdown(peer.as_raw_fd(), nix::sys::socket::Shutdown::Both).unwrap();
    }
    state.set_nonblocking(true).unwrap();
    let mut request = Vec::new();
    let mut pending = None;
    let mut replied = false;
    let mut heard = None;
    let mut expired_silent = false;
    let mut dead_replaced = false;
    let mut filled = Vec::new();
    let unprivileged = mode == "response-listener-credentials" && !nix::unistd::geteuid().is_root();
    let mut clock_failed = false;
    for _ in 0..1024 {
        if mode == "response-listener-clock-reversed" && rollback.dials.listener.held().is_some() {
            let accepted = rollback.dials.listener.accepted_ns().unwrap();
            let reversed = accepted - 1;
            assert_eq!(
                rollback.advance(&mut ReversedClock(reversed)),
                RollbackStage::Stopped
            );
            assert!(matches!(
                rollback.stop(),
                Some(RollbackStop::Run(RunFailure::Flow {
                    reason: FlowHaltReason::TraceFailed,
                    ..
                }))
            ));
            assert!(rollback.dials.listener.held().is_some());
            assert_eq!(rollback.dials.listener.accepted_ns(), Some(accepted));
            assert!(rollback.dials.active.is_none());
            assert_eq!(rollback.directive().envelope, original);
            return;
        }
        let stage = if mode == "response-listener-send-clock" {
            rollback.advance(&mut DialSendClock {
                io,
                peer: &peer,
                failed: &mut clock_failed,
            })
        } else {
            rollback.advance(&mut CircuitIo(io))
        };
        if mode == "response-listener-send-clock" && stage == RollbackStage::Stopped {
            assert!(clock_failed);
            let Some(RollbackStop::Run(first)) = rollback.stop() else {
                panic!("missing clock stop")
            };
            assert!(matches!(
                first,
                RunFailure::Flow {
                    reason: FlowHaltReason::TraceFailed,
                    ..
                }
            ));
            assert_eq!(
                rollback.response.refusal.owner.startup.context.failure(),
                Some(first)
            );
            let dial = rollback
                .dials
                .active
                .as_ref()
                .expect("clock failure discarded actual sent evidence");
            assert!(dial.send.as_ref().unwrap().sent());
            assert_eq!(dial.send.as_ref().unwrap().sent_at(), None);
            let actual = take_answer(&peer).expect("actual whole answer was sent");
            assert_eq!(actual.exchange, ask.exchange);
            assert_eq!(rollback.directive().envelope, original);
            assert_eq!(rollback.leave(), leave_bound);
            assert!(rollback.into_prepared().is_err());
            return;
        }
        if mode == "response-listener-loss" && stage == RollbackStage::Stopped {
            assert!(matches!(
                rollback.stop(),
                Some(RollbackStop::Run(RunFailure::Transport {
                    source: crate::flow::pump::PumpSource::Listener,
                    ..
                }))
            ));
            assert_eq!(rollback.directive().envelope, original);
            assert_eq!(rollback.leave(), leave_bound);
            assert!(rollback.current().is_none());
            assert!(rollback.into_prepared().is_err());
            return;
        }
        assert_ne!(stage, RollbackStage::Stopped, "{:?}", rollback.stop());
        if mode == "response-listener-backpressure"
            && filled.is_empty()
            && let Some(dial) = &rollback.dials.active
        {
            assert!(dial.send.is_none());
            filled = fill(&dial.peer);
        }
        if mode == "response-listener-leave-expiry" && rollback.dials.active.is_some() {
            io.advance_to(leave_bound.deadline_ns()).unwrap();
            assert_eq!(rollback.advance(&mut CircuitIo(io)), RollbackStage::Stopped);
            assert!(
                matches!(rollback.stop(),Some(RollbackStop::Run(RunFailure::Wait {key:PumpDeadline::Leave,bound})) if *bound==leave_bound)
            );
            assert_eq!(rollback.directive().envelope, original);
            assert!(rollback.dials.active.is_some());
            assert!(rollback.into_prepared().is_err());
            return;
        }
        if mode == "response-listener-malformed"
            && !dead_replaced
            && matches!(
                recv(
                    peer.as_raw_fd(),
                    &mut [0; 1],
                    MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_PEEK
                ),
                Ok(0) | Err(Errno::ECONNRESET)
            )
        {
            drop(peer);
            peer = door.connect();
            send_request(&peer, &ask);
            dead_replaced = true;
        }
        assert_eq!(rollback.directive().envelope, original);
        assert_eq!(rollback.leave(), leave_bound);
        assert_eq!(
            rollback
                .response
                .refusal
                .owner
                .startup
                .context
                .deadlines
                .get(PumpDeadline::Leave),
            Some(leave_bound)
        );
        assert!(
            !rollback
                .response
                .refusal
                .owner
                .startup
                .context
                .boot_enter_cancelled,
            "refused secondary dial revoked boot"
        );
        assert!(
            !rollback
                .response
                .refusal
                .owner
                .startup
                .context
                .place_leaving,
            "refused secondary dial became accepted Leave"
        );
        if mode == "response-listener-peer-gone"
            && !dead_replaced
            && rollback.dials.active.is_some()
        {
            drop(peer);
            peer = door.connect();
            send_request(&peer, &ask);
            dead_replaced = true;
        }
        if mode == "response-listener-silent" && !expired_silent && pending.is_some() {
            let accepted = door.accepted.unwrap();
            let bound = rollback
                .dials
                .active
                .as_ref()
                .and_then(|dial| dial.read)
                .unwrap();
            assert_eq!(bound.began_ns(), accepted);
            assert_eq!(bound.deadline_ns(), accepted + 2_000_000_000);
            // The member's read started after acceptance. Its original window
            // remains live when this earlier silent-dial allowance ends.
            assert!(
                rollback
                    .response
                    .refusal
                    .owner
                    .startup
                    .context
                    .deadlines
                    .get(PumpDeadline::StateControl)
                    .unwrap()
                    .deadline_ns()
                    > bound.deadline_ns()
            );
            io.advance_to(bound.deadline_ns()).unwrap();
            rollback.advance(&mut CircuitIo(io));
            assert!(rollback.dials.active.is_none());
            assert_eq!(
                rollback
                    .response
                    .refusal
                    .owner
                    .startup
                    .context
                    .deadlines
                    .get(PumpDeadline::AdminRead),
                None
            );
            drop(peer);
            peer = door.connect();
            send_request(&peer, &ask);
            expired_silent = true;
        }
        if heard.is_none()
            && mode != "response-listener-backpressure"
            && !unprivileged
            && (mode != "response-listener-malformed" || dead_replaced)
        {
            heard = take_answer(&peer);
        }
        if stage == RollbackStage::Prepared {
            break;
        }
        let mut bytes = [0; 8192];
        match state.read(&mut bytes) {
            Ok(0) => panic!("state closed during healthy read-back"),
            Ok(n) => request.extend_from_slice(&bytes[..n]),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => panic!("state read: {e}"),
        }
        if request.ends_with(b"\n") {
            assert!(pending.is_none() && !replied);
            let frame = s::decode_state_line(&request, s::FrameDirection::Request).unwrap();
            assert!(matches!(
                &frame,
                s::StateFrame::Control {
                    frame: s::StateControl::Grants {},
                    ..
                }
            ));
            pending = Some(frame);
            request.clear();
        }
        // Deliberately withhold the Grants answer until the listener answers.
        // A blocking standalone lifecycle reader cannot pass this join.
        if (heard.is_some() || mode == "response-listener-backpressure" || unprivileged)
            && let Some(request) = pending.take()
        {
            let began = io.now().unwrap() + 1;
            let answer = s::StateFrame::ControlAnswer {
                common: request.common().clone(),
                frame: s::StateControlAnswer::Grants {
                    surface: vec!["read".into()],
                },
                observation: operation(&request, 3, began),
            };
            io.advance_to(began + 2).unwrap();
            state
                .write_all(&s::encode_state_line(&answer, s::FrameDirection::Response).unwrap())
                .unwrap();
            replied = true;
        }
    }
    assert_eq!(
        rollback.stage(),
        RollbackStage::Prepared,
        "{:?}",
        rollback.stop()
    );
    assert!(
        !matches!(
            mode,
            "response-listener-loss"
                | "response-listener-send-clock"
                | "response-listener-clock-reversed"
        ),
        "expected failure branch was never reached"
    );
    if mode == "response-listener-backpressure" {
        assert!(heard.is_none());
        assert!(!filled.is_empty());
        let dial = rollback
            .dials
            .active
            .as_ref()
            .expect("pressured answer retained");
        assert!(dial.peer.owns_send(dial.send.as_ref().unwrap()));
        assert!(!dial.send.as_ref().unwrap().sent());
        assert_eq!(dial.answer.as_ref().unwrap().exchange, ask.exchange);
        assert_eq!(dial.message.as_ref().unwrap().envelope, ask);
        assert!(
            matches!(rollback.current(),Some(InspectionCompletion::Grants {surface,..}) if surface==&["read".to_owned()])
        );
        let prepared = match rollback.into_prepared() {
            Ok(p) => p,
            Err(_) => panic!("pressure blocked actual read-back"),
        };
        assert_eq!(prepared.grant_surface(), weaver_trace::GrantSurface::Varied);
        assert_eq!(prepared.leave(), leave_bound);
        assert!(prepared.owner.dials.active.is_some());
        return;
    }
    if unprivileged {
        assert!(heard.is_none());
        assert!(rollback.dials.active.is_none());
        let mut byte = [0];
        assert!(matches!(
            recv(peer.as_raw_fd(), &mut byte, MsgFlags::MSG_DONTWAIT),
            Ok(0) | Err(Errno::ECONNRESET)
        ));
        assert_eq!(rollback.directive().envelope, original);
        assert!(rollback.into_prepared().is_ok());
        return;
    }
    for witness in &mut witnesses {
        witness.set_nonblocking(true).unwrap();
        assert_eq!(
            witness.read(&mut [0; 1]).unwrap(),
            0,
            "refused Enter handle leaked or was adopted"
        );
    }
    let answer = heard.expect("listener did not answer before Grants completed");
    assert_eq!(answer.exchange, ask.exchange);
    assert_eq!(answer.position, Position::Close);
    if matches!(
        mode,
        "response-listener-join"
            | "response-listener-second-leave"
            | "response-listener-enter-handles"
    ) {
        assert!(matches!(
            answer.payload,
            weaver_types::Payload::Refusal(weaver_types::LifecycleRefusal::OutOfOrder)
        ));
    } else {
        assert!(
            matches!(answer.payload,weaver_types::Payload::Answer(weaver_types::LifecycleAnswer::InTransition {constituents}) if constituents.is_empty())
        );
    }
    assert_eq!(expired_silent, mode == "response-listener-silent");
    assert_eq!(
        dead_replaced,
        matches!(
            mode,
            "response-listener-peer-gone" | "response-listener-malformed"
        )
    );
    super::super::super::tests::drain(rollback.response.refusal.owner.startup.context, reader);
    let prepared = match rollback.into_prepared() {
        Ok(owner) => owner,
        Err(_) => panic!("read-back handoff"),
    };
    assert_eq!(prepared.grant_surface(), weaver_trace::GrantSurface::Varied);
    assert_eq!(prepared.directive().envelope, original);
    assert_eq!(prepared.leave(), leave_bound);
    assert!(prepared.owner.dials.listener.failure().is_none());
    if let Some(extra) = extra {
        assert_eq!(
            prepared.owner.peer.held().unwrap().envelope,
            extra,
            "extra original-peer frame was lost or treated as authority"
        );
        assert_eq!(prepared.directive().envelope, original);
    }
}

fn root_test(name: &str) -> bool {
    if nix::unistd::geteuid().is_root() {
        return true;
    }
    let status = std::process::Command::new("unshare")
        .arg("--map-root-user")
        .arg(std::env::current_exe().unwrap())
        .arg("--exact")
        .arg(format!(
            "flow::context::startup_composition::refusal::response::tests::listener_tests::{name}"
        ))
        .arg("--nocapture")
        .status()
        .unwrap();
    assert!(status.success(), "root-credential listener scenario failed");
    false
}

fn scenario(name: &str, mode: &'static str) {
    if root_test(name) {
        crate::flow::context::startup_composition::tests::refusal_join(mode);
    }
}
macro_rules! case {
    ($name:ident,$mode:literal) => {
        #[test]
        fn $name() {
            scenario(stringify!($name), $mode);
        }
    };
}
case!(
    rollback_listener_composition_observe_answers_while_grants_is_pending,
    "response-listener-observe"
);
case!(
    rollback_listener_composition_join_is_refused_without_cancelling_accepted_leave,
    "response-listener-join"
);
case!(
    rollback_listener_composition_second_leave_preserves_original_cause_and_bound,
    "response-listener-second-leave"
);
case!(
    rollback_listener_composition_silent_dial_uses_original_accept_deadline,
    "response-listener-silent"
);
case!(
    rollback_listener_composition_dead_secondary_peer_does_not_end_rollback,
    "response-listener-peer-gone"
);

case!(
    rollback_listener_composition_enter_refusal_closes_both_unadopted_handles,
    "response-listener-enter-handles"
);
case!(
    rollback_listener_composition_pressure_does_not_hold_grants_or_lose_answer,
    "response-listener-backpressure"
);
case!(
    rollback_listener_composition_malformed_dial_does_not_end_accepted_leave,
    "response-listener-malformed"
);
case!(
    rollback_listener_composition_listener_loss_remains_a_listener_failure,
    "response-listener-loss"
);
case!(
    rollback_listener_composition_original_leave_expiry_still_stops_work,
    "response-listener-leave-expiry"
);
#[test]
fn rollback_listener_composition_authenticates_actual_connector() {
    crate::flow::context::startup_composition::tests::refusal_join("response-listener-credentials");
}

case!(
    rollback_listener_composition_failed_post_send_clock_retains_sent_fact_and_first_halt,
    "response-listener-send-clock"
);
case!(
    rollback_listener_composition_clock_reversal_keeps_accepted_listener_custody,
    "response-listener-clock-reversed"
);

case!(
    rollback_listener_composition_original_connection_extra_frame_is_held_without_new_authority,
    "response-listener-original-extra"
);
