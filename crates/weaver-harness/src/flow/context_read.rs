//! One bounded, immutable paged read on the existing state channel.
//! Every successful read records exact provenance after releasing its cursor.
//! Ordinary proofs grant inspection; the one boot entry seats its private head.
//! Neither result grants placement or decoder authority.

use super::*;
use crate::flow::coordinator::{ControlOperation, OperationStart};
use crate::flow::read::{CompletedRead, PageRead, ReadAdvance, ReadFailure};
use weaver_trace::TurnRef;
use weaver_types::state_flow::{DecimalU64, QueryResult, ReadSelector, StateControl, StateFrame};

#[path = "context_boot_read.rs"]
mod boot;
use boot::BootCandidate;
pub(in crate::flow) use boot::{
    BootHead, BootReadBeginError, BootReadBeginRefusal, BootReadCompletion,
};

const READ_NS: u64 = 2_000_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum ReadBeginRefusal {
    WrongPhase,
    Busy,
}

#[derive(Debug)]
pub(in crate::flow) struct ReadBeginError {
    pub selector: ReadSelector,
    pub turn: Option<TurnRef>,
    pub refusal: ReadBeginRefusal,
    pub failure: Option<RunFailure>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum ReadProgress {
    Progress,
    Pending,
    Complete,
}

/// Only this read owner can bind its released result to the recorder's proof.
/// Inspection copies cannot construct a HeldMaterial or a new RecordedRead.
#[derive(Debug)]
pub(in crate::flow) struct RecordedRead {
    read: CompletedRead,
    sequence: Sequence,
    origin: std::sync::Arc<()>,
}

impl RecordedRead {
    pub(in crate::flow) fn origin(&self) -> &std::sync::Arc<()> {
        &self.origin
    }

    pub(in crate::flow) fn belongs_to(&self, context: &RecordingContext) -> bool {
        std::sync::Arc::ptr_eq(&self.origin, &context.read_origin)
    }

    pub(in crate::flow) fn selector(&self) -> &ReadSelector {
        self.read.selector()
    }
    pub(in crate::flow) fn snapshot_id(&self) -> DecimalU64 {
        self.read.snapshot_id()
    }
    pub(in crate::flow) fn result(&self) -> &QueryResult {
        self.read.result()
    }
    pub(in crate::flow) fn raw_bytes(&self) -> &[u8] {
        self.read.raw_bytes()
    }
    pub(in crate::flow) fn recall_sequence(&self) -> Sequence {
        self.sequence
    }
}

/// Revoked consumption retains its exact disposition for inspection. It has no
/// accessor that transfers a read proof or exposes material for placement.
#[derive(Debug)]
pub(in crate::flow) struct CancelledRead {
    outcome: Result<RecordedRead, ReadFailure>,
}

impl CancelledRead {
    pub(in crate::flow) fn recall_sequence(&self) -> Option<Sequence> {
        self.outcome
            .as_ref()
            .ok()
            .map(RecordedRead::recall_sequence)
    }
    pub(in crate::flow) fn refusal(&self) -> Option<&ReadFailure> {
        self.outcome.as_ref().err()
    }
}

#[derive(Debug)]
pub(in crate::flow) enum ReadCompletion {
    Recorded(RecordedRead),
    Refused(ReadFailure),
    Cancelled(CancelledRead),
}

pub(super) struct ReadTransaction {
    purpose: ReadPurpose,
    pages: PageRead,
    turn: Option<TurnRef>,
    bound: AbsoluteBound,
    expected: Option<(DecimalU64, StateControl)>,
    // Validation consumed this answer, but its final clock check can yield a
    // lifecycle wait. Resume that check without replaying or losing the answer.
    accepted: Option<ReadAdvance>,
    released: Option<Result<CompletedRead, ReadFailure>>,
    recall: Option<AuthoredEvent>,
    author_started: bool,
    recorded: Option<Sequence>,
    cancelled: bool,
    complete: bool,
    // Preserve the received answer if higher-level validation fails after its
    // operation measurement was recorded. A failure never becomes empty data.
    rejected: Option<OperationCompletion>,
}

#[allow(clippy::large_enum_variant)] // Retain exact boot inputs in the one read owner.
enum ReadPurpose {
    Ordinary,
    Marks,
    Inward(weaver_types::state_flow::CallId),
    Boot {
        start: StartIdentity,
        enter: AbsoluteBound,
        candidate: Option<BootCandidate>,
    },
}

impl ReadTransaction {
    pub(super) fn freeze(&mut self) {
        self.pages.freeze();
    }

    pub(super) fn needs_failed_close(&self) -> bool {
        self.released.is_none() && self.accepted != Some(ReadAdvance::Complete)
    }

    #[cfg(test)]
    pub(super) fn rejected_for_test(&self) -> Option<&OperationCompletion> {
        self.rejected.as_ref()
    }
}

impl RecordingContext {
    /// A busy caller retains its selector and receives no request identity or
    /// new wait. Fresh-history filing barriers remain an explicit caller duty.
    /// This external entry excludes an owned pair, including gaps between its
    /// legs. Inward execution uses its exact private pair-owned purpose.
    #[allow(clippy::result_large_err)] // Return owned inputs without allocating on refusal.
    pub(in crate::flow) fn begin_read(
        &mut self,
        selector: ReadSelector,
        turn: Option<TurnRef>,
        io: &mut impl PumpIo,
    ) -> Result<(), ReadBeginError> {
        self.begin_read_with_purpose(selector, turn, ReadPurpose::Ordinary, io)
    }

    #[allow(clippy::result_large_err)] // Return the exact selector with admission refusal.
    pub(super) fn begin_marks_read(
        &mut self,
        turn: TurnRef,
        io: &mut impl PumpIo,
    ) -> Result<(), ReadBeginError> {
        self.begin_read_with_purpose(ReadSelector::Marks {}, Some(turn), ReadPurpose::Marks, io)
    }

    #[allow(clippy::result_large_err)] // Return the exact selector with admission refusal.
    pub(super) fn begin_inward_read(
        &mut self,
        call: weaver_types::state_flow::CallId,
        selector: ReadSelector,
        turn: TurnRef,
        io: &mut impl PumpIo,
    ) -> Result<(), ReadBeginError> {
        self.begin_read_with_purpose(selector, Some(turn), ReadPurpose::Inward(call), io)
    }

    #[allow(clippy::result_large_err)]
    fn begin_read_with_purpose(
        &mut self,
        selector: ReadSelector,
        turn: Option<TurnRef>,
        purpose: ReadPurpose,
        io: &mut impl PumpIo,
    ) -> Result<(), ReadBeginError> {
        if let Err(failure) = self.live(io) {
            return Err(ReadBeginError {
                selector,
                turn,
                refusal: ReadBeginRefusal::WrongPhase,
                failure: Some(failure),
            });
        }
        if self.phase != RecordingPhase::Entered
            || self.flow.phase() != CoordinatorPhase::Active
            || !self.barrier.is_active()
        {
            return Err(ReadBeginError {
                selector,
                turn,
                refusal: ReadBeginRefusal::WrongPhase,
                failure: None,
            });
        }
        let inward = match &purpose {
            ReadPurpose::Inward(call) if self.inward_read_allowed(call) => Some(call.clone()),
            ReadPurpose::Inward(_) => {
                return Err(ReadBeginError {
                    selector,
                    turn,
                    refusal: ReadBeginRefusal::Busy,
                    failure: None,
                });
            }
            _ => None,
        };
        if (self.marks.is_some() && !matches!(purpose, ReadPurpose::Marks))
            || self.read.is_some()
            || self.measure_blocks_external()
            || self.member_capture.is_some()
            || self.freshness.is_some()
            || self.output.is_some()
            || self.filing.is_some()
            || (self.pair_work.is_some() && inward.is_none())
            || self.templates_block_content()
            || self.input.snapshot().is_some()
            || self.input_author.is_some()
            || self.input_turn.is_some()
            || self.repair_blocks_external()
            || self.waiting_content.is_some()
            || self.completed_author.is_some()
            || self.barrier.pending().is_some()
            || self.flow.has_pending_record()
            || self.flow.pending_control().is_some()
            || self.flow.pending_check().is_some()
            || self.flow.operation_completion().is_some()
            || !self.flow.state().outstanding().is_empty()
            || self.flow.state().held_measurement().is_some()
            || self.deadlines.get(PumpDeadline::Gate).is_some()
            || self.deadlines.get(PumpDeadline::Cut).is_some()
            || self.deadlines.get(PumpDeadline::StateControl).is_some()
        {
            return Err(ReadBeginError {
                selector,
                turn,
                refusal: ReadBeginRefusal::Busy,
                failure: None,
            });
        }
        let start = self.last_ns;
        let prepared = (|| {
            let limits = self
                .open_limits
                .as_ref()
                .ok_or_else(|| "read has no admitted byte limit".to_owned())?;
            let pages = PageRead::new(selector.clone(), limits.max_result_bytes)
                .map_err(|error| error.to_string())?;
            let mut end = start
                .checked_add(READ_NS)
                .ok_or_else(|| "read deadline overflow".to_owned())?;
            for key in [PumpDeadline::Enter, PumpDeadline::Leave] {
                if let Some(bound) = self.deadlines.get(key) {
                    end = end.min(bound.deadline_ns());
                }
            }
            let bound =
                AbsoluteBound::from_absolute(start, end).map_err(|error| error.to_string())?;
            self.deadlines
                .arm(PumpDeadline::StateControl, bound)
                .map_err(|error| error.to_string())?;
            Ok::<_, String>((pages, bound))
        })();
        let (pages, bound) = match prepared {
            Ok(prepared) => prepared,
            Err(detail) => {
                return Err(ReadBeginError {
                    selector,
                    turn,
                    refusal: ReadBeginRefusal::WrongPhase,
                    failure: Some(self.read_fault(io, detail)),
                });
            }
        };
        self.read = Some(ReadTransaction {
            purpose,
            pages,
            turn: turn.clone(),
            bound,
            expected: None,
            accepted: None,
            released: None,
            recall: None,
            author_started: false,
            recorded: None,
            cancelled: false,
            complete: false,
            rejected: None,
        });
        if let Some(call) = &inward
            && let Err(failure) = self.admit_inward_read(call, io)
        {
            return Err(ReadBeginError {
                selector,
                turn,
                refusal: ReadBeginRefusal::WrongPhase,
                failure: Some(failure),
            });
        }
        match self.queue_read_leg(io) {
            Ok(true) => Ok(()),
            // Admission already transferred the exact pair into its read owner.
            // Retain that deadline and retry the first control without rollback.
            Ok(false) if inward.is_some() => Ok(()),
            Ok(false) => {
                self.read = None;
                self.deadlines.disarm(PumpDeadline::StateControl);
                Err(ReadBeginError {
                    selector,
                    turn,
                    refusal: ReadBeginRefusal::Busy,
                    failure: None,
                })
            }
            Err(failure) => Err(ReadBeginError {
                selector,
                turn,
                refusal: ReadBeginRefusal::WrongPhase,
                failure: Some(failure),
            }),
        }
    }

    /// Bounded local reduction only. The caller drives I/O with the common
    /// pump, whose lifecycle priority and existing waits remain unchanged.
    pub(in crate::flow) fn advance_read<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<ReadProgress, RunFailure> {
        self.live(ports.io)?;
        if self.marks_blocks_external() {
            return Err(RunFailure::MarksOwned);
        }
        if self.inward_blocks_external() {
            return Err(RunFailure::PairOwned);
        }
        self.read_wait_matches(ports.io)?;
        if self.read_blocks_reduction() {
            return self.advance_read_recall(ports);
        }
        let advance = self.reduce_flow(ports.io)?;
        if self.read_blocks_reduction() {
            return self.advance_read_recall(ports);
        }
        if self.read.as_ref().is_some_and(|read| read.complete) {
            return Ok(ReadProgress::Complete);
        }
        if self.queue_read_leg(ports.io)? || advance == Advance::Progress {
            Ok(ReadProgress::Progress)
        } else {
            Ok(ReadProgress::Pending)
        }
    }

    /// Transfer one result only after Released and recall Recorded. A correct
    /// refusal stays distinct from successful empty content and cancellation.
    pub(in crate::flow) fn take_read(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<Option<ReadCompletion>, RunFailure> {
        self.live(io)?;
        if self.marks_blocks_external() {
            return Err(RunFailure::MarksOwned);
        }
        if self.inward_blocks_external() {
            return Err(RunFailure::PairOwned);
        }
        if self
            .read
            .as_ref()
            .is_some_and(|read| matches!(read.purpose, ReadPurpose::Boot { .. }))
        {
            return Err(RunFailure::ReadOwned);
        }
        let Some(read) = self.read.as_mut().filter(|read| read.complete) else {
            return Ok(None);
        };
        let Some(result) = read.released.take() else {
            return Err(self.read_fault(io, "complete read lost its owned result"));
        };
        let result = match result {
            Ok(completed) => Ok(RecordedRead {
                read: completed,
                origin: self.read_origin.clone(),
                sequence: read
                    .recorded
                    .expect("successful read keeps its recall proof"),
            }),
            Err(refusal) => Err(refusal),
        };
        let result = if read.cancelled {
            ReadCompletion::Cancelled(CancelledRead { outcome: result })
        } else {
            match result {
                Ok(recorded) => ReadCompletion::Recorded(recorded),
                Err(refusal) => ReadCompletion::Refused(refusal),
            }
        };
        self.read = None;
        Ok(Some(result))
    }

    /// Revoke consumption without discarding a sent exchange, acquired cursor
    /// or pending author. The same bounded read finishes its owed work.
    /// This sets the disposition only: the caller must continue driving the
    /// owner and take its Cancelled result. Cancelling without an owned read is
    /// a caller defect and uses the same protocol-failure path as other owners.
    pub(in crate::flow) fn cancel_read(&mut self, io: &mut impl PumpIo) -> Result<(), RunFailure> {
        self.live(io)?;
        if self.marks_blocks_external() {
            return Err(RunFailure::MarksOwned);
        }
        if self.inward_blocks_external() {
            return Err(RunFailure::PairOwned);
        }
        self.read_wait_matches(io)?;
        self.read.as_mut().expect("checked read owner").cancelled = true;
        Ok(())
    }

    pub(super) fn read_blocks_reduction(&self) -> bool {
        self.read
            .as_ref()
            .is_some_and(|read| read.released.is_some() && !read.complete)
    }

    pub(super) fn read_wait_still_owned(&self, key: PumpDeadline) -> bool {
        self.read.as_ref().is_some_and(|read| match key {
            PumpDeadline::StateControl => !read.complete,
            PumpDeadline::Enter => matches!(read.purpose, ReadPurpose::Boot { .. }),
            _ => false,
        })
    }

    fn read_wait_matches(&mut self, io: &mut impl PumpIo) -> Result<(), RunFailure> {
        let Some(read) = &self.read else {
            return Err(self.read_fault(io, "read advance has no owner"));
        };
        if !read.complete && self.deadlines.get(PumpDeadline::StateControl) != Some(read.bound) {
            return Err(self.read_fault(io, "read's absolute wait was removed or replaced"));
        }
        if let ReadPurpose::Boot { enter, .. } = &read.purpose
            && self.deadlines.get(PumpDeadline::Enter) != Some(*enter)
        {
            return Err(self.read_fault(io, "boot read lost its original Enter wait"));
        }
        Ok(())
    }

    fn queue_read_leg(&mut self, io: &mut impl PumpIo) -> Result<bool, RunFailure> {
        self.live(io)?;
        let Some(read) = &self.read else {
            return Ok(false);
        };
        if read.complete || read.expected.is_some() || read.released.is_some() {
            return Ok(false);
        }
        self.read_wait_matches(io)?;
        let read = self.read.as_ref().unwrap();
        let Some(request) = read.pages.next_control() else {
            return Err(self.read_fault(io, "unfinished read has no next request"));
        };
        let operation = ControlOperation::try_from((request.clone(), read.turn.clone()))
            .expect("read core emits only read controls");
        let result = self.flow.begin_control(
            &mut FlowRecording {
                barrier: &mut self.barrier,
                inbox: &mut self.inbox,
                budget: &mut self.budget,
                author: &self.author,
            },
            io,
            operation,
        );
        let result = result.map_err(|failure| self.flow_failed(io, failure))?;
        let OperationStart::Started { request_id } = result else {
            return Ok(false);
        };
        let read = self.read.as_mut().unwrap();
        read.expected = Some((request_id, request.clone()));
        if let StateControl::Read { selector } = &request {
            // Commit first-content ownership before a post-enqueue clock can
            // yield. A Busy call that allocated no request changes no marker.
            if boot::carries_content(selector) {
                self.content_read_started = true;
            }
            if matches!(read.purpose, ReadPurpose::Boot { .. }) {
                self.boot_read_started = true;
            }
        }
        if let Err(error) = read.pages.bind_request(request_id, &request) {
            return Err(self.read_fault(io, error.to_string()));
        }
        self.live(io)?;
        Ok(true)
    }

    /// A consumed answer keeps the next reduction across a lifecycle yield.
    /// Finish it before the coordinator can claim another recorder ticket;
    /// successful release must install its recall ownership first.
    pub(super) fn resume_accepted_read(&mut self, io: &mut impl PumpIo) -> Result<(), RunFailure> {
        if self
            .read
            .as_ref()
            .is_some_and(|read| read.accepted.is_some())
        {
            self.finish_accepted_read(io)?;
        }
        Ok(())
    }

    /// A state answer is consumed only after the coordinator's exact
    /// operation-measurement proof, never on socket read or send success.
    pub(super) fn settle_recorded_read(&mut self, io: &mut impl PumpIo) -> Result<(), RunFailure> {
        let Some(read) = &self.read else {
            return Ok(());
        };
        if read.complete || read.released.is_some() {
            return Ok(());
        }
        if read.accepted.is_some() {
            return self.finish_accepted_read(io);
        }
        let Some(completion) = self.flow.operation_completion() else {
            return Ok(());
        };
        let Some((id, expected)) = &read.expected else {
            return Err(self.read_fault(io, "read result arrived without an owned request"));
        };
        let OperationCompletion::State {
            request,
            turn,
            completion,
        } = completion
        else {
            return Err(self.read_fault(io, "read owner received a foreign completion"));
        };
        if request != expected
            || turn != &read.turn
            || completion.answer.common().request_id != *id
            || completion.discard_capture_result
        {
            return Err(self.read_fault(io, "read result differs from its exact request"));
        }
        let bound = read.bound;
        if self.deadlines.get(PumpDeadline::StateControl) != Some(bound) {
            return Err(self.read_fault(io, "read result has no original wait"));
        }
        // Check the common clock before accepting the final release as well as
        // each page. Trace pressure may consume the remaining read allowance.
        self.live(io)?;
        let result = self.flow.take_operation_completion(&mut FlowRecording {
            barrier: &mut self.barrier,
            inbox: &mut self.inbox,
            budget: &mut self.budget,
            author: &self.author,
        });
        let Some(result) = result.map_err(|failure| self.flow_failed(io, failure))? else {
            return Ok(());
        };
        let OperationCompletion::State {
            request,
            completion,
            ..
        } = &result
        else {
            unreachable!("checked state completion keeps its owner");
        };
        let StateFrame::ControlAnswer { common, frame, .. } = &completion.answer else {
            self.read.as_mut().unwrap().rejected = Some(result);
            return Err(self.read_fault(io, "read completed with a non-control answer"));
        };
        let accepted = self
            .read
            .as_mut()
            .unwrap()
            .pages
            .accept(common.request_id, request, frame);
        match accepted {
            Ok(progress) => {
                self.read.as_mut().unwrap().accepted = Some(progress);
                self.finish_accepted_read(io)
            }
            Err(error) => {
                self.read.as_mut().unwrap().rejected = Some(result);
                Err(self.read_fault(io, error.to_string()))
            }
        }
    }

    fn finish_accepted_read(&mut self, io: &mut impl PumpIo) -> Result<(), RunFailure> {
        // Validation and the final release reduction belong to this same wait.
        // Retain their continuation across a recoverable lifecycle yield, and
        // recheck before publishing another request or disarming the read bound.
        self.read_wait_matches(io)?;
        self.live(io)?;
        let read = self.read.as_mut().unwrap();
        let progress = read.accepted.take().expect("validated read continuation");
        read.expected = None;
        if progress == ReadAdvance::Complete {
            read.released = Some(
                read.pages
                    .take()
                    .expect("complete read keeps its disposition"),
            );
            if matches!(read.released, Some(Err(_))) {
                read.complete = true;
                self.deadlines.disarm(PumpDeadline::StateControl);
            } else {
                let completed = read.released.as_ref().unwrap().as_ref().unwrap();
                let record = match &mut read.purpose {
                    ReadPurpose::Ordinary | ReadPurpose::Marks | ReadPurpose::Inward(_) => {
                        completed.recall().map_err(|error| error.to_string())
                    }
                    ReadPurpose::Boot {
                        start, candidate, ..
                    } => BootCandidate::prepare(completed, start).map(|(held, record)| {
                        *candidate = Some(held);
                        record
                    }),
                };
                match record {
                    Ok(record) => {
                        read.recall = Some(AuthoredEvent {
                            kind: Kind::Recall,
                            subsystem: weaver_trace::Subsystem::Harness,
                            turn: None,
                            source: super::super::author::AuthoredSource::Runtime,
                            causal_parent: None,
                            payload: Some(Payload::RecallRecord(Box::new(record))),
                        })
                    }
                    Err(error) => return Err(self.read_fault(io, error.to_string())),
                }
            }
        }
        Ok(())
    }

    fn advance_read_recall<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<ReadProgress, RunFailure> {
        self.live(ports.io)?;
        self.read_wait_matches(ports.io)?;
        if self.flow.has_pending_record()
            || self.waiting_content.is_some()
            || self.completed_author.is_some()
        {
            return Err(self.read_fault(ports.io, "recall author has an unsettled predecessor"));
        }
        let progress = if self.read.as_ref().unwrap().author_started {
            self.barrier
                .resume_with_clock(&mut self.inbox, &mut self.budget, &mut || ports.io.now())
        } else {
            let wall = wall_ms().map_err(|detail| {
                self.fail(
                    ports.io,
                    FlowHaltReason::TraceFailed,
                    HaltPoint::Author,
                    detail,
                )
            })?;
            let read = self.read.as_mut().unwrap();
            let event = read
                .recall
                .take()
                .expect("released success keeps its immutable recall");
            read.author_started = true;
            self.barrier.submit_authored_with_clock(
                event,
                RecordPurpose::Observation,
                &self.author,
                wall,
                &mut self.inbox,
                &mut self.budget,
                &mut || ports.io.now(),
            )
        };
        match progress {
            RecordProgress::Recorded {
                sequence,
                proof: None,
            } => {
                // Save the actual admitted identity before any fallible hook.
                self.read.as_mut().unwrap().recorded = Some(sequence);
                self.last_author_sequence = Some(sequence);
                let now = ports.io.now().map_err(|error| {
                    self.fail(
                        ports.io,
                        FlowHaltReason::TraceFailed,
                        HaltPoint::Author,
                        format!("recall Recorded clock failed: {error}"),
                    )
                })?;
                if now < self.last_ns {
                    return Err(self.fail(
                        ports.io,
                        FlowHaltReason::TraceFailed,
                        HaltPoint::Author,
                        "recall Recorded clock reversed",
                    ));
                }
                self.last_ns = now;
                let bound = self.read.as_ref().unwrap().bound;
                if bound.expired(now) {
                    // An earlier lifecycle expiry still owns precedence. The
                    // admitted sequence remains held and cannot be resubmitted.
                    self.live(ports.io)?;
                    return Err(self.expired(ports.io, PumpDeadline::StateControl, bound));
                }
                self.read.as_mut().unwrap().complete = true;
                self.deadlines.disarm(PumpDeadline::StateControl);
                self.live(ports.io)?;
                Ok(ReadProgress::Complete)
            }
            RecordProgress::Pending => {
                self.live(ports.io)?;
                Ok(ReadProgress::Pending)
            }
            RecordProgress::BusyAuthored { event, .. } => {
                self.retain_refused(event);
                Err(self.record_failed(ports.io))
            }
            RecordProgress::Failed => Err(self.record_failed(ports.io)),
            RecordProgress::Busy { .. } | RecordProgress::Recorded { proof: Some(_), .. } => {
                Err(self.read_fault(ports.io, "recall author received another owner's proof"))
            }
        }
    }

    fn read_fault(&mut self, io: &mut impl PumpIo, detail: impl Into<String>) -> RunFailure {
        self.fail(
            io,
            FlowHaltReason::ProtocolFailed,
            HaltPoint::StateReply,
            detail,
        )
    }
}
