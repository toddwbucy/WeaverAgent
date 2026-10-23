//! Explicit begin_task retains accepted input, catalog and both exact receipts.
//! The current Decision bound covers the complete transaction. No mark clock,
//! direct store write, placement grant or automatic input-triggered task exists.

use super::*;
use crate::flow::author::AuthoredSource;
use crate::flow::input::AcceptedInput;
use crate::flow::ledger::{LedgerFile, ReceiptSet, SourceDisposition};
use crate::flow::marks::{CatalogError, TaskCatalog};
use crate::flow::read::ReadFailure;
use crate::flow::templates::{
    ExpansionIdentity, TemplateCertificate, TemplateExpansion, expand_begin_task,
};
use receipt_driver::FilingOwner;
use weaver_trace::serving::payloads as trace;
use weaver_trace::{Subsystem, TurnRef};
use weaver_traits::{ContentBlock, Message};
use weaver_types::state_flow::{
    ContentRef, MarkEffect, SourceBlockId, StatementEffect, StatementFile, TaskScopeRef,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum BeginTaskRefusal {
    WrongPhase,
    Busy,
    MissingWait,
    InvalidTemplate,
}

#[derive(Debug)]
pub(in crate::flow) struct BeginTaskError {
    pub input: AcceptedInput,
    pub refusal: BeginTaskRefusal,
    pub failure: Option<RunFailure>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum TaskProgress {
    Pending,
    Complete,
}

/// Exact filing evidence stays with the source introduced by this operation.
/// It is not constructible from a copied SeatedInput or a source ID.
#[derive(Debug)]
pub(in crate::flow) struct FiledTaskInput {
    seated: SeatedInput,
    receipt: ReceiptSet,
}
impl FiledTaskInput {
    pub(in crate::flow) fn message(&self) -> &Message {
        &self.seated.message
    }
    pub(in crate::flow) fn source(&self) -> &SourceBlockId {
        &self.seated.source
    }
}

#[derive(Debug)]
pub(in crate::flow) struct BegunTask {
    scope: TaskScopeRef,
    input: FiledTaskInput,
    catalog: RecordedRead,
    mark: Option<FilingCompletion>,
}
impl BegunTask {
    pub(in crate::flow) fn origin(&self) -> &std::sync::Arc<()> {
        self.catalog.origin()
    }
    pub(in crate::flow) fn scope(&self) -> &TaskScopeRef {
        &self.scope
    }
    pub(in crate::flow) fn input(&self) -> &FiledTaskInput {
        &self.input
    }
    pub(in crate::flow) fn belongs_to(&self, context: &RecordingContext) -> bool {
        self.catalog.belongs_to(context)
    }
    pub(in crate::flow) fn mark(&self) -> Option<&FilingCompletion> {
        self.mark.as_ref()
    }
}

#[derive(Debug)]
pub(in crate::flow) enum TaskRefusal {
    Read(ReadFailure),
    CounterExhausted,
    TemplateRejected { fault: Sequence },
}

/// Refusal/cancellation retains evidence without an accessor that grants a task.
/// Only an unconsumed AcceptedInput can be returned. A consumed input's seated
/// Message and receipts remain private in this account, including after settlement.
pub(in crate::flow) struct TaskAccount {
    work: MarksTransaction,
}
impl std::fmt::Debug for TaskAccount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TaskAccount")
            .field("cancelled", &self.work.cancelled)
            .finish_non_exhaustive()
    }
}
impl TaskAccount {
    pub(in crate::flow) fn take_unconsumed_input(&mut self) -> Option<AcceptedInput> {
        self.work.token.take()
    }
}

#[derive(Debug)]
pub(in crate::flow) enum TaskCompletion {
    Begun(BegunTask),
    Refused {
        refusal: TaskRefusal,
        account: TaskAccount,
    },
    Cancelled(TaskAccount),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Input,
    Read,
    Mark,
    Fault,
    Complete,
}

pub(super) struct MarksTransaction {
    // Only the synchronous private driver enters dependent input/read helpers.
    driving: bool,
    bound: AbsoluteBound,
    settled: bool,
    turn: TurnRef,
    token: Option<AcceptedInput>,
    input_sequence: Option<Sequence>,
    seated: Option<SeatedInput>,
    input_receipt: Option<ReceiptSet>,
    read: Option<ReadCompletion>,
    expansion: TemplateExpansion,
    scope: Option<TaskScopeRef>,
    effect: Option<MarkEffect>,
    filing: Option<FilingCompletion>,
    stage: Stage,
    fault_started: bool,
    fault_sequence: Option<Sequence>,
    refusal: Option<TaskRefusal>,
    cancelled: bool,
    // Custody of a withdrawn, never-recorded mark belongs to the cancelled
    // account. Retaining this evidence does not expose a task or replay port.
    cancelled_event: Option<AuthoredEvent>,
}

impl MarksTransaction {
    pub(super) fn has_original_wait(&self, current: Option<AbsoluteBound>) -> bool {
        current == Some(self.bound)
    }
}

// These predicates inspect association values. They cannot construct receipts
// or publish an input/task grant, including when exercised on copies by tests.
fn input_receipt_matches(
    sequence: Option<Sequence>,
    seated: &SeatedInput,
    turn: &TurnRef,
    source: &SourceBlockId,
    file: &StatementFile,
) -> bool {
    source == &seated.source
        && file.source.kind == "message.user"
        && file.source.turn.as_deref() == Some(turn.0.as_str())
        && file.source.causal_parent == Some(seated.offered_sequence)
        && matches!(seated.message.content.as_slice(), [ContentBlock::Text { text }] if text == &file.text)
        && sequence == Some(Sequence(seated.source.sequence.0))
}

fn mark_source_matches(source: &SourceBlockId, run: &str, sequence: Sequence) -> bool {
    source.run == run && source.sequence.0 == sequence.0 && source.block.0 == 0
}

fn mark_content_matches(
    file: &StatementFile,
    turn: &TurnRef,
    text: &str,
    effect: &MarkEffect,
    rejected: bool,
) -> bool {
    let expected = serde_json::to_string(effect).expect("typed effect serializes");
    file.source.kind == "state.mark"
        && file.source.channel == "internal"
        && file.source.origin == "loop"
        && file.source.turn.as_deref() == Some(turn.0.as_str())
        && file.text == text
        && file
            .context
            .as_ref()
            .is_some_and(|context| context.raw.get() == expected)
        && (rejected || file.effect.as_ref() == Some(&StatementEffect::Mark(effect.clone())))
}

impl RecordingContext {
    pub(super) fn marks_blocks_external(&self) -> bool {
        self.marks.as_ref().is_some_and(|work| !work.driving)
    }

    #[allow(clippy::result_large_err)]
    pub(in crate::flow) fn begin_task(
        &mut self,
        input: AcceptedInput,
        turn: TurnRef,
        certificate: &TemplateCertificate,
        io: &mut impl PumpIo,
    ) -> Result<(), BeginTaskError> {
        let error = |input, refusal, failure| BeginTaskError {
            input,
            refusal,
            failure,
        };
        if let Err(failure) = self.live(io) {
            return Err(error(input, BeginTaskRefusal::WrongPhase, Some(failure)));
        }
        if self.phase != RecordingPhase::Entered
            || self.flow.phase() != CoordinatorPhase::Active
            || !self.barrier.is_active()
        {
            return Err(error(input, BeginTaskRefusal::WrongPhase, None));
        }
        let Some(bound) = self.deadlines.get(PumpDeadline::Decision) else {
            return Err(error(input, BeginTaskRefusal::MissingWait, None));
        };
        if self.marks.is_some()
            || self.boot_pairs.is_some()
            || self.freshness.is_some()
            || self.output.is_some()
            || self.filing.is_some()
            || self.pair_work.is_some()
            || self.repair.is_some()
            || self.read.is_some()
            || self.inspection.is_some()
            || self.member_capture.is_some()
            || self.boot_open_blocks_external()
            || self.plan_summary_blocks_external()
            || self.place_blocks_external()
            || self.measure.is_some()
            || self.input_turn.is_some()
            || self.input_author.is_some()
            || self.open_turn.is_some()
            || self.templates_block_content()
            || self.template_blocked_author.is_some()
            || self.waiting_content.is_some()
            || self.completed_author.is_some()
            || self.barrier.pending().is_some()
            || self.barrier.pending_author().is_some()
            || self.flow.has_pending_record()
            || self.flow.has_filing_consumer()
            || self.flow.has_output_consumer()
            || self.flow.pending_check().is_some()
            || self.flow.pending_control().is_some()
            || self.flow.operation_completion().is_some()
            || self.deadlines.get(PumpDeadline::Cut).is_some()
            || self.deadlines.get(PumpDeadline::Gate).is_some()
            || self.deadlines.get(PumpDeadline::StateControl).is_some()
        {
            return Err(error(input, BeginTaskRefusal::Busy, None));
        }
        if turn.0.is_empty() || self.input_turns.contains(&turn.0) || self.open_start.is_none() {
            return Err(error(input, BeginTaskRefusal::WrongPhase, None));
        }
        if let Err(invalid) = self.input.validate_accepted(
            &input,
            self.flow.run(),
            &self.declaration.scheme.scheme_id,
        ) {
            let failure = self.marks_fault(io, invalid.to_string());
            return Err(error(input, BeginTaskRefusal::WrongPhase, Some(failure)));
        }
        if !self.repair_certificate_matches(certificate) {
            return Err(error(input, BeginTaskRefusal::InvalidTemplate, None));
        }
        let expansion = match expand_begin_task(
            certificate,
            ExpansionIdentity {
                run_id: self.flow.run(),
                scheme: &self.declaration.scheme,
                checker_artifact_sha256: certificate.checker_artifact_sha256(),
                inventory_sha256: certificate.inventory_sha256(),
            },
        ) {
            Ok(expansion) => expansion,
            Err(_) => return Err(error(input, BeginTaskRefusal::InvalidTemplate, None)),
        };
        let result = self.flow.reserve_filing_receipts(&mut FlowRecording {
            barrier: &mut self.barrier,
            inbox: &mut self.inbox,
            budget: &mut self.budget,
            author: &self.author,
        });
        if let Err(failure) = result {
            let failure = self.flow_failed(io, failure);
            return Err(error(input, BeginTaskRefusal::WrongPhase, Some(failure)));
        }
        self.marks = Some(MarksTransaction {
            driving: false,
            bound,
            settled: false,
            turn,
            token: Some(input),
            input_sequence: None,
            seated: None,
            input_receipt: None,
            read: None,
            expansion,
            scope: None,
            effect: None,
            filing: None,
            stage: Stage::Input,
            fault_started: false,
            fault_sequence: None,
            refusal: None,
            cancelled: false,
            cancelled_event: None,
        });
        Ok(())
    }

    /// Bind before intake or a post-Recorded clock can yield. TurnStarted does
    /// not bind the reservation intended for this operation's one message.user.
    pub(super) fn bind_marks_author(
        &mut self,
        sequence: Sequence,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        let Some(work) = &self.marks else {
            return Ok(());
        };
        if work.stage == Stage::Fault && work.fault_started && work.fault_sequence.is_none() {
            self.marks.as_mut().unwrap().fault_sequence = Some(sequence);
            return Ok(());
        }
        if work.stage != Stage::Input
            || work.input_sequence.is_some()
            || !matches!(self.author_turn, Some((Kind::MessageUser, _)))
        {
            return Ok(());
        }
        self.marks.as_mut().unwrap().input_sequence = Some(sequence);
        let recorded_ns = io
            .now()
            .map_err(|error| self.marks_fault(io, error.to_string()))?;
        let result = self.flow.bind_filing_receipts(
            &mut FlowRecording {
                barrier: &mut self.barrier,
                inbox: &mut self.inbox,
                budget: &mut self.budget,
                author: &self.author,
            },
            sequence,
            recorded_ns,
        );
        result.map_err(|failure| self.flow_failed(io, failure))
    }

    pub(in crate::flow) fn advance_task<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<TaskProgress, RunFailure> {
        self.live(ports.io)?;
        let Some(work) = &self.marks else {
            return Err(RunFailure::MarksOwned);
        };
        if work.settled {
            return Ok(TaskProgress::Complete);
        }
        if !work.has_original_wait(self.deadlines.get(PumpDeadline::Decision)) {
            return Err(self.marks_fault(ports.io, "begin_task lost its original Decision bound"));
        }
        self.marks.as_mut().unwrap().driving = true;
        let result = self.advance_task_owned(ports);
        self.marks.as_mut().unwrap().driving = false;
        if result.is_ok() && self.marks.as_ref().unwrap().stage == Stage::Complete {
            // All required work completed under the last checked live boundary.
            // Retire the operation's wait atomically with its terminal custody.
            // A later caller retrieval must not restart or expire finished work.
            self.deadlines.disarm(PumpDeadline::Decision);
            self.marks.as_mut().unwrap().settled = true;
        }
        result
    }

    fn advance_task_owned<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<TaskProgress, RunFailure> {
        match self.marks.as_ref().unwrap().stage {
            Stage::Input => self.advance_task_input(ports)?,
            Stage::Read => self.advance_task_read(ports)?,
            Stage::Mark => self.advance_task_mark(ports)?,
            Stage::Fault => self.advance_task_fault(ports)?,
            Stage::Complete => return Ok(TaskProgress::Complete),
        }
        Ok(if self.marks.as_ref().unwrap().stage == Stage::Complete {
            TaskProgress::Complete
        } else {
            TaskProgress::Pending
        })
    }

    fn advance_task_input<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<(), RunFailure> {
        let work = self.marks.as_mut().unwrap();
        if work.cancelled && work.token.is_some() {
            self.flow.cancel_filing_receipts();
            work.stage = Stage::Complete;
            return Ok(());
        }
        if work.seated.is_none() {
            let turn = work.turn.clone();
            let seated = if let Some(token) = work.token.take() {
                match self.consume_input(token, turn, ports) {
                    Ok(seated) => seated,
                    Err(error) => {
                        if let Some(token) = error.token {
                            let work = self.marks.as_mut().unwrap();
                            work.token = Some(token);
                        }
                        return Err(error.failure);
                    }
                }
            } else {
                self.resume_input_turn(ports)?
            };
            self.marks.as_mut().unwrap().seated = seated;
            return Ok(());
        }
        if self.marks.as_ref().unwrap().input_receipt.is_none() {
            self.reduce_flow(ports.io)?;
            if !self
                .flow
                .filing_receipts()
                .is_some_and(ReceiptSet::is_complete)
            {
                return Ok(());
            }
            let result = self.flow.take_filing_receipts(&mut FlowRecording {
                barrier: &mut self.barrier,
                inbox: &mut self.inbox,
                budget: &mut self.budget,
                author: &self.author,
            });
            let receipt = result.map_err(|failure| self.flow_failed(ports.io, failure))?;
            self.marks.as_mut().unwrap().input_receipt = receipt;
            if self.marks.as_ref().unwrap().input_receipt.is_none() {
                return Ok(());
            }
        }
        let work = self.marks.as_ref().unwrap();
        let seated = work.seated.as_ref().unwrap();
        let valid = {
            let mut outcomes = work.input_receipt.as_ref().unwrap().dispositions();
            matches!((outcomes.next(), outcomes.next()), (Some(SourceDisposition::Committed(committed)), None)
                if matches!(committed.file(), LedgerFile::Statement(file)
                    if input_receipt_matches(work.input_sequence, seated, &work.turn, committed.source(), file)))
        };
        if !valid {
            return Err(self.marks_fault(
                ports.io,
                "begin_task requires its exact selected, committed input Text",
            ));
        }
        let work = self.marks.as_mut().unwrap();
        work.stage = if work.cancelled {
            Stage::Complete
        } else {
            Stage::Read
        };
        Ok(())
    }

    fn advance_task_read<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<(), RunFailure> {
        if self.marks.as_ref().unwrap().cancelled && self.read.is_none() {
            self.marks.as_mut().unwrap().stage = Stage::Complete;
            return Ok(());
        }
        if self.marks.as_ref().unwrap().read.is_none() {
            if self.read.is_none() {
                let turn = self.marks.as_ref().unwrap().turn.clone();
                match self.begin_marks_read(turn, ports.io) {
                    Ok(()) => {}
                    Err(error) => {
                        if let Some(failure) = error.failure {
                            return Err(failure);
                        }
                        if error.refusal == ReadBeginRefusal::Busy {
                            self.reduce_flow(ports.io)?;
                            return Ok(());
                        }
                        return Err(self.marks_fault(ports.io, "marks read lost admission"));
                    }
                }
            }
            if self.marks.as_ref().unwrap().cancelled {
                self.cancel_read(ports.io)?;
            }
            self.advance_read(ports)?;
            let read = self.take_read(ports.io)?;
            self.marks.as_mut().unwrap().read = read;
            if self.marks.as_ref().unwrap().read.is_none() {
                return Ok(());
            }
        }
        let work = self.marks.as_ref().unwrap();
        if work.cancelled {
            self.marks.as_mut().unwrap().stage = Stage::Complete;
            return Ok(());
        }
        let read = match work.read.as_ref().unwrap() {
            ReadCompletion::Recorded(read) => read,
            ReadCompletion::Refused(_) => {
                // The exact member refusal and its operation record remain in
                // this account. Transfer it only when the caller takes it.
                self.marks.as_mut().unwrap().stage = Stage::Complete;
                return Ok(());
            }
            ReadCompletion::Cancelled(_) => {
                return Err(self.marks_fault(ports.io, "marks read was cancelled by another owner"));
            }
        };
        if !read.belongs_to(self) {
            return Err(self.marks_fault(ports.io, "foreign marks read"));
        }
        let seed = &self.open_start.as_ref().unwrap().seed_id;
        let selected = crate::flow::marks::catalog(read.result(), seed);
        match selected {
            Ok(TaskCatalog::Existing(scope)) => {
                let work = self.marks.as_mut().unwrap();
                work.scope = Some(scope);
                work.stage = Stage::Complete;
            }
            Ok(TaskCatalog::Proposed(scope)) => {
                let work = self.marks.as_mut().unwrap();
                work.effect = Some(MarkEffect::BeginTask {
                    task: scope.task.clone(),
                    prompt: scope.prompt.clone(),
                    source: ContentRef::Event {
                        block_id: work.seated.as_ref().unwrap().source.clone(),
                    },
                });
                work.scope = Some(scope);
                work.stage = Stage::Mark;
            }
            Err(CatalogError::CounterExhausted) => {
                let work = self.marks.as_mut().unwrap();
                work.refusal = Some(TaskRefusal::CounterExhausted);
                work.stage = Stage::Complete;
            }
            Err(CatalogError::Invalid) => {
                return Err(self.marks_fault(ports.io, "malformed begin_task mark catalog"));
            }
        }
        Ok(())
    }

    fn advance_task_mark<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<(), RunFailure> {
        if self.marks.as_ref().unwrap().cancelled
            && let Some(event) = self.withdraw_unstarted_mark()
        {
            let work = self.marks.as_mut().unwrap();
            work.cancelled_event = Some(event);
            work.stage = Stage::Complete;
            return Ok(());
        }
        if self.filing.is_none() && self.marks.as_ref().unwrap().filing.is_none() {
            let work = self.marks.as_ref().unwrap();
            if work.cancelled {
                self.marks.as_mut().unwrap().stage = Stage::Complete;
                return Ok(());
            }
            let MarkEffect::BeginTask {
                task,
                prompt,
                source: ContentRef::Event { block_id },
            } = work.effect.as_ref().unwrap()
            else {
                unreachable!("owned begin_task effect");
            };
            let event = AuthoredEvent {
                kind: Kind::StateMark,
                subsystem: Subsystem::Harness,
                turn: Some(work.turn.clone()),
                source: AuthoredSource::Loop,
                causal_parent: Some(Sequence(block_id.sequence.0)),
                payload: Some(Payload::StateMark(Box::new(trace::StateMarkRecord {
                    template_id: work.expansion.template_id().into(),
                    message: serde_json::value::to_raw_value(work.expansion.message())
                        .expect("typed Message serializes"),
                    context: trace::MarkEffect::BeginTask {
                        task: super::super::mirror::span_ref(task),
                        prompt: super::super::mirror::span_ref(prompt),
                        source: trace::ContentRef::Event {
                            block_id: super::super::mirror::source(block_id),
                        },
                    },
                }))),
            };
            match self.begin_owned_filing(
                event,
                PumpDeadline::Decision,
                FilingOwner::Marks,
                ports.io,
            ) {
                Ok(()) => {}
                Err(error) => {
                    if let Some(failure) = error.failure {
                        return Err(failure);
                    }
                    if error.refusal == FilingBeginRefusal::Busy {
                        self.reduce_flow(ports.io)?;
                        return Ok(());
                    }
                    return Err(self.marks_fault(ports.io, "mark author lost admission"));
                }
            }
        }
        if self.marks.as_ref().unwrap().filing.is_none() {
            self.advance_owned_filing(FilingOwner::Marks, ports)?;
            let filing = self.take_owned_filing(FilingOwner::Marks, ports.io)?;
            self.marks.as_mut().unwrap().filing = filing;
            if self.marks.as_ref().unwrap().filing.is_none() {
                return Ok(());
            }
        }
        let work = self.marks.as_ref().unwrap();
        let filing = work.filing.as_ref().unwrap();
        let outcome = {
            let mut outcomes = filing.receipts().dispositions();
            match (outcomes.next(), outcomes.next()) {
                (Some(outcome), None) => Some(outcome),
                _ => None,
            }
        };
        let Some(outcome) = outcome else {
            return Err(self.marks_fault(
                ports.io,
                "required state.mark content was not selected exactly once",
            ));
        };
        if !mark_source_matches(outcome.source(), self.flow.run(), filing.sequence()) {
            return Err(self.marks_fault(ports.io, "mark receipt names another source"));
        }
        let (file, rejected) = match outcome {
            SourceDisposition::Committed(source) => {
                let LedgerFile::Statement(file) = source.file() else {
                    return Err(self.marks_fault(ports.io, "mark receipt is not a statement"));
                };
                (file.as_ref(), false)
            }
            SourceDisposition::CorrectlyRejected(source) => (source.file(), true),
        };
        if !mark_content_matches(
            file,
            &work.turn,
            work.expansion.text(),
            work.effect.as_ref().unwrap(),
            rejected,
        ) {
            return Err(self.marks_fault(
                ports.io,
                "mark filing changed its exact template/context/source association",
            ));
        }
        self.marks.as_mut().unwrap().stage = if rejected {
            Stage::Fault
        } else {
            Stage::Complete
        };
        Ok(())
    }

    fn advance_task_fault<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<(), RunFailure> {
        let work = self.marks.as_ref().unwrap();
        if !work.fault_started {
            let filing = work.filing.as_ref().unwrap();
            let rejected = filing
                .receipts()
                .dispositions()
                .find_map(|outcome| match outcome {
                    SourceDisposition::CorrectlyRejected(source) => Some(source),
                    _ => None,
                })
                .unwrap();
            let fault = weaver_types::FaultReport { case: weaver_types::FaultCase::StateTemplateRejected,
                account: serde_json::value::to_raw_value(&serde_json::json!({ "template_id": work.expansion.template_id(), "source_id": rejected.source(), "result": rejected.reply() })).expect("typed fault serializes") };
            let event = AuthoredEvent {
                kind: Kind::Fault,
                subsystem: Subsystem::Harness,
                turn: Some(work.turn.clone()),
                source: AuthoredSource::Runtime,
                causal_parent: Some(filing.sequence()),
                payload: Some(Payload::Fault(
                    serde_json::value::to_raw_value(&fault).expect("typed fault serializes"),
                )),
            };
            self.marks.as_mut().unwrap().fault_started = true;
            let sequence = self.author_and_wait(ports, event)?;
            self.marks.as_mut().unwrap().fault_sequence = Some(sequence);
        } else if self.completed_author.is_some()
            || self.waiting_content.is_some()
            || self.barrier.pending_author().is_some()
        {
            let sequence = self.resume_author_and_wait(ports)?;
            self.marks.as_mut().unwrap().fault_sequence = Some(sequence);
        }
        let work = self.marks.as_mut().unwrap();
        let sequence = work
            .fault_sequence
            .expect("recorded fault retained before fallible continuation");
        work.refusal = Some(TaskRefusal::TemplateRejected { fault: sequence });
        work.stage = Stage::Complete;
        Ok(())
    }

    pub(in crate::flow) fn cancel_task(&mut self) {
        if let Some(work) = &mut self.marks {
            work.cancelled = true;
        }
    }

    pub(in crate::flow) fn take_task(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<Option<TaskCompletion>, RunFailure> {
        self.live(io)?;
        let Some(work) = &self.marks else {
            return Ok(None);
        };
        if !work.settled {
            return Ok(None);
        }
        if self.deadlines.get(PumpDeadline::Decision).is_some() {
            return Err(self.marks_fault(
                io,
                "another Decision replaced the completed begin_task wait",
            ));
        }
        if work.stage != Stage::Complete {
            return Ok(None);
        }
        let mut work = self.marks.take().unwrap();
        if work.cancelled {
            return Ok(Some(TaskCompletion::Cancelled(TaskAccount { work })));
        }
        if matches!(work.read, Some(ReadCompletion::Refused(_))) {
            let Some(ReadCompletion::Refused(refusal)) = work.read.take() else {
                unreachable!();
            };
            return Ok(Some(TaskCompletion::Refused {
                refusal: TaskRefusal::Read(refusal),
                account: TaskAccount { work },
            }));
        }
        if let Some(refusal) = work.refusal.take() {
            return Ok(Some(TaskCompletion::Refused {
                refusal,
                account: TaskAccount { work },
            }));
        }
        let Some(ReadCompletion::Recorded(catalog)) = work.read.take() else {
            unreachable!("completed mark retains its recorded catalog");
        };
        Ok(Some(TaskCompletion::Begun(BegunTask {
            scope: work.scope.unwrap(),
            input: FiledTaskInput {
                seated: work.seated.unwrap(),
                receipt: work.input_receipt.unwrap(),
            },
            catalog,
            mark: work.filing,
        })))
    }

    fn marks_fault(&mut self, io: &mut impl PumpIo, detail: impl Into<String>) -> RunFailure {
        self.fail(
            io,
            FlowHaltReason::ProtocolFailed,
            HaltPoint::Barrier,
            detail,
        )
    }
}

#[cfg(test)]
#[path = "context_marks_validation_tests.rs"]
pub(super) mod validation_tests;
