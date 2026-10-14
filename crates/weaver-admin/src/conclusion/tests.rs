use super::*;

/// A row settled without publication, with its reset through `reset_of`.
fn settled(kind: LeftKind) -> (MarkerWrite, NextReset, Printed) {
    match conclude(kind) {
        Conclusion::Settled(outcome) => (outcome.marker, reset_of(outcome.marker), outcome.printed),
        Conclusion::Publish { .. } => panic!("{kind:?} publishes"),
    }
}

/// A row after its publication, with its reset through `reset_of`, and the
/// publication's form the row names.
fn published(this_run: bool, landed: bool) -> (Publication, MarkerWrite, NextReset, Printed) {
    let Conclusion::Publish {
        this_run: carried,
        publication,
    } = conclude(LeftKind::Reported { this_run })
    else {
        panic!("a reported save point publishes");
    };
    assert_eq!(carried, this_run);
    let outcome = after_publication(this_run, landed);
    (
        publication,
        outcome.marker,
        reset_of(outcome.marker),
        outcome.printed,
    )
}

/// Row 1, a load's rollback before its `load` event is on the trace: the
/// marker stays as the load found it (K1).
#[test]
fn row1_rollback_before_load_leaves_the_marker_as_found() {
    assert_eq!(
        settled(LeftKind::Rollback {
            load_on_trace: false
        }),
        (
            MarkerWrite::AsFound,
            NextReset::AsMarkerCarries,
            Printed::LoadsOwnRefusal
        )
    );
}

/// Row 1, a rollback after `load` is on the trace: the marker stands `Open`
/// on the run (K1).
#[test]
fn row1_rollback_after_load_writes_the_marker_open() {
    assert_eq!(
        settled(LeftKind::Rollback {
            load_on_trace: true
        }),
        (
            MarkerWrite::Open,
            NextReset::NoCleanUnload,
            Printed::LoadsOwnRefusal
        )
    );
}

/// Row 2, a force over a territory that does not judge (K5): nothing
/// published, the marker `Forced`.
#[test]
fn row2_unjudged_territory_closes_forced() {
    assert_eq!(
        settled(LeftKind::TerritoryUnjudged),
        (
            MarkerWrite::Forced,
            NextReset::ForcedUnload,
            Printed::Unloaded
        )
    );
}

/// Row 3, `no_state`: the run had nothing to keep, so it closes clean (I4).
#[test]
fn row3_no_state_closes_clean() {
    assert_eq!(
        settled(LeftKind::NoState),
        (MarkerWrite::Closed, NextReset::None, Printed::Unloaded)
    );
}

/// Row 4, forced with its save point not taken: the marker `Forced`.
#[test]
fn row4_forced_not_taken_closes_forced() {
    assert_eq!(
        settled(LeftKind::ForcedNotTaken),
        (
            MarkerWrite::Forced,
            NextReset::ForcedUnload,
            Printed::Unloaded
        )
    );
}

/// Row 5, a save point reported whose `event_run` is not the run the marker
/// stands open on: the room published, the reported file as recovered (S17,
/// the operator's ruling of 2026-10-14 on #140), the marker left open,
/// whatever the publication's outcome (the key is blank in the Spec).
/// Perturbation: publish it as reported, as before S17, and this fails.
#[test]
fn row5_another_runs_save_point_publishes_as_recovered_and_leaves_open() {
    for landed in [true, false] {
        assert_eq!(
            published(false, landed),
            (
                Publication::Recovered,
                MarkerWrite::LeaveOpen,
                NextReset::NoCleanUnload,
                Printed::Unloaded
            ),
            "landed = {landed}"
        );
    }
}

/// Row 6, this run's save point published: as reported, the marker `Closed`.
#[test]
fn row6_published_closes_clean() {
    assert_eq!(
        published(true, true),
        (
            Publication::Reported,
            MarkerWrite::Closed,
            NextReset::None,
            Printed::Unloaded
        )
    );
}

/// Row 7, this run's save point attempted as reported and not landed: the
/// marker left open, the caller refused `SavePointNotTaken` naming the
/// publication.
#[test]
fn row7_unpublished_leaves_open_and_refuses() {
    assert_eq!(
        published(true, false),
        (
            Publication::Reported,
            MarkerWrite::LeaveOpen,
            NextReset::NoCleanUnload,
            Printed::SavePointNotTakenPublished
        )
    );
}

/// **The `save-point` verb's table** (3.0, the `save-point` verb's
/// publication; S17), row for row: this run's report published as on
/// demand answers `SavePointTaken`; this run's not landed refuses naming
/// `published`; another run's, its file published as recovered, refuses
/// naming `published` whether or not it landed. Perturbation: answer
/// another run's landed file `SavePointTaken`, or publish it as reported,
/// and this fails.
#[test]
fn the_save_point_verbs_rows() {
    // (event_run the marker's run, landed, publication, answer)
    for (this_run, landed, publication, answer) in [
        (true, true, Publication::Reported, SavePointAnswer::Taken),
        (
            true,
            false,
            Publication::Reported,
            SavePointAnswer::NotTakenPublished,
        ),
        (
            false,
            true,
            Publication::Recovered,
            SavePointAnswer::NotTakenPublished,
        ),
        (
            false,
            false,
            Publication::Recovered,
            SavePointAnswer::NotTakenPublished,
        ),
    ] {
        assert_eq!(publication_of(this_run), publication, "{this_run} {landed}");
        assert_eq!(
            save_point_answer(this_run, landed),
            answer,
            "{this_run} {landed}"
        );
    }
}

/// **No row before the reported ones publishes**: rows 1 to 4 settle from
/// the `Left` alone, so nothing is written into an unjudged territory (K5).
#[test]
fn only_a_reported_save_point_is_published() {
    for kind in [
        LeftKind::Rollback {
            load_on_trace: false,
        },
        LeftKind::Rollback {
            load_on_trace: true,
        },
        LeftKind::TerritoryUnjudged,
        LeftKind::NoState,
        LeftKind::ForcedNotTaken,
    ] {
        assert!(matches!(conclude(kind), Conclusion::Settled(_)), "{kind:?}");
    }
}

/// **The keys are tested in the Spec's order**: a rollback is row 1 even
/// with `no_state` true, and an unjudged territory is row 2 whatever else
/// holds, a save point reported among it.
#[test]
fn the_keys_are_tested_in_order() {
    let base = Seen {
        rollback: None,
        territory_judged: true,
        forced: false,
        no_state: false,
        reported: None,
    };
    let reported = Some(Reported { this_run: true });
    for (seen, kind) in [
        (
            Seen {
                rollback: Some(true),
                forced: true,
                no_state: true,
                ..base
            },
            LeftKind::Rollback {
                load_on_trace: true,
            },
        ),
        (
            Seen {
                territory_judged: false,
                forced: true,
                reported,
                ..base
            },
            LeftKind::TerritoryUnjudged,
        ),
        (
            Seen {
                no_state: true,
                ..base
            },
            LeftKind::NoState,
        ),
        (
            Seen {
                forced: true,
                ..base
            },
            LeftKind::ForcedNotTaken,
        ),
        (
            Seen {
                reported: Some(Reported { this_run: false }),
                ..base
            },
            LeftKind::Reported { this_run: false },
        ),
        // The cases each key admits beside the ones above.
        (
            Seen {
                rollback: Some(false),
                forced: true,
                ..base
            },
            LeftKind::Rollback {
                load_on_trace: false,
            },
        ),
        (
            Seen {
                no_state: true,
                forced: true,
                ..base
            },
            LeftKind::NoState,
        ),
        (
            Seen { reported, ..base },
            LeftKind::Reported { this_run: true },
        ),
        (
            Seen {
                forced: true,
                reported,
                ..base
            },
            LeftKind::Reported { this_run: true },
        ),
        (
            Seen {
                forced: true,
                reported: Some(Reported { this_run: false }),
                ..base
            },
            LeftKind::Reported { this_run: false },
        ),
    ] {
        assert_eq!(classify(seen), Ok(kind), "{seen:?}");
    }
}

/// **The combinations the Spec lists as never answered** classify as
/// unreachable: a rollback that is not forced or that reports a save point,
/// an unjudged territory with `forced` false, `no_state` with a save point,
/// and a graceful leave with no save point and some state to keep.
#[test]
fn the_unreachable_combinations_classify_as_unreachable() {
    let base = Seen {
        rollback: None,
        territory_judged: true,
        forced: false,
        no_state: false,
        reported: None,
    };
    let reported = Some(Reported { this_run: true });
    for (case, seen) in [
        (
            "a rollback not forced",
            Seen {
                rollback: Some(false),
                ..base
            },
        ),
        (
            "a rollback reporting a save point",
            Seen {
                rollback: Some(false),
                forced: true,
                reported,
                ..base
            },
        ),
        (
            "an unjudged territory, unforced",
            Seen {
                territory_judged: false,
                ..base
            },
        ),
        (
            "no state with a save point",
            Seen {
                no_state: true,
                reported,
                ..base
            },
        ),
        ("graceful, no save point, state to keep", base),
    ] {
        assert_eq!(classify(seen), Err(Unreachable), "{case}");
    }
}
