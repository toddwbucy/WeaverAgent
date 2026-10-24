//! Task admission keeps accepted input and the original bound through retry
//! or a real pre-turn refusal close. Peers are processes; models are fixtures.
use super::super::management::tests::{Guard, trace};
use super::tests::until;
use super::*;
use crate::flow::clock::{WaitInterest, WaitOutcome, testing::FakePumpIo};
use crate::flow::context::held_input_tests::{HOLDS, hold};
use crate::flow::context::startup_composition::tests::CircuitIo;
use crate::flow::context::worker_startup::tests::Member;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};

pub(super) fn assert_refused(input: &FoundationInput<'_>, root: &Path, rows: &[serde_json::Value]) {
    assert_eq!(input.progress(), FoundationInputProgress::Refused);
    let context = &input.serving.owner.context;
    assert!(context.failure().is_none());
    assert!(context.open_turn.is_none());
    assert!(
        context.input.snapshot().is_none(),
        "retire the refused input"
    );
    assert!(context.deadlines.get(PumpDeadline::Decision).is_none());
    assert!(input.serving.owner.certificate.is_some());
    assert!(
        input
            .operation_detail()
            .unwrap()
            .contains("InvalidTemplate")
    );
    for kind in [
        "turn.started",
        "turn.closed",
        "message.user",
        "state.mark",
        "message.assistant",
    ] {
        assert!(
            !rows.iter().any(|row| row["kind"] == kind),
            "no {kind} before a turn"
        );
    }
    assert!(!root.join("measured.json").exists());
    assert!(!root.join("appended.json").exists());
    let end = Instant::now() + Duration::from_secs(2);
    while !root.join("closed.json").exists() && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(1));
    }
    let close: OrganEnvelope =
        serde_json::from_slice(&std::fs::read(root.join("closed.json")).unwrap()).unwrap();
    assert_eq!(
        close.exchange,
        input.received.as_ref().unwrap().envelope().exchange
    );
    assert_eq!(close.position, Position::Close);
    let Payload::Frame(frame) = close.payload else {
        panic!("actual close")
    };
    let value: serde_json::Value = serde_json::from_slice(&frame.octets().unwrap()).unwrap();
    assert_eq!(
        value,
        serde_json::json!({"kind":"stopped","reason":"the operation was refused"})
    );
}

pub(super) fn next_input(
    guard: &mut Guard<'_>,
    member: &mut Member,
    clock: &mut FakePumpIo,
    mode: &str,
) {
    let FoundationInputResult::Refused(serving) =
        guard.input.take().unwrap().finish().ok().unwrap()
    else {
        panic!("refused input returns serving")
    };
    guard.input = Some(serving.input());
    let input = guard.input.as_mut().unwrap();
    until(
        input,
        member,
        clock,
        mode,
        FoundationInputProgress::Accepted,
    );
    assert_eq!(
        input.received.as_ref().unwrap().envelope().exchange.ordinal,
        102
    );
}

struct AdmissionWait<'a> {
    clock: &'a mut FakePumpIo,
    expected: Option<u64>,
    expire: bool,
    seen: bool,
}
impl PumpIo for AdmissionWait<'_> {
    fn now(&mut self) -> std::io::Result<u64> {
        self.clock.now()
    }
    fn wait(
        &mut self,
        _interests: &[WaitInterest<'_>],
        deadline: Option<u64>,
    ) -> std::io::Result<WaitOutcome> {
        let now = self.clock.now()?;
        if deadline != Some(now) {
            let bound = deadline.expect("Busy admission has a bound");
            if let Some(expected) = self.expected {
                assert_eq!(bound, expected, "original admission bound");
            }
            self.expected = Some(bound);
            self.seen = true;
            self.clock
                .advance_to(if self.expire { bound } else { now + 1 })?;
            if self.expire {
                return Ok(WaitOutcome::Timeout);
            }
        }
        Ok(WaitOutcome::Ready(vec![]))
    }
}

pub(super) fn exercise(
    serving: FoundationServing<'_>,
    member: &mut Member,
    reader: &mut UnixStream,
    clock: &mut FakePumpIo,
    root: &Path,
    mode: &str,
) {
    let mut guard = Guard {
        input: Some(serving.input()),
        management: None,
    };
    let input = guard.input.as_mut().unwrap();
    until(
        input,
        member,
        clock,
        mode,
        FoundationInputProgress::Accepted,
    );
    let original = input.received.as_ref().unwrap().raw_bytes().to_vec();
    input.start_task(TurnKey("generation-turn".into())).unwrap();
    if mode.contains("task-admission-busy") {
        let mut io = AdmissionWait {
            clock,
            expected: None,
            expire: mode.contains("deadline"),
            seen: false,
        };
        for slot in HOLDS {
            let held = hold(input.serving.owner.context, slot);
            for _ in 0..3 {
                let p = input.advance_with_io(&mut io);
                assert!(io.seen, "Busy must enter the common bounded wait");
                held.assert_held(input.serving.owner.context);
                assert!(input.token.is_some());
                assert!(input.serving.owner.context.open_turn.is_none());
                assert!(input.serving.owner.context.marks.is_none());
                assert_eq!(input.received.as_ref().unwrap().raw_bytes(), original);
                if io.expire {
                    assert_eq!(p, FoundationInputProgress::Stopped);
                    assert!(matches!(
                        input.serving.owner.context.failure(),
                        Some(RunFailure::Flow {
                            reason: FlowHaltReason::DecisionBound,
                            ..
                        })
                    ));
                    assert!(!root.join("closed.json").exists());
                    held.take(input.serving.owner.context);
                    return;
                }
                assert_eq!(
                    p,
                    FoundationInputProgress::Pending,
                    "{:?}",
                    input.operation_detail()
                );
                assert!(
                    input
                        .serving
                        .owner
                        .context
                        .input
                        .validate_accepted(
                            input.token.as_ref().unwrap(),
                            input.serving.owner.context.flow.run(),
                            &input.serving.owner.context.declaration.scheme.scheme_id
                        )
                        .is_ok()
                );
                assert_eq!(
                    input
                        .serving
                        .owner
                        .context
                        .deadlines
                        .get(PumpDeadline::Decision)
                        .unwrap()
                        .deadline_ns(),
                    io.expected.unwrap()
                );
            }
            held.take(input.serving.owner.context);
        }
        until(input, member, io.clock, mode, FoundationInputProgress::Task);
        assert!(input.token.is_none());
        assert!(input.serving.owner.context.failure().is_none());
        let rows = trace(reader);
        assert_eq!(
            rows.iter().filter(|r| r["kind"] == "turn.started").count(),
            1
        );
        assert_eq!(
            rows.iter().filter(|r| r["kind"] == "message.user").count(),
            1
        );
        assert_eq!(rows.iter().filter(|r| r["kind"] == "state.mark").count(), 1);
        assert!(!root.join("measured.json").exists());
        return;
    }
    until(
        input,
        member,
        clock,
        mode,
        FoundationInputProgress::OperationRefused,
    );
    assert!(input.token.is_some());
    let bound = input
        .serving
        .owner
        .context
        .deadlines
        .get(PumpDeadline::Decision)
        .expect("admission refusal keeps its bound");
    if mode.contains("revoked-election") {
        input.serving.owner.context.cancel_input(clock).unwrap();
        let sequence = input.serving.owner.context.last_author_sequence;
        assert!(input.close_operation_refusal().is_err());
        assert!(input.token.is_some());
        assert_eq!(input.serving.owner.context.last_author_sequence, sequence);
        assert!(!root.join("closed.json").exists());
        return;
    }
    if mode.contains("proof") {
        input.serving.owner.context.open_turn = Some(TurnRef("foreign-turn".into()));
        assert!(input.close_operation_refusal().is_err());
        assert!(
            input.token.is_some(),
            "failed close election retains accepted input"
        );
        input.serving.owner.context.open_turn = None;
    }
    input.close_operation_refusal().unwrap();
    assert!(input.close_operation_refusal().is_err());
    if mode.contains("revoked-use") {
        input.serving.owner.context.cancel_input(clock).unwrap();
        assert_eq!(
            input.advance_with_io(&mut CircuitIo(clock)),
            FoundationInputProgress::Stopped
        );
        assert!(matches!(
            input.serving.owner.context.failure(),
            Some(RunFailure::Flow {
                reason: FlowHaltReason::ProtocolFailed,
                ..
            })
        ));
        assert!(!root.join("closed.json").exists());
        return;
    }
    if mode.contains("-gate-") {
        gate_pressure(
            input,
            clock,
            root,
            bound.deadline_ns(),
            mode.contains("deadline"),
        );
        if mode.contains("deadline") {
            return;
        }
    }
    until(input, member, clock, mode, FoundationInputProgress::Refused);
    let rows = trace(reader);
    assert_refused(input, root, &rows);
    next_input(&mut guard, member, clock, mode);
}

fn gate_pressure(
    input: &mut FoundationInput<'_>,
    clock: &mut FakePumpIo,
    root: &Path,
    deadline: u64,
    expire: bool,
) {
    use crate::flow::context::worker_startup::input::management::stop::tests::fill;
    use std::os::fd::AsRawFd;
    fill(input.serving.owner.gate.fd().as_raw_fd());
    let now = clock.now().unwrap();
    clock.advance_to(now + 100).unwrap();
    struct Wait<'a> {
        clock: &'a mut FakePumpIo,
        root: &'a Path,
        deadline: u64,
        expire: bool,
        seen: bool,
    }
    impl PumpIo for Wait<'_> {
        fn now(&mut self) -> std::io::Result<u64> {
            self.clock.now()
        }
        fn wait(
            &mut self,
            interests: &[WaitInterest<'_>],
            deadline: Option<u64>,
        ) -> std::io::Result<WaitOutcome> {
            if deadline != Some(self.clock.now()?) && !self.seen {
                assert_eq!(
                    deadline,
                    Some(self.deadline),
                    "pre-turn close retains admission's bound"
                );
                assert!(!self.root.join("closed.json").exists());
                self.seen = true;
                if self.expire {
                    self.clock.advance_to(self.deadline)?;
                    return Ok(WaitOutcome::Timeout);
                }
                std::fs::write(self.root.join("drain"), b"yes")?;
            }
            CircuitIo(self.clock).wait(interests, deadline)
        }
    }
    let mut io = Wait {
        clock,
        root,
        deadline,
        expire,
        seen: false,
    };
    assert_eq!(
        input.advance_with_io(&mut io),
        FoundationInputProgress::Pending
    );
    let snapshot = input
        .serving
        .owner
        .context
        .input
        .snapshot()
        .expect("retain input through gate pressure");
    assert!(
        snapshot.cancelled,
        "revoke the accepted capability before its close"
    );
    assert!(!snapshot.response_sent);
    for _ in 0..2000 {
        let p = input.advance_with_io(&mut io);
        if io.seen || p == FoundationInputProgress::Stopped {
            break;
        }
    }
    assert!(io.seen, "actual gate pressure must reach the bounded wait");
    if expire {
        assert_eq!(input.progress(), FoundationInputProgress::Stopped);
        assert!(matches!(
            input.serving.owner.context.failure(),
            Some(RunFailure::Flow {
                reason: FlowHaltReason::DecisionBound,
                ..
            })
        ));
        assert!(input.serving.owner.context.input.snapshot().is_some());
        assert!(!root.join("closed.json").exists());
        std::fs::write(root.join("drain"), b"yes").unwrap();
    }
}
