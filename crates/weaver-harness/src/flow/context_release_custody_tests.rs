//! Exact unadmitted return custody. Debt-rise probes are explicitly defensive;
//! real Boot/Placement account fixtures separately pin public owner admission.
use super::*;

#[test]
fn release_custody_nonfatal_originals_replace_only_held_evidence_and_can_be_taken() {
    for (boot, place, expected) in [
        (true, false, RunFailure::BootOpenOwned),
        (false, true, RunFailure::PlaceOwned),
        (true, true, RunFailure::PlaceOwned),
    ] {
        let mut fx = releasing(true, false);
        fx.context.barrier.set_boot_open_owed(boot);
        fx.context.barrier.set_place_owed(place);
        let first = shutdown(Kind::Fault);
        let second = shutdown(Kind::Refusal);
        let first_pointer = original_pointer(&first);
        let second_pointer = original_pointer(&second);
        assert_ne!(first_pointer, second_pointer);
        let before = fx.context.barrier.recorder().structure().len();
        for (original, pointer) in [(first, first_pointer), (second, second_pointer)] {
            // Both events exist before the earlier held allocation is replaced.
            assert_eq!(fx.author(original), Err(expected.clone()));
            assert_eq!(original_pointer(fx.context.held_author().unwrap()), pointer);
            assert!(fx.context.refused_author().is_none());
            assert!(fx.context.author_turn.is_none());
            assert!(fx.context.release_author.is_none());
            assert!(fx.context.pending_author().is_none());
            assert!(fx.context.barrier.pending().is_none());
            assert!(fx.context.failure().is_none());
            assert_eq!(fx.context.barrier.recorder().structure().len(), before);
        }
        let returned = fx.context.take_held_author().unwrap();
        assert!(fx.context.take_held_author().is_none());
        assert_eq!(original_pointer(&returned), second_pointer);
        fx.context.barrier.set_boot_open_owed(false);
        fx.context.barrier.set_place_owed(false);
        fx.author(returned).unwrap();
        assert_eq!(fx.context.barrier.recorder().structure().len(), before + 1);
        assert_eq!(
            fx.context
                .barrier
                .recorder()
                .structure()
                .by_kind(Kind::Refusal)
                .count(),
            1
        );
    }
}

#[test]
fn release_custody_prior_hold_cannot_replace_first_fatal_original() {
    let mut fx = releasing(true, false);
    fx.context.barrier.set_boot_open_owed(true);
    let held = shutdown(Kind::Fault);
    let fatal = shutdown(Kind::Refusal);
    let held_pointer = original_pointer(&held);
    let fatal_pointer = original_pointer(&fatal);
    assert_eq!(fx.author(held), Err(RunFailure::BootOpenOwned));
    assert!(fx.context.refused_author().is_none());
    fx.context.barrier.set_boot_open_owed(false);
    let progress =
        fx.context
            .submit_context_author(fatal, RecordPurpose::Content, 1, &mut fx.clock);
    let failure = fx
        .context
        .wait_recorded(&mut ports(&mut fx.clock), progress)
        .unwrap_err();
    assert!(matches!(
        failure,
        RunFailure::Flow {
            reason: FlowHaltReason::ProtocolFailed,
            ..
        }
    ));
    assert_eq!(
        original_pointer(fx.context.refused_author().unwrap()),
        fatal_pointer
    );
    assert_eq!(
        original_pointer(fx.context.held_author().unwrap()),
        held_pointer
    );
    // A later original cannot replace the first fatal evidence or failure.
    assert_eq!(fx.author(shutdown(Kind::Fault)), Err(failure.clone()));
    assert_eq!(fx.context.failure(), Some(&failure));
    assert_eq!(
        original_pointer(fx.context.refused_author().unwrap()),
        fatal_pointer
    );
    let held = fx.context.take_held_author().unwrap();
    assert_eq!(original_pointer(&held), held_pointer);
    assert_eq!(fx.author(held), Err(failure));
    assert_eq!(
        original_pointer(fx.context.refused_author().unwrap()),
        fatal_pointer
    );
}

#[test]
fn release_custody_early_nonfatal_and_fatal_second_callers_keep_pending_author_turn() {
    for fatal in [false, true] {
        let mut fx = releasing(true, true);
        let wait = AbsoluteBound::after(fx.clock.now().unwrap(), 10).unwrap();
        fx.context
            .arm(PumpDeadline::Label, wait, &mut fx.clock)
            .unwrap();
        let first = shutdown(Kind::Fault);
        let pointer = original_pointer(&first);
        assert_eq!(
            fx.author(first),
            Err(RunFailure::Wait {
                key: PumpDeadline::Label,
                bound: wait
            })
        );
        let turn = fx.context.author_turn.clone();
        assert!(turn.is_some());
        let ticket = fx.context.pending_author().unwrap().ticket;
        let attempt = Arc::clone(&fx.context.release_author.as_ref().unwrap().identity);
        fx.context.complete_wait(PumpDeadline::Label).unwrap();
        if fatal {
            assert!(matches!(
                fx.author(shutdown(Kind::Refusal)),
                Err(RunFailure::Flow {
                    reason: FlowHaltReason::ProtocolFailed,
                    ..
                })
            ));
            assert!(fx.context.refused_author().is_some());
        } else {
            assert_eq!(
                fx.author(event(Kind::ToolCallStarted, None)),
                Err(RunFailure::PairOwned)
            );
            assert_eq!(
                fx.context.take_held_author().unwrap().kind,
                Kind::ToolCallStarted
            );
            assert!(fx.context.refused_author().is_none());
        }
        assert_eq!(fx.context.author_turn, turn);
        assert_eq!(fx.context.pending_author().unwrap().ticket, ticket);
        assert_eq!(
            original_pointer(&fx.context.pending_author().unwrap().event),
            pointer
        );
        assert!(Arc::ptr_eq(
            &attempt,
            &fx.context.release_author.as_ref().unwrap().identity
        ));
        if !fatal {
            drain(&mut fx);
            fx.context
                .resume_author_and_wait(&mut ports(&mut fx.clock))
                .unwrap();
            assert!(fx.context.author_turn.is_none());
        }
    }
}

#[test]
fn release_custody_defensive_debt_rise_returns_exact_unadmitted_ticket_without_renewal() {
    for (boot, place, expected) in [
        (true, false, OwedAuthor::BootOpen),
        (false, true, OwedAuthor::Placement),
        (true, true, OwedAuthor::Placement),
    ] {
        let mut fx = releasing(true, true);
        let leave = fx.context.deadlines.get(PumpDeadline::Leave).unwrap();
        let original = shutdown(Kind::Refusal);
        let pointer = original_pointer(&original);
        let label = AbsoluteBound::after(fx.clock.now().unwrap(), 10).unwrap();
        fx.context
            .arm(PumpDeadline::Label, label, &mut fx.clock)
            .unwrap();
        assert_eq!(
            fx.author(original),
            Err(RunFailure::Wait {
                key: PumpDeadline::Label,
                bound: label
            })
        );
        let ticket = fx.context.pending_author().unwrap().ticket;
        assert!(fx.context.barrier.pending().is_some());
        let before = fx.context.barrier.recorder().structure().len();
        fx.context.complete_wait(PumpDeadline::Label).unwrap();
        // No public owner can newly start in Releasing. Inject only debt flags
        // after real capacity pressure issued the exact scope/mark/ticket.
        fx.context.barrier.set_boot_open_owed(boot);
        fx.context.barrier.set_place_owed(place);
        let progress = fx.context.resume_context_author(&mut fx.clock);
        assert!(
            matches!(&progress, RecordProgress::BusyAuthored { owed: Some(actual), purpose: RecordPurpose::Observation, .. } if *actual == expected)
        );
        assert!(fx.context.pending_author().is_none());
        assert!(fx.context.barrier.pending().is_none());
        let failure = fx
            .context
            .wait_recorded(&mut ports(&mut fx.clock), progress)
            .unwrap_err();
        assert_eq!(
            failure,
            if expected == OwedAuthor::Placement {
                RunFailure::PlaceOwned
            } else {
                RunFailure::BootOpenOwned
            }
        );
        assert!(fx.context.author_turn.is_none());
        assert!(fx.context.release_author.is_none());
        assert!(fx.context.failure().is_none());
        assert!(fx.context.barrier.failure().is_none());
        assert!(!fx.context.budget.is_frozen());
        assert!(fx.context.barrier.admitted_before_failure().is_none());
        assert_eq!(fx.context.deadlines.get(PumpDeadline::Leave), Some(leave));
        assert_eq!(fx.context.barrier.recorder().structure().len(), before);
        let returned = fx.context.take_held_author().unwrap();
        assert_eq!(original_pointer(&returned), pointer);
        fx.context.barrier.set_boot_open_owed(false);
        fx.context.barrier.set_place_owed(false);
        drain(&mut fx);
        fx.author(returned).unwrap();
        assert_eq!(fx.context.barrier.recorder().structure().len(), before + 1);
        assert!(ticket > 0);
    }
}

#[test]
fn release_custody_debt_cannot_hide_invalid_pending_scope_ticket_or_first_failure() {
    for defect in ["scope", "missing_scope", "ticket", "failure"] {
        let mut fx = releasing(true, true);
        let original = shutdown(Kind::Fault);
        let pointer = original_pointer(&original);
        assert!(matches!(submit(&mut fx, original), RecordProgress::Pending));
        fx.context.barrier.set_boot_open_owed(true);
        fx.context.barrier.set_place_owed(true);
        match defect {
            "scope" => {
                fx.context.release_author = Some(ReleaseAttempt {
                    identity: Arc::new(()),
                    recorded: None,
                })
            }
            "missing_scope" => fx.context.release_author = None,
            "ticket" => {
                fx.context.barrier.replace_pending_author_ticket_for_test();
            }
            "failure" => {
                assert!(matches!(
                    fx.context.barrier.submit_authored_with_clock(
                        shutdown(Kind::Refusal),
                        RecordPurpose::Observation,
                        &fx.context.author,
                        1,
                        &mut fx.context.inbox,
                        &mut fx.context.budget,
                        &mut || Ok(1),
                    ),
                    RecordProgress::BusyAuthored { owed: None, .. }
                ));
            }
            _ => unreachable!(),
        }
        let ticket = fx.context.pending_author().unwrap().ticket;
        let before = fx.context.barrier.recorder().structure().len();
        assert!(matches!(
            fx.context.resume_context_author(&mut fx.clock),
            RecordProgress::Failed
        ));
        let expected = if defect == "failure" {
            "second author while a ticket is pending"
        } else {
            "release author lost its exact scope or ticket"
        };
        assert!(
            matches!(fx.context.barrier.failure(), Some(RecordFailure::Protocol(detail)) if detail == expected)
        );
        assert_eq!(fx.context.pending_author().unwrap().ticket, ticket);
        assert_eq!(
            original_pointer(&fx.context.pending_author().unwrap().event),
            pointer
        );
        assert!(fx.context.barrier.pending().is_some());
        assert!(fx.context.held_author().is_none());
        assert_eq!(fx.context.barrier.recorder().structure().len(), before);
    }
}

#[test]
fn release_custody_terminal_unload_diagnostic_names_the_actual_owed_owner() {
    for (boot, place, expected) in [
        (true, false, "boot Open account must settle before unload"),
        (false, true, "placement account must settle before unload"),
        (true, true, "placement account must settle before unload"),
    ] {
        let mut fx = releasing(true, false);
        fx.context.barrier.set_boot_open_owed(boot);
        fx.context.barrier.set_place_owed(place);
        let event = shutdown(Kind::Unload);
        let pointer = original_pointer(&event);
        let before = fx.context.barrier.recorder().structure().len();
        assert_eq!(
            fx.context
                .barrier
                .submit_terminal_authored_ref(
                    &event,
                    &fx.context.author,
                    fx.clock.now().unwrap(),
                    1
                )
                .unwrap_err(),
            expected
        );
        assert_eq!(original_pointer(&event), pointer);
        let prepared = fx
            .context
            .author
            .prepare(&event, fx.clock.now().unwrap(), 1)
            .unwrap();
        let prepared_text = format!("{prepared:?}");
        let progress = fx.context.barrier.submit_with_clock(
            prepared,
            RecordPurpose::Observation,
            &mut fx.context.inbox,
            &mut fx.context.budget,
            &mut || fx.clock.now(),
        );
        let RecordProgress::Busy {
            event: returned,
            purpose: RecordPurpose::Observation,
        } = progress
        else {
            panic!("raw unload retains its caller's prepared event")
        };
        assert_eq!(format!("{returned:?}"), prepared_text);
        assert!(fx.context.barrier.pending().is_none());
        assert!(fx.context.barrier.failure().is_none());
        assert_eq!(fx.context.barrier.recorder().structure().len(), before);
    }
}
