//! Dormant HARNESS-OPEN transaction. Wire consistency is only one prerequisite;
//! this owner binds real boot provenance, transport, Recorded placement and LIVE.
//! Cancellation retains bounded accounting, never an executable decoder grant.

use super::*;
use crate::channel::DecodeReply;
use crate::flow::author::AuthoredSource;
use crate::flow::pump::LifecyclePorts;
use crate::flow::pump_transport::{
    DecodePort, DecodeReservation, DecodeStage, PortIdentity, ReceivedDecode,
};
use std::sync::Arc;
use weaver_trace::Subsystem;
use weaver_trace::serving::payloads::{
    ArgumentCursor, MarkOccurrenceId, MarkPosition, MarkPositionSource, OpenEvidenceRecord,
    PlacementPhase, PlacementRecord,
};
use weaver_types::state_flow::{PersistenceCase, PersistenceStage, SpanRef};
use weaver_types::{
    LifecycleAsk, LifecycleRefusal, RefusalRecord, SessionId, TokenAnswer, TokenAsk, TokenDirective,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum BootOpenBeginRefusal {
    WrongPhase,
    MissingHead,
    AlreadyStarted,
    Busy,
}
#[derive(Debug)]
pub(in crate::flow) struct BootOpenBeginError {
    pub refusal: BootOpenBeginRefusal,
    pub failure: Option<RunFailure>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum BootOpenProgress {
    Pending,
    PlacementRecorded,
    Complete,
    Refused,
    Cancelled,
    Incomplete,
}

/// No scalar constructor, Clone, decoder writer, or deserializer. The actual owner
/// alone creates this proof after its own Recorded placement and checked fit/LIVE.
#[derive(Debug)]
pub(in crate::flow) struct CheckedBootOpen {
    origin: Arc<()>,
    run: String,
    head: Sequence,
    placement: Sequence,
    span: SpanRef,
    enter: AbsoluteBound,
    prefix: u64,
    capacity: u64,
    generation_limit: u64,
    terminal_tokens: u64,
    c_head: u64,
    c_tail: u64,
}
impl CheckedBootOpen {
    pub(in crate::flow) fn origin(&self) -> &Arc<()> {
        &self.origin
    }
    pub(in crate::flow) fn run(&self) -> &str {
        &self.run
    }
    pub(in crate::flow) fn head(&self) -> Sequence {
        self.head
    }
    pub(in crate::flow) fn placement(&self) -> Sequence {
        self.placement
    }
    pub(in crate::flow) fn span(&self) -> &SpanRef {
        &self.span
    }
    pub(in crate::flow) fn enter(&self) -> AbsoluteBound {
        self.enter
    }
    pub(in crate::flow) fn prefix(&self) -> u64 {
        self.prefix
    }
    pub(in crate::flow) fn capacity(&self) -> u64 {
        self.capacity
    }
    pub(in crate::flow) fn limits(&self) -> (u64, u64, u64, u64) {
        (
            self.c_head,
            self.c_tail,
            self.generation_limit,
            self.terminal_tokens,
        )
    }
}

/// A private, immutable borrow of the actual correlated Opened. Only its checked
/// constructor can license this owner's placement builder. Wire consistency does
/// not imply fit, LIVE, Recorded, or successful canonical trace encoding.
pub(super) struct ValidatedOpen<'a> {
    opened: &'a weaver_types::Opened,
}
impl<'a> ValidatedOpen<'a> {
    fn check(
        opened: &'a weaver_types::Opened,
        session: &SessionId,
    ) -> Result<Self, crate::open_evidence::OpenEvidenceError> {
        crate::open_evidence::validate(opened, session)?;
        Ok(Self { opened })
    }

    fn placement(&self, wait_ns: u64) -> PlacementRecord {
        let opened = self.opened;
        PlacementRecord {
            assembly_id: 0.into(),
            phase: PlacementPhase::Boot,
            context: None,
            cut: None,
            method: None,
            n: opened.prefix_tokens,
            prefix_tokens: opened.prefix_tokens,
            resident_before: opened.prefix_tokens,
            input_tokens: 0,
            capacity: opened.capacity,
            generation_limit: opened.generation_limit,
            terminal_tokens: opened.terminal_tokens,
            unit_map: vec![],
            message_ids: vec![],
            assistant_message_id: None,
            recomputed_messages: 0.into(),
            wait_ns: wait_ns.into(),
            open_evidence: Some(OpenEvidenceRecord {
                request: opened.request.clone(),
                measurement: opened.measurement.clone(),
            }),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Exchange,
    Placement,
    PlacementRecorded,
    Refusal,
    Mark,
    Complete,
    Refused,
    Settled,
    Incomplete,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AuthorKind {
    Placement,
    Refusal,
    Mark,
}
struct BootAuthor {
    kind: AuthorKind,
    event: Option<AuthoredEvent>,
    queued: bool,
    ticket: Option<u64>,
}
/// Recorder-only permit. Its fields and constructor are private to this owner.
pub(in crate::flow) struct OwedBootAuthor {
    event: AuthoredEvent,
}
impl OwedBootAuthor {
    pub(in crate::flow) fn into_event(self) -> AuthoredEvent {
        self.event
    }
}
/// Only the boot owner can license an active flow's failed-enter release while
/// its own unselected author is pending. No content obligation is waived.
pub(in crate::flow) struct BootReleasePermit {
    ticket: Option<u64>,
}
impl BootReleasePermit {
    pub(in crate::flow) fn matches(&self, barrier: &RecordBarrier) -> bool {
        match (self.ticket, barrier.pending_author()) {
            (Some(ticket), Some(held)) => ticket == held.ticket,
            (None, None) => barrier.pending().is_none(),
            _ => false,
        }
    }
}

pub(super) struct BootOpenTransaction {
    request: TokenDirective,
    reservation: Option<DecodeReservation>,
    identity: PortIdentity,
    origin: Arc<()>,
    head: Sequence,
    span: SpanRef,
    enter: AbsoluteBound,
    account_deadline: u64,
    began: u64,
    wait_ns: Option<u64>,
    budget: serving::payloads::PlacementConfig,
    received: Option<ReceivedDecode>,
    failed_wire: Option<Vec<u8>>,
    fit: Option<bool>,
    stage: Stage,
    author: Option<BootAuthor>,
    placement: Option<Sequence>,
    refusal_sequence: Option<Sequence>,
    mark_sequence: Option<Sequence>,
    refusal: Option<LifecycleRefusal>,
    checked: Option<CheckedBootOpen>,
    journal: Option<super::super::journal::Journal>,
    revoked: bool,
    owed: bool,
    grant_taken: bool,
    incomplete: Option<String>,
    unavailable: Option<String>,
}
impl BootOpenTransaction {
    #[cfg(test)]
    pub(super) fn unsubmitted_event_for_test(&self) -> Option<&AuthoredEvent> {
        self.author.as_ref()?.event.as_ref()
    }
    pub(super) fn revoke(&mut self) {
        self.revoked = true;
    }
    fn progress(&self) -> BootOpenProgress {
        if self.grant_taken {
            return BootOpenProgress::Complete;
        }
        match self.stage {
            Stage::Incomplete => BootOpenProgress::Incomplete,
            Stage::Settled => BootOpenProgress::Cancelled,
            Stage::Refused => BootOpenProgress::Refused,
            Stage::Complete if !self.revoked => BootOpenProgress::Complete,
            _ if self.revoked => BootOpenProgress::Cancelled,
            Stage::PlacementRecorded => BootOpenProgress::PlacementRecorded,
            _ => BootOpenProgress::Pending,
        }
    }
}

impl RecordingContext {
    pub(super) fn boot_open_blocks_external(&self) -> bool {
        self.boot_open.as_ref().is_some_and(|owner| {
            if matches!(
                self.phase,
                RecordingPhase::Releasing | RecordingPhase::Failed
            ) {
                owner.owed
            } else {
                !owner.grant_taken
            }
        })
    }
    pub(super) fn check_boot_open_port(&self, decode: &DecodePort) -> Result<(), RunFailure> {
        if self.boot_open.as_ref().is_some_and(|owner| {
            !owner.identity.matches_decode(decode)
                || owner
                    .reservation
                    .as_ref()
                    .is_some_and(|reservation| !reservation.belongs_to(decode))
        }) {
            Err(RunFailure::BootOpenOwned)
        } else {
            Ok(())
        }
    }
    pub(super) fn observe_boot_lifecycle(&mut self, ports: &LifecyclePorts<'_>) {
        let cancelled = ports
            .peer
            .as_deref()
            .is_some_and(|peer| peer.cancels_boot_open());
        if cancelled {
            self.boot_enter_cancelled = true;
            if let Some(owner) = &mut self.boot_open
                && !owner.grant_taken
            {
                owner.revoke();
            }
        }
    }

    pub(in crate::flow) fn begin_boot_open(
        &mut self,
        decode: &mut DecodePort,
        io: &mut impl PumpIo,
    ) -> Result<(), BootOpenBeginError> {
        let failure = self.live(io).err();
        let refusal = if failure.is_some()
            || self.boot_enter_cancelled
            || self.phase != RecordingPhase::Entered
            || self.flow.phase() != CoordinatorPhase::Active
            || !self.barrier.is_active()
        {
            Some(BootOpenBeginRefusal::WrongPhase)
        } else if self.boot_open.is_some() || self.boot_journal.is_some() {
            Some(BootOpenBeginRefusal::AlreadyStarted)
        } else if self.boot_head.as_ref().is_none_or(|head| {
            !head.belongs_to(self)
                || self.deadlines.get(PumpDeadline::Enter) != Some(head.enter_bound())
        }) || self.load_duty.is_none()
            || self.open_start.is_none()
            || self.completed_template_enter().is_err()
        {
            Some(BootOpenBeginRefusal::MissingHead)
        } else if self.boot_pairs.is_some() || self.boot_begin_busy() {
            Some(BootOpenBeginRefusal::Busy)
        } else {
            None
        };
        if let Some(refusal) = refusal {
            return Err(BootOpenBeginError { refusal, failure });
        }
        let head = self.boot_head.as_ref().unwrap();
        // The held request is authored from this immutable trace session;
        // validation independently compares the received echo with that request.
        let request = TokenDirective::Open {
            session: SessionId(self.author.session().to_owned()),
            messages: vec![head.message().clone()],
            // Foundation serving has no diagnostic per-column readout request.
            column_ask: false,
        };
        let reservation = match decode.stage_reserved(request.clone()) {
            Ok(DecodeStage::Queued(reservation)) => reservation,
            Ok(DecodeStage::Busy(_)) => {
                return Err(BootOpenBeginError {
                    refusal: BootOpenBeginRefusal::Busy,
                    failure: None,
                });
            }
            Err(error) => {
                return Err(BootOpenBeginError {
                    refusal: BootOpenBeginRefusal::WrongPhase,
                    failure: Some(
                        self.boot_open_fault(io, format!("boot Open stage: {:?}", error.failure)),
                    ),
                });
            }
        };
        self.boot_open = Some(BootOpenTransaction {
            request,
            reservation: Some(reservation),
            identity: decode.port_identity(),
            origin: self.read_origin.clone(),
            head: head.recall_sequence(),
            span: head.span().clone(),
            enter: head.enter_bound(),
            account_deadline: head.enter_bound().deadline_ns(),
            began: self.last_ns,
            wait_ns: None,
            budget: self.load_duty.as_ref().unwrap().placement.clone(),
            received: None,
            failed_wire: None,
            fit: None,
            stage: Stage::Exchange,
            author: None,
            placement: None,
            refusal_sequence: None,
            mark_sequence: None,
            refusal: None,
            checked: None,
            journal: None,
            revoked: false,
            owed: true,
            grant_taken: false,
            incomplete: None,
            unavailable: None,
        });
        self.barrier.set_boot_open_owed(true);
        Ok(())
    }

    pub(super) fn boot_begin_busy(&self) -> bool {
        self.open_turn.is_some()
            || self.input.snapshot().is_some()
            || self.input_turn.is_some()
            || self.input_author.is_some()
            || self.output.is_some()
            || self.pair_work.is_some()
            || self.repair.is_some()
            || self.filing.is_some()
            || self.read.is_some()
            || self.marks.is_some()
            || self.freshness.is_some()
            || self.inspection.is_some()
            || self.measure.is_some()
            || self.member_capture.is_some()
            || self.templates_block_content()
            || self.waiting_content.is_some()
            || self.completed_author.is_some()
            || self.template_blocked_author.is_some()
            || self.barrier.pending().is_some()
            || self.barrier.pending_author().is_some()
            || self.flow.has_pending_record()
            || self.flow.has_filing_consumer()
            || self.flow.has_output_consumer()
            || self.flow.pending_control().is_some()
            || self.flow.pending_check().is_some()
            || self.flow.operation_completion().is_some()
            || !self.flow.state().outstanding().is_empty()
            || self.flow.state().held_measurement().is_some()
            || [
                PumpDeadline::Decision,
                PumpDeadline::Cut,
                PumpDeadline::Gate,
                PumpDeadline::StateControl,
            ]
            .into_iter()
            .any(|key| self.deadlines.get(key).is_some())
    }

    pub(super) fn boot_open_live(&mut self, io: &mut impl PumpIo) -> Result<(), RunFailure> {
        if let Err(error) = self.live(io) {
            if let Some(owner) = &mut self.boot_open {
                owner.revoke();
            }
            return Err(error);
        }
        let owner = self.boot_open.as_ref().ok_or(RunFailure::BootOpenOwned)?;
        if owner.revoked
            || self.boot_enter_cancelled
            || self.phase != RecordingPhase::Entered
            || self.flow.phase() != CoordinatorPhase::Active
            || !self.barrier.is_active()
            || self.failure.is_some()
            || self.deadlines.get(PumpDeadline::Enter) != Some(owner.enter)
            || owner.enter.expired(self.last_ns)
        {
            self.boot_open.as_mut().unwrap().revoke();
            return Err(RunFailure::BootOpenOwned);
        }
        Ok(())
    }

    /// One owner step; lifecycle completions remain with the lifecycle reducer.
    pub(in crate::flow) fn advance_boot_open<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<BootOpenProgress, RunFailure> {
        let decode = ports.decode.as_deref().ok_or(RunFailure::BootOpenOwned)?;
        self.check_boot_open_port(decode)?;
        self.observe_boot_lifecycle(&ports.lifecycle);
        if self.boot_open.as_ref().is_some_and(|owner| owner.revoked) {
            return Ok(self.boot_open.as_ref().unwrap().progress());
        }
        self.boot_open_live(ports.io)?;
        match self.boot_open.as_ref().unwrap().stage {
            Stage::Exchange => {
                if ports.decode.as_deref().unwrap().held().is_none() {
                    if let Err(failure) = self.pump_once(ports) {
                        self.retain_boot_wire(ports.decode.as_deref_mut().unwrap());
                        if matches!(failure, RunFailure::Wait { .. }) {
                            self.boot_open.as_mut().unwrap().revoke();
                        } else if self.failure.is_none() {
                            let _ = self.boot_open_fault(
                                ports.io,
                                format!("boot Open transport: {failure:?}"),
                            );
                        }
                        return Err(failure);
                    }
                    self.observe_boot_lifecycle(&ports.lifecycle);
                    if self.boot_open.as_ref().unwrap().revoked {
                        return Ok(BootOpenProgress::Cancelled);
                    }
                }
                self.consume_boot_open(ports.decode.as_deref_mut().unwrap(), ports.io)?;
            }
            Stage::Placement | Stage::Refusal | Stage::Mark => {
                self.record_boot_author(ports.io, false)?
            }
            Stage::PlacementRecorded => self.after_boot_placement(ports.io)?,
            _ => {}
        }
        // This check follows retaining every receive/Recorded continuation.
        self.boot_open_live(ports.io)?;
        if self.boot_open.as_ref().unwrap().stage == Stage::Complete {
            self.finish_boot_reservation(ports.decode.as_deref_mut().unwrap(), ports.io)?;
        } else if self.boot_open.as_ref().unwrap().stage == Stage::Refused {
            self.finish_boot_reservation(ports.decode.as_deref_mut().unwrap(), ports.io)?;
            self.boot_open.as_mut().unwrap().revoke();
        }
        Ok(self.boot_open.as_ref().unwrap().progress())
    }

    fn consume_boot_open(
        &mut self,
        decode: &mut DecodePort,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        let owner = self.boot_open.as_ref().ok_or(RunFailure::BootOpenOwned)?;
        let reservation = owner
            .reservation
            .as_ref()
            .ok_or(RunFailure::BootOpenOwned)?;
        let received = match decode.take_reserved(reservation) {
            Ok(Some(value)) => value,
            Ok(None) => return Ok(()),
            Err(error) => {
                self.retain_boot_wire(decode);
                return Err(self.boot_open_fault(io, format!("boot receive: {error:?}")));
            }
        };
        let matched = received.matches(reservation)
            && received.belongs_to(decode)
            && received.after_send()
            && reservation.sent();
        self.boot_open.as_mut().unwrap().received = Some(received);
        if !matched {
            return Err(
                self.boot_open_fault(io, "boot answer preceded or mismatched the complete Open")
            );
        }
        let now = io.now().map_err(|error| {
            self.boot_trace_fault(io, format!("boot completion clock: {error}"))
        })?;
        let owner = self.boot_open.as_mut().unwrap();
        if now < self.last_ns || now < owner.began {
            return Err(self.boot_trace_fault(io, "boot completion clock reversed"));
        }
        self.last_ns = now;
        owner.wait_ns = Some(now - owner.began);
        let TokenDirective::Open { session, .. } = &owner.request else {
            unreachable!()
        };
        let reply = owner.received.as_ref().unwrap().reply();
        match reply {
            DecodeReply::Answer(TokenAnswer::Opened(opened)) => {
                let validated = match ValidatedOpen::check(opened, session) {
                    Ok(validated) => validated,
                    Err(error) => {
                        return Err(self.boot_open_fault(
                            io,
                            format!("invalid boot Opened evidence or trace session: {error:?}"),
                        ));
                    }
                };
                let opened = validated.opened;
                owner.fit = Some(
                    opened.prefix_tokens <= owner.budget.c_head
                        && owner.budget.c_head > 0
                        && owner.budget.c_tail > 0
                        && owner
                            .budget
                            .c_head
                            .checked_add(owner.budget.c_tail)
                            .and_then(|sum| sum.checked_add(opened.generation_limit))
                            .and_then(|sum| sum.checked_add(opened.terminal_tokens))
                            .is_some_and(|sum| sum <= opened.capacity),
                );
                let payload = validated.placement(owner.wait_ns.unwrap());
                owner.author = Some(BootAuthor {
                    kind: AuthorKind::Placement,
                    event: Some(AuthoredEvent {
                        kind: Kind::StatePlacement,
                        subsystem: Subsystem::Harness,
                        turn: None,
                        source: AuthoredSource::Loop,
                        causal_parent: Some(owner.head),
                        payload: Some(Payload::Placement(Box::new(payload))),
                    }),
                    queued: false,
                    ticket: None,
                });
                owner.stage = Stage::Placement;
            }
            DecodeReply::Refusal(refusal) => {
                owner.refusal = Some(LifecycleRefusal::DeviceCannotAdmit);
                let record = RefusalRecord::Decode {
                    asked: TokenAsk::Open,
                    refusal: refusal.clone(),
                };
                let payload = serde_json::value::to_raw_value(&record).map_err(|error| {
                    self.boot_open_fault(io, format!("boot refusal encoding: {error}"))
                })?;
                self.stage_boot_refusal(payload, Subsystem::SpuDecoder);
            }
            _ => {
                return Err(self.boot_open_fault(
                    io,
                    "unexpected boot Open reply arm; original answer retained",
                ));
            }
        }
        Ok(())
    }

    fn stage_boot_refusal(
        &mut self,
        payload: Box<serde_json::value::RawValue>,
        subsystem: Subsystem,
    ) {
        let owner = self.boot_open.as_mut().unwrap();
        owner.author = Some(BootAuthor {
            kind: AuthorKind::Refusal,
            event: Some(AuthoredEvent {
                kind: Kind::Refusal,
                subsystem,
                turn: None,
                source: AuthoredSource::Runtime,
                causal_parent: owner.placement,
                payload: Some(Payload::Refusal(payload)),
            }),
            queued: false,
            ticket: None,
        });
        owner.stage = Stage::Refusal;
    }

    fn after_boot_placement(&mut self, io: &mut impl PumpIo) -> Result<(), RunFailure> {
        self.boot_open_live(io)?;
        let owner = self.boot_open.as_ref().unwrap();
        if owner.fit != Some(true) {
            let refusal = LifecycleRefusal::StatePersistenceInvalid {
                stage: PersistenceStage::Boot, case: PersistenceCase::BootFailed,
                artifact_id: self.open_start.as_ref().map(|start| start.artifact_id.clone()),
                detail: "boot prefix and effective generation reserve do not fit the declared placement budget".into(),
            };
            let record = RefusalRecord::Lifecycle {
                asked: LifecycleAsk::Enter,
                refusal: refusal.clone(),
            };
            let raw = serde_json::value::to_raw_value(&record)
                .map_err(|error| self.boot_open_fault(io, format!("boot fit refusal: {error}")))?;
            self.boot_open.as_mut().unwrap().refusal = Some(refusal);
            self.stage_boot_refusal(raw, Subsystem::Harness);
            return Ok(());
        }
        let DecodeReply::Answer(TokenAnswer::Opened(opened)) =
            owner.received.as_ref().unwrap().reply()
        else {
            unreachable!()
        };
        let checked = CheckedBootOpen {
            origin: owner.origin.clone(),
            run: self.flow.run().to_owned(),
            head: owner.head,
            placement: owner.placement.unwrap(),
            span: owner.span.clone(),
            enter: owner.enter,
            prefix: opened.prefix_tokens,
            capacity: opened.capacity,
            generation_limit: opened.generation_limit,
            terminal_tokens: opened.terminal_tokens,
            c_head: owner.budget.c_head,
            c_tail: owner.budget.c_tail,
        };
        let journal = super::super::journal::Journal::from_boot(&checked);
        let mark = MarkPosition {
            id: MarkOccurrenceId {
                run: checked.run.clone(),
                ordinal: 0.into(),
            },
            span: super::super::mirror::span_ref(&checked.span),
            end: ArgumentCursor::Head {},
            source: MarkPositionSource::Boot {},
        };
        let owner = self.boot_open.as_mut().unwrap();
        owner.checked = Some(checked);
        owner.journal = Some(journal);
        owner.author = Some(BootAuthor {
            kind: AuthorKind::Mark,
            event: Some(AuthoredEvent {
                kind: Kind::StateMarkPosition,
                subsystem: Subsystem::Harness,
                turn: None,
                source: AuthoredSource::Loop,
                causal_parent: owner.placement,
                payload: Some(Payload::MarkPosition(Box::new(mark))),
            }),
            queued: false,
            ticket: None,
        });
        owner.stage = Stage::Mark;
        Ok(())
    }

    fn record_boot_author(
        &mut self,
        io: &mut impl PumpIo,
        stopped: bool,
    ) -> Result<(), RunFailure> {
        let owner = self.boot_open.as_ref().ok_or(RunFailure::BootOpenOwned)?;
        let author = owner.author.as_ref().ok_or(RunFailure::BootOpenOwned)?;
        if author.queued {
            if self.barrier.pending_author().map(|held| held.ticket) != author.ticket {
                return Err(self.boot_open_fault(io, "boot author lost its exact recorder ticket"));
            }
        } else if self.barrier.pending().is_some()
            || self.barrier.pending_author().is_some()
            || self.flow.has_pending_record()
        {
            return Ok(());
        }
        if stopped {
            if !author.queued {
                let wall = wall_ms().map_err(|detail| self.boot_trace_fault(io, detail))?;
                let permit = OwedBootAuthor {
                    event: self
                        .boot_author_event_mut()
                        .take()
                        .expect("unsubmitted boot event"),
                };
                if let Err(error) =
                    self.barrier
                        .queue_boot_open_account(permit, &self.author, self.last_ns, wall)
                {
                    *self.boot_author_event_mut() = Some(error.event);
                    return Err(self.boot_trace_fault(io, error.detail));
                }
                self.retain_boot_author_ticket();
            }
            let result = self.barrier.resume_trace_only();
            match result {
                Ok(Some(sequence)) => self.bind_boot_recorded(sequence),
                Ok(None) => {}
                Err(error) => {
                    return Err(self.boot_trace_fault(io, format!("owed boot trace: {error:?}")));
                }
            }
        } else {
            let progress = if author.queued {
                self.barrier
                    .resume_with_clock(&mut self.inbox, &mut self.budget, &mut || io.now())
            } else {
                let wall = wall_ms().map_err(|detail| self.boot_trace_fault(io, detail))?;
                let event = self
                    .boot_author_event_mut()
                    .take()
                    .expect("unsubmitted boot event");
                self.barrier.submit_authored_with_clock(
                    event,
                    RecordPurpose::Observation,
                    &self.author,
                    wall,
                    &mut self.inbox,
                    &mut self.budget,
                    &mut || io.now(),
                )
            };
            // Consume the actual Recorded identity before ANY lifecycle, intake or clock hook.
            match progress {
                RecordProgress::Recorded {
                    sequence,
                    proof: None,
                } => self.bind_boot_recorded(sequence),
                RecordProgress::Pending => self.retain_boot_author_ticket(),
                RecordProgress::Failed => {
                    if let Some(sequence) = self.barrier.admitted_before_failure() {
                        self.bind_boot_recorded(sequence);
                    } else if self.barrier.pending_author().is_some() {
                        // This invocation moved the sole event into the previously
                        // empty slot, or resumed its checked exact ticket. Preserve
                        // custody even if the clock/prepare failed before admission.
                        self.retain_boot_author_ticket();
                    }
                    return Err(self.record_failed(io));
                }
                RecordProgress::BusyAuthored { event, .. } => {
                    *self.boot_author_event_mut() = Some(event);
                    return Err(self.boot_open_fault(io, "boot author admission refused"));
                }
                _ => {
                    return Err(
                        self.boot_open_fault(io, "boot author received another owner's result")
                    );
                }
            }
        }
        Ok(())
    }
    fn boot_author_event_mut(&mut self) -> &mut Option<AuthoredEvent> {
        &mut self
            .boot_open
            .as_mut()
            .expect("boot author owns this event")
            .author
            .as_mut()
            .expect("boot author owns this event")
            .event
    }

    fn retain_boot_author_ticket(&mut self) {
        let held = self
            .barrier
            .pending_author()
            .expect("boot pending author retains ticket");
        let author = self.boot_open.as_mut().unwrap().author.as_mut().unwrap();
        author.queued = true;
        author.ticket = Some(held.ticket);
    }
    fn bind_boot_recorded(&mut self, sequence: Sequence) {
        let owner = self.boot_open.as_mut().unwrap();
        let author = owner.author.take().expect("boot author owns this record");
        match author.kind {
            AuthorKind::Placement => {
                owner.placement = Some(sequence);
                owner.stage = Stage::PlacementRecorded;
            }
            AuthorKind::Refusal => {
                owner.refusal_sequence = Some(sequence);
                owner.stage = Stage::Refused;
            }
            AuthorKind::Mark => {
                owner.mark_sequence = Some(sequence);
                owner.stage = Stage::Complete;
            }
        }
        self.last_author_sequence = Some(sequence);
    }

    fn finish_boot_reservation(
        &mut self,
        decode: &mut DecodePort,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        if let Some(ticket) = self.boot_open.as_mut().unwrap().reservation.take()
            && let Err(error) = decode.finish_reserved(ticket)
        {
            self.boot_open.as_mut().unwrap().reservation = Some(error.reservation);
            return Err(
                self.boot_open_fault(io, format!("boot exchange cleanup: {:?}", error.failure))
            );
        }
        self.boot_open.as_mut().unwrap().owed = false;
        self.barrier.set_boot_open_owed(false);
        Ok(())
    }

    /// Transfer one actual boot continuation. The journal stays with its context;
    /// subsequent pair repair is still required and no user-gate admission follows.
    pub(in crate::flow) fn take_boot_open(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<Option<CheckedBootOpen>, RunFailure> {
        self.boot_open_live(io)?;
        let owner = self.boot_open.as_mut().unwrap();
        if owner.stage != Stage::Complete || owner.owed || owner.grant_taken {
            return Ok(None);
        }
        let proof = owner
            .checked
            .take()
            .expect("complete boot retains checked facts");
        if !Arc::ptr_eq(&proof.origin, &self.read_origin) {
            owner.checked = Some(proof);
            return Err(self.boot_open_fault(io, "boot proof identity changed"));
        }
        self.boot_journal = owner.journal.take();
        owner.grant_taken = true;
        Ok(Some(proof))
    }

    pub(in crate::flow) fn cancel_boot_open(&mut self) -> Result<(), RunFailure> {
        if self
            .boot_open
            .as_ref()
            .is_some_and(|owner| owner.grant_taken)
        {
            return Err(RunFailure::BootOpenOwned);
        }
        self.boot_enter_cancelled = true;
        if let Some(owner) = &mut self.boot_open {
            owner.revoke();
            owner.checked = None;
            owner.journal = None;
        }
        Ok(())
    }

    /// Active-tap failed Enter is distinct from the older preactivation rollback.
    /// Only this already-held boot account may cross the Releasing boundary.
    pub(in crate::flow) fn begin_boot_open_release(
        &mut self,
        leave: AbsoluteBound,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        if self.boot_open.is_none() {
            return Err(RunFailure::BootOpenOwned);
        }
        self.cancel_boot_open()?;
        if let Some(failure) = &self.failure {
            return Err(failure.clone());
        }
        // Expired Enter/Leave means immediate stopped disposal, not an
        // unreachable rollback that requires renewing the original window.
        let now = io
            .now()
            .map_err(|error| self.boot_open_fault(io, format!("boot release clock: {error}")))?;
        if now < self.last_ns {
            return Err(self.boot_open_fault(io, "boot release clock reversed"));
        }
        self.last_ns = now;
        if self.phase != RecordingPhase::Entered || self.flow.phase() != CoordinatorPhase::Active {
            return Err(RunFailure::BootOpenOwned);
        }
        if let Some(held) = self.deadlines.get(PumpDeadline::Leave) {
            if held != leave {
                return Err(self.boot_open_fault(io, "boot release replaced its Leave bound"));
            }
        } else {
            self.deadlines
                .arm(PumpDeadline::Leave, leave)
                .map_err(|error| {
                    self.boot_open_fault(io, format!("boot release bound: {error}"))
                })?;
        }
        self.boot_open.as_mut().unwrap().account_deadline = self
            .boot_open
            .as_ref()
            .unwrap()
            .account_deadline
            .min(leave.deadline_ns());
        let permit = BootReleasePermit {
            ticket: self
                .boot_open
                .as_ref()
                .unwrap()
                .author
                .as_ref()
                .and_then(|author| author.ticket),
        };
        self.flow
            .begin_boot_releasing(
                &mut FlowRecording {
                    barrier: &mut self.barrier,
                    inbox: &mut self.inbox,
                    budget: &mut self.budget,
                    author: &self.author,
                },
                &permit,
            )
            .map_err(|failure| self.flow_failed(io, failure))?;
        self.barrier.stop_for_flow_failure();
        self.phase = RecordingPhase::Releasing;
        Ok(())
    }

    /// One nonblocking step after cancellation or failure. Caller interleaves this
    /// with reserved terminal duties; this method never waits for ordinary capacity.
    pub(in crate::flow) fn advance_boot_open_account(
        &mut self,
        decode: &mut DecodePort,
        io: &mut impl PumpIo,
    ) -> Result<BootOpenProgress, RunFailure> {
        self.check_boot_open_port(decode)?;
        if !matches!(
            self.phase,
            RecordingPhase::Releasing | RecordingPhase::Failed
        ) {
            return Err(RunFailure::BootOpenOwned);
        }
        let Some(owner) = &mut self.boot_open else {
            return Err(RunFailure::BootOpenOwned);
        };
        owner.revoke();
        if !owner.owed {
            return Ok(owner.progress());
        }
        if !self.boot_account_clock(io) {
            self.abandon_boot_open(
                decode,
                "boot account reached its bound or lost the common clock",
            );
            return Ok(BootOpenProgress::Incomplete);
        }
        // Deadline/clock disposal above never delays mandatory terminal work.
        // Actual ordinary accounting cannot run ahead of the first fault.
        if self.phase == RecordingPhase::Failed
            && !self
                .halt_report
                .as_ref()
                .is_some_and(|report| report.attempted())
        {
            return Ok(BootOpenProgress::Pending);
        }
        if let Some(detail) = self.boot_open.as_ref().unwrap().unavailable.clone() {
            self.abandon_boot_open(decode, &detail);
            return Ok(BootOpenProgress::Incomplete);
        }
        let result = self.advance_boot_account_owned(decode, io);
        if let Err(error) = result {
            self.abandon_boot_open(decode, &format!("boot account unavailable: {error:?}"));
        }
        // Any consumed record or answer has already been retained above.
        if !self.boot_account_clock(io) && self.boot_open.as_ref().unwrap().owed {
            self.abandon_boot_open(
                decode,
                "boot account reached its original bound after progress",
            );
        }
        Ok(self.boot_open.as_ref().unwrap().progress())
    }

    fn advance_boot_account_owned(
        &mut self,
        decode: &mut DecodePort,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        let owner = self.boot_open.as_mut().unwrap();
        // No mark permission follows cancellation; retain any actually Recorded
        // position but withdraw this owner's unadmitted mark event.
        if owner.stage == Stage::Mark {
            if let Some(ticket) = owner.author.as_ref().and_then(|author| author.ticket)
                && !self.barrier.abandon_boot_author(ticket)
            {
                return Err(RunFailure::BootOpenOwned);
            }
            owner.author = None;
            owner.stage = Stage::PlacementRecorded;
        }
        owner.checked = None;
        owner.journal = None;
        if owner.stage == Stage::Exchange {
            if decode.open_not_started() {
                let reservation = owner.reservation.take().unwrap();
                match decode.withdraw_unsent(reservation) {
                    Ok(()) => {
                        owner.stage = Stage::Settled;
                        owner.owed = false;
                        self.barrier.set_boot_open_owed(false);
                        return Ok(());
                    }
                    Err(error) => {
                        owner.reservation = Some(error.reservation);
                        // Inbound protocol debris cannot license a first Open byte
                        // after cancellation. The caller retires this account incomplete.
                        return Err(RunFailure::BootOpenOwned);
                    }
                }
            }
            self.pump.stopped_decode_step(decode).map_err(|error| {
                self.boot_open_fault(io, format!("stopped boot exchange: {error:?}"))
            })?;
            if !self.boot_account_clock(io) {
                return Err(RunFailure::BootOpenOwned);
            }
            self.consume_boot_open(decode, io)?;
            if !self.boot_account_clock(io) {
                return Err(RunFailure::BootOpenOwned);
            }
        }
        let stage = self.boot_open.as_ref().unwrap().stage;
        if matches!(stage, Stage::Placement | Stage::Refusal) {
            self.record_boot_author(io, true)?;
        }
        if matches!(
            self.boot_open.as_ref().unwrap().stage,
            Stage::PlacementRecorded | Stage::Refused | Stage::Complete
        ) {
            self.finish_boot_reservation(decode, io)?;
            self.boot_open.as_mut().unwrap().stage = Stage::Settled;
        }
        Ok(())
    }

    fn boot_account_clock(&mut self, io: &mut impl PumpIo) -> bool {
        let Some(owner) = self.boot_open.as_ref() else {
            return false;
        };
        let mut deadline = owner.account_deadline;
        if let Some(leave) = self.deadlines.get(PumpDeadline::Leave) {
            deadline = deadline.min(leave.deadline_ns());
        }
        if let Some(halt) = self.flow.halt() {
            let Ok(halt) = halt.deadline() else {
                return false;
            };
            deadline = deadline.min(halt);
        }
        self.boot_open.as_mut().unwrap().account_deadline = deadline;
        match io.now() {
            Ok(now) if now >= self.last_ns && now < deadline => {
                self.last_ns = now;
                true
            }
            _ => false,
        }
    }
    fn abandon_boot_open(&mut self, decode: &mut DecodePort, detail: &str) {
        self.retain_boot_wire(decode);
        let owner = self.boot_open.as_mut().unwrap();
        if let Some(ticket) = owner.author.as_ref().and_then(|author| author.ticket) {
            let _ = self.barrier.abandon_boot_author(ticket);
        }
        owner.revoke();
        owner.owed = false;
        owner.stage = Stage::Incomplete;
        owner.checked = None;
        owner.journal = None;
        owner.incomplete.get_or_insert_with(|| detail.to_owned());
        // Drop poisons rather than unlocks the incomplete residency.
        owner.reservation.take();
        self.barrier.set_boot_open_owed(false);
        if let Some(halt) = self.flow.halt_mut() {
            let _ = halt.append_secondary(SecondaryFailure {
                reason: FlowHaltReason::CarryFailed,
                point: HaltPoint::Teardown,
                detail: detail.into(),
                observed_ns: Some(self.last_ns.into()),
            });
        }
    }
    fn retain_boot_wire(&mut self, decode: &mut DecodePort) {
        let Some(owner) = &mut self.boot_open else {
            return;
        };
        let Some(reservation) = &owner.reservation else {
            return;
        };
        if !reservation.belongs_to(decode) {
            return;
        }
        if owner.received.is_none()
            && let Ok(Some(received)) = decode.take_reserved(reservation)
        {
            owner.received = Some(received);
        }
        if owner.failed_wire.is_none()
            && let Ok(wire) = decode.take_failed_wire_reserved(reservation)
        {
            owner.failed_wire = wire;
        }
    }
    pub(in crate::flow) fn boot_open_request(&self) -> Option<&TokenDirective> {
        self.boot_open.as_ref().map(|owner| &owner.request)
    }
    pub(in crate::flow) fn boot_open_reply(&self) -> Option<&DecodeReply> {
        self.boot_open
            .as_ref()?
            .received
            .as_ref()
            .map(ReceivedDecode::reply)
    }
    pub(in crate::flow) fn boot_open_raw(&self) -> Option<&[u8]> {
        self.boot_open
            .as_ref()?
            .received
            .as_ref()
            .map(ReceivedDecode::raw_bytes)
    }
    pub(in crate::flow) fn boot_open_refusal(&self) -> Option<&LifecycleRefusal> {
        self.boot_open.as_ref()?.refusal.as_ref()
    }
    pub(in crate::flow) fn boot_open_incomplete(&self) -> Option<&str> {
        self.boot_open.as_ref()?.incomplete.as_deref()
    }
    pub(super) fn boot_owns_pending_author(&self) -> bool {
        self.boot_open
            .as_ref()
            .and_then(|owner| owner.author.as_ref())
            .and_then(|author| author.ticket)
            .is_some_and(|ticket| {
                self.barrier
                    .pending_author()
                    .is_some_and(|held| held.ticket == ticket)
            })
    }
    pub(super) fn freeze_boot_account_bound(&mut self) {
        let leave = self
            .deadlines
            .get(PumpDeadline::Leave)
            .map(AbsoluteBound::deadline_ns);
        let halt = self.flow.halt().and_then(|halt| halt.deadline().ok());
        if let Some(owner) = &mut self.boot_open {
            for deadline in leave.into_iter().chain(halt) {
                owner.account_deadline = owner.account_deadline.min(deadline);
            }
        }
    }
    pub(super) fn boot_open_revoked(&self) -> bool {
        self.boot_open
            .as_ref()
            .is_some_and(|owner| !owner.grant_taken && owner.revoked)
    }
    pub(in crate::flow) fn boot_open_account_settled(&self) -> bool {
        self.boot_open.as_ref().is_none_or(|owner| !owner.owed)
    }
    /// Consuming retirement keeps the decode descriptor until its account has a
    /// final disposition. Returning the port on refusal preserves caller custody.
    #[allow(clippy::result_large_err)] // Refusal returns the original owned transport.
    pub(in crate::flow) fn retire_boot_decode(&self, decode: DecodePort) -> Result<(), DecodePort> {
        if !matches!(
            self.phase,
            RecordingPhase::Releasing | RecordingPhase::Failed
        ) || !self.boot_open_account_settled()
            || self.check_boot_open_port(&decode).is_err()
        {
            return Err(decode);
        }
        drop(decode);
        Ok(())
    }
    fn boot_trace_fault(&mut self, io: &mut impl PumpIo, detail: impl Into<String>) -> RunFailure {
        let detail = detail.into();
        if let Some(owner) = &mut self.boot_open {
            owner.unavailable.get_or_insert_with(|| detail.clone());
        }
        self.fail(io, FlowHaltReason::TraceFailed, HaltPoint::Author, detail)
    }
    fn boot_open_fault(&mut self, io: &mut impl PumpIo, detail: impl Into<String>) -> RunFailure {
        let detail = detail.into();
        if let Some(owner) = &mut self.boot_open {
            owner.unavailable.get_or_insert_with(|| detail.clone());
        }
        self.fail(
            io,
            FlowHaltReason::ProtocolFailed,
            HaltPoint::Barrier,
            detail,
        )
    }
}
