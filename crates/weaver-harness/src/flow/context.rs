//! The serving run's recording lifetime. Its owners move together from enter
//! through final drain. No lifecycle transition constructs a second recorder,
//! clock, author counter, state cursor or classifier cursor.

use std::os::fd::OwnedFd;
use std::time::{SystemTime, UNIX_EPOCH};

use weaver_trace::serving::{self, ContentSchema, Kind, Payload, PreflightRecorder};
use weaver_trace::{RunRef, Sequence, SessionRef};
use weaver_types::state_flow::{FlowConfig, ReadLimits, StartIdentity, Ste100Bootstrap};

use super::author::{AuthoredEvent, ServingAuthor};
use super::channel::{ChannelFailureKind, ChannelPoint, StateChannel};
use super::clock::{AbsoluteBound, ClockOrigin, DeadlineRegistry, PumpIo};
use super::coordinator::{
    Advance, CoordinatorPhase, FlowCoordinator, FlowFailure, FlowRecording, OperationCompletion,
};
use super::intake::{FlowInbox, RetentionBudget};
use super::pump::{CommonPump, PumpDeadline, PumpFailure, PumpPorts, PumpSource, PumpStep};
use super::recording::{RecordBarrier, RecordFailure, RecordProgress, RecordPurpose};
use super::report::{FlowHaltReason, HaltPoint, SecondaryFailure, WaitAtHalt};
use super::ste100_socket::SocketPort;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RecordingPhase {
    Preflight,
    Entered,
    Releasing,
    Failed,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum RunFailure {
    Flow {
        reason: FlowHaltReason,
        point: HaltPoint,
        detail: String,
    },
    Wait {
        key: PumpDeadline,
        bound: AbsoluteBound,
    },
    Transport {
        source: PumpSource,
        failure: super::pump_transport::Failure,
    },
    Closed,
    /// Certification owns the content boundary. This operation did not start
    /// and does not latch a component failure.
    TemplateBlocked,
}

/// Creation errors happen before a usable recording context exists. They go to
/// the existing enter diagnostic path, never to a fabricated trace head.
#[derive(Debug)]
pub(super) struct ContextError(pub String);

#[derive(Debug)]
pub(super) enum ActivationFailure {
    Partition(serving::PartitionError),
    Run(RunFailure),
}

#[derive(Debug)]
// Return owned failure evidence without allocating on the failed-report path.
#[allow(clippy::large_enum_variant)]
pub(super) enum HaltAccountFailure {
    Snapshot(super::halt_snapshot::SnapshotFailure),
    Install(super::report::InstallFailure),
}

struct LoadDuty {
    scheme_text: String,
    instruction: serving::payloads::Ste100Instruction,
    persistence: serving::payloads::PersistenceConfig,
    template_inventory: serving::payloads::TemplateInventory,
}

pub(crate) struct RecordingContext {
    phase: RecordingPhase,
    origin: ClockOrigin,
    last_ns: u64,
    author: ServingAuthor,
    barrier: RecordBarrier,
    flow: FlowCoordinator<SocketPort>,
    inbox: FlowInbox,
    budget: RetentionBudget,
    pump: CommonPump,
    deadlines: DeadlineRegistry<PumpDeadline>,
    declaration: FlowConfig,
    open_start: Option<StartIdentity>,
    open_limits: Option<ReadLimits>,
    load_duty: Option<LoadDuty>,
    template_checks: Option<template_driver::TemplateChecks>,
    template_setup_refusal: Option<super::templates::TemplateError>,
    template_blocked_author: Option<AuthoredEvent>,
    input: super::input::InputSlot,
    input_author: Option<input_driver::InputAuthorStage>,
    input_turn: Option<input_driver::InputTurn>,
    input_turns: std::collections::BTreeSet<String>,
    member_capture: Option<capture_driver::MemberCapture>,
    filing: Option<receipt_driver::FilingTransaction>,
    output: Option<output_driver::OutputTransaction>,
    last_author_sequence: Option<Sequence>,
    author_turn: Option<(Kind, Option<weaver_trace::TurnRef>)>,
    open_turn: Option<weaver_trace::TurnRef>,
    /// A caller's source remains owned if refusal happens before a ticket.
    /// It is distinct from the recorder's first pending author.
    refused_author: Option<AuthoredEvent>,
    waiting_content: Option<AuthoredEvent>,
    completed_author: Option<Sequence>,
    failure: Option<RunFailure>,
    failed_wait: Option<WaitAtHalt>,
}

impl RecordingContext {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        sink: OwnedFd,
        state: StateChannel,
        session: String,
        run: String,
        operator: u32,
        declaration: FlowConfig,
        remaining_enter_ns: u64,
        io: &mut impl PumpIo,
    ) -> Result<Self, ContextError> {
        declaration
            .validate()
            .map_err(|e| ContextError(e.to_string()))?;
        if state.identity().0 != session || state.identity().1 != run {
            return Err(ContextError("state and recording identities differ".into()));
        }
        // Capture before any state open or classifier bootstrap bytes can be sent.
        let origin = ClockOrigin::capture(io).map_err(|e| ContextError(e.to_string()))?;
        let author = ServingAuthor::new(session.clone(), run.clone(), operator, origin.raw())
            .map_err(|e| ContextError(format!("author identity: {e:?}")))?;
        let recorder = PreflightRecorder::receive(sink, RunRef(run.clone()), SessionRef(session))
            .map_err(|e| ContextError(format!("trace receive: {e:?}")))?;
        let budget = RetentionBudget::new(declaration.backlog.clone())
            .map_err(|e| ContextError(e.to_string()))?;
        let flow = FlowCoordinator::preflight(
            state,
            declaration.scheme.clone(),
            declaration.observation_interval.clone(),
            declaration.backlog.lag_alarm_turns.0,
        )
        .map_err(|e| ContextError(e.detail))?;
        let mut deadlines = DeadlineRegistry::new();
        deadlines
            .arm(
                PumpDeadline::Enter,
                AbsoluteBound::after(origin.raw(), remaining_enter_ns)
                    .map_err(|e| ContextError(e.to_string()))?,
            )
            .map_err(|e| ContextError(e.to_string()))?;
        let input = super::input::InputSlot::new(run.clone(), declaration.scheme.scheme_id.clone())
            .map_err(|error| ContextError(error.to_string()))?;
        Ok(Self {
            phase: RecordingPhase::Preflight,
            origin,
            last_ns: origin.raw(),
            author,
            barrier: RecordBarrier::preflight(recorder, run),
            flow,
            inbox: FlowInbox::new(),
            budget,
            pump: CommonPump::new(origin),
            deadlines,
            declaration,
            open_start: None,
            open_limits: None,
            load_duty: None,
            template_checks: None,
            template_setup_refusal: None,
            template_blocked_author: None,
            input,
            input_author: None,
            input_turn: None,
            input_turns: std::collections::BTreeSet::new(),
            member_capture: None,
            filing: None,
            output: None,
            last_author_sequence: None,
            author_turn: None,
            open_turn: None,
            refused_author: None,
            waiting_content: None,
            completed_author: None,
            failure: None,
            failed_wait: None,
        })
    }

    pub(super) fn phase(&self) -> RecordingPhase {
        self.phase
    }
    pub(super) fn origin_ns(&self) -> u64 {
        self.origin.raw()
    }
    pub(super) fn failure(&self) -> Option<&RunFailure> {
        self.failure.as_ref()
    }
    pub(super) fn pending_author(&self) -> Option<&super::recording::AuthorTicket> {
        self.barrier.pending_author()
    }
    pub(super) fn refused_author(&self) -> Option<&AuthoredEvent> {
        self.refused_author.as_ref()
    }

    pub(super) fn halt(&self) -> Option<&super::report::HaltLatch> {
        self.flow.halt()
    }

    /// External owners freeze their own pair, capture and input obligations at
    /// the same failure signal. This method reads the stopped flow without any
    /// transport progress or release. Reporting never replaces the first cause.
    #[allow(clippy::result_large_err)]
    pub(super) fn freeze_halt_account(
        &mut self,
        external: super::halt_snapshot::ExternalFacts,
    ) -> Result<(), HaltAccountFailure> {
        if external.input_id != self.input.snapshot().map(|input| input.input_id) {
            return Err(HaltAccountFailure::Snapshot(
                super::halt_snapshot::SnapshotFailure {
                    external,
                    error: super::report::ReportError::Invalid(
                        "input differs from retained transaction",
                    ),
                },
            ));
        }
        if self.failed_wait.is_some() && external.wait != self.failed_wait {
            return Err(HaltAccountFailure::Snapshot(
                super::halt_snapshot::SnapshotFailure {
                    external,
                    error: super::report::ReportError::Invalid("wait differs from latched bound"),
                },
            ));
        }
        if let Some(capture) = &self.member_capture
            && external.capture.as_ref().map(|facts| facts.capture_id) != Some(capture.id)
        {
            return Err(HaltAccountFailure::Snapshot(
                super::halt_snapshot::SnapshotFailure {
                    external,
                    error: super::report::ReportError::Invalid(
                        "capture differs from retained member transaction",
                    ),
                },
            ));
        }
        let account = super::halt_snapshot::snapshot(
            &self.flow,
            &self.barrier,
            &self.inbox,
            &self.budget,
            external,
        )
        .map_err(HaltAccountFailure::Snapshot)?;
        // snapshot requires this exact owner's latch before constructing an account.
        self.flow
            .halt_mut()
            .expect("snapshot requires latched flow")
            .install_account(account)
            .map_err(HaltAccountFailure::Install)
    }

    pub(super) fn halt_wait(&self) -> Option<&WaitAtHalt> {
        self.failed_wait.as_ref()
    }

    pub(super) fn begin_open(
        &mut self,
        start: StartIdentity,
        limits: ReadLimits,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        self.live(io)?;
        let result = self.flow.begin_open(
            &mut FlowRecording {
                barrier: &mut self.barrier,
                inbox: &mut self.inbox,
                budget: &mut self.budget,
                author: &self.author,
            },
            io,
            start.clone(),
            limits.clone(),
        );
        result.map_err(|failure| self.flow_failed(io, failure))?;
        self.open_start = Some(start);
        self.open_limits = Some(limits);
        Ok(())
    }

    pub(super) fn opened_identity(
        &self,
    ) -> Option<&weaver_types::state_flow::MemberObservationIdentity> {
        self.flow.opened_identity()
    }

    pub(super) fn open_refusal(
        &self,
    ) -> Option<(weaver_types::state_flow::StateRefusalCode, &str)> {
        self.flow.open_refusal()
    }

    pub(super) fn begin_classifier(
        &mut self,
        end: OwnedFd,
        bootstrap: Ste100Bootstrap,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        self.live(io)?;
        if !self.load_duty.as_ref().is_some_and(|duty| {
            duty.scheme_text == bootstrap.scheme_text
                && duty.instruction == super::mirror::instruction(&bootstrap.instruction)
        }) {
            return Err(self.fail(
                io,
                FlowHaltReason::ProtocolFailed,
                HaltPoint::Barrier,
                "classifier bootstrap differs from the recorded load duty",
            ));
        }
        let result = self.flow.begin_classifier_admission(
            &mut FlowRecording {
                barrier: &mut self.barrier,
                inbox: &mut self.inbox,
                budget: &mut self.budget,
                author: &self.author,
            },
            end,
            bootstrap,
        );
        result.map_err(|failure| self.flow_failed(io, failure))
    }

    pub(super) fn take_operation_completion(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<Option<OperationCompletion>, RunFailure> {
        self.live(io)?;
        // A template result belongs to its ordered transcript, never a second
        // operation consumer. Correct rejection keeps that boundary closed.
        if self.templates_block_content() || self.output.is_some() {
            return Ok(None);
        }
        let result = self.flow.take_operation_completion(&mut FlowRecording {
            barrier: &mut self.barrier,
            inbox: &mut self.inbox,
            budget: &mut self.budget,
            author: &self.author,
        });
        result.map_err(|failure| self.flow_failed(io, failure))
    }

    pub(super) fn take_classifier_admission(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<Option<super::ste100_socket::AdmissionPoll>, RunFailure> {
        self.live(io)?;
        let result = self.flow.take_admission_completion(&mut FlowRecording {
            barrier: &mut self.barrier,
            inbox: &mut self.inbox,
            budget: &mut self.budget,
            author: &self.author,
        });
        result.map_err(|failure| self.flow_failed(io, failure))
    }

    pub(super) fn retire_refused_classifier(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        self.live(io)?;
        let result = self.flow.retire_refused_classifier(&mut FlowRecording {
            barrier: &mut self.barrier,
            inbox: &mut self.inbox,
            budget: &mut self.budget,
            author: &self.author,
        });
        result.map_err(|failure| self.flow_failed(io, failure))
    }

    /// The owner of a contract wait supplies its original absolute window.
    /// Progress cannot arm the same key again or lengthen its remaining time.
    pub(super) fn arm(
        &mut self,
        key: PumpDeadline,
        bound: AbsoluteBound,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        self.live(io)?;
        self.deadlines.arm(key, bound).map_err(|error| {
            self.fail(
                io,
                FlowHaltReason::ProtocolFailed,
                HaltPoint::Barrier,
                error.to_string(),
            )
        })
    }

    pub(super) fn complete_wait(&mut self, key: PumpDeadline) -> Option<AbsoluteBound> {
        if self.failure.is_some()
            || self.phase == RecordingPhase::Closed
            || self.filing_wait_still_owned(key)
            || self.output_wait_still_owned(key)
            || (key == PumpDeadline::Enter
                && self.phase == RecordingPhase::Entered
                && self
                    .template_checks
                    .as_ref()
                    .is_some_and(|owner| owner.can_certify()))
        {
            return None;
        }
        self.deadlines.disarm(key)
    }

    pub(super) fn begin_releasing(
        &mut self,
        leave: AbsoluteBound,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        self.live(io)?;
        if self.output.is_some()
            || self.filing.is_some()
            || self.input.snapshot().is_some()
            || self.input_turn.is_some()
            || self.member_capture.is_some()
            || self.open_turn.is_some()
        {
            return Err(self.fail(
                io,
                FlowHaltReason::ProtocolFailed,
                HaltPoint::Barrier,
                "release requires settled input, a closed turn and completed capture",
            ));
        }
        if let Some(armed) = self.deadlines.get(PumpDeadline::Leave) {
            if armed != leave {
                return Err(self.fail(
                    io,
                    FlowHaltReason::ProtocolFailed,
                    HaltPoint::Barrier,
                    "release cannot replace the existing leave window",
                ));
            }
        } else {
            self.arm(PumpDeadline::Leave, leave, io)?;
        }
        let result = self.flow.begin_releasing(&mut FlowRecording {
            barrier: &mut self.barrier,
            inbox: &mut self.inbox,
            budget: &mut self.budget,
            author: &self.author,
        });
        result.map_err(|failure| self.flow_failed(io, failure))?;
        self.phase = RecordingPhase::Releasing;
        Ok(())
    }

    /// Only the lifecycle's explicit rollback directive calls this after the
    /// enter refusal was recorded. A refused partition never activates the tap,
    /// grants the unused Open result, or takes a save point.
    pub(super) fn begin_enter_rollback(
        &mut self,
        leave: AbsoluteBound,
        io: &mut impl PumpIo,
    ) -> Result<Advance, RunFailure> {
        self.live(io)?;
        if self.phase != RecordingPhase::Entered || !self.barrier.is_refused() {
            return Err(self.fail(
                io,
                FlowHaltReason::ProtocolFailed,
                HaltPoint::Barrier,
                "enter rollback requires this context's recorded refusal",
            ));
        }
        if let Some(armed) = self.deadlines.get(PumpDeadline::Leave) {
            if armed != leave {
                return Err(self.fail(
                    io,
                    FlowHaltReason::ProtocolFailed,
                    HaltPoint::Barrier,
                    "rollback cannot replace the existing leave window",
                ));
            }
        } else {
            self.arm(PumpDeadline::Leave, leave, io)?;
        }
        let result = self.flow.begin_enter_rollback(
            &mut FlowRecording {
                barrier: &mut self.barrier,
                inbox: &mut self.inbox,
                budget: &mut self.budget,
                author: &self.author,
            },
            io,
        );
        let advance = result.map_err(|failure| self.flow_failed(io, failure))?;
        if advance == Advance::Progress {
            self.phase = RecordingPhase::Releasing;
        }
        Ok(advance)
    }

    pub(super) fn retire_state(&mut self, io: &mut impl PumpIo) -> Result<Advance, RunFailure> {
        self.live(io)?;
        let result = self.flow.retire_state(
            &mut FlowRecording {
                barrier: &mut self.barrier,
                inbox: &mut self.inbox,
                budget: &mut self.budget,
                author: &self.author,
            },
            io,
        );
        result.map_err(|failure| self.flow_failed(io, failure))
    }

    pub(super) fn retire_classifier(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<Advance, RunFailure> {
        self.live(io)?;
        let result = self.flow.retire_classifier(&mut FlowRecording {
            barrier: &mut self.barrier,
            inbox: &mut self.inbox,
            budget: &mut self.budget,
            author: &self.author,
        });
        result.map_err(|failure| self.flow_failed(io, failure))
    }

    pub(super) fn close_after_drain<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<(), RunFailure> {
        self.live(ports.io)?;
        if self.phase != RecordingPhase::Releasing || !self.barrier.is_closed() {
            return Err(self.fail(
                ports.io,
                FlowHaltReason::ProtocolFailed,
                HaltPoint::Teardown,
                "close requires the recorded unload of a releasing context",
            ));
        }
        if self.flow.phase() != CoordinatorPhase::Closed {
            let result = self.flow.close(&mut FlowRecording {
                barrier: &mut self.barrier,
                inbox: &mut self.inbox,
                budget: &mut self.budget,
                author: &self.author,
            });
            result.map_err(|failure| self.flow_failed(ports.io, failure))?;
        }
        loop {
            match self.barrier.drain_status(&mut self.budget) {
                Ok(serving::DrainStatus::Emptied) => {
                    self.live(ports.io)?;
                    self.phase = RecordingPhase::Closed;
                    return Ok(());
                }
                Ok(serving::DrainStatus::Pending { .. }) => match self.pump_once(ports) {
                    Err(failure) => {
                        self.barrier.abandon();
                        return Err(failure);
                    }
                    Ok(PumpStep::Deadline { key, bound }) => {
                        self.barrier.abandon();
                        return Err(RunFailure::Wait { key, bound });
                    }
                    Ok(_) => {}
                },
                Err(_) => {
                    self.barrier.abandon();
                    return Err(self.record_failed(ports.io));
                }
            }
        }
    }

    /// Only complete, unchanged original author input enters this road. While
    /// its record waits, transport may fill slots but no completion is reduced.
    pub(super) fn author_and_wait<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
        event: AuthoredEvent,
    ) -> Result<Sequence, RunFailure> {
        if let Err(failure) = self.live(ports.io) {
            self.retain_refused(event);
            return Err(failure);
        }
        if self.filing.is_some() || self.output.is_some() {
            self.retain_refused(event);
            return Err(self.fail(
                ports.io,
                FlowHaltReason::ProtocolFailed,
                HaltPoint::Author,
                "another author entered an owned content transaction",
            ));
        }
        if self.template_blocked_author.is_some() {
            // The first refused input has not returned to its caller. A second
            // author violates the one-owner protocol. Preserve both events:
            // the first in its refusal slot, this one as terminal evidence.
            self.retain_refused(event);
            return Err(self.fail(
                ports.io,
                FlowHaltReason::ProtocolFailed,
                HaltPoint::Author,
                "previous template-blocked author has not been taken",
            ));
        }
        if self.templates_block_content()
            && (serving::content_schema(event.kind) != ContentSchema::None
                || event.kind == Kind::TurnStarted)
        {
            // This is refused caller input, never a pending author ticket.
            // The caller takes it unchanged before attempting another author.
            // resume_author_and_wait cannot turn it into admitted content.
            self.template_blocked_author = Some(event);
            return Err(RunFailure::TemplateBlocked);
        }
        if !self.author_allowed(&event) {
            self.retain_refused(event);
            return Err(self.fail(
                ports.io,
                FlowHaltReason::ProtocolFailed,
                HaltPoint::Author,
                "author kind is not legal in this recording phase",
            ));
        }
        if self.barrier.pending().is_some()
            || self.barrier.pending_author().is_some()
            || self.waiting_content.is_some()
            || self.completed_author.is_some()
            || self.flow.has_pending_record()
        {
            self.retain_refused(event);
            return Err(self.fail(
                ports.io,
                FlowHaltReason::ProtocolFailed,
                HaltPoint::Author,
                "second author while a ticket is pending",
            ));
        }
        self.waiting_content = Some(event);
        self.continue_waiting_content(ports)
    }

    fn continue_waiting_content<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<Sequence, RunFailure> {
        let kind = self
            .waiting_content
            .as_ref()
            .expect("caller input is owned")
            .kind;
        let purpose = if serving::content_schema(kind) == ContentSchema::None {
            RecordPurpose::Observation
        } else {
            RecordPurpose::Content
        };
        // The existing content barrier owns backlog waiting. Do not turn a
        // whole copy's lack of room into a component failure at author intake.
        while matches!(purpose, RecordPurpose::Content) && self.budget.waiting_copy().is_some() {
            let advance = self.reduce_flow(ports.io)?;
            if self.budget.waiting_copy().is_some()
                && advance != Advance::Progress
                && let PumpStep::Deadline { key, bound } = self.pump_once(ports)?
            {
                return Err(RunFailure::Wait { key, bound });
            }
        }
        let event = self
            .waiting_content
            .take()
            .expect("caller input remained owned");
        if let Some(Payload::Elections(load)) = &event.payload {
            // Only Recorded load can advance the phase and permit admission.
            // Keep its precise duty even across a trace-capacity interruption.
            self.load_duty = Some(LoadDuty {
                scheme_text: load.ste100_scheme.clone(),
                instruction: load.state_flow.ste100.clone(),
                persistence: load.state_flow.persistence.clone(),
                template_inventory: load.template_inventory.clone(),
            });
        }
        let wall_ms = match wall_ms() {
            Ok(wall_ms) => wall_ms,
            Err(detail) => {
                self.retain_refused(event);
                return Err(self.fail(
                    ports.io,
                    FlowHaltReason::TraceFailed,
                    HaltPoint::Author,
                    detail,
                ));
            }
        };
        self.author_turn = Some((event.kind, event.turn.clone()));
        let progress = self.barrier.submit_authored_with_clock(
            event,
            purpose,
            &self.author,
            wall_ms,
            &mut self.inbox,
            &mut self.budget,
            &mut || ports.io.now(),
        );
        self.wait_recorded(ports, progress)
    }

    /// A lifecycle or legacy-label wait can interrupt a nested author. Its
    /// owner handles that typed outcome, then resumes this same ticket.
    pub(super) fn resume_author_and_wait<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<Sequence, RunFailure> {
        self.live(ports.io)?;
        if self.filing.is_some() || self.output.is_some() {
            return Err(self.fail(
                ports.io,
                FlowHaltReason::ProtocolFailed,
                HaltPoint::Author,
                "generic resume cannot consume a content owner's author",
            ));
        }
        if let Some(sequence) = self.completed_author.take() {
            return Ok(sequence);
        }
        if self.waiting_content.is_some() {
            // A backlog wait can suspend before the caller owns a trace ticket.
            // Continue that original input, including any coordinator-owned
            // measurement which must record before the backlog can move.
            return self.continue_waiting_content(ports);
        }
        if self.flow.has_pending_record() || self.barrier.pending_author().is_none() {
            return Err(self.fail(
                ports.io,
                FlowHaltReason::ProtocolFailed,
                HaltPoint::Author,
                "resume requires this caller's pending author",
            ));
        }
        let progress =
            self.barrier
                .resume_with_clock(&mut self.inbox, &mut self.budget, &mut || ports.io.now());
        self.wait_recorded(ports, progress)
    }

    fn wait_recorded<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
        mut progress: RecordProgress,
    ) -> Result<Sequence, RunFailure> {
        loop {
            self.drain_intake(ports.io)?;
            match progress {
                RecordProgress::Recorded {
                    sequence,
                    proof: None,
                } => {
                    self.last_author_sequence = Some(sequence);
                    if let Some((kind, turn)) = self.author_turn.take() {
                        if kind == Kind::TurnStarted {
                            self.open_turn = turn;
                        } else if kind == Kind::TurnClosed && self.open_turn == turn {
                            self.open_turn = None;
                        }
                    }
                    if self.phase == RecordingPhase::Preflight {
                        self.phase = RecordingPhase::Entered;
                    }
                    self.completed_author = Some(sequence);
                    self.bind_filing_author(sequence, ports.io)?;
                    self.bind_output_author(sequence, ports.io)?;
                    self.live(ports.io)?;
                    return Ok(self
                        .completed_author
                        .take()
                        .expect("recorded caller result is owned"));
                }
                RecordProgress::Pending => {
                    if let PumpStep::Deadline { key, bound } = self.pump_once(ports)? {
                        return Err(RunFailure::Wait { key, bound });
                    }
                }
                RecordProgress::BusyAuthored { event, .. } => {
                    self.retain_refused(event);
                    return Err(self.record_failed(ports.io));
                }
                RecordProgress::Failed => return Err(self.record_failed(ports.io)),
                RecordProgress::Busy { .. } | RecordProgress::Recorded { proof: Some(_), .. } => {
                    return Err(self.fail(
                        ports.io,
                        FlowHaltReason::ProtocolFailed,
                        HaltPoint::Author,
                        "ordinary author received another owner's proof",
                    ));
                }
            }
            progress =
                self.barrier
                    .resume_with_clock(&mut self.inbox, &mut self.budget, &mut || ports.io.now());
        }
    }

    fn author_allowed(&self, event: &AuthoredEvent) -> bool {
        if self.member_capture.is_some()
            && (serving::content_schema(event.kind) != ContentSchema::None
                || event.kind == Kind::TurnStarted)
        {
            return false;
        }
        match self.phase {
            RecordingPhase::Preflight => {
                event.kind == Kind::Load
                    && self.flow.opened_identity().is_some()
                    && matches!(&event.payload, Some(Payload::Elections(load))
                    if self.load_matches(load))
            }
            RecordingPhase::Entered => !matches!(event.kind, Kind::Load | Kind::Unload),
            RecordingPhase::Releasing => {
                serving::content_schema(event.kind) == ContentSchema::None
                    && matches!(
                        event.kind,
                        Kind::Unload | Kind::SessionClosed | Kind::Fault | Kind::Refusal
                    )
            }
            RecordingPhase::Failed | RecordingPhase::Closed => false,
        }
    }

    fn load_matches(&self, load: &serving::payloads::LoadRecord) -> bool {
        load.origin_ns.0 == self.origin.raw()
            && load.clock_source == serving::payloads::ClockSource::ClockMonotonic
            && super::mirror::matches_flow(&self.declaration, &load.state_flow)
            && load.template_inventory.scheme_id == self.declaration.scheme.scheme_id
            && load.template_inventory.checker_artifact_sha256
                == load.state_flow.ste100.model_artifact_sha256
            && self
                .open_start
                .as_ref()
                .is_some_and(|start| load.state_start == super::mirror::start(start))
            && self.open_limits.as_ref().is_some_and(|limits| {
                load.state_flow.reads.max_result_bytes.0 == limits.max_result_bytes.0
                    && load.state_flow.reads.measurement_ref == limits.measurement_ref
            })
            && self.flow.opened_identity().is_some_and(|member| {
                load.state_member_observation == super::mirror::member(member)
            })
    }

    /// The declaration is converted field for field. No caller can replace it
    /// with a differently validated partition after the recorded load.
    pub(super) fn activate(&mut self, io: &mut impl PumpIo) -> Result<(), ActivationFailure> {
        self.live(io).map_err(ActivationFailure::Run)?;
        if self.phase != RecordingPhase::Entered || !self.barrier.is_loaded() {
            return Err(ActivationFailure::Run(self.fail(
                io,
                FlowHaltReason::ProtocolFailed,
                HaltPoint::Author,
                "activation requires this context's recorded load",
            )));
        }
        let partition = serving::Partition {
            rules: self
                .declaration
                .partition
                .iter()
                .map(|rule| serving::PartitionRule {
                    id: rule.id.clone(),
                    kind: rule.kind.clone(),
                    channels: rule.channels.clone(),
                    origins: rule.origins.clone(),
                    blocks: rule.blocks.clone(),
                    context: rule.context.clone(),
                })
                .collect(),
        };
        let partition =
            serving::validate_partition(&partition).map_err(ActivationFailure::Partition)?;
        self.barrier.activate(partition).map_err(|error| {
            ActivationFailure::Run(self.fail(
                io,
                FlowHaltReason::TraceFailed,
                HaltPoint::Author,
                format!("{error:?}"),
            ))
        })?;
        let result = self.flow.activate_after_load(&mut FlowRecording {
            barrier: &mut self.barrier,
            inbox: &mut self.inbox,
            budget: &mut self.budget,
            author: &self.author,
        });
        result.map_err(|failure| ActivationFailure::Run(self.flow_failed(io, failure)))
    }

    /// Perform bounded transport progress only. Holding an answer never gives
    /// it to a model or lifecycle consumer before that consumer's trace barrier.
    pub(super) fn pump_once<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<PumpStep, RunFailure> {
        self.live(ports.io)?;
        if self.phase == RecordingPhase::Preflight
            && (ports.gate.is_some()
                || ports.decode.is_some()
                || ports.label.is_some()
                || ports.lifecycle.residency.is_some())
        {
            return Err(self.fail(
                ports.io,
                FlowHaltReason::ProtocolFailed,
                HaltPoint::Barrier,
                "preflight pump received a serving port before load",
            ));
        }
        let step = self.pump.step(
            ports,
            Some(&mut self.flow),
            &mut FlowRecording {
                barrier: &mut self.barrier,
                inbox: &mut self.inbox,
                budget: &mut self.budget,
                author: &self.author,
            },
            &self.deadlines,
        );
        match step {
            Ok(PumpStep::Deadline { key, bound }) => Err(self.expired(ports.io, key, bound)),
            Ok(step) => Ok(step),
            Err(PumpFailure::Transport { source, failure }) => {
                Err(RunFailure::Transport { source, failure })
            }
            Err(error) => {
                if self.flow.failure().is_some() {
                    return Err(self.flow_failed(ports.io, self.flow.failure().unwrap().clone()));
                }
                if self.barrier.failure().is_some() || self.barrier.standing_failure().is_some() {
                    return Err(self.record_failed(ports.io));
                }
                Err(self.fail(
                    ports.io,
                    FlowHaltReason::TraceFailed,
                    HaltPoint::Author,
                    format!("{error:?}"),
                ))
            }
        }
    }

    /// Reduce only the state/classifier owner's exact recorded completions.
    /// Lifecycle, gate, decoder and label completions remain in their own slots.
    pub(super) fn reduce_flow(&mut self, io: &mut impl PumpIo) -> Result<Advance, RunFailure> {
        self.live(io)?;
        let wall_ms = wall_ms().map_err(|detail| {
            self.fail(io, FlowHaltReason::TraceFailed, HaltPoint::Author, detail)
        })?;
        let result = self.flow.advance(
            &mut FlowRecording {
                barrier: &mut self.barrier,
                inbox: &mut self.inbox,
                budget: &mut self.budget,
                author: &self.author,
            },
            io,
            wall_ms,
        );
        let advance = result.map_err(|failure| self.flow_failed(io, failure))?;
        self.settle_recorded_input(io)?;
        self.settle_recorded_capture(io)?;
        self.live(io)?;
        Ok(advance)
    }

    fn drain_intake(&mut self, io: &mut impl PumpIo) -> Result<(), RunFailure> {
        let result = self.flow.drain(
            &mut FlowRecording {
                barrier: &mut self.barrier,
                inbox: &mut self.inbox,
                budget: &mut self.budget,
                author: &self.author,
            },
            io,
        );
        result.map_err(|failure| self.flow_failed(io, failure))
    }

    fn live(&mut self, io: &mut impl PumpIo) -> Result<(), RunFailure> {
        if let Some(failure) = &self.failure {
            return Err(failure.clone());
        }
        if self.phase == RecordingPhase::Closed {
            return Err(RunFailure::Closed);
        }
        if let Some(failure) = self.flow.failure().cloned() {
            return Err(self.flow_failed(io, failure));
        }
        if self.barrier.failure().is_some() || self.barrier.standing_failure().is_some() {
            return Err(self.record_failed(io));
        }
        let now = io.now().map_err(|error| {
            self.fail(
                io,
                FlowHaltReason::TraceFailed,
                HaltPoint::Author,
                format!("common clock failed: {error}"),
            )
        })?;
        if now < self.last_ns {
            return Err(self.fail(
                io,
                FlowHaltReason::TraceFailed,
                HaltPoint::Author,
                "common clock reversed",
            ));
        }
        self.last_ns = now;
        let expired = self
            .deadlines
            .expired(now)
            .min_by_key(|(_, bound)| bound.deadline_ns());
        if let Some((key, bound)) = expired {
            return Err(self.expired(io, key, bound));
        }
        Ok(())
    }

    fn expired(
        &mut self,
        io: &mut impl PumpIo,
        key: PumpDeadline,
        bound: AbsoluteBound,
    ) -> RunFailure {
        use serving::payloads::BlockedStage;
        let (reason, stage) = match key {
            PumpDeadline::Gate => (FlowHaltReason::GateBound, BlockedStage::Gate),
            PumpDeadline::Cut => (FlowHaltReason::CutBound, BlockedStage::Cut),
            PumpDeadline::Decision if self.budget.waiting_copy().is_some() => {
                (FlowHaltReason::QueueLimit, BlockedStage::Decision)
            }
            PumpDeadline::Decision => (FlowHaltReason::DecisionBound, BlockedStage::Decision),
            PumpDeadline::StateControl => (FlowHaltReason::ControlBound, BlockedStage::Control),
            PumpDeadline::Enter
            | PumpDeadline::Leave
            | PumpDeadline::Label
            | PumpDeadline::CaptureResult
            | PumpDeadline::CaptureWindow => {
                return RunFailure::Wait { key, bound };
            }
        };
        self.failed_wait = Some(WaitAtHalt {
            stage,
            began_ns: bound.began_ns().into(),
            deadline_ns: bound.deadline_ns().into(),
        });
        self.fail(
            io,
            reason,
            HaltPoint::Barrier,
            format!(
                "{key:?} wait expired at {} from {}",
                bound.deadline_ns(),
                bound.began_ns()
            ),
        )
    }

    fn record_failed(&mut self, io: &mut impl PumpIo) -> RunFailure {
        let (reason, point) = match self.barrier.failure() {
            Some(RecordFailure::Tap(_)) => (FlowHaltReason::TapFailed, HaltPoint::Tap),
            Some(RecordFailure::Intake(_)) => (FlowHaltReason::TapFailed, HaltPoint::CopyIntake),
            Some(RecordFailure::Protocol(_) | RecordFailure::Author(_)) => {
                (FlowHaltReason::ProtocolFailed, HaltPoint::Author)
            }
            _ => (FlowHaltReason::TraceFailed, HaltPoint::Author),
        };
        self.fail(
            io,
            reason,
            point,
            format!(
                "record failure: {:?}, {:?}",
                self.barrier.failure(),
                self.barrier.standing_failure()
            ),
        )
    }

    fn flow_failed(&mut self, io: &mut impl PumpIo, failure: FlowFailure) -> RunFailure {
        let recording_failure =
            self.barrier.failure().is_some() || self.barrier.standing_failure().is_some();
        if failure.point == "author" && self.barrier.failure().is_some() {
            return self.record_failed(io);
        }
        let (reason, point) = if let Some(error) =
            self.flow.standing_state().and_then(StateChannel::failure)
        {
            let reason = match error.kind {
                ChannelFailureKind::Carry => FlowHaltReason::CarryFailed,
                ChannelFailureKind::FilingRefused => FlowHaltReason::FilingRefused,
                ChannelFailureKind::Protocol | ChannelFailureKind::OpenRefused => {
                    FlowHaltReason::ProtocolFailed
                }
            };
            let point = match error.point {
                ChannelPoint::Send => HaltPoint::StateSend,
                ChannelPoint::Reply => HaltPoint::StateReply,
                ChannelPoint::Counter => HaltPoint::Counter,
            };
            (reason, point)
        } else {
            match failure.point {
                "classifier" => {
                    let unsolicited = self.flow.standing_classifier().is_some_and(|owner| {
                        owner.outstanding().is_none()
                            && owner.failure().is_some_and(|failure| {
                                failure.kind == super::classifier::PortFailureKind::ProtocolFailed
                            })
                    });
                    let reason = if unsolicited {
                        FlowHaltReason::ProtocolFailed
                    } else {
                        FlowHaltReason::ClassifierFailed
                    };
                    (reason, HaltPoint::Classifier)
                }
                "copy_intake" => (FlowHaltReason::TapFailed, HaltPoint::CopyIntake),
                "clock" => (FlowHaltReason::TraceFailed, HaltPoint::Author),
                "counter" => (FlowHaltReason::ProtocolFailed, HaltPoint::Counter),
                "author" => (FlowHaltReason::ProtocolFailed, HaltPoint::Author),
                _ => (FlowHaltReason::ProtocolFailed, HaltPoint::Barrier),
            }
        };
        let detail = match self
            .flow
            .standing_classifier()
            .and_then(|owner| owner.outstanding())
        {
            Some(weaver_types::state_flow::Ste100Request {
                target: weaver_types::state_flow::Ste100Target::Template { template_id },
                ..
            }) if point == HaltPoint::Classifier => {
                format!("template {template_id}: {}", failure.detail)
            }
            _ => failure.detail,
        };
        let was_latched = self.failure.is_some();
        let result = self.fail(io, reason, point, detail);
        if !was_latched && recording_failure {
            // This coordinator failure was already observed before the writer
            // failure was inspected. Preserve that order in the single latch.
            let secondary = SecondaryFailure {
                reason: FlowHaltReason::TraceFailed,
                point: HaltPoint::Author,
                observed_ns: io.now().ok().map(Into::into),
                detail: format!(
                    "later record failure: {:?}, {:?}",
                    self.barrier.failure(),
                    self.barrier.standing_failure()
                ),
            };
            if let Some(halt) = self.flow.halt_mut() {
                // Allocation failure cannot replace the first cause or resume
                // serving. The first account is still owned by the latch.
                let _ = halt.append_secondary(secondary);
            }
        }
        result
    }

    fn retain_refused(&mut self, event: AuthoredEvent) {
        if self.refused_author.is_none() {
            self.refused_author = Some(event);
        }
    }

    fn fail(
        &mut self,
        io: &mut impl PumpIo,
        reason: FlowHaltReason,
        point: HaltPoint,
        detail: impl Into<String>,
    ) -> RunFailure {
        if let Some(failure) = &self.failure {
            return failure.clone();
        }
        let detail = detail.into();
        let failure = RunFailure::Flow {
            reason,
            point,
            detail: detail.clone(),
        };
        // Revoke before sampling or reporting. A broken clock cannot grant work.
        self.input.halt();
        self.phase = RecordingPhase::Failed;
        self.failure = Some(failure.clone());
        self.barrier.stop_for_flow_failure();
        self.budget.freeze();
        let first = SecondaryFailure {
            reason,
            point,
            detail,
            observed_ns: io.now().ok().map(Into::into),
        };
        let leave = self
            .deadlines
            .get(PumpDeadline::Leave)
            .map(AbsoluteBound::deadline_ns);
        self.flow.latch(
            first,
            leave,
            &mut FlowRecording {
                barrier: &mut self.barrier,
                inbox: &mut self.inbox,
                budget: &mut self.budget,
                author: &self.author,
            },
        );
        failure
    }
}

impl Drop for RecordingContext {
    fn drop(&mut self) {
        // A forgotten failed-exit cleanup cannot join a blocked trace sink.
        // Normal close has already completed the observable bounded drain.
        if self.phase != RecordingPhase::Closed {
            self.barrier.abandon();
        }
    }
}

fn wall_ms() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())
        .and_then(|duration| u64::try_from(duration.as_millis()).map_err(|e| e.to_string()))
}

#[cfg(test)]
#[path = "context_tests.rs"]
pub(in crate::flow) mod tests;

#[cfg(test)]
#[path = "context_edge_tests.rs"]
mod edge_tests;

#[cfg(test)]
#[path = "context_review_tests.rs"]
mod review_tests;

#[path = "context_input.rs"]
mod input_driver;
#[allow(unused_imports)] // Dormant serving boundary, consumed at lifecycle cutover.
pub(super) use input_driver::{InputBeginError, InputConsumeError, InputProgress, SeatedInput};

#[cfg(test)]
#[path = "context_input_tests.rs"]
mod input_tests;

#[path = "context_capture.rs"]
mod capture_driver;
#[allow(unused_imports)] // Dormant member port, consumed by the paired save lifecycle.
pub(super) use capture_driver::{CaptureAnswer, CaptureProgress};

#[cfg(test)]
#[path = "context_capture_tests.rs"]
mod capture_tests;

#[path = "context_templates.rs"]
mod template_driver;
#[allow(unused_imports)] // Dormant certification boundary, consumed by enter at cutover.
pub(super) use template_driver::{TemplateBeginError, TemplateBeginRefusal, TemplateProgress};

#[cfg(test)]
#[path = "context_template_tests.rs"]
mod template_tests;

#[path = "context_receipts.rs"]
mod receipt_driver;
#[allow(unused_imports)] // Dormant filing consumer, consumed by dependent primitive ports.
pub(super) use receipt_driver::{
    FilingBeginError, FilingBeginRefusal, FilingCancellation, FilingCompletion, FilingProgress,
};

#[cfg(test)]
#[path = "context_receipt_tests.rs"]
mod receipt_tests;

#[path = "context_output.rs"]
mod output_driver;
#[allow(unused_imports)] // Consumed by the complete repair and pair owner at cutover.
pub(super) use output_driver::{
    OutputBeginError, OutputBeginRefusal, OutputProgress, SourceReceiptError,
};

#[cfg(test)]
#[path = "context_output_tests.rs"]
mod output_tests;

#[cfg(test)]
#[path = "context_output_receipt_tests.rs"]
mod output_receipt_tests;
