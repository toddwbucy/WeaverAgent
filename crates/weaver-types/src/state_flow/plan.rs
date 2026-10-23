//! Locked-plan values and atomic effects, from Spec 07 and TYPES-PLAN-CONTROL.
//!
//! Serializable evidence is distinct from the harness's private authority grants.

use serde::{Deserialize, Serialize};

use super::{
    CallId, DecimalU64, Digest, ReadRefusalCode, SourceBlockId, SourcePosition, SpanRef,
    StatementRef, StepQueryRef, TaskScopeRef,
};

/// A stable plan identity in one seeded chain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanId {
    pub seed_id: String,
    pub local_id: DecimalU64,
}

/// A stable step identity, independent of its plan ordinal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StepId {
    pub seed_id: String,
    pub local_id: DecimalU64,
}

/// One immutable plan revision address.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanRef {
    pub plan_id: PlanId,
    pub revision: DecimalU64,
}

/// Identity of an operator-selected external review procedure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewSetupRef {
    pub setup_id: String,
    pub sha256: Digest,
}

/// Attribution of a held work preview. This value grants no closure authority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkRef {
    pub plan: PlanRef,
    pub step_id: StepId,
    pub span: SpanRef,
    pub through: SourcePosition,
    pub content_sha256: Digest,
}

/// The five durable plan states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanStatus {
    Authorized,
    Locked,
    Broken,
    Complete,
    Superseded,
}

/// The four durable step states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    Waiting,
    Active,
    Closed,
    Retired,
}

/// Closed causes of a plan operation refusal or break.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanBreakCase {
    NoLockedPlan,
    WrongCategory,
    UnknownTool,
    SelfEdit,
    InvalidTarget,
    WorkChanged,
    ReviewSetupChanged,
    ReviewRequired,
}

/// Root-update refusal vocabulary, distinct from component failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanUpdateCase {
    MalformedPlan,
    StalePlan,
    NonSte100,
    InvalidCategory,
    InvalidGroup,
    ClosureChanged,
    WrongSeed,
    PlanExists,
    CounterExhausted,
    TemplateRejected,
}

/// The review outcome reasons, without prescribing a review script.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewFailure {
    MismatchedWork,
    MismatchedSetup,
    AssessmentFailed,
    Refused,
    Errored,
    Killed,
}

/// An exact plan-scoped act, never an executable permission predicate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum PlanAct {
    Statement,
    ExternalTool { name: String },
    InwardRecall { verb: String },
}

/// Operator categories require an explicit review election.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Category {
    pub id: String,
    pub verbs: Vec<String>,
    pub acts: Vec<PlanAct>,
    pub review_required: bool,
}

/// Operator group membership before stable steps are allocated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroupDraft {
    pub id: String,
    pub step_indices: Vec<DecimalU64>,
}

/// Resolved logical group membership and its optional execution span.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanGroup {
    pub id: String,
    pub step_ids: Vec<StepId>,
    #[serde(deserialize_with = "super::required_option")]
    pub span: Option<SpanRef>,
}

/// An operator step proposal, with null identity for a new step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StepDraft {
    #[serde(deserialize_with = "super::required_option")]
    pub id: Option<StepId>,
    pub instruction: String,
}

/// The complete replacement body, also carried by answer_break.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanReplacement {
    #[serde(deserialize_with = "super::required_option")]
    pub task: Option<String>,
    pub steps: Vec<StepDraft>,
    pub categories: Vec<Category>,
    pub groups: Vec<GroupDraft>,
    pub review: ReviewSetupRef,
    pub preserved_closures: Vec<StepId>,
}

/// The four root-authorized plan operations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum PlanOperation {
    Create {
        task: String,
        steps: Vec<StepDraft>,
        categories: Vec<Category>,
        groups: Vec<GroupDraft>,
        review: ReviewSetupRef,
    },
    Authorize {
        task: String,
        categories: Vec<Category>,
        groups: Vec<GroupDraft>,
        review: ReviewSetupRef,
    },
    Replace(PlanReplacement),
    AnswerBreak {
        replacement: PlanReplacement,
    },
}

/// The only admitted root plan document format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlanUpdateFormat {
    #[serde(rename = "state-plan-update/1")]
    V1,
}

/// Exact root plan document. The original file bytes remain the digest input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanUpdateDocument {
    pub format: PlanUpdateFormat,
    #[serde(deserialize_with = "super::required_option")]
    pub expected: Option<PlanRef>,
    pub operation: PlanOperation,
}

/// Copied plan-input metadata names a Text block and carries no duplicate text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanInputStep {
    #[serde(deserialize_with = "super::required_option")]
    pub id: Option<StepId>,
    pub block: DecimalU64,
}

/// Replacement context with source indices in place of strings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanInputReplacement {
    #[serde(deserialize_with = "super::required_option")]
    pub task: Option<DecimalU64>,
    pub steps: Vec<PlanInputStep>,
    pub categories: Vec<Category>,
    pub groups: Vec<GroupDraft>,
    pub review: ReviewSetupRef,
    pub preserved_closures: Vec<StepId>,
}

/// The source-index counterpart of PlanOperation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum PlanInputOperation {
    Create {
        task: DecimalU64,
        steps: Vec<PlanInputStep>,
        categories: Vec<Category>,
        groups: Vec<GroupDraft>,
        review: ReviewSetupRef,
    },
    Authorize {
        task: DecimalU64,
        categories: Vec<Category>,
        groups: Vec<GroupDraft>,
        review: ReviewSetupRef,
    },
    Replace(PlanInputReplacement),
    AnswerBreak {
        replacement: PlanInputReplacement,
    },
}

/// Operator provenance retained on block zero without instruction text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlanInputContext {
    pub document_sha256: Digest,
    pub cause: crate::Cause,
    pub expected: Option<PlanRef>,
    pub operation: PlanInputOperation,
}

impl<'de> Deserialize<'de> for PlanInputContext {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            document_sha256: Digest,
            cause: crate::Cause,
            #[serde(deserialize_with = "super::required_option")]
            expected: Option<PlanRef>,
            operation: PlanInputOperation,
        }
        let fields: Fields = super::codec::deserialize_object(deserializer)?;
        Ok(Self {
            document_sha256: fields.document_sha256,
            cause: fields.cause,
            expected: fields.expected,
            operation: fields.operation,
        })
    }
}

/// The held provenance of a lock, not a model-supplied authority grant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum PlanLockSource {
    Operator {
        input: SourceBlockId,
    },
    Model {
        authorization: SourceBlockId,
        candidate: Vec<StatementRef>,
    },
}

/// The final step instruction reference in plan order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanStepInit {
    pub step_id: StepId,
    pub instruction: StatementRef,
}

/// Atomic plan mutations licensed only beside their filed source context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum PlanEffect {
    Authorize {
        plan: PlanRef,
        source: SourceBlockId,
        task: TaskScopeRef,
        review: ReviewSetupRef,
    },
    Lock {
        plan: PlanRef,
        source: PlanLockSource,
        task: TaskScopeRef,
        steps: Vec<PlanStepInit>,
        review: ReviewSetupRef,
    },
    Replace {
        expected: PlanRef,
        next: PlanRef,
        source: SourceBlockId,
        task: TaskScopeRef,
        steps: Vec<PlanStepInit>,
        review: ReviewSetupRef,
    },
    Activate {
        expected: PlanRef,
        step_id: StepId,
        step_span: SpanRef,
        #[serde(deserialize_with = "super::required_option")]
        group_span: Option<SpanRef>,
        successor: StepQueryRef,
    },
    Close {
        expected: PlanRef,
        step_id: StepId,
        #[serde(deserialize_with = "super::required_option")]
        review_call: Option<CallId>,
        work: WorkRef,
        outcome_span: SpanRef,
    },
    Break {
        expected: PlanRef,
        #[serde(deserialize_with = "super::required_option")]
        step_id: Option<StepId>,
        case: PlanBreakCase,
        #[serde(deserialize_with = "super::required_option")]
        proposed_source: Option<SourceBlockId>,
    },
}

/// Complete retained step state and historical closure evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanStep {
    pub step_id: StepId,
    pub ordinal: DecimalU64,
    pub instruction: StatementRef,
    pub category: String,
    #[serde(deserialize_with = "super::required_option")]
    pub group: Option<String>,
    pub status: StepStatus,
    #[serde(deserialize_with = "super::required_option")]
    pub span: Option<SpanRef>,
    #[serde(deserialize_with = "super::required_option")]
    pub outcome_span: Option<SpanRef>,
    #[serde(deserialize_with = "super::required_option")]
    pub review_call: Option<CallId>,
    #[serde(deserialize_with = "super::required_option")]
    pub work: Option<WorkRef>,
    #[serde(deserialize_with = "super::required_option")]
    pub closure_source: Option<SourceBlockId>,
}

/// A held break disposition and its source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanBreakRecord {
    pub case: PlanBreakCase,
    #[serde(deserialize_with = "super::required_option")]
    pub step_id: Option<StepId>,
    #[serde(deserialize_with = "super::required_option")]
    pub proposed_source: Option<SourceBlockId>,
    pub source: SourceBlockId,
}

/// One complete committed revision, independent of argument placement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlanSnapshot {
    pub plan: PlanRef,
    pub task: TaskScopeRef,
    pub status: PlanStatus,
    pub review: ReviewSetupRef,
    pub operator_source: SourceBlockId,
    pub lock_source: Option<SourceBlockId>,
    pub document_sha256: Digest,
    pub last_source: SourcePosition,
    pub categories: Vec<Category>,
    pub group_drafts: Vec<GroupDraft>,
    pub groups: Vec<PlanGroup>,
    pub steps: Vec<PlanStep>,
    pub broken: Option<PlanBreakRecord>,
}

impl<'de> Deserialize<'de> for PlanSnapshot {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            plan: PlanRef,
            task: TaskScopeRef,
            status: PlanStatus,
            review: ReviewSetupRef,
            operator_source: SourceBlockId,
            #[serde(deserialize_with = "super::required_option")]
            lock_source: Option<SourceBlockId>,
            document_sha256: Digest,
            last_source: SourcePosition,
            categories: Vec<Category>,
            group_drafts: Vec<GroupDraft>,
            groups: Vec<PlanGroup>,
            steps: Vec<PlanStep>,
            #[serde(deserialize_with = "super::required_option")]
            broken: Option<PlanBreakRecord>,
        }
        let fields: Fields = super::codec::deserialize_object(deserializer)?;
        Ok(Self {
            plan: fields.plan,
            task: fields.task,
            status: fields.status,
            review: fields.review,
            operator_source: fields.operator_source,
            lock_source: fields.lock_source,
            document_sha256: fields.document_sha256,
            last_source: fields.last_source,
            categories: fields.categories,
            group_drafts: fields.group_drafts,
            groups: fields.groups,
            steps: fields.steps,
            broken: fields.broken,
        })
    }
}

/// One fixed plan and content frontier for forward-flag counts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlanFlagSnapshot {
    pub plan: PlanRef,
    pub after: SourceBlockId,
    pub through: SourcePosition,
    pub total_flags: DecimalU64,
    pub matched_flags: DecimalU64,
    pub unmatched_flags: DecimalU64,
}

impl<'de> Deserialize<'de> for PlanFlagSnapshot {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            plan: PlanRef,
            after: SourceBlockId,
            through: SourcePosition,
            total_flags: DecimalU64,
            matched_flags: DecimalU64,
            unmatched_flags: DecimalU64,
        }
        let fields: Fields = super::codec::deserialize_object(deserializer)?;
        Ok(Self {
            plan: fields.plan,
            after: fields.after,
            through: fields.through,
            total_flags: fields.total_flags,
            matched_flags: fields.matched_flags,
            unmatched_flags: fields.unmatched_flags,
        })
    }
}

/// Committed plan account carried by lifecycle observation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlanSummary {
    pub plan: PlanRef,
    pub status: PlanStatus,
    pub document_sha256: Digest,
    pub last_source: SourcePosition,
}

impl<'de> Deserialize<'de> for PlanSummary {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            plan: PlanRef,
            status: PlanStatus,
            document_sha256: Digest,
            last_source: SourcePosition,
        }
        let fields: Fields = super::codec::deserialize_object(deserializer)?;
        Ok(Self {
            plan: fields.plan,
            status: fields.status,
            document_sha256: fields.document_sha256,
            last_source: fields.last_source,
        })
    }
}

/// An ordinary plan operation refusal, never a component halt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanOperationRefused {
    pub case: PlanBreakCase,
    pub detail: String,
}

/// A root plan-update refusal and its rejected source indices.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanUpdateRefused {
    pub case: PlanUpdateCase,
    pub detail: String,
    pub rejected_blocks: Vec<DecimalU64>,
}

/// Trusted caller assessment, distinct from persisted closure authority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ReviewAssessment {
    Passed {},
    Failed { case: ReviewFailure },
}

/// The step outcome record label.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ReviewOutcome {
    Closed {},
    Failed { case: ReviewFailure },
}

/// A correct negative assessment consumes no closure evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewRejected {
    pub case: ReviewFailure,
}

/// Trace metadata about a plan break, not another state mutation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanBreakReport {
    #[serde(deserialize_with = "super::required_option")]
    pub plan: Option<PlanRef>,
    #[serde(deserialize_with = "super::required_option")]
    pub step_id: Option<StepId>,
    #[serde(deserialize_with = "super::required_option")]
    pub proposed_source: Option<SourceBlockId>,
    pub case: PlanBreakCase,
}

/// The component that reported a mandatory fit refusal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanFitStage {
    MandatoryProbe,
    PlaceBackstop,
}

/// The two mandatory-only fit refusal causes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanFitCode {
    TailOverCapacity,
    GenerationRoom,
}

/// A measured count or explicit query unavailability, never invented zero.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum PlanFlagObservation {
    Measured { snapshot: PlanFlagSnapshot },
    Unavailable { code: ReadRefusalCode },
}

/// The three explicit plan measurements.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum PlanMeasureEvent {
    Flags {
        after: SourceBlockId,
        request_id: DecimalU64,
        observation: PlanFlagObservation,
    },
    NextReady {
        closed: SourceBlockId,
        previous_step: StepId,
        step_id: StepId,
        assembly_id: DecimalU64,
        began_ns: DecimalU64,
        ended_ns: DecimalU64,
        closure_to_next_ns: DecimalU64,
    },
    FitRefused {
        step_id: StepId,
        assembly_id: DecimalU64,
        stage: PlanFitStage,
        code: PlanFitCode,
        mandatory_fit_refusals: DecimalU64,
    },
}

/// Versioned plan measurements, excluded from content admission.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanMeasurement {
    pub instrument_version: String,
    pub plan: PlanRef,
    pub event: PlanMeasureEvent,
}

impl PlanFlagSnapshot {
    /// Overflow or a changed count partition cannot masquerade as a measured result.
    pub fn counts_agree(&self) -> bool {
        self.matched_flags.0.checked_add(self.unmatched_flags.0) == Some(self.total_flags.0)
    }
}

impl PlanUpdateDocument {
    /// Context-free source-shape checks, without reserving IDs or granting authority.
    pub fn validate_shape(&self) -> Result<(), PlanUpdateCase> {
        let malformed = PlanUpdateCase::MalformedPlan;
        match &self.operation {
            PlanOperation::Create { task, steps, .. } => {
                if self.expected.is_some()
                    || task.is_empty()
                    || steps.is_empty()
                    || steps
                        .iter()
                        .any(|step| step.id.is_some() || step.instruction.is_empty())
                {
                    return Err(malformed);
                }
            }
            PlanOperation::Authorize { task, .. } => {
                if self.expected.is_some() || task.is_empty() {
                    return Err(malformed);
                }
            }
            PlanOperation::Replace(replacement) | PlanOperation::AnswerBreak { replacement } => {
                if self.expected.is_none()
                    || replacement.task.as_ref().is_some_and(String::is_empty)
                    || replacement.steps.is_empty()
                    || replacement
                        .steps
                        .iter()
                        .any(|step| step.instruction.is_empty())
                {
                    return Err(malformed);
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot_json() -> serde_json::Value {
        serde_json::json!({
            "plan":{"plan_id":{"seed_id":"seed","local_id":"1"},"revision":"1"},
            "task":{
                "task":{"seed_id":"seed","local_id":"1","revision":"1"},
                "prompt":{"seed_id":"seed","local_id":"2","revision":"1"}
            },
            "status":"authorized",
            "review":{"setup_id":"review","sha256":"0".repeat(64)},
            "operator_source":{"run":"original","sequence":"1","block":"0"},
            "lock_source":null,
            "document_sha256":"1".repeat(64),
            "last_source":{
                "block_id":{"run":"original","sequence":"2","block":"0"},
                "turn":null
            },
            "categories":[],"group_drafts":[],"groups":[],"steps":[],"broken":null
        })
    }

    fn query_json(snapshot: serde_json::Value, current: bool) -> serde_json::Value {
        if current {
            serde_json::json!({"plan_current":{
                "current":snapshot,"next_plan_id":"2","next_step_id":"1"
            }})
        } else {
            serde_json::json!({"plan":{"snapshot":snapshot}})
        }
    }

    #[test]
    fn plan_snapshot_carrier_is_required_for_both_selectors_and_preserves_nullable_turn() {
        for current in [false, true] {
            for turn in [serde_json::Value::Null, serde_json::json!("original-turn")] {
                let mut snapshot = snapshot_json();
                snapshot["last_source"]["turn"] = turn;
                let wire = query_json(snapshot, current);
                let result: super::super::QueryResult =
                    super::super::decode_strict(&wire.to_string()).unwrap();
                assert_eq!(serde_json::to_value(&result).unwrap(), wire);
            }
            for defect in 0..14 {
                let mut snapshot = snapshot_json();
                match defect {
                    0 => {
                        snapshot.as_object_mut().unwrap().remove("document_sha256");
                    }
                    1 => snapshot["document_sha256"] = serde_json::Value::Null,
                    2 => snapshot["document_sha256"] = serde_json::json!("A".repeat(64)),
                    3 => snapshot["document_sha256"] = serde_json::json!("1".repeat(63)),
                    4 => {
                        snapshot.as_object_mut().unwrap().remove("last_source");
                    }
                    5 => snapshot["last_source"] = snapshot["last_source"]["block_id"].clone(),
                    6 => {
                        snapshot["last_source"]
                            .as_object_mut()
                            .unwrap()
                            .remove("turn");
                    }
                    7 => snapshot["last_source"]["legacy_turn"] = serde_json::Value::Null,
                    8 => {
                        snapshot["last_source"] = serde_json::json!([
                            snapshot["last_source"]["block_id"],
                            snapshot["last_source"]["turn"]
                        ])
                    }
                    9 => {
                        snapshot["last_source"]["block_id"] =
                            serde_json::json!(["original", "2", "0"])
                    }
                    10 => {
                        snapshot = serde_json::Value::Array(
                            [
                                "plan",
                                "task",
                                "status",
                                "review",
                                "operator_source",
                                "lock_source",
                                "document_sha256",
                                "last_source",
                                "categories",
                                "group_drafts",
                                "groups",
                                "steps",
                                "broken",
                            ]
                            .map(|name| snapshot[name].clone())
                            .into(),
                        )
                    }
                    11 => snapshot["document_sha256"] = serde_json::json!(["1".repeat(64)]),
                    12 => {
                        snapshot["last_source"]
                            .as_object_mut()
                            .unwrap()
                            .remove("block_id");
                    }
                    13 => snapshot["last_source"]["turn"] = serde_json::json!(3),
                    _ => unreachable!(),
                }
                assert!(serde_json::from_value::<PlanSnapshot>(snapshot.clone()).is_err());
                assert!(
                    super::super::decode_strict::<super::super::QueryResult>(
                        &query_json(snapshot, current).to_string()
                    )
                    .is_err(),
                    "current={current}, defect={defect}"
                );
            }
            let wire = query_json(snapshot_json(), current).to_string();
            for duplicate in [
                wire.replace("\"turn\":null", "\"turn\":null,\"turn\":\"other\""),
                wire.replace(
                    "\"document_sha256\":",
                    "\"document_sha256\":null,\"document_sha256\":",
                ),
            ] {
                assert!(
                    super::super::decode_strict::<super::super::QueryResult>(&duplicate).is_err()
                );
            }
        }
    }

    fn assert_closed_object<T>(object: serde_json::Value, fields: &[&str])
    where
        T: serde::de::DeserializeOwned + Serialize,
    {
        let raw = object.to_string();
        let value: T = serde_json::from_str(&raw).unwrap();
        assert_eq!(serde_json::to_value(value).unwrap(), object);
        let value: T = super::super::decode_strict(&raw).unwrap();
        assert_eq!(serde_json::to_value(value).unwrap(), object);
        let mut decoder = serde_json::Deserializer::from_str(&raw);
        let value = T::deserialize(&mut decoder).unwrap();
        assert_eq!(serde_json::to_value(value).unwrap(), object);

        let mut invalid = vec![
            serde_json::Value::Array(fields.iter().map(|key| object[*key].clone()).collect())
                .to_string(),
        ];
        let mut unknown = object.clone();
        unknown["unknown"] = serde_json::Value::Null;
        invalid.push(unknown.to_string());
        for key in fields {
            let mut missing = object.clone();
            missing.as_object_mut().unwrap().remove(*key);
            invalid.push(missing.to_string());
            let encoded_key = serde_json::to_string(key).unwrap();
            invalid.push(format!("{{{encoded_key}:{},{}", object[*key], &raw[1..]));
        }
        for raw in invalid {
            assert!(serde_json::from_str::<T>(&raw).is_err(), "direct: {raw}");
            assert!(
                super::super::decode_strict::<T>(&raw).is_err(),
                "strict: {raw}"
            );
            let mut decoder = serde_json::Deserializer::from_str(&raw);
            assert!(T::deserialize(&mut decoder).is_err(), "visitor: {raw}");
        }
    }

    #[test]
    fn plan_input_context_requires_a_closed_named_object_at_every_decoder() {
        let object = serde_json::json!({
            "document_sha256":"1".repeat(64),
            "cause":{"uid":0},
            "expected":null,
            "operation":{"authorize":{
                "task":"0","categories":[],"groups":[],
                "review":{"setup_id":"review","sha256":"0".repeat(64)}
            }}
        });
        assert_closed_object::<PlanInputContext>(
            object,
            &["document_sha256", "cause", "expected", "operation"],
        );
    }

    #[test]
    fn plan_summary_requires_a_closed_named_object_at_every_decoder() {
        for turn in [serde_json::Value::Null, serde_json::json!("original-turn")] {
            let snapshot = snapshot_json();
            let mut object = serde_json::json!({
                "plan":snapshot["plan"],"status":snapshot["status"],
                "document_sha256":snapshot["document_sha256"],
                "last_source":snapshot["last_source"]
            });
            object["last_source"]["turn"] = turn;
            assert_closed_object::<PlanSummary>(
                object,
                &["plan", "status", "document_sha256", "last_source"],
            );
        }
    }

    #[test]
    fn plan_flag_snapshot_requires_a_closed_named_object_at_every_decoder() {
        for turn in [serde_json::Value::Null, serde_json::json!("original-turn")] {
            let snapshot = snapshot_json();
            let mut object = serde_json::json!({
                "plan":snapshot["plan"],"after":snapshot["operator_source"],
                "through":snapshot["last_source"],
                "total_flags":"3","matched_flags":"2","unmatched_flags":"1"
            });
            object["through"]["turn"] = turn;
            let fields = [
                "plan",
                "after",
                "through",
                "total_flags",
                "matched_flags",
                "unmatched_flags",
            ];
            assert_closed_object::<PlanFlagSnapshot>(object.clone(), &fields);
            let wire = serde_json::json!({"plan_flags":{"snapshot":object}});
            assert!(
                super::super::decode_strict::<super::super::QueryResult>(&wire.to_string()).is_ok()
            );
            let array = serde_json::Value::Array(fields.map(|name| object[name].clone()).into());
            let wire = serde_json::json!({"plan_flags":{"snapshot":array}});
            assert!(
                super::super::decode_strict::<super::super::QueryResult>(&wire.to_string())
                    .is_err()
            );
        }
    }

    #[test]
    fn categories_require_explicit_review_and_do_not_accept_predicates() {
        let valid =
            r#"{"id":"read","verbs":["read"],"acts":["statement"],"review_required":false}"#;
        assert!(serde_json::from_str::<Category>(valid).is_ok());
        assert!(
            serde_json::from_str::<Category>(&valid.replace(",\"review_required\":false", ""))
                .is_err()
        );
        assert!(
            serde_json::from_str::<PlanAct>(r#"{"external_tool":{"name":"sh","prefix":"safe"}}"#)
                .is_err()
        );
    }

    #[test]
    fn plan_input_context_cannot_smuggle_unclassified_instruction_text() {
        let source = r#"{"create":{"task":"0","steps":[{"id":null,"block":"1"}],"categories":[],"groups":[],"review":{"setup_id":"review","sha256":"0000000000000000000000000000000000000000000000000000000000000000"}}}"#;
        assert!(serde_json::from_str::<PlanInputOperation>(source).is_ok());
        assert!(
            serde_json::from_str::<PlanInputOperation>(&source.replace(
                "\"block\":\"1\"",
                "\"block\":\"1\",\"instruction\":\"Do extra work.\""
            ))
            .is_err()
        );
        assert!(
            serde_json::from_str::<PlanInputOperation>(
                &source.replace("\"task\":\"0\"", "\"task\":\"Do extra work.\"")
            )
            .is_err()
        );
    }

    #[test]
    fn root_document_replacement_requires_expected_revision() {
        let json = r#"{"format":"state-plan-update/1","expected":null,"operation":{"replace":{"task":null,"steps":[{"id":null,"instruction":"Read the record."}],"categories":[],"groups":[],"review":{"setup_id":"review","sha256":"0000000000000000000000000000000000000000000000000000000000000000"},"preserved_closures":[]}}}"#;
        let document: PlanUpdateDocument = serde_json::from_str(json).unwrap();
        assert_eq!(
            document.validate_shape(),
            Err(PlanUpdateCase::MalformedPlan)
        );
        assert!(
            serde_json::from_str::<PlanUpdateDocument>(
                &json.replace("state-plan-update/1", "state-plan-update/2")
            )
            .is_err()
        );
        assert!(
            serde_json::from_str::<PlanUpdateDocument>(&json.replace("\"expected\":null,", ""))
                .is_err()
        );
    }

    #[test]
    fn plan_effect_requires_nullable_evidence_and_no_added_script() {
        let json = r#"{"break":{"expected":{"plan_id":{"seed_id":"s","local_id":"1"},"revision":"1"},"step_id":null,"case":"no_locked_plan","proposed_source":null}}"#;
        assert!(serde_json::from_str::<PlanEffect>(json).is_ok());
        assert!(
            serde_json::from_str::<PlanEffect>(&json.replace(",\"proposed_source\":null", ""))
                .is_err()
        );
        assert!(
            serde_json::from_str::<ReviewAssessment>(r#"{"passed":{"script":"run"}}"#).is_err()
        );
        assert!(serde_json::from_str::<ReviewOutcome>(r#"{"failed":{"case":"made_up"}}"#).is_err());
    }
}
