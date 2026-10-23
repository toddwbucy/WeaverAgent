//! Build startup inputs only from an Enter adopted by this recording context.
//! The actual member identity and clock enter the load only after Open completes.
use super::*;
use crate::flow::{author::AuthoredSource, mirror};
use weaver_trace::serving::payloads as trace;
use weaver_types::state_flow as shared;
use weaver_types::state_flow::enter::{EnterBinding, EnterPayload};

pub(in crate::flow) struct EnterLoadFacts {
    composer: weaver_trace::LoopIdentity,
    checker_digest: shared::Digest,
}

impl<'a> StartupComposition<'a> {
    /// No caller-supplied load event, start, limits or classifier bootstrap can
    /// replace the adopted envelope's values. Rejection returns its whole input.
    #[allow(clippy::result_large_err)]
    pub(in crate::flow) fn from_enter(
        context: &'a mut RecordingContext,
        resources: &'a mut FailedEnterResources,
        inputs: EnterStartupInputs,
    ) -> Result<Self, (EnterStartupInputs, &'static str)> {
        let checked = (|| {
            if !inputs.original.adopted_by(context)
                || context.phase != RecordingPhase::Preflight
                || context.open_start.is_some()
                || !resources.matches(context)
                || resources.classifier_child().is_some()
                || resources.spu_child().is_some()
                || context.deadlines.get(PumpDeadline::Enter).is_none()
            {
                return Err("startup requires the original Enter's unused context and resources");
            }
            let EnterBinding::Serving {
                state_management,
                start,
                ste100_scheme,
                shell,
                ..
            } = &inputs.original.envelope().enter().binding
            else {
                return Err("startup requires Serving Enter");
            };
            let elected = if shell.is_some() {
                vec!["sh".to_owned()]
            } else {
                vec![]
            };
            state_management
                .validate(&elected)
                .map_err(|_| "invalid Enter state-management declaration")?;
            if !inputs.classifier_binary.is_absolute() {
                return Err("checker launch path must be absolute");
            }
            let name = inputs
                .classifier_binary
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or("checker launch path has no UTF-8 basename")?;
            let digest = inputs
                .original
                .envelope()
                .enter()
                .stack
                .get(name)
                .ok_or("checker launch has no root-judged stack digest")?;
            let digest = shared::Digest::new(digest.clone())
                .map_err(|_| "invalid checker executable digest")?;
            Ok((
                start.clone(),
                state_management.reads.clone(),
                Ste100Bootstrap {
                    protocol: shared::STE100_PROTOCOL.into(),
                    scheme_text: ste100_scheme.clone(),
                    instruction: state_management.ste100.clone(),
                },
                digest,
            ))
        })();
        let (start, limits, bootstrap, checker_digest) = match checked {
            Ok(values) => values,
            Err(reason) => return Err((inputs, reason)),
        };
        let enter_bound = context.deadlines.get(PumpDeadline::Enter);
        Ok(Self {
            context,
            resources,
            enter_bound,
            start,
            limits,
            load: None,
            original_enter: Some(inputs.original),
            enter_load: Some(EnterLoadFacts {
                composer: inputs.composer,
                checker_digest,
            }),
            bootstrap,
            binary: inputs.classifier_binary,
            inventory: inputs.inventory,
            stage: StartupStage::Open,
            started: false,
            load_sequence: None,
            opened: None,
            initial_grants: None,
            classifier: None,
            stop: None,
        })
    }
}

impl EnterLoadFacts {
    pub(super) fn event(
        &self,
        payload: &EnterPayload,
        inventory: &CompiledInventory,
        origin: u64,
        member: &shared::MemberObservationIdentity,
    ) -> AuthoredEvent {
        let EnterBinding::Serving {
            state_management,
            start,
            ste100_scheme,
            ..
        } = &payload.binding
        else {
            unreachable!("checked Serving Enter")
        };
        let load = trace::LoadRecord {
            residual_readout: payload.spu_instruction.decoder.residual_readout_election,
            field: payload
                .spu_instruction
                .decoder
                .field_election
                .as_ref()
                .map(|value| value.depth),
            surprisal: payload.spu_instruction.decoder.surprisal_election,
            state_member: true,
            state_store: weaver_trace::StoreIdentity {
                engine: "sqlite".into(),
            },
            declaration: payload.declaration.clone(),
            composer: self.composer.clone(),
            lineage: payload.restore.as_ref().map(|value| {
                Box::new(weaver_trace::Lineage {
                    save_point: value.save_point.clone(),
                    run: value.run.0.clone(),
                    sequence: value.sequence,
                    turn: value.turn,
                    named_at_restore: value.named_at_restore,
                    built_from: value
                        .built_from
                        .as_ref()
                        .map(|branch| weaver_trace::Branch {
                            parent: branch.parent.0.clone(),
                            run: branch.run.0.clone(),
                            through: branch.through,
                        }),
                })
            }),
            reset: payload.reset.as_ref().map(|reset| weaver_trace::Reset {
                prior_run: reset.prior_run.0.clone(),
                reason: match reset.reason {
                    weaver_types::ResetReason::NoCleanUnload => "no-clean-unload",
                    weaver_types::ResetReason::ForcedUnload => "forced-unload",
                }
                .into(),
            }),
            stack: payload.stack.clone(),
            boundary: payload.boundary.clone(),
            cause: weaver_trace::Cause {
                uid: payload.cause.uid,
            },
            library_path: payload.library_path.clone(),
            state_flow: declaration(state_management),
            state_start: mirror::start(start),
            clock_source: trace::ClockSource::ClockMonotonic,
            origin_ns: origin.into(),
            state_member_observation: mirror::member(member),
            template_inventory: inventory.record(
                &state_management.flow.scheme,
                &state_management.ste100.model_artifact_sha256,
            ),
            ste100_scheme: ste100_scheme.clone(),
            ste100_binary_sha256: self.checker_digest.to_string(),
        };
        AuthoredEvent {
            kind: Kind::Load,
            subsystem: weaver_trace::Subsystem::Harness,
            turn: None,
            source: AuthoredSource::Runtime,
            causal_parent: None,
            payload: Some(Payload::Elections(Box::new(load))),
        }
    }
}

fn wait(value: &shared::MeasuredWait) -> trace::MeasuredWait {
    trace::MeasuredWait {
        elapsed_ns: value.elapsed_ns.0.into(),
        measurement_ref: value.measurement_ref.clone(),
    }
}
fn declaration(value: &shared::StateManagementConfig) -> trace::FlowDeclarationRecord {
    let shared::StateManagementConfig {
        flow,
        start: _,
        persistence,
        reads,
        ste100,
        tools,
        placement,
    } = value;
    let shared::FlowConfig {
        version,
        partition,
        gate_wait,
        cut_wait,
        observation_interval,
        backlog,
        scheme,
    } = flow;
    trace::FlowDeclarationRecord {
        registry_id: weaver_trace::serving::REGISTRY_ID.into(),
        version: *version,
        partition: partition
            .iter()
            .map(
                |shared::PartitionRule {
                     id,
                     kind,
                     channels,
                     origins,
                     blocks,
                     context,
                 }| trace::PartitionRule {
                    id: id.clone(),
                    kind: kind.clone(),
                    channels: channels.clone(),
                    origins: origins.clone(),
                    blocks: blocks.clone(),
                    context: context.clone(),
                },
            )
            .collect(),
        gate_wait: wait(gate_wait),
        cut_wait: wait(cut_wait),
        observation_interval: trace::MeasuredInterval {
            elapsed_ns: observation_interval.elapsed_ns.0.into(),
            measurement_ref: observation_interval.measurement_ref.clone(),
        },
        backlog: trace::FlowBacklog {
            max_pending_blocks: backlog.max_pending_blocks.0.into(),
            max_pending_bytes: backlog.max_pending_bytes.0.into(),
            lag_alarm_turns: backlog.lag_alarm_turns.0.into(),
            measurement_ref: backlog.measurement_ref.clone(),
        },
        scheme: trace::SchemeDeclaration {
            scheme_id: scheme.scheme_id.clone(),
            artifact_sha256: scheme.artifact_sha256.to_string(),
            verbs: scheme.verbs.clone(),
        },
        persistence: trace::PersistenceConfig {
            save_wait: wait(&persistence.save_wait),
            snapshot_wait: wait(&persistence.snapshot_wait),
        },
        reads: trace::ReadLimits {
            max_result_bytes: reads.max_result_bytes.0.into(),
            measurement_ref: reads.measurement_ref.clone(),
        },
        ste100: mirror::instruction(ste100),
        tools: trace::ToolPolicies {
            sh: tools.sh.as_ref().map(
                |shared::ToolPolicy {
                     clock_ms,
                     measurement_ref,
                 }| trace::ToolPolicy {
                    clock_ms: *clock_ms,
                    measurement_ref: measurement_ref.clone(),
                },
            ),
        },
        placement: trace::PlacementConfig {
            c_head: placement.c_head,
            c_tail: placement.c_tail,
            measurement_ref: placement.measurement_ref.clone(),
        },
    }
}
