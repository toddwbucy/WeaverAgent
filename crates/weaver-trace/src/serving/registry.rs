use serde::Serialize;

pub const REGISTRY_ID: &str = "state-copy/1";

macro_rules! vocabulary {
    ($name:ident { $($variant:ident => $wire:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
        pub enum $name {
            $(#[serde(rename = $wire)] $variant),+
        }

        impl $name {
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            pub fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $wire),+ }
            }

            pub fn from_name(name: &str) -> Option<Self> {
                match name { $($wire => Some(Self::$variant),)+ _ => None }
            }
        }
    };
}

vocabulary!(Channel {
    Input => "input", Output => "output", Internal => "internal",
    Tool => "tool", Measurement => "measurement",
});

vocabulary!(Origin {
    Operator => "operator", External => "external", Model => "model",
    Loop => "loop", Tool => "tool", Runtime => "runtime",
});

// Keep this serving vocabulary separate until cutover. Legacy and diagnostic
// authoring still use crate::Kind, unchanged.
vocabulary!(Kind {
    Load => "load", Unload => "unload", SessionClosed => "session.closed",
    TurnStarted => "turn.started", TurnClosed => "turn.closed",
    MessageSystem => "message.system", MessageUser => "message.user",
    MessageAssistant => "message.assistant", MessageToolResult => "message.tool_result",
    ToolCallStarted => "tool.call.started", ToolCallCompleted => "tool.call.completed",
    Fault => "fault", Elision => "elision", Refusal => "refusal", Flush => "flush",
    ModelRequest => "model.request", ModelOutput => "model.output",
    ModelMeasurement => "model.measurement", ModelField => "model.field",
    ClassifyRequest => "classify.request", ClassifyOutput => "classify.output",
    Recall => "recall", MessageRestored => "message.restored", Score => "score",
    SavePoint => "save_point", ModelUnrecoveredCall => "model.unrecovered_call",
    ToolReturnRejected => "tool.return_rejected", LoopInstruction => "loop.instruction",
    SortDecision => "sort.decision", StateSortMeasurement => "state.sort.measurement",
    StepOutcome => "step.outcome", PlanEvent => "plan.event", PlanBreak => "plan.break",
    StatePlanMeasurement => "state.plan.measurement", StateMark => "state.mark",
    StateMarkPosition => "state.mark.position", StatePlacement => "state.placement",
    InputOffered => "input.offered", StateFlow => "state.flow",
    StateStoreMeasurement => "state.store.measurement", Ste100Result => "ste100.result",
});

/// The finite selectable shape, not a decoder for event-specific metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentSchema {
    Head,
    None,
    Message,
    Pair,
    OneText { context: bool },
    Plan,
}

/// Total over the serving vocabulary. No fallback licenses a new kind.
pub fn content_schema(kind: Kind) -> ContentSchema {
    use ContentSchema as S;
    use Kind as K;
    match kind {
        K::Load => S::Head,
        K::MessageUser | K::MessageAssistant => S::Message,
        K::ToolCallStarted | K::ToolCallCompleted => S::Pair,
        K::LoopInstruction | K::SortDecision => S::OneText { context: false },
        K::StepOutcome | K::StateMark => S::OneText { context: true },
        K::PlanEvent => S::Plan,
        K::Unload
        | K::SessionClosed
        | K::TurnStarted
        | K::TurnClosed
        | K::MessageSystem
        | K::MessageToolResult
        | K::Fault
        | K::Elision
        | K::Refusal
        | K::Flush
        | K::ModelRequest
        | K::ModelOutput
        | K::ModelMeasurement
        | K::ModelField
        | K::ClassifyRequest
        | K::ClassifyOutput
        | K::Recall
        | K::MessageRestored
        | K::Score
        | K::SavePoint
        | K::ModelUnrecoveredCall
        | K::ToolReturnRejected
        | K::StateSortMeasurement
        | K::PlanBreak
        | K::StatePlanMeasurement
        | K::StateMarkPosition
        | K::StatePlacement
        | K::InputOffered
        | K::StateFlow
        | K::StateStoreMeasurement
        | K::Ste100Result => S::None,
    }
}

/// Conditional rules require the later typed authoring codec. The tap never
/// reads purpose, pair fate or a plan effect to decide admission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnRule {
    Required,
    Forbidden,
    Optional,
    PayloadDependent,
}

pub fn turn_rule(kind: Kind) -> TurnRule {
    use Kind as K;
    use TurnRule as T;
    match kind {
        K::TurnStarted
        | K::TurnClosed
        | K::MessageUser
        | K::MessageAssistant
        | K::MessageToolResult
        | K::ToolCallStarted
        | K::ModelRequest
        | K::ModelOutput
        | K::ModelMeasurement
        | K::ModelField
        | K::ModelUnrecoveredCall
        | K::StepOutcome
        | K::StateMark => T::Required,
        K::Load
        | K::Unload
        | K::SessionClosed
        | K::Elision
        | K::Flush
        | K::Recall
        | K::MessageRestored
        | K::Score
        | K::SavePoint
        | K::InputOffered => T::Forbidden,
        K::MessageSystem
        | K::Fault
        | K::Refusal
        | K::ClassifyRequest
        | K::ClassifyOutput
        | K::ToolReturnRejected
        | K::SortDecision
        | K::StateSortMeasurement
        | K::PlanBreak
        | K::StateMarkPosition
        | K::StatePlacement
        | K::StateFlow
        | K::StateStoreMeasurement
        | K::Ste100Result => T::Optional,
        K::ToolCallCompleted | K::LoopInstruction | K::PlanEvent | K::StatePlanMeasurement => {
            T::PayloadDependent
        }
    }
}
