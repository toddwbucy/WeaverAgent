//! First-failure fault reporting on the stopped recorder. Every step is bounded
//! local work. Nothing here closes a transport or grants the next halt phase.

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum HaltReportRefusal {
    NotFailed,
    MissingAccount,
    AlreadyStarted,
    NotStarted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum HaltTraceProgress {
    FaultRecorded(Sequence),
    TraceRecorded(Sequence),
    Pending,
    Idle,
    Unavailable,
    AbortRequired,
}

/// Retained reporting evidence, not a release, containment or permission proof.
/// Only this context's stopped-recorder operation can assign fault_sequence.
pub(in crate::flow) struct HaltReport {
    attempted: bool,
    event: Option<AuthoredEvent>,
    fault_sequence: Option<Sequence>,
    last_trace_sequence: Option<Sequence>,
    unavailable: Option<String>,
    aborted: Option<String>,
    trace_unavailable: bool,
}

impl HaltReport {
    pub(super) fn attempted(&self) -> bool {
        self.attempted
    }
    pub(in crate::flow) fn fault_sequence(&self) -> Option<Sequence> {
        self.fault_sequence
    }
    pub(in crate::flow) fn event(&self) -> Option<&AuthoredEvent> {
        self.event.as_ref()
    }
    pub(in crate::flow) fn unavailable(&self) -> Option<&str> {
        self.unavailable.as_deref()
    }
    pub(in crate::flow) fn aborted(&self) -> Option<&str> {
        self.aborted.as_deref()
    }
    /// Latest stopped-tap ordinary admission, which may predate this report.
    /// The reserved fault sequence is separate. This is not a capture frontier.
    pub(in crate::flow) fn last_trace_sequence(&self) -> Option<Sequence> {
        self.last_trace_sequence
    }
    /// This reporting road cannot continue. A rejected ordinary obligation also
    /// stops it, so this value alone does not prove an operating-system sink loss.
    pub(in crate::flow) fn trace_unavailable(&self) -> bool {
        self.trace_unavailable
    }
}

impl RecordingContext {
    /// The caller first freezes the actual external owner facts through
    /// freeze_halt_account. Requiring that installed snapshot avoids inventing
    /// capture publication state or a pre-open flow observation here.
    pub(in crate::flow) fn begin_halt_report(&mut self) -> Result<(), HaltReportRefusal> {
        if self.phase != RecordingPhase::Failed
            || self.failure.is_none()
            || self.flow.halt().is_none()
        {
            return Err(HaltReportRefusal::NotFailed);
        }
        if self.halt_report.is_some() {
            return Err(HaltReportRefusal::AlreadyStarted);
        }
        if self.flow.halt().unwrap().account().is_none() {
            return Err(HaltReportRefusal::MissingAccount);
        }
        self.halt_report = Some(HaltReport {
            attempted: false,
            event: None,
            fault_sequence: None,
            last_trace_sequence: self.barrier.last_trace_only_sequence(),
            unavailable: None,
            aborted: None,
            trace_unavailable: false,
        });
        Ok(())
    }

    pub(in crate::flow) fn halt_report(&self) -> Option<&HaltReport> {
        self.halt_report.as_ref()
    }

    /// One terminal attempt or one opportunistic ordinary trace step. This never
    /// polls or waits for capacity and therefore cannot postpone a later driver
    /// sending ToolInterrupt. The ordinary live() road stays permanently closed.
    pub(in crate::flow) fn advance_halt_trace(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<HaltTraceProgress, HaltReportRefusal> {
        if self.phase != RecordingPhase::Failed || self.failure.is_none() {
            return Err(HaltReportRefusal::NotFailed);
        }
        if self.halt_report.is_none() {
            return Err(HaltReportRefusal::NotStarted);
        }
        if !self.halt_report_clock(io) {
            return Ok(HaltTraceProgress::AbortRequired);
        }
        let progress = if !self.halt_report.as_ref().unwrap().attempted {
            self.attempt_halt_fault()
        } else if self.halt_report.as_ref().unwrap().trace_unavailable {
            HaltTraceProgress::Unavailable
        } else if self.has_owed_measure_refusal() {
            // This owner also resumes any earlier pending event. Do not steal
            // its queued refusal or assign its sequence from another road.
            match self.failed_measure_refusal_progress(io) {
                Ok(Some(sequence)) => HaltTraceProgress::TraceRecorded(sequence),
                Ok(None) => HaltTraceProgress::Pending,
                Err(_) => {
                    // The measurement owner already appended its exact failure.
                    // A clock or author refusal is not evidence of a dead sink.
                    self.halt_report.as_mut().unwrap().trace_unavailable =
                        self.barrier.standing_failure().is_some();
                    HaltTraceProgress::Unavailable
                }
            }
        } else if self.boot_owns_pending_author() {
            // Its dedicated bounded driver must retain Recorded and the next
            // boot continuation before any fallible clock/lifecycle hook.
            HaltTraceProgress::Pending
        } else if self.barrier.pending().is_some() {
            match self.barrier.resume_trace_only() {
                Ok(Some(sequence)) => HaltTraceProgress::TraceRecorded(sequence),
                Ok(None) => HaltTraceProgress::Pending,
                Err(error) => {
                    // Retain the invalid obligation without trying it again.
                    // A protocol refusal here does not establish sink death.
                    self.halt_report.as_mut().unwrap().trace_unavailable = true;
                    self.halt_report_secondary(
                        Some(self.last_ns),
                        format!("owed trace-only record: {error:?}"),
                    );
                    HaltTraceProgress::Unavailable
                }
            }
        } else {
            HaltTraceProgress::Idle
        };
        // The measurement owner can drain an older ticket without completing
        // its own refusal. Preserve that actual admission as separate evidence.
        self.halt_report.as_mut().unwrap().last_trace_sequence =
            self.barrier.last_trace_only_sequence();
        // Preserve every consumed result before this possible clock failure.
        if !self.halt_report_clock(io) {
            return Ok(HaltTraceProgress::AbortRequired);
        }
        Ok(progress)
    }

    fn attempt_halt_fault(&mut self) -> HaltTraceProgress {
        // Rendering, preparation or submission failure cannot start a second
        // attempt, including an uncertain admission whose sequence is absent.
        self.halt_report.as_mut().unwrap().attempted = true;
        match self.prepare_and_submit_halt_fault() {
            Ok(sequence) => {
                self.halt_report.as_mut().unwrap().fault_sequence = Some(sequence);
                HaltTraceProgress::FaultRecorded(sequence)
            }
            Err(detail) => {
                let report = self.halt_report.as_mut().unwrap();
                report.unavailable = Some(detail.clone());
                report.trace_unavailable = self.barrier.standing_failure().is_some();
                self.halt_report_secondary(Some(self.last_ns), detail);
                HaltTraceProgress::Unavailable
            }
        }
    }

    fn prepare_and_submit_halt_fault(&mut self) -> Result<Sequence, String> {
        let halt = self.flow.halt().expect("failed owner retains its halt");
        let account = halt
            .fault_account()
            .map_err(|error| format!("halt account: {error}"))?;
        let case = if halt.first().reason == FlowHaltReason::TraceFailed {
            weaver_types::FaultCase::StreamWriteFailed
        } else {
            weaver_types::FaultCase::StateFlowFailed
        };
        let report = weaver_types::FaultReport { case, account };
        let payload = serde_json::value::to_raw_value(&report)
            .map_err(|error| format!("halt fault encoding: {error}"))?;
        let event = AuthoredEvent {
            kind: Kind::Fault,
            subsystem: weaver_trace::Subsystem::Harness,
            turn: self.open_turn.clone(),
            source: super::super::author::AuthoredSource::Runtime,
            causal_parent: None,
            payload: Some(Payload::Fault(payload)),
        };
        self.halt_report.as_mut().unwrap().event = Some(event);
        let wall = wall_ms()?;
        self.barrier.submit_terminal_authored_ref(
            self.halt_report.as_ref().unwrap().event.as_ref().unwrap(),
            &self.author,
            self.last_ns,
            wall,
        )
    }

    fn halt_report_clock(&mut self, io: &mut impl PumpIo) -> bool {
        if self.halt_report.as_ref().unwrap().aborted.is_some() {
            return false;
        }
        let sampled = io.now();
        let (observed, error) = match sampled {
            Ok(now) if now < self.last_ns => {
                (Some(now), Some("halt report clock reversed".to_owned()))
            }
            Ok(now) => {
                self.last_ns = now;
                let deadline = self.flow.halt().unwrap().deadline();
                let error = match deadline {
                    Ok(deadline) if now < deadline => None,
                    Ok(_) => Some("halt report reached its original deadline".to_owned()),
                    Err(error) => Some(format!("halt report has no deadline: {error}")),
                };
                (Some(now), error)
            }
            Err(error) => (None, Some(format!("halt report clock: {error}"))),
        };
        if let Some(detail) = error {
            self.halt_report.as_mut().unwrap().aborted = Some(detail.clone());
            self.halt_report_secondary(observed, detail);
            return false;
        }
        true
    }

    fn halt_report_secondary(&mut self, observed: Option<u64>, detail: String) {
        // Even failure to retain secondary evidence leaves the original cause
        // and stopped owner intact. It cannot recursively attempt another fault.
        let _ = self
            .flow
            .halt_mut()
            .unwrap()
            .append_secondary(SecondaryFailure {
                reason: FlowHaltReason::TraceFailed,
                point: HaltPoint::Teardown,
                observed_ns: observed.map(Into::into),
                detail,
            });
    }
}
