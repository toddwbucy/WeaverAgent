//! Only the original authenticated opener can begin a root update. The worker
//! returns its suspended input while that peer and operation stay owned.
use super::super::plan::PendingPlan;
use super::*;
use crate::flow::pump::plan::PlanReceiveError;

impl<'a> FoundationManagement<'a> {
    pub fn queue_plan_update(self: Box<Self>) -> Result<Box<FoundationInput<'a>>, Box<Self>> {
        self.queue_plan_update_with_io(&mut MonotonicIo)
    }

    pub(in crate::flow::context) fn queue_plan_update_with_io(
        mut self: Box<Self>,
        io: &mut impl PumpIo,
    ) -> Result<Box<FoundationInput<'a>>, Box<Self>> {
        if self.progress != FoundationManagementProgress::DirectivePending
            || self.message.is_some()
            || self.declined_plan.is_some()
            || self.save_message.is_some()
            || self.answer.is_some()
            || self.send.is_some()
            || self.stop.is_some()
            || self.read_armed
            || !self.input.plan_at_rest()
            || !self.peer.held().is_some_and(|message| {
                matches!(
                    message.envelope.payload,
                    Payload::Directive(LifecycleDirective::PlanUpdate { .. })
                )
            })
        {
            return Err(self);
        }
        let packet = match self.peer.take_root_plan_update() {
            Ok(Some(packet)) => packet,
            Ok(None) => return Err(self),
            Err(PlanReceiveError::Refused(refusal)) => {
                // A failed authentication/cause judgment leaves the packet on
                // its port until this same owner takes it for its refusal.
                self.declined_plan = match self.peer.take_refused_plan_update() {
                    Ok(value) => value,
                    Err(error) => {
                        self.detail = Some(format!("root refusal take: {error:?}"));
                        return Err(self);
                    }
                };
                self.progress = FoundationManagementProgress::Pending;
                let exchange = self
                    .declined_plan
                    .as_ref()
                    .unwrap()
                    .envelope()
                    .exchange
                    .clone();
                let result = self.refuse_plan(exchange, refusal, io);
                self.reduce_result(result, io);
                return Err(self);
            }
            Err(PlanReceiveError::Transport(failure)) => {
                self.detail = Some(format!("root plan receive: {failure:?}"));
                self.progress = FoundationManagementProgress::Dropped;
                return Err(self);
            }
        };
        let exchange = packet.envelope().exchange.clone();
        if let Err(error) = self
            .input
            .serving
            .owner
            .context
            .begin_root_plan_update(packet, io)
        {
            self.declined_plan = error
                .original
                .map(crate::flow::pump::plan::RootPlanPacket::into_refused);
            if let Some(failure) = error.failure {
                self.detail = Some(format!("root plan admission: {failure:?}"));
                self.input.failure = Some(failure);
                self.input.stage = Stage::Stopped;
                self.progress = FoundationManagementProgress::Stopped;
            } else {
                self.progress = FoundationManagementProgress::Pending;
                let result = self.refuse_plan(exchange, error.refusal, io);
                self.reduce_result(result, io);
            }
            return Err(self);
        }
        let bound = self
            .input
            .serving
            .owner
            .context
            .root_plan_bound()
            .expect("accepted root bound");
        let Self {
            mut input, peer, ..
        } = *self;
        input.serving.plan = Some(Box::new(PendingPlan::new(peer, exchange, bound)));
        Ok(input)
    }
}
