//! Caller-return custody is separate from an allocated pending author ticket.
use super::*;
use weaver_types::state_flow::ReadSelector;

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
            assert!(matches!(
                result,
                Err(RunFailure::Flow {
                    reason: FlowHaltReason::ProtocolFailed,
                    ..
                })
            ));
            assert_eq!(format!("{:?}", fx.context.held_author().unwrap()), expected);
            assert_eq!(fx.context.phase(), RecordingPhase::Failed);
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
    assert!(matches!(
        fx.context.close_after_drain(&mut ports(&mut fx.clock)),
        Err(RunFailure::Flow {
            reason: FlowHaltReason::ProtocolFailed,
            ..
        })
    ));
    assert_eq!(
        format!("{:?}", fx.context.take_held_author().unwrap()),
        expected
    );
    assert_ne!(fx.context.phase(), RecordingPhase::Closed);
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
