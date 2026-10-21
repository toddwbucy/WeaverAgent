//! HARNESS-ACCOUNT and FLOW-COUNTS accounting, with no clock or transport owner.
//!
//! The coordinator supplies disjoint installed/queued obligations and measured
//! timestamps. A waiting copy has no ledger admission: it contributes its identity
//! and turn, but not pending traffic or reserved-source totals. A terminal sample
//! may exclude its disposition from the owed set while its reservation stays held
//! until RecordedFlow permits release. Counters advance once at validated result
//! reduction; only a matching RecordedFlow advances the sample baseline.

use std::collections::HashSet;

use weaver_trace::serving::payloads::{
    BlockedStage, CopyAtHalt, FlowObservation, FlowPhase, SourceBlockId as TraceSource,
};
use weaver_types::state_flow::{MeasuredInterval, SourceBlockId};

use super::recording::{RecordedFlow, RecordedTerminal};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ObservationError {
    InvalidDeclaration,
    InvalidIdentity,
    DuplicateSource,
    InvalidTransition,
    CounterOverflow,
    InputUnderflow,
    ClockReversed,
    AllocationFailed,
    NoPendingSample,
    ProofMismatch,
    Finalized,
}

impl std::fmt::Display for ObservationError {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(out, "flow observation: {self:?}")
    }
}

impl std::error::Error for ObservationError {}

/// Facts borrowed from the ledger or a queued copy, once per owed source block.
/// Context bytes are traffic once per obligation, unlike shared retention charge.
pub(super) struct PendingSource<'a> {
    pub source: &'a SourceBlockId,
    pub turn: Option<&'a str>,
    pub raw_bytes: u64,
    pub context_bytes: u64,
    pub intake_ns: u64,
}

pub(super) struct WaitingSource<'a> {
    pub copy: &'a CopyAtHalt,
    pub turn: Option<&'a str>,
}

/// Constructible only from the owning flow's explicit facts. No omitted producer
/// is silently represented by zero, and no caller can write a final gauge field.
#[derive(Debug, Clone)]
pub(super) struct GaugeSnapshot {
    run: String,
    pending_count: u64,
    pending_bytes: u64,
    oldest_intake_ns: Option<u64>,
    newest_intake_ns: Option<u64>,
    input_pending: u64,
    retained_blocks: u64,
    retained_bytes: u64,
    pending_turns: u64,
    waiting_copy: Option<CopyAtHalt>,
}

impl GaugeSnapshot {
    pub(super) fn from_sources<'a>(
        run: &str,
        sources: impl IntoIterator<Item = PendingSource<'a>>,
        input_pending: u64,
        retained: (u64, u64),
        waiting: Option<WaitingSource<'a>>,
    ) -> Result<Self, ObservationError> {
        if run.is_empty() {
            return Err(ObservationError::InvalidIdentity);
        }
        let mut ids = HashSet::new();
        let mut turns = HashSet::new();
        let mut snapshot = Self {
            run: run.into(),
            pending_count: 0,
            pending_bytes: 0,
            oldest_intake_ns: None,
            newest_intake_ns: None,
            input_pending,
            retained_blocks: retained.0,
            retained_bytes: retained.1,
            pending_turns: 0,
            waiting_copy: None,
        };
        for source in sources {
            if source.source.run != run {
                return Err(ObservationError::InvalidIdentity);
            }
            insert_source(&mut ids, (source.source.sequence.0, source.source.block.0))?;
            insert_turn(&mut turns, source.turn)?;
            snapshot.pending_count = add(snapshot.pending_count, 1)?;
            snapshot.pending_bytes = add(
                snapshot.pending_bytes,
                add(source.raw_bytes, source.context_bytes)?,
            )?;
            snapshot.oldest_intake_ns = Some(
                snapshot
                    .oldest_intake_ns
                    .map_or(source.intake_ns, |oldest| oldest.min(source.intake_ns)),
            );
            snapshot.newest_intake_ns = Some(
                snapshot
                    .newest_intake_ns
                    .map_or(source.intake_ns, |newest| newest.max(source.intake_ns)),
            );
        }
        if let Some(waiting) = waiting {
            let copy = waiting.copy;
            if copy.source_event.run != run || copy.partition_rule_id.is_empty() {
                return Err(ObservationError::InvalidIdentity);
            }
            let mut previous = None;
            for source in &copy.sources {
                if source.run != run || source.sequence != copy.source_event.sequence {
                    return Err(ObservationError::InvalidIdentity);
                }
                if previous.is_some_and(|previous| source.block.0 <= previous) {
                    return Err(ObservationError::DuplicateSource);
                }
                previous = Some(source.block.0);
                insert_source(&mut ids, (source.sequence.0, source.block.0))?;
            }
            insert_turn(&mut turns, waiting.turn)?;
            snapshot.waiting_copy = Some(copy.clone());
        }
        snapshot.pending_turns =
            u64::try_from(turns.len()).map_err(|_| ObservationError::CounterOverflow)?;
        Ok(snapshot)
    }

    pub(super) fn work_pending(&self) -> bool {
        self.pending_count != 0 || self.input_pending != 0 || self.waiting_copy.is_some()
    }
}

fn insert_source(ids: &mut HashSet<(u64, u64)>, id: (u64, u64)) -> Result<(), ObservationError> {
    ids.try_reserve(1)
        .map_err(|_| ObservationError::AllocationFailed)?;
    if !ids.insert(id) {
        return Err(ObservationError::DuplicateSource);
    }
    Ok(())
}

fn insert_turn<'a>(
    turns: &mut HashSet<&'a str>,
    turn: Option<&'a str>,
) -> Result<(), ObservationError> {
    if let Some(turn) = turn {
        if turn.is_empty() {
            return Err(ObservationError::InvalidIdentity);
        }
        turns
            .try_reserve(1)
            .map_err(|_| ObservationError::AllocationFailed)?;
        turns.insert(turn);
    }
    Ok(())
}

fn add(left: u64, right: u64) -> Result<u64, ObservationError> {
    left.checked_add(right)
        .ok_or(ObservationError::CounterOverflow)
}

#[derive(Debug, Clone)]
pub(super) struct Transition {
    pub phase: FlowPhase,
    pub source_id: Option<SourceBlockId>,
    pub input_id: Option<u64>,
    pub request_id: Option<u64>,
    pub elapsed_ns: Option<u64>,
    pub blocked_stage: Option<BlockedStage>,
    pub failed_corrections: Option<u64>,
    pub retry_cap: Option<u64>,
}

impl Transition {
    pub(super) fn new(phase: FlowPhase) -> Self {
        Self {
            phase,
            source_id: None,
            input_id: None,
            request_id: None,
            elapsed_ns: None,
            blocked_stage: None,
            failed_corrections: None,
            retry_cap: None,
        }
    }

    fn validate(&self, run: &str) -> Result<(), ObservationError> {
        if self
            .source_id
            .as_ref()
            .is_some_and(|source| source.run != run)
        {
            return Err(ObservationError::InvalidIdentity);
        }
        let repair = matches!(
            self.phase,
            FlowPhase::RepairRequested | FlowPhase::RepairExhausted
        );
        if self.failed_corrections.is_some() != repair
            || self.retry_cap.is_some() != repair
            || (repair && self.source_id.is_none())
            || (matches!(self.phase, FlowPhase::Filed | FlowPhase::CorrectlyRejected)
                && self.source_id.is_none())
        {
            return Err(ObservationError::InvalidTransition);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Counts {
    filed: u64,
    filed_bytes: u64,
    rejected: u64,
    input_started: u64,
    input_completed: u64,
}

impl Counts {
    fn since(self, baseline: Self) -> Result<Self, ObservationError> {
        let sub = |value: u64, base: u64| {
            value
                .checked_sub(base)
                .ok_or(ObservationError::CounterOverflow)
        };
        Ok(Self {
            filed: sub(self.filed, baseline.filed)?,
            filed_bytes: sub(self.filed_bytes, baseline.filed_bytes)?,
            rejected: sub(self.rejected, baseline.rejected)?,
            input_started: sub(self.input_started, baseline.input_started)?,
            input_completed: sub(self.input_completed, baseline.input_completed)?,
        })
    }
}

#[derive(Debug)]
struct FrozenSample {
    sample: FlowObservation,
    totals: Counts,
}

/// Facts at one semantic transition, before it owns the sole author ticket.
/// Interval deltas are derived only when earlier points have been Recorded.
#[derive(Debug, Clone)]
pub(super) struct SamplePoint {
    run: String,
    sample: FlowObservation,
    totals: Counts,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Prepared {
    Prepared,
    Busy,
}

/// Counts validated dispositions, not transport polling or attempted authoring.
/// The coordinator must not call a note method twice for the same transition.
pub(super) struct Observation {
    run: String,
    interval_ns: u64,
    lag_alarm_turns: u64,
    baseline_ns: u64,
    latest_capture_ns: u64,
    baseline: Counts,
    totals: Counts,
    pending: Option<FrozenSample>,
    finalized: bool,
}

impl Observation {
    pub(super) fn run(&self) -> &str {
        &self.run
    }
    pub(super) fn new(
        run: String,
        open_confirmed_ns: u64,
        observation_interval: MeasuredInterval,
        lag_alarm_turns: u64,
    ) -> Result<Self, ObservationError> {
        if run.is_empty() || lag_alarm_turns == 0 || observation_interval.validate().is_err() {
            return Err(ObservationError::InvalidDeclaration);
        }
        Ok(Self {
            run,
            interval_ns: observation_interval.elapsed_ns.0,
            lag_alarm_turns,
            baseline_ns: open_confirmed_ns,
            latest_capture_ns: open_confirmed_ns,
            baseline: Counts::default(),
            totals: Counts::default(),
            pending: None,
            finalized: false,
        })
    }

    pub(super) fn note_filed(&mut self, traffic_bytes: u64) -> Result<(), ObservationError> {
        self.active()?;
        let filed = add(self.totals.filed, 1)?;
        let filed_bytes = add(self.totals.filed_bytes, traffic_bytes)?;
        self.totals.filed = filed;
        self.totals.filed_bytes = filed_bytes;
        Ok(())
    }

    pub(super) fn note_rejected(&mut self) -> Result<(), ObservationError> {
        self.active()?;
        self.totals.rejected = add(self.totals.rejected, 1)?;
        Ok(())
    }

    pub(super) fn note_input_started(&mut self) -> Result<(), ObservationError> {
        self.active()?;
        self.totals.input_started = add(self.totals.input_started, 1)?;
        Ok(())
    }

    pub(super) fn note_input_completed(&mut self) -> Result<(), ObservationError> {
        self.active()?;
        if self.totals.input_completed >= self.totals.input_started {
            return Err(ObservationError::InputUnderflow);
        }
        self.totals.input_completed = add(self.totals.input_completed, 1)?;
        Ok(())
    }

    pub(super) fn prepare(
        &mut self,
        transition: Transition,
        gauges: GaugeSnapshot,
        sampled_ns: u64,
    ) -> Result<Prepared, ObservationError> {
        self.active()?;
        if self.pending.is_some() {
            return Ok(Prepared::Busy);
        }
        let point = self.capture(transition, gauges, sampled_ns)?;
        self.prepare_captured(&point)
    }

    /// Freeze gauges, correlation, time and totals while the transition's
    /// state still exists. Later intake or release cannot rewrite this point.
    pub(super) fn capture(
        &mut self,
        transition: Transition,
        gauges: GaugeSnapshot,
        sampled_ns: u64,
    ) -> Result<SamplePoint, ObservationError> {
        self.active()?;
        if sampled_ns < self.latest_capture_ns {
            return Err(ObservationError::ClockReversed);
        }
        let sample = self.render(transition, gauges, sampled_ns)?;
        self.latest_capture_ns = sampled_ns;
        Ok(SamplePoint {
            run: self.run.clone(),
            sample,
            totals: self.totals,
        })
    }

    pub(super) fn prepare_captured(
        &mut self,
        point: &SamplePoint,
    ) -> Result<Prepared, ObservationError> {
        self.active()?;
        if self.pending.is_some() {
            return Ok(Prepared::Busy);
        }
        if point.run != self.run {
            return Err(ObservationError::InvalidIdentity);
        }
        if point.sample.sampled_ns.0 < self.baseline_ns {
            return Err(ObservationError::ClockReversed);
        }
        let counts = point.totals.since(self.baseline)?;
        let mut sample = point.sample.clone();
        sample.sample_start_ns = self.baseline_ns.into();
        sample.filed_count = counts.filed.into();
        sample.filed_bytes = counts.filed_bytes.into();
        sample.rejected_count = counts.rejected.into();
        sample.input_started = counts.input_started.into();
        sample.input_completed = counts.input_completed.into();
        self.pending = Some(FrozenSample {
            sample,
            totals: point.totals,
        });
        Ok(Prepared::Prepared)
    }

    pub(super) fn pending(&self) -> Option<&FlowObservation> {
        self.pending.as_ref().map(|pending| &pending.sample)
    }

    pub(super) fn recorded(
        &mut self,
        proof: RecordedFlow,
    ) -> Result<Option<RecordedTerminal>, ObservationError> {
        self.active()?;
        let pending = self
            .pending
            .as_ref()
            .ok_or(ObservationError::NoPendingSample)?;
        if proof.run() != self.run || proof.sample() != &pending.sample {
            return Err(ObservationError::ProofMismatch);
        }
        let pending = self.pending.take().expect("matched pending sample");
        self.baseline_ns = pending.sample.sampled_ns.0;
        self.baseline = pending.totals;
        Ok(proof.into_terminal())
    }

    /// A due tick is not a deadline. While authoring is pending, it coalesces
    /// without consuming another interval or changing the frozen event.
    pub(super) fn next_tick(&self, work_pending: bool) -> Result<Option<u64>, ObservationError> {
        self.active()?;
        if !work_pending || self.pending.is_some() {
            return Ok(None);
        }
        Ok(Some(add(self.baseline_ns, self.interval_ns)?))
    }

    pub(super) fn tick_due(&self, now: u64, work_pending: bool) -> Result<bool, ObservationError> {
        self.active()?;
        if now < self.latest_capture_ns
            || self
                .pending
                .as_ref()
                .is_some_and(|pending| now < pending.sample.sampled_ns.0)
        {
            return Err(ObservationError::ClockReversed);
        }
        Ok(self
            .next_tick(work_pending)?
            .is_some_and(|tick| now >= tick))
    }

    /// Returned only for the halt account, never a second state.flow event.
    /// It covers the whole interval since the last Recorded sample, including
    /// counters held by a superseded unrecorded author. Reporting owns that author.
    pub(super) fn final_sample(
        &mut self,
        gauges: GaugeSnapshot,
        sampled_ns: u64,
    ) -> Result<FlowObservation, ObservationError> {
        self.active()?;
        if sampled_ns < self.latest_capture_ns {
            return Err(ObservationError::ClockReversed);
        }
        let sample = self.render(Transition::new(FlowPhase::Sampled), gauges, sampled_ns)?;
        self.pending = None;
        self.finalized = true;
        Ok(sample)
    }

    fn active(&self) -> Result<(), ObservationError> {
        if self.finalized {
            Err(ObservationError::Finalized)
        } else {
            Ok(())
        }
    }

    fn render(
        &self,
        transition: Transition,
        gauges: GaugeSnapshot,
        sampled_ns: u64,
    ) -> Result<FlowObservation, ObservationError> {
        transition.validate(&self.run)?;
        if gauges.run != self.run {
            return Err(ObservationError::InvalidIdentity);
        }
        if sampled_ns < self.baseline_ns
            || gauges
                .newest_intake_ns
                .is_some_and(|time| sampled_ns < time)
        {
            return Err(ObservationError::ClockReversed);
        }
        let counts = self.totals.since(self.baseline)?;
        let oldest_age = match gauges.oldest_intake_ns {
            Some(start) => sampled_ns
                .checked_sub(start)
                .ok_or(ObservationError::ClockReversed)?,
            None => 0,
        };
        let backlog_waiting = gauges.waiting_copy.is_some();
        if matches!(transition.phase, FlowPhase::BacklogWait) && !backlog_waiting
            || matches!(transition.phase, FlowPhase::BacklogResumed) && backlog_waiting
        {
            return Err(ObservationError::InvalidTransition);
        }
        Ok(FlowObservation {
            phase: transition.phase,
            source_id: transition.source_id.map(|source| TraceSource {
                run: source.run,
                sequence: source.sequence.0.into(),
                block: source.block.0.into(),
            }),
            input_id: transition.input_id.map(Into::into),
            request_id: transition.request_id.map(Into::into),
            pending_count: gauges.pending_count.into(),
            oldest_age_ns: oldest_age.into(),
            elapsed_ns: transition.elapsed_ns.map(Into::into),
            blocked_stage: transition.blocked_stage,
            sample_start_ns: self.baseline_ns.into(),
            sampled_ns: sampled_ns.into(),
            pending_bytes: gauges.pending_bytes.into(),
            filed_count: counts.filed.into(),
            filed_bytes: counts.filed_bytes.into(),
            rejected_count: counts.rejected.into(),
            input_pending: gauges.input_pending.into(),
            input_started: counts.input_started.into(),
            input_completed: counts.input_completed.into(),
            retained_blocks: gauges.retained_blocks.into(),
            retained_bytes: gauges.retained_bytes.into(),
            pending_turns: gauges.pending_turns.into(),
            backlog_waiting,
            lag_alarm: backlog_waiting || gauges.pending_turns >= self.lag_alarm_turns,
            waiting_copy: gauges.waiting_copy,
            failed_corrections: transition.failed_corrections.map(Into::into),
            retry_cap: transition.retry_cap.map(Into::into),
        })
    }
}

#[cfg(test)]
#[path = "observation_tests.rs"]
mod tests;
