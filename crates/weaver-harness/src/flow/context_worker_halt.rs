//! Failed serving custody through Reporting and Interrupting. Closing, release
//! and final drain must consume this same owner; preparation is no exit proof.
use super::management::FoundationManagement;
use super::*;
use crate::flow::clock::WaitInterest;
use crate::flow::context::{
    HaltAccountFailure, HaltGateProgress, HaltTraceProgress, RecordingPhase,
};
use crate::flow::halt_snapshot::ExternalFacts;
use crate::flow::report::SecondaryFailure;
use nix::poll::PollFlags;

#[path = "context_worker_exit.rs"]
pub(crate) mod exit;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoundationHaltProgress {
    Pending,
    Progress,
    /// No execution, its exact unsent request withdrawn, or its terminal held.
    /// Owed ordinary trace records can remain retained for the next halt phase.
    /// This does not grant turn closure, release, successful Leave or worker exit.
    Prepared,
    /// Gate evidence is unavailable. Actual process/cgroup containment is owed.
    NeedsContainment,
    /// The original cap or clock failed. The next owner must exit failed.
    Expired,
}

enum Owner<'a> {
    Input(Box<FoundationInput<'a>>),
    Management(Box<FoundationManagement<'a>>),
}
impl<'a> Owner<'a> {
    fn input(&self) -> &FoundationInput<'a> {
        match self {
            Self::Input(input) => input,
            Self::Management(owner) => &owner.input,
        }
    }
    fn input_mut(&mut self) -> &mut FoundationInput<'a> {
        match self {
            Self::Input(input) => input,
            Self::Management(owner) => &mut owner.input,
        }
    }
}

/// Holds every original, descriptor, child and pending author across the halt.
/// No live serving seat can be extracted from this owner.
pub struct FoundationHalt<'a> {
    owner: Owner<'a>,
    progress: FoundationHaltProgress,
    deadline: Option<u64>,
    reporting: bool,
    account_failure: Option<HaltAccountFailure>,
    received: Vec<ReceivedOrgan>,
    rejection_unavailable: bool,
    capacity_unavailable: bool,
    detail: Option<String>,
}

fn can_begin(input: &FoundationInput<'_>) -> bool {
    let context = &input.serving.owner.context;
    context.phase == RecordingPhase::Failed
        && context.failure.is_some()
        && context.flow.halt().is_some()
        && context.member_capture.is_none()
        && context
            .flow
            .halt()
            .unwrap()
            .account()
            .is_none_or(|account| account.capture.is_none())
}
impl<'a> FoundationInput<'a> {
    /// Move the actual failed run; a healthy or capture-owned attempt is unchanged.
    pub fn halt(self: Box<Self>) -> Result<Box<FoundationHalt<'a>>, Box<Self>> {
        if !can_begin(&self) {
            return Err(self);
        }
        Ok(Box::new(FoundationHalt::new(Owner::Input(self))))
    }
}
impl<'a> FoundationManagement<'a> {
    /// The complete accepted peer, directive, descriptors and send remain held.
    pub fn halt(self: Box<Self>) -> Result<Box<FoundationHalt<'a>>, Box<Self>> {
        if !can_begin(&self.input) {
            return Err(self);
        }
        Ok(Box::new(FoundationHalt::new(Owner::Management(self))))
    }
}
impl<'a> FoundationHalt<'a> {
    fn new(owner: Owner<'a>) -> Self {
        let deadline = owner
            .input()
            .serving
            .owner
            .context
            .flow
            .halt()
            .unwrap()
            .deadline()
            .ok();
        Self {
            owner,
            progress: FoundationHaltProgress::Progress,
            deadline,
            reporting: true,
            account_failure: None,
            received: Vec::new(),
            rejection_unavailable: false,
            capacity_unavailable: false,
            detail: None,
        }
    }
    pub fn progress(&self) -> FoundationHaltProgress {
        self.progress
    }
    pub fn detail(&self) -> Option<&str> {
        self.detail.as_deref()
    }
    pub fn advance(&mut self) -> FoundationHaltProgress {
        self.advance_with_io(&mut MonotonicIo)
    }
    fn terminal(&self) -> bool {
        matches!(
            self.progress,
            FoundationHaltProgress::Prepared
                | FoundationHaltProgress::NeedsContainment
                | FoundationHaltProgress::Expired
        )
    }
    fn secondary(&mut self, detail: String) {
        let observed = self.owner.input().serving.owner.context.last_ns;
        self.secondary_at(Some(observed), detail);
    }
    fn secondary_at(&mut self, observed: Option<u64>, detail: String) {
        let context = &mut self.owner.input_mut().serving.owner.context;
        if !context
            .flow
            .halt()
            .unwrap()
            .secondary()
            .iter()
            .any(|prior| prior.detail == detail)
        {
            let _ = context
                .flow
                .halt_mut()
                .unwrap()
                .append_secondary(SecondaryFailure {
                    reason: FlowHaltReason::ProtocolFailed,
                    point: HaltPoint::Teardown,
                    observed_ns: observed.map(Into::into),
                    detail: detail.clone(),
                });
        }
        self.detail.get_or_insert(detail);
    }
    fn clock(&mut self, io: &mut impl PumpIo) -> bool {
        let context = &mut self.owner.input_mut().serving.owner.context;
        let result = io.now();
        let observed = result.as_ref().ok().copied();
        let detail = match result {
            Ok(now) if now >= context.last_ns => {
                context.last_ns = now;
                if self.deadline.is_some_and(|deadline| now < deadline) {
                    return true;
                }
                "failed serving reached its original halt deadline".to_owned()
            }
            Ok(_) => "failed serving halt clock reversed".to_owned(),
            Err(error) => format!("failed serving halt clock: {error}"),
        };
        self.secondary_at(observed, detail);
        self.progress = FoundationHaltProgress::Expired;
        false
    }

    pub(in crate::flow::context::worker_startup) fn advance_with_io(
        &mut self,
        io: &mut impl PumpIo,
    ) -> FoundationHaltProgress {
        if self.terminal() || !self.clock(io) {
            return self.progress;
        }
        self.progress = FoundationHaltProgress::Pending;
        if self.reporting {
            let context = &mut self.owner.input_mut().serving.owner.context;
            if context.flow.halt().unwrap().account().is_none() {
                let facts = ExternalFacts {
                    outstanding_pairs: context.outstanding_pairs(),
                    capture: None,
                    wait: context.halt_wait().cloned(),
                    input_id: context.input.snapshot().map(|input| input.input_id),
                };
                if let Err(error) = context.freeze_halt_account(facts) {
                    let detail = format!("failed serving account: {error:?}");
                    // The failed snapshot itself is the evidence for skipping
                    // this report. Keep it while mandatory interruption proceeds.
                    context
                        .unavailable_halt_report(&error)
                        .expect("failed context without a report");
                    self.account_failure = Some(error);
                    self.secondary(detail);
                }
            }
            let context = &mut self.owner.input_mut().serving.owner.context;
            if context.halt_report.is_none() {
                context
                    .begin_halt_report()
                    .expect("failed context with frozen account");
            }
            if matches!(
                context.advance_halt_trace(io),
                Ok(HaltTraceProgress::AbortRequired)
            ) {
                self.progress = FoundationHaltProgress::Expired;
                return self.progress;
            }
            let owner = &mut self.owner.input_mut().serving.owner;
            if owner.context.pairs.halt_execution().is_none() {
                self.progress = FoundationHaltProgress::Prepared;
            } else if owner.context.halt_gate.is_none() {
                match owner.context.begin_halt_gate(&owner.gate) {
                    Ok(()) => self.progress = FoundationHaltProgress::Progress,
                    Err(error) => {
                        self.secondary(format!("failed serving interrupt admission: {error:?}"));
                        self.progress = FoundationHaltProgress::NeedsContainment;
                    }
                }
            } else {
                self.progress = FoundationHaltProgress::Progress;
            }
            self.reporting = false;
        } else {
            // Existing ordinary debt cannot delay the mandatory interrupt.
            let owner = &mut self.owner.input_mut().serving.owner;
            let gate = owner.context.advance_halt_gate(&mut owner.gate, io);
            match gate {
                Ok(HaltGateProgress::Terminal | HaltGateProgress::Withdrawn) => {
                    self.progress = FoundationHaltProgress::Prepared
                }
                Ok(HaltGateProgress::AbortRequired) => {
                    self.progress = FoundationHaltProgress::Expired
                }
                Ok(HaltGateProgress::Unavailable) => {
                    if let Some(detail) = self
                        .owner
                        .input()
                        .serving
                        .owner
                        .context
                        .halt_gate()
                        .and_then(|gate| gate.unavailable())
                    {
                        self.secondary(detail.to_owned());
                    }
                    self.progress = FoundationHaltProgress::NeedsContainment
                }
                Ok(HaltGateProgress::InterruptSent) => {
                    self.progress = FoundationHaltProgress::Progress
                }
                Ok(HaltGateProgress::Pending) => {}
                Err(error) => {
                    self.secondary(format!("failed serving interrupt: {error:?}"));
                    self.progress = FoundationHaltProgress::NeedsContainment;
                }
            }
            if !self.terminal() {
                self.drain_other();
                let context = &mut self.owner.input_mut().serving.owner.context;
                match context.advance_halt_trace(io) {
                    Ok(HaltTraceProgress::TraceRecorded(_)) => {
                        self.progress = FoundationHaltProgress::Progress
                    }
                    Ok(HaltTraceProgress::AbortRequired) => {
                        self.progress = FoundationHaltProgress::Expired
                    }
                    _ => {}
                }
                if !self.terminal() && !self.rejection_unavailable {
                    match self
                        .owner
                        .input_mut()
                        .serving
                        .owner
                        .context
                        .advance_halt_rejection()
                    {
                        Ok(Some(_)) => self.progress = FoundationHaltProgress::Progress,
                        Ok(None) => {}
                        Err(detail) => {
                            self.rejection_unavailable = true;
                            self.secondary(detail);
                        }
                    }
                }
            }
        }
        self.clock(io);
        self.progress
    }

    fn drain_other(&mut self) {
        let input = self.owner.input_mut();
        let owner = &mut input.serving.owner;
        let take = owner.gate.held().is_some_and(|frame| {
            !matches!(frame.payload, Payload::ToolAnswer(_))
                && owner
                    .context
                    .halt_gate()
                    .is_some_and(|halt| *halt.exchange() != frame.exchange)
        });
        if !take {
            return;
        }
        let frame = owner.gate.held().unwrap();
        let valid_world = frame.exchange.opener == Opener::Gate
            && frame.position == Position::Open
            && matches!(frame.payload, Payload::Frame(_))
            && !input
                .serving
                .gate_exchanges
                .contains(&frame.exchange.ordinal)
            && !input
                .serving
                .queued_inputs
                .iter()
                .any(|queued| queued.envelope().exchange == frame.exchange)
            && !self
                .received
                .iter()
                .any(|prior| prior.envelope().exchange == frame.exchange);
        if self.received.try_reserve(1).is_err() {
            self.secondary("failed serving could not retain the held gate packet".into());
            self.progress = FoundationHaltProgress::NeedsContainment;
            return;
        }
        let owner = &mut self.owner.input_mut().serving.owner;
        match owner.gate.take_received() {
            Ok(Some(original)) => {
                self.received.push(original);
                self.progress = FoundationHaltProgress::Progress;
                if !valid_world {
                    // A Fault keeps its existing indeterminate gate outcome;
                    // another control cannot stand in for a tool terminal.
                    self.secondary(
                        "failed serving received a gate fault or unexpected control".into(),
                    );
                    self.progress = FoundationHaltProgress::NeedsContainment;
                }
            }
            Ok(None) => {}
            Err(error) => {
                self.secondary(format!("failed serving held packet: {error:?}"));
                self.progress = FoundationHaltProgress::NeedsContainment;
            }
        }
    }

    pub fn wait_for_progress(&mut self) -> FoundationHaltProgress {
        self.wait_with_io(&mut MonotonicIo)
    }
    pub(in crate::flow::context::worker_startup) fn wait_with_io(
        &mut self,
        io: &mut impl PumpIo,
    ) -> FoundationHaltProgress {
        if self.progress != FoundationHaltProgress::Pending || !self.clock(io) {
            return self.progress;
        }
        let owner = &mut self.owner.input_mut().serving.owner;
        let mut interests = Vec::new();
        let events = owner.gate.interest().events();
        if !events.is_empty() {
            interests.push(WaitInterest {
                fd: owner.gate.fd(),
                events,
            });
        }
        if !self.capacity_unavailable
            && owner.context.barrier.pending().is_some()
            && owner.context.barrier.standing_failure().is_none()
        {
            if let Err(error) = owner.context.barrier.drain_capacity_wake() {
                self.capacity_unavailable = true;
                self.secondary(format!("failed serving capacity wake: {error:?}"));
                // A failed notification cannot become a repeated immediate wake.
                // The next call still waits on the gate and original deadline.
                self.progress = FoundationHaltProgress::Progress;
                return self.progress;
            }
            interests.push(WaitInterest {
                fd: owner.context.barrier.capacity_fd(),
                events: PollFlags::POLLIN,
            });
        }
        let result = io.wait(&interests, self.deadline);
        if let Err(error) = result {
            self.secondary(format!("failed serving wait: {error}"));
            self.progress = FoundationHaltProgress::Expired;
        }
        self.clock(io);
        self.progress
    }

    #[cfg(test)]
    pub(in crate::flow::context::worker_startup) fn test_input(
        &mut self,
    ) -> &mut FoundationInput<'a> {
        self.owner.input_mut()
    }
    #[cfg(test)]
    pub(in crate::flow::context::worker_startup) fn test_received(&self) -> &[ReceivedOrgan] {
        &self.received
    }
    #[cfg(test)]
    pub(in crate::flow::context::worker_startup) fn test_management(
        &self,
    ) -> Option<&FoundationManagement<'a>> {
        match &self.owner {
            Owner::Input(_) => None,
            Owner::Management(owner) => Some(owner),
        }
    }
    #[cfg(test)]
    pub(in crate::flow::context::worker_startup) fn into_test_owner(
        self: Box<Self>,
    ) -> Result<Box<FoundationInput<'a>>, Box<FoundationManagement<'a>>> {
        match self.owner {
            Owner::Input(input) => Ok(input),
            Owner::Management(owner) => Err(owner),
        }
    }
}
