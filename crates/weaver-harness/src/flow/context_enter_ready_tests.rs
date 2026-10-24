use super::*;
use crate::flow::clock::{PumpIo, WaitInterest, WaitOutcome, testing::FakePumpIo};
use crate::flow::context::startup_composition::tests::entered_tests::{
    close_original_peer, original_peer_fd,
};
use nix::sys::socket::{MsgFlags, recv, send};
use std::os::fd::AsRawFd;
use std::time::Duration;

fn stop(fd: i32) {
    let packet = OrganEnvelope {
        exchange: weaver_types::ExchangeId {
            opener: Opener::Admin,
            ordinal: 8,
        },
        position: Position::Open,
        payload: weaver_types::Payload::Directive(LifecycleDirective::Stop {
            cause: weaver_types::Cause { uid: 73 },
        }),
    };
    let bytes = serde_json::to_vec(&packet).unwrap();
    assert_eq!(send(fd, &bytes, MsgFlags::MSG_NOSIGNAL), Ok(bytes.len()));
}
fn receive(fd: i32) -> Option<Vec<u8>> {
    let mut bytes = vec![0; weaver_types::MAX_ENVELOPE_BYTES];
    match recv(fd, &mut bytes, MsgFlags::MSG_DONTWAIT) {
        Ok(n) if n > 0 => {
            bytes.truncate(n);
            Some(bytes)
        }
        Ok(_) | Err(nix::errno::Errno::EAGAIN) => None,
        Err(error) => panic!("receive original peer: {error}"),
    }
}
struct SendClock<'a> {
    clock: &'a mut FakePumpIo,
    fail: bool,
    after: Option<u64>,
}
impl PumpIo for SendClock<'_> {
    fn now(&mut self) -> std::io::Result<u64> {
        let mut byte = [0];
        if recv(
            original_peer_fd(),
            &mut byte,
            MsgFlags::MSG_PEEK | MsgFlags::MSG_DONTWAIT,
        ) == Ok(1)
        {
            if self.fail {
                return Err(std::io::Error::other("failed after actual Ready send"));
            }
            if let Some(at) = self.after {
                self.clock.advance_to(at)?;
            }
        }
        self.clock.now()
    }
    fn wait(
        &mut self,
        interests: &[WaitInterest<'_>],
        deadline: Option<u64>,
    ) -> std::io::Result<WaitOutcome> {
        super::super::tests::CircuitIo(self.clock).wait(interests, deadline)
    }
}

pub(in crate::flow::context::boot_composition::gate_startup) fn drive<'a>(
    mut raised: RaisedBoot<'a>,
    clock: &mut FakePumpIo,
    mode: &str,
) -> GateStartup<'a> {
    if mode == "resident-aggregate-missing" {
        return match raised.into_ready() {
            Err(raised) => raised.owner,
            Ok(_) => panic!("missing original granted aggregate"),
        };
    }
    if mode == "entered-aggregate-plain-ready" {
        let port = raised.owner.port.as_mut().unwrap();
        assert_eq!(
            send(port.fd().as_raw_fd(), b"g", MsgFlags::MSG_NOSIGNAL),
            Ok(1)
        );
        for _ in 0..1000 {
            port.try_receive().unwrap();
            if let Some(reply) = port.take_received().unwrap() {
                assert!(reply.belongs_to(port));
                assert!(matches!(
                    reply.envelope().payload,
                    weaver_types::Payload::Answer(LifecycleAnswer::Ready)
                ));
                raised.owner.reply = Some(reply);
                assert!(receive(original_peer_fd()).is_none());
                return match raised.into_ready() {
                    Err(raised) => raised.owner,
                    Ok(_) => panic!("plain Ready from the gate granted aggregate admission"),
                };
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        panic!("same gate did not supply the plain Ready probe");
    }
    let mut ready = raised
        .into_ready()
        .ok()
        .expect("actual composed prerequisites");
    let enter = ready.enter;
    let fd = original_peer_fd();
    assert!(
        receive(fd).is_none(),
        "no early admin Ready from gate confirmation"
    );
    let source = ready
        .raised
        .owner
        .boot
        .resident
        .as_mut()
        .unwrap()
        .evidence
        .startup
        .original_enter
        .as_mut()
        .unwrap();
    if mode == "entered-aggregate-busy" {
        let mut unrelated = source.ready_envelope();
        unrelated.exchange.ordinal += 1;
        assert!(matches!(
            source.peer_mut().stage_tracked(unrelated),
            Ok(TrackedStage::Queued(_))
        ));
    }
    let mut filler = 0;
    if matches!(
        mode,
        "entered-aggregate-pressure" | "entered-aggregate-stop-pressure"
    ) {
        loop {
            match send(
                source.peer().fd().as_raw_fd(),
                &[b'x'; 4096],
                MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_NOSIGNAL,
            ) {
                Ok(4096) => filler += 1,
                Err(nix::errno::Errno::EAGAIN) => break,
                other => panic!("fill Ready sender: {other:?}"),
            }
        }
        assert!(filler > 0);
    }
    if mode == "entered-aggregate-stop" {
        stop(fd);
    }
    if mode == "entered-aggregate-eof" {
        close_original_peer();
    }
    if mode == "entered-aggregate-expiry" {
        clock.advance_to(enter.deadline_ns()).unwrap();
    }
    if mode == "entered-aggregate-gate-dead" {
        ready.raised.owner.child.as_mut().unwrap().stop_at_cap();
        std::thread::sleep(Duration::from_millis(20));
    }
    if mode == "entered-aggregate-gate-terminal" {
        nix::sys::socket::shutdown(
            ready.raised.owner.port.as_ref().unwrap().fd().as_raw_fd(),
            nix::sys::socket::Shutdown::Read,
        )
        .unwrap();
        assert_eq!(
            ready.raised.owner.child.as_mut().unwrap().poll(),
            ChildState::Running
        );
    }
    if mode == "entered-aggregate-spu-dead" {
        ready
            .raised
            .owner
            .boot
            .resident
            .as_mut()
            .unwrap()
            .spu
            .stop_at_cap();
        std::thread::sleep(Duration::from_millis(20));
    }
    if filler > 0 {
        for _ in 0..3 {
            assert_eq!(
                ready.advance(&mut super::super::tests::CircuitIo(clock)),
                ReadyStage::Sending
            );
            assert!(!ready.send().unwrap().sent());
            assert_eq!(
                ready
                    .raised
                    .owner
                    .boot
                    .context
                    .deadlines
                    .get(PumpDeadline::Enter),
                Some(enter)
            );
        }
        if mode == "entered-aggregate-stop-pressure" {
            stop(fd);
        }
        for _ in 0..filler {
            assert_eq!(receive(fd).unwrap(), vec![b'x'; 4096]);
        }
    }
    let mut io = SendClock {
        clock,
        fail: mode == "entered-aggregate-clock-after-send",
        after: (mode == "entered-aggregate-expiry-after-send").then_some(enter.deadline_ns()),
    };
    for _ in 0..1000 {
        if ready.advance(&mut io) != ReadyStage::Sending {
            break;
        }
    }
    let successful = matches!(
        mode,
        "entered-aggregate-ready" | "entered-aggregate-pressure"
    );
    if !successful {
        assert_eq!(ready.stage(), ReadyStage::Stopped, "{:?}", ready.stop());
        match mode {
            "entered-aggregate-stop" | "entered-aggregate-stop-pressure" => {
                assert!(matches!(ready.stop(), Some(ReadyStop::Interrupted)))
            }
            "entered-aggregate-gate-terminal" => assert!(matches!(
                ready.stop(),
                Some(ReadyStop::Run(RunFailure::Transport {
                    source: PumpSource::Gate,
                    ..
                }))
            )),
            "entered-aggregate-eof" => assert!(matches!(
                ready.stop(),
                Some(ReadyStop::Run(RunFailure::Transport {
                    source: PumpSource::Admin,
                    ..
                }))
            )),
            "entered-aggregate-expiry" | "entered-aggregate-expiry-after-send" => {
                assert!(matches!(
                    ready.stop(),
                    Some(ReadyStop::Run(RunFailure::Wait {
                        key: PumpDeadline::Enter,
                        ..
                    }))
                ))
            }
            "entered-aggregate-gate-dead" | "entered-aggregate-spu-dead" => {
                assert!(matches!(ready.stop(), Some(ReadyStop::ChildStopped)))
            }
            "entered-aggregate-busy" => {
                assert!(matches!(ready.stop(), Some(ReadyStop::Busy)));
                assert!(ready.returned.is_some());
            }
            "entered-aggregate-clock-after-send" => {
                assert!(ready.send().unwrap().sent());
                assert!(ready.send().unwrap().sent_at().is_none());
                assert!(ready.raised.owner.boot.context.failure().is_some());
            }
            _ => panic!("unhandled mode"),
        }
        if !mode.ends_with("after-send") {
            assert!(!ready.send().is_some_and(OrganSend::sent));
            assert!(receive(fd).is_none());
        } else {
            let answer: OrganEnvelope = serde_json::from_slice(&receive(fd).unwrap()).unwrap();
            assert_eq!(answer, ready.answer);
            assert!(receive(fd).is_none());
        }
        let cause = format!("{:?}", ready.stop());
        assert_eq!(ready.advance(&mut io), ReadyStage::Stopped);
        assert_eq!(format!("{:?}", ready.stop()), cause);
        ready = match ready.finish() {
            Err(ready) => ready,
            Ok(_) => panic!("stopped Ready escaped"),
        };
        assert!(
            ready
                .raised
                .owner
                .boot
                .resident
                .as_ref()
                .unwrap()
                .evidence
                .startup
                .original_enter
                .is_some()
        );
        return ready.raised.owner;
    }
    assert_eq!(ready.stage(), ReadyStage::Complete, "{:?}", ready.stop());
    let bytes = receive(fd).expect("actual Ready on original connection");
    let answer: OrganEnvelope = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        answer.exchange,
        weaver_types::ExchangeId {
            opener: Opener::Admin,
            ordinal: 7
        }
    );
    assert_eq!(answer.position, Position::Close);
    assert!(matches!(
        answer.payload,
        weaver_types::Payload::Answer(LifecycleAnswer::Ready)
    ));
    assert!(receive(fd).is_none(), "one aggregate answer");
    assert_eq!(
        ready
            .raised
            .owner
            .boot
            .context
            .deadlines
            .get(PumpDeadline::Enter),
        None
    );
    assert_eq!(ready.raised.owner.boot.context.retired_enter, Some(enter));
    close_original_peer(); // Expected admin exit cannot revoke a completed handoff.
    io.clock.advance_to(enter.deadline_ns() + 1).unwrap();
    assert_eq!(ready.advance(&mut io), ReadyStage::Complete);
    let entered = ready
        .finish()
        .ok()
        .expect("sent answer yields serving resources");
    assert!(entered.ready.sent());
    assert!(entered.ready.matches_exchange(&answer.exchange));
    assert!(
        entered
            .resident
            .evidence
            .reply
            .belongs_to(entered.resident.residency)
    );
    assert!(entered.gate_reply.belongs_to(&entered.gate));
    assert!(
        entered
            .resident
            .evidence
            .startup
            .original_enter
            .as_ref()
            .unwrap()
            .adopted_by(entered.context)
    );
    assert!(entered.context.place_engine.is_some());
    assert!(entered.context.committed_plan_summary().is_ok());
    // Return the complete actual gate owner to the circuit's existing teardown guard.
    GateStartup {
        boot: ComposedBoot {
            resident: Some(entered.resident),
            context: entered.context,
            decode: entered.decode,
            certificate: entered.certificate.unwrap(),
        },
        request: entered.gate_request,
        listener: entered.listener,
        enter: Some(entered.enter),
        stage: GateStartupStage::Complete,
        tools: Some(entered.tools),
        child: Some(entered.gate_child),
        port: Some(entered.gate),
        raise: entered.gate_raise,
        send: Some(entered.gate_send),
        reply: Some(entered.gate_reply),
        returned: None,
        stop: None,
    }
}
