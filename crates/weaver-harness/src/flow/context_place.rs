//! Complete dormant place/engine ownership. Runtime composition remains held.
use super::*;
use crate::channel::DecodeReply;
use crate::flow::author::AuthoredSource;
use crate::flow::place::*;
use crate::flow::pump_transport::{
    DecodePort, DecodeReservation, DecodeStage, ReceivedDecode, Stage,
};
use std::collections::VecDeque;
use std::sync::Arc;
use weaver_trace::serving::payloads as t;
use weaver_trace::{Subsystem, TurnRef};
use weaver_traits::{Message, Role};
use weaver_types::state_flow::{
    CutMethod, CutReference, DecimalU64, PlacementCode, PlacementContext, SourceBlockId,
};
use weaver_types::{RefusalRecord, TokenAnswer, TokenAsk, TokenDirective, TokenRefusal, TurnKey};

#[path = "context_place_generation.rs"]
mod generation;
#[path = "context_place_reload.rs"]
mod reload;
pub(in crate::flow) use reload::CheckedCut;
#[path = "context_place_marks.rs"]
mod marks;
pub(in crate::flow) use marks::{
    NativeMarkBeginError, NativeMarkCompletion, NativeMarkTransaction, NativePosition, NativeScopes,
};
#[path = "context_place_account.rs"]
mod account;
#[path = "context_place_refusal.rs"]
mod refusal;
pub(in crate::flow) use account::{OwedPlaceAuthor, PlaceReleasePermit};
pub(in crate::flow) use refusal::PlaceNotice;

#[derive(Debug)]
pub(in crate::flow) struct PlaceInstallError {
    pub prerequisite: BootPlanSummary,
    pub refusal: PlaceError,
    pub failure: Option<RunFailure>,
}
#[derive(Debug)]
pub(in crate::flow) struct PlaceBeginError<T = PlaceRequest> {
    pub input: T,
    pub refusal: PlaceError,
    pub failure: Option<RunFailure>,
    pub staged: Option<Staged>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum PlaceProgress {
    Progress,
    Pending,
    Complete,
}
pub(in crate::flow) type GenerationProgress = PlaceProgress;
pub(in crate::flow) type ReloadProgress = PlaceProgress;
#[derive(Debug)]
pub(in crate::flow) enum PlacementCompletion {
    Granted(PlacementGrant),
    Refused(PlaceRefused),
    Cancelled(PlaceAccount),
}
#[derive(Debug)]
pub(in crate::flow) enum GenerationCompletion {
    Generated(Box<ConfirmedGeneration>),
    Refused(PlaceAccount),
    Cancelled(PlaceAccount),
}
#[derive(Debug)]
pub(in crate::flow) enum ReloadCompletion {
    Staged(Staged),
    CancelledStaged {
        staged: Staged,
        account: PlaceAccount,
    },
    Refused(PlaceAccount),
    Cancelled(PlaceAccount),
}
/// Last checked no-mutation prefix and the already-filed portion of the tail.
/// Queue/drain may have refreshed this prefix while preserving its exact boundary.
/// A rejection stays explicit; conversion never retries a rejected source.
#[derive(Debug)]
pub(in crate::flow) struct ReturnedReloadInputs {
    prefix: PreparedPrefix,
    original_tail: ReloadTail,
    resolved: Vec<Arc<crate::flow::material::SelectedUnit>>,
    rejected: Option<crate::flow::ledger::SourceDisposition>,
}
impl ReturnedReloadInputs {
    pub(in crate::flow) fn prefix(&self) -> &PreparedPrefix {
        &self.prefix
    }
    pub(in crate::flow) fn original_tail(&self) -> &ReloadTail {
        &self.original_tail
    }
    pub(in crate::flow) fn resolved(&self) -> &[Arc<crate::flow::material::SelectedUnit>] {
        &self.resolved
    }
    pub(in crate::flow) fn rejected(&self) -> Option<&crate::flow::ledger::SourceDisposition> {
        self.rejected.as_ref()
    }
    #[allow(clippy::result_large_err)]
    pub(in crate::flow) fn into_retry(self) -> Result<(PreparedPrefix, ReloadTail), Self> {
        if self.rejected.is_some() {
            return Err(self);
        }
        let Self {
            prefix,
            mut original_tail,
            resolved,
            rejected: _,
        } = self;
        for (index, unit) in resolved.into_iter().enumerate() {
            let original =
                std::mem::replace(&mut original_tail.items[index], ReloadItem::Held(unit));
            if let ReloadItem::Pending(pending) = original {
                original_tail.filed_sources.push((index, pending));
            }
        }
        Ok((prefix, original_tail))
    }
}
#[derive(Debug)]
pub(in crate::flow) struct PlaceAccount {
    // Captured by the actual context when custody leaves its owner.
    phase: RecordingPhase,
    work: AccountWork,
    measure: Option<measure_driver::StoppedPlaceMeasure>,
}
#[derive(Debug)]
enum AccountWork {
    Place(Box<PlaceTransaction>),
    Generation(Box<GenerationTransaction>),
    Reload(Box<ReloadTransaction>),
    Native(Box<NativeMarkTransaction>),
    Notice(Box<PlaceNotice>),
}
impl PlaceAccount {
    pub(in crate::flow::context) fn refused_restatement(
        &self,
        context: &RecordingContext,
        source: &SourceBlockId,
    ) -> bool {
        matches!(&self.work, AccountWork::Place(work)
            if !work.close_on_refusal && work.done
                && work.request.context.instruction.as_ref() == Some(source)
                && work.units.len() == 1
                && work.units.iter().all(|unit| unit.belongs_to(context))
                && (!work.recorded_authors.is_empty()
                    || matches!(&work.measurement, Some(MeasureCompletion::Refused(refused)) if refused.sources().1.0 > 0)))
    }
    pub(in crate::flow) fn recorded_sequences(&self) -> &[Sequence] {
        match &self.work {
            AccountWork::Place(w) => &w.recorded_authors,
            AccountWork::Generation(w) => &w.recorded_authors,
            AccountWork::Reload(w) => &w.recorded_authors,
            AccountWork::Notice(w) => &w.recorded_authors,
            AccountWork::Native(_) => self.native_recorded_sequences(),
        }
    }
    pub(in crate::flow) fn confirmed_generation(&self) -> Option<&ConfirmedGeneration> {
        match &self.work {
            AccountWork::Generation(w) => w.confirmed.as_ref(),
            _ => None,
        }
    }
    pub(in crate::flow) fn staged_evidence(&self) -> Option<&Staged> {
        match &self.work {
            AccountWork::Reload(w) => w.staged.as_ref(),
            AccountWork::Place(w) => w.staged.as_ref(),
            AccountWork::Generation(w) => w.grant.as_ref().and_then(|g| g.staged.as_ref()),
            _ => None,
        }
    }
    pub(in crate::flow) fn raw_bytes(&self) -> Option<&[u8]> {
        match &self.work {
            AccountWork::Place(w) => w.measurement.as_ref().map(MeasureCompletion::raw_bytes),
            AccountWork::Generation(w) => w
                .terminal
                .as_ref()
                .map(ReceivedDecode::raw_bytes)
                .or_else(|| w.confirmed.as_ref().map(ConfirmedGeneration::raw_bytes))
                .or(w.failed_wire.as_deref()),
            AccountWork::Reload(w) => w
                .received
                .as_ref()
                .map(ReceivedDecode::raw_bytes)
                .or(w.failed_wire.as_deref()),
            AccountWork::Native(_) | AccountWork::Notice(_) => None,
        }
        .or_else(|| self.measure.as_ref().and_then(|m| m.raw_bytes()))
    }
    fn can_return_inputs(&self) -> bool {
        self.phase == RecordingPhase::Entered
    }
    pub(in crate::flow) fn take_staged(&mut self) -> Option<Staged> {
        if !self.can_return_inputs() {
            return None;
        }
        match &mut self.work {
            AccountWork::Place(w) if w.done && w.incomplete.is_none() => w.staged.take(),
            AccountWork::Reload(w)
                if w.done
                    && w.incomplete.is_none()
                    && w.staged
                        .as_ref()
                        .is_some_and(|staged| staged.recorded.is_some()) =>
            {
                w.staged.take()
            }
            AccountWork::Generation(w)
                if w.done
                    && w.incomplete.is_none()
                    && w.confirmed.is_none()
                    && w.reservation.is_none() =>
            {
                w.grant.as_mut().and_then(|g| g.staged.take())
            }
            _ => None,
        }
    }
    pub(in crate::flow) fn reload_prefix(&self) -> Option<&PreparedPrefix> {
        match &self.work {
            AccountWork::Reload(work) => work.prefix.as_ref(),
            _ => None,
        }
    }
    pub(in crate::flow) fn reload_tail(&self) -> Option<&ReloadTail> {
        match &self.work {
            AccountWork::Reload(work) => work.tail.as_ref(),
            _ => None,
        }
    }
    pub(in crate::flow) fn take_reload_inputs(&mut self) -> Option<ReturnedReloadInputs> {
        if !self.can_return_inputs() {
            return None;
        }
        let AccountWork::Reload(work) = &mut self.work else {
            return None;
        };
        // A confirmed cut owns Staged, even after that capability is taken.
        // Failed/incomplete exchange custody remains trace-only.
        if !work.done
            || work.cut_confirmed
            || work.incomplete.is_some()
            || work.prefix.is_none()
            || work.tail.is_none()
        {
            return None;
        }
        Some(ReturnedReloadInputs {
            prefix: work.prefix.take().unwrap(),
            original_tail: work.tail.take().unwrap(),
            resolved: std::mem::take(&mut work.units),
            rejected: work.rejected.take(),
        })
    }
    pub(in crate::flow) fn take_unplaced_selection(
        &mut self,
    ) -> Option<Vec<crate::flow::material::SelectedUnit>> {
        if !self.can_return_inputs() {
            return None;
        }
        let AccountWork::Place(work) = &mut self.work else {
            return None;
        };
        if work.incomplete.is_some() {
            return None;
        }
        work.pending.take();
        if work.selected_count > work.units.len() {
            return None;
        }
        let start = work.units.len() - work.selected_count;
        if work.units[start..]
            .iter()
            .any(|u| Arc::strong_count(u) != 1)
        {
            return None;
        }
        work.selected_count = 0;
        Some(
            work.units
                .split_off(start)
                .into_iter()
                .map(|u| Arc::try_unwrap(u).expect("exclusive retained selection"))
                .collect(),
        )
    }
    pub(in crate::flow) fn incomplete(&self) -> Option<&str> {
        match &self.work {
            AccountWork::Place(w) => w.incomplete.as_deref(),
            AccountWork::Generation(w) => w.incomplete.as_deref(),
            AccountWork::Reload(w) => w.incomplete.as_deref(),
            AccountWork::Native(w) => w.incomplete.as_deref(),
            AccountWork::Notice(w) => w.incomplete.as_deref(),
        }
    }
    pub(in crate::flow) fn reload_refusal(&self) -> Option<(ReloadCode, &str)> {
        match &self.work {
            AccountWork::Reload(w) => Some((w.reload_code?, w.reload_detail.as_deref()?)),
            _ => None,
        }
    }
    pub(in crate::flow) fn refusal(&self) -> Option<&TokenRefusal> {
        match &self.work {
            AccountWork::Generation(w) => w.terminal.as_ref(),
            AccountWork::Reload(w) => w.received.as_ref(),
            _ => None,
        }
        .and_then(|r| match r.reply() {
            DecodeReply::Refusal(r) => Some(r),
            _ => None,
        })
    }
}
#[derive(Debug)]
struct PlaceAuthor {
    record: AuthorRecord,
    event: Option<AuthoredEvent>,
    kind: Kind,
    turn: Option<TurnRef>,
}
impl PlaceAuthor {
    fn new(event: AuthoredEvent) -> Self {
        Self {
            record: AuthorRecord::new(),
            kind: event.kind,
            turn: event.turn.clone(),
            event: Some(event),
        }
    }
}
impl std::ops::Deref for PlaceAuthor {
    type Target = AuthorRecord;
    fn deref(&self) -> &AuthorRecord {
        &self.record
    }
}
impl std::ops::DerefMut for PlaceAuthor {
    fn deref_mut(&mut self) -> &mut AuthorRecord {
        &mut self.record
    }
}
#[derive(Debug)]
pub(super) struct PlaceTransaction {
    request: PlaceRequest,
    units: Vec<Arc<crate::flow::material::SelectedUnit>>,
    pending: Option<crate::flow::journal::PendingAppend>,
    delta: Vec<Message>,
    measurement: Option<MeasureCompletion>,
    facts: Option<MeasureFacts>,
    author: Option<PlaceAuthor>,
    recorded_authors: Vec<Sequence>,
    bound: AbsoluteBound,
    recorded: Option<Sequence>,
    driving: bool,
    cancelled: bool,
    done: bool,
    refusal: Option<PlaceRefused>,
    staged: Option<Staged>,
    incomplete: Option<String>,
    started_ns: u64,
    close_pending: bool,
    close_on_refusal: bool,
    account_deadline: Option<u64>,
    selected_count: usize,
    return_author: Option<weaver_types::state_flow::CallId>,
    placement_after_returns: Option<AuthoredEvent>,
}
#[derive(Debug)]
pub(super) struct GenerationTransaction {
    grant: Option<PlacementGrant>,
    native_admission: Option<NativeScopes>,
    reservation: Option<DecodeReservation>,
    terminal: Option<ReceivedDecode>,
    failed_wire: Option<Vec<u8>>,
    intermediates: Vec<ReceivedDecode>,
    authors: VecDeque<AuthoredEvent>,
    author: Option<PlaceAuthor>,
    recorded_authors: Vec<Sequence>,
    confirmed: Option<ConfirmedGeneration>,
    cancelled: bool,
    cancel_sent: bool,
    // Terminal evidence settled the original reservation; no Cancel was sent.
    // This does not settle still-owed model/output/placement authors.
    cancel_settled: bool,
    driving: bool,
    done: bool,
    output_started: bool,
    output_done: bool,
    output_cancelled: Option<crate::flow::coordinator::OutputCancellation>,
    incomplete: Option<String>,
    account_deadline: Option<u64>,
}
#[derive(Debug)]
pub(super) struct ReloadTransaction {
    // Taken only from a settled no-mutation account; live work retains both.
    prefix: Option<PreparedPrefix>,
    tail: Option<ReloadTail>,
    units: Vec<Arc<crate::flow::material::SelectedUnit>>,
    frozen_sources: Vec<SourceBlockId>,
    bound: Option<AbsoluteBound>,
    measurement: Option<MeasureCompletion>,
    reservation: Option<DecodeReservation>,
    received: Option<ReceivedDecode>,
    failed_wire: Option<Vec<u8>>,
    authors: VecDeque<AuthoredEvent>,
    author: Option<PlaceAuthor>,
    recorded_authors: Vec<Sequence>,
    staged: Option<Staged>,
    cut_confirmed: bool,
    driving: bool,
    cancelled: bool,
    done: bool,
    incomplete: Option<String>,
    refused: bool,
    rejected: Option<crate::flow::ledger::SourceDisposition>,
    reload_code: Option<ReloadCode>,
    reload_detail: Option<String>,
    started_ns: Option<u64>,
    account_deadline: Option<u64>,
}

impl ReloadTransaction {
    fn live_prefix(&self) -> &PreparedPrefix {
        self.prefix
            .as_ref()
            .expect("live reload retains its checked prefix until account extraction")
    }
    fn live_tail(&self) -> &ReloadTail {
        self.tail
            .as_ref()
            .expect("live reload retains its original tail until account extraction")
    }
}

#[cfg(test)]
pub(super) struct ReloadDrainEvidence<'a> {
    pub bound: Option<AbsoluteBound>,
    pub measured: bool,
    pub reserved: bool,
    pub refused: bool,
    pub code: Option<ReloadCode>,
    pub detail: Option<&'a str>,
}

#[cfg(test)]
impl ReloadTransaction {
    pub(super) fn drain_evidence_for_test(&self) -> ReloadDrainEvidence<'_> {
        ReloadDrainEvidence {
            bound: self.bound,
            measured: self.measurement.is_some(),
            reserved: self.reservation.is_some(),
            refused: self.refused,
            code: self.reload_code,
            detail: self.reload_detail.as_deref(),
        }
    }

    pub(super) fn saved_prefix_for_drain_review(&self) -> &PreparedPrefix {
        self.prefix.as_ref().expect("live reload owns its prefix")
    }

    pub(super) fn saved_prefix_mut_for_drain_review(&mut self) -> &mut PreparedPrefix {
        self.prefix.as_mut().expect("live reload owns its prefix")
    }
}

impl RecordingContext {
    pub(in crate::flow) fn install_place(
        &mut self,
        boot: BootPlanSummary,
        io: &mut impl PumpIo,
    ) -> Result<(), PlaceInstallError> {
        let failure = self.live(io).err();
        let valid = self.boot_proof_current(boot.repaired())
            && self.committed_plan_summary().is_ok()
            && self.place_engine.is_none()
            && self.load_duty.is_some()
            && !self.place_admission_stopped()
            && !self.boot_enter_cancelled
            && self.phase == RecordingPhase::Entered;
        if failure.is_some() || !valid {
            return Err(PlaceInstallError {
                prerequisite: boot,
                refusal: PlaceError::WrongProof,
                failure,
            });
        }
        if let Some(held) = self.held_input() {
            return Err(PlaceInstallError {
                prerequisite: boot,
                refusal: PlaceError::Busy(Some(held)),
                failure: None,
            });
        }
        let schemas = if self.load_duty.as_ref().unwrap().tools.sh.is_some() {
            vec![crate::tools::shell_schema().to_owned()]
        } else {
            vec![]
        };
        let native = Arc::new(NativeScopes::from_boot(&boot));
        self.place_engine = Some(PlaceEngine {
            native,
            boot,
            schemas,
            outstanding: None,
            staged: None,
        });
        Ok(())
    }
    pub(in crate::flow) fn hold_place_return(
        &self,
        call: &weaver_types::state_flow::CallId,
    ) -> Result<crate::flow::material::SelectedUnit, PlaceError> {
        if self.place_engine.is_none() {
            return Err(PlaceError::WrongProof);
        }
        self.pairs
            .take_result(call)
            .map_err(|_| PlaceError::WrongProof)?;
        let pair = self
            .pairs
            .closed_for_material(call)
            .ok_or(PlaceError::WrongProof)?;
        Ok(crate::flow::material::from_completed_pair(
            self.read_origin.clone(),
            pair,
        ))
    }
    pub(in crate::flow) fn loaded_place_schemas(&self) -> Option<&[String]> {
        self.place_engine
            .as_ref()
            .map(|engine| engine.schemas.as_slice())
    }
    pub(in crate::flow) fn prepare_prefix(
        &mut self,
        cut: Option<CutReference>,
        method: CutMethod,
        io: &mut impl PumpIo,
    ) -> Result<PreparedPrefix, PlaceError> {
        self.live(io).map_err(|_| PlaceError::WrongPhase)?;
        if self.place_admission_stopped() || self.phase != RecordingPhase::Entered {
            return Err(PlaceError::WrongPhase);
        }
        if self.place_blocks_external()
            || self
                .place_engine
                .as_ref()
                .is_some_and(|e| e.staged.is_some())
        {
            return Err(PlaceError::Busy(self.held_input()));
        }
        if self.place_engine.is_none() {
            return Err(PlaceError::WrongProof);
        }
        let journal = self.boot_journal.as_ref().ok_or(PlaceError::WrongProof)?;
        // InvalidCut queues a notice owner, so custody must be clear first.
        if self.caller_author_held() {
            return Err(PlaceError::Busy(self.held_input()));
        }
        let assembly = journal.next_assembly().into();
        let prefix = match journal.prepare(cut, method) {
            Ok(p) => p,
            Err(_) => {
                self.queue_place_refusal(
                    assembly,
                    PlacementCode::InvalidCut,
                    "cut address is not an active occurrence",
                )?;
                return Err(PlaceError::InvalidPrefix);
            }
        };
        Ok(prefix)
    }
    pub(super) fn place_measurement_continuation(&self, key: PumpDeadline) -> bool {
        match key {
            PumpDeadline::Decision => self.place.as_ref().is_some_and(|work| {
                work.driving
                    && work.measurement.is_none()
                    && self.deadlines.get(key) == Some(work.bound)
            }),
            PumpDeadline::Cut => self.reload.as_ref().is_some_and(|work| {
                work.driving
                    && work.measurement.is_none()
                    && work.bound.is_some()
                    && self.deadlines.get(key) == work.bound
            }),
            _ => false,
        }
    }
    pub(super) fn generation_output_continuation(&self) -> bool {
        self.generation
            .as_ref()
            .is_some_and(|work| work.driving && work.confirmed.is_some() && !work.output_started)
    }
    pub(super) fn place_blocks_external(&self) -> bool {
        self.place_notice.is_some()
            || self.place_close.is_some()
            || self.native_marks.is_some()
            || self.place.as_ref().is_some_and(|w| !w.driving)
            || self.generation.as_ref().is_some_and(|w| !w.driving)
            || self
                .reload
                .as_ref()
                .is_some_and(|w| w.bound.is_some() && !w.driving)
            || (self.place.is_none()
                && self.generation.is_none()
                && self
                    .place_engine
                    .as_ref()
                    .is_some_and(|e| e.outstanding.is_some()))
    }
    pub(super) fn place_wait_owned(&self, key: PumpDeadline) -> bool {
        self.place_notice
            .as_ref()
            .is_some_and(|w| w.key == key && !w.done)
            || self
                .native_marks
                .as_ref()
                .is_some_and(|w| w.key == key && !w.done)
            || (key == PumpDeadline::Leave && self.place_owed())
            || (key == PumpDeadline::Decision && self.place.is_some())
            || (key == PumpDeadline::Cut && self.reload.as_ref().is_some_and(|w| w.bound.is_some()))
            || (key == PumpDeadline::Decision
                && self.generation.as_ref().is_some_and(|w| w.output_started))
    }
    pub(super) fn reload_draining(&self) -> bool {
        self.reload.as_ref().is_some_and(|w| {
            w.bound.is_some()
                && !w.cancelled
                && !w.done
                && !w.refused
                && w.author.is_none()
                && w.authors.is_empty()
                && w.measurement.is_none()
                && w.reservation.is_none()
                && w.staged.is_none()
        })
    }
    /// Borrowing an owed drain requires this reload's unchanged original Cut.
    pub(super) fn reload_drain_bound(&self) -> Option<AbsoluteBound> {
        if !self.reload_draining() {
            return None;
        }
        let bound = self.reload.as_ref()?.bound?;
        (self.deadlines.get(PumpDeadline::Cut) == Some(bound)).then_some(bound)
    }
    fn place_busy(&self) -> bool {
        self.place_busy_except_reload() || self.reload.as_ref().is_some_and(|w| w.bound.is_some())
    }
    fn place_busy_except_reload(&self) -> bool {
        self.caller_author_held() || self.place_owners_busy_except_reload()
    }
    fn place_owners_busy(&self) -> bool {
        self.place_owners_busy_except_reload()
            || self.reload.as_ref().is_some_and(|w| w.bound.is_some())
    }
    fn place_owners_busy_except_reload(&self) -> bool {
        self.place_notice.is_some()
            || self.native_marks.is_some()
            || self.place_close.is_some()
            || self.place.is_some()
            || self.generation.is_some()
            || self.measure.is_some()
            || self.output.is_some()
            || self.filing.is_some()
            || self.pair_work.is_some()
            || self.read.is_some()
            || self.inspection.is_some()
            || self.marks.is_some()
            || self.member_capture.is_some()
            || self.freshness.is_some()
            || self.repair_blocks_external()
            || self.plan_summary_blocks_external()
            || self.boot_open_blocks_external()
            || self.input.snapshot().is_some()
            || self.input_turn.is_some()
            || self.input_author.is_some()
            || self.waiting_content.is_some()
            || self.completed_author.is_some()
            || self.barrier.pending().is_some()
            || self.barrier.pending_author().is_some()
            || self.flow.has_pending_record()
    }
    pub(super) fn place_fault(
        &mut self,
        io: &mut impl PumpIo,
        detail: impl Into<String>,
    ) -> RunFailure {
        self.fail(
            io,
            FlowHaltReason::ProtocolFailed,
            HaltPoint::Barrier,
            detail,
        )
    }
    /// Sort's read-only adapter validates the current private prefix and exact
    /// immutable material; it burns no assembly or message IDs and grants no send.
    pub(in crate::flow) fn begin_place_measurement(
        &mut self,
        prefix: &PreparedPrefix,
        units: &[crate::flow::material::SelectedUnit],
        schemas: &[String],
        wait: PumpDeadline,
        decode: &mut DecodePort,
        io: &mut impl PumpIo,
    ) -> Result<(), PlaceError> {
        self.live(io).map_err(|_| PlaceError::WrongPhase)?;
        if self.place_admission_stopped() || self.phase != RecordingPhase::Entered {
            return Err(PlaceError::WrongPhase);
        }
        self.check_place_port(decode)
            .map_err(|_| PlaceError::WrongProof)?;
        if self.place_owners_busy()
            || self
                .place_engine
                .as_ref()
                .is_some_and(|e| e.outstanding.is_some() || e.staged.is_some())
        {
            return Err(PlaceError::Busy(self.held_input()));
        }
        let engine = self.place_engine.as_ref().ok_or(PlaceError::WrongProof)?;
        let journal = self.boot_journal.as_ref().ok_or(PlaceError::WrongProof)?;
        if journal.check_projection(&prefix.projection).is_err() {
            return Err(PlaceError::Stale);
        }
        if schemas != engine.schemas
            || units.iter().any(|u| {
                !u.belongs_to(self)
                    || u.completed_pair()
                        .is_some_and(|p| self.pairs.result_record(p).is_err())
            })
        {
            return Err(PlaceError::InvalidUnits);
        }
        journal
            .validate_selection(&prefix.projection, units.iter())
            .map_err(|_| PlaceError::InvalidUnits)?;
        if self.caller_author_held() {
            return Err(PlaceError::Busy(self.held_input()));
        }
        let mut delta = prefix.rebuild();
        for unit in units {
            delta.extend(unit.messages().iter().cloned());
        }
        let request = MeasureRequest {
            assembly_id: prefix.assembly,
            revision: prefix.revision(),
            keep: prefix.n(),
            delta,
            tool_schemas: schemas.to_vec(),
            turn: self.open_turn.clone(),
            position: MeasurePosition {
                resident: journal.resident(),
                prefix_tokens: journal.prefix_tokens(),
                capacity: journal.capacity(),
            },
        };
        self.begin_measurement(request, wait, decode, io)
            .map_err(|e| {
                if e.failure.is_some() {
                    PlaceError::WrongPhase
                } else {
                    PlaceError::Busy(None)
                }
            })
    }
    pub(in crate::flow) fn take_place_measurement(
        &mut self,
        decode: &mut DecodePort,
        io: &mut impl PumpIo,
    ) -> Result<Option<MeasureCompletion>, RunFailure> {
        self.check_place_port(decode)?;
        let revision = self
            .boot_journal
            .as_ref()
            .ok_or(RunFailure::PlaceOwned)?
            .revision()
            .into();
        self.take_measurement(revision, decode, io)
    }
    #[allow(clippy::result_large_err)]
    pub(in crate::flow) fn begin_place(
        &mut self,
        request: PlaceRequest,
        decode: &mut DecodePort,
        io: &mut impl PumpIo,
    ) -> Result<(), PlaceBeginError> {
        self.begin_place_inner(request, None, decode, io)
    }
    #[allow(clippy::result_large_err)] // Refusal returns the exact owned request and carry.
    fn begin_place_inner(
        &mut self,
        mut request: PlaceRequest,
        staged: Option<Staged>,
        decode: &mut DecodePort,
        io: &mut impl PumpIo,
    ) -> Result<(), PlaceBeginError> {
        let failure = self
            .live(io)
            .err()
            .or_else(|| self.check_place_port(decode).err());
        let refusal = if failure.is_some()
            || self.place_admission_stopped()
            || self.phase != RecordingPhase::Entered
        {
            Some(PlaceError::WrongPhase)
        } else if self.place_owners_busy()
            || self.place_engine.as_ref().is_some_and(|e| {
                e.outstanding.is_some() || (e.staged.is_some() && staged.is_none())
            })
        {
            Some(PlaceError::Busy(self.held_input()))
        } else if self.place_engine.is_none() || !self.native_place_cache_ready() {
            Some(PlaceError::WrongProof)
        } else if self.open_turn.is_none() || self.deadlines.get(PumpDeadline::Decision).is_none() {
            Some(PlaceError::MissingWait)
        } else if request.context.label.is_empty()
            || request.tool_schemas != self.place_engine.as_ref().unwrap().schemas
            || !(self.place_instruction_valid(&request.context)
                || self.restatement_instruction_valid(&request))
        {
            Some(PlaceError::InvalidContext)
        } else if self.caller_author_held() {
            // Prefix/selection refusals below create a notice owner. Custody
            // blocks those side effects as well as successful placement.
            Some(PlaceError::Busy(self.held_input()))
        } else if request.prefix.cut.is_some()
            || self
                .boot_journal
                .as_ref()
                .unwrap()
                .check_projection(&request.prefix.projection)
                .is_err()
        {
            Some(PlaceError::InvalidPrefix)
        } else {
            None
        };
        if let Some(refusal) = refusal {
            if failure.is_none() && refusal == PlaceError::InvalidPrefix {
                let _ = self.queue_place_refusal(
                    request.prefix.assembly,
                    PlacementCode::StalePosition,
                    "placement prefix no longer names the current journal",
                );
            }
            return Err(PlaceBeginError {
                input: request,
                refusal,
                failure,
                staged,
            });
        }
        // A previously minted selection is still a live grant, not permission
        // to redeliver a result acknowledged by a later completed append.
        if request
            .selection
            .iter()
            .chain(staged.iter().flat_map(|s| s.units.iter().map(Arc::as_ref)))
            .any(|unit| {
                !unit.belongs_to(self)
                    || unit
                        .completed_pair()
                        .is_some_and(|pair| self.pairs.result_record(pair).is_err())
            })
        {
            return Err(PlaceBeginError {
                input: request,
                refusal: PlaceError::InvalidUnits,
                failure: None,
                staged,
            });
        }
        let bound = self.deadlines.get(PumpDeadline::Decision).unwrap();
        let selected_count = request.selection.len();
        let mut units: Vec<_> = std::mem::take(&mut request.selection)
            .into_iter()
            .map(Arc::new)
            .collect();
        if let Some(held) = &staged {
            let mut ordered = held.units.clone();
            ordered.append(&mut units);
            units = ordered;
        }
        let pending = match self
            .boot_journal
            .as_mut()
            .unwrap()
            .reserve(&request.prefix.projection, &units)
        {
            Ok(p) => p,
            Err(error) => {
                let code = if error == crate::flow::journal::JournalError::InvalidPair {
                    PlacementCode::InvalidPair
                } else {
                    PlacementCode::InvalidSpan
                };
                let _ = self.queue_place_refusal(
                    request.prefix.assembly,
                    code,
                    "selected material cannot be placed at this prefix",
                );
                request.selection = units
                    .split_off(units.len() - selected_count)
                    .into_iter()
                    .map(|u| Arc::try_unwrap(u).expect("new request owns its units"))
                    .collect();
                return Err(PlaceBeginError {
                    input: request,
                    refusal: PlaceError::InvalidUnits,
                    failure: None,
                    staged,
                });
            }
        };
        let delta = pending.delta();
        let position = MeasurePosition {
            resident: self.boot_journal.as_ref().unwrap().resident(),
            prefix_tokens: self.boot_journal.as_ref().unwrap().prefix_tokens(),
            capacity: self.boot_journal.as_ref().unwrap().capacity(),
        };
        let measure = MeasureRequest {
            assembly_id: pending.assembly().into(),
            revision: request.prefix.revision(),
            keep: request.prefix.n(),
            delta: delta.clone(),
            tool_schemas: request.tool_schemas.clone(),
            turn: self.open_turn.clone(),
            position,
        };
        self.barrier.set_place_owed(true);
        self.place = Some(PlaceTransaction {
            request,
            units,
            pending: Some(pending),
            delta,
            measurement: None,
            facts: None,
            author: None,
            recorded_authors: vec![],
            bound,
            recorded: None,
            driving: true,
            cancelled: false,
            done: false,
            refusal: None,
            staged,
            incomplete: None,
            started_ns: self.last_ns,
            close_pending: false,
            // Repair retains original requests that still owe terminal facts.
            // Its caller judges the refusal before any turn/world close.
            close_on_refusal: self.repair.is_none(),
            account_deadline: None,
            selected_count,
            return_author: None,
            placement_after_returns: None,
        });
        let result = self.begin_measurement(measure, PumpDeadline::Decision, decode, io);
        self.place.as_mut().unwrap().driving = false;
        if let Err(error) = result {
            let mut work = self.place.take().unwrap();
            let _ = self
                .boot_journal
                .as_mut()
                .unwrap()
                .cancel(work.pending.as_ref().unwrap());
            work.pending.take();
            let mut input = work.request;
            input.selection = work
                .units
                .split_off(work.units.len() - selected_count)
                .into_iter()
                .map(|u| Arc::try_unwrap(u).expect("new request units remain exclusive"))
                .collect();
            self.barrier.set_place_owed(self.place_owed());
            return Err(PlaceBeginError {
                input,
                refusal: PlaceError::Busy(None),
                failure: error.failure,
                staged: work.staged,
            });
        }
        Ok(())
    }
    pub(in crate::flow) fn advance_place<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<PlaceProgress, RunFailure> {
        self.live(ports.io)?;
        let Some(work) = self.place.as_ref() else {
            return Err(RunFailure::PlaceOwned);
        };
        if self.deadlines.get(PumpDeadline::Decision) != Some(work.bound) {
            return Err(self.place_fault(ports.io, "placement lost its original Decision bound"));
        }
        if work.done {
            return Ok(PlaceProgress::Complete);
        }
        if work.measurement.is_none() {
            self.place.as_mut().unwrap().driving = true;
            let result = (|| {
                self.advance_measurement(ports)?;
                let revision = self.boot_journal.as_ref().unwrap().revision().into();
                self.take_measurement(
                    revision,
                    ports.decode.as_deref_mut().ok_or(RunFailure::PlaceOwned)?,
                    ports.io,
                )
            })();
            self.place.as_mut().unwrap().driving = false;
            let Some(measured) = result? else {
                return Ok(PlaceProgress::Pending);
            };
            let work = self.place.as_mut().unwrap();
            work.facts = match &measured {
                MeasureCompletion::Measured(m) => Some(*m.facts()),
                _ => None,
            };
            work.measurement = Some(measured);
            if work.cancelled || matches!(work.measurement, Some(MeasureCompletion::Cancelled(_))) {
                work.cancelled = true;
                work.done = true;
                return Ok(PlaceProgress::Complete);
            }
            if let Some(MeasureCompletion::Refused(refused)) = &work.measurement {
                work.refusal = Some(PlaceRefused {
                    assembly_id: work.measurement.as_ref().unwrap().request().assembly_id,
                    code: refused.code(),
                    detail: "decode measurement refused".into(),
                    account: None,
                });
                if !work.close_on_refusal {
                    work.done = true;
                    return Ok(PlaceProgress::Complete);
                }
                work.author = Some(PlaceAuthor::new(place_event(
                    Kind::TurnClosed,
                    AuthoredSource::Runtime,
                    Payload::TurnClosed(weaver_trace::TurnClose::Stopped {
                        reason: weaver_trace::StopReason::Refused,
                        cause: None,
                    }),
                    self.open_turn.clone(),
                    None,
                )));
                return Ok(PlaceProgress::Progress);
            }
            let facts = work.facts.unwrap();
            let limits = self.place_engine.as_ref().unwrap().limits();
            let event = if let Err(code) = crate::flow::place::fit(&facts, limits) {
                let refusal = PlaceRefused {
                    assembly_id: work.measurement.as_ref().unwrap().request().assembly_id,
                    code,
                    detail: "measured argument exceeds declared placement budget".into(),
                    account: None,
                };
                let record = RefusalRecord::Placement {
                    assembly_id: refusal.assembly_id,
                    code,
                    detail: refusal.detail.clone(),
                };
                work.close_pending = work.close_on_refusal;
                work.refusal = Some(refusal);
                place_event(
                    Kind::Refusal,
                    AuthoredSource::Runtime,
                    Payload::Refusal(serde_json::value::to_raw_value(&record).map_err(|e| {
                        RunFailure::Flow {
                            reason: FlowHaltReason::ProtocolFailed,
                            point: HaltPoint::Barrier,
                            detail: e.to_string(),
                        }
                    })?),
                    self.open_turn.clone(),
                    None,
                )
            } else {
                let record = placement_record(work, limits, self.last_ns);
                place_event(
                    Kind::StatePlacement,
                    AuthoredSource::Loop,
                    Payload::Placement(Box::new(record)),
                    self.open_turn.clone(),
                    Some(
                        self.place_engine
                            .as_ref()
                            .unwrap()
                            .boot
                            .repaired()
                            .checked_open()
                            .unwrap()
                            .head(),
                    ),
                )
            };
            if event.kind == Kind::StatePlacement {
                self.place.as_mut().unwrap().placement_after_returns = Some(event);
                self.prepare_place_return_author(ports.io)?;
            } else {
                self.place.as_mut().unwrap().author = Some(PlaceAuthor::new(event));
            }
        }
        if self
            .place
            .as_ref()
            .unwrap()
            .author
            .as_ref()
            .is_some_and(|a| a.sequence.is_some())
        {
            self.consume_place_completion()?;
            if self.place.as_ref().unwrap().return_author.is_some() {
                self.retain_place_return_record(ports.io)?;
                let work = self.place.as_mut().unwrap();
                work.author = None;
                if work.cancelled {
                    work.done = true;
                    return Ok(PlaceProgress::Complete);
                }
                self.prepare_place_return_author(ports.io)?;
            } else if self.place.as_ref().unwrap().close_pending {
                let work = self.place.as_mut().unwrap();
                work.close_pending = false;
                work.author = Some(PlaceAuthor::new(place_event(
                    Kind::TurnClosed,
                    AuthoredSource::Runtime,
                    Payload::TurnClosed(weaver_trace::TurnClose::Stopped {
                        reason: weaver_trace::StopReason::Refused,
                        cause: None,
                    }),
                    self.open_turn.clone(),
                    None,
                )));
            } else {
                self.place.as_mut().unwrap().done = true;
                return Ok(PlaceProgress::Complete);
            }
        }
        self.advance_place_author(ports)?;
        Ok(if self.place.as_ref().unwrap().done {
            PlaceProgress::Complete
        } else {
            PlaceProgress::Pending
        })
    }
    pub(in crate::flow) fn take_placement(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<Option<PlacementCompletion>, RunFailure> {
        self.live(io)?;
        if self.phase != RecordingPhase::Entered || !self.place.as_ref().is_some_and(|w| w.done) {
            return Ok(None);
        }
        self.consume_place_completion()?;
        let mut work = self.place.take().unwrap();
        if work.cancelled || work.refusal.is_some() {
            if let Some(pending) = &work.pending {
                self.boot_journal
                    .as_mut()
                    .unwrap()
                    .cancel(pending)
                    .map_err(|e| self.place_fault(io, format!("cancel placement: {e:?}")))?;
            }
            self.deadlines.disarm(PumpDeadline::Decision);
            self.barrier.set_place_owed(self.place_owed());
            return Ok(Some(if let Some(mut refusal) = work.refusal.take() {
                refusal.account = Some(PlaceAccount {
                    phase: self.phase,
                    work: AccountWork::Place(Box::new(work)),
                    measure: None,
                });
                PlacementCompletion::Refused(refusal)
            } else {
                PlacementCompletion::Cancelled(PlaceAccount {
                    phase: self.phase,
                    work: AccountWork::Place(Box::new(work)),
                    measure: None,
                })
            }));
        }
        let identity = Arc::new(());
        self.place_engine.as_mut().unwrap().outstanding = Some(identity.clone());
        let grant = PlacementGrant {
            identity,
            origin: self.read_origin.clone(),
            pending: work.pending.take().unwrap(),
            delta: work.delta,
            units: work.units,
            schemas: work.request.tool_schemas,
            facts: work.facts.unwrap(),
            recorded: work.recorded.expect("Recorded placement"),
            turn: self.open_turn.clone().ok_or(RunFailure::PlaceOwned)?,
            staged: work.staged,
        };
        self.deadlines.disarm(PumpDeadline::Decision);
        Ok(Some(PlacementCompletion::Granted(grant)))
    }
    fn place_response_ready(&self) -> bool {
        self.phase == RecordingPhase::Entered
            && self.failure.is_none()
            && self.place.as_ref().is_none_or(|w| w.done)
            && self.reload.as_ref().is_none_or(|w| w.done)
            && self.generation.as_ref().is_none_or(|w| w.done)
            && self.native_marks.as_ref().is_none_or(|w| w.done)
            && self.place_notice.as_ref().is_none_or(|w| w.done)
    }
    pub(in crate::flow) fn place_close(&self) -> Option<&weaver_types::TurnFrame> {
        if self.place_response_ready() {
            self.place_close.as_ref()
        } else {
            None
        }
    }
    /// A measured refusal already owns its refusal/turn-close transaction.
    /// Enclosing lifecycle composition settles that transaction before judging
    /// whether Stop still has an open turn to abort.
    pub(in crate::flow) fn placement_refusing(&self) -> bool {
        self.place
            .as_ref()
            .is_some_and(|work| work.refusal.is_some())
    }
    pub(in crate::flow) fn place_response_sent(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        self.live(io)?;
        if !self.place_response_ready() || self.place_close.take().is_none() {
            return Err(RunFailure::PlaceOwned);
        }
        Ok(())
    }
    fn place_instruction_valid(&self, context: &PlacementContext) -> bool {
        let Some(source) = &context.instruction else {
            return true;
        };
        self.place_instructions.iter().any(|proof|proof.belongs_to(self)&&proof.receipts().dispositions().any(|d|match d {
            crate::flow::ledger::SourceDisposition::Committed(c)=>matches!(c.file(),crate::flow::ledger::LedgerFile::Statement(file) if &file.block_id==source && file.source.kind=="loop.instruction" && file.source.origin=="loop"),_=>false}))
    }
    pub(in crate::flow) fn retain_place_instruction(
        &mut self,
        proof: FilingCompletion,
    ) -> Result<(), FilingCompletion> {
        if !proof.belongs_to(self)||proof.receipts().dispositions().all(|d|!matches!(d,crate::flow::ledger::SourceDisposition::Committed(c) if matches!(c.file(),crate::flow::ledger::LedgerFile::Statement(f) if f.source.kind=="loop.instruction"&&f.source.origin=="loop"))){return Err(proof)}
        self.place_instructions.push(proof);
        Ok(())
    }
    fn active_place_author(&self) -> Option<&PlaceAuthor> {
        self.place
            .as_ref()
            .and_then(|w| w.author.as_ref())
            .or_else(|| self.generation.as_ref().and_then(|w| w.author.as_ref()))
            .or_else(|| self.reload.as_ref().and_then(|w| w.author.as_ref()))
            .or_else(|| self.native_marks.as_ref().and_then(|w| w.author.as_ref()))
            .or_else(|| self.place_notice.as_ref().and_then(|w| w.author.as_ref()))
    }
    fn active_place_author_mut(&mut self) -> Option<&mut PlaceAuthor> {
        if let Some(w) = &mut self.place
            && w.author.is_some()
        {
            return w.author.as_mut();
        }
        if let Some(w) = &mut self.generation
            && w.author.is_some()
        {
            return w.author.as_mut();
        }
        if let Some(w) = &mut self.reload
            && w.author.is_some()
        {
            return w.author.as_mut();
        }
        if let Some(w) = &mut self.native_marks
            && w.author.is_some()
        {
            return w.author.as_mut();
        }
        self.place_notice.as_mut().and_then(|w| w.author.as_mut())
    }
    #[cfg(test)]
    pub(super) fn place_author_record_state(
        &self,
    ) -> Option<(Option<u64>, Option<Sequence>, Kind)> {
        self.active_place_author()
            .map(|a| (a.ticket, a.sequence, a.kind))
    }
    #[cfg(test)]
    pub(super) fn place_author_identity_for_test(&self) -> Option<Arc<()>> {
        self.active_place_author().map(|a| Arc::clone(&a.identity))
    }
    pub(super) fn place_owns_pending_author(&self) -> bool {
        self.active_place_author()
            .is_some_and(|a| a.sequence.is_none() && (a.ticket.is_some() || a.event.is_none()))
    }
    pub(super) fn claim_place_author_ticket(&mut self, identity: &Arc<()>, ticket: u64) -> bool {
        let Some(author) = self.active_place_author_mut() else {
            return false;
        };
        if author.event.is_some() {
            return false;
        }
        author.record.claim(identity, ticket)
    }

    pub(super) fn bind_place_author(&mut self, sequence: Sequence) {
        let Some(author) = self.active_place_author() else {
            return;
        };
        if author.sequence == Some(sequence) {
            return;
        }
        let recorded_ticket = self.barrier.recorded_author_ticket(sequence);
        if author.event.is_some() {
            return;
        }
        if !self
            .active_place_author_mut()
            .unwrap()
            .record
            .bind(recorded_ticket, sequence)
        {
            return;
        }
        let closing = self
            .active_place_author()
            .filter(|a| a.event.is_none() && a.kind == Kind::TurnClosed)
            .and_then(|a| a.turn.clone());
        if closing.is_some() && self.open_turn == closing {
            self.open_turn = None;
            self.place_turn_stopped = false;
        }

        macro_rules! retain {
            ($work:expr) => {
                if let Some(work) = $work.as_mut() {
                    if work
                        .author
                        .as_ref()
                        .is_some_and(|a| a.sequence == Some(sequence))
                        && !work.recorded_authors.contains(&sequence)
                    {
                        work.recorded_authors.push(sequence);
                    }
                }
            };
        }
        retain!(self.place);
        retain!(self.generation);
        retain!(self.reload);
        retain!(self.place_notice);
        if let Some(work) = self.place.as_mut()
            && work
                .author
                .as_ref()
                .is_some_and(|a| a.sequence == Some(sequence))
        {
            if work.return_author.is_some() {
                return;
            }
            if work.refusal.is_none() {
                work.recorded = Some(sequence);
            }
            work.done = !work.close_pending;
            if work.done
                && work.close_on_refusal
                && let Some(refusal) = &work.refusal
            {
                let run = self
                    .place_engine
                    .as_ref()
                    .unwrap()
                    .boot
                    .repaired()
                    .checked_open()
                    .unwrap()
                    .run();
                let frame = if let Some(turn) = work.author.as_ref().and_then(|a| a.turn.as_ref()) {
                    serde_json::json!({"kind":"stopped","reason":refusal.detail,"run":run,"turn":turn.0})
                } else {
                    serde_json::json!({"kind":"refused","reason":refusal.detail})
                };
                self.place_close =
                    Some(weaver_types::TurnFrame::carry(frame.to_string().as_bytes()));
            }
        }
    }
    fn consume_place_completion(&mut self) -> Result<(), RunFailure> {
        if let Some(sequence) = self.completed_author {
            if self.active_place_author().and_then(|a| a.sequence) != Some(sequence) {
                return Err(RunFailure::PlaceOwned);
            }
            self.completed_author = None;
        }
        Ok(())
    }
    fn advance_place_author<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<(), RunFailure> {
        let Some(author) = self.active_place_author() else {
            return Ok(());
        };
        author.returned.check()?;
        if author.sequence.is_some() {
            self.consume_place_completion()?;
            return Ok(());
        }
        if let Some(ticket) = author.ticket
            && self.barrier.pending_author().map(|a| a.ticket) != Some(ticket)
        {
            return Err(self.place_fault(ports.io, "placement author ticket was replaced"));
        }
        let identity = Arc::clone(&author.identity);
        if let Some(event) = self.active_place_author_mut().unwrap().event.take() {
            self.waiting_content = Some(WaitingAuthor::claimed(
                event,
                AuthorClaim::Placement(Arc::clone(&identity)),
            ));
        }
        let result =
            self.resume_claimed_author_and_wait(ports, Some(AuthorClaim::Placement(identity)));
        if let Ok(sequence) = result {
            self.complete_place_author(sequence, ports.io)?;
        }
        result.map(|_| ()).map_err(|failure| {
            self.active_place_author_mut()
                .unwrap()
                .record
                .retain_failure(failure)
        })
    }
    pub(super) fn complete_place_author(
        &mut self,
        sequence: Sequence,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        self.bind_place_author(sequence);
        if self.active_place_author().and_then(|a| a.sequence) != Some(sequence) {
            return Err(self.place_fault(io, "placement completion has another author's ticket"));
        }
        Ok(())
    }

    fn prepare_place_return_author(&mut self, io: &mut impl PumpIo) -> Result<(), RunFailure> {
        let work = self.place.as_ref().ok_or(RunFailure::PlaceOwned)?;
        for unit in &work.units {
            let Some(pair) = unit.completed_pair() else {
                continue;
            };
            match self.pairs.result_record(pair) {
                Ok(Some(_)) => continue,
                Err(error) => return Err(self.place_fault(io, format!("result record: {error:?}"))),
                Ok(None) => {}
            }
            let call = pair.started().call_id.clone();
            let parent = Sequence(pair.terminal_source().sequence.0);
            let message = Message {
                role: weaver_traits::Role::ToolResult,
                content: vec![weaver_traits::ContentBlock::ToolResult(
                    pair.result().block(),
                )],
            };
            let event = place_event(
                Kind::MessageToolResult,
                AuthoredSource::Tool,
                Payload::Message(
                    serde_json::value::to_raw_value(&message)
                        .map_err(|e| self.place_fault(io, e.to_string()))?,
                ),
                self.open_turn.clone(),
                Some(parent),
            );
            let work = self.place.as_mut().unwrap();
            work.return_author = Some(call);
            work.author = Some(PlaceAuthor::new(event));
            return Ok(());
        }
        let work = self.place.as_mut().unwrap();
        work.author = work.placement_after_returns.take().map(PlaceAuthor::new);
        if work.author.is_none() {
            return Err(self.place_fault(io, "result author lost its placement record"));
        }
        Ok(())
    }

    pub(super) fn retain_place_return_record(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        let Some(work) = self.place.as_ref() else {
            return Ok(());
        };
        let Some(call) = &work.return_author else {
            return Ok(());
        };
        let sequence = work
            .author
            .as_ref()
            .and_then(|a| a.sequence)
            .ok_or(RunFailure::PlaceOwned)?;
        self.pairs
            .record_result(call, sequence)
            .map_err(|e| self.place_fault(io, format!("result record receipt: {e:?}")))?;
        self.place.as_mut().unwrap().return_author = None;
        Ok(())
    }

    pub(super) fn acknowledge_delivery(
        &mut self,
        call: &weaver_types::state_flow::CallId,
        proof: &ConfirmedGeneration,
    ) -> Result<(), pairs::PairError> {
        if !Arc::ptr_eq(&proof.grant.origin, &self.read_origin) {
            return Err(pairs::PairError::WrongReceipt);
        }
        self.pairs.acknowledge_delivery(call, proof)
    }
}
fn place_event(
    kind: Kind,
    source: AuthoredSource,
    payload: Payload,
    turn: Option<TurnRef>,
    parent: Option<Sequence>,
) -> AuthoredEvent {
    AuthoredEvent {
        kind,
        subsystem: Subsystem::Harness,
        turn,
        source,
        causal_parent: parent,
        payload: Some(payload),
    }
}
fn placement_record(
    work: &PlaceTransaction,
    limits: (u64, u64, u64, u64),
    now: u64,
) -> t::PlacementRecord {
    let facts = work.facts.unwrap();
    let pending = work.pending.as_ref().unwrap();
    t::PlacementRecord {
        assembly_id: pending.assembly().into(),
        phase: t::PlacementPhase::Append,
        context: Some(t::PlacementContext {
            label: work.request.context.label.clone(),
            instruction: work
                .request
                .context
                .instruction
                .as_ref()
                .map(crate::flow::mirror::source),
        }),
        cut: work.request.prefix.cut.as_ref().map(cut_mirror),
        method: work.request.prefix.method.map(method_mirror),
        n: facts.keep,
        prefix_tokens: facts.prefix_tokens,
        resident_before: facts.resident,
        input_tokens: facts.input_tokens,
        capacity: facts.capacity,
        generation_limit: limits.2,
        terminal_tokens: limits.3,
        unit_map: pending.map(&work.units),
        message_ids: pending.ids(),
        assistant_message_id: Some(pending.assistant_id().0.into()),
        recomputed_messages: (pending.recomputed() as u64).into(),
        wait_ns: now.saturating_sub(work.started_ns).into(),
        open_evidence: None,
    }
}
fn method_mirror(method: CutMethod) -> t::CutMethod {
    match method {
        CutMethod::Flush => t::CutMethod::Flush,
        CutMethod::Elide => t::CutMethod::Elide,
    }
}
fn cut_mirror(cut: &CutReference) -> t::CutReference {
    t::CutReference {
        span: crate::flow::mirror::span_ref(&cut.span),
        occurrence_id: t::MarkOccurrenceId {
            run: cut.occurrence_id.run.clone(),
            ordinal: cut.occurrence_id.ordinal.0.into(),
        },
    }
}

#[cfg(test)]
impl RecordingContext {
    /// Defensive consumer-corruption probes start with a genuinely completed cut.
    /// No fixture can manufacture a Staged, receipt, completion or phase here.
    pub(super) fn review_mark_completed_reload_incomplete(&mut self) {
        let work = self.reload.as_mut().expect("actual reload");
        assert!(work.done && work.incomplete.is_none());
        assert!(
            work.staged
                .as_ref()
                .is_some_and(|staged| staged.recorded.is_some())
        );
        work.incomplete = Some("defensive completed-account interruption".into());
    }

    pub(super) fn review_remove_completed_reload_record(&mut self) -> Sequence {
        let work = self.reload.as_mut().expect("actual reload");
        assert!(work.done && work.incomplete.is_none());
        work.staged
            .as_mut()
            .expect("actual confirmed cut")
            .recorded
            .take()
            .expect("actual Recorded")
    }
}
