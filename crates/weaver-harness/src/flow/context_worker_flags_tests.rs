//! Explicit samples through the real worker, SQLite member and recorder.
use super::*;
use crate::flow::context::worker_startup::input::halt::exit::flags_tests as failed;
use crate::{FoundationFlagObservation, FoundationFlagsRefusal};

pub(in crate::flow::context::worker_startup) struct CheckedTurn {
    root: std::path::PathBuf,
    wire: Vec<u8>,
    turn: TurnKey,
    run: String,
    source: s::SourceBlockId,
}

pub(in crate::flow::context::worker_startup) enum Cleanup {
    Leave {
        handoff: Box<crate::FoundationEnteredRollbackHandoff>,
        peer: OwnedFd,
        sequence: weaver_trace::Sequence,
        checked: Option<CheckedTurn>,
    },
    Failed(failed::Failure),
}

pub(in crate::flow::context::worker_startup) fn exercise(
    serving: FoundationServing<'_>,
    member: &mut Member,
    reader: &mut UnixStream,
    socket: &Path,
    mode: &str,
) -> Option<Cleanup> {
    let mut guard = sqlite_update(serving, member, reader, socket, false);
    let turn_boundary = mode.contains("-flags-turn-boundary");
    if turn_boundary {
        prepare_checked_turn(&mut guard, member, socket);
    }
    let summary = guard
        .input
        .as_mut()
        .unwrap()
        .serving
        .owner
        .context
        .committed_plan_summary()
        .unwrap()
        .unwrap()
        .clone();
    let plan = summary.plan.clone();
    let mut after = summary.last_source.block_id.clone();
    if mode.ends_with("unavailable") {
        after.sequence.0 += 1;
    }
    let input = guard.input.as_mut().unwrap();
    assert!(input.take_observed_flags().is_none());
    input.observe_flags(plan.clone(), after.clone()).unwrap();
    assert!(input.flags_pending());
    let original_input = turn_boundary.then(|| {
        assert_eq!(input.progress(), FoundationInputProgress::Pending);
        assert_eq!(
            input.serving.owner.context.open_turn,
            Some(weaver_trace::TurnRef("flags-boundary-turn".into()))
        );
        CheckedTurn {
            root: socket.parent().unwrap().to_path_buf(),
            wire: input.received.as_ref().unwrap().raw_bytes().to_vec(),
            turn: input.turn.clone().unwrap(),
            run: input.serving.owner.context.flow.run().to_owned(),
            source: input.seated.as_ref().unwrap().source.clone(),
        }
    });
    assert!(input.take_observed_flags().is_none());
    let refused = input
        .observe_flags(plan.clone(), after.clone())
        .unwrap_err();
    assert_eq!(refused.refusal, FoundationFlagsRefusal::Busy);
    assert_eq!(refused.plan, plan);
    assert_eq!(refused.after, after);
    let bound = input
        .serving
        .owner
        .context
        .deadlines
        .get(PumpDeadline::StateControl)
        .unwrap();
    if turn_boundary {
        guard.input = Some(match guard.input.take().unwrap().finish() {
            Err(input) => input,
            Ok(_) => panic!("a pending flag sample must retain the checked turn"),
        });
    }
    // A read does not mutate the plan or invalidate the last committed summary.
    assert!(
        matches!(observe(&mut guard, socket), s::enter::ObservationContent::Answer(
        s::enter::ObservationAnswer::State { plan: Some(ref shown), .. }) if shown == &summary)
    );
    assert!(guard.input.as_ref().unwrap().flags_pending());
    if mode.ends_with("flags-failed") {
        return Some(Cleanup::Failed(failed::begin(guard, member, false)));
    }
    let queued_save = mode
        .ends_with("queued-save")
        .then(|| queue_save(&mut guard, socket));
    let stop = mode.ends_with("stop");
    let leave = mode.ends_with("leave");
    let disconnect = mode.ends_with("disconnect-stop");
    let peer = if stop || leave {
        let peer = dial(socket);
        let request = OrganEnvelope {
            exchange: ExchangeId {
                opener: Opener::Admin,
                ordinal: 83,
            },
            position: Position::Open,
            payload: Payload::Directive(if stop {
                LifecycleDirective::Stop {
                    cause: Cause { uid: 0 },
                }
            } else {
                LifecycleDirective::Leave {
                    cause: Cause { uid: 0 },
                    forced: false,
                    rollback: true,
                }
            }),
        };
        let raw = serde_json::to_vec(&request).unwrap();
        assert_eq!(
            send(peer.as_raw_fd(), &raw, MsgFlags::MSG_NOSIGNAL).unwrap(),
            raw.len()
        );
        let end = Instant::now() + Duration::from_secs(5);
        while guard.input.as_mut().unwrap().advance() != FoundationInputProgress::ManagementPending
        {
            assert!(Instant::now() < end);
        }
        guard.management = Some(guard.input.take().unwrap().management().ok().unwrap());
        if disconnect {
            crate::flow::context::worker_startup::input::management::tests::disconnect_accepted_stop_during_drain(
                guard.management.as_mut().unwrap(),
                &peer,
            );
        }
        Some(peer)
    } else {
        None
    };
    let done = AtomicBool::new(false);
    let cpu_before = thread_cpu_ns();
    std::thread::scope(|scope| {
        let responder = scope.spawn(|| {
            // A real non-ready state socket must block without an eager polling loop.
            std::thread::sleep(Duration::from_millis(200));
            let end = Instant::now() + Duration::from_secs(10);
            while !done.load(Ordering::SeqCst) && Instant::now() < end {
                member.step_live("ready");
                std::thread::sleep(Duration::from_millis(1));
            }
        });
        struct Done<'a>(&'a AtomicBool);
        impl Drop for Done<'_> {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }
        let _done = Done(&done);
        let end = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(owner) = &mut guard.management {
                let progress = owner.advance();
                assert_ne!(
                    progress,
                    FoundationManagementProgress::Stopped,
                    "{:?}",
                    owner.detail()
                );
                if progress
                    == if leave {
                        FoundationManagementProgress::DirectivePending
                    } else if disconnect {
                        FoundationManagementProgress::Dropped
                    } else {
                        FoundationManagementProgress::Answered
                    }
                {
                    break;
                }
            } else {
                let input = guard.input.as_mut().unwrap();
                let progress = input.advance();
                assert_ne!(
                    progress,
                    FoundationInputProgress::Stopped,
                    "{:?}",
                    input.detail()
                );
                if !input.flags_pending() {
                    break;
                }
            }
            assert!(Instant::now() < end, "flag observation did not settle");
            assert!(
                thread_cpu_ns() - cpu_before < 120_000_000,
                "flag sampling spun on non-ready state socket"
            );
        }
        done.store(true, Ordering::SeqCst);
        responder.join().unwrap();
    });
    let input = if let Some(owner) = &mut guard.management {
        &mut *owner.input
    } else {
        guard.input.as_mut().unwrap()
    };
    assert!(!input.flags_pending());
    assert!(input.serving.owner.context.read.is_none());
    assert!(
        input
            .serving
            .owner
            .context
            .deadlines
            .get(PumpDeadline::StateControl)
            .is_none()
    );
    assert_eq!(
        input
            .serving
            .owner
            .context
            .committed_plan_summary()
            .unwrap(),
        Some(&summary)
    );
    let refused = input
        .observe_flags(plan.clone(), after.clone())
        .unwrap_err();
    assert_eq!(
        refused.refusal,
        FoundationFlagsRefusal::Busy,
        "an untaken result has custody"
    );
    let result = input.serving.flags.as_ref().expect("one retained result");
    assert!(result.request_id().0 > 0);
    assert!(result.sequence().0 > summary.last_source.block_id.sequence.0);
    let sequence = result.sequence();
    let request_id = result.request_id();
    if stop || leave {
        assert_eq!(result.observation(), FoundationFlagObservation::Cancelled);
    } else if mode.ends_with("unavailable") {
        assert_eq!(
            result.observation(),
            FoundationFlagObservation::Unavailable(s::ReadRefusalCode::NotFound)
        );
    } else {
        let FoundationFlagObservation::Measured(snapshot) = result.observation() else {
            panic!("normal member measured snapshot required")
        };
        assert_eq!(snapshot.plan, plan);
        assert_eq!(snapshot.after, after);
        assert_eq!(snapshot.total_flags, s::DecimalU64(0));
        assert!(snapshot.counts_agree());
    }
    let rows = trace(reader);
    let samples: Vec<_> = rows
        .iter()
        .filter(|row| row["kind"] == "state.plan.measurement")
        .collect();
    assert_eq!(samples.len(), 1, "Observe and Busy do not sample");
    let encoded = serde_json::to_value(samples[0]).unwrap();
    assert!(encoded.to_string().contains("plan-observation/1"));
    assert_eq!(
        samples[0]["sequence"].as_str().unwrap(),
        sequence.0.to_string()
    );
    let payload = &samples[0]["payload"];
    assert_eq!(
        payload["event"]["flags"]["request_id"],
        request_id.0.to_string()
    );
    assert!(input.serving.owner.context.last_ns < bound.deadline_ns());
    if let Some(original) = &original_input {
        assert_eq!(input.received.as_ref().unwrap().raw_bytes(), original.wire);
        assert_eq!(input.turn.as_ref(), Some(&original.turn));
        assert_eq!(input.seated.as_ref().unwrap().source, original.source);
        let started: Vec<_> = rows
            .iter()
            .filter(|row| row["kind"] == "turn.started")
            .collect();
        assert_eq!(started.len(), 1);
        assert!(trace_sequence(started[0]) < sequence.0);
        if stop {
            assert_turn_closed(&rows, sequence, &original.turn);
            assert!(input.serving.owner.context.open_turn.is_none());
        } else {
            assert_eq!(
                input
                    .serving
                    .owner
                    .context
                    .open_turn
                    .as_ref()
                    .map(|turn| &turn.0),
                Some(&original.turn.0)
            );
            assert!(rows.iter().all(|row| row["kind"] != "turn.closed"));
        }
    }
    if let Some((peer, window)) = &queued_save {
        let saved = input
            .serving
            .save
            .as_ref()
            .expect("original save remains queued");
        assert_eq!(
            input
                .serving
                .owner
                .context
                .deadlines
                .get(PumpDeadline::SaveQueued)
                .unwrap()
                .deadline_ns(),
            window.deadline_ns.0
        );
        assert_eq!(saved.request.message.envelope.exchange.ordinal, 97);
        assert!(
            read_packet(peer).is_none(),
            "sampling cannot start the save boundary"
        );
        assert!(rows.iter().all(|row| row["kind"] != "save_point"));
    }
    if leave {
        let handoff = match guard.management.take().unwrap().rollback_handoff() {
            Ok(handoff) => handoff,
            Err(owner) => {
                guard.management = Some(owner);
                panic!("settled flags blocked original Leave")
            }
        };
        assert_eq!(handoff._flags.as_ref().unwrap().sequence(), sequence);
        assert_eq!(
            handoff._flags.as_ref().unwrap().observation(),
            FoundationFlagObservation::Cancelled
        );
        if let Some(original) = &original_input {
            assert_eq!(
                handoff.input.original_input().unwrap().raw_bytes(),
                original.wire
            );
            assert_eq!(
                handoff.input.test_checked_source(),
                Some((&original.turn, &original.source))
            );
        }
        return Some(Cleanup::Leave {
            handoff,
            peer: peer.unwrap(),
            sequence,
            checked: original_input,
        });
    } else {
        if stop {
            let expected = if turn_boundary {
                weaver_types::LifecycleAnswer::TurnAborted {
                    turn: original_input.as_ref().unwrap().turn.clone(),
                }
            } else {
                weaver_types::LifecycleAnswer::AtRest
            };
            if disconnect {
                crate::flow::context::worker_startup::input::management::tests::assert_undelivered_stop_answer(
                    guard.management.as_ref().unwrap(), expected,
                );
            } else {
                let answer: OrganEnvelope =
                    serde_json::from_slice(&read_packet(peer.as_ref().unwrap()).unwrap()).unwrap();
                assert_eq!(answer.payload, Payload::Answer(expected));
            }
            guard.input = Some(guard.management.take().unwrap().finish().ok().unwrap());
        }
        if mode.ends_with("recorded-failed") {
            return Some(Cleanup::Failed(failed::begin(guard, member, true)));
        }
        let input = guard.input.as_mut().unwrap();
        let taken = input.take_observed_flags().unwrap();
        assert_eq!(taken.sequence(), sequence);
        assert!(input.take_observed_flags().is_none());
        if let Some(original) = original_input {
            assert_eq!(input.received.as_ref().unwrap().raw_bytes(), original.wire);
            if stop {
                assert_eq!(input.progress(), FoundationInputProgress::Aborted);
                assert_world_close(&original, "the operator stopped the turn");
                assert_stop_keeps_gate(input, &original.root);
                return None;
            }
            assert_eq!(input.progress(), FoundationInputProgress::Turn);
            // The same checked turn becomes available only after the sample's
            // exact measurement above has crossed the recorder.
            assert_eq!(
                input.serving.owner.context.open_turn,
                Some(weaver_trace::TurnRef("flags-boundary-turn".into()))
            );
            let returned = guard
                .input
                .take()
                .unwrap()
                .finish()
                .ok()
                .expect("Recorded flags release the exact turn");
            let super::super::FoundationInputResult::Turn(turn) = returned else {
                panic!("the original checked turn must return")
            };
            assert_eq!(turn.turn(), original.turn);
            assert_eq!(turn.source(), &original.source);
            guard.input = Some(turn.into_service());
        }
    }
    None
}

pub(in crate::flow::context::worker_startup) fn finish(
    run: Box<crate::FoundationRun>,
    cleanup: Cleanup,
    member: &mut Member,
    reader: &mut UnixStream,
) {
    let Cleanup::Leave {
        handoff,
        peer,
        sequence,
        checked,
    } = cleanup
    else {
        let Cleanup::Failed(failure) = cleanup else {
            unreachable!()
        };
        return failed::finish(run, failure, reader);
    };
    use crate::flow::context::worker_startup::tests::{reap, worker};
    use std::io::Read;
    struct ExitGuard(Option<Box<crate::FoundationEnteredRollback>>);
    impl Drop for ExitGuard {
        fn drop(&mut self) {
            if let Some(owner) = &mut self.0 {
                owner.test_reap();
            }
        }
    }
    let done = AtomicBool::new(false);
    reader.set_nonblocking(true).unwrap();
    let (mut guard, bytes) = std::thread::scope(|scope| {
        let responder = scope.spawn(|| {
            while !done.load(Ordering::SeqCst) {
                member.step_live("rollback-ready");
                std::thread::sleep(Duration::from_millis(1));
            }
        });
        let recorder = scope.spawn(|| {
            let mut bytes = vec![];
            while !done.load(Ordering::SeqCst) {
                let _ = reader.read_to_end(&mut bytes);
                std::thread::sleep(Duration::from_millis(1));
            }
            let _ = reader.read_to_end(&mut bytes);
            bytes
        });
        struct Done<'a>(&'a AtomicBool);
        impl Drop for Done<'_> {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }
        let _done = Done(&done);
        let owner = match worker::entered_rollback(run, handoff) {
            Ok(owner) => owner,
            Err((mut run, mut handoff)) => {
                reap(handoff.arms.child.as_mut().unwrap());
                run.resources.reap_startup_test_children();
                panic!("original run refused flags rollback handoff")
            }
        };
        let guard = ExitGuard(Some(owner));
        done.store(true, Ordering::SeqCst);
        responder.join().unwrap();
        (guard, recorder.join().unwrap())
    });
    let owner = guard.0.as_mut().unwrap();
    assert_eq!(
        owner.progress(),
        crate::FoundationEnteredRollbackProgress::Closed,
        "{:?}",
        owner.detail()
    );
    assert_eq!(
        owner.process_exit(),
        Some(crate::FoundationProcessExit::Success),
        "{}",
        owner.test_exit_detail()
    );
    let (run, retained) = owner.test_account();
    assert!(run.context.failure().is_none());
    assert!(run.context.read.is_none());
    let sample = retained._flags.as_ref().unwrap();
    assert_eq!(sample.sequence(), sequence);
    assert_eq!(sample.observation(), FoundationFlagObservation::Cancelled);
    if let Some(original) = &checked {
        assert_eq!(
            retained.input.original_input().unwrap().raw_bytes(),
            original.wire
        );
        assert_eq!(
            retained.input.test_checked_source(),
            Some((&original.turn, &original.source))
        );
        assert!(run.context.open_turn.is_none());
        assert!(
            original.root.join("flags-turn-lowered").exists(),
            "actual gate must confirm Lower"
        );
        assert!(
            !original.root.join("flags-turn-close.json").exists(),
            "rollback leaves the retained world's disposition to Lower"
        );
    }
    member.assert_closed_without_request();
    let answer: OrganEnvelope = serde_json::from_slice(&read_packet(&peer).unwrap()).unwrap();
    assert!(matches!(
        answer.payload,
        Payload::Answer(weaver_types::LifecycleAnswer::Left {
            save_point: None,
            ..
        })
    ));
    let rows: Vec<serde_json::Value> = bytes
        .split(|b| *b == b'\n')
        .filter(|b| !b.is_empty())
        .map(|b| serde_json::from_slice(b).unwrap())
        .collect();
    assert_eq!(rows.iter().filter(|r| r["kind"] == "unload").count(), 1);
    if let Some(original) = &checked {
        assert_turn_closed(&rows, sequence, &original.turn);
    }
    assert!(
        rows.iter().all(|r| r["kind"] != "state.plan.measurement"),
        "cleanup must not sample again"
    );
}

fn prepare_checked_turn(guard: &mut Guard<'_>, member: &mut Member, socket: &Path) {
    std::fs::write(
        socket.parent().unwrap().join("flags-turn-request"),
        b"offer\n",
    )
    .unwrap();
    let input = guard.input.as_mut().unwrap();
    advance_input_with_member(input, member, FoundationInputProgress::Accepted);
    assert!(input.token.is_some());
    assert!(input.serving.owner.context.open_turn.is_none());
    input
        .start_turn(TurnKey("flags-boundary-turn".into()))
        .unwrap();
    advance_input_with_member(input, member, FoundationInputProgress::Turn);
}

fn advance_input_with_member(
    input: &mut FoundationInput<'_>,
    member: &mut Member,
    wanted: FoundationInputProgress,
) {
    let done = AtomicBool::new(false);
    std::thread::scope(|scope| {
        let responder = scope.spawn(|| {
            let end = Instant::now() + Duration::from_secs(10);
            while !done.load(Ordering::SeqCst) && Instant::now() < end {
                member.step_live("ready");
                std::thread::sleep(Duration::from_millis(1));
            }
        });
        struct Done<'a>(&'a AtomicBool);
        impl Drop for Done<'_> {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }
        let _done = Done(&done);
        let end = Instant::now() + Duration::from_secs(5);
        while input.progress() != wanted {
            let progress = input.advance();
            assert!(
                progress == FoundationInputProgress::Pending || progress == wanted,
                "input reached {progress:?} while waiting for {wanted:?}: {:?}",
                input.detail()
            );
            assert!(
                Instant::now() < end,
                "input did not reach {wanted:?}: {progress:?}, {:?}",
                input.detail()
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        done.store(true, Ordering::SeqCst);
        responder.join().unwrap();
    });
}

fn assert_turn_closed(rows: &[serde_json::Value], sample: weaver_trace::Sequence, turn: &TurnKey) {
    let closes: Vec<_> = rows
        .iter()
        .filter(|row| row["kind"] == "turn.closed")
        .collect();
    assert_eq!(
        closes.len(),
        1,
        "cancelled sample must close the original turn once"
    );
    assert_eq!(closes[0]["turn"], turn.0);
    assert!(trace_sequence(closes[0]) > sample.0);
}

fn trace_sequence(row: &serde_json::Value) -> u64 {
    row["sequence"].as_str().unwrap().parse().unwrap()
}

fn assert_world_close(original: &CheckedTurn, text: &str) {
    let path = original.root.join("flags-turn-close.json");
    let end = Instant::now() + Duration::from_secs(2);
    while !path.exists() {
        assert!(Instant::now() < end, "gate did not receive the turn close");
        std::thread::sleep(Duration::from_millis(1));
    }
    let raw = std::fs::read(path).unwrap();
    let envelope: OrganEnvelope = serde_json::from_slice(&raw).unwrap();
    let Payload::Frame(frame) = envelope.payload else {
        panic!("world frame required")
    };
    let world: serde_json::Value = serde_json::from_slice(&frame.octets().unwrap()).unwrap();
    assert_eq!(world["kind"], "stopped");
    assert_eq!(world["reason"], text);
    assert_eq!(world["turn"], original.turn.0);
    assert_eq!(world["run"], original.run);
}

fn assert_stop_keeps_gate(input: &FoundationInput<'_>, root: &Path) {
    let lower = root.join("flags-turn-lowered");
    assert!(!lower.exists(), "Stop must not lower the gate");
    let fence = OrganEnvelope {
        exchange: weaver_types::ExchangeId {
            opener: weaver_types::Opener::Harness,
            ordinal: u64::MAX,
        },
        position: Position::Open,
        payload: Payload::Directive(LifecycleDirective::Observe),
    };
    let bytes = serde_json::to_vec(&fence).unwrap();
    let end = Instant::now() + Duration::from_secs(2);
    loop {
        assert!(!lower.exists(), "Stop must not lower the gate");
        match send(
            input.serving.owner.gate.fd().as_raw_fd(),
            &bytes,
            MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_NOSIGNAL,
        ) {
            Ok(n) => {
                assert_eq!(n, bytes.len());
                break;
            }
            Err(nix::errno::Errno::EAGAIN | nix::errno::Errno::EINTR) => {}
            other => panic!("gate fixture fence: {other:?}"),
        }
        assert!(Instant::now() < end, "gate fixture fence did not send");
        std::thread::sleep(Duration::from_millis(1));
    }
    let checked = root.join("flags-turn-stop-checked");
    while !checked.exists() {
        assert!(!lower.exists(), "Stop must not lower the gate");
        assert!(
            Instant::now() < end,
            "gate did not acknowledge the Stop boundary"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(std::fs::read(checked).unwrap(), b"checked\n");
    assert!(!lower.exists(), "Stop must not lower the gate");
}

fn queue_save(guard: &mut Guard<'_>, socket: &Path) -> (OwnedFd, s::SaveWindow) {
    use s::SaveWindow;
    use s::capture::{CaptureDirective, CaptureEnvelope, CapturePayload};
    let peer = dial(socket);
    let began = crate::flow::clock::monotonic_ns().unwrap();
    let duration = guard
        .input
        .as_ref()
        .unwrap()
        .serving
        .owner
        .context
        .load_duty
        .as_ref()
        .unwrap()
        .persistence
        .save_wait
        .elapsed_ns
        .0;
    let window = SaveWindow {
        began_ns: began.into(),
        deadline_ns: (began + duration).into(),
    };
    let request = CaptureEnvelope {
        exchange: ExchangeId {
            opener: Opener::Admin,
            ordinal: 97,
        },
        position: Position::Open,
        payload: CapturePayload::Directive(CaptureDirective::SavePoint {
            cause: Cause { uid: 0 },
            window,
        }),
    };
    let raw = serde_json::to_vec(&request).unwrap();
    assert_eq!(
        send(peer.as_raw_fd(), &raw, MsgFlags::MSG_NOSIGNAL).unwrap(),
        raw.len()
    );
    let end = Instant::now() + Duration::from_secs(5);
    while guard.input.as_mut().unwrap().advance() != FoundationInputProgress::ManagementPending {
        assert!(Instant::now() < end);
    }
    guard.management = Some(guard.input.take().unwrap().management().ok().unwrap());
    while guard.management.as_mut().unwrap().advance()
        != FoundationManagementProgress::DirectivePending
    {
        assert!(Instant::now() < end);
    }
    guard.input = Some(match guard.management.take().unwrap().queue_save() {
        Ok(input) => input,
        Err(owner) => {
            guard.management = Some(owner);
            panic!("an active flag read must retain the accepted save")
        }
    });
    assert!(guard.input.as_ref().unwrap().flags_pending());
    (peer, window)
}
