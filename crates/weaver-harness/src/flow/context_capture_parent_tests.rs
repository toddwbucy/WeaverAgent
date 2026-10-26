//! Worker capture with real recorder, state stream and lifecycle packets.
//! The peer's image is synthetic. Publication and process composition are separate.
use super::*;
use crate::flow::clock::testing::FakePumpIo;
use crate::flow::context::leave_capture::{LeaveCapture, LeaveCaptureProgress};
use crate::flow::pump::LifecyclePeer;
use nix::sys::socket::{AddressFamily, MsgFlags, SockFlag, SockType, recv, send, socketpair};
use std::os::fd::{AsRawFd, OwnedFd};
use weaver_types::state_flow::capture::*;
use weaver_types::{ExchangeId, Opener, Position};

struct Parent<'a> {
    member: Member<'a>,
    capture: LeaveCapture,
    peer: LifecyclePeer,
    admin: OwnedFd,
    exchange: ExchangeId,
}

impl<'a> Parent<'a> {
    fn new(run: &'a mut Fixture) -> Self {
        let (near, far) = socketpair(
            AddressFamily::Unix,
            SockType::SeqPacket,
            None,
            SockFlag::SOCK_NONBLOCK | SockFlag::SOCK_CLOEXEC,
        )
        .unwrap();
        let mut peer = LifecyclePeer::from_owned(near).unwrap();
        peer.enable_capture().unwrap();
        let exchange = ExchangeId {
            opener: Opener::Admin,
            ordinal: 19,
        };
        let now = run.clock.now().unwrap();
        let leave = AbsoluteBound::after(now, weaver_types::LEAVE_BOUND_MS * 1_000_000).unwrap();
        run.context
            .arm(PumpDeadline::Leave, leave, &mut run.clock)
            .unwrap();
        let capture = LeaveCapture::begin(
            &mut run.context,
            &peer,
            exchange.clone(),
            leave,
            &mut run.clock,
        )
        .unwrap();
        Self {
            member: Member::new(run),
            capture,
            peer,
            admin: far,
            exchange,
        }
    }
    fn step(&mut self) -> Result<LeaveCaptureProgress, RunFailure> {
        self.capture.advance(
            &mut self.member.run.context,
            &mut PumpPorts {
                lifecycle: LifecyclePorts {
                    leave_peer: Some(&mut self.peer),
                    ..Default::default()
                },
                gate: None,
                decode: None,
                label: None,
                io: &mut self.member.run.clock,
            },
        )
    }
    fn next(&mut self) -> CaptureEnvelope {
        let mut bytes = vec![0; weaver_types::MAX_ENVELOPE_BYTES];
        for _ in 0..256 {
            self.step().unwrap();
            match recv(self.admin.as_raw_fd(), &mut bytes, MsgFlags::MSG_DONTWAIT) {
                Ok(n) => {
                    return CaptureEnvelope::decode(&bytes[..n], CaptureSender::Harness).unwrap();
                }
                Err(nix::errno::Errno::EAGAIN) => (),
                other => panic!("capture packet: {other:?}"),
            }
        }
        panic!("worker did not carry capture packet");
    }
    fn submit(&mut self, payload: CapturePayload) {
        let envelope = CaptureEnvelope {
            exchange: self.exchange.clone(),
            position: Position::Continue,
            payload,
        };
        let bytes = envelope.encode(CaptureSender::Admin).unwrap();
        assert_eq!(
            send(
                self.admin.as_raw_fd(),
                &bytes,
                MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_NOSIGNAL
            )
            .unwrap(),
            bytes.len()
        );
    }
    fn request(&mut self) -> s::StateFrame {
        for _ in 0..256 {
            self.step().unwrap();
            if !self
                .member
                .run
                .context
                .flow
                .state()
                .outstanding()
                .is_empty()
            {
                return self.member.sent();
            }
        }
        panic!("no state request");
    }
    fn image(&mut self) -> (CaptureBeginning, CaptureImage) {
        let beginning = self.next();
        assert_eq!(beginning.exchange, self.exchange);
        let CapturePayload::SavePointBeginning(beginning) = beginning.payload else {
            panic!("Beginning first")
        };
        assert!(
            self.member.run.context.capture_snapshot().is_none(),
            "Beginning must be carried before barrier"
        );
        let request = self.request();
        assert!(
            matches!(request, s::StateFrame::Control { frame: s::StateControl::Snapshot { capture_id }, .. } if capture_id == beginning.capture_id)
        );
        let image = self.member.answer(
            &request,
            s::StateControlAnswer::Snapshot {
                capture_id: beginning.capture_id,
                room_name: format!("capture-{}.sqlite.part", beginning.capture_id.0),
                size_bytes: 10.into(),
                position: None,
            },
        );
        self.member.receive(&image).unwrap();
        let CapturePayload::SavePointImage(image) = self.next().payload else {
            panic!("Image after snapshot")
        };
        (beginning, image)
    }
    fn prepared(&mut self) -> (CaptureBeginning, CaptureArtifact) {
        self.prepared_with_digest(s::Digest::new("9".repeat(64)).unwrap())
    }
    fn prepared_with_digest(&mut self, digest: s::Digest) -> (CaptureBeginning, CaptureArtifact) {
        let (beginning, image) = self.image();
        self.submit(CapturePayload::SavePointImageChecked(CaptureImageChecked {
            capture_id: image.capture_id,
            dump_sha256: digest.clone(),
        }));
        let request = self.request();
        assert!(
            matches!(request, s::StateFrame::Control { frame: s::StateControl::Acknowledge { capture_id, .. }, .. } if capture_id == image.capture_id)
        );
        let answer = self.member.answer(
            &request,
            s::StateControlAnswer::Finished {
                capture_id: image.capture_id,
                room_name: format!("capture-{}.sqlite", image.capture_id.0),
            },
        );
        self.member.receive(&answer).unwrap();
        let CapturePayload::SavePointImageFinished(finished) = self.next().payload else {
            panic!("Finished before Prepared")
        };
        assert_eq!(finished.capture_id, image.capture_id);
        let artifact = CaptureArtifact {
            capture_id: image.capture_id,
            artifact_id: format!("{:020}-{}", 2, "d".repeat(64)),
            dump_sha256: digest,
            config_sha256: s::Digest::new("d".repeat(64)).unwrap(),
        };
        (beginning, artifact)
    }
    fn pressure(&mut self) {
        loop {
            match send(
                self.peer.fd().as_raw_fd(),
                b"pressure",
                MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_NOSIGNAL,
            ) {
                Ok(_) => (),
                Err(nix::errno::Errno::EAGAIN) => break,
                other => panic!("capture pressure: {other:?}"),
            }
        }
    }
    fn drain_pressure(&mut self) {
        let mut bytes = [0; 64];
        loop {
            match recv(self.admin.as_raw_fd(), &mut bytes, MsgFlags::MSG_DONTWAIT) {
                Ok(n) => assert_eq!(&bytes[..n], b"pressure"),
                Err(nix::errno::Errno::EAGAIN) => break,
                other => panic!("capture pressure drain: {other:?}"),
            }
        }
    }
}

#[test]
fn leave_capture_parent_records_confirmation_before_commit_and_keeps_tail_after_complete() {
    let mut run = active();
    let loaded = run.context.open_start.clone().unwrap();
    let mut parent = Parent::new(&mut run);
    assert!(parent.capture.report().is_none());
    assert!(parent.member.run.context.capture_snapshot().is_none());
    let (beginning, artifact) = parent.prepared();
    let frozen = parent.member.run.context.capture_snapshot().unwrap();
    parent.submit(CapturePayload::SavePointPrepared(artifact.clone()));
    let commit = parent.next();
    assert!(
        matches!(commit.payload, CapturePayload::SavePointCommit(CaptureIdentity { capture_id }) if capture_id == beginning.capture_id)
    );
    let records = parent.member.records(Kind::SavePoint);
    assert_eq!(records.len(), 2);
    assert_eq!(records[1]["payload"]["phase"], "confirmed");
    assert!(
        parent.capture.report().is_none(),
        "Commit is not publication or completion"
    );
    assert!(
        parent
            .capture
            .at_halt(&parent.member.run.context)
            .commit_sent
    );
    assert!(
        parent
            .capture
            .at_halt(&parent.member.run.context)
            .published
            .is_none()
    );
    assert_eq!(
        parent.member.run.context.capture_snapshot().unwrap(),
        frozen
    );
    parent.submit(CapturePayload::SavePointPublished(artifact.clone()));
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
    let complete = parent.next();
    assert!(
        matches!(complete.payload, CapturePayload::SavePointCaptureComplete(CaptureIdentity { capture_id }) if capture_id == beginning.capture_id)
    );
    for _ in 0..8 {
        if parent.step().unwrap() == LeaveCaptureProgress::Published {
            break;
        }
    }
    assert_eq!(parent.step().unwrap(), LeaveCaptureProgress::Published);
    let report = parent.capture.report().unwrap();
    assert_eq!(report.start.seed_id, loaded.seed_id);
    assert_eq!(report.start.parent_artifact_id, Some(loaded.artifact_id));
    assert_eq!(report.start.artifact_id, artifact.artifact_id);
    assert_eq!(report.start.position, None);
    assert_eq!(
        report.event_sequence.0,
        parent
            .member
            .run
            .context
            .barrier
            .recorder()
            .structure()
            .by_kind(Kind::SavePoint)
            .nth(1)
            .unwrap()
            .sequence
            .0
    );
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
        parent
            .member
            .run
            .context
            .deadlines
            .get(PumpDeadline::Leave)
            .unwrap()
            .deadline_ns(),
        beginning.tail_window.unwrap().deadline_ns.0
    );
    assert_eq!(parent.member.records(Kind::SavePoint).len(), 3);
}

#[test]
fn leave_capture_parent_accepts_the_actual_publisher_prepared_corpus() {
    let envelope = CaptureEnvelope::decode(
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../weaver-types/tests/fixtures/state-capture-prepared.json"
        )),
        CaptureSender::Admin,
    )
    .unwrap();
    let CapturePayload::SavePointPrepared(artifact) = envelope.payload.clone() else {
        panic!("Prepared corpus")
    };
    let mut run = active();
    let mut parent = Parent::new(&mut run);
    let (beginning, _) = parent.prepared_with_digest(artifact.dump_sha256.clone());
    assert_eq!(envelope.exchange, parent.exchange);
    assert_eq!(artifact.capture_id, beginning.capture_id);
    parent.submit(envelope.payload);
    assert!(matches!(
        parent.next().payload,
        CapturePayload::SavePointCommit(_)
    ));
    let records = parent.member.records(Kind::SavePoint);
    assert_eq!(records[1]["payload"]["artifact_id"], artifact.artifact_id);
    assert_eq!(
        records[1]["payload"]["config_sha256"],
        artifact.config_sha256.as_str()
    );
}

#[test]
fn leave_capture_parent_prepared_uses_artifact_ordinal_and_b_not_seed_identity() {
    for bad in ["seed", "zero", "overflow", "short", "nondigit", "B"] {
        let mut run = active();
        let mut parent = Parent::new(&mut run);
        let (_, mut artifact) = parent.prepared();
        artifact.artifact_id = match bad {
            "seed" => "c".repeat(32),
            "zero" => format!("{:020}-{}", 0, artifact.config_sha256),
            "overflow" => format!("18446744073709551616-{}", artifact.config_sha256),
            "short" => format!("{:019}-{}", 2, artifact.config_sha256),
            "nondigit" => format!("{:+020}-{}", 2, artifact.config_sha256),
            "B" => format!("{:020}-{}", 2, "e".repeat(64)),
            _ => unreachable!(),
        };
        parent.submit(CapturePayload::SavePointPrepared(artifact));
        assert!(
            parent.step().is_err(),
            "{bad} is not the prepared artifact identity"
        );
        assert!(
            !parent
                .capture
                .at_halt(&parent.member.run.context)
                .commit_sent
        );
        assert_eq!(parent.member.records(Kind::SavePoint).len(), 1);
    }
}

#[test]
fn leave_capture_parent_rejects_out_of_order_and_changed_prepared_evidence() {
    for bad in ["early", "id", "hash"] {
        let mut run = active();
        let mut parent = Parent::new(&mut run);
        let (_, mut artifact) = if bad == "early" {
            let (beginning, image) = parent.image();
            (
                beginning,
                CaptureArtifact {
                    capture_id: image.capture_id,
                    artifact_id: format!("{:020}-{}", 2, "d".repeat(64)),
                    dump_sha256: s::Digest::new("9".repeat(64)).unwrap(),
                    config_sha256: s::Digest::new("d".repeat(64)).unwrap(),
                },
            )
        } else {
            parent.prepared()
        };
        if bad == "id" {
            artifact.capture_id.0 += 1;
        }
        if bad == "hash" {
            artifact.dump_sha256 = s::Digest::new("8".repeat(64)).unwrap();
        }
        parent.submit(CapturePayload::SavePointPrepared(artifact));
        let mut failed = false;
        for _ in 0..32 {
            if parent.step().is_err() {
                failed = true;
                break;
            }
        }
        assert!(failed, "{bad} must fail before Commit");
        assert!(
            !parent
                .capture
                .at_halt(&parent.member.run.context)
                .commit_sent
        );
        assert!(parent.capture.report().is_none());
        assert!(parent.member.run.context.halt().is_some());
    }
}

#[test]
fn leave_capture_parent_admin_abort_cleans_the_member_before_refusal_disposition() {
    for finished in [false, true] {
        let mut run = active();
        let mut parent = Parent::new(&mut run);
        let id = if finished {
            parent.prepared().0.capture_id
        } else {
            parent.image().0.capture_id
        };
        parent.submit(CapturePayload::SavePointAborted(CaptureAborted {
            capture_id: id,
            code: "publication_failed".into(),
            detail: "fixture root copy failure".into(),
        }));
        let request = parent.request();
        let answer = if finished {
            assert!(
                matches!(request, s::StateFrame::Control { frame: s::StateControl::ReleaseCapture { capture_id }, .. } if capture_id == id)
            );
            s::StateControlAnswer::CaptureReleased { capture_id: id }
        } else {
            assert!(
                matches!(request, s::StateFrame::Control { frame: s::StateControl::Abort { capture_id }, .. } if capture_id == id)
            );
            s::StateControlAnswer::Aborted { capture_id: id }
        };
        assert!(parent.capture.report().is_none());
        assert!(parent.member.run.context.capture_snapshot().is_some());
        let answer = parent.member.answer(&request, answer);
        parent.member.receive(&answer).unwrap();
        let mut done = false;
        for _ in 0..64 {
            if parent.step().unwrap() == LeaveCaptureProgress::Refused {
                done = true;
                break;
            }
        }
        assert!(done);
        assert!(parent.member.run.context.capture_snapshot().is_none());
        assert!(parent.member.run.context.halt().is_none());
        assert!(
            !parent
                .capture
                .at_halt(&parent.member.run.context)
                .commit_sent
        );
        assert!(parent.capture.report().is_none());
        let records = parent.member.records(Kind::SavePoint);
        assert_eq!(records.len(), 2);
        assert_eq!(records[1]["payload"]["phase"], "refused");
        assert_eq!(records[1]["payload"]["refusal_code"], "publication_failed");
    }
}

#[test]
fn leave_capture_parent_missing_image_cleans_without_commit_or_component_halt() {
    let mut run = active();
    let mut parent = Parent::new(&mut run);
    let CapturePayload::SavePointBeginning(beginning) = parent.next().payload else {
        panic!("Beginning")
    };
    let original = parent.request();
    let bound = parent
        .member
        .run
        .context
        .deadlines
        .get(PumpDeadline::CaptureResult)
        .unwrap();
    parent
        .member
        .run
        .clock
        .advance_to(bound.deadline_ns())
        .unwrap();
    let abort = parent.request();
    assert_eq!(abort.common(), original.common());
    assert!(
        matches!(abort, s::StateFrame::Control { frame: s::StateControl::Abort { capture_id }, .. } if capture_id == beginning.capture_id)
    );
    let late = parent.member.image(&original, beginning.capture_id.0);
    parent.member.receive(&late).unwrap();
    for _ in 0..16 {
        assert_ne!(parent.step().unwrap(), LeaveCaptureProgress::Refused);
    }
    assert!(parent.member.run.context.capture_snapshot().is_some());
    let aborted = parent.member.answer(
        &abort,
        s::StateControlAnswer::Aborted {
            capture_id: beginning.capture_id,
        },
    );
    parent.member.receive(&aborted).unwrap();
    let mut done = false;
    for _ in 0..64 {
        if parent.step().unwrap() == LeaveCaptureProgress::Refused {
            done = true;
            break;
        }
    }
    assert!(done);
    assert!(parent.member.run.context.halt().is_none());
    assert!(parent.capture.report().is_none());
    assert!(
        !parent
            .capture
            .at_halt(&parent.member.run.context)
            .commit_sent
    );
    assert!(parent.member.run.context.capture_snapshot().is_none());
    assert_eq!(
        parent.member.records(Kind::SavePoint)[1]["payload"]["refusal_code"],
        "snapshot_timeout"
    );
}

#[test]
fn leave_capture_parent_beginning_backpressure_precedes_barrier_and_keeps_window() {
    let mut run = active();
    let mut parent = Parent::new(&mut run);
    let window = parent
        .member
        .run
        .context
        .deadlines
        .get(PumpDeadline::CaptureWindow)
        .unwrap();
    parent.step().unwrap();
    parent.pressure();
    for _ in 0..8 {
        assert_eq!(parent.step().unwrap(), LeaveCaptureProgress::Pending);
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
        assert_eq!(
            parent
                .member
                .run
                .context
                .deadlines
                .get(PumpDeadline::CaptureWindow),
            Some(window)
        );
    }
    parent.drain_pressure();
    let CapturePayload::SavePointBeginning(beginning) = parent.next().payload else {
        panic!("original Beginning")
    };
    assert_eq!(beginning.window.began_ns.0, window.began_ns());
    assert_eq!(beginning.window.deadline_ns.0, window.deadline_ns());
    assert!(parent.member.run.context.capture_snapshot().is_none());
}

#[test]
fn leave_capture_parent_capture_complete_pressure_keeps_original_window_and_no_report() {
    let mut run = active();
    let mut parent = Parent::new(&mut run);
    let (beginning, artifact) = parent.prepared();
    parent.submit(CapturePayload::SavePointPrepared(artifact.clone()));
    assert!(matches!(
        parent.next().payload,
        CapturePayload::SavePointCommit(_)
    ));
    parent.submit(CapturePayload::SavePointPublished(artifact));
    let request = parent.request();
    let answer = parent.member.answer(
        &request,
        s::StateControlAnswer::CaptureReleased {
            capture_id: beginning.capture_id,
        },
    );
    parent.member.receive(&answer).unwrap();
    parent.pressure();
    for _ in 0..32 {
        parent.step().unwrap();
    }
    assert!(
        parent
            .capture
            .at_halt(&parent.member.run.context)
            .published
            .is_some()
    );
    assert!(
        parent.capture.report().is_none(),
        "queued Complete grants no final report"
    );
    assert!(parent.member.run.context.capture_snapshot().is_some());
    assert_eq!(
        parent
            .member
            .run
            .context
            .deadlines
            .get(PumpDeadline::CaptureWindow)
            .unwrap()
            .deadline_ns(),
        beginning.window.deadline_ns.0
    );
    parent.drain_pressure();
    assert!(matches!(
        parent.next().payload,
        CapturePayload::SavePointCaptureComplete(_)
    ));
    assert!(parent.capture.report().is_some());
    assert!(
        parent
            .member
            .run
            .context
            .deadlines
            .get(PumpDeadline::CaptureWindow)
            .is_none()
    );
}

#[test]
fn leave_capture_parent_reduces_only_timely_complete_before_checking_remaining_leave() {
    for boundary in ["timely", "late_send", "leave", "fatal"] {
        let mut run = active();
        let mut parent = Parent::new(&mut run);
        let (beginning, artifact) = parent.prepared();
        parent.submit(CapturePayload::SavePointPrepared(artifact.clone()));
        assert!(matches!(
            parent.next().payload,
            CapturePayload::SavePointCommit(_)
        ));
        parent.submit(CapturePayload::SavePointPublished(artifact));
        let request = parent.request();
        let answer = parent.member.answer(
            &request,
            s::StateControlAnswer::CaptureReleased {
                capture_id: beginning.capture_id,
            },
        );
        parent.member.receive(&answer).unwrap();
        parent.pressure();
        for _ in 0..32 {
            parent.step().unwrap();
        }
        assert!(parent.capture.report().is_none());
        parent.drain_pressure();
        if boundary == "late_send" {
            parent
                .member
                .run
                .clock
                .advance_to(beginning.window.deadline_ns.0)
                .unwrap();
        }
        // The shared pump may finish this retained send before the owner is
        // scheduled again. The kernel carry and its clock are the evidence.
        parent
            .peer
            .try_send_observed(&mut parent.member.run.clock)
            .unwrap();
        let mut bytes = vec![0; weaver_types::MAX_ENVELOPE_BYTES];
        let n = recv(parent.admin.as_raw_fd(), &mut bytes, MsgFlags::MSG_DONTWAIT).unwrap();
        assert!(matches!(
            CaptureEnvelope::decode(&bytes[..n], CaptureSender::Harness)
                .unwrap()
                .payload,
            CapturePayload::SavePointCaptureComplete(_)
        ));
        let leave = parent
            .member
            .run
            .context
            .deadlines
            .get(PumpDeadline::Leave)
            .unwrap();
        parent
            .member
            .run
            .clock
            .advance_to(if boundary == "leave" {
                leave.deadline_ns()
            } else {
                beginning.window.deadline_ns.0
            })
            .unwrap();
        if boundary == "fatal" {
            parent.member.run.context.fail(
                &mut parent.member.run.clock,
                FlowHaltReason::ProtocolFailed,
                HaltPoint::Barrier,
                "fixture preexisting failure",
            );
        }
        if boundary == "timely" {
            assert_eq!(parent.step().unwrap(), LeaveCaptureProgress::Published);
            assert!(parent.capture.report().is_some());
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
        } else {
            assert!(
                parent.step().is_err(),
                "{boundary} cannot become a fresh success"
            );
            assert!(parent.capture.report().is_none());
            assert!(parent.member.run.context.capture_snapshot().is_some());
            assert_eq!(
                parent.member.run.context.deadlines.get(PumpDeadline::Leave),
                Some(leave)
            );
        }
    }
}

#[test]
fn leave_capture_parent_retains_commit_and_rejects_changed_published_or_backward_abort() {
    for changed in ["artifact", "A", "B", "abort"] {
        let mut run = active();
        let mut parent = Parent::new(&mut run);
        let (_, mut artifact) = parent.prepared();
        parent.submit(CapturePayload::SavePointPrepared(artifact.clone()));
        assert!(matches!(
            parent.next().payload,
            CapturePayload::SavePointCommit(_)
        ));
        match changed {
            "artifact" => artifact.artifact_id = format!("{:020}-{}", 3, "d".repeat(64)),
            "A" => artifact.dump_sha256 = s::Digest::new("f".repeat(64)).unwrap(),
            "B" => artifact.config_sha256 = s::Digest::new("f".repeat(64)).unwrap(),
            _ => (),
        }
        parent.submit(if changed == "abort" {
            CapturePayload::SavePointAborted(CaptureAborted {
                capture_id: artifact.capture_id,
                code: "publication_failed".into(),
                detail: "backward after Commit".into(),
            })
        } else {
            CapturePayload::SavePointPublished(artifact)
        });
        assert!(parent.step().is_err());
        let first = parent.member.run.context.failure.clone().unwrap();
        assert_eq!(parent.step().unwrap_err(), first);
        assert!(
            parent
                .capture
                .at_halt(&parent.member.run.context)
                .commit_sent
        );
        assert!(
            parent
                .capture
                .at_halt(&parent.member.run.context)
                .published
                .is_none()
        );
        assert!(parent.capture.report().is_none());
        assert!(
            parent
                .member
                .run
                .context
                .flow
                .state()
                .outstanding()
                .is_empty(),
            "no release after mismatched publication"
        );
    }
}

#[test]
fn leave_capture_parent_abort_after_recorded_confirmation_cannot_send_commit() {
    let mut run = active();
    let mut parent = Parent::new(&mut run);
    let (beginning, artifact) = parent.prepared();
    parent.submit(CapturePayload::SavePointPrepared(artifact));
    for _ in 0..32 {
        parent.step().unwrap();
        if parent.member.records(Kind::SavePoint).len() == 2 {
            break;
        }
    }
    assert_eq!(
        parent.member.records(Kind::SavePoint)[1]["payload"]["phase"],
        "confirmed"
    );
    assert!(
        !parent
            .capture
            .at_halt(&parent.member.run.context)
            .commit_sent
    );
    parent.submit(CapturePayload::SavePointAborted(CaptureAborted {
        capture_id: beginning.capture_id,
        code: "publication_failed".into(),
        detail: "pre-Commit cancellation".into(),
    }));
    let request = parent.request();
    assert!(matches!(
        request,
        s::StateFrame::Control {
            frame: s::StateControl::ReleaseCapture { .. },
            ..
        }
    ));
    assert!(
        !parent
            .capture
            .at_halt(&parent.member.run.context)
            .commit_sent
    );
    let answer = parent.member.answer(
        &request,
        s::StateControlAnswer::CaptureReleased {
            capture_id: beginning.capture_id,
        },
    );
    parent.member.receive(&answer).unwrap();
    for _ in 0..64 {
        if parent.step().unwrap() == LeaveCaptureProgress::Refused {
            break;
        }
    }
    assert_eq!(parent.step().unwrap(), LeaveCaptureProgress::Refused);
    assert!(
        !parent
            .capture
            .at_halt(&parent.member.run.context)
            .commit_sent
    );
    assert_eq!(parent.member.records(Kind::SavePoint).len(), 3);
}

#[test]
fn leave_capture_parent_rejects_a_foreign_port_with_the_same_exchange() {
    let mut run = active();
    let mut parent = Parent::new(&mut run);
    let (near, _far) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_NONBLOCK | SockFlag::SOCK_CLOEXEC,
    )
    .unwrap();
    let mut foreign = LifecyclePeer::from_owned(near).unwrap();
    foreign.enable_capture().unwrap();
    let original = std::mem::replace(&mut parent.peer, foreign);
    assert!(parent.step().is_err());
    assert!(parent.member.records(Kind::SavePoint).is_empty());
    assert!(original.capture_held().is_none());
    assert!(parent.capture.report().is_none());
}

#[test]
fn leave_capture_parent_halt_phase_keeps_a_waiting_barrier_distinct_from_snapshot() {
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
    let CapturePayload::SavePointBeginning(_) = parent.next().payload else {
        panic!("Beginning first")
    };
    assert_eq!(parent.step().unwrap(), LeaveCaptureProgress::Pending);
    assert_eq!(
        parent.member.run.context.capture_snapshot().unwrap().phase,
        CapturePhase::Barrier
    );
    assert_eq!(
        parent.capture.at_halt(&parent.member.run.context).state,
        crate::flow::report::CaptureState::Barrier
    );
}

#[test]
fn leave_capture_parent_disconnect_racing_send_or_receive_cancels_without_component_halt() {
    use crate::flow::clock::{WaitInterest, WaitOutcome};
    struct Disconnect<'a> {
        clock: &'a mut FakePumpIo,
        admin: i32,
        reads: usize,
        at: usize,
    }
    impl PumpIo for Disconnect<'_> {
        fn now(&mut self) -> std::io::Result<u64> {
            self.reads += 1;
            if self.reads == self.at {
                nix::sys::socket::shutdown(self.admin, nix::sys::socket::Shutdown::Both).unwrap();
            }
            self.clock.now()
        }
        fn wait(
            &mut self,
            interests: &[WaitInterest<'_>],
            deadline: Option<u64>,
        ) -> std::io::Result<WaitOutcome> {
            self.clock.wait(interests, deadline)
        }
    }
    for phase in ["beginning", "checked", "prepared"] {
        let mut run = active();
        let mut parent = Parent::new(&mut run);
        let id = match phase {
            "checked" => Some(parent.image().0.capture_id),
            "prepared" => Some(parent.prepared().0.capture_id),
            _ => {
                parent.step().unwrap();
                None
            }
        };
        let mut clock = Disconnect {
            clock: &mut parent.member.run.clock,
            admin: parent.admin.as_raw_fd(),
            reads: 0,
            at: if phase == "beginning" { 2 } else { 1 },
        };
        let result = parent.capture.advance(
            &mut parent.member.run.context,
            &mut PumpPorts {
                lifecycle: LifecyclePorts {
                    leave_peer: Some(&mut parent.peer),
                    ..Default::default()
                },
                gate: None,
                decode: None,
                label: None,
                io: &mut clock,
            },
        );
        assert!(
            clock.reads >= clock.at,
            "disconnect occurred after the receive probe"
        );
        assert!(
            result.is_ok(),
            "pre-Commit loss at {phase} is capture cancellation: {result:?}"
        );
        if let Some(id) = id {
            let request = parent.request();
            let answer = if phase == "checked" {
                assert!(matches!(
                    request,
                    s::StateFrame::Control {
                        frame: s::StateControl::Abort { .. },
                        ..
                    }
                ));
                s::StateControlAnswer::Aborted { capture_id: id }
            } else {
                assert!(matches!(
                    request,
                    s::StateFrame::Control {
                        frame: s::StateControl::ReleaseCapture { .. },
                        ..
                    }
                ));
                s::StateControlAnswer::CaptureReleased { capture_id: id }
            };
            let answer = parent.member.answer(&request, answer);
            parent.member.receive(&answer).unwrap();
        }
        for _ in 0..32 {
            if parent.step().unwrap() == LeaveCaptureProgress::Refused {
                break;
            }
        }
        assert_eq!(parent.step().unwrap(), LeaveCaptureProgress::Refused);
        assert!(parent.member.run.context.halt().is_none());
        assert!(parent.capture.report().is_none());
        assert!(
            !parent
                .capture
                .at_halt(&parent.member.run.context)
                .commit_sent
        );
        assert_eq!(parent.member.records(Kind::SavePoint).len(), 2);
    }
}

#[test]
fn leave_capture_parent_abort_with_pending_commit_withdraws_the_original_datagram() {
    let mut run = active();
    let mut parent = Parent::new(&mut run);
    let (beginning, artifact) = parent.prepared();
    parent.pressure();
    parent.submit(CapturePayload::SavePointPrepared(artifact));
    for _ in 0..32 {
        parent.step().unwrap();
    }
    assert_eq!(parent.member.records(Kind::SavePoint).len(), 2);
    assert!(
        !parent
            .capture
            .at_halt(&parent.member.run.context)
            .commit_sent
    );
    parent.submit(CapturePayload::SavePointAborted(CaptureAborted {
        capture_id: beginning.capture_id,
        code: "publication_failed".into(),
        detail: "cancel queued Commit".into(),
    }));
    let request = parent.request();
    assert!(matches!(
        request,
        s::StateFrame::Control {
            frame: s::StateControl::ReleaseCapture { .. },
            ..
        }
    ));
    parent.drain_pressure();
    parent
        .peer
        .try_send_observed(&mut parent.member.run.clock)
        .unwrap();
    let mut bytes = [0; 1024];
    assert_eq!(
        recv(parent.admin.as_raw_fd(), &mut bytes, MsgFlags::MSG_DONTWAIT),
        Err(nix::errno::Errno::EAGAIN)
    );
    let answer = parent.member.answer(
        &request,
        s::StateControlAnswer::CaptureReleased {
            capture_id: beginning.capture_id,
        },
    );
    parent.member.receive(&answer).unwrap();
    for _ in 0..64 {
        if parent.step().unwrap() == LeaveCaptureProgress::Refused {
            break;
        }
    }
    assert_eq!(parent.step().unwrap(), LeaveCaptureProgress::Refused);
    assert!(
        !parent
            .capture
            .at_halt(&parent.member.run.context)
            .commit_sent
    );
}

#[test]
fn leave_capture_parent_records_exact_member_refusal_only_after_matching_abort() {
    for finished in [false, true] {
        let mut run = active();
        let mut parent = Parent::new(&mut run);
        let (id, request) = if finished {
            let (beginning, image) = parent.image();
            parent.submit(CapturePayload::SavePointImageChecked(CaptureImageChecked {
                capture_id: beginning.capture_id,
                dump_sha256: s::Digest::new("9".repeat(64)).unwrap(),
            }));
            (image.capture_id, parent.request())
        } else {
            let CapturePayload::SavePointBeginning(beginning) = parent.next().payload else {
                panic!("Beginning")
            };
            (beginning.capture_id, parent.request())
        };
        let original = parent.member.answer(
            &request,
            s::StateControlAnswer::ControlRefused {
                code: s::ControlRefusalCode::Capture(s::CaptureRefusalCode::SnapshotFailed),
                detail: "exact explicit member refusal".into(),
            },
        );
        parent.member.receive(&original).unwrap();
        // The original request remains present until the refusal is Recorded.
        for _ in 0..64 {
            parent.step().unwrap();
            if parent.member.run.context.capture_snapshot().unwrap().phase == CapturePhase::Aborting
            {
                break;
            }
        }
        let abort = parent.member.sent();
        assert!(
            matches!(abort, s::StateFrame::Control { frame: s::StateControl::Abort { capture_id }, .. } if capture_id == id)
        );
        assert_eq!(parent.member.records(Kind::SavePoint).len(), 1);
        let answer = parent
            .member
            .answer(&abort, s::StateControlAnswer::Aborted { capture_id: id });
        parent.member.receive(&answer).unwrap();
        for _ in 0..64 {
            if parent.step().unwrap() == LeaveCaptureProgress::Refused {
                break;
            }
        }
        assert_eq!(parent.step().unwrap(), LeaveCaptureProgress::Refused);
        assert_eq!(
            parent.member.records(Kind::SavePoint)[1]["payload"]["refusal_code"],
            "snapshot_failed"
        );
        assert!(parent.member.run.context.halt().is_none());
        assert!(
            !parent
                .capture
                .at_halt(&parent.member.run.context)
                .commit_sent
        );
    }
}

#[test]
fn leave_capture_parent_failed_transfer_freezes_actual_commit_and_publication() {
    use crate::flow::context::failed_enter_driver::FailedRunCleanup;
    for phase in ["prepared", "committed", "published"] {
        let mut run = active();
        let resources = run.context.failed_enter_resources().unwrap();
        let mut parent = Parent::new(&mut run);
        let (beginning, artifact) = parent.prepared();
        if phase != "prepared" {
            parent.submit(CapturePayload::SavePointPrepared(artifact.clone()));
            assert!(matches!(
                parent.next().payload,
                CapturePayload::SavePointCommit(_)
            ));
        }
        if phase == "published" {
            parent.submit(CapturePayload::SavePointPublished(artifact));
            parent.step().unwrap();
        }
        let leave = parent
            .member
            .run
            .context
            .deadlines
            .get(PumpDeadline::Leave)
            .unwrap();
        let first = parent.member.run.context.fail(
            &mut parent.member.run.clock,
            FlowHaltReason::ProtocolFailed,
            HaltPoint::Barrier,
            "fixture capture failure",
        );
        let evidence = parent
            .capture
            .into_failed(&parent.member.run.context, parent.peer)
            .ok()
            .unwrap();
        assert_eq!(evidence.facts().capture_id, beginning.capture_id);
        assert_eq!(evidence.facts().commit_sent, phase != "prepared");
        assert_eq!(evidence.facts().published.is_some(), phase == "published");
        let expected = evidence.facts().clone();
        let context = crate::flow::context::failed_enter_driver::tests::take_context(&mut run);
        let mut cleanup = FailedRunCleanup::after_serving_rollback(
            context,
            resources,
            leave,
            weaver_types::Cause { uid: 0 },
            None,
            None,
        );
        assert!(cleanup.retain_failed_capture(evidence).is_ok());
        cleanup.advance(&mut run.clock);
        assert_eq!(cleanup.context.failure, Some(first));
        let account = cleanup
            .context
            .flow
            .halt()
            .unwrap()
            .account()
            .expect("actual frozen halt account");
        assert_eq!(account.capture.as_ref(), Some(&expected));
        assert!(cleanup.deadline().unwrap() <= leave.deadline_ns());
        assert!(cleanup.context.capture_snapshot().is_some());
    }
}

#[test]
fn leave_capture_parent_expired_failure_keeps_capture_and_closes_member_without_trace() {
    use crate::flow::context::failed_enter_driver::{FailedEnterProgress, FailedRunCleanup};
    let mut run = active();
    let resources = run.context.failed_enter_resources().unwrap();
    let mut parent = Parent::new(&mut run);
    let (_, artifact) = parent.prepared();
    parent.submit(CapturePayload::SavePointPrepared(artifact));
    assert!(matches!(
        parent.next().payload,
        CapturePayload::SavePointCommit(_)
    ));
    let context = &mut parent.member.run.context;
    let leave = context.deadlines.get(PumpDeadline::Leave).unwrap();
    let frozen = context.capture_snapshot().unwrap();
    let recorded = context.barrier.latest_recorded();
    let first = context.fail(
        &mut parent.member.run.clock,
        FlowHaltReason::ProtocolFailed,
        HaltPoint::Barrier,
        "fixture capture failure before expired cleanup",
    );
    let evidence = parent
        .capture
        .into_failed(context, parent.peer)
        .ok()
        .unwrap();
    let expected = evidence.facts().clone();
    assert!(
        evidence.disposal(context).is_some(),
        "owned failed capture permits closing even before halt account allocation"
    );
    let context = crate::flow::context::failed_enter_driver::tests::take_context(&mut run);
    let mut cleanup = FailedRunCleanup::after_serving_rollback(
        context,
        resources,
        leave,
        weaver_types::Cause { uid: 0 },
        None,
        None,
    );
    assert!(cleanup.retain_failed_capture(evidence).is_ok());
    run.clock.advance_to(leave.deadline_ns()).unwrap();
    assert_eq!(cleanup.advance(&mut run.clock), FailedEnterProgress::Closed);
    assert!(cleanup.abandoned());
    assert_eq!(cleanup.context.failure, Some(first));
    assert_eq!(cleanup.context.capture_snapshot(), Some(frozen));
    assert!(cleanup.context.member_capture.is_some());
    assert_eq!(
        cleanup
            .context
            .flow
            .halt()
            .unwrap()
            .account()
            .unwrap()
            .capture
            .as_ref(),
        Some(&expected)
    );
    assert!(cleanup.context.flow.standing_state().is_none());
    assert_eq!(cleanup.context.barrier.latest_recorded(), recorded);
    assert!(cleanup.context.release_recorded_sequence().is_none());
}

#[test]
fn leave_capture_parent_failed_disposal_rejects_foreign_custody_and_changed_facts() {
    let mut run = active();
    let mut parent = Parent::new(&mut run);
    parent.prepared();
    let context = &mut parent.member.run.context;
    context.fail(
        &mut parent.member.run.clock,
        FlowHaltReason::ProtocolFailed,
        HaltPoint::Barrier,
        "fixture failed capture disposal",
    );
    let evidence = parent
        .capture
        .into_failed(context, parent.peer)
        .ok()
        .unwrap();
    let permit = evidence.disposal(context).unwrap();
    let capture = context.capture_snapshot().unwrap();
    // Expiry must close owned FDs even if allocation of the account failed.
    assert!(permit.allows(&context.barrier, None, Some(capture)));
    context
        .freeze_halt_account(crate::flow::halt_snapshot::ExternalFacts {
            outstanding_pairs: context.outstanding_pairs(),
            capture: Some(evidence.facts().clone()),
            wait: context.halt_wait().cloned(),
            input_id: context.input.snapshot().map(|input| input.input_id),
        })
        .unwrap();
    let account = context.flow.halt().unwrap().account().unwrap().clone();
    let mut changed = account.clone();
    changed.capture = None;
    assert!(!permit.allows(&context.barrier, Some(&changed), Some(capture)));
    changed.capture = account.capture.clone();
    changed.capture.as_mut().unwrap().commit_sent = true;
    assert!(!permit.allows(&context.barrier, Some(&changed), Some(capture)));
    let mut foreign_capture = capture;
    foreign_capture.capture_id = (capture.capture_id.0 + 1).into();
    assert!(!permit.allows(&context.barrier, Some(&account), Some(foreign_capture)));
    let foreign = active();
    assert!(
        context
            .flow
            .retire_failed_capture_ports(&permit, &foreign.context.barrier)
            .is_none()
    );
    assert!(context.flow.standing_state().is_some());
    assert!(
        context
            .flow
            .retire_failed_capture_ports(&permit, &context.barrier)
            .is_some()
    );
    assert!(context.flow.standing_state().is_none());
    assert_eq!(context.capture_snapshot(), Some(capture));
    assert_eq!(
        context.flow.halt().unwrap().account().unwrap().capture,
        account.capture
    );
}
