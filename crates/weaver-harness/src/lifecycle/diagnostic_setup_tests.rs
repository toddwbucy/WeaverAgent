use super::*;
use nix::sys::socket::{MsgFlags, recv, send};
use std::os::fd::AsRawFd;

fn bounded_reply(channel: &OrganChannel) -> OrganEnvelope {
    let mut fds = [PollFd::new(channel.as_fd(), PollFlags::POLLIN)];
    poll(&mut fds, 2000_u16).unwrap();
    assert!(
        fds[0]
            .revents()
            .unwrap_or(PollFlags::empty())
            .contains(PollFlags::POLLIN),
        "the attributed management reply must arrive within the fixture bound"
    );
    channel.recv().unwrap()
}

#[test]
fn diagnostic_setup_checkpoint_owns_queued_rollback_before_the_next_step() {
    for canonical in [false, true] {
        let root = crate::scratch::dir(format!(
            "diagnostic-checkpoint-{}-{canonical}",
            std::process::id()
        ));
        let listener = crate::channel::bind_coordination(&root.join("admin.sock")).unwrap();
        let (original, far) = OrganChannel::pair().unwrap();
        let original = if canonical {
            OrganChannel::diagnostic_from_owned(original.into_fd())
        } else {
            original
        };
        let far = far.into_channel();
        let enter = ExchangeId {
            opener: Opener::Admin,
            ordinal: 1,
        };
        let rollback = ExchangeId {
            opener: Opener::Admin,
            ordinal: 2,
        };
        let mut setup = Setup::new(&listener, &original, enter.clone()).unwrap();
        far.send(&OrganEnvelope {
            exchange: rollback.clone(),
            position: Position::Open,
            payload: weaver_types::Payload::Directive(LifecycleDirective::Leave {
                cause: weaver_types::Cause { uid: 43 },
                forced: true,
                rollback: true,
            }),
        })
        .unwrap();
        assert!(
            setup.checkpoint().is_err(),
            "accepting rollback must forbid the next setup step"
        );
        let leave = setup.leaving.as_ref().expect("rollback retained");
        assert_eq!(leave.exchange, rollback);
        assert_eq!(leave.cause, weaver_types::Cause { uid: 43 });
        assert!(leave.forced && leave.rollback);
        let deadline = leave.deadline;
        assert!(!setup.ready);
        assert!(setup.checkpoint().is_err());
        assert_eq!(setup.leaving.as_ref().unwrap().deadline, deadline);
        let answer = bounded_reply(&far);
        assert_eq!(answer.exchange, enter);
        assert_eq!(
            answer.payload,
            weaver_types::Payload::Refusal(LifecycleRefusal::NoResidency)
        );
    }
}

#[test]
fn diagnostic_setup_ready_handoff_waits_for_a_peer_accepted_in_the_last_poll() {
    let root = crate::scratch::dir(format!("diagnostic-unheard-{}", std::process::id()));
    let listener = crate::channel::bind_coordination(&root.join("admin.sock")).unwrap();
    let (original, far) = OrganChannel::pair().unwrap();
    let far = far.into_channel();
    let mut setup = Setup::new(
        &listener,
        &original,
        ExchangeId {
            opener: Opener::Admin,
            ordinal: 1,
        },
    )
    .unwrap();
    setup.enter_outcome(Ok(LifecycleAnswer::Ready)).unwrap();
    setup.checkpoint().unwrap();
    assert_eq!(
        bounded_reply(&far).payload,
        weaver_types::Payload::Answer(LifecycleAnswer::Ready)
    );
    let (accepted, client) = OrganChannel::pair().unwrap();
    let client = client.into_channel();
    setup.peers.push(Peer {
        channel: Some(accepted),
        replies: VecDeque::new(),
        first_request_deadline: None,
        first_request_pending: true,
    });
    assert!(
        !setup.replies_done(),
        "an accepted peer is not an answered peer"
    );
    client
        .send(&OrganEnvelope {
            exchange: ExchangeId {
                opener: Opener::Admin,
                ordinal: 2,
            },
            position: Position::Open,
            payload: weaver_types::Payload::Directive(LifecycleDirective::Observe),
        })
        .unwrap();
    setup.checkpoint().unwrap();
    let answer = bounded_reply(&client);
    assert_eq!(answer.exchange.ordinal, 2);
    assert!(matches!(
        answer.payload,
        weaver_types::Payload::Answer(LifecycleAnswer::State {
            state: weaver_types::AgentState::Idle,
            ..
        })
    ));
    assert!(setup.replies_done());
}

#[test]
#[ignore = "requires a root management peer"]
fn diagnostic_setup_listener_retains_request_accepted_as_ready_is_delivered() {
    use nix::sys::socket::{AddressFamily, SockFlag, SockType, UnixAddr, connect, socket};
    assert!(nix::unistd::geteuid().is_root());
    let root = crate::scratch::dir(format!("diagnostic-ready-listener-{}", std::process::id()));
    let address = root.join("admin.sock");
    let listener = crate::channel::bind_coordination(&address).unwrap();
    let (original, far) = OrganChannel::pair().unwrap();
    let far = far.into_channel();
    let mut setup = Setup::new(
        &listener,
        &original,
        ExchangeId {
            opener: Opener::Admin,
            ordinal: 1,
        },
    )
    .unwrap();
    setup.enter_outcome(Ok(LifecycleAnswer::Ready)).unwrap();
    let client = socket(
        AddressFamily::Unix,
        SockType::SeqPacket,
        SockFlag::SOCK_CLOEXEC,
        None,
    )
    .unwrap();
    connect(client.as_raw_fd(), &UnixAddr::new(&address).unwrap()).unwrap();
    let request = OrganEnvelope {
        exchange: ExchangeId {
            opener: Opener::Admin,
            ordinal: 2,
        },
        position: Position::Open,
        payload: weaver_types::Payload::Directive(LifecycleDirective::Leave {
            cause: weaver_types::Cause { uid: 72 },
            forced: true,
            rollback: true,
        }),
    };
    let bytes = serde_json::to_vec(&request).unwrap();
    assert_eq!(
        send(client.as_raw_fd(), &bytes, MsgFlags::MSG_NOSIGNAL).unwrap(),
        bytes.len()
    );
    // This poll's descriptor set predates accept. It can deliver Ready, but
    // cannot yet receive the newly accepted peer's already queued rollback.
    setup.poll(None, None, Some(Duration::ZERO)).unwrap();
    assert_eq!(
        bounded_reply(&far).payload,
        weaver_types::Payload::Answer(LifecycleAnswer::Ready)
    );
    assert_eq!(setup.peers.len(), 2, "the root peer must be accepted");
    assert!(setup.leaving.is_none());
    assert!(
        !setup.replies_done(),
        "the newly accepted request must survive handoff"
    );
    setup.poll(None, None, Some(Duration::ZERO)).unwrap();
    let leave = setup
        .leaving
        .as_ref()
        .expect("the accepted peer's rollback was heard");
    assert_eq!(leave.exchange, request.exchange);
    assert_eq!(leave.cause, weaver_types::Cause { uid: 72 });
    assert!(leave.forced && leave.rollback);
    assert!(setup.ready, "rollback cannot undo selected Ready");
}

#[test]
fn diagnostic_setup_refuses_force_without_rollback_and_join_during_rollback() {
    for canonical in [false, true] {
        let root = crate::scratch::dir(format!(
            "diagnostic-management-refusal-{}-{canonical}",
            std::process::id()
        ));
        let listener = crate::channel::bind_coordination(&root.join("admin.sock")).unwrap();
        let (original, far) = OrganChannel::pair().unwrap();
        let original = if canonical {
            OrganChannel::diagnostic_from_owned(original.into_fd())
        } else {
            original
        };
        let far = far.into_channel();
        let mut setup = Setup::new(
            &listener,
            &original,
            ExchangeId {
                opener: Opener::Admin,
                ordinal: 1,
            },
        )
        .unwrap();
        for (ordinal, directive) in [
            (
                2,
                LifecycleDirective::Leave {
                    cause: weaver_types::Cause { uid: 7 },
                    forced: true,
                    rollback: false,
                },
            ),
            (
                3,
                LifecycleDirective::Leave {
                    cause: weaver_types::Cause { uid: 8 },
                    forced: true,
                    rollback: true,
                },
            ),
            (
                4,
                LifecycleDirective::JoinLeave {
                    cause: weaver_types::Cause { uid: 9 },
                },
            ),
        ] {
            far.send(&OrganEnvelope {
                exchange: ExchangeId {
                    opener: Opener::Admin,
                    ordinal,
                },
                position: Position::Open,
                payload: weaver_types::Payload::Directive(directive),
            })
            .unwrap();
            let _ = setup.checkpoint();
            let answer = bounded_reply(&far);
            if ordinal == 3 {
                assert_eq!(answer.exchange.ordinal, 1);
                assert_eq!(
                    answer.payload,
                    weaver_types::Payload::Refusal(LifecycleRefusal::NoResidency)
                );
            } else {
                assert_eq!(answer.exchange.ordinal, ordinal);
                assert_eq!(
                    answer.payload,
                    weaver_types::Payload::Refusal(LifecycleRefusal::OutOfOrder)
                );
            }
        }
        let leave = setup.leaving.as_ref().unwrap();
        assert_eq!(leave.exchange.ordinal, 3);
        assert_eq!(leave.cause, weaver_types::Cause { uid: 8 });
        assert!(leave.joins.is_empty());
    }
}

#[test]
fn diagnostic_setup_unsent_admit_under_pressure_hears_rollback_without_sending() {
    for canonical in [false, true] {
        let root = crate::scratch::dir(format!(
            "diagnostic-unsent-{}-{canonical}",
            std::process::id()
        ));
        let listener = crate::channel::bind_coordination(&root.join("admin.sock")).unwrap();
        let (original, far) = OrganChannel::pair().unwrap();
        let original = if canonical {
            OrganChannel::diagnostic_from_owned(original.into_fd())
        } else {
            original
        };
        let far = far.into_channel();
        let exchange = ExchangeId {
            opener: Opener::Admin,
            ordinal: 4,
        };
        let mut setup = Setup::new(&listener, &original, exchange.clone()).unwrap();
        let (spu, receiver) = OrganChannel::pair().unwrap();
        let receiver = receiver.into_channel();
        let pressure = vec![b'x'; 16_384];
        let mut count = 0;
        loop {
            match send(
                spu.as_fd().as_raw_fd(),
                &pressure,
                MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_NOSIGNAL,
            ) {
                Ok(n) => {
                    assert_eq!(n, pressure.len());
                    count += 1;
                }
                Err(nix::errno::Errno::EAGAIN) => break,
                other => panic!("fill actual Admit transport: {other:?}"),
            }
        }
        assert!(count > 0);
        far.send(&OrganEnvelope {
            exchange: ExchangeId {
                opener: Opener::Admin,
                ordinal: 5,
            },
            position: Position::Open,
            payload: weaver_types::Payload::Directive(LifecycleDirective::Leave {
                cause: weaver_types::Cause { uid: 0 },
                forced: true,
                rollback: true,
            }),
        })
        .unwrap();
        let source = weaver_types::state_flow::enter::EnterEnvelope::decode(include_bytes!(
            "../../../weaver-types/tests/fixtures/state-enter.json"
        ))
        .unwrap();
        assert_eq!(
            setup.admit(&spu, 1, source.enter().spu_instruction.clone()),
            Err(LifecycleRefusal::NoResidency)
        );
        assert!(matches!(setup.admission, Admission::Unsent));
        assert!(setup.leaving.is_some());
        let mut bytes = vec![0; weaver_types::MAX_ENVELOPE_BYTES];
        for _ in 0..count {
            let n = recv(
                receiver.as_fd().as_raw_fd(),
                &mut bytes,
                MsgFlags::MSG_DONTWAIT,
            )
            .unwrap();
            assert_eq!(&bytes[..n], pressure.as_slice());
        }
        assert_eq!(
            recv(
                receiver.as_fd().as_raw_fd(),
                &mut bytes,
                MsgFlags::MSG_DONTWAIT
            ),
            Err(nix::errno::Errno::EAGAIN)
        );
        setup.poll(None, None, Some(Duration::ZERO)).unwrap();
        let n = recv(far.as_fd().as_raw_fd(), &mut bytes, MsgFlags::MSG_DONTWAIT).unwrap();
        let refusal: OrganEnvelope = serde_json::from_slice(&bytes[..n]).unwrap();
        assert_eq!(refusal.exchange, exchange);
        assert_eq!(
            refusal.payload,
            weaver_types::Payload::Refusal(LifecycleRefusal::NoResidency)
        );
        assert!(!setup.ready);
    }
}
