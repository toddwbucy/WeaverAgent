use super::super::tests::{Member, dial, packet};
use super::*;
use crate::flow::clock::testing::FakePumpIo;
use crate::flow::context::startup_composition::tests::CircuitIo;
use nix::sys::socket::{MsgFlags, send};
use std::io::Read;
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};
use weaver_types::{
    Cause, ExchangeId, LifecycleAnswer, LifecycleDirective, Opener, OrganEnvelope, Payload,
    Position,
};

// All inputs are actual ends of this one circuit, retained by its parent fixture.
#[allow(clippy::too_many_arguments)]
pub(in crate::flow::context::worker_startup) fn exercise(
    enter: FoundationEnter<'_>,
    original: &OwnedFd,
    member: &mut Member,
    reader: &mut UnixStream,
    clock: &mut FakePumpIo,
    path: &Path,
    envelope: &weaver_types::state_flow::enter::EnterEnvelope,
    mode: &str,
) {
    let mut owner = enter
        .rollback_refusal()
        .ok()
        .expect("actual classifier refusal");
    assert_eq!(owner.process_exit(), None);
    let _panic_reader = PanicReader(reader.try_clone().unwrap());
    let limit = Instant::now() + Duration::from_secs(8);
    let mut enter_lost = false;
    while owner.progress() != FoundationRollbackProgress::AwaitRollback {
        assert!(
            Instant::now() < limit,
            "refusal response: {:?}",
            owner.detail()
        );
        if mode == "rollback-enter-lost"
            && !enter_lost
            && matches!(owner.state, Some(RollbackState::Response(_)))
        {
            nix::sys::socket::shutdown(original.as_raw_fd(), nix::sys::socket::Shutdown::Both)
                .unwrap();
            enter_lost = true;
        }
        assert_ne!(
            owner.advance_with_io(&mut CircuitIo(clock)),
            FoundationRollbackProgress::Stopped,
            "{:?}",
            owner.detail()
        );
        member.step(clock.now().unwrap(), mode);
    }
    if !enter_lost {
        let answer = packet(original).expect("actual recorded refusal reaches its original peer");
        assert_eq!(answer.exchange, envelope.exchange);
        assert_eq!(answer.position, Position::Close);
        assert!(matches!(
            answer.payload,
            Payload::Refusal(weaver_types::LifecycleRefusal::StateManagementInvalid { .. })
        ));
    }
    assert!(owner.refusal_sequence().is_some());
    assert_eq!(member.requests, ["open", "grants"]);
    if mode == "rollback-late" {
        clock
            .advance_to(owner.enter_window().deadline_ns() + 1)
            .unwrap();
    }
    if mode == "rollback-secondary" {
        for directive in [
            LifecycleDirective::Observe,
            LifecycleDirective::Leave {
                cause: Cause { uid: 10 },
                forced: true,
                rollback: false,
            },
        ] {
            let peer = dial(path);
            let request = OrganEnvelope {
                exchange: ExchangeId {
                    opener: Opener::Admin,
                    ordinal: 71,
                },
                position: Position::Open,
                payload: Payload::Directive(directive),
            };
            send(
                peer.as_raw_fd(),
                &serde_json::to_vec(&request).unwrap(),
                MsgFlags::MSG_NOSIGNAL,
            )
            .unwrap();
            loop {
                assert!(Instant::now() < limit);
                assert_ne!(
                    owner.advance_with_io(&mut CircuitIo(clock)),
                    FoundationRollbackProgress::Stopped
                );
                if let Some(reply) = packet(&peer) {
                    assert_eq!(reply.exchange, request.exchange);
                    match request.payload {
                        Payload::Directive(LifecycleDirective::Observe) => assert!(matches!(
                            reply.payload,
                            Payload::Answer(LifecycleAnswer::InTransition { .. })
                        )),
                        _ => assert!(matches!(
                            reply.payload,
                            Payload::Refusal(weaver_types::LifecycleRefusal::OutOfOrder)
                        )),
                    }
                    break;
                }
            }
        }
        let silent = dial(path);
        owner.advance_with_io(&mut CircuitIo(clock));
        let bound = owner.dial.as_ref().unwrap().bound;
        clock.advance_to(bound.deadline_ns()).unwrap();
        assert_eq!(
            owner.advance_with_io(&mut CircuitIo(clock)),
            FoundationRollbackProgress::AwaitRollback
        );
        assert!(
            owner.dial.is_none(),
            "silent peer cannot own a new read window"
        );
        drop(silent);
    }
    if mode == "rollback-await-eof" {
        member.stop_responses();
        assert_eq!(
            owner.advance_with_io(&mut CircuitIo(clock)),
            FoundationRollbackProgress::Stopped
        );
        assert!(owner.leave_window().is_none());
        assert!(owner.unload_sequence().is_none());
        return;
    }
    let peer = dial(path);
    let leave = OrganEnvelope {
        exchange: ExchangeId {
            opener: Opener::Admin,
            ordinal: 93,
        },
        position: Position::Open,
        payload: Payload::Directive(LifecycleDirective::Leave {
            cause: Cause { uid: 987 },
            forced: false,
            rollback: true,
        }),
    };
    let bytes = serde_json::to_vec(&leave).unwrap();
    assert_eq!(
        send(peer.as_raw_fd(), &bytes, MsgFlags::MSG_NOSIGNAL),
        Ok(bytes.len())
    );
    let _silent = (mode == "rollback-seal-silent").then(|| dial(path));
    let mut secondary = Vec::new();
    let mut trace = Vec::new();
    reader.set_nonblocking(true).unwrap();
    if mode == "rollback-worker" {
        let done = std::sync::atomic::AtomicBool::new(false);
        owner = std::thread::scope(|scope| {
            scope.spawn(|| {
                while !done.load(std::sync::atomic::Ordering::SeqCst) && Instant::now() < limit {
                    member.step_live(mode);
                    let _ = reader.read_to_end(&mut trace);
                    std::thread::sleep(Duration::from_millis(1));
                }
            });
            let owner = super::super::tests::worker::rollback(owner);
            done.store(true, std::sync::atomic::Ordering::SeqCst);
            owner
        });
        assert_eq!(
            owner.progress(),
            FoundationRollbackProgress::Closed,
            "{:?}",
            owner.detail()
        );
    }
    let mut expired = false;
    let mut injected = false;
    let mut pressure = 0;
    if mode == "rollback-leave-lost" {
        nix::sys::socket::shutdown(peer.as_raw_fd(), nix::sys::socket::Shutdown::Read).unwrap();
    }
    loop {
        assert!(Instant::now() < limit, "rollback: {:?}", owner.detail());
        if mode == "rollback-expired"
            && !expired
            && let Some(bound) = owner.leave_window()
        {
            clock.advance_to(bound.deadline_ns()).unwrap();
            expired = true;
        }
        if let Some(RollbackState::Terminal(terminal)) = &mut owner.state {
            if matches!(
                terminal.stage(),
                TerminalStage::State
                    | TerminalStage::Unload
                    | TerminalStage::Drain
                    | TerminalStage::Answer
            ) {
                assert!(!path.exists(), "seal precedes member release and Unload");
            }
            if mode == "rollback-seal-silent"
                && !injected
                && terminal.stage() == TerminalStage::Seal
            {
                let bound = terminal
                    .test_secondary_bound()
                    .expect("actual secondary read retained into seal");
                clock.advance_to(bound.deadline_ns()).unwrap();
                injected = true;
            }
            if mode == "rollback-seal-backlog"
                && !injected
                && terminal.stage() == TerminalStage::Seal
            {
                for (index, directive) in [
                    LifecycleDirective::Observe,
                    LifecycleDirective::JoinLeave {
                        cause: Cause { uid: 3 },
                    },
                ]
                .into_iter()
                .enumerate()
                {
                    let peer = dial(path);
                    let request = OrganEnvelope {
                        exchange: ExchangeId {
                            opener: Opener::Admin,
                            ordinal: 100 + index as u64,
                        },
                        position: Position::Open,
                        payload: Payload::Directive(directive),
                    };
                    send(
                        peer.as_raw_fd(),
                        &serde_json::to_vec(&request).unwrap(),
                        MsgFlags::MSG_NOSIGNAL,
                    )
                    .unwrap();
                    secondary.push((peer, request));
                }
                injected = true;
            }
            if mode == "rollback-left-expired"
                && !injected
                && terminal.stage() == TerminalStage::Answer
            {
                clock.advance_to(terminal.leave().deadline_ns()).unwrap();
                injected = true;
            }
            if mode == "rollback-drain-expired"
                && !injected
                && terminal.stage() == TerminalStage::Unload
            {
                use crate::flow::context::AuthoredEvent;
                let context = terminal.test_context();
                let event = AuthoredEvent {
                    kind: weaver_trace::serving::Kind::Fault,
                    subsystem: weaver_trace::Subsystem::Harness,
                    turn: None,
                    causal_parent: None,
                    source: crate::flow::author::AuthoredSource::Runtime,
                    payload: Some(weaver_trace::serving::Payload::Fault(
                        serde_json::value::to_raw_value(
                            &serde_json::json!({"fixture": "x".repeat(2_000_000)}),
                        )
                        .unwrap(),
                    )),
                };
                context
                    .author_and_wait(
                        &mut crate::flow::pump::PumpPorts {
                            lifecycle: LifecyclePorts::default(),
                            gate: None,
                            decode: None,
                            label: None,
                            io: &mut CircuitIo(clock),
                        },
                        event,
                    )
                    .unwrap();
                injected = true;
            }
            if mode == "rollback-observer" && !injected && terminal.stage() == TerminalStage::Unload
            {
                terminal
                    .test_context()
                    .barrier
                    .fail_next_recorded_observer_for_test();
                injected = true;
            }
            if mode == "rollback-send-pressure"
                && !injected
                && terminal.stage() == TerminalStage::Answer
            {
                let bytes = vec![b'x'; 16_384];
                loop {
                    match send(
                        terminal.test_peer().fd().as_raw_fd(),
                        &bytes,
                        MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_NOSIGNAL,
                    ) {
                        Ok(n) => {
                            assert_eq!(n, bytes.len());
                            pressure += 1;
                        }
                        Err(nix::errno::Errno::EAGAIN) => break,
                        other => panic!("fill Left transport: {other:?}"),
                    }
                }
                assert!(pressure > 0);
                injected = true;
            }
        }
        let progress = if mode == "rollback-drain-expired"
            && matches!(&owner.state, Some(RollbackState::Terminal(terminal)) if terminal.stage() == TerminalStage::Drain)
        {
            owner.advance_with_io(&mut ExpireWait {
                clock,
                at: owner.leave.unwrap().deadline_ns(),
            })
        } else {
            owner.advance_with_io(&mut CircuitIo(clock))
        };
        if pressure > 0 {
            assert_ne!(
                progress,
                FoundationRollbackProgress::Closed,
                "staged Left cannot complete"
            );
            assert!(owner.unload_sequence().is_some());
            let mut bytes = vec![0; 16_384];
            for _ in 0..pressure {
                assert_eq!(
                    nix::sys::socket::recv(peer.as_raw_fd(), &mut bytes, MsgFlags::MSG_DONTWAIT)
                        .unwrap(),
                    bytes.len()
                );
            }
            pressure = 0;
        }
        if mode != "rollback-drain-expired" || !injected {
            let _ = reader.read_to_end(&mut trace);
        }
        member.step(clock.now().unwrap(), mode);
        if mode == "rollback-expired" && progress == FoundationRollbackProgress::Stopped {
            assert_eq!(owner.process_exit(), None);
            assert!(expired);
            assert!(
                packet(&peer).is_none(),
                "expired rollback cannot answer Left"
            );
            assert!(owner.unload_sequence().is_none());
            assert_eq!(owner.advance_with_io(&mut CircuitIo(clock)), progress);
            return;
        }
        if matches!(
            mode,
            "rollback-observer"
                | "rollback-leave-lost"
                | "rollback-left-expired"
                | "rollback-drain-expired"
        ) && progress == FoundationRollbackProgress::Stopped
        {
            assert_eq!(
                owner.process_exit(),
                None,
                "Recorded Unload is not completion"
            );
            assert!(
                owner.unload_sequence().is_some(),
                "retain actual Unload after a later failure"
            );
            assert!(packet(&peer).is_none(), "failure cannot answer Left");
            assert_eq!(owner.advance_with_io(&mut CircuitIo(clock)), progress);
            member.assert_closed_without_request();
            return;
        }
        assert_ne!(
            progress,
            FoundationRollbackProgress::Stopped,
            "{:?}",
            owner.detail()
        );
        if progress == FoundationRollbackProgress::Closed {
            assert_ne!(
                mode, "rollback-drain-expired",
                "a blocked writer cannot grant Left"
            );
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    for (peer, request) in secondary {
        let reply = packet(&peer).expect("sealed backlog answered before organ release");
        assert_eq!(reply.exchange, request.exchange);
        assert!(matches!(
            reply.payload,
            Payload::Refusal(weaver_types::LifecycleRefusal::OutOfOrder)
        ));
    }
    let reply = packet(&peer).expect("Left after actual drain");
    assert_eq!(
        owner.process_exit(),
        Some(crate::FoundationProcessExit::Success)
    );
    assert_eq!(reply.exchange, leave.exchange);
    assert!(matches!(
        reply.payload,
        Payload::Answer(LifecycleAnswer::Left {
            save_point: None,
            forced: true,
            no_state: false
        })
    ));
    assert!(packet(original).is_none(), "no second Enter answer");
    assert_eq!(member.requests, ["open", "grants", "grants"]);
    member.assert_closed_without_request();
    let through = weaver_trace::Sequence(owner.unload_sequence().unwrap());
    let rows = super::super::tests::trace_through(reader, trace, through, limit).unwrap();
    assert_eq!(rows.iter().filter(|r| r["kind"] == "refusal").count(), 1);
    assert_eq!(rows.iter().filter(|r| r["kind"] == "unload").count(), 1);
    let unload = rows.last().unwrap();
    assert_eq!(unload["kind"], "unload");
    assert_eq!(
        unload["payload"]["cause"]["uid"],
        envelope.enter().cause.uid
    );
    assert_eq!(unload["payload"]["forced"], true);
    assert_eq!(
        unload["payload"]["grant_surface"],
        if mode == "rollback-varied" {
            "varied"
        } else {
            "unchanged"
        }
    );
    assert!(!path.exists());
    assert!(unload["payload"]["release"].get("spu").is_none());
    assert_eq!(unload["payload"]["release"]["member"], "closed");
    assert_eq!(
        owner.advance_with_io(&mut CircuitIo(clock)),
        FoundationRollbackProgress::Closed
    );
    assert!(packet(&peer).is_none(), "Left is not resent");
}

struct ExpireWait<'a> {
    clock: &'a mut FakePumpIo,
    at: u64,
}
impl PumpIo for ExpireWait<'_> {
    fn now(&mut self) -> std::io::Result<u64> {
        self.clock.now()
    }
    fn wait(
        &mut self,
        _interests: &[crate::flow::clock::WaitInterest<'_>],
        deadline: Option<u64>,
    ) -> std::io::Result<crate::flow::clock::WaitOutcome> {
        assert_eq!(deadline, Some(self.at));
        self.clock.advance_to(self.at)?;
        Ok(crate::flow::clock::WaitOutcome::Timeout)
    }
}

// Close the real blocked sink before an assertion unwinds into the parent run's
// recorder destructor. A failed control must report failure rather than hang.
struct PanicReader(UnixStream);
impl Drop for PanicReader {
    fn drop(&mut self) {
        if std::thread::panicking() {
            let _ = self.0.shutdown(std::net::Shutdown::Both);
        }
    }
}
