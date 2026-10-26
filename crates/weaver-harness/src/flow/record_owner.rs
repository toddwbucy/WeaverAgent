//! One trace owner through load, activation and shutdown. Moving its typestate
//! never creates a second writer or resets the run's author tickets.

use std::os::fd::BorrowedFd;

use weaver_trace::serving::{
    AwaitingLoad, CapacityWake, Closed, DrainStatus, Event, Kind, Loaded, PreflightAttempt,
    PreflightRecorder, Recorder, Refused, Submission, SubmitAttempt, ValidatedPartition,
};
use weaver_trace::{Failure, Sequence, SubmitRefusal};

enum Phase {
    AwaitingLoad(PreflightRecorder<AwaitingLoad>),
    Loaded(PreflightRecorder<Loaded>),
    Active(Recorder),
    Refused(PreflightRecorder<Refused>),
    Closed(PreflightRecorder<Closed>),
}

/// The temporary None exists only while a consuming typestate call is on stack.
/// Every return installs its returned owner, including all failure paths.
pub(super) struct RunRecorder {
    phase: Option<Phase>,
    stopped: bool,
    unloaded: bool,
    latest_recorded: Option<Sequence>,
}

macro_rules! dispatch {
    ($self:ident, $method:ident) => {
        match $self.phase.as_mut().expect("trace owner is installed") {
            Phase::AwaitingLoad(r) => r.$method(),
            Phase::Loaded(r) => r.$method(),
            Phase::Active(r) => r.$method(),
            Phase::Refused(r) => r.$method(),
            Phase::Closed(r) => r.$method(),
        }
    };
}

impl RunRecorder {
    pub(super) fn preflight(recorder: PreflightRecorder) -> Self {
        Self {
            phase: Some(Phase::AwaitingLoad(recorder)),
            stopped: false,
            unloaded: false,
            latest_recorded: None,
        }
    }

    pub(super) fn active(recorder: Recorder) -> Self {
        let latest_recorded = recorder
            .structure()
            .iter()
            .last()
            .map(|record| record.sequence);
        Self {
            phase: Some(Phase::Active(recorder)),
            stopped: false,
            unloaded: false,
            latest_recorded,
        }
    }

    /// Actual admission across every author path in this run. This says
    /// nothing about filing, capture content or survival of the sink.
    pub(super) fn latest_recorded(&self) -> Option<Sequence> {
        self.latest_recorded
    }

    pub(super) fn is_loaded(&self) -> bool {
        !matches!(self.phase, Some(Phase::AwaitingLoad(_)))
    }

    pub(super) fn is_active(&self) -> bool {
        matches!(self.phase, Some(Phase::Active(_))) && !self.unloaded
    }

    pub(super) fn is_refused(&self) -> bool {
        matches!(self.phase, Some(Phase::Refused(_)))
    }

    pub(super) fn is_preflight_shutdown(&self) -> bool {
        !self.stopped && matches!(self.phase, Some(Phase::Refused(_) | Phase::Closed(_)))
    }

    pub(super) fn is_closed(&self) -> bool {
        self.unloaded
    }

    pub(super) fn activate(&mut self, partition: ValidatedPartition) -> Result<(), Failure> {
        if self.stopped {
            return Err(refused());
        }
        match self.phase.take().expect("trace owner is installed") {
            Phase::Loaded(recorder) => {
                self.phase = Some(Phase::Active(recorder.activate(partition)));
                Ok(())
            }
            other => {
                self.phase = Some(other);
                Err(refused())
            }
        }
    }

    pub(super) fn submit(&mut self, event: Event) -> Result<SubmitAttempt, Failure> {
        if self.unloaded {
            return Err(refused());
        }
        let unload = event.envelope.kind == Kind::Unload;
        let result = match self.phase.take().expect("trace owner is installed") {
            Phase::AwaitingLoad(recorder) => match recorder.record_load(event) {
                Ok(PreflightAttempt::Recorded {
                    recorder,
                    submission,
                }) => {
                    self.phase = Some(Phase::Loaded(recorder));
                    Ok(SubmitAttempt::Recorded(submission))
                }
                Ok(PreflightAttempt::CapacityPending { recorder, event }) => {
                    self.phase = Some(Phase::AwaitingLoad(recorder));
                    Ok(SubmitAttempt::CapacityPending(event))
                }
                Err(failure) => {
                    self.phase = Some(Phase::AwaitingLoad(failure.recorder));
                    Err(failure.failure)
                }
            },
            Phase::Loaded(recorder) if unload => match recorder.rollback(event) {
                Ok(PreflightAttempt::Recorded {
                    recorder,
                    submission,
                }) => {
                    self.phase = Some(Phase::Closed(recorder));
                    Ok(SubmitAttempt::Recorded(submission))
                }
                Ok(PreflightAttempt::CapacityPending { recorder, event }) => {
                    self.phase = Some(Phase::Loaded(recorder));
                    Ok(SubmitAttempt::CapacityPending(event))
                }
                Err(failure) => {
                    self.phase = Some(Phase::Loaded(failure.recorder));
                    Err(failure.failure)
                }
            },
            Phase::Loaded(recorder) => match recorder.refuse(event) {
                Ok(PreflightAttempt::Recorded {
                    recorder,
                    submission,
                }) => {
                    self.phase = Some(Phase::Refused(recorder));
                    Ok(SubmitAttempt::Recorded(submission))
                }
                Ok(PreflightAttempt::CapacityPending { recorder, event }) => {
                    self.phase = Some(Phase::Loaded(recorder));
                    Ok(SubmitAttempt::CapacityPending(event))
                }
                Err(failure) => {
                    self.phase = Some(Phase::Loaded(failure.recorder));
                    Err(failure.failure)
                }
            },
            Phase::Refused(recorder) if unload => match recorder.record_unload(event) {
                Ok(PreflightAttempt::Recorded {
                    recorder,
                    submission,
                }) => {
                    self.phase = Some(Phase::Closed(recorder));
                    Ok(SubmitAttempt::Recorded(submission))
                }
                Ok(PreflightAttempt::CapacityPending { recorder, event }) => {
                    self.phase = Some(Phase::Refused(recorder));
                    Ok(SubmitAttempt::CapacityPending(event))
                }
                Err(failure) => {
                    self.phase = Some(Phase::Refused(failure.recorder));
                    Err(failure.failure)
                }
            },
            Phase::Refused(mut recorder) => {
                let result = recorder.record_fault(event);
                self.phase = Some(Phase::Refused(recorder));
                result
            }
            Phase::Active(mut recorder) => {
                let result = recorder.submit(event);
                self.phase = Some(Phase::Active(recorder));
                result
            }
            other => {
                self.phase = Some(other);
                Err(refused())
            }
        };
        if let Ok(SubmitAttempt::Recorded(submission)) = &result {
            // Retain admission before the caller's tap, clock or lifecycle
            // checks can fail. CapacityPending supplies no sequence.
            self.latest_recorded = Some(submission.sequence);
            if unload {
                self.unloaded = true;
            }
        }
        result
    }

    pub(super) fn submit_terminal(&mut self, event: Event) -> Result<Submission, Failure> {
        if !self.stopped || self.unloaded {
            return Err(refused());
        }
        let unload = event.envelope.kind == Kind::Unload;
        let result = match self.phase.take().expect("trace owner is installed") {
            Phase::Loaded(recorder) => match recorder.record_terminal_fault(event) {
                Ok(result) => {
                    self.phase = Some(Phase::Refused(result.recorder));
                    Ok(result.submission)
                }
                Err(failure) => {
                    self.phase = Some(Phase::Loaded(failure.recorder));
                    Err(failure.failure)
                }
            },
            Phase::Refused(recorder) if unload => match recorder.record_terminal_unload(event) {
                Ok(result) => {
                    self.phase = Some(Phase::Closed(result.recorder));
                    Ok(result.submission)
                }
                Err(failure) => {
                    self.phase = Some(Phase::Refused(failure.recorder));
                    Err(failure.failure)
                }
            },
            Phase::Refused(recorder) => match recorder.record_terminal_fault(event) {
                Ok(result) => {
                    self.phase = Some(Phase::Refused(result.recorder));
                    Ok(result.submission)
                }
                Err(failure) => {
                    self.phase = Some(Phase::Refused(failure.recorder));
                    Err(failure.failure)
                }
            },
            Phase::Active(mut recorder) => {
                let result = recorder.submit_terminal(event);
                self.phase = Some(Phase::Active(recorder));
                result
            }
            other => {
                self.phase = Some(other);
                Err(refused())
            }
        };
        if let Ok(submission) = &result {
            self.latest_recorded = Some(submission.sequence);
            if unload {
                self.unloaded = true;
            }
        }
        result
    }

    pub(super) fn stop_tap(&mut self) {
        self.stopped = true;
        if let Some(Phase::Active(recorder)) = &mut self.phase {
            recorder.stop_tap();
        }
    }

    pub(super) fn tap_stopped(&self) -> bool {
        self.stopped
    }

    #[cfg(test)]
    pub(super) fn active_recorder(&self) -> &Recorder {
        match self.phase.as_ref().expect("trace owner is installed") {
            Phase::Active(recorder) => recorder,
            _ => panic!("fixture requested active recorder before activation"),
        }
    }

    pub(super) fn capacity_fd(&self) -> BorrowedFd<'_> {
        match self.phase.as_ref().expect("trace owner is installed") {
            Phase::AwaitingLoad(r) => r.capacity_fd(),
            Phase::Loaded(r) => r.capacity_fd(),
            Phase::Active(r) => r.capacity_fd(),
            Phase::Refused(r) => r.capacity_fd(),
            Phase::Closed(r) => r.capacity_fd(),
        }
    }

    pub(super) fn standing_failure(&self) -> Option<Failure> {
        match self.phase.as_ref().expect("trace owner is installed") {
            Phase::AwaitingLoad(r) => r.standing_failure(),
            Phase::Loaded(r) => r.standing_failure(),
            Phase::Active(r) => r.standing_failure(),
            Phase::Refused(r) => r.standing_failure(),
            Phase::Closed(r) => r.standing_failure(),
        }
    }

    pub(super) fn try_capacity_wake(&mut self) -> Result<CapacityWake, Failure> {
        dispatch!(self, try_capacity_wake)
    }
    pub(super) fn drain_status(&mut self) -> Result<DrainStatus, Failure> {
        dispatch!(self, drain_status)
    }
    pub(super) fn drain_capacity_wake(&mut self) -> Result<(), Failure> {
        dispatch!(self, drain_capacity_wake)
    }
    pub(super) fn abandon(&mut self) {
        dispatch!(self, abandon)
    }
    pub(super) fn close_unadmitted(&mut self) -> Result<(), Failure> {
        let Some(Phase::AwaitingLoad(recorder)) = &mut self.phase else {
            return Err(refused());
        };
        recorder.close_unadmitted()?;
        self.stopped = true;
        Ok(())
    }
}

fn refused() -> Failure {
    Failure::RefusedOnSubmit {
        reason: SubmitRefusal::PayloadMalformed,
    }
}
