//! Exact conversion into serialize-only trace mirrors. Validation remains at
//! the owning seam; these conversions neither normalize nor repair a value.
//!
//! Exhaustive matches and struct patterns make additions to either side require
//! an explicit conversion change. No trace payload is deserialized from JSON.

use weaver_trace::serving::payloads as trace;
use weaver_types::state_flow as shared;

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
        position: position.as_ref().map(|value| trace::SourcePosition {
            block_id: source(&value.block_id),
            turn: value.turn.clone(),
        }),
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

fn source(value: &shared::SourceBlockId) -> trace::SourceBlockId {
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
