use crate::flow::mirror::{source, span_ref};
use weaver_trace::serving::payloads as t;
use weaver_types::state_flow as s;
pub(super) fn content(value: &s::ContentRef) -> t::ContentRef {
    match value {
        s::ContentRef::Seed { seed_id, ordinal } => t::ContentRef::Seed {
            seed_id: seed_id.clone(),
            ordinal: ordinal.0.into(),
        },
        s::ContentRef::Event { block_id } => t::ContentRef::Event {
            block_id: source(block_id),
        },
    }
}
pub(super) fn call(value: &s::CallId) -> t::CallId {
    t::CallId {
        run: value.run.clone(),
        sequence: value.sequence.0.into(),
        call: value.call.0.into(),
    }
}
pub(super) fn statement(value: &s::StatementRef) -> t::StatementRef {
    t::StatementRef {
        block_id: source(&value.block_id),
        binding: value.binding.0.into(),
    }
}
pub(in crate::flow) fn material(value: &s::MaterialRef) -> t::MaterialRef {
    match value {
        s::MaterialRef::Text { source: src, span } => t::MaterialRef::Text {
            source: content(src),
            span: t::Span {
                start: span.start.0.into(),
                end: span.end.0.into(),
            },
        },
        s::MaterialRef::Request { call_id } => t::MaterialRef::Request {
            call_id: call(call_id),
        },
        s::MaterialRef::Returned { call_id } => t::MaterialRef::Returned {
            call_id: call(call_id),
        },
    }
}
pub(in crate::flow) fn unit(value: &s::UnitId) -> t::UnitId {
    match value {
        s::UnitId::Block { source } => t::UnitId::Block {
            source: content(source),
        },
        s::UnitId::Instruction { source } => t::UnitId::Instruction {
            source: content(source),
        },
        s::UnitId::Statement { statement: src } => t::UnitId::Statement {
            statement: statement(src),
        },
        s::UnitId::Pair { call_id } => t::UnitId::Pair {
            call_id: call(call_id),
        },
        s::UnitId::Result { call_id } => t::UnitId::Result {
            call_id: call(call_id),
        },
        s::UnitId::Span { span } => t::UnitId::Span {
            span: span_ref(span),
        },
    }
}
pub(in crate::flow) fn query(value: &s::StepQueryRef) -> t::StepQueryRef {
    t::StepQueryRef {
        plan: t::PlanRef {
            plan_id: t::PlanId {
                seed_id: value.plan.plan_id.seed_id.clone(),
                local_id: value.plan.plan_id.local_id.0.into(),
            },
            revision: value.plan.revision.0.into(),
        },
        previous_step: value.previous_step.as_ref().map(step),
        step_id: step(&value.step_id),
        instruction: statement(&value.instruction),
    }
}
fn step(value: &s::StepId) -> t::StepId {
    t::StepId {
        seed_id: value.seed_id.clone(),
        local_id: value.local_id.0.into(),
    }
}
pub(in crate::flow) fn cursor(value: &s::ArgumentCursor) -> t::ArgumentCursor {
    match value {
        s::ArgumentCursor::Head {} => t::ArgumentCursor::Head {},
        s::ArgumentCursor::Message {
            message_id,
            block_end,
            byte_end,
        } => t::ArgumentCursor::Message {
            message_id: message_id.0.into(),
            block_end: block_end.0.into(),
            byte_end: byte_end.map(|n| n.0.into()),
        },
    }
}
