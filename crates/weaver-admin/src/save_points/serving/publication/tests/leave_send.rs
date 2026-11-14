//! Peer closure after publication's final probe, at the actual packet send.
//!
//! **Each fixture that closes the peer before the send runs in its own
//! process** (#523). Its design is the closure seen at admin's send. A fork
//! taken by a parallel test inherits the peer and holds it across the drop,
//! so the send succeeds and the closure comes later, as a reset at the read
//! ahead of the packets queued behind it. That order has its own cases now:
//! `precommit_send_to_a_held_peer_drains_exact_refusal_behind_its_reset` here,
//! and `channel::tests::a_reset_ahead_of_queued_packets_is_read_past_to_them`
//! for every reader.
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

/// A fork that holds every descriptor of this process and pauses until it is
/// killed. It never execs, so its close-on-exec descriptors stay open.
/// Dropping it kills and reaps it, so a panic between the fork and the kill
/// cannot leave it holding the isolated process's output pipes open, which
/// would hold the parent's read of them.
struct Holder(nix::unistd::Pid);

impl Holder {
    fn fork() -> Self {
        // SAFETY: the child calls only pause, which is async-signal-safe,
        // until the parent's SIGKILL ends it.
        match unsafe { nix::unistd::fork() }.unwrap() {
            nix::unistd::ForkResult::Child => loop {
                // SAFETY: pause takes no arguments and touches no memory.
                unsafe { nix::libc::pause() };
            },
            nix::unistd::ForkResult::Parent { child } => Holder(child),
        }
    }
}

impl Drop for Holder {
    fn drop(&mut self) {
        // SAFETY: the pid is this process's own unreaped child.
        if unsafe { nix::libc::kill(self.0.as_raw(), nix::libc::SIGKILL) } == 0 {
            let _ = nix::sys::wait::waitpid(self.0, None);
        }
    }
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
    if crate::test_isolation::isolated!(
        precommit_send_peer_closure_drains_exact_refusal_under_original_leave_windows
    ) {
        return;
    }
    drains_exact_refusal_under_original_leave_windows(false);
}

/// **A peer held across its drop is read to its queued refusal behind the
/// reset** (#523). A worker whose close follows admin's send leaves that
/// packet unread, and the fork makes that order certain: it holds the peer
/// across the drop, so admin's send succeeds and its packet stays unread. The
/// fork is then killed and reaped, which closes the peer with that packet
/// unread. Where admin reads before the window expires, the reset is read
/// ahead of both the queued refusal and the final answer. Where the window
/// has expired first, no read meets it. A
/// parallel test's fork gave the same order to the fixture above. Run in its
/// own process, since its fork holds every descriptor this process has.
/// Perturbation: read the reset as the
/// end in `channel::recv_packet`, and the forced leave reads `Stopped`
/// where `LeftReceived` is due, the graceful one `Channel(Closed)` where
/// the worker's outcome is due.
#[test]
fn precommit_send_to_a_held_peer_drains_exact_refusal_behind_its_reset() {
    if crate::test_isolation::isolated!(
        precommit_send_to_a_held_peer_drains_exact_refusal_behind_its_reset
    ) {
        return;
    }
    drains_exact_refusal_under_original_leave_windows(true);
}

/// The worker sends both original packets and closes before admin's send.
/// `held`: a fork holds the peer across that close until after the send.
fn drains_exact_refusal_under_original_leave_windows(held: bool) {
    for mode in ["forced", "graceful"] {
        for (site, send_kind, missed) in send_sites() {
            for expire_before_read in [false, true] {
                let fixture = Fixture::new();
                let mut repo = fixture.repository();
                let (mut owner, peer, exchange) = at_refusal_site(&fixture, &mut repo, mode, site);
                let original_index = fixture.index_bytes();
                let called = Rc::new(Cell::new(false));
                let witnessed = Rc::clone(&called);
                let holder = Rc::new(Cell::new(None));
                let holding = Rc::clone(&holder);
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
                        // closes before the real send syscall observes EPIPE,
                        // or, held, after it.
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
                        if held {
                            holding.set(Some(Holder::fork()));
                        }
                        drop(peer);
                        witnessed.set(true);
                    },
                    || pending(&mut owner),
                );
                // After admin's send, before its read: killing and reaping the
                // holder closes the peer with admin's packet unread.
                drop(holder.take());
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
                        assert!(
                            matches!(owner.failure(), Some(FailureCause::WorkerOutcome)),
                            "{site}/{mode}/expired={expire_before_read}: {:?}",
                            owner.failure()
                        );
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
    if crate::test_isolation::isolated!(
        precommit_send_peer_closure_needs_correlated_cleanup_and_final_refusal
    ) {
        return;
    }
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
    if crate::test_isolation::isolated!(
        closed_send_does_not_hide_a_later_clock_failure_behind_buffered_refusal
    ) {
        return;
    }
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
    if crate::test_isolation::isolated!(
        closed_send_postdecode_clock_failure_cannot_be_recovered_by_refusal
    ) {
        return;
    }
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
    if crate::test_isolation::isolated!(
        demand_send_peer_closure_cannot_enter_the_leave_refusal_continuation
    ) {
        return;
    }
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
    if crate::test_isolation::isolated!(
        published_send_peer_closure_keeps_postcommit_failure_and_guard
    ) {
        return;
    }
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
