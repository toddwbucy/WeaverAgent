//! Consuming Failed-only pre-input cleanup. The live context cannot escape this
//! owner while teardown progresses, and every terminal result remains failed.

use super::*;
use crate::flow::author::AuthoredSource;
use crate::flow::clock::WaitInterest;
use crate::flow::coordinator::FailedFlowResources;
use crate::flow::halt_snapshot::ExternalFacts;
use crate::flow::pump_transport::{
    DecodePort, OrganPort, OrganSend, PacketPort, PortIdentity, ReceivedOrgan, TrackedStage,
};
use nix::poll::PollFlags;
use std::path::Path;
use std::sync::Arc;
use weaver_trace::{GrantSurface, MemberRelease, Release, ReleaseOutcome, Subsystem};
use weaver_types::{
    ExchangeId, LifecycleAnswer, LifecycleDirective, Opener, OrganEnvelope, Position,
};

#[path = "failed_enter_process.rs"]
mod process;
pub(in crate::flow) use process::ChildState;
use process::OwnedChild;

/// Created by this context, then populated only by actual fork producers.
/// There is no PID/descriptor adoption constructor for a fabricated child.
pub(in crate::flow) struct FailedEnterResources {
    origin: Arc<()>,
    spu: Option<SpuResource>,
    classifier: Option<OwnedChild>,
}

struct SpuResource {
    child: OwnedChild,
    decode_identity: PortIdentity,
    residency_identity: PortIdentity,
    decode: Option<DecodePort>,
    residency: Option<OrganPort>,
    next_ordinal: Option<u64>,
    retired_decode: Option<PacketPort<crate::channel::DecodeReply, ()>>,
    retired_residency: Option<PacketPort<OrganEnvelope, ()>>,
}

impl FailedEnterResources {
    #[cfg(test)]
    pub(super) fn poll_startup_test_children(&mut self) -> bool {
        self.poll_children()
    }

    #[cfg(test)]
    pub(super) fn reap_startup_test_children(&mut self) {
        self.close_at_cap();
        for _ in 0..3000 {
            if self.poll_children() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        panic!("startup fixture child was not reaped");
    }

    pub(super) fn matches(&self, context: &RecordingContext) -> bool {
        Arc::ptr_eq(&self.origin, &context.read_origin)
    }

    /// # Safety
    /// The caller satisfies spawn::fork_organ's serving-thread lifetime rule.
    pub(in crate::flow) unsafe fn spawn_spu(
        &mut self,
        context: &mut RecordingContext,
        binary: &Path,
        arguments: &[String],
        io: &mut impl PumpIo,
    ) -> Result<(), String> {
        if !self.matches(context)
            || self.spu.is_some()
            || context.phase != RecordingPhase::Entered
            || context.failure.is_some()
            || context.load_duty.is_none()
        {
            return Err("SPU provisioning requires this live loaded context".into());
        }
        context
            .live(io)
            .map_err(|error| format!("SPU before fork: {error:?}"))?;
        let (residency, child_residency) = crate::channel::OrganChannel::pair()
            .map_err(|error| format!("SPU residency pair: {error:?}"))?;
        let (decode, child_decode) = crate::channel::DecodeChannel::pair()
            .map_err(|error| format!("SPU decode pair: {error:?}"))?;
        let residency = OrganPort::from_channel(residency)
            .map_err(|error| format!("SPU residency port: {error:?}"))?;
        let decode = DecodePort::from_channel(decode)
            .map_err(|error| format!("SPU decode port: {error:?}"))?;
        // SAFETY: forwarded from this method's serving-thread contract.
        let child = unsafe {
            OwnedChild::spawn(
                binary,
                &[child_residency.as_fd(), child_decode.as_fd()],
                arguments,
            )
        }?;
        self.spu = Some(SpuResource {
            child,
            decode_identity: decode.port_identity(),
            residency_identity: residency.port_identity(),
            decode: Some(decode),
            residency: Some(residency),
            next_ordinal: Some(1),
            retired_decode: None,
            retired_residency: None,
        });
        drop((child_residency, child_decode));
        Ok(())
    }

    /// Launch and attach the exact matching classifier endpoint in one call.
    /// A failed bootstrap still leaves its actual child in this resource owner.
    /// # Safety
    /// The caller satisfies spawn::fork_organ's serving-thread lifetime rule.
    pub(in crate::flow) unsafe fn spawn_classifier(
        &mut self,
        context: &mut RecordingContext,
        binary: &Path,
        bootstrap: Ste100Bootstrap,
        io: &mut impl PumpIo,
    ) -> Result<(), String> {
        if !self.matches(context)
            || self.classifier.is_some()
            || context.phase != RecordingPhase::Entered
            || context.failure.is_some()
            || context.load_duty.is_none()
            || context.flow.standing_classifier().is_some()
            || context.flow.starting_classifier().is_some()
        {
            return Err("classifier provisioning requires this live loaded context".into());
        }
        context
            .live(io)
            .map_err(|error| format!("classifier before fork: {error:?}"))?;
        let (near, far) = crate::channel::ClassifyChannel::pair()
            .map_err(|error| format!("classifier pair: {error:?}"))?;
        let near = near
            .into_pump_fd()
            .map_err(|_| "classifier pair is not fresh".to_owned())?;
        let arguments = vec![
            "--max-wire-bytes".into(),
            bootstrap.instruction.max_wire_bytes.0.to_string(),
        ];
        // SAFETY: forwarded from this method's serving-thread contract.
        let child = unsafe { OwnedChild::spawn(binary, &[far.as_fd()], &arguments) }?;
        self.classifier = Some(child);
        drop(far);
        context
            .begin_classifier(near, bootstrap, io)
            .map_err(|error| format!("classifier bootstrap: {error:?}"))
    }

    pub(super) fn spu_ports_mut(&mut self) -> Option<(&mut OrganPort, &mut DecodePort)> {
        let spu = self.spu.as_mut()?;
        Some((spu.residency.as_mut()?, spu.decode.as_mut()?))
    }

    pub(in crate::flow) fn decode_mut(&mut self) -> Option<&mut DecodePort> {
        self.spu.as_mut()?.decode.as_mut()
    }

    /// The resource owner assigns each actual residency exchange exactly once.
    /// Busy/refusal returns the original directive and does not spend its ordinal.
    #[allow(clippy::result_large_err)] // Refusal returns the exact move-owned directive.
    pub(in crate::flow) fn stage_residency(
        &mut self,
        directive: LifecycleDirective,
    ) -> Result<OrganSend, (LifecycleDirective, String)> {
        let Some(spu) = self.spu.as_mut() else {
            return Err((directive, "no SPU".into()));
        };
        let Some(ordinal) = spu.next_ordinal else {
            return Err((directive, "residency ordinal exhausted".into()));
        };
        let Some(port) = spu.residency.as_mut() else {
            return Err((directive, "residency retired".into()));
        };
        if !spu.residency_identity.matches(port) {
            return Err((directive, "foreign residency endpoint".into()));
        }
        let envelope = OrganEnvelope {
            exchange: ExchangeId {
                opener: Opener::Harness,
                ordinal,
            },
            position: Position::Open,
            payload: weaver_types::Payload::Directive(directive),
        };
        match port.stage_tracked(envelope) {
            Ok(TrackedStage::Queued(send)) => {
                spu.next_ordinal = ordinal.checked_add(1);
                Ok(send)
            }
            Ok(TrackedStage::Busy(envelope)) => {
                let weaver_types::Payload::Directive(directive) = envelope.payload else {
                    unreachable!()
                };
                Err((directive, "residency has an original pending send".into()))
            }
            Err(error) => {
                let weaver_types::Payload::Directive(directive) = error.input.payload else {
                    unreachable!()
                };
                Err((directive, format!("residency stage: {:?}", error.failure)))
            }
        }
    }

    pub(in crate::flow) fn residency_mut(&mut self) -> Option<&mut OrganPort> {
        self.spu.as_mut()?.residency.as_mut()
    }

    fn original_ports(&self) -> bool {
        self.spu.as_ref().is_none_or(|spu| {
            spu.decode
                .as_ref()
                .is_some_and(|port| spu.decode_identity.matches_decode(port))
                && spu
                    .residency
                    .as_ref()
                    .is_some_and(|port| spu.residency_identity.matches(port))
        })
    }

    pub(in crate::flow) fn has_unreaped_child(&self) -> bool {
        self.classifier
            .as_ref()
            .is_some_and(|child| !matches!(child.state(), ChildState::Reaped(_)))
            || self
                .spu
                .as_ref()
                .is_some_and(|spu| !matches!(spu.child.state(), ChildState::Reaped(_)))
    }
    pub(in crate::flow) fn classifier_child(&self) -> Option<ChildState> {
        self.classifier.as_ref().map(OwnedChild::state)
    }
    pub(in crate::flow) fn spu_child(&self) -> Option<ChildState> {
        self.spu.as_ref().map(|spu| spu.child.state())
    }
    fn child_failures(&self) -> [Option<String>; 2] {
        [
            self.spu.as_ref().map(|spu| &spu.child),
            self.classifier.as_ref(),
        ]
        .map(|child| {
            child.and_then(|child| {
                child
                    .wait_error()
                    .map(|error| format!("child wait unavailable: {error}"))
                    .or_else(|| {
                        child
                            .kill_attempt()
                            .and_then(Result::err)
                            .map(|error| format!("child kill unavailable: {error}"))
                    })
            })
        })
    }

    fn close_at_cap(&mut self) {
        if let Some(spu) = &mut self.spu {
            if let Some(port) = spu.decode.take() {
                spu.retired_decode = Some(port.retire_failed());
            }
            if let Some(port) = spu.residency.take() {
                spu.retired_residency = Some(port.retire_failed());
            }
            spu.child.stop_at_cap();
        }
        if let Some(child) = &mut self.classifier {
            child.stop_at_cap();
        }
    }

    fn poll_children(&mut self) -> bool {
        if let Some(spu) = &mut self.spu {
            spu.child.poll();
        }
        if let Some(child) = &mut self.classifier {
            child.poll();
        }
        self.classifier
            .as_ref()
            .is_none_or(|child| child.state() != ChildState::Running)
            && self
                .spu
                .as_ref()
                .is_none_or(|spu| spu.child.state() != ChildState::Running)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum FailedEnterRefusal {
    NotFailed,
    ForeignResources,
    LiveDuty,
    BootProofExtracted,
    ForeignDecode,
    ForeignTransport,
}

pub(in crate::flow) struct FailedEnterRejected {
    pub context: RecordingContext,
    pub resources: FailedEnterResources,
    pub refusal: FailedEnterRefusal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum FailedEnterProgress {
    Pending,
    Progress,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Reporting,
    Accounts,
    Release,
    Retire,
    Reap,
    Unload,
    Drain,
    Closed,
}

pub(in crate::flow) struct FailedEnterCleanup {
    context: RecordingContext,
    resources: FailedEnterResources,
    stage: Stage,
    deadline: Option<u64>,
    report_unavailable: Option<String>,
    account_failure: Option<HaltAccountFailure>,
    retired: Option<FailedFlowResources>,
    summary_account: Option<PlanSummaryAccount>,
    release: ReleaseOutcome,
    release_send: Option<OrganSend>,
    release_exchange: Option<ExchangeId>,
    release_reply: Option<ReceivedOrgan>,
    release_failure: Option<String>,
    refused_release: Option<LifecycleDirective>,
    unload: Option<AuthoredEvent>,
    unload_sequence: Option<Sequence>,
    unload_attempted: bool,
    unload_unavailable: Option<String>,
    abandoned: bool,
    member_stood: bool,
    member_failed: bool,
}

impl RecordingContext {
    /// One run issues one move-owned resource carrier. Reissuing an empty bag
    /// would omit children previously provisioned through the first carrier.
    pub(in crate::flow) fn failed_enter_resources(&mut self) -> Option<FailedEnterResources> {
        if self.failed_enter_resources_issued || self.phase == RecordingPhase::Closed {
            return None;
        }
        self.failed_enter_resources_issued = true;
        Some(FailedEnterResources {
            origin: Arc::clone(&self.read_origin),
            spu: None,
            classifier: None,
        })
    }

    fn preinput_cleanup_domain(&self) -> bool {
        self.flow.preinput_failed_owner()
            && self.enter_refusal_record.is_none()
            && self.held_author.is_none()
            && self.template_blocked_author.is_none()
            && self.waiting_content.is_none()
            && !self.author_admission_pending
            && !self.ever_started_turn
            && self.open_turn.is_none()
            && !self.place_leaving
            && self
                .boot_pairs
                .as_ref()
                .is_none_or(|owner| owner.actual_boot_owner(self))
            && self.input.snapshot().is_none()
            && self.input_author.is_none()
            && self.input_turn.is_none()
            && self.input_turns.is_empty()
            && self.member_capture.is_none()
            && self.freshness.is_none()
            && self.place_engine.is_none()
            && self.place.is_none()
            && self.generation.is_none()
            && self.reload.is_none()
            && self.native_marks.is_none()
            && self.place_notice.is_none()
            && self.place_close.is_none()
            && self.place_instructions.is_empty()
            && self.place_stopped_measure.is_none()
            && self.measure.is_none()
            && self.inspection.is_none()
            && self.marks.is_none()
            && self.output.is_none()
            && self.pair_work.is_none()
            && self.pairs.halt_execution().is_none()
            && self.outstanding_pairs().is_empty()
            && self.repair.is_none()
            && self.halt_gate.is_none()
            && self.plan_summary_is_boot_cleanup()
            && self
                .read
                .as_ref()
                .is_none_or(|read| read.is_boot_cleanup_read(self))
            && self
                .filing
                .as_ref()
                .is_none_or(|filing| filing.is_boot_cleanup_filing() && self.boot_pairs.is_some())
            && self
                .author_turn
                .as_ref()
                .is_none_or(|(kind, _)| *kind != Kind::TurnStarted)
    }

    /// Consumes all custody. Refusal returns the exact context and resources;
    /// no owner bit, deadline, child or transport is changed on this road.
    #[allow(clippy::result_large_err)]
    pub(in crate::flow) fn into_failed_enter_cleanup(
        self,
        resources: FailedEnterResources,
    ) -> Result<FailedEnterCleanup, FailedEnterRejected> {
        let refusal = if self.phase != RecordingPhase::Failed
            || self.failure.is_none()
            || self.flow.halt().is_none()
        {
            Some(FailedEnterRefusal::NotFailed)
        } else if !resources.matches(&self) {
            Some(FailedEnterRefusal::ForeignResources)
        } else if !resources.original_ports() {
            Some(FailedEnterRefusal::ForeignTransport)
        } else if !self.preinput_cleanup_domain() {
            Some(FailedEnterRefusal::LiveDuty)
        } else if self.boot_summary_proof_extracted()
            || (self.boot_open_grant_transferred()
                && self.boot_pairs.is_none()
                && self.plan_summary.is_none())
        {
            Some(FailedEnterRefusal::BootProofExtracted)
        } else if resources
            .spu
            .as_ref()
            .and_then(|spu| spu.decode.as_ref())
            .is_some_and(|decode| {
                self.check_boot_open_port(decode).is_err()
                    || self.check_measure_port(decode).is_err()
            })
            || (self.boot_open.is_some() && resources.decode_present().is_none())
        {
            Some(FailedEnterRefusal::ForeignDecode)
        } else {
            None
        };
        if let Some(refusal) = refusal {
            return Err(FailedEnterRejected {
                context: self,
                resources,
                refusal,
            });
        }
        let deadline = self.flow.halt().and_then(|halt| halt.deadline().ok());
        let member_stood = self.flow.failed_cleanup_state_stood();
        let member_failed = self.flow.failed_cleanup_state_failed();
        Ok(FailedEnterCleanup {
            context: self,
            resources,
            stage: Stage::Reporting,
            deadline,
            report_unavailable: None,
            account_failure: None,
            retired: None,
            summary_account: None,
            release: ReleaseOutcome::Unconfirmed,
            release_send: None,
            release_exchange: None,
            release_reply: None,
            release_failure: None,
            refused_release: None,
            unload: None,
            unload_sequence: None,
            unload_attempted: false,
            unload_unavailable: None,
            abandoned: false,
            member_stood,
            member_failed,
        })
    }
}

impl FailedEnterResources {
    fn decode_present(&self) -> Option<&DecodePort> {
        self.spu.as_ref()?.decode.as_ref()
    }
}

struct AccountClock<'a, I> {
    io: &'a mut I,
    last: u64,
    deadline: Option<u64>,
    unavailable: bool,
}
impl<I: PumpIo> PumpIo for AccountClock<'_, I> {
    fn now(&mut self) -> std::io::Result<u64> {
        match self.io.now() {
            Ok(now) => {
                if now < self.last || self.deadline.is_none_or(|bound| now >= bound) {
                    self.unavailable = true;
                }
                self.last = now;
                Ok(now) // Preserve the observed value for the original owner.
            }
            Err(error) => {
                self.unavailable = true;
                Err(error)
            }
        }
    }
    fn wait(
        &mut self,
        interests: &[WaitInterest<'_>],
        deadline: Option<u64>,
    ) -> std::io::Result<crate::flow::clock::WaitOutcome> {
        let bound = match (deadline, self.deadline) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (_, inherited) => inherited,
        };
        let result = self.io.wait(interests, bound);
        if result.is_err() {
            self.unavailable = true;
        }
        result
    }
}

impl FailedEnterCleanup {
    pub(in crate::flow) fn failure(&self) -> &RunFailure {
        self.context.failure.as_ref().unwrap()
    }
    pub(in crate::flow) fn deadline(&self) -> Option<u64> {
        self.deadline
    }
    pub(in crate::flow) fn context(&self) -> &RecordingContext {
        &self.context
    }
    pub(in crate::flow) fn unload_sequence(&self) -> Option<Sequence> {
        self.unload_sequence
    }
    pub(in crate::flow) fn summary_account(&self) -> Option<&PlanSummaryAccount> {
        self.summary_account.as_ref()
    }
    pub(in crate::flow) fn release_outcome(&self) -> ReleaseOutcome {
        self.release
    }
    #[cfg(test)]
    pub(in crate::flow) fn retired_summary_transport_for_test(&self) -> bool {
        self.retired
            .as_ref()
            .is_some_and(FailedFlowResources::has_stopped_plan_read)
    }
    #[cfg(test)]
    pub(in crate::flow) fn waiting_boot_decode_for_test(&self) -> bool {
        self.stage == Stage::Accounts
            && self.release_send.is_none()
            && self
                .resources
                .spu
                .as_ref()
                .is_some_and(|spu| spu.decode.is_some())
    }
    pub(in crate::flow) fn abandoned(&self) -> bool {
        self.abandoned
    }
    pub(in crate::flow) fn resources(&self) -> &FailedEnterResources {
        &self.resources
    }

    fn secondary(&mut self, detail: impl Into<String>) {
        let _ = self
            .context
            .flow
            .halt_mut()
            .unwrap()
            .append_secondary(SecondaryFailure {
                reason: FlowHaltReason::ProtocolFailed,
                point: HaltPoint::Teardown,
                observed_ns: Some(self.context.last_ns.into()),
                detail: detail.into(),
            });
    }

    fn clock(&mut self, io: &mut impl PumpIo) -> bool {
        if let Some(leave) = self.context.deadlines.get(PumpDeadline::Leave) {
            self.deadline = self.deadline.map(|bound| bound.min(leave.deadline_ns()));
        }
        match io.now() {
            Ok(now) if now >= self.context.last_ns => {
                self.context.last_ns = now;
                if self.deadline.is_some_and(|deadline| now < deadline) {
                    return true;
                }
                self.secondary(
                    "failed Enter cleanup reached its original cap or has no clock anchor",
                );
            }
            Ok(_) => self.secondary("failed Enter cleanup clock reversed"),
            Err(error) => self.secondary(format!("failed Enter cleanup clock: {error}")),
        }
        false
    }

    fn abort(&mut self) {
        // At expiry no account or writer may keep a descriptor or blocking wait.
        // Original unfinished owners remain in context, never relabelled settled.
        self.retire_flow();
        self.resources.close_at_cap();
        for detail in self.resources.child_failures().into_iter().flatten() {
            self.secondary(detail);
        }
        self.context.barrier.abandon();
        self.abandoned = true;
        self.context.phase = RecordingPhase::Closed;
        self.stage = Stage::Closed;
    }

    fn retire_flow(&mut self) {
        if self.retired.is_none() {
            self.retired = self.context.flow.retire_failed_ports();
        }
    }

    /// Each call performs bounded local work only. The caller chooses when to
    /// wait through wait_for_progress; it never re-enters an ordinary pump.
    pub(in crate::flow) fn advance(&mut self, io: &mut impl PumpIo) -> FailedEnterProgress {
        if self.stage == Stage::Closed {
            return FailedEnterProgress::Closed;
        }
        if !self.clock(io) {
            self.abort();
            return FailedEnterProgress::Closed;
        }
        let result = match self.stage {
            Stage::Reporting => {
                if self.context.flow.halt().unwrap().account().is_none() {
                    let facts = ExternalFacts {
                        outstanding_pairs: self.context.outstanding_pairs(),
                        capture: None,
                        wait: self.context.halt_wait().cloned(),
                        input_id: self.context.input.snapshot().map(|input| input.input_id),
                    };
                    if let Err(error) = self.context.freeze_halt_account(facts) {
                        self.report_unavailable =
                            Some(format!("failed Enter frozen account: {error:?}"));
                        self.account_failure = Some(error);
                    }
                }
                if self.context.flow.halt().unwrap().account().is_some() {
                    if self.context.halt_report.is_none()
                        && let Err(error) = self.context.begin_halt_report()
                    {
                        self.report_unavailable =
                            Some(format!("failed Enter fault owner: {error:?}"));
                    }
                    if self.context.halt_report.is_some()
                        && matches!(
                            self.context.advance_halt_trace(io),
                            Ok(HaltTraceProgress::AbortRequired)
                        )
                    {
                        self.abort();
                        return FailedEnterProgress::Closed;
                    }
                }
                self.stage = Stage::Accounts;
                FailedEnterProgress::Progress
            }
            Stage::Accounts => {
                if !self.advance_accounts(io) {
                    self.abort();
                    return FailedEnterProgress::Closed;
                }
                if !self.context.boot_open_account_settled() {
                    FailedEnterProgress::Pending
                } else {
                    if let Some(decode) = self
                        .resources
                        .spu
                        .as_mut()
                        .and_then(|spu| spu.decode.take())
                    {
                        // Same exact port/account condition as retire_boot_decode,
                        // while retaining its original framing evidence on close.
                        if self.context.check_boot_open_port(&decode).is_err()
                            || self.context.check_measure_port(&decode).is_err()
                            || self.context.place_owed()
                        {
                            self.resources.spu.as_mut().unwrap().decode = Some(decode);
                            self.release_failure = Some("exact decoder retirement refused".into());
                            return FailedEnterProgress::Pending;
                        }
                        self.resources.spu.as_mut().unwrap().retired_decode =
                            Some(decode.retire_failed());
                    }
                    self.stage = Stage::Release;
                    FailedEnterProgress::Progress
                }
            }
            Stage::Release => {
                if !self.advance_accounts(io) {
                    self.abort();
                    return FailedEnterProgress::Closed;
                }
                self.advance_release(io)
            }
            Stage::Retire => {
                if !self.advance_accounts(io) {
                    self.abort();
                    return FailedEnterProgress::Closed;
                }
                self.retire_flow();
                if let Some(spu) = &mut self.resources.spu
                    && let Some(port) = spu.residency.take()
                {
                    spu.retired_residency = Some(port.retire_failed());
                }
                self.stage = Stage::Reap;
                FailedEnterProgress::Progress
            }
            Stage::Reap => {
                if !self.advance_accounts(io) {
                    self.abort();
                    return FailedEnterProgress::Closed;
                }
                if self.resources.poll_children() {
                    for detail in self.resources.child_failures().into_iter().flatten() {
                        self.secondary(detail);
                    }
                    self.stage = Stage::Unload;
                    FailedEnterProgress::Progress
                } else {
                    FailedEnterProgress::Pending
                }
            }
            Stage::Unload => {
                if !self.advance_accounts(io) {
                    self.abort();
                    return FailedEnterProgress::Closed;
                }
                if self.context.plan_summary.is_some() {
                    FailedEnterProgress::Pending
                } else {
                    if self.attempt_unload(io) {
                        self.stage = Stage::Drain;
                        FailedEnterProgress::Progress
                    } else {
                        FailedEnterProgress::Closed
                    }
                }
            }
            Stage::Drain => match self.context.barrier.drain_status(&mut self.context.budget) {
                Ok(serving::DrainStatus::Emptied) => {
                    self.context.phase = RecordingPhase::Closed;
                    self.stage = Stage::Closed;
                    FailedEnterProgress::Closed
                }
                Ok(serving::DrainStatus::Pending { .. }) => FailedEnterProgress::Pending,
                Err(error) => {
                    self.secondary(format!("failed Enter trace drain: {error:?}"));
                    self.abort();
                    FailedEnterProgress::Closed
                }
            },
            Stage::Closed => FailedEnterProgress::Closed,
        };
        // Retain consumed records/replies before a broken post-work clock.
        if !self.clock(io) {
            self.abort();
            return FailedEnterProgress::Closed;
        }
        result
    }

    fn advance_accounts(&mut self, io: &mut impl PumpIo) -> bool {
        let mut observed = AccountClock {
            io,
            last: self.context.last_ns,
            deadline: self.deadline,
            unavailable: false,
        };
        if self.context.halt_report.is_some()
            && matches!(
                self.context.advance_halt_trace(&mut observed),
                Ok(HaltTraceProgress::AbortRequired)
            )
        {
            return false;
        }
        if observed.unavailable {
            return false;
        }
        if self.context.boot_open.is_some()
            && !self.context.boot_open_account_settled()
            && let Some(decode) = self
                .resources
                .spu
                .as_mut()
                .and_then(|spu| spu.decode.as_mut())
            && let Err(error) = self
                .context
                .advance_boot_open_account(decode, &mut observed)
        {
            self.secondary(format!("failed Enter Open account: {error:?}"));
        }
        if observed.unavailable {
            return false;
        }
        if self.context.plan_summary.is_some() {
            if let Err(error) = self.context.advance_plan_summary_account(&mut observed) {
                self.secondary(format!("failed Enter summary account: {error:?}"));
            }
            match self.context.take_stopped_plan_summary() {
                Ok(Some(account)) => self.summary_account = Some(account),
                Ok(None) => {}
                Err(error) => self.secondary(format!("failed Enter summary take: {error:?}")),
            }
        }
        !observed.unavailable
    }

    fn advance_release(&mut self, io: &mut impl PumpIo) -> FailedEnterProgress {
        if self.resources.spu.is_none() {
            self.stage = Stage::Retire;
            return FailedEnterProgress::Progress;
        }
        if self.release_send.is_none() {
            let ordinal = self.resources.spu.as_ref().unwrap().next_ordinal;
            self.release_exchange = ordinal.map(|ordinal| ExchangeId {
                opener: Opener::Harness,
                ordinal,
            });
            match self.resources.stage_residency(LifecycleDirective::Release) {
                Ok(send) => self.release_send = Some(send),
                Err((original, detail)) => {
                    self.refused_release = Some(original);
                    self.release_failure = Some(detail.clone());
                    self.secondary(detail);
                    self.stage = Stage::Retire;
                    return FailedEnterProgress::Progress;
                }
            }
        }
        if !self.clock(io) {
            self.abort();
            return FailedEnterProgress::Closed;
        }
        let port = self
            .resources
            .spu
            .as_mut()
            .unwrap()
            .residency
            .as_mut()
            .unwrap();
        // Receive before send so an already-arrived answer cannot become valid
        // merely because Release reaches the wire later in this work unit.
        let receive_failure = if port.held().is_none() {
            port.try_receive().err()
        } else {
            None
        };
        // A whole retained reply is consumed before a following EOF/fault. The
        // owning port keeps it available even when that terminal probe fails.
        match port.take_received() {
            Ok(Some(reply)) => {
                let envelope = reply.envelope();
                let matched = self.release_send.as_ref().is_some_and(OrganSend::sent)
                    && self.release_exchange.as_ref() == Some(&envelope.exchange)
                    && envelope.position == Position::Close
                    && matches!(
                        envelope.payload,
                        weaver_types::Payload::Answer(LifecycleAnswer::Released)
                    );
                self.release = if matched {
                    ReleaseOutcome::Confirmed
                } else {
                    ReleaseOutcome::Unconfirmed
                };
                if !matched {
                    self.release_failure =
                        Some("Release reply was not an exact sent-exchange Released".into());
                }
                self.release_reply = Some(reply);
                if let Some(detail) = &self.release_failure {
                    self.secondary(detail.clone());
                }
                self.stage = Stage::Retire;
                FailedEnterProgress::Progress
            }
            Ok(None) => {
                if let Some(error) = receive_failure {
                    self.release_failure = Some(format!("Release receive: {error:?}"));
                    self.secondary(self.release_failure.as_ref().unwrap().clone());
                    self.stage = Stage::Retire;
                    return FailedEnterProgress::Progress;
                }
                if !self.clock(io) {
                    self.abort();
                    return FailedEnterProgress::Closed;
                }
                let port = self
                    .resources
                    .spu
                    .as_mut()
                    .unwrap()
                    .residency
                    .as_mut()
                    .unwrap();
                if let Err(error) = port.try_send_observed(io) {
                    self.release_failure = Some(format!("Release send: {error:?}"));
                    self.secondary(self.release_failure.as_ref().unwrap().clone());
                    self.stage = Stage::Retire;
                    FailedEnterProgress::Progress
                } else {
                    FailedEnterProgress::Pending
                }
            }
            Err(error) => {
                self.release_failure = Some(format!("Release completion: {error:?}"));
                self.secondary(self.release_failure.as_ref().unwrap().clone());
                self.stage = Stage::Retire;
                FailedEnterProgress::Progress
            }
        }
    }

    fn attempt_unload(&mut self, io: &mut impl PumpIo) -> bool {
        if self.unload_attempted {
            return true;
        }
        self.unload_attempted = true;
        let account = match self.context.flow.halt().unwrap().unload_account() {
            Ok(account) => account,
            Err(error) => {
                self.unload_unavailable = Some(format!("unload account: {error:?}"));
                return true;
            }
        };
        let fault_sequence = self
            .context
            .halt_report
            .as_ref()
            .and_then(|report| report.fault_sequence())
            .map(|sequence| sequence.0.into());
        let payload = serving::payloads::UnloadClose {
            grant_surface: self.member_stood.then_some(GrantSurface::Unreadable),
            // This admitted domain has no accepted Leave/capture/turn cause.
            cause: None,
            forced: false,
            forced_by: None,
            release: Some(Release {
                spu: self.release,
                member: self.member_stood.then_some(if self.member_failed {
                    MemberRelease::Unconfirmed
                } else {
                    MemberRelease::Closed
                }),
            }),
            end_reason: Some(serving::payloads::EndReason::ComponentFailure),
            state_flow_failure: Some(serving::payloads::StateFlowFailure {
                fault_sequence,
                account,
            }),
        };
        self.unload = Some(AuthoredEvent {
            kind: Kind::Unload,
            subsystem: Subsystem::Harness,
            turn: None,
            source: AuthoredSource::Runtime,
            causal_parent: None,
            payload: Some(Payload::Unload(Box::new(payload))),
        });
        let wall = match wall_ms() {
            Ok(wall) => wall,
            Err(error) => {
                self.unload_unavailable = Some(error);
                return true;
            }
        };
        if !self.clock(io) {
            self.abort();
            return false;
        }
        match self.context.barrier.submit_terminal_authored_ref(
            self.unload.as_ref().unwrap(),
            &self.context.author,
            self.context.last_ns,
            wall,
        ) {
            Ok(sequence) => self.unload_sequence = Some(sequence),
            Err(error) => self.unload_unavailable = Some(error),
        }
        true
    }

    /// A bounded wake, not another phase clock. Child status polling uses a
    /// short local wake capped by the original deadline; no waitpid blocks.
    pub(in crate::flow) fn wait_for_progress(
        &mut self,
        io: &mut impl PumpIo,
    ) -> FailedEnterProgress {
        if self.stage == Stage::Closed {
            return FailedEnterProgress::Closed;
        }
        if !self.clock(io) {
            self.abort();
            return FailedEnterProgress::Closed;
        }
        let deadline = self.deadline.unwrap();
        let wake = self
            .context
            .last_ns
            .saturating_add(10_000_000)
            .min(deadline);
        if let Err(error) = self.context.barrier.drain_capacity_wake() {
            self.secondary(format!("failed Enter capacity wake: {error:?}"));
            self.abort();
            return FailedEnterProgress::Closed;
        }
        let mut interests = vec![WaitInterest {
            fd: self.context.barrier.capacity_fd(),
            events: PollFlags::POLLIN,
        }];
        if self.stage == Stage::Accounts
            && let Some(port) = self
                .resources
                .spu
                .as_ref()
                .and_then(|spu| spu.decode.as_ref())
        {
            let events = port.interest().events();
            if !events.is_empty() {
                interests.push(WaitInterest {
                    fd: port.fd(),
                    events,
                });
            }
        }
        if self.stage == Stage::Release
            && let Some(port) = self
                .resources
                .spu
                .as_ref()
                .and_then(|spu| spu.residency.as_ref())
        {
            let interest = port.interest();
            interests.push(WaitInterest {
                fd: port.fd(),
                events: interest.events(),
            });
        }
        let result = io.wait(&interests, Some(wake));
        if let Err(error) = result {
            self.secondary(format!("failed Enter wait: {error}"));
            self.abort();
            return FailedEnterProgress::Closed;
        }
        if !self.clock(io) {
            self.abort();
            FailedEnterProgress::Closed
        } else {
            FailedEnterProgress::Pending
        }
    }

    /// Closed still returns the original context, retained accounts and any
    /// unreaped child custody; it never returns a reusable live run or Left.
    #[allow(clippy::result_large_err)] // An unfinished owner must be returned intact.
    pub(in crate::flow) fn finish(self) -> Result<FailedEnterFinished, Self> {
        if self.stage == Stage::Closed {
            Ok(FailedEnterFinished { cleanup: self })
        } else {
            Err(self)
        }
    }
}

/// Terminal evidence and any unreaped actual children. This exposes no live
/// context/transport or operation that can answer Ready or successful Left.
pub(in crate::flow) struct FailedEnterFinished {
    cleanup: FailedEnterCleanup,
}
impl FailedEnterFinished {
    pub(in crate::flow) fn failure(&self) -> &RunFailure {
        self.cleanup.failure()
    }
    pub(in crate::flow) fn channel_fault(&self) -> crate::ChannelFault {
        crate::ChannelFault::Closed
    }
    pub(in crate::flow) fn resources(&self) -> &FailedEnterResources {
        &self.cleanup.resources
    }
    pub(in crate::flow) fn poll_children(&mut self) -> bool {
        self.cleanup.resources.poll_children()
    }
    pub(in crate::flow) fn context(&self) -> &RecordingContext {
        &self.cleanup.context
    }
}

#[cfg(test)]
#[path = "context_failed_enter_tests.rs"]
pub(in crate::flow::context) mod tests;
