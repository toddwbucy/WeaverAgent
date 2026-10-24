//! Stop settles the original operation before the shared Recorded world close.
use super::super::close::WorldClose;
use super::*;
use weaver_types::{Cause, LifecycleAnswer};

pub(super) struct StopWork {
    close: WorldClose,
    pub(super) gate_done: bool,
}
#[cfg(test)]
#[path = "context_worker_stop_tests.rs"]
pub(in crate::flow::context::worker_startup) mod tests;

impl StopWork {
    pub(super) fn begin(
        input: &mut FoundationInput<'_>,
        cause: Cause,
        io: &mut impl PumpIo,
    ) -> Result<Self, RunFailure> {
        Ok(Self {
            close: WorldClose::stopped(input, cause, io)?,
            gate_done: false,
        })
    }
    pub(super) fn gate_send(&self) -> Option<&OrganSend> {
        self.close.send()
    }
    pub(super) fn close_recorded(&self) -> bool {
        self.close.recorded().is_some()
    }
    pub(super) fn answer(&self) -> Option<LifecycleAnswer> {
        self.close.recorded().map(|_| LifecycleAnswer::TurnAborted {
            turn: TurnKey(self.close.turn().0.clone()),
        })
    }
    pub(super) fn advance(
        &mut self,
        input: &mut FoundationInput<'_>,
        peer: Option<&mut LifecyclePeer>,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        if self.gate_done {
            return Ok(());
        }
        input.cancel_engine(io)?;
        if !input.engine_settled() {
            input.advance_engine(peer, io)?;
            return Ok(());
        }
        let owner = &mut input.serving.owner;
        let mut ports = PumpPorts {
            lifecycle: LifecyclePorts {
                listener: Some(&mut owner.listener),
                dial: peer,
                residency: Some(owner.resident.residency),
                ..Default::default()
            },
            gate: Some(&mut owner.gate),
            decode: Some(owner.decode),
            label: None,
            io,
        };
        if self.close.advance(owner.context, &mut ports)? {
            self.gate_done = true;
            input.stage = Stage::Aborted;
        }
        Ok(())
    }
}
