//! Existing rollback owners followed by the closed ordinary shutdown boundary.
//! These scripted transports do not stand up the still-held lifecycle composer.
use super::*;
use crate::flow::context::tests::event;
use std::time::{Duration, Instant};

fn shutdown_event(kind: Kind) -> AuthoredEvent {
    let payload = match kind {
        Kind::Fault => Some(Payload::Fault(
            serde_json::value::to_raw_value(&weaver_types::FaultReport {
                case: weaver_types::FaultCase::OrganDeathObserved,
                account: serde_json::value::to_raw_value(
                    &serde_json::json!({"fixture":"release organ death"}),
                )
                .unwrap(),
            })
            .unwrap(),
        )),
        Kind::Refusal => Some(Payload::Refusal(
            serde_json::value::to_raw_value(&weaver_types::LifecycleRefusal::OutOfOrder).unwrap(),
        )),
        Kind::SessionClosed => None,
        Kind::Unload => return unload(),
        _ => panic!("not shutdown evidence"),
    };
    event(kind, payload)
}
fn finish_shutdown(run: &mut Fixture, stopped: bool) {
    let before = run.context.barrier.recorder().structure().len();
    let ledger = run.context.flow.ledger().facts().count();
    let mut sequences = vec![];
    for kind in [
        Kind::Fault,
        Kind::Refusal,
        Kind::SessionClosed,
        Kind::Unload,
    ] {
        sequences.push(
            run.context
                .author_and_wait(&mut ports(&mut run.clock), shutdown_event(kind))
                .unwrap(),
        );
        assert!(run.context.failure().is_none());
        assert_eq!(run.context.barrier.recorder().tap_stopped(), stopped);
        assert_eq!(run.context.flow.ledger().facts().count(), ledger);
        assert_eq!(run.context.budget.gauges(), (0, 0));
        assert_eq!(run.context.inbox.queued().count(), 0);
        assert!(run.context.flow.standing_state().is_none());
    }
    assert!(sequences.windows(2).all(|pair| pair[0].0 + 1 == pair[1].0));
    assert_eq!(run.context.barrier.recorder().structure().len(), before + 4);
    assert!(run.context.barrier.is_closed());
    run._reader.set_nonblocking(true).unwrap();
    let end = Instant::now() + Duration::from_secs(5);
    let mut bytes = [0; 65536];
    while run.context.barrier.recorder().boundary().queued != 0 {
        assert!(Instant::now() < end);
        match run._reader.read(&mut bytes) {
            Ok(0) => panic!("trace closed before drain"),
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => std::thread::yield_now(),
            Err(error) => panic!("trace drain: {error}"),
        }
    }
    run.context
        .close_after_drain(&mut ports(&mut run.clock))
        .unwrap();
    assert_eq!(run.context.phase(), RecordingPhase::Closed);
    assert_eq!(
        run.context
            .author_and_wait(&mut ports(&mut run.clock), shutdown_event(Kind::Fault)),
        Err(RunFailure::Closed)
    );
    assert!(run.context.reduce_flow(&mut run.clock).is_err());
    assert_eq!(run.context.barrier.recorder().structure().len(), before + 4);
}

#[test]
fn release_shutdown_follows_real_boot_unsent_sent_pressure_fit_and_expired_accounts() {
    for road in ["unsent", "sent", "pressure", "fit", "expired"] {
        let mut fx = OpenFixture::new();
        fx.begin();
        if road != "unsent" {
            assert!(matches!(fx.sent(), TokenDirective::Open { .. }));
        }
        if road == "fit" {
            let mut answer = fx.opened();
            answer.capacity = 8192;
            fx.answer(answer);
            let mut refused = false;
            for _ in 0..16 {
                if fx.advance().unwrap() == BootOpenProgress::Refused {
                    refused = true;
                    break;
                }
            }
            assert!(refused);
            assert_eq!(fx.records(Kind::StatePlacement).len(), 1);
            assert_eq!(fx.records(Kind::Refusal).len(), 1);
        }
        if road == "pressure" {
            fx.answer(fx.opened());
            fx.advance().unwrap();
            fx.boot.member.fill_trace();
            fx.advance().unwrap();
            assert!(fx.boot.member.run.context.pending_author().is_some());
        }
        if road == "expired" {
            let end = fx
                .boot
                .member
                .run
                .context
                .deadlines
                .get(PumpDeadline::Enter)
                .unwrap()
                .deadline_ns();
            fx.boot.member.run.clock.advance_to(end).unwrap();
        }
        fx.release(1000);
        if road == "sent" {
            fx.answer(fx.opened());
        }
        if road == "pressure" {
            let run = &mut fx.boot.member.run;
            let ticket = run.context.pending_author().unwrap().ticket;
            let before = run.context.barrier.recorder().structure().len();
            let first = shutdown_event(Kind::Fault);
            let second = shutdown_event(Kind::Refusal);
            let pointer = |event: &AuthoredEvent| match event.payload.as_ref().unwrap() {
                Payload::Fault(raw) | Payload::Refusal(raw) => raw.get().as_ptr() as usize,
                _ => unreachable!(),
            };
            let pointers = [pointer(&first), pointer(&second)];
            assert_ne!(pointers[0], pointers[1]);
            let author_turn = run.context.author_turn.clone();
            for (original, expected) in [first, second].into_iter().zip(pointers) {
                assert_eq!(
                    run.context
                        .author_and_wait(&mut ports(&mut run.clock), original),
                    Err(RunFailure::BootOpenOwned)
                );
                assert_eq!(pointer(run.context.held_author().unwrap()), expected);
                assert!(run.context.refused_author().is_none());
                assert_eq!(run.context.pending_author().unwrap().ticket, ticket);
                assert_eq!(run.context.author_turn, author_turn);
                assert_eq!(run.context.barrier.recorder().structure().len(), before);
                assert!(run.context.failure().is_none());
            }
            fx.drain();
        }
        for _ in 0..16 {
            fx.account();
            if fx.boot.member.run.context.boot_open_account_settled() {
                break;
            }
        }
        assert!(fx.boot.member.run.context.boot_open_account_settled());
        if road == "pressure" {
            let run = &mut fx.boot.member.run;
            let returned = run.context.take_held_author().unwrap();
            let before = run
                .context
                .barrier
                .recorder()
                .structure()
                .by_kind(Kind::Refusal)
                .count();
            run.context
                .author_and_wait(&mut ports(&mut run.clock), returned)
                .unwrap();
            assert_eq!(
                run.context
                    .barrier
                    .recorder()
                    .structure()
                    .by_kind(Kind::Refusal)
                    .count(),
                before + 1
            );
            assert!(run.context.take_held_author().is_none());
        }
        assert!(fx.boot.member.run.context.boot_journal.is_none());
        assert!(fx.records(Kind::StateMarkPosition).is_empty());
        assert_eq!(
            fx.records(Kind::StatePlacement).len(),
            usize::from(matches!(road, "sent" | "pressure" | "fit"))
        );
        if road == "unsent" {
            assert!(receive(fx.peer.as_fd()).is_none());
        }
        assert!(
            fx.boot
                .member
                .run
                .context
                .retire_boot_decode(fx.decode)
                .is_ok()
        );
        let run = &mut fx.boot.member.run;
        assert!(run.context.complete_wait(PumpDeadline::Enter).is_some());
        assert_eq!(
            run.context.retire_state(&mut run.clock).unwrap(),
            Advance::Progress
        );
        assert_eq!(
            run.context.retire_classifier(&mut run.clock).unwrap(),
            Advance::Progress
        );
        finish_shutdown(run, true);
    }
}

fn finish_summary_shutdown(fx: &mut SummaryFixture) {
    let (decode, _peer) = fx._decode.take().unwrap();
    assert!(fx.run().context.retire_boot_decode(decode).is_ok());
    let run = fx.run();
    // The sealed summary rollback retained and retired its exact state channel.
    assert!(run.context.flow.standing_state().is_none());
    assert!(run.context.deadlines.get(PumpDeadline::Enter).is_none());
    assert_eq!(
        run.context.retire_classifier(&mut run.clock).unwrap(),
        Advance::Progress
    );
    finish_shutdown(run, false);
}

#[test]
fn release_shutdown_after_actual_summary_refusal_or_cancel_keeps_live_tap_and_read_evidence() {
    for road in ["refused", "cancel-unsent", "cancel-released"] {
        let mut fx = SummaryFixture::new(vec![]);
        fx.begin_boot();
        let raw = if road == "refused" {
            let request = fx.next_request();
            fx.boot
                .member
                .respond(&request, refused(s::ReadRefusalCode::ResultTooLarge));
            None
        } else {
            let raw = if road == "cancel-released" {
                Some(fx.release(Some(fx.snapshot())))
            } else {
                None
            };
            let run = fx.run();
            run.context.cancel_plan_summary(&mut run.clock).unwrap();
            raw
        };
        finish_summary_without_take(&mut fx);
        let account = rollback_boot_summary(&mut fx);
        assert_eq!(
            account.recorded_read().map(|read| read.raw_bytes()),
            raw.as_deref()
        );
        assert_eq!(account.refusal().is_some(), road == "refused");
        assert!(account.incomplete().is_none());
        assert!(!fx.run().context.barrier.recorder().tap_stopped());
        finish_summary_shutdown(&mut fx);
        // The account remains owned after the ordinary shutdown stream drains.
        assert_eq!(
            account.recorded_read().map(|read| read.raw_bytes()),
            raw.as_deref()
        );
    }
}

#[test]
fn release_shutdown_after_summary_original_expiry_retains_partial_snapshot_and_live_tap() {
    let mut fx = SummaryFixture::new(vec![]);
    fx.begin_boot();
    let request = fx.next_request();
    let original = fx.boot.member.bound();
    let raw = summary_bytes(Some(fx.snapshot()));
    let cut = raw.len() / 2;
    fx.boot
        .member
        .respond(&request, page(41, 0, raw.len(), &raw[..cut]));
    assert!(matches!(
        fx.next_request(),
        s::StateFrame::Control {
            frame: s::StateControl::Next { .. },
            ..
        }
    ));
    let run = fx.run();
    let early = AbsoluteBound::after(run.clock.now().unwrap(), 1000).unwrap();
    assert_eq!(
        run.context
            .begin_boot_plan_summary_release(early, &mut run.clock),
        Err(RunFailure::PlanSummaryOwned)
    );
    run.clock.advance_to(original.deadline_ns()).unwrap();
    let leave = AbsoluteBound::after(original.deadline_ns(), 1000).unwrap();
    run.context
        .begin_boot_plan_summary_release(leave, &mut run.clock)
        .unwrap();
    let account = run.context.take_stopped_plan_summary().unwrap().unwrap();
    assert!(account.incomplete().is_some());
    assert_eq!(
        account.read_evidence().unwrap().assembled_bytes(),
        &raw[..cut]
    );
    assert_eq!(
        account.read_evidence().unwrap().acquired_snapshot(),
        Some(41.into())
    );
    assert!(account.recorded_read().is_none());
    assert!(!run.context.barrier.recorder().tap_stopped());
    finish_summary_shutdown(&mut fx);
    assert_eq!(
        account.read_evidence().unwrap().assembled_bytes(),
        &raw[..cut]
    );
}

#[test]
fn release_custody_actual_boot_head_cannot_start_new_open_after_release() {
    let mut fx = OpenFixture::new();
    let run = &mut fx.boot.member.run;
    let leave = AbsoluteBound::after(run.clock.now().unwrap(), 1000).unwrap();
    run.context.begin_releasing(leave, &mut run.clock).unwrap();
    assert_eq!(run.context.phase(), RecordingPhase::Releasing);
    let before = run.context.barrier.recorder().structure().len();
    assert_eq!(
        run.context
            .begin_boot_open(&mut fx.decode, &mut run.clock)
            .unwrap_err()
            .refusal,
        BootOpenBeginRefusal::WrongPhase
    );
    assert!(!run.context.barrier.boot_open_owed());
    assert!(run.context.boot_open.is_none());
    assert!(run.context.failure().is_none());
    assert_eq!(run.context.barrier.recorder().structure().len(), before);
    assert!(receive(fx.peer.as_fd()).is_none());
}
