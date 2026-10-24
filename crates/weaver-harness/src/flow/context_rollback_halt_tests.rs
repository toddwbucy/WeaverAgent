use super::*;
use crate::channel::OrganChannel;
use crate::flow::clock::testing::FakePumpIo;
use crate::flow::context::startup_composition::tests::CircuitIo;
use crate::flow::coordinator::tests::operation;
use crate::flow::pump_transport::ReceiveProgress;
use std::io::{Read, Write};
use std::os::fd::{AsFd, AsRawFd};
use std::os::unix::net::UnixStream;
use weaver_types::state_flow as s;

pub(in crate::flow::context::startup_composition::refusal::response) fn drive(
    rollback: RollbackComposition<'_>,
    io: &mut FakePumpIo,
    reader: &mut UnixStream,
    state: &mut UnixStream,
    mode: &str,
    far: &OrganChannel,
) {
    let original_leave = rollback.leave();
    let original_directive = rollback.directive().envelope.clone();
    let source = rollback.response.refusal.source().detail.as_ptr();
    let refusal_sequence = rollback.response.refusal.sequence();
    let initial = match rollback.initial() {
        InspectionCompletion::Grants { evidence, .. } => {
            s::encode_state_line(evidence.answer(), s::FrameDirection::Response).unwrap()
        }
        _ => panic!("actual initial reading"),
    };
    let mut rollback = match rollback.into_failed_retirement(io) {
        Err(owner) => owner,
        Ok(_) => panic!("live rollback admitted to failed retirement"),
    };
    state.set_nonblocking(true).unwrap();
    let mut bytes = Vec::new();
    for _ in 0..128 {
        assert_ne!(rollback.advance(&mut CircuitIo(io)), RollbackStage::Stopped);
        let mut chunk = [0; 8192];
        match state.read(&mut chunk) {
            Ok(n) => bytes.extend_from_slice(&chunk[..n]),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            e => panic!("actual Grants request: {e:?}"),
        }
        if bytes.ends_with(b"\n") {
            break;
        }
    }
    let request = s::decode_state_line(&bytes, s::FrameDirection::Request).unwrap();
    assert!(matches!(
        &request,
        s::StateFrame::Control {
            frame: s::StateControl::Grants {},
            ..
        }
    ));
    let bound = rollback
        .response
        .refusal
        .owner
        .startup
        .context
        .deadlines
        .get(PumpDeadline::StateControl)
        .unwrap();
    assert!(bound.deadline_ns() < original_leave.deadline_ns());
    let mut pending_ticket = None;
    let mut pending_bytes = None;
    if mode == "response-halt-extra" {
        let extra = OrganEnvelope {
            exchange: weaver_types::ExchangeId {
                opener: Opener::Admin,
                ordinal: 199,
            },
            position: Position::Open,
            payload: weaver_types::Payload::Directive(LifecycleDirective::Stop {
                cause: weaver_types::Cause { uid: 199 },
            }),
        };
        let encoded = serde_json::to_vec(&extra).unwrap();
        nix::sys::socket::send(
            far.as_fd().as_raw_fd(),
            &encoded,
            nix::sys::socket::MsgFlags::MSG_NOSIGNAL,
        )
        .unwrap();
        assert_eq!(rollback.peer.try_receive().unwrap(), ReceiveProgress::Held);
    }
    if matches!(
        mode,
        "response-halt-pressure"
            | "response-halt-record"
            | "response-halt-record-clock"
            | "response-halt-record-expiry"
    ) {
        crate::flow::context::startup_composition::refusal::tests::fill(
            rollback.response.refusal.owner.startup.context,
            io,
        );
        let began = io.now().unwrap() + 1;
        let answer = s::StateFrame::ControlAnswer {
            common: request.common().clone(),
            frame: s::StateControlAnswer::Grants {
                surface: vec!["never a live grant".into()],
            },
            observation: operation(&request, 3, began),
        };
        let encoded = s::encode_state_line(&answer, s::FrameDirection::Response).unwrap();
        io.advance_to(began + 2).unwrap();
        state.write_all(&encoded).unwrap();
        for _ in 0..128 {
            assert_ne!(rollback.advance(&mut CircuitIo(io)), RollbackStage::Stopped);
            if rollback
                .response
                .refusal
                .owner
                .startup
                .context
                .barrier
                .pending_author()
                .is_some()
            {
                break;
            }
        }
        let context = &mut rollback.response.refusal.owner.startup.context;
        let author = context
            .barrier
            .pending_author()
            .expect("exact measurement held under pressure");
        assert_eq!(author.event.kind, Kind::StateStoreMeasurement);
        pending_ticket = Some(author.ticket);
        pending_bytes =
            Some(serde_json::to_vec(context.flow.state().held_measurement().unwrap()).unwrap());
        context.fail(
            io,
            FlowHaltReason::ProtocolFailed,
            HaltPoint::Barrier,
            "scripted first fault with measured Grants held",
        );
        if mode.starts_with("response-halt-record") {
            crate::flow::context::startup_composition::refusal::tests::drain(context, reader);
        }
    } else if mode == "response-halt-timeout" {
        io.advance_to(bound.deadline_ns()).unwrap();
    } else {
        if mode == "response-halt-partial" {
            state
                .write_all(b"{\"version\":\"state/1\",\"common\":")
                .unwrap();
            for _ in 0..4 {
                assert_ne!(rollback.advance(&mut CircuitIo(io)), RollbackStage::Stopped);
            }
        }
        state.shutdown(std::net::Shutdown::Both).unwrap();
    }
    for _ in 0..128 {
        if rollback.advance(&mut CircuitIo(io)) == RollbackStage::Stopped {
            break;
        }
    }
    assert_eq!(rollback.stage(), RollbackStage::Stopped);
    let first = rollback
        .response
        .refusal
        .owner
        .startup
        .context
        .failure()
        .expect("actual fatal state outcome")
        .clone();
    let expected_halt = rollback
        .response
        .refusal
        .owner
        .startup
        .context
        .flow
        .halt()
        .unwrap()
        .deadline()
        .unwrap()
        .min(original_leave.deadline_ns());
    if mode == "response-halt-foreign" {
        let context = &mut rollback.response.refusal.owner.startup.context;
        context.deadlines.disarm(PumpDeadline::StateControl);
        let foreign =
            AbsoluteBound::after(bound.began_ns(), bound.deadline_ns() - bound.began_ns() + 1)
                .unwrap();
        context
            .deadlines
            .arm(PumpDeadline::StateControl, foreign)
            .unwrap();
        rollback = match rollback.into_failed_retirement(io) {
            Err(owner) => owner,
            Ok(_) => panic!("substituted control wait admitted"),
        };
        assert!(
            rollback
                .response
                .refusal
                .owner
                .startup
                .context
                .halt_report()
                .is_none()
        );
        let context = &mut rollback.response.refusal.owner.startup.context;
        context.deadlines.disarm(PumpDeadline::StateControl);
        context
            .deadlines
            .arm(PumpDeadline::StateControl, bound)
            .unwrap();
    }
    let raw_received = rollback
        .response
        .refusal
        .owner
        .startup
        .context
        .flow
        .state()
        .retained_received_bytes()
        .to_vec();
    let outstanding = format!(
        "{:?}",
        rollback
            .response
            .refusal
            .owner
            .startup
            .context
            .flow
            .state()
            .outstanding()
    );
    if mode == "response-halt-partial" {
        assert!(
            !raw_received.is_empty(),
            "partial wire was actually consumed"
        );
    }
    if mode == "response-halt-record-expiry" {
        io.advance_to(bound.deadline_ns()).unwrap();
    }
    if mode == "response-halt-cap" {
        io.advance_to(original_leave.deadline_ns()).unwrap();
    }
    let retired = if mode == "response-halt-clock" {
        let mut broken = BrokenClock;
        rollback.into_failed_retirement(&mut broken)
    } else if mode == "response-halt-record-clock" {
        rollback.into_failed_retirement(&mut FailSixth {
            inner: io,
            reads: 0,
        })
    } else {
        rollback.into_failed_retirement(io)
    };
    let retired = match retired {
        Ok(owner) => owner,
        Err(owner) => panic!("actual stopped owner refused: {:?}", owner.stop()),
    };
    assert_eq!(retired.failure(), &first);
    assert_eq!(retired.grant_surface(), GrantSurface::Unreadable);
    assert_eq!(retired.leave(), original_leave);
    assert_eq!(retired.deadline(), expected_halt);
    assert_eq!(retired.directive().envelope, original_directive);
    assert_eq!(retired.refusal.source().detail.as_ptr(), source);
    assert_eq!(retired.refusal.sequence(), refusal_sequence);
    assert_eq!(retired.read().bound, bound);
    assert_eq!(retired.read().request_id, Some(request.common().request_id));
    assert!(retired.current.is_none());
    let context = &retired.refusal.owner.startup.context;
    assert_eq!(
        retired
            .flow
            .state
            .as_ref()
            .unwrap()
            .retained_received_bytes(),
        raw_received
    );
    assert_eq!(
        format!("{:?}", retired.flow.state.as_ref().unwrap().outstanding()),
        outstanding
    );

    assert!(
        context.inspection.is_some(),
        "actual stopped inspection retained"
    );
    let initial_after = match retired
        .refusal
        .owner
        .startup
        .initial_grants
        .as_ref()
        .unwrap()
    {
        InspectionCompletion::Grants { evidence, .. } => {
            s::encode_state_line(evidence.answer(), s::FrameDirection::Response).unwrap()
        }
        _ => panic!("initial observation replaced"),
    };
    assert_eq!(initial_after, initial);
    let rows = context.barrier.recorder().structure();
    assert!(
        !rows
            .iter()
            .any(|row| row.line.contains("\"kind\":\"unload\"")),
        "no-SPU Unload rule is not merged"
    );
    if matches!(mode, "response-halt-cap" | "response-halt-clock") {
        assert!(retired.expired());
        assert!(retired.fault_sequence().is_none());
    } else {
        assert_eq!(retired.expired(), mode == "response-halt-record-clock");
        assert!(retired.fault_sequence().is_some());
        assert!(context.flow.halt().unwrap().account().is_some());
    }
    if matches!(mode, "response-halt-record" | "response-halt-record-clock") {
        let sequence = retired
            .read()
            .trace_sequence
            .expect("owed trace-only measurement recorded");
        assert!(sequence > retired.fault_sequence().unwrap());
        assert_eq!(
            context.barrier.recorded_author_ticket(sequence),
            pending_ticket
        );
        assert!(context.barrier.pending().is_none());
        assert!(
            context.flow.operation_completion().is_none(),
            "trace-only is not a live completion"
        );
    } else {
        assert!(retired.read().trace_sequence.is_none());
    }
    if let Some(encoded) = pending_bytes {
        assert_eq!(retired.read().ticket, pending_ticket);
        assert_eq!(
            serde_json::to_vec(
                retired
                    .flow
                    .state
                    .as_ref()
                    .unwrap()
                    .held_measurement()
                    .unwrap()
            )
            .unwrap(),
            encoded
        );
        if mode == "response-halt-pressure" {
            assert_eq!(
                Some(context.barrier.pending_author().unwrap().ticket),
                pending_ticket
            );
            assert!(context.barrier.pending().is_some());
        }
    }
    if mode == "response-halt-extra" {
        let held = retired
            .peer
            .held
            .as_ref()
            .expect("extra actual packet retained");
        assert_eq!(held.envelope.exchange.ordinal, 199);
    }
    let mut byte = [0];
    assert_eq!(
        nix::sys::socket::recv(
            far.as_fd().as_raw_fd(),
            &mut byte,
            nix::sys::socket::MsgFlags::MSG_DONTWAIT
        ),
        Ok(0),
        "accepted Leave connection still open or answered"
    );
    assert!(
        retired
            .refusal
            .owner
            .startup
            .resources
            .spu_child()
            .is_none()
    );
    // The outer fixture owns child reaping. Retirement never calls blocking wait.
}
struct BrokenClock;
impl PumpIo for BrokenClock {
    fn now(&mut self) -> std::io::Result<u64> {
        Err(std::io::Error::other("retirement clock failed"))
    }
    fn wait(
        &mut self,
        _: &[crate::flow::clock::WaitInterest<'_>],
        _: Option<u64>,
    ) -> std::io::Result<crate::flow::clock::WaitOutcome> {
        panic!("halt retirement must not wait")
    }
}

struct FailSixth<'a> {
    inner: &'a mut FakePumpIo,
    reads: usize,
}
impl PumpIo for FailSixth<'_> {
    fn now(&mut self) -> std::io::Result<u64> {
        self.reads += 1;
        if self.reads == 6 {
            Err(std::io::Error::other("post-record clock failed"))
        } else {
            self.inner.now()
        }
    }
    fn wait(
        &mut self,
        _: &[crate::flow::clock::WaitInterest<'_>],
        _: Option<u64>,
    ) -> std::io::Result<crate::flow::clock::WaitOutcome> {
        panic!("halt retirement must not wait")
    }
}
