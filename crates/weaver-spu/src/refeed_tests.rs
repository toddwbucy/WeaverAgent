//! The production cancel poll, session drive and terminal sender on real
//! sockets. The backend records the exact resident sequence and can fail the
//! terminator. Loaded-child tests cover the actual artifact and dispatch.

use super::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use weaver_spu::channel::decode_from_owned;
use weaver_spu::decoder::backend::{Backend, FlushMechanism, TokenId};
use weaver_spu::decoder::session::{PositionedSinks, SamplerBuild, Session, Stopped};
use weaver_types::TurnKey;

#[derive(Default)]
struct Observed {
    decoded: Vec<(Vec<TokenId>, usize)>,
    fail_terminator: bool,
}

struct RecordingBackend(Rc<RefCell<Observed>>);
impl Backend for RecordingBackend {
    fn decode_at(&mut self, tokens: &[TokenId], position: usize) -> Result<(), DecodeFault> {
        let mut log = self.0.borrow_mut();
        log.decoded.push((tokens.to_vec(), position));
        if log.fail_terminator && tokens == [TokenId(0)] {
            Err(DecodeFault::Engine {
                detail: "terminator failed".into(),
            })
        } else {
            Ok(())
        }
    }
    fn distribution(&self) -> Result<&[f32], DecodeFault> {
        Ok(&[1.0; 16])
    }
    fn sample(&mut self) -> Result<TokenId, DecodeFault> {
        Ok(TokenId(12))
    }
    fn reseed(&mut self, _: u64, _: &[TokenId]) -> Result<(), DecodeFault> {
        Ok(())
    }
    fn truncate_to(&mut self, _: usize) -> Result<(), DecodeFault> {
        Ok(())
    }
    fn reestablish(&mut self) -> Result<(), DecodeFault> {
        Ok(())
    }
    fn take_columns(&mut self) -> Option<Vec<Vec<f32>>> {
        Some(vec![vec![2.0, 3.0]])
    }
    fn close(&mut self) {}
}

fn pair() -> (DecodeSocket, DecodeSocket) {
    use nix::sys::socket::{AddressFamily, SockFlag, SockType, setsockopt, socketpair, sockopt};
    use nix::sys::time::TimeVal;
    let (a, b) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .unwrap();
    for fd in [&a, &b] {
        setsockopt(fd, sockopt::ReceiveTimeout, &TimeVal::new(2, 0)).unwrap();
        setsockopt(fd, sockopt::SendTimeout, &TimeVal::new(2, 0)).unwrap();
    }
    (decode_from_owned(a), decode_from_owned(b))
}

fn send(socket: &DecodeSocket, directive: TokenDirective) {
    socket
        .send_octets(&serde_json::to_vec(&directive).unwrap())
        .unwrap();
}
fn recv(socket: &DecodeSocket) -> TokenAnswer {
    serde_json::from_slice(&socket.recv_octets().unwrap()).unwrap()
}

#[test]
fn refeed_cancel_matches_the_original_turn_and_refuses_foreign_work() {
    let (client, server) = pair();
    let turn = TurnKey("original".into());
    let mut cancel = SeamCancel {
        socket: &server,
        refeed_turn: Some(&turn),
        cancelled: false,
        fatal: false,
    };
    send(
        &client,
        TokenDirective::Cancel {
            turn: TurnKey("foreign".into()),
        },
    );
    assert!(!cancel.cancelled());
    assert!(!cancel.cancelled && !cancel.fatal);
    assert_eq!(
        serde_json::from_slice::<TokenRefusal>(&client.recv_octets().unwrap()).unwrap(),
        TokenRefusal::OutOfOrder
    );
    assert!(client.try_recv_octets().unwrap().is_none());
    send(&client, TokenDirective::Cancel { turn: turn.clone() });
    assert!(cancel.cancelled());
    assert!(cancel.cancelled && !cancel.fatal);
}

fn fail_intermediate_then_restore(
    client: &DecodeSocket,
    server: &DecodeSocket,
    frame: &TokenAnswer,
) {
    use nix::fcntl::{FcntlArg, OFlag, fcntl};
    use nix::sys::socket::{MsgFlags, recv, send};
    use std::os::fd::AsRawFd;
    let flags = OFlag::from_bits_retain(fcntl(server.as_fd(), FcntlArg::F_GETFL).unwrap());
    fcntl(server.as_fd(), FcntlArg::F_SETFL(flags | OFlag::O_NONBLOCK)).unwrap();
    loop {
        match send(
            server.as_fd().as_raw_fd(),
            &[0; 1024],
            MsgFlags::MSG_DONTWAIT,
        ) {
            Ok(1024) => (),
            Err(nix::errno::Errno::EAGAIN) => break,
            other => panic!("fill the actual outgoing queue: {other:?}"),
        }
    }
    assert!(send_answer(server, frame).is_err());
    loop {
        match recv(
            client.as_fd().as_raw_fd(),
            &mut [0; 2048],
            MsgFlags::MSG_DONTWAIT,
        ) {
            Ok(n) if n > 0 => (),
            Err(nix::errno::Errno::EAGAIN) => break,
            other => panic!("drain the fixture pressure: {other:?}"),
        }
    }
    fcntl(server.as_fd(), FcntlArg::F_SETFL(flags)).unwrap();
}

fn interrupted_prefix(
    consumed: usize,
    fail_terminator: bool,
    fail_stream: bool,
    fail_terminal: bool,
) {
    let log = Rc::new(RefCell::new(Observed {
        fail_terminator,
        ..Observed::default()
    }));
    let mut session = Session::new(
        Box::new(RecordingBackend(log.clone())),
        64,
        FlushMechanism::TruncateToPosition,
        false,
    );
    session.open(&[TokenId(4)]).unwrap();
    session.ask_columns();
    let (client, server) = pair();
    let turn = TurnKey("retained-turn".into());
    let mut cancel = SeamCancel {
        socket: &server,
        refeed_turn: Some(&turn),
        cancelled: false,
        fatal: false,
    };
    let fields = Cell::new(0);
    let stream_failed = Cell::new(false);
    let mut field = |position, ranked: Vec<(u32, f32)>, realized| {
        fields.set(fields.get() + 1);
        let frame = TokenAnswer::Field {
            position,
            ranked: ranked
                .into_iter()
                .map(|(token, probability)| weaver_types::Candidate { token, probability })
                .collect(),
            realized,
        };
        if fail_stream {
            // Fail a real intermediate send, with Cancel already queued so
            // the next poll can stop without reading EOF instead.
            send(&client, TokenDirective::Cancel { turn: turn.clone() });
            fail_intermediate_then_restore(&client, &server, &frame);
            stream_failed.set(true);
        } else {
            send_answer(&server, &frame).unwrap();
        }
    };
    let mut column = |position, layers| {
        send_answer(&server, &TokenAnswer::Column { position, layers }).unwrap();
    };
    // Cancellation arrives independently of field/column delivery, so
    // removing an intermediate cannot merely disable the fixture's Cancel.
    struct Scheduled<'a, 'b> {
        inner: &'a mut SeamCancel<'b>,
        client: &'a DecodeSocket,
        turn: &'a TurnKey,
        at: Option<usize>,
        polls: usize,
    }
    impl CancelPoll for Scheduled<'_, '_> {
        fn cancelled(&mut self) -> bool {
            if self.at == Some(self.polls) {
                send(
                    self.client,
                    TokenDirective::Cancel {
                        turn: self.turn.clone(),
                    },
                );
            }
            self.polls += 1;
            self.inner.cancelled()
        }
    }
    let mut scheduled = Scheduled {
        inner: &mut cancel,
        client: &client,
        turn: &turn,
        at: (!fail_stream).then_some(consumed),
        polls: 0,
    };
    let generated = session.refeed(
        &[TokenId(5)],
        &[TokenId(7), TokenId(8), TokenId(9)],
        TokenId(0),
        &mut scheduled,
        PositionedSinks {
            field: Some((4, &mut field)),
            on_column: &mut column,
        },
        SamplerBuild {
            seed: 11,
            penalty_window: 64,
        },
    );
    if fail_terminator {
        assert!(matches!(generated, Err(DecodeFault::Engine { .. })));
        assert_eq!(session.placement_position(0), Err(DecodeFault::NotOpen));
        assert!(client.try_recv_octets().unwrap().is_none());
        return;
    }
    let generated = generated.unwrap();
    assert_eq!(generated.stopped, Stopped::Cancelled);
    assert_eq!(generated.tokens.len(), consumed);
    let mut expected = vec![(vec![TokenId(4)], 0), (vec![TokenId(5)], 1)];
    expected.extend(
        [7, 8, 9]
            .into_iter()
            .take(consumed)
            .enumerate()
            .map(|(n, t)| (vec![TokenId(t)], n + 2)),
    );
    expected.push((vec![TokenId(0)], consumed + 2));
    assert_eq!(log.borrow().decoded, expected);
    if fail_terminal {
        use std::os::fd::AsRawFd;
        nix::sys::socket::shutdown(client.as_fd().as_raw_fd(), nix::sys::socket::Shutdown::Read)
            .unwrap();
    }
    let result = finish_refeed(
        &server,
        &turn,
        &session,
        &generated,
        &cancel,
        stream_failed.get(),
    );
    if fail_stream || fail_terminal {
        assert!(result.is_err());
        if fail_stream {
            assert!(client.try_recv_octets().unwrap().is_none());
        }
        return;
    }
    assert_eq!(result, Ok(true));
    for n in 0..consumed {
        assert_eq!(
            recv(&client),
            TokenAnswer::Column {
                position: (n + 2) as u64,
                layers: vec![vec![2.0, 3.0]]
            }
        );
        let TokenAnswer::Field { position, .. } = recv(&client) else {
            panic!("retained field expected")
        };
        assert_eq!(position, (n + 2) as u64);
    }
    assert_eq!(
        recv(&client),
        TokenAnswer::RefeedInterrupted {
            turn,
            consumed: consumed as u64,
            resident: (consumed + 3) as u64,
            capacity: 64
        }
    );
    assert_eq!(recv(&client), TokenAnswer::AtRest);
    assert!(client.try_recv_octets().unwrap().is_none());
}

#[test]
fn refeed_zero_prefix_settles_both_answers_without_generation() {
    interrupted_prefix(0, false, false, false);
}
#[test]
fn refeed_interior_prefix_retains_exact_fields_columns_and_terminal_order() {
    interrupted_prefix(2, false, false, false);
}
#[test]
fn refeed_failed_terminator_never_becomes_an_interrupted_answer() {
    interrupted_prefix(0, true, false, false);
}
#[test]
fn refeed_failed_intermediate_remains_failed_despite_matching_cancel() {
    interrupted_prefix(1, false, true, false);
}
#[test]
fn refeed_failed_terminal_send_does_not_complete_cancel() {
    interrupted_prefix(0, false, false, true);
}

#[test]
fn refeed_malformed_cancel_poll_remains_a_fault_without_terminal_answers() {
    let (client, server) = pair();
    let turn = TurnKey("original".into());
    client.send_octets(b"{broken").unwrap();
    let mut cancel = SeamCancel {
        socket: &server,
        refeed_turn: Some(&turn),
        cancelled: false,
        fatal: false,
    };
    let mut session = Session::new(
        Box::new(RecordingBackend(Rc::new(RefCell::new(Observed::default())))),
        64,
        FlushMechanism::TruncateToPosition,
        false,
    );
    session.open(&[TokenId(4)]).unwrap();
    let generated = session
        .refeed(
            &[TokenId(5)],
            &[TokenId(7)],
            TokenId(0),
            &mut cancel,
            PositionedSinks {
                field: None,
                on_column: &mut |_, _| {},
            },
            SamplerBuild {
                seed: 1,
                penalty_window: 64,
            },
        )
        .unwrap();
    assert!(cancel.fatal && !cancel.cancelled);
    assert!(finish_refeed(&server, &turn, &session, &generated, &cancel, false).is_err());
    assert!(client.try_recv_octets().unwrap().is_none());
}
