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

// Uncalled until the plumbing wires it, the whole table at once.
#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "wired into the verbs by the lifecycle plumbing, PR B"
    )
)]

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
    /// A worker answers at rest or with a turn: S2, S3, S9.
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
            | HolderVerb::Validate,
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
mod tests {
    use super::*;

    macro_rules! class {
        ($name:ident, $seen:expr, $view:expr) => {
            #[test]
            fn $name() {
                assert_eq!(class_of($seen), $view);
            }
        };
    }

    macro_rules! cell {
        ($name:ident, $verb:expr, $view:expr, $action:expr) => {
            #[test]
            fn $name() {
                assert_eq!(act($verb, $view), $action);
            }
        };
    }

    use AdminView as V;
    use HolderVerb as H;
    use VerbAction as A;

    // **The state-to-class mapping**: each state's presentation to admin.
    class!(
        s0_s0d_and_s8b_freed_present_down,
        Seen::RunLockFree,
        V::Down
    );
    class!(
        s1_presents_held_by_other,
        Seen::Holder(H::Load),
        V::HeldByOther
    );
    class!(
        s2_presents_serving,
        Seen::RunLockHeld(WorkerSeen::Idle),
        V::Serving
    );
    class!(
        s3_presents_serving,
        Seen::RunLockHeld(WorkerSeen::Active),
        V::Serving
    );
    class!(
        s4_to_s7_present_held_by_unload,
        Seen::Holder(H::Unload),
        V::HeldByUnload
    );
    class!(
        s8a_under_a_force_presents_held_by_other,
        Seen::Holder(H::ForceUnload),
        V::HeldByOther
    );
    class!(
        s8b_with_the_run_lock_held_presents_worker_gone,
        Seen::RunLockHeld(WorkerSeen::NoSocket),
        V::WorkerGone
    );
    class!(
        s10_presents_held_by_other,
        Seen::Holder(H::ForceUnload),
        V::HeldByOther
    );
    class!(
        a_brief_holder_presents_held_by_other_save_point,
        Seen::Holder(H::SavePoint),
        V::HeldByOther
    );
    class!(
        a_brief_holder_presents_held_by_other_restore,
        Seen::Holder(H::Restore),
        V::HeldByOther
    );
    class!(
        a_brief_holder_presents_held_by_other_stop,
        Seen::Holder(H::Stop),
        V::HeldByOther
    );
    class!(
        a_brief_holder_presents_held_by_other_validate,
        Seen::Holder(H::Validate),
        V::HeldByOther
    );
    class!(
        a_wedged_worker_presents_wedged,
        Seen::RunLockHeld(WorkerSeen::Silent),
        V::Wedged
    );
    class!(
        a_killed_leave_holder_presents_leave_pending_unheld,
        Seen::RunLockHeld(WorkerSeen::InTransition),
        V::LeavePendingUnheld
    );
    class!(
        a_killed_load_presents_never_entered,
        Seen::RunLockHeld(WorkerSeen::Unloaded),
        V::NeverEntered
    );

    // S0 and S0d, the room published where it holds anything.
    cell!(s0_s0d_x_load_starts, Verb::Load, V::Down, A::Start);
    cell!(
        s0_s0d_x_unload_publishes_the_room_then_unloaded,
        Verb::Unload,
        V::Down,
        A::PublishRoomThenUnloaded
    );
    cell!(
        s0_s0d_x_force_unload_publishes_the_room_then_unloaded,
        Verb::ForceUnload,
        V::Down,
        A::PublishRoomThenUnloaded
    );
    cell!(
        s0_s0d_x_save_point_refuses_out_of_order,
        Verb::SavePoint,
        V::Down,
        A::RefuseOutOfOrder
    );
    cell!(
        s0_s0d_x_restore_names_for_the_next_load,
        Verb::Restore,
        V::Down,
        A::NameForNextLoad
    );
    cell!(
        s0_s0d_x_show_answers_unloaded,
        Verb::Show,
        V::Down,
        A::ShowUnloaded
    );

    // S4, S5, S6, S7 and S8a under an `unload`.
    cell!(
        s4_s7_s8a_x_load_refuses_invocation_in_flight,
        Verb::Load,
        V::HeldByUnload,
        A::RefuseInvocationInFlight
    );
    cell!(
        s4_s7_s8a_x_unload_refuses_invocation_in_flight,
        Verb::Unload,
        V::HeldByUnload,
        A::RefuseInvocationInFlight
    );
    cell!(
        s4_s7_s8a_x_force_unload_joins,
        Verb::ForceUnload,
        V::HeldByUnload,
        A::Join
    );
    cell!(
        s4_s7_s8a_x_save_point_refuses_invocation_in_flight,
        Verb::SavePoint,
        V::HeldByUnload,
        A::RefuseInvocationInFlight
    );
    cell!(
        s4_s7_s8a_x_restore_refuses_invocation_in_flight,
        Verb::Restore,
        V::HeldByUnload,
        A::RefuseInvocationInFlight
    );
    cell!(
        s4_s7_s8a_x_show_answers_in_transition,
        Verb::Show,
        V::HeldByUnload,
        A::ShowInTransition
    );

    // S1, S10, and S8a or S11 under a force.
    cell!(
        s1_s10_s11_x_load_refuses_invocation_in_flight,
        Verb::Load,
        V::HeldByOther,
        A::RefuseInvocationInFlight
    );
    cell!(
        s1_s10_s11_x_unload_refuses_invocation_in_flight,
        Verb::Unload,
        V::HeldByOther,
        A::RefuseInvocationInFlight
    );
    cell!(
        s1_s10_s11_x_force_unload_refuses_invocation_in_flight,
        Verb::ForceUnload,
        V::HeldByOther,
        A::RefuseInvocationInFlight
    );
    cell!(
        s1_s10_s11_x_save_point_refuses_invocation_in_flight,
        Verb::SavePoint,
        V::HeldByOther,
        A::RefuseInvocationInFlight
    );
    cell!(
        s1_s10_s11_x_restore_refuses_invocation_in_flight,
        Verb::Restore,
        V::HeldByOther,
        A::RefuseInvocationInFlight
    );
    cell!(
        s1_s10_s11_x_show_answers_in_transition,
        Verb::Show,
        V::HeldByOther,
        A::ShowInTransition
    );

    // S2, S3, S9.
    cell!(
        s2_s3_s9_x_load_refuses_agent_running,
        Verb::Load,
        V::Serving,
        A::RefuseAgentRunning
    );
    cell!(
        s2_s3_s9_x_unload_directs_the_leave,
        Verb::Unload,
        V::Serving,
        A::DirectLeave
    );
    cell!(
        s2_s3_s9_x_force_unload_directs_the_sole_force,
        Verb::ForceUnload,
        V::Serving,
        A::DirectForcedLeave
    );
    cell!(
        s2_s3_s9_x_save_point_directs_it,
        Verb::SavePoint,
        V::Serving,
        A::DirectSavePoint
    );
    cell!(
        s2_s3_s9_x_restore_names_for_the_next_load,
        Verb::Restore,
        V::Serving,
        A::NameForNextLoad
    );
    cell!(
        s2_s3_s9_x_show_answers_the_workers_word,
        Verb::Show,
        V::Serving,
        A::ShowWorkersWord
    );

    // S2, S3, S9 with the worker wedged.
    cell!(
        wedged_x_load_refuses_unanswered,
        Verb::Load,
        V::Wedged,
        A::RefuseUnanswered
    );
    cell!(
        wedged_x_unload_directs_the_leave,
        Verb::Unload,
        V::Wedged,
        A::DirectLeave
    );
    cell!(
        wedged_x_force_unload_directs_the_sole_force,
        Verb::ForceUnload,
        V::Wedged,
        A::DirectForcedLeave
    );
    cell!(
        wedged_x_save_point_directs_it,
        Verb::SavePoint,
        V::Wedged,
        A::DirectSavePoint
    );
    cell!(
        wedged_x_restore_names_for_the_next_load,
        Verb::Restore,
        V::Wedged,
        A::NameForNextLoad
    );
    cell!(
        wedged_x_show_answers_unanswered,
        Verb::Show,
        V::Wedged,
        A::ShowUnanswered
    );

    // Outside the envelope: a leave pending, its holder killed.
    cell!(
        outside_leave_pending_x_load_refuses_agent_running,
        Verb::Load,
        V::LeavePendingUnheld,
        A::RefuseAgentRunning
    );
    cell!(
        outside_leave_pending_x_unload_refuses_out_of_order,
        Verb::Unload,
        V::LeavePendingUnheld,
        A::RefuseOutOfOrder
    );
    cell!(
        outside_leave_pending_x_force_unload_forces_else_escalates,
        Verb::ForceUnload,
        V::LeavePendingUnheld,
        A::ForcedLeaveElseEscalateForced
    );
    cell!(
        outside_leave_pending_x_save_point_refuses_out_of_order,
        Verb::SavePoint,
        V::LeavePendingUnheld,
        A::RefuseOutOfOrder
    );
    cell!(
        outside_leave_pending_x_restore_names_for_the_next_load,
        Verb::Restore,
        V::LeavePendingUnheld,
        A::NameForNextLoad
    );
    cell!(
        outside_leave_pending_x_show_answers_the_workers_word,
        Verb::Show,
        V::LeavePendingUnheld,
        A::ShowWorkersWord
    );

    // Outside the envelope: a run never entered, its load killed.
    cell!(
        outside_never_entered_x_load_refuses_agent_running,
        Verb::Load,
        V::NeverEntered,
        A::RefuseAgentRunning
    );
    cell!(
        outside_never_entered_x_unload_ends_by_escalation,
        Verb::Unload,
        V::NeverEntered,
        A::EndByEscalation
    );
    cell!(
        outside_never_entered_x_force_unload_forces_else_escalates,
        Verb::ForceUnload,
        V::NeverEntered,
        A::ForcedLeaveElseEscalateForced
    );
    cell!(
        outside_never_entered_x_save_point_refuses_out_of_order,
        Verb::SavePoint,
        V::NeverEntered,
        A::RefuseOutOfOrder
    );
    cell!(
        outside_never_entered_x_restore_names_for_the_next_load,
        Verb::Restore,
        V::NeverEntered,
        A::NameForNextLoad
    );
    cell!(
        outside_never_entered_x_show_names_the_constituents,
        Verb::Show,
        V::NeverEntered,
        A::ShowInTransitionConstituents
    );

    // S8b, the run lock held.
    cell!(
        s8b_x_load_refuses_agent_running,
        Verb::Load,
        V::WorkerGone,
        A::RefuseAgentRunning
    );
    cell!(
        s8b_x_unload_ends_by_escalation,
        Verb::Unload,
        V::WorkerGone,
        A::EndByEscalation
    );
    cell!(
        s8b_x_force_unload_ends_by_escalation,
        Verb::ForceUnload,
        V::WorkerGone,
        A::EndByEscalation
    );
    cell!(
        s8b_x_save_point_refuses_out_of_order,
        Verb::SavePoint,
        V::WorkerGone,
        A::RefuseOutOfOrder
    );
    cell!(
        s8b_x_restore_names_for_the_next_load,
        Verb::Restore,
        V::WorkerGone,
        A::NameForNextLoad
    );
    cell!(
        s8b_x_show_names_the_constituents,
        Verb::Show,
        V::WorkerGone,
        A::ShowInTransitionConstituents
    );
}
