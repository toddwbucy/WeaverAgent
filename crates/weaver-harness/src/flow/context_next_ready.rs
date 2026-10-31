//! Spec 07 PLAN-OBSERVATION's next_ready interval. It begins when a closure
//! receipt is consumed and ends at the successor placement's Recorded receipt,
//! both read from this run's monotonic clock. The port checks the receipts
//! against each other and the committed revision. It starts no progression and
//! never bridges a replacement, a reload or a missing endpoint into an interval.
use super::*;
use crate::flow::author::AuthoredSource;
use crate::flow::context::plan_port_driver::{ActivationReceipt, ClosureReceipt};
use crate::flow::context::worker_startup::input::generation::step_instruction::StepPlacementReceipt;
use crate::flow::mirror;
use weaver_types::state_flow as s;

/// The consumed closure receipt and the one clock reading taken as it was
/// consumed. Only begin_next_ready creates it; a retry reuses it unchanged.
#[derive(Debug)]
pub(in crate::flow) struct NextReadyStart {
    pub(in crate::flow) receipt: ClosureReceipt,
    pub(in crate::flow) origin: std::sync::Arc<()>,
    pub(in crate::flow) began_ns: u64,
}

/// Why the two receipts do not bound one interval. Nothing is authored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum NextReadyRefusal {
    /// A receipt or the start belongs to another run, and so another clock.
    /// A save-point reload enters a new run and refuses here.
    CrossRun,
    /// The committed revision is not the one close and placement shared.
    Replaced,
    /// The committed revision no longer holds the closure, or no longer holds
    /// the activated step active over its activated span.
    MissingEndpoint,
    /// The activation did not follow this closure's step, or the placement
    /// does not carry the activated step's filed instruction unit.
    NotTheSuccessor,
    /// A reading is zero or reversed, or the chain's sources are not one
    /// run's in order: closure, activation, step instruction, placement.
    ClockOrder,
    /// The placement's turn is no longer the open turn: a measurement would
    /// land after that turn's turn.closed.
    TurnClosed,
    /// The committed summary is settling. Nothing is decided; retry.
    Pending,
}

impl RecordingContext {
    /// Consume the closure receipt into the successor progression, reading
    /// began_ns once. A foreign receipt is returned unconsumed.
    #[allow(clippy::result_large_err)] // Refusal returns the exact receipt.
    pub(in crate::flow) fn begin_next_ready(
        &mut self,
        receipt: ClosureReceipt,
        io: &mut impl PumpIo,
    ) -> Result<NextReadyStart, (ClosureReceipt, Option<RunFailure>)> {
        if let Err(failure) = self.live(io) {
            return Err((receipt, Some(failure)));
        }
        if !receipt.belongs_to(self) {
            return Err((receipt, None));
        }
        match io.now() {
            Ok(began_ns) if began_ns > 0 => Ok(NextReadyStart {
                receipt,
                origin: std::sync::Arc::clone(&self.read_origin),
                began_ns,
            }),
            Ok(_) => Err((receipt, None)),
            Err(error) => {
                let failure = self.fail(
                    io,
                    FlowHaltReason::TraceFailed,
                    HaltPoint::Author,
                    format!("next_ready clock: {error}"),
                );
                Err((receipt, Some(failure)))
            }
        }
    }

    /// Check the identity chain closure -> activation of the successor ->
    /// its step loop.instruction -> its placement, against this run and the
    /// committed revision, then build the unauthored measurement. The
    /// successor is the step the activation's checked step_successor result
    /// named, never the snapshot's next step. Its causal parent is the placement.
    pub(in crate::flow) fn next_ready_event(
        &mut self,
        start: &NextReadyStart,
        activation: &ActivationReceipt,
        placed: &StepPlacementReceipt,
    ) -> Result<AuthoredEvent, NextReadyRefusal> {
        use NextReadyRefusal as R;
        let receipt = &start.receipt;
        if !std::sync::Arc::ptr_eq(&start.origin, &self.read_origin)
            || !std::sync::Arc::ptr_eq(&placed.origin, &self.read_origin)
            || !receipt.belongs_to(self)
            || !activation.belongs_to(self)
            || !placed.unit.belongs_to(self)
        {
            return Err(R::CrossRun);
        }
        if self.open_turn.as_ref() != Some(&placed.meta.turn) {
            return Err(R::TurnClosed);
        }
        if placed.plan() != receipt.plan() || activation.plan() != receipt.plan() {
            return Err(R::Replaced);
        }
        let plan = self.committed_revision()?;
        if &plan.plan != receipt.plan() || plan.status != s::PlanStatus::Locked {
            return Err(R::Replaced);
        }
        // One run's sources, in chain order: closure, successor query and
        // activation, filed step instruction, placement.
        let closed = receipt.closed();
        let activated = activation.activated();
        if start.began_ns == 0
            || placed.ended_ns < start.began_ns
            || activated.run != closed.run
            || placed.source.run != closed.run
            || activation.successor_sequence().0 <= closed.sequence.0
            || activated.sequence.0 <= closed.sequence.0
            || placed.source.sequence.0 <= activated.sequence.0
            || placed.placement.0 <= placed.source.sequence.0
        {
            return Err(R::ClockOrder);
        }
        let successor = activation.step();
        let held = |id: &s::StepId| plan.steps.iter().find(|step| &step.step_id == id);
        let Some(closed_step) = held(receipt.previous_step()).filter(|step| {
            step.status == s::StepStatus::Closed && step.closure_source.as_ref() == Some(closed)
        }) else {
            return Err(R::MissingEndpoint);
        };
        if !held(&successor.step_id).is_some_and(|step| {
            step.status == s::StepStatus::Active
                && step.span.is_some()
                && step.span == successor.span
        }) {
            return Err(R::MissingEndpoint);
        }
        // The activation follows this closure: its successor comes after the
        // closed step, and every step between them holds a closure that
        // predates this one (a preserved closure), so no later closure in this
        // run intervened. Its query names the successor's own predecessor.
        let between_preserved = plan.steps.iter().all(|step| {
            step.ordinal <= closed_step.ordinal
                || step.ordinal >= successor.ordinal
                || (step.status == s::StepStatus::Closed
                    && step.closure_source.as_ref().is_some_and(|source| {
                        source.run != closed.run || source.sequence.0 < closed.sequence.0
                    }))
        });
        let predecessor = successor.ordinal.0.checked_sub(1).and_then(|ordinal| {
            plan.steps
                .iter()
                .find(|step| step.ordinal.0 == ordinal)
                .map(|step| &step.step_id)
        });
        let query = activation.successor();
        if successor.ordinal <= closed_step.ordinal
            || !between_preserved
            || query.previous_step.as_ref() != predecessor
            || query.step_id != successor.step_id
            || placed.step_id() != &successor.step_id
            || placed.instruction() != &successor.instruction
            || Some(placed.span()) != successor.span.as_ref()
            || !crate::flow::material::is_instruction_unit(&placed.unit, &placed.source, self)
        {
            return Err(R::NotTheSuccessor);
        }
        let payload = mirror::plan_measurement(&s::PlanMeasurement {
            instrument_version: "plan-observation/1".into(),
            plan: receipt.plan().clone(),
            event: s::PlanMeasureEvent::NextReady {
                closed: closed.clone(),
                previous_step: receipt.previous_step().clone(),
                step_id: successor.step_id.clone(),
                assembly_id: placed.assembly_id(),
                began_ns: start.began_ns.into(),
                ended_ns: placed.ended_ns.into(),
                closure_to_next_ns: (placed.ended_ns - start.began_ns).into(),
            },
        });
        Ok(AuthoredEvent {
            kind: Kind::StatePlanMeasurement,
            subsystem: weaver_trace::Subsystem::Harness,
            turn: Some(placed.meta.turn.clone()),
            source: AuthoredSource::Loop,
            causal_parent: Some(placed.placement),
            payload: Some(Payload::PlanMeasurement(Box::new(payload))),
        })
    }

    /// The committed cache's current plan. A settling summary is Pending, not
    /// a replacement; no committed revision at all is Replaced.
    fn committed_revision(&mut self) -> Result<s::PlanSnapshot, NextReadyRefusal> {
        match self.committed_plan_summary() {
            Ok(Some(_)) => {}
            Err(PlanSummaryUnavailable::Pending) => return Err(NextReadyRefusal::Pending),
            Ok(None) | Err(_) => return Err(NextReadyRefusal::Replaced),
        }
        match self
            .plan_cache
            .committed
            .as_ref()
            .map(|cache| cache.read().result())
        {
            Some(s::QueryResult::PlanCurrent {
                current: Some(plan),
                ..
            }) => Ok(plan.clone()),
            _ => Err(NextReadyRefusal::Replaced),
        }
    }
}
