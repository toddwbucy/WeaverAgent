//! Compiled template inputs and the proof of their recorded load-time check.
//!
//! TEMPLATES and TYPES-TEMPLATE keep this authority inside the harness. The
//! checker proves grammar inclusion. This owner accounts for the exact compiled
//! inventory and the ordered, recorded replies. It elects no production words,
//! grammar, substitution spelling, or qualification artifact.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use sha2::{Digest as _, Sha256};
use weaver_trace::serving::{ContentSchema, Kind, content_schema, payloads as trace};
use weaver_types::state_flow::{
    DecimalU64, Digest, SchemeDeclaration, SourcePosition, Ste100Reply, Ste100Request,
    Ste100Result, Ste100Target, TemplatePart, TemplateSpec,
};

use super::coordinator::{CheckOperation, RecordedCheck};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TemplateError {
    InvalidInventory,
    DuplicateTemplate,
    ConflictingSlot,
    Encoding,
    InvalidIdentity,
    LoadMismatch,
    OutOfOrder,
    CheckMismatch,
    InvalidResult,
    ComponentFailed,
    Incomplete,
    Rejected,
}

impl fmt::Display for TemplateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidInventory => "invalid compiled template inventory",
            Self::DuplicateTemplate => "duplicate compiled template identity",
            Self::ConflictingSlot => "one template slot has conflicting classes",
            Self::Encoding => "template inventory encoding failed",
            Self::InvalidIdentity => "invalid template check identity",
            Self::LoadMismatch => "template inventory differs from recorded load",
            Self::OutOfOrder => "template check is out of inventory order",
            Self::CheckMismatch => "template check does not match the compiled entry",
            Self::InvalidResult => "invalid recorded template result",
            Self::ComponentFailed => "template checker reported component failure",
            Self::Incomplete => "template check transcript is incomplete",
            Self::Rejected => "template inventory was rejected",
        })
    }
}

impl std::error::Error for TemplateError {}

/// Static storage is the compiled source, not runtime text supplied by a loop.
/// The ordered vector's compact JSON bytes are the inventory digest's input.
#[derive(Debug, Clone)]
pub(super) struct CompiledInventory {
    templates: &'static [TemplateSpec],
    bytes: Vec<u8>,
    digest: Digest,
    texts: Vec<String>,
}

impl CompiledInventory {
    pub(super) fn new(templates: &'static [TemplateSpec]) -> Result<Self, TemplateError> {
        let mut ids = BTreeSet::new();
        let mut texts = Vec::with_capacity(templates.len());
        for template in templates {
            template
                .validate()
                .map_err(|_| TemplateError::InvalidInventory)?;
            if !Kind::from_name(&template.kind).is_some_and(|kind| {
                matches!(
                    content_schema(kind),
                    ContentSchema::OneText { .. } | ContentSchema::Plan
                )
            }) {
                return Err(TemplateError::InvalidInventory);
            }
            if !ids.insert(&template.template_id) {
                return Err(TemplateError::DuplicateTemplate);
            }
            let mut slots = BTreeMap::new();
            for part in &template.parts {
                if let TemplatePart::Slot { name, class } = part
                    && slots.insert(name, class).is_some_and(|old| old != class)
                {
                    return Err(TemplateError::ConflictingSlot);
                }
            }
            texts.push(serde_json::to_string(template).map_err(|_| TemplateError::Encoding)?);
        }
        let bytes = serde_json::to_vec(templates).map_err(|_| TemplateError::Encoding)?;
        let digest = Digest::new(format!("{:x}", Sha256::digest(&bytes)))
            .map_err(|_| TemplateError::Encoding)?;
        Ok(Self {
            templates,
            bytes,
            digest,
            texts,
        })
    }

    pub(super) fn templates(&self) -> &'static [TemplateSpec] {
        self.templates
    }

    pub(super) fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub(super) fn digest(&self) -> &Digest {
        &self.digest
    }

    pub(super) fn record(
        &self,
        scheme: &SchemeDeclaration,
        checker: &Digest,
    ) -> trace::TemplateInventory {
        trace::TemplateInventory {
            scheme_id: scheme.scheme_id.clone(),
            inventory_sha256: self.digest.as_str().into(),
            checker_artifact_sha256: checker.as_str().into(),
            templates: self.templates.iter().map(mirror_template).collect(),
        }
    }

    pub(super) fn matches_load(
        &self,
        load: &trace::TemplateInventory,
        scheme: &SchemeDeclaration,
        checker: &Digest,
    ) -> Result<(), TemplateError> {
        if *load != self.record(scheme, checker) {
            return Err(TemplateError::LoadMismatch);
        }
        Ok(())
    }

    fn operation(&self, index: usize) -> Option<CheckOperation> {
        self.templates
            .get(index)
            .zip(self.texts.get(index))
            .map(|(template, text)| CheckOperation::Template {
                template_id: template.template_id.clone(),
                text: text.clone(),
            })
    }
}

fn mirror_template(template: &TemplateSpec) -> trace::TemplateSpec {
    let TemplateSpec {
        template_id,
        kind,
        parts,
    } = template;
    trace::TemplateSpec {
        template_id: template_id.clone(),
        kind: kind.clone(),
        parts: parts
            .iter()
            .map(|part| match part {
                TemplatePart::Literal(text) => trace::TemplatePart::Literal(text.clone()),
                TemplatePart::Slot { name, class } => trace::TemplatePart::Slot {
                    name: name.clone(),
                    class: match class {
                        weaver_types::state_flow::SlotClass::TechnicalName => {
                            trace::SlotClass::TechnicalName
                        }
                        weaver_types::state_flow::SlotClass::StepId => trace::SlotClass::StepId,
                    },
                },
            })
            .collect(),
    }
}

/// A validated recorded acceptance constructs this entry. Only the complete
/// accepted transcript can place these entries in a certificate.
#[derive(Debug)]
pub(super) struct TemplateAcceptance {
    template_id: String,
    request_id: DecimalU64,
    result_source: SourcePosition,
}

impl TemplateAcceptance {
    pub(super) fn template_id(&self) -> &str {
        &self.template_id
    }
    pub(super) fn request_id(&self) -> DecimalU64 {
        self.request_id
    }
    pub(super) fn result_source(&self) -> &SourcePosition {
        &self.result_source
    }
}

/// The run-local capability has no serialization or public value constructor.
#[derive(Debug)]
pub(super) struct TemplateCertificate {
    run_id: String,
    scheme_id: String,
    scheme: SchemeDeclaration,
    checker_artifact_sha256: Digest,
    inventory_sha256: Digest,
    accepted: Vec<TemplateAcceptance>,
    inventory: CompiledInventory,
}

impl TemplateCertificate {
    pub(super) fn run_id(&self) -> &str {
        &self.run_id
    }
    pub(super) fn scheme_id(&self) -> &str {
        &self.scheme_id
    }
    pub(super) fn scheme(&self) -> &SchemeDeclaration {
        &self.scheme
    }
    pub(super) fn checker_artifact_sha256(&self) -> &Digest {
        &self.checker_artifact_sha256
    }
    pub(super) fn inventory_sha256(&self) -> &Digest {
        &self.inventory_sha256
    }
    pub(super) fn accepted(&self) -> &[TemplateAcceptance] {
        &self.accepted
    }
    pub(super) fn inventory(&self) -> &CompiledInventory {
        &self.inventory
    }
}

#[derive(Debug)]
pub(super) struct TemplateCheckRecord {
    operation: CheckOperation,
    reply: Ste100Reply,
    result_source: SourcePosition,
}

impl TemplateCheckRecord {
    pub(super) fn operation(&self) -> &CheckOperation {
        &self.operation
    }
    pub(super) fn reply(&self) -> &Ste100Reply {
        &self.reply
    }
    pub(super) fn result_source(&self) -> &SourcePosition {
        &self.result_source
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TranscriptProgress {
    Accepted,
    Complete,
    Rejected,
}

/// The context supplies the actually recorded load tuple. Only RecordedCheck
/// can advance this transcript, so decoded replies alone cannot certify it.
#[derive(Debug)]
pub(super) struct TemplateCheckTranscript {
    inventory: CompiledInventory,
    run_id: String,
    scheme: SchemeDeclaration,
    checker: Digest,
    records: Vec<TemplateCheckRecord>,
    accepted: Vec<TemplateAcceptance>,
    failure: Option<TemplateError>,
    certified: bool,
}

impl TemplateCheckTranscript {
    pub(super) fn begin(
        inventory: CompiledInventory,
        run_id: String,
        scheme: SchemeDeclaration,
        checker: Digest,
        load: &trace::TemplateInventory,
    ) -> Result<Self, TemplateError> {
        Self::begin_preserving(inventory, run_id, scheme, checker, load).map_err(|(_, error)| error)
    }

    pub(super) fn begin_preserving(
        inventory: CompiledInventory,
        run_id: String,
        scheme: SchemeDeclaration,
        checker: Digest,
        load: &trace::TemplateInventory,
    ) -> Result<Self, (CompiledInventory, TemplateError)> {
        if run_id.is_empty() || scheme.validate().is_err() {
            return Err((inventory, TemplateError::InvalidIdentity));
        }
        if let Err(error) = inventory.matches_load(load, &scheme, &checker) {
            return Err((inventory, error));
        }
        Ok(Self {
            inventory,
            run_id,
            scheme,
            checker,
            records: Vec::new(),
            accepted: Vec::new(),
            failure: None,
            certified: false,
        })
    }

    pub(super) fn next_operation(&self) -> Option<CheckOperation> {
        if self.failure.is_some() || self.certified {
            return None;
        }
        self.inventory.operation(self.accepted.len())
    }

    pub(super) fn accepted_count(&self) -> usize {
        self.accepted.len()
    }

    pub(super) fn is_complete(&self) -> bool {
        self.failure.is_none() && self.accepted.len() == self.inventory.templates.len()
    }

    pub(super) fn records(&self) -> &[TemplateCheckRecord] {
        &self.records
    }

    pub(super) fn rejection(&self) -> Option<&Ste100Reply> {
        if self.failure == Some(TemplateError::Rejected) {
            self.records
                .get(self.accepted.len())
                .map(|record| &record.reply)
        } else {
            None
        }
    }

    pub(super) fn failure(&self) -> Option<TemplateError> {
        self.failure
    }

    pub(super) fn recorded(
        &mut self,
        recorded: RecordedCheck,
    ) -> Result<TranscriptProgress, TemplateError> {
        let (operation, reply, result_source) = recorded.into_parts();
        let record = TemplateCheckRecord {
            operation,
            reply,
            result_source,
        };
        // Preserve a complete offending record too. No subsequent result can
        // erase the first failure or turn a failed transcript into a pass.
        let result = self.validate_record(&record);
        self.records.push(record);
        match result {
            Ok(acceptance) => {
                self.accepted.push(acceptance);
                Ok(if self.is_complete() {
                    TranscriptProgress::Complete
                } else {
                    TranscriptProgress::Accepted
                })
            }
            Err(TemplateError::Rejected) if self.failure.is_none() => {
                self.failure = Some(TemplateError::Rejected);
                Ok(TranscriptProgress::Rejected)
            }
            Err(error) => {
                let first = *self.failure.get_or_insert(error);
                Err(first)
            }
        }
    }

    fn validate_record(
        &self,
        record: &TemplateCheckRecord,
    ) -> Result<TemplateAcceptance, TemplateError> {
        if self.failure.is_some() || self.certified || self.is_complete() {
            return Err(TemplateError::OutOfOrder);
        }
        let expected = self
            .inventory
            .operation(self.accepted.len())
            .ok_or(TemplateError::OutOfOrder)?;
        if record.operation != expected {
            return Err(TemplateError::CheckMismatch);
        }
        let CheckOperation::Template { template_id, text } = &expected else {
            unreachable!("the compiled inventory contains only template operations")
        };
        let source = &record.result_source;
        if source.block_id.run != self.run_id
            || source.block_id.sequence.0 == 0
            || source.block_id.block.0 != 0
            || source.turn.is_some()
            || self.records.last().is_some_and(|previous| {
                source.block_id.sequence <= previous.result_source.block_id.sequence
                    || record.reply.request_id <= previous.reply.request_id
            })
        {
            return Err(TemplateError::InvalidResult);
        }
        let request = Ste100Request {
            request_id: record.reply.request_id,
            target: Ste100Target::Template {
                template_id: template_id.clone(),
            },
            text: text.clone(),
            scheme_id: self.scheme.scheme_id.clone(),
        };
        record
            .reply
            .validate_for(&request, &self.scheme.verbs)
            .map_err(|_| TemplateError::InvalidResult)?;
        match &record.reply.result {
            Ste100Result::Accepted { bindings } if bindings.is_empty() => Ok(TemplateAcceptance {
                template_id: template_id.clone(),
                request_id: record.reply.request_id,
                result_source: record.result_source.clone(),
            }),
            Ste100Result::Accepted { .. } => Err(TemplateError::InvalidResult),
            Ste100Result::NotSte100 { .. } => Err(TemplateError::Rejected),
            Ste100Result::ComponentFailed { .. } => Err(TemplateError::ComponentFailed),
        }
    }

    pub(super) fn finish(&mut self) -> Result<TemplateCertificate, TemplateError> {
        if let Some(error) = self.failure {
            return Err(error);
        }
        if self.certified {
            return Err(TemplateError::OutOfOrder);
        }
        if !self.is_complete() {
            return Err(TemplateError::Incomplete);
        }
        let certificate = TemplateCertificate {
            run_id: self.run_id.clone(),
            scheme_id: self.scheme.scheme_id.clone(),
            scheme: self.scheme.clone(),
            checker_artifact_sha256: self.checker.clone(),
            inventory_sha256: self.inventory.digest.clone(),
            accepted: self
                .accepted
                .iter()
                .map(|acceptance| TemplateAcceptance {
                    template_id: acceptance.template_id.clone(),
                    request_id: acceptance.request_id,
                    result_source: acceptance.result_source.clone(),
                })
                .collect(),
            inventory: self.inventory.clone(),
        };
        self.certified = true;
        Ok(certificate)
    }
}

#[path = "template_expansion.rs"]
mod expansion;
#[allow(unused_imports)] // The repair owner consumes the certified runtime port.
pub(super) use expansion::{
    ExpansionIdentity, ExpansionRefusal, RestatementTemplate, TemplateExpansion, expand_begin_task,
    expand_outbound, expand_wind_down,
};

#[cfg(test)]
#[path = "templates_tests.rs"]
mod tests;
