//! Refusal acknowledgement is distinct from cleanup and the final Leave.
use super::*;

fn abort(parent: &mut Parent<'_>, id: s::DecimalU64) {
    parent.submit(CapturePayload::SavePointAborted(CaptureAborted {
        capture_id: id,
        code: "publication_failed".into(),
        detail: "retained root preparation failure".into(),
    }));
}

#[test]
fn leave_capture_parent_refused_ack_waits_for_cleanup_record_and_original_tail() {
    for expire_leave in [false, true] {
        let mut run = active();
        let mut parent = Parent::new(&mut run);
        let (beginning, _) = parent.image();
        let leave = parent
            .member
            .run
            .context
            .deadlines
            .get(PumpDeadline::Leave)
            .unwrap();
        parent.pressure();
        abort(&mut parent, beginning.capture_id);
        let request = parent.request();
        assert!(matches!(
            request,
            s::StateFrame::Control {
                frame: s::StateControl::Abort { .. },
                ..
            }
        ));
        for _ in 0..16 {
            assert_ne!(parent.step().unwrap(), ServingCaptureProgress::Refused);
        }
        assert_eq!(
            parent.member.records(Kind::SavePoint).len(),
            1,
            "no refused disposition before cleanup"
        );
        let answer = parent.member.answer(
            &request,
            s::StateControlAnswer::Aborted {
                capture_id: beginning.capture_id,
            },
        );
        parent.member.receive(&answer).unwrap();
        for _ in 0..64 {
            assert_ne!(
                parent.step().unwrap(),
                ServingCaptureProgress::Refused,
                "acknowledgement must actually leave its original pressured port"
            );
        }
        assert_eq!(parent.member.records(Kind::SavePoint).len(), 2);
        assert!(parent.member.run.context.capture_snapshot().is_none());
        assert!(
            parent
                .member
                .run
                .context
                .deadlines
                .get(PumpDeadline::CaptureWindow)
                .is_none()
        );
        assert_eq!(
            parent.member.run.context.deadlines.get(PumpDeadline::Leave),
            Some(leave)
        );
        assert!(beginning.window.deadline_ns.0 < leave.deadline_ns());
        parent
            .member
            .run
            .clock
            .advance_to(if expire_leave {
                leave.deadline_ns()
            } else {
                beginning.window.deadline_ns.0
            })
            .unwrap();
        parent.drain_pressure();
        if expire_leave {
            assert!(matches!(parent.step(), Err(RunFailure::Wait {
                key: PumpDeadline::Leave, bound,
            }) if bound == leave));
            let mut bytes = [0; 256];
            assert_eq!(
                recv(parent.admin.as_raw_fd(), &mut bytes, MsgFlags::MSG_DONTWAIT),
                Err(nix::errno::Errno::EAGAIN)
            );
        } else {
            let acknowledgement = parent.next();
            assert_eq!(acknowledgement.exchange, parent.exchange);
            assert_eq!(acknowledgement.position, Position::Continue);
            assert_eq!(
                acknowledgement.payload,
                CapturePayload::SavePointCaptureRefused(CaptureRefused {
                    capture_id: beginning.capture_id,
                    missed: SavePointLeg::Published,
                })
            );
            assert_eq!(parent.step().unwrap(), ServingCaptureProgress::Refused);
            assert!(parent.capture.report().is_none());
        }
    }
}

#[test]
fn leave_capture_parent_crossed_abort_keeps_member_attribution_and_rejects_duplicate() {
    for crossing in ["aborting", "refusing", "carry", "ready"] {
        let mut run = active();
        let mut parent = Parent::new(&mut run);
        let CapturePayload::SavePointBeginning(beginning) = parent.next().payload else {
            panic!()
        };
        let request = parent.request();
        let refusal = parent.member.answer(
            &request,
            s::StateControlAnswer::ControlRefused {
                code: s::ControlRefusalCode::Capture(s::CaptureRefusalCode::SnapshotFailed),
                detail: "original snapshot refusal".into(),
            },
        );
        parent.member.receive(&refusal).unwrap();
        for _ in 0..64 {
            parent.step().unwrap();
            if parent.member.run.context.capture_snapshot().unwrap().phase == CapturePhase::Aborting
            {
                break;
            }
        }
        let request = parent.member.sent();
        assert!(matches!(
            request,
            s::StateFrame::Control {
                frame: s::StateControl::Abort { .. },
                ..
            }
        ));
        if crossing == "aborting" {
            abort(&mut parent, beginning.capture_id);
            parent.step().unwrap();
        }
        if crossing == "carry" {
            parent.pressure();
        }
        let answer = parent.member.answer(
            &request,
            s::StateControlAnswer::Aborted {
                capture_id: beginning.capture_id,
            },
        );
        parent.member.receive(&answer).unwrap();
        if crossing == "refusing" {
            for _ in 0..64 {
                parent.step().unwrap();
                if parent
                    .member
                    .run
                    .context
                    .capture_snapshot()
                    .is_some_and(|snapshot| snapshot.phase == CapturePhase::Complete)
                {
                    break;
                }
            }
            assert_eq!(
                parent.member.run.context.capture_snapshot().unwrap().phase,
                CapturePhase::Complete
            );
            assert_eq!(parent.member.records(Kind::SavePoint).len(), 1);
            abort(&mut parent, beginning.capture_id);
        } else if crossing == "carry" {
            for _ in 0..64 {
                assert_ne!(parent.step().unwrap(), ServingCaptureProgress::Refused);
            }
            assert!(parent.member.run.context.capture_snapshot().is_none());
            assert_eq!(parent.member.records(Kind::SavePoint).len(), 2);
            abort(&mut parent, beginning.capture_id);
            assert_ne!(parent.step().unwrap(), ServingCaptureProgress::Refused);
            parent.drain_pressure();
        }
        let acknowledgement = parent.next();
        assert_eq!(
            acknowledgement.payload,
            CapturePayload::SavePointCaptureRefused(CaptureRefused {
                capture_id: beginning.capture_id,
                missed: SavePointLeg::Answer,
            })
        );
        if crossing == "ready" {
            abort(&mut parent, beginning.capture_id);
        }
        assert_eq!(parent.step().unwrap(), ServingCaptureProgress::Refused);
        let records = parent.member.records(Kind::SavePoint);
        assert_eq!(records.len(), 2);
        assert_eq!(records[1]["payload"]["refusal_code"], "snapshot_failed");
        assert!(parent.member.run.context.halt().is_none());
        abort(&mut parent, beginning.capture_id);
        assert!(
            parent.step().is_err(),
            "a second admin Aborted is not a second cleanup"
        );
        assert!(parent.member.run.context.halt().is_some());
    }
}

#[test]
fn leave_capture_parent_cancelled_beginning_finishes_carry_without_requesting_snapshot() {
    let mut run = active();
    let mut parent = Parent::new(&mut run);
    parent.pressure();
    for _ in 0..8 {
        parent.step().unwrap();
    }
    // The fixture pins the first allocated identity while Beginning is blocked.
    let id = parent
        .capture
        .at_halt(&parent.member.run.context)
        .capture_id;
    abort(&mut parent, id);
    for _ in 0..8 {
        assert_ne!(parent.step().unwrap(), ServingCaptureProgress::Refused);
        assert!(parent.member.run.context.capture_snapshot().is_none());
    }
    parent.drain_pressure();
    assert!(matches!(
        parent.next().payload,
        CapturePayload::SavePointBeginning(_)
    ));
    assert_eq!(
        parent.next().payload,
        CapturePayload::SavePointCaptureRefused(CaptureRefused {
            capture_id: id,
            missed: SavePointLeg::Published,
        })
    );
    assert!(parent.member.run.context.capture_snapshot().is_none());
    assert!(
        parent
            .member
            .run
            .context
            .flow
            .state()
            .outstanding()
            .is_empty()
    );
    assert_eq!(parent.member.records(Kind::SavePoint).len(), 2);
}

#[test]
fn leave_capture_parent_admin_abort_drains_each_pending_member_result_before_acknowledging() {
    for finished in [false, true] {
        for late_refusal in [false, true] {
            let mut run = active();
            let mut parent = Parent::new(&mut run);
            let beginning = if finished {
                let (beginning, image) = parent.image();
                parent.submit(CapturePayload::SavePointImageChecked(CaptureImageChecked {
                    capture_id: image.capture_id,
                    dump_sha256: s::Digest::new("9".repeat(64)).unwrap(),
                }));
                beginning
            } else {
                let CapturePayload::SavePointBeginning(beginning) = parent.next().payload else {
                    panic!("Beginning first")
                };
                beginning
            };
            let original = parent.request();
            assert!(matches!(&original, s::StateFrame::Control { frame, .. }
                if matches!(frame, s::StateControl::Acknowledge { .. }) == finished));
            abort(&mut parent, beginning.capture_id);
            let cleanup = parent.request();
            assert_eq!(cleanup.common(), original.common());
            assert!(matches!(&cleanup, s::StateFrame::Control {
                frame: s::StateControl::Abort { capture_id }, ..
            } if *capture_id == beginning.capture_id));
            let bound = parent
                .member
                .run
                .context
                .deadlines
                .get(PumpDeadline::StateControl)
                .unwrap();
            let late = if late_refusal {
                s::StateControlAnswer::ControlRefused {
                    code: s::ControlRefusalCode::Capture(s::CaptureRefusalCode::UnknownCapture),
                    detail: "late member refusal cannot replace the retained admin cancellation"
                        .into(),
                }
            } else if finished {
                s::StateControlAnswer::Finished {
                    capture_id: beginning.capture_id,
                    room_name: format!("capture-{}.sqlite", beginning.capture_id.0),
                }
            } else {
                s::StateControlAnswer::Snapshot {
                    capture_id: beginning.capture_id,
                    room_name: format!("capture-{}.sqlite.part", beginning.capture_id.0),
                    size_bytes: 10.into(),
                    position: None,
                }
            };
            let late = parent.member.answer(&original, late);
            parent.member.receive(&late).unwrap();
            for _ in 0..16 {
                assert_ne!(parent.step().unwrap(), ServingCaptureProgress::Refused);
            }
            assert_eq!(
                parent
                    .member
                    .run
                    .context
                    .deadlines
                    .get(PumpDeadline::StateControl),
                Some(bound)
            );
            assert_eq!(parent.member.records(Kind::SavePoint).len(), 1);
            let mut bytes = [0; 4096];
            assert_eq!(
                recv(parent.admin.as_raw_fd(), &mut bytes, MsgFlags::MSG_DONTWAIT),
                Err(nix::errno::Errno::EAGAIN),
                "late result grants neither Image nor Finished nor refusal acknowledgement"
            );
            let answer = parent.member.answer(
                &cleanup,
                s::StateControlAnswer::Aborted {
                    capture_id: beginning.capture_id,
                },
            );
            parent.member.receive(&answer).unwrap();
            assert_eq!(
                parent.next().payload,
                CapturePayload::SavePointCaptureRefused(CaptureRefused {
                    capture_id: beginning.capture_id,
                    missed: SavePointLeg::Published,
                })
            );
            assert_eq!(parent.step().unwrap(), ServingCaptureProgress::Refused);
            assert!(parent.member.run.context.capture_snapshot().is_none());
            assert!(parent.member.run.context.halt().is_none());
            let records = parent.member.records(Kind::SavePoint);
            assert_eq!(records.len(), 2);
            assert_eq!(records[1]["payload"]["refusal_code"], "publication_failed");
            let measurements = parent.member.records(Kind::StateStoreMeasurement);
            let tail = &measurements[measurements.len() - 2..];
            assert_eq!(
                tail[0]["payload"]["observation"]["operation"],
                if finished { "acknowledge" } else { "snapshot" }
            );
            assert_eq!(tail[1]["payload"]["observation"]["operation"], "abort");
            let sequence =
                |row: &serde_json::Value| row["sequence"].as_str().unwrap().parse::<u64>().unwrap();
            assert!(sequence(&tail[0]) < sequence(&tail[1]));
            assert!(sequence(&tail[1]) < sequence(&records[1]));
        }
    }
}

#[test]
fn leave_capture_parent_cancelled_barrier_keeps_prior_control_and_never_sends_snapshot() {
    let mut run = active();
    let context = &mut run.context;
    context
        .flow
        .begin_control(
            &mut FlowRecording {
                barrier: &mut context.barrier,
                inbox: &mut context.inbox,
                budget: &mut context.budget,
                author: &context.author,
            },
            &mut run.clock,
            ControlOperation::try_from((s::StateControl::Position {}, None)).unwrap(),
        )
        .unwrap();
    let mut parent = Parent::new(&mut run);
    let position = parent.member.sent();
    let CapturePayload::SavePointBeginning(beginning) = parent.next().payload else {
        panic!()
    };
    parent.step().unwrap();
    assert_eq!(
        parent.member.run.context.capture_snapshot().unwrap().phase,
        CapturePhase::Barrier
    );
    let cut = parent
        .member
        .run
        .context
        .deadlines
        .get(PumpDeadline::Cut)
        .unwrap();
    abort(&mut parent, beginning.capture_id);
    for _ in 0..16 {
        assert_eq!(parent.step().unwrap(), ServingCaptureProgress::Pending);
    }
    assert_eq!(
        parent.member.run.context.deadlines.get(PumpDeadline::Cut),
        Some(cut)
    );
    assert_eq!(parent.member.records(Kind::SavePoint).len(), 1);
    let answer = parent
        .member
        .answer(&position, s::StateControlAnswer::Position(None));
    parent.member.receive(&answer).unwrap();
    let mut consumed = false;
    for _ in 0..64 {
        parent.step().unwrap();
        if parent
            .member
            .run
            .context
            .take_operation_completion(&mut parent.member.run.clock)
            .unwrap()
            .is_some()
        {
            consumed = true;
            break;
        }
    }
    assert!(
        consumed,
        "the original control's consumer must retain its completion"
    );
    assert_eq!(
        parent.next().payload,
        CapturePayload::SavePointCaptureRefused(CaptureRefused {
            capture_id: beginning.capture_id,
            missed: SavePointLeg::Published,
        })
    );
    assert!(parent.member.run.context.capture_snapshot().is_none());
    assert!(
        parent
            .member
            .run
            .context
            .flow
            .state()
            .outstanding()
            .is_empty()
    );
    assert!(
        parent
            .member
            .run
            .context
            .deadlines
            .get(PumpDeadline::Cut)
            .is_none()
    );
    let measurements = parent.member.records(Kind::StateStoreMeasurement);
    assert!(measurements.iter().all(|row| !matches!(
        row["payload"]["observation"]["operation"].as_str(),
        Some("snapshot" | "abort" | "release_capture")
    )));
    parent.member.run.state.set_nonblocking(true).unwrap();
    let mut bytes = [0; 4096];
    assert_eq!(
        parent.member.run.state.read(&mut bytes).unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    assert_eq!(parent.member.records(Kind::SavePoint).len(), 2);
}

#[test]
fn leave_capture_parent_abort_during_confirmation_wait_settles_the_author_without_commit() {
    use crate::flow::clock::{ReadyFd, WaitInterest, WaitOutcome};
    use nix::poll::{PollFd, PollFlags, poll};

    struct CancelWhileRecording<'a> {
        clock: &'a mut FakePumpIo,
        reader: std::os::unix::net::UnixStream,
        sender: i32,
        receiver: i32,
        packet: Option<Vec<u8>>,
        observed_pending: bool,
        watchdog: Instant,
    }
    impl PumpIo for CancelWhileRecording<'_> {
        fn now(&mut self) -> std::io::Result<u64> {
            self.clock.now()
        }
        fn wait(
            &mut self,
            interests: &[WaitInterest<'_>],
            _deadline: Option<u64>,
        ) -> std::io::Result<WaitOutcome> {
            assert!(
                Instant::now() < self.watchdog,
                "confirmation wait did not finish"
            );
            if let Some(packet) = self.packet.take() {
                // Reaching the actual recorder wait proves the confirmation
                // already owns its pending author before Aborted arrives.
                assert_eq!(
                    send(self.sender, &packet, MsgFlags::MSG_NOSIGNAL).unwrap(),
                    packet.len()
                );
                self.observed_pending = true;
                let index = interests
                    .iter()
                    .position(|interest| interest.fd.as_raw_fd() == self.receiver)
                    .unwrap();
                return Ok(WaitOutcome::Ready(vec![ReadyFd {
                    index,
                    events: PollFlags::POLLIN,
                }]));
            }
            // The numeric clock stays fixed. Drain the real sink and probe
            // readiness without blocking a real poll on a synthetic deadline.
            let mut bytes = [0; 65536];
            loop {
                match self.reader.read(&mut bytes) {
                    Ok(0) => panic!("confirmation trace closed"),
                    Ok(_) => (),
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => (),
                    Err(error) => return Err(error),
                }
                let mut descriptors: Vec<_> = interests
                    .iter()
                    .map(|interest| PollFd::new(interest.fd, interest.events))
                    .collect();
                poll(&mut descriptors, 0u8)?;
                let ready: Vec<_> = descriptors
                    .iter()
                    .enumerate()
                    .filter_map(|(index, descriptor)| {
                        let events = descriptor.revents().unwrap();
                        (!events.is_empty()).then_some(ReadyFd { index, events })
                    })
                    .collect();
                if !ready.is_empty() {
                    return Ok(WaitOutcome::Ready(ready));
                }
                assert!(
                    Instant::now() < self.watchdog,
                    "pending confirmation did not observe released trace capacity"
                );
                std::thread::yield_now();
            }
        }
    }

    let mut run = active();
    let mut parent = Parent::new(&mut run);
    let (beginning, artifact) = parent.prepared();
    parent.submit(CapturePayload::SavePointPrepared(artifact));
    parent.step().unwrap();
    assert_eq!(
        parent.capture.at_halt(&parent.member.run.context).state,
        crate::flow::report::CaptureState::AwaitRecorded
    );
    parent.member.fill_trace();
    let packet = CaptureEnvelope {
        exchange: parent.exchange.clone(),
        position: Position::Continue,
        payload: CapturePayload::SavePointAborted(CaptureAborted {
            capture_id: beginning.capture_id,
            code: "publication_failed".into(),
            detail: "cancel during pending confirmation".into(),
        }),
    }
    .encode(CaptureSender::Admin)
    .unwrap();
    let reader = parent.member.run._reader.try_clone().unwrap();
    reader.set_nonblocking(true).unwrap();
    let mut io = CancelWhileRecording {
        clock: &mut parent.member.run.clock,
        reader,
        sender: parent.admin.as_raw_fd(),
        receiver: parent.peer.fd().as_raw_fd(),
        packet: Some(packet),
        observed_pending: false,
        watchdog: Instant::now() + Duration::from_secs(3),
    };
    parent
        .capture
        .advance(
            &mut parent.member.run.context,
            &mut PumpPorts {
                lifecycle: LifecyclePorts {
                    leave_peer: Some(&mut parent.peer),
                    ..Default::default()
                },
                gate: None,
                decode: None,
                label: None,
                io: &mut io,
            },
        )
        .unwrap();
    assert!(io.observed_pending && io.packet.is_none());
    assert!(parent.peer.capture_held().is_some());
    assert!(
        !parent
            .capture
            .at_halt(&parent.member.run.context)
            .commit_sent
    );
    parent.member.drain_trace();
    let request = parent.request();
    assert!(
        matches!(request, s::StateFrame::Control { frame: s::StateControl::ReleaseCapture { capture_id }, .. } if capture_id == beginning.capture_id)
    );
    let answer = parent.member.answer(
        &request,
        s::StateControlAnswer::CaptureReleased {
            capture_id: beginning.capture_id,
        },
    );
    parent.member.receive(&answer).unwrap();
    assert_eq!(
        parent.next().payload,
        CapturePayload::SavePointCaptureRefused(CaptureRefused {
            capture_id: beginning.capture_id,
            missed: SavePointLeg::Published,
        })
    );
    let records = parent.member.records(Kind::SavePoint);
    assert_eq!(
        records
            .iter()
            .map(|record| record["payload"]["phase"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["requested", "confirmed", "refused"]
    );
    assert!(parent.member.run.context.pending_author().is_none());
    assert!(
        !parent
            .capture
            .at_halt(&parent.member.run.context)
            .commit_sent
    );
    assert!(parent.capture.report().is_none());
}
