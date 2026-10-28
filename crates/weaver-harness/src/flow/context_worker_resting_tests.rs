//! Actual S9 sockets: Observe, Stop, demand save, then a fresh sole Leave.
use super::*;
use crate::flow::clock::AbsoluteBound;
use weaver_types::state_flow::enter::{ObservationAnswer, ObservationContent, ObservationEnvelope};

fn response(peer: &OwnedFd) -> Vec<u8> {
    let end = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(bytes) = read_packet(peer) {
            return bytes;
        }
        assert!(Instant::now() < end, "S9 command has no actual response");
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn saved(peer: &OwnedFd, forced: Option<bool>, previous: AbsoluteBound) -> s::DecimalU64 {
    let mut id = None;
    let mut artifact = None;
    let mut complete = false;
    loop {
        let packet = CaptureEnvelope::decode(&response(peer), CaptureSender::Harness).unwrap();
        match packet.payload {
            CapturePayload::SavePointBeginning(beginning) => {
                assert!(id.replace(beginning.capture_id).is_none());
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
            }
            CapturePayload::SavePointImageFinished(finished) => {
                assert_eq!(Some(finished.capture_id), id);
                reply(
                    peer,
                    &packet.exchange,
                    CapturePayload::SavePointPrepared(artifact.as_ref().unwrap().clone()),
                );
            }
            CapturePayload::SavePointCommit(commit) => {
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
    let lower = std::fs::read(root.join("ready-rollback-lower.json")).unwrap();
    let forced = mode.contains("-resting-force-");
    let spu_dead = mode.contains("-resting-spu-");
    if spu_dead {
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
    let done = AtomicBool::new(false);
    let admin_done = AtomicBool::new(false);
    let bytes = std::thread::scope(|scope| {
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
            let peer = management(socket, LifecycleDirective::Observe);
            let answer = ObservationEnvelope::decode(&response(&peer)).unwrap();
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
            let answer: OrganEnvelope = serde_json::from_slice(&response(&peer)).unwrap();
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
            let demand = saved(&peer, None, previous);
            let peer = management(
                socket,
                LifecycleDirective::Leave {
                    cause: weaver_types::Cause { uid: 77 },
                    forced,
                    rollback: false,
                },
            );
            let retry = saved(&peer, Some(forced || spu_dead), previous);
            assert_ne!(demand, retry, "a retry owns a fresh capture identity");
            admin_done.store(true, Ordering::SeqCst);
        });
        let _stop = StopThreads(&done);
        let end = Instant::now() + Duration::from_secs(20);
        loop {
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
                owner.0.wait_for_progress();
            }
        }
        admin.join().unwrap();
        assert!(admin_done.load(Ordering::SeqCst));
        done.store(true, Ordering::SeqCst);
        responder.join().unwrap();
        recorder.join().unwrap()
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
            faults[0].0
                < rows
                    .iter()
                    .position(
                        |row| row["kind"] == "save_point" && row["payload"]["phase"] == "requested"
                    )
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
