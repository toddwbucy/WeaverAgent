use super::*;
use nix::sys::socket::{MsgFlags, recv, send};
use std::os::fd::AsRawFd;

fn exchange(ordinal: u64) -> ExchangeId {
    ExchangeId {
        opener: Opener::Admin,
        ordinal,
    }
}

fn full(fd: BorrowedFd<'_>) -> usize {
    let packet = [b'x'; 16_384];
    let mut count = 0;
    loop {
        match send(
            fd.as_raw_fd(),
            &packet,
            MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_NOSIGNAL,
        ) {
            Ok(n) => {
                assert_eq!(n, packet.len());
                count += 1;
            }
            Err(nix::errno::Errno::EAGAIN) => return count,
            other => panic!("fill the actual transport: {other:?}"),
        }
    }
}

fn fixture(name: &str, f: impl FnOnce(&mut Setup<'_>, &OrganChannel)) {
    let root = crate::scratch::dir(format!("diagnostic-replay-{name}-{}", std::process::id()));
    let listener = crate::channel::bind_coordination(&root.join("admin.sock")).unwrap();
    let (near, far) = OrganChannel::pair().unwrap();
    let far = far.into_channel();
    let mut owner = Setup::new(&listener, &near, exchange(1)).unwrap();
    owner.ready = true;
    owner.enter_selected = true;
    owner.enter_done = true;
    owner.phase = Phase::Replay;
    owner.replay = Some(Control::new(None));
    f(&mut owner, &far);
}

#[test]
fn diagnostic_replay_stop_abandons_unsent_request_under_actual_send_pressure() {
    fixture("unsent", |owner, far| {
        let (decode, spu) = DecodeChannel::pair().unwrap();
        let spu = spu.into_channel();
        let count = full(decode.as_fd());
        assert!(count > 0);
        owner.replay.as_mut().unwrap().turn = Some(TurnKey("t".into()));
        far.send(&OrganEnvelope {
            exchange: exchange(2),
            position: Position::Open,
            payload: weaver_types::Payload::Directive(LifecycleDirective::Stop {
                cause: weaver_types::Cause { uid: 41 },
            }),
        })
        .unwrap();
        let began = Instant::now();
        assert!(
            !owner
                .replay_wait(
                    &decode,
                    decode.as_fd(),
                    PollFlags::POLLOUT,
                    Some(began + Duration::from_secs(1)),
                    Abandon::Unsent
                )
                .unwrap()
        );
        assert!(began.elapsed() < Duration::from_millis(500));
        let control = owner.replay.as_ref().unwrap();
        assert!(control.cancel_deadline.is_some());
        assert!(!control.active && !control.cancel_sent);
        assert_eq!(control.stop.as_ref().unwrap().exchange, exchange(2));
        let mut bytes = [0; 16_384];
        for _ in 0..count {
            assert_eq!(
                recv(spu.as_fd().as_raw_fd(), &mut bytes, MsgFlags::MSG_DONTWAIT).unwrap(),
                bytes.len()
            );
            assert!(bytes.iter().all(|&b| b == b'x'));
        }
        assert_eq!(
            recv(spu.as_fd().as_raw_fd(), &mut bytes, MsgFlags::MSG_DONTWAIT),
            Err(nix::errno::Errno::EAGAIN),
            "no request or invented Cancel"
        );
    });
}

#[test]
fn diagnostic_replay_graceful_drain_and_join_keep_their_original_intervals() {
    fixture("join", |owner, _| {
        owner.replay.as_mut().unwrap().turn = Some(TurnKey("t".into()));
        owner
            .replay_directive(
                0,
                exchange(2),
                LifecycleDirective::Leave {
                    cause: weaver_types::Cause { uid: 41 },
                    forced: false,
                    rollback: false,
                },
            )
            .unwrap();
        let control = owner.replay.as_ref().unwrap();
        assert!(control.interrupted);
        assert!(control.cancel_deadline.is_none() && control.drain_deadline.is_none());
        assert!(owner.leaving.as_ref().unwrap().deadline.is_none());
        owner
            .replay_directive(
                0,
                exchange(3),
                LifecycleDirective::JoinLeave {
                    cause: weaver_types::Cause { uid: 42 },
                },
            )
            .unwrap();
        let deadline = owner.replay.as_ref().unwrap().cancel_deadline.unwrap();
        owner
            .replay_directive(
                0,
                exchange(4),
                LifecycleDirective::JoinLeave {
                    cause: weaver_types::Cause { uid: 43 },
                },
            )
            .unwrap();
        assert_eq!(
            owner.replay.as_ref().unwrap().cancel_deadline,
            Some(deadline)
        );
        let leave = owner.leaving.as_ref().unwrap();
        assert!(leave.forced);
        assert_eq!(leave.exchange, exchange(2));
        assert_eq!(leave.cause, weaver_types::Cause { uid: 41 });
        assert_eq!(leave.joins.len(), 2);
    });
}

#[test]
fn diagnostic_replay_settled_decode_still_wakes_a_blocked_record_wait() {
    fixture("duplicate", |owner, _| {
        let (decode, spu) = DecodeChannel::pair().unwrap();
        let spu = spu.into_channel();
        let (sink, _reader) = OrganChannel::pair().unwrap();
        assert!(full(sink.as_fd()) > 0);
        let control = owner.replay.as_mut().unwrap();
        control.turn = Some(TurnKey("t".into()));
        control.decode_settled = true;
        control.cancel_sent = true;
        let bytes = serde_json::to_vec(&TokenAnswer::AtRest).unwrap();
        assert!(crate::channel::managed::try_send(spu.as_fd(), &bytes).unwrap());
        let began = Instant::now();
        assert!(matches!(
            owner.replay_wait(
                &decode,
                sink.as_fd(),
                PollFlags::POLLOUT,
                Some(began + Duration::from_secs(1)),
                Abandon::Never
            ),
            Err(ChannelFault::Undecodable)
        ));
        assert!(began.elapsed() < Duration::from_millis(500));
    });
}

#[test]
fn diagnostic_replay_idle_report_keeps_the_original_preload_deadline() {
    use std::io::Write;
    use std::os::unix::net::UnixStream;
    fixture("report-bound", |owner, _| {
        let (decode, spu) = DecodeChannel::pair().unwrap();
        let (mut sink, _reader) = UnixStream::pair().unwrap();
        sink.set_nonblocking(true).unwrap();
        loop {
            match sink.write(&[b'x'; 16_384]) {
                Ok(n) => assert!(n > 0),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                other => panic!("fill the real record sink: {other:?}"),
            }
        }
        let mut recorder = crate::record::Record::from(
            weaver_diagnostic::Recorder::receive(
                sink.into(),
                weaver_diagnostic::RunRef("r".into()),
                weaver_diagnostic::SessionRef("s".into()),
            )
            .unwrap(),
        );
        let author = Author::new(&SessionId("s".into()), &weaver_types::RunId("r".into()));
        let (member, _peer) = UnixStream::pair().unwrap();
        let mut state = crate::state::StateSeam::new(member);
        let control = owner.replay.as_mut().unwrap();
        control.opened = true;
        // An independent outer cap makes a missing parked-ask cap fail rather
        // than leave this instrument blocked forever.
        control.failure_deadline = Some(Instant::now() + Duration::from_secs(1));
        let report = TokenAnswer::Fault(crate::authorship::harness_report(
            weaver_types::FaultCase::ResidencyDegraded,
            "{\"pending\":true}",
        ));
        let spu = spu.into_channel();
        assert!(
            crate::channel::managed::try_send(spu.as_fd(), &serde_json::to_vec(&report).unwrap())
                .unwrap()
        );
        let mut ports = ReplayPorts {
            management: owner,
            decode: &decode,
            author: &author,
            recorder: &mut recorder,
            state: Some(&mut state),
            failed: false,
        };
        let began = Instant::now();
        assert!(ports.replay(60).is_none());
        assert!(began.elapsed() >= Duration::from_millis(50));
        assert!(began.elapsed() < Duration::from_millis(800));
        assert!(ports.failed && ports.recorder.failed());
        assert!(ports.management.replay.as_ref().unwrap().turn.is_none());
    });
}

#[test]
fn diagnostic_replay_completed_left_survives_unread_bystander_deadline() {
    fixture("left", |owner, _| {
        assert!(full(owner.peers[0].channel.as_ref().unwrap().as_fd()) > 0);
        let deadline = Instant::now() + Duration::from_millis(50);
        owner.leaving = Some(Leaving {
            peer: 0,
            exchange: exchange(2),
            cause: weaver_types::Cause { uid: 41 },
            forced: true,
            rollback: false,
            deadline: Some(deadline),
            joinable: false,
            joins: Vec::new(),
        });
        owner.phase = Phase::Cleanup;
        owner.queue_left().unwrap();
        owner.deliver_left().unwrap();
        assert!(Instant::now() >= deadline);
        assert_eq!(
            owner.left,
            Some(LifecycleAnswer::Left {
                save_point: None,
                forced: true,
                no_state: true,
            })
        );
        assert_eq!(owner.leaving.as_ref().unwrap().deadline, Some(deadline));
        assert!(owner.peers[0].channel.is_none());
    });
}

#[test]
fn diagnostic_replay_retains_the_exact_interrupted_terminal_until_recorded_close() {
    use std::os::unix::net::UnixStream;
    fixture("terminal-custody", |owner, far| {
        let turn = TurnKey("original-turn".into());
        owner.replay.as_mut().unwrap().turn = Some(turn.clone());
        let (decode, spu) = DecodeChannel::pair().unwrap();
        let spu = spu.into_channel();
        let (sink, _reader) = UnixStream::pair().unwrap();
        let mut recorder = crate::record::Record::from(
            weaver_diagnostic::Recorder::receive(
                sink.into(),
                weaver_diagnostic::RunRef("r".into()),
                weaver_diagnostic::SessionRef("s".into()),
            )
            .unwrap(),
        );
        let author = Author::new(&SessionId("s".into()), &weaver_types::RunId("r".into()));
        let (done, wait_done) = std::sync::mpsc::channel();
        std::thread::scope(|scope| {
            let expected_turn = turn.clone();
            let peer = scope.spawn(move || {
                let take = || {
                    let mut fds = [PollFd::new(spu.as_fd(), PollFlags::POLLIN)];
                    poll(&mut fds, 2000_u16).unwrap();
                    assert!(fds[0].revents().unwrap().contains(PollFlags::POLLIN));
                    let mut bytes = vec![0; 4096];
                    let n =
                        recv(spu.as_fd().as_raw_fd(), &mut bytes, MsgFlags::MSG_DONTWAIT).unwrap();
                    serde_json::from_slice::<TokenDirective>(&bytes[..n]).unwrap()
                };
                assert!(matches!(take(), TokenDirective::ReFeed { .. }));
                far.send(&OrganEnvelope {
                    exchange: exchange(2),
                    position: Position::Open,
                    payload: weaver_types::Payload::Directive(LifecycleDirective::Stop {
                        cause: weaver_types::Cause { uid: 41 },
                    }),
                })
                .unwrap();
                assert_eq!(
                    take(),
                    TokenDirective::Cancel {
                        turn: expected_turn.clone()
                    }
                );
                for answer in [
                    TokenAnswer::RefeedInterrupted {
                        turn: expected_turn.clone(),
                        consumed: 1,
                        resident: 7,
                        capacity: 64,
                    },
                    TokenAnswer::AtRest,
                ] {
                    assert!(
                        crate::channel::managed::try_send(
                            spu.as_fd(),
                            &serde_json::to_vec(&answer).unwrap()
                        )
                        .unwrap()
                    );
                }
                wait_done.recv_timeout(Duration::from_secs(2)).unwrap();
            });
            let mut ports = ReplayPorts {
                management: owner,
                decode: &decode,
                author: &author,
                recorder: &mut recorder,
                state: None,
                failed: false,
            };
            assert!(
                ports
                    .refeed(&turn, "rendered".into(), vec![3, 4, 5])
                    .unwrap()
                    .is_none()
            );
            assert_eq!(
                ports.management.replay.as_ref().unwrap().terminal,
                Some(TokenAnswer::RefeedInterrupted {
                    turn: turn.clone(),
                    consumed: 1,
                    resident: 7,
                    capacity: 64,
                })
            );
            assert!(ports.management.replay.as_ref().unwrap().stop.is_some());
            ports.close(&turn, TurnClose::Clean).unwrap();
            assert!(ports.management.replay.as_ref().unwrap().terminal.is_none());
            assert!(ports.management.replay.as_ref().unwrap().turn.is_none());
            done.send(()).unwrap();
            peer.join().unwrap();
        });
    });
}

fn take_directive(spu: &OrganChannel) -> TokenDirective {
    let mut fds = [PollFd::new(spu.as_fd(), PollFlags::POLLIN)];
    poll(&mut fds, 2000_u16).unwrap();
    assert!(fds[0].revents().unwrap().contains(PollFlags::POLLIN));
    let mut bytes = vec![0; 4096];
    let n = recv(spu.as_fd().as_raw_fd(), &mut bytes, MsgFlags::MSG_DONTWAIT).unwrap();
    serde_json::from_slice::<TokenDirective>(&bytes[..n]).unwrap()
}

fn spu_sends(spu: &OrganChannel, frame: Vec<u8>) {
    assert!(crate::channel::managed::try_send(spu.as_fd(), &frame).unwrap());
}

fn stop(far: &OrganChannel, ordinal: u64) {
    far.send(&OrganEnvelope {
        exchange: exchange(ordinal),
        position: Position::Open,
        payload: weaver_types::Payload::Directive(LifecycleDirective::Stop {
            cause: weaver_types::Cause { uid: 41 },
        }),
    })
    .unwrap();
}

fn recorder(sink: std::os::unix::net::UnixStream) -> crate::record::Record {
    crate::record::Record::from(
        weaver_diagnostic::Recorder::receive(
            sink.into(),
            weaver_diagnostic::RunRef("r".into()),
            weaver_diagnostic::SessionRef("s".into()),
        )
        .unwrap(),
    )
}

fn delivered(owner: &mut Setup<'_>, far: &OrganChannel) -> OrganEnvelope {
    owner.checkpoint().unwrap();
    let mut fds = [PollFd::new(far.as_fd(), PollFlags::POLLIN)];
    poll(&mut fds, 2000_u16).unwrap();
    assert!(fds[0].revents().unwrap().contains(PollFlags::POLLIN));
    far.recv().unwrap()
}

/// A ReFeed refusal that crosses an already-sent Cancel is the ReFeed's owed
/// terminal: it is recorded, the Cancel's AtRest is drained, and the Stop is
/// settled rather than the run ending as a protocol failure.
#[test]
fn diagnostic_replay_refusal_crossing_a_sent_cancel_settles_the_stop() {
    use std::os::unix::net::UnixStream;
    fixture("refusal-race", |owner, far| {
        let turn = TurnKey("original-turn".into());
        owner.replay.as_mut().unwrap().turn = Some(turn.clone());
        let (decode, spu) = DecodeChannel::pair().unwrap();
        let spu = spu.into_channel();
        let (sink, _reader) = UnixStream::pair().unwrap();
        let mut recorder = recorder(sink);
        let author = Author::new(&SessionId("s".into()), &weaver_types::RunId("r".into()));
        let overflow = weaver_types::TokenRefusal::Overflow {
            resident: 60,
            requested: 9,
            capacity: 64,
        };
        std::thread::scope(|scope| {
            let expected = turn.clone();
            let refusal = overflow.clone();
            let peer = scope.spawn(move || {
                assert!(matches!(
                    take_directive(&spu),
                    TokenDirective::ReFeed { .. }
                ));
                stop(far, 2);
                assert_eq!(
                    take_directive(&spu),
                    TokenDirective::Cancel { turn: expected }
                );
                // The real SPU refuses before its cancel poll, then answers the
                // Cancel at rest: the refusal is already written when it hears it.
                spu_sends(&spu, serde_json::to_vec(&refusal).unwrap());
                spu_sends(&spu, serde_json::to_vec(&TokenAnswer::AtRest).unwrap());
                spu
            });
            let mut ports = ReplayPorts {
                management: owner,
                decode: &decode,
                author: &author,
                recorder: &mut recorder,
                state: None,
                failed: false,
            };
            let refused = ports.refeed(&turn, "rendered".into(), vec![3, 4, 5]);
            let spu = peer.join().unwrap();
            match refused {
                Err(TurnError::Refused {
                    turn: named,
                    refusal,
                }) => {
                    assert_eq!(named, turn);
                    assert_eq!(refusal, overflow);
                }
                other => panic!("the refusal is the ReFeed's terminal, got {other:?}"),
            }
            assert!(!ports.failed);
            let control = ports.management.replay.as_ref().unwrap();
            assert!(control.decode_settled && !control.decode_busy && !control.active);
            assert!(control.stop.is_some());
            let mut bytes = [0; 64];
            assert_eq!(
                recv(spu.as_fd().as_raw_fd(), &mut bytes, MsgFlags::MSG_DONTWAIT),
                Err(nix::errno::Errno::EAGAIN),
                "the harness sent nothing after the Cancel"
            );
            assert_eq!(
                recv(
                    decode.as_fd().as_raw_fd(),
                    &mut bytes,
                    MsgFlags::MSG_DONTWAIT
                ),
                Err(nix::errno::Errno::EAGAIN),
                "the Cancel's AtRest was drained"
            );
            ports
                .close(
                    &turn,
                    TurnClose::Stopped {
                        reason: StopReason::Refused,
                        cause: None,
                    },
                )
                .unwrap();
            let answer = delivered(ports.management, far);
            assert_eq!(answer.exchange, exchange(2));
            assert_eq!(
                answer.payload,
                weaver_types::Payload::Answer(LifecycleAnswer::TurnAborted { turn })
            );
        });
    });
}

/// An SPU Fault on a ReFeed with no Cancel sent ends the pass, not the run:
/// the caller closes the bracket stopped and the run stands for Leave, and the
/// bound taken for the fault's record does not outlive it.
#[test]
fn diagnostic_replay_uninterrupted_fault_stands_for_leave() {
    use std::os::unix::net::UnixStream;
    fixture("fault", |owner, _| {
        let turn = TurnKey("original-turn".into());
        owner.replay.as_mut().unwrap().turn = Some(turn.clone());
        let (decode, spu) = DecodeChannel::pair().unwrap();
        let spu = spu.into_channel();
        let (sink, _reader) = UnixStream::pair().unwrap();
        let mut recorder = recorder(sink);
        let author = Author::new(&SessionId("s".into()), &weaver_types::RunId("r".into()));
        let report = crate::authorship::harness_report(
            weaver_types::FaultCase::DeviceFaultDuringGeneration,
            "{\"refeed\":true}",
        );
        std::thread::scope(|scope| {
            let sent = report.clone();
            let peer = scope.spawn(move || {
                assert!(matches!(
                    take_directive(&spu),
                    TokenDirective::ReFeed { .. }
                ));
                spu_sends(&spu, serde_json::to_vec(&TokenAnswer::Fault(sent)).unwrap());
            });
            let mut ports = ReplayPorts {
                management: owner,
                decode: &decode,
                author: &author,
                recorder: &mut recorder,
                state: None,
                failed: false,
            };
            let faulted = ports.refeed(&turn, "rendered".into(), vec![3, 4, 5]);
            peer.join().unwrap();
            match faulted {
                Err(TurnError::Faulted {
                    turn: named,
                    report: carried,
                }) => {
                    assert_eq!(named, turn);
                    assert_eq!(carried, report);
                }
                other => panic!("an uninterrupted fault is Faulted, got {other:?}"),
            }
            assert!(!ports.failed);
            let control = ports.management.replay.as_ref().unwrap();
            assert!(!control.active && !control.decode_busy);
            assert!(
                control.failure_deadline.is_none(),
                "the fault's record bound must not cap the standing run"
            );
            ports
                .close(
                    &turn,
                    TurnClose::Stopped {
                        reason: StopReason::Fault,
                        cause: None,
                    },
                )
                .unwrap();
            assert!(ports.management.replay.as_ref().unwrap().turn.is_none());
        });
    });
}

/// A Stop heard while a clean turn.closed is still being written answers
/// AtRest after that close and leaves the pass running (Spec section 6.2).
#[test]
fn diagnostic_replay_stop_during_a_clean_close_keeps_the_pass() {
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;
    fixture("closing-stop", |owner, far| {
        let turn = TurnKey("original-turn".into());
        owner.replay.as_mut().unwrap().turn = Some(turn.clone());
        let (decode, _spu) = DecodeChannel::pair().unwrap();
        let (mut sink, mut reader) = UnixStream::pair().unwrap();
        sink.set_nonblocking(true).unwrap();
        loop {
            match sink.write(&[b'x'; 16_384]) {
                Ok(n) => assert!(n > 0),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                other => panic!("fill the real record sink: {other:?}"),
            }
        }
        let mut recorder = recorder(sink);
        let author = Author::new(&SessionId("s".into()), &weaver_types::RunId("r".into()));
        std::thread::scope(|scope| {
            let drain = scope.spawn(move || {
                // The close is blocked on the full sink when the Stop lands.
                stop(far, 2);
                std::thread::sleep(Duration::from_millis(100));
                reader
                    .set_read_timeout(Some(Duration::from_millis(300)))
                    .unwrap();
                let mut bytes = [0; 16_384];
                while reader.read(&mut bytes).is_ok_and(|n| n > 0) {}
            });
            let mut ports = ReplayPorts {
                management: owner,
                decode: &decode,
                author: &author,
                recorder: &mut recorder,
                state: None,
                failed: false,
            };
            ports.close(&turn, TurnClose::Clean).unwrap();
            drain.join().unwrap();
            let control = ports.management.replay.as_ref().unwrap();
            assert!(control.turn.is_none());
            assert!(
                !control.interrupted,
                "a Stop between turns does not cancel the pass"
            );
            assert!(control.cancel_deadline.is_none());
            assert!(
                control.stop.is_none(),
                "the Stop was answered after the close"
            );
            assert!(ports.checkpoint().unwrap(), "the pass continues");
            let answer = delivered(ports.management, far);
            assert_eq!(answer.exchange, exchange(2));
            assert_eq!(
                answer.payload,
                weaver_types::Payload::Answer(LifecycleAnswer::AtRest)
            );
        });
    });
}
