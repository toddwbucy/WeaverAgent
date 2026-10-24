//! Actual owner phase/custody checks plus labelled defensive consumer corruption.
use super::*;
use crate::flow::ledger::LeaseSlotStatus;
use std::sync::Arc;

fn completed_reload() -> PlaceFixture {
    let mut fx = PlaceFixture::new();
    let carry = fx.text(8, "carry");
    let prefix = fx.prefix(None, s::CutMethod::Elide);
    let n = prefix.n();
    assert_eq!(prefix.resident_before(), n);
    fx.begin_reload(prefix, ReloadTail::held(carry));
    assert!(matches!(
        fx.reload_packet(),
        TokenDirective::MeasureDelta { .. }
    ));
    fx.answer(&fx.measured(n, 3));
    let mut completed = false;
    for _ in 0..128 {
        if fx.advance_reload().unwrap() == ReloadProgress::Complete {
            completed = true;
            break;
        }
    }
    assert!(completed, "actual cut and its record must complete");
    assert!(fx.run().context.reload.is_some());
    fx
}

fn measured_staged_placement() -> PlaceFixture {
    let mut fx = PlaceFixture::new();
    let carry = fx.text(8, "carry");
    let prefix = fx.prefix(None, s::CutMethod::Elide);
    let staged = fx.staged(prefix, ReloadTail::held(carry), 3);
    fx.turn("capability-phase");
    fx.begin_staged(staged, vec![]);
    let TokenDirective::MeasureDelta { keep, .. } = fx.next_measure() else {
        unreachable!()
    };
    fx.answer(&fx.measured(keep, 3));
    fx
}

fn actual_staged_placement() -> PlaceFixture {
    let mut fx = measured_staged_placement();
    let mut completed = false;
    for _ in 0..128 {
        if fx.advance().unwrap() == PlaceProgress::Complete {
            completed = true;
            break;
        }
    }
    assert!(completed);
    fx
}

fn release_owed(fx: &mut PlaceFixture) {
    let run = fx.run();
    let bound = AbsoluteBound::after(run.clock.now().unwrap(), PLACE_WAIT / 2).unwrap();
    run.context
        .begin_place_release(bound, &mut run.clock)
        .unwrap();
    assert_eq!(run.context.phase(), RecordingPhase::Releasing);
}

fn release_settled(fx: &mut PlaceFixture) {
    let run = fx.run();
    assert!(
        !run.context.place_owed(),
        "actual completed work has no release debt"
    );
    assert!(
        run.context.open_turn.is_none(),
        "ordinary release follows real turn closure"
    );
    let bound = AbsoluteBound::after(run.clock.now().unwrap(), PLACE_WAIT / 2).unwrap();
    run.context.begin_releasing(bound, &mut run.clock).unwrap();
    assert_eq!(run.context.phase(), RecordingPhase::Releasing);
}

#[test]
fn place_capability_review_releasing_done_placement_preserves_owner_and_staged_evidence() {
    let mut fx = measured_staged_placement();
    let run = fx.run();
    assert!(run.context.place.is_some() && run.context.place_owed());
    let decision = run.context.deadlines.get(PumpDeadline::Decision);
    // Enter release while the actual measurement/account is still owed, then
    // settle the cancelled owner without extracting it. A done ordinary owner
    // cannot bypass its own TurnClosed admission guard to manufacture this phase.
    release_owed(&mut fx);
    let mut completed = false;
    for _ in 0..128 {
        if fx.account_step().unwrap() == PlaceProgress::Complete {
            completed = true;
            break;
        }
    }
    assert!(completed, "actual stopped placement account must complete");
    let journal = fx.journal();
    let run = fx.run();
    assert!(run.context.place.is_some() && !run.context.place_owed());
    let owner = run.context.place.as_ref().unwrap() as *const _;
    assert!(
        run.context
            .take_placement(&mut run.clock)
            .unwrap()
            .is_none()
    );
    assert_eq!(run.context.place.as_ref().unwrap() as *const _, owner);
    assert_eq!(run.context.deadlines.get(PumpDeadline::Decision), decision);
    assert_eq!(fx.journal(), journal);
    let mut account = fx.settled_account();
    let identity = account.staged_evidence().unwrap().identity.clone();
    let recorded = account.staged_evidence().unwrap().recorded.unwrap();
    assert!(account.take_staged().is_none());
    assert!(account.take_unplaced_selection().is_none());
    assert!(Arc::ptr_eq(
        &account.staged_evidence().unwrap().identity,
        &identity
    ));
    assert_eq!(account.staged_evidence().unwrap().recorded, Some(recorded));
    assert!(receive(fx.peer.as_fd()).is_none());
}

#[test]
fn place_capability_review_releasing_done_unsent_generation_keeps_same_carry_in_account() {
    let mut fx = actual_staged_placement();
    let PlacementCompletion::Granted(grant) = fx.take() else {
        panic!("actual grant")
    };
    fx.start_generation(grant);
    let run = &mut fx.summary.boot.member.run;
    run.context
        .cancel_generation(&mut fx.decode, &mut run.clock)
        .unwrap();
    let mut complete = false;
    for _ in 0..128 {
        if fx.advance_generation().unwrap() == PlaceProgress::Complete {
            complete = true;
            break;
        }
    }
    assert!(complete);
    release_owed(&mut fx);
    let run = fx.run();
    let owner = run.context.generation.as_ref().unwrap() as *const _;
    assert!(
        run.context
            .take_placed_generation(&mut run.clock)
            .unwrap()
            .is_none()
    );
    assert_eq!(run.context.generation.as_ref().unwrap() as *const _, owner);
    let mut account = fx.settled_account();
    let identity = account.staged_evidence().unwrap().identity.clone();
    assert!(account.incomplete().is_none());
    assert!(account.take_staged().is_none());
    assert!(Arc::ptr_eq(
        &account.staged_evidence().unwrap().identity,
        &identity
    ));
    assert!(receive(fx.peer.as_fd()).is_none());
}

#[test]
fn place_capability_review_failed_place_and_generation_keep_actual_staged_accounts() {
    for generation in [false, true] {
        let mut fx = actual_staged_placement();
        if generation {
            let PlacementCompletion::Granted(grant) = fx.take() else {
                panic!("grant")
            };
            fx.start_generation(grant);
            let run = &mut fx.summary.boot.member.run;
            run.context
                .cancel_generation(&mut fx.decode, &mut run.clock)
                .unwrap();
            let mut complete = false;
            for _ in 0..128 {
                if fx.advance_generation().unwrap() == PlaceProgress::Complete {
                    complete = true;
                    break;
                }
            }
            assert!(complete);
        }
        let run = fx.run();
        let first = run.context.fail(
            &mut run.clock,
            FlowHaltReason::ProtocolFailed,
            HaltPoint::Barrier,
            "capability review actual first latch",
        );
        // Failed overlaps live(), while the Releasing fixtures isolate the new guards.
        assert_eq!(
            if generation {
                run.context
                    .take_placed_generation(&mut run.clock)
                    .unwrap_err()
            } else {
                run.context.take_placement(&mut run.clock).unwrap_err()
            },
            first
        );
        fx.first_fault_attempt();
        let mut account = fx.settled_account();
        let original = account.staged_evidence().unwrap().identity.clone();
        assert!(account.take_staged().is_none());
        assert!(Arc::ptr_eq(
            &account.staged_evidence().unwrap().identity,
            &original
        ));
        assert_eq!(fx.run().context.failure(), Some(&first));
        assert!(receive(fx.peer.as_fd()).is_none());
    }
}

#[test]
fn place_capability_review_releasing_completed_reload_never_returns_cleared_reservation() {
    let mut fx = completed_reload();
    release_settled(&mut fx);
    let mut account = fx.settled_account();
    let original = account.staged_evidence().unwrap().identity.clone();
    assert!(
        fx.run()
            .context
            .place_engine
            .as_ref()
            .unwrap()
            .staged
            .is_none()
    );
    assert!(account.take_staged().is_none());
    assert!(account.take_reload_inputs().is_none());
    assert!(Arc::ptr_eq(
        &account.staged_evidence().unwrap().identity,
        &original
    ));
    assert!(account.staged_evidence().unwrap().recorded.is_some());
}

#[test]
fn place_capability_review_defensive_incomplete_and_unrecorded_reload_guards_are_independent() {
    for incomplete in [true, false] {
        let mut fx = completed_reload();
        let run = fx.run();
        run.context.cancel_reload(&mut run.clock).unwrap();
        if incomplete {
            run.context.review_mark_completed_reload_incomplete();
        } else {
            run.context.review_remove_completed_reload_record();
        }
        let ReloadCompletion::Cancelled(mut account) = run
            .context
            .take_reload(&mut run.clock)
            .unwrap()
            .expect("actual done cancelled owner")
        else {
            panic!("account")
        };
        assert_eq!(account.incomplete().is_some(), incomplete);
        assert_eq!(
            account.staged_evidence().unwrap().recorded.is_some(),
            incomplete
        );
        let identity = account.staged_evidence().unwrap().identity.clone();
        assert!(
            account.take_staged().is_none(),
            "the other guard remains permissive"
        );
        assert!(Arc::ptr_eq(
            &account.staged_evidence().unwrap().identity,
            &identity
        ));
        assert!(fx.run().context.failure().is_none());
    }
}

#[test]
fn place_capability_review_defensive_unrecorded_success_fault_retains_actual_reload() {
    let mut fx = completed_reload();
    let run = fx.run();
    let removed = run.context.review_remove_completed_reload_record();
    let owner = run.context.reload.as_ref().unwrap() as *const _;
    let original_cut = run.context.deadlines.get(PumpDeadline::Cut);
    let first = run.context.take_reload(&mut run.clock).unwrap_err();
    assert_eq!(
        first,
        RunFailure::Flow {
            reason: FlowHaltReason::ProtocolFailed,
            point: HaltPoint::Barrier,
            detail: "completed reload lost its Recorded staged receipt".into(),
        }
    );
    assert_eq!(run.context.reload.as_ref().unwrap() as *const _, owner);
    assert_eq!(run.context.deadlines.get(PumpDeadline::Cut), original_cut);
    assert_eq!(run.context.failure(), Some(&first));
    fx.first_fault_attempt();
    let account = fx.settled_account();
    assert!(account.recorded_sequences().contains(&removed));
    assert!(account.staged_evidence().is_some());
}

#[test]
fn place_capability_review_native_account_retains_exact_shared_completed_proposal() {
    let (mut fx, plan) = PlaceFixture::with_active_scope();
    let units = fx.text(8, "record");
    fx.turn("native-exact-proposal");
    fx.generation(units, 3, 0, "", Finish::Completed);
    let proof = fx.closed_effect(plan);
    let read = proof.read() as *const RecordedRead;
    let filing = proof.filing().sequence();
    let run = fx.run();
    run.context
        .begin_place_effect_marks(proof, &mut run.clock)
        .unwrap();
    let mut complete = false;
    for _ in 0..128 {
        let run = fx.run();
        if run
            .context
            .advance_place_marks(&mut ports(&mut run.clock))
            .unwrap()
            == PlaceProgress::Complete
        {
            complete = true;
            break;
        }
    }
    assert!(complete);
    let run = fx.run();
    let engine = run.context.place_engine.as_mut().unwrap();
    let exact = Arc::clone(&engine.native);
    assert_eq!(
        Arc::strong_count(&exact),
        3,
        "engine, original owner and this observation"
    );
    let bytes = format!("{exact:?}");
    // Defensive later-writer probe: replacing engine scopes from the actual boot
    // proof must not reconstruct or rewrite the completed account's exact proposal.
    let boot = NativeScopes::from_boot(&engine.boot);
    *Arc::make_mut(&mut engine.native) = boot;
    assert!(!Arc::ptr_eq(&engine.native, &exact));
    run.context.cancel_place_marks().unwrap();
    let account = run
        .context
        .take_cancelled_place_marks(&mut run.clock)
        .unwrap()
        .unwrap();
    assert_eq!(
        account.native_proposal().unwrap() as *const NativeScopes,
        Arc::as_ptr(&exact)
    );
    assert_eq!(format!("{:?}", account.native_proposal().unwrap()), bytes);
    assert_eq!(
        account.native_prerequisite().unwrap().read() as *const RecordedRead,
        read
    );
    assert_eq!(
        account.native_prerequisite().unwrap().filing().sequence(),
        filing
    );
    assert_eq!(
        account.native_recorded_sequences().len(),
        account.native_positions().len()
    );
}

#[test]
fn place_capability_review_multi_slot_whole_output_lease_is_returned_without_losing_siblings() {
    use crate::flow::context::output_tests::{OutputFixture, original, text};
    let mut fx = OutputFixture::new();
    fx.begin(original(vec![text("Read first."), text("Read second.")]));
    fx.complete(None, false);
    let run = &mut fx.input.run;
    let mut checked = run.context.take_output(&mut run.clock).unwrap().unwrap();
    let lease = checked.take_receipt_lease().unwrap();
    let sources: Vec<_> = run
        .context
        .flow
        .ledger()
        .source_lease_sources(&lease)
        .unwrap()
        .cloned()
        .collect();
    assert_eq!(sources.len(), 2);
    let before: Vec<_> = sources
        .iter()
        .map(|source| {
            run.context
                .flow
                .source_receipt_status(&lease, source)
                .unwrap()
        })
        .collect();
    let (returned, source) = run
        .context
        .hold_reload_text(lease, sources[0].clone())
        .unwrap_err();
    assert_eq!(source, sources[0]);
    assert_eq!(
        run.context
            .flow
            .ledger()
            .source_lease_sources(&returned)
            .unwrap()
            .cloned()
            .collect::<Vec<_>>(),
        sources
    );
    for (source, status) in sources.iter().zip(before) {
        assert_eq!(
            run.context
                .flow
                .source_receipt_status(&returned, source)
                .unwrap(),
            status
        );
    }
    assert!(run.context.failure().is_none());
}

fn consumed_retry_tail() -> (PlaceFixture, PreparedPrefix, ReloadTail) {
    let mut fx = PlaceFixture::new();
    let original = fx.whole_span(8, "record", 80);
    let held_first = fx.text(9, "first");
    let held_last = fx.text(10, "last");
    let second_units = fx.text(11, "second");
    fx.turn("lease-boundary-retry");
    fx.hold_files = true;
    let mut first = fx.generation(original, 3, 1, "Read pending.", Finish::Completed);
    let mut output = first.take_output().unwrap();
    let source = output.source().block_id.clone();
    let lease = output.take_receipt_lease().unwrap();
    let pending = fx.run().context.hold_reload_text(lease, source).unwrap();
    let mut second = fx.generation(second_units, 3, 1, "Read second.", Finish::Completed);
    let mut output = second.take_output().unwrap();
    let source = output.source().block_id.clone();
    let lease = output.take_receipt_lease().unwrap();
    let second_pending = fx.run().context.hold_reload_text(lease, source).unwrap();
    let mut tail = ReloadTail::held(held_first);
    tail.push_pending(pending);
    tail.push_pending(second_pending);
    tail.items.extend(ReloadTail::held(held_last).items);
    fx.close_turn();
    let mark = fx.mark(80);
    let prefix = fx.prefix(Some(mark), s::CutMethod::Flush);
    fx.begin_reload(prefix, tail);
    fx.hold_files = false;
    let TokenDirective::MeasureDelta { keep, .. } = fx.reload_packet() else {
        panic!("measure")
    };
    fx.answer(&fx.measured(keep, u64::MAX));
    let ReloadCompletion::Refused(mut account) = fx.finish_reload() else {
        panic!("fit refusal")
    };
    let returned = account.take_reload_inputs().unwrap();
    assert!(returned.rejected().is_none());
    let (prefix, tail) = returned.into_retry().unwrap();
    assert_eq!(tail.items.len(), 4);
    assert_eq!(tail.filed_sources.len(), 2);
    for (index, pending) in &tail.filed_sources {
        assert_eq!(
            fx.run()
                .context
                .flow
                .source_receipt_status(&pending.lease, &pending.source)
                .unwrap(),
            LeaseSlotStatus::Taken
        );
        let ReloadItem::Held(unit) = &tail.items[*index] else {
            panic!("actual Held")
        };
        assert_eq!(unit.filed_source().unwrap().source(), &pending.source);
    }
    let run = fx.run();
    run.context.place_response_sent(&mut run.clock).unwrap();
    (fx, prefix, tail)
}

#[test]
fn place_capability_review_defensive_retry_requires_actual_taken_status() {
    for status in [LeaseSlotStatus::Pending, LeaseSlotStatus::Ready] {
        let (mut fx, prefix, tail) = consumed_retry_tail();
        let (_, pending) = &tail.filed_sources[0];
        // The public consumer cannot possess both a fresh slot and its consumed
        // Held proof. Corrupt only the status of the actual consumed registration;
        // no new Ready proof, receipt, owner, phase or Held is fabricated.
        fx.run().context.flow.review_set_taken_source_status(
            &pending.lease,
            &pending.source,
            status,
        );
        let registered = fx.run().context.flow.ledger().source_lease_count();
        let sources: Vec<_> = tail
            .filed_sources
            .iter()
            .map(|(index, pending)| (*index, pending.source.clone()))
            .collect();
        let run = fx.run();
        let error = run
            .context
            .begin_cut_and_reload(prefix, tail, &mut run.clock)
            .unwrap_err();
        assert_eq!(error.refusal, PlaceError::InvalidTail);
        assert!(error.failure.is_none());
        assert!(run.context.reload.is_none());
        assert_eq!(run.context.flow.ledger().source_lease_count(), registered);
        assert_eq!(
            error
                .input
                .1
                .filed_sources
                .iter()
                .map(|(index, pending)| (*index, pending.source.clone()))
                .collect::<Vec<_>>(),
            sources
        );
        assert!(receive(fx.peer.as_fd()).is_none());
    }
}

#[test]
fn place_capability_review_defensive_retry_requires_order_and_exact_origin() {
    for foreign_origin in [false, true] {
        let (mut fx, prefix, mut tail) = consumed_retry_tail();
        if foreign_origin {
            let mut foreign = PlaceFixture::new();
            tail.filed_sources[0].1.origin = foreign.run().context.read_origin.clone();
        } else {
            tail.filed_sources.swap(0, 1);
        }
        let expected: Vec<_> = tail
            .filed_sources
            .iter()
            .map(|(index, pending)| (*index, pending.source.clone(), Arc::as_ptr(&pending.origin)))
            .collect();
        let registered = fx.run().context.flow.ledger().source_lease_count();
        let run = fx.run();
        let error = run
            .context
            .begin_cut_and_reload(prefix, tail, &mut run.clock)
            .unwrap_err();
        assert_eq!(error.refusal, PlaceError::InvalidTail);
        assert!(error.failure.is_none());
        assert!(run.context.reload.is_none());
        assert_eq!(run.context.flow.ledger().source_lease_count(), registered);
        assert_eq!(
            error
                .input
                .1
                .filed_sources
                .iter()
                .map(|(index, pending)| (
                    *index,
                    pending.source.clone(),
                    Arc::as_ptr(&pending.origin)
                ))
                .collect::<Vec<_>>(),
            expected
        );
        for (_, pending) in &error.input.1.filed_sources {
            assert_eq!(
                run.context
                    .flow
                    .source_receipt_status(&pending.lease, &pending.source)
                    .unwrap(),
                LeaseSlotStatus::Taken
            );
        }
        assert!(receive(fx.peer.as_fd()).is_none());
    }
}

#[test]
fn place_capability_review_retry_releases_both_actual_consumed_leases_without_refiling() {
    let (mut fx, prefix, tail) = consumed_retry_tail();
    let registered = fx.run().context.flow.ledger().source_lease_count();
    let files = fx.summary.files.len();
    let checks = fx.records(Kind::Ste100Result).len();
    let run = fx.run();
    run.context
        .begin_cut_and_reload(prefix, tail, &mut run.clock)
        .unwrap();
    assert_eq!(
        run.context.flow.ledger().source_lease_count(),
        registered - 2
    );
    assert!(run.context.reload.is_some());
    assert_eq!(fx.summary.files.len(), files);
    assert_eq!(fx.records(Kind::Ste100Result).len(), checks);
    assert!(matches!(
        fx.reload_packet(),
        TokenDirective::MeasureDelta { .. }
    ));
    assert_eq!(fx.summary.files.len(), files);
    assert_eq!(fx.records(Kind::Ste100Result).len(), checks);
}
