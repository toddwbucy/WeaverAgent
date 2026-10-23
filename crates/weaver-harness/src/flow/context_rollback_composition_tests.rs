use super::*;
use crate::channel::OrganChannel;
use crate::flow::clock::{WaitInterest, WaitOutcome, testing::FakePumpIo};
use crate::flow::context::startup_composition::tests::CircuitIo;
use crate::flow::coordinator::tests::operation;
use crate::flow::pump_transport::{ReceiveProgress, Stage};
use nix::errno::Errno;
use nix::sys::socket::{MsgFlags, recv, send};
use std::io::{Read, Write};
use std::os::fd::{AsFd, AsRawFd};
use std::os::unix::net::UnixStream;
use weaver_types::state_flow as s;

fn receive(channel: &OrganChannel) -> Option<Vec<u8>> {
    let mut bytes = vec![0; weaver_types::MAX_ENVELOPE_BYTES];
    match recv(
        channel.as_fd().as_raw_fd(),
        &mut bytes,
        MsgFlags::MSG_DONTWAIT,
    ) {
        Ok(0) => panic!("fixture channel closed"),
        Ok(n) => {
            bytes.truncate(n);
            Some(bytes)
        }
        Err(Errno::EAGAIN) => None,
        Err(e) => panic!("fixture receive: {e}"),
    }
}

fn leave() -> OrganEnvelope {
    OrganEnvelope {
        exchange: ExchangeId {
            opener: Opener::Admin,
            ordinal: 93,
        },
        position: Position::Open,
        payload: weaver_types::Payload::Directive(LifecycleDirective::Leave {
            cause: weaver_types::Cause { uid: 1042 },
            forced: true,
            rollback: true,
        }),
    }
}

fn delivered(envelope: &OrganEnvelope, handle: bool) -> (LifecyclePeer, OrganChannel) {
    let (near, far) = OrganChannel::pair().unwrap();
    let far = far.into_channel();
    if handle {
        let file = std::fs::File::open("/dev/null").unwrap();
        far.send_with_descriptor(envelope, file.as_fd()).unwrap();
    } else {
        let bytes = serde_json::to_vec(envelope).unwrap();
        assert_eq!(
            send(far.as_fd().as_raw_fd(), &bytes, MsgFlags::MSG_NOSIGNAL),
            Ok(bytes.len())
        );
    }
    let mut peer = LifecyclePeer::from_channel(near).unwrap();
    // The outer listener's real receive owner supplies the held directive. This
    // uses its one parser, never a forged LifecycleMessage or cloned read fd.
    assert_eq!(peer.try_receive().unwrap(), ReceiveProgress::Held);
    (peer, far)
}

struct AfterSendClock<'a> {
    inner: &'a mut FakePumpIo,
    peer: &'a OrganChannel,
    failed: bool,
}
impl PumpIo for AfterSendClock<'_> {
    fn now(&mut self) -> std::io::Result<u64> {
        let mut byte = [0];
        if !self.failed
            && recv(
                self.peer.as_fd().as_raw_fd(),
                &mut byte,
                MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_PEEK,
            )
            .is_ok_and(|n| n > 0)
        {
            self.failed = true;
            return Err(std::io::Error::other("post-send clock failed"));
        }
        self.inner.now()
    }
    fn wait(
        &mut self,
        interests: &[WaitInterest<'_>],
        deadline: Option<u64>,
    ) -> std::io::Result<WaitOutcome> {
        CircuitIo(self.inner).wait(interests, deadline)
    }
}

struct BrokenClock;
impl PumpIo for BrokenClock {
    fn now(&mut self) -> std::io::Result<u64> {
        Err(std::io::Error::other("scripted rollback clock failure"))
    }
    fn wait(&mut self, _: &[WaitInterest<'_>], _: Option<u64>) -> std::io::Result<WaitOutcome> {
        unreachable!()
    }
}

pub(in crate::flow::context::startup_composition::refusal) fn drive(
    recorded: RecordedStartupRefusal<'_>,
    io: &mut FakePumpIo,
    reader: &mut UnixStream,
    state: &mut UnixStream,
    mode: &str,
) {
    let sequence = recorded.sequence();
    let source = recorded.source().detail.as_ptr();
    let original = recorded.owner.bound;
    let expected = serde_json::to_vec(recorded.envelope()).unwrap();
    let (near, far) = OrganChannel::pair().unwrap();
    let far = far.into_channel();
    let mut peer = LifecyclePeer::from_channel(near).unwrap();
    if mode == "response-busy" {
        let mut other = leave();
        other.position = Position::Close;
        assert!(matches!(peer.stage(other), Ok(Stage::Queued)));
        let mut response = recorded.into_response(peer);
        assert_eq!(response.advance(&mut CircuitIo(io)), ResponseStage::Stopped);
        assert!(matches!(response.stop(), Some(ResponseStop::Busy)));
        assert_eq!(
            serde_json::to_vec(response.returned.as_ref().unwrap()).unwrap(),
            expected
        );
        assert!(response.send().is_none());
        assert!(receive(&far).is_none());
        assert_eq!(response.refusal.source().detail.as_ptr(), source);
        return;
    }
    let mut filled = 0;
    if matches!(mode, "response-pressure" | "response-unsent-rollback") {
        for _ in 0..1000 {
            match send(
                peer.fd().as_raw_fd(),
                &[b'x'; 4096],
                MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_NOSIGNAL,
            ) {
                Ok(4096) => filled += 1,
                Err(Errno::EAGAIN) => break,
                other => panic!("fill response socket: {other:?}"),
            }
        }
        assert!(filled > 0);
    }
    let mut response = recorded.into_response(peer);
    if mode == "response-expired" {
        io.advance_to(original.deadline_ns()).unwrap();
        assert_eq!(response.advance(&mut CircuitIo(io)), ResponseStage::Stopped);
        assert!(matches!(
            response.stop(),
            Some(ResponseStop::Run(RunFailure::Wait {
                key: PumpDeadline::Enter,
                ..
            }))
        ));
        assert!(response.send().is_none());
        assert!(receive(&far).is_none());
        assert_eq!(response.refusal.sequence(), sequence);
        return;
    }
    if matches!(mode, "response-pressure" | "response-unsent-rollback") {
        for _ in 0..8 {
            assert_eq!(response.advance(&mut CircuitIo(io)), ResponseStage::Sending);
        }
        assert!(!response.send().unwrap().sent());
        assert_eq!(response.refusal.sequence(), sequence);
        for _ in 0..filled {
            assert_eq!(receive(&far).unwrap(), vec![b'x'; 4096]);
        }
        assert!(receive(&far).is_none());
    }
    if mode == "response-send-clock" {
        let mut broken = AfterSendClock {
            inner: io,
            peer: &far,
            failed: false,
        };
        for _ in 0..128 {
            if response.advance(&mut broken) == ResponseStage::Stopped {
                break;
            }
        }
        assert!(broken.failed);
        assert_eq!(response.stage(), ResponseStage::Stopped);
        assert!(response.send().unwrap().sent());
        assert_eq!(response.send().unwrap().sent_at(), None);
        assert_eq!(receive(&far).unwrap(), expected);
        let first = response
            .refusal
            .owner
            .startup
            .context
            .failure()
            .unwrap()
            .clone();
        assert!(matches!(
            first,
            RunFailure::Flow {
                reason: FlowHaltReason::TraceFailed,
                ..
            }
        ));
        assert_eq!(response.advance(&mut CircuitIo(io)), ResponseStage::Stopped);
        let mut extra = [0; 1024];
        assert!(
            matches!(
                recv(far.as_fd().as_raw_fd(), &mut extra, MsgFlags::MSG_DONTWAIT),
                Ok(0) | Err(Errno::EAGAIN)
            ),
            "failed sender must not resend"
        );
        let (peer, _far) = delivered(&leave(), false);
        let bound = AbsoluteBound::after(io.now().unwrap(), 100000).unwrap();
        let mut rollback = match response.accept_rollback(peer, bound) {
            Ok(x) => x,
            Err(_) => panic!("retain actual Leave"),
        };
        assert_eq!(rollback.advance(&mut CircuitIo(io)), RollbackStage::Stopped);
        assert!(matches!(rollback.stop(),Some(RollbackStop::Run(error)) if *error==first));
        assert_eq!(rollback.response.refusal.sequence(), sequence);
        assert_eq!(rollback.response.refusal.source().detail.as_ptr(), source);
        assert_eq!(rollback.directive().envelope, leave());
        return;
    }
    if mode != "response-unsent-rollback" {
        for _ in 0..128 {
            if response.advance(&mut CircuitIo(io)) != ResponseStage::Sending {
                break;
            }
        }
        assert_eq!(
            response.stage(),
            ResponseStage::Complete,
            "{:?}",
            response.stop()
        );
        assert!(response.send().unwrap().sent());
        let sent_at = response.send().unwrap().sent_at().unwrap();
        assert!(sent_at >= original.began_ns() && !original.expired(sent_at));
        assert_eq!(receive(&far).unwrap(), expected);
        assert_eq!(
            response.advance(&mut CircuitIo(io)),
            ResponseStage::Complete
        );
        assert!(
            receive(&far).is_none(),
            "repeated completion resent an answer"
        );
    }
    if matches!(mode, "response-send" | "response-pressure") {
        return;
    }
    if mode == "response-invalid-leave" {
        for variant in 0..6 {
            let mut invalid = leave();
            match variant {
                0 => invalid.exchange.opener = Opener::Harness,
                1 => invalid.position = Position::Close,
                2 => {
                    if let weaver_types::Payload::Directive(LifecycleDirective::Leave {
                        rollback,
                        ..
                    }) = &mut invalid.payload
                    {
                        *rollback = false;
                    }
                }
                3 => {
                    invalid.payload = weaver_types::Payload::Directive(LifecycleDirective::Observe)
                }
                4 => {
                    invalid.payload = weaver_types::Payload::Directive(LifecycleDirective::Stop {
                        cause: weaver_types::Cause { uid: 1042 },
                    })
                }
                _ => {}
            }
            let (peer, _far) = delivered(&invalid, variant == 5);
            let bound = AbsoluteBound::after(io.now().unwrap(), 100000).unwrap();
            let rejected = match response.accept_rollback(peer, bound) {
                Err(error) => error,
                Ok(_) => panic!("invalid Leave accepted, variant {variant}"),
            };
            assert!(matches!(
                rejected.reason,
                RollbackJoinReason::WrongDirective
            ));
            assert_eq!(rejected.peer.held().unwrap().envelope, invalid);
            assert_eq!(rejected.peer.held().unwrap().sink.is_some(), variant == 5);
            assert_eq!(
                rejected
                    .response
                    .refusal
                    .owner
                    .startup
                    .context
                    .deadlines
                    .get(PumpDeadline::Enter),
                Some(original)
            );
            response = rejected.response;
        }
        return;
    }
    if mode == "response-late-rollback" {
        io.advance_to(original.deadline_ns() + 1).unwrap();
    }
    if mode == "response-retired-enter" {
        assert_eq!(
            response
                .refusal
                .owner
                .startup
                .context
                .complete_wait(PumpDeadline::Enter),
            Some(original)
        );
    }
    if mode == "response-replaced-enter" {
        let context = &mut response.refusal.owner.startup.context;
        context.deadlines.disarm(PumpDeadline::Enter);
        context
            .deadlines
            .arm(
                PumpDeadline::Enter,
                AbsoluteBound::after(io.now().unwrap(), 2_000_000_000).unwrap(),
            )
            .unwrap();
    }
    let mut directive = leave();
    if mode == "response-wire-unforced"
        && let weaver_types::Payload::Directive(LifecycleDirective::Leave { forced, .. }) =
            &mut directive.payload
    {
        *forced = false;
    }
    let (peer, _rollback_far) = delivered(&directive, false);
    let leave_bound = AbsoluteBound::after(io.now().unwrap(), 100000).unwrap();
    if mode == "response-replaced-enter" {
        let rejected = match response.accept_rollback(peer, leave_bound) {
            Err(x) => x,
            Ok(_) => panic!("replaced Enter accepted"),
        };
        assert!(matches!(rejected.reason, RollbackJoinReason::WrongWindow));
        assert_eq!(rejected.peer.held().unwrap().envelope, directive);
        assert_eq!(rejected.response.refusal.sequence(), sequence);
        return;
    }
    let mut rollback = match response.accept_rollback(peer, leave_bound) {
        Ok(x) => x,
        Err(e) => panic!("rollback join: {:?}", e.reason),
    };
    assert_eq!(rollback.directive().envelope, directive);
    assert!(rollback.forced());
    assert_eq!(rollback.leave(), leave_bound);
    if mode == "response-clock" {
        assert_eq!(rollback.advance(&mut BrokenClock), RollbackStage::Stopped);
        let RollbackStop::Run(first) = rollback.stop().unwrap() else {
            panic!("clock failure")
        };
        assert_eq!(
            rollback.response.refusal.owner.startup.context.failure(),
            Some(first)
        );
        assert_eq!(rollback.directive().envelope, directive);
        assert_eq!(rollback.response.refusal.sequence(), sequence);
        assert_eq!(rollback.response.refusal.source().detail.as_ptr(), source);
        assert!(
            rollback
                .response
                .refusal
                .owner
                .startup
                .context
                .deadlines
                .get(PumpDeadline::Leave)
                .is_none()
        );
        return;
    }
    assert_eq!(
        rollback.advance(&mut CircuitIo(io)),
        RollbackStage::RetireClassifier
    );
    assert_eq!(
        rollback
            .response
            .refusal
            .owner
            .startup
            .context
            .deadlines
            .get(PumpDeadline::Enter),
        None
    );
    assert_eq!(
        rollback
            .response
            .refusal
            .owner
            .startup
            .context
            .retired_enter,
        Some(original)
    );
    if mode == "response-leave-expiry" {
        io.advance_to(leave_bound.deadline_ns()).unwrap();
        assert_eq!(rollback.advance(&mut CircuitIo(io)), RollbackStage::Stopped);
        assert!(
            matches!(rollback.stop(), Some(RollbackStop::Run(RunFailure::Wait { key: PumpDeadline::Leave, bound })) if *bound == leave_bound)
        );
        assert_eq!(rollback.leave(), leave_bound);
        assert_eq!(rollback.directive().envelope, directive);
        return;
    }
    state.set_nonblocking(true).unwrap();
    let mut bytes = Vec::new();
    let mut answered = false;
    let mut expected_answer = None;
    for _ in 0..512 {
        let stage = rollback.advance(&mut CircuitIo(io));
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
        if stage == RollbackStage::Prepared {
            break;
        }
        if stage == RollbackStage::Stopped {
            if mode == "response-grants-refused" {
                assert!(matches!(
                    rollback.stop(),
                    Some(RollbackStop::Inspection(
                        InspectionCompletion::Refused { .. }
                    ))
                ));
                assert_eq!(
                    rollback.response.refusal.owner.startup.context.phase(),
                    RecordingPhase::Entered
                );
                assert!(
                    rollback
                        .response
                        .refusal
                        .owner
                        .startup
                        .context
                        .failure()
                        .is_none()
                );
                assert!(rollback.current().is_none());
                assert_eq!(rollback.directive().envelope, directive);
                return;
            }
            panic!("rollback stopped: {:?}", rollback.stop());
        }
        let mut chunk = [0; 8192];
        match state.read(&mut chunk) {
            Ok(0) => panic!("state closed before rollback inspection"),
            Ok(n) => bytes.extend_from_slice(&chunk[..n]),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => panic!("state receive: {e}"),
        }
        if bytes.ends_with(b"\n") {
            assert!(!answered, "rollback sent another state operation");
            let request = s::decode_state_line(&bytes, s::FrameDirection::Request).unwrap();
            assert!(matches!(
                &request,
                s::StateFrame::Control {
                    frame: s::StateControl::Grants {},
                    ..
                }
            ));
            let began = io.now().unwrap() + 1;
            let answer = s::StateFrame::ControlAnswer {
                common: request.common().clone(),
                frame: if mode == "response-grants-refused" {
                    s::StateControlAnswer::ControlRefused {
                        code: s::ControlRefusalCode::Read(s::ReadRefusalCode::AnswerTooLarge),
                        detail: "exact rollback grants refusal".into(),
                    }
                } else {
                    s::StateControlAnswer::Grants {
                        surface: vec!["read".into()],
                    }
                },
                observation: operation(&request, 2, began),
            };
            let encoded = s::encode_state_line(&answer, s::FrameDirection::Response).unwrap();
            io.advance_to(began + 2).unwrap();
            state.write_all(&encoded).unwrap();
            expected_answer = Some(encoded);
            bytes.clear();
            answered = true;
        }
    }
    assert_eq!(rollback.stage(), RollbackStage::Prepared);
    let InspectionCompletion::Grants { surface, evidence } = rollback.current().unwrap() else {
        panic!("actual Grants")
    };
    assert_eq!(surface, &["read".to_owned()]);
    assert_eq!(
        s::encode_state_line(evidence.answer(), s::FrameDirection::Response).unwrap(),
        expected_answer.unwrap()
    );
    assert_eq!(evidence.bound().deadline_ns(), leave_bound.deadline_ns());
    assert!(!evidence.bound().expired(evidence.recorded_ns()));
    assert_eq!(
        rollback.response.refusal.owner.startup.context.phase(),
        RecordingPhase::Releasing
    );
    assert!(
        rollback
            .response
            .refusal
            .owner
            .startup
            .context
            .flow
            .classifier_retired()
    );
    assert!(
        rollback
            .response
            .refusal
            .owner
            .startup
            .resources
            .classifier_child()
            .is_some()
    );
    assert!(
        rollback
            .response
            .refusal
            .owner
            .startup
            .resources
            .spu_child()
            .is_none()
    );
    assert_eq!(rollback.response.refusal.source().detail.as_ptr(), source);
    assert_eq!(rollback.response.refusal.sequence(), sequence);
    assert_eq!(rollback.directive().envelope, directive);
    assert!(
        receive(&far).is_none(),
        "rollback pumped the retired Enter response"
    );
    super::super::tests::drain(rollback.response.refusal.owner.startup.context, reader);
}

macro_rules! scenario {
    ($name:ident, $mode:literal) => {
        #[test]
        fn $name() {
            crate::flow::context::startup_composition::tests::refusal_join($mode);
        }
    };
}
scenario!(
    rollback_composition_sends_only_the_recorded_original_once,
    "response-send"
);
scenario!(
    rollback_composition_retains_actual_send_under_backpressure,
    "response-pressure"
);
scenario!(
    rollback_composition_busy_peer_returns_original_without_pumping_foreign_send,
    "response-busy"
);
scenario!(
    rollback_composition_expired_enter_does_not_stage_response,
    "response-expired"
);
scenario!(
    rollback_composition_records_current_grants_before_release,
    "response-rollback"
);
scenario!(
    rollback_composition_actual_rollback_can_retire_expired_enter,
    "response-late-rollback"
);
scenario!(
    rollback_composition_preserves_retired_enter_receipt,
    "response-retired-enter"
);
scenario!(
    rollback_composition_unsent_response_is_not_pumped_after_rollback,
    "response-unsent-rollback"
);
scenario!(
    rollback_composition_rejects_invalid_directive_and_returns_owned_handles,
    "response-invalid-leave"
);
scenario!(
    rollback_composition_rejects_substituted_enter,
    "response-replaced-enter"
);
scenario!(
    rollback_composition_clock_failure_retains_actual_leave_and_first_cause,
    "response-clock"
);
scenario!(
    rollback_composition_preserves_original_leave_expiry,
    "response-leave-expiry"
);
scenario!(
    rollback_composition_keeps_typed_grants_refusal_without_release,
    "response-grants-refused"
);

scenario!(
    rollback_composition_post_send_clock_failure_retains_sent_fact_and_halt,
    "response-send-clock"
);

scenario!(
    rollback_composition_rollback_is_forced_while_preserving_original_wire_bit,
    "response-wire-unforced"
);
