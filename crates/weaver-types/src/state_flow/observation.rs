//! Required operation evidence carried with state responses, never a content unit.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::{DecimalU64, SourceBlockId, ValidationError};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateOperationObservation {
    pub operation_id: DecimalU64,
    #[serde(deserialize_with = "super::required_option")]
    pub parent_id: Option<DecimalU64>,
    pub request_id: DecimalU64,
    pub operation: StateOperation,
    pub began_ns: DecimalU64,
    pub ended_ns: DecimalU64,
    pub phases: Vec<OperationPhase>,
    pub resource: OperationResource,
    pub disk: OperationDisk,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StateOperation {
    Open,
    File,
    FilePair,
    Read,
    Next,
    Release,
    Shape,
    Held,
    Position,
    Grants,
    Snapshot,
    Acknowledge,
    Abort,
    ReleaseCapture,
}

impl StateOperation {
    pub fn is_filing(self) -> bool {
        matches!(self, Self::File | Self::FilePair)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationPhase {
    pub name: PhaseName,
    pub began_ns: DecimalU64,
    pub ended_ns: DecimalU64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PhaseName {
    Validate,
    Query,
    Transaction,
    Capture,
    Filesystem,
    Encode,
}

impl PhaseName {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Validate => "validate",
            Self::Query => "query",
            Self::Transaction => "transaction",
            Self::Capture => "capture",
            Self::Filesystem => "filesystem",
            Self::Encode => "encode",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationResource {
    #[serde(deserialize_with = "super::required_option")]
    pub cpu_ns: Option<DecimalU64>,
    #[serde(deserialize_with = "super::required_option")]
    pub copied_bytes: Option<DecimalU64>,
    #[serde(deserialize_with = "super::required_option")]
    pub held_bytes: Option<DecimalU64>,
    pub unavailable: Vec<ResourceUnavailable>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceUnavailable {
    pub field: ResourceField,
    pub reason: ObservationUnavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceField {
    CpuNs,
    CopiedBytes,
    HeldBytes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationUnavailable {
    Unsupported,
    Denied,
    Disabled,
    ReadFailed,
    CounterReset,
    Overflow,
    Uninstrumented,
    NotSampled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationDisk {
    #[serde(deserialize_with = "super::required_option")]
    pub enabled: Option<bool>,
    #[serde(deserialize_with = "super::required_option")]
    pub before: Option<DiskSample>,
    #[serde(deserialize_with = "super::required_option")]
    pub after: Option<DiskSample>,
    #[serde(deserialize_with = "super::required_option")]
    pub unavailable_reason: Option<ObservationUnavailable>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiskSample {
    pub major_faults: DecimalU64,
    pub read_bytes: DecimalU64,
    pub write_bytes: DecimalU64,
    pub blkio_delay_ticks: DecimalU64,
    pub ticks_per_second: DecimalU64,
}

impl OperationResource {
    pub fn validate(&self) -> Result<(), ValidationError> {
        let missing: Vec<_> = [
            (ResourceField::CpuNs, &self.cpu_ns),
            (ResourceField::CopiedBytes, &self.copied_bytes),
            (ResourceField::HeldBytes, &self.held_bytes),
        ]
        .into_iter()
        .filter_map(|(field, value)| value.is_none().then_some(field))
        .collect();
        if !self.unavailable.iter().map(|entry| entry.field).eq(missing) {
            return Err(ValidationError(
                "resource availability does not match null fields in order",
            ));
        }
        Ok(())
    }
}

impl OperationDisk {
    pub fn validate(&self) -> Result<(), ValidationError> {
        match (&self.before, &self.after) {
            (Some(before), Some(after)) => {
                if before.ticks_per_second.0 == 0
                    || before.ticks_per_second != after.ticks_per_second
                    || before.major_faults > after.major_faults
                    || before.read_bytes > after.read_bytes
                    || before.write_bytes > after.write_bytes
                    || before.blkio_delay_ticks > after.blkio_delay_ticks
                {
                    return Err(ValidationError("invalid disk sample interval"));
                }
                if self.enabled.is_some() != self.unavailable_reason.is_none() {
                    return Err(ValidationError("disk availability reason mismatch"));
                }
            }
            (None, None) if self.unavailable_reason.is_some() => {}
            _ => {
                return Err(ValidationError(
                    "disk samples must be complete or unavailable together",
                ));
            }
        }
        Ok(())
    }
}

impl StateOperationObservation {
    /// Tight maximum for validated compact JSON, including all six phases and unavailable entries.
    /// The maximum fixture below pins the finite representation, not a measured budget.
    pub const MAX_ENCODED_LEN: usize = 1422;

    pub fn validate(
        &self,
        request_id: DecimalU64,
        operation: StateOperation,
        operation_id: DecimalU64,
    ) -> Result<(), ValidationError> {
        if self.request_id != request_id
            || self.operation != operation
            || self.operation_id != operation_id
        {
            return Err(ValidationError("observation correlation mismatch"));
        }
        self.validate_shape()
    }

    pub fn validate_shape(&self) -> Result<(), ValidationError> {
        if self.operation_id.0 == 0
            || self.request_id.0 == 0
            || self.parent_id.is_some()
            || self.began_ns > self.ended_ns
        {
            return Err(ValidationError("invalid observation identity or interval"));
        }
        let mut seen = BTreeSet::new();
        let mut previous = None;
        for phase in &self.phases {
            let order = (phase.began_ns, phase.name.as_str());
            if !seen.insert(phase.name)
                || previous.is_some_and(|last| last > order)
                || phase.began_ns < self.began_ns
                || phase.ended_ns > self.ended_ns
                || phase.began_ns > phase.ended_ns
            {
                return Err(ValidationError(
                    "invalid, repeated or unordered observation phase",
                ));
            }
            previous = Some(order);
        }
        if !seen.contains(&PhaseName::Validate) || !seen.contains(&PhaseName::Encode) {
            return Err(ValidationError("missing required validate or encode phase"));
        }
        self.resource.validate()?;
        self.disk.validate()?;
        if self.operation != StateOperation::Snapshot
            && (self.resource.cpu_ns.is_some()
                || self
                    .resource
                    .unavailable
                    .iter()
                    .find(|entry| entry.field == ResourceField::CpuNs)
                    .map(|entry| entry.reason)
                    != Some(ObservationUnavailable::NotSampled)
                || self.disk.enabled.is_some()
                || self.disk.before.is_some()
                || self.disk.after.is_some()
                || self.disk.unavailable_reason != Some(ObservationUnavailable::NotSampled))
        {
            return Err(ValidationError(
                "non-snapshot operation cannot sample CPU or disk",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemberObservationIdentity {
    pub instrument_version: String,
    pub pid: u32,
    #[serde(deserialize_with = "super::required_option")]
    pub start_ticks: Option<DecimalU64>,
    #[serde(deserialize_with = "super::required_option")]
    pub ticks_per_second: Option<DecimalU64>,
    #[serde(deserialize_with = "super::required_option")]
    pub unavailable_reason: Option<ObservationUnavailable>,
}

impl MemberObservationIdentity {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.instrument_version != "state-observation/1" || self.pid == 0 {
            return Err(ValidationError("invalid member observation identity"));
        }
        match (
            self.start_ticks,
            self.ticks_per_second,
            self.unavailable_reason,
        ) {
            (Some(_), Some(ticks), None) if ticks.0 > 0 => Ok(()),
            (None, None, Some(_)) => Ok(()),
            _ => Err(ValidationError(
                "member start identity availability mismatch",
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoredOperationObservation {
    #[serde(deserialize_with = "super::required_option")]
    pub request_source: Option<SourceBlockId>,
    pub observation: StateOperationObservation,
}

impl StoredOperationObservation {
    pub fn validate(&self) -> Result<(), ValidationError> {
        self.observation.validate_shape()?;
        if self.request_source.is_some() != self.observation.operation.is_filing() {
            return Err(ValidationError(
                "observation source must name a filing only",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateStoreMeasurement {
    pub observation: StateOperationObservation,
    #[serde(deserialize_with = "super::required_option")]
    pub request_source: Option<SourceBlockId>,
    pub harness_queued_ns: DecimalU64,
    pub harness_sent_ns: DecimalU64,
    pub harness_received_ns: DecimalU64,
}

impl StateStoreMeasurement {
    pub fn validate(&self) -> Result<(), ValidationError> {
        self.observation.validate_shape()?;
        if self.request_source.is_some() != self.observation.operation.is_filing()
            || self.harness_queued_ns > self.harness_sent_ns
            || self.harness_sent_ns > self.observation.began_ns
            || self.observation.ended_ns > self.harness_received_ns
        {
            return Err(ValidationError(
                "store measurement source or clock ordering mismatch",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observation() -> StateOperationObservation {
        StateOperationObservation {
            operation_id: DecimalU64(1),
            parent_id: None,
            request_id: DecimalU64(1),
            operation: StateOperation::Read,
            began_ns: DecimalU64(10),
            ended_ns: DecimalU64(20),
            phases: vec![
                OperationPhase {
                    name: PhaseName::Validate,
                    began_ns: DecimalU64(10),
                    ended_ns: DecimalU64(11),
                },
                OperationPhase {
                    name: PhaseName::Encode,
                    began_ns: DecimalU64(19),
                    ended_ns: DecimalU64(20),
                },
            ],
            resource: OperationResource {
                cpu_ns: None,
                copied_bytes: Some(DecimalU64(1)),
                held_bytes: Some(DecimalU64(2)),
                unavailable: vec![ResourceUnavailable {
                    field: ResourceField::CpuNs,
                    reason: ObservationUnavailable::NotSampled,
                }],
            },
            disk: OperationDisk {
                enabled: None,
                before: None,
                after: None,
                unavailable_reason: Some(ObservationUnavailable::NotSampled),
            },
        }
    }

    #[test]
    fn phases_and_missing_values_are_checked() {
        let valid = observation();
        assert!(
            valid
                .validate(DecimalU64(1), StateOperation::Read, DecimalU64(1))
                .is_ok()
        );
        assert!(
            valid
                .validate(DecimalU64(2), StateOperation::Read, DecimalU64(1))
                .is_err()
        );
        let mut bad = valid.clone();
        bad.phases.push(bad.phases[0].clone());
        assert!(bad.validate_shape().is_err());
        let mut bad = valid.clone();
        bad.phases[1].ended_ns = DecimalU64(21);
        assert!(bad.validate_shape().is_err());
        let mut bad = valid.clone();
        bad.resource.unavailable.clear();
        assert!(bad.validate_shape().is_err());
        let mut bad = valid;
        bad.parent_id = Some(DecimalU64(1));
        assert!(bad.validate_shape().is_err());
    }

    #[test]
    fn response_metadata_never_samples_serving_cpu_or_disk() {
        let mut bad = observation();
        bad.resource.cpu_ns = Some(DecimalU64(0));
        bad.resource.unavailable.clear();
        assert!(bad.validate_shape().is_err());
        bad.operation = StateOperation::Snapshot;
        assert!(bad.validate_shape().is_ok());
        bad.operation_id = DecimalU64(2);
        assert!(
            bad.validate(DecimalU64(1), StateOperation::Snapshot, DecimalU64(2))
                .is_ok()
        );
    }

    #[test]
    fn stored_observation_cannot_hide_a_zero_request_id() {
        let mut stored = StoredOperationObservation {
            request_source: None,
            observation: observation(),
        };
        assert!(stored.validate().is_ok());
        stored.observation.request_id = DecimalU64(0);
        assert!(stored.validate().is_err());
    }

    #[test]
    fn nulls_are_required_and_unknown_fields_refuse() {
        let json = serde_json::to_string(&observation()).unwrap();
        assert!(
            serde_json::from_str::<StateOperationObservation>(
                &json.replace("\"parent_id\":null,", "")
            )
            .is_err()
        );
        assert!(
            serde_json::from_str::<StateOperationObservation>(&json.replace(
                "\"parent_id\":null",
                "\"parent_id\":null,\"parent_id\":null"
            ))
            .is_err()
        );
        assert!(
            serde_json::from_str::<StateOperationObservation>(
                &json.replace("\"resource\":{", "\"resource\":{\"unknown\":0,")
            )
            .is_err()
        );
    }

    #[test]
    fn tight_encoded_bound_includes_worst_case_availability_and_all_phases() {
        let max = DecimalU64(u64::MAX);
        let sample = DiskSample {
            major_faults: max,
            read_bytes: max,
            write_bytes: max,
            blkio_delay_ticks: max,
            ticks_per_second: max,
        };
        let mut value = observation();
        value.operation_id = max;
        value.request_id = max;
        value.operation = StateOperation::Snapshot;
        value.began_ns = max;
        value.ended_ns = max;
        value.phases = [
            PhaseName::Capture,
            PhaseName::Encode,
            PhaseName::Filesystem,
            PhaseName::Query,
            PhaseName::Transaction,
            PhaseName::Validate,
        ]
        .map(|name| OperationPhase {
            name,
            began_ns: max,
            ended_ns: max,
        })
        .to_vec();
        value.resource = OperationResource {
            cpu_ns: None,
            copied_bytes: None,
            held_bytes: None,
            unavailable: [
                ResourceField::CpuNs,
                ResourceField::CopiedBytes,
                ResourceField::HeldBytes,
            ]
            .map(|field| ResourceUnavailable {
                field,
                reason: ObservationUnavailable::Uninstrumented,
            })
            .to_vec(),
        };
        value.disk = OperationDisk {
            enabled: None,
            before: Some(sample.clone()),
            after: Some(sample),
            unavailable_reason: Some(ObservationUnavailable::Uninstrumented),
        };
        assert!(value.validate_shape().is_ok());
        assert_eq!(
            serde_json::to_vec(&value).unwrap().len(),
            StateOperationObservation::MAX_ENCODED_LEN
        );
        // Missing counters are longer than present maximum-width counters because each names its reason.
        for mask in 0..8 {
            let mut variant = value.clone();
            if mask & 1 != 0 {
                variant.resource.cpu_ns = Some(max);
            }
            if mask & 2 != 0 {
                variant.resource.copied_bytes = Some(max);
            }
            if mask & 4 != 0 {
                variant.resource.held_bytes = Some(max);
            }
            variant
                .resource
                .unavailable
                .retain(|entry| match entry.field {
                    ResourceField::CpuNs => mask & 1 == 0,
                    ResourceField::CopiedBytes => mask & 2 == 0,
                    ResourceField::HeldBytes => mask & 4 == 0,
                });
            assert!(variant.validate_shape().is_ok());
            assert!(
                serde_json::to_vec(&variant).unwrap().len()
                    <= StateOperationObservation::MAX_ENCODED_LEN
            );
        }
    }
}
