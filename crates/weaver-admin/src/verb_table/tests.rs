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

#[test]
fn plan_update_table_requires_a_serving_observation_without_an_invocation_holder() {
    for (view, expected) in [
        (V::Down, A::RefuseOutOfOrder),
        (V::HeldByUnload, A::RefuseInvocationInFlight),
        (V::HeldByOther, A::RefuseInvocationInFlight),
        (V::Serving, A::DirectPlanUpdate),
        (V::Wedged, A::RefuseUnanswered),
        (V::LeavePendingUnheld, A::RefuseOutOfOrder),
        (V::NeverEntered, A::RefuseOutOfOrder),
        (V::WorkerGone, A::RefuseOutOfOrder),
    ] {
        assert_eq!(act(Verb::PlanUpdate, view), expected, "{view:?}");
    }
}

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

class!(
    a_seed_holder_presents_held_by_other,
    Seen::Holder(H::Seed),
    V::HeldByOther
);
