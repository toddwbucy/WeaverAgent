//! Real actor sockets/publication with controlled capture-clock observations.
use super::*;
use crate::serving_capture_window::with_test_times;

fn no_packet(peer: &OwnedFd) {
    let mut byte = [0];
    assert_eq!(
        recv(peer.as_raw_fd(), &mut byte, MsgFlags::MSG_DONTWAIT),
        Err(nix::errno::Errno::EAGAIN)
    );
}
fn expired_now() -> u64 {
    crate::serving_load::window::raw_ns().unwrap() + SAVE_NS + 1_000_000_000
}
fn abort(owner: &mut Capture<'_>, peer: &OwnedFd) {
    pending(owner);
    pending(owner);
    let CapturePayload::SavePointAborted(value) = response(peer) else {
        panic!()
    };
    assert_eq!(value.capture_id, 9.into());
    assert_eq!(value.code, "capture_expired");
    no_packet(peer);
}
fn unsaved_left() -> CapturePayload {
    CapturePayload::Answer(CaptureAnswer::Left {
        save_point: None,
        forced: true,
        no_state: false,
    })
}

#[test]
fn expired_leave_capture_cleans_each_precommit_phase_before_its_final_outcome() {
    for mode in ["forced", "graceful"] {
        for site in ["image", "hashing", "finished", "preparing", "commit"] {
            let fixture = Fixture::new();
            let mut repo = fixture.repository();
            let (mut owner, peer, exchange) = at_refusal_site(&fixture, &mut repo, mode, site);
            with_test_times(&[expired_now()], || {
                abort(&mut owner, &peer);
                assert!(owner.finish_leave().is_err());
                let missed = if site == "commit" {
                    SavePointLeg::Published
                } else {
                    SavePointLeg::Answer
                };
                refused_packet(&peer, &exchange, missed);
                pending(&mut owner);
                packet(
                    &peer,
                    &exchange,
                    if mode == "graceful" {
                        CapturePayload::Refusal(CaptureRefusal::Save(
                            SaveRefusal::SavePointNotTaken { missed },
                        ))
                    } else {
                        unsaved_left()
                    },
                );
                assert_eq!(
                    owner.advance(),
                    if mode == "graceful" {
                        LeaveProgress::Stopped
                    } else {
                        LeaveProgress::LeftReceived
                    },
                    "{mode}/{site}: {:?}",
                    owner.failure()
                );
                assert!(owner.finish_leave().is_err());
                no_packet(&peer);
                assert_eq!(fixture.index().len(), 1, "{mode}/{site}");
                assert_eq!(fixture.names().len(), 2, "{mode}/{site}");
            });
        }
    }
}

#[test]
fn expired_leave_discards_only_the_one_matching_crossed_image_or_finished() {
    for site in ["image", "finished"] {
        for duplicate in [false, true] {
            let fixture = Fixture::new();
            let mut repo = fixture.repository();
            let (mut owner, peer, exchange) = at_refusal_site(&fixture, &mut repo, "forced", site);
            with_test_times(&[expired_now()], || {
                abort(&mut owner, &peer);
                let crossed = if site == "image" {
                    CapturePayload::SavePointImage(CaptureImage {
                        capture_id: 9.into(),
                        room_name: "capture-9.sqlite.part".into(),
                        size_bytes: (CAPTURE_BYTES.len() as u64).into(),
                        position: Some(position(9)),
                    })
                } else {
                    CapturePayload::SavePointImageFinished(CaptureImageFinished {
                        capture_id: 9.into(),
                        room_name: "capture-9.sqlite".into(),
                    })
                };
                packet(&peer, &exchange, crossed.clone());
                pending(&mut owner);
                no_packet(&peer); // No late ImageChecked or Prepared.
                if duplicate {
                    packet(&peer, &exchange, crossed);
                    assert_eq!(owner.advance(), LeaveProgress::Stopped);
                    assert!(matches!(owner.failure(), Some(FailureCause::Protocol(_))));
                } else {
                    refused_packet(&peer, &exchange, SavePointLeg::Finished);
                    pending(&mut owner);
                    packet(&peer, &exchange, unsaved_left());
                    assert_eq!(owner.advance(), LeaveProgress::LeftReceived);
                }
                no_packet(&peer);
                assert_eq!(fixture.index().len(), 1);
            });
        }
    }
}

#[test]
fn expired_leave_rejects_crossed_commit_success_and_foreign_cleanup() {
    for outcome in [
        "commit",
        "complete",
        "left-before-cleanup",
        "foreign-cleanup",
    ] {
        let fixture = Fixture::new();
        let mut repo = fixture.repository();
        let (mut owner, peer, exchange, _) = through_prepared(&fixture, &mut repo, "forced");
        with_test_times(&[expired_now()], || {
            abort(&mut owner, &peer);
            packet(
                &peer,
                &exchange,
                match outcome {
                    "commit" => CapturePayload::SavePointCommit(CaptureIdentity {
                        capture_id: 9.into(),
                    }),
                    "complete" => CapturePayload::SavePointCaptureComplete(CaptureIdentity {
                        capture_id: 9.into(),
                    }),
                    "left-before-cleanup" => unsaved_left(),
                    _ => CapturePayload::SavePointCaptureRefused(CaptureRefused {
                        capture_id: 10.into(),
                        missed: SavePointLeg::Published,
                    }),
                },
            );
            assert_eq!(owner.advance(), LeaveProgress::Stopped, "{outcome}");
            assert!(
                matches!(owner.failure(), Some(FailureCause::Protocol(_))),
                "{outcome}: {:?}",
                owner.failure()
            );
            assert!(owner.finish_leave().is_err());
            assert_eq!(fixture.index().len(), 1);
            no_packet(&peer);
        });
    }
}

#[test]
fn commit_received_across_capture_expiry_is_retained_as_a_failed_crossing() {
    let fixture = Fixture::new();
    let mut repo = fixture.repository();
    let (mut owner, peer, exchange, _) = through_prepared(&fixture, &mut repo, "forced");
    packet(
        &peer,
        &exchange,
        CapturePayload::SavePointCommit(CaptureIdentity {
            capture_id: 9.into(),
        }),
    );
    let now = crate::serving_load::window::raw_ns().unwrap();
    // Step check, poll allowance, post-poll check, then post-decode expiry.
    with_test_times(&[now, now, now, expired_now()], || {
        assert_eq!(owner.advance(), LeaveProgress::Stopped);
        assert!(
            matches!(owner.failure(), Some(FailureCause::Protocol(_))),
            "{:?}",
            owner.failure()
        );
        assert!(owner.finish_leave().is_err());
        no_packet(&peer); // Never Published or a cancellation of consumed Commit.
        assert_eq!(fixture.index().len(), 1);
    });
}

#[test]
fn demand_and_postcommit_expiry_do_not_enter_refusal_only_continuation() {
    for mode in ["demand", "forced", "graceful"] {
        let fixture = Fixture::new();
        let mut repo = fixture.repository();
        let (mut owner, peer, exchange, _) = through_prepared(&fixture, &mut repo, mode);
        if mode != "demand" {
            packet(
                &peer,
                &exchange,
                CapturePayload::SavePointCommit(CaptureIdentity {
                    capture_id: 9.into(),
                }),
            );
            pending(&mut owner);
        }
        with_test_times(&[expired_now()], || {
            assert_eq!(owner.advance(), LeaveProgress::Stopped, "{mode}");
            assert!(
                matches!(
                    owner.failure(),
                    Some(FailureCause::Channel(ChannelFault::Unanswered))
                ),
                "{mode}: {:?}",
                owner.failure()
            );
            no_packet(&peer);
            assert_eq!(fixture.index().len(), 1);
        });
    }
}

#[test]
fn local_preparation_refusal_accepts_delayed_cleanup_under_the_original_leave_tail() {
    for mode in ["forced", "graceful"] {
        let fixture = Fixture::new();
        let mut repo = fixture.repository();
        // The absent room image causes a real preparation failure. Its sole
        // publication_failed Aborted has already been carried by this helper.
        let (mut owner, peer, exchange) = at_refusal_site(&fixture, &mut repo, mode, "cancelled");
        with_test_times(&[expired_now()], || {
            refused_packet(&peer, &exchange, SavePointLeg::Published);
            pending(&mut owner);
            packet(
                &peer,
                &exchange,
                if mode == "forced" {
                    unsaved_left()
                } else {
                    CapturePayload::Refusal(CaptureRefusal::Save(SaveRefusal::SavePointNotTaken {
                        missed: SavePointLeg::Published,
                    }))
                },
            );
            assert_eq!(
                owner.advance(),
                if mode == "forced" {
                    LeaveProgress::LeftReceived
                } else {
                    LeaveProgress::Stopped
                }
            );
            assert!(owner.finish_leave().is_err());
            no_packet(&peer);
            assert_eq!(fixture.index().len(), 1);
            assert_eq!(fixture.names().len(), 2);
        });
    }
}

#[test]
fn cleanup_acknowledgement_received_across_expiry_retains_its_wire_and_finishes_unsaved() {
    let fixture = Fixture::new();
    let mut repo = fixture.repository();
    let (mut owner, peer, exchange) = through_beginning(&fixture, &mut repo, "forced");
    refused_packet(&peer, &exchange, SavePointLeg::Answer);
    let now = crate::serving_load::window::raw_ns().unwrap();
    with_test_times(&[now, now, now, expired_now()], || {
        pending(&mut owner);
        no_packet(&peer); // Cleanup is already proven; no redundant Aborted.
        packet(&peer, &exchange, unsaved_left());
        assert_eq!(owner.advance(), LeaveProgress::LeftReceived);
        assert!(owner.finish_leave().is_err());
        assert_eq!(fixture.index().len(), 1);
    });
}
