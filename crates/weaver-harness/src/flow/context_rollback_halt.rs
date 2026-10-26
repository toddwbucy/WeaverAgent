//! Accepted rollback failure -> bounded fault/owed measurement -> retired ports.
//! This private handoff retains the account; its outer owner completes terminal cleanup.
use super::*;
use crate::flow::coordinator::FailedFlowResources;
use crate::flow::halt_snapshot::ExternalFacts;
use crate::flow::pump::RetiredLifecyclePeer;
use weaver_types::state_flow::DecimalU64;

/// This is retained evidence, never a live read completion or permission.
pub(in crate::flow) struct FailedRollbackRead {
    pub(in crate::flow) bound: AbsoluteBound,
    pub(in crate::flow) request_id: Option<DecimalU64>,
    pub(in crate::flow) ticket: Option<u64>,
    pub(in crate::flow) trace_sequence: Option<Sequence>,
    pub(in crate::flow) unavailable: Option<String>,
    attempted: bool,
}

/// Every original owner remains attached to the same borrowed run. Transport
/// evidence is descriptorless, including unsent responses and held admin input.
/// There is no mutable context escape, successful Left, or terminal-record API.
pub(in crate::flow) struct RetiredFailedRollback<'a> {
    refusal: RecordedStartupRefusal<'a>,
    enter_peer: RetiredLifecyclePeer,
    enter_send: Option<OrganSend>,
    enter_returned: Option<OrganEnvelope>,
    enter_stop: Option<ResponseStop>,
    _original_enter: Option<crate::flow::pump::enter::DetachedEnter>,
    peer: RetiredLifecyclePeer,
    dials: listener::RetiredRollbackDials,
    directive: LifecycleMessage,
    leave: AbsoluteBound,
    current: Option<InspectionCompletion>,
    stop: Option<RollbackStop>,
    flow: FailedFlowResources,
    read: FailedRollbackRead,
    deadline: u64,
    report_unavailable: Option<String>,
    account_failure: Option<HaltAccountFailure>,
    expired: bool,
}

impl<'a> RollbackComposition<'a> {
    /// Only an actually failed, accepted rollback with its retained current
    /// Grants owner can use this road. Refusal returns the whole chain unchanged.
    /// All work is bounded local work; no live pump, receive, resend, new state
    /// operation, wait for ordinary capacity, or healthy release transition runs.
    #[allow(clippy::result_large_err)]
    pub(in crate::flow) fn into_failed_retirement(
        mut self,
        io: &mut impl PumpIo,
    ) -> Result<RetiredFailedRollback<'a>, Self> {
        let context = &mut self.response.refusal.owner.startup.context;
        if self.stage != RollbackStage::Stopped
            || context.phase != RecordingPhase::Failed
            || context.failure.is_none()
            || !context.flow.preinput_failed_owner()
            || context.deadlines.get(PumpDeadline::Leave) != Some(self.leave)
            || !self.inspecting
            || self.current.is_some()
            || context.halt_report.is_some()
            || self
                .response
                .refusal
                .owner
                .startup
                .resources
                .spu_child()
                .is_some()
        {
            return Err(self);
        }
        let Some(halt_deadline) = context.flow.halt().and_then(|halt| halt.deadline().ok()) else {
            return Err(self);
        };
        let Ok(mut read) = FailedRollbackRead::capture(context, self.leave) else {
            return Err(self);
        };
        let deadline = halt_deadline.min(self.leave.deadline_ns());
        let mut expired = !before_cap(context, io, deadline);
        let mut report_unavailable = None;
        let mut account_failure = None;
        if !expired {
            let facts = ExternalFacts {
                outstanding_pairs: context.outstanding_pairs(),
                capture: None,
                wait: context.halt_wait().cloned(),
                input_id: context.input.snapshot().map(|input| input.input_id),
            };
            if context.flow.halt().unwrap().account().is_none()
                && let Err(error) = context.freeze_halt_account(facts)
            {
                report_unavailable = Some(format!("rollback halt snapshot: {error:?}"));
                account_failure = Some(error);
            }
            if context.flow.halt().unwrap().account().is_some() {
                match context.begin_halt_report() {
                    Ok(()) => {
                        // Exactly the first terminal attempt, before any owed
                        // ordinary measurement. Never run the generic drain here.
                        match context.advance_halt_trace(io) {
                            Ok(HaltTraceProgress::AbortRequired) => expired = true,
                            Ok(_) => {}
                            Err(error) => {
                                report_unavailable = Some(format!("rollback fault: {error:?}"))
                            }
                        }
                    }
                    Err(error) => {
                        report_unavailable = Some(format!("rollback fault owner: {error:?}"))
                    }
                }
            }
        }
        if !expired {
            expired = !before_cap(context, io, deadline);
        }
        if !expired {
            expired = !read.record_once(context, io, deadline);
        } else if read.ticket.is_some() {
            read.unavailable =
                Some("owed Grants measurement reached its original bound or clock failed".into());
        }
        // Frozen request, partial bytes, held answer, pending author and original
        // inspection all survive. Retirement cannot mint InspectionEvidence.
        let flow = context
            .flow
            .retire_failed_ports()
            .expect("validated failed pre-input owner");
        if expired {
            context.barrier.abandon();
        }
        self.response
            .refusal
            .owner
            .startup
            .resources
            .retire_rollback_child(expired);
        let Self {
            response,
            peer,
            dials,
            directive,
            leave,
            current,
            stop,
            ..
        } = self;
        let EnterRefusalResponse {
            refusal,
            peer: enter_peer,
            send,
            returned,
            stop: enter_stop,
            original,
            ..
        } = response;
        Ok(RetiredFailedRollback {
            refusal,
            enter_peer: enter_peer.retire_failed(),
            enter_send: send,
            enter_returned: returned,
            enter_stop,
            _original_enter: original,
            peer: peer.retire_failed(),
            dials: dials.retire_failed(),
            directive,
            leave,
            current,
            stop,
            flow,
            read,
            deadline,
            report_unavailable,
            account_failure,
            expired,
        })
    }
}

impl FailedRollbackRead {
    pub(in crate::flow::context) fn capture(
        context: &RecordingContext,
        leave: AbsoluteBound,
    ) -> Result<Self, &'static str> {
        let (bound, request_id, ticket) = context.stopped_rollback_grants(leave)?;
        Ok(Self {
            bound,
            request_id,
            ticket,
            trace_sequence: None,
            attempted: false,
            unavailable: Some("current Grants stopped before a live completion".into()),
        })
    }
    /// One nonblocking trace-only attempt. No new control, live completion or
    /// ordinary-capacity wait may precede mandatory terminal work.
    pub(in crate::flow::context) fn record_once(
        &mut self,
        context: &mut RecordingContext,
        io: &mut impl PumpIo,
        deadline: u64,
    ) -> bool {
        if self.attempted {
            return true;
        }
        self.attempted = true;
        if !before_cap(context, io, deadline) {
            self.unavailable =
                Some("owed Grants measurement reached its original bound or clock failed".into());
            return false;
        }
        let Some(ticket) = self.ticket else {
            return true;
        };
        if self.bound.expired(context.last_ns) {
            self.unavailable =
                Some("owed Grants measurement reached its original bound or clock failed".into());
        } else if !context
            .halt_report
            .as_ref()
            .is_some_and(HaltReport::attempted)
        {
            self.unavailable = Some("first fault attempt unavailable; measurement retained".into());
        } else if context.barrier.pending_author().map(|author| author.ticket) != Some(ticket) {
            self.unavailable = Some("owed Grants measurement lost its exact ticket".into());
        } else {
            match context.resume_trace_only_and_bind() {
                Ok(Some(sequence))
                    if context.barrier.recorded_author_ticket(sequence) == Some(ticket) =>
                {
                    self.trace_sequence = Some(sequence);
                }
                Ok(None) => {
                    self.unavailable =
                        Some("owed Grants measurement has no ordinary trace capacity".into())
                }
                other => {
                    self.unavailable =
                        Some(format!("owed Grants measurement unavailable: {other:?}"))
                }
            }
        }
        before_cap(context, io, deadline)
    }
}

fn before_cap(context: &mut RecordingContext, io: &mut impl PumpIo, deadline: u64) -> bool {
    let observed = io.now();
    match observed {
        Ok(now) if now >= context.last_ns && now < deadline => {
            context.last_ns = now;
            true
        }
        other => {
            let _ = context.flow.halt_mut().unwrap().append_secondary(SecondaryFailure {
                reason: FlowHaltReason::TraceFailed, point: HaltPoint::Teardown,
                observed_ns: other.as_ref().ok().copied().map(Into::into),
                detail: format!("rollback halt clock unavailable or reached original cap {deadline}: {other:?}"),
            });
            false
        }
    }
}

impl RetiredFailedRollback<'_> {
    pub(in crate::flow) fn grant_surface(&self) -> GrantSurface {
        GrantSurface::Unreadable
    }
    pub(in crate::flow) fn failure(&self) -> &RunFailure {
        self.refusal
            .owner
            .startup
            .context
            .failure()
            .expect("first failure remains latched")
    }
    pub(in crate::flow) fn read(&self) -> &FailedRollbackRead {
        &self.read
    }
    pub(in crate::flow) fn deadline(&self) -> u64 {
        self.deadline
    }
    pub(in crate::flow) fn leave(&self) -> AbsoluteBound {
        self.leave
    }
    pub(in crate::flow) fn directive(&self) -> &LifecycleMessage {
        &self.directive
    }
    pub(in crate::flow) fn expired(&self) -> bool {
        self.expired
    }
    pub(in crate::flow) fn fault_sequence(&self) -> Option<Sequence> {
        self.refusal
            .owner
            .startup
            .context
            .halt_report()
            .and_then(HaltReport::fault_sequence)
    }
}

#[cfg(test)]
#[path = "context_rollback_halt_tests.rs"]
pub(super) mod tests;
