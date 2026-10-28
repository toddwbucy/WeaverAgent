//! Pre-Commit Leave refusal stays separate from capture success and run end.
use super::*;
use weaver_types::state_flow::capture::{CaptureRefusal, CaptureRefused, SaveRefusal};

impl Capture<'_> {
    pub(super) fn prepare_abort(&mut self, code: &str, detail: &str) {
        assert!(self.abort.is_none(), "one admin cancellation per capture");
        self.aborted_at = Some(self.stage);
        self.abort = Some(CaptureEnvelope {
            exchange: self.directive.exchange().clone(),
            position: Position::Continue,
            payload: CapturePayload::SavePointAborted(
                weaver_types::state_flow::capture::CaptureAborted {
                    capture_id: self.windows.capture().unwrap().capture_id,
                    code: code.into(),
                    detail: detail.into(),
                },
            ),
        });
        self.stage = Stage::Aborting;
    }

    pub(super) fn begin_expired_refusal(&mut self) -> Result<bool, FailureCause> {
        if self.windows.demand_window().is_some()
            || self.committed
            || self.refused.is_some()
            || self.failure.is_some()
            || self.abort.is_some()
            || !matches!(
                self.stage,
                Stage::Image | Stage::Hashing | Stage::Finished | Stage::Preparing | Stage::Commit
            )
            || !self
                .windows
                .capture_expired()
                .map_err(FailureCause::Channel)?
        {
            return Ok(false);
        }
        self.windows
            .begin_refusal()
            .map_err(FailureCause::Channel)?;
        if let Some(publication) = &mut self.publication {
            self.publication_failure = Some(publication.failed("capture expired before Commit"));
        }
        self.prepare_abort("capture_expired", "capture window expired before Commit");
        if let Some(received) = self.unexpected.take() {
            let correlated = &received.envelope.exchange == self.directive.exchange()
                && same_run(&received.envelope, &self.observed.facts.run)
                && same_capture(
                    &received.envelope,
                    self.windows.capture().unwrap().capture_id,
                );
            self.received.push(received);
            if !correlated {
                return Err(FailureCause::Protocol(
                    "foreign packet crossed capture expiry",
                ));
            }
            self.reduce_refusal_packet()?;
        }
        Ok(true)
    }

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
        self.reduce_refusal_packet()
    }

    fn reduce_refusal_packet(&mut self) -> Result<(), FailureCause> {
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
