//! Loaded state-management values. Runtime configuration parsing elects their use.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::{
    DecimalU64, Digest, MeasuredInterval, MeasuredWait, StateStartConfig, Ste100Instruction,
    ValidationError,
};

/// The existing gate ceiling, shared by declaration and wire validation.
pub const SHELL_MAX_CLOCK_MS: u64 = 60_000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateManagementConfig {
    pub flow: FlowConfig,
    pub start: StateStartConfig,
    pub persistence: PersistenceConfig,
    pub reads: ReadLimits,
    pub ste100: Ste100Instruction,
    pub tools: ToolPolicies,
    pub placement: PlacementConfig,
}

impl StateManagementConfig {
    /// Structural validation only. Artifact custody and measured qualification are callers' duties.
    pub fn validate(&self, elected_tools: &[String]) -> Result<(), ValidationError> {
        self.flow.validate()?;
        self.persistence.validate()?;
        self.reads.validate()?;
        self.ste100.validate()?;
        self.tools.validate(elected_tools)?;
        self.placement.validate()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlowConfig {
    pub version: u32,
    pub partition: Vec<PartitionRule>,
    pub gate_wait: MeasuredWait,
    pub cut_wait: MeasuredWait,
    pub observation_interval: MeasuredInterval,
    pub backlog: FlowBacklog,
    pub scheme: SchemeDeclaration,
}

impl FlowConfig {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.version != 1 {
            return Err(ValidationError("unsupported flow version"));
        }
        self.gate_wait.validate()?;
        self.cut_wait.validate()?;
        self.observation_interval.validate()?;
        self.backlog.validate()?;
        self.scheme.validate()?;
        let mut ids = BTreeSet::new();
        for rule in &self.partition {
            rule.validate()?;
            if !ids.insert(&rule.id) {
                return Err(ValidationError("duplicate partition rule id"));
            }
        }
        // Kind/envelope vocabulary and tuple overlap belong to the trace-aware loader.
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SchemeDeclaration {
    pub scheme_id: String,
    pub artifact_sha256: Digest,
    pub verbs: Vec<String>,
}

impl SchemeDeclaration {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.scheme_id != format!("ste100:{}", self.artifact_sha256.as_str()) {
            return Err(ValidationError("scheme id does not name its artifact"));
        }
        nonempty_unique(&self.verbs, "invalid closed verb list")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlowBacklog {
    pub max_pending_blocks: DecimalU64,
    pub max_pending_bytes: DecimalU64,
    pub lag_alarm_turns: DecimalU64,
    pub measurement_ref: String,
}

impl FlowBacklog {
    pub fn validate(&self) -> Result<(), ValidationError> {
        positive_size(self.max_pending_blocks, "invalid pending block limit")?;
        positive_size(self.max_pending_bytes, "invalid pending byte limit")?;
        if self.lag_alarm_turns.0 == 0 || self.measurement_ref.is_empty() {
            return Err(ValidationError("invalid backlog measurement or turn limit"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartitionRule {
    pub id: String,
    pub kind: String,
    pub channels: Vec<String>,
    pub origins: Vec<String>,
    pub blocks: Vec<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_option"
    )]
    pub context: Option<String>,
}

impl PartitionRule {
    /// Check projection syntax, not trace-owned vocabulary or deployment coverage.
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.id.is_empty() || self.kind.is_empty() {
            return Err(ValidationError("empty partition identity"));
        }
        nonempty_unique(&self.channels, "invalid partition channels")?;
        nonempty_unique(&self.origins, "invalid partition origins")?;
        nonempty_unique(&self.blocks, "invalid partition projections")?;
        if self
            .context
            .as_deref()
            .is_some_and(|value| value != "context")
        {
            return Err(ValidationError("invalid context projection"));
        }
        let mut paths = Vec::new();
        for block in &self.blocks {
            let path = Projection::parse(block)?;
            if paths
                .iter()
                .any(|previous: &Projection| previous.overlaps(path))
            {
                return Err(ValidationError("overlapping block projections"));
            }
            paths.push(path);
        }
        if paths.contains(&Projection::Pair) && (paths.len() != 1 || self.context.is_some()) {
            return Err(ValidationError(
                "pair projection must be whole and exclusive",
            ));
        }
        Ok(())
    }
}

/// Parsed syntax retained independently of trace kind licensing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Projection {
    Text {
        nested_message: bool,
        index: Option<u64>,
    },
    Pair,
}

impl Projection {
    pub fn parse(value: &str) -> Result<Self, ValidationError> {
        if value == "pair" {
            return Ok(Self::Pair);
        }
        let (nested_message, suffix) =
            if let Some(suffix) = value.strip_prefix("message.content.text[") {
                (true, suffix)
            } else if let Some(suffix) = value.strip_prefix("content.text[") {
                (false, suffix)
            } else {
                return Err(ValidationError("invalid projection path"));
            };
        let index = suffix
            .strip_suffix(']')
            .ok_or(ValidationError("invalid projection path"))?;
        let index = if index == "*" {
            None
        } else {
            if index.is_empty()
                || !index.bytes().all(|byte| byte.is_ascii_digit())
                || (index.len() > 1 && index.starts_with('0'))
            {
                return Err(ValidationError("noncanonical projection index"));
            }
            Some(
                index
                    .parse()
                    .map_err(|_| ValidationError("projection index overflow"))?,
            )
        };
        Ok(Self::Text {
            nested_message,
            index,
        })
    }

    fn overlaps(&self, other: Self) -> bool {
        match (*self, other) {
            (Self::Pair, _) | (_, Self::Pair) => true,
            (
                Self::Text {
                    nested_message: left,
                    index: a,
                },
                Self::Text {
                    nested_message: right,
                    index: b,
                },
            ) => left == right && (a.is_none() || b.is_none() || a == b),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersistenceConfig {
    pub save_wait: MeasuredWait,
    pub snapshot_wait: MeasuredWait,
}

impl PersistenceConfig {
    pub fn validate(&self) -> Result<(), ValidationError> {
        self.save_wait.validate()?;
        self.snapshot_wait.validate()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadLimits {
    pub max_result_bytes: DecimalU64,
    pub measurement_ref: String,
}

impl ReadLimits {
    pub fn validate(&self) -> Result<(), ValidationError> {
        positive_size(self.max_result_bytes, "invalid complete read limit")?;
        if self.measurement_ref.is_empty() {
            return Err(ValidationError("missing read measurement reference"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlacementConfig {
    pub c_head: u64,
    pub c_tail: u64,
    pub measurement_ref: String,
}

impl PlacementConfig {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.c_head == 0 || self.c_tail == 0 || self.measurement_ref.is_empty() {
            return Err(ValidationError("invalid placement declaration"));
        }
        self.c_head
            .checked_add(self.c_tail)
            .ok_or(ValidationError("placement budget overflow"))?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolPolicies {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_option"
    )]
    pub sh: Option<ToolPolicy>,
}

impl ToolPolicies {
    pub fn validate(&self, elected: &[String]) -> Result<(), ValidationError> {
        if elected.len() > 1 || elected.iter().any(|name| name != "sh") {
            return Err(ValidationError("unknown or duplicate elected tool"));
        }
        if self.sh.is_some() != (elected.len() == 1) {
            return Err(ValidationError("tool policy does not match election"));
        }
        if let Some(policy) = &self.sh {
            policy.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolPolicy {
    pub clock_ms: u64,
    pub measurement_ref: String,
}

impl ToolPolicy {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if !(1..=SHELL_MAX_CLOCK_MS).contains(&self.clock_ms) || self.measurement_ref.is_empty() {
            return Err(ValidationError(
                "invalid tool clock or measurement reference",
            ));
        }
        Ok(())
    }
}

pub(super) fn positive_size(
    value: DecimalU64,
    message: &'static str,
) -> Result<usize, ValidationError> {
    if value.0 == 0 {
        return Err(ValidationError(message));
    }
    usize::try_from(value.0).map_err(|_| ValidationError(message))
}

fn present_option<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

pub(super) fn nonempty_unique(
    values: &[String],
    message: &'static str,
) -> Result<(), ValidationError> {
    let mut seen = BTreeSet::new();
    if values.is_empty()
        || values
            .iter()
            .any(|value| value.is_empty() || !seen.insert(value))
    {
        return Err(ValidationError(message));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projection_rejects_arbitrary_and_noncanonical_paths() {
        for invalid in [
            "content.text[01]",
            "content.text[-1]",
            "content[0]",
            "content.text[+1]",
            "content.text[18446744073709551616]",
            "content.text[0].value",
        ] {
            assert!(Projection::parse(invalid).is_err(), "{invalid}");
        }
        for valid in ["content.text[0]", "message.content.text[*]", "pair"] {
            assert!(Projection::parse(valid).is_ok(), "{valid}");
        }
    }

    #[test]
    fn projection_checks_overlap_and_closed_fields() {
        let base = r#"{"id":"input","kind":"message.user","channels":["input"],"origins":["operator"],"blocks":["content.text[0]"]}"#;
        let mut rule: PartitionRule = serde_json::from_str(base).unwrap();
        assert!(rule.validate().is_ok());
        rule.blocks.push("content.text[*]".into());
        assert!(rule.validate().is_err());
        assert!(
            serde_json::from_str::<PartitionRule>(
                &base.replace("\"id\":\"input\"", "\"id\":\"input\",\"id\":\"again\"")
            )
            .is_err()
        );
        assert!(
            serde_json::from_str::<PartitionRule>(&base.replace(
                "\"id\":\"input\"",
                "\"id\":\"input\",\"predicate\":\"secret\""
            ))
            .is_err()
        );
    }

    #[test]
    fn policy_checks_election_and_existing_ceiling() {
        let mut policies = ToolPolicies {
            sh: Some(ToolPolicy {
                clock_ms: SHELL_MAX_CLOCK_MS,
                measurement_ref: "measured".into(),
            }),
        };
        assert!(policies.validate(&["sh".into()]).is_ok());
        assert!(policies.validate(&[]).is_err());
        policies.sh.as_mut().unwrap().clock_ms += 1;
        assert!(policies.validate(&["sh".into()]).is_err());
    }

    #[test]
    fn capacities_refuse_zero_and_overflow() {
        let config = PlacementConfig {
            c_head: u64::MAX,
            c_tail: 1,
            measurement_ref: "measured".into(),
        };
        assert!(config.validate().is_err());
        assert!(
            ReadLimits {
                max_result_bytes: DecimalU64(0),
                measurement_ref: "measured".into()
            }
            .validate()
            .is_err()
        );
    }
}
