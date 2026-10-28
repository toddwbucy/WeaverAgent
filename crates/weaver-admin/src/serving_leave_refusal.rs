//! Pre-Commit Leave refusal stays separate from capture success and run end.
use super::*;
use weaver_types::state_flow::capture::{CaptureRefusal, CaptureRefused, SaveRefusal};

impl Capture<'_> {
    pub(super) fn accept_capture_refused(
        &mut self,
        value: CaptureRefused,
    ) -> Result<(), FailureCause> {
        if self.windows.demand_window().is_some()
            || self.committed
            || self.refused.is_some()
            || !matches!(
                self.stage,
                Stage::Image
                    | Stage::Hashing
                    | Stage::Finished
                    | Stage::Preparing
                    | Stage::Commit
                    | Stage::AwaitCaptureRefused
            )
            || !matches!(
                value.missed,
                SavePointLeg::Answer | SavePointLeg::Finished | SavePointLeg::Published
            )
        {
            return Err(FailureCause::Protocol(
                "CaptureRefused outside pre-Commit Leave",
            ));
        }
        self.windows
            .refused(value.capture_id)
            .map_err(FailureCause::Channel)?;
        // The acknowledgement is retained even if local artifact cleanup fails.
        // Such a failure cannot start a second pre-Commit cancellation exchange.
        self.refused = Some(value.missed);
        if let Some(publication) = &mut self.publication {
            publication
                .cancel_unconfirmed_leave(&self.observed.facts.run, value.capture_id)
                .map_err(|error| FailureCause::Publication(Box::new(error)))?;
            self.publication_failure = Some(publication.failed("worker completed capture refusal"));
        } else if let Some(failure) = &self.publication_failure
            && !failure.artifacts.is_empty()
        {
            return Err(FailureCause::Publication(Box::new(failure.clone())));
        }
        // A local preparation error remains in publication_failure. Once the
        // worker proves cleanup it is an operation refusal, not a failed run.
        self.failure = None;
        self.checkpoint_failure = None;
        self.stage = Stage::AwaitRefusedLeave;
        self.windows.check().map_err(FailureCause::Channel)
    }

    pub(super) fn accept_checkpoint_refusal(&mut self) -> Result<(), FailureCause> {
        let received = self
            .unexpected
            .as_ref()
            .expect("retained checkpoint packet");
        if &received.envelope.exchange != self.directive.exchange()
            || !same_run(&received.envelope, &self.observed.facts.run)
        {
            return Err(FailureCause::Protocol("foreign capture acknowledgement"));
        }
        let CapturePayload::SavePointCaptureRefused(value) = &received.envelope.payload else {
            unreachable!("caller selects the refused-capture checkpoint")
        };
        self.accept_capture_refused(value.clone())?;
        self.received.push(self.unexpected.take().unwrap());
        Ok(())
    }

    pub(super) fn await_capture_refused(&mut self) -> Result<(), FailureCause> {
        self.receive_correlated()?;
        let envelope = &self.received.last().unwrap().envelope;
        if let CapturePayload::SavePointCaptureRefused(value) = &envelope.payload {
            return self.accept_capture_refused(value.clone());
        }
        if negative(envelope, &self.windows, false) {
            return Err(FailureCause::WorkerOutcome);
        }
        let crossed = matches!(
            (self.aborted_at, &envelope.payload),
            (Some(Stage::Image), CapturePayload::SavePointImage(_))
                | (
                    Some(Stage::Finished),
                    CapturePayload::SavePointImageFinished(_)
                )
        );
        if !crossed || self.abort_crossed {
            return Err(FailureCause::Protocol(
                "unexpected continuation after admin Aborted",
            ));
        }
        self.abort_crossed = true;
        Ok(())
    }

    pub(super) fn await_refused_leave(&mut self) -> Result<(), FailureCause> {
        self.receive_correlated()?;
        match &self.received.last().unwrap().envelope.payload {
            CapturePayload::Answer(CaptureAnswer::Left {
                save_point: None,
                forced: true,
                no_state: false,
            }) => {
                self.stage = Stage::LeftReceived;
                Ok(())
            }
            CapturePayload::Refusal(CaptureRefusal::Save(SaveRefusal::SavePointNotTaken {
                missed,
            })) if Some(*missed) == self.refused && self.windows.matches_leave(false) => {
                Err(FailureCause::WorkerOutcome)
            }
            CapturePayload::Refusal(refusal) if crate::terminal_unwind_refusal(refusal) => {
                Err(FailureCause::WorkerOutcome)
            }
            _ => Err(FailureCause::Protocol(
                "refused capture lacks its final Leave disposition",
            )),
        }
    }

    pub(super) fn refused_leave_standing(&self) -> bool {
        self.windows.matches_leave(false)
            && matches!(self.failure, Some(FailureCause::WorkerOutcome))
            && self.received.last().is_some_and(|received| matches!(
                &received.envelope.payload,
                CapturePayload::Refusal(CaptureRefusal::Save(SaveRefusal::SavePointNotTaken { missed }))
                    if Some(*missed) == self.refused
            ))
    }
}
