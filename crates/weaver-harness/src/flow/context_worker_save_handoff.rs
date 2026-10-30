//! Transfer the exact paired SavePoint opener to its serving seat. The current
//! input and worker policy continue with their original identities and clocks.
use super::super::save::PendingSave;
use super::*;
use crate::flow::context::leave_capture::ServingCapture;

impl<'a> FoundationManagement<'a> {
    pub fn queue_save(self: Box<Self>) -> Result<Box<FoundationInput<'a>>, Box<Self>> {
        self.queue_save_with_io(&mut MonotonicIo)
    }

    pub(in crate::flow::context) fn queue_save_with_io(
        mut self: Box<Self>,
        io: &mut impl PumpIo,
    ) -> Result<Box<FoundationInput<'a>>, Box<Self>> {
        if self.progress != FoundationManagementProgress::DirectivePending
            || self.message.is_some()
            || self.save_message.is_some()
            || self.answer.is_some()
            || self.send.is_some()
            || self.stop.is_some()
            || self.read_armed
            || self.input.has_graceful_leave()
            || self.input.serving.save.is_some()
            // Queueing may retain a save behind a non-root state owner. The
            // capture boundary, not this handoff, requires plan_work_pending
            // to clear. Root plan exchanges already own the shared peer lane.
            || self.input.plan_pending()
            || !self.input.rollback_admissible()
        {
            return Err(self);
        }
        let Some(held) = self.peer.capture_held() else {
            return Err(self);
        };
        let CapturePayload::Directive(CaptureDirective::SavePoint { window, .. }) =
            held.envelope.payload
        else {
            return Err(self);
        };
        if held.envelope.exchange.opener != Opener::Admin
            || held.envelope.position != Position::Open
            || held.sink.is_some()
            || held.state.is_some()
        {
            return Err(self);
        }
        let Some(received_ns) = self.heard_ns else {
            return Err(self);
        };
        self.save_message = match self.peer.take_capture() {
            Ok(Some(message)) => Some(message),
            Ok(None) => return Err(self),
            Err(error) => {
                self.detail = Some(format!("serving save directive: {error:?}"));
                return Err(self);
            }
        };
        if let Err(error) = self.peer.enable_capture() {
            self.detail = Some(format!("serving save projection: {error:?}"));
            self.peer_unavailable = true;
            self.progress = FoundationManagementProgress::Dropped;
            return Err(self);
        }
        let result = ServingCapture::queue_demand(
            self.input.serving.owner.context,
            &self.peer,
            self.save_message
                .as_ref()
                .unwrap()
                .message
                .envelope
                .exchange
                .clone(),
            window,
            received_ns,
            io,
        );
        let capture = match result {
            Ok(capture) => capture,
            Err(failure) => {
                self.detail = Some(format!("serving save admission: {failure:?}"));
                self.input.failure = Some(failure);
                self.input.stage = Stage::Stopped;
                self.progress = FoundationManagementProgress::Stopped;
                return Err(self);
            }
        };
        let Self {
            mut input,
            peer,
            save_message,
            ..
        } = *self;
        input.serving.save = Some(Box::new(PendingSave::new(
            peer,
            save_message.unwrap(),
            capture,
        )));
        Ok(input)
    }
}
