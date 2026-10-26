//! Later client frames must leave the shared gate receive slot without
//! receiving a check identity or deadline before serialized input intake.
use super::tests::{step, until};
use super::*;
use crate::flow::clock::testing::FakePumpIo;
use crate::flow::context::startup_composition::tests::CircuitIo;
use crate::flow::context::worker_startup::input::management::{
    FoundationManagementProgress,
    tests::{Guard, dial, trace},
};
use crate::flow::context::worker_startup::tests::Member;
use nix::sys::socket::{MsgFlags, send};
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};
use weaver_types::ExchangeId;

pub(super) fn count(mode: &str) -> usize {
    if mode.contains("capacity") { 63 } else { 3 }
}

pub(super) fn queue_packets(mode: &str, root: &Path, fd: i32) {
    for index in 0..count(mode) {
        let last = index + 1 == count(mode);
        let content = if last {
            br#"{"text":"Read next."}"#.to_vec()
        } else {
            format!("unparsed queued input {index}").into_bytes()
        };
        let ordinal = if mode.contains("duplicate-current") && index == 0 {
            101
        } else if mode.contains("duplicate-queued") && index == 1 {
            102
        } else {
            102 + index as u64
        };
        let frame = OrganEnvelope {
            exchange: ExchangeId {
                opener: Opener::Gate,
                ordinal,
            },
            position: if mode.contains("not-open") && index == 0 {
                Position::Continue
            } else {
                Position::Open
            },
            payload: Payload::Frame(weaver_types::TurnFrame::carry_from(&content, 42)),
        };
        let raw = format!(
            " \t\r\n{}\n\t ",
            serde_json::to_string_pretty(&frame)
                .unwrap()
                .replace('\n', "\r\n")
        );
        std::fs::write(root.join(format!("queued-{index}.json")), &raw).unwrap();
        assert_eq!(
            send(fd, raw.as_bytes(), MsgFlags::MSG_NOSIGNAL),
            Ok(raw.len())
        );
    }
    std::fs::write(root.join("queued-sent"), b"yes").unwrap();
}

pub(super) fn fresh_packet(mode: &str, root: &Path, fd: i32) {
    let frame = OrganEnvelope {
        exchange: ExchangeId {
            opener: Opener::Gate,
            ordinal: 102 + count(mode) as u64,
        },
        position: Position::Open,
        payload: Payload::Frame(weaver_types::TurnFrame::carry_from(b"newer input", 42)),
    };
    let raw = serde_json::to_vec(&frame).unwrap();
    std::fs::write(root.join("queued-fresh.json"), &raw).unwrap();
    assert_eq!(send(fd, &raw, MsgFlags::MSG_NOSIGNAL), Ok(raw.len()));
}

pub(super) fn wait_queued(
    input: &mut FoundationInput<'_>,
    member: &mut Member,
    clock: &mut FakePumpIo,
    mode: &str,
) {
    for _ in 0..1000 {
        assert_eq!(
            step(input, member, clock, mode),
            FoundationInputProgress::Pending
        );
        if input.serving.queued_inputs.len() == count(mode) {
            break;
        }
    }
    assert_eq!(input.serving.queued_inputs.len(), count(mode));
    assert_eq!(
        input.serving.gate_exchanges.len(),
        1,
        "queued frames are not check intake"
    );
    assert!(
        input
            .serving
            .owner
            .context
            .deadlines
            .get(PumpDeadline::Gate)
            .is_none()
    );
    assert!(
        input
            .serving
            .owner
            .context
            .deadlines
            .get(PumpDeadline::Decision)
            .is_none()
    );
}

pub(super) fn invalid(
    input: &mut FoundationInput<'_>,
    member: &mut Member,
    clock: &mut FakePumpIo,
    root: &Path,
    mode: &str,
) {
    until(input, member, clock, mode, FoundationInputProgress::Stopped);
    assert!(matches!(
        input.serving.owner.context.failure(),
        Some(RunFailure::Flow {
            reason: FlowHaltReason::ProtocolFailed,
            ..
        })
    ));
    assert!(input.serving.owner.context.pairs.executing().is_some());
    assert!(input.serving.owner.context.pairs.next_terminal().is_none());
    let index = usize::from(mode.contains("duplicate-queued"));
    assert_eq!(input.serving.queued_inputs.len(), index);
    assert_eq!(input.serving.gate_exchanges.len(), 1);
    assert!(
        input.serving.owner.gate.held().is_some(),
        "invalid original stays in its port"
    );
    let original = input.serving.owner.gate.take_received().unwrap().unwrap();
    assert_eq!(
        original.raw_bytes(),
        std::fs::read(root.join(format!("queued-{index}.json"))).unwrap()
    );
    assert!(!root.join("external-3.json").exists());
    assert!(!root.join("closed.json").exists());
}

pub(super) fn leave(
    mut guard: Guard<'_>,
    member: &mut Member,
    clock: &mut FakePumpIo,
    socket: &Path,
    root: &Path,
    mode: &str,
) {
    let input = guard.input.as_mut().unwrap();
    wait_queued(input, member, clock, mode);
    let peer = dial(socket);
    let command = OrganEnvelope {
        exchange: ExchangeId {
            opener: Opener::Admin,
            ordinal: 98,
        },
        position: Position::Open,
        payload: Payload::Directive(weaver_types::LifecycleDirective::Leave {
            cause: weaver_types::Cause { uid: 73 },
            forced: false,
            rollback: false,
        }),
    };
    let raw = serde_json::to_vec(&command).unwrap();
    assert_eq!(
        send(peer.as_raw_fd(), &raw, MsgFlags::MSG_NOSIGNAL),
        Ok(raw.len())
    );
    until(
        input,
        member,
        clock,
        mode,
        FoundationInputProgress::ManagementPending,
    );
    guard.management = Some(guard.input.take().unwrap().management().ok().unwrap());
    let management = guard.management.as_mut().unwrap();
    for _ in 0..1000 {
        let progress = management.advance_with_io(&mut CircuitIo(clock));
        member.step(clock.now().unwrap(), mode);
        if progress == FoundationManagementProgress::DirectivePending {
            break;
        }
    }
    assert_eq!(
        management.progress(),
        FoundationManagementProgress::DirectivePending
    );
    assert_eq!(guard.owned_input().serving.queued_inputs.len(), count(mode));
    for (index, received) in guard.owned_input().serving.queued_inputs.iter().enumerate() {
        assert_eq!(
            received.raw_bytes(),
            std::fs::read(root.join(format!("queued-{index}.json"))).unwrap()
        );
    }
    guard.management = Some(
        guard
            .management
            .take()
            .unwrap()
            .finish()
            .err()
            .expect("Leave retains the complete input owner"),
    );
    assert!(!root.join("closed.json").exists());
}

/// Complete each queued malformed input through its real refused close,
/// then accept the last one. A newer socket frame must stay behind this FIFO.
pub(super) fn handoff(
    guard: &mut Guard<'_>,
    member: &mut Member,
    reader: &mut UnixStream,
    clock: &mut FakePumpIo,
    root: &Path,
    mode: &str,
) {
    let input = guard.input.as_ref().unwrap();
    assert_eq!(input.serving.queued_inputs.len(), count(mode));
    assert_eq!(input.serving.gate_exchanges.len(), 1);
    assert!(
        input
            .serving
            .owner
            .context
            .deadlines
            .get(PumpDeadline::Gate)
            .is_none()
    );
    let through = input.serving.owner.context.last_author_sequence.unwrap();
    if !mode.starts_with("input-worker-") {
        let elapsed = input
            .serving
            .owner
            .context
            .declaration
            .gate_wait
            .elapsed_ns
            .0;
        clock
            .advance_to(input.serving.owner.context.last_ns + elapsed + 1)
            .unwrap();
    }
    // The gate receives the original close and sends a newer frame. Make its
    // arrival deterministic before testing FIFO priority at the next intake.
    let end = Instant::now() + Duration::from_secs(2);
    while !root.join("queued-fresh.json").exists() && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(root.join("queued-fresh.json").exists());
    for index in 0..count(mode) {
        let serving = match guard.input.take().unwrap().finish().ok().unwrap() {
            FoundationInputResult::Completed(serving)
            | FoundationInputResult::Aborted(serving)
            | FoundationInputResult::Refused(serving) => serving,
            _ => panic!("completed world exchange returns the serving owner"),
        };
        guard.input = Some(serving.input());
        let input = guard.input.as_mut().unwrap();
        until(
            input,
            member,
            clock,
            mode,
            if index + 1 == count(mode) {
                FoundationInputProgress::Accepted
            } else {
                FoundationInputProgress::Refused
            },
        );
        let original = input.received.as_ref().unwrap();
        assert_eq!(
            original.raw_bytes(),
            std::fs::read(root.join(format!("queued-{index}.json"))).unwrap()
        );
        assert_eq!(original.envelope().exchange.ordinal, 102 + index as u64);
        assert!(input.serving.owner.context.open_turn.is_none());
        assert!(input.serving.owner.context.failure().is_none());
        assert!(
            input
                .serving
                .owner
                .context
                .deadlines
                .get(PumpDeadline::Gate)
                .is_none()
        );
    }
    let input = guard.input.as_ref().unwrap();
    assert!(input.serving.queued_inputs.is_empty());
    assert_eq!(
        input.serving.owner.gate.held().unwrap().exchange.ordinal,
        102 + count(mode) as u64
    );
    let rows = trace(reader);
    let offered: Vec<_> = rows
        .iter()
        .filter(|row| row["kind"] == "input.offered")
        .collect();
    assert_eq!(offered.len(), count(mode));
    assert!(
        offered
            .iter()
            .all(|row| super::emission::tests::sequence(row) > through.0)
    );
    assert!(
        !rows
            .iter()
            .any(|row| row["kind"] == "turn.started" || row["kind"] == "message.user")
    );
}
