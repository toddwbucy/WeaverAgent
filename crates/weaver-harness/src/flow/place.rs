//! Dormant placement capabilities. Only the run owner combines these with actual
//! decode, receipt and Recorded evidence; none is a public wire constructor.
pub(super) mod mirror;
use super::context::{BootPlanSummary, MeasureFacts};
use super::journal::{PendingAppend, PrefixProjection};
use super::material::SelectedUnit;
use super::pump_transport::ReceivedDecode;
use std::sync::Arc;
use weaver_trace::{Sequence, TurnRef};
use weaver_traits::Message;
use weaver_types::state_flow::{
    CutMethod, CutReference, DecimalU64, PlacementCode, PlacementContext,
};

#[derive(Debug)]
pub(in crate::flow) struct PreparedPrefix {
    pub(super) projection: PrefixProjection,
    pub(super) assembly: DecimalU64,
    pub(super) cut: Option<CutReference>,
    pub(super) method: Option<CutMethod>,
}
impl PreparedPrefix {
    pub(in crate::flow) fn assembly_id(&self) -> DecimalU64 {
        self.assembly
    }
    pub(in crate::flow) fn coverage(&self) -> &[weaver_types::state_flow::MaterialRef] {
        self.projection.coverage()
    }
    pub(in crate::flow) fn revision(&self) -> DecimalU64 {
        self.projection.revision().into()
    }
    pub(in crate::flow) fn n(&self) -> u64 {
        self.projection.n()
    }
    pub(in crate::flow) fn resident_before(&self) -> u64 {
        self.projection.resident_before()
    }
    pub(in crate::flow) fn rebuild(&self) -> Vec<Message> {
        self.projection.rebuild_messages()
    }
}
#[derive(Debug)]
pub(in crate::flow) struct PlaceRequest {
    pub prefix: PreparedPrefix,
    pub selection: Vec<SelectedUnit>,
    pub tool_schemas: Vec<String>,
    pub context: PlacementContext,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum PlaceError {
    Busy,
    WrongPhase,
    WrongProof,
    MissingWait,
    InvalidContext,
    InvalidPrefix,
    InvalidUnits,
    Stale,
    Exhausted,
    ClassifierNotAdmitted,
    InvalidTail,
    NonSte100,
}
#[derive(Debug)]
pub(in crate::flow) struct PlaceRefused {
    pub assembly_id: DecimalU64,
    pub code: PlacementCode,
    pub detail: String,
    pub(super) account: Option<super::context::PlaceAccount>,
}

#[derive(Debug)]
pub(super) struct PlaceEngine {
    pub boot: BootPlanSummary,
    pub schemas: Vec<String>,
    pub outstanding: Option<Arc<()>>,
    pub staged: Option<Arc<()>>,
    pub native: super::context::NativeScopes,
}
impl PlaceEngine {
    pub(super) fn limits(&self) -> (u64, u64, u64, u64) {
        self.boot
            .repaired()
            .checked_open()
            .expect("actual checked Open retained")
            .limits()
    }
}

#[derive(Debug)]
pub(in crate::flow) struct PlacementGrant {
    pub(super) identity: Arc<()>,
    pub(super) origin: Arc<()>,
    pub(super) pending: PendingAppend,
    pub(super) delta: Vec<Message>,
    pub(super) units: Vec<Arc<SelectedUnit>>,
    pub(super) schemas: Vec<String>,
    pub(super) facts: MeasureFacts,
    pub(super) recorded: Sequence,
    pub(super) turn: TurnRef,
    pub(super) staged: Option<Staged>,
}
impl PlacementGrant {
    pub(in crate::flow) fn assembly_id(&self) -> DecimalU64 {
        self.pending.assembly().into()
    }
    pub(in crate::flow) fn delta(&self) -> &[Message] {
        &self.delta
    }
    pub(in crate::flow) fn tool_schemas(&self) -> &[String] {
        &self.schemas
    }
    pub(in crate::flow) fn assistant_id(&self) -> DecimalU64 {
        self.pending.assistant_id()
    }
    pub(in crate::flow) fn placement_sequence(&self) -> Sequence {
        self.recorded
    }
}

#[derive(Debug)]
pub(in crate::flow) struct ConfirmedGeneration {
    pub(super) grant: PlacementGrant,
    pub(super) received: ReceivedDecode,
    pub(super) output: Option<super::coordinator::CheckedGeneration>,
}
impl ConfirmedGeneration {
    pub(in crate::flow) fn take_output(&mut self) -> Option<super::coordinator::CheckedGeneration> {
        self.output.take()
    }
    pub(in crate::flow) fn generation(&self) -> &weaver_types::Generation {
        let crate::channel::DecodeReply::Answer(weaver_types::TokenAnswer::Generated(value)) =
            self.received.reply()
        else {
            unreachable!("validated generation")
        };
        value
    }
    pub(in crate::flow) fn assistant_id(&self) -> DecimalU64 {
        self.grant.assistant_id()
    }
    pub(in crate::flow) fn placement_sequence(&self) -> Sequence {
        self.grant.recorded
    }
    pub(in crate::flow) fn raw_bytes(&self) -> &[u8] {
        self.received.raw_bytes()
    }
    pub(in crate::flow) fn turn(&self) -> &TurnRef {
        &self.grant.turn
    }
}

#[derive(Debug)]
pub(in crate::flow) struct ReloadTail {
    pub(super) items: Vec<ReloadItem>,
}
#[derive(Debug)]
pub(super) enum ReloadItem {
    Held(Arc<SelectedUnit>),
    Pending(PendingReloadText),
}
#[derive(Debug)]
pub(in crate::flow) struct PendingReloadText {
    pub(super) origin: Arc<()>,
    pub(super) source: weaver_types::state_flow::SourceBlockId,
    pub(super) lease: super::ledger::SourceLease,
}
impl ReloadTail {
    pub(in crate::flow) fn held(units: Vec<SelectedUnit>) -> Self {
        Self {
            items: units
                .into_iter()
                .map(|unit| ReloadItem::Held(Arc::new(unit)))
                .collect(),
        }
    }
    pub(in crate::flow) fn push_pending(&mut self, pending: PendingReloadText) {
        self.items.push(ReloadItem::Pending(pending));
    }
}
#[derive(Debug)]
pub(in crate::flow) struct Staged {
    pub(super) identity: Arc<()>,
    pub(super) origin: Arc<()>,
    pub(super) assembly: DecimalU64,
    pub(super) units: Vec<Arc<SelectedUnit>>,
    pub(super) measured: MeasureFacts,
    pub(super) resident_after: u64,
    pub(super) recorded: Option<Sequence>,
    pub(super) method: CutMethod,
}
impl Staged {
    pub(in crate::flow) fn assembly_id(&self) -> DecimalU64 {
        self.assembly
    }
    pub(in crate::flow) fn measured(&self) -> &MeasureFacts {
        &self.measured
    }
    pub(in crate::flow) fn resident_after(&self) -> u64 {
        self.resident_after
    }
}

pub(super) fn fit(facts: &MeasureFacts, limits: (u64, u64, u64, u64)) -> Result<(), PlacementCode> {
    let (head, tail, generation, terminal) = limits;
    if facts.prefix_tokens > head {
        return Err(PlacementCode::HeadOverCapacity);
    }
    let tail_used = facts
        .keep
        .checked_sub(facts.prefix_tokens)
        .and_then(|n| n.checked_add(facts.input_tokens));
    if tail_used.is_none_or(|n| n > tail) {
        return Err(PlacementCode::TailOverCapacity);
    }
    if facts
        .keep
        .checked_add(facts.input_tokens)
        .and_then(|n| n.checked_add(generation))
        .and_then(|n| n.checked_add(terminal))
        .is_none_or(|n| n > facts.capacity)
    {
        return Err(PlacementCode::GenerationRoom);
    }
    Ok(())
}

/// Consume only the existing arrays' shape/count facts; never retain token IDs.
pub(super) fn generation_facts(
    generation: &weaver_types::Generation,
    facts: &MeasureFacts,
    generation_limit: u64,
    terminal: u64,
) -> Result<(), &'static str> {
    let value: serde_json::Value =
        weaver_types::state_flow::decode_strict(generation.measurement.get())
            .map_err(|_| "malformed generation measurement")?;
    let object = value
        .as_object()
        .ok_or("generation measurement is not an object")?;
    let count = |name: &str| -> Result<u64, &'static str> {
        let items = object
            .get(name)
            .and_then(serde_json::Value::as_array)
            .ok_or("missing generation token array")?;
        if items
            .iter()
            .any(|item| item.as_u64().is_none_or(|token| token > u32::MAX as u64))
        {
            return Err("invalid generation token");
        }
        u64::try_from(items.len()).map_err(|_| "generation count overflow")
    };
    let input = count("input_tokens")?;
    let output = count("output_tokens")?;
    if input != facts.input_tokens
        || output > generation_limit
        || generation.capacity != facts.capacity
        || terminal != 1
        || facts
            .keep
            .checked_add(input)
            .and_then(|n| n.checked_add(output))
            .and_then(|n| n.checked_add(terminal))
            != Some(generation.resident)
        || generation.resident > generation.capacity
    {
        return Err("generation residency differs from measured proposal");
    }
    if generation.content.iter().any(|block| {
        !matches!(
            block,
            weaver_traits::ContentBlock::Text { .. }
                | weaver_traits::ContentBlock::ToolCall(_)
                | weaver_traits::ContentBlock::UnrecoveredCall { .. }
        )
    }) {
        return Err("generation contains non-assistant block");
    }
    Ok(())
}

impl PlaceRefused {
    pub(in crate::flow) fn account(&self) -> Option<&super::context::PlaceAccount> {
        self.account.as_ref()
    }
    pub(in crate::flow) fn account_mut(&mut self) -> Option<&mut super::context::PlaceAccount> {
        self.account.as_mut()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum ReloadCode {
    ClassifierNotAdmitted,
    InvalidPrefix,
    InvalidTail,
    NonSte100,
}
