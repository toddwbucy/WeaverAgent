//! **The conclusion's outcome table**, `weaver-admin-Spec` section 3.0, "the
//! conclusion's outcomes", as pure decisions: what a caller answered `Left`
//! publishes, writes to the marker, leaves the next load to record, and prints.
//!
//! The table's keys are tested in the Spec's order, and the kinds are encoded so
//! that a combination the Spec lists as unreachable cannot be built: `classify`
//! refuses it instead. **Publication splits the table in two**: `conclude`
//! decides from the `Left` alone whether a publication is attempted, and where
//! one is, `after_publication` gives the outcome from whether it landed, so the
//! decision never needs the result of the act it orders. The I/O, publication
//! and the marker's write, takes each result and decides nothing (section 10,
//! the decision instruments).

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
/// marker stands open on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reported {
    pub this_run: bool,
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
    Reported { this_run: bool },
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

/// The table's output columns but the reset, which the marker decides
/// (`reset_of`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Outcome {
    pub marker: MarkerWrite,
    pub printed: Printed,
}

/// **How a reported save point is published** (S17, the operator's ruling
/// of 2026-10-14 on #140): a report is accepted only for the run the marker
/// stands open on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Publication {
    /// The room, the reported save point with its event's position and its
    /// arrival.
    Reported,
    /// The room, the reported file among the recovered: no position, ordered
    /// by the member's own count, never as the reported save point.
    Recovered,
}

/// The first half of the table: settled from the `Left` alone, or a
/// publication to attempt first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Conclusion {
    /// Nothing is published.
    Settled(Outcome),
    /// Attempt the publication, then `after_publication` with whether it
    /// landed.
    Publish {
        this_run: bool,
        publication: Publication,
    },
}

/// **The keys, tested in the Spec's order**, so every reachable `Left` matches
/// exactly one row: the rollback, the territory, `no_state`, a save point
/// reported. Each unreachable combination is refused rather than fitted to a
/// row.
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
        Some(Reported { this_run }) => Ok(LeftKind::Reported { this_run }),
    }
}

/// **The outcome table before publication, one exhaustive match and no
/// wildcard arm**: a new kind of `Left` does not compile until it has a row.
pub fn conclude(kind: LeftKind) -> Conclusion {
    let settled = |marker, printed| Conclusion::Settled(Outcome { marker, printed });
    match kind {
        LeftKind::Rollback {
            load_on_trace: false,
        } => settled(MarkerWrite::AsFound, Printed::LoadsOwnRefusal),
        LeftKind::Rollback {
            load_on_trace: true,
        } => settled(MarkerWrite::Open, Printed::LoadsOwnRefusal),
        LeftKind::TerritoryUnjudged => settled(MarkerWrite::Forced, Printed::Unloaded),
        LeftKind::NoState => settled(MarkerWrite::Closed, Printed::Unloaded),
        LeftKind::ForcedNotTaken => settled(MarkerWrite::Forced, Printed::Unloaded),
        LeftKind::Reported { this_run } => Conclusion::Publish {
            this_run,
            publication: publication_of(this_run),
        },
    }
}

/// **A report is accepted only for the open marker's run** (S17): this
/// run's is published as reported, any other's file as recovered.
pub fn publication_of(this_run: bool) -> Publication {
    if this_run {
        Publication::Reported
    } else {
        Publication::Recovered
    }
}

/// What the `save-point` verb answers (3.0, the `save-point` verb's
/// publication).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SavePointAnswer {
    /// `SavePointTaken` with the report.
    Taken,
    /// Refused `SavePointNotTaken` naming `published`.
    NotTakenPublished,
}

/// **The `save-point` verb's table, after its publication** (S17): it answers
/// the save point taken only where the report is this run's and its file
/// published as reported; a report of another run, its file published as
/// recovered, answers no save point of this run.
pub fn save_point_answer(this_run: bool, landed: bool) -> SavePointAnswer {
    match (this_run, landed) {
        (true, true) => SavePointAnswer::Taken,
        (true, false) | (false, true | false) => SavePointAnswer::NotTakenPublished,
    }
}

/// **Rows 5 to 7 after the publication**: the marker closes clean only for
/// this run's own save point, landed (section 6).
pub fn after_publication(this_run: bool, landed: bool) -> Outcome {
    let outcome = |marker, printed| Outcome { marker, printed };
    match (this_run, landed) {
        (false, true | false) => outcome(MarkerWrite::LeaveOpen, Printed::Unloaded),
        (true, true) => outcome(MarkerWrite::Closed, Printed::Unloaded),
        (true, false) => outcome(MarkerWrite::LeaveOpen, Printed::SavePointNotTakenPublished),
    }
}

/// **The reset column, derived from the marker**: the next load records the
/// cause the marker carries, so a row cannot state a reset its marker cannot
/// produce.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the reset column: the next load derives it from the marker itself"
    )
)]
pub fn reset_of(marker: MarkerWrite) -> NextReset {
    match marker {
        MarkerWrite::AsFound => NextReset::AsMarkerCarries,
        MarkerWrite::Open | MarkerWrite::LeaveOpen => NextReset::NoCleanUnload,
        MarkerWrite::Closed => NextReset::None,
        MarkerWrite::Forced => NextReset::ForcedUnload,
    }
}

#[cfg(test)]
mod tests;
