//! Exact conversion into serialize-only trace mirrors. Validation remains at
//! the owning seam; these conversions neither normalize nor repair a value.
//!
//! Exhaustive matches and struct patterns make additions to either side require
//! an explicit conversion change. No trace payload is deserialized from JSON.

use weaver_trace::serving::payloads as trace;
use weaver_types::state_flow as shared;

#[path = "mirror_plan.rs"]
mod plan;
pub(super) use plan::{plan_flags, plan_input, plan_ref, read_refusal, root_plan_effect};

pub(super) fn ste100(value: &shared::Ste100Reply) -> trace::Ste100Reply {
    let shared::Ste100Reply {
        request_id,
        target,
        scheme_id,
        result,
    } = value;
    trace::Ste100Reply {
        request_id: number(*request_id),
        target: match target {
            shared::Ste100Target::Source(id) => trace::Ste100Target::Source(source(id)),
            shared::Ste100Target::Input { input_id, block } => trace::Ste100Target::Input {
                input_id: number(*input_id),
                block: number(*block),
            },
            shared::Ste100Target::Template { template_id } => trace::Ste100Target::Template {
                template_id: template_id.clone(),
            },
        },
        scheme_id: scheme_id.clone(),
        result: match result {
            shared::Ste100Result::Accepted { bindings } => trace::Ste100Result::Accepted {
                bindings: bindings.iter().map(binding).collect(),
            },
            shared::Ste100Result::NotSte100 {
                code,
                detail,
                spans,
            } => trace::Ste100Result::NotSte100 {
                code: match code {
                    shared::Ste100DefectCode::NonSte100 => trace::Ste100DefectCode::NonSte100,
                },
                detail: detail.clone(),
                spans: spans.iter().copied().map(span).collect(),
            },
            shared::Ste100Result::ComponentFailed { code, detail } => {
                trace::Ste100Result::ComponentFailed {
                    code: match code {
                        shared::Ste100ComponentCode::Crashed => trace::Ste100ComponentCode::Crashed,
                        shared::Ste100ComponentCode::NoResult => {
                            trace::Ste100ComponentCode::NoResult
                        }
                        shared::Ste100ComponentCode::InvalidResult => {
                            trace::Ste100ComponentCode::InvalidResult
                        }
                        shared::Ste100ComponentCode::TransportFailed => {
                            trace::Ste100ComponentCode::TransportFailed
                        }
                    },
                    detail: detail.clone(),
                }
            }
        },
    }
}

pub(super) fn measurement(value: &shared::StateStoreMeasurement) -> trace::StateStoreMeasurement {
    let shared::StateStoreMeasurement {
        observation: observed,
        request_source,
        harness_queued_ns,
        harness_sent_ns,
        harness_received_ns,
    } = value;
    trace::StateStoreMeasurement {
        observation: observation(observed),
        request_source: request_source.as_ref().map(source),
        harness_queued_ns: number(*harness_queued_ns),
        harness_sent_ns: number(*harness_sent_ns),
        harness_received_ns: number(*harness_received_ns),
    }
}

pub(super) fn member(
    value: &shared::MemberObservationIdentity,
) -> trace::MemberObservationIdentity {
    let shared::MemberObservationIdentity {
        instrument_version,
        pid,
        start_ticks,
        ticks_per_second,
        unavailable_reason: reason,
    } = value;
    trace::MemberObservationIdentity {
        instrument_version: instrument_version.clone(),
        pid: *pid,
        start_ticks: start_ticks.map(number),
        ticks_per_second: ticks_per_second.map(number),
        unavailable_reason: reason.map(unavailable_reason),
    }
}

pub(super) fn instruction(value: &shared::Ste100Instruction) -> trace::Ste100Instruction {
    let shared::Ste100Instruction {
        model_binding,
        model_artifact_sha256,
        instances,
        max_wire_bytes,
        measurement_ref,
        repair_cap,
        decision_wait,
    } = value;
    trace::Ste100Instruction {
        model_binding: trace::ModelBinding {
            artifact: model_binding.artifact.0.clone(),
            devices: model_binding
                .devices
                .iter()
                .map(|device| device.0)
                .collect(),
        },
        model_artifact_sha256: model_artifact_sha256.as_str().into(),
        instances: *instances,
        max_wire_bytes: number(*max_wire_bytes),
        measurement_ref: measurement_ref.clone(),
        repair_cap: trace::RepairCap {
            attempts: repair_cap.attempts,
            measurement_ref: repair_cap.measurement_ref.clone(),
        },
        decision_wait: trace::MeasuredWait {
            elapsed_ns: number(decision_wait.elapsed_ns),
            measurement_ref: decision_wait.measurement_ref.clone(),
        },
    }
}

pub(super) fn start(value: &shared::StartIdentity) -> trace::StartIdentity {
    let shared::StartIdentity {
        seed_id,
        artifact_id,
        dump_sha256,
        config_sha256,
        parent_artifact_id,
        position,
    } = value;
    trace::StartIdentity {
        seed_id: seed_id.clone(),
        artifact_id: artifact_id.clone(),
        dump_sha256: dump_sha256.as_str().into(),
        config_sha256: config_sha256.as_str().into(),
        parent_artifact_id: parent_artifact_id.clone(),
        position: position.as_ref().map(source_position),
    }
}

pub(super) fn pair_started(value: &shared::PairStarted) -> trace::PairStarted {
    let shared::PairStarted {
        call_id: id,
        call_block,
        statement: dependency,
        lane,
        request,
        retry_of,
    } = value;
    trace::PairStarted {
        call_id: call_id(id),
        call_block: number(*call_block),
        statement: statement(dependency),
        lane: match lane {
            shared::PairLane::External => trace::PairLane::External,
            shared::PairLane::Recall => trace::PairLane::Recall,
            shared::PairLane::Review => trace::PairLane::Review,
        },
        request: match request {
            shared::PairRequest::External { name, arguments } => trace::PairRequest::External {
                name: name.clone(),
                arguments: arguments.clone(),
            },
            shared::PairRequest::Recall { subject } => trace::PairRequest::Recall {
                subject: subject.clone(),
            },
            shared::PairRequest::Review {
                setup,
                work,
                command,
            } => trace::PairRequest::Review {
                setup: review_setup(setup),
                work: Box::new(work_ref(work)),
                command: command.clone(),
            },
        },
        retry_of: retry_of.as_ref().map(call_id),
    }
}

pub(super) fn pair_ended(value: &shared::PairEnded) -> trace::PairEnded {
    let shared::PairEnded {
        call_id: id,
        execution,
        fate,
        reason,
        outcome,
        loaded_from,
    } = value;
    trace::PairEnded {
        call_id: call_id(id),
        execution: execution.as_ref().map(exchange),
        fate: match fate {
            shared::TerminalPairFate::Completed => trace::TerminalPairFate::Completed,
            shared::TerminalPairFate::Failed => trace::TerminalPairFate::Failed,
        },
        reason: reason.as_ref().map(pair_failure),
        outcome: outcome.as_ref().map(tool_outcome),
        loaded_from: loaded_from.as_ref().map(start),
    }
}

fn call_id(value: &shared::CallId) -> trace::CallId {
    let shared::CallId {
        run,
        sequence,
        call,
    } = value;
    trace::CallId {
        run: run.clone(),
        sequence: number(*sequence),
        call: number(*call),
    }
}

fn statement(value: &shared::StatementRef) -> trace::StatementRef {
    let shared::StatementRef { block_id, binding } = value;
    trace::StatementRef {
        block_id: source(block_id),
        binding: number(*binding),
    }
}

pub(super) fn source_position(value: &shared::SourcePosition) -> trace::SourcePosition {
    let shared::SourcePosition { block_id, turn } = value;
    trace::SourcePosition {
        block_id: source(block_id),
        turn: turn.clone(),
    }
}

pub(super) fn outbound(
    source: &shared::SourcePosition,
    acts: &[shared::SortAct],
) -> trace::SortDecision {
    trace::SortDecision::Outbound {
        source: source_position(source),
        criterion: trace::OutboundCriterion::Identity,
        acts: acts
            .iter()
            .map(|act| match act {
                shared::SortAct::Tool {
                    call_id: id,
                    statement: stmt,
                } => trace::SortAct::Tool {
                    call_id: call_id(id),
                    statement: statement(stmt),
                },
                shared::SortAct::Recall {
                    call_id: id,
                    statement: stmt,
                } => trace::SortAct::Recall {
                    call_id: call_id(id),
                    statement: statement(stmt),
                },
                shared::SortAct::Answer { source: id } => trace::SortAct::Answer {
                    source: self::source(id),
                },
            })
            .collect(),
    }
}

fn review_setup(value: &shared::ReviewSetupRef) -> trace::ReviewSetupRef {
    let shared::ReviewSetupRef { setup_id, sha256 } = value;
    trace::ReviewSetupRef {
        setup_id: setup_id.clone(),
        sha256: sha256.as_str().into(),
    }
}

fn work_ref(value: &shared::WorkRef) -> trace::WorkRef {
    let shared::WorkRef {
        plan,
        step_id,
        span,
        through,
        content_sha256,
    } = value;
    let shared::PlanRef { plan_id, revision } = plan;
    let shared::PlanId { seed_id, local_id } = plan_id;
    let plan = trace::PlanRef {
        plan_id: trace::PlanId {
            seed_id: seed_id.clone(),
            local_id: number(*local_id),
        },
        revision: number(*revision),
    };
    let shared::StepId { seed_id, local_id } = step_id;
    let step_id = trace::StepId {
        seed_id: seed_id.clone(),
        local_id: number(*local_id),
    };
    let shared::SpanRef {
        seed_id,
        local_id,
        revision,
    } = span;
    trace::WorkRef {
        plan,
        step_id,
        span: trace::SpanRef {
            seed_id: seed_id.clone(),
            local_id: number(*local_id),
            revision: number(*revision),
        },
        through: source_position(through),
        content_sha256: content_sha256.as_str().into(),
    }
}

fn exchange(value: &weaver_types::ExchangeId) -> trace::ExchangeId {
    let weaver_types::ExchangeId { opener, ordinal } = value;
    trace::ExchangeId {
        opener: match opener {
            weaver_types::Opener::Admin => trace::Opener::Admin,
            weaver_types::Opener::Harness => trace::Opener::Harness,
            weaver_types::Opener::Spu => trace::Opener::Spu,
            weaver_types::Opener::Gate => trace::Opener::Gate,
        },
        ordinal: *ordinal,
    }
}

fn pair_failure(value: &shared::PairFailure) -> trace::PairFailure {
    match value {
        shared::PairFailure::Timeout {} => trace::PairFailure::Timeout {},
        shared::PairFailure::Cancelled {} => trace::PairFailure::Cancelled {},
        shared::PairFailure::Unload {} => trace::PairFailure::Unload {},
        shared::PairFailure::FlowFailure {} => trace::PairFailure::FlowFailure {},
        shared::PairFailure::GateFault {} => trace::PairFailure::GateFault {},
        shared::PairFailure::NotGranted {} => trace::PairFailure::NotGranted {},
        shared::PairFailure::NotDispatched { cause } => trace::PairFailure::NotDispatched {
            cause: match cause {
                shared::NonDispatchCause::PassPurpose => trace::NonDispatchCause::PassPurpose,
                shared::NonDispatchCause::RepairExhausted => {
                    trace::NonDispatchCause::RepairExhausted
                }
                shared::NonDispatchCause::PlanBreak => trace::NonDispatchCause::PlanBreak,
                shared::NonDispatchCause::RoundLimit => trace::NonDispatchCause::RoundLimit,
                shared::NonDispatchCause::Stop => trace::NonDispatchCause::Stop,
                shared::NonDispatchCause::Unload => trace::NonDispatchCause::Unload,
                shared::NonDispatchCause::NoGate => trace::NonDispatchCause::NoGate,
            },
        },
        shared::PairFailure::AbandonedAtLoad {} => trace::PairFailure::AbandonedAtLoad {},
    }
}

fn tool_outcome(value: &weaver_types::ToolOutcome) -> trace::ToolOutcome {
    match value {
        weaver_types::ToolOutcome::Result { content } => trace::ToolOutcome::Result {
            content: content.clone(),
        },
        weaver_types::ToolOutcome::Refused { reason } => trace::ToolOutcome::Refused {
            reason: reason.clone(),
        },
        weaver_types::ToolOutcome::Errored { detail } => trace::ToolOutcome::Errored {
            detail: detail.clone(),
        },
        weaver_types::ToolOutcome::Killed { partial, by } => trace::ToolOutcome::Killed {
            partial: partial.clone(),
            by: match by {
                weaver_types::KillCause::Clock => trace::KillCause::Clock,
                weaver_types::KillCause::Cancel => trace::KillCause::Cancel,
                weaver_types::KillCause::Unload => trace::KillCause::Unload,
                weaver_types::KillCause::FlowFailure => trace::KillCause::FlowFailure,
                weaver_types::KillCause::Fault => trace::KillCause::Fault,
            },
        },
    }
}

/// Compare only the flow fields owned by this context. The loader owns the
/// remaining deployment facts. No trace value is parsed back into a contract.
pub(super) fn matches_flow(
    value: &shared::FlowConfig,
    record: &trace::FlowDeclarationRecord,
) -> bool {
    let shared::FlowConfig {
        version,
        partition,
        gate_wait,
        cut_wait,
        observation_interval,
        backlog,
        scheme,
    } = value;
    record.registry_id == weaver_trace::serving::REGISTRY_ID
        && record.version == *version
        && record.partition.len() == partition.len()
        && record.partition.iter().zip(partition).all(|(a, b)| {
            a.id == b.id
                && a.kind == b.kind
                && a.channels == b.channels
                && a.origins == b.origins
                && a.blocks == b.blocks
                && a.context == b.context
        })
        && record.gate_wait.elapsed_ns.0 == gate_wait.elapsed_ns.0
        && record.gate_wait.measurement_ref == gate_wait.measurement_ref
        && record.cut_wait.elapsed_ns.0 == cut_wait.elapsed_ns.0
        && record.cut_wait.measurement_ref == cut_wait.measurement_ref
        && record.observation_interval.elapsed_ns.0 == observation_interval.elapsed_ns.0
        && record.observation_interval.measurement_ref == observation_interval.measurement_ref
        && record.backlog.max_pending_blocks.0 == backlog.max_pending_blocks.0
        && record.backlog.max_pending_bytes.0 == backlog.max_pending_bytes.0
        && record.backlog.lag_alarm_turns.0 == backlog.lag_alarm_turns.0
        && record.backlog.measurement_ref == backlog.measurement_ref
        && record.scheme.scheme_id == scheme.scheme_id
        && record.scheme.artifact_sha256 == scheme.artifact_sha256.as_str()
        && record.scheme.verbs == scheme.verbs
}

fn number(value: shared::DecimalU64) -> trace::DecimalU64 {
    value.0.into()
}

pub(super) fn source(value: &shared::SourceBlockId) -> trace::SourceBlockId {
    let shared::SourceBlockId {
        run,
        sequence,
        block,
    } = value;
    trace::SourceBlockId {
        run: run.clone(),
        sequence: number(*sequence),
        block: number(*block),
    }
}

pub(super) fn span_ref(value: &shared::SpanRef) -> trace::SpanRef {
    trace::SpanRef {
        seed_id: value.seed_id.clone(),
        local_id: number(value.local_id),
        revision: number(value.revision),
    }
}

fn span(value: shared::Span) -> trace::Span {
    let shared::Span { start, end } = value;
    trace::Span {
        start: number(start),
        end: number(end),
    }
}

fn binding(value: &shared::Binding) -> trace::Binding {
    let shared::Binding {
        statement_span,
        verb,
        verb_span,
        condition,
        condition_span,
        nouns,
    } = value;
    trace::Binding {
        statement_span: span(*statement_span),
        verb: verb.clone(),
        verb_span: span(*verb_span),
        condition: condition.clone(),
        condition_span: condition_span.map(span),
        nouns: nouns
            .iter()
            .map(|value| {
                let shared::NounBinding { value, span: at } = value;
                trace::NounBinding {
                    value: value.clone(),
                    span: span(*at),
                }
            })
            .collect(),
    }
}

fn observation(value: &shared::StateOperationObservation) -> trace::StateOperationObservation {
    let shared::StateOperationObservation {
        operation_id,
        parent_id,
        request_id,
        operation,
        began_ns,
        ended_ns,
        phases,
        resource: measured_resource,
        disk: measured_disk,
    } = value;
    trace::StateOperationObservation {
        operation_id: number(*operation_id),
        parent_id: parent_id.map(number),
        request_id: number(*request_id),
        operation: match operation {
            shared::StateOperation::Open => trace::StateOperation::Open,
            shared::StateOperation::File => trace::StateOperation::File,
            shared::StateOperation::FilePair => trace::StateOperation::FilePair,
            shared::StateOperation::Read => trace::StateOperation::Read,
            shared::StateOperation::Next => trace::StateOperation::Next,
            shared::StateOperation::Release => trace::StateOperation::Release,
            shared::StateOperation::Shape => trace::StateOperation::Shape,
            shared::StateOperation::Held => trace::StateOperation::Held,
            shared::StateOperation::Position => trace::StateOperation::Position,
            shared::StateOperation::Grants => trace::StateOperation::Grants,
            shared::StateOperation::Snapshot => trace::StateOperation::Snapshot,
            shared::StateOperation::Acknowledge => trace::StateOperation::Acknowledge,
            shared::StateOperation::Abort => trace::StateOperation::Abort,
            shared::StateOperation::ReleaseCapture => trace::StateOperation::ReleaseCapture,
        },
        began_ns: number(*began_ns),
        ended_ns: number(*ended_ns),
        phases: phases.iter().map(phase).collect(),
        resource: resource(measured_resource),
        disk: disk(measured_disk),
    }
}

fn phase(value: &shared::OperationPhase) -> trace::OperationPhase {
    let shared::OperationPhase {
        name,
        began_ns,
        ended_ns,
    } = value;
    trace::OperationPhase {
        name: match name {
            shared::PhaseName::Validate => trace::PhaseName::Validate,
            shared::PhaseName::Query => trace::PhaseName::Query,
            shared::PhaseName::Transaction => trace::PhaseName::Transaction,
            shared::PhaseName::Capture => trace::PhaseName::Capture,
            shared::PhaseName::Filesystem => trace::PhaseName::Filesystem,
            shared::PhaseName::Encode => trace::PhaseName::Encode,
        },
        began_ns: number(*began_ns),
        ended_ns: number(*ended_ns),
    }
}

fn resource(value: &shared::OperationResource) -> trace::OperationResource {
    let shared::OperationResource {
        cpu_ns,
        copied_bytes,
        held_bytes,
        unavailable,
    } = value;
    trace::OperationResource {
        cpu_ns: cpu_ns.map(number),
        copied_bytes: copied_bytes.map(number),
        held_bytes: held_bytes.map(number),
        unavailable: unavailable
            .iter()
            .map(|value| {
                let shared::ResourceUnavailable { field, reason } = value;
                trace::ResourceUnavailable {
                    field: match field {
                        shared::ResourceField::CpuNs => trace::ResourceField::CpuNs,
                        shared::ResourceField::CopiedBytes => trace::ResourceField::CopiedBytes,
                        shared::ResourceField::HeldBytes => trace::ResourceField::HeldBytes,
                    },
                    reason: unavailable_reason(*reason),
                }
            })
            .collect(),
    }
}

fn unavailable_reason(value: shared::ObservationUnavailable) -> trace::ObservationUnavailable {
    match value {
        shared::ObservationUnavailable::Unsupported => trace::ObservationUnavailable::Unsupported,
        shared::ObservationUnavailable::Denied => trace::ObservationUnavailable::Denied,
        shared::ObservationUnavailable::Disabled => trace::ObservationUnavailable::Disabled,
        shared::ObservationUnavailable::ReadFailed => trace::ObservationUnavailable::ReadFailed,
        shared::ObservationUnavailable::CounterReset => trace::ObservationUnavailable::CounterReset,
        shared::ObservationUnavailable::Overflow => trace::ObservationUnavailable::Overflow,
        shared::ObservationUnavailable::Uninstrumented => {
            trace::ObservationUnavailable::Uninstrumented
        }
        shared::ObservationUnavailable::NotSampled => trace::ObservationUnavailable::NotSampled,
    }
}

fn disk(value: &shared::OperationDisk) -> trace::OperationDisk {
    let shared::OperationDisk {
        enabled,
        before,
        after,
        unavailable_reason: reason,
    } = value;
    trace::OperationDisk {
        enabled: *enabled,
        before: before.as_ref().map(disk_sample),
        after: after.as_ref().map(disk_sample),
        unavailable_reason: reason.map(unavailable_reason),
    }
}

fn disk_sample(value: &shared::DiskSample) -> trace::DiskSample {
    let shared::DiskSample {
        major_faults,
        read_bytes,
        write_bytes,
        blkio_delay_ticks,
        ticks_per_second,
    } = value;
    trace::DiskSample {
        major_faults: number(*major_faults),
        read_bytes: number(*read_bytes),
        write_bytes: number(*write_bytes),
        blkio_delay_ticks: number(*blkio_delay_ticks),
        ticks_per_second: number(*ticks_per_second),
    }
}

#[cfg(test)]
#[path = "mirror_tests.rs"]
mod tests;
