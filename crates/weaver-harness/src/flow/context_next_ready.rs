//! Spec 07 PLAN-OBSERVATION's next_ready interval. It begins when a closure
//! receipt is consumed and ends at the successor placement's Recorded receipt,
//! both read from this run's monotonic clock. The port checks the receipts
//! against each other and the committed revision. It starts no progression and
//! never bridges a replacement, a reload or a missing endpoint into an interval.
use super::*;
use crate::flow::author::AuthoredSource;
use crate::flow::context::plan_port_driver::ClosureReceipt;
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
    /// The committed revision no longer holds the closure, or the placed step
    /// is not the active successor of the closed step.
    MissingEndpoint,
    /// The placement does not carry the successor's filed instruction unit.
    NotTheSuccessor,
    /// The readings are out of order, zero, or the placement precedes closure.
    ClockOrder,
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

    /// Check both receipts against this run and the committed revision, then
    /// build the unauthored measurement. Its causal parent is the placement.
    pub(in crate::flow) fn next_ready_event(
        &mut self,
        start: &NextReadyStart,
        placed: &StepPlacementReceipt,
    ) -> Result<AuthoredEvent, NextReadyRefusal> {
        use NextReadyRefusal as R;
        let receipt = &start.receipt;
        if !std::sync::Arc::ptr_eq(&start.origin, &self.read_origin)
            || !std::sync::Arc::ptr_eq(&placed.origin, &self.read_origin)
            || !receipt.belongs_to(self)
            || !placed.unit.belongs_to(self)
        {
            return Err(R::CrossRun);
        }
        if placed.plan() != receipt.plan() {
            return Err(R::Replaced);
        }
        let plan = self.committed_revision().ok_or(R::Replaced)?;
        if &plan.plan != receipt.plan() || plan.status != s::PlanStatus::Locked {
            return Err(R::Replaced);
        }
        let closed = plan
            .steps
            .iter()
            .position(|step| &step.step_id == receipt.previous_step())
            .filter(|&at| {
                let step = &plan.steps[at];
                step.status == s::StepStatus::Closed
                    && step.closure_source.as_ref() == Some(receipt.closed())
            })
            .ok_or(R::MissingEndpoint)?;
        let successor = plan.steps.get(closed + 1).ok_or(R::MissingEndpoint)?;
        if successor.status != s::StepStatus::Active || successor.span.is_none() {
            return Err(R::MissingEndpoint);
        }
        if placed.step_id() != &successor.step_id
            || placed.instruction() != &successor.instruction
            || Some(placed.span()) != successor.span.as_ref()
            || !crate::flow::material::is_instruction_unit(&placed.unit, &placed.source, self)
        {
            return Err(R::NotTheSuccessor);
        }
        let closed_at = receipt.closed().sequence.0;
        if start.began_ns == 0
            || placed.ended_ns < start.began_ns
            || placed.source.run != receipt.closed().run
            || placed.source.sequence.0 <= closed_at
            || placed.placement.0 <= placed.source.sequence.0
        {
            return Err(R::ClockOrder);
        }
        let payload = mirror::plan_measurement(&s::PlanMeasurement {
            instrument_version: "plan-observation/1".into(),
            plan: receipt.plan().clone(),
            event: s::PlanMeasureEvent::NextReady {
                closed: receipt.closed().clone(),
                previous_step: receipt.previous_step().clone(),
                step_id: placed.step_id().clone(),
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

    /// The committed cache's current locked plan, if it holds one.
    fn committed_revision(&mut self) -> Option<s::PlanSnapshot> {
        self.committed_plan_summary().ok()??;
        let s::QueryResult::PlanCurrent {
            current: Some(plan),
            ..
        } = self.plan_cache.committed.as_ref()?.read().result()
        else {
            return None;
        };
        Some(plan.clone())
    }
}
