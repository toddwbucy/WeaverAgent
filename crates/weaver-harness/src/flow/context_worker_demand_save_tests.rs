//! Real serving turns, recorder and normal SQLite capture. The admin protocol
//! is driven here or by the actual admin participant. Language qualification remains separate.
use super::*;
use crate::flow::context::worker_startup::tests::normal_member;
use weaver_types::state_flow::capture::*;

#[path = "context_worker_demand_failed_tests.rs"]
pub(in crate::flow::context::worker_startup) mod failed;
#[path = "context_worker_demand_management_tests.rs"]
mod management;

pub(in crate::flow::context::worker_startup) fn is_mode(mode: &str) -> bool {
    mode.contains("-session-demand-save")
}

pub(super) fn exercise(
    serving: FoundationServing<'_>,
    member: &mut Member,
    reader: &mut UnixStream,
    socket: &Path,
    root: &Path,
    mode: &str,
) {
    let original_context = std::ptr::from_ref(&*serving.owner.context);
    let loaded = serving.owner.context.open_start.clone().unwrap();
    let wait = serving
        .owner
        .context
        .load_duty
        .as_ref()
        .unwrap()
        .persistence
        .save_wait
        .elapsed_ns
        .0;
    let late = mode.ends_with("-expired");
    let expired = late || mode.ends_with("-expires");
    let held_filing = mode.ends_with("-held-filing");
    let idle = mode.ends_with("-idle");
    let joint = mode.contains("-sqlite-joint");
    let failed_save = failed::is_mode(mode);
    let bad_published = mode.ends_with("-bad-published");
    let secondary = mode.ends_with("-secondary");
    let retry = mode.ends_with("-timeout-retry");
    let mut failed_handoff = None;
    let mut guard = SessionGuard(Some(Session::begin(serving, &FAMILY, CutMethod::Flush)));
    let done = AtomicBool::new(false);
    let answered = AtomicBool::new(false);
    reader.set_nonblocking(true).unwrap();
    let (bytes, packets, windows) = std::thread::scope(|scope| {
        let responder = scope.spawn(|| {
            let end = Instant::now() + Duration::from_secs(30);
            let mut released = false;
            while !done.load(Ordering::SeqCst) && Instant::now() < end {
                member.step_live(mode);
                if held_filing && member.normal_filing_held() && !released {
                    std::fs::write(
                        root.join("demand-held-filing"),
                        b"original committed receipt held",
                    )
                    .unwrap();
                    if root.join("demand-resume-filing").exists() {
                        member.release_normal_filing();
                        released = true;
                    }
                }
                std::thread::sleep(Duration::from_millis(1));
            }
        });
        let recorder = scope.spawn(|| {
            let mut bytes = vec![];
            let end = Instant::now() + Duration::from_secs(30);
            while !done.load(Ordering::SeqCst) && Instant::now() < end {
                let _ = reader.read_to_end(&mut bytes);
                std::thread::sleep(Duration::from_millis(1));
            }
            let _ = reader.read_to_end(&mut bytes);
            bytes
        });
        let admin = scope.spawn(|| {
            if held_filing {
                wait_file(root, "demand-held-filing");
            } else if !idle {
                wait_file(root, "session-generation-0.json");
            }
            if joint {
                std::fs::write(root.join("ready.json.part"), b"{}").unwrap();
                std::fs::rename(root.join("ready.json.part"), root.join("ready.json")).unwrap();
                wait_file(root, "concluded.json");
                let request = CaptureEnvelope::decode(
                    &std::fs::read(root.join("admin-save-request.json")).unwrap(),
                    CaptureSender::Admin,
                )
                .unwrap();
                let CapturePayload::Directive(CaptureDirective::SavePoint { window, .. }) =
                    request.payload
                else {
                    panic!("actual demand request")
                };
                let mut packets = vec![];
                for index in 0.. {
                    let path = root.join(format!("admin-received-{index}.json"));
                    if !path.exists() {
                        break;
                    }
                    let value = CaptureEnvelope::decode(
                        &std::fs::read(path).unwrap(),
                        CaptureSender::Harness,
                    )
                    .unwrap();
                    assert_eq!(value.exchange, request.exchange);
                    if let CapturePayload::SavePointBeginning(beginning) = &value.payload {
                        assert_eq!(beginning.window, window);
                        assert!(beginning.tail_window.is_none());
                    }
                    packets.push(value);
                }
                assert!(matches!(
                    packets.last().unwrap().payload,
                    CapturePayload::Answer(CaptureAnswer::SavePointTaken { .. })
                ));
                answered.store(true, Ordering::SeqCst);
                return (packets, vec![window]);
            }
            let now = crate::flow::clock::monotonic_ns().unwrap();
            let began = if late {
                now.checked_sub(wait).unwrap()
            } else {
                now
            };
            let mut window = s::SaveWindow {
                began_ns: began.into(),
                deadline_ns: began.checked_add(wait).unwrap().into(),
            };
            let mut request = CaptureEnvelope {
                exchange: weaver_types::ExchangeId {
                    opener: Opener::Admin,
                    ordinal: 329,
                },
                position: Position::Open,
                payload: CapturePayload::Directive(CaptureDirective::SavePoint {
                    cause: weaver_types::Cause { uid: 74 },
                    window,
                }),
            };
            let mut peer = dial(socket);
            let bytes = request.encode(CaptureSender::Admin).unwrap();
            assert_eq!(
                send(peer.as_raw_fd(), &bytes, MsgFlags::MSG_NOSIGNAL),
                Ok(bytes.len())
            );
            std::fs::write(root.join("demand-request-sent"), &bytes).unwrap();
            if secondary {
                wait_file(root, "demand-save-accepted");
                management::check(socket, window);
                std::fs::write(
                    root.join("demand-resume-generation"),
                    b"queued management checked",
                )
                .unwrap();
            }
            let mut packets = vec![];
            let mut windows = vec![window];
            let mut artifact = None;
            let mut image_position = None;
            let end = Instant::now() + Duration::from_secs(25);
            loop {
                assert!(Instant::now() < end, "demand capture did not answer");
                if failed_save && root.join("demand-failed-handoff").exists() {
                    assert!(
                        read_packet(&peer).is_none(),
                        "failed cleanup grants no retryable outcome"
                    );
                    answered.store(true, Ordering::SeqCst);
                    break;
                }
                let Some(bytes) = read_packet(&peer) else {
                    std::thread::sleep(Duration::from_millis(1));
                    continue;
                };
                let message = CaptureEnvelope::decode(&bytes, CaptureSender::Harness).unwrap();
                assert_eq!(message.exchange, request.exchange);
                let reply = match &message.payload {
                    CapturePayload::SavePointBeginning(beginning) => {
                        assert!(!expired);
                        assert_eq!(beginning.window, window);
                        assert!(beginning.tail_window.is_none());
                        if !idle {
                            wait_file(root, "session-close-0.json");
                        }
                        assert!(
                            !root
                                .join(if idle {
                                    "session-generation-0.json"
                                } else {
                                    "session-generation-1.json"
                                })
                                .exists()
                        );
                        None
                    }
                    CapturePayload::SavePointImage(image) => {
                        if secondary {
                            management::check(socket, window);
                        }
                        let bytes =
                            std::fs::read(root.join("normal-member/room").join(&image.room_name))
                                .unwrap();
                        assert_eq!(bytes.len() as u64, image.size_bytes.0);
                        assert_eq!(&bytes[..16], b"SQLite format 3\0");
                        std::fs::write(root.join("demand-captured.sqlite"), &bytes).unwrap();
                        image_position = image.position.clone();
                        assert_eq!(image_position.is_none(), idle);
                        let value = CaptureArtifact {
                            capture_id: image.capture_id,
                            artifact_id: format!("{:020}-{}", 2, loaded.config_sha256.as_str()),
                            dump_sha256: normal_member::digest(&bytes),
                            config_sha256: loaded.config_sha256.clone(),
                        };
                        let reply = CapturePayload::SavePointImageChecked(CaptureImageChecked {
                            capture_id: image.capture_id,
                            dump_sha256: value.dump_sha256.clone(),
                        });
                        artifact = Some(value);
                        Some(reply)
                    }
                    CapturePayload::SavePointImageFinished(finished) => {
                        assert_eq!(
                            std::fs::read(
                                root.join("normal-member/room").join(&finished.room_name)
                            )
                            .unwrap(),
                            std::fs::read(root.join("demand-captured.sqlite")).unwrap()
                        );
                        Some(CapturePayload::SavePointPrepared(
                            artifact.as_ref().unwrap().clone(),
                        ))
                    }
                    CapturePayload::SavePointCommit(commit) => {
                        assert_eq!(commit.capture_id, artifact.as_ref().unwrap().capture_id);
                        let mut value = artifact.as_ref().unwrap().clone();
                        if bad_published {
                            value.dump_sha256 = s::Digest::new("0".repeat(64)).unwrap();
                        }
                        Some(CapturePayload::SavePointPublished(value))
                    }
                    CapturePayload::Answer(CaptureAnswer::SavePointTaken { report }) => {
                        assert!(!expired);
                        assert_eq!(message.position, Position::Close);
                        assert_eq!(report.start.seed_id, loaded.seed_id);
                        assert_eq!(
                            report.start.parent_artifact_id.as_ref(),
                            Some(&loaded.artifact_id)
                        );
                        assert_eq!(report.start.position, image_position);
                        assert_eq!(
                            report.start.dump_sha256,
                            artifact.as_ref().unwrap().dump_sha256
                        );
                        let room = root.join("normal-member/room");
                        assert!(
                            !room
                                .join(format!("capture-{}.sqlite", report.capture_id.0))
                                .exists()
                        );
                        if retry {
                            // The serving driver may block on its next input
                            // before returning. The independent admin releases
                            // the gate only after the successful retry reply.
                            std::fs::write(
                                root.join("session-input-1"),
                                b"next turn after successful retry",
                            )
                            .unwrap();
                        }
                        answered.store(true, Ordering::SeqCst);
                        packets.push(message);
                        break;
                    }
                    CapturePayload::Refusal(CaptureRefusal::Save(
                        SaveRefusal::SavePointNotTaken {
                            missed: SavePointLeg::CaptureNotReady,
                        },
                    )) => {
                        assert!(expired);
                        assert_eq!(message.position, Position::Close);
                        assert!(packets.is_empty(), "queued expiry starts no capture");
                        assert!(
                            !root.join("session-close-0.json").exists(),
                            "the same paused generation is still running"
                        );
                        std::fs::write(
                            root.join("demand-resume-generation"),
                            b"typed refusal received",
                        )
                        .unwrap();
                        answered.store(true, Ordering::SeqCst);
                        packets.push(message);
                        break;
                    }
                    CapturePayload::Refusal(CaptureRefusal::Save(
                        SaveRefusal::SavePointNotTaken { missed },
                    )) if retry => {
                        assert_eq!(windows.len(), 1, "only the first capture result is delayed");
                        assert_eq!(
                            *missed,
                            if mode.ends_with("-snapshot-timeout-retry") {
                                SavePointLeg::Answer
                            } else {
                                SavePointLeg::Finished
                            }
                        );
                        assert_eq!(message.position, Position::Close);
                        assert!(root.join("demand-late-result.jsonl").exists());
                        assert!(root.join("demand-aborted.jsonl").exists());
                        assert_eq!(
                            std::fs::read_dir(root.join("normal-member/room"))
                                .unwrap()
                                .count(),
                            0
                        );
                        assert!(!packets.iter().any(|packet: &CaptureEnvelope| matches!(
                            packet.payload,
                            CapturePayload::SavePointCommit(_)
                        )));
                        packets.push(message);
                        let now = crate::flow::clock::monotonic_ns().unwrap();
                        window = s::SaveWindow {
                            began_ns: now.into(),
                            deadline_ns: (now + wait).into(),
                        };
                        request.exchange.ordinal += 1;
                        request.payload = CapturePayload::Directive(CaptureDirective::SavePoint {
                            cause: weaver_types::Cause { uid: 74 },
                            window,
                        });
                        peer = dial(socket);
                        let bytes = request.encode(CaptureSender::Admin).unwrap();
                        assert_eq!(
                            send(peer.as_raw_fd(), &bytes, MsgFlags::MSG_NOSIGNAL),
                            Ok(bytes.len())
                        );
                        windows.push(window);
                        artifact = None;
                        image_position = None;
                        continue;
                    }
                    other => panic!("unexpected demand reply: {other:?}"),
                };
                if let Some(payload) = reply {
                    let failed_packet =
                        bad_published && matches!(payload, CapturePayload::SavePointPublished(_));
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
                    if failed_packet {
                        std::fs::write(root.join("demand-bad-published.json"), &bytes).unwrap();
                        wait_file(root, "demand-failed-handoff");
                        assert!(
                            read_packet(&peer).is_none(),
                            "bad Published cannot grant a save outcome"
                        );
                        packets.push(message);
                        answered.store(true, Ordering::SeqCst);
                        break;
                    }
                }
                packets.push(message);
            }
            (packets, windows)
        });
        let _stop = StopThreads(&done);
        if idle {
            wait_file(root, "demand-request-sent");
        }
        let session = guard.0.as_mut().unwrap();
        let mut accepted_saves = 0;
        let end = Instant::now() + Duration::from_secs(25);
        'turns: for index in 0..2 {
            if index > 0 && !failed_save && !retry {
                std::fs::write(root.join(format!("session-input-{index}")), b"next input").unwrap();
            }
            loop {
                assert!(
                    Instant::now() < end,
                    "demand session stalled: {:?} {:?}",
                    session.progress(),
                    session.detail()
                );
                match session.advance() {
                    Progress::DirectivePending => {
                        assert!(accepted_saves < if retry { 2 } else { 1 });
                        assert!(session.queue_save());
                        accepted_saves += 1;
                        std::fs::write(
                            root.join("demand-save-accepted"),
                            b"original save accepted",
                        )
                        .unwrap();
                        if held_filing {
                            assert!(!root.join("session-generation-0.json").exists());
                            std::fs::write(
                                root.join("demand-resume-filing"),
                                b"accepted without consuming original filing",
                            )
                            .unwrap();
                        }
                        if !idle && !expired && !secondary && !joint {
                            std::fs::write(
                                root.join("demand-resume-generation"),
                                b"same queued turn resumes",
                            )
                            .unwrap();
                        }
                    }
                    Progress::Accepted => session
                        .start_turn(TurnKey(format!("session-{index}")))
                        .unwrap(),
                    Progress::Completed => break,
                    Progress::Pending => (),
                    Progress::Stopped if failed_save => break 'turns,
                    progress => panic!(
                        "demand save changed the current turn: {progress:?} {:?}",
                        session.detail()
                    ),
                }
            }
            wait_file(root, &format!("session-close-{index}.json"));
            assert_eq!(session.next_input(), Ok(Progress::Completed));
        }
        assert_eq!(accepted_saves, if retry { 2 } else { 1 });
        if failed_save {
            let session = guard.0.take().unwrap();
            let input = session
                .into_owner()
                .ok()
                .expect("failed capture retains input owner");
            failed_handoff = Some(failed::handoff(input, root, mode));
        }
        let (packets, windows) = admin.join().unwrap();
        assert!(answered.load(Ordering::SeqCst));
        done.store(true, Ordering::SeqCst);
        responder.join().unwrap();
        (recorder.join().unwrap(), packets, windows)
    });
    if let Some((handoff, through)) = failed_handoff {
        let rows = trace_through(
            reader,
            bytes,
            through,
            Instant::now() + Duration::from_secs(3),
        )
        .unwrap();
        failed::retain(handoff, rows);
        return;
    }
    let session = guard.0.take().unwrap();
    let input = match session.into_owner() {
        Ok(input) => input,
        Err(_) => panic!("demand owns no outstanding management directive"),
    };
    assert_eq!(
        std::ptr::from_ref(&*input.serving.owner.context),
        original_context
    );
    assert!(input.serving.save.is_none());
    assert!(input.serving.owner.context.failure().is_none());
    assert_eq!(
        input.serving.owner.context.open_start.as_ref().unwrap(),
        &loaded
    );
    assert!(input.serving.owner.context.capture_snapshot().is_none());
    let retained = Guard {
        input: Some(input),
        management: None,
    };
    let rows = trace_through(
        reader,
        bytes,
        retained.recorded_sequence(),
        Instant::now() + Duration::from_secs(3),
    )
    .unwrap();
    let saves: Vec<_> = rows
        .iter()
        .filter(|row| row["kind"] == "save_point")
        .collect();
    if joint {
        member.write_normal_evidence();
        let trace = rows
            .iter()
            .map(|row| serde_json::to_string(row).unwrap() + "\n")
            .collect::<String>();
        std::fs::write(root.join("worker-trace.jsonl"), trace).unwrap();
    }
    assert_eq!(
        saves.len(),
        if retry {
            5
        } else if expired {
            2
        } else {
            3
        }
    );
    assert_eq!(saves[0]["payload"]["phase"], "requested");
    assert_eq!(
        saves.last().unwrap()["payload"]["phase"],
        if expired { "refused" } else { "published" }
    );
    if retry {
        assert_eq!(saves[1]["payload"]["phase"], "refused");
        assert_ne!(
            saves[0]["payload"]["capture_id"],
            saves[2]["payload"]["capture_id"]
        );
        assert_eq!(
            member
                .requests
                .iter()
                .filter(|request| *request == "snapshot")
                .count(),
            2
        );
        assert_eq!(
            member
                .requests
                .iter()
                .filter(|request| *request == "abort")
                .count(),
            1
        );
        assert_eq!(
            member
                .requests
                .iter()
                .filter(|request| *request == "release_capture")
                .count(),
            1
        );
    }
    for (index, row) in saves.iter().enumerate() {
        let window = windows[usize::from(retry && index >= 2)];
        assert_eq!(row["payload"]["cause"], "requested");
        assert_eq!(
            row["payload"]["window"]["began_ns"],
            window.began_ns.0.to_string()
        );
        assert_eq!(
            row["payload"]["window"]["deadline_ns"],
            window.deadline_ns.0.to_string()
        );
        assert!(row["payload"]["tail_window"].is_null());
    }
    assert_eq!(
        rows.iter()
            .filter(|row| row["kind"] == "turn.closed")
            .count(),
        2
    );
    assert!(!packets.iter().any(|packet| matches!(
        packet.payload,
        CapturePayload::SavePointCaptureComplete(_)
            | CapturePayload::Answer(CaptureAnswer::Left { .. })
    )));
}
