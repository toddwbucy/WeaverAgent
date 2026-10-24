//! Dormant committed PlanSummary evidence. These private projections grant no
//! plan effect, management input, placement, generation or lifecycle readiness.
use super::clock::AbsoluteBound;
use super::context::RecordedRead;
use super::ledger::{CommittedSource, LedgerFile};
use super::pump::PumpDeadline;
use std::sync::Arc;
use weaver_types::state_flow::{
    Digest, PlanEffect, PlanRef, PlanSummary, QueryResult, ReadSelector, SourcePathPart,
    SourcePosition, StartIdentity, StatementEffect, decode_strict,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum PlanSummaryUnavailable {
    Uninitialized,
    Pending,
    Revoked,
    Failed,
}

/// Shared immutable custody lets the cache and its consuming continuation retain
/// the same actual proof. Neither view can construct, clone or modify that proof.
#[derive(Debug)]
pub(in crate::flow) struct CommittedPlanSummary {
    read: RecordedRead,
    summary: Option<PlanSummary>,
}
impl CommittedPlanSummary {
    pub(in crate::flow) fn read(&self) -> &RecordedRead {
        &self.read
    }
    pub(in crate::flow) fn summary(&self) -> Option<&PlanSummary> {
        self.summary.as_ref()
    }
    #[allow(clippy::result_large_err)] // Refusal retains the original immutable read.
    pub(super) fn from_recorded(
        read: RecordedRead,
        expected: Option<&EffectExpectation>,
    ) -> Result<Self, (RecordedRead, &'static str)> {
        if !matches!(read.selector(), ReadSelector::PlanCurrent {}) {
            return Err((read, "summary requires the actual plan_current read"));
        }
        let QueryResult::PlanCurrent { current, .. } = read.result() else {
            return Err((read, "summary result does not match plan_current"));
        };
        let summary = current.as_ref().map(|plan| PlanSummary {
            plan: plan.plan.clone(),
            status: plan.status,
            document_sha256: plan.document_sha256.clone(),
            last_source: plan.last_source.clone(),
        });
        if let Some(expected) = expected {
            let Some(actual) = &summary else {
                return Err((read, "committed plan effect requires a current plan"));
            };
            if actual.plan != expected.plan || actual.last_source != expected.source {
                return Err((
                    read,
                    "plan summary differs from the committed effect identity",
                ));
            }
            if expected
                .digest
                .as_ref()
                .is_some_and(|digest| digest != &actual.document_sha256)
            {
                return Err((
                    read,
                    "plan summary differs from held operator document digest",
                ));
            }
        }
        Ok(Self { read, summary })
    }
}

#[derive(Debug)]
pub(super) struct EffectExpectation {
    pub source: SourcePosition,
    pub plan: PlanRef,
    pub digest: Option<Digest>,
    pub receipt: Arc<()>,
    pub key: PumpDeadline,
    pub bound: AbsoluteBound,
}
impl EffectExpectation {
    /// This accepts only an actual private committed disposition. It does not
    /// authorize the effect that already happened or create trusted root input.
    #[allow(clippy::too_many_arguments)] // All arguments are independently held owner facts.
    pub(super) fn from_commit(
        commit: &CommittedSource,
        start: &StartIdentity,
        run: &str,
        session: &str,
        receipt: Arc<()>,
        key: PumpDeadline,
        bound: AbsoluteBound,
        previous: Option<&PlanSummary>,
    ) -> Result<Self, &'static str> {
        let LedgerFile::Statement(file) = commit.file() else {
            return Err("plan summary prerequisite is not a statement receipt");
        };
        let Some(StatementEffect::Plan(effect)) = file.effect.as_ref() else {
            return Err("plan summary prerequisite has no committed plan effect");
        };
        let common = commit.common();
        if common.run_id != run
            || common.session_id != session
            || file.source.run != run
            || file.source.session != session
            || file.block_id.run != run
            || file.block_id.sequence != file.source.sequence
            || file.block_id.block.0 != 0
            || file.source.subsystem != "harness"
            || file.source.channel != "internal"
            || file.source.origin != "loop"
            || !matches!(file.source.kind.as_str(), "plan.event" | "step.outcome")
        {
            return Err("plan effect receipt has a foreign source envelope");
        }
        let context = file
            .context
            .as_ref()
            .ok_or("plan effect receipt lost source context")?;
        if context.source_path != [SourcePathPart::Key("context".into())] {
            return Err("plan effect receipt has another context path");
        }
        let held: PlanEffect =
            decode_strict(context.raw.get()).map_err(|_| "plan effect context is malformed")?;
        if &held != effect.as_ref()
            || (file.source.kind == "step.outcome") != matches!(held, PlanEffect::Close { .. })
        {
            return Err("plan effect receipt differs from its held source context");
        }
        let plan = match effect.as_ref() {
            PlanEffect::Authorize { plan, .. } | PlanEffect::Lock { plan, .. } => plan,
            PlanEffect::Replace { next, .. } => next,
            PlanEffect::Activate { expected, .. }
            | PlanEffect::Close { expected, .. }
            | PlanEffect::Break { expected, .. } => expected,
        }
        .clone();
        if plan.plan_id.seed_id != start.seed_id {
            return Err("plan effect receipt has another plan seed");
        }
        // A prior cache is independent only for this exact revision. Replacement
        // never borrows the old revision's operator digest. Future accepted root
        // input can extend this proof; a free Digest parameter cannot.
        let digest = previous
            .filter(|old| old.plan == plan)
            .map(|old| old.document_sha256.clone());
        Ok(Self {
            source: SourcePosition {
                block_id: file.block_id.clone(),
                turn: file.source.turn.clone(),
            },
            plan,
            digest,
            receipt,
            key,
            bound,
        })
    }
}

pub(super) struct PlanSummaryCache {
    pub committed: Option<Arc<CommittedPlanSummary>>,
    pub required: Option<EffectExpectation>,
    pub unavailable: Option<PlanSummaryUnavailable>,
    pub invalid: Option<&'static str>,
    pub boot_started: bool,
}
impl Default for PlanSummaryCache {
    fn default() -> Self {
        Self {
            committed: None,
            required: None,
            unavailable: Some(PlanSummaryUnavailable::Uninitialized),
            invalid: None,
            boot_started: false,
        }
    }
}
impl PlanSummaryCache {
    pub(super) fn fail(&mut self) {
        self.unavailable = Some(PlanSummaryUnavailable::Failed);
    }
    pub(super) fn revoke(&mut self) {
        if self.unavailable != Some(PlanSummaryUnavailable::Failed) {
            self.unavailable = Some(PlanSummaryUnavailable::Revoked);
        }
    }
}
