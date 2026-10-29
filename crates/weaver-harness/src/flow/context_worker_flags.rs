//! The explicit flag caller shares the serving reader and its original bound.
use super::*;
use crate::flow::context::read_driver::ReadProgress;
use crate::flow::context::read_driver::flags::{FlagObservation, FlagsBeginRefusal, RecordedFlags};
use weaver_types::state_flow::{
    DecimalU64, PlanFlagSnapshot, PlanRef, ReadRefusalCode, ReadSelector, SourceBlockId,
};

/// An admission refusal returns the caller's exact attribution. It creates no read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoundationFlagsRefusal {
    Busy,
    WrongPhase,
    InvalidAttribution,
    Stopped,
}

#[derive(Debug)]
pub struct FoundationFlagsError {
    pub plan: PlanRef,
    pub after: SourceBlockId,
    pub refusal: FoundationFlagsRefusal,
}

#[derive(Debug, PartialEq, Eq)]
pub enum FoundationFlagObservation<'a> {
    Measured(&'a PlanFlagSnapshot),
    Unavailable(ReadRefusalCode),
    Cancelled,
}

/// A move-owned Recorded observation, with its read and release account retained.
/// It grants no placement, mutation or progression authority.
///
/// ```compile_fail,E0277
/// fn cloned<T: Clone>() {}
/// cloned::<weaver_harness::FoundationFlags>();
/// ```
#[derive(Debug)]
pub struct FoundationFlags {
    recorded: RecordedFlags,
}

impl FoundationFlags {
    pub fn observation(&self) -> FoundationFlagObservation<'_> {
        match self.recorded.observation() {
            Some(FlagObservation::Measured(snapshot)) => {
                FoundationFlagObservation::Measured(snapshot)
            }
            Some(FlagObservation::Unavailable(code)) => {
                FoundationFlagObservation::Unavailable(*code)
            }
            None => FoundationFlagObservation::Cancelled,
        }
    }
    pub fn sequence(&self) -> weaver_trace::Sequence {
        self.recorded.sequence()
    }
    pub fn request_id(&self) -> DecimalU64 {
        self.recorded.request_id()
    }
}

impl FoundationInput<'_> {
    /// Explicit sampling at an admissible read boundary. Busy returns both
    /// identities unchanged; the caller chooses whether and when to try again.
    #[allow(clippy::result_large_err)]
    pub fn observe_flags(
        &mut self,
        plan: PlanRef,
        after: SourceBlockId,
    ) -> Result<(), FoundationFlagsError> {
        let context = &mut self.serving.owner.context;
        if let Err(failure) = context.live(&mut MonotonicIo) {
            self.failure = Some(failure);
            self.stage = Stage::Stopped;
        }
        if self.stage == Stage::Stopped {
            return Err(FoundationFlagsError {
                plan,
                after,
                refusal: FoundationFlagsRefusal::Stopped,
            });
        }
        if self.serving.flags.is_some()
            || self.serving.owner.listener.held().is_some()
            || self.plan_pending()
            || self.plan_operation_pending()
            || self.serving.save.is_some()
            || self.has_graceful_leave()
            || self.report.is_some()
        {
            return Err(FoundationFlagsError {
                plan,
                after,
                refusal: FoundationFlagsRefusal::Busy,
            });
        }
        self.serving
            .owner
            .context
            .observe_flags(plan, after, &mut MonotonicIo)
            .map_err(|error| {
                let ReadSelector::PlanFlags { plan, after } = error.request else {
                    unreachable!()
                };
                let refusal = if let Some(failure) = error.failure {
                    self.failure = Some(failure);
                    self.stage = Stage::Stopped;
                    FoundationFlagsRefusal::Stopped
                } else {
                    match error.refusal {
                        FlagsBeginRefusal::Busy => FoundationFlagsRefusal::Busy,
                        FlagsBeginRefusal::WrongPhase => FoundationFlagsRefusal::WrongPhase,
                        FlagsBeginRefusal::InvalidAttribution => {
                            FoundationFlagsRefusal::InvalidAttribution
                        }
                    }
                };
                FoundationFlagsError {
                    plan,
                    after,
                    refusal,
                }
            })
    }

    pub fn flags_pending(&self) -> bool {
        self.serving.owner.context.flags_pending()
    }

    /// Take the one completed observation. An absent, pending or failed owner
    /// yields no sample; failed cleanup retains the original account.
    pub fn take_observed_flags(&mut self) -> Option<FoundationFlags> {
        if self.stage == Stage::Stopped || self.serving.owner.context.failure().is_some() {
            return None;
        }
        self.serving.flags.take().map(|recorded| *recorded)
    }

    pub(super) fn advance_flags(
        &mut self,
        peer: Option<&mut LifecyclePeer>,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        if !self.flags_pending() {
            return Ok(());
        }
        let owner = &mut self.serving.owner;
        let mut ports = PumpPorts {
            lifecycle: LifecyclePorts {
                listener: Some(&mut owner.listener),
                dial: peer,
                residency: Some(owner.resident.residency),
                leave_peer: plan::retained_port(&mut self.serving.plan, &mut self.serving.save),
                ..Default::default()
            },
            gate: Some(&mut owner.gate),
            decode: Some(owner.decode),
            label: None,
            io,
        };
        loop {
            match owner.context.advance_read(&mut ports)? {
                ReadProgress::Complete => {
                    if self.serving.flags.is_some() {
                        return Err(owner.context.fail(
                            ports.io,
                            FlowHaltReason::ProtocolFailed,
                            HaltPoint::Author,
                            "flag completion found an untaken account",
                        ));
                    }
                    let recorded = owner
                        .context
                        .take_observed_flags(ports.io)?
                        .expect("completed flag owner");
                    let matches = recorded.belongs_to(owner.context);
                    self.serving.flags = Some(Box::new(FoundationFlags { recorded }));
                    if !matches {
                        return Err(owner.context.fail(
                            ports.io,
                            FlowHaltReason::ProtocolFailed,
                            HaltPoint::Author,
                            "flag completion changed owner",
                        ));
                    }
                    return Ok(());
                }
                ReadProgress::Progress => (),
                ReadProgress::Pending => {
                    owner.context.pump_once(&mut ports)?;
                    return Ok(());
                }
            }
        }
    }
}
