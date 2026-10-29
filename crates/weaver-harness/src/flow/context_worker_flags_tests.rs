//! Explicit samples through the real worker, SQLite member and recorder.
use super::*;
use crate::flow::context::worker_startup::input::halt::exit::flags_tests as failed;
use crate::{FoundationFlagObservation, FoundationFlagsRefusal};

pub(in crate::flow::context::worker_startup) enum Cleanup {
    Leave {
        handoff: Box<crate::FoundationEnteredRollbackHandoff>,
        peer: OwnedFd,
        sequence: weaver_trace::Sequence,
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
        return Some(Cleanup::Leave {
            handoff,
            peer: peer.unwrap(),
            sequence,
        });
    } else {
        if stop {
            let answer: OrganEnvelope =
                serde_json::from_slice(&read_packet(peer.as_ref().unwrap()).unwrap()).unwrap();
            assert_eq!(
                answer.payload,
                Payload::Answer(weaver_types::LifecycleAnswer::AtRest)
            );
            guard.input = Some(guard.management.take().unwrap().finish().ok().unwrap());
        }
        if mode.ends_with("recorded-failed") {
            return Some(Cleanup::Failed(failed::begin(guard, member, true)));
        }
        let input = guard.input.as_mut().unwrap();
        let taken = input.take_observed_flags().unwrap();
        assert_eq!(taken.sequence(), sequence);
        assert!(input.take_observed_flags().is_none());
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
        Some(crate::FoundationProcessExit::Success)
    );
    let (run, retained) = owner.test_account();
    assert!(run.context.failure().is_none());
    assert!(run.context.read.is_none());
    let sample = retained._flags.as_ref().unwrap();
    assert_eq!(sample.sequence(), sequence);
    assert_eq!(sample.observation(), FoundationFlagObservation::Cancelled);
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
    assert!(
        rows.iter().all(|r| r["kind"] != "state.plan.measurement"),
        "cleanup must not sample again"
    );
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
