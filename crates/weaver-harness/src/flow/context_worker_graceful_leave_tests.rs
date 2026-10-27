//! Actual root Leave, gate execution and world close. The member and model are
//! declared doubles. These drain cases do not qualify Quiesce or wind-down.
use super::emission::tests::{FAMILY, selection};
use super::tests::{step, until};
use super::*;
use crate::flow::clock::testing::FakePumpIo;
use crate::flow::context::worker_startup::input::management::{
    FoundationManagementProgress,
    tests::{Guard, dial, read_packet},
};
use crate::flow::context::worker_startup::tests::{Member, trace_through};
use nix::sys::socket::{MsgFlags, send};
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};
use weaver_types::{Cause, ExchangeId, LifecycleDirective, state_flow as s};

fn wait_file(root: &Path, name: &str) {
    let end = Instant::now() + Duration::from_secs(5);
    while !root.join(name).exists() {
        assert!(Instant::now() < end, "missing {name}");
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[allow(clippy::too_many_arguments)]
pub(in crate::flow::context::worker_startup) fn exercise(
    mut guard: Guard<'_>,
    member: &mut Member,
    reader: &mut UnixStream,
    clock: &mut FakePumpIo,
    socket: &Path,
    root: &Path,
    mode: &str,
) {
    let input = guard.input.as_mut().unwrap();
    input.repair(&FAMILY).unwrap();
    until(
        input,
        member,
        clock,
        mode,
        FoundationInputProgress::Effective,
    );
    let requests = input.effective_emission().unwrap().requests().to_vec();
    input
        .sort_outbound(selection(input.effective_emission().unwrap()))
        .unwrap();
    until(input, member, clock, mode, FoundationInputProgress::Sorted);
    if !mode.contains("graceful-new") {
        input.begin_pairs().unwrap();
        until(
            input,
            member,
            clock,
            mode,
            FoundationInputProgress::PairReady,
        );
        input.execute(requests[0].call_id.clone()).unwrap();
        for _ in 0..2000 {
            step(input, member, clock, mode);
            if input
                .serving
                .owner
                .context
                .pairs
                .executing()
                .is_some_and(|(_, _, sent)| sent)
            {
                break;
            }
        }
        assert!(
            input
                .serving
                .owner
                .context
                .pairs
                .executing()
                .is_some_and(|(_, _, sent)| sent)
        );
        wait_file(root, "external-2.json");
        if mode.contains("graceful-crossed") || mode.contains("graceful-result") {
            wait_file(root, "graceful-result-sent");
        }
    }
    let peer = dial(socket);
    let request = OrganEnvelope {
        exchange: ExchangeId {
            opener: Opener::Admin,
            ordinal: 701,
        },
        position: Position::Open,
        payload: Payload::Directive(LifecycleDirective::Leave {
            cause: Cause { uid: 74 },
            forced: false,
            rollback: false,
        }),
    };
    let raw = serde_json::to_vec(&request).unwrap();
    assert_eq!(
        send(peer.as_raw_fd(), &raw, MsgFlags::MSG_NOSIGNAL),
        Ok(raw.len())
    );
    for _ in 0..2000 {
        let p = step(guard.input.as_mut().unwrap(), member, clock, mode);
        if p == FoundationInputProgress::ManagementPending {
            break;
        }
        assert_ne!(p, FoundationInputProgress::Stopped);
    }
    guard.management = Some(guard.input.take().unwrap().management().ok().unwrap());
    for _ in 0..2000 {
        let p = guard.management.as_mut().unwrap().advance();
        member.step_live(mode);
        if p == FoundationManagementProgress::DirectivePending {
            break;
        }
        assert_eq!(p, FoundationManagementProgress::Pending);
    }
    let heard = guard.owned_input().serving.owner.context.last_ns;
    if mode.ends_with("delayed") {
        std::thread::sleep(Duration::from_millis(1050));
    }
    guard.input = Some(
        match guard.management.take().unwrap().defer_graceful_leave() {
            Ok(input) => input,
            Err(owner) => {
                guard.management = Some(owner);
                panic!("actual graceful Leave was not admitted");
            }
        },
    );
    let input = guard.input.as_mut().unwrap();
    let original = input.serving.graceful.as_ref().unwrap();
    assert_eq!(original.message.envelope, request);
    assert_eq!(original.cause.uid, 74);
    assert_eq!(original.heard_ns, heard);
    assert_eq!(original.wind_down_seconds, None);
    assert!(!original.forced);
    assert!(
        input
            .serving
            .owner
            .context
            .deadlines
            .get(PumpDeadline::Leave)
            .is_none()
    );
    assert!(!input.accepts_turn(&TurnKey("forbidden-turn".into())));
    assert!(input.start_task(TurnKey("forbidden-turn".into())).is_err());
    assert!(read_packet(&peer).is_none());

    if mode.ends_with("early-join") {
        let drain = original_drain(input);
        let joined = hear_join(&mut guard, member, clock, socket, mode);
        assert_eq!(
            guard.management.as_ref().unwrap().progress(),
            FoundationManagementProgress::DirectivePending
        );
        assert!(!drain.expired(guard.owned_input().serving.owner.context.last_ns));
        std::thread::sleep(Duration::from_millis(1050));
        guard.input = Some(
            guard
                .management
                .take()
                .unwrap()
                .join_graceful_leave()
                .ok()
                .unwrap(),
        );
        let input = guard.input.as_ref().unwrap();
        assert_eq!(input.progress(), FoundationInputProgress::LeavePending);
        assert_eq!(
            input
                .serving
                .graceful
                .as_ref()
                .unwrap()
                .forced_by
                .map(|cause| cause.uid),
            Some(81)
        );
        assert_eq!(input.serving.graceful.as_ref().unwrap().cause.uid, 74);
        assert!(input.serving.owner.context.failure().is_none());
        assert!(
            input
                .serving
                .owner
                .context
                .deadlines
                .get(PumpDeadline::Leave)
                .is_none()
        );
        assert!(read_packet(&joined).is_none());
        assert!(read_packet(&peer).is_none());
        return;
    }
    if mode.contains("graceful-expiry") {
        let drain = original_drain(input);
        assert_eq!(
            input
                .serving
                .owner
                .context
                .deadlines
                .get(PumpDeadline::GracefulDrain),
            Some(drain)
        );
        assert_eq!(drain.began_ns(), heard);
        assert_eq!(drain.deadline_ns() - heard, 1_000_000_000);
        let began = Instant::now();
        until(
            input,
            member,
            clock,
            mode,
            FoundationInputProgress::LeavePending,
        );
        if mode.ends_with("delayed") {
            assert!(
                began.elapsed() < Duration::from_millis(500),
                "handoff renewed the drain"
            );
        } else {
            assert!(began.elapsed() >= Duration::from_millis(700));
        }
        assert!(input.serving.graceful.as_ref().unwrap().forced);
        assert_eq!(input.serving.graceful.as_ref().unwrap().drain, Some(drain));
        assert!(input.failure.is_none());
        assert!(input.serving.owner.context.failure().is_none());
        assert!(
            input
                .serving
                .owner
                .context
                .deadlines
                .get(PumpDeadline::Leave)
                .is_none()
        );
        assert!(input.serving.owner.context.pairs.executing().is_some());
        assert!(!root.join("closed.json").exists());
        assert!(read_packet(&peer).is_none());
        let joined = hear_join(&mut guard, member, clock, socket, mode);
        guard.input = Some(
            guard
                .management
                .take()
                .unwrap()
                .join_graceful_leave()
                .ok()
                .unwrap(),
        );
        let leave = guard
            .input
            .as_ref()
            .unwrap()
            .serving
            .graceful
            .as_ref()
            .unwrap();
        assert!(leave.forced);
        assert_eq!(leave.forced_by, None);
        assert_eq!(leave.cause.uid, 74);
        assert_eq!(leave.joins.len(), 1);
        assert!(read_packet(&joined).is_none());
        return;
    }
    assert_eq!(input.serving.graceful.as_ref().unwrap().drain, None);
    if mode.contains("graceful-new") {
        input.begin_pairs().unwrap();
    }
    if mode.contains("graceful-held") {
        member.hold_kind = Some("tool.call.completed");
        for _ in 0..2000 {
            assert_eq!(
                step(input, member, clock, mode),
                FoundationInputProgress::Pending
            );
            if member.held_file.is_some() {
                break;
            }
        }
        assert!(member.held_file.is_some());
        assert!(!root.join("closed.json").exists());
        assert!(input.pair_outcome(&requests[0].call_id).is_none());
        assert!(read_packet(&peer).is_none());
        member.hold_kind = None;
    }
    if mode.contains("graceful-result") {
        until(
            input,
            member,
            clock,
            mode,
            FoundationInputProgress::PairsComplete,
        );
        let outcome = input.pair_outcome(&requests[0].call_id).unwrap();
        assert_eq!(outcome.fate, s::TerminalPairFate::Completed);
        assert_eq!(outcome.reason, None);
        input
            .place_results(
                vec![requests[0].call_id.clone()],
                "drained result".into(),
                CutMethod::Flush,
            )
            .unwrap();
        until(input, member, clock, mode, FoundationInputProgress::Placed);
        input.generate().unwrap();
        until(
            input,
            member,
            clock,
            mode,
            FoundationInputProgress::Generated,
        );
        input.repair(&FAMILY).unwrap();
        until(
            input,
            member,
            clock,
            mode,
            FoundationInputProgress::Effective,
        );
        assert!(input.effective_emission().unwrap().requests().is_empty());
        input
            .sort_outbound(selection(input.effective_emission().unwrap()))
            .unwrap();
        until(input, member, clock, mode, FoundationInputProgress::Sorted);
        input.begin_pairs().unwrap();
        until(
            input,
            member,
            clock,
            mode,
            FoundationInputProgress::PairsComplete,
        );
        input.close_answer().unwrap();
    }
    until(
        input,
        member,
        clock,
        mode,
        FoundationInputProgress::LeavePending,
    );
    wait_file(root, "closed.json");
    assert!(input.serving.owner.context.open_turn.is_none());
    assert!(!input.serving.graceful.as_ref().unwrap().forced);
    assert!(input.serving.owner.context.failure().is_none());
    assert!(!root.join("external-3.json").exists());
    if mode.contains("graceful-new") {
        assert!(!root.join("external-2.json").exists());
    } else if mode.contains("graceful-running") || mode.contains("graceful-held") {
        assert!(root.join("graceful-interrupt.json").exists());
    }
    assert!(read_packet(&peer).is_none(), "drain does not answer Left");
    let through = input.serving.owner.context.last_author_sequence.unwrap();
    reader.set_nonblocking(true).unwrap();
    let rows = trace_through(
        reader,
        vec![],
        through,
        Instant::now() + Duration::from_secs(5),
    )
    .unwrap();
    let terminals: Vec<_> = rows
        .iter()
        .filter(|row| {
            matches!(
                row["kind"].as_str(),
                Some("tool.call.completed" | "tool.call.failed")
            )
        })
        .collect();
    assert_eq!(terminals.len(), requests.len());
    for (index, row) in terminals.iter().enumerate() {
        assert_eq!(
            serde_json::from_value::<s::CallId>(row["payload"]["call_id"].clone()).unwrap(),
            requests[index].call_id
        );
        if index == 0 && (mode.contains("graceful-crossed") || mode.contains("graceful-result")) {
            assert_eq!(row["kind"], "tool.call.completed");
            assert!(row["payload"]["reason"].is_null());
        } else {
            assert_eq!(row["kind"], "tool.call.completed");
            assert_eq!(row["payload"]["fate"], "failed");
            let reason: s::PairFailure =
                serde_json::from_value(row["payload"]["reason"].clone()).unwrap();
            if index == 0 && (mode.contains("graceful-running") || mode.contains("graceful-held")) {
                assert_eq!(reason, s::PairFailure::Unload {});
                let outcome: weaver_types::ToolOutcome =
                    serde_json::from_value(row["payload"]["outcome"].clone()).unwrap();
                assert!(matches!(
                    outcome,
                    weaver_types::ToolOutcome::Killed {
                        by: weaver_types::KillCause::Unload,
                        ..
                    }
                ));
            } else {
                assert_eq!(
                    reason,
                    s::PairFailure::NotDispatched {
                        cause: NonDispatchCause::Unload
                    }
                );
            }
        }
    }
    let closes: Vec<_> = rows
        .iter()
        .filter(|row| row["kind"] == "turn.closed")
        .collect();
    assert_eq!(closes.len(), 1);
    if mode.contains("graceful-result") {
        assert_eq!(closes[0]["payload"]["close"], "clean");
        assert!(root.join("return-appended.json").exists());
    } else {
        assert_eq!(closes[0]["payload"]["reason"], "unload");
        assert_eq!(closes[0]["payload"]["cause"]["uid"], 74);
    }
    // The exact input/engine and pending Leave remain move-owned. A completed
    // turn alone cannot put this seat back into ordinary intake.
    guard.input = Some(match guard.input.take().unwrap().finish() {
        Err(input) => input,
        Ok(_) => panic!("pending Leave returned a serving seat"),
    });
}

fn original_drain(input: &FoundationInput<'_>) -> AbsoluteBound {
    input.serving.graceful.as_ref().unwrap().drain.unwrap()
}

fn hear_join(
    guard: &mut Guard<'_>,
    member: &mut Member,
    clock: &mut FakePumpIo,
    socket: &Path,
    mode: &str,
) -> std::os::fd::OwnedFd {
    let peer = dial(socket);
    let request = OrganEnvelope {
        exchange: ExchangeId {
            opener: Opener::Admin,
            ordinal: 702,
        },
        position: Position::Open,
        payload: Payload::Directive(LifecycleDirective::JoinLeave {
            cause: Cause { uid: 81 },
        }),
    };
    let bytes = serde_json::to_vec(&request).unwrap();
    assert_eq!(
        send(peer.as_raw_fd(), &bytes, MsgFlags::MSG_NOSIGNAL),
        Ok(bytes.len())
    );
    let end = Instant::now() + Duration::from_secs(3);
    loop {
        if step(guard.input.as_mut().unwrap(), member, clock, mode)
            == FoundationInputProgress::ManagementPending
        {
            break;
        }
        assert!(Instant::now() < end);
    }
    guard.management = Some(guard.input.take().unwrap().management().ok().unwrap());
    loop {
        let p = guard.management.as_mut().unwrap().advance();
        if p == FoundationManagementProgress::DirectivePending {
            break;
        }
        assert_eq!(p, FoundationManagementProgress::Pending);
        assert!(Instant::now() < end);
    }
    peer
}
