//! **The conclusion's outcome table**, `weaver-admin-Spec` section 3.0, "the
//! conclusion's outcomes", as one pure decision: what a caller answered `Left`
//! publishes, writes to the marker, leaves the next load to record, and prints.
//!
//! The table's keys are tested in the Spec's order, and the kinds are encoded so
//! that a combination the Spec lists as unreachable cannot be built: `classify`
//! refuses it instead. The I/O, publication and the marker's write, takes the
//! decision's result and decides nothing (section 10, the decision instruments).

/// What the conclusion knows of the `Left` it was answered, before the table:
/// the caller's own kind (a load's rollback, with whether `load` is on the
/// trace, or not a rollback), its own judgment of the territory, the answer's
/// `forced` and `no_state`, and the save point reported, if any.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Seen {
    /// `Some(load_on_trace)` where the caller is the load whose rollback this
    /// is; `None` for every other caller.
    pub rollback: Option<bool>,
    pub territory_judged: bool,
    pub forced: bool,
    pub no_state: bool,
    pub reported: Option<Reported>,
}

/// A save point the `Left` reported: whether its `event_run` is the run the
/// marker stands open on, and whether its publication landed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reported {
    pub this_run: bool,
    pub published: bool,
}

/// **One variant per row of the outcome table**, in the Spec's key order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeftKind {
    /// Row 1: the load's own rollback (S1 x `Leave`), whoever else is answered
    /// nothing.
    Rollback { load_on_trace: bool },
    /// Row 2: a force over a territory that does not judge (K5).
    TerritoryUnjudged,
    /// Row 3: `no_state`, the run having nothing to keep.
    NoState,
    /// Row 4: forced, its save point not taken.
    ForcedNotTaken,
    /// Rows 5 to 7: a save point reported.
    Reported { this_run: bool, published: bool },
}

/// A combination the Spec lists as never answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unreachable;

/// What the marker-write rule is asked to do (section 4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkerWrite {
    /// Write nothing: the marker stays as the load found it.
    AsFound,
    /// Write `Open` on the run (a rollback after `load` is on the trace).
    Open,
    /// Leave the marker open on the run, as it stands.
    LeaveOpen,
    Closed,
    Forced,
}

/// The reset the next load records.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NextReset {
    None,
    /// Whatever cause the marker as found carries.
    AsMarkerCarries,
    NoCleanUnload,
    ForcedUnload,
}

/// What the caller prints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Printed {
    Unloaded,
    /// Refused `SavePointNotTaken` naming `published`.
    SavePointNotTakenPublished,
    /// The load invocation's own refusal (a rollback).
    LoadsOwnRefusal,
}

/// The table's output columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Outcome {
    /// Whether the reported save point is published.
    pub publish: bool,
    pub marker: MarkerWrite,
    pub reset: NextReset,
    pub printed: Printed,
}

/// **The keys, tested in the Spec's order**, so every reachable `Left` matches
/// exactly one row: the rollback, the territory, `no_state`, a save point
/// reported. Each unreachable combination is refused rather than fitted to a
/// row.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "wired into the conclusion by the lifecycle plumbing, PR B"
    )
)]
pub fn classify(seen: Seen) -> Result<LeftKind, Unreachable> {
    if let Some(load_on_trace) = seen.rollback {
        return if seen.forced && seen.reported.is_none() {
            Ok(LeftKind::Rollback { load_on_trace })
        } else {
            Err(Unreachable)
        };
    }
    if !seen.territory_judged {
        return if seen.forced {
            Ok(LeftKind::TerritoryUnjudged)
        } else {
            Err(Unreachable)
        };
    }
    if seen.no_state {
        return if seen.reported.is_none() {
            Ok(LeftKind::NoState)
        } else {
            Err(Unreachable)
        };
    }
    match seen.reported {
        None if seen.forced => Ok(LeftKind::ForcedNotTaken),
        None => Err(Unreachable),
        Some(Reported {
            this_run,
            published,
        }) => Ok(LeftKind::Reported {
            this_run,
            published,
        }),
    }
}

/// **The outcome table, one exhaustive match and no wildcard arm**: a new kind
/// of `Left` does not compile until it has a row.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "wired into the conclusion by the lifecycle plumbing, PR B"
    )
)]
pub fn conclude(kind: LeftKind) -> Outcome {
    match kind {
        LeftKind::Rollback {
            load_on_trace: false,
        } => Outcome {
            publish: false,
            marker: MarkerWrite::AsFound,
            reset: NextReset::AsMarkerCarries,
            printed: Printed::LoadsOwnRefusal,
        },
        LeftKind::Rollback {
            load_on_trace: true,
        } => Outcome {
            publish: false,
            marker: MarkerWrite::Open,
            reset: NextReset::NoCleanUnload,
            printed: Printed::LoadsOwnRefusal,
        },
        LeftKind::TerritoryUnjudged => Outcome {
            publish: false,
            marker: MarkerWrite::Forced,
            reset: NextReset::ForcedUnload,
            printed: Printed::Unloaded,
        },
        LeftKind::NoState => Outcome {
            publish: false,
            marker: MarkerWrite::Closed,
            reset: NextReset::None,
            printed: Printed::Unloaded,
        },
        LeftKind::ForcedNotTaken => Outcome {
            publish: false,
            marker: MarkerWrite::Forced,
            reset: NextReset::ForcedUnload,
            printed: Printed::Unloaded,
        },
        LeftKind::Reported {
            this_run: false,
            published: _,
        } => Outcome {
            publish: true,
            marker: MarkerWrite::LeaveOpen,
            reset: NextReset::NoCleanUnload,
            printed: Printed::Unloaded,
        },
        LeftKind::Reported {
            this_run: true,
            published: true,
        } => Outcome {
            publish: true,
            marker: MarkerWrite::Closed,
            reset: NextReset::None,
            printed: Printed::Unloaded,
        },
        LeftKind::Reported {
            this_run: true,
            published: false,
        } => Outcome {
            publish: true,
            marker: MarkerWrite::LeaveOpen,
            reset: NextReset::NoCleanUnload,
            printed: Printed::SavePointNotTakenPublished,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Row 1, a load's rollback before its `load` event is on the trace: the
    /// marker stays as the load found it (K1).
    #[test]
    fn row1_rollback_before_load_leaves_the_marker_as_found() {
        assert_eq!(
            conclude(LeftKind::Rollback {
                load_on_trace: false
            }),
            Outcome {
                publish: false,
                marker: MarkerWrite::AsFound,
                reset: NextReset::AsMarkerCarries,
                printed: Printed::LoadsOwnRefusal,
            }
        );
    }

    /// Row 1, a rollback after `load` is on the trace: the marker stands `Open`
    /// on the run (K1).
    #[test]
    fn row1_rollback_after_load_writes_the_marker_open() {
        assert_eq!(
            conclude(LeftKind::Rollback {
                load_on_trace: true
            }),
            Outcome {
                publish: false,
                marker: MarkerWrite::Open,
                reset: NextReset::NoCleanUnload,
                printed: Printed::LoadsOwnRefusal,
            }
        );
    }

    /// Row 2, a force over a territory that does not judge (K5): nothing
    /// published, the marker `Forced`.
    #[test]
    fn row2_unjudged_territory_closes_forced() {
        assert_eq!(
            conclude(LeftKind::TerritoryUnjudged),
            Outcome {
                publish: false,
                marker: MarkerWrite::Forced,
                reset: NextReset::ForcedUnload,
                printed: Printed::Unloaded,
            }
        );
    }

    /// Row 3, `no_state`: the run had nothing to keep, so it closes clean (I4).
    #[test]
    fn row3_no_state_closes_clean() {
        assert_eq!(
            conclude(LeftKind::NoState),
            Outcome {
                publish: false,
                marker: MarkerWrite::Closed,
                reset: NextReset::None,
                printed: Printed::Unloaded,
            }
        );
    }

    /// Row 4, forced with its save point not taken: the marker `Forced`.
    #[test]
    fn row4_forced_not_taken_closes_forced() {
        assert_eq!(
            conclude(LeftKind::ForcedNotTaken),
            Outcome {
                publish: false,
                marker: MarkerWrite::Forced,
                reset: NextReset::ForcedUnload,
                printed: Printed::Unloaded,
            }
        );
    }

    /// Row 5, a save point reported whose `event_run` is not the run the marker
    /// stands open on: published, the marker left open, whatever the
    /// publication's outcome (the key is blank in the Spec).
    #[test]
    fn row5_another_runs_save_point_publishes_and_leaves_open() {
        for published in [true, false] {
            assert_eq!(
                conclude(LeftKind::Reported {
                    this_run: false,
                    published
                }),
                Outcome {
                    publish: true,
                    marker: MarkerWrite::LeaveOpen,
                    reset: NextReset::NoCleanUnload,
                    printed: Printed::Unloaded,
                },
                "published = {published}"
            );
        }
    }

    /// Row 6, this run's save point published: the marker `Closed`.
    #[test]
    fn row6_published_closes_clean() {
        assert_eq!(
            conclude(LeftKind::Reported {
                this_run: true,
                published: true
            }),
            Outcome {
                publish: true,
                marker: MarkerWrite::Closed,
                reset: NextReset::None,
                printed: Printed::Unloaded,
            }
        );
    }

    /// Row 7, this run's save point not published: the marker left open, the
    /// caller refused `SavePointNotTaken` naming the publication.
    #[test]
    fn row7_unpublished_leaves_open_and_refuses() {
        assert_eq!(
            conclude(LeftKind::Reported {
                this_run: true,
                published: false
            }),
            Outcome {
                publish: true,
                marker: MarkerWrite::LeaveOpen,
                reset: NextReset::NoCleanUnload,
                printed: Printed::SavePointNotTakenPublished,
            }
        );
    }

    /// **The keys are tested in the Spec's order**: a rollback is row 1 even
    /// with `no_state` true, and an unjudged territory is row 2 whatever else
    /// holds.
    #[test]
    fn the_keys_are_tested_in_order() {
        assert_eq!(
            classify(Seen {
                rollback: Some(true),
                territory_judged: true,
                forced: true,
                no_state: true,
                reported: None,
            }),
            Ok(LeftKind::Rollback {
                load_on_trace: true
            })
        );
        assert_eq!(
            classify(Seen {
                rollback: None,
                territory_judged: false,
                forced: true,
                no_state: false,
                reported: Some(Reported {
                    this_run: true,
                    published: true
                }),
            }),
            Ok(LeftKind::TerritoryUnjudged)
        );
        assert_eq!(
            classify(Seen {
                rollback: None,
                territory_judged: true,
                forced: false,
                no_state: true,
                reported: None,
            }),
            Ok(LeftKind::NoState)
        );
        assert_eq!(
            classify(Seen {
                rollback: None,
                territory_judged: true,
                forced: true,
                no_state: false,
                reported: None,
            }),
            Ok(LeftKind::ForcedNotTaken)
        );
        assert_eq!(
            classify(Seen {
                rollback: None,
                territory_judged: true,
                forced: false,
                no_state: false,
                reported: Some(Reported {
                    this_run: false,
                    published: false
                }),
            }),
            Ok(LeftKind::Reported {
                this_run: false,
                published: false
            })
        );
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
        let reported = Some(Reported {
            this_run: true,
            published: true,
        });
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
}
