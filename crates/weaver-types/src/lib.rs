//! conforms: types-one-floor-link
//! conforms: types-peer-identity-no-deserialize
//! conforms: types-wire-enums-exhaustive
//!
//! The floor's data definitions: the agent config the operator writes, the
//! identity pair the two external boundaries judge, and the loop 0 wire
//! vocabulary every organ channel carries. This crate defines what crosses a
//! boundary, and crossing it is somebody else's crate.
//!
//! The doctests below are compile-time instruments of `weaver-types-Spec`
//! section 5.
//!
//! `PeerIdentity` implements no `Deserialize`
//! (`types-peer-identity-no-deserialize`): the kernel supplies it and the peer
//! never asserts it, so the machinery for constructing one from socket bytes
//! must not exist.
//!
//! ```compile_fail
//! fn de<T: for<'de> serde::Deserialize<'de>>() {}
//! de::<weaver_types::PeerIdentity>();
//! ```
//!
//! The three wire enums are exhaustive (`types-wire-enums-exhaustive`): a
//! consumer's match with no wildcard arm compiles, so a case added to loop 0
//! breaks every consumer loudly, in the act that edits the floor. The matches
//! below carry no wildcard, and the day the attribute arrives they stop
//! compiling, which is this pin firing.
//!
//! ```
//! use weaver_types::{LifecycleAnswer, LifecycleDirective, LifecycleRefusal};
//! fn directive_name(d: &LifecycleDirective) -> &'static str {
//!     match d {
//!         LifecycleDirective::Enter { .. } => "enter",
//!         LifecycleDirective::Leave => "leave",
//!         LifecycleDirective::Stop => "stop",
//!         LifecycleDirective::Admit { .. } => "admit",
//!         LifecycleDirective::Release => "release",
//!         LifecycleDirective::Raise { .. } => "raise",
//!         LifecycleDirective::Lower => "lower",
//!         LifecycleDirective::Load { .. } => "load",
//!         LifecycleDirective::Unload { .. } => "unload",
//!         LifecycleDirective::Validate { .. } => "validate",
//!         LifecycleDirective::List => "list",
//!         LifecycleDirective::Show { .. } => "show",
//!         LifecycleDirective::Observe => "observe",
//!     }
//! }
//! fn answer_name(a: &LifecycleAnswer) -> &'static str {
//!     match a {
//!         LifecycleAnswer::Ready => "ready",
//!         LifecycleAnswer::Left => "left",
//!         LifecycleAnswer::TurnAborted { .. } => "turn_aborted",
//!         LifecycleAnswer::AtRest => "at_rest",
//!         LifecycleAnswer::Admitted => "admitted",
//!         LifecycleAnswer::Released => "released",
//!         LifecycleAnswer::GateReady => "gate_ready",
//!         LifecycleAnswer::GateStopped => "gate_stopped",
//!         LifecycleAnswer::Validated => "validated",
//!         LifecycleAnswer::State { .. } => "state",
//!         LifecycleAnswer::Agents { .. } => "agents",
//!     }
//! }
//! fn refusal_name(r: &LifecycleRefusal) -> &'static str {
//!     match r {
//!         LifecycleRefusal::Unauthorized => "unauthorized",
//!         LifecycleRefusal::Malformed => "malformed",
//!         LifecycleRefusal::NoSuchAgent => "no_such_agent",
//!         LifecycleRefusal::CarriedWork => "carried_work",
//!         LifecycleRefusal::OutOfOrder => "out_of_order",
//!         LifecycleRefusal::DescriptorsUnusable => "descriptors_unusable",
//!         LifecycleRefusal::BoundaryUnverified => "boundary_unverified",
//!         LifecycleRefusal::ConfigInvalid { .. } => "config_invalid",
//!         LifecycleRefusal::ArtifactUnresolvable => "artifact_unresolvable",
//!         LifecycleRefusal::ArtifactUnreadable => "artifact_unreadable",
//!         LifecycleRefusal::DeviceCannotAdmit => "device_cannot_admit",
//!         LifecycleRefusal::NoResidency => "no_residency",
//!         LifecycleRefusal::BindFailed => "bind_failed",
//!         LifecycleRefusal::PriorUnitUnreaped => "prior_unit_unreaped",
//!         LifecycleRefusal::OrganRefused { .. } => "organ_refused",
//!         LifecycleRefusal::ActivityNotAtRest => "activity_not_at_rest",
//!     }
//! }
//! ```

mod config;
mod identity;
mod wire;

#[cfg(feature = "config")]
pub use config::parse;
pub use config::{
    AgentConfig, ArtifactRef, BindingKind, ClassifyInstruction, ConfigError, ConfigErrorKind, Cut,
    DecoderInstruction, DeviceOrdinal, ElectedKindConfig, FieldElection, FieldName,
    GateInstruction, ModelBinding, Restore, SpuInstruction, StateElection, StateStore, StoreEngine,
    ToolName, TraceSink,
};
pub use identity::{AccessRule, PeerIdentity, authorized};
pub use wire::{
    AgentName, AgentState, AgentSummary, Candidate, Composer, DECODE_MESSAGE_BOUND, EnterBinding,
    EnterPayload, ExchangeId, FaultCase, FaultReport, Finish, Generation, KillCause, LabelAnswer,
    LabelDirective, LabelRefusal, LifecycleAnswer, LifecycleAsk, LifecycleDirective,
    LifecycleRefusal, Lineage, LoadFacts, MAX_ENVELOPE_BYTES, Opener, OrganEnvelope, Payload,
    Position, RefusalRecord, RefusingOrgan, RunId, ScoredLabel, SegmentPreamble, SessionId,
    TokenAnswer, TokenAsk, TokenDirective, TokenRefusal, ToolExecution, ToolOutcome, TurnFrame,
    TurnKey,
};
