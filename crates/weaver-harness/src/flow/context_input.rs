//! The frame transaction uses the run's recorder, classifier and absolute clock.
//! It neither opens the gate nor elects a model pass. The engine consumes the
//! private accepted capability through the source-authoring handoff below.

use super::*;
use crate::flow::author::AuthoredSource;
use crate::flow::coordinator::{CheckOperation, OperationStart};
use crate::flow::input::{AcceptedInput, ConsumedInput, InputState};
use weaver_trace::{Subsystem, TurnRef};
use weaver_traits::Message;
use weaver_types::TurnFrame;
use weaver_types::state_flow::{DecimalU64, SourceBlockId};

#[derive(Debug)]
pub(in crate::flow) struct InputBeginError {
    pub frame: TurnFrame,
    pub detail: String,
    pub failure: Option<RunFailure>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum InputProgress {
    Idle,
    Progress,
    Pending,
    Accepted,
    CloseReady,
    Finished,
}

#[derive(Debug)]
pub(in crate::flow) struct InputConsumeError {
    /// A pre-consumption failure returns the original token. Once source
    /// authoring begins, the context retains the handoff across interruption.
    pub token: Option<AcceptedInput>,
    pub failure: RunFailure,
}

#[derive(Debug)]
pub(in crate::flow) struct SeatedInput {
    pub message: Message,
    pub source: SourceBlockId,
    pub input_id: DecimalU64,
    pub offered_sequence: DecimalU64,
    pub turn: TurnRef,
}

#[derive(Debug, Clone, Copy)]
pub(in crate::flow) enum InputAuthorStage {
    Offered,
    Refusal,
}

pub(in crate::flow) struct InputAuthor {
    stage: InputAuthorStage,
    returned: StoppedAuthor,
}

pub(in crate::flow) struct InputTurn {
    input: ConsumedInput,
    turn: TurnRef,
    turn_recorded: bool,
    author_started: bool,
    returned_author: StoppedAuthor,
    reuse_installed: bool,
}

impl RecordingContext {
    /// The serialized frame owner calls this only when the previous turn has
    /// released service. A busy return keeps the frame outside the check and
    /// starts neither an input ID nor a gate deadline.
    pub(in crate::flow) fn begin_input(
        &mut self,
        frame: TurnFrame,
        io: &mut impl PumpIo,
    ) -> Result<(), InputBeginError> {
        if let Err(failure) = self.live(io) {
            return Err(InputBeginError {
                frame,
                detail: "recording context failed".into(),
                failure: Some(failure),
            });
        }
        if self.marks.is_some()
            || self.boot_pairs.is_some()
            || self.output.is_some()
            || self.freshness.is_some()
            || self.filing.is_some()
            || self.pair_work.is_some()
            || self.input.snapshot().is_some()
            || self.templates_block_content()
            || self.member_capture.is_some()
            || self.read.is_some()
            || self.inspection.is_some()
            || self.boot_open_blocks_external()
            || self.plan_summary_blocks_external()
            || self.place_blocks_external()
            || self.measure_blocks_external()
            || self.open_turn.is_some()
            || self.input_turn.is_some()
            || self.input_author.is_some()
            || (self.barrier.pending().is_some() && !self.flow.has_pending_record())
            || self.caller_author_held()
            || self.waiting_content.is_some()
            || self.completed_author.is_some()
        {
            return Err(InputBeginError {
                frame,
                detail: "input intake is busy".into(),
                failure: None,
            });
        }
        if self.phase != RecordingPhase::Entered || self.flow.phase() != CoordinatorPhase::Active {
            let failure = self.input_fault(io, "input requires an active serving flow");
            return Err(InputBeginError {
                frame,
                detail: "input before flow readiness".into(),
                failure: Some(failure),
            });
        }
        let bound =
            match AbsoluteBound::after(self.last_ns, self.declaration.gate_wait.elapsed_ns.0) {
                Ok(bound) => bound,
                Err(error) => {
                    let failure = self.input_fault(io, error.to_string());
                    return Err(InputBeginError {
                        frame,
                        detail: error.to_string(),
                        failure: Some(failure),
                    });
                }
            };
        if let Err(error) = self.input.begin(frame, bound) {
            let detail = error.error.to_string();
            let failure = self.input_fault(io, detail.clone());
            return Err(InputBeginError {
                frame: error.frame,
                detail,
                failure: Some(failure),
            });
        }
        let id = self
            .input
            .snapshot()
            .expect("intake retained its frame")
            .input_id;
        // After ownership transfers the frame remains in InputSlot, even if
        // clock registration or the accounting operation fails.
        let result = self.flow.input_started(
            &mut FlowRecording {
                barrier: &mut self.barrier,
                inbox: &mut self.inbox,
                budget: &mut self.budget,
                author: &self.author,
            },
            io,
            id,
            bound.began_ns(),
        );
        if let Err(error) = result {
            let failure = self.flow_failed(io, error);
            return Err(InputBeginError {
                frame: self.input_frame(),
                detail: "input accounting failed".into(),
                failure: Some(failure),
            });
        }
        if let Err(failure) = self.arm(PumpDeadline::Gate, bound, io) {
            return Err(InputBeginError {
                frame: self.input_frame(),
                detail: "gate registration failed".into(),
                failure: Some(failure),
            });
        }
        Ok(())
    }

    /// No implicit classifier wait. The owner calls the common pump when this
    /// returns Pending. Trace authors use the sole existing author barrier.
    pub(in crate::flow) fn advance_input<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<InputProgress, RunFailure> {
        self.live(ports.io)?;
        if self.input.snapshot().is_none() {
            return Ok(InputProgress::Idle);
        }
        if self.input_turn.is_some() {
            return Ok(InputProgress::Pending);
        }
        if let Some(author) = &self.input_author {
            author.returned.check()?;
            let stage = author.stage;
            let sequence = self
                .resume_author_and_wait(ports)
                .map_err(|failure| self.input_author.as_mut().unwrap().returned.retain(failure))?;
            self.finish_input_author_stage(stage, sequence, ports.io)?;
            return Ok(InputProgress::Progress);
        }
        // A prior measurement may own the one author when this eligible frame
        // arrives. Its wait counts inside the input's already armed bound.
        if self.flow.has_pending_record() {
            return self.reduce_flow(ports.io).map(|progress| match progress {
                Advance::Progress => InputProgress::Progress,
                _ => InputProgress::Pending,
            });
        }
        if let Some(event) = self.input.offered_event() {
            self.input_author = Some(InputAuthor {
                stage: InputAuthorStage::Offered,
                returned: StoppedAuthor::default(),
            });
            let sequence = self
                .author_and_wait(ports, event)
                .map_err(|failure| self.input_author.as_mut().unwrap().returned.retain(failure))?;
            self.finish_input_author_stage(InputAuthorStage::Offered, sequence, ports.io)?;
            return Ok(InputProgress::Progress);
        }
        if self.settle_recorded_input(ports.io)? {
            return Ok(InputProgress::Progress);
        }
        if self.serving_organs.no_residency() {
            self.input.refuse_no_residency();
        }
        if let Some(refusal) = self.input.refusal() {
            let causal_parent = matches!(refusal, crate::flow::input::Refusal::NoResidency)
                .then(|| Sequence(self.input.snapshot().unwrap().offered_sequence.unwrap().0));
            let payload = serde_json::value::to_raw_value(&refusal.record())
                .map_err(|error| self.input_fault(ports.io, error.to_string()))?;
            let event = AuthoredEvent {
                kind: Kind::Refusal,
                subsystem: Subsystem::Harness,
                turn: None,
                source: AuthoredSource::Runtime,
                causal_parent,
                payload: Some(Payload::Refusal(payload)),
            };
            self.input_author = Some(InputAuthor {
                stage: InputAuthorStage::Refusal,
                returned: StoppedAuthor::default(),
            });
            let sequence = self
                .author_and_wait(ports, event)
                .map_err(|failure| self.input_author.as_mut().unwrap().returned.retain(failure))?;
            self.finish_input_author_stage(InputAuthorStage::Refusal, sequence, ports.io)?;
            return Ok(InputProgress::Progress);
        }
        if let Some(operation) = self.input.check_operation() {
            let started = self
                .flow
                .begin_input_check(
                    &mut FlowRecording {
                        barrier: &mut self.barrier,
                        inbox: &mut self.inbox,
                        budget: &mut self.budget,
                        author: &self.author,
                    },
                    ports.io,
                    operation,
                )
                .map_err(|error| self.flow_failed(ports.io, error))?;
            if let OperationStart::Started { request_id } = started {
                self.input
                    .submitted(request_id)
                    .map_err(|error| self.input_fault(ports.io, error.to_string()))?;
                return Ok(InputProgress::Progress);
            }
        }
        self.finish_input_wait(ports.io, None)?;
        if self.flow.input_measurements_pending() {
            return self.reduce_flow(ports.io).map(|progress| match progress {
                Advance::Progress => InputProgress::Progress,
                _ => InputProgress::Pending,
            });
        }
        if self.input.close_frame().is_some() {
            return Ok(InputProgress::CloseReady);
        }
        if self.input.gate_may_disarm() {
            return Ok(
                if self
                    .input
                    .snapshot()
                    .is_some_and(|input| input.state == InputState::Accepted)
                {
                    InputProgress::Accepted
                } else {
                    InputProgress::Finished
                },
            );
        }
        self.reduce_flow(ports.io).map(|progress| match progress {
            Advance::Progress => InputProgress::Progress,
            _ => InputProgress::Pending,
        })
    }

    /// Called in the same reduction that records the result, before control
    /// returns to a caller which might wait or perform unrelated work. That
    /// prevents an accepted result's already-finished gate wait from expiring
    /// merely because its token has not yet been taken by the engine.
    pub(super) fn settle_recorded_input(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<bool, RunFailure> {
        if !matches!(
            self.flow.operation_completion(),
            Some(OperationCompletion::Input { .. })
        ) {
            // A completed trace admission can precede release when the final
            // terminal-status probe is interrupted. Preserve that boundary
            // without minting an input capability or retiring the exchange.
            if let Some((CheckOperation::Input { input_id, .. }, reply, recorded_ns)) =
                self.flow.pending_classifier_recorded()
            {
                let Some(input) = self.input.snapshot() else {
                    return Err(self.input_fault(io, "pending input proof has no owner"));
                };
                if input.input_id != *input_id {
                    return Err(self.input_fault(io, "pending input proof has another owner"));
                }
                if input.bound.expired(recorded_ns) {
                    return Err(self.expired(io, PumpDeadline::Gate, input.bound));
                }
                // Rejection still owes its refusal record. A cancelled check
                // ends at its result record regardless of the verdict.
                if input.cancelled
                    || matches!(
                        reply.result,
                        weaver_types::state_flow::Ste100Result::Accepted { .. }
                    )
                {
                    self.deadlines.disarm(PumpDeadline::Gate);
                }
            }
            return Ok(false);
        }
        let recorded_ns = match self.flow.operation_completion() {
            Some(OperationCompletion::Input { recorded_ns, .. }) => *recorded_ns,
            _ => return Ok(false),
        };
        let bound = self
            .input
            .snapshot()
            .ok_or_else(|| self.input_fault(io, "input completion has no owner"))?
            .bound;
        if recorded_ns >= bound.deadline_ns() {
            return Err(self.expired(io, PumpDeadline::Gate, bound));
        }
        // Preserve component/terminal checks while avoiding a new check of the
        // already-completed gate interval. recorded_ns is its boundary sample.
        let completion = self
            .flow
            .take_operation_completion(&mut FlowRecording {
                barrier: &mut self.barrier,
                inbox: &mut self.inbox,
                budget: &mut self.budget,
                author: &self.author,
            })
            .map_err(|error| self.flow_failed(io, error))?;
        let Some(OperationCompletion::Input {
            operation,
            reply,
            reuse,
            ..
        }) = completion
        else {
            return Ok(false);
        };
        self.input
            .accept_completion(operation, reply, reuse)
            .map_err(|error| self.input_fault(io, error.error.to_string()))?;
        self.finish_input_wait(io, Some(recorded_ns))?;
        Ok(true)
    }

    fn finish_input_author_stage(
        &mut self,
        stage: InputAuthorStage,
        sequence: Sequence,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        let result = match stage {
            InputAuthorStage::Offered => self
                .input
                .offered_recorded(sequence, self.serving_organs.no_residency()),
            InputAuthorStage::Refusal => self.input.refusal_recorded(sequence),
        };
        result.map_err(|error| self.input_fault(io, error.to_string()))?;
        self.input_author = None;
        self.finish_input_wait(io, None)?;
        if matches!(stage, InputAuthorStage::Refusal)
            && self
                .input
                .snapshot()
                .is_some_and(|input| input.state == InputState::Refused)
            && self.deadlines.get(PumpDeadline::Decision).is_none()
        {
            // The required world close now blocks reuse of the serving seat.
            // Its one send interval also covers the intervening input-wait
            // measurement records; trace pressure cannot leave that gap idle.
            let elapsed = self
                .load_duty
                .as_ref()
                .unwrap()
                .instruction
                .decision_wait
                .elapsed_ns
                .0;
            let bound = AbsoluteBound::after(self.last_ns, elapsed)
                .map_err(|error| self.input_fault(io, error.to_string()))?;
            self.arm(PumpDeadline::Decision, bound, io)?;
        }
        Ok(())
    }

    fn finish_input_wait(
        &mut self,
        io: &mut impl PumpIo,
        completed_ns: Option<u64>,
    ) -> Result<(), RunFailure> {
        if self.input.gate_may_disarm()
            && (self.deadlines.get(PumpDeadline::Gate).is_some() || completed_ns.is_some())
        {
            let id = self.input.snapshot().expect("active disposition").input_id;
            self.flow
                .input_finished_at(
                    &mut FlowRecording {
                        barrier: &mut self.barrier,
                        inbox: &mut self.inbox,
                        budget: &mut self.budget,
                        author: &self.author,
                    },
                    io,
                    id,
                    completed_ns.unwrap_or(self.last_ns),
                )
                .map_err(|error| self.flow_failed(io, error))?;
            self.complete_wait(PumpDeadline::Gate);
        }
        Ok(())
    }

    pub(in crate::flow) fn take_accepted_input(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<Option<AcceptedInput>, RunFailure> {
        self.live(io)?;
        if self.input_author.is_some() || self.flow.input_measurements_pending() {
            return Ok(None);
        }
        Ok(self.input.take_accepted())
    }

    pub(in crate::flow) fn cancel_input(&mut self, io: &mut impl PumpIo) -> Result<(), RunFailure> {
        self.live(io)?;
        if self.boot_pairs_blocks_external() {
            return Err(RunFailure::BootPairsOwned);
        }
        if self.marks_blocks_external() {
            return Err(RunFailure::MarksOwned);
        }
        self.input
            .cancel()
            .map_err(|error| self.input_fault(io, error.to_string()))?;
        self.settle_recorded_input(io)?;
        self.finish_input_wait(io, None)
    }

    /// The existing gate exchange owner carries this exact unnamed response.
    /// It acknowledges the successful carry through input_response_sent.
    pub(in crate::flow) fn input_close(&self) -> Option<TurnFrame> {
        if self.input_author.is_some() && self.failure.is_none() {
            return None;
        }
        self.input.close_frame()
    }

    pub(in crate::flow) fn input_response_sent(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        if self.failure.is_some() {
            // Terminal reporting may carry the stopped close after the latch.
            // Account that carry once without reopening or disarming anything.
            return self
                .input
                .response_sent()
                .map_err(|error| self.input_fault(io, error.to_string()));
        }
        self.live(io)?;
        self.input
            .response_sent()
            .map_err(|error| self.input_fault(io, error.to_string()))?;
        self.finish_input_wait(io, None)?;
        Ok(())
    }

    /// The checked rollback gate has closed this run's world connections.
    /// Preserve the original check until its Recorded result; a Lower is no
    /// substitute response receipt and does not renew the input deadline.
    pub(in crate::flow::context) fn input_closed_by_lower(
        &mut self,
        proof: &super::worker_startup::expired::gate::RollbackGateClosed,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        self.live(io)?;
        if !proof.allows(self) {
            return Err(self.input_fault(io, "input Lower proof belongs to another rollback"));
        }
        self.input
            .cancellation_lowered()
            .map_err(|error| self.input_fault(io, error.to_string()))?;
        self.finish_input_wait(io, None)
    }

    pub(in crate::flow::context) fn input_closed_by_gate_loss(
        &mut self,
        gate: &crate::flow::pump_transport::OrganPort,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        self.live(io)?;
        if !self.serving_organs.gate_delivery_lost(gate) {
            return Err(self.input_fault(io, "input gate loss lacks its original observed port"));
        }
        self.input
            .response_lost()
            .map_err(|error| self.input_fault(io, error.to_string()))?;
        self.finish_input_wait(io, None)
    }

    /// Releasing a refused/cancelled transaction never releases an unfinished
    /// check. The original exchange stays owned until its recorded disposition.
    pub(in crate::flow) fn finish_input(&mut self, io: &mut impl PumpIo) -> Result<(), RunFailure> {
        self.live(io)?;
        if self.boot_pairs_blocks_external() {
            return Err(RunFailure::BootPairsOwned);
        }
        if self.marks_blocks_external() {
            return Err(RunFailure::MarksOwned);
        }
        self.finish_input_wait(io, None)?;
        if self.input_author.is_some()
            || self.input_turn.is_some()
            || self.flow.input_measurements_pending()
        {
            return Err(self.input_fault(io, "input records or source handoff are still pending"));
        }
        let closing_input = self
            .input
            .snapshot()
            .is_some_and(|input| input.state != InputState::Consumed);
        self.input
            .finish()
            .map_err(|error| self.input_fault(io, error.to_string()))?;
        if closing_input {
            self.complete_wait(PumpDeadline::Decision);
        }
        Ok(())
    }

    pub(in crate::flow) fn consume_input<I: PumpIo>(
        &mut self,
        token: AcceptedInput,
        turn: TurnRef,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<Option<SeatedInput>, InputConsumeError> {
        if let Err(failure) = self.live(ports.io) {
            return Err(InputConsumeError {
                token: Some(token),
                failure,
            });
        }
        if self.boot_pairs_blocks_external() {
            return Err(InputConsumeError {
                token: Some(token),
                failure: RunFailure::BootPairsOwned,
            });
        }
        if self.marks_blocks_external() {
            return Err(InputConsumeError {
                token: Some(token),
                failure: RunFailure::MarksOwned,
            });
        }
        if turn.0.is_empty() || self.input_turn.is_some() || self.input_turns.contains(&turn.0) {
            let failure = self.input_fault(ports.io, "input requires one fresh external turn");
            return Err(InputConsumeError {
                token: Some(token),
                failure,
            });
        }
        let run = self.flow.run().to_owned();
        let input = match self
            .input
            .consume_input(token, &run, &self.declaration.scheme.scheme_id)
        {
            Ok(input) => input,
            Err(error) => {
                let failure = self.input_fault(ports.io, error.error.to_string());
                return Err(InputConsumeError {
                    token: Some(error.token),
                    failure,
                });
            }
        };
        self.input_turns.insert(turn.0.clone());
        self.input_turn = Some(InputTurn {
            input,
            turn,
            turn_recorded: false,
            author_started: false,
            returned_author: StoppedAuthor::default(),
            reuse_installed: false,
        });
        self.resume_input_turn(ports)
            .map_err(|failure| InputConsumeError {
                token: None,
                failure,
            })
    }

    /// Engine-side handoff. No Message escapes until both trace events record.
    /// A lifecycle interruption resumes the same event and cannot reauthor it.
    pub(in crate::flow) fn resume_input_turn<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<Option<SeatedInput>, RunFailure> {
        self.live(ports.io)?;
        if self.boot_pairs_blocks_external() {
            return Err(RunFailure::BootPairsOwned);
        }
        if self.marks_blocks_external() {
            return Err(RunFailure::MarksOwned);
        }
        if let Some(work) = &self.input_turn {
            work.returned_author.check()?;
        }
        let Some(mut work) = self.input_turn.take() else {
            return Err(self.input_fault(ports.io, "input turn has no retained handoff"));
        };
        if !work.turn_recorded {
            let result = if work.author_started {
                self.resume_author_and_wait(ports)
            } else {
                work.author_started = true;
                self.author_and_wait(
                    ports,
                    AuthoredEvent {
                        kind: Kind::TurnStarted,
                        subsystem: Subsystem::Harness,
                        turn: Some(work.turn.clone()),
                        source: AuthoredSource::Runtime,
                        causal_parent: None,
                        payload: None,
                    },
                )
            };
            match result {
                Ok(_) => {
                    work.turn_recorded = true;
                    work.author_started = false;
                }
                Err(error) => {
                    let reason = work
                        .returned_author
                        .retain_started(error, &mut work.author_started);
                    self.input_turn = Some(work);
                    return Err(reason);
                }
            }
        }
        if !work.reuse_installed {
            // The input slot owns the one grant until this exact author begins.
            let reuse = work.input.reuse.take().expect("uninstalled input reuse");
            if let Err(reuse) = self.flow.reuse_next_input(reuse) {
                work.input.reuse = Some(reuse);
                self.input_turn = Some(work);
                return Err(self.input_fault(ports.io, "input reuse owner is unavailable"));
            }
            work.reuse_installed = true;
        }
        let result = if work.author_started {
            self.resume_author_and_wait(ports)
        } else {
            let payload = match serde_json::value::to_raw_value(&work.input.message) {
                Ok(payload) => payload,
                Err(error) => {
                    self.input_turn = Some(work);
                    return Err(self.input_fault(ports.io, error.to_string()));
                }
            };
            work.author_started = true;
            self.author_and_wait(
                ports,
                AuthoredEvent {
                    kind: Kind::MessageUser,
                    subsystem: Subsystem::Harness,
                    turn: Some(work.turn.clone()),
                    source: AuthoredSource::BoundaryInput(work.input.dialer),
                    causal_parent: Some(Sequence(work.input.offered_sequence.0)),
                    payload: Some(Payload::Message(payload)),
                },
            )
        };
        let sequence = match result {
            Ok(sequence) => sequence,
            Err(error) => {
                let reason = work
                    .returned_author
                    .retain_started(error, &mut work.author_started);
                self.input_turn = Some(work);
                return Err(reason);
            }
        };
        let finish = self.flow.finish_input_author(
            &mut FlowRecording {
                barrier: &mut self.barrier,
                inbox: &mut self.inbox,
                budget: &mut self.budget,
                author: &self.author,
            },
            sequence,
        );
        if let Err(error) = finish {
            self.input_turn = Some(work);
            return Err(self.flow_failed(ports.io, error));
        }
        if let Err(error) = self.input.finish() {
            self.input_turn = Some(work);
            return Err(self.input_fault(ports.io, error.to_string()));
        }
        Ok(Some(SeatedInput {
            message: work.input.message,
            source: SourceBlockId {
                run: self.flow.run().to_owned(),
                sequence: sequence.0.into(),
                block: 0.into(),
            },
            input_id: work.input.input_id,
            offered_sequence: work.input.offered_sequence,
            turn: work.turn,
        }))
    }

    fn input_fault(&mut self, io: &mut impl PumpIo, detail: impl Into<String>) -> RunFailure {
        self.fail(
            io,
            FlowHaltReason::ProtocolFailed,
            HaltPoint::Barrier,
            detail,
        )
    }

    fn input_frame(&self) -> TurnFrame {
        self.input.frame().expect("owned intake frame").clone()
    }
}

#[cfg(test)]
#[path = "context_input_author_failure_tests.rs"]
mod author_failure_tests;
