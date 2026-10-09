//! **The in-run event and organ-death tables**, `weaver-admin-Spec` section 3.0
//! ("Transitions: events inside a run" and "a gate, an SPU or a trace relay
//! dying inside a run"), as one pure decision: what this crate does when an
//! event meets the run in a position.
//!
//! The worker's own death is not here: a dead worker answers nothing, and the
//! tables' "the worker dies" column is admin's to observe (S8b). A diagnostic
//! binding needs no row of its own: it has no gate and keeps no state, so the
//! gate's events and the member's never arise in it, and its positions are the
//! ones it reaches (S4 straight to S6, S9 unreachable).

/// Whether the pending leave is still graceful or has turned forced (a join, a
/// declared bound passing, or a dead gate or SPU): S4 and S6 split on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Leave {
    Graceful,
    Forced,
}

/// The run's position, as the tables name it. S4 and S6 carry the leave's
/// state; S5 is graceful by definition, a forced leave running no wind-down;
/// S10 is a force's.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "constructed by the lifecycle plumbing, PR B")
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Position {
    S2,
    S3,
    S4(Leave),
    S5,
    S6(Leave),
    S7,
    S9,
    S10,
}

/// An event meeting the run, one per column of the two tables but the worker's
/// own death.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "constructed by the lifecycle plumbing, PR B")
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    DialerRequest,
    /// A tool return; `crossed` where the gate sent the result before the
    /// interrupt reached it (the first outcome wins).
    ToolReturn {
        crossed: bool,
    },
    TurnCloses,
    BoundPasses,
    LegMissed,
    MemberDies,
    GateDies,
    SpuDies,
    RelayDies,
}

/// **What this crate does**, one variant per distinct cell meaning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Response {
    /// A `-` cell: the event does not arise in this position.
    NoCell,
    Admit,
    QueueBehindTurn,
    /// Refused through the gate, its connection standing, recorded `Unloading`.
    RefuseUnloading,
    /// At the lower, a frame met is recorded refused and its connection closes.
    RecordRefusedAtLower,
    /// The gate is lowered: the connection is refused at connect.
    ConnectionRefusedGateLowered,
    DeliverToTurn,
    /// `ToolInterrupt`, answered `Killed { by: unload }`, re-runnable.
    InterruptKilledUnload,
    /// The wind-down's own calls are never sent, recorded interrupted.
    WindDownCallsNeverSent,
    /// S2, `turn.closed`.
    CloseToS2,
    /// S5, `turn.closed`, `Clean` where the turn finished.
    CloseCleanToS5,
    /// The turn cancelled, `Stopped { reason: unload }`, no wind-down.
    CancelTurnNoWindDown,
    /// S6, the wind-down's request and summary on the record and in state.
    WindDownClosesToS6,
    /// A declared bound passing: the leave turns forced from where it stands,
    /// no `forced_by`, the graceful cause kept, to S6.
    ForceFromHere,
    /// Ruling (A) for the gate: forced from here, skipping the quiesce and the
    /// drain's refusals, the lower meeting no answer and reaping the gate.
    ForceFromHereGateDead,
    /// The lower meets no answer: close the gate channel and reap the gate.
    LowerUnansweredReap,
    /// A graceful leave stops in S9, `SavePointNotTaken` naming the leg.
    StopInS9,
    /// A forced leave records the miss as a refusal of the leave and goes on to
    /// S7, `Left { forced: true }` with no save point.
    ForcedMissToS7,
    /// The run serves on, the state seam retired; later save points refuse
    /// `SavePointNotTaken` naming `MemberDead`.
    ServeOnSeamRetired,
    /// The leave goes on, the seam retired; its save point at S6 misses
    /// `MemberDead`.
    LeaveGoesOnSeamRetired,
    /// `release.member` reads `Unconfirmed`, no `fault` authored.
    MemberUnconfirmedAtRelease,
    /// Stays S9: a retried `unload` refuses `MemberDead` again.
    StaysS9MemberDead,
    /// A `fault` authored, the run serving on unreachable.
    GateFaultServeOn,
    /// A `fault` authored and the tool call out closed `Killed { by: fault }`.
    GateFaultToolKilledFault,
    /// A `fault` authored; later requests refused `NoResidency` (B).
    SpuFaultServeOnRefuseLater,
    /// The turn closes `Stopped { reason: fault }`, its caller answered an
    /// error, later requests refused `NoResidency` (B).
    SpuFaultTurnFailed,
    /// As `SpuFaultTurnFailed`, and the leave turns forced (A).
    SpuFaultTurnFailedAndForce,
    /// The wind-down closes `Stopped { reason: fault }` with no answer, and the
    /// leave turns forced (A).
    SpuFaultWindDownFailedAndForce,
    /// A `fault` authored; the save point's legs go on, `release.spu`
    /// `Unconfirmed`.
    SpuFaultLegsGoOn,
    /// A `fault` authored before `unload`, `release.spu` `Unconfirmed` (I9).
    SpuFaultBeforeUnload,
    /// A `fault` authored; the retried leave's release reads `Unconfirmed`.
    SpuFaultRetriedReleaseUnconfirmed,
    /// A `fault` authored; the force goes on, `release.spu` `Unconfirmed`.
    SpuFaultForceGoesOn,
    /// The worker serves on, the trace door closed until the next load.
    RelayDeathServeOn,
}

/// **The two tables, one exhaustive match and no wildcard arm**: a new position
/// or event does not compile until it has a cell.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "wired into the leave by the lifecycle plumbing, PR B"
    )
)]
pub fn respond(position: Position, event: Event) -> Response {
    use Event::*;
    use Leave::{Forced, Graceful};
    use Position::*;
    use Response::*;
    match (position, event) {
        (S2 | S3 | S4(_) | S5 | S6(_) | S7 | S9 | S10, RelayDies) => RelayDeathServeOn,

        (S2, DialerRequest) => Admit,
        (S2, ToolReturn { .. } | TurnCloses | BoundPasses | LegMissed) => NoCell,
        (S2, MemberDies) => ServeOnSeamRetired,
        (S2, GateDies) => GateFaultServeOn,
        (S2, SpuDies) => SpuFaultServeOnRefuseLater,

        (S3, DialerRequest) => QueueBehindTurn,
        (S3, ToolReturn { .. }) => DeliverToTurn,
        (S3, TurnCloses) => CloseToS2,
        (S3, BoundPasses | LegMissed) => NoCell,
        (S3, MemberDies) => ServeOnSeamRetired,
        (S3, GateDies) => GateFaultToolKilledFault,
        (S3, SpuDies) => SpuFaultTurnFailed,

        (S4(_) | S5, DialerRequest) => RefuseUnloading,
        (S4(_) | S10, ToolReturn { crossed: false }) => InterruptKilledUnload,
        (S4(_) | S10, ToolReturn { crossed: true }) => DeliverToTurn,
        (S4(Graceful), TurnCloses) => CloseCleanToS5,
        (S4(Forced) | S10, TurnCloses) => CancelTurnNoWindDown,
        (S4(Graceful) | S5, BoundPasses) => ForceFromHere,
        (S4(Forced), BoundPasses) => NoCell,
        (S4(_) | S5, LegMissed) => NoCell,
        (S4(_) | S5, MemberDies) => LeaveGoesOnSeamRetired,
        (S4(Graceful) | S5, GateDies) => ForceFromHereGateDead,
        (S4(Forced) | S6(_) | S10, GateDies) => LowerUnansweredReap,
        (S4(_), SpuDies) => SpuFaultTurnFailedAndForce,

        (S5, ToolReturn { .. }) => WindDownCallsNeverSent,
        (S5, TurnCloses) => WindDownClosesToS6,
        (S5, SpuDies) => SpuFaultWindDownFailedAndForce,

        (S6(_) | S10, DialerRequest) => RecordRefusedAtLower,
        (S6(_), ToolReturn { .. } | TurnCloses | BoundPasses) => NoCell,
        (S6(Graceful), LegMissed | MemberDies) => StopInS9,
        (S6(Forced) | S10, LegMissed | MemberDies) => ForcedMissToS7,
        (S6(_), SpuDies) => SpuFaultLegsGoOn,

        (
            S7,
            DialerRequest | ToolReturn { .. } | TurnCloses | BoundPasses | LegMissed | GateDies,
        ) => NoCell,
        (S7, MemberDies) => MemberUnconfirmedAtRelease,
        (S7, SpuDies) => SpuFaultBeforeUnload,

        (S9, DialerRequest) => ConnectionRefusedGateLowered,
        (S9, ToolReturn { .. } | TurnCloses | BoundPasses | LegMissed | GateDies) => NoCell,
        (S9, MemberDies) => StaysS9MemberDead,
        (S9, SpuDies) => SpuFaultRetriedReleaseUnconfirmed,

        (S10, BoundPasses) => NoCell,
        (S10, SpuDies) => SpuFaultForceGoesOn,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! cell {
        ($name:ident, $position:expr, $event:expr, $response:expr) => {
            #[test]
            fn $name() {
                assert_eq!(respond($position, $event), $response);
            }
        };
    }

    use Event::*;
    use Leave::{Forced, Graceful};
    use Position::*;
    use Response::*;

    const OUT: Event = ToolReturn { crossed: false };
    const CROSSED: Event = ToolReturn { crossed: true };

    // S2, serving at rest.
    cell!(s2_x_dialer_request_admits, S2, DialerRequest, Admit);
    cell!(s2_x_tool_return_has_no_cell, S2, OUT, NoCell);
    cell!(s2_x_turn_closes_has_no_cell, S2, TurnCloses, NoCell);
    cell!(s2_x_bound_passes_has_no_cell, S2, BoundPasses, NoCell);
    cell!(s2_x_leg_missed_has_no_cell, S2, LegMissed, NoCell);
    cell!(
        s2_x_member_dies_serves_on_seam_retired,
        S2,
        MemberDies,
        ServeOnSeamRetired
    );
    cell!(
        s2_x_gate_dies_serves_on_unreachable,
        S2,
        GateDies,
        GateFaultServeOn
    );
    cell!(
        s2_x_spu_dies_serves_on_refusing_later,
        S2,
        SpuDies,
        SpuFaultServeOnRefuseLater
    );
    cell!(s2_x_relay_dies_serves_on, S2, RelayDies, RelayDeathServeOn);

    // S3, serving, a turn in flight.
    cell!(
        s3_x_dialer_request_queues_behind_the_turn,
        S3,
        DialerRequest,
        QueueBehindTurn
    );
    cell!(s3_x_tool_return_delivers, S3, OUT, DeliverToTurn);
    cell!(
        s3_x_tool_return_crossed_delivers,
        S3,
        CROSSED,
        DeliverToTurn
    );
    cell!(s3_x_turn_closes_to_s2, S3, TurnCloses, CloseToS2);
    cell!(s3_x_bound_passes_has_no_cell, S3, BoundPasses, NoCell);
    cell!(s3_x_leg_missed_has_no_cell, S3, LegMissed, NoCell);
    cell!(
        s3_x_member_dies_serves_on_seam_retired,
        S3,
        MemberDies,
        ServeOnSeamRetired
    );
    cell!(
        s3_x_gate_dies_kills_the_tool_call_by_fault,
        S3,
        GateDies,
        GateFaultToolKilledFault
    );
    cell!(
        s3_x_spu_dies_fails_the_turn,
        S3,
        SpuDies,
        SpuFaultTurnFailed
    );
    cell!(s3_x_relay_dies_serves_on, S3, RelayDies, RelayDeathServeOn);

    // S4, graceful: draining.
    cell!(
        s4_graceful_x_dialer_request_refuses_unloading,
        S4(Graceful),
        DialerRequest,
        RefuseUnloading
    );
    cell!(
        s4_graceful_x_tool_return_interrupted_by_the_unload,
        S4(Graceful),
        OUT,
        InterruptKilledUnload
    );
    cell!(
        s4_graceful_x_tool_return_crossed_the_first_outcome_wins,
        S4(Graceful),
        CROSSED,
        DeliverToTurn
    );
    cell!(
        s4_graceful_x_turn_closes_clean_to_s5,
        S4(Graceful),
        TurnCloses,
        CloseCleanToS5
    );
    cell!(
        s4_graceful_x_bound_passes_forces_from_here,
        S4(Graceful),
        BoundPasses,
        ForceFromHere
    );
    cell!(
        s4_graceful_x_leg_missed_has_no_cell,
        S4(Graceful),
        LegMissed,
        NoCell
    );
    cell!(
        s4_graceful_x_member_dies_leave_goes_on,
        S4(Graceful),
        MemberDies,
        LeaveGoesOnSeamRetired
    );
    cell!(
        s4_graceful_x_gate_dies_forces_from_here_a,
        S4(Graceful),
        GateDies,
        ForceFromHereGateDead
    );
    cell!(
        s4_graceful_x_spu_dies_fails_the_turn_and_forces,
        S4(Graceful),
        SpuDies,
        SpuFaultTurnFailedAndForce
    );
    cell!(
        s4_graceful_x_relay_dies_serves_on,
        S4(Graceful),
        RelayDies,
        RelayDeathServeOn
    );

    // S4 with the leave turned forced (a join, a bound, a dead organ).
    cell!(
        s4_forced_x_dialer_request_refuses_unloading,
        S4(Forced),
        DialerRequest,
        RefuseUnloading
    );
    cell!(
        s4_forced_x_tool_return_interrupted_by_the_unload,
        S4(Forced),
        OUT,
        InterruptKilledUnload
    );
    cell!(
        s4_forced_x_tool_return_crossed_the_first_outcome_wins,
        S4(Forced),
        CROSSED,
        DeliverToTurn
    );
    cell!(
        s4_forced_x_turn_closes_cancelled_no_wind_down,
        S4(Forced),
        TurnCloses,
        CancelTurnNoWindDown
    );
    cell!(
        s4_forced_x_bound_passes_has_no_cell,
        S4(Forced),
        BoundPasses,
        NoCell
    );
    cell!(
        s4_forced_x_leg_missed_has_no_cell,
        S4(Forced),
        LegMissed,
        NoCell
    );
    cell!(
        s4_forced_x_member_dies_leave_goes_on,
        S4(Forced),
        MemberDies,
        LeaveGoesOnSeamRetired
    );
    cell!(
        s4_forced_x_gate_dies_lower_unanswered_reaps,
        S4(Forced),
        GateDies,
        LowerUnansweredReap
    );
    cell!(
        s4_forced_x_spu_dies_fails_the_turn_and_forces,
        S4(Forced),
        SpuDies,
        SpuFaultTurnFailedAndForce
    );
    cell!(
        s4_forced_x_relay_dies_serves_on,
        S4(Forced),
        RelayDies,
        RelayDeathServeOn
    );

    // S5, graceful: winding down.
    cell!(
        s5_x_dialer_request_refuses_unloading,
        S5,
        DialerRequest,
        RefuseUnloading
    );
    cell!(
        s5_x_tool_return_wind_down_calls_never_sent,
        S5,
        OUT,
        WindDownCallsNeverSent
    );
    cell!(
        s5_x_tool_return_crossed_wind_down_calls_never_sent,
        S5,
        CROSSED,
        WindDownCallsNeverSent
    );
    cell!(s5_x_turn_closes_to_s6, S5, TurnCloses, WindDownClosesToS6);
    cell!(
        s5_x_bound_passes_forces_from_here,
        S5,
        BoundPasses,
        ForceFromHere
    );
    cell!(s5_x_leg_missed_has_no_cell, S5, LegMissed, NoCell);
    cell!(
        s5_x_member_dies_leave_goes_on,
        S5,
        MemberDies,
        LeaveGoesOnSeamRetired
    );
    cell!(
        s5_x_gate_dies_forces_from_here_a,
        S5,
        GateDies,
        ForceFromHereGateDead
    );
    cell!(
        s5_x_spu_dies_fails_the_wind_down_and_forces,
        S5,
        SpuDies,
        SpuFaultWindDownFailedAndForce
    );
    cell!(s5_x_relay_dies_serves_on, S5, RelayDies, RelayDeathServeOn);

    // S6, lowering and saving, the leave graceful.
    cell!(
        s6_graceful_x_dialer_request_refused_at_the_lower,
        S6(Graceful),
        DialerRequest,
        RecordRefusedAtLower
    );
    cell!(
        s6_graceful_x_tool_return_has_no_cell,
        S6(Graceful),
        OUT,
        NoCell
    );
    cell!(
        s6_graceful_x_turn_closes_has_no_cell,
        S6(Graceful),
        TurnCloses,
        NoCell
    );
    cell!(
        s6_graceful_x_bound_passes_has_no_cell,
        S6(Graceful),
        BoundPasses,
        NoCell
    );
    cell!(
        s6_graceful_x_leg_missed_stops_in_s9,
        S6(Graceful),
        LegMissed,
        StopInS9
    );
    cell!(
        s6_graceful_x_member_dies_stops_in_s9,
        S6(Graceful),
        MemberDies,
        StopInS9
    );
    cell!(
        s6_graceful_x_gate_dies_lower_unanswered_reaps,
        S6(Graceful),
        GateDies,
        LowerUnansweredReap
    );
    cell!(
        s6_graceful_x_spu_dies_legs_go_on,
        S6(Graceful),
        SpuDies,
        SpuFaultLegsGoOn
    );
    cell!(
        s6_graceful_x_relay_dies_serves_on,
        S6(Graceful),
        RelayDies,
        RelayDeathServeOn
    );

    // S6, the leave forced.
    cell!(
        s6_forced_x_dialer_request_refused_at_the_lower,
        S6(Forced),
        DialerRequest,
        RecordRefusedAtLower
    );
    cell!(s6_forced_x_tool_return_has_no_cell, S6(Forced), OUT, NoCell);
    cell!(
        s6_forced_x_turn_closes_has_no_cell,
        S6(Forced),
        TurnCloses,
        NoCell
    );
    cell!(
        s6_forced_x_bound_passes_has_no_cell,
        S6(Forced),
        BoundPasses,
        NoCell
    );
    cell!(
        s6_forced_x_leg_missed_comes_down_forced,
        S6(Forced),
        LegMissed,
        ForcedMissToS7
    );
    cell!(
        s6_forced_x_member_dies_comes_down_forced,
        S6(Forced),
        MemberDies,
        ForcedMissToS7
    );
    cell!(
        s6_forced_x_gate_dies_lower_unanswered_reaps,
        S6(Forced),
        GateDies,
        LowerUnansweredReap
    );
    cell!(
        s6_forced_x_spu_dies_legs_go_on,
        S6(Forced),
        SpuDies,
        SpuFaultLegsGoOn
    );
    cell!(
        s6_forced_x_relay_dies_serves_on,
        S6(Forced),
        RelayDies,
        RelayDeathServeOn
    );

    // S7, leaving.
    cell!(s7_x_dialer_request_has_no_cell, S7, DialerRequest, NoCell);
    cell!(s7_x_tool_return_has_no_cell, S7, OUT, NoCell);
    cell!(s7_x_turn_closes_has_no_cell, S7, TurnCloses, NoCell);
    cell!(s7_x_bound_passes_has_no_cell, S7, BoundPasses, NoCell);
    cell!(s7_x_leg_missed_has_no_cell, S7, LegMissed, NoCell);
    cell!(
        s7_x_member_dies_release_unconfirmed,
        S7,
        MemberDies,
        MemberUnconfirmedAtRelease
    );
    cell!(s7_x_gate_dies_has_no_cell, S7, GateDies, NoCell);
    cell!(
        s7_x_spu_dies_fault_before_unload,
        S7,
        SpuDies,
        SpuFaultBeforeUnload
    );
    cell!(s7_x_relay_dies_serves_on, S7, RelayDies, RelayDeathServeOn);

    // S9, an unload stopped at its save point.
    cell!(
        s9_x_dialer_request_meets_no_listener,
        S9,
        DialerRequest,
        ConnectionRefusedGateLowered
    );
    cell!(s9_x_tool_return_has_no_cell, S9, OUT, NoCell);
    cell!(s9_x_turn_closes_has_no_cell, S9, TurnCloses, NoCell);
    cell!(s9_x_bound_passes_has_no_cell, S9, BoundPasses, NoCell);
    cell!(s9_x_leg_missed_has_no_cell, S9, LegMissed, NoCell);
    cell!(s9_x_member_dies_stays_s9, S9, MemberDies, StaysS9MemberDead);
    cell!(s9_x_gate_dies_has_no_cell, S9, GateDies, NoCell);
    cell!(
        s9_x_spu_dies_retried_release_unconfirmed,
        S9,
        SpuDies,
        SpuFaultRetriedReleaseUnconfirmed
    );
    cell!(s9_x_relay_dies_serves_on, S9, RelayDies, RelayDeathServeOn);

    // S10, forcing.
    cell!(
        s10_x_dialer_request_refused_at_the_lower,
        S10,
        DialerRequest,
        RecordRefusedAtLower
    );
    cell!(
        s10_x_tool_return_interrupted_by_the_unload,
        S10,
        OUT,
        InterruptKilledUnload
    );
    cell!(
        s10_x_tool_return_crossed_the_first_outcome_wins,
        S10,
        CROSSED,
        DeliverToTurn
    );
    cell!(
        s10_x_turn_closes_cancelled_no_wind_down,
        S10,
        TurnCloses,
        CancelTurnNoWindDown
    );
    cell!(s10_x_bound_passes_has_no_cell, S10, BoundPasses, NoCell);
    cell!(
        s10_x_leg_missed_comes_down_forced,
        S10,
        LegMissed,
        ForcedMissToS7
    );
    cell!(
        s10_x_member_dies_comes_down_forced,
        S10,
        MemberDies,
        ForcedMissToS7
    );
    cell!(
        s10_x_gate_dies_lower_unanswered_reaps,
        S10,
        GateDies,
        LowerUnansweredReap
    );
    cell!(
        s10_x_spu_dies_force_goes_on,
        S10,
        SpuDies,
        SpuFaultForceGoesOn
    );
    cell!(
        s10_x_relay_dies_serves_on,
        S10,
        RelayDies,
        RelayDeathServeOn
    );
}
