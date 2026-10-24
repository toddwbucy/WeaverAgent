//! Frozen cut boundary checks with real owners and explicit metadata corruption.
use super::*;

fn frozen_native_drain() -> (PlaceFixture, EffectPlanSummary, AbsoluteBound) {
    let (mut fx, plan) = PlaceFixture::with_active_scope();
    let units = fx.whole_span(8, "record", 80);
    fx.turn("drain-boundary-review");
    fx.generation(units, 3, 0, "", Finish::Completed);
    let proof = fx.closed_effect(plan);
    fx.close_turn();
    let prefix = fx.prefix(None, s::CutMethod::Flush);
    assert!(!prefix.coverage().is_empty());
    assert!(prefix.n() > fx.journal().0);
    fx.begin_reload(prefix, ReloadTail::held(vec![]));
    assert_eq!(fx.advance_reload().unwrap(), ReloadProgress::Pending);
    let run = fx.run();
    let cut = run.context.deadlines.get(PumpDeadline::Cut).unwrap();
    let evidence = run
        .context
        .reload
        .as_ref()
        .unwrap()
        .drain_evidence_for_test();
    assert_eq!(evidence.bound, Some(cut));
    assert!(!evidence.measured);
    assert!(run.context.measure.is_none());
    assert!(run.context.native_marks.is_none());
    assert!(!run.context.native_place_cache_ready());
    assert!(receive(fx.peer.as_fd()).is_none());
    (fx, proof, cut)
}

fn finish_owed_native_drain(fx: &mut PlaceFixture, proof: EffectPlanSummary) {
    let run = fx.run();
    run.context
        .begin_place_effect_marks(proof, &mut run.clock)
        .unwrap();
    let completed = fx.finish_native();
    assert_eq!(completed.recorded_sequences().len(), 2);
    assert!(fx.run().context.native_place_cache_ready());
    assert!(fx.run().context.native_marks.is_none());
}

fn assert_frozen_boundary_refused(fx: &mut PlaceFixture, cut: AbsoluteBound) {
    let journal = fx.journal();
    let placement_count = fx.records(Kind::StatePlacement).len();
    let run = fx.run();
    assert!(run.clock.now().unwrap() < cut.deadline_ns());
    assert_eq!(fx.advance_reload().unwrap(), ReloadProgress::Progress);
    let run = fx.run();
    let evidence = run
        .context
        .reload
        .as_ref()
        .unwrap()
        .drain_evidence_for_test();
    assert!(
        evidence.refused,
        "the frozen-boundary guard itself must refuse before measurement"
    );
    assert_eq!(evidence.code, Some(ReloadCode::InvalidPrefix));
    assert_eq!(evidence.detail, Some("cut boundary changed during drain"));
    assert!(!evidence.measured);
    assert!(!evidence.reserved);
    assert!(run.context.measure.is_none());
    assert_eq!(run.context.deadlines.get(PumpDeadline::Cut), Some(cut));
    assert!(run.clock.now().unwrap() < cut.deadline_ns());
    assert!(run.context.failure().is_none());
    assert!(receive(fx.peer.as_fd()).is_none());
    let ReloadCompletion::Refused(mut account) = fx.finish_reload() else {
        panic!("changed frozen boundary must return its real refusal account");
    };
    assert_eq!(
        account.reload_refusal(),
        Some((
            ReloadCode::InvalidPrefix,
            "cut boundary changed during drain"
        ))
    );
    assert!(account.take_reload_inputs().is_some());
    assert_eq!(fx.journal(), journal);
    assert_eq!(fx.records(Kind::StatePlacement).len(), placement_count);
    assert!(fx.records(Kind::Flush).is_empty());
    assert!(fx.records(Kind::Elision).is_empty());
    assert!(receive(fx.peer.as_fd()).is_none());
    assert!(fx.run().context.failure().is_none());
}

#[test]
fn place_drain_review_defensive_saved_n_change_refuses_before_measurement() {
    let (mut fx, proof, cut) = frozen_native_drain();
    let journal = fx.journal();
    let prefix = fx
        .run()
        .context
        .reload
        .as_mut()
        .unwrap()
        .saved_prefix_mut_for_drain_review();
    let n = prefix.n();
    // Normal admitted drainage cannot change the physical N. Corrupt only this
    // genuine owner's saved arithmetic to isolate the otherwise redundant guard.
    prefix.replace_saved_n_for_drain_review(n - 1);
    assert_eq!(fx.journal(), journal);
    finish_owed_native_drain(&mut fx, proof);
    assert_frozen_boundary_refused(&mut fx, cut);
}

#[test]
fn place_drain_review_defensive_lost_message_source_refuses_before_measurement() {
    let (mut fx, proof, cut) = frozen_native_drain();
    let journal = fx.journal();
    let prefix = fx
        .run()
        .context
        .reload
        .as_mut()
        .unwrap()
        .saved_prefix_mut_for_drain_review();
    // Reuse an actual placed source in the saved metadata of another retained
    // Message. No production drain does this. Refresh must refuse that claimed
    // per-message source interval's disappearance independently of N/end/IDs.
    let source = prefix.add_saved_source_for_drain_review();
    assert!(prefix.coverage().contains(&source));
    assert_eq!(fx.journal(), journal);
    finish_owed_native_drain(&mut fx, proof);
    assert_frozen_boundary_refused(&mut fx, cut);
}

#[test]
fn place_drain_review_actual_native_marks_preserve_n_messages_and_covered_sources() {
    let (mut fx, proof, cut) = frozen_native_drain();
    let prefix = fx
        .run()
        .context
        .reload
        .as_ref()
        .unwrap()
        .saved_prefix_for_drain_review();
    let n = prefix.n();
    let resident = prefix.resident_before();
    let revision = prefix.revision();
    let coverage = prefix.coverage().to_vec();
    let rebuild = prefix.rebuild();
    let marks = fx.journal().7.len();
    finish_owed_native_drain(&mut fx, proof);
    assert_eq!(fx.journal().7.len(), marks + 2);
    let TokenDirective::MeasureDelta { keep, delta, .. } = fx.reload_packet() else {
        panic!("actual native drain must allow the same frozen prefix to measure");
    };
    assert_eq!(keep, n);
    assert_eq!(delta, rebuild);
    let run = fx.run();
    let refreshed = run
        .context
        .reload
        .as_ref()
        .unwrap()
        .saved_prefix_for_drain_review();
    assert_ne!(refreshed.revision(), revision);
    assert_eq!(refreshed.n(), n);
    assert_eq!(refreshed.resident_before(), resident);
    assert!(
        coverage
            .iter()
            .all(|source| refreshed.coverage().contains(source))
    );
    assert_eq!(run.context.deadlines.get(PumpDeadline::Cut), Some(cut));
    fx.answer(&fx.measured(n, 0));
    assert_eq!(fx.reload_packet(), TokenDirective::Flush { keep: n });
    fx.answer(&TokenAnswer::Flushed {
        resident_before: resident,
        resident_after: n,
    });
    assert!(matches!(fx.finish_reload(), ReloadCompletion::Staged(_)));
    assert_eq!(fx.journal().1, n);
    assert!(receive(fx.peer.as_fd()).is_none());
    assert!(fx.run().context.failure().is_none());
}
