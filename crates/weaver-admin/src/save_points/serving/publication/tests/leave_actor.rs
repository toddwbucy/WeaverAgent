//! Actual admin channel and publisher sequence, with a scripted worker peer.
use super::*;
use crate::channel::{ChannelFault, Coordination};
use crate::serving_leave::{FailureCause, Leave, Observed, Progress as LeaveProgress};
use nix::sys::socket::{AddressFamily, MsgFlags, SockFlag, SockType, recv, send, socketpair};
use serde_json::json;
use std::os::fd::{AsRawFd, OwnedFd};
use std::time::Duration;
use weaver_types::state_flow::capture::*;
use weaver_types::state_flow::enter::ObservationEnvelope;
use weaver_types::{ExchangeId, Opener, Position};

const SAVE_NS: u64 = 10_000_000_000;

fn pair() -> (Coordination, OwnedFd) {
    let (end, peer) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .unwrap();
    (Coordination::adopt(end), peer)
}

fn read(peer: &OwnedFd) -> Vec<u8> {
    let mut bytes = vec![0; weaver_types::MAX_ENVELOPE_BYTES];
    let count = recv(peer.as_raw_fd(), &mut bytes, MsgFlags::MSG_DONTWAIT).unwrap();
    bytes.truncate(count);
    bytes
}

fn packet(peer: &OwnedFd, exchange: &ExchangeId, payload: CapturePayload) {
    let envelope = CaptureEnvelope {
        exchange: exchange.clone(),
        position: if matches!(
            payload,
            CapturePayload::Answer(_) | CapturePayload::Refusal(_)
        ) {
            Position::Close
        } else {
            Position::Continue
        },
        payload,
    };
    let bytes = envelope.encode(CaptureSender::Harness).unwrap();
    assert_eq!(
        send(peer.as_raw_fd(), &bytes, MsgFlags::MSG_NOSIGNAL),
        Ok(bytes.len())
    );
}

fn response(peer: &OwnedFd) -> CapturePayload {
    CaptureEnvelope::decode(&read(peer), CaptureSender::Admin)
        .unwrap()
        .payload
}

fn seed(fixture: &Fixture, repo: &mut Repository) -> StartIdentity {
    let rendered = declaration::render(&declaration::tests::fixture())
        .unwrap()
        .replace(
            "save-wait.elapsed-ns = \"17\"",
            &format!("save-wait.elapsed-ns = \"{SAVE_NS}\""),
        );
    let created = fixture.seed_image();
    let room = open_directory(&fixture.room).unwrap();
    let mut owner = Publication::seed(
        repo,
        &fixture.scope(),
        parse_serving_draft(&rendered).unwrap(),
        SeedImage {
            expected_seed_id: &created.seed_id,
            created: &created,
            room: room.as_fd(),
            member_uid: fixture.owner.uid,
        },
        &mut alive,
    )
    .unwrap();
    owner.seed_cleanup_complete(&mut alive).unwrap();
    owner.advance(&mut alive).unwrap();
    owner.advance(&mut alive).unwrap();
    let Progress::Complete(identity) = owner.advance(&mut alive).unwrap() else {
        panic!()
    };
    identity
}

fn observed(
    fixture: &Fixture,
    repo: &Repository,
    loaded: &StartIdentity,
    case: &str,
) -> Result<Observed, FailureCause> {
    let artifact = repo
        .verify_indexed(
            &ArtifactId::parse(&loaded.artifact_id).unwrap(),
            &fixture.scope(),
        )
        .unwrap();
    let mut facts = json!({
        "session":fixture.session,"run":run(),"declaration":loaded.config_sha256,
        "artifact":"/models/decoder.gguf","residual_readout":false,"surprisal":false,
        "state":{"serving":{"state_management":artifact.declaration().state_management(),
            "start":loaded,"ste100_scheme":"retained test scheme","ste100_binary_sha256":"e".repeat(64)}},
        "state_store":{"engine":"sqlite"},"state_member":true,"composer":{"binary":"worker"}
    });
    match case {
        "wrong-declaration" => facts["declaration"] = json!("f".repeat(64)),
        "wrong-run" => facts["run"] = json!(""),
        "wrong-settings" => {
            facts["state"]["serving"]["state_management"]["persistence"]["save_wait"]["elapsed_ns"] =
                json!("10000000001")
        }
        _ => (),
    }
    let (mut channel, peer) = pair();
    let worker = std::thread::spawn(move || {
        let mut descriptor = nix::libc::pollfd {
            fd: peer.as_raw_fd(),
            events: nix::libc::POLLIN,
            revents: 0,
        };
        // SAFETY: peer remains owned through this bounded fixture poll.
        assert_eq!(unsafe { nix::libc::poll(&mut descriptor, 1, 3000) }, 1);
        let request: weaver_types::OrganEnvelope = serde_json::from_slice(&read(&peer)).unwrap();
        assert_eq!(
            request.payload,
            weaver_types::Payload::Directive(weaver_types::LifecycleDirective::Observe)
        );
        let value = json!({"exchange":request.exchange,"position":"close",
            "payload":{"kind":"answer","body":{"kind":"state","state":"idle","load":facts}}});
        let answer = ObservationEnvelope::decode(&serde_json::to_vec(&value).unwrap()).unwrap();
        let bytes = answer.encode().unwrap();
        assert_eq!(
            send(peer.as_raw_fd(), &bytes, MsgFlags::MSG_NOSIGNAL),
            Ok(bytes.len())
        );
    });
    let result = Observed::ask(&mut channel, repo, &fixture.scope());
    worker.join().unwrap();
    result
}

fn pending(owner: &mut Leave<'_>) {
    assert_eq!(
        owner.advance(),
        LeaveProgress::Pending,
        "{:?}",
        owner.failure()
    );
}

fn through_prepared<'a>(
    fixture: &Fixture,
    repo: &'a mut Repository,
    case: &str,
) -> (Leave<'a>, OwnedFd, ExchangeId, CaptureArtifact) {
    let loaded = seed(fixture, repo);
    let observed = observed(fixture, repo, &loaded, "valid").unwrap();
    let (channel, peer) = pair();
    let begin = if case.starts_with("graceful") {
        Leave::graceful
    } else {
        Leave::begin
    };
    let mut owner = begin(
        observed,
        repo,
        channel,
        open_directory(&fixture.room).unwrap(),
        fixture.owner.uid,
        weaver_types::Cause { uid: 73 },
        Duration::from_secs(30),
    )
    .unwrap();
    pending(&mut owner);
    let request: weaver_types::OrganEnvelope = serde_json::from_slice(&read(&peer)).unwrap();
    assert_eq!(
        request.exchange,
        ExchangeId {
            opener: Opener::Admin,
            ordinal: 1
        }
    );
    assert_eq!(
        request.payload,
        weaver_types::Payload::Directive(weaver_types::LifecycleDirective::Leave {
            cause: weaver_types::Cause { uid: 73 },
            forced: !case.starts_with("graceful"),
            rollback: false
        })
    );
    let exchange = request.exchange;
    let now = crate::serving_load::window::raw_ns().unwrap();
    packet(
        &peer,
        &exchange,
        CapturePayload::SavePointBeginning(CaptureBeginning {
            capture_id: 9.into(),
            window: weaver_types::state_flow::SaveWindow {
                began_ns: now.into(),
                deadline_ns: (now + SAVE_NS).into(),
            },
            tail_window: Some(weaver_types::state_flow::SaveWindow {
                began_ns: now.into(),
                deadline_ns: (now + weaver_types::LEAVE_BOUND_MS * 1_000_000).into(),
            }),
        }),
    );
    pending(&mut owner);
    fixture.write_room("capture-9.sqlite.part", CAPTURE_BYTES);
    packet(
        &peer,
        &exchange,
        CapturePayload::SavePointImage(CaptureImage {
            capture_id: 9.into(),
            room_name: "capture-9.sqlite.part".into(),
            size_bytes: (CAPTURE_BYTES.len() as u64).into(),
            position: Some(position(9)),
        }),
    );
    pending(&mut owner);
    pending(&mut owner);
    let CapturePayload::SavePointImageChecked(checked) = response(&peer) else {
        panic!()
    };
    assert_eq!(checked.dump_sha256, hash(CAPTURE_BYTES));
    fs::rename(
        fixture.room.join("capture-9.sqlite.part"),
        fixture.room.join("capture-9.sqlite"),
    )
    .unwrap();
    packet(
        &peer,
        &exchange,
        CapturePayload::SavePointImageFinished(CaptureImageFinished {
            capture_id: 9.into(),
            room_name: "capture-9.sqlite".into(),
        }),
    );
    pending(&mut owner);
    pending(&mut owner);
    let CapturePayload::SavePointPrepared(prepared) = response(&peer) else {
        panic!()
    };
    assert_eq!(fixture.index().len(), 1, "{case}: no index before Commit");
    assert!(!fixture.root.join(&prepared.artifact_id).exists());
    (owner, peer, exchange, prepared)
}

#[test]
fn forced_leave_admin_pins_actual_observe_before_requesting_capture() {
    for case in ["wrong-declaration", "wrong-run", "wrong-settings"] {
        let fixture = Fixture::new();
        let mut repo = fixture.repository();
        let loaded = seed(&fixture, &mut repo);
        assert!(
            matches!(
                observed(&fixture, &repo, &loaded, case),
                Err(FailureCause::Protocol(_))
            ),
            "{case}: the actor must reject validly encoded foreign facts"
        );
        assert_eq!(fixture.index().len(), 1);
        assert_eq!(fixture.names().len(), 2);
    }
}

#[test]
fn forced_leave_admin_requires_commit_complete_and_exact_left() {
    for case in [
        "valid",
        "foreign-commit",
        "early-left",
        "foreign-left",
        "wrong-position",
        "unforced-answer",
        "graceful",
        "graceful-converted",
    ] {
        let fixture = Fixture::new();
        let mut repo = fixture.repository();
        let (mut owner, peer, exchange, prepared) = through_prepared(&fixture, &mut repo, case);
        packet(
            &peer,
            &exchange,
            CapturePayload::SavePointCommit(CaptureIdentity {
                capture_id: if case == "foreign-commit" { 10 } else { 9 }.into(),
            }),
        );
        if case == "foreign-commit" {
            assert_eq!(owner.advance(), LeaveProgress::Stopped);
            assert_eq!(fixture.index().len(), 1);
            continue;
        }
        pending(&mut owner);
        pending(&mut owner);
        pending(&mut owner);
        pending(&mut owner);
        assert_eq!(
            response(&peer),
            CapturePayload::SavePointPublished(prepared.clone())
        );
        let identity = fixture.index().last().unwrap().clone();
        assert_eq!(identity.artifact_id.as_str(), prepared.artifact_id);
        assert_eq!(
            identity.cause,
            if case.starts_with("graceful") {
                PersistenceCause::GracefulUnload
            } else {
                PersistenceCause::ForcedUnload
            }
        );
        let published = fixture
            .repository()
            .verify_unindexed_publication(identity.clone(), &fixture.scope())
            .unwrap()
            .identity();
        fixture.assert_guarded(&published, PublicationState::Pending);
        if case != "early-left" {
            packet(
                &peer,
                &exchange,
                CapturePayload::SavePointCaptureComplete(CaptureIdentity {
                    capture_id: 9.into(),
                }),
            );
            pending(&mut owner);
        }
        let mut report = report(&published, 9);
        if case == "foreign-left" {
            report.event_run = RunId("other".into());
        }
        if case == "wrong-position" {
            report.start.position = None;
        }
        packet(
            &peer,
            &exchange,
            CapturePayload::Answer(CaptureAnswer::Left {
                save_point: Some(report),
                forced: !matches!(case, "graceful" | "unforced-answer"),
                no_state: false,
            }),
        );
        assert_eq!(
            owner.advance(),
            if matches!(case, "valid" | "graceful" | "graceful-converted") {
                LeaveProgress::LeftReceived
            } else {
                LeaveProgress::Stopped
            },
            "{case}: {:?}",
            owner.failure()
        );
        fixture.assert_guarded(
            &published,
            if matches!(case, "valid" | "graceful" | "graceful-converted") {
                PublicationState::Pending
            } else {
                PublicationState::Broken
            },
        );
        assert_eq!(owner.advance(), owner.progress());
    }
}

#[test]
fn forced_leave_admin_peer_loss_stops_each_publication_step_and_keeps_guard() {
    for steps in 0..=3 {
        let fixture = Fixture::new();
        let mut repo = fixture.repository();
        let (mut owner, peer, exchange, prepared) = through_prepared(&fixture, &mut repo, "lost");
        if steps > 0 {
            packet(
                &peer,
                &exchange,
                CapturePayload::SavePointCommit(CaptureIdentity {
                    capture_id: 9.into(),
                }),
            );
            for _ in 0..steps {
                pending(&mut owner);
            }
        }
        drop(peer);
        assert_eq!(owner.advance(), LeaveProgress::Stopped);
        assert!(matches!(
            owner.failure(),
            Some(FailureCause::Channel(ChannelFault::Closed))
        ));
        assert_eq!(fixture.index().len(), 1, "no later index write after EOF");
        let guard = PublicationGuard::from_json(
            &fs::read(
                fixture
                    .root
                    .join(format!(".{}.pending", prepared.artifact_id)),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(guard.state, PublicationState::Broken);
        assert_eq!(owner.advance(), LeaveProgress::Stopped);
    }
}

#[test]
fn forced_leave_admin_expired_beginning_starts_no_publication() {
    let fixture = Fixture::new();
    let mut repo = fixture.repository();
    let loaded = seed(&fixture, &mut repo);
    let observed = observed(&fixture, &repo, &loaded, "valid").unwrap();
    let (channel, peer) = pair();
    let mut owner = Leave::begin(
        observed,
        &mut repo,
        channel,
        open_directory(&fixture.room).unwrap(),
        fixture.owner.uid,
        weaver_types::Cause { uid: 73 },
        Duration::from_secs(30),
    )
    .unwrap();
    pending(&mut owner);
    let request: weaver_types::OrganEnvelope = serde_json::from_slice(&read(&peer)).unwrap();
    let now = crate::serving_load::window::raw_ns().unwrap();
    let began = now - SAVE_NS - 1_000_000_000;
    packet(
        &peer,
        &request.exchange,
        CapturePayload::SavePointBeginning(CaptureBeginning {
            capture_id: 9.into(),
            window: weaver_types::state_flow::SaveWindow {
                began_ns: began.into(),
                deadline_ns: (began + SAVE_NS).into(),
            },
            tail_window: Some(weaver_types::state_flow::SaveWindow {
                began_ns: began.into(),
                deadline_ns: (began + weaver_types::LEAVE_BOUND_MS * 1_000_000).into(),
            }),
        }),
    );
    assert_eq!(owner.advance(), LeaveProgress::Stopped);
    assert!(matches!(
        owner.failure(),
        Some(FailureCause::Channel(ChannelFault::Unanswered))
    ));
    assert_eq!(fixture.index().len(), 1);
    assert_eq!(fixture.names().len(), 2);
    assert!(matches!(
        recv(peer.as_raw_fd(), &mut [0; 1], MsgFlags::MSG_DONTWAIT),
        Err(nix::errno::Errno::EAGAIN)
    ));
}

#[test]
fn forced_leave_admin_negative_packet_retains_a_stopped_owner_without_left() {
    for payload in [
        CapturePayload::SavePointAborted(CaptureAborted {
            capture_id: 9.into(),
            code: "trace_failed".into(),
            detail: "original failure".into(),
        }),
        CapturePayload::Refusal(CaptureRefusal::Save(SaveRefusal::SavePointNotTaken {
            missed: SavePointLeg::Answer,
        })),
    ]
    .into_iter()
    .chain([run(), RunId("foreign".into())].into_iter().map(|run| {
        CapturePayload::Refusal(CaptureRefusal::Save(SaveRefusal::FailedRun {
            run,
            capture_id: Some(9.into()),
            phase: PersistencePhase::Confirming,
            published: None,
            artifacts: Vec::new(),
            detail: "original failed run".into(),
        }))
    })) {
        let fixture = Fixture::new();
        let mut repo = fixture.repository();
        let (mut owner, peer, exchange, prepared) =
            through_prepared(&fixture, &mut repo, "negative");
        let foreign = matches!(&payload, CapturePayload::Refusal(CaptureRefusal::Save(
            SaveRefusal::FailedRun { run, .. }
        )) if run.0 == "foreign");
        packet(&peer, &exchange, payload);
        assert_eq!(owner.advance(), LeaveProgress::Stopped);
        if foreign {
            assert!(matches!(owner.failure(), Some(FailureCause::Protocol(_))));
        } else {
            assert!(matches!(owner.failure(), Some(FailureCause::WorkerOutcome)));
        }
        assert_eq!(fixture.index().len(), 1);
        let guard = PublicationGuard::from_json(
            &fs::read(
                fixture
                    .root
                    .join(format!(".{}.pending", prepared.artifact_id)),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(guard.state, PublicationState::Broken);
        assert_eq!(owner.advance(), LeaveProgress::Stopped);
    }
}
