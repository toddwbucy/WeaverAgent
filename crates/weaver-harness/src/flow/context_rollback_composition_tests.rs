use super::*;
#[path = "context_rollback_listener_tests.rs"]
mod listener_tests;
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

fn reading_buffer(completion: &InspectionCompletion) -> usize {
    match completion {
        InspectionCompletion::Grants { surface, .. } => surface.as_ptr() as usize,
        InspectionCompletion::Refused {
            failure: crate::flow::read::ReadFailure::Refused { detail, .. },
            ..
        } => detail.as_ptr() as usize,
        other => panic!("unexpected read-back: {other:?}"),
    }
}

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
    let (mut door, listener) = listener_tests::Door::new();
    let mut listener = Some(listener);
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
        let mut rollback = match response.accept_rollback(peer, listener.take().unwrap(), bound) {
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
            let rejected = match response.accept_rollback(peer, listener.take().unwrap(), bound) {
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
            listener = Some(rejected.listener);
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
    if mode == "response-listener-silent" {
        door.early = Some(door.connect());
        let context = &mut response.refusal.owner.startup.context;
        context
            .pump_once(&mut PumpPorts {
                lifecycle: LifecyclePorts {
                    listener: listener.as_mut(),
                    ..Default::default()
                },
                gate: None,
                decode: None,
                label: None,
                io: &mut CircuitIo(io),
            })
            .unwrap();
        let accepted = listener
            .as_ref()
            .unwrap()
            .accepted_ns()
            .expect("actual clocked accept");
        door.accepted = Some(accepted);
        io.advance_to(accepted + 500_000_000).unwrap();
    }
    let mut directive = leave();
    if mode == "response-wire-unforced"
        && let weaver_types::Payload::Directive(LifecycleDirective::Leave { forced, .. }) =
            &mut directive.payload
    {
        *forced = false;
    }
    let (peer, rollback_far) = delivered(&directive, false);
    let mut rollback_far = Some(rollback_far);
    let allowance = if mode == "response-listener-leave-expiry" {
        1_000_000_000
    } else if mode.starts_with("response-listener-") || mode.starts_with("response-halt-") {
        5_000_000_000
    } else {
        100000
    };
    let leave_bound = AbsoluteBound::after(io.now().unwrap(), allowance).unwrap();
    if mode == "response-replaced-enter" {
        let rejected = match response.accept_rollback(peer, listener.take().unwrap(), leave_bound) {
            Err(x) => x,
            Ok(_) => panic!("replaced Enter accepted"),
        };
        assert!(matches!(rejected.reason, RollbackJoinReason::WrongWindow));
        assert_eq!(rejected.peer.held().unwrap().envelope, directive);
        assert_eq!(rejected.response.refusal.sequence(), sequence);
        return;
    }
    let rollback = match response.accept_rollback(peer, listener.take().unwrap(), leave_bound) {
        Ok(x) => x,
        Err(e) => panic!("rollback join: {:?}", e.reason),
    };
    if mode == "response-peer-gone" {
        drop(rollback_far.take());
    }
    let mut rollback = match rollback.into_prepared() {
        Err(owner) => owner,
        Ok(_) => panic!("rollback granted preparation before reading current Grants"),
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
    if mode.starts_with("response-halt-") {
        super::halt::tests::drive(
            rollback,
            io,
            reader,
            state,
            mode,
            rollback_far.as_ref().unwrap(),
        );
        return;
    }
    if mode.starts_with("response-listener-") {
        listener_tests::drive(
            rollback,
            &mut door,
            io,
            reader,
            state,
            mode,
            rollback_far.as_ref().unwrap(),
        );
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
        if stage == RollbackStage::Release {
            rollback = match rollback.into_prepared() {
                Err(owner) => owner,
                Ok(_) => panic!("Grants alone granted preparation before the release transition"),
            };
        }
        if stage == RollbackStage::Stopped {
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
            if mode == "response-state-gone" {
                state.shutdown(std::net::Shutdown::Both).unwrap();
                for _ in 0..128 {
                    if rollback.advance(&mut CircuitIo(io)) == RollbackStage::Stopped {
                        break;
                    }
                }
                assert_eq!(rollback.stage(), RollbackStage::Stopped);
                assert!(matches!(rollback.stop(), Some(RollbackStop::Run(_))));
                assert!(rollback.current().is_none());
                assert_eq!(rollback.directive().envelope, directive);
                assert_eq!(rollback.leave(), leave_bound);
                assert!(
                    rollback.into_prepared().is_err(),
                    "state transport loss granted preparation"
                );
                return;
            }
            let began = io.now().unwrap() + 1;
            let answer = s::StateFrame::ControlAnswer {
                common: request.common().clone(),
                frame: if matches!(
                    mode,
                    "response-grants-refused" | "response-both-grants-refused"
                ) {
                    s::StateControlAnswer::ControlRefused {
                        code: s::ControlRefusalCode::Read(s::ReadRefusalCode::AnswerTooLarge),
                        detail: "exact rollback grants refusal".into(),
                    }
                } else {
                    s::StateControlAnswer::Grants {
                        surface: match mode {
                            "response-grants-same" => {
                                vec!["owner:actual".into(), "mode:0750".into()]
                            }
                            "response-grants-reordered" => {
                                vec!["mode:0750".into(), "owner:actual".into()]
                            }
                            "response-grants-empty" => vec![],
                            _ => vec!["read".into()],
                        },
                    }
                },
                observation: operation(&request, 3, began),
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
    let refused = matches!(
        mode,
        "response-grants-refused" | "response-both-grants-refused"
    );
    let evidence = match rollback.current().unwrap() {
        InspectionCompletion::Grants { surface, evidence } if !refused => {
            let expected: &[&str] = match mode {
                "response-grants-same" => &["owner:actual", "mode:0750"],
                "response-grants-reordered" => &["mode:0750", "owner:actual"],
                "response-grants-empty" => &[],
                _ => &["read"],
            };
            assert_eq!(surface, expected);
            evidence
        }
        InspectionCompletion::Refused { failure, evidence } if refused => {
            assert!(matches!(failure, crate::flow::read::ReadFailure::Refused {
                code: s::ReadRefusalCode::AnswerTooLarge, detail
            } if detail == "exact rollback grants refusal"));
            evidence
        }
        other => panic!("lost current Grants: {other:?}"),
    };
    let initial = match rollback.initial() {
        InspectionCompletion::Grants { surface, evidence }
            if !matches!(
                mode,
                "response-initial-grants-refused" | "response-both-grants-refused"
            ) =>
        {
            assert_eq!(
                surface,
                &["owner:actual".to_owned(), "mode:0750".to_owned()]
            );
            evidence
        }
        InspectionCompletion::Refused { failure, evidence }
            if matches!(
                mode,
                "response-initial-grants-refused" | "response-both-grants-refused"
            ) =>
        {
            assert!(
                matches!(failure, crate::flow::read::ReadFailure::Refused { code: s::ReadRefusalCode::AnswerTooLarge, detail } if detail == "exact initial Grants refusal")
            );
            evidence
        }
        other => panic!("lost initial Grants: {other:?}"),
    };
    assert_ne!(
        initial.answer().common().request_id,
        evidence.answer().common().request_id
    );
    assert!(initial.recorded_ns() < evidence.recorded_ns());

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
    let initial_ptr = reading_buffer(rollback.initial());
    let current_ptr = reading_buffer(rollback.current().unwrap());
    // Capture the actual payload buffers; moving the owner must not replace or
    // reconstruct either observation from caller-supplied summaries.
    let current_wire =
        s::encode_state_line(evidence.answer(), s::FrameDirection::Response).unwrap();
    let initial_wire = s::encode_state_line(initial.answer(), s::FrameDirection::Response).unwrap();
    super::super::tests::drain(rollback.response.refusal.owner.startup.context, reader);
    let prepared = match rollback.into_prepared() {
        Ok(owner) => owner,
        Err(_) => panic!("actual read-back did not produce its consuming handoff"),
    };
    let expected_surface = match mode {
        "response-grants-refused"
        | "response-both-grants-refused"
        | "response-initial-grants-refused" => weaver_trace::GrantSurface::Unreadable,
        "response-grants-same" => weaver_trace::GrantSurface::Unchanged,
        _ => weaver_trace::GrantSurface::Varied,
    };
    assert_eq!(prepared.grant_surface(), expected_surface);
    assert_eq!(prepared.leave(), leave_bound);
    assert_eq!(prepared.directive().envelope, directive);
    assert!(prepared.forced());
    assert_eq!(reading_buffer(prepared.initial()), initial_ptr);
    assert_eq!(reading_buffer(prepared.current()), current_ptr);
    let read_evidence = |completion: &InspectionCompletion| match completion {
        InspectionCompletion::Grants { evidence, .. }
        | InspectionCompletion::Refused { evidence, .. } => {
            s::encode_state_line(evidence.answer(), s::FrameDirection::Response).unwrap()
        }
        other => panic!("unexpected retained read-back: {other:?}"),
    };
    assert_eq!(read_evidence(prepared.initial()), initial_wire);
    assert_eq!(read_evidence(prepared.current()), current_wire);
    assert_eq!(prepared.owner.response.refusal.sequence(), sequence);
    assert_eq!(
        prepared.owner.response.refusal.source().detail.as_ptr(),
        source
    );
    assert_eq!(
        prepared
            .owner
            .response
            .refusal
            .owner
            .startup
            .context
            .deadlines
            .get(PumpDeadline::Leave),
        Some(leave_bound)
    );
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
    rollback_composition_records_typed_grants_refusal_and_prepares_unreadable_release,
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

scenario!(
    startup_grants_composition_preserves_initial_refusal_through_actual_rollback,
    "response-initial-grants-refused"
);

scenario!(
    rollback_readback_composition_same_actual_surfaces_are_unchanged,
    "response-grants-same"
);
scenario!(
    rollback_readback_composition_reordered_actual_surface_is_varied,
    "response-grants-reordered"
);
scenario!(
    rollback_readback_composition_empty_actual_surface_is_varied,
    "response-grants-empty"
);
scenario!(
    rollback_readback_composition_two_refusals_are_unreadable,
    "response-both-grants-refused"
);
scenario!(
    rollback_readback_composition_accepted_leave_survives_its_peer_eof,
    "response-peer-gone"
);

scenario!(
    rollback_readback_composition_state_loss_does_not_grant_preparation,
    "response-state-gone"
);

scenario!(rollback_halt_composition_state_eof, "response-halt-eof");

scenario!(
    rollback_halt_composition_state_timeout,
    "response-halt-timeout"
);

scenario!(
    rollback_halt_composition_partial_response,
    "response-halt-partial"
);

scenario!(
    rollback_halt_composition_pending_measurement_retained,
    "response-halt-pressure"
);

scenario!(
    rollback_halt_composition_pending_measurement_records_once,
    "response-halt-record"
);

scenario!(rollback_halt_composition_expired_leave, "response-halt-cap");

scenario!(
    rollback_halt_composition_broken_clock,
    "response-halt-clock"
);

scenario!(
    rollback_halt_composition_foreign_wait_rejected,
    "response-halt-foreign"
);

scenario!(
    rollback_halt_composition_original_peer_input_retained,
    "response-halt-extra"
);

scenario!(
    rollback_halt_composition_recorded_before_clock_failure,
    "response-halt-record-clock"
);
scenario!(
    rollback_halt_composition_owed_measurement_keeps_original_cap,
    "response-halt-record-expiry"
);
