//! The next_ready observation port on the serving owner. Its event is built
//! once from two receipts and retained under its original author ticket until
//! Recorded; a retry resumes that ticket and samples no new endpoint.
use super::*;
use crate::flow::context::next_ready::{NextReadyRefusal, NextReadyStart};
use crate::flow::context::plan_port_driver::ClosureReceipt;
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
    /// The committed revision is not the one close and placement shared.
    Replaced,
    /// The closure or the active successor is gone from the committed revision.
    MissingEndpoint,
    /// The placement does not carry the successor's filed instruction unit.
    NotTheSuccessor,
    /// A reading is zero or out of order, or the placement precedes closure.
    ClockOrder,
}

/// A refusal authors nothing and returns both receipts unchanged.
#[derive(Debug)]
pub struct FoundationNextReadyError {
    pub start: FoundationNextReadyStart,
    pub placed: StepPlacementReceipt,
    pub refusal: FoundationNextReadyRefusal,
}

/// One checked observation, retained with both receipts until Recorded.
#[derive(Debug)]
pub(in crate::flow::context::worker_startup) struct PendingNextReady {
    _start: NextReadyStart,
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

    /// Check the two receipts and retain the measurement for authoring. It
    /// starts no progression; advance carries it to its Recorded receipt.
    #[allow(clippy::result_large_err)]
    pub fn observe_next_ready(
        &mut self,
        start: FoundationNextReadyStart,
        placed: StepPlacementReceipt,
    ) -> Result<(), FoundationNextReadyError> {
        let refuse = |start, placed, refusal| {
            Err(FoundationNextReadyError {
                start,
                placed,
                refusal,
            })
        };
        if let Err(failure) = self.serving.owner.context.live(&mut MonotonicIo) {
            self.failure = Some(failure);
            self.stage = Stage::Stopped;
        }
        if self.stage == Stage::Stopped {
            return refuse(start, placed, FoundationNextReadyRefusal::Stopped);
        }
        if self.serving.next_ready.is_some()
            || self.serving.owner.listener.held().is_some()
            || self.plan_work_pending()
            || self.serving.save.is_some()
            || self.has_graceful_leave()
            || self.report.is_some()
        {
            return refuse(start, placed, FoundationNextReadyRefusal::Busy);
        }
        let event = match self
            .serving
            .owner
            .context
            .next_ready_event(&start.start, &placed)
        {
            Ok(event) => event,
            Err(refusal) => {
                let refusal = match refusal {
                    NextReadyRefusal::CrossRun => FoundationNextReadyRefusal::CrossRun,
                    NextReadyRefusal::Replaced => FoundationNextReadyRefusal::Replaced,
                    NextReadyRefusal::MissingEndpoint => {
                        FoundationNextReadyRefusal::MissingEndpoint
                    }
                    NextReadyRefusal::NotTheSuccessor => {
                        FoundationNextReadyRefusal::NotTheSuccessor
                    }
                    NextReadyRefusal::ClockOrder => FoundationNextReadyRefusal::ClockOrder,
                };
                return refuse(start, placed, refusal);
            }
        };
        self.serving.next_ready = Some(Box::new(PendingNextReady {
            _start: start.start,
            _placed: placed,
            event: Some(event),
            started: false,
            returned: Default::default(),
            sequence: None,
        }));
        Ok(())
    }

    pub fn next_ready_pending(&self) -> bool {
        self.serving
            .next_ready
            .as_ref()
            .is_some_and(|work| work.sequence.is_none())
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

    pub(super) fn advance_next_ready(
        &mut self,
        peer: Option<&mut LifecyclePeer>,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        if !self.next_ready_pending() {
            return Ok(());
        }
        let work = self.serving.next_ready.as_mut().unwrap();
        let owner = &mut self.serving.owner;
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
                Ok(())
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
                    return Ok(());
                }
                Err(work.returned.retain_started(failure, &mut work.started))
            }
        }
    }
}
