//! Actual S9 sockets: Observe, Stop, demand save, then a fresh sole Leave.
use super::*;
use crate::flow::clock::{AbsoluteBound, PumpIo, WaitInterest, WaitOutcome};
use weaver_types::state_flow::enter::{ObservationAnswer, ObservationContent, ObservationEnvelope};

fn refused_dial(socket: &Path, bytes: Option<&[u8]>) {
    let began = Instant::now();
    let peer = dial(socket);
    if let Some(bytes) = bytes {
        assert_eq!(
            send(peer.as_raw_fd(), bytes, MsgFlags::MSG_NOSIGNAL),
            Ok(bytes.len())
        );
    }
    let mut byte = [0];
    loop {
        match recv(peer.as_raw_fd(), &mut byte, MsgFlags::MSG_DONTWAIT) {
            Ok(0) => break,
            Err(nix::errno::Errno::EAGAIN) => (),
            other => panic!("invalid S9 dial must close unanswered: {other:?}"),
        }
        assert!(
            began.elapsed() < Duration::from_secs(5),
            "S9 dial blocked later management"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    if bytes.is_none() {
        assert!(began.elapsed() >= Duration::from_millis(crate::state::ANSWER_BOUND_MS));
    }
}

fn expired_save(socket: &Path, save_wait: u64) -> s::SaveWindow {
    let peer = dial(socket);
    let now = crate::flow::clock::monotonic_ns().unwrap();
    let window = s::SaveWindow {
        began_ns: (now - save_wait - 1).into(),
        deadline_ns: (now - 1).into(),
    };
    let request = CaptureEnvelope {
        exchange: weaver_types::ExchangeId {
            opener: Opener::Admin,
            ordinal: 329,
        },
        position: Position::Open,
        payload: CapturePayload::Directive(CaptureDirective::SavePoint {
            cause: weaver_types::Cause { uid: 76 },
            window,
        }),
    };
    let wire = request.encode(CaptureSender::Admin).unwrap();
    assert_eq!(
        send(peer.as_raw_fd(), &wire, MsgFlags::MSG_NOSIGNAL),
        Ok(wire.len())
    );
    let answer =
        CaptureEnvelope::decode(&response(&peer, "expired save"), CaptureSender::Harness).unwrap();
    assert_eq!(answer.exchange, request.exchange);
    assert_eq!(answer.position, Position::Close);
    assert_eq!(
        answer.payload,
        CapturePayload::Refusal(CaptureRefusal::Save(SaveRefusal::SavePointNotTaken {
            missed: SavePointLeg::CaptureNotReady
        }))
    );
    window
}

fn response(peer: &OwnedFd, stage: &str) -> Vec<u8> {
    let end = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(bytes) = read_packet(peer) {
            return bytes;
        }
        assert!(Instant::now() < end, "S9 {stage} has no actual response");
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn saved(
    peer: &OwnedFd,
    forced: Option<bool>,
    previous: AbsoluteBound,
    held_root: Option<&Path>,
) -> s::DecimalU64 {
    let mut id = None;
    let mut artifact = None;
    let mut complete = false;
    let mut stage = "beginning";
    loop {
        let packet = CaptureEnvelope::decode(
            &response(
                peer,
                &format!(
                    "{} {stage}",
                    if forced.is_some() {
                        "Leave"
                    } else {
                        "demand save"
                    }
                ),
            ),
            CaptureSender::Harness,
        )
        .unwrap();
        match packet.payload {
            CapturePayload::SavePointBeginning(beginning) => {
                assert!(id.replace(beginning.capture_id).is_none());
                stage = "image";
                if forced.is_some() {
                    let tail = beginning.tail_window.unwrap();
                    assert!(tail.began_ns.0 > previous.began_ns());
                    assert_eq!(
                        tail.deadline_ns.0 - tail.began_ns.0,
                        weaver_types::LEAVE_BOUND_MS * 1_000_000
                    );
                } else {
                    assert!(beginning.tail_window.is_none());
                }
            }
            CapturePayload::SavePointImage(image) => {
                assert_eq!(Some(image.capture_id), id);
                if let Some(root) = held_root {
                    std::fs::write(root.join("s9-image-held"), b"actual demand Image received")
                        .unwrap();
                    wait_file(root, "s9-pin-ready");
                }
                let value = CaptureArtifact {
                    capture_id: image.capture_id,
                    artifact_id: format!(
                        "{:020}-{}",
                        if forced.is_some() { 3 } else { 2 },
                        "d".repeat(64)
                    ),
                    dump_sha256: s::Digest::new("c".repeat(64)).unwrap(),
                    config_sha256: s::Digest::new("d".repeat(64)).unwrap(),
                };
                reply(
                    peer,
                    &packet.exchange,
                    CapturePayload::SavePointImageChecked(CaptureImageChecked {
                        capture_id: image.capture_id,
                        dump_sha256: value.dump_sha256.clone(),
                    }),
                );
                artifact = Some(value);
                stage = "image finished";
            }
            CapturePayload::SavePointImageFinished(finished) => {
                stage = "commit";
                assert_eq!(Some(finished.capture_id), id);
                reply(
                    peer,
                    &packet.exchange,
                    CapturePayload::SavePointPrepared(artifact.as_ref().unwrap().clone()),
                );
            }
            CapturePayload::SavePointCommit(commit) => {
                stage = "publication confirmation";
                assert_eq!(Some(commit.capture_id), id);
                reply(
                    peer,
                    &packet.exchange,
                    CapturePayload::SavePointPublished(artifact.as_ref().unwrap().clone()),
                );
            }
            CapturePayload::SavePointCaptureComplete(value) => {
                assert!(forced.is_some() && !complete);
                assert_eq!(Some(value.capture_id), id);
                complete = true;
                stage = "Left";
            }
            CapturePayload::Answer(CaptureAnswer::SavePointTaken { report }) => {
                assert!(forced.is_none() && !complete);
                assert_eq!(Some(report.capture_id), id);
                return id.unwrap();
            }
            CapturePayload::Answer(CaptureAnswer::Left {
                save_point: Some(report),
                forced: actual,
                no_state: false,
            }) => {
                assert_eq!(Some(actual), forced);
                assert!(complete);
                assert_eq!(Some(report.capture_id), id);
                return id.unwrap();
            }
            other => panic!("S9 capture: {other:?}"),
        }
    }
}

/// Move past the ended Leave without sleeping for its 150 seconds. Numeric
/// readings and real kernel waits use the same offset; readiness is not faked.
struct AfterLeaveClock {
    offset: u64,
    waits: usize,
}
impl PumpIo for AfterLeaveClock {
    fn now(&mut self) -> std::io::Result<u64> {
        MonotonicIo
            .now()?
            .checked_add(self.offset)
            .ok_or_else(|| std::io::Error::other("S9 test clock overflow"))
    }
    fn wait(
        &mut self,
        interests: &[WaitInterest<'_>],
        deadline: Option<u64>,
    ) -> std::io::Result<WaitOutcome> {
        self.waits += 1;
        MonotonicIo.wait(
            interests,
            deadline.map(|end| end.saturating_sub(self.offset)),
        )
    }
}

fn failed_at_rest(
    owner: &mut ForcedGuard,
    member: &mut Member,
    reader: &mut UnixStream,
    root: &Path,
    mode: &str,
    previous: AbsoluteBound,
) {
    use crate::FoundationEnteredRollbackProgress as Progress;
    use crate::flow::context::failed_enter_driver::ChildState;
    use crate::flow::report::{FlowHaltReason, HaltPoint};

    let lower = std::fs::read(root.join("ready-rollback-lower.json")).unwrap();
    let requests = member.requests.clone();
    let trace_failed = mode.contains("-resting-trace-failure-");
    let mut clock = AfterLeaveClock {
        offset: (previous.deadline_ns() + 1_000_000_000).saturating_sub(MonotonicIo.now().unwrap()),
        waits: 0,
    };
    assert!(clock.now().unwrap() > previous.deadline_ns());
    let inner = owner.0.test_inner();
    assert_ne!(inner.advance_with_io(&mut clock), Progress::Closed);
    assert!(inner.test_context().failure().is_none());
    assert!(
        inner
            .test_context()
            .deadlines
            .get(PumpDeadline::Leave)
            .is_none()
    );
    if trace_failed {
        reader.shutdown(std::net::Shutdown::Read).unwrap();
        // Idle SPU loss owes an observation. Its actual write discovers the
        // broken trace sink while the resting owner still holds the run.
        inner.test_end_resting_spu();
    } else {
        member.stop_responses();
    }
    let end = Instant::now() + Duration::from_secs(10);
    let mut bytes = Vec::new();
    let mut first = None;
    let mut cap = None;
    loop {
        assert!(
            Instant::now() < end,
            "S9 failure cleanup stalled: {:?}",
            inner.detail()
        );
        let _ = reader.read_to_end(&mut bytes);
        let progress = inner.advance_with_io(&mut clock);
        if let Some(halt) = inner.test_context().halt() {
            let original = first.get_or_insert_with(|| halt.first().clone());
            assert_eq!(halt.first(), original);
            let deadline = halt.deadline().unwrap();
            assert_eq!(deadline, *cap.get_or_insert(deadline));
            assert_eq!(deadline, original.observed_ns.unwrap().0 + 150_000_000_000);
            assert!(deadline > previous.deadline_ns());
        }
        if progress == Progress::Closed {
            break;
        }
        if progress == Progress::Pending {
            inner.wait_with_io(&mut clock);
        }
    }
    let _ = reader.read_to_end(&mut bytes);
    let first = first.expect("actual component failure must latch");
    assert_eq!(
        first.reason,
        if trace_failed {
            FlowHaltReason::TraceFailed
        } else {
            FlowHaltReason::CarryFailed
        }
    );
    if !trace_failed {
        assert_eq!(first.point, HaltPoint::StateReply);
    }
    assert!(
        clock.waits > 0,
        "failure must traverse the actual transport wait"
    );
    assert_eq!(
        inner.abandoned(),
        trace_failed,
        "{:?}; secondary: {:?}",
        inner.detail(),
        inner.test_context().halt().unwrap().secondary()
    );
    let tail = inner
        .test_failed()
        .expect("failed-run owner must finish S9");
    assert!(tail.context().flow.standing_state().is_none());
    assert!(tail.context().flow.standing_classifier().is_none());
    if trace_failed {
        assert!(tail.resources().children_ended_or_signalled_for_test());
        assert!(tail.context().barrier.standing_failure().is_some());
    } else {
        for child in [
            tail.resources().classifier_child(),
            tail.resources().spu_child(),
        ] {
            assert!(matches!(child, Some(ChildState::Reaped(_))), "{child:?}");
        }
    }
    assert_eq!(member.requests, requests);
    member.assert_closed_without_request();
    assert_eq!(
        std::fs::read(root.join("ready-rollback-lower.json")).unwrap(),
        lower
    );
    if trace_failed {
        assert!(
            inner.unload_sequence().is_none(),
            "broken trace cannot claim Recorded Unload"
        );
    } else {
        assert!(inner.unload_sequence().is_some());
        let rows: Vec<serde_json::Value> = bytes
            .split(|b| *b == b'\n')
            .filter(|line| !line.is_empty())
            .map(|line| serde_json::from_slice(line).unwrap())
            .collect();
        assert_eq!(rows.last().unwrap()["kind"], "unload");
        assert_eq!(
            rows.iter()
                .filter(
                    |row| row["kind"] == "fault" && row["payload"]["case"] == "state_flow_failed"
                )
                .count(),
            1
        );
        assert!(rows.iter().all(|row| !matches!(
            row["kind"].as_str(),
            Some("save_point" | "turn.started" | "loop.instruction")
        )));
        assert_eq!(
            rows.last().unwrap()["payload"]["end_reason"],
            "component_failure"
        );
        assert_eq!(rows.last().unwrap()["payload"]["forced"], false);
        assert!(rows.last().unwrap()["payload"]["cause"].is_null());
    }
    if trace_failed {
        // Abandonment records termination attempts, not a fabricated reap.
        // Observe eventual actual reap separately after the failed owner closed.
        inner.test_reap();
        let resources = inner.test_failed().unwrap().resources();
        for child in [resources.classifier_child(), resources.spu_child()] {
            assert!(matches!(child, Some(ChildState::Reaped(_))), "{child:?}");
        }
    }
}

pub(super) fn exercise(
    owner: &mut ForcedGuard,
    member: &mut Member,
    reader: &mut UnixStream,
    socket: &Path,
    root: &Path,
    mode: &str,
    previous: AbsoluteBound,
) {
    assert_eq!(owner.0.progress(), LeaveProgress::CaptureRefused);
    if mode.contains("-resting-member-failure-") || mode.contains("-resting-trace-failure-") {
        failed_at_rest(owner, member, reader, root, mode, previous);
        return;
    }
    let lower = std::fs::read(root.join("ready-rollback-lower.json")).unwrap();
    let forced = mode.contains("-resting-force-");
    let held_reply = mode.contains("-resting-held-reply-");
    let spu_dead = mode.contains("-resting-spu-") || held_reply;
    if spu_dead && !held_reply {
        owner.0.test_inner().test_end_resting_spu();
    }
    let save_wait = owner
        .0
        .test_inner()
        .test_context()
        .load_duty
        .as_ref()
        .unwrap()
        .persistence
        .save_wait
        .elapsed_ns
        .0;
    let paced = mode.contains("-resting-paced-");
    let done = AtomicBool::new(false);
    let admin_done = AtomicBool::new(false);
    let (bytes, expired_window) = std::thread::scope(|scope| {
        let responder = scope.spawn(|| {
            let end = Instant::now() + Duration::from_secs(20);
            while !done.load(Ordering::SeqCst) && Instant::now() < end {
                member.step_live("input-worker-generation-emission-session-graceful-save");
                std::thread::sleep(Duration::from_millis(1));
            }
        });
        let recorder = scope.spawn(|| {
            let mut bytes = vec![];
            let end = Instant::now() + Duration::from_secs(20);
            while !done.load(Ordering::SeqCst) && Instant::now() < end {
                let _ = reader.read_to_end(&mut bytes);
                std::thread::sleep(Duration::from_millis(1));
            }
            let _ = reader.read_to_end(&mut bytes);
            bytes
        });
        let admin = scope.spawn(|| {
            if mode.contains("-resting-silent-") {
                refused_dial(socket, None);
            }
            if mode.contains("-resting-malformed-") {
                refused_dial(socket, Some(b"{"));
                refused_dial(socket, Some(br#"{"exchange":{"opener":"admin","ordinal":99},"position":"close","payload":{"kind":"directive","body":{"kind":"observe"}}}"#));
            }
            let expired_window = mode.contains("-resting-expired-save-")
                .then(|| expired_save(socket, save_wait));
            let peer = management(socket, LifecycleDirective::Observe);
            let answer = ObservationEnvelope::decode(&response(&peer, "Observe")).unwrap();
            assert!(matches!(
                answer.payload,
                ObservationContent::Answer(ObservationAnswer::State {
                    state: weaver_types::AgentState::Idle,
                    load: Some(_),
                    ..
                })
            ));
            let peer = management(
                socket,
                LifecycleDirective::Stop {
                    cause: weaver_types::Cause { uid: 75 },
                },
            );
            let answer: OrganEnvelope = serde_json::from_slice(&response(&peer, "Stop")).unwrap();
            assert_eq!(answer.payload, Payload::Answer(LifecycleAnswer::AtRest));

            let peer = dial(socket);
            let now = crate::flow::clock::monotonic_ns().unwrap();
            let ask = CaptureEnvelope {
                exchange: weaver_types::ExchangeId {
                    opener: Opener::Admin,
                    ordinal: 330,
                },
                position: Position::Open,
                payload: CapturePayload::Directive(CaptureDirective::SavePoint {
                    cause: weaver_types::Cause { uid: 76 },
                    window: s::SaveWindow {
                        began_ns: now.into(),
                        deadline_ns: (now + save_wait).into(),
                    },
                }),
            }
            .encode(CaptureSender::Admin)
            .unwrap();
            assert_eq!(
                send(peer.as_raw_fd(), &ask, MsgFlags::MSG_NOSIGNAL),
                Ok(ask.len())
            );
            let demand = saved(&peer, None, previous, held_reply.then_some(root));
            let peer = management(
                socket,
                LifecycleDirective::Leave {
                    cause: weaver_types::Cause { uid: 77 },
                    forced,
                    rollback: false,
                },
            );
            let retry = saved(&peer, Some(forced || spu_dead), previous, None);
            assert_ne!(demand, retry, "a retry owns a fresh capture identity");
            admin_done.store(true, Ordering::SeqCst);
            expired_window
        });
        let _stop = StopThreads(&done);
        let end = Instant::now() + Duration::from_secs(20);
        let mut pinned = false;
        loop {
            if held_reply && !pinned && root.join("s9-image-held").exists() {
                owner
                    .0
                    .test_inner()
                    .test_reduce_resting_reply_during_settlement(root);
                pinned = true;
            }
            if paced {
                // Let peer replies and writer notifications arrive between
                // owner steps; readiness must survive a slower caller.
                std::thread::sleep(Duration::from_millis(2));
            }
            let progress = owner.0.advance();
            assert!(
                Instant::now() < end,
                "S9 drive {:?}: {:?}",
                progress,
                owner.0.detail()
            );
            if progress == LeaveProgress::Closed {
                break;
            }
            if matches!(
                progress,
                LeaveProgress::Pending | LeaveProgress::CaptureRefused
            ) {
                assert!(
                    !owner.0.test_inner().test_resting_reply_held(),
                    "S9 must reduce a held capture reply before another blocking wait"
                );
                owner.0.wait_for_progress();
            }
        }
        assert_eq!(pinned, held_reply);
        let expired_window = admin.join().unwrap();
        assert!(admin_done.load(Ordering::SeqCst));
        done.store(true, Ordering::SeqCst);
        responder.join().unwrap();
        (recorder.join().unwrap(), expired_window)
    });
    assert!(!owner.0.abandoned(), "{:?}", owner.0.detail());
    assert!(owner.0.unload_sequence().is_some());
    assert_eq!(
        std::fs::read(root.join("ready-rollback-lower.json")).unwrap(),
        lower
    );
    let rows: Vec<serde_json::Value> = bytes
        .split(|b| *b == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice(line).unwrap())
        .collect();
    assert!(
        rows.iter()
            .all(|row| row["kind"] != "turn.started" && row["kind"] != "loop.instruction"),
        "S9 must not rerun wind-down or admit a turn"
    );
    assert_eq!(rows.last().unwrap()["kind"], "unload");
    assert!(
        !rows
            .iter()
            .any(|row| row["payload"]["case"] == "state_flow_failed")
    );
    if mode.contains("-resting-expired-save-") {
        let refused: Vec<_> = rows
            .iter()
            .filter(|row| row["kind"] == "save_point" && row["payload"]["phase"] == "refused")
            .collect();
        assert_eq!(
            refused.len(),
            1,
            "the expired queued save is accounted once"
        );
        assert_eq!(refused[0]["payload"]["refusal_code"], "capture_not_ready");
        assert_eq!(
            refused[0]["payload"]["window"],
            serde_json::to_value(expired_window.unwrap()).unwrap()
        );
        assert!(refused[0]["payload"]["tail_window"].is_null());
        let requested = rows
            .iter()
            .find(|row| row["kind"] == "save_point" && row["payload"]["phase"] == "requested")
            .unwrap();
        assert_eq!(
            requested["payload"]["capture_id"],
            refused[0]["payload"]["capture_id"]
        );
        assert_eq!(
            requested["payload"]["window"],
            refused[0]["payload"]["window"]
        );
    }
    assert_eq!(
        rows.last().unwrap()["payload"]["forced"],
        forced || spu_dead
    );
    if spu_dead {
        let faults: Vec<_> = rows
            .iter()
            .enumerate()
            .filter(|(_, row)| {
                row["kind"] == "fault"
                    && row["payload"]["case"] == "organ_death_observed"
                    && row["payload"]["account"]["organ"] == "spu"
            })
            .collect();
        assert_eq!(faults.len(), 1);
        assert!(
            held_reply
                || faults[0].0
                    < rows
                        .iter()
                        .position(|row| row["kind"] == "save_point"
                            && row["payload"]["phase"] == "requested")
                        .unwrap(),
            "the retained SPU death is Recorded before a new S9 capture"
        );
        assert_eq!(
            rows.last().unwrap()["payload"]["release"]["spu"],
            "unconfirmed"
        );
    }
    assert_eq!(
        member
            .requests
            .iter()
            .filter(|kind| kind.as_str() == "snapshot")
            .count(),
        3
    );
    assert_eq!(
        member
            .requests
            .iter()
            .filter(|kind| kind.as_str() == "abort")
            .count(),
        1
    );
}
