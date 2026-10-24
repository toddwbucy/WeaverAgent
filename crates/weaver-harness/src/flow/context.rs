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
use super::recording::{OwedAuthor, RecordBarrier, RecordFailure, RecordProgress, RecordPurpose};
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
    /// Untaken caller input blocks a new boundary without failing the run.
    Busy(HeldInput),
    /// Certification owns the content boundary. This operation did not start
    /// and does not latch a component failure.
    TemplateBlocked,
    /// Another primitive owns this operation. This caller acquired no work.
    PairOwned,
    /// A repair transaction retains this author or pair account.
    RepairOwned,
    /// A measurement owns the decode exchange or its exact pending author.
    MeasureOwned,
    /// A read retains its result, recall author or completed proof.
    ReadOwned,
    /// The explicit mark operation retains input, read or filing custody.
    MarksOwned,
    /// A filing pause owns admission until its freshness result is taken.
    FreshnessOwned,
    /// A small inspection retains its control or recorded result.
    InspectionOwned,
    /// Dormant boot repair retains its reads, terminal authors and receipts.
    BootPairsOwned,
    BootOpenOwned,
    PlanSummaryOwned,
    PlaceOwned,
    EnterRefusalOwned,
}

/// Failure and custody are separate facts. An original returned here has no
/// recorder ticket or sequence. Moving this result must not discard that input.
#[derive(Debug)]
#[must_use]
pub(super) struct AuthorFailure {
    pub(super) reason: RunFailure,
    pub(super) original: Option<AuthoredEvent>,
}

impl From<RunFailure> for AuthorFailure {
    fn from(reason: RunFailure) -> Self {
        Self {
            reason,
            original: None,
        }
    }
}

impl AuthorFailure {
    /// Restore an unaccepted original into this attempt's existing owner. The
    /// caller keeps the reason as its stopped disposition; this grants no retry.
    fn retain(self, original: &mut Option<AuthoredEvent>) -> RunFailure {
        if let Some(event) = self.original {
            assert!(
                original.is_none(),
                "returned author already has an original"
            );
            *original = Some(event);
        }
        self.reason
    }

    #[cfg(test)]
    fn retained_reason(self) -> RunFailure {
        assert!(
            self.original.is_none(),
            "test must inspect returned author custody"
        );
        self.reason
    }
}

/// One caller's returned attempt, retained with its operation through teardown.
/// It is not a queue, a pending ticket, or authority to retry the event.
#[derive(Debug, Default)]
struct StoppedAuthor {
    failure: Option<AuthorFailure>,
}
impl StoppedAuthor {
    fn check(&self) -> Result<(), RunFailure> {
        match &self.failure {
            Some(failure) => Err(failure.reason.clone()),
            None => Ok(()),
        }
    }
    fn retain(&mut self, failure: AuthorFailure) -> RunFailure {
        if failure.original.is_some() {
            assert!(self.failure.is_none(), "stopped attempt cannot be replaced");
            let reason = failure.reason.clone();
            self.failure = Some(failure);
            reason
        } else {
            failure.reason
        }
    }
    fn retain_started(&mut self, failure: AuthorFailure, started: &mut bool) -> RunFailure {
        *started = failure.original.is_none();
        self.retain(failure)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum HeldInput {
    Caller,
    Template,
    Both,
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
    placement: serving::payloads::PlacementConfig,
    scheme_text: String,
    instruction: serving::payloads::Ste100Instruction,
    persistence: serving::payloads::PersistenceConfig,
    template_inventory: serving::payloads::TemplateInventory,
    tools: serving::payloads::ToolPolicies,
}

/// A private owner's original is queued under this exact identity, never
/// inferred from event shape or from another owner's empty completion slot.
#[derive(Debug, Clone)]
enum AuthorClaim {
    Placement(std::sync::Arc<()>),
    Measurement(std::sync::Arc<()>),
    EnterRefusal(std::sync::Arc<()>),
}

impl AuthorClaim {
    fn matches(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Placement(a), Self::Placement(b))
            | (Self::Measurement(a), Self::Measurement(b))
            | (Self::EnterRefusal(a), Self::EnterRefusal(b)) => std::sync::Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}

/// The exact waiting original and its private owner travel in one carrier.
/// Ordinary authors have no claim and cannot be adopted by an owner later.
#[derive(Debug)]
struct WaitingAuthor {
    event: AuthoredEvent,
    claim: Option<AuthorClaim>,
    // A freshness continuation may withdraw the original caller's attempt.
    // Reuse this same slot until that caller takes the failure, never resubmit it.
    withdrawn: Option<RunFailure>,
}
impl WaitingAuthor {
    fn ordinary(event: AuthoredEvent) -> Self {
        Self {
            event,
            claim: None,
            withdrawn: None,
        }
    }
    fn claimed(event: AuthoredEvent, claim: AuthorClaim) -> Self {
        Self {
            event,
            claim: Some(claim),
            withdrawn: None,
        }
    }
    fn matches(&self, claim: Option<&AuthorClaim>) -> bool {
        match (&self.claim, claim) {
            (None, None) => true,
            (Some(a), Some(b)) => a.matches(b),
            _ => false,
        }
    }
    fn into_event(self) -> AuthoredEvent {
        self.event
    }
}
impl std::ops::Deref for WaitingAuthor {
    type Target = AuthoredEvent;
    fn deref(&self) -> &AuthoredEvent {
        &self.event
    }
}

/// Shared custody facts; only a real owner's constructor mints identity, and
/// only the barrier's allocated/Recorded ticket can advance these fields.
#[derive(Debug)]
struct AuthorRecord {
    returned: StoppedAuthor,
    identity: std::sync::Arc<()>,
    ticket: Option<u64>,
    sequence: Option<Sequence>,
}
impl AuthorRecord {
    fn new() -> Self {
        Self {
            returned: StoppedAuthor::default(),
            identity: std::sync::Arc::new(()),
            ticket: None,
            sequence: None,
        }
    }
    fn retain_failure(&mut self, failure: AuthorFailure) -> RunFailure {
        if failure.original.is_some() {
            assert!(
                self.sequence.is_none(),
                "returned original cannot be Recorded"
            );
            self.ticket = None;
        }
        self.returned.retain(failure)
    }
    fn claim(&mut self, identity: &std::sync::Arc<()>, ticket: u64) -> bool {
        if !std::sync::Arc::ptr_eq(&self.identity, identity)
            || self.sequence.is_some()
            || self.ticket.is_some_and(|held| held != ticket)
        {
            return false;
        }
        self.ticket = Some(ticket);
        true
    }
    fn bind(&mut self, ticket: Option<u64>, sequence: Sequence) -> bool {
        if self.ticket.is_none() || self.ticket != ticket || self.sequence.is_some() {
            return false;
        }
        self.sequence = Some(sequence);
        true
    }
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
    enter_refusal_record: Option<AuthorRecord>,
    template_checks: Option<template_driver::TemplateChecks>,
    template_setup_refusal: Option<super::templates::TemplateError>,
    template_blocked_author: Option<AuthoredEvent>,
    input: super::input::InputSlot,
    input_author: Option<input_driver::InputAuthor>,
    input_turn: Option<input_driver::InputTurn>,
    input_turns: std::collections::BTreeSet<String>,
    member_capture: Option<capture_driver::MemberCapture>,
    freshness: Option<freshness_driver::FreshnessTransaction>,
    read: Option<read_driver::ReadTransaction>,
    place_engine: Option<super::place::PlaceEngine>,
    place_turn_stopped: bool,
    place_leaving: bool,
    place_revoked_grant: Option<std::sync::Arc<()>>,
    place: Option<place_driver::PlaceTransaction>,
    generation: Option<place_driver::GenerationTransaction>,
    reload: Option<place_driver::ReloadTransaction>,
    place_instructions: Vec<FilingCompletion>,
    place_stopped_measure: Option<measure_driver::StoppedPlaceMeasure>,
    native_marks: Option<NativeMarkTransaction>,
    place_notice: Option<place_driver::PlaceNotice>,
    place_close: Option<weaver_types::TurnFrame>,
    plan_cache: super::plan::PlanSummaryCache,
    plan_summary: Option<plan_summary_driver::PlanSummaryTransaction>,
    retired_enter: Option<AbsoluteBound>,
    inspection: Option<inspection_driver::InspectionTransaction>,
    marks: Option<marks_driver::MarksTransaction>,
    // Pointer identity survives moves and is never recreated from trace IDs.
    read_origin: std::sync::Arc<()>,
    content_read_started: bool,
    boot_read_started: bool,
    boot_head: Option<read_driver::BootHead>,
    boot_open: Option<boot_open_driver::BootOpenTransaction>,
    boot_enter_cancelled: bool,
    boot_journal: Option<super::journal::Journal>,
    boot_pairs_started: bool,
    boot_pairs: Option<boot_pair_driver::BootPairsTransaction>,
    measure: Option<measure_driver::MeasureTransaction>,
    filing: Option<receipt_driver::FilingTransaction>,
    output: Option<output_driver::OutputTransaction>,
    pairs: pairs::PairLedger,
    pair_work: Option<pair_driver::PairTransaction>,
    pair_gate_identity: Option<crate::flow::pump_transport::PortIdentity>,
    pair_rejected_authoring: bool,
    pair_rejected_return: StoppedAuthor,
    pair_rejection_wait: Option<(AbsoluteBound, bool)>,
    pair_refused_gate: Option<crate::flow::pump_transport::ReceivedOrgan>,
    repair: Option<repair_driver::RepairTransaction>,
    repair_certificate: Option<super::templates::TemplateCertificate>,
    last_author_sequence: Option<Sequence>,
    author_turn: Option<(Kind, Option<weaver_trace::TurnRef>)>,
    open_turn: Option<weaver_trace::TurnRef>,
    ever_started_turn: bool,
    failed_enter_resources_issued: bool,
    /// A caller's source remains owned if refusal happens before a ticket.
    /// It is distinct from the recorder's first pending author.
    // First fatal original is immutable; retryable holds have separate custody.
    refused_author: Option<AuthoredEvent>,
    held_author: Option<AuthoredEvent>,
    waiting_content: Option<WaitingAuthor>,
    #[cfg(test)]
    reject_queued_claim_for_test: bool,
    #[cfg(test)]
    pause_claim_queue_for_test: bool,
    // An initial lifecycle wait retained a valid original before admission.
    // Its retry must pass the generic admission checks again.
    author_admission_pending: bool,
    completed_author: Option<Sequence>,
    release_author: Option<release_driver::ReleaseAttempt>,
    failure: Option<RunFailure>,
    failed_wait: Option<WaitAtHalt>,
    halt_report: Option<halt_driver::HaltReport>,
    halt_gate: Option<halt_gate_driver::HaltGate>,
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
        let pairs = pairs::PairLedger::new(
            flow.state().identity().0.to_owned(),
            run.clone(),
            declaration.scheme.clone(),
        )
        .map_err(|error| ContextError(format!("pair identity: {error:?}")))?;
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
            enter_refusal_record: None,
            template_checks: None,
            template_setup_refusal: None,
            template_blocked_author: None,
            input,
            input_author: None,
            input_turn: None,
            input_turns: std::collections::BTreeSet::new(),
            member_capture: None,
            freshness: None,
            read: None,
            place_engine: None,
            place_turn_stopped: false,
            place_leaving: false,
            place_revoked_grant: None,
            place: None,
            generation: None,
            reload: None,
            place_instructions: vec![],
            place_stopped_measure: None,
            native_marks: None,
            place_close: None,
            place_notice: None,
            plan_cache: super::plan::PlanSummaryCache::default(),
            plan_summary: None,
            retired_enter: None,
            inspection: None,
            marks: None,
            read_origin: std::sync::Arc::new(()),
            content_read_started: false,
            boot_read_started: false,
            boot_head: None,
            boot_open: None,
            boot_enter_cancelled: false,
            boot_journal: None,
            boot_pairs_started: false,
            boot_pairs: None,
            measure: None,
            filing: None,
            output: None,
            pairs,
            pair_work: None,
            pair_gate_identity: None,
            pair_rejected_authoring: false,
            pair_rejected_return: StoppedAuthor::default(),
            pair_rejection_wait: None,
            pair_refused_gate: None,
            repair: None,
            repair_certificate: None,
            last_author_sequence: None,
            author_turn: None,
            open_turn: None,
            ever_started_turn: false,
            failed_enter_resources_issued: false,
            refused_author: None,
            held_author: None,
            waiting_content: None,
            #[cfg(test)]
            reject_queued_claim_for_test: false,
            #[cfg(test)]
            pause_claim_queue_for_test: false,
            author_admission_pending: false,
            completed_author: None,
            release_author: None,
            failure: None,
            failed_wait: None,
            halt_report: None,
            halt_gate: None,
        })
    }

    /// Shared private identity for adoption joins; only the actual receiver can
    /// attach it to its retained original Enter.
    pub(in crate::flow) fn enter_origin(&self) -> &std::sync::Arc<()> {
        &self.read_origin
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
    /// Original associated with the first fatal author refusal. Never retryable.
    pub(super) fn refused_author(&self) -> Option<&AuthoredEvent> {
        self.refused_author.as_ref()
    }

    /// First nonfatal owner hold. A later attempt cannot replace this original.
    pub(super) fn held_author(&self) -> Option<&AuthoredEvent> {
        self.held_author.as_ref()
    }

    /// Return the exact unadmitted original. Taking it does not grant admission:
    /// retry must pass the same owner/phase/deadline checks as any new attempt.
    pub(super) fn take_held_author(&mut self) -> Option<AuthoredEvent> {
        self.held_author.take()
    }

    /// Caller input has not returned to its owner yet. It is not a pending
    /// ticket, and cannot be dropped by starting another owner or closing.
    fn caller_author_held(&self) -> bool {
        self.held_input().is_some()
    }

    fn held_input(&self) -> Option<HeldInput> {
        match (
            self.held_author.is_some(),
            self.template_blocked_author.is_some(),
        ) {
            (false, false) => None,
            (true, false) => Some(HeldInput::Caller),
            (false, true) => Some(HeldInput::Template),
            (true, true) => Some(HeldInput::Both),
        }
    }

    /// An earlier admitted owner may continue only through its existing private
    /// proof. Template-held input still excludes a new content continuation.
    fn caller_author_blocks_continuation(&self, owns: impl FnOnce() -> bool) -> bool {
        match self.held_input() {
            None => false,
            Some(HeldInput::Caller) => !owns(),
            Some(HeldInput::Template | HeldInput::Both) => true,
        }
    }

    /// Pure custody check, after the boundary's liveness and prerequisite checks
    /// and before its first transition. A held-only refusal grants no new time.
    fn check_held_input(&self) -> Result<(), RunFailure> {
        match self.held_input() {
            Some(held) => Err(RunFailure::Busy(held)),
            None => Ok(()),
        }
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
        if external.outstanding_pairs != self.outstanding_pairs() {
            return Err(HaltAccountFailure::Snapshot(
                super::halt_snapshot::SnapshotFailure {
                    external,
                    error: super::report::ReportError::Invalid("pairs differ from retained owner"),
                },
            ));
        }
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
        if self.place_blocks_external() {
            return Err(RunFailure::PlaceOwned);
        }
        if self.plan_summary_blocks_external() {
            return Err(RunFailure::PlanSummaryOwned);
        }
        if self.boot_open_blocks_external() {
            return Err(RunFailure::BootOpenOwned);
        }
        if self.boot_pairs.is_some() {
            return Err(RunFailure::BootPairsOwned);
        }
        if self.marks.is_some() {
            return Err(RunFailure::MarksOwned);
        }
        if self.inspection.is_some() {
            return Err(RunFailure::InspectionOwned);
        }
        if self.templates_block_content() || self.output.is_some() || self.read.is_some() {
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
        if self.place_owed() {
            return Err(RunFailure::PlaceOwned);
        }
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
        })?;
        if key == PumpDeadline::Enter {
            self.retired_enter = None;
        }
        Ok(())
    }

    pub(super) fn complete_wait(&mut self, key: PumpDeadline) -> Option<AbsoluteBound> {
        self.freeze_plan_summary_account_bound();
        self.freeze_place_account_bound();
        if self.failure.is_some()
            || self.phase == RecordingPhase::Closed
            || self.filing_wait_still_owned(key)
            || self.output_wait_still_owned(key)
            || self.pair_wait_still_owned(key)
            || self.repair_wait_still_owned(key)
            || self.read_wait_still_owned(key)
            || self.plan_summary_wait_owned(key)
            || self.place_wait_owned(key)
            || self.inspection_wait_still_owned(key)
            || self.measure_wait_still_owned(key)
            || (key == PumpDeadline::Decision && self.marks.is_some())
            || self.freshness_wait_still_owned(key)
            || self.boot_pairs_wait_owned(key)
            || (key == PumpDeadline::Enter && self.boot_open_blocks_external())
            || (key == PumpDeadline::Leave && !self.boot_open_account_settled())
            || (key == PumpDeadline::Enter
                && self.phase == RecordingPhase::Entered
                && self
                    .template_checks
                    .as_ref()
                    .is_some_and(|owner| owner.can_certify()))
        {
            return None;
        }
        self.disarm_wait(key)
    }

    /// A retired Enter receipt proves the original registered cap was actually
    /// removed. A no-op disarm cannot invent or erase retirement evidence.
    fn disarm_wait(&mut self, key: PumpDeadline) -> Option<AbsoluteBound> {
        let retired = self.deadlines.disarm(key);
        if key == PumpDeadline::Enter
            && let Some(bound) = retired
        {
            self.retired_enter = Some(bound);
        }
        retired
    }

    pub(super) fn begin_releasing(
        &mut self,
        leave: AbsoluteBound,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        self.live(io)?;
        if self.place_owed()
            || self.boot_open_blocks_external()
            || self.plan_summary.is_some()
            || self.output.is_some()
            || self.repair.is_some()
            || self.pair_work.is_some()
            || self.filing.is_some()
            || self.input.snapshot().is_some()
            || self.input_turn.is_some()
            || self.member_capture.is_some()
            || self.freshness.is_some()
            || self.read.is_some()
            || self.inspection.is_some()
            || self.measure.is_some()
            || self.open_turn.is_some()
            || self.marks.is_some()
            || self.boot_pairs.is_some()
        {
            return Err(self.fail(
                io,
                FlowHaltReason::ProtocolFailed,
                HaltPoint::Barrier,
                "release requires settled input, a closed turn and completed capture",
            ));
        }
        if let Some(armed) = self.deadlines.get(PumpDeadline::Leave)
            && armed != leave
        {
            return Err(self.fail(
                io,
                FlowHaltReason::ProtocolFailed,
                HaltPoint::Barrier,
                "release cannot replace the existing leave window",
            ));
        }
        self.flow
            .check_releasing(&mut FlowRecording {
                barrier: &mut self.barrier,
                inbox: &mut self.inbox,
                budget: &mut self.budget,
                author: &self.author,
            })
            .map_err(|failure| self.flow_failed(io, failure))?;
        self.check_held_input()?;
        if self.deadlines.get(PumpDeadline::Leave).is_none() {
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
        if let Some(armed) = self.deadlines.get(PumpDeadline::Leave)
            && armed != leave
        {
            return Err(self.fail(
                io,
                FlowHaltReason::ProtocolFailed,
                HaltPoint::Barrier,
                "rollback cannot replace the existing leave window",
            ));
        }
        self.flow
            .check_enter_rollback(&mut FlowRecording {
                barrier: &mut self.barrier,
                inbox: &mut self.inbox,
                budget: &mut self.budget,
                author: &self.author,
            })
            .map_err(|failure| self.flow_failed(io, failure))?;
        self.check_held_input()?;
        if self.deadlines.get(PumpDeadline::Leave).is_none() {
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
        if self.place_owed() {
            return Err(RunFailure::PlaceOwned);
        }
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
        if self.place_owed() {
            return Err(RunFailure::PlaceOwned);
        }
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
            self.flow
                .check_close(&mut FlowRecording {
                    barrier: &mut self.barrier,
                    inbox: &mut self.inbox,
                    budget: &mut self.budget,
                    author: &self.author,
                })
                .map_err(|failure| self.flow_failed(ports.io, failure))?;
        }
        self.check_held_input()?;
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
                    // Lifecycle expiry returns ownership to its caller. The
                    // same writer must remain able to finish a resumed drain.
                    Err(failure @ RunFailure::Wait { .. }) => return Err(failure),
                    Err(failure) => {
                        self.barrier.abandon();
                        return Err(failure);
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
    // Return exact caller custody without allocating on a failure path.
    #[allow(clippy::result_large_err)]
    pub(super) fn author_and_wait<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
        event: AuthoredEvent,
    ) -> Result<Sequence, AuthorFailure> {
        self.admit_author_and_wait(ports, event, false)
    }

    // Return exact caller custody without allocating on a failure path.
    #[allow(clippy::result_large_err)]
    fn admit_author_and_wait<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
        event: AuthoredEvent,
        retained_retry: bool,
    ) -> Result<Sequence, AuthorFailure> {
        let initial_wait = match self.live(ports.io) {
            Ok(()) => None,
            Err(failure @ RunFailure::Wait { .. }) => Some(failure),
            Err(failure) => {
                return Err(self.refused_author_failure(failure, event));
            }
        };
        // Earlier owners may complete only their synchronous private author
        // continuation. A competing caller must first take its own held input.
        if self.caller_author_blocks_continuation(|| {
            (self.marks.is_some() && !self.marks_blocks_external())
                || (self.repair.is_some() && !self.repair_blocks_external())
        }) {
            let reason = self.fail(
                ports.io,
                FlowHaltReason::ProtocolFailed,
                HaltPoint::Author,
                "second author while an earlier caller original is held",
            );
            return Err(self.refused_author_failure(reason, event));
        }
        if self.place_blocks_external()
            || (self.place_admission_stopped() && event.kind == Kind::TurnStarted)
        {
            self.held_author = Some(event);
            return Err((RunFailure::PlaceOwned).into());
        }
        if (self.plan_summary_blocks_external() && !self.plan_summary_allows_turn_close(&event))
            || (receipt_driver::is_plan_effect_event(&event) && !self.plan_effect_inflight())
        {
            self.held_author = Some(event);
            return Err((RunFailure::PlanSummaryOwned).into());
        }
        if self.boot_open_blocks_external() {
            self.held_author = Some(event);
            return Err((RunFailure::BootOpenOwned).into());
        }
        if self.boot_pairs_blocks_external() {
            self.held_author = Some(event);
            return Err((RunFailure::BootPairsOwned).into());
        }
        if self.marks_blocks_external() {
            self.held_author = Some(event);
            return Err((RunFailure::MarksOwned).into());
        }
        if self.freshness.is_some() && !retained_retry {
            self.held_author = Some(event);
            return Err((RunFailure::FreshnessOwned).into());
        }
        if self.inspection.is_some() {
            self.held_author = Some(event);
            return Err((RunFailure::InspectionOwned).into());
        }
        if self.read.is_some() {
            self.held_author = Some(event);
            return Err((RunFailure::ReadOwned).into());
        }
        if self.repair_blocks_external() {
            self.held_author = Some(event);
            return Err((RunFailure::RepairOwned).into());
        }
        if self.measure_blocks_external() {
            self.held_author = Some(event);
            return Err((RunFailure::MeasureOwned).into());
        }
        if self.filing.is_some()
            || self.output.is_some()
            || self.pair_work.is_some()
            || self.pair_rejected_authoring
        {
            let reason = self.fail(
                ports.io,
                FlowHaltReason::ProtocolFailed,
                HaltPoint::Author,
                "another author entered an owned content transaction",
            );
            return Err(self.refused_author_failure(reason, event));
        }
        if self.templates_block_content()
            && (serving::content_schema(event.kind) != ContentSchema::None
                || event.kind == Kind::TurnStarted)
        {
            // This is refused caller input, never a pending author ticket.
            // The caller takes it unchanged before attempting another author.
            // resume_author_and_wait cannot turn it into admitted content.
            self.template_blocked_author = Some(event);
            return Err((RunFailure::TemplateBlocked).into());
        }
        if matches!(event.kind, Kind::ToolCallStarted | Kind::ToolCallCompleted) {
            self.held_author = Some(event);
            return Err((RunFailure::PairOwned).into());
        }
        if !self.author_allowed(&event) {
            let reason = self.fail(
                ports.io,
                FlowHaltReason::ProtocolFailed,
                HaltPoint::Author,
                "author kind is not legal in this recording phase",
            );
            return Err(self.refused_author_failure(reason, event));
        }
        if self.barrier.pending().is_some()
            || self.barrier.pending_author().is_some()
            || self.waiting_content.is_some()
            || self.completed_author.is_some()
            || self.flow.has_pending_record()
        {
            let reason = self.fail(
                ports.io,
                FlowHaltReason::ProtocolFailed,
                HaltPoint::Author,
                "second author while an earlier original or ticket is owned",
            );
            return Err(self.refused_author_failure(reason, event));
        }
        self.waiting_content = Some(WaitingAuthor::ordinary(event));
        if let Some(failure) = initial_wait {
            self.author_admission_pending = true;
            return Err((failure).into());
        }
        self.continue_waiting_content(ports)
    }

    // Return exact caller custody without allocating on a failure path.
    #[allow(clippy::result_large_err)]
    fn continue_waiting_content<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<Sequence, AuthorFailure> {
        self.continue_claimed_content(ports, None)
    }

    fn take_claimed_waiting_author(&mut self, claim: &AuthorClaim) -> Option<AuthoredEvent> {
        if !self
            .waiting_content
            .as_ref()
            .is_some_and(|waiting| waiting.matches(Some(claim)))
        {
            return None;
        }
        self.waiting_content.take().map(WaitingAuthor::into_event)
    }

    // Return exact caller custody without allocating on a failure path.
    #[allow(clippy::result_large_err)]
    fn continue_claimed_content<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
        claim: Option<AuthorClaim>,
    ) -> Result<Sequence, AuthorFailure> {
        if let Some(failure) = self.failure.clone() {
            return Err(failure.into());
        }
        if claim.is_some() && self.phase != RecordingPhase::Entered {
            return Err((match claim {
                Some(AuthorClaim::Placement(_)) => RunFailure::PlaceOwned,
                Some(AuthorClaim::EnterRefusal(_)) => RunFailure::EnterRefusalOwned,
                _ => RunFailure::MeasureOwned,
            })
            .into());
        }
        if !self
            .waiting_content
            .as_ref()
            .is_some_and(|waiting| waiting.matches(claim.as_ref()))
        {
            return Err((self.fail(
                ports.io,
                FlowHaltReason::ProtocolFailed,
                HaltPoint::Author,
                "waiting author does not belong to the exact private claim",
            ))
            .into());
        }
        #[cfg(test)]
        if claim.is_some() && std::mem::take(&mut self.pause_claim_queue_for_test) {
            return Err((match claim {
                Some(AuthorClaim::Placement(_)) => RunFailure::PlaceOwned,
                Some(AuthorClaim::EnterRefusal(_)) => RunFailure::EnterRefusalOwned,
                _ => RunFailure::MeasureOwned,
            })
            .into());
        }
        if self.author_admission_pending {
            let event = self
                .waiting_content
                .take()
                .expect("deferred author keeps its original")
                .into_event();
            self.author_admission_pending = false;
            return self.admit_author_and_wait(ports, event, true);
        }
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
                return Err((RunFailure::Wait { key, bound }).into());
            }
        }
        let event = self
            .waiting_content
            .take()
            .expect("caller input remained owned")
            .into_event();
        if let Some(Payload::Elections(load)) = &event.payload {
            // Only Recorded load can advance the phase and permit admission.
            // Keep its precise duty even across a trace-capacity interruption.
            self.load_duty = Some(LoadDuty {
                placement: load.state_flow.placement.clone(),
                scheme_text: load.ste100_scheme.clone(),
                instruction: load.state_flow.ste100.clone(),
                persistence: load.state_flow.persistence.clone(),
                template_inventory: load.template_inventory.clone(),
                tools: load.state_flow.tools.clone(),
            });
        }
        let wall_ms = match wall_ms() {
            Ok(wall_ms) => wall_ms,
            Err(detail) => {
                let reason = self.fail(
                    ports.io,
                    FlowHaltReason::TraceFailed,
                    HaltPoint::Author,
                    detail,
                );
                return Err(self.refused_author_failure(reason, event));
            }
        };
        self.author_turn = Some((event.kind, event.turn.clone()));
        let progress = if let Some(claim) = claim {
            let queued = self.barrier.queue_authored_with_clock(
                event,
                purpose,
                &self.author,
                wall_ms,
                &mut self.inbox,
                &mut self.budget,
                &mut || ports.io.now(),
            );
            // Failed preparation may already own its allocated original too.
            // Busy returns the new original and must never claim an older slot.
            if let Some(ticket) = queued.ticket {
                #[cfg(not(test))]
                let reject_claim = false;
                #[cfg(test)]
                let reject_claim = std::mem::take(&mut self.reject_queued_claim_for_test);
                let retained = !reject_claim
                    && match claim {
                        AuthorClaim::Placement(identity) => {
                            self.claim_place_author_ticket(&identity, ticket)
                        }
                        AuthorClaim::EnterRefusal(identity) => self
                            .enter_refusal_record
                            .as_mut()
                            .is_some_and(|record| record.claim(&identity, ticket)),
                        AuthorClaim::Measurement(identity) => {
                            self.claim_measure_author_ticket(&identity, ticket)
                        }
                    };
                if !retained {
                    return Err((self.fail(
                        ports.io,
                        FlowHaltReason::ProtocolFailed,
                        HaltPoint::Author,
                        "queued author lost its exact private owner",
                    ))
                    .into());
                }
            }
            if matches!(&queued.progress, RecordProgress::Pending) {
                self.resume_context_author(ports.io)
            } else {
                queued.progress
            }
        } else {
            self.submit_context_author(event, purpose, wall_ms, ports.io)
        };
        self.wait_recorded(ports, progress)
    }

    /// A lifecycle or legacy-label wait can interrupt a nested author. Its
    /// owner handles that typed outcome, then resumes this same ticket.
    // Return exact caller custody without allocating on a failure path.
    #[allow(clippy::result_large_err)]
    pub(super) fn resume_author_and_wait<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<Sequence, AuthorFailure> {
        if let Some(failure) = self.take_withdrawn_author(None, ports.io) {
            return Err(failure);
        }
        self.live(ports.io)?;
        if self.place_blocks_external() {
            return Err((RunFailure::PlaceOwned).into());
        }
        if self.plan_summary_blocks_external()
            && !self
                .barrier
                .pending_author()
                .is_some_and(|held| self.plan_summary_allows_turn_close(&held.event))
        {
            return Err((RunFailure::PlanSummaryOwned).into());
        }
        if self.boot_open_blocks_external() {
            return Err((RunFailure::BootOpenOwned).into());
        }
        if self.boot_pairs_blocks_external() {
            return Err((RunFailure::BootPairsOwned).into());
        }
        if self.marks_blocks_external() {
            return Err((RunFailure::MarksOwned).into());
        }
        if self.inspection.is_some() {
            return Err((RunFailure::InspectionOwned).into());
        }
        if self.read.is_some() {
            return Err((RunFailure::ReadOwned).into());
        }
        if self.repair_blocks_external() {
            return Err((RunFailure::RepairOwned).into());
        }
        if self.measure_blocks_external() {
            return Err((RunFailure::MeasureOwned).into());
        }
        if self.filing.is_some()
            || self.output.is_some()
            || self.pair_work.is_some()
            || self.pair_rejected_authoring
        {
            return Err((self.fail(
                ports.io,
                FlowHaltReason::ProtocolFailed,
                HaltPoint::Author,
                "generic resume cannot consume a content owner's author",
            ))
            .into());
        }
        self.resume_owned_author_and_wait(ports)
    }

    // Only a private owner that reserved this exact author may call this path.
    // Return exact caller custody without allocating on a failure path.
    #[allow(clippy::result_large_err)]
    fn resume_owned_author_and_wait<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<Sequence, AuthorFailure> {
        self.resume_claimed_author_and_wait(ports, None)
    }

    // Return exact caller custody without allocating on a failure path.
    #[allow(clippy::result_large_err)]
    fn resume_claimed_author_and_wait<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
        claim: Option<AuthorClaim>,
    ) -> Result<Sequence, AuthorFailure> {
        if let Some(failure) = self.take_withdrawn_author(claim.as_ref(), ports.io) {
            return Err(failure);
        }
        if let Some(failure) = self.failure.clone() {
            return Err(failure.into());
        }
        if let Some(sequence) = self.completed_author.take() {
            self.finish_release_author();
            return Ok(sequence);
        }
        if self.waiting_content.is_some() {
            // A backlog wait can suspend before the caller owns a trace ticket.
            // Continue that original input, including any coordinator-owned
            // measurement which must record before the backlog can move.
            return self.continue_claimed_content(ports, claim);
        }
        if self.flow.has_pending_record() || self.barrier.pending_author().is_none() {
            return Err((self.fail(
                ports.io,
                FlowHaltReason::ProtocolFailed,
                HaltPoint::Author,
                "resume requires this caller's pending author",
            ))
            .into());
        }
        let progress = self.resume_context_author(ports.io);
        self.wait_recorded(ports, progress)
    }

    /// Bind the actual ticket while this admission still owns the barrier's
    /// witness. No later trace-only author may replace it before its owner
    /// retains the admitted facts. Boot/read callers retain their own proofs.
    fn resume_trace_only_and_bind(&mut self) -> Result<Option<Sequence>, weaver_trace::Failure> {
        let recorded = self.barrier.resume_trace_only()?;
        if let Some(sequence) = recorded {
            self.bind_place_measure_author(sequence);
            self.bind_place_author(sequence);
            self.bind_enter_refusal_author(sequence);
        }
        Ok(recorded)
    }

    // Return exact caller custody without allocating on a failure path.
    #[allow(clippy::result_large_err)]
    fn wait_recorded<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
        mut progress: RecordProgress,
    ) -> Result<Sequence, AuthorFailure> {
        loop {
            // Retain this exact owner's actual admission even when the recorder
            // subsequently reports an observer failure. The failure remains
            // terminal. Binding may complete admitted factual state (including
            // TurnClosed), but cannot return successful caller continuation or
            // clear the Failed phase.
            if matches!(&progress, RecordProgress::Failed)
                && let Some(sequence) = self.barrier.admitted_before_failure()
            {
                self.bind_place_measure_author(sequence);
                self.bind_place_author(sequence);
                self.bind_enter_refusal_author(sequence);
            }
            // Other completion hooks keep their existing successful boundary.
            if let RecordProgress::Recorded {
                sequence,
                proof: None,
            } = &progress
            {
                if self.release_author.is_some() {
                    self.retain_recorded_author(*sequence);
                }
                self.bind_place_measure_author(*sequence);
                self.bind_place_author(*sequence);
                self.bind_enter_refusal_author(*sequence);
                self.bind_marks_author(*sequence, ports.io)?;
                self.bind_repair_author(*sequence, ports.io)?;
                self.bind_pair_author(*sequence, ports.io)?;
            }
            // A returned original has no other owner until the BusyAuthored
            // arm retains it. Intake may fail immediately on the same refusal.
            if !matches!(&progress, RecordProgress::BusyAuthored { .. }) {
                self.drain_intake(ports.io)?;
            }
            match progress {
                RecordProgress::Recorded {
                    sequence,
                    proof: None,
                } => {
                    if self.release_author.is_none() {
                        self.retain_recorded_author(sequence);
                    }
                    if let Some((kind, turn)) = self.author_turn.take() {
                        if kind == Kind::TurnStarted {
                            self.ever_started_turn = true;
                            self.open_turn = turn;
                        } else if kind == Kind::TurnClosed && self.open_turn == turn {
                            self.place_turn_stopped = false;
                            self.open_turn = None;
                        }
                    }
                    if self.phase == RecordingPhase::Preflight {
                        self.phase = RecordingPhase::Entered;
                    }
                    self.bind_filing_author(sequence, ports.io)?;
                    self.bind_output_author(sequence, ports.io)?;
                    self.live(ports.io)?;
                    let sequence = self
                        .completed_author
                        .take()
                        .expect("recorded caller result is owned");
                    self.finish_release_author();
                    return Ok(sequence);
                }
                RecordProgress::Pending => {
                    if let PumpStep::Deadline { key, bound } = self.pump_once(ports)? {
                        return Err((RunFailure::Wait { key, bound }).into());
                    }
                }
                RecordProgress::BusyAuthored { event, owed, .. } => {
                    // Only this unadmitted attempt is retired. Its earlier held
                    // siblings, first failure, and original waits stay untouched.
                    if let Some(owed) = owed {
                        self.author_turn = None;
                        self.author_admission_pending = false;
                        self.finish_release_author();
                        let original = if self.held_author.is_none() {
                            self.held_author = Some(event);
                            None
                        } else {
                            Some(event)
                        };
                        let reason = match self.drain_intake(ports.io) {
                            Err(reason) => reason,
                            Ok(()) => self.failure.clone().unwrap_or(match owed {
                                OwedAuthor::BootOpen => RunFailure::BootOpenOwned,
                                OwedAuthor::Placement => RunFailure::PlaceOwned,
                            }),
                        };
                        if self.failure.is_some()
                            && original.is_none()
                            && self.refused_author.is_none()
                        {
                            // This attempt just occupied the empty held slot.
                            // A simultaneous intake failure makes it first-fatal
                            // evidence, without touching any earlier original.
                            self.refused_author = self.held_author.take();
                        }
                        return Err(AuthorFailure { reason, original });
                    }
                    let original = self.retain_refused(event);
                    let reason = match self.drain_intake(ports.io) {
                        Err(reason) => reason,
                        Ok(()) => self.record_failed(ports.io),
                    };
                    return Err(AuthorFailure { reason, original });
                }
                RecordProgress::Failed => return Err((self.record_failed(ports.io)).into()),
                RecordProgress::Busy { .. } | RecordProgress::Recorded { proof: Some(_), .. } => {
                    return Err((self.fail(
                        ports.io,
                        FlowHaltReason::ProtocolFailed,
                        HaltPoint::Author,
                        "ordinary author received another owner's proof",
                    ))
                    .into());
                }
            }
            progress = self.resume_context_author(ports.io);
        }
    }

    /// One writer owns the caller result at its existing completion boundary.
    /// Release retains it before observers; content owners first bind their proof.
    fn retain_recorded_author(&mut self, sequence: Sequence) {
        self.last_author_sequence = Some(sequence);
        self.completed_author = Some(sequence);
        self.retain_release_recorded(sequence);
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
            RecordingPhase::Releasing => release_driver::shutdown_kind(event.kind),
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
        self.pump_with_wait(ports, true)
    }

    fn observe_ready<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<PumpStep, RunFailure> {
        self.pump_with_wait(ports, false)
    }

    fn pump_with_wait<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
        wait: bool,
    ) -> Result<PumpStep, RunFailure> {
        if let Some(decode) = ports.decode.as_deref() {
            self.check_measure_port(decode)?;
            self.check_boot_open_port(decode)?;
            self.check_place_port(decode)?;
        }
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
        self.observe_boot_lifecycle(&ports.lifecycle);
        self.observe_plan_summary_lifecycle(&ports.lifecycle);
        self.observe_place_lifecycle(&ports.lifecycle);
        if ports.decode.is_some() && self.boot_open_revoked() {
            return Err(RunFailure::BootOpenOwned);
        }
        if ports
            .decode
            .as_deref()
            .is_some_and(|decode| decode.open_not_started())
            && self.boot_open.is_some()
        {
            self.boot_open_live(ports.io)?;
        }
        // Hold only this wholly unsent generation's write leg. The same socket
        // stays in the sole pump for receive, terminal readiness and standing
        // failure while required summary traffic advances.
        let defer_generation = self.plan_summary_blocks_generation()
            && ports
                .decode
                .as_deref()
                .is_some_and(|decode| decode.generation_not_started());
        if !defer_generation {
            self.place_first_send(ports.decode.as_deref(), ports.io)?;
        }
        let previous_generation_hold = if defer_generation {
            ports
                .decode
                .as_deref_mut()
                .map(|decode| decode.replace_generation_send_hold(true))
        } else {
            None
        };
        let step = self.pump.step_with_wait(
            ports,
            Some(&mut self.flow),
            &mut FlowRecording {
                barrier: &mut self.barrier,
                inbox: &mut self.inbox,
                budget: &mut self.budget,
                author: &self.author,
            },
            &self.deadlines,
            wait,
        );
        // Restore before every fallible continuation, including a failed step.
        if let Some(previous) = previous_generation_hold {
            ports
                .decode
                .as_deref_mut()
                .expect("the common pump retains its decode owner")
                .replace_generation_send_hold(previous);
        }
        self.observe_boot_lifecycle(&ports.lifecycle);
        self.observe_plan_summary_lifecycle(&ports.lifecycle);
        self.observe_place_lifecycle(&ports.lifecycle);
        let sent = self.reconcile_pair_send(ports.io)?;
        match step {
            Ok(PumpStep::Deadline {
                key: PumpDeadline::Decision,
                ..
            }) if sent && self.deadlines.get(PumpDeadline::Decision).is_none() => {
                self.live(ports.io)?;
                Ok(PumpStep::Progress {
                    sources: vec![PumpSource::Gate],
                })
            }
            Ok(PumpStep::Deadline { key, bound }) => Err(self.expired(ports.io, key, bound)),
            Ok(step) => Ok(step),
            Err(PumpFailure::Transport { source, failure }) => {
                if source == PumpSource::Decode && self.measure.is_some() {
                    if let Some(decode) = ports.decode.as_deref_mut() {
                        self.retain_measure_transport_evidence(decode);
                    }
                    Err(self.measure_fault(ports.io, format!("decode transport: {failure:?}")))
                } else if source == PumpSource::Decode
                    && (self.generation.is_some() || self.reload.is_some())
                {
                    if let Some(decode) = ports.decode.as_deref_mut() {
                        self.retain_place_wire(decode);
                    }
                    Err(self.place_fault(ports.io, format!("decode transport: {failure:?}")))
                } else {
                    Err(RunFailure::Transport { source, failure })
                }
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
        self.resume_accepted_read(io)?;
        if self.read_blocks_reduction() {
            return Ok(Advance::Pending);
        }
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
        self.settle_recorded_read(io)?;
        self.settle_recorded_inspection(io)?;
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

    /// Stopped disposal keeps the original Enter/Leave cap even after expiry.
    /// Other waits and every independently observed failure keep their meaning.
    fn release_live(&mut self, io: &mut impl PumpIo) -> Result<(), RunFailure> {
        match self.live(io) {
            Ok(())
            | Err(RunFailure::Wait {
                key: PumpDeadline::Enter | PumpDeadline::Leave,
                ..
            }) => Ok(()),
            Err(failure) => Err(failure),
        }
    }

    fn live(&mut self, io: &mut impl PumpIo) -> Result<(), RunFailure> {
        self.observe_plan_effect_receipts();
        if self.failure.is_none()
            && let Some(detail) = self.plan_cache.invalid
        {
            return Err(self.fail(
                io,
                FlowHaltReason::ProtocolFailed,
                HaltPoint::StateReply,
                detail,
            ));
        }
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
        // A read can own the recorder and prevent ordinary flow reduction.
        // Inspect an already observed channel failure without consuming that
        // author's ticket, progressing transport or replacing its original bound.
        if let Some(failure) = self.flow.standing_state().and_then(StateChannel::failure) {
            let failure = FlowFailure {
                point: "state_transport",
                detail: failure.to_string(),
            };
            return Err(self.flow_failed(io, failure));
        }
        self.settle_boot_pairs_receipt_wait();
        self.retire_recorded_inspection_wait(io)?;
        self.reconcile_pair_send(io)?;
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
            | PumpDeadline::AdminRead
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

    #[must_use]
    fn retain_refused(&mut self, event: AuthoredEvent) -> Option<AuthoredEvent> {
        if self.refused_author.is_some() || self.caller_author_held() {
            Some(event)
        } else {
            self.refused_author = Some(event);
            None
        }
    }

    fn refused_author_failure(
        &mut self,
        reason: RunFailure,
        event: AuthoredEvent,
    ) -> AuthorFailure {
        AuthorFailure {
            reason,
            original: self.retain_refused(event),
        }
    }

    fn take_withdrawn_author(
        &mut self,
        claim: Option<&AuthorClaim>,
        io: &mut impl PumpIo,
    ) -> Option<AuthorFailure> {
        if !self
            .waiting_content
            .as_ref()
            .is_some_and(|waiting| waiting.withdrawn.is_some() && waiting.matches(claim))
        {
            return None;
        }
        let observed = self.live(io).err();
        let waiting = self
            .waiting_content
            .take()
            .expect("withdrawn caller original");
        Some(AuthorFailure {
            reason: observed.unwrap_or_else(|| waiting.withdrawn.unwrap()),
            original: Some(waiting.event),
        })
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
        if let Some(owner) = &mut self.boot_open {
            owner.revoke();
        }
        // Revoke before sampling or reporting. A broken clock cannot grant work.
        self.observe_plan_effect_receipts();
        self.plan_cache.fail();
        self.revoke_place();
        self.input.halt();
        self.pairs.freeze();
        self.freeze_repair();
        if let Some(read) = &mut self.read {
            read.freeze();
        }
        if let Some(measure) = &mut self.measure {
            measure.freeze();
        }
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
        self.freeze_boot_account_bound();
        self.freeze_plan_summary_account_bound();
        self.freeze_place_account_bound();
        if self
            .read
            .as_ref()
            .is_some_and(|read| read.needs_failed_close())
            && let Err(error) = self.flow.close_failed_read()
            && let Some(halt) = self.flow.halt_mut()
        {
            let _ = halt.append_secondary(SecondaryFailure {
                reason: FlowHaltReason::CarryFailed,
                point: HaltPoint::Teardown,
                detail: format!("close failed read: {error}"),
                observed_ns: io.now().ok().map(Into::into),
            });
        }
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
#[path = "context_held_input_tests.rs"]
mod held_input_tests;

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

#[path = "context_residency_composition.rs"]
pub(in crate::flow) mod residency_composition;

#[path = "context_startup_composition.rs"]
pub(in crate::flow) mod startup_composition;

#[path = "context_worker_startup.rs"]
pub(crate) mod worker_startup;

#[path = "context_boot_composition.rs"]
pub(in crate::flow) mod boot_composition;

#[path = "context_templates.rs"]
mod template_driver;
#[allow(unused_imports)] // Dormant certification boundary, consumed by enter at cutover.
pub(super) use template_driver::{TemplateBeginError, TemplateBeginRefusal, TemplateProgress};

#[cfg(test)]
#[path = "context_template_tests.rs"]
mod template_tests;

#[path = "context_receipts.rs"]
mod receipt_driver;

#[path = "context_sort.rs"]
mod sort_driver;
#[allow(unused_imports)] // Dormant filing consumer, consumed by dependent primitive ports.
pub(super) use receipt_driver::{
    FilingBeginError, FilingBeginRefusal, FilingCancellation, FilingCompletion, FilingProgress,
};
#[allow(unused_imports)] // The worker consumes the held outbound port at cutover.
pub(in crate::flow) use sort_driver::{OutboundBeginError, OutboundCompletion, OutboundProgress};

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

#[path = "context_inward_recall.rs"]
mod inward_recall;
#[path = "context_pairs.rs"]
mod pair_driver;
#[path = "context_pair_execution.rs"]
mod pair_execution;
#[allow(unused_imports)]
pub(super) use inward_recall::{InwardProgress, InwardRefusal};
#[path = "pairs.rs"]
mod pairs;
#[allow(unused_imports)] // The loop composes this port at the coupled cutover.
pub(super) use pair_driver::{
    PairBeginError, PairBeginRefusal, PairProgress, PairReadError, RepairPairBeginError,
};
#[allow(unused_imports)]
pub(super) use pair_execution::{ExecutionBeginError, ExecutionProgress, ExecutionRefusal};
#[allow(unused_imports)]
pub(super) use pairs::CompletedPair;

#[cfg(test)]
#[path = "context_pair_tests.rs"]
mod pair_tests;

#[path = "repair.rs"]
mod repair;

#[path = "context_repair.rs"]
mod repair_driver;
#[allow(unused_imports)]
pub(super) use repair::{
    CorrectionProgress, EffectiveEmission, RepairAccount, RepairError, RepairExhausted,
    RestatementRequest,
};
#[allow(unused_imports)]
pub(super) use repair_driver::{
    RepairBeginError, RepairBeginRefusal, RepairCancelled, RepairObserveError, RepairProgress,
    RepairRefusalEvidence, RepairRefused,
};

#[cfg(test)]
#[path = "context_repair_tests.rs"]
mod repair_tests;

#[path = "context_marks.rs"]
mod marks_driver;
#[allow(unused_imports)] // Composed by the loop at the coupled cutover.
pub(in crate::flow) use marks_driver::{
    BeginTaskError, BeginTaskRefusal, BegunTask, TaskCompletion, TaskProgress,
};

#[cfg(test)]
#[path = "context_marks_tests.rs"]
mod marks_tests;

#[path = "context_material.rs"]
mod material_driver;

#[path = "context_boot_open.rs"]
pub(super) mod boot_open_driver;
#[allow(unused_imports)]
pub(in crate::flow) use boot_open_driver::{
    BootOpenBeginError, BootOpenBeginRefusal, BootOpenProgress, CheckedBootOpen,
};

#[path = "context_boot_pairs.rs"]
mod boot_pair_driver;
#[allow(unused_imports)] // Dormant until the checked prior-Open owner exists.
pub(super) use boot_pair_driver::{
    BootPairsBeginError, BootPairsBeginRefusal, BootPairsCompletion, BootPairsProgress,
    BootPairsRefusal, PriorBootOpen, RepairedBootPairs,
};

#[path = "context_plan_summary.rs"]
pub(in crate::flow) mod plan_summary_driver;
#[allow(unused_imports)]
pub(in crate::flow) use super::plan::PlanSummaryUnavailable;
#[allow(unused_imports)]
pub(in crate::flow) use plan_summary_driver::{
    BootPlanSummary, EffectPlanSummary, PlanSummaryAccount, PlanSummaryBeginError,
    PlanSummaryBeginRefusal, PlanSummaryCompletion, PlanSummaryProgress,
};

#[path = "context_read.rs"]
mod read_driver;
#[allow(unused_imports)] // Composed by dependent boot, recall and placement primitives.
pub(super) use read_driver::{
    BootHead, BootReadBeginError, BootReadBeginRefusal, BootReadCompletion, CancelledRead,
    ReadBeginError, ReadBeginRefusal, ReadCompletion, ReadProgress, RecordedRead,
};

#[path = "context_freshness.rs"]
mod freshness_driver;
#[allow(unused_imports)] // Composed by fresh-history owners at the held cutover.
pub(super) use freshness_driver::{
    FreshnessBeginError, FreshnessBeginRefusal, FreshnessCompletion, FreshnessProgress,
    PreparedFreshness,
};

#[cfg(test)]
#[path = "context_read_tests.rs"]
mod read_tests;

#[path = "context_measure.rs"]
mod measure_driver;
#[allow(unused_imports)] // The later place owner consumes these count-only proofs.
pub(super) use measure_driver::{
    CancelledMeasure, MeasureBeginError, MeasureBeginRefusal, MeasureCompletion, MeasureFacts,
    MeasurePosition, MeasureProgress, MeasureRequest, MeasuredProposal, RefusedMeasure,
};

#[cfg(test)]
#[path = "context_measure_tests.rs"]
mod measure_tests;

#[path = "context_halt.rs"]
mod halt_driver;
#[allow(unused_imports)] // Failed lifecycle composition remains dormant until cutover.
pub(super) use halt_driver::{HaltReport, HaltReportRefusal, HaltTraceProgress};

#[path = "context_halt_gate.rs"]
mod halt_gate_driver;
#[allow(unused_imports)] // Failed lifecycle composition remains held for cutover.
pub(super) use halt_gate_driver::{HaltGate, HaltGateProgress, HaltGateRefusal};

#[path = "context_inspection.rs"]
mod inspection_driver;
#[allow(unused_imports)] // Dormant ports composed only at the coupled cutover.
pub(super) use inspection_driver::{
    InspectionBeginError, InspectionBeginRefusal, InspectionCompletion, InspectionEvidence,
    InspectionKind, InspectionProgress,
};

#[path = "context_release.rs"]
pub(in crate::flow) mod release_driver;

#[path = "context_place.rs"]
pub(in crate::flow) mod place_driver;
#[allow(unused_imports)] // These private errors are consumed by the future loop composition.
pub(in crate::flow) use place_driver::{
    CheckedCut, GenerationCompletion, GenerationProgress, NativeMarkBeginError,
    NativeMarkCompletion, NativeMarkTransaction, NativePosition, NativeScopes, PlaceAccount,
    PlaceBeginError, PlaceInstallError, PlaceProgress, PlacementCompletion, ReloadCompletion,
    ReloadProgress,
};

#[path = "context_failed_enter.rs"]
mod failed_enter_driver;
#[allow(unused_imports)] // Dormant failed Enter process composition.
pub(super) use failed_enter_driver::{
    FailedEnterCleanup, FailedEnterFinished, FailedEnterProgress, FailedEnterRefusal,
    FailedEnterRejected, FailedEnterResources,
};
