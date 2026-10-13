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
const CLOSES: Event = TurnCloses {
    unload_interrupted: false,
};
/// The turn closes without a tool return the unload's interrupt ended.
const INTERRUPTED: Event = TurnCloses {
    unload_interrupted: true,
};

// S2(Resident), serving at rest.
cell!(
    s2_x_dialer_request_admits,
    S2(Resident),
    DialerRequest,
    Admit
);
cell!(s2_x_tool_return_has_no_cell, S2(Resident), OUT, NoCell);
cell!(s2_x_turn_closes_has_no_cell, S2(Resident), CLOSES, NoCell);
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
    CLOSES,
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

// **A turn the unload ended closes stopped**, its tool call interrupted
// (weaver-trace-Spec, `StopReason::Unload`; 3.0 S4 x tool return): the
// leave stays graceful and goes on to the wind-down. A forced leave or a
// force cancels the turn either way, and the wind-down closes as it does.
cell!(
    s4_x_turn_closes_after_an_interrupt_stops_unload,
    S4(Graceful, Serving),
    INTERRUPTED,
    CloseStoppedUnloadToS5
);
cell!(
    s4_forced_x_turn_closes_after_an_interrupt_is_cancelled,
    S4(Forced, Serving),
    INTERRUPTED,
    CancelTurnNoWindDown
);
cell!(
    s10_x_turn_closes_after_an_interrupt_is_cancelled,
    S10,
    INTERRUPTED,
    CancelTurnNoWindDown
);
cell!(
    s5_x_wind_down_closes_after_a_call_never_sent_to_s6,
    S5,
    INTERRUPTED,
    WindDownClosesToS6
);

// S4 on a diagnostic binding: no wind-down, so straight to S6.
cell!(
    s4_graceful_diagnostic_x_turn_closes_clean_to_s6,
    S4(Graceful, Diagnostic),
    CLOSES,
    CloseCleanToS6
);
cell!(
    s4_forced_diagnostic_x_turn_closes_cancelled_no_wind_down,
    S4(Forced, Diagnostic),
    CLOSES,
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
cell!(s3_x_turn_closes_to_s2, S3, CLOSES, CloseToS2);
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
    CLOSES,
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
    CLOSES,
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
cell!(s5_x_turn_closes_to_s6, S5, CLOSES, WindDownClosesToS6);
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
    CLOSES,
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
    CLOSES,
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
cell!(s7_x_turn_closes_has_no_cell, S7, CLOSES, NoCell);
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
cell!(s9_x_turn_closes_has_no_cell, S9, CLOSES, NoCell);
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
    CLOSES,
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
