//! Canonical bound declarations, written only by admin.
//!
//! Dotted keys keep the parser's field order without TOML table-context changes.
//! Typed matches deliberately name every field, so a floor addition requires a
//! decision here instead of disappearing from the saved declaration.

use std::fmt;

use weaver_types::state_flow::{
    FlowBacklog, FlowConfig, MeasuredInterval, MeasuredWait, PartitionRule, PersistenceConfig,
    PlacementConfig, ReadLimits, RepairCap, SchemeDeclaration, StateManagementConfig,
    StateStartConfig, Ste100Instruction, ToolPolicies, ToolPolicy,
};
use weaver_types::{
    AccessRule, AgentConfig, BindingKind, ClassifyInstruction, DecoderInstruction, FieldElection,
    GateInstruction, Lifecycle, ModelBinding, ServingDeclaration, SpuInstruction, StateStore,
    StoreEngine, TraceSink,
};

#[derive(Debug)]
pub(crate) enum RenderError {
    Unsupported(&'static str),
    Encoding(serde_json::Error),
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported(field) => write!(f, "unrepresentable declaration field: {field}"),
            Self::Encoding(error) => write!(f, "declaration string encoding: {error}"),
        }
    }
}

impl std::error::Error for RenderError {}

/// The input is the floor's validated bound value, never a raw draft or a map.
pub(crate) fn render(declaration: &ServingDeclaration) -> Result<String, RenderError> {
    let mut writer = Writer(String::new());
    writer.agent(declaration.agent())?;
    writer.state_management(declaration.state_management())?;
    Ok(writer.0)
}

#[cfg(test)]
pub(crate) fn render_fixture(agent: &AgentConfig, state: &StateManagementConfig) -> String {
    let mut writer = Writer(String::new());
    writer.agent(agent).unwrap();
    writer.state_management(state).unwrap();
    writer.0
}

struct Writer(String);

impl Writer {
    fn line(&mut self, key: &str, value: &str) {
        self.0.push_str(key);
        self.0.push_str(" = ");
        self.0.push_str(value);
        self.0.push('\n');
    }

    fn string(&mut self, key: &str, value: &str) -> Result<(), RenderError> {
        self.line(key, &quote(value)?);
        Ok(())
    }

    fn boolean(&mut self, key: &str, value: bool) {
        self.line(key, if value { "true" } else { "false" });
    }

    fn integer(&mut self, key: &str, value: u64) -> Result<(), RenderError> {
        self.line(key, &native_integer(value)?);
        Ok(())
    }

    fn strings<'a>(
        &mut self,
        key: &str,
        values: impl IntoIterator<Item = &'a str>,
    ) -> Result<(), RenderError> {
        self.line(key, &string_array(values)?);
        Ok(())
    }

    fn integers(&mut self, key: &str, values: impl IntoIterator<Item = u32>) {
        self.line(
            key,
            &format!(
                "[{}]",
                values
                    .into_iter()
                    .map(|value| value.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        );
    }

    fn agent(&mut self, value: &AgentConfig) -> Result<(), RenderError> {
        let AgentConfig {
            session,
            spu_instruction,
            tool_set,
            binding_kind,
            gate_instruction,
            trace_sink,
            state_election,
            state_store,
            restore,
            lifecycle,
        } = value;
        if state_election.is_some() || restore.is_some() {
            return Err(RenderError::Unsupported("retired serving declaration"));
        }
        self.string("session", &session.0)?;
        self.spu(spu_instruction)?;
        self.strings("tool-set", tool_set.iter().map(|tool| tool.0.as_str()))?;
        if let Some(kind) = binding_kind {
            self.string(
                "binding-kind",
                match kind {
                    BindingKind::Serving => "serving",
                    BindingKind::Diagnostic => {
                        return Err(RenderError::Unsupported("binding-kind"));
                    }
                },
            )?;
        }
        if let Some(GateInstruction { access_rule }) = gate_instruction {
            let AccessRule {
                allowed_uids,
                allowed_gids,
                denied_uids,
            } = access_rule;
            self.integers(
                "gate-instruction.access-rule.allowed-uids",
                allowed_uids.iter().copied(),
            );
            self.integers(
                "gate-instruction.access-rule.allowed-gids",
                allowed_gids.iter().copied(),
            );
            self.integers(
                "gate-instruction.access-rule.denied-uids",
                denied_uids.iter().copied(),
            );
        }
        match trace_sink {
            TraceSink::File { path, create } | TraceSink::Pipe { path, create } => {
                self.string(
                    "trace-sink.kind",
                    if matches!(trace_sink, TraceSink::File { .. }) {
                        "file"
                    } else {
                        "pipe"
                    },
                )?;
                self.string(
                    "trace-sink.path",
                    path.to_str()
                        .ok_or(RenderError::Unsupported("trace-sink.path"))?,
                )?;
                self.boolean("trace-sink.create", *create);
            }
            TraceSink::Socket { path } => {
                self.string("trace-sink.kind", "socket")?;
                self.string(
                    "trace-sink.path",
                    path.to_str()
                        .ok_or(RenderError::Unsupported("trace-sink.path"))?,
                )?;
            }
        }
        if let Some(StateStore { engine }) = state_store {
            match engine {
                StoreEngine::Sqlite => self.string("state-store.engine", "sqlite")?,
                StoreEngine::None => {
                    return Err(RenderError::Unsupported("state-store.engine"));
                }
            }
        }
        if let Some(Lifecycle {
            drain_bound,
            wind_down_bound,
        }) = lifecycle
        {
            if drain_bound.is_none() && wind_down_bound.is_none() {
                self.line("lifecycle", "{}");
            }
            if let Some(value) = drain_bound {
                self.integer("lifecycle.drain-bound", *value)?;
            }
            if let Some(value) = wind_down_bound {
                self.integer("lifecycle.wind-down-bound", *value)?;
            }
        }
        Ok(())
    }

    fn spu(&mut self, value: &SpuInstruction) -> Result<(), RenderError> {
        let SpuInstruction { decoder, classify } = value;
        let DecoderInstruction {
            model_binding,
            residual_readout_election,
            field_election,
            surprisal_election,
            refeed_permission,
            column_permission,
            tunable_values,
        } = decoder;
        const PREFIX: &str = "spu-instruction.decoder";
        self.model(&format!("{PREFIX}.model-binding"), model_binding)?;
        self.boolean(
            &format!("{PREFIX}.residual-readout-election"),
            *residual_readout_election,
        );
        if let Some(FieldElection { depth }) = field_election {
            self.integer(&format!("{PREFIX}.field-election.depth"), u64::from(*depth))?;
        }
        self.boolean(&format!("{PREFIX}.surprisal-election"), *surprisal_election);
        self.boolean(&format!("{PREFIX}.refeed-permission"), *refeed_permission);
        self.boolean(&format!("{PREFIX}.column-permission"), *column_permission);
        if tunable_values.is_empty() {
            self.line(&format!("{PREFIX}.tunable-values"), "{}");
        } else {
            for (key, value) in tunable_values {
                // Quoted components retain dots, quotes and control characters as
                // part of the parameter name, not a nested table path.
                self.line(
                    &format!("{PREFIX}.tunable-values.{}", quote(key)?),
                    &finite_float(*value)?,
                );
            }
        }
        if let Some(ClassifyInstruction { model_binding }) = classify {
            self.model("spu-instruction.classify.model-binding", model_binding)?;
        }
        Ok(())
    }

    fn model(&mut self, prefix: &str, value: &ModelBinding) -> Result<(), RenderError> {
        let ModelBinding { artifact, devices } = value;
        self.string(&format!("{prefix}.artifact"), &artifact.0)?;
        self.integers(
            &format!("{prefix}.devices"),
            devices.iter().map(|value| value.0),
        );
        Ok(())
    }

    fn wait(&mut self, prefix: &str, value: &MeasuredWait) -> Result<(), RenderError> {
        let MeasuredWait {
            elapsed_ns,
            measurement_ref,
        } = value;
        self.string(&format!("{prefix}.elapsed-ns"), &elapsed_ns.0.to_string())?;
        self.string(&format!("{prefix}.measurement-ref"), measurement_ref)
    }

    fn interval(&mut self, prefix: &str, value: &MeasuredInterval) -> Result<(), RenderError> {
        let MeasuredInterval {
            elapsed_ns,
            measurement_ref,
        } = value;
        self.string(&format!("{prefix}.elapsed-ns"), &elapsed_ns.0.to_string())?;
        self.string(&format!("{prefix}.measurement-ref"), measurement_ref)
    }

    fn state_management(&mut self, value: &StateManagementConfig) -> Result<(), RenderError> {
        let StateManagementConfig {
            flow,
            start,
            persistence,
            reads,
            ste100,
            tools,
            placement,
        } = value;
        self.flow(flow)?;
        let StateStartConfig { dump_sha256 } = start;
        self.string("state-management.start.dump-sha256", dump_sha256.as_str())?;
        let PersistenceConfig {
            save_wait,
            snapshot_wait,
        } = persistence;
        self.wait("state-management.persistence.save-wait", save_wait)?;
        self.wait("state-management.persistence.snapshot-wait", snapshot_wait)?;
        let ReadLimits {
            max_result_bytes,
            measurement_ref,
        } = reads;
        self.string(
            "state-management.reads.max-result-bytes",
            &max_result_bytes.0.to_string(),
        )?;
        self.string("state-management.reads.measurement-ref", measurement_ref)?;
        self.ste100(ste100)?;
        let ToolPolicies { sh } = tools;
        if let Some(ToolPolicy {
            clock_ms,
            measurement_ref,
        }) = sh
        {
            self.integer("state-management.tools.sh.clock-ms", *clock_ms)?;
            self.string("state-management.tools.sh.measurement-ref", measurement_ref)?;
        } else {
            self.line("state-management.tools", "{}");
        }
        let PlacementConfig {
            c_head,
            c_tail,
            measurement_ref,
        } = placement;
        self.integer("state-management.placement.c-head", *c_head)?;
        self.integer("state-management.placement.c-tail", *c_tail)?;
        self.string(
            "state-management.placement.measurement-ref",
            measurement_ref,
        )
    }

    fn flow(&mut self, value: &FlowConfig) -> Result<(), RenderError> {
        let FlowConfig {
            version,
            partition,
            gate_wait,
            cut_wait,
            observation_interval,
            backlog,
            scheme,
        } = value;
        const PREFIX: &str = "state-management.flow";
        self.integer(&format!("{PREFIX}.version"), u64::from(*version))?;
        self.line(
            &format!("{PREFIX}.partition"),
            &format!(
                "[{}]",
                partition
                    .iter()
                    .map(partition_rule)
                    .collect::<Result<Vec<_>, _>>()?
                    .join(", ")
            ),
        );
        self.wait(&format!("{PREFIX}.gate-wait"), gate_wait)?;
        self.wait(&format!("{PREFIX}.cut-wait"), cut_wait)?;
        self.interval(
            &format!("{PREFIX}.observation-interval"),
            observation_interval,
        )?;
        let FlowBacklog {
            max_pending_blocks,
            max_pending_bytes,
            lag_alarm_turns,
            measurement_ref,
        } = backlog;
        self.string(
            &format!("{PREFIX}.backlog.max-pending-blocks"),
            &max_pending_blocks.0.to_string(),
        )?;
        self.string(
            &format!("{PREFIX}.backlog.max-pending-bytes"),
            &max_pending_bytes.0.to_string(),
        )?;
        self.string(
            &format!("{PREFIX}.backlog.lag-alarm-turns"),
            &lag_alarm_turns.0.to_string(),
        )?;
        self.string(
            &format!("{PREFIX}.backlog.measurement-ref"),
            measurement_ref,
        )?;
        let SchemeDeclaration {
            scheme_id,
            artifact_sha256,
            verbs,
        } = scheme;
        self.string(&format!("{PREFIX}.scheme.scheme-id"), scheme_id)?;
        self.string(
            &format!("{PREFIX}.scheme.artifact-sha256"),
            artifact_sha256.as_str(),
        )?;
        self.strings(
            &format!("{PREFIX}.scheme.verbs"),
            verbs.iter().map(String::as_str),
        )
    }

    fn ste100(&mut self, value: &Ste100Instruction) -> Result<(), RenderError> {
        let Ste100Instruction {
            model_binding,
            model_artifact_sha256,
            instances,
            max_wire_bytes,
            measurement_ref,
            repair_cap,
            decision_wait,
        } = value;
        const PREFIX: &str = "state-management.ste100";
        self.model(&format!("{PREFIX}.model-binding"), model_binding)?;
        self.string(
            &format!("{PREFIX}.model-artifact-sha256"),
            model_artifact_sha256.as_str(),
        )?;
        self.integer(&format!("{PREFIX}.instances"), u64::from(*instances))?;
        self.string(
            &format!("{PREFIX}.max-wire-bytes"),
            &max_wire_bytes.0.to_string(),
        )?;
        self.string(&format!("{PREFIX}.measurement-ref"), measurement_ref)?;
        let RepairCap {
            attempts,
            measurement_ref,
        } = repair_cap;
        self.integer(
            &format!("{PREFIX}.repair-cap.attempts"),
            u64::from(*attempts),
        )?;
        self.string(
            &format!("{PREFIX}.repair-cap.measurement-ref"),
            measurement_ref,
        )?;
        self.wait(&format!("{PREFIX}.decision-wait"), decision_wait)
    }
}

fn partition_rule(value: &PartitionRule) -> Result<String, RenderError> {
    let PartitionRule {
        id,
        kind,
        channels,
        origins,
        blocks,
        context,
    } = value;
    let mut fields = vec![
        format!("id = {}", quote(id)?),
        format!("kind = {}", quote(kind)?),
        format!(
            "channels = {}",
            string_array(channels.iter().map(String::as_str))?
        ),
        format!(
            "origins = {}",
            string_array(origins.iter().map(String::as_str))?
        ),
        format!(
            "blocks = {}",
            string_array(blocks.iter().map(String::as_str))?
        ),
    ];
    if let Some(context) = context {
        fields.push(format!("context = {}", quote(context)?));
    }
    Ok(format!("{{ {} }}", fields.join(", ")))
}

fn string_array<'a>(values: impl IntoIterator<Item = &'a str>) -> Result<String, RenderError> {
    Ok(format!(
        "[{}]",
        values
            .into_iter()
            .map(quote)
            .collect::<Result<Vec<_>, _>>()?
            .join(", ")
    ))
}

fn quote(value: &str) -> Result<String, RenderError> {
    // serde_json's string escapes are TOML basic-string escapes too. DEL is
    // allowed literally in JSON but not TOML, so it needs the additional escape.
    serde_json::to_string(value)
        .map(|value| value.replace('\u{7f}', "\\u007f"))
        .map_err(RenderError::Encoding)
}

fn native_integer(value: u64) -> Result<String, RenderError> {
    i64::try_from(value)
        .map(|value| value.to_string())
        .map_err(|_| RenderError::Unsupported("native TOML integer"))
}

fn finite_float(value: f64) -> Result<String, RenderError> {
    if !value.is_finite() {
        return Err(RenderError::Unsupported("tunable-values"));
    }
    serde_json::to_string(&value).map_err(RenderError::Encoding)
}

#[cfg(test)]
pub(super) mod tests;
