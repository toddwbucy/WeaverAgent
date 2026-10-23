//! Bounded Shape/Grants controls on the one state channel. These observations
//! retain their exact Recorded completion and grant no content or permission.

use super::*;
use crate::flow::coordinator::{ControlOperation, OperationStart};
use crate::flow::read::ReadFailure;
use crate::state::{RunShape, SessionShape};
use weaver_trace::TurnRef;
use weaver_types::state_flow::{
    ControlRefusalCode, DecimalU64, StateControl, StateControlAnswer, StateFrame,
};

const INSPECTION_NS: u64 = 2_000_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum InspectionKind {
    Shape,
    Grants,
}

impl InspectionKind {
    fn control(self) -> StateControl {
        match self {
            Self::Shape => StateControl::Shape {},
            Self::Grants => StateControl::Grants {},
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum InspectionBeginRefusal {
    WrongPhase,
    Busy,
}

#[derive(Debug)]
pub(in crate::flow) struct InspectionBeginError {
    pub kind: InspectionKind,
    pub turn: Option<TurnRef>,
    pub refusal: InspectionBeginRefusal,
    pub failure: Option<RunFailure>,
    /// True only when this call installed an owner that must be resumed or cancelled.
    pub retained: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum InspectionProgress {
    Progress,
    Pending,
    Complete,
}

/// This private constructor retains the exact measurement-backed state answer.
/// It is not a read proof, permission, HeldMaterial or placement grant.
#[derive(Debug)]
pub(in crate::flow) struct InspectionEvidence {
    request: StateControl,
    answer: StateFrame,
    turn: Option<TurnRef>,
    recorded_ns: u64,
    bound: AbsoluteBound,
}

impl InspectionEvidence {
    pub(in crate::flow) fn request(&self) -> &StateControl {
        &self.request
    }
    pub(in crate::flow) fn answer(&self) -> &StateFrame {
        &self.answer
    }
    pub(in crate::flow) fn turn(&self) -> Option<&TurnRef> {
        self.turn.as_ref()
    }
    pub(in crate::flow) fn recorded_ns(&self) -> u64 {
        self.recorded_ns
    }
    pub(in crate::flow) fn bound(&self) -> AbsoluteBound {
        self.bound
    }
}

#[derive(Debug)]
pub(in crate::flow) enum InspectionCompletion {
    Shape {
        shape: SessionShape,
        evidence: InspectionEvidence,
    },
    Grants {
        surface: Vec<String>,
        evidence: InspectionEvidence,
    },
    Refused {
        failure: ReadFailure,
        evidence: InspectionEvidence,
    },
    Cancelled(InspectionEvidence),
    /// Cancellation before request allocation has no answer or Recorded proof.
    Withdrawn {
        request: StateControl,
        turn: Option<TurnRef>,
        bound: AbsoluteBound,
    },
}

pub(super) struct InspectionTransaction {
    kind: InspectionKind,
    turn: Option<TurnRef>,
    bound: AbsoluteBound,
    request_id: Option<DecimalU64>,
    recorded: bool,
    cancelled: bool,
    completion: Option<InspectionEvidence>,
}

impl InspectionTransaction {
    fn withdrawn(&self) -> bool {
        self.cancelled && self.request_id.is_none()
    }
}

#[cfg(test)]
impl InspectionTransaction {
    pub(super) fn request_id_for_test(&self) -> Option<DecimalU64> {
        self.request_id
    }
}

impl RecordingContext {
    /// Busy admission returns the exact caller inputs and allocates no control.
    /// A lifecycle yield after admission retains the owner for advance/cancel.
    pub(in crate::flow) fn begin_inspection(
        &mut self,
        kind: InspectionKind,
        turn: Option<TurnRef>,
        io: &mut impl PumpIo,
    ) -> Result<(), InspectionBeginError> {
        let error = |refusal, failure| InspectionBeginError {
            kind,
            turn: turn.clone(),
            refusal,
            failure,
            retained: false,
        };
        if let Err(failure) = self.live(io) {
            return Err(error(InspectionBeginRefusal::WrongPhase, Some(failure)));
        }
        if self.phase != RecordingPhase::Entered
            || self.flow.phase() != CoordinatorPhase::Active
            || !self.barrier.is_active()
        {
            return Err(error(InspectionBeginRefusal::WrongPhase, None));
        }
        if self.inspection.is_some()
            || self.marks.is_some()
            || self.boot_pairs.is_some()
            || self.read.is_some()
            || self.boot_open_blocks_external()
            || self.measure.is_some()
            || self.member_capture.is_some()
            || self.freshness.is_some()
            || self.output.is_some()
            || self.filing.is_some()
            || self.pair_work.is_some()
            || self.pair_rejected_authoring
            || self.pairs.pending_rejection().is_some()
            || self.templates_block_content()
            || self.template_blocked_author.is_some()
            || self.input.snapshot().is_some()
            || self.input_author.is_some()
            || self.input_turn.is_some()
            || self.repair.is_some()
            || self.waiting_content.is_some()
            || self.completed_author.is_some()
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
            || self.deadlines.get(PumpDeadline::Gate).is_some()
            || self.deadlines.get(PumpDeadline::Cut).is_some()
            || self.deadlines.get(PumpDeadline::StateControl).is_some()
        {
            return Err(error(InspectionBeginRefusal::Busy, None));
        }
        let start = self.last_ns;
        let prepared = (|| {
            let mut end = start
                .checked_add(INSPECTION_NS)
                .ok_or_else(|| "inspection deadline overflow".to_owned())?;
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
            Ok::<_, String>(bound)
        })();
        let bound = prepared.map_err(|detail| {
            error(
                InspectionBeginRefusal::WrongPhase,
                Some(self.inspection_fault(io, detail)),
            )
        })?;
        self.inspection = Some(InspectionTransaction {
            kind,
            turn: turn.clone(),
            bound,
            request_id: None,
            recorded: false,
            cancelled: false,
            completion: None,
        });
        match self.queue_inspection(io) {
            Ok(true) => Ok(()),
            Ok(false) => {
                self.inspection = None;
                self.deadlines.disarm(PumpDeadline::StateControl);
                Err(error(InspectionBeginRefusal::Busy, None))
            }
            Err(failure) => {
                let mut error = error(InspectionBeginRefusal::WrongPhase, Some(failure));
                error.retained = self.inspection.is_some();
                Err(error)
            }
        }
    }

    pub(in crate::flow) fn advance_inspection<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<InspectionProgress, RunFailure> {
        self.live(ports.io)?;
        self.inspection_wait_matches(ports.io)?;
        if self.inspection.as_ref().unwrap().withdrawn() {
            return Ok(InspectionProgress::Complete);
        }
        let advance = self.reduce_flow(ports.io)?;
        if self.inspection.as_ref().unwrap().completion.is_some() {
            return Ok(InspectionProgress::Complete);
        }
        if self.queue_inspection(ports.io)? || advance == Advance::Progress {
            Ok(InspectionProgress::Progress)
        } else {
            Ok(InspectionProgress::Pending)
        }
    }

    /// Cancellation before allocation withdraws the original proposal. An allocated
    /// exchange still drains its measurement. Neither path starts another window.
    pub(in crate::flow) fn cancel_inspection(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        self.live(io)?;
        self.inspection_wait_matches(io)?;
        let owner = self.inspection.as_mut().unwrap();
        owner.cancelled = true;
        if owner.request_id.is_none() {
            self.deadlines.disarm(PumpDeadline::StateControl);
        }
        Ok(())
    }

    pub(in crate::flow) fn take_inspection(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<Option<InspectionCompletion>, RunFailure> {
        self.live(io)?;
        let Some(owner) = &self.inspection else {
            return Ok(None);
        };
        if owner.completion.is_none() && !owner.withdrawn() {
            return Ok(None);
        }
        self.inspection_wait_matches(io)?;
        let owner = self.inspection.take().unwrap();
        if owner.withdrawn() {
            return Ok(Some(InspectionCompletion::Withdrawn {
                request: owner.kind.control(),
                turn: owner.turn,
                bound: owner.bound,
            }));
        }
        let evidence = owner.completion.unwrap();
        // No fallible clock or yield follows this exact ownership transfer.
        if owner.cancelled {
            return Ok(Some(InspectionCompletion::Cancelled(evidence)));
        }
        let StateFrame::ControlAnswer { frame, .. } = &evidence.answer else {
            unreachable!("settlement checked the exact control answer");
        };
        let completion = match frame {
            StateControlAnswer::Shape(rows) => {
                let mut runs: Vec<RunShape> = Vec::new();
                let mut indices = std::collections::HashMap::new();
                for row in rows {
                    let index = *indices.entry(row.run.as_str()).or_insert_with(|| {
                        runs.push(RunShape {
                            run: row.run.clone(),
                            kinds: Vec::new(),
                        });
                        runs.len() - 1
                    });
                    runs[index].kinds.push((row.kind.clone(), row.count.0));
                }
                InspectionCompletion::Shape {
                    shape: SessionShape { runs },
                    evidence,
                }
            }
            StateControlAnswer::Grants { surface } => InspectionCompletion::Grants {
                surface: surface.clone(),
                evidence,
            },
            StateControlAnswer::ControlRefused {
                code: ControlRefusalCode::Read(code),
                detail,
            } => InspectionCompletion::Refused {
                failure: ReadFailure::Refused {
                    code: *code,
                    detail: detail.clone(),
                },
                evidence,
            },
            _ => unreachable!("settlement checked inspection response vocabulary"),
        };
        Ok(Some(completion))
    }

    pub(super) fn inspection_wait_still_owned(&self, key: PumpDeadline) -> bool {
        key == PumpDeadline::StateControl && self.inspection.is_some()
    }

    fn inspection_wait_matches(&mut self, io: &mut impl PumpIo) -> Result<(), RunFailure> {
        let Some(owner) = &self.inspection else {
            return Err(self.inspection_fault(io, "inspection has no owner"));
        };
        let expected = (!owner.recorded && !owner.withdrawn()).then_some(owner.bound);
        if self.deadlines.get(PumpDeadline::StateControl) != expected {
            return Err(
                self.inspection_fault(io, "inspection's original wait was removed or replaced")
            );
        }
        Ok(())
    }

    fn queue_inspection(&mut self, io: &mut impl PumpIo) -> Result<bool, RunFailure> {
        self.live(io)?;
        self.inspection_wait_matches(io)?;
        let owner = self.inspection.as_ref().unwrap();
        if owner.request_id.is_some() || owner.withdrawn() {
            return Ok(false);
        }
        let operation = ControlOperation::try_from((owner.kind.control(), owner.turn.clone()))
            .expect("inspection uses a small control");
        let result = self
            .flow
            .begin_control(
                &mut FlowRecording {
                    barrier: &mut self.barrier,
                    inbox: &mut self.inbox,
                    budget: &mut self.budget,
                    author: &self.author,
                },
                io,
                operation,
            )
            .map_err(|failure| self.flow_failed(io, failure))?;
        let OperationStart::Started { request_id } = result else {
            return Ok(false);
        };
        // Preserve the allocated identity before a post-enqueue lifecycle yield.
        self.inspection.as_mut().unwrap().request_id = Some(request_id);
        self.live(io)?;
        Ok(true)
    }

    /// This passive check retires only the exact Recorded control's interval.
    /// It neither consumes the completion nor retries a channel syscall.
    pub(super) fn retire_recorded_inspection_wait(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        let Some(owner) = &self.inspection else {
            return Ok(());
        };
        if owner.recorded || owner.withdrawn() {
            return Ok(());
        }
        let Some((request, turn, request_id, recorded_ns)) = self.flow.control_recorded_boundary()
        else {
            return Ok(());
        };
        if !inspection_identity_matches(
            request,
            turn,
            request_id,
            &owner.kind.control(),
            &owner.turn,
            owner.request_id,
        ) || self.deadlines.get(PumpDeadline::StateControl) != Some(owner.bound)
        {
            return Err(
                self.inspection_fault(io, "Recorded inspection differs from its exact owner")
            );
        }
        if owner.bound.expired(recorded_ns) {
            // A capped control cannot replace its earlier enclosing lifecycle
            // wait. Preserve the same nearest-deadline priority as live().
            let (key, bound) = self
                .deadlines
                .expired(recorded_ns)
                .min_by_key(|(_, bound)| bound.deadline_ns())
                .expect("the original inspection wait is armed and expired");
            return Err(self.expired(io, key, bound));
        }
        self.deadlines.disarm(PumpDeadline::StateControl);
        self.inspection.as_mut().unwrap().recorded = true;
        Ok(())
    }

    pub(super) fn settle_recorded_inspection(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        self.retire_recorded_inspection_wait(io)?;
        let Some(owner) = &self.inspection else {
            return Ok(());
        };
        if owner.completion.is_some() {
            return Ok(());
        }
        let Some(completion) = self.flow.operation_completion() else {
            return Ok(());
        };
        let OperationCompletion::State {
            request,
            turn,
            completion,
            ..
        } = completion
        else {
            return Err(self.inspection_fault(io, "inspection received a foreign completion"));
        };
        let valid = inspection_answer_matches(
            owner.kind,
            &completion.answer,
            completion.discard_capture_result,
        );
        if !owner.recorded
            || !valid
            || !inspection_identity_matches(
                request,
                turn,
                completion.answer.common().request_id,
                &owner.kind.control(),
                &owner.turn,
                owner.request_id,
            )
        {
            return Err(self.inspection_fault(io, "inspection completion differs from its request"));
        }
        let completion = self
            .flow
            .take_operation_completion(&mut FlowRecording {
                barrier: &mut self.barrier,
                inbox: &mut self.inbox,
                budget: &mut self.budget,
                author: &self.author,
            })
            .map_err(|failure| self.flow_failed(io, failure))?;
        let Some(OperationCompletion::State {
            request,
            turn,
            completion,
            recorded_ns,
        }) = completion
        else {
            return Ok(());
        };
        let owner = self.inspection.as_mut().unwrap();
        owner.completion = Some(InspectionEvidence {
            request,
            answer: completion.answer,
            turn,
            recorded_ns,
            bound: owner.bound,
        });
        Ok(())
    }

    fn inspection_fault(&mut self, io: &mut impl PumpIo, detail: impl Into<String>) -> RunFailure {
        self.fail(
            io,
            FlowHaltReason::ProtocolFailed,
            HaltPoint::StateReply,
            detail,
        )
    }
}

// These predicates inspect values only. They cannot create a completion proof.
fn inspection_identity_matches(
    request: &StateControl,
    turn: &Option<TurnRef>,
    request_id: DecimalU64,
    expected_request: &StateControl,
    expected_turn: &Option<TurnRef>,
    expected_id: Option<DecimalU64>,
) -> bool {
    request == expected_request && turn == expected_turn && Some(request_id) == expected_id
}

fn inspection_answer_matches(kind: InspectionKind, answer: &StateFrame, discarded: bool) -> bool {
    !discarded
        && matches!(
            (kind, answer),
            (
                InspectionKind::Shape,
                StateFrame::ControlAnswer {
                    frame: StateControlAnswer::Shape(_),
                    ..
                }
            ) | (
                InspectionKind::Grants,
                StateFrame::ControlAnswer {
                    frame: StateControlAnswer::Grants { .. },
                    ..
                }
            ) | (
                _,
                StateFrame::ControlAnswer {
                    frame: StateControlAnswer::ControlRefused {
                        code: ControlRefusalCode::Read(_),
                        ..
                    },
                    ..
                }
            )
        )
}

#[cfg(test)]
#[path = "context_inspection_validation_tests.rs"]
pub(super) mod validation_tests;
