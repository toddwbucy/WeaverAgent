//! Caller-return custody is separate from an allocated pending author ticket.
use super::*;
use crate::flow::context::held_input_tests::{HOLDS, hold};
use weaver_types::state_flow::ReadSelector;

#[test]
fn held_boundary_release_preserves_both_originals_and_the_original_leave() {
    for held in HOLDS {
        for expired in [false, true] {
            let mut fx = Fixture::new(config());
            fx.activate();
            fx.context.complete_wait(PumpDeadline::Enter).unwrap();
            let leave = AbsoluteBound::after(fx.clock.now().unwrap(), 1_000_000).unwrap();
            fx.context
                .arm(PumpDeadline::Leave, leave, &mut fx.clock)
                .unwrap();
            let before = fx.context.barrier.recorder().structure().len();
            let originals = hold(&mut fx.context, held);
            for _ in 0..2 {
                assert_eq!(
                    fx.context.begin_releasing(leave, &mut fx.clock),
                    Err(RunFailure::Busy(held))
                );
                assert!(fx.context.failure().is_none());
                assert_eq!(fx.context.phase(), RecordingPhase::Entered);
                assert_eq!(fx.context.deadlines.get(PumpDeadline::Leave), Some(leave));
                assert!(fx.context.flow.standing_state().is_some());
                assert!(fx.context.pending_author().is_none());
                assert_eq!(fx.context.barrier.recorder().structure().len(), before);
                originals.assert_held(&fx.context);
            }
            originals.take(&mut fx.context);
            if expired {
                fx.clock.advance_to(leave.deadline_ns()).unwrap();
                assert_eq!(
                    fx.context.begin_releasing(leave, &mut fx.clock),
                    Err(RunFailure::Wait {
                        key: PumpDeadline::Leave,
                        bound: leave
                    })
                );
                assert_eq!(fx.context.phase(), RecordingPhase::Entered);
            } else {
                fx.context.begin_releasing(leave, &mut fx.clock).unwrap();
                assert_eq!(fx.context.phase(), RecordingPhase::Releasing);
            }
            assert_eq!(fx.context.deadlines.get(PumpDeadline::Leave), Some(leave));
        }
    }
}

fn refused_enter() -> Fixture {
    let mut declaration = config();
    declaration.partition[0].kind = "save_point".into();
    let mut fx = Fixture::new(declaration);
    fx.opened();
    fx.author(fx.load_event()).unwrap();
    assert!(matches!(
        fx.context.activate(&mut fx.clock),
        Err(ActivationFailure::Partition(_))
    ));
    fx.author(event(
        Kind::Refusal,
        Some(Payload::Refusal(
            serde_json::value::RawValue::from_string(r#"{"case":"partition_invalid"}"#.into())
                .unwrap(),
        )),
    ))
    .unwrap();
    assert!(fx.context.barrier.is_refused());
    fx
}

#[test]
fn held_boundary_enter_rollback_returns_busy_then_uses_the_same_recorded_refusal() {
    for held in HOLDS {
        let mut fx = refused_enter();
        let opened = fx.context.opened_identity().unwrap().clone();
        let enter = fx.context.deadlines.get(PumpDeadline::Enter).unwrap();
        let before = fx.context.last_author_sequence;
        let originals = hold(&mut fx.context, held);
        let leave = AbsoluteBound::after(fx.clock.now().unwrap(), 1_000_000).unwrap();
        assert_eq!(
            fx.context.begin_enter_rollback(leave, &mut fx.clock),
            Err(RunFailure::Busy(held))
        );
        assert_eq!(fx.context.phase(), RecordingPhase::Entered);
        assert!(fx.context.failure().is_none());
        assert!(fx.context.deadlines.get(PumpDeadline::Leave).is_none());
        assert_eq!(fx.context.deadlines.get(PumpDeadline::Enter), Some(enter));
        assert_eq!(fx.context.opened_identity(), Some(&opened));
        assert_eq!(fx.context.last_author_sequence, before);
        assert!(fx.context.pending_author().is_none());
        assert!(fx.context.flow.unused_open().is_none());
        originals.assert_held(&fx.context);
        originals.take(&mut fx.context);
        assert_eq!(
            fx.context
                .begin_enter_rollback(leave, &mut fx.clock)
                .unwrap(),
            Advance::Progress
        );
        assert_eq!(fx.context.phase(), RecordingPhase::Releasing);
        assert!(matches!(fx.context.flow.unused_open().unwrap().answer(),
            weaver_types::state_flow::StateFrame::Opened { member_observation, .. } if member_observation == &opened));
        assert_eq!(fx.context.deadlines.get(PumpDeadline::Leave), Some(leave));
    }
}

#[test]
fn held_boundary_closed_preserves_unload_and_does_not_renew_the_drain_cap() {
    for held in HOLDS {
        for expired in [false, true] {
            let mut fx = releasing(false, false);
            fx.author(shutdown(Kind::Unload)).unwrap();
            drain(&mut fx);
            let leave = fx.context.deadlines.get(PumpDeadline::Leave).unwrap();
            let before = fx.context.barrier.recorder().structure().len();
            let originals = hold(&mut fx.context, held);
            assert_eq!(
                fx.context.close_after_drain(&mut ports(&mut fx.clock)),
                Err(RunFailure::Busy(held))
            );
            assert_eq!(fx.context.phase(), RecordingPhase::Releasing);
            assert!(fx.context.failure().is_none());
            assert!(fx.context.barrier.is_closed());
            assert_eq!(fx.context.barrier.recorder().structure().len(), before);
            assert_eq!(fx.context.deadlines.get(PumpDeadline::Leave), Some(leave));
            originals.assert_held(&fx.context);
            originals.take(&mut fx.context);
            if expired {
                fx.clock.advance_to(leave.deadline_ns()).unwrap();
                assert_eq!(
                    fx.context.close_after_drain(&mut ports(&mut fx.clock)),
                    Err(RunFailure::Wait {
                        key: PumpDeadline::Leave,
                        bound: leave
                    })
                );
                assert_eq!(fx.context.phase(), RecordingPhase::Releasing);
            } else {
                fx.context
                    .close_after_drain(&mut ports(&mut fx.clock))
                    .unwrap();
                assert_eq!(fx.context.phase(), RecordingPhase::Closed);
            }
            assert_eq!(fx.context.deadlines.get(PumpDeadline::Leave), Some(leave));
        }
    }
}

#[test]
fn held_boundary_first_or_independent_failure_precedes_busy_and_cannot_be_taken_away() {
    for held in HOLDS {
        for boundary in 0..3 {
            for already_failed in [false, true] {
                let mut fx = match boundary {
                    0 => {
                        let mut fx = Fixture::new(config());
                        fx.activate();
                        fx
                    }
                    1 => refused_enter(),
                    _ => {
                        let mut fx = releasing(false, false);
                        fx.author(shutdown(Kind::Unload)).unwrap();
                        drain(&mut fx);
                        fx
                    }
                };
                let leave =
                    fx.context.deadlines.get(PumpDeadline::Leave).unwrap_or(
                        AbsoluteBound::after(fx.clock.now().unwrap(), 1_000_000).unwrap(),
                    );
                let originals = hold(&mut fx.context, held);
                let first = already_failed.then(|| {
                    fx.context.fail(
                        &mut fx.clock,
                        FlowHaltReason::ProtocolFailed,
                        HaltPoint::Barrier,
                        "earlier independent cause",
                    )
                });
                if !already_failed {
                    fx.clock = crate::flow::clock::testing::FakePumpIo::new(0);
                }
                let attempt = |fx: &mut Fixture| match boundary {
                    0 => fx.context.begin_releasing(leave, &mut fx.clock),
                    1 => fx
                        .context
                        .begin_enter_rollback(leave, &mut fx.clock)
                        .map(|_| ()),
                    _ => fx.context.close_after_drain(&mut ports(&mut fx.clock)),
                };
                let failure = attempt(&mut fx).unwrap_err();
                assert!(matches!(failure, RunFailure::Flow { .. }));
                if let Some(first) = first {
                    assert_eq!(failure, first);
                }
                assert_eq!(fx.context.failure(), Some(&failure));
                assert_eq!(fx.context.phase(), RecordingPhase::Failed);
                originals.assert_held(&fx.context);
                originals.take(&mut fx.context);
                assert_eq!(attempt(&mut fx), Err(failure));
                assert_eq!(fx.context.phase(), RecordingPhase::Failed);
            }
        }
    }
}

#[test]
fn author_admission_review_held_original_blocks_read_and_release_until_taken() {
    for take_before_release in [false, true] {
        let mut fx = Fixture::new(config());
        fx.activate();
        let original = event(Kind::ToolCallStarted, None);
        let expected = format!("{original:?}");
        assert_eq!(fx.author(original), Err(RunFailure::PairOwned));
        assert!(fx.context.pending_author().is_none());
        let before = fx.context.barrier.recorder().structure().len();
        let enter = fx.context.deadlines.get(PumpDeadline::Enter).unwrap();
        let error = fx
            .context
            .begin_read(ReadSelector::PlanCurrent {}, None, &mut fx.clock)
            .unwrap_err();
        assert_eq!(error.refusal, ReadBeginRefusal::Busy);
        assert!(error.failure.is_none());
        assert!(fx.context.read.is_none());
        assert_eq!(fx.context.deadlines.get(PumpDeadline::Enter), Some(enter));
        assert_eq!(format!("{:?}", fx.context.held_author().unwrap()), expected);
        assert_eq!(fx.context.barrier.recorder().structure().len(), before);
        fx.context.complete_wait(PumpDeadline::Enter).unwrap();
        if take_before_release {
            let returned = fx.context.take_held_author().unwrap();
            assert_eq!(format!("{returned:?}"), expected);
        }
        let leave = AbsoluteBound::after(fx.clock.now().unwrap(), 1_000_000).unwrap();
        let result = fx.context.begin_releasing(leave, &mut fx.clock);
        if take_before_release {
            result.unwrap();
            assert_eq!(fx.context.phase(), RecordingPhase::Releasing);
        } else {
            assert_eq!(result, Err(RunFailure::Busy(HeldInput::Caller)));
            assert_eq!(format!("{:?}", fx.context.held_author().unwrap()), expected);
            assert_eq!(fx.context.phase(), RecordingPhase::Entered);
            assert!(fx.context.deadlines.get(PumpDeadline::Leave).is_none());
        }
    }
}

#[test]
fn author_admission_review_late_held_original_cannot_disappear_at_closed() {
    let mut fx = releasing(false, false);
    fx.author(shutdown(Kind::Unload)).unwrap();
    assert!(fx.context.barrier.is_closed());
    // The existing early PairOwned path can return a caller original even
    // after recorded Unload. This is actual API misuse, not an invented slot.
    let original = event(Kind::ToolCallStarted, None);
    let expected = format!("{original:?}");
    assert_eq!(fx.author(original), Err(RunFailure::PairOwned));
    drain(&mut fx);
    assert_eq!(
        fx.context.close_after_drain(&mut ports(&mut fx.clock)),
        Err(RunFailure::Busy(HeldInput::Caller))
    );
    assert_eq!(
        format!("{:?}", fx.context.take_held_author().unwrap()),
        expected
    );
    assert_eq!(fx.context.phase(), RecordingPhase::Releasing);
    fx.context
        .close_after_drain(&mut ports(&mut fx.clock))
        .unwrap();
    assert_eq!(fx.context.phase(), RecordingPhase::Closed);
}

#[test]
fn author_admission_review_defensive_pending_ordinary_unload_returns_exact_owed_custody() {
    for raw in [false, true] {
        for (boot, place, expected) in [
            (true, false, OwedAuthor::BootOpen),
            (false, true, OwedAuthor::Placement),
            (true, true, OwedAuthor::Placement),
        ] {
            let mut fx = releasing(false, true);
            let leave = fx.context.deadlines.get(PumpDeadline::Leave).unwrap();
            let before = fx.context.barrier.recorder().structure().len();
            let original = shutdown(Kind::Unload);
            let pointer = original_pointer(&original);
            let mut raw_expected = None;
            let progress = if raw {
                let prepared = fx
                    .context
                    .author
                    .prepare(&original, fx.clock.now().unwrap(), 1)
                    .unwrap();
                raw_expected = Some(format!("{prepared:?}"));
                fx.context.barrier.submit_with_clock(
                    prepared,
                    RecordPurpose::Observation,
                    &mut fx.context.inbox,
                    &mut fx.context.budget,
                    &mut || fx.clock.now(),
                )
            } else {
                fx.context.barrier.submit_authored_with_clock(
                    original,
                    RecordPurpose::Observation,
                    &fx.context.author,
                    1,
                    &mut fx.context.inbox,
                    &mut fx.context.budget,
                    &mut || fx.clock.now(),
                )
            };
            assert!(matches!(progress, RecordProgress::Pending));
            assert_eq!(fx.context.pending_author().is_none(), raw);
            // Public owner starts exclude this debt rise; retain the actual
            // CapacityPending owner while isolating the defensive resume guard.
            fx.context.barrier.set_boot_open_owed(boot);
            fx.context.barrier.set_place_owed(place);
            let returned = fx.context.barrier.resume_with_clock(
                &mut fx.context.inbox,
                &mut fx.context.budget,
                &mut || fx.clock.now(),
            );
            assert!(fx.context.barrier.pending().is_none());
            assert!(fx.context.pending_author().is_none());
            assert!(fx.context.barrier.failure().is_none());
            assert!(!fx.context.budget.is_frozen());
            assert_eq!(fx.context.barrier.recorder().structure().len(), before);
            assert_eq!(fx.context.deadlines.get(PumpDeadline::Leave), Some(leave));
            fx.context.barrier.set_boot_open_owed(false);
            fx.context.barrier.set_place_owed(false);
            drain(&mut fx);
            let retried = match returned {
                RecordProgress::Busy { event, purpose } if raw => {
                    assert_eq!(format!("{event:?}"), raw_expected.unwrap());
                    fx.context.barrier.submit_with_clock(
                        event,
                        purpose,
                        &mut fx.context.inbox,
                        &mut fx.context.budget,
                        &mut || fx.clock.now(),
                    )
                }
                RecordProgress::BusyAuthored {
                    event,
                    purpose,
                    owed: Some(owed),
                } if !raw => {
                    assert_eq!(owed, expected);
                    assert_eq!(original_pointer(&event), pointer);
                    fx.context.barrier.submit_authored_with_clock(
                        event,
                        purpose,
                        &fx.context.author,
                        1,
                        &mut fx.context.inbox,
                        &mut fx.context.budget,
                        &mut || fx.clock.now(),
                    )
                }
                other => panic!("ordinary Unload must return its own ownership shape: {other:?}"),
            };
            assert!(matches!(retried, RecordProgress::Recorded { .. }));
            assert_eq!(fx.context.barrier.recorder().structure().len(), before + 1);
        }
    }
}
