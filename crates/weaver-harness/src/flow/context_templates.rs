//! Ordered enter-template certification through the run's existing classifier,
//! trace barrier and original Enter wait. No runtime instruction is authored here.

use super::*;
use crate::flow::classifier::ClassifierState;
use crate::flow::coordinator::OperationStart;
use crate::flow::templates::{
    CompiledInventory, TemplateCertificate, TemplateCheckTranscript, TemplateError,
    TranscriptProgress,
};
use weaver_types::state_flow::{DecimalU64, Digest};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum TemplateBeginRefusal {
    WrongPhase,
    Busy,
    AlreadyStarted,
    Inventory(TemplateError),
}

#[derive(Debug)]
pub(in crate::flow) struct TemplateBeginError {
    pub inventory: CompiledInventory,
    pub refusal: TemplateBeginRefusal,
    pub failure: Option<RunFailure>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum TemplateProgress {
    Progress,
    Pending,
    Complete,
    Rejected,
}

pub(super) struct TemplateChecks {
    transcript: TemplateCheckTranscript,
    enter: AbsoluteBound,
    pending_request: Option<DecimalU64>,
    certificate: Option<TemplateCertificate>,
    complete: bool,
    rejected: bool,
}

impl TemplateChecks {
    pub(super) fn can_certify(&self) -> bool {
        !self.complete && !self.rejected
    }
}

impl RecordingContext {
    /// Return the refused caller event unchanged. It never owned a recorder
    /// ticket and this transfer cannot admit it or resume another author.
    pub(super) fn take_template_blocked_author(&mut self) -> Option<AuthoredEvent> {
        self.template_blocked_author.take()
    }

    // Refusal returns the owned compiled inventory without a failure-path allocation.
    #[allow(clippy::result_large_err)]
    pub(super) fn begin_template_checks(
        &mut self,
        inventory: CompiledInventory,
        io: &mut impl PumpIo,
    ) -> Result<(), TemplateBeginError> {
        if let Err(failure) = self.live(io) {
            return Err(TemplateBeginError {
                inventory,
                refusal: TemplateBeginRefusal::WrongPhase,
                failure: Some(failure),
            });
        }
        if self.template_checks.is_some() || self.template_setup_refusal.is_some() {
            return Err(TemplateBeginError {
                inventory,
                refusal: TemplateBeginRefusal::AlreadyStarted,
                failure: None,
            });
        }
        let enter = self.deadlines.get(PumpDeadline::Enter);
        if self.phase != RecordingPhase::Entered
            || !self.barrier.is_active()
            || self.flow.phase() != CoordinatorPhase::Active
            || self.flow.standing_classifier().is_none()
            || enter.is_none()
            || self.load_duty.is_none()
        {
            return Err(TemplateBeginError {
                inventory,
                refusal: TemplateBeginRefusal::WrongPhase,
                failure: None,
            });
        }
        if self.output.is_some()
            || self.filing.is_some()
            || self.pair_work.is_some()
            || self.input.snapshot().is_some()
            || !self.input_turns.is_empty()
            || self.input_turn.is_some()
            || self.input_author.is_some()
            || self.member_capture.is_some()
            || self.read.is_some()
            || self.measure_blocks_external()
            || self.open_turn.is_some()
            || self.barrier.pending().is_some()
            || self.barrier.pending_author().is_some()
            || self.waiting_content.is_some()
            || self.completed_author.is_some()
            || self.template_blocked_author.is_some()
            || self.flow.has_pending_record()
            || self.flow.pending_check().is_some()
            || self.flow.pending_control().is_some()
            || self.flow.operation_completion().is_some()
            || self.flow.ledger().facts().next().is_some()
            || self.inbox.queued().next().is_some()
            || self.budget.waiting_copy().is_some()
            || self.budget.gauges() != (0, 0)
            || self.flow.classifier().state() != ClassifierState::Idle
        {
            return Err(TemplateBeginError {
                inventory,
                refusal: TemplateBeginRefusal::Busy,
                failure: None,
            });
        }
        let duty = self.load_duty.as_ref().expect("recorded load duty");
        let checker = match Digest::new(duty.instruction.model_artifact_sha256.clone()) {
            Ok(checker) => checker,
            Err(_) => {
                self.template_setup_refusal = Some(TemplateError::InvalidIdentity);
                return Err(TemplateBeginError {
                    inventory,
                    refusal: TemplateBeginRefusal::Inventory(TemplateError::InvalidIdentity),
                    failure: None,
                });
            }
        };
        // begin_classifier compared the exact load duty with bootstrap. Taking
        // its validated Ready created this classifier under the same scheme.
        if self.flow.classifier().scheme() != &self.declaration.scheme
            || self.flow.classifier().run() != self.barrier.run()
        {
            self.template_setup_refusal = Some(TemplateError::InvalidIdentity);
            return Err(TemplateBeginError {
                inventory,
                refusal: TemplateBeginRefusal::Inventory(TemplateError::InvalidIdentity),
                failure: None,
            });
        }
        let transcript = match TemplateCheckTranscript::begin_preserving(
            inventory,
            self.barrier.run().to_owned(),
            self.declaration.scheme.clone(),
            checker,
            &duty.template_inventory,
        ) {
            Ok(transcript) => transcript,
            Err((inventory, error)) => {
                self.template_setup_refusal = Some(error);
                return Err(TemplateBeginError {
                    inventory,
                    refusal: TemplateBeginRefusal::Inventory(error),
                    failure: None,
                });
            }
        };
        self.template_checks = Some(TemplateChecks {
            transcript,
            enter: enter.expect("the original Enter bound exists"),
            pending_request: None,
            certificate: None,
            complete: false,
            rejected: false,
        });
        Ok(())
    }

    /// One bounded reduction or submission. Transport and waiting stay in the
    /// common pump. A pending check or author remains owned across interruption.
    pub(super) fn advance_template_checks(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<TemplateProgress, RunFailure> {
        self.live(io)?;
        self.template_enter_live()?;
        let owner = self
            .template_checks
            .as_ref()
            .expect("checked template owner");
        if owner.rejected {
            return Ok(TemplateProgress::Rejected);
        }
        if owner.complete {
            return Ok(TemplateProgress::Complete);
        }
        self.reduce_flow(io)?;
        let expected_request = self.template_checks.as_ref().unwrap().pending_request;
        if let Some(completion) = self.flow.operation_completion() {
            let OperationCompletion::Check(recorded) = completion else {
                return Err(
                    self.template_fault(io, "another operation entered template certification")
                );
            };
            if expected_request != Some(recorded.reply().request_id) {
                return Err(
                    self.template_fault(io, "template result has no matching owned request")
                );
            }
            let completion = self.flow.take_operation_completion(&mut FlowRecording {
                barrier: &mut self.barrier,
                inbox: &mut self.inbox,
                budget: &mut self.budget,
                author: &self.author,
            });
            let completion = completion.map_err(|failure| self.flow_failed(io, failure))?;
            let Some(OperationCompletion::Check(recorded)) = completion else {
                return Ok(TemplateProgress::Pending);
            };
            let progress = {
                let owner = self.template_checks.as_mut().unwrap();
                owner.pending_request = None;
                owner.transcript.recorded(recorded)
            };
            match progress {
                Ok(TranscriptProgress::Rejected) => {
                    self.template_checks.as_mut().unwrap().rejected = true;
                    return Ok(TemplateProgress::Rejected);
                }
                Ok(TranscriptProgress::Accepted) => {
                    self.live(io)?;
                    return Ok(TemplateProgress::Progress);
                }
                Ok(TranscriptProgress::Complete) => return self.finish_template_checks(io),
                Err(error) => return Err(self.template_fault(io, error.to_string())),
            }
        }
        let owner = self.template_checks.as_ref().unwrap();
        if owner.pending_request.is_some() || self.barrier.pending().is_some() {
            return Ok(TemplateProgress::Pending);
        }
        if owner.transcript.is_complete() {
            return self.finish_template_checks(io);
        }
        let operation = owner.transcript.next_operation().ok_or_else(|| {
            self.template_fault(io, "unfinished template transcript has no next entry")
        })?;
        let result = self.flow.begin_check(
            &mut FlowRecording {
                barrier: &mut self.barrier,
                inbox: &mut self.inbox,
                budget: &mut self.budget,
                author: &self.author,
            },
            operation,
        );
        match result.map_err(|failure| self.flow_failed(io, failure))? {
            OperationStart::Started { request_id } => {
                self.template_checks.as_mut().unwrap().pending_request = Some(request_id);
                self.live(io)?;
                Ok(TemplateProgress::Progress)
            }
            OperationStart::Busy(_) => Ok(TemplateProgress::Pending),
        }
    }

    pub(super) fn template_transcript(&self) -> Option<&TemplateCheckTranscript> {
        self.template_checks.as_ref().map(|owner| &owner.transcript)
    }

    pub(super) fn take_template_certificate(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<Option<TemplateCertificate>, RunFailure> {
        self.live(io)?;
        self.template_enter_live()?;
        if self.barrier.pending().is_some()
            || self.barrier.pending_author().is_some()
            || self.waiting_content.is_some()
            || self.completed_author.is_some()
            || self.template_blocked_author.is_some()
            || self.flow.has_pending_record()
        {
            return Ok(None);
        }
        Ok(self.template_checks.as_mut().unwrap().certificate.take())
    }

    pub(super) fn templates_block_content(&self) -> bool {
        self.template_setup_refusal.is_some()
            || self.template_blocked_author.is_some()
            || self
                .template_checks
                .as_ref()
                .is_some_and(|owner| !owner.complete)
    }

    fn template_enter_live(&self) -> Result<(), RunFailure> {
        let Some(owner) = self.template_checks.as_ref() else {
            return Err(RunFailure::TemplateBlocked);
        };
        if self.phase != RecordingPhase::Entered
            || self.deadlines.get(PumpDeadline::Enter) != Some(owner.enter)
        {
            return Err(RunFailure::TemplateBlocked);
        }
        // live() checked every original deadline immediately before this call.
        // Keep the explicit owner-bound check so a retired Enter cannot certify.
        if owner.enter.expired(self.last_ns) {
            return Err(RunFailure::Wait {
                key: PumpDeadline::Enter,
                bound: owner.enter,
            });
        }
        Ok(())
    }

    fn finish_template_checks(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<TemplateProgress, RunFailure> {
        self.live(io)?;
        self.template_enter_live()?;
        let certificate = self
            .template_checks
            .as_mut()
            .unwrap()
            .transcript
            .finish()
            .map_err(|error| self.template_fault(io, error.to_string()))?;
        let owner = self.template_checks.as_mut().unwrap();
        owner.certificate = Some(certificate);
        owner.complete = true;
        Ok(TemplateProgress::Complete)
    }

    fn template_fault(&mut self, io: &mut impl PumpIo, detail: impl Into<String>) -> RunFailure {
        self.fail(
            io,
            FlowHaltReason::ProtocolFailed,
            HaltPoint::Classifier,
            detail,
        )
    }
}
