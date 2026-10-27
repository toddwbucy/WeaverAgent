//! Actual child EOF while normal SQLite capture holds its original image.
//! These fixture organs qualify composition, not language or model artifacts.
use super::*;
use crate::flow::context::worker_startup::input::FoundationInputProgress;
use crate::flow::context::worker_startup::input::management::FoundationManagementProgress;
use crate::flow::pump::PumpDeadline;
use crate::{FoundationGracefulLeaveHandoff, FoundationRun};
use std::os::fd::OwnedFd;
use weaver_trace::serving;

#[path = "context_worker_organ_loss_tail_tests.rs"]
mod tail;
pub(in crate::flow::context::worker_startup) use tail::finish;

pub(in crate::flow::context::worker_startup) struct Handoff {
    actual: Box<FoundationGracefulLeaveHandoff>,
    peer: OwnedFd,
    rows: Vec<serde_json::Value>,
}

pub(in crate::flow::context) fn is_mode(mode: &str) -> bool {
    mode.contains("-session-demand-save-sqlite-organ-loss-")
}

pub(in crate::flow::context) fn gate(mode: &str, root: &Path, fd: i32) {
    if mode.ends_with("-gate") {
        wait_file(root, "organ-loss-image-held");
        eprintln!("{{\"fixture\":\"gate last word before image verification\"}}");
        return;
    }
    if mode.ends_with("-spu-release") {
        return super::super::ready_rollback::gate(mode, root, fd);
    }
    wait_file(root, "organ-loss-requests-ready");
    for index in 0..2 {
        let request = super::super::frame(300 + index, false);
        packet!(fd, &request);
        let mut bytes = vec![0; weaver_types::MAX_ENVELOPE_BYTES];
        let count = recv(fd, &mut bytes, MsgFlags::empty()).unwrap();
        assert!(count > 0, "the standing run owes NoResidency");
        let reply: OrganEnvelope = serde_json::from_slice(&bytes[..count]).unwrap();
        assert_eq!(reply.exchange, request.exchange);
        assert_eq!(reply.position, Position::Close);
        let Payload::Frame(frame) = reply.payload else {
            panic!("NoResidency must close the actual world exchange");
        };
        let value: serde_json::Value = serde_json::from_slice(&frame.octets().unwrap()).unwrap();
        assert_eq!(
            value,
            serde_json::json!({"kind":"refused","reason":"no model is resident"})
        );
    }
    std::fs::write(root.join("organ-loss-requests-done"), b"two exact refusals").unwrap();
    super::super::ready_rollback::gate(mode, root, fd);
}

pub(in crate::flow::context) fn spu(mode: &str, fd: i32) {
    if mode.ends_with("-spu") || mode.ends_with("-spu-release") {
        let root = std::path::PathBuf::from(std::env::var("WEAVER_WORKER_INPUT_ROOT").unwrap());
        wait_file(&root, "organ-loss-image-held");
        eprintln!("{{\"fixture\":\"SPU last word before image verification\"}}");
        return;
    }
    let mut bytes = vec![0; weaver_types::MAX_ENVELOPE_BYTES];
    assert_eq!(recv(fd, &mut bytes, MsgFlags::empty()).unwrap_or(0), 0);
}

pub(in crate::flow::context::worker_startup) fn handoff(
    serving: FoundationServing<'_>,
    member: &mut Member,
    reader: &mut UnixStream,
    socket: &Path,
    root: &Path,
    mode: &str,
) -> Handoff {
    let expiry = mode.contains("-organ-loss-expiry-");
    let loaded = serving.owner.context.open_start.clone().unwrap();
    let duration = serving
        .owner
        .context
        .load_duty
        .as_ref()
        .unwrap()
        .persistence
        .save_wait
        .elapsed_ns
        .0;
    let mut guard = Guard {
        input: Some(serving.input()),
        management: None,
    };
    let done = AtomicBool::new(false);
    let fault_seen = AtomicBool::new(false);
    let hold_trace = AtomicBool::new(false);
    let trace_paused = AtomicBool::new(false);
    reader.set_nonblocking(true).unwrap();
    let mut frozen = None;
    let mut refused_worlds = 0;
    let mut pressure = false;
    let mut expiry_seen = false;
    let (bytes, packets, peer) = std::thread::scope(|scope| {
        let responder = scope.spawn(|| {
            while !done.load(Ordering::SeqCst) {
                member.step_live(mode);
                std::thread::sleep(Duration::from_millis(1));
            }
        });
        let recorder = scope.spawn(|| {
            let mut bytes = Vec::new();
            while !done.load(Ordering::SeqCst) {
                if hold_trace.load(Ordering::SeqCst) {
                    trace_paused.store(true, Ordering::SeqCst);
                    std::thread::sleep(Duration::from_millis(1));
                    continue;
                }
                let _ = reader.read_to_end(&mut bytes);
                for line in bytes.split(|byte| *byte == b'\n') {
                    if let Ok(row) = serde_json::from_slice::<serde_json::Value>(line)
                        && row["kind"] == "fault"
                        && row["payload"]["case"] == "organ_death_observed"
                    {
                        fault_seen.store(true, Ordering::SeqCst);
                    }
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            let _ = reader.read_to_end(&mut bytes);
            bytes
        });
        let admin = scope.spawn(|| {
            let peer = dial(socket);
            let began = crate::flow::clock::monotonic_ns().unwrap();
            let window = s::SaveWindow {
                began_ns: began.into(),
                deadline_ns: (began + duration).into(),
            };
            let request = CaptureEnvelope {
                exchange: weaver_types::ExchangeId {
                    opener: Opener::Admin,
                    ordinal: 701,
                },
                position: Position::Open,
                payload: CapturePayload::Directive(CaptureDirective::SavePoint {
                    cause: weaver_types::Cause { uid: 74 },
                    window,
                }),
            };
            let bytes = request.encode(CaptureSender::Admin).unwrap();
            assert_eq!(
                send(peer.as_raw_fd(), &bytes, MsgFlags::MSG_NOSIGNAL),
                Ok(bytes.len())
            );
            let mut packets = Vec::new();
            let mut artifact: Option<CaptureArtifact> = None;
            let end = Instant::now() + Duration::from_secs(20);
            loop {
                if done.load(Ordering::SeqCst) {
                    return (packets, None);
                }
                assert!(
                    Instant::now() < end,
                    "healthy capture stalled after organ loss"
                );
                let Some(bytes) = read_packet(&peer) else {
                    std::thread::sleep(Duration::from_millis(1));
                    continue;
                };
                let message = CaptureEnvelope::decode(&bytes, CaptureSender::Harness).unwrap();
                assert_eq!(message.exchange, request.exchange);
                let response = match &message.payload {
                    CapturePayload::SavePointBeginning(beginning) => {
                        assert_eq!(beginning.window, window);
                        assert!(beginning.tail_window.is_none());
                        None
                    }
                    CapturePayload::SavePointImage(image) => {
                        let image_bytes =
                            std::fs::read(root.join("normal-member/room").join(&image.room_name))
                                .unwrap();
                        assert_eq!(&image_bytes[..16], b"SQLite format 3\0");
                        assert_eq!(image_bytes.len() as u64, image.size_bytes.0);
                        assert!(
                            image.position.is_none(),
                            "no content entered this idle capture"
                        );
                        if expiry {
                            hold_trace.store(true, Ordering::SeqCst);
                            while !trace_paused.load(Ordering::SeqCst) {
                                assert!(Instant::now() < end, "trace reader did not pause");
                                std::thread::sleep(Duration::from_millis(1));
                            }
                        }
                        std::fs::write(root.join("organ-loss-image-held"), &image_bytes).unwrap();
                        let observed_by = Instant::now() + Duration::from_secs(3);
                        while !expiry && !fault_seen.load(Ordering::SeqCst) {
                            if done.load(Ordering::SeqCst) {
                                return (packets, None);
                            }
                            assert!(
                                Instant::now() < observed_by,
                                "organ EOF did not produce its fault during paused capture"
                            );
                            std::thread::sleep(Duration::from_millis(1));
                        }
                        assert_eq!(
                            std::fs::read(root.join("normal-member/room").join(&image.room_name))
                                .unwrap(),
                            image_bytes
                        );
                        let value = CaptureArtifact {
                            capture_id: image.capture_id,
                            artifact_id: format!("{:020}-{}", 2, loaded.config_sha256.as_str()),
                            dump_sha256: normal_member::digest(&image_bytes),
                            config_sha256: loaded.config_sha256.clone(),
                        };
                        let checked = CapturePayload::SavePointImageChecked(CaptureImageChecked {
                            capture_id: image.capture_id,
                            dump_sha256: value.dump_sha256.clone(),
                        });
                        artifact = Some(value);
                        (!expiry).then_some(checked)
                    }
                    CapturePayload::SavePointImageFinished(finished) => {
                        assert_eq!(finished.capture_id, artifact.as_ref().unwrap().capture_id);
                        Some(CapturePayload::SavePointPrepared(
                            artifact.as_ref().unwrap().clone(),
                        ))
                    }
                    CapturePayload::SavePointCommit(commit) => {
                        assert_eq!(commit.capture_id, artifact.as_ref().unwrap().capture_id);
                        Some(CapturePayload::SavePointPublished(
                            artifact.as_ref().unwrap().clone(),
                        ))
                    }
                    CapturePayload::Answer(CaptureAnswer::SavePointTaken { report }) => {
                        assert!(!expiry, "expired capture cannot publish");
                        assert_eq!(message.position, Position::Close);
                        assert_eq!(report.capture_id, artifact.as_ref().unwrap().capture_id);
                        assert_eq!(
                            report.start.dump_sha256,
                            artifact.as_ref().unwrap().dump_sha256
                        );
                        assert!(report.start.position.is_none());
                        assert!(
                            !root
                                .join("normal-member/room")
                                .join(format!("capture-{}.sqlite", report.capture_id.0))
                                .exists()
                        );
                        packets.push(message);
                        if mode.ends_with("-spu") {
                            std::fs::write(root.join("organ-loss-requests-ready"), b"request")
                                .unwrap();
                            while !root.join("organ-loss-requests-done").exists() {
                                if done.load(Ordering::SeqCst) {
                                    return (packets, None);
                                }
                                assert!(Instant::now() < end, "later world requests stalled");
                                std::thread::sleep(Duration::from_millis(1));
                            }
                        }
                        let leave = super::super::management(
                            socket,
                            LifecycleDirective::Leave {
                                cause: weaver_types::Cause { uid: 75 },
                                forced: false,
                                rollback: false,
                            },
                        );
                        return (packets, Some(leave));
                    }
                    CapturePayload::Refusal(CaptureRefusal::Save(
                        SaveRefusal::SavePointNotTaken {
                            missed: SavePointLeg::Finished,
                        },
                    )) if expiry => {
                        assert_eq!(message.position, Position::Close);
                        assert!(
                            crate::flow::clock::monotonic_ns().unwrap() >= window.deadline_ns.0
                        );
                        assert_eq!(
                            std::fs::read_dir(root.join("normal-member/room"))
                                .unwrap()
                                .count(),
                            0,
                            "refusal follows actual member abort"
                        );
                        assert!(!packets.iter().any(|p: &CaptureEnvelope| matches!(
                            p.payload,
                            CapturePayload::SavePointCommit(_)
                        )));
                        packets.push(message);
                        let leave = super::super::management(
                            socket,
                            LifecycleDirective::Leave {
                                cause: weaver_types::Cause { uid: 75 },
                                forced: false,
                                rollback: false,
                            },
                        );
                        return (packets, Some(leave));
                    }
                    other => panic!("healthy save lost to unrelated organ death: {other:?}"),
                };
                if let Some(payload) = response {
                    let response = CaptureEnvelope {
                        exchange: request.exchange.clone(),
                        position: Position::Continue,
                        payload,
                    };
                    let bytes = response.encode(CaptureSender::Admin).unwrap();
                    assert_eq!(
                        send(peer.as_raw_fd(), &bytes, MsgFlags::MSG_NOSIGNAL),
                        Ok(bytes.len())
                    );
                }
                packets.push(message);
            }
        });
        let _stop = StopThreads(&done);
        let mut queued = false;
        let end = Instant::now() + Duration::from_secs(20);
        loop {
            assert!(Instant::now() < end, "organ loss driver stalled");
            if let Some(input) = &mut guard.input {
                if expiry
                    && !pressure
                    && input.serving.owner.context.serving_organs.any()
                    && root.join("organ-loss-image-held").exists()
                {
                    assert!(trace_paused.load(Ordering::SeqCst));
                    let context = &mut input.serving.owner.context;
                    let mut clock = crate::flow::clock::testing::FakePumpIo::new(context.last_ns);
                    crate::flow::context::worker_startup::input::management::stop::tests::fill_trace(
                        context, &mut clock,
                    );
                    assert_eq!(
                        context.barrier.recorder().boundary().queued,
                        serving::ORDINARY_CAPACITY
                    );
                    pressure = true;
                }
                let progress = input.advance();
                if expiry
                    && pressure
                    && !expiry_seen
                    && input
                        .serving
                        .owner
                        .context
                        .deadlines
                        .get(PumpDeadline::CaptureWindow)
                        .is_some_and(|bound| {
                            bound.expired(crate::flow::clock::monotonic_ns().unwrap())
                        })
                {
                    assert!(input.serving.owner.context.failure().is_none());
                    assert!(input.serving.owner.context.has_owed_organ_observation());
                    assert_eq!(
                        input
                            .serving
                            .owner
                            .context
                            .pending_author()
                            .map(|author| author.event.kind)
                            .or_else(|| input
                                .serving
                                .owner
                                .context
                                .waiting_content
                                .as_ref()
                                .map(|held| held.event.kind)),
                        Some(serving::Kind::Fault),
                        "the original fault must be retained at trace admission"
                    );
                    if let Some(author) = input.serving.owner.context.pending_author() {
                        assert_eq!(
                            author.event.kind,
                            serving::Kind::Fault,
                            "an admitted organ fault keeps its own blocked author"
                        );
                    }
                    expiry_seen = true;
                    hold_trace.store(false, Ordering::SeqCst);
                }
                if let Some(snapshot) = input.serving.owner.context.capture_snapshot() {
                    let original = frozen.get_or_insert((snapshot.capture_id, snapshot.frontier));
                    assert_eq!(
                        *original,
                        (snapshot.capture_id, snapshot.frontier),
                        "later fault changed frozen capture frontier"
                    );
                }
                match progress {
                    FoundationInputProgress::ManagementPending => {
                        guard.management = Some(
                            guard
                                .input
                                .take()
                                .unwrap()
                                .management()
                                .ok()
                                .expect("actual admin dial"),
                        );
                    }
                    FoundationInputProgress::Pending => (),
                    FoundationInputProgress::Refused => {
                        assert!(mode.ends_with("-spu"));
                        assert!(input.serving.owner.context.input.snapshot().is_none());
                        assert!(input.serving.owner.context.open_turn.is_none());
                        let result = guard.input.take().unwrap().finish();
                        let Ok(crate::FoundationInputResult::Refused(serving)) = result else {
                            panic!("NoResidency must return the original serving seat");
                        };
                        guard.input = Some(serving.input());
                        refused_worlds += 1;
                    }
                    other => panic!(
                        "organ EOF stopped healthy capture: {other:?} {:?}",
                        input.detail()
                    ),
                }
            }
            if let Some(management) = &mut guard.management {
                match management.advance() {
                    FoundationManagementProgress::Pending => (),
                    FoundationManagementProgress::DirectivePending => {
                        let management = guard.management.take().unwrap();
                        if !queued {
                            guard.input =
                                Some(management.queue_save().ok().expect("original demand save"));
                            queued = true;
                        } else {
                            guard.input = Some(
                                management
                                    .defer_graceful_leave()
                                    .ok()
                                    .expect("later graceful Leave"),
                            );
                            break;
                        }
                    }
                    other => panic!("organ EOF lost management: {other:?}"),
                }
            }
        }
        let (packets, leave) = admin.join().unwrap();
        assert!(leave.is_some());
        let input = guard.input.as_ref().unwrap();
        assert!(input.serving.owner.context.failure().is_none());
        assert!(input.serving.save.is_none());
        assert!(input.serving.owner.context.capture_snapshot().is_none());
        let graceful = input.serving.graceful.as_ref().unwrap();
        assert!(
            graceful.forced,
            "a later graceful Leave must recognize the dead organ"
        );
        assert!(graceful.forced_by.is_none());
        assert_eq!(graceful.cause.uid, 75);
        assert!(graceful.quiesce.is_none());
        assert!(graceful.wind_down.is_none());
        done.store(true, Ordering::SeqCst);
        responder.join().unwrap();
        (recorder.join().unwrap(), packets, leave.unwrap())
    });
    let rows = trace_through(
        reader,
        bytes,
        guard.recorded_sequence(),
        Instant::now() + Duration::from_secs(3),
    )
    .unwrap();
    assert_eq!(expiry_seen, expiry);
    let expected_refusals = if mode.ends_with("-spu") { 2 } else { 0 };
    assert_eq!(refused_worlds, expected_refusals);
    assert_eq!(
        rows.iter()
            .filter(|row| row["kind"] == "refusal"
                && matches!(
                    serde_json::from_value::<weaver_types::RefusalRecord>(row["payload"].clone()),
                    Ok(weaver_types::RefusalRecord::Lifecycle {
                        refusal: weaver_types::LifecycleRefusal::NoResidency,
                        ..
                    })
                )
                && row["turn"].is_null())
            .count(),
        expected_refusals
    );
    assert!(
        rows.iter()
            .all(|row| row["kind"] != "message.user" && row["kind"] != "turn.started")
    );
    let faults: Vec<_> = rows
        .iter()
        .filter(|row| row["kind"] == "fault" && row["payload"]["case"] == "organ_death_observed")
        .collect();
    assert_eq!(
        faults.len(),
        1,
        "one observed organ death, not one per dead socket"
    );
    assert!(
        faults[0]["sequence"]
            .as_str()
            .unwrap()
            .parse::<u64>()
            .unwrap()
            > frozen.unwrap().1
    );
    assert_eq!(
        faults[0]["payload"]["account"]["organ"],
        if mode.ends_with("-gate") {
            "gate"
        } else {
            "spu"
        }
    );
    assert_eq!(
        faults[0]["payload"]["account"]["last_word"]["fixture"],
        if mode.ends_with("-gate") {
            "gate last word before image verification"
        } else {
            "SPU last word before image verification"
        }
    );
    if mode.ends_with("-gate") {
        assert_eq!(
            faults[0]["payload"]["account"]["containment"],
            "tools/ killed and emptied"
        );
    }
    assert_eq!(
        member
            .requests
            .iter()
            .filter(|request| *request == "snapshot")
            .count(),
        1
    );
    assert_eq!(
        member
            .requests
            .iter()
            .filter(|request| *request == "release_capture")
            .count(),
        usize::from(!expiry)
    );
    assert_eq!(
        member
            .requests
            .iter()
            .filter(|request| *request == "abort")
            .count(),
        usize::from(expiry)
    );
    assert_eq!(
        rows.iter()
            .filter(|row| row["kind"] == "save_point")
            .count(),
        if expiry { 2 } else { 3 }
    );
    assert!(
        rows.iter()
            .all(|row| row["kind"] != "turn.started" && row["kind"] != "state.flow_failed")
    );
    if expiry {
        assert!(matches!(
            packets.last().unwrap().payload,
            CapturePayload::Refusal(_)
        ));
    } else {
        assert!(matches!(
            packets.last().unwrap().payload,
            CapturePayload::Answer(CaptureAnswer::SavePointTaken { .. })
        ));
    }
    let actual = match guard.input.take().unwrap().graceful_leave_handoff() {
        Ok(actual) => actual,
        Err(input) => {
            guard.input = Some(input);
            panic!("the original forced graceful Leave must retain its actual handoff");
        }
    };
    Handoff { actual, peer, rows }
}
