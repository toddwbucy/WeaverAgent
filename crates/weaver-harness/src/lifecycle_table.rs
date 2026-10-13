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

/// Where a pending leave stands among 3.0's states (`weaver-harness-Spec`
/// section 6, the graceful leave's items): draining (S4), winding down (S5),
/// lowering and taking the save point (S6), releasing (S7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeaveStep {
    Draining,
    WindingDown,
    Lowering,
    Saving,
    Releasing,
}

/// A pending leave's progress, the part of it the position is derived from:
/// how it was directed, what it is now (a join, a bound or a dead organ
/// turning a graceful one forced), and its step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LeaveProgress {
    pub directed: Leave,
    pub leave: Leave,
    pub step: LeaveStep,
}

/// An event meeting the run, one per column of the two tables but the worker's
/// own death.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "ToolReturn, BoundPasses, MemberDies and RelayDies are cells the harness meets without asking the table: the engine's interrupt, the leave's deadlines, the retired seam and the relay's own lane"
    )
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    DialerRequest,
    /// A tool return; `crossed` where the gate sent the result before the
    /// interrupt reached it (the first outcome wins).
    ToolReturn {
        crossed: bool,
    },
    /// The turn closes. `unload_interrupted` where it closes without a tool
    /// return because the unload's `ToolInterrupt` ended a call, answered
    /// `Killed { by: unload }`: the turn knows, holding its calls' outcomes,
    /// and the position does not need to (`weaver-trace-Spec`,
    /// `StopReason::Unload`).
    TurnCloses {
        unload_interrupted: bool,
    },
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
    /// S5, `turn.closed` `Stopped { reason: unload }` with the leave's cause:
    /// the unload's interrupt ended a call, so the turn did not finish, and
    /// the leave, still graceful, winds down.
    CloseStoppedUnloadToS5,
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
        (S2(_), ToolReturn { .. } | TurnCloses { .. } | BoundPasses | LegMissed) => NoCell,
        (S2(_), MemberDies) => ServeOnSeamRetired,
        (S2(_), GateDies) => GateFaultServeOn,
        (S2(Resident), SpuDies) => SpuFaultServeOnRefuseLater,
        // 3.0, under (B): an SPU already dead does not die again.
        (S2(Dead), SpuDies) => NoCell,

        (S3, DialerRequest) => QueueBehindTurn,
        (S3, ToolReturn { .. }) => DeliverToTurn,
        // No unload stands in S3, so no call of its turn is interrupted by one.
        (S3, TurnCloses { .. }) => CloseToS2,
        (S3, BoundPasses | LegMissed) => NoCell,
        (S3, MemberDies) => ServeOnSeamRetired,
        (S3, GateDies) => GateFaultToolKilledFault,
        (S3, SpuDies) => SpuFaultTurnFailed,

        (S4(..) | S5, DialerRequest) => RefuseUnloading,
        (S4(..) | S10, ToolReturn { crossed: false }) => InterruptKilledUnload,
        (S4(..) | S10, ToolReturn { crossed: true }) => DeliverToTurn,
        (
            S4(Graceful, Serving),
            TurnCloses {
                unload_interrupted: false,
            },
        ) => CloseCleanToS5,
        (
            S4(Graceful, Serving),
            TurnCloses {
                unload_interrupted: true,
            },
        ) => CloseStoppedUnloadToS5,
        // A diagnostic binding has no gate, so no call to interrupt.
        (S4(Graceful, Diagnostic), TurnCloses { .. }) => CloseCleanToS6,
        (S4(Forced, _) | S10, TurnCloses { .. }) => CancelTurnNoWindDown,
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
        // The wind-down's own calls are never sent (S5 x tool return), and
        // it closes as 3.0 says either way.
        (S5, TurnCloses { .. }) => WindDownClosesToS6,
        (S5, SpuDies) => SpuFaultWindDownFailedAndForce,

        (S6(_) | S10, DialerRequest) => RecordRefusedAtLower,
        (S6(_), ToolReturn { .. } | TurnCloses { .. } | BoundPasses) => NoCell,
        (S6(Graceful), LegMissed | MemberDies) => StopInS9,
        (S6(Forced) | S10, LegMissed | MemberDies) => ForcedMissToS7,
        (S6(_), SpuDies) => SpuFaultLegsGoOn,

        (
            S7,
            DialerRequest
            | ToolReturn { .. }
            | TurnCloses { .. }
            | BoundPasses
            | LegMissed
            | GateDies,
        ) => NoCell,
        (S7, MemberDies) => MemberUnconfirmedAtRelease,
        (S7, SpuDies) => SpuFaultBeforeUnload,

        (S9, DialerRequest) => ConnectionRefusedGateLowered,
        (S9, ToolReturn { .. } | TurnCloses { .. } | BoundPasses | LegMissed | GateDies) => NoCell,
        (S9, MemberDies) => StaysS9MemberDead,
        (S9, SpuDies) => SpuFaultRetriedReleaseUnconfirmed,

        (S10, BoundPasses) => NoCell,
        (S10, SpuDies) => SpuFaultForceGoesOn,
    }
}

/// **Ruling (A) at the leave's arrival**: a graceful leave directed with the
/// gate or the SPU dead starts forced from where it stands, the graceful
/// caller's cause kept and no `forced_by` authored.
pub fn leave_on_arrival(directed: Leave, gate_or_spu_dead: bool) -> Leave {
    match (directed, gate_or_spu_dead) {
        (Leave::Graceful, false) => Leave::Graceful,
        (Leave::Graceful, true) | (Leave::Forced, false | true) => Leave::Forced,
    }
}

#[cfg(test)]
mod tests;
