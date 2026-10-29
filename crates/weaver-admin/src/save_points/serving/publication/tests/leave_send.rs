//! Peer closure after publication's final probe, at the actual packet send.
use super::*;
use crate::channel::{TestCaptureSend, with_before_capture_send};
use crate::serving_capture_window::with_test_times;
use std::cell::Cell;
use std::rc::Rc;

fn final_refusal(missed: SavePointLeg) -> CapturePayload {
    CapturePayload::Refusal(CaptureRefusal::Save(SaveRefusal::SavePointNotTaken {
        missed,
    }))
}

fn terminal(owner: &mut Capture<'_>) -> LeaveProgress {
    for _ in 0..8 {
        let progress = owner.advance();
        if progress != LeaveProgress::Pending {
            return progress;
        }
    }
    panic!("queued closed-peer outcome did not settle");
}

fn send_sites() -> [(&'static str, TestCaptureSend, SavePointLeg); 2] {
    [
        (
            "hashing",
            TestCaptureSend::ImageChecked,
            SavePointLeg::Answer,
        ),
        (
            "preparing",
            TestCaptureSend::Prepared,
            SavePointLeg::Finished,
        ),
    ]
}

#[test]
fn precommit_send_peer_closure_drains_exact_refusal_under_original_leave_windows() {
    for mode in ["forced", "graceful"] {
        for (site, send_kind, missed) in send_sites() {
            for expire_before_read in [false, true] {
                let fixture = Fixture::new();
                let mut repo = fixture.repository();
                let (mut owner, peer, exchange) = at_refusal_site(&fixture, &mut repo, mode, site);
                let original_index = fixture.index_bytes();
                let called = Rc::new(Cell::new(false));
                let witnessed = Rc::clone(&called);
                let room_file = fixture.room.join(if site == "hashing" {
                    "capture-9.sqlite.part"
                } else {
                    "capture-9.sqlite"
                });
                with_before_capture_send(
                    send_kind,
                    move || {
                        // The publication checkpoints have returned. The peer now
                        // completes its cleanup, sends both original packets and
                        // closes before the real send syscall observes EPIPE.
                        fs::remove_file(room_file).unwrap();
                        refused_packet(&peer, &exchange, missed);
                        packet(
                            &peer,
                            &exchange,
                            if mode == "graceful" {
                                final_refusal(missed)
                            } else {
                                CapturePayload::Answer(CaptureAnswer::Left {
                                    save_point: None,
                                    forced: true,
                                    no_state: false,
                                })
                            },
                        );
                        drop(peer);
                        witnessed.set(true);
                    },
                    || pending(&mut owner),
                );
                assert!(
                    called.get(),
                    "{site}: must reach the actual send after all probes"
                );
                let expected = if mode == "graceful" {
                    LeaveProgress::Stopped
                } else {
                    LeaveProgress::LeftReceived
                };
                let mut settle = || {
                    assert_eq!(terminal(&mut owner), expected, "{site}/{mode}");
                    if mode == "graceful" {
                        assert!(matches!(owner.failure(), Some(FailureCause::WorkerOutcome)));
                    } else {
                        assert!(owner.failure().is_none());
                    }
                    assert!(owner.cleanup_failure().is_none());
                    assert!(owner.finish_leave().is_err(), "no completed save");
                    assert_eq!(fixture.index_bytes(), original_index);
                    assert_eq!(fixture.names().len(), 2, "only the seed and index remain");
                };
                if expire_before_read {
                    let expired =
                        crate::serving_load::window::raw_ns().unwrap() + SAVE_NS + 1_000_000_000;
                    with_test_times(&[expired], settle);
                } else {
                    settle();
                }
            }
        }
    }
}

#[test]
fn precommit_send_peer_closure_needs_correlated_cleanup_and_final_refusal() {
    for (site, send_kind, missed) in send_sites() {
        for case in [
            "missing",
            "foreign-ack",
            "foreign-final",
            "mismatched-final",
            "final-only",
            "commit",
        ] {
            let fixture = Fixture::new();
            let mut repo = fixture.repository();
            let (mut owner, peer, exchange) =
                at_refusal_site(&fixture, &mut repo, "graceful", site);
            let original_index = fixture.index_bytes();
            let called = Rc::new(Cell::new(false));
            let witnessed = Rc::clone(&called);
            with_before_capture_send(
                send_kind,
                move || {
                    if case == "commit" {
                        packet(
                            &peer,
                            &exchange,
                            CapturePayload::SavePointCommit(CaptureIdentity {
                                capture_id: 9.into(),
                            }),
                        );
                    } else if case != "missing" {
                        if case != "final-only" {
                            packet(
                                &peer,
                                &exchange,
                                CapturePayload::SavePointCaptureRefused(CaptureRefused {
                                    capture_id: if case == "foreign-ack" {
                                        10.into()
                                    } else {
                                        9.into()
                                    },
                                    missed,
                                }),
                            );
                        }
                        let mut final_exchange = exchange.clone();
                        if case == "foreign-final" {
                            final_exchange.ordinal += 1;
                        }
                        packet(
                            &peer,
                            &final_exchange,
                            final_refusal(if case == "mismatched-final" {
                                SavePointLeg::Published
                            } else {
                                missed
                            }),
                        );
                    }
                    drop(peer);
                    witnessed.set(true);
                },
                || pending(&mut owner),
            );
            assert!(called.get(), "{site}/{case}");
            assert_eq!(terminal(&mut owner), LeaveProgress::Stopped);
            assert!(
                matches!(
                    owner.failure(),
                    Some(FailureCause::Channel(_) | FailureCause::Protocol(_))
                ),
                "{site}/{case}: {:?}",
                owner.failure()
            );
            assert!(owner.finish_leave().is_err());
            assert_eq!(fixture.index_bytes(), original_index);
        }
    }
}

#[test]
fn closed_send_does_not_hide_a_later_clock_failure_behind_buffered_refusal() {
    for (site, send_kind, missed) in send_sites() {
        let fixture = Fixture::new();
        let mut repo = fixture.repository();
        let (mut owner, peer, exchange) = at_refusal_site(&fixture, &mut repo, "graceful", site);
        let original_index = fixture.index_bytes();
        with_before_capture_send(
            send_kind,
            move || {
                refused_packet(&peer, &exchange, missed);
                packet(&peer, &exchange, final_refusal(missed));
                drop(peer);
            },
            || pending(&mut owner),
        );
        with_test_times(&[0], || {
            assert_eq!(owner.advance(), LeaveProgress::Stopped);
        });
        assert!(matches!(
            owner.failure(),
            Some(FailureCause::Channel(ChannelFault::Closed))
        ));
        assert!(owner.finish_leave().is_err());
        assert_eq!(fixture.index_bytes(), original_index);
    }
}

#[test]
fn closed_send_postdecode_clock_failure_cannot_be_recovered_by_refusal() {
    for (site, send_kind, missed) in send_sites() {
        let fixture = Fixture::new();
        let mut repo = fixture.repository();
        let (mut owner, peer, exchange) = at_refusal_site(&fixture, &mut repo, "graceful", site);
        let original_index = fixture.index_bytes();
        with_before_capture_send(
            send_kind,
            move || {
                refused_packet(&peer, &exchange, missed);
                packet(&peer, &exchange, final_refusal(missed));
                drop(peer);
            },
            || pending(&mut owner),
        );
        let now = crate::serving_load::window::raw_ns().unwrap();
        // Step, pre-poll and post-poll are timely. The complete decoded
        // acknowledgement then lacks a valid clock witness. Recovering the
        // clock cannot authorize that retained packet or the queued final.
        with_test_times(&[now, now, now, now - 1, now], || {
            assert_eq!(owner.advance(), LeaveProgress::Stopped);
        });
        assert!(matches!(
            owner.failure(),
            Some(FailureCause::Channel(ChannelFault::Closed))
        ));
        assert!(owner.finish_leave().is_err());
        assert_eq!(fixture.index_bytes(), original_index);
    }
}

#[test]
fn publication_checkpoint_postdecode_clock_failure_cannot_be_recovered_by_refusal() {
    for (site, _, missed) in send_sites() {
        let fixture = Fixture::new();
        let mut repo = fixture.repository();
        let (mut owner, peer, exchange) = at_refusal_site(&fixture, &mut repo, "graceful", site);
        let original_index = fixture.index_bytes();
        refused_packet(&peer, &exchange, missed);
        packet(&peer, &exchange, final_refusal(missed));
        let now = crate::serving_load::window::raw_ns().unwrap();
        // The checkpoint uses the sole nonblocking reader: step and its
        // pre-read check are timely, while the post-decode reading reverses.
        with_test_times(&[now, now, now - 1, now], || {
            assert_eq!(owner.advance(), LeaveProgress::Stopped);
        });
        assert!(matches!(
            owner.failure(),
            Some(FailureCause::Channel(ChannelFault::Closed))
        ));
        assert!(owner.finish_leave().is_err());
        assert_eq!(fixture.index_bytes(), original_index);
    }
}

#[test]
fn demand_send_peer_closure_cannot_enter_the_leave_refusal_continuation() {
    for (site, send_kind, missed) in send_sites() {
        let fixture = Fixture::new();
        let mut repo = fixture.repository();
        let (mut owner, peer, exchange) = at_refusal_site(&fixture, &mut repo, "demand", site);
        with_before_capture_send(
            send_kind,
            move || {
                refused_packet(&peer, &exchange, missed);
                packet(&peer, &exchange, final_refusal(missed));
                drop(peer);
            },
            || assert_eq!(owner.advance(), LeaveProgress::Stopped),
        );
        assert!(matches!(
            owner.failure(),
            Some(FailureCause::Channel(ChannelFault::Closed))
        ));
        assert!(owner.finish_demand().is_err());
        assert_eq!(fixture.index().len(), 1);
    }
}

#[test]
fn published_send_peer_closure_keeps_postcommit_failure_and_guard() {
    let fixture = Fixture::new();
    let mut repo = fixture.repository();
    let (mut owner, peer, exchange, prepared) = through_prepared(&fixture, &mut repo, "graceful");
    packet(
        &peer,
        &exchange,
        CapturePayload::SavePointCommit(CaptureIdentity {
            capture_id: 9.into(),
        }),
    );
    pending(&mut owner);
    let called = Rc::new(Cell::new(false));
    let witnessed = Rc::clone(&called);
    with_before_capture_send(
        TestCaptureSend::Published,
        move || {
            refused_packet(&peer, &exchange, SavePointLeg::Published);
            packet(&peer, &exchange, final_refusal(SavePointLeg::Published));
            drop(peer);
            witnessed.set(true);
        },
        || assert_eq!(terminal(&mut owner), LeaveProgress::Stopped),
    );
    assert!(
        called.get(),
        "must reach Published after its final publication probe"
    );
    assert!(matches!(
        owner.failure(),
        Some(FailureCause::Channel(ChannelFault::Closed))
    ));
    assert!(owner.finish_leave().is_err());
    assert_eq!(
        fixture.index().len(),
        2,
        "the already indexed artifact stays guarded"
    );
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
}
