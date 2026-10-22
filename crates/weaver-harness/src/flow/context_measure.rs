//! One read-only decode measurement, with owned proposal and actual wire evidence.
//!
//! The supplied position and revision are expectations from the caller's journal,
//! not a boot certificate. The result grants no cut, placement or generation. Its
//! enclosing Decision or Cut wait remains armed for the later recorded disposition.

use super::*;
use crate::channel::DecodeReply;
use crate::flow::author::AuthoredSource;
use crate::flow::pump_transport::{DecodePort, DecodeReservation, DecodeStage, ReceivedDecode};
use weaver_trace::{Subsystem, TurnRef};
use weaver_traits::Message;
use weaver_types::state_flow::{DecimalU64, PlacementCode};
use weaver_types::{RefusalRecord, TokenAnswer, TokenAsk, TokenDirective, TokenRefusal};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) struct MeasurePosition {
    pub resident: u64,
    pub prefix_tokens: u64,
    pub capacity: u64,
}

#[derive(Debug)]
pub(in crate::flow) struct MeasureRequest {
    pub assembly_id: DecimalU64,
    pub revision: DecimalU64,
    pub keep: u64,
    pub delta: Vec<Message>,
    pub tool_schemas: Vec<String>,
    pub turn: Option<TurnRef>,
    pub position: MeasurePosition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum MeasureBeginRefusal {
    WrongPhase,
    Busy,
    MissingWait,
    InvalidPosition,
    WrongTurn,
    WrongWait,
}

#[derive(Debug)]
pub(in crate::flow) struct MeasureBeginError {
    pub request: MeasureRequest,
    pub refusal: MeasureBeginRefusal,
    pub failure: Option<RunFailure>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum MeasureProgress {
    Progress,
    Pending,
    Complete,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) struct MeasureFacts {
    pub resident: u64,
    pub prefix_tokens: u64,
    pub keep: u64,
    pub input_tokens: u64,
    pub capacity: u64,
}

#[derive(Debug)]
struct MeasureEvidence {
    request: MeasureRequest,
    received: ReceivedDecode,
    wait: PumpDeadline,
    bound: AbsoluteBound,
}

#[derive(Debug)]
pub(in crate::flow) struct MeasuredProposal {
    evidence: MeasureEvidence,
    facts: MeasureFacts,
}

impl MeasuredProposal {
    pub(in crate::flow) fn facts(&self) -> &MeasureFacts {
        &self.facts
    }
}

#[derive(Debug)]
pub(in crate::flow) struct RefusedMeasure {
    evidence: MeasureEvidence,
    code: PlacementCode,
    decode_source: Option<Sequence>,
    placement_source: Sequence,
}

impl RefusedMeasure {
    pub(in crate::flow) fn code(&self) -> PlacementCode {
        self.code
    }
    pub(in crate::flow) fn sources(&self) -> (Option<Sequence>, Sequence) {
        (self.decode_source, self.placement_source)
    }
}

#[derive(Debug)]
pub(in crate::flow) struct CancelledMeasure {
    evidence: MeasureEvidence,
}

macro_rules! evidence_accessors {
    ($type:ty) => {
        impl $type {
            pub(in crate::flow) fn request(&self) -> &MeasureRequest {
                &self.evidence.request
            }
            pub(in crate::flow) fn reply(&self) -> &DecodeReply {
                self.evidence.received.reply()
            }
            pub(in crate::flow) fn raw_bytes(&self) -> &[u8] {
                self.evidence.received.raw_bytes()
            }
            pub(in crate::flow) fn wait(&self) -> PumpDeadline {
                self.evidence.wait
            }
            pub(in crate::flow) fn bound(&self) -> AbsoluteBound {
                self.evidence.bound
            }
        }
    };
}

evidence_accessors!(MeasuredProposal);
evidence_accessors!(RefusedMeasure);
evidence_accessors!(CancelledMeasure);

/// These are factual results of one exchange, never executable capabilities.
/// Only the private transaction can construct them after consuming its terminal.
#[derive(Debug)]
pub(in crate::flow) enum MeasureCompletion {
    Measured(MeasuredProposal),
    Refused(RefusedMeasure),
    Cancelled(CancelledMeasure),
}

impl MeasureCompletion {
    fn evidence(&self) -> &MeasureEvidence {
        match self {
            Self::Measured(value) => &value.evidence,
            Self::Refused(value) => &value.evidence,
            Self::Cancelled(value) => &value.evidence,
        }
    }
    pub(in crate::flow) fn request(&self) -> &MeasureRequest {
        &self.evidence().request
    }
    pub(in crate::flow) fn reply(&self) -> &DecodeReply {
        self.evidence().received.reply()
    }
    pub(in crate::flow) fn raw_bytes(&self) -> &[u8] {
        self.evidence().received.raw_bytes()
    }
    pub(in crate::flow) fn wait(&self) -> PumpDeadline {
        self.evidence().wait
    }
    pub(in crate::flow) fn bound(&self) -> AbsoluteBound {
        self.evidence().bound
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AuthorPurpose {
    Fault,
    DecodeRefusal,
    PlacementRefusal,
}

struct MeasureAuthor {
    purpose: AuthorPurpose,
}

pub(super) struct MeasureTransaction {
    request: MeasureRequest,
    reservation: Option<DecodeReservation>,
    wait: PumpDeadline,
    bound: AbsoluteBound,
    received: Option<ReceivedDecode>,
    failed_wire: Option<Vec<u8>>,
    facts: Option<MeasureFacts>,
    refusal: Option<PlacementCode>,
    decode_source: Option<Sequence>,
    placement_source: Option<Sequence>,
    author: Option<MeasureAuthor>,
    failed_refusal: Option<AuthoredEvent>,
    failed_refusal_queued: bool,
    failed_refusal_error: bool,
    cancelled: bool,
    frozen: bool,
}

impl MeasureTransaction {
    pub(super) fn freeze(&mut self) {
        self.frozen = true;
    }

    fn complete(&self) -> bool {
        !self.frozen
            && self.author.is_none()
            && self.received.is_some()
            && (self.facts.is_some() || (self.refusal.is_some() && self.placement_source.is_some()))
    }

    fn progress(&self) -> MeasureProgress {
        if self.complete() {
            if self.cancelled {
                MeasureProgress::Cancelled
            } else {
                MeasureProgress::Complete
            }
        } else {
            MeasureProgress::Pending
        }
    }
}

impl RecordingContext {
    #[cfg(test)]
    pub(super) fn pending_measure_raw(&self) -> Option<&[u8]> {
        self.measure
            .as_ref()?
            .received
            .as_ref()
            .map(ReceivedDecode::raw_bytes)
    }

    pub(super) fn measure_blocks_external(&self) -> bool {
        self.measure.is_some()
    }

    pub(super) fn measure_wait_still_owned(&self, wait: PumpDeadline) -> bool {
        self.measure
            .as_ref()
            .is_some_and(|owner| owner.wait == wait)
    }

    pub(super) fn check_measure_port(&self, decode: &DecodePort) -> Result<(), RunFailure> {
        if self.measure.as_ref().is_some_and(|owner| {
            owner
                .reservation
                .as_ref()
                .is_some_and(|ticket| !ticket.belongs_to(decode))
        }) {
            return Err(RunFailure::MeasureOwned);
        }
        Ok(())
    }

    /// No decode work starts on refusal, and the intact proposal returns to its
    /// caller. A count expectation is not proof of a journal or admitted boot.
    #[allow(clippy::result_large_err)]
    pub(in crate::flow) fn begin_measurement(
        &mut self,
        request: MeasureRequest,
        wait: PumpDeadline,
        decode: &mut DecodePort,
        io: &mut impl PumpIo,
    ) -> Result<(), MeasureBeginError> {
        let error = |request, refusal, failure| MeasureBeginError {
            request,
            refusal,
            failure,
        };
        if let Err(failure) = self.live(io) {
            return Err(error(
                request,
                MeasureBeginRefusal::WrongPhase,
                Some(failure),
            ));
        }
        if self.phase != RecordingPhase::Entered
            || self.flow.phase() != CoordinatorPhase::Active
            || !self.barrier.is_active()
        {
            return Err(error(request, MeasureBeginRefusal::WrongPhase, None));
        }
        if !matches!(wait, PumpDeadline::Decision | PumpDeadline::Cut) {
            return Err(error(request, MeasureBeginRefusal::WrongWait, None));
        }
        let Some(bound) = self.deadlines.get(wait) else {
            return Err(error(request, MeasureBeginRefusal::MissingWait, None));
        };
        if request.position.capacity == 0
            || request.position.prefix_tokens > request.position.resident
            || request.position.resident > request.position.capacity
            || (wait == PumpDeadline::Decision && request.keep != request.position.resident)
        {
            return Err(error(request, MeasureBeginRefusal::InvalidPosition, None));
        }
        if request.turn != self.open_turn || (wait == PumpDeadline::Cut && self.open_turn.is_some())
        {
            return Err(error(request, MeasureBeginRefusal::WrongTurn, None));
        }
        if self.measure.is_some()
            || self.output.is_some()
            || self.filing.is_some()
            || self.pair_work.is_some()
            || self.repair_blocks_external()
            || self.input.snapshot().is_some()
            || self.input_turn.is_some()
            || self.input_author.is_some()
            || self.member_capture.is_some()
            || self.read.is_some()
            || self.templates_block_content()
            || self.template_blocked_author.is_some()
            || self.waiting_content.is_some()
            || self.completed_author.is_some()
            || self.barrier.pending().is_some()
            || self.barrier.pending_author().is_some()
            || self.flow.has_pending_record()
        {
            return Err(error(request, MeasureBeginRefusal::Busy, None));
        }
        let directive = TokenDirective::MeasureDelta {
            keep: request.keep,
            delta: request.delta.clone(),
            tool_schemas: request.tool_schemas.clone(),
        };
        let reservation = match decode.stage_reserved(directive) {
            Ok(DecodeStage::Queued(ticket)) => ticket,
            Ok(DecodeStage::Busy(_)) => {
                return Err(error(request, MeasureBeginRefusal::Busy, None));
            }
            Err(failed) => {
                let failure =
                    self.measure_fault(io, format!("measure stage: {:?}", failed.failure));
                return Err(error(
                    request,
                    MeasureBeginRefusal::WrongPhase,
                    Some(failure),
                ));
            }
        };
        self.measure = Some(MeasureTransaction {
            request,
            reservation: Some(reservation),
            wait,
            bound,
            received: None,
            failed_wire: None,
            facts: None,
            refusal: None,
            decode_source: None,
            placement_source: None,
            author: None,
            failed_refusal: None,
            failed_refusal_queued: false,
            failed_refusal_error: false,
            cancelled: false,
            frozen: false,
        });
        Ok(())
    }

    fn measure_wait_matches(&mut self, io: &mut impl PumpIo) -> Result<(), RunFailure> {
        let Some(owner) = &self.measure else {
            return Err(RunFailure::MeasureOwned);
        };
        if self.deadlines.get(owner.wait) != Some(owner.bound) {
            return Err(
                self.measure_fault(io, "measurement's enclosing wait was removed or replaced")
            );
        }
        Ok(())
    }

    /// One common-pump step advances transport when no completion is held. This
    /// reduction consumes only the completion reserved by this exact exchange.
    pub(in crate::flow) fn advance_measurement<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<MeasureProgress, RunFailure> {
        let decode = ports.decode.as_deref().ok_or(RunFailure::MeasureOwned)?;
        self.check_measure_port(decode)?;
        self.live(ports.io)?;
        self.measure_wait_matches(ports.io)?;
        if self.measure.as_ref().unwrap().author.is_some() {
            return self.record_measure_author(ports);
        }
        self.reduce_flow(ports.io)?;
        let owner = self.measure.as_ref().unwrap();
        if owner.complete() {
            return Ok(owner.progress());
        }
        if self.flow.has_pending_record()
            || self.barrier.pending().is_some()
            || self.barrier.pending_author().is_some()
        {
            self.pump_once(ports)?;
            self.reduce_flow(ports.io)?;
            return Ok(MeasureProgress::Pending);
        }
        if ports.decode.as_deref().unwrap().held().is_none() {
            self.pump_once(ports)?;
            self.reduce_flow(ports.io)?;
            if self.flow.has_pending_record()
                || self.barrier.pending().is_some()
                || self.barrier.pending_author().is_some()
            {
                return Ok(MeasureProgress::Pending);
            }
        }
        let ticket = self
            .measure
            .as_ref()
            .unwrap()
            .reservation
            .as_ref()
            .expect("measurement retains its reservation");
        let decode = ports.decode.as_deref_mut().unwrap();
        let received = match decode.take_reserved(ticket) {
            Ok(Some(received)) => received,
            Ok(None) => return Ok(MeasureProgress::Pending),
            Err(failure) => {
                self.retain_measure_transport_evidence(decode);
                return Err(self.measure_fault(ports.io, format!("measure receive: {failure:?}")));
            }
        };
        // Fault is an unsolicited emission. Only terminal replies need proof
        // that their first segment followed the complete request.
        let fault = matches!(received.reply(), DecodeReply::Answer(TokenAnswer::Fault(_)));
        let legitimate =
            received.matches(ticket) && (fault || (received.after_send() && ticket.sent()));
        self.measure.as_mut().unwrap().received = Some(received);
        if !legitimate {
            return Err(self.measure_fault(
                ports.io,
                "measurement reply preceded or mismatched its whole request",
            ));
        }
        if let Some(failure) = ports.decode.as_deref().unwrap().failure() {
            return Err(
                self.measure_fault(ports.io, format!("measure channel failed: {failure:?}"))
            );
        }
        let reply = self
            .measure
            .as_ref()
            .unwrap()
            .received
            .as_ref()
            .unwrap()
            .reply();
        match reply {
            DecodeReply::Answer(TokenAnswer::DeltaMeasured {
                resident,
                prefix_tokens,
                keep,
                input_tokens,
                capacity,
            }) => {
                let facts = MeasureFacts {
                    resident: *resident,
                    prefix_tokens: *prefix_tokens,
                    keep: *keep,
                    input_tokens: *input_tokens,
                    capacity: *capacity,
                };
                let request = &self.measure.as_ref().unwrap().request;
                let expected_keep = request
                    .position
                    .prefix_tokens
                    .max(request.keep.min(request.position.resident));
                if facts.resident != request.position.resident
                    || facts.prefix_tokens != request.position.prefix_tokens
                    || facts.capacity != request.position.capacity
                    || facts.keep != expected_keep
                {
                    return Err(self.measure_fault(
                        ports.io,
                        "measurement counts differ from the unchanged expected position",
                    ));
                }
                self.measure.as_mut().unwrap().facts = Some(facts);
                self.live(ports.io)?;
                Ok(self.measure.as_ref().unwrap().progress())
            }
            DecodeReply::Answer(TokenAnswer::Fault(report)) => {
                let payload = serde_json::value::to_raw_value(report)
                    .map_err(|error| self.measure_fault(ports.io, error.to_string()))?;
                self.stage_measure_author(
                    AuthorPurpose::Fault,
                    Kind::Fault,
                    Payload::Fault(payload),
                );
                Ok(MeasureProgress::Progress)
            }
            DecodeReply::Refusal(refusal) => {
                if *refusal != TokenRefusal::MalformedDelta {
                    let detail = format!(
                        "SPU refused the measurement outside its admitted render case: {refusal:?}"
                    );
                    let record = RefusalRecord::Decode {
                        asked: TokenAsk::MeasureDelta {
                            keep: self.measure.as_ref().unwrap().request.keep,
                        },
                        refusal: refusal.clone(),
                    };
                    let first = self.measure_fault(ports.io, detail);
                    match serde_json::value::to_raw_value(&record) {
                        Ok(payload) => {
                            let owner = self.measure.as_mut().unwrap();
                            owner.failed_refusal = Some(AuthoredEvent {
                                kind: Kind::Refusal,
                                subsystem: Subsystem::SpuDecoder,
                                turn: owner.request.turn.clone(),
                                source: AuthoredSource::Runtime,
                                causal_parent: None,
                                payload: Some(Payload::Refusal(payload)),
                            });
                            let _ = self.failed_measure_refusal_progress(ports.io);
                        }
                        Err(error) => self.measure_refusal_report_failed(None, error.to_string()),
                    }
                    return Err(first);
                }
                let record = RefusalRecord::Decode {
                    asked: TokenAsk::MeasureDelta {
                        keep: self.measure.as_ref().unwrap().request.keep,
                    },
                    refusal: refusal.clone(),
                };
                let payload = serde_json::value::to_raw_value(&record)
                    .map_err(|error| self.measure_fault(ports.io, error.to_string()))?;
                let owner = self.measure.as_mut().unwrap();
                owner.refusal = Some(PlacementCode::InvalidSpan);
                self.stage_measure_author(
                    AuthorPurpose::DecodeRefusal,
                    Kind::Refusal,
                    Payload::Refusal(payload),
                );
                Ok(MeasureProgress::Progress)
            }
            other => {
                let detail = format!("unexpected measurement answer: {other:?}");
                Err(self.measure_fault(ports.io, detail))
            }
        }
    }

    fn stage_measure_author(&mut self, purpose: AuthorPurpose, kind: Kind, payload: Payload) {
        let owner = self.measure.as_mut().expect("measurement owns its author");
        owner.author = Some(MeasureAuthor { purpose });
        self.waiting_content = Some(AuthoredEvent {
            kind,
            subsystem: if purpose == AuthorPurpose::PlacementRefusal {
                Subsystem::Harness
            } else {
                Subsystem::SpuDecoder
            },
            turn: owner.request.turn.clone(),
            source: AuthoredSource::Runtime,
            causal_parent: None,
            payload: Some(payload),
        });
    }

    fn stage_measure_placement_refusal(&mut self, io: &mut impl PumpIo) -> Result<(), RunFailure> {
        let owner = self.measure.as_ref().unwrap();
        let code = owner.refusal.expect("refusal code was selected");
        let detail = match code {
            PlacementCode::InvalidSpan => "SPU refused the measurement's family rendering",
            PlacementCode::StalePosition => {
                "journal revision changed before measurement evidence was taken"
            }
            _ => unreachable!("measurement owns only render and revision refusals"),
        };
        let record = RefusalRecord::Placement {
            assembly_id: owner.request.assembly_id,
            code,
            detail: detail.into(),
        };
        let payload = serde_json::value::to_raw_value(&record)
            .map_err(|error| self.measure_fault(io, error.to_string()))?;
        self.stage_measure_author(
            AuthorPurpose::PlacementRefusal,
            Kind::Refusal,
            Payload::Refusal(payload),
        );
        Ok(())
    }

    fn record_measure_author<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<MeasureProgress, RunFailure> {
        let purpose = self
            .measure
            .as_ref()
            .unwrap()
            .author
            .as_ref()
            .unwrap()
            .purpose;
        let sequence = self.resume_owned_author_and_wait(ports)?;
        self.measure_wait_matches(ports.io)?;
        // The shared author has transferred this sequence. Save the next
        // measurement state before a lifecycle check can yield, so resumption
        // cannot ask the shared author to return the consumed sequence again.
        let owner = self.measure.as_mut().unwrap();
        owner.author = None;
        let progress = match purpose {
            AuthorPurpose::Fault => {
                // The fault is now on the trace. It closes no decode exchange.
                owner.received = None;
                MeasureProgress::Progress
            }
            AuthorPurpose::DecodeRefusal => {
                owner.decode_source = Some(sequence);
                self.stage_measure_placement_refusal(ports.io)?;
                MeasureProgress::Progress
            }
            AuthorPurpose::PlacementRefusal => {
                owner.placement_source = Some(sequence);
                owner.progress()
            }
        };
        self.live(ports.io)?;
        Ok(progress)
    }

    /// Cancellation revokes only the result. The pump must still finish a staged
    /// segmented request and consume its answer, without sending Cancel.
    pub(in crate::flow) fn cancel_measurement(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        self.live(io)?;
        self.measure_wait_matches(io)?;
        self.measure.as_mut().unwrap().cancelled = true;
        Ok(())
    }

    /// This check compares the caller's current journal revision with the one
    /// it supplied at begin. Place must supply journal truth and recheck again
    /// before append. Returning these counts never authorizes buffer mutation.
    pub(in crate::flow) fn take_measurement(
        &mut self,
        current_revision: DecimalU64,
        decode: &mut DecodePort,
        io: &mut impl PumpIo,
    ) -> Result<Option<MeasureCompletion>, RunFailure> {
        self.check_measure_port(decode)?;
        self.live(io)?;
        if self.measure.is_none() {
            return Ok(None);
        }
        self.measure_wait_matches(io)?;
        let owner = self.measure.as_ref().unwrap();
        if !owner.complete() {
            return Ok(None);
        }
        if !owner.cancelled && owner.refusal.is_none() && owner.request.revision != current_revision
        {
            self.measure.as_mut().unwrap().refusal = Some(PlacementCode::StalePosition);
            self.stage_measure_placement_refusal(io)?;
            return Ok(None);
        }
        if let Some(failure) = decode.failure() {
            let detail = format!("measure take on failed decode channel: {failure:?}");
            self.retain_measure_transport_evidence(decode);
            return Err(self.measure_fault(io, detail));
        }
        // Settlement consumes the reservation. Check the completed work before
        // that irreversible handoff, then transfer its exact owner without a
        // recoverable yield between those two ownership changes.
        self.live(io)?;
        let ticket = self
            .measure
            .as_mut()
            .unwrap()
            .reservation
            .take()
            .expect("one reservation");
        if let Err(error) = decode.finish_reserved(ticket) {
            self.measure.as_mut().unwrap().reservation = Some(error.reservation);
            return Err(
                self.measure_fault(io, format!("measurement settlement: {:?}", error.failure))
            );
        }
        let owner = self.measure.take().unwrap();
        let evidence = MeasureEvidence {
            request: owner.request,
            received: owner
                .received
                .expect("complete measurement retains its terminal"),
            wait: owner.wait,
            bound: owner.bound,
        };
        // The enclosing barrier is deliberately still armed. Its complete
        // place/cut owner owes the recorded disposition before retiring it.
        if owner.cancelled {
            Ok(Some(MeasureCompletion::Cancelled(CancelledMeasure {
                evidence,
            })))
        } else if let Some(code) = owner.refusal {
            Ok(Some(MeasureCompletion::Refused(RefusedMeasure {
                evidence,
                code,
                decode_source: owner.decode_source,
                placement_source: owner.placement_source.expect("refusal was recorded"),
            })))
        } else {
            Ok(Some(MeasureCompletion::Measured(MeasuredProposal {
                evidence,
                facts: owner.facts.expect("count was checked"),
            })))
        }
    }

    pub(super) fn retain_measure_transport_evidence(&mut self, decode: &mut DecodePort) {
        let Some(owner) = &mut self.measure else {
            return;
        };
        let Some(ticket) = &owner.reservation else {
            return;
        };
        if !ticket.belongs_to(decode) {
            return;
        }
        if owner.received.is_none()
            && let Ok(Some(received)) = decode.take_reserved(ticket)
        {
            owner.received = Some(received);
        }
        if owner.failed_wire.is_none()
            && let Ok(bytes) = decode.take_failed_wire_reserved(ticket)
        {
            owner.failed_wire = bytes;
        }
    }

    /// Only owed trace reporting progresses after this measurement's protocol
    /// failure. This uses the halt's existing deadline, never a serving wait,
    /// and cannot issue a copy or a measurement completion.
    pub(in crate::flow) fn failed_measure_refusal_progress(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<Option<Sequence>, RunFailure> {
        if self.phase != RecordingPhase::Failed {
            return Err(RunFailure::MeasureOwned);
        }
        let first = self
            .failure
            .clone()
            .expect("failed context retains first cause");
        let Some(owner) = &self.measure else {
            return Ok(None);
        };
        if owner.failed_refusal_error {
            return Err(first);
        }
        if owner.failed_refusal.is_none() && !owner.failed_refusal_queued {
            return Ok(owner.decode_source);
        }
        let now = match io.now() {
            Ok(now) if now >= self.last_ns => now,
            Ok(now) => {
                self.measure_refusal_report_failed(Some(now), "refusal report clock reversed");
                return Err(first);
            }
            Err(error) => {
                self.measure_refusal_report_failed(None, format!("refusal report clock: {error}"));
                return Err(first);
            }
        };
        self.last_ns = now;
        let deadline = self
            .flow
            .halt()
            .expect("failed context has a halt")
            .deadline();
        if deadline.is_err() || deadline.is_ok_and(|deadline| now >= deadline) {
            self.measure_refusal_report_failed(
                Some(now),
                "refusal report exceeded the halt deadline",
            );
            return Err(first);
        }

        if !self.measure.as_ref().unwrap().failed_refusal_queued {
            if self.barrier.pending().is_some() {
                if let Err(error) = self.barrier.resume_trace_only() {
                    self.measure_refusal_report_failed(
                        Some(now),
                        format!("prior trace drain: {error:?}"),
                    );
                    return Err(first);
                }
                return Ok(None);
            }
            let wall = match wall_ms() {
                Ok(wall) => wall,
                Err(error) => {
                    self.measure_refusal_report_failed(Some(now), error);
                    return Err(first);
                }
            };
            let event = self
                .measure
                .as_mut()
                .unwrap()
                .failed_refusal
                .take()
                .expect("unsubmitted refusal retains its original");
            if let Err(error) =
                self.barrier
                    .queue_trace_only_authored(event, &self.author, now, wall)
            {
                self.measure.as_mut().unwrap().failed_refusal = Some(error.event);
                self.measure_refusal_report_failed(Some(now), error.detail);
                return Err(first);
            }
            self.measure.as_mut().unwrap().failed_refusal_queued = true;
        }
        match self.barrier.resume_trace_only() {
            Ok(Some(sequence)) => {
                let owner = self.measure.as_mut().unwrap();
                owner.failed_refusal_queued = false;
                owner.decode_source = Some(sequence);
                Ok(Some(sequence))
            }
            Ok(None) => Ok(None),
            Err(error) => {
                self.measure_refusal_report_failed(
                    Some(now),
                    format!("measurement refusal trace: {error:?}"),
                );
                Err(first)
            }
        }
    }

    fn measure_refusal_report_failed(
        &mut self,
        observed_ns: Option<u64>,
        detail: impl Into<String>,
    ) {
        let Some(owner) = &mut self.measure else {
            return;
        };
        if owner.failed_refusal_error {
            return;
        }
        owner.failed_refusal_error = true;
        if let Some(halt) = self.flow.halt_mut() {
            let _ = halt.append_secondary(SecondaryFailure {
                reason: FlowHaltReason::TraceFailed,
                point: HaltPoint::Teardown,
                observed_ns: observed_ns.map(Into::into),
                detail: detail.into(),
            });
        }
    }

    pub(super) fn measure_fault(
        &mut self,
        io: &mut impl PumpIo,
        detail: impl Into<String>,
    ) -> RunFailure {
        self.fail(
            io,
            FlowHaltReason::ProtocolFailed,
            HaltPoint::Barrier,
            detail,
        )
    }
}
