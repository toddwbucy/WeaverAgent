//! Test-only declaration bytes copied from the trace payload fixture.
//! These declarations and hashes make no production qualification claim.

use weaver_trace::serving::payloads::*;

pub(super) fn load_record() -> LoadRecord {
    let wait = MeasuredWait {
        elapsed_ns: 1_000_000.into(),
        measurement_ref: "fixture".into(),
    };
    LoadRecord {
        residual_readout: false,
        field: None,
        surprisal: false,
        state_member: true,
        state_store: weaver_trace::StoreIdentity {
            engine: "serving_sqlite".into(),
        },
        declaration: "declaration".into(),
        composer: weaver_trace::LoopIdentity::compiled("worker"),
        lineage: None,
        reset: None,
        stack: std::collections::BTreeMap::from([("worker".into(), "worker-hash".into())]),
        boundary: "boundary".into(),
        cause: weaver_trace::Cause { uid: 0 },
        library_path: None,
        state_flow: FlowDeclarationRecord {
            registry_id: "state-copy/1".into(),
            version: 1,
            partition: vec![],
            gate_wait: wait.clone(),
            cut_wait: wait.clone(),
            observation_interval: MeasuredInterval {
                elapsed_ns: 1000.into(),
                measurement_ref: "fixture".into(),
            },
            backlog: FlowBacklog {
                max_pending_blocks: 100.into(),
                max_pending_bytes: 10000.into(),
                lag_alarm_turns: 2.into(),
                measurement_ref: "fixture".into(),
            },
            scheme: SchemeDeclaration {
                scheme_id: "scheme".into(),
                artifact_sha256: "s".repeat(64),
                verbs: vec!["read".into()],
            },
            persistence: PersistenceConfig {
                save_wait: wait.clone(),
                snapshot_wait: wait.clone(),
            },
            reads: ReadLimits {
                max_result_bytes: 10000.into(),
                measurement_ref: "fixture".into(),
            },
            ste100: Ste100Instruction {
                model_binding: ModelBinding {
                    artifact: "checker".into(),
                    devices: vec![0],
                },
                model_artifact_sha256: "m".repeat(64),
                instances: 1,
                max_wire_bytes: 10000.into(),
                measurement_ref: "fixture".into(),
                repair_cap: RepairCap {
                    attempts: 3,
                    measurement_ref: "fixture".into(),
                },
                decision_wait: wait,
            },
            tools: ToolPolicies { sh: None },
            placement: PlacementConfig {
                c_head: 2048,
                c_tail: 6144,
                measurement_ref: "fixture".into(),
            },
        },
        state_start: StartIdentity {
            seed_id: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            artifact_id: "seed-artifact".into(),
            dump_sha256: "a".repeat(64),
            config_sha256: "b".repeat(64),
            parent_artifact_id: None,
            position: None,
        },
        clock_source: ClockSource::ClockMonotonic,
        origin_ns: 9_007_199_254_740_993.into(),
        state_member_observation: MemberObservationIdentity {
            instrument_version: "state-observation/2".into(),
            pid: 123,
            start_ticks: Some(100.into()),
            ticks_per_second: Some(100.into()),
            unavailable_reason: None,
        },
        template_inventory: TemplateInventory {
            scheme_id: "scheme".into(),
            inventory_sha256: "i".repeat(64),
            checker_artifact_sha256: "c".repeat(64),
            templates: vec![TemplateSpec {
                template_id: "wind_down".into(),
                kind: "loop.instruction".into(),
                parts: vec![TemplatePart::Literal("Stop work.".into())],
            }],
        },
        ste100_scheme: "{\n  \"edition\": \"Issue 9\"\n}\n".into(),
        ste100_binary_sha256: "e".repeat(64),
    }
}

use weaver_trace::serving::{Partition, PartitionRule};

pub(super) fn partition() -> Partition {
    let mut rules = Vec::new();
    for (id, kind, channels, origins, blocks, context) in [
        (
            "user",
            "message.user",
            "input",
            "operator external",
            "content.text[*]",
            false,
        ),
        (
            "assistant",
            "message.assistant",
            "output",
            "model",
            "content.text[*]",
            false,
        ),
        (
            "loop",
            "loop.instruction",
            "internal",
            "loop",
            "message.content.text[*]",
            false,
        ),
        (
            "pair-start",
            "tool.call.started",
            "tool",
            "model loop",
            "pair",
            false,
        ),
        (
            "pair-end",
            "tool.call.completed",
            "tool",
            "tool loop",
            "pair",
            false,
        ),
        (
            "mark",
            "state.mark",
            "internal",
            "loop",
            "message.content.text[0]",
            true,
        ),
        (
            "plan-input",
            "plan.event",
            "input",
            "operator",
            "message.content.text[*]",
            true,
        ),
        (
            "plan-effect",
            "plan.event",
            "internal",
            "loop",
            "message.content.text[0]",
            true,
        ),
        (
            "outcome",
            "step.outcome",
            "internal",
            "loop",
            "message.content.text[*]",
            true,
        ),
    ] {
        rules.push(PartitionRule {
            id: id.into(),
            kind: kind.into(),
            channels: channels.split(' ').map(str::to_owned).collect(),
            origins: origins.split(' ').map(str::to_owned).collect(),
            blocks: vec![blocks.into()],
            context: context.then(|| "context".into()),
        });
    }
    Partition { rules }
}
