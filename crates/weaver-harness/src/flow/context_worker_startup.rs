//! Worker-facing custody across the fixed replacement Enter sequence.
//! Workflow policy remains in worker/dev_loop. This surface is not selected by
//! the active worker, and exposes no state stream, author or placement mint.
use super::RecordingContext;
use super::boot_composition::gate_startup::ready::{EnterReady, EnteredServing, ReadyStage};
use super::boot_composition::gate_startup::{GateStartup, GateStartupRequest, GateStartupStage};
use super::boot_composition::{BootComposition, BootCompositionProgress};
use super::failed_enter_driver::FailedEnterResources;
use super::residency_composition::{
    AdmittedStartup, ResidencyComposition, ResidencyRequest, ResidencyStage,
};
use super::startup_composition::{EnterStartupInputs, StartupComposition, StartupStage};
use crate::flow::clock::{MonotonicIo, PumpIo};
use crate::flow::pump::enter::{EnterReceiver, ReceivedEnter};
use crate::flow::pump::{LifecycleListener, LifecyclePorts};
use crate::flow::templates::CompiledInventory;
use crate::{CoordinationListener, LoopIdentity, OrganBinaries, OrganParameters};
use weaver_types::state_flow::TemplateSpec;
use weaver_types::state_flow::enter::EnterBinding;

/// Compiled worker inputs. Runtime instructions, start identities and scheme
/// text come exclusively from the actual root-judged Enter, not this caller.
pub struct FoundationProgram {
    pub organs: OrganBinaries,
    pub parameters: OrganParameters,
    pub composer: LoopIdentity,
    pub templates: &'static [TemplateSpec],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoundationAdmissionProgress {
    Pending,
    Received,
    Stopped,
}

/// Owns the root-only listener, exact received bytes and ancillary descriptors.
/// A stopped admission remains owned for the lifecycle's refusal/cleanup path.
pub struct FoundationAdmission {
    listener: LifecycleListener,
    receiver: Option<EnterReceiver>,
    received: Option<ReceivedEnter>,
    detail: Option<String>,
    malformed_answer: Option<weaver_types::OrganEnvelope>,
    malformed_send: Option<crate::flow::pump_transport::OrganSend>,
    malformed_pending: bool,
}
impl FoundationAdmission {
    pub fn listen(listener: CoordinationListener) -> Result<Self, String> {
        Ok(Self {
            listener: LifecycleListener::new(listener).map_err(|error| format!("{error:?}"))?,
            receiver: None,
            received: None,
            detail: None,
            malformed_answer: None,
            malformed_send: None,
            malformed_pending: false,
        })
    }

    /// One nonblocking accept/receive step. No child or recording context exists
    /// yet. No additional connection is consumed while this Enter is retained.
    pub fn advance(&mut self) -> FoundationAdmissionProgress {
        self.advance_with_io(&mut MonotonicIo)
    }

    fn advance_with_io(&mut self, io: &mut impl PumpIo) -> FoundationAdmissionProgress {
        if self.malformed_pending {
            return self.advance_malformed(io);
        }
        if self.detail.is_some() {
            return FoundationAdmissionProgress::Stopped;
        }
        if self.received.is_some() {
            return FoundationAdmissionProgress::Received;
        }
        if self.receiver.is_none() {
            match self.listener.accept_enter() {
                Ok(Some(peer)) => {
                    // accept_enter hands out only an unused, authenticated peer.
                    self.receiver = Some(EnterReceiver::from_peer(peer).ok().expect("unused peer"));
                }
                Ok(None) => return FoundationAdmissionProgress::Pending,
                Err(error) => {
                    self.detail = Some(format!("Enter accept: {error:?}"));
                    return FoundationAdmissionProgress::Stopped;
                }
            }
        }
        let receiver = self.receiver.as_mut().unwrap();
        if let Err(error) = receiver.advance(io) {
            self.detail = Some(format!("Enter receive: {error:?}"));
            return FoundationAdmissionProgress::Stopped;
        }
        match self.receiver.take().unwrap().into_received() {
            Ok(original) => {
                let timing = original.check_window(io);
                self.received = Some(original);
                match timing {
                    Ok(()) => FoundationAdmissionProgress::Received,
                    Err(crate::flow::pump::enter::EnterTimeError::Future) => {
                        let original = self.received.as_mut().unwrap();
                        original.close_unadmitted();
                        self.malformed_answer = Some(weaver_types::OrganEnvelope {
                            exchange: original.envelope().exchange.clone(),
                            position: weaver_types::Position::Close,
                            payload: weaver_types::Payload::Refusal(
                                weaver_types::LifecycleRefusal::Malformed,
                            ),
                        });
                        self.detail =
                            Some("Malformed Enter: window starts after packet receipt".into());
                        self.malformed_pending = true;
                        self.advance_malformed(io)
                    }
                    Err(error) => {
                        self.received.as_mut().unwrap().close_unadmitted();
                        self.detail = Some(format!("Enter window: {error:?}"));
                        FoundationAdmissionProgress::Stopped
                    }
                }
            }
            Err(receiver) => {
                self.receiver = Some(receiver);
                FoundationAdmissionProgress::Pending
            }
        }
    }

    fn advance_malformed(&mut self, io: &mut impl PumpIo) -> FoundationAdmissionProgress {
        use crate::flow::pump_transport::TrackedStage;
        let original = self.received.as_mut().expect("retained malformed Enter");
        let result = (|| {
            if let Some(answer) = self.malformed_answer.take() {
                match original.peer_mut().stage_tracked(answer) {
                    Ok(TrackedStage::Queued(send)) => self.malformed_send = Some(send),
                    Ok(TrackedStage::Busy(answer)) => self.malformed_answer = Some(answer),
                    Err(error) => {
                        self.malformed_answer = Some(error.input);
                        return Err(error.failure);
                    }
                }
            }
            original.peer_mut().try_send_observed(io)?;
            Ok(())
        })();
        if result.is_err() || self.malformed_send.as_ref().is_some_and(|send| send.sent()) {
            self.malformed_pending = false;
            FoundationAdmissionProgress::Stopped
        } else {
            FoundationAdmissionProgress::Pending
        }
    }

    pub fn detail(&self) -> Option<&str> {
        self.detail.as_deref()
    }

    /// Adopt only the exact original Enter window retained with the received
    /// packet. No caller can replace it with a fresh remaining allowance.
    #[allow(clippy::result_large_err)]
    pub fn adopt(self) -> Result<FoundationRun, Self> {
        self.adopt_with_io(&mut MonotonicIo)
    }

    #[allow(clippy::result_large_err)]
    fn adopt_with_io(mut self, io: &mut impl PumpIo) -> Result<FoundationRun, Self> {
        if self.detail.is_some() || self.received.is_none() {
            return Err(self);
        }
        let original = self.received.take().unwrap();
        match original.adopt(io) {
            Ok(adopted) => {
                let (mut context, original) = adopted.into_parts();
                let resources = context
                    .failed_enter_resources()
                    .expect("fresh adopted context has issued no resources");
                Ok(FoundationRun {
                    context,
                    resources,
                    listener: Some(self.listener),
                    original: Some(original),
                })
            }
            Err(failure) => {
                self.received = Some(failure.source);
                self.detail = Some(failure.detail);
                Err(self)
            }
        }
    }
}

/// The outer worker frame owns one context and its real child resources. The
/// startup and serving seats borrow these fields, so neither needs a cloned
/// stream or a self-referential owner. Keep this frame through final drain.
///
/// ```compile_fail,E0616
/// fn raw(run: weaver_harness::FoundationRun) { let _ = run.context; }
/// ```
pub struct FoundationRun {
    context: RecordingContext,
    resources: FailedEnterResources,
    listener: Option<LifecycleListener>,
    original: Option<ReceivedEnter>,
}
impl FoundationRun {
    pub fn begin(&mut self, program: FoundationProgram) -> Result<FoundationEnter<'_>, String> {
        let inventory = CompiledInventory::new(program.templates).map_err(|e| e.to_string())?;
        let Some(binary) = program.organs.classify else {
            return Err("Serving requires the compiled checker launch path".into());
        };
        let Some(original) = self.original.as_ref() else {
            return Err("this run already transferred its Enter".into());
        };
        let payload = original.envelope().enter();
        let EnterBinding::Serving {
            gate_instruction,
            shell,
            ..
        } = &payload.binding
        else {
            return Err("worker startup requires Serving Enter".into());
        };
        let residency = ResidencyRequest {
            binary: program.organs.spu,
            arguments: program.parameters.spu_arguments(),
            instruction: payload.spu_instruction.clone(),
        };
        let gate = GateStartupRequest {
            binary: program.organs.gate,
            instruction: gate_instruction.clone(),
            shell: shell.clone(),
        };
        let inputs = EnterStartupInputs {
            original: self.original.take().unwrap(),
            composer: program.composer,
            classifier_binary: binary,
            inventory,
        };
        match StartupComposition::from_enter(&mut self.context, &mut self.resources, inputs) {
            Ok(owner) => Ok(FoundationEnter {
                state: Some(EnterState::Startup(Box::new(owner))),
                listener: self.listener.take().map(Box::new),
                residency: Some(Box::new(residency)),
                gate: Some(Box::new(gate)),
                detail: None,
            }),
            Err((inputs, reason)) => {
                self.original = Some(inputs.original);
                Err(reason.into())
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoundationEnterProgress {
    Pending,
    Ready,
    Stopped,
}

enum EnterState<'a> {
    Startup(Box<StartupComposition<'a>>),
    Residency(Box<ResidencyComposition<'a>>),
    // Failed joins keep the consumed owner too, not a reconstructed summary.
    Admitted(Box<AdmittedStartup<'a>>),
    Boot(Box<BootComposition<'a>>),
    Gate(Box<GateStartup<'a>>),
    Raised(Box<super::boot_composition::gate_startup::RaisedBoot<'a>>),
    Ready(Box<EnterReady<'a>>),
    Entered(Box<EnteredServing<'a>>),
}

/// Owns every intermediate value across backpressure and failure. The common
/// pump keeps later connections queued while the original Enter owns its peer.
/// Only completed aggregate Ready can yield a serving seat.
///
/// ```compile_fail,E0507
/// fn steal<'a>(enter: &mut weaver_harness::FoundationEnter<'a>) {
///     let _ = enter.finish();
/// }
/// ```
pub struct FoundationEnter<'a> {
    state: Option<EnterState<'a>>,
    listener: Option<Box<LifecycleListener>>,
    residency: Option<Box<ResidencyRequest>>,
    gate: Option<Box<GateStartupRequest>>,
    detail: Option<String>,
}
impl<'a> FoundationEnter<'a> {
    pub fn progress(&self) -> FoundationEnterProgress {
        if self.detail.is_some() {
            FoundationEnterProgress::Stopped
        } else if matches!(self.state, Some(EnterState::Entered(_))) {
            FoundationEnterProgress::Ready
        } else {
            FoundationEnterProgress::Pending
        }
    }

    pub fn detail(&self) -> Option<&str> {
        self.detail.as_deref()
    }

    /// Advance the existing fixed lifecycle owners using the one common pump.
    ///
    /// # Safety
    /// Run on the serving thread. That thread must outlive and reap the actual
    /// children, including stopped owners, or exit the whole worker as failed.
    /// This is fork_organ's existing child/thread lifetime contract. Dropping a
    /// stopped owner is not a completed cleanup or permission to start again.
    pub unsafe fn advance(&mut self) -> FoundationEnterProgress {
        // SAFETY: the caller retains the serving thread and all returned custody.
        unsafe { self.advance_with_io(&mut MonotonicIo) }
    }

    unsafe fn advance_with_io(&mut self, io: &mut impl PumpIo) -> FoundationEnterProgress {
        if self.progress() != FoundationEnterProgress::Pending {
            return self.progress();
        }
        let state = self.state.take().expect("one retained startup stage");
        self.state = Some(match state {
            EnterState::Startup(mut owner) => {
                // SAFETY: inherited from advance's serving-thread contract.
                let stage = unsafe { owner.advance(ports(&mut self.listener), io) };
                if stage == StartupStage::Stopped {
                    self.detail = Some(format!("startup: {:?}", owner.stopped()));
                }
                if stage == StartupStage::Complete {
                    match owner.finish(ports(&mut self.listener), io) {
                        Ok(prepared) => EnterState::Residency(Box::new(ResidencyComposition::new(
                            prepared,
                            *self.residency.take().expect("original SPU request"),
                        ))),
                        Err(owner) => {
                            self.detail = Some(format!("startup handoff: {:?}", owner.stopped()));
                            EnterState::Startup(Box::new(owner))
                        }
                    }
                } else {
                    EnterState::Startup(owner)
                }
            }
            EnterState::Residency(mut owner) => {
                // SAFETY: inherited from advance's serving-thread contract.
                let stage = unsafe { owner.advance(ports(&mut self.listener), io) };
                if stage == ResidencyStage::Stopped {
                    self.detail = Some(format!("residency: {:?}", owner.stopped()));
                }
                if stage == ResidencyStage::Complete {
                    match owner.finish(ports(&mut self.listener), io) {
                        Ok(admitted) => match admitted.into_boot() {
                            Ok(boot) => EnterState::Boot(Box::new(boot)),
                            Err(admitted) => {
                                self.detail = Some("resident boot handoff refused".into());
                                EnterState::Admitted(Box::new(admitted))
                            }
                        },
                        Err(owner) => {
                            self.detail = Some(format!("residency handoff: {:?}", owner.stopped()));
                            EnterState::Residency(Box::new(owner))
                        }
                    }
                } else {
                    EnterState::Residency(owner)
                }
            }
            EnterState::Boot(mut owner) => {
                let progress = owner.advance(ports(&mut self.listener), io);
                if progress == BootCompositionProgress::Stopped {
                    self.detail = Some(format!("boot: {:?}", owner.stopped()));
                }
                if progress == BootCompositionProgress::Complete {
                    match owner.finish(ports(&mut self.listener), io) {
                        Ok(boot) => EnterState::Gate(Box::new(boot.into_gate(
                            *self.gate.take().expect("original gate request"),
                            *self.listener.take().expect("original listener"),
                        ))),
                        Err(owner) => {
                            self.detail = Some(format!("boot handoff: {:?}", owner.stopped()));
                            EnterState::Boot(Box::new(owner))
                        }
                    }
                } else {
                    EnterState::Boot(owner)
                }
            }
            EnterState::Gate(mut owner) => {
                // SAFETY: inherited from advance's serving-thread contract.
                let stage = unsafe { owner.advance(LifecyclePorts::default(), io) };
                if stage == GateStartupStage::Stopped {
                    self.detail = Some(format!("gate: {:?}", owner.stop()));
                }
                if stage == GateStartupStage::Complete {
                    match owner.finish(LifecyclePorts::default(), io) {
                        Ok(raised) => match raised.into_ready() {
                            Ok(ready) => EnterState::Ready(Box::new(ready)),
                            Err(raised) => {
                                self.detail = Some("aggregate Ready prerequisites refused".into());
                                EnterState::Raised(Box::new(raised))
                            }
                        },
                        Err(owner) => {
                            self.detail = Some(format!("gate handoff: {:?}", owner.stop()));
                            EnterState::Gate(Box::new(owner))
                        }
                    }
                } else {
                    EnterState::Gate(owner)
                }
            }
            EnterState::Ready(mut owner) => {
                let stage = owner.advance(io);
                if stage == ReadyStage::Stopped {
                    self.detail = Some(format!("aggregate Ready: {:?}", owner.stop()));
                }
                if stage == ReadyStage::Complete {
                    EnterState::Entered(Box::new(owner.finish().ok().expect("completed Ready")))
                } else {
                    EnterState::Ready(owner)
                }
            }
            owner => owner,
        });
        self.progress()
    }

    /// Consuming the startup owner transfers the same standing resources.
    /// Every unsuccessful attempt returns the entire owner unchanged.
    pub fn finish(mut self) -> Result<FoundationServing<'a>, Self> {
        if self.progress() != FoundationEnterProgress::Ready {
            return Err(self);
        }
        let Some(EnterState::Entered(owner)) = self.state.take() else {
            unreachable!()
        };
        Ok(FoundationServing {
            gate_outgoing: owner.gate_raise.exchange.ordinal,
            owner,
            gate_exchanges: Default::default(),
            queued_inputs: Default::default(),
            graceful: None,
        })
    }
}

fn ports(listener: &mut Option<Box<LifecycleListener>>) -> LifecyclePorts<'_> {
    LifecyclePorts {
        listener: listener.as_deref_mut(),
        ..Default::default()
    }
}

/// An opaque serving seat, granted only by the whole aggregate Enter answer.
/// Its retained context, ports, child custody, template certificate and original
/// evidence are the inputs for subsequent serving/teardown composition. No raw
/// authority escapes through this handoff. It is intentionally not Clone.
///
/// ```compile_fail,E0277
/// fn cloned<T: Clone>() {}
/// cloned::<weaver_harness::FoundationServing<'static>>();
/// ```
pub struct FoundationServing<'a> {
    owner: Box<EnteredServing<'a>>,
    gate_exchanges: std::collections::HashSet<u64>,
    queued_inputs: std::collections::VecDeque<crate::flow::pump_transport::ReceivedOrgan>,
    gate_outgoing: u64,
    graceful: Option<Box<input::graceful::PendingGraceful>>,
}

#[path = "context_worker_input.rs"]
pub(crate) mod input;

#[cfg(test)]
#[path = "context_worker_startup_tests.rs"]
mod tests;

#[path = "context_worker_rollback.rs"]
pub(crate) mod rollback;

#[path = "context_worker_startup_failure.rs"]
pub(crate) mod failed;

#[path = "context_worker_startup_expired.rs"]
pub(crate) mod expired;

#[path = "context_worker_entered_rollback.rs"]
pub(crate) mod entered_rollback;
