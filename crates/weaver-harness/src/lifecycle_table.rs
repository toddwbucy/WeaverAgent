//! **The in-run event and organ-death tables**, `weaver-admin-Spec` section 3.0
//! ("Transitions: events inside a run" and "a gate, an SPU or a trace relay
//! dying inside a run"), as one pure decision: what this crate does when an
//! event meets the run in a position.
//!
//! The worker's own death is not here: a dead worker answers nothing, and the
//! tables' "the worker dies" column is admin's to observe (S8b). **A diagnostic
//! binding differs in one cell**: it runs no wind-down, so its graceful S4 goes
//! to S6 when the turn closes, skipping S5; S4 carries the binding for it. It has
//! no gate and keeps no state, so the gate's events, a tool return and the
//! member's never arise in it, and S9 is unreachable.
//!
//! The SPU's death is state the run carries: S2 says whether a model is
//! resident, so a later request is refused `NoResidency` (ruling (B)). A leave
//! directed after the gate or the SPU died starts forced (ruling (A)), which
//! `leave_on_arrival` decides.

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
    S2(Spu),
    S3,
    S4(Leave, Binding),
    S5,
    S6(Leave),
    S7,
    S9,
    S10,
}

/// Whether a model is resident: an SPU that died at rest leaves the run
/// standing with none (ruling (B)).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Spu {
    Resident,
    Dead,
}

/// The run's binding: a diagnostic one runs no wind-down.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Binding {
    Serving,
    Diagnostic,
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
    /// A `-` cell: nothing happens. Where the event can still arise, such as
    /// a bound's timer armed while the leave was graceful firing after a join
    /// turned it forced, it is ignored, never treated as unreachable.
    NoCell,
    Admit,
    /// Refused `NoResidency`, no model being resident (ruling (B)).
    RefuseNoResidency,
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
    /// S6, `turn.closed`, `Clean` where the turn finished: a diagnostic
    /// binding, which runs no wind-down.
    CloseCleanToS6,
    /// The turn cancelled, `Stopped { reason: unload }`, no wind-down.
    CancelTurnNoWindDown,
    /// S6, the wind-down's request and summary on the record and in state.
    WindDownClosesToS6,
    /// A declared bound passing: the leave turns forced from where it stands,
    /// no `forced_by`, the graceful cause kept, to S6.
    ForceFromHere,
    /// Ruling (A) for the gate: forced from here, skipping the quiesce and the
    /// drain's refusals, the lower meeting no answer and reaping the gate, and a
    /// tool call out still open closed `Killed { by: fault }`, never re-run.
    ForceFromHereGateDeadCallKilledFault,
    /// The lower meets no answer, the gate reaped, and a tool call out still
    /// open closed `Killed { by: fault }`, never re-run (a force under way).
    ReapGateCallKilledFault,
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
    use Binding::{Diagnostic, Serving};
    use Event::*;
    use Leave::{Forced, Graceful};
    use Position::*;
    use Response::*;
    use Spu::{Dead, Resident};
    match (position, event) {
        (S2(_) | S3 | S4(..) | S5 | S6(_) | S7 | S9 | S10, RelayDies) => RelayDeathServeOn,

        (S2(Resident), DialerRequest) => Admit,
        (S2(Dead), DialerRequest) => RefuseNoResidency,
        (S2(_), ToolReturn { .. } | TurnCloses | BoundPasses | LegMissed) => NoCell,
        (S2(_), MemberDies) => ServeOnSeamRetired,
        (S2(_), GateDies) => GateFaultServeOn,
        (S2(Resident), SpuDies) => SpuFaultServeOnRefuseLater,
        // 3.0, under (B): an SPU already dead does not die again.
        (S2(Dead), SpuDies) => NoCell,

        (S3, DialerRequest) => QueueBehindTurn,
        (S3, ToolReturn { .. }) => DeliverToTurn,
        (S3, TurnCloses) => CloseToS2,
        (S3, BoundPasses | LegMissed) => NoCell,
        (S3, MemberDies) => ServeOnSeamRetired,
        (S3, GateDies) => GateFaultToolKilledFault,
        (S3, SpuDies) => SpuFaultTurnFailed,

        (S4(..) | S5, DialerRequest) => RefuseUnloading,
        (S4(..) | S10, ToolReturn { crossed: false }) => InterruptKilledUnload,
        (S4(..) | S10, ToolReturn { crossed: true }) => DeliverToTurn,
        (S4(Graceful, Serving), TurnCloses) => CloseCleanToS5,
        (S4(Graceful, Diagnostic), TurnCloses) => CloseCleanToS6,
        (S4(Forced, _) | S10, TurnCloses) => CancelTurnNoWindDown,
        (S4(Graceful, _) | S5, BoundPasses) => ForceFromHere,
        // 3.0, under the event table: a bound passing after the leave
        // turned forced changes nothing, its late timer ignored.
        (S4(Forced, _), BoundPasses) => NoCell,
        (S4(..) | S5, LegMissed) => NoCell,
        (S4(..) | S5, MemberDies) => LeaveGoesOnSeamRetired,
        (S4(Graceful, _) | S5, GateDies) => ForceFromHereGateDeadCallKilledFault,
        (S4(Forced, _) | S10, GateDies) => ReapGateCallKilledFault,
        (S6(_), GateDies) => LowerUnansweredReap,
        (S4(..), SpuDies) => SpuFaultTurnFailedAndForce,

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

/// **Ruling (A) at the leave's arrival**: a graceful leave directed with the
/// gate or the SPU dead starts forced from where it stands, the graceful
/// caller's cause kept and no `forced_by` authored.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "wired into the leave by the lifecycle plumbing, PR B"
    )
)]
pub fn leave_on_arrival(directed: Leave, gate_or_spu_dead: bool) -> Leave {
    match (directed, gate_or_spu_dead) {
        (Leave::Graceful, false) => Leave::Graceful,
        (Leave::Graceful, true) | (Leave::Forced, false | true) => Leave::Forced,
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

    use Binding::{Diagnostic, Serving};
    use Event::*;
    use Leave::{Forced, Graceful};
    use Position::*;
    use Response::*;
    use Spu::{Dead, Resident};

    const OUT: Event = ToolReturn { crossed: false };
    const CROSSED: Event = ToolReturn { crossed: true };

    // S2(Resident), serving at rest.
    cell!(
        s2_x_dialer_request_admits,
        S2(Resident),
        DialerRequest,
        Admit
    );
    cell!(s2_x_tool_return_has_no_cell, S2(Resident), OUT, NoCell);
    cell!(
        s2_x_turn_closes_has_no_cell,
        S2(Resident),
        TurnCloses,
        NoCell
    );
    cell!(
        s2_x_bound_passes_has_no_cell,
        S2(Resident),
        BoundPasses,
        NoCell
    );
    cell!(s2_x_leg_missed_has_no_cell, S2(Resident), LegMissed, NoCell);
    cell!(
        s2_x_member_dies_serves_on_seam_retired,
        S2(Resident),
        MemberDies,
        ServeOnSeamRetired
    );
    cell!(
        s2_x_gate_dies_serves_on_unreachable,
        S2(Resident),
        GateDies,
        GateFaultServeOn
    );
    cell!(
        s2_x_spu_dies_serves_on_refusing_later,
        S2(Resident),
        SpuDies,
        SpuFaultServeOnRefuseLater
    );
    cell!(
        s2_x_relay_dies_serves_on,
        S2(Resident),
        RelayDies,
        RelayDeathServeOn
    );

    // S2 with the SPU dead (B): later requests refused, the run standing.
    cell!(
        s2_spu_dead_x_dialer_request_refuses_no_residency,
        S2(Dead),
        DialerRequest,
        RefuseNoResidency
    );
    cell!(s2_spu_dead_x_tool_return_has_no_cell, S2(Dead), OUT, NoCell);
    cell!(
        s2_spu_dead_x_turn_closes_has_no_cell,
        S2(Dead),
        TurnCloses,
        NoCell
    );
    cell!(
        s2_spu_dead_x_bound_passes_has_no_cell,
        S2(Dead),
        BoundPasses,
        NoCell
    );
    cell!(
        s2_spu_dead_x_leg_missed_has_no_cell,
        S2(Dead),
        LegMissed,
        NoCell
    );
    cell!(
        s2_spu_dead_x_member_dies_serves_on_seam_retired,
        S2(Dead),
        MemberDies,
        ServeOnSeamRetired
    );
    cell!(
        s2_spu_dead_x_gate_dies_serves_on_unreachable,
        S2(Dead),
        GateDies,
        GateFaultServeOn
    );
    cell!(
        s2_spu_dead_x_spu_dies_has_no_cell,
        S2(Dead),
        SpuDies,
        NoCell
    );
    cell!(
        s2_spu_dead_x_relay_dies_serves_on,
        S2(Dead),
        RelayDies,
        RelayDeathServeOn
    );

    // S4 on a diagnostic binding: no wind-down, so straight to S6.
    cell!(
        s4_graceful_diagnostic_x_turn_closes_clean_to_s6,
        S4(Graceful, Diagnostic),
        TurnCloses,
        CloseCleanToS6
    );
    cell!(
        s4_forced_diagnostic_x_turn_closes_cancelled_no_wind_down,
        S4(Forced, Diagnostic),
        TurnCloses,
        CancelTurnNoWindDown
    );

    /// **(A): a leave directed with the gate or the SPU dead starts forced**,
    /// the graceful caller's cause kept; a forced one stays forced.
    #[test]
    fn a_leave_arriving_with_a_dead_organ_starts_forced() {
        assert_eq!(leave_on_arrival(Graceful, false), Graceful);
        assert_eq!(leave_on_arrival(Graceful, true), Forced);
        assert_eq!(leave_on_arrival(Forced, false), Forced);
        assert_eq!(leave_on_arrival(Forced, true), Forced);
    }

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
        S4(Graceful, Serving),
        DialerRequest,
        RefuseUnloading
    );
    cell!(
        s4_graceful_x_tool_return_interrupted_by_the_unload,
        S4(Graceful, Serving),
        OUT,
        InterruptKilledUnload
    );
    cell!(
        s4_graceful_x_tool_return_crossed_the_first_outcome_wins,
        S4(Graceful, Serving),
        CROSSED,
        DeliverToTurn
    );
    cell!(
        s4_graceful_x_turn_closes_clean_to_s5,
        S4(Graceful, Serving),
        TurnCloses,
        CloseCleanToS5
    );
    cell!(
        s4_graceful_x_bound_passes_forces_from_here,
        S4(Graceful, Serving),
        BoundPasses,
        ForceFromHere
    );
    cell!(
        s4_graceful_x_leg_missed_has_no_cell,
        S4(Graceful, Serving),
        LegMissed,
        NoCell
    );
    cell!(
        s4_graceful_x_member_dies_leave_goes_on,
        S4(Graceful, Serving),
        MemberDies,
        LeaveGoesOnSeamRetired
    );
    cell!(
        s4_graceful_x_gate_dies_forces_from_here_a_killing_the_call_by_fault,
        S4(Graceful, Serving),
        GateDies,
        ForceFromHereGateDeadCallKilledFault
    );
    cell!(
        s4_graceful_x_spu_dies_fails_the_turn_and_forces,
        S4(Graceful, Serving),
        SpuDies,
        SpuFaultTurnFailedAndForce
    );
    cell!(
        s4_graceful_x_relay_dies_serves_on,
        S4(Graceful, Serving),
        RelayDies,
        RelayDeathServeOn
    );

    // S4 with the leave turned forced (a join, a bound, a dead organ).
    cell!(
        s4_forced_x_dialer_request_refuses_unloading,
        S4(Forced, Serving),
        DialerRequest,
        RefuseUnloading
    );
    cell!(
        s4_forced_x_tool_return_interrupted_by_the_unload,
        S4(Forced, Serving),
        OUT,
        InterruptKilledUnload
    );
    cell!(
        s4_forced_x_tool_return_crossed_the_first_outcome_wins,
        S4(Forced, Serving),
        CROSSED,
        DeliverToTurn
    );
    cell!(
        s4_forced_x_turn_closes_cancelled_no_wind_down,
        S4(Forced, Serving),
        TurnCloses,
        CancelTurnNoWindDown
    );
    cell!(
        s4_forced_x_bound_passes_has_no_cell,
        S4(Forced, Serving),
        BoundPasses,
        NoCell
    );
    cell!(
        s4_forced_x_leg_missed_has_no_cell,
        S4(Forced, Serving),
        LegMissed,
        NoCell
    );
    cell!(
        s4_forced_x_member_dies_leave_goes_on,
        S4(Forced, Serving),
        MemberDies,
        LeaveGoesOnSeamRetired
    );
    cell!(
        s4_forced_x_gate_dies_reaps_killing_the_call_by_fault,
        S4(Forced, Serving),
        GateDies,
        ReapGateCallKilledFault
    );
    cell!(
        s4_forced_x_spu_dies_fails_the_turn_and_forces,
        S4(Forced, Serving),
        SpuDies,
        SpuFaultTurnFailedAndForce
    );
    cell!(
        s4_forced_x_relay_dies_serves_on,
        S4(Forced, Serving),
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
        s5_x_gate_dies_forces_from_here_a_killing_the_call_by_fault,
        S5,
        GateDies,
        ForceFromHereGateDeadCallKilledFault
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
        s10_x_gate_dies_reaps_killing_the_call_by_fault,
        S10,
        GateDies,
        ReapGateCallKilledFault
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
