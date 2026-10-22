//! Serialize-only trace mirrors. The owning state-management Spec defines the wire.
use super::*;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CallId {
    pub run: String,
    pub sequence: DecimalU64,
    pub call: DecimalU64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PairLane {
    External,
    Recall,
    Review,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PairRequest {
    External {
        name: String,
        arguments: String,
    },
    Recall {
        subject: String,
    },
    Review {
        setup: ReviewSetupRef,
        work: Box<WorkRef>,
        command: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PairStarted {
    pub call_id: CallId,
    pub call_block: DecimalU64,
    pub statement: StatementRef,
    pub lane: PairLane,
    pub request: PairRequest,
    pub retry_of: Option<CallId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PairFate {
    Opened,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalPairFate {
    Completed,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NonDispatchCause {
    PassPurpose,
    RepairExhausted,
    PlanBreak,
    RoundLimit,
    Stop,
    Unload,
    NoGate,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PairFailure {
    Timeout {},
    Cancelled {},
    Unload {},
    FlowFailure {},
    GateFault {},
    NotGranted {},
    NotDispatched { cause: NonDispatchCause },
    AbandonedAtLoad {},
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PairEnded {
    pub call_id: CallId,
    pub execution: Option<ExchangeId>,
    pub fate: TerminalPairFate,
    pub reason: Option<PairFailure>,
    pub outcome: Option<ToolOutcome>,
    pub loaded_from: Option<StartIdentity>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ExchangeId {
    pub opener: Opener,
    pub ordinal: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Opener {
    Admin,
    Harness,
    Spu,
    Gate,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum ToolOutcome {
    /// The invocation ran and the tool answers in its own words. For the
    /// shell a nonzero exit is a result, accounted in content, not an
    /// error.
    Result { content: String },
    /// Nothing ran, and the gate speaks in its own voice: a name it does
    /// not hold - never a nearest match, the family registry's discipline
    /// one organ over - malformed arguments, or a clock beyond the
    /// declared maximum. No side effect exists.
    Refused { reason: String },
    /// The invocation machinery failed - the fork, a pipe, the supervisor -
    /// and the account's speaker is the infrastructure, never the tool.
    Errored { detail: String },
    /// The caller's clock expired or the caller cancelled, and the gate killed the invocation's
    /// whole process group. No account from the tool exists by
    /// construction - the absence of the tool's words is the fact - and
    /// output drained before the kill rides as an attachment, never a
    /// result.
    Killed {
        partial: Option<String>,
        by: KillCause,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KillCause {
    Clock,
    Cancel,
    /// The agent's unload interrupted the call (`ToolInterrupt`): never
    /// finished, and re-runnable after the reload.
    Unload,
    /// The gate contained a call after the state flow failed. Trace only.
    FlowFailure,
    /// The gate died with the call open and the harness closed it: its outcome
    /// indeterminate, never re-run automatically.
    Fault,
}
