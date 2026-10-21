//! One bounded, immutable paged read on the existing state channel.
//! This owner publishes no recall record or placement grant. The caller must
//! supply those later duties before any returned content can reach the model.

use super::*;
use crate::flow::coordinator::{ControlOperation, OperationStart};
use crate::flow::read::{CompletedRead, PageRead, ReadAdvance, ReadFailure};
use weaver_trace::TurnRef;
use weaver_types::state_flow::{DecimalU64, ReadSelector, StateControl, StateFrame};

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

pub(super) struct ReadTransaction {
    pages: PageRead,
    turn: Option<TurnRef>,
    bound: AbsoluteBound,
    expected: Option<(DecimalU64, StateControl)>,
    complete: bool,
    // Preserve the received answer if higher-level validation fails after its
    // operation measurement was recorded. A failure never becomes empty data.
    rejected: Option<OperationCompletion>,
}

impl ReadTransaction {
    pub(super) fn freeze(&mut self) {
        self.pages.freeze();
    }
}

impl RecordingContext {
    /// A busy caller retains its selector and receives no request identity or
    /// new wait. Fresh-history filing barriers remain an explicit caller duty.
    /// This external entry excludes an owned pair, including gaps between its
    /// legs. Spec 05's future inward execution needs an explicit pair-owned read
    /// entry; it must not borrow this unrelated consumer's admission path.
    #[allow(clippy::result_large_err)] // Return owned inputs without allocating on refusal.
    pub(in crate::flow) fn begin_read(
        &mut self,
        selector: ReadSelector,
        turn: Option<TurnRef>,
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
        if self.read.is_some()
            || self.measure_blocks_external()
            || self.member_capture.is_some()
            || self.output.is_some()
            || self.filing.is_some()
            || self.pair_work.is_some()
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
            pages,
            turn: turn.clone(),
            bound,
            expected: None,
            complete: false,
            rejected: None,
        });
        match self.queue_read_leg(io) {
            Ok(true) => Ok(()),
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
        self.read_wait_matches(ports.io)?;
        let advance = self.reduce_flow(ports.io)?;
        if self.read.as_ref().is_some_and(|read| read.complete) {
            return Ok(ReadProgress::Complete);
        }
        if self.queue_read_leg(ports.io)? || advance == Advance::Progress {
            Ok(ReadProgress::Progress)
        } else {
            Ok(ReadProgress::Pending)
        }
    }

    /// Return the one complete, validated result only after its release was
    /// recorded. This is not a HeldMaterial or recall-provenance constructor.
    pub(in crate::flow) fn take_read(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<Option<Result<CompletedRead, ReadFailure>>, RunFailure> {
        self.live(io)?;
        let Some(read) = self.read.as_mut().filter(|read| read.complete) else {
            return Ok(None);
        };
        let result = read.pages.take();
        if result.is_none() {
            return Err(self.read_fault(io, "complete read lost its owned result"));
        }
        self.read = None;
        Ok(result)
    }

    pub(super) fn read_wait_still_owned(&self, key: PumpDeadline) -> bool {
        key == PumpDeadline::StateControl && self.read.as_ref().is_some_and(|read| !read.complete)
    }

    fn read_wait_matches(&mut self, io: &mut impl PumpIo) -> Result<(), RunFailure> {
        let Some(read) = &self.read else {
            return Err(self.read_fault(io, "read advance has no owner"));
        };
        if !read.complete && self.deadlines.get(PumpDeadline::StateControl) != Some(read.bound) {
            return Err(self.read_fault(io, "read's absolute wait was removed or replaced"));
        }
        Ok(())
    }

    fn queue_read_leg(&mut self, io: &mut impl PumpIo) -> Result<bool, RunFailure> {
        self.live(io)?;
        let Some(read) = &self.read else {
            return Ok(false);
        };
        if read.complete || read.expected.is_some() {
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
        if let Err(error) = read.pages.bind_request(request_id, &request) {
            return Err(self.read_fault(io, error.to_string()));
        }
        self.live(io)?;
        Ok(true)
    }

    /// A state answer is consumed only after the coordinator's exact
    /// operation-measurement proof, never on socket read or send success.
    pub(super) fn settle_recorded_read(&mut self, io: &mut impl PumpIo) -> Result<(), RunFailure> {
        let Some(read) = &self.read else {
            return Ok(());
        };
        if read.complete {
            return Ok(());
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
                // Validation and the final release reduction belong to this
                // same wait. Recheck before publishing or disarming it.
                self.live(io)?;
                let read = self.read.as_mut().unwrap();
                read.expected = None;
                if progress == ReadAdvance::Complete {
                    read.complete = true;
                    self.deadlines.disarm(PumpDeadline::StateControl);
                }
                Ok(())
            }
            Err(error) => {
                self.read.as_mut().unwrap().rejected = Some(result);
                Err(self.read_fault(io, error.to_string()))
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
