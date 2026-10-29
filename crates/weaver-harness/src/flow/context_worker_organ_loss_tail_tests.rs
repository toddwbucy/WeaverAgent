//! The later graceful directive must still save, reap, release and answer Left.
use super::*;
use crate::flow::context::failed_enter_driver::ChildState;
use crate::{FoundationGracefulLeave, FoundationGracefulLeaveProgress};

struct TailGuard(Box<FoundationGracefulLeave>);
impl Drop for TailGuard {
    fn drop(&mut self) {
        self.0.test_inner().test_reap();
    }
}

pub(in crate::flow::context::worker_startup) fn finish(
    run: Box<FoundationRun>,
    handoff: Handoff,
    member: &mut Member,
    reader: &mut UnixStream,
    root: &Path,
    mode: &str,
) {
    let Handoff {
        actual,
        peer,
        mut rows,
    } = handoff;
    let leave = actual.inner.leave;
    let parent = run.context.open_start.clone().unwrap();
    let owner = match run.graceful_leave(actual) {
        Ok(owner) => owner,
        Err((mut run, mut handoff)) => {
            crate::flow::context::worker_startup::tests::reap(
                handoff.inner.arms.child.as_mut().unwrap(),
            );
            run.resources.reap_startup_test_children();
            panic!("original run refused its retained graceful Leave");
        }
    };
    let mut owner = TailGuard(owner);
    let done = AtomicBool::new(false);
    let mut artifact: Option<CaptureArtifact> = None;
    let mut report = None;
    let mut complete = false;
    let bytes = std::thread::scope(|scope| {
        let responder = scope.spawn(|| {
            while !done.load(Ordering::SeqCst) {
                member.step_live(mode);
                std::thread::sleep(Duration::from_millis(1));
            }
        });
        let recorder = scope.spawn(|| {
            let mut bytes = Vec::new();
            while !done.load(Ordering::SeqCst) {
                let _ = reader.read_to_end(&mut bytes);
                std::thread::sleep(Duration::from_millis(1));
            }
            let _ = reader.read_to_end(&mut bytes);
            bytes
        });
        let _stop = StopThreads(&done);
        let end = Instant::now() + Duration::from_secs(20);
        loop {
            assert!(
                Instant::now() < end,
                "organ loss Leave stalled: {:?}",
                owner.0.detail()
            );
            let progress = owner.0.advance();
            assert!(
                owner.0.detail().is_none(),
                "healthy organ-loss Leave failed: {:?}",
                owner.0.detail()
            );
            if let Some(bytes) = read_packet(&peer) {
                let packet = CaptureEnvelope::decode(&bytes, CaptureSender::Harness).unwrap();
                assert_eq!(packet.exchange.ordinal, 99);
                let response = match &packet.payload {
                    CapturePayload::SavePointBeginning(beginning) => {
                        assert!(artifact.is_none());
                        assert_eq!(beginning.tail_window.unwrap().began_ns.0, leave.began_ns());
                        assert_eq!(
                            beginning.tail_window.unwrap().deadline_ns.0,
                            leave.deadline_ns()
                        );
                        let (_, handoff) = owner.0.test_inner().test_account();
                        assert!(matches!(
                            handoff.arms.child.as_ref().unwrap().state(),
                            ChildState::Reaped(_)
                        ));
                        assert_eq!(
                            root.join("ready-rollback-lower.json").exists(),
                            mode.ends_with("-spu") || mode.ends_with("-spu-release")
                        );
                        None
                    }
                    CapturePayload::SavePointImage(image) => {
                        let bytes =
                            std::fs::read(root.join("normal-member/room").join(&image.room_name))
                                .unwrap();
                        assert_eq!(&bytes[..16], b"SQLite format 3\0");
                        assert_eq!(bytes.len() as u64, image.size_bytes.0);
                        assert!(image.position.is_none());
                        std::fs::write(root.join("retained-capture.sqlite"), &bytes).unwrap();
                        let value = CaptureArtifact {
                            capture_id: image.capture_id,
                            artifact_id: format!("{:020}-{}", 3, parent.config_sha256.as_str()),
                            dump_sha256: normal_member::digest(&bytes),
                            config_sha256: parent.config_sha256.clone(),
                        };
                        let checked = CapturePayload::SavePointImageChecked(CaptureImageChecked {
                            capture_id: image.capture_id,
                            dump_sha256: value.dump_sha256.clone(),
                        });
                        artifact = Some(value);
                        Some(checked)
                    }
                    CapturePayload::SavePointImageFinished(finished) => {
                        assert_eq!(finished.capture_id, artifact.as_ref().unwrap().capture_id);
                        Some(CapturePayload::SavePointPrepared(
                            artifact.as_ref().unwrap().clone(),
                        ))
                    }
                    CapturePayload::SavePointCommit(commit) => {
                        assert_eq!(commit.capture_id, artifact.as_ref().unwrap().capture_id);
                        assert!(owner.0.unload_sequence().is_none());
                        Some(CapturePayload::SavePointPublished(
                            artifact.as_ref().unwrap().clone(),
                        ))
                    }
                    CapturePayload::SavePointCaptureComplete(_) => {
                        assert!(owner.0.unload_sequence().is_none());
                        assert_eq!(
                            std::fs::read_dir(root.join("normal-member/room"))
                                .unwrap()
                                .count(),
                            0
                        );
                        complete = true;
                        None
                    }
                    CapturePayload::Answer(CaptureAnswer::Left {
                        save_point: Some(value),
                        forced: true,
                        no_state: false,
                    }) => {
                        assert!(complete);
                        assert!(owner.0.unload_sequence().is_some());
                        assert_eq!(packet.position, Position::Close);
                        report = Some(value.clone());
                        None
                    }
                    other => panic!("organ-loss Leave lost its actual capture: {other:?}"),
                };
                if let Some(payload) = response {
                    let bytes = CaptureEnvelope {
                        exchange: packet.exchange,
                        position: Position::Continue,
                        payload,
                    }
                    .encode(CaptureSender::Admin)
                    .unwrap();
                    assert_eq!(
                        send(peer.as_raw_fd(), &bytes, MsgFlags::MSG_NOSIGNAL),
                        Ok(bytes.len())
                    );
                }
            }
            if progress == FoundationGracefulLeaveProgress::Closed {
                break;
            }
            assert_ne!(progress, FoundationGracefulLeaveProgress::CaptureRefused);
            if progress == FoundationGracefulLeaveProgress::Pending {
                owner.0.wait_for_progress();
            }
        }
        done.store(true, Ordering::SeqCst);
        responder.join().unwrap();
        recorder.join().unwrap()
    });
    let report = report.expect("actual forced Left follows both completed saves");
    assert!(!owner.0.abandoned());
    member.finish_normal();
    member.assert_normal_capture(&report);
    assert_eq!(report.start.parent_artifact_id, Some(parent.artifact_id));
    rows.extend(
        trace_through(
            reader,
            bytes,
            weaver_trace::Sequence(owner.0.unload_sequence().unwrap()),
            Instant::now() + Duration::from_secs(3),
        )
        .unwrap(),
    );
    assert_eq!(rows.iter().filter(|row| row["kind"] == "fault" && row["payload"]["case"] == "organ_death_observed").count(), 1, "Lower and release must not report a second death");
    assert_eq!(
        rows.iter()
            .filter(|row| row["kind"] == "save_point")
            .count(),
        if mode.contains("-organ-loss-expiry-") {
            5
        } else {
            6
        }
    );
    assert!(
        rows.iter()
            .all(|row| row["kind"] != "turn.started" && row["kind"] != "state.flow_failed")
    );
    let unload = rows.last().unwrap();
    assert_eq!(unload["kind"], "unload");
    assert_eq!(unload["payload"]["cause"]["uid"], 75);
    assert_eq!(unload["payload"]["forced"], true);
    assert!(unload["payload"]["forced_by"].is_null());
    assert_eq!(
        unload["payload"]["release"]["spu"],
        if mode.ends_with("-spu") || mode.ends_with("-spu-release") {
            "unconfirmed"
        } else {
            "confirmed"
        }
    );
    let (run, handoff) = owner.0.test_inner().test_account();
    for child in [
        run.resources.spu_child(),
        run.resources.classifier_child(),
        handoff.arms.child.as_ref().map(|child| child.state()),
    ] {
        assert!(
            matches!(child, Some(ChildState::Reaped(_))),
            "actual child remains owned until reap: {child:?}"
        );
    }
    assert!(run.context.failure().is_none());
    if mode.contains("-world-pressure-") {
        let refusals: Vec<_> = rows
            .iter()
            .filter(|row| {
                row["kind"] == "refusal" && row["payload"]["refusal"]["kind"] == "unloading"
            })
            .collect();
        assert_eq!(refusals.len(), 1, "Leave retains Unloading after SPU loss");
        assert_eq!(refusals[0]["payload"]["asked"]["ask"], "leave");
        assert_eq!(
            rows.iter()
                .filter(|row| row["kind"] == "input.offered")
                .count(),
            2,
            "the frame behind Leave must not reenter serialized check intake"
        );
        assert!(!root.join("organ-loss-world-2").exists());
    }
}
