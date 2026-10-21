//! **The operator's verbs, decided**, per `weaver-admin-Spec` section 3.0's
//! verb table, its lock rule and its answer outside the envelope.
//!
//! The table is keyed by the agent's state, which admin never sees whole: it
//! sees the invocation lock (and, held, the holder's recorded verb), the run
//! lock and the worker's word to `Observe`, never the marker. So the decision
//! is two functions, each with its own table test. `class_of` folds what admin
//! saw into the classes it can tell apart, which is where two states that look
//! alike to admin (S2, S3 and S9 all answering) meet. `act` gives each verb's
//! action in each class, one case per cell or cells of 3.0 the class covers.
//!
//! Before either runs, the lock primitive has already done its part
//! (`start::take_invocation_lock`): a `show` reading beside is waited out
//! within `show`'s bound, and a holder's verb is read only once its record
//! names the holder. `show` itself takes the lock shared and sees a holder as
//! an exclusive hold refused.

/// What admin saw, in the order it looks: the invocation lock first, then the
/// run lock, then the worker's word where the run lock is held.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Seen {
    /// Another invocation holds the invocation lock exclusively, its recorded
    /// verb read.
    Holder(HolderVerb),
    /// The invocation lock is this verb's and the run lock is free.
    RunLockFree,
    /// The invocation lock is this verb's and the run lock is held.
    RunLockHeld(WorkerSeen),
}

/// The verb a holder of the invocation lock recorded, every verb but `show`
/// taking it exclusively.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HolderVerb {
    Load,
    Unload,
    ForceUnload,
    SavePoint,
    Restore,
    Stop,
    Validate,
    /// Replacement seed driver. The command surface stays inactive until cutover.
    Seed,
}

impl HolderVerb {
    /// **The verb as `admin.lock` records it**, the operator's own spelling
    /// of the verb (section 3, the lock's verb).
    pub fn spelled(self) -> &'static str {
        match self {
            HolderVerb::Load => "load",
            HolderVerb::Unload => "unload",
            HolderVerb::ForceUnload => "force-unload",
            HolderVerb::SavePoint => "save-point",
            HolderVerb::Restore => "restore",
            HolderVerb::Stop => "stop",
            HolderVerb::Validate => "validate",
            HolderVerb::Seed => "seed",
        }
    }

    /// The verb a record spells, or none for any other text.
    pub fn from_spelled(spelled: &str) -> Option<HolderVerb> {
        // Each arm spelled through `spelled()`, so the two directions
        // cannot drift; `every_exclusive_verb_records_its_name` reads every
        // variant back.
        let verb = match spelled {
            "load" => HolderVerb::Load,
            "unload" => HolderVerb::Unload,
            "force-unload" => HolderVerb::ForceUnload,
            "save-point" => HolderVerb::SavePoint,
            "restore" => HolderVerb::Restore,
            "stop" => HolderVerb::Stop,
            "validate" => HolderVerb::Validate,
            "seed" => HolderVerb::Seed,
            _ => return None,
        };
        (verb.spelled() == spelled).then_some(verb)
    }
}

/// The worker's word to `Observe`, or what stood in for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerSeen {
    Idle,
    Active,
    InTransition,
    Unloaded,
    /// No socket at connect.
    NoSocket,
    /// Accepted, and nothing answered inside the observation's bound.
    Silent,
}

/// The classes of state admin can tell apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdminView {
    /// S0, S0d, and S8b once the run lock frees: nothing stands. The marker
    /// tells them apart and is the next load's to read; what S0d asks of an
    /// unload, publishing the room, finds nothing to do in S0.
    Down,
    /// A living `unload` holds the invocation lock: S4 to S7, S8a concluding
    /// it, and S11 where the holder is that `unload`.
    HeldByUnload,
    /// A living holder of any other verb: S1, S10, S8a or S11 under a force,
    /// and the verbs outside the table holding briefly.
    HeldByOther,
    /// A worker answers at rest or with a turn: S2, S3, S9. **Admin directs
    /// and the harness tells the three apart**, each split pinned there:
    /// S3's `ActivityNotAtRest` to a save point by
    /// `lifecycle::tests::a_save_point_with_a_turn_in_flight_is_refused_not_at_rest`
    /// and `engine::tests::a_save_point_mid_turn_is_refused_not_at_rest`;
    /// `show`'s word by `lifecycle::tests::observe_answers_from_any_position_and_authors_nothing`
    /// and `engine::tests::an_observation_dialed_mid_stream_answers_active_and_disturbs_nothing`;
    /// S9's lowered gate, never lowered twice, by
    /// `lifecycle::tests::a_leave_without_its_save_point_stops_and_a_forced_leave_takes_one`.
    Serving,
    /// A worker accepts and does not answer: S2, S3 or S9, wedged.
    Wedged,
    /// The lock free and a worker answering a leave pending: its holder
    /// killed in S4 to S7 or S10, outside the envelope.
    LeavePendingUnheld,
    /// The lock free and a worker answering `Unloaded`: a run that never
    /// entered, its load killed in S1, outside the envelope.
    NeverEntered,
    /// The lock free, the run lock held and no socket: S8b, the worker gone
    /// and a member or relay still standing.
    WorkerGone,
}

/// The verbs of 3.0's verb table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verb {
    Load,
    Unload,
    ForceUnload,
    SavePoint,
    // `restore` names the save point for the next load in every view it can
    // meet, so it reads no view until the live restore, A5.
    #[cfg_attr(not(test), expect(dead_code, reason = "read by the live restore, A5"))]
    Restore,
    Show,
}

/// What a verb does, one variant per distinct cell meaning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerbAction {
    /// S1: the load's start step, publishing a dirty room as recovered.
    Start,
    RefuseInvocationInFlight,
    RefuseAgentRunning,
    /// A held run lock whose worker is silent: no load ends a run.
    RefuseUnanswered,
    RefuseOutOfOrder,
    /// Publish the room's finished save points as recovered, nothing in S0's
    /// empty room, then `Unloaded`,
    /// the marker left as it stands for the next load's reset.
    PublishRoomThenUnloaded,
    /// The graceful leave, the harness choosing S4, S5 or S9's retry.
    DirectLeave,
    /// The sole force, `Leave{forced}`: S10.
    DirectForcedLeave,
    /// Lock-free `JoinLeave`, then the joining waits of section 3.
    Join,
    /// `Leave{forced}`, and where it is refused `OutOfOrder` the escalation,
    /// writing `Forced` by the marker-write rule.
    ForcedLeaveElseEscalateForced,
    /// End what holds the run lock by the escalation, no worker listening,
    /// the marker as it stands, then the S0d path.
    EndByEscalation,
    /// The `SavePoint` directive, the harness taking it at rest or refusing.
    DirectSavePoint,
    /// Name the save point the next load restores.
    NameForNextLoad,
    /// Rung (0): `InTransition` at once, without dialing.
    ShowInTransition,
    /// Rung (i): the worker's word.
    ShowWorkersWord,
    /// Rung (ii): `Unanswered`.
    ShowUnanswered,
    /// Rungs (i) for `Unloaded` and (iii): `InTransition` with the
    /// constituents named.
    ShowInTransitionConstituents,
    /// Rung (iv): `Unloaded`, without dialing.
    ShowUnloaded,
}

/// **Folds what admin saw into the class it can tell apart**: the holder's verb
/// decides alone where one holds, since only `force-unload` acts beside a
/// holder and only an `unload`'s; the run lock says whether anything stands; the
/// worker's word where something does.
pub fn class_of(seen: Seen) -> AdminView {
    match seen {
        Seen::Holder(HolderVerb::Unload) => AdminView::HeldByUnload,
        Seen::Holder(
            HolderVerb::Load
            | HolderVerb::ForceUnload
            | HolderVerb::SavePoint
            | HolderVerb::Restore
            | HolderVerb::Stop
            | HolderVerb::Validate
            | HolderVerb::Seed,
        ) => AdminView::HeldByOther,
        Seen::RunLockFree => AdminView::Down,
        Seen::RunLockHeld(WorkerSeen::Idle | WorkerSeen::Active) => AdminView::Serving,
        Seen::RunLockHeld(WorkerSeen::Silent) => AdminView::Wedged,
        Seen::RunLockHeld(WorkerSeen::InTransition) => AdminView::LeavePendingUnheld,
        Seen::RunLockHeld(WorkerSeen::Unloaded) => AdminView::NeverEntered,
        Seen::RunLockHeld(WorkerSeen::NoSocket) => AdminView::WorkerGone,
    }
}

/// **The verb table, one exhaustive match and no wildcard arm**: a new verb
/// or class does not compile until every cell it adds has an action.
pub fn act(verb: Verb, view: AdminView) -> VerbAction {
    use AdminView::*;
    use VerbAction::*;
    match (verb, view) {
        (Verb::Show, HeldByUnload | HeldByOther) => ShowInTransition,
        (Verb::ForceUnload, HeldByUnload) => Join,
        (
            Verb::Load | Verb::Unload | Verb::SavePoint | Verb::Restore,
            HeldByUnload | HeldByOther,
        )
        | (Verb::ForceUnload, HeldByOther) => RefuseInvocationInFlight,

        // Wherever the lock is this verb's: the live restore is A5's.
        (
            Verb::Restore,
            Down | Serving | Wedged | LeavePendingUnheld | NeverEntered | WorkerGone,
        ) => NameForNextLoad,

        (Verb::Load, Down) => Start,
        (Verb::Load, Serving | LeavePendingUnheld | NeverEntered | WorkerGone) => {
            RefuseAgentRunning
        }
        (Verb::Load, Wedged) => RefuseUnanswered,

        (Verb::Unload | Verb::ForceUnload, Down) => PublishRoomThenUnloaded,
        (Verb::Unload, Serving | Wedged) => DirectLeave,
        (Verb::ForceUnload, Serving | Wedged) => DirectForcedLeave,
        (Verb::Unload, LeavePendingUnheld) => RefuseOutOfOrder,
        (Verb::ForceUnload, LeavePendingUnheld | NeverEntered) => ForcedLeaveElseEscalateForced,
        (Verb::Unload, NeverEntered | WorkerGone) | (Verb::ForceUnload, WorkerGone) => {
            EndByEscalation
        }

        // A wedged worker is S2, S3 or S9 to the verbs (3.0, a worker that
        // accepts and does not answer): the directive goes, and its bound
        // answers `Unanswered`.
        (Verb::SavePoint, Serving | Wedged) => DirectSavePoint,
        (Verb::SavePoint, Down | LeavePendingUnheld | NeverEntered | WorkerGone) => {
            RefuseOutOfOrder
        }

        (Verb::Show, Down) => ShowUnloaded,
        (Verb::Show, Serving | LeavePendingUnheld) => ShowWorkersWord,
        (Verb::Show, Wedged) => ShowUnanswered,
        (Verb::Show, NeverEntered | WorkerGone) => ShowInTransitionConstituents,
    }
}

#[cfg(test)]
mod tests;
