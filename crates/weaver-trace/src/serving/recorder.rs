//! Typed load admission and the terminal serving tap, without orchestration.

use super::event::{Event, Payload, admit, malformed};
use super::structure::{Record, WorkingStructure};
use super::writer::{CapacityWake, DrainStatus, Terminal, Writer};
use super::{Kind, SourceEvent, TapOutcome, Tee, ValidatedPartition};
use crate::{Boundary, Drained, Failure, Pressure, RunRef, Sequence, SessionRef};
use std::marker::PhantomData;
use std::os::fd::{BorrowedFd, OwnedFd};
use std::sync::Arc;
use std::time::Instant;

#[must_use = "account for the copy outcome before allowing the next decision"]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Submission {
    pub sequence: Sequence,
    pub tap: TapOutcome,
}

#[must_use = "pending capacity returns the exact event for a later attempt"]
#[derive(Debug)]
pub enum SubmitAttempt {
    Recorded(Submission),
    CapacityPending(Event),
}

pub struct AwaitingLoad;
pub struct Loaded;
pub struct Refused;
pub struct Closed;

/// Only a recorded load produces the state on which activate is available.
/// No state exposes a general content submission method.
///
/// ```
/// use std::os::fd::OwnedFd;
/// use weaver_trace::{Failure, RunRef, SessionRef};
/// use weaver_trace::serving::{Loaded, PreflightRecorder, Recorder, ValidatedPartition};
/// let _: fn(OwnedFd, RunRef, SessionRef) -> Result<PreflightRecorder, Failure> =
///     PreflightRecorder::receive;
/// let _: fn(PreflightRecorder<Loaded>, ValidatedPartition) -> Recorder =
///     PreflightRecorder::activate;
/// ```
///
/// ```compile_fail
/// use weaver_trace::serving::{AwaitingLoad, Event, PreflightRecorder};
/// fn fault_before_load(recorder: PreflightRecorder<AwaitingLoad>, event: Event) {
///     recorder.record_terminal_fault(event);
/// }
/// ```
///
/// ```compile_fail
/// use weaver_trace::serving::{Closed, Event, PreflightRecorder};
/// fn repeat_unload(recorder: PreflightRecorder<Closed>, event: Event) {
///     recorder.record_terminal_unload(event);
/// }
/// ```
///
/// ```compile_fail
/// use weaver_trace::serving::{PreflightRecorder, ValidatedPartition};
/// fn before_load(recorder: PreflightRecorder, partition: ValidatedPartition) {
///     recorder.activate(partition);
/// }
/// ```
///
/// ```compile_fail
/// use weaver_trace::serving::{PreflightRecorder, Refused, ValidatedPartition};
/// fn after_refusal(recorder: PreflightRecorder<Refused>, partition: ValidatedPartition) {
///     recorder.activate(partition);
/// }
/// ```
///
/// ```compile_fail
/// use weaver_trace::serving::{Event, Loaded, PreflightRecorder};
/// fn before_activation(mut recorder: PreflightRecorder<Loaded>, event: Event) {
///     recorder.submit(event);
/// }
/// ```
pub struct PreflightRecorder<State = AwaitingLoad> {
    core: Box<Core>,
    state: PhantomData<State>,
}

#[must_use]
pub enum PreflightAttempt<Before, After> {
    Recorded {
        recorder: PreflightRecorder<After>,
        submission: Submission,
    },
    CapacityPending {
        recorder: PreflightRecorder<Before>,
        event: Event,
    },
}

#[must_use]
pub struct PreflightFailure<State> {
    pub recorder: PreflightRecorder<State>,
    pub failure: Failure,
}

/// A reserved preflight record never waits for ordinary queue capacity.
#[must_use]
pub struct PreflightTerminal<State> {
    pub recorder: PreflightRecorder<State>,
    pub submission: Submission,
}

/// Failed terminal authoring retains both the recorder and the original event.
pub struct PreflightTerminalFailure<State> {
    pub recorder: PreflightRecorder<State>,
    pub event: Box<Event>,
    pub failure: Failure,
}

impl PreflightRecorder<AwaitingLoad> {
    pub fn receive(sink: OwnedFd, run: RunRef, session: SessionRef) -> Result<Self, Failure> {
        Ok(Self {
            core: Box::new(Core::new(sink, run, session)?),
            state: PhantomData,
        })
    }
    pub fn record_load(
        self,
        event: Event,
    ) -> Result<PreflightAttempt<AwaitingLoad, Loaded>, PreflightFailure<AwaitingLoad>> {
        self.transition(event, |event| event.envelope.kind == Kind::Load)
    }
}

impl PreflightRecorder<Loaded> {
    pub fn activate(self, partition: ValidatedPartition) -> Recorder {
        Recorder {
            core: self.core,
            tee: Tee::new(partition),
            closed: false,
        }
    }
    pub fn refuse(
        self,
        event: Event,
    ) -> Result<PreflightAttempt<Loaded, Refused>, PreflightFailure<Loaded>> {
        self.transition(event, |event| {
            matches!(event.envelope.kind, Kind::Refusal | Kind::Fault)
        })
    }

    /// Only the harness halt path spends this first-fault reservation. No tee
    /// has been installed, and the resulting refusal can never activate one.
    pub fn record_terminal_fault(
        self,
        event: Event,
    ) -> Result<PreflightTerminal<Refused>, PreflightTerminalFailure<Loaded>> {
        self.terminal_transition(event, Terminal::Fault)
    }
}

impl PreflightRecorder<Refused> {
    pub fn record_fault(&mut self, event: Event) -> Result<SubmitAttempt, Failure> {
        if event.envelope.kind != Kind::Fault || self.core.preflight_fault {
            return Err(malformed());
        }
        self.core.submit(event, None, None)
    }
    pub fn record_unload(
        self,
        event: Event,
    ) -> Result<PreflightAttempt<Refused, Closed>, PreflightFailure<Refused>> {
        self.transition(event, |event| event.envelope.kind == Kind::Unload)
    }

    /// A component failure after an ordinary refusal still owns its fault
    /// reservation. A fault already recorded by either path cannot repeat.
    pub fn record_terminal_fault(
        self,
        event: Event,
    ) -> Result<PreflightTerminal<Refused>, PreflightTerminalFailure<Refused>> {
        self.terminal_transition(event, Terminal::Fault)
    }

    /// Finish a failed preflight using the unload reservation. Ordinary refused
    /// enters continue to call record_unload and use ordinary capacity.
    pub fn record_terminal_unload(
        self,
        event: Event,
    ) -> Result<PreflightTerminal<Closed>, PreflightTerminalFailure<Refused>> {
        self.terminal_transition(event, Terminal::Unload)
    }
}

impl<State> PreflightRecorder<State> {
    fn terminal_transition<After>(
        mut self,
        event: Event,
        terminal: Terminal,
    ) -> Result<PreflightTerminal<After>, PreflightTerminalFailure<State>> {
        let allowed = match terminal {
            Terminal::Fault => event.envelope.kind == Kind::Fault && !self.core.preflight_fault,
            Terminal::Unload => event.envelope.kind == Kind::Unload,
            _ => false,
        };
        if !allowed {
            return Err(PreflightTerminalFailure {
                recorder: self,
                event: Box::new(event),
                failure: malformed(),
            });
        }
        match self.core.submit(event.clone(), None, Some(terminal)) {
            Ok(SubmitAttempt::Recorded(submission)) => Ok(PreflightTerminal {
                recorder: PreflightRecorder {
                    core: self.core,
                    state: PhantomData,
                },
                submission,
            }),
            Ok(SubmitAttempt::CapacityPending(_)) => {
                unreachable!("terminal capacity was reserved per kind")
            }
            Err(failure) => Err(PreflightTerminalFailure {
                recorder: self,
                event: Box::new(event),
                failure,
            }),
        }
    }

    fn transition<After>(
        mut self,
        event: Event,
        allowed: impl FnOnce(&Event) -> bool,
    ) -> Result<PreflightAttempt<State, After>, PreflightFailure<State>> {
        if !allowed(&event) {
            return Err(PreflightFailure {
                recorder: self,
                failure: malformed(),
            });
        }
        match self.core.submit(event, None, None) {
            Ok(SubmitAttempt::Recorded(submission)) => Ok(PreflightAttempt::Recorded {
                recorder: PreflightRecorder {
                    core: self.core,
                    state: PhantomData,
                },
                submission,
            }),
            Ok(SubmitAttempt::CapacityPending(event)) => Ok(PreflightAttempt::CapacityPending {
                recorder: self,
                event,
            }),
            Err(failure) => Err(PreflightFailure {
                recorder: self,
                failure,
            }),
        }
    }
    pub fn structure(&self) -> &WorkingStructure {
        &self.core.structure
    }
    pub fn boundary(&self) -> Boundary {
        self.core.writer.boundary(self.core.admitted)
    }
    pub fn pressure(&self) -> Pressure {
        self.core.writer.pressure()
    }
    pub fn capacity_fd(&self) -> BorrowedFd<'_> {
        self.core.writer.capacity_fd()
    }
    pub fn standing_failure(&self) -> Option<Failure> {
        self.core.standing_failure()
    }
    /// Perform at most one nonblocking read of the capacity-wake descriptor.
    pub fn try_capacity_wake(&mut self) -> Result<CapacityWake, Failure> {
        self.core.try_capacity_wake()
    }
    /// Inspect the drain predicate without waiting or electing a deadline.
    pub fn drain_status(&self) -> Result<DrainStatus, Failure> {
        self.core.drain_status()
    }
    pub fn drain_capacity_wake(&mut self) -> Result<(), Failure> {
        self.core.writer.drain_capacity_wake()
    }
    pub fn drain(&mut self) -> Result<(), Failure> {
        self.core.writer.drain()
    }
    pub fn drain_by(&mut self, deadline: Instant) -> Result<Drained, Failure> {
        self.core.writer.drain_by(deadline)
    }
    pub fn abandon(&mut self) {
        self.core.writer.abandon();
    }
}

/// A run has one terminal tap, installed by consuming its recorded preflight.
///
/// ```compile_fail
/// use weaver_trace::serving::{Recorder, Tee};
/// fn reattach(mut recorder: Recorder, tee: Tee) {
///     recorder.attach_tee(tee);
/// }
/// ```
pub struct Recorder {
    core: Box<Core>,
    tee: Tee,
    closed: bool,
}

impl Recorder {
    pub fn submit(&mut self, event: Event) -> Result<SubmitAttempt, Failure> {
        self.try_submit(event)
    }
    pub fn try_submit(&mut self, event: Event) -> Result<SubmitAttempt, Failure> {
        if self.closed || event.envelope.kind == Kind::Load {
            return Err(malformed());
        }
        let unload = event.envelope.kind == Kind::Unload;
        let result = self.core.submit(event, Some(&mut self.tee), None)?;
        if unload && matches!(result, SubmitAttempt::Recorded(_)) {
            self.closed = true;
        }
        Ok(result)
    }
    /// The harness's halt latch is the sole serving caller. A stopped tap never
    /// resumes, and trace-only shutdown records still reach both trace holders.
    pub fn stop_tap(&mut self) {
        self.tee.stop();
    }
    pub fn tap_stopped(&self) -> bool {
        self.tee.is_stopped()
    }
    pub fn submit_terminal(&mut self, event: Event) -> Result<Submission, Failure> {
        if !self.tee.is_stopped() || self.closed {
            return Err(malformed());
        }
        let permit = match event.envelope.kind {
            Kind::Fault => Terminal::Fault,
            Kind::ToolCallCompleted => Terminal::ToolCompleted,
            Kind::MessageToolResult => Terminal::ToolResult,
            Kind::TurnClosed => Terminal::TurnClosed,
            Kind::SessionClosed => Terminal::SessionClosed,
            Kind::SavePoint if matches!(event.payload.as_ref(), Some(Payload::SavePointRecord(record)) if matches!(record.as_ref(),super::payloads::SavePointRecord::Refused { .. })) => {
                Terminal::SaveRefused
            }
            Kind::Unload => Terminal::Unload,
            _ => return Err(malformed()),
        };
        let result = self.core.submit(event, Some(&mut self.tee), Some(permit))?;
        match result {
            SubmitAttempt::Recorded(submission) => {
                if permit == Terminal::Unload {
                    self.closed = true;
                }
                Ok(submission)
            }
            SubmitAttempt::CapacityPending(_) => {
                unreachable!("terminal capacity was reserved per kind")
            }
        }
    }
    pub fn structure(&self) -> &WorkingStructure {
        &self.core.structure
    }
    pub fn boundary(&self) -> Boundary {
        self.core.writer.boundary(self.core.admitted)
    }
    pub fn pressure(&self) -> Pressure {
        self.core.writer.pressure()
    }
    pub fn capacity_fd(&self) -> BorrowedFd<'_> {
        self.core.writer.capacity_fd()
    }
    pub fn standing_failure(&self) -> Option<Failure> {
        self.core.standing_failure()
    }
    /// Perform at most one nonblocking read of the capacity-wake descriptor.
    pub fn try_capacity_wake(&mut self) -> Result<CapacityWake, Failure> {
        self.core.try_capacity_wake()
    }
    /// Inspect the drain predicate without waiting or electing a deadline.
    pub fn drain_status(&self) -> Result<DrainStatus, Failure> {
        self.core.drain_status()
    }
    pub fn drain_capacity_wake(&mut self) -> Result<(), Failure> {
        self.core.writer.drain_capacity_wake()
    }
    pub fn drain(&mut self) -> Result<(), Failure> {
        self.core.writer.drain()
    }
    pub fn drain_by(&mut self, deadline: Instant) -> Result<Drained, Failure> {
        self.core.writer.drain_by(deadline)
    }
    pub fn abandon(&mut self) {
        self.core.writer.abandon();
    }
}

struct Core {
    run: RunRef,
    session: SessionRef,
    next: Option<u64>,
    admitted: Option<Sequence>,
    structure: WorkingStructure,
    writer: Writer,
    append_failure: Option<Failure>,
    preflight_fault: bool,
}

impl Core {
    fn new(sink: OwnedFd, run: RunRef, session: SessionRef) -> Result<Self, Failure> {
        Ok(Self {
            run,
            session,
            next: Some(0),
            admitted: None,
            structure: WorkingStructure::default(),
            writer: Writer::start(sink)?,
            append_failure: None,
            preflight_fault: false,
        })
    }
    fn standing_failure(&self) -> Option<Failure> {
        self.append_failure
            .clone()
            .or_else(|| self.writer.standing_failure())
    }
    fn try_capacity_wake(&mut self) -> Result<CapacityWake, Failure> {
        if let Some(failure) = self.standing_failure() {
            return Err(failure);
        }
        self.writer.try_capacity_wake()
    }
    fn drain_status(&self) -> Result<DrainStatus, Failure> {
        if let Some(failure) = self.standing_failure() {
            return Err(failure);
        }
        self.writer.drain_status()
    }
    fn submit(
        &mut self,
        mut event: Event,
        tee: Option<&mut Tee>,
        terminal: Option<Terminal>,
    ) -> Result<SubmitAttempt, Failure> {
        admit(&event, &self.session, &self.run)?;
        if let Some(failure) = &self.append_failure {
            return Err(failure.clone());
        }
        if let Some(failure) = self.writer.standing_failure() {
            return Err(failure);
        }
        let Some(reservation) = self.writer.reserve(terminal)? else {
            return Ok(SubmitAttempt::CapacityPending(event));
        };
        let sequence = Sequence(self.next.ok_or_else(malformed)?);
        event.envelope.sequence = sequence;
        let line = render(&event)?;
        if let Err(failure) = self.structure.append(Record {
            sequence,
            kind: event.envelope.kind,
            turn: event.envelope.turn.clone(),
            line: Arc::clone(&line),
        }) {
            self.append_failure = Some(failure.clone());
            return Err(failure);
        }
        self.next = sequence.0.checked_add(1);
        self.admitted = Some(sequence);
        let tap = if event.envelope.kind == Kind::Load {
            TapOutcome::HeadUntapped
        } else if let Some(tee) = tee {
            tee.tap(
                &SourceEvent {
                    run: self.run.0.clone(),
                    sequence,
                },
                &line,
            )
        } else {
            TapOutcome::Stopped
        };
        if event.envelope.kind == Kind::Fault {
            self.preflight_fault = true;
        }
        reservation.enqueue(sequence, line);
        Ok(SubmitAttempt::Recorded(Submission { sequence, tap }))
    }
}

fn render(event: &Event) -> Result<crate::Line, Failure> {
    let mut body = serde_json::to_string(event).map_err(|_| malformed())?;
    if body.contains(['\r', '\n']) {
        return Err(malformed());
    }
    body.try_reserve(1).map_err(|_| Failure::AppendFailed {
        sequence: event.envelope.sequence,
    })?;
    body.push('\n');
    Ok(Arc::from(body))
}

#[cfg(test)]
#[path = "recorder_tests.rs"]
mod tests;
