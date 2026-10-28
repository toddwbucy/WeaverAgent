//! Actual admin channel and publisher sequence, with a scripted worker peer.
use super::*;
use crate::channel::{ChannelFault, Coordination};
use crate::serving_leave::{Capture, FailureCause, Observed, Progress as LeaveProgress};
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

fn pending(owner: &mut Capture<'_>) {
    assert_eq!(
        owner.advance(),
        LeaveProgress::Pending,
        "{:?}",
        owner.failure()
    );
}

fn through_beginning<'a>(
    fixture: &Fixture,
    repo: &'a mut Repository,
    case: &str,
) -> (Capture<'a>, OwnedFd, ExchangeId) {
    let loaded = seed(fixture, repo);
    let observed = observed(fixture, repo, &loaded, "valid").unwrap();
    let (channel, peer) = pair();
    let demand = case.starts_with("demand");
    let room = open_directory(&fixture.room).unwrap();
    let mut owner = if demand {
        Capture::demand(
            observed,
            repo,
            channel,
            room,
            fixture.owner.uid,
            weaver_types::Cause { uid: 73 },
        )
    } else {
        let begin = if case.starts_with("graceful") {
            Capture::graceful
        } else {
            Capture::begin
        };
        begin(
            observed,
            repo,
            channel,
            room,
            fixture.owner.uid,
            weaver_types::Cause { uid: 73 },
            Duration::from_secs(30),
        )
    }
    .unwrap();
    pending(&mut owner);
    let (exchange, demand_window) = if demand {
        let request = CaptureEnvelope::decode(&read(&peer), CaptureSender::Admin).unwrap();
        assert_eq!(request.position, Position::Open);
        let CapturePayload::Directive(CaptureDirective::SavePoint { cause, window }) =
            request.payload
        else {
            panic!("paired demand opener")
        };
        assert_eq!(cause, weaver_types::Cause { uid: 73 });
        assert_eq!(window.deadline_ns.0 - window.began_ns.0, SAVE_NS);
        (request.exchange, Some(window))
    } else {
        let request: weaver_types::OrganEnvelope = serde_json::from_slice(&read(&peer)).unwrap();
        assert_eq!(
            request.payload,
            weaver_types::Payload::Directive(weaver_types::LifecycleDirective::Leave {
                cause: weaver_types::Cause { uid: 73 },
                forced: !case.starts_with("graceful"),
                rollback: false,
            })
        );
        (request.exchange, None)
    };
    assert_eq!(
        exchange,
        ExchangeId {
            opener: Opener::Admin,
            ordinal: 1
        }
    );
    let now = crate::serving_load::window::raw_ns().unwrap();
    packet(
        &peer,
        &exchange,
        CapturePayload::SavePointBeginning(CaptureBeginning {
            capture_id: 9.into(),
            window: demand_window.unwrap_or(weaver_types::state_flow::SaveWindow {
                began_ns: now.into(),
                deadline_ns: (now + SAVE_NS).into(),
            }),
            tail_window: (!demand).then_some(weaver_types::state_flow::SaveWindow {
                began_ns: now.into(),
                deadline_ns: (now + weaver_types::LEAVE_BOUND_MS * 1_000_000).into(),
            }),
        }),
    );
    pending(&mut owner);
    (owner, peer, exchange)
}

fn through_prepared<'a>(
    fixture: &Fixture,
    repo: &'a mut Repository,
    case: &str,
) -> (Capture<'a>, OwnedFd, ExchangeId, CaptureArtifact) {
    let (mut owner, peer, exchange) = through_beginning(fixture, repo, case);
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

fn refused_packet(peer: &OwnedFd, exchange: &ExchangeId, missed: SavePointLeg) {
    packet(
        peer,
        exchange,
        CapturePayload::SavePointCaptureRefused(CaptureRefused {
            capture_id: 9.into(),
            missed,
        }),
    );
}

#[test]
fn refused_leave_admin_awaits_final_outcome_and_removes_only_unconfirmed_publication() {
    for mode in ["forced", "graceful", "graceful-converted"] {
        let fixture = Fixture::new();
        let mut repo = fixture.repository();
        let (mut owner, peer, exchange, _) = through_prepared(&fixture, &mut repo, mode);
        refused_packet(&peer, &exchange, SavePointLeg::Published);
        pending(&mut owner);
        assert!(
            owner.finish_leave().is_err(),
            "acknowledgement is no saved Leave"
        );
        assert_eq!(fixture.index().len(), 1);
        assert_eq!(
            fixture.names().len(),
            2,
            "unconfirmed part and owned guard removed"
        );
        packet(
            &peer,
            &exchange,
            if mode == "graceful" {
                CapturePayload::Refusal(CaptureRefusal::Save(SaveRefusal::SavePointNotTaken {
                    missed: SavePointLeg::Published,
                }))
            } else {
                CapturePayload::Answer(CaptureAnswer::Left {
                    save_point: None,
                    forced: true,
                    no_state: false,
                })
            },
        );
        assert_eq!(
            owner.advance(),
            if mode == "graceful" {
                LeaveProgress::Stopped
            } else {
                LeaveProgress::LeftReceived
            },
            "{mode}: {:?}",
            owner.failure()
        );
        if mode == "graceful" {
            assert!(matches!(owner.failure(), Some(FailureCause::WorkerOutcome)));
        } else {
            assert!(owner.failure().is_none());
        }
        assert!(
            owner.finish_leave().is_err(),
            "refused capture has no save to finalize"
        );
        assert_eq!(fixture.index().len(), 1);
    }
}

#[test]
fn refused_leave_admin_preserves_typed_terminal_refusals_and_rejects_foreign_identity() {
    for mode in ["forced", "graceful"] {
        for case in [
            "lifecycle",
            "malformed",
            "gate-refusal",
            "failed-run",
            "foreign-run",
            "foreign-capture",
            "foreign-exchange",
        ] {
            let fixture = Fixture::new();
            let mut repo = fixture.repository();
            let (mut owner, peer, exchange, _) = through_prepared(&fixture, &mut repo, mode);
            refused_packet(&peer, &exchange, SavePointLeg::Published);
            pending(&mut owner);
            let refusal = if case == "lifecycle" {
                CaptureRefusal::Lifecycle(weaver_types::LifecycleRefusal::DescriptorsUnusable)
            } else if case == "malformed" {
                CaptureRefusal::Lifecycle(weaver_types::LifecycleRefusal::Malformed)
            } else if case == "gate-refusal" {
                CaptureRefusal::Lifecycle(weaver_types::LifecycleRefusal::OrganRefused {
                    organ: weaver_types::RefusingOrgan::Gate,
                    reason: Box::new(weaver_types::LifecycleRefusal::OutOfOrder),
                })
            } else {
                CaptureRefusal::Save(SaveRefusal::FailedRun {
                    run: if case == "foreign-run" {
                        RunId("foreign".into())
                    } else {
                        run()
                    },
                    capture_id: Some(if case == "foreign-capture" { 10 } else { 9 }.into()),
                    phase: PersistencePhase::Confirming,
                    published: None,
                    artifacts: Vec::new(),
                    detail: "retained terminal failure".into(),
                })
            };
            let mut response_exchange = exchange.clone();
            if case == "foreign-exchange" {
                response_exchange.ordinal += 1;
            }
            packet(&peer, &response_exchange, CapturePayload::Refusal(refusal));
            assert_eq!(owner.advance(), LeaveProgress::Stopped);
            if case.starts_with("foreign-") {
                assert!(
                    matches!(owner.failure(), Some(FailureCause::Protocol(_))),
                    "{mode}/{case}: {:?}",
                    owner.failure()
                );
            } else {
                assert!(
                    matches!(owner.failure(), Some(FailureCause::WorkerOutcome)),
                    "{mode}/{case}: {:?}",
                    owner.failure()
                );
            }
            assert!(owner.finish_leave().is_err());
            assert_eq!(fixture.index().len(), 1);
            assert_eq!(fixture.names().len(), 2);
            assert_eq!(
                recv(peer.as_raw_fd(), &mut [0; 1], MsgFlags::MSG_DONTWAIT),
                Err(nix::errno::Errno::EAGAIN)
            );
        }
    }
}

fn rejected_terminal_refusals() -> Vec<(&'static str, weaver_types::LifecycleRefusal)> {
    use weaver_types::state_flow::{PersistenceStage, StateManagementCode, StateManagementStage};
    use weaver_types::{LifecycleRefusal as L, RefusingOrgan};
    vec![
        ("carried-work", L::CarriedWork),
        ("out-of-order", L::OutOfOrder),
        (
            "state-management",
            L::StateManagementInvalid {
                stage: StateManagementStage::Serving,
                code: StateManagementCode::InvalidSpan,
                rule_id: None,
                path: None,
                detail: "worker retry claim".into(),
            },
        ),
        (
            "publication-claim",
            L::StatePersistenceInvalid {
                stage: PersistenceStage::Publish,
                case: PersistenceCase::PublicationFailed,
                artifact_id: Some("a".repeat(64)),
                detail: "worker publication claim".into(),
            },
        ),
        ("not-at-rest", L::ActivityNotAtRest),
        ("unauthorized", L::Unauthorized),
        (
            "spu-refusal",
            L::OrganRefused {
                organ: RefusingOrgan::Spu,
                reason: Box::new(L::OutOfOrder),
            },
        ),
        ("unanswered", L::Unanswered),
        ("no-residency", L::NoResidency),
    ]
}

fn at_refusal_site<'a>(
    fixture: &Fixture,
    repo: &'a mut Repository,
    mode: &str,
    site: &str,
) -> (Capture<'a>, OwnedFd, ExchangeId) {
    if matches!(site, "commit" | "refused") {
        let (mut owner, peer, exchange, _) = through_prepared(fixture, repo, mode);
        if site == "refused" {
            refused_packet(&peer, &exchange, SavePointLeg::Published);
            pending(&mut owner);
        }
        return (owner, peer, exchange);
    }
    let (mut owner, peer, exchange) = through_beginning(fixture, repo, mode);
    if site == "image" {
        return (owner, peer, exchange);
    }
    if site != "cancelled" {
        fixture.write_room("capture-9.sqlite.part", CAPTURE_BYTES);
    }
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
    if site == "hashing" {
        return (owner, peer, exchange);
    }
    pending(&mut owner);
    if site == "cancelled" {
        pending(&mut owner);
        assert!(matches!(
            response(&peer),
            CapturePayload::SavePointAborted(_)
        ));
    } else {
        assert!(matches!(
            response(&peer),
            CapturePayload::SavePointImageChecked(_)
        ));
        if site == "preparing" {
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
        }
    }
    (owner, peer, exchange)
}

#[test]
fn capture_receives_reject_retry_codes_and_worker_publication_claims_at_every_site() {
    for mode in ["forced", "graceful", "demand"] {
        for site in [
            "image",
            "hashing",
            "finished",
            "preparing",
            "commit",
            "cancelled",
            "refused",
        ] {
            if mode == "demand" && matches!(site, "cancelled" | "refused") {
                continue; // Demand's separate Abort outcome is covered in its existing cases.
            }
            for (case, refusal) in rejected_terminal_refusals() {
                let fixture = Fixture::new();
                let mut repo = fixture.repository();
                let (mut owner, peer, exchange) = at_refusal_site(&fixture, &mut repo, mode, site);
                let names = fixture.names().len();
                packet(
                    &peer,
                    &exchange,
                    CapturePayload::Refusal(CaptureRefusal::Lifecycle(refusal)),
                );
                assert_eq!(
                    owner.advance(),
                    LeaveProgress::Stopped,
                    "{mode}/{site}/{case}"
                );
                let failure = if site == "cancelled" {
                    owner.cleanup_failure()
                } else {
                    owner.failure()
                };
                assert!(
                    matches!(failure, Some(FailureCause::Protocol(_))),
                    "{mode}/{site}/{case}: {failure:?}"
                );
                assert!(owner.finish_leave().is_err());
                assert_eq!(fixture.index().len(), 1);
                assert_eq!(
                    fixture.names().len(),
                    if matches!(site, "finished" | "preparing") {
                        2
                    } else {
                        names
                    },
                    "{mode}/{site}/{case}: unconfirmed parts are removed, guards retained"
                );
                assert_eq!(
                    recv(peer.as_raw_fd(), &mut [0; 1], MsgFlags::MSG_DONTWAIT),
                    Err(nix::errno::Errno::EAGAIN),
                    "{mode}/{site}/{case}: no continuation"
                );
            }
        }
    }
}

#[test]
fn capture_receives_keep_legitimate_terminal_codes_at_each_receive_and_checkpoint() {
    use weaver_types::{LifecycleRefusal as L, RefusingOrgan};
    for site in [
        "image",
        "hashing",
        "finished",
        "preparing",
        "commit",
        "cancelled",
        "refused",
    ] {
        for refusal in [
            L::Malformed,
            L::DescriptorsUnusable,
            L::OrganRefused {
                organ: RefusingOrgan::Gate,
                reason: Box::new(L::StatePersistenceInvalid {
                    stage: weaver_types::state_flow::PersistenceStage::Publish,
                    case: PersistenceCase::PublicationFailed,
                    artifact_id: Some("b".repeat(64)),
                    detail: "gate-attributed claim".into(),
                }),
            },
        ] {
            let fixture = Fixture::new();
            let mut repo = fixture.repository();
            let (mut owner, peer, exchange) = at_refusal_site(&fixture, &mut repo, "forced", site);
            packet(
                &peer,
                &exchange,
                CapturePayload::Refusal(CaptureRefusal::Lifecycle(refusal)),
            );
            assert_eq!(owner.advance(), LeaveProgress::Stopped);
            let failure = if site == "cancelled" {
                owner.cleanup_failure()
            } else {
                owner.failure()
            };
            assert!(
                matches!(failure, Some(FailureCause::WorkerOutcome)),
                "{site}: {failure:?}"
            );
            assert_eq!(fixture.index().len(), 1);
            assert_eq!(
                recv(peer.as_raw_fd(), &mut [0; 1], MsgFlags::MSG_DONTWAIT),
                Err(nix::errno::Errno::EAGAIN)
            );
        }
    }
}

#[test]
fn begun_capture_requires_failed_run_identity_at_receive_and_preparation_checkpoints() {
    for mode in ["forced", "graceful"] {
        for phase in ["image", "hashing", "commit", "refused"] {
            let fixture = Fixture::new();
            let mut repo = fixture.repository();
            let (mut owner, peer, exchange) = if matches!(phase, "commit" | "refused") {
                let (owner, peer, exchange, _) = through_prepared(&fixture, &mut repo, mode);
                (owner, peer, exchange)
            } else {
                through_beginning(&fixture, &mut repo, mode)
            };
            if phase == "hashing" {
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
            } else if phase == "refused" {
                refused_packet(&peer, &exchange, SavePointLeg::Published);
                pending(&mut owner);
            }
            let names = fixture.names().len();
            packet(
                &peer,
                &exchange,
                CapturePayload::Refusal(CaptureRefusal::Save(SaveRefusal::FailedRun {
                    run: run(),
                    capture_id: None,
                    phase: PersistencePhase::Preparing,
                    published: None,
                    artifacts: Vec::new(),
                    detail: "missing capture identity".into(),
                })),
            );
            assert_eq!(owner.advance(), LeaveProgress::Stopped);
            assert!(
                matches!(owner.failure(), Some(FailureCause::Protocol(_))),
                "{mode}/{phase}: {:?}",
                owner.failure()
            );
            assert_eq!(fixture.index().len(), 1);
            assert_eq!(fixture.names().len(), names);
            assert_eq!(
                recv(peer.as_raw_fd(), &mut [0; 1], MsgFlags::MSG_DONTWAIT),
                Err(nix::errno::Errno::EAGAIN)
            );
        }
    }
}

#[test]
fn refused_leave_admin_rejects_publication_restart_duplicate_ack_and_wrong_final_shape() {
    for case in [
        "duplicate",
        "complete",
        "commit",
        "report",
        "no-state",
        "unforced",
        "wrong-leg",
        "forced-refusal",
    ] {
        let fixture = Fixture::new();
        let mut repo = fixture.repository();
        let (mut owner, peer, exchange, _) = through_prepared(
            &fixture,
            &mut repo,
            if case == "forced-refusal" {
                "forced"
            } else {
                "graceful"
            },
        );
        refused_packet(&peer, &exchange, SavePointLeg::Published);
        pending(&mut owner);
        let payload = match case {
            "duplicate" => CapturePayload::SavePointCaptureRefused(CaptureRefused {
                capture_id: 9.into(),
                missed: SavePointLeg::Published,
            }),
            "complete" => CapturePayload::SavePointCaptureComplete(CaptureIdentity {
                capture_id: 9.into(),
            }),
            "commit" => CapturePayload::SavePointCommit(CaptureIdentity {
                capture_id: 9.into(),
            }),
            "wrong-leg" | "forced-refusal" => {
                CapturePayload::Refusal(CaptureRefusal::Save(SaveRefusal::SavePointNotTaken {
                    missed: if case == "wrong-leg" {
                        SavePointLeg::Finished
                    } else {
                        SavePointLeg::Published
                    },
                }))
            }
            _ => {
                let saved = (case == "report").then(|| {
                    let loaded = fixture
                        .repository()
                        .load(None, &fixture.scope())
                        .unwrap()
                        .identity();
                    report(&loaded, 9)
                });
                CapturePayload::Answer(CaptureAnswer::Left {
                    save_point: saved,
                    forced: case != "unforced",
                    no_state: case == "no-state",
                })
            }
        };
        packet(&peer, &exchange, payload);
        assert_eq!(owner.advance(), LeaveProgress::Stopped, "{case}");
        assert!(
            matches!(owner.failure(), Some(FailureCause::Protocol(_))),
            "{case}: {:?}",
            owner.failure()
        );
        assert_eq!(fixture.index().len(), 1);
    }
}

#[test]
fn refused_leave_admin_accepts_the_acknowledgement_at_a_hashing_checkpoint() {
    let fixture = Fixture::new();
    let mut repo = fixture.repository();
    let (mut owner, peer, exchange) = through_beginning(&fixture, &mut repo, "forced");
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
    refused_packet(&peer, &exchange, SavePointLeg::Answer);
    pending(&mut owner);
    assert_eq!(fixture.names().len(), 2);
    let mut wire = [0; 128];
    assert_eq!(
        recv(peer.as_raw_fd(), &mut wire, MsgFlags::MSG_DONTWAIT),
        Err(nix::errno::Errno::EAGAIN),
        "no ImageChecked after CaptureRefused"
    );
    packet(
        &peer,
        &exchange,
        CapturePayload::Answer(CaptureAnswer::Left {
            save_point: None,
            forced: true,
            no_state: false,
        }),
    );
    assert_eq!(
        owner.advance(),
        LeaveProgress::LeftReceived,
        "{:?}",
        owner.failure()
    );
}

#[test]
fn refused_leave_admin_expired_capture_accepts_only_cleanup_and_final_unsaved_outcome() {
    for graceful in [false, true] {
        let fixture = Fixture::new();
        let mut repo = fixture.repository();
        let loaded = seed(&fixture, &mut repo);
        let observed = observed(&fixture, &repo, &loaded, "valid").unwrap();
        let (channel, peer) = pair();
        let begin = if graceful {
            Capture::graceful
        } else {
            Capture::begin
        };
        let mut owner = begin(
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
        let directive: weaver_types::OrganEnvelope = serde_json::from_slice(&read(&peer)).unwrap();
        let now = crate::serving_load::window::raw_ns().unwrap();
        let deadline = now + 250_000_000;
        let began = deadline - SAVE_NS;
        packet(
            &peer,
            &directive.exchange,
            CapturePayload::SavePointBeginning(CaptureBeginning {
                capture_id: 9.into(),
                window: weaver_types::state_flow::SaveWindow {
                    began_ns: began.into(),
                    deadline_ns: deadline.into(),
                },
                tail_window: Some(weaver_types::state_flow::SaveWindow {
                    began_ns: began.into(),
                    deadline_ns: (began + weaver_types::LEAVE_BOUND_MS * 1_000_000).into(),
                }),
            }),
        );
        pending(&mut owner);
        while let Some(remaining) =
            deadline.checked_sub(crate::serving_load::window::raw_ns().unwrap())
        {
            std::thread::sleep(Duration::from_nanos(remaining + 1));
        }
        // Whole cleanup/final frames may be queued after capture expiry,
        // but only the original Leave bounds can authorize their reduction.
        refused_packet(&peer, &directive.exchange, SavePointLeg::Answer);
        packet(
            &peer,
            &directive.exchange,
            if graceful {
                CapturePayload::Refusal(CaptureRefusal::Save(SaveRefusal::SavePointNotTaken {
                    missed: SavePointLeg::Answer,
                }))
            } else {
                CapturePayload::Answer(CaptureAnswer::Left {
                    save_point: None,
                    forced: true,
                    no_state: false,
                })
            },
        );
        pending(&mut owner); // Withdraw success and retain one Aborted.
        pending(&mut owner); // Carry that cancellation under the original tail.
        let CapturePayload::SavePointAborted(aborted) = response(&peer) else {
            panic!()
        };
        assert_eq!(aborted.capture_id, 9.into());
        assert_eq!(aborted.code, "capture_expired");
        pending(&mut owner); // Matching CaptureRefused.
        assert_eq!(
            owner.advance(),
            if graceful {
                LeaveProgress::Stopped
            } else {
                LeaveProgress::LeftReceived
            }
        );
        assert!(
            owner.finish_leave().is_err(),
            "an unsaved Leave cannot finish a publication"
        );
        let mut byte = [0];
        assert_eq!(
            recv(peer.as_raw_fd(), &mut byte, MsgFlags::MSG_DONTWAIT),
            Err(nix::errno::Errno::EAGAIN),
            "no second cancellation or late publication continuation"
        );
        assert_eq!(fixture.index().len(), 1);
        assert_eq!(fixture.names().len(), 2);
    }
}

#[test]
fn refused_leave_admin_preparation_error_sends_one_abort_then_requires_cleanup_and_left() {
    for outcome in ["clean", "worker-aborted", "crossed-commit"] {
        let fixture = Fixture::new();
        let mut repo = fixture.repository();
        let (mut owner, peer, exchange) = through_beginning(&fixture, &mut repo, "forced");
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
        pending(&mut owner); // The named room file is missing: actual preparation error.
        pending(&mut owner); // Carry the original Aborted, retaining the same wait.
        let CapturePayload::SavePointAborted(aborted) = response(&peer) else {
            panic!()
        };
        assert_eq!(aborted.capture_id, 9.into());
        assert_eq!(aborted.code, "publication_failed");
        assert!(owner.finish_leave().is_err());
        packet(
            &peer,
            &exchange,
            match outcome {
                "worker-aborted" => CapturePayload::SavePointAborted(aborted),
                "crossed-commit" => CapturePayload::SavePointCommit(CaptureIdentity {
                    capture_id: 9.into(),
                }),
                _ => CapturePayload::SavePointCaptureRefused(CaptureRefused {
                    capture_id: 9.into(),
                    missed: SavePointLeg::Answer,
                }),
            },
        );
        if outcome == "clean" {
            pending(&mut owner); // Worker's first attribution wins over admin's Published.
            packet(
                &peer,
                &exchange,
                CapturePayload::Answer(CaptureAnswer::Left {
                    save_point: None,
                    forced: true,
                    no_state: false,
                }),
            );
            assert_eq!(
                owner.advance(),
                LeaveProgress::LeftReceived,
                "{:?}",
                owner.failure()
            );
        } else {
            assert_eq!(owner.advance(), LeaveProgress::Stopped);
        }
        assert_eq!(fixture.index().len(), 1);
    }
}

#[test]
fn refused_leave_admin_cleanup_failure_cannot_restart_the_cancelled_exchange() {
    let fixture = Fixture::new();
    let mut repo = fixture.repository();
    let (mut owner, peer, exchange, prepared) = through_prepared(&fixture, &mut repo, "forced");
    let guard = fixture
        .root
        .join(format!(".{}.pending", prepared.artifact_id));
    fs::write(&guard, b"foreign publication guard").unwrap();
    refused_packet(&peer, &exchange, SavePointLeg::Published);
    assert_eq!(owner.advance(), LeaveProgress::Stopped);
    assert!(matches!(
        owner.failure(),
        Some(FailureCause::Publication(_))
    ));
    assert_eq!(owner.advance(), LeaveProgress::Stopped);
    assert_eq!(fs::read(&guard).unwrap(), b"foreign publication guard");
    assert_eq!(fixture.index().len(), 1);
    let mut bytes = [0; 1024];
    assert_eq!(
        recv(peer.as_raw_fd(), &mut bytes, MsgFlags::MSG_DONTWAIT),
        Err(nix::errno::Errno::EAGAIN),
        "cleanup failure after CaptureRefused must not send another Aborted"
    );
}

#[test]
fn refused_capture_acknowledgement_cannot_replace_a_demand_outcome() {
    let fixture = Fixture::new();
    let mut repo = fixture.repository();
    let (mut owner, peer, exchange) = through_beginning(&fixture, &mut repo, "demand");
    refused_packet(&peer, &exchange, SavePointLeg::Answer);
    assert_eq!(owner.advance(), LeaveProgress::Stopped);
    assert!(matches!(owner.failure(), Some(FailureCause::Protocol(_))));
}

#[test]
fn demand_admin_requires_the_indexed_report_and_no_leave_completion() {
    for case in [
        "demand",
        "demand-complete",
        "demand-left",
        "demand-position",
        "demand-run",
        "demand-capture",
    ] {
        let fixture = Fixture::new();
        let mut repo = fixture.repository();
        let (mut owner, peer, exchange, prepared) = through_prepared(&fixture, &mut repo, case);
        assert!(
            owner.finish_demand().is_err(),
            "Prepared is not a saved outcome"
        );
        packet(
            &peer,
            &exchange,
            CapturePayload::SavePointCommit(CaptureIdentity {
                capture_id: 9.into(),
            }),
        );
        for _ in 0..4 {
            pending(&mut owner);
        }
        assert_eq!(
            response(&peer),
            CapturePayload::SavePointPublished(prepared)
        );
        let entry = fixture.index().last().unwrap().clone();
        assert_eq!(entry.cause, PersistenceCause::Requested);
        let identity = fixture
            .repository()
            .verify_unindexed_publication(entry, &fixture.scope())
            .unwrap()
            .identity();
        fixture.assert_guarded(&identity, PublicationState::Pending);
        let mut value = report(&identity, 9);
        match case {
            "demand-position" => value.start.position = None,
            "demand-run" => value.event_run = RunId("other".into()),
            "demand-capture" => value.capture_id = 10.into(),
            _ => (),
        }
        let payload = match case {
            "demand-complete" => CapturePayload::SavePointCaptureComplete(CaptureIdentity {
                capture_id: 9.into(),
            }),
            "demand-left" => CapturePayload::Answer(CaptureAnswer::Left {
                save_point: Some(value),
                forced: false,
                no_state: false,
            }),
            _ => CapturePayload::Answer(CaptureAnswer::SavePointTaken { report: value }),
        };
        packet(&peer, &exchange, payload);
        if case == "demand" {
            assert_eq!(owner.advance(), LeaveProgress::TakenReceived);
            assert_eq!(owner.finish_demand().unwrap(), identity);
            assert_eq!(
                fixture
                    .repository()
                    .verify_indexed(
                        &ArtifactId::parse(&identity.artifact_id).unwrap(),
                        &fixture.scope()
                    )
                    .unwrap()
                    .identity(),
                identity
            );
            assert!(
                !fixture
                    .names()
                    .iter()
                    .any(|name| name.ends_with(".pending"))
            );
        } else {
            assert_eq!(
                owner.advance(),
                LeaveProgress::Stopped,
                "{case}: {:?}",
                owner.failure()
            );
            assert!(owner.finish_demand().is_err());
            fixture.assert_guarded(&identity, PublicationState::Broken);
        }
    }
}

#[test]
fn demand_admin_accepts_a_queued_capture_not_ready_without_publication() {
    let fixture = Fixture::new();
    let mut repo = fixture.repository();
    let loaded = seed(&fixture, &mut repo);
    let observed = observed(&fixture, &repo, &loaded, "valid").unwrap();
    let (channel, peer) = pair();
    let mut owner = Capture::demand(
        observed,
        &mut repo,
        channel,
        open_directory(&fixture.room).unwrap(),
        fixture.owner.uid,
        weaver_types::Cause { uid: 73 },
    )
    .unwrap();
    pending(&mut owner);
    let request = CaptureEnvelope::decode(&read(&peer), CaptureSender::Admin).unwrap();
    packet(
        &peer,
        &request.exchange,
        CapturePayload::Refusal(CaptureRefusal::Save(SaveRefusal::SavePointNotTaken {
            missed: SavePointLeg::CaptureNotReady,
        })),
    );
    assert_eq!(owner.advance(), LeaveProgress::Stopped);
    assert!(matches!(owner.failure(), Some(FailureCause::WorkerOutcome)));
    assert_eq!(fixture.index().len(), 1);
    assert_eq!(fixture.names().len(), 2);
}

#[test]
fn demand_admin_local_copy_failure_aborts_before_returning_and_keeps_first_cause() {
    for foreign in [false, true] {
        let fixture = Fixture::new();
        let mut repo = fixture.repository();
        let loaded = seed(&fixture, &mut repo);
        let observed = observed(&fixture, &repo, &loaded, "valid").unwrap();
        let (channel, peer) = pair();
        let mut owner = Capture::demand(
            observed,
            &mut repo,
            channel,
            open_directory(&fixture.room).unwrap(),
            fixture.owner.uid,
            weaver_types::Cause { uid: 73 },
        )
        .unwrap();
        pending(&mut owner);
        let request = CaptureEnvelope::decode(&read(&peer), CaptureSender::Admin).unwrap();
        let CapturePayload::Directive(CaptureDirective::SavePoint { window, .. }) = request.payload
        else {
            panic!("demand opener")
        };
        packet(
            &peer,
            &request.exchange,
            CapturePayload::SavePointBeginning(CaptureBeginning {
                capture_id: 9.into(),
                window,
                tail_window: None,
            }),
        );
        pending(&mut owner);
        fixture.write_room("capture-9.sqlite.part", CAPTURE_BYTES);
        packet(
            &peer,
            &request.exchange,
            CapturePayload::SavePointImage(CaptureImage {
                capture_id: 9.into(),
                room_name: "capture-9.sqlite.part".into(),
                size_bytes: (CAPTURE_BYTES.len() as u64 + 1).into(),
                position: None,
            }),
        );
        pending(&mut owner);
        pending(&mut owner);
        assert!(matches!(
            owner.failure(),
            Some(FailureCause::Publication(_))
        ));
        let first = format!("{:?}", owner.failure());
        pending(&mut owner);
        assert_eq!(
            response(&peer),
            CapturePayload::SavePointAborted(CaptureAborted {
                capture_id: 9.into(),
                code: "publication_failed".into(),
                detail: "admin could not prepare the capture for publication".into()
            })
        );
        assert_eq!(fixture.index().len(), 1);
        assert_eq!(
            fixture.names().len(),
            2,
            "unlisted root part removed after local copy failure"
        );
        let mut exchange = request.exchange;
        if foreign {
            exchange.ordinal += 1;
        }
        packet(
            &peer,
            &exchange,
            CapturePayload::Refusal(CaptureRefusal::Save(SaveRefusal::SavePointNotTaken {
                missed: SavePointLeg::Published,
            })),
        );
        assert_eq!(owner.advance(), LeaveProgress::Stopped);
        assert_eq!(format!("{:?}", owner.failure()), first);
        assert_eq!(owner.cleanup_failure().is_some(), foreign);
        assert!(owner.finish_demand().is_err());
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
        if matches!(case, "valid" | "graceful" | "graceful-converted") {
            assert_eq!(owner.finish_leave().unwrap(), published);
            assert_eq!(
                fixture.index().last().unwrap().cause,
                if case == "graceful" {
                    PersistenceCause::GracefulUnload
                } else {
                    PersistenceCause::ForcedUnload
                },
                "the validated Left, not the initiating request, settles the save cause: {case}"
            );
            drop(owner);
            assert_eq!(
                repo.load(None, &fixture.scope()).unwrap().identity(),
                published
            );
        } else {
            assert!(owner.finish_leave().is_err());
        }
    }
}

#[test]
fn forced_leave_admin_peer_loss_stops_each_publication_step_and_keeps_guard() {
    if crate::test_isolation::isolated!(
        forced_leave_admin_peer_loss_stops_each_publication_step_and_keeps_guard
    ) {
        return;
    }
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
    let mut owner = Capture::begin(
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
        let unacknowledged = matches!(
            &payload,
            CapturePayload::Refusal(CaptureRefusal::Save(SaveRefusal::SavePointNotTaken { .. }))
        );
        let foreign = matches!(&payload, CapturePayload::Refusal(CaptureRefusal::Save(
            SaveRefusal::FailedRun { run, .. }
        )) if run.0 == "foreign");
        packet(&peer, &exchange, payload);
        assert_eq!(owner.advance(), LeaveProgress::Stopped);
        if foreign || unacknowledged {
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

#[path = "leave_expiry.rs"]
mod expiry;
