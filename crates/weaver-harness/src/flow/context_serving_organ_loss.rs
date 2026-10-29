//! A serving organ's observed end is distinct from a failed flow component.
//! The original ports and children remain with the existing run; this account
//! retains their identities and exactly one owed fault through later lifecycle.
use super::serving_observation::ServingObservationAuthor;
use super::*;
use crate::flow::author::AuthoredSource;
use crate::flow::pump_transport::{DecodePort, Failure, OrganPort, PortIdentity};
use crate::tools_cgroup::{Containment, ToolsCgroup};
use std::sync::Arc;

#[path = "context_serving_organ_halt.rs"]
mod halt;

#[path = "context_serving_gate_loss.rs"]
mod gate_loss;

#[cfg(test)]
#[path = "context_serving_organ_loss_tests.rs"]
pub(super) mod tests;

#[cfg(test)]
#[path = "context_serving_organ_clock_tests.rs"]
mod clock_tests;

pub(super) enum ObservationProgress {
    Complete,
    Pending { wake_ns: u64 },
    EarlierAuthor,
}

pub(super) struct ServingOrganPorts<'a> {
    pub(super) gate: &'a OrganPort,
    pub(super) decode: &'a DecodePort,
    pub(super) residency: &'a OrganPort,
    pub(super) tools: &'a ToolsCgroup,
}

#[derive(Default)]
pub(super) struct ObservedOrgans {
    gate: Option<ObservedGate>,
    spu: Option<ObservedSpu>,
    first_gate: Option<bool>,
    wake_ns: Option<(bool, u64)>,
}

struct ObservedGate {
    port: PortIdentity,
    observation: Option<Observation>,
}
struct ObservedSpu {
    decode: PortIdentity,
    residency: PortIdentity,
    observation: Option<Observation>,
}

struct Observation {
    organ: &'static str,
    failure: Failure,
    observed_ns: Option<u64>,
    settle_by: Option<u64>,
    containment_by: Option<u64>,
    containment: Option<Containment>,
    killed: bool,
    fallback: bool,
    leaving: bool,
    last_word: Option<crate::spawn::LastWordSnapshot>,
    author: Option<Arc<ServingObservationAuthor>>,
    event: Option<AuthoredEvent>,
    started: bool,
    sequence: Option<Sequence>,
    returned: StoppedAuthor,
    unavailable: Option<String>,
    stopped_record: Option<crate::flow::recording::StoppedStartupRecord>,
}

/// Only the retained observation can submit this exact ordinary-capacity debt.
pub(in crate::flow) struct OwedOrganObservation(AuthoredEvent);
impl OwedOrganObservation {
    pub(in crate::flow) fn into_event(self) -> AuthoredEvent {
        self.0
    }
}

impl ObservedOrgans {
    pub(super) fn wake(&self) -> Option<u64> {
        let (gate, wake) = self.wake_ns?;
        self.observation(gate)
            .filter(|observation| {
                observation.sequence.is_none() && observation.unavailable.is_none()
            })
            .map(|_| wake)
    }

    fn order(&self) -> [bool; 2] {
        let first = self.first_gate.unwrap_or(true);
        [first, !first]
    }

    fn observation(&self, gate: bool) -> Option<&Observation> {
        if gate {
            self.gate.as_ref()?.observation.as_ref()
        } else {
            self.spu.as_ref()?.observation.as_ref()
        }
    }

    fn take(&mut self, gate: bool) -> Option<Observation> {
        if gate {
            self.gate.as_mut()?.observation.take()
        } else {
            self.spu.as_mut()?.observation.take()
        }
    }

    fn restore(&mut self, gate: bool, observation: Observation) {
        if gate {
            self.gate.as_mut().unwrap().observation = Some(observation);
        } else {
            self.spu.as_mut().unwrap().observation = Some(observation);
        }
    }

    pub(super) fn any(&self) -> bool {
        self.gate.is_some() || self.spu.is_some()
    }
    pub(super) fn no_residency(&self) -> bool {
        self.spu.is_some()
    }
    pub(super) fn gate_matches(&self, port: &OrganPort) -> bool {
        self.gate.as_ref().is_some_and(|end| end.port.matches(port))
    }
    pub(super) fn gate_delivery_lost(&self, port: &OrganPort) -> bool {
        self.gate_matches(port)
            && port
                .failure()
                .is_some_and(|failure| failure.fault == crate::failure::ChannelFault::Closed)
            && self.observation(true).is_some_and(|observation| {
                observation.observed_ns.is_some()
                    && matches!(
                        observation.containment,
                        Some(Containment::Emptied | Containment::Delivered)
                    )
            })
    }
    pub(super) fn decode_matches(&self, port: &DecodePort) -> bool {
        self.spu
            .as_ref()
            .is_some_and(|end| end.decode.matches_decode(port))
    }
    pub(super) fn residency_matches(&self, port: &OrganPort) -> bool {
        self.spu
            .as_ref()
            .is_some_and(|end| end.residency.matches(port))
    }
    pub(super) fn refresh(
        &mut self,
        gate: &failed_enter_driver::OwnedChild,
        spu: &failed_enter_driver::OwnedChild,
    ) {
        self.refresh_gate(gate);
        self.refresh_spu(spu);
    }

    pub(super) fn refresh_gate(&mut self, gate: &failed_enter_driver::OwnedChild) {
        self.refresh_one(true, gate);
    }

    pub(super) fn refresh_spu(&mut self, spu: &failed_enter_driver::OwnedChild) {
        self.refresh_one(false, spu);
    }

    fn refresh_one(&mut self, gate: bool, child: &failed_enter_driver::OwnedChild) {
        if let Some(mut observation) = self.take(gate) {
            if observation.unavailable.is_none()
                && observation.author.is_none()
                && let Some(snapshot) = child.last_word_snapshot()
            {
                observation.last_word = Some(snapshot);
            }
            self.restore(gate, observation);
        }
    }
}

impl Observation {
    fn new(organ: &'static str, failure: Failure, leaving: bool) -> Self {
        Self {
            organ,
            failure,
            observed_ns: None,
            settle_by: None,
            containment_by: None,
            containment: None,
            killed: false,
            fallback: false,
            leaving,
            last_word: None,
            author: None,
            event: None,
            started: false,
            sequence: None,
            returned: StoppedAuthor::default(),
            unavailable: None,
            stopped_record: None,
        }
    }

    fn anchor(&mut self, now: u64, tools: Option<&ToolsCgroup>) {
        assert!(self.observed_ns.is_none(), "one original organ-loss anchor");
        self.observed_ns = Some(now);
        self.settle_by =
            Some(now.saturating_add(crate::spawn::LAST_WORD_SETTLE_BOUND.as_nanos() as u64));
        if let Some(tools) = tools {
            self.killed = tools.kill().is_ok();
            if !self.killed {
                self.fallback = tools.signal_pinned_pass();
            }
            if self.leaving {
                self.containment = Some(crate::tools_cgroup::step(
                    crate::tools_cgroup::Context::Leave,
                    self.killed,
                    self.fallback,
                    false,
                ));
            } else {
                self.containment_by = Some(
                    now.saturating_add(crate::tools_cgroup::CONTAINMENT_BOUND.as_nanos() as u64),
                );
            }
        }
    }

    fn prepare(&mut self, context: &RecordingContext) {
        if self.author.is_none() {
            let mut account = serde_json::json!({
                "organ": self.organ,
                "death": "its channel was lost while serving",
                "transport": self.failure.detail,
            });
            if let Some(word) = self.last_word.as_ref().and_then(|word| word.word.as_ref()) {
                account["last_word"] = word.clone();
            }
            if let Some(containment) = self.containment {
                account["containment"] = serde_json::Value::String(match containment {
                    Containment::Emptied => "tools/ killed and emptied",
                    Containment::Delivered => "tools/ killed, not waited on inside the leave",
                    Containment::PastBound => "tools/ killed and not empty within the bound: service ends",
                    Containment::Undelivered => "containment failed: the write of cgroup.kill and the pinned fallback both failed, the kill not delivered",
                }.into());
            }
            let report = crate::authorship::harness_report(
                weaver_types::FaultCase::OrganDeathObserved,
                &account.to_string(),
            );
            self.author = Some(Arc::new(ServingObservationAuthor::organ_loss(
                context, &report,
            )));
            self.event = Some(AuthoredEvent {
                kind: Kind::Fault,
                subsystem: weaver_trace::Subsystem::Harness,
                turn: None,
                source: AuthoredSource::Runtime,
                causal_parent: None,
                payload: Some(Payload::Fault(
                    serde_json::value::to_raw_value(&report).expect("typed fault serializes"),
                )),
            });
        }
    }

    fn advance<I: PumpIo>(
        &mut self,
        context: &mut RecordingContext,
        ports: &mut PumpPorts<'_, I>,
        tools: Option<&ToolsCgroup>,
    ) -> Result<ObservationProgress, RunFailure> {
        if self.sequence.is_some() {
            return Ok(ObservationProgress::Complete);
        }
        context.live(ports.io)?;
        self.returned.check()?;
        let now = context.last_ns;
        if self.observed_ns.is_none() {
            // An interrupted initial clock cannot silently acquire a new window.
            return Err(context.fail(
                ports.io,
                FlowHaltReason::TraceFailed,
                HaltPoint::Author,
                "organ loss has no original clock witness",
            ));
        }
        if let Some(tools) = tools
            && self.containment.is_none()
        {
            let empty = tools.populated() == Some(false);
            if !self.killed && !empty {
                self.fallback |= tools.signal_pinned_pass();
            }
            if empty || now >= self.containment_by.expect("serving containment anchor") {
                self.containment = Some(crate::tools_cgroup::step(
                    crate::tools_cgroup::Context::Serving,
                    self.killed,
                    self.fallback,
                    empty,
                ));
            }
        }
        if !self.leaving
            && matches!(
                self.containment,
                Some(Containment::PastBound | Containment::Undelivered)
            )
        {
            // Containment ends serving before any trace or last-word wait.
            // The exact observation moves with this context into bounded halt
            // accounting, without spending the reserved first-failure entry.
            return Err(context.fail(
                ports.io,
                FlowHaltReason::ProtocolFailed,
                HaltPoint::Barrier,
                "serving gate loss could not contain tools",
            ));
        }
        if self.organ == "gate"
            && !self.leaving
            && self.containment == Some(Containment::Emptied)
            && !self.settle_gate_pairs(context, ports)?
        {
            // An actual received packet or earlier rejection keeps its owner.
            // Let that owner consume/file it before assigning an absent result.
            return Ok(ObservationProgress::EarlierAuthor);
        }
        let stderr_settled = self.last_word.as_ref().is_some_and(|word| word.eof)
            || now >= self.settle_by.expect("observed last-word bound");
        if !stderr_settled || (tools.is_some() && self.containment.is_none()) {
            let mut wake = now.saturating_add(10_000_000);
            if !stderr_settled {
                wake = wake.min(self.settle_by.unwrap());
            }
            if self.containment.is_none()
                && let Some(bound) = self.containment_by
            {
                wake = wake.min(bound);
            }
            if let Some(bound) = context.deadlines.nearest() {
                wake = wake.min(bound);
            }
            // The caller's common pump combines this wake with every real
            // interest and original deadline. Advancing here never sleeps.
            return Ok(ObservationProgress::Pending { wake_ns: wake });
        }
        self.prepare(context);
        let owner = self.author.as_ref().unwrap();
        if !self.started && !context.serving_observation_author_available() {
            return Ok(ObservationProgress::EarlierAuthor);
        }
        let result = if self.started {
            context.resume_serving_observation(ports, owner)
        } else {
            self.started = true;
            context.author_serving_observation(ports, owner, self.event.take().unwrap())
        };
        match result {
            Ok(sequence) => self.sequence = Some(sequence),
            Err(error) => {
                // The claim retains any actual Recorded evidence. A recoverable
                // wait still owes the exact completion/claim continuation.
                return Err(self.returned.retain_started(error, &mut self.started));
            }
        }
        Ok(ObservationProgress::Complete)
    }
}

impl RecordingContext {
    pub(super) fn observe_serving_organ_end(
        &mut self,
        source: PumpSource,
        failure: &Failure,
        ports: ServingOrganPorts<'_>,
        leaving: bool,
        io: &mut impl PumpIo,
    ) -> Result<bool, RunFailure> {
        let ServingOrganPorts {
            gate,
            decode,
            residency,
            tools,
        } = ports;
        if failure.fault != crate::failure::ChannelFault::Closed
            || self.phase != RecordingPhase::Entered
            || self.failure().is_some()
            || self.flow.halt().is_some()
        {
            return Ok(false);
        }
        let original_closed = |port: &OrganPort| {
            port.failure()
                .is_some_and(|end| end.fault == crate::failure::ChannelFault::Closed)
        };
        let gate_loss = match source {
            PumpSource::Gate if original_closed(gate) => true,
            PumpSource::Residency if original_closed(residency) && decode.has_no_exchange() => {
                false
            }
            PumpSource::Decode
                if decode.has_no_exchange()
                    && decode
                        .failure()
                        .is_some_and(|end| end.fault == crate::failure::ChannelFault::Closed) =>
            {
                false
            }
            _ => return Ok(false),
        };
        if !gate_loss {
            return self.observe_idle_spu_end(source, failure, decode, residency, leaving, io);
        }
        self.serving_organs.first_gate.get_or_insert(true);
        if self.serving_organs.gate_matches(gate) {
            return Ok(true);
        }
        self.serving_organs.gate = Some(ObservedGate {
            port: gate.port_identity(),
            observation: Some(Observation::new("gate", failure.clone(), leaving)),
        });
        self.anchor_organ_end(true, Some(tools), io)
    }

    /// The admitted idle SPU remains observable after Lower, without requiring
    /// a live gate or inventing another gate ownership token.
    pub(super) fn observe_idle_spu_end(
        &mut self,
        source: PumpSource,
        failure: &Failure,
        decode: &DecodePort,
        residency: &OrganPort,
        leaving: bool,
        io: &mut impl PumpIo,
    ) -> Result<bool, RunFailure> {
        if failure.fault != crate::failure::ChannelFault::Closed
            || self.phase != RecordingPhase::Entered
            || self.failure().is_some()
            || self.flow.halt().is_some()
            || !decode.has_no_exchange()
            || !match source {
                PumpSource::Residency => residency.failure(),
                PumpSource::Decode => decode.failure(),
                _ => return Ok(false),
            }
            .is_some_and(|end| end.fault == crate::failure::ChannelFault::Closed)
        {
            return Ok(false);
        }
        if self.serving_organs.decode_matches(decode)
            && self.serving_organs.residency_matches(residency)
        {
            return Ok(true);
        }
        self.serving_organs.first_gate.get_or_insert(false);
        self.serving_organs.spu = Some(ObservedSpu {
            decode: decode.port_identity(),
            residency: residency.port_identity(),
            observation: Some(Observation::new("spu", failure.clone(), leaving)),
        });
        self.anchor_organ_end(false, None, io)
    }

    fn anchor_organ_end(
        &mut self,
        gate_loss: bool,
        tools: Option<&ToolsCgroup>,
        io: &mut impl PumpIo,
    ) -> Result<bool, RunFailure> {
        // Retain evidence before sampling the clock. A fatal error leaves the exact
        // unanchored observation in this context's failed-run owner.
        let interrupted = match self.live(io) {
            Ok(()) => None,
            Err(wait @ RunFailure::Wait { .. }) => Some(wait),
            Err(failure) => return Err(failure),
        };
        // A recorded inspection can yield its original lifecycle expiry before
        // live() reads the clock. Sample here, next to this worker's first kill,
        // even on that recoverable exit; neither last_ns nor a later retry is
        // the clock witness for this observation.
        let now = io.now().map_err(|error| {
            self.fail(
                io,
                FlowHaltReason::TraceFailed,
                HaltPoint::Author,
                format!("organ-observation clock failed: {error}"),
            )
        })?;
        if now < self.last_ns {
            return Err(self.fail(
                io,
                FlowHaltReason::TraceFailed,
                HaltPoint::Author,
                "organ-observation clock reversed",
            ));
        }
        self.last_ns = now;
        let observation = if gate_loss {
            self.serving_organs
                .gate
                .as_mut()
                .unwrap()
                .observation
                .as_mut()
                .unwrap()
        } else {
            self.serving_organs
                .spu
                .as_mut()
                .unwrap()
                .observation
                .as_mut()
                .unwrap()
        };
        observation.anchor(now, tools);
        match interrupted {
            Some(wait) => Err(wait),
            None => Ok(true),
        }
    }

    pub(super) fn advance_serving_organ_faults<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
        tools: &ToolsCgroup,
    ) -> Result<ObservationProgress, RunFailure> {
        self.advance_organ_faults(ports, Some(tools))
    }

    pub(super) fn advance_leaving_organ_faults<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
        closed: Option<&worker_startup::expired::gate::RollbackGateClosed>,
    ) -> Result<ObservationProgress, RunFailure> {
        // The consuming Leave owner carries the same observations, anchors and
        // author claims. A gate's unfinished Serving containment adopts only
        // the actual original gate's completed Leave sweep.
        self.serving_organs.wake_ns = None;
        for gate in self.serving_organs.order() {
            let containment = if gate {
                self.serving_organs.gate.as_ref().and_then(|end| {
                    closed
                        .filter(|proof| proof.allows(self) && proof.owns_port(&end.port))
                        .and_then(|proof| proof.containment())
                })
            } else {
                None
            };
            let Some(mut observation) = self.serving_organs.take(gate) else {
                continue;
            };
            if observation.sequence.is_none() {
                observation.leaving = true;
                if gate && observation.containment.is_none() {
                    observation.containment = containment;
                    if observation.containment.is_none() {
                        self.serving_organs.restore(gate, observation);
                        return Ok(ObservationProgress::EarlierAuthor);
                    }
                }
            }
            self.serving_organs.restore(gate, observation);
        }
        self.advance_organ_faults(ports, None)
    }

    fn advance_organ_faults<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
        tools: Option<&ToolsCgroup>,
    ) -> Result<ObservationProgress, RunFailure> {
        self.serving_organs.wake_ns = None;
        for gate in self.serving_organs.order() {
            let Some(mut observation) = self.serving_organs.take(gate) else {
                continue;
            };
            let result = observation.advance(self, ports, if gate { tools } else { None });
            self.serving_organs.restore(gate, observation);
            match result? {
                ObservationProgress::Complete => (),
                pending @ ObservationProgress::Pending { wake_ns } => {
                    self.serving_organs.wake_ns = Some((gate, wake_ns));
                    return Ok(pending);
                }
                pending => return Ok(pending),
            }
        }
        Ok(ObservationProgress::Complete)
    }
}
