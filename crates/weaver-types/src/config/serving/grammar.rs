//! TOML-only spelling adapter. Shared wire objects retain their closed snake_case form.

use std::collections::BTreeSet;

use serde::de::DeserializeOwned;
use toml::{Table, Value};

use crate::state_flow::{
    DecimalU64, Digest, FlowBacklog, FlowConfig, MeasuredInterval, MeasuredWait, PartitionRule,
    PersistenceConfig, PlacementConfig, ReadLimits, RepairCap, SHELL_MAX_CLOCK_MS,
    SchemeDeclaration, StateStartConfig, Ste100Instruction, ToolPolicies, ToolPolicy,
    ValidationError,
};
use crate::{ArtifactRef, DeviceOrdinal, ModelBinding, ToolName};

use super::{ConfigError, ConfigErrorKind, DraftStateManagement, FieldName};

pub(super) fn error(path: &str, kind: ConfigErrorKind) -> ConfigError {
    ConfigError {
        field: Some(FieldName(path.into())),
        kind,
    }
}

fn bad(path: &str) -> ConfigError {
    error(path, ConfigErrorKind::BadValue)
}

fn checked(path: &str, result: Result<(), ValidationError>) -> Result<(), ConfigError> {
    result.map_err(|_| bad(path))
}

struct Object<'a> {
    table: &'a Table,
    path: String,
}

impl<'a> Object<'a> {
    fn new(value: &'a Value, path: String, allowed: &[&str]) -> Result<Self, ConfigError> {
        let table = value.as_table().ok_or_else(|| bad(&path))?;
        for key in table.keys() {
            if !allowed.contains(&key.as_str()) {
                return Err(error(
                    &format!("{path}.{key}"),
                    ConfigErrorKind::UnknownField,
                ));
            }
        }
        Ok(Self { table, path })
    }

    fn path(&self, key: &str) -> String {
        format!("{}.{}", self.path, key)
    }

    fn value(&self, key: &str) -> Result<&'a Value, ConfigError> {
        self.table
            .get(key)
            .ok_or_else(|| error(&self.path(key), ConfigErrorKind::MissingField))
    }

    fn get<T: DeserializeOwned>(&self, key: &str) -> Result<T, ConfigError> {
        self.value(key)?
            .clone()
            .try_into()
            .map_err(|_| bad(&self.path(key)))
    }

    fn child(&self, key: &str, allowed: &[&str]) -> Result<Self, ConfigError> {
        Self::new(self.value(key)?, self.path(key), allowed)
    }

    fn nonempty(&self, key: &str) -> Result<String, ConfigError> {
        let value: String = self.get(key)?;
        if value.is_empty() {
            return Err(bad(&self.path(key)));
        }
        Ok(value)
    }

    fn positive(&self, key: &str) -> Result<DecimalU64, ConfigError> {
        let value: DecimalU64 = self.get(key)?;
        if value.0 == 0 {
            return Err(bad(&self.path(key)));
        }
        Ok(value)
    }

    fn size(&self, key: &str) -> Result<DecimalU64, ConfigError> {
        let value = self.positive(key)?;
        usize::try_from(value.0).map_err(|_| bad(&self.path(key)))?;
        Ok(value)
    }

    fn strings(&self, key: &str) -> Result<Vec<String>, ConfigError> {
        let values: Vec<String> = self.get(key)?;
        let mut seen = BTreeSet::new();
        if values.is_empty() || values.iter().any(|s| s.is_empty() || !seen.insert(s)) {
            return Err(bad(&self.path(key)));
        }
        Ok(values)
    }

    fn wait(&self, key: &str) -> Result<MeasuredWait, ConfigError> {
        let child = self.child(key, &["elapsed-ns", "measurement-ref"])?;
        let wait = MeasuredWait {
            elapsed_ns: child.positive("elapsed-ns")?,
            measurement_ref: child.nonempty("measurement-ref")?,
        };
        checked(&child.path, wait.validate())?;
        Ok(wait)
    }
}

pub(super) fn parse_state(
    value: &Value,
    elected: &[ToolName],
) -> Result<DraftStateManagement, ConfigError> {
    let root = Object::new(
        value,
        "state-management".into(),
        &[
            "flow",
            "start",
            "persistence",
            "reads",
            "ste100",
            "tools",
            "placement",
        ],
    )?;
    let flow = parse_flow(&root)?;
    let start = if let Some(value) = root.table.get("start") {
        let start = Object::new(value, root.path("start"), &["dump-sha256"])?;
        if start.table.contains_key("dump-sha256") {
            Some(StateStartConfig {
                dump_sha256: start.get("dump-sha256")?,
            })
        } else {
            None
        }
    } else {
        None
    };
    let persistence = root.child("persistence", &["save-wait", "snapshot-wait"])?;
    let persistence = PersistenceConfig {
        save_wait: persistence.wait("save-wait")?,
        snapshot_wait: persistence.wait("snapshot-wait")?,
    };
    checked(&root.path("persistence"), persistence.validate())?;
    let reads = root.child("reads", &["max-result-bytes", "measurement-ref"])?;
    let reads = ReadLimits {
        max_result_bytes: reads.size("max-result-bytes")?,
        measurement_ref: reads.nonempty("measurement-ref")?,
    };
    checked(&root.path("reads"), reads.validate())?;
    let ste100 = parse_ste100(&root)?;
    let tools = parse_tools(&root, elected)?;
    let placement = root.child("placement", &["c-head", "c-tail", "measurement-ref"])?;
    let c_head: u64 = placement.get("c-head")?;
    let c_tail: u64 = placement.get("c-tail")?;
    if c_head == 0 {
        return Err(bad(&placement.path("c-head")));
    }
    if c_tail == 0 {
        return Err(bad(&placement.path("c-tail")));
    }
    let placement = PlacementConfig {
        c_head,
        c_tail,
        measurement_ref: placement.nonempty("measurement-ref")?,
    };
    checked(&root.path("placement"), placement.validate())?;
    Ok(DraftStateManagement {
        flow,
        start,
        persistence,
        reads,
        ste100,
        tools,
        placement,
    })
}

fn parse_flow(root: &Object<'_>) -> Result<FlowConfig, ConfigError> {
    let flow = root.child(
        "flow",
        &[
            "version",
            "partition",
            "gate-wait",
            "cut-wait",
            "observation-interval",
            "backlog",
            "scheme",
        ],
    )?;
    let version: u32 = flow.get("version")?;
    if version != 1 {
        return Err(bad(&flow.path("version")));
    }
    let rules = flow
        .value("partition")?
        .as_array()
        .ok_or_else(|| bad(&flow.path("partition")))?;
    let mut partition = Vec::with_capacity(rules.len());
    let mut ids = BTreeSet::new();
    for (index, value) in rules.iter().enumerate() {
        let rule = Object::new(
            value,
            format!("{}.partition.{index}", flow.path),
            &["id", "kind", "channels", "origins", "blocks", "context"],
        )?;
        let context: Option<String> = if rule.table.contains_key("context") {
            let context: String = rule.get("context")?;
            if context != "context" {
                return Err(bad(&rule.path("context")));
            }
            Some(context)
        } else {
            None
        };
        let entry = PartitionRule {
            id: rule.nonempty("id")?,
            kind: rule.nonempty("kind")?,
            channels: rule.strings("channels")?,
            origins: rule.strings("origins")?,
            blocks: rule.strings("blocks")?,
            context,
        };
        checked(&rule.path("blocks"), entry.validate())?;
        if !ids.insert(entry.id.clone()) {
            return Err(bad(&rule.path("id")));
        }
        partition.push(entry);
    }
    let gate_wait = flow.wait("gate-wait")?;
    let cut_wait = flow.wait("cut-wait")?;
    let interval = flow.child("observation-interval", &["elapsed-ns", "measurement-ref"])?;
    let observation_interval = MeasuredInterval {
        elapsed_ns: interval.positive("elapsed-ns")?,
        measurement_ref: interval.nonempty("measurement-ref")?,
    };
    checked(&interval.path, observation_interval.validate())?;
    let backlog = flow.child(
        "backlog",
        &[
            "max-pending-blocks",
            "max-pending-bytes",
            "lag-alarm-turns",
            "measurement-ref",
        ],
    )?;
    let backlog = FlowBacklog {
        max_pending_blocks: backlog.size("max-pending-blocks")?,
        max_pending_bytes: backlog.size("max-pending-bytes")?,
        lag_alarm_turns: backlog.positive("lag-alarm-turns")?,
        measurement_ref: backlog.nonempty("measurement-ref")?,
    };
    checked(&flow.path("backlog"), backlog.validate())?;
    let scheme = flow.child("scheme", &["scheme-id", "artifact-sha256", "verbs"])?;
    let scheme = SchemeDeclaration {
        scheme_id: scheme.nonempty("scheme-id")?,
        artifact_sha256: scheme.get::<Digest>("artifact-sha256")?,
        verbs: scheme.strings("verbs")?,
    };
    checked(
        &format!("{}.scheme.scheme-id", flow.path),
        scheme.validate(),
    )?;
    let value = FlowConfig {
        version,
        partition,
        gate_wait,
        cut_wait,
        observation_interval,
        backlog,
        scheme,
    };
    checked(&flow.path, value.validate())?;
    Ok(value)
}

fn parse_ste100(root: &Object<'_>) -> Result<Ste100Instruction, ConfigError> {
    let ste100 = root.child(
        "ste100",
        &[
            "model-binding",
            "model-artifact-sha256",
            "instances",
            "max-wire-bytes",
            "measurement-ref",
            "repair-cap",
            "decision-wait",
        ],
    )?;
    let binding = ste100.child("model-binding", &["artifact", "devices"])?;
    let artifact: ArtifactRef = binding.get("artifact")?;
    checked(
        &binding.path("artifact"),
        Ste100Instruction::validate_artifact_locator(&artifact.0),
    )?;
    let devices: Vec<DeviceOrdinal> = binding.get("devices")?;
    let mut seen = BTreeSet::new();
    if devices.is_empty() || devices.iter().any(|device| !seen.insert(device.0)) {
        return Err(bad(&binding.path("devices")));
    }
    let model_binding = ModelBinding { artifact, devices };
    let instances: u32 = ste100.get("instances")?;
    if instances == 0 {
        return Err(bad(&ste100.path("instances")));
    }
    let repair = ste100.child("repair-cap", &["attempts", "measurement-ref"])?;
    let attempts: u32 = repair.get("attempts")?;
    if attempts == 0 {
        return Err(bad(&repair.path("attempts")));
    }
    let repair_cap = RepairCap {
        attempts,
        measurement_ref: repair.nonempty("measurement-ref")?,
    };
    checked(&repair.path, repair_cap.validate())?;
    let value = Ste100Instruction {
        model_binding,
        model_artifact_sha256: ste100.get("model-artifact-sha256")?,
        instances,
        max_wire_bytes: ste100.size("max-wire-bytes")?,
        measurement_ref: ste100.nonempty("measurement-ref")?,
        repair_cap,
        decision_wait: ste100.wait("decision-wait")?,
    };
    checked(&ste100.path, value.validate())?;
    Ok(value)
}

fn parse_tools(root: &Object<'_>, elected: &[ToolName]) -> Result<ToolPolicies, ConfigError> {
    let tools = root.child("tools", &["sh"])?;
    let sh = if tools.table.contains_key("sh") {
        let policy = tools.child("sh", &["clock-ms", "measurement-ref"])?;
        let clock_ms: u64 = policy.get("clock-ms")?;
        if !(1..=SHELL_MAX_CLOCK_MS).contains(&clock_ms) {
            return Err(bad(&policy.path("clock-ms")));
        }
        let value = ToolPolicy {
            clock_ms,
            measurement_ref: policy.nonempty("measurement-ref")?,
        };
        checked(&policy.path, value.validate())?;
        Some(value)
    } else {
        None
    };
    let value = ToolPolicies { sh };
    let names = elected
        .iter()
        .map(|name| name.0.clone())
        .collect::<Vec<_>>();
    checked(&tools.path, value.validate(&names))?;
    Ok(value)
}
