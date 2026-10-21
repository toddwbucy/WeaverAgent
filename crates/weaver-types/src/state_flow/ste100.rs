//! The separate SPU language-checking contract, never the legacy label scorer.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::declaration::{nonempty_unique, positive_size};
use super::{
    Binding, CallId, DecimalU64, Digest, MeasuredWait, PlanRef, ReviewSetupRef, SchemeDeclaration,
    SourceBlockId, SourcePosition, Span, SpanRef, StatementRef, StepId, ValidationError, WorkRef,
    validate_bindings,
};
use crate::ModelBinding;

pub const STE100_PROTOCOL: &str = "state-ste100/1";
pub const STE100_BINDER_ABI: &str = "ste100-span/1";
pub const STE100_LABELS: [&str; 7] = [
    "O", "B-VERB", "I-VERB", "B-NOUN", "I-NOUN", "B-COND", "I-COND",
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ste100Instruction {
    pub model_binding: ModelBinding,
    pub model_artifact_sha256: Digest,
    pub instances: u32,
    pub max_wire_bytes: DecimalU64,
    pub measurement_ref: String,
    pub repair_cap: RepairCap,
    pub decision_wait: MeasuredWait,
}

impl Ste100Instruction {
    pub fn validate(&self) -> Result<(), ValidationError> {
        positive_size(self.max_wire_bytes, "invalid STE100 wire limit")?;
        if self.instances == 0 || self.measurement_ref.is_empty() {
            return Err(ValidationError("invalid STE100 instances or measurement"));
        }
        let mut devices = BTreeSet::new();
        if self.model_binding.artifact.0.is_empty()
            || self.model_binding.devices.is_empty()
            || self
                .model_binding
                .devices
                .iter()
                .any(|device| !devices.insert(device.0))
        {
            return Err(ValidationError("invalid STE100 model binding"));
        }
        self.repair_cap.validate()?;
        self.decision_wait.validate()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepairCap {
    pub attempts: u32,
    pub measurement_ref: String,
}

impl RepairCap {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.attempts == 0 || self.measurement_ref.is_empty() {
            return Err(ValidationError("invalid repair cap"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Ste100Target {
    Source(SourceBlockId),
    Input {
        input_id: DecimalU64,
        block: DecimalU64,
    },
    Template {
        template_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ste100Request {
    pub request_id: DecimalU64,
    pub target: Ste100Target,
    pub text: String,
    pub scheme_id: String,
}

impl Ste100Request {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.request_id.0 == 0 || self.scheme_id.is_empty() {
            return Err(ValidationError("invalid classifier request identity"));
        }
        match &self.target {
            Ste100Target::Input { input_id, .. } if input_id.0 == 0 => {
                Err(ValidationError("input id must be positive"))
            }
            Ste100Target::Template { template_id } => {
                let template: TemplateSpec = serde_json::from_str(&self.text)
                    .map_err(|_| ValidationError("invalid template representation"))?;
                template.validate()?;
                if &template.template_id != template_id {
                    return Err(ValidationError("template target mismatch"));
                }
                let canonical = serde_json::to_string(&template)
                    .map_err(|_| ValidationError("template encoding failed"))?;
                if canonical != self.text {
                    return Err(ValidationError("template text is not canonical JSON"));
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ste100Reply {
    pub request_id: DecimalU64,
    pub target: Ste100Target,
    pub scheme_id: String,
    pub result: Ste100Result,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Ste100Result {
    Accepted {
        bindings: Vec<Binding>,
    },
    NotSte100 {
        code: Ste100DefectCode,
        detail: String,
        spans: Vec<Span>,
    },
    ComponentFailed {
        code: Ste100ComponentCode,
        detail: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ste100DefectCode {
    NonSte100,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ste100ComponentCode {
    Crashed,
    NoResult,
    InvalidResult,
    TransportFailed,
}

impl Ste100Reply {
    /// Prove representation and correlation, not truth or semantic accuracy.
    pub fn validate_for(
        &self,
        request: &Ste100Request,
        verbs: &[String],
    ) -> Result<(), ValidationError> {
        request.validate()?;
        if self.request_id != request.request_id
            || self.target != request.target
            || self.scheme_id != request.scheme_id
        {
            return Err(ValidationError("classifier reply identity mismatch"));
        }
        match &self.result {
            Ste100Result::Accepted { bindings } => {
                if matches!(request.target, Ste100Target::Template { .. }) {
                    if !bindings.is_empty() {
                        return Err(ValidationError(
                            "template acceptance cannot invent bindings",
                        ));
                    }
                    Ok(())
                } else {
                    validate_bindings(bindings, &request.text, verbs)
                }
            }
            Ste100Result::NotSte100 { detail, spans, .. } => {
                if detail.is_empty() {
                    return Err(ValidationError("language defect requires detail"));
                }
                for span in spans {
                    let start = usize::try_from(span.start.0)
                        .map_err(|_| ValidationError("defect span overflow"))?;
                    let end = usize::try_from(span.end.0)
                        .map_err(|_| ValidationError("defect span overflow"))?;
                    if start >= end || request.text.get(start..end).is_none() {
                        return Err(ValidationError("invalid defect byte span"));
                    }
                }
                Ok(())
            }
            Ste100Result::ComponentFailed { detail, .. } => {
                if detail.is_empty() {
                    Err(ValidationError("component failure requires detail"))
                } else {
                    Ok(())
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ste100Bootstrap {
    pub protocol: String,
    pub scheme_text: String,
    pub instruction: Ste100Instruction,
}

impl Ste100Bootstrap {
    pub fn validate(&self, launch_max_wire_bytes: DecimalU64) -> Result<(), ValidationError> {
        self.instruction.validate()?;
        if self.protocol != STE100_PROTOCOL
            || self.scheme_text.is_empty()
            || self.instruction.max_wire_bytes != launch_max_wire_bytes
        {
            return Err(ValidationError("invalid STE100 bootstrap or launch limit"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ste100Ready {
    pub protocol: String,
    pub scheme_id: String,
    pub artifact_sha256: Digest,
    pub checker_artifact_sha256: Digest,
    pub instances: u32,
    pub max_wire_bytes: DecimalU64,
}

impl Ste100Ready {
    pub fn validate_for(
        &self,
        scheme: &SchemeDeclaration,
        instruction: &Ste100Instruction,
    ) -> Result<(), ValidationError> {
        scheme.validate()?;
        instruction.validate()?;
        if self.protocol != STE100_PROTOCOL
            || self.scheme_id != scheme.scheme_id
            || self.artifact_sha256 != scheme.artifact_sha256
            || self.checker_artifact_sha256 != instruction.model_artifact_sha256
            || self.instances != instruction.instances
            || self.max_wire_bytes != instruction.max_wire_bytes
        {
            return Err(ValidationError("STE100 admission identity mismatch"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ste100AdmissionRefused {
    pub protocol: String,
    pub code: Ste100AdmissionCode,
    pub detail: String,
}

impl Ste100AdmissionRefused {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.protocol != STE100_PROTOCOL || self.detail.is_empty() {
            return Err(ValidationError("invalid STE100 admission refusal"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ste100AdmissionCode {
    SchemeMismatch,
    UnsupportedArtifact,
    InvalidGrammar,
    InsufficientCapacity,
    InvalidInstruction,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateSpec {
    pub template_id: String,
    pub kind: String,
    pub parts: Vec<TemplatePart>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum TemplatePart {
    Literal(String),
    Slot { name: String, class: SlotClass },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlotClass {
    TechnicalName,
    StepId,
}

impl TemplateSpec {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.template_id.is_empty() || self.kind.is_empty() || self.parts.is_empty() {
            return Err(ValidationError("empty template identity or body"));
        }
        if self
            .parts
            .iter()
            .any(|part| matches!(part, TemplatePart::Slot { name, .. } if name.is_empty()))
        {
            return Err(ValidationError("empty template slot name"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum LoopInstructionMeta {
    Restatement {
        source: SourceBlockId,
        attempt: DecimalU64,
    },
    Retry {
        retry_of: CallId,
        attempt: DecimalU64,
    },
    PlanProposal {
        plan: PlanRef,
        authorization: SourceBlockId,
    },
    Step {
        plan: PlanRef,
        step_id: StepId,
        instruction: StatementRef,
        span: SpanRef,
    },
    Review {
        setup: ReviewSetupRef,
        work: WorkRef,
    },
    Compact {
        reservation: SpanRef,
        through: SourcePosition,
    },
    WindDown {},
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputRefusalCase {
    NonSte100,
    InvalidInputShape,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolRepairCase {
    CorrectionCall,
    MissingToolStatement,
    AmbiguousToolStatement,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstructionRefusal {
    InvalidTemplate,
    InvalidContext,
    StaleSource,
    WrongTurn,
}

/// An artifact describes the language instance. Parsing it does not qualify that language.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ste100Scheme {
    pub format: String,
    pub reference: SchemeReference,
    pub grammar: GrammarBlob,
    pub verbs: Vec<VerbDefinition>,
    pub technical_names: Vec<TechnicalNameDefinition>,
    pub conditions: GrammarBlob,
    pub recall_keys: Vec<String>,
    pub qualification: SchemeQualification,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SchemeReference {
    pub edition: String,
    pub document_sha256: Digest,
    pub rule_ids: Vec<String>,
    pub dictionary_sha256: Digest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GrammarBlob {
    pub format: String,
    pub bytes_base64: String,
    pub sha256: Digest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerbDefinition {
    pub key: String,
    pub forms: Vec<String>,
    pub sense: String,
    pub authority: RuleReference,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TechnicalNameDefinition {
    pub text: String,
    pub authority: RuleReference,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleReference {
    pub rule_id: String,
    pub disposition: RuleDisposition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleDisposition {
    ApprovedWord,
    TechnicalUse,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SchemeQualification {
    pub report_sha256: Digest,
    pub corpus_sha256: Digest,
    pub checker_abi: String,
}

impl Ste100Scheme {
    /// Caller supplies the digest of the exact received bytes, not a reserialization.
    pub fn validate(
        &self,
        actual_digest: &Digest,
        declared: &SchemeDeclaration,
    ) -> Result<(), ValidationError> {
        declared.validate()?;
        if self.format != "ste100-scheme/1"
            || actual_digest != &declared.artifact_sha256
            || self.reference.edition.is_empty()
            || self.qualification.checker_abi.is_empty()
        {
            return Err(ValidationError("invalid scheme identity or reference"));
        }
        nonempty_unique(&self.reference.rule_ids, "invalid scheme rules")?;
        nonempty_unique(&self.recall_keys, "invalid recall keys")?;
        let keys: Vec<_> = self.verbs.iter().map(|verb| verb.key.clone()).collect();
        nonempty_unique(&keys, "invalid scheme verb keys")?;
        if keys != declared.verbs || self.recall_keys.iter().any(|key| !keys.contains(key)) {
            return Err(ValidationError("scheme dictionary or recall keys mismatch"));
        }
        let mut forms = BTreeSet::new();
        for verb in &self.verbs {
            nonempty_unique(&verb.forms, "invalid verb forms")?;
            if verb.sense.is_empty()
                || verb.authority.rule_id.is_empty()
                || verb.forms.iter().any(|form| !forms.insert(form))
            {
                return Err(ValidationError("ambiguous verb forms or missing authority"));
            }
        }
        let mut names = BTreeSet::new();
        for name in &self.technical_names {
            if name.text.is_empty()
                || name.authority.rule_id.is_empty()
                || !names.insert(&name.text)
            {
                return Err(ValidationError("invalid technical name"));
            }
        }
        self.grammar.decode()?;
        self.conditions.decode()?;
        Ok(())
    }

    /// Cryptographic implementation is supplied by the artifact owner, not another floor dependency.
    pub fn verify_blobs(&self, hash: impl Fn(&[u8]) -> Digest) -> Result<(), ValidationError> {
        for blob in [&self.grammar, &self.conditions] {
            if hash(&blob.decode()?) != blob.sha256 {
                return Err(ValidationError("grammar blob digest mismatch"));
            }
        }
        Ok(())
    }
}

impl GrammarBlob {
    /// Decode canonical padded RFC 4648 base64, rejecting nonzero unused bits.
    pub fn decode(&self) -> Result<Vec<u8>, ValidationError> {
        let bytes = self.bytes_base64.as_bytes();
        if self.format.is_empty() || bytes.is_empty() || !bytes.len().is_multiple_of(4) {
            return Err(ValidationError("invalid grammar blob encoding"));
        }
        let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
        for (index, group) in bytes.chunks_exact(4).enumerate() {
            let last = index + 1 == bytes.len() / 4;
            let a = base64_digit(group[0])?;
            let b = base64_digit(group[1])?;
            out.push(a << 2 | b >> 4);
            if group[2] == b'=' {
                if !last || group[3] != b'=' || b & 15 != 0 {
                    return Err(ValidationError("noncanonical base64 padding"));
                }
                continue;
            }
            let c = base64_digit(group[2])?;
            out.push(b << 4 | c >> 2);
            if group[3] == b'=' {
                if !last || c & 3 != 0 {
                    return Err(ValidationError("noncanonical base64 padding"));
                }
                continue;
            }
            out.push(c << 6 | base64_digit(group[3])?);
        }
        Ok(out)
    }
}

fn base64_digit(byte: u8) -> Result<u8, ValidationError> {
    match byte {
        b'A'..=b'Z' => Ok(byte - b'A'),
        b'a'..=b'z' => Ok(byte - b'a' + 26),
        b'0'..=b'9' => Ok(byte - b'0' + 52),
        b'+' => Ok(62),
        b'/' => Ok(63),
        _ => Err(ValidationError("invalid base64 byte")),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ste100ModelManifest {
    pub format: String,
    pub checker_abi: String,
    pub encoder: EncoderArtifact,
    pub tokenizer: ArtifactFile,
    pub span_head: ArtifactFile,
    pub alignment: String,
    pub max_sentence_tokens: u32,
    pub labels: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EncoderArtifact {
    pub family: String,
    pub config: ArtifactFile,
    pub weights: Vec<ArtifactFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactFile {
    pub name: String,
    pub sha256: Digest,
}

impl ArtifactFile {
    /// Lexical path check only. The SPU must also prevent symlink escape while opening.
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.name.contains('\0')
            || self
                .name
                .split('/')
                .any(|part| part.is_empty() || part == "." || part == "..")
        {
            return Err(ValidationError("invalid artifact relative path"));
        }
        Ok(())
    }
}

impl Ste100ModelManifest {
    /// This rejects incompatible representations, not unsupported compiled adapters.
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.format != "ste100-model/1"
            || self.checker_abi != STE100_BINDER_ABI
            || self.encoder.family.is_empty()
            || self.encoder.weights.is_empty()
            || self.alignment.is_empty()
            || self.max_sentence_tokens == 0
            || !self.labels.iter().map(String::as_str).eq(STE100_LABELS)
        {
            return Err(ValidationError("incompatible STE100 model manifest"));
        }
        self.encoder.config.validate()?;
        self.tokenizer.validate()?;
        self.span_head.validate()?;
        for weights in &self.encoder.weights {
            weights.validate()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digest() -> Digest {
        Digest::new("0".repeat(64)).unwrap()
    }

    fn instruction() -> Ste100Instruction {
        Ste100Instruction {
            model_binding: ModelBinding {
                artifact: crate::ArtifactRef("/models/checker".into()),
                devices: vec![crate::DeviceOrdinal(0)],
            },
            model_artifact_sha256: digest(),
            instances: 1,
            max_wire_bytes: DecimalU64(65536),
            measurement_ref: "fixture".into(),
            repair_cap: RepairCap {
                attempts: 2,
                measurement_ref: "fixture".into(),
            },
            decision_wait: serde_json::from_str(
                r#"{"elapsed_ns":"1","measurement_ref":"fixture"}"#,
            )
            .unwrap(),
        }
    }

    #[test]
    fn every_ready_echo_is_required() {
        let scheme = SchemeDeclaration {
            scheme_id: format!("ste100:{}", digest().as_str()),
            artifact_sha256: digest(),
            verbs: vec!["read".into()],
        };
        let instruction = instruction();
        let ready = Ste100Ready {
            protocol: STE100_PROTOCOL.into(),
            scheme_id: scheme.scheme_id.clone(),
            artifact_sha256: digest(),
            checker_artifact_sha256: digest(),
            instances: 1,
            max_wire_bytes: instruction.max_wire_bytes,
        };
        assert!(ready.validate_for(&scheme, &instruction).is_ok());
        for field in [
            "protocol",
            "scheme_id",
            "artifact_sha256",
            "checker_artifact_sha256",
            "instances",
            "max_wire_bytes",
        ] {
            let mut altered = serde_json::to_value(&ready).unwrap();
            altered[field] = match field {
                "instances" => serde_json::json!(2),
                "max_wire_bytes" => serde_json::json!("65537"),
                "artifact_sha256" | "checker_artifact_sha256" => serde_json::json!("1".repeat(64)),
                _ => serde_json::json!("wrong"),
            };
            let altered: Ste100Ready = serde_json::from_value(altered).unwrap();
            assert!(
                altered.validate_for(&scheme, &instruction).is_err(),
                "{field}"
            );
        }
        assert!(serde_json::from_str::<Ste100Ready>(r#"{"ready":true}"#).is_err());
        let bootstrap = Ste100Bootstrap {
            protocol: STE100_PROTOCOL.into(),
            scheme_text: "{}".into(),
            instruction,
        };
        assert!(bootstrap.validate(DecimalU64(65536)).is_ok());
        assert!(bootstrap.validate(DecimalU64(65535)).is_err());
    }

    #[test]
    fn accepted_binding_must_preserve_exact_utf8_source() {
        let request = Ste100Request {
            request_id: DecimalU64(1),
            target: Ste100Target::Input {
                input_id: DecimalU64(1),
                block: DecimalU64(0),
            },
            text: "Read caf\u{e9}.".into(),
            scheme_id: "test".into(),
        };
        let binding = Binding {
            statement_span: Span {
                start: DecimalU64(0),
                end: DecimalU64(11),
            },
            verb: "read".into(),
            verb_span: Span {
                start: DecimalU64(0),
                end: DecimalU64(4),
            },
            condition: None,
            condition_span: None,
            nouns: vec![super::super::NounBinding {
                value: "caf\u{e9}".into(),
                span: Span {
                    start: DecimalU64(5),
                    end: DecimalU64(10),
                },
            }],
        };
        let mut reply = Ste100Reply {
            request_id: request.request_id,
            target: request.target.clone(),
            scheme_id: request.scheme_id.clone(),
            result: Ste100Result::Accepted {
                bindings: vec![binding],
            },
        };
        assert!(reply.validate_for(&request, &["read".into()]).is_ok());
        if let Ste100Result::Accepted { bindings } = &mut reply.result {
            bindings[0].nouns[0].span.end = DecimalU64(9);
        }
        assert!(reply.validate_for(&request, &["read".into()]).is_err());
        reply.result = Ste100Result::NotSte100 {
            code: Ste100DefectCode::NonSte100,
            detail: "fixture lexical defect".into(),
            spans: vec![Span {
                start: DecimalU64(5),
                end: DecimalU64(10),
            }],
        };
        assert!(reply.validate_for(&request, &["read".into()]).is_ok());
    }

    #[test]
    fn template_target_and_binding_exception_are_correlated() {
        let template = TemplateSpec {
            template_id: "repair".into(),
            kind: "loop.instruction".into(),
            parts: vec![TemplatePart::Literal("Read item.".into())],
        };
        let request = Ste100Request {
            request_id: DecimalU64(1),
            target: Ste100Target::Template {
                template_id: "repair".into(),
            },
            text: serde_json::to_string(&template).unwrap(),
            scheme_id: "test".into(),
        };
        let mut reply = Ste100Reply {
            request_id: request.request_id,
            target: request.target.clone(),
            scheme_id: request.scheme_id.clone(),
            result: Ste100Result::Accepted { bindings: vec![] },
        };
        assert!(reply.validate_for(&request, &["read".into()]).is_ok());
        reply.request_id = DecimalU64(2);
        assert!(reply.validate_for(&request, &["read".into()]).is_err());
        let mut input = request.clone();
        input.target = Ste100Target::Input {
            input_id: DecimalU64(1),
            block: DecimalU64(0),
        };
        reply.request_id = input.request_id;
        reply.target = input.target.clone();
        assert!(reply.validate_for(&input, &["read".into()]).is_err());
    }

    #[test]
    fn all_nested_objects_and_codes_are_closed() {
        for invalid in [
            r#"{"template_id":"x","kind":"x","parts":[{"slot":{"name":"x","class":"technical_name","extra":1}}]}"#,
            r#"{"template_id":"x","kind":"x","parts":[{"slot":{"name":"x","name":"y","class":"technical_name"}}]}"#,
            r#"{"template_id":"x","kind":"x","parts":[{"slot":{"name":"x","class":"arbitrary_string"}}]}"#,
        ] {
            assert!(serde_json::from_str::<TemplateSpec>(invalid).is_err());
        }
        assert!(
            serde_json::from_str::<Ste100Result>(
                r#"{"not_ste100":{"code":"no_result","detail":"x","spans":[]}}"#
            )
            .is_err()
        );
        assert!(
            serde_json::from_str::<LoopInstructionMeta>(r#"{"wind_down":{"surprise":true}}"#)
                .is_err()
        );
    }

    #[test]
    fn canonical_base64_refuses_padding_aliases() {
        for (encoded, expected) in [("YQ==", "a"), ("YWI=", "ab"), ("YWJj", "abc")] {
            let blob = GrammarBlob {
                format: "fixture".into(),
                bytes_base64: encoded.into(),
                sha256: digest(),
            };
            assert_eq!(blob.decode().unwrap(), expected.as_bytes());
        }
        for encoded in ["YR==", "YWJ=", "YQ=", "YQ==YQ==", "Y Q=", "===="] {
            let blob = GrammarBlob {
                format: "fixture".into(),
                bytes_base64: encoded.into(),
                sha256: digest(),
            };
            assert!(blob.decode().is_err(), "{encoded}");
        }
    }

    #[test]
    fn scheme_checks_exact_dictionary_and_blob_digests() {
        let artifact = digest();
        let declared = SchemeDeclaration {
            scheme_id: format!("ste100:{}", artifact.as_str()),
            artifact_sha256: artifact.clone(),
            verbs: vec!["read".into()],
        };
        let authority = RuleReference {
            rule_id: "fixture-only".into(),
            disposition: RuleDisposition::TechnicalUse,
        };
        let blob = GrammarBlob {
            format: "fixture-only".into(),
            bytes_base64: "YQ==".into(),
            sha256: artifact.clone(),
        };
        let mut scheme = Ste100Scheme {
            format: "ste100-scheme/1".into(),
            reference: SchemeReference {
                edition: "test-only".into(),
                document_sha256: artifact.clone(),
                rule_ids: vec!["fixture-only".into()],
                dictionary_sha256: artifact.clone(),
            },
            grammar: blob.clone(),
            verbs: vec![VerbDefinition {
                key: "read".into(),
                forms: vec!["read".into()],
                sense: "fixture".into(),
                authority: authority.clone(),
            }],
            technical_names: vec![TechnicalNameDefinition {
                text: "item".into(),
                authority,
            }],
            conditions: blob,
            recall_keys: vec!["read".into()],
            qualification: SchemeQualification {
                report_sha256: artifact.clone(),
                corpus_sha256: artifact.clone(),
                checker_abi: STE100_BINDER_ABI.into(),
            },
        };
        assert!(scheme.validate(&artifact, &declared).is_ok());
        assert!(
            scheme
                .verify_blobs(|bytes| {
                    assert_eq!(bytes, b"a");
                    artifact.clone()
                })
                .is_ok()
        );
        assert!(
            scheme
                .verify_blobs(|_| Digest::new("1".repeat(64)).unwrap())
                .is_err()
        );
        scheme.verbs[0].forms.push("read".into());
        assert!(scheme.validate(&artifact, &declared).is_err());
        scheme.verbs[0].forms.pop();
        scheme.recall_keys = vec!["invented".into()];
        assert!(scheme.validate(&artifact, &declared).is_err());
    }

    #[test]
    fn model_manifest_does_not_admit_sequence_scores() {
        let file = ArtifactFile {
            name: "weights/model.safetensors".into(),
            sha256: digest(),
        };
        let mut manifest = Ste100ModelManifest {
            format: "ste100-model/1".into(),
            checker_abi: STE100_BINDER_ABI.into(),
            encoder: EncoderArtifact {
                family: "fixture".into(),
                config: file.clone(),
                weights: vec![file.clone()],
            },
            tokenizer: file.clone(),
            span_head: file,
            alignment: "fixture".into(),
            max_sentence_tokens: 10,
            labels: STE100_LABELS.map(String::from).to_vec(),
        };
        assert!(manifest.validate().is_ok());
        manifest.labels = vec!["entailment".into(), "not_entailment".into()];
        assert!(manifest.validate().is_err());
        for name in ["/root/model", "../model", "a//b", "a/./b"] {
            assert!(
                ArtifactFile {
                    name: name.into(),
                    sha256: digest()
                }
                .validate()
                .is_err()
            );
        }
    }
}
