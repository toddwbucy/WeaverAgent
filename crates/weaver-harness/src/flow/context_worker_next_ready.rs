//! The next_ready observation port on the serving owner. Its event is built
//! once from its three receipts and retained under its original author ticket until
//! Recorded; a retry resumes that ticket and samples no new endpoint.
use super::*;
use crate::flow::context::next_ready::{NextReadyRefusal, NextReadyStart};
use crate::flow::context::plan_port_driver::{ActivationReceipt, ClosureReceipt};
use generation::step_instruction::StepPlacementReceipt;
use weaver_trace::Sequence;

/// The consumed closure receipt and its began_ns reading, move-owned.
///
/// ```compile_fail,E0277
/// fn cloned<T: Clone>() {}
/// cloned::<weaver_harness::FoundationNextReadyStart>();
/// ```
///
/// ```compile_fail,E0451
/// fn forged() -> weaver_harness::FoundationNextReadyStart {
///     weaver_harness::FoundationNextReadyStart { start: todo!() }
/// }
/// ```
#[derive(Debug)]
pub struct FoundationNextReadyStart {
    start: NextReadyStart,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoundationNextReadyRefusal {
    Busy,
    Stopped,
    /// A receipt or the start belongs to another run's clock, a reload included.
    CrossRun,
    /// The committed revision is not the one close, activation and placement shared.
    Replaced,
    /// The closure or the activated step is gone from the committed revision.
    MissingEndpoint,
    /// The activation did not follow this closure's step, or the placement
    /// does not carry the activated step's filed instruction unit.
    NotTheSuccessor,
    /// A reading is zero or reversed, or the chain's sources are out of order.
    ClockOrder,
    /// The placement's turn is no longer open.
    TurnClosed,
}

impl From<NextReadyRefusal> for FoundationNextReadyRefusal {
    fn from(refusal: NextReadyRefusal) -> Self {
        match refusal {
            NextReadyRefusal::CrossRun => Self::CrossRun,
            NextReadyRefusal::Replaced => Self::Replaced,
            NextReadyRefusal::MissingEndpoint => Self::MissingEndpoint,
            NextReadyRefusal::NotTheSuccessor => Self::NotTheSuccessor,
            NextReadyRefusal::ClockOrder => Self::ClockOrder,
            NextReadyRefusal::TurnClosed => Self::TurnClosed,
            // A settling committed summary: retry, as for any busy owner.
            NextReadyRefusal::Pending => Self::Busy,
        }
    }
}

/// A refusal authors nothing and returns every receipt unchanged.
#[derive(Debug)]
pub struct FoundationNextReadyError {
    pub start: FoundationNextReadyStart,
    pub activation: ActivationReceipt,
    pub placed: StepPlacementReceipt,
    pub refusal: FoundationNextReadyRefusal,
}

/// One checked observation, retained with its receipts until Recorded.
#[derive(Debug)]
pub(in crate::flow::context::worker_startup) struct PendingNextReady {
    _start: NextReadyStart,
    _activation: ActivationReceipt,
    _placed: StepPlacementReceipt,
    event: Option<AuthoredEvent>,
    started: bool,
    returned: crate::flow::context::StoppedAuthor,
    sequence: Option<Sequence>,
}

impl FoundationInput<'_> {
    /// Consume a closure receipt into the successor progression, reading
    /// began_ns from the run clock once. A foreign receipt comes back unchanged.
    #[allow(clippy::result_large_err)]
    pub fn begin_next_ready(
        &mut self,
        receipt: ClosureReceipt,
    ) -> Result<FoundationNextReadyStart, ClosureReceipt> {
        if self.stage == Stage::Stopped {
            return Err(receipt);
        }
        match self
            .serving
            .owner
            .context
            .begin_next_ready(receipt, &mut MonotonicIo)
        {
            Ok(start) => Ok(FoundationNextReadyStart { start }),
            Err((receipt, failure)) => {
                if let Some(failure) = failure {
                    self.failure = Some(failure);
                    self.stage = Stage::Stopped;
                }
                Err(receipt)
            }
        }
    }

    /// Check the closure, the successor's activation and its placement, and
    /// retain the measurement for authoring. It starts no progression;
    /// advance carries it to its Recorded receipt.
    #[allow(clippy::result_large_err)]
    pub fn observe_next_ready(
        &mut self,
        start: FoundationNextReadyStart,
        activation: ActivationReceipt,
        placed: StepPlacementReceipt,
    ) -> Result<(), FoundationNextReadyError> {
        let refuse = |start, activation, placed, refusal| {
            Err(FoundationNextReadyError {
                start,
                activation,
                placed,
                refusal,
            })
        };
        if let Err(failure) = self.serving.owner.context.live(&mut MonotonicIo) {
            self.failure = Some(failure);
            self.stage = Stage::Stopped;
        }
        if self.stage == Stage::Stopped {
            return refuse(
                start,
                activation,
                placed,
                FoundationNextReadyRefusal::Stopped,
            );
        }
        if self.serving.next_ready.is_some()
            || self.serving.owner.listener.held().is_some()
            || self.plan_work_pending()
            || self.serving.save.is_some()
            || self.has_graceful_leave()
            || self.report.is_some()
        {
            return refuse(start, activation, placed, FoundationNextReadyRefusal::Busy);
        }
        let event =
            match self
                .serving
                .owner
                .context
                .next_ready_event(&start.start, &activation, &placed)
            {
                Ok(event) => event,
                Err(refusal) => {
                    return refuse(start, activation, placed, refusal.into());
                }
            };
        self.serving.next_ready = Some(Box::new(PendingNextReady {
            _start: start.start,
            _activation: activation,
            _placed: placed,
            event: Some(event),
            started: false,
            returned: Default::default(),
            sequence: None,
        }));
        Ok(())
    }

    /// An observation still owed: checked, not yet Recorded.
    pub fn next_ready_pending(&self) -> bool {
        self.serving
            .next_ready
            .as_ref()
            .is_some_and(|work| work.sequence.is_none())
    }

    /// Its original author ticket is admitted: only Recorded settles it.
    pub(super) fn next_ready_started(&self) -> bool {
        self.serving
            .next_ready
            .as_ref()
            .is_some_and(|work| work.sequence.is_none() && work.started)
    }

    /// An honored Stop or Leave before the observation was admitted: drop
    /// it with its receipts and author nothing, freeing the slot for a later
    /// observation. An admitted ticket is not discarded; it returns false and
    /// must settle.
    pub(super) fn discard_unstarted_next_ready(&mut self) -> bool {
        match self.serving.next_ready.as_ref() {
            Some(work) if work.sequence.is_none() && work.started => false,
            Some(work) if work.sequence.is_none() => {
                self.serving.next_ready = None;
                true
            }
            _ => true,
        }
    }

    /// The Recorded measurement's sequence, once. A pending or failed owner
    /// yields nothing and keeps its receipts.
    pub fn take_next_ready(&mut self) -> Option<Sequence> {
        if self.stage == Stage::Stopped || self.serving.owner.context.failure().is_some() {
            return None;
        }
        let sequence = self.serving.next_ready.as_ref()?.sequence?;
        self.serving.next_ready = None;
        Some(sequence)
    }

    /// Advance the owed observation. Ok(true) when it held the author slot
    /// this call (Recorded, or its admitted ticket still pending); Ok(false)
    /// when another owner holds external authorship, such as an outstanding
    /// placement grant, so the caller lets that owner run first. The event
    /// is never rebuilt: a retry admits or resumes the original.
    pub(super) fn advance_next_ready(
        &mut self,
        peer: Option<&mut LifecyclePeer>,
        io: &mut impl PumpIo,
    ) -> Result<bool, RunFailure> {
        if !self.next_ready_pending() {
            return Ok(false);
        }
        let work = self.serving.next_ready.as_mut().unwrap();
        let owner = &mut self.serving.owner;
        if !work.started {
            // The measurement never lands after its turn's turn.closed: an
            // unadmitted observation whose turn closed is dropped unauthored.
            let turn = work.event.as_ref().and_then(|event| event.turn.as_ref());
            if owner.context.open_turn.as_ref() != turn {
                self.serving.next_ready = None;
                return Ok(false);
            }
            if owner.context.place_blocks_external() {
                return Ok(false);
            }
        }
        let mut ports = PumpPorts {
            lifecycle: LifecyclePorts {
                listener: Some(&mut owner.listener),
                dial: peer,
                residency: Some(owner.resident.residency),
                leave_peer: plan::retained_port(&mut self.serving.plan, &mut self.serving.save),
                ..Default::default()
            },
            gate: Some(&mut owner.gate),
            decode: Some(owner.decode),
            label: None,
            io,
        };
        work.returned.check()?;
        let result = if work.started {
            owner.context.resume_author_and_wait(&mut ports)
        } else {
            work.started = true;
            let event = work
                .event
                .take()
                .expect("unstarted observation keeps its event");
            owner.context.author_and_wait(&mut ports, event)
        };
        match result {
            Ok(sequence) => {
                work.sequence = Some(sequence);
                Ok(true)
            }
            Err(mut failure) => {
                if failure.reclaim_held(owner.context) {
                    work.started = false;
                    work.event = failure.original.take();
                    if owner.context.reduce_flow(ports.io)?
                        == crate::flow::coordinator::Advance::Pending
                    {
                        owner.context.pump_once(&mut ports)?;
                    }
                    return Ok(false);
                }
                Err(work.returned.retain_started(failure, &mut work.started))
            }
        }
    }
}
