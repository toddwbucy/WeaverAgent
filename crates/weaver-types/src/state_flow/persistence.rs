//! Seed and persistence values from TYPES-PERSIST and the foundation protocol.
//!
//! Admin owns publication and hashing. These records grant no path access.

use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

use super::{
    Binding, DecimalU64, Digest, SchemeDeclaration, SourcePosition, Ste100Scheme, ValidationError,
    validate_bindings,
};

/// The required image binding in a published operational declaration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateStartConfig {
    pub dump_sha256: Digest,
}

/// Verified start identity. The member checks image facts, admin checks configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StartIdentity {
    pub seed_id: String,
    pub artifact_id: String,
    pub dump_sha256: Digest,
    pub config_sha256: Digest,
    #[serde(deserialize_with = "super::required_option")]
    pub parent_artifact_id: Option<String>,
    #[serde(deserialize_with = "super::required_option")]
    pub position: Option<SourcePosition>,
}

/// The exact nine PROMPT-3 duties, not additional prompt requirements.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptDuty {
    Ste100Emission,
    Recall,
    ForwardFlags,
    LockedPlan,
    ReviewClosure,
    Restatement,
    Ste100InputHistory,
    InitialPlan,
    WallSummary,
}

/// Operator-provided certification of exact prompt bytes and reviewed duties.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeedCertificate {
    pub prompt_sha256: Digest,
    pub scheme_id: String,
    pub checker_artifact_sha256: Digest,
    pub review_reference: String,
    pub bindings: Vec<Binding>,
    #[serde(deserialize_with = "deserialize_duties")]
    pub duties: Vec<PromptDuty>,
}

/// The sole pre-agent seed insertion request, with no root file path.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeedRequest {
    pub schema_id: String,
    pub seed_id: String,
    pub scheme_id: String,
    pub ste100_scheme: String,
    pub checker_artifact_sha256: Digest,
    pub prompt_text: String,
    pub raw_block: Box<RawValue>,
    pub bindings: Vec<Binding>,
    pub certificate: SeedCertificate,
}

/// The private result of structural and attributional certificate verification.
///
/// This is not proof of language conformance. The reviewed certificate producer
/// owns that evidence. No wire decoder or public constructor can mint this result.
///
/// ```compile_fail,E0451
/// use weaver_types::state_flow::{Digest, VerifiedSeed};
/// let digest = Digest::new("0".repeat(64)).unwrap();
/// let forged = VerifiedSeed {
///     prompt_sha256: digest.clone(),
///     scheme_id: "unverified".into(),
///     checker_artifact_sha256: digest,
///     bindings: vec![],
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedSeed {
    prompt_sha256: Digest,
    scheme_id: String,
    checker_artifact_sha256: Digest,
    bindings: Vec<Binding>,
}

impl VerifiedSeed {
    pub fn prompt_sha256(&self) -> &Digest {
        &self.prompt_sha256
    }

    pub fn scheme_id(&self) -> &str {
        &self.scheme_id
    }

    pub fn checker_artifact_sha256(&self) -> &Digest {
        &self.checker_artifact_sha256
    }

    pub fn bindings(&self) -> &[Binding] {
        &self.bindings
    }
}

/// Check a certificate against independently supplied identities and exact text.
///
/// The caller computes `prompt_digest` from `prompt` and validates the received
/// scheme artifact against `scheme` before this call. The expected checker digest
/// comes from the authoring declaration, never the certificate. This function
/// neither hashes bytes nor runs a classifier.
pub fn verify_seed_certificate(
    prompt: &str,
    prompt_digest: &Digest,
    certificate: &SeedCertificate,
    scheme: &SchemeDeclaration,
    checker_digest: &Digest,
) -> Result<VerifiedSeed, SeedRefusal> {
    let invalid = SeedRefusal::SeedCertificateInvalid;
    scheme.validate().map_err(|_| invalid)?;
    if &certificate.prompt_sha256 != prompt_digest
        || certificate.scheme_id != scheme.scheme_id
        || &certificate.checker_artifact_sha256 != checker_digest
        || certificate.review_reference.is_empty()
        || !valid_duties(&certificate.duties)
    {
        return Err(invalid);
    }
    validate_bindings(&certificate.bindings, prompt, &scheme.verbs).map_err(|_| invalid)?;
    Ok(VerifiedSeed {
        prompt_sha256: prompt_digest.clone(),
        scheme_id: scheme.scheme_id.clone(),
        checker_artifact_sha256: checker_digest.clone(),
        bindings: certificate.bindings.clone(),
    })
}

impl SeedRequest {
    /// Validate before insertion, in the required schema, shape, certificate order.
    ///
    /// Admin and the member supply their own hash implementation. This method
    /// applies it to the exact prompt, scheme bytes and decoded grammar blobs
    /// before the dictionary is used, without issuing a semantic conformance
    /// judgment. `expected_checker` comes from the authoring declaration.
    pub fn validate_structure(
        &self,
        hash: impl Fn(&[u8]) -> Digest,
        scheme: &SchemeDeclaration,
        expected_checker: &Digest,
    ) -> Result<VerifiedSeed, SeedRefusal> {
        if self.schema_id != "state-content/1" {
            return Err(SeedRefusal::UnsupportedSchema);
        }
        if self.seed_id.len() != 32
            || !self
                .seed_id
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            || self.bindings.is_empty()
            || self.bindings != self.certificate.bindings
            || super::content::validate_text_block(&self.raw_block, &self.prompt_text).is_err()
        {
            return Err(SeedRefusal::MalformedSeed);
        }
        let invalid = SeedRefusal::SeedCertificateInvalid;
        let artifact: Ste100Scheme =
            super::decode_strict(&self.ste100_scheme).map_err(|_| invalid)?;
        artifact
            .validate(&hash(self.ste100_scheme.as_bytes()), scheme)
            .map_err(|_| invalid)?;
        artifact.verify_blobs(&hash).map_err(|_| invalid)?;
        if self.scheme_id != scheme.scheme_id || &self.checker_artifact_sha256 != expected_checker {
            return Err(invalid);
        }
        verify_seed_certificate(
            &self.prompt_text,
            &hash(self.prompt_text.as_bytes()),
            &self.certificate,
            scheme,
            expected_checker,
        )
    }
}

/// A complete member-room image, not inline dump bytes or publication success.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeedCreated {
    pub schema_id: String,
    pub seed_id: String,
    pub room_name: String,
    pub size_bytes: DecimalU64,
}

/// The closed state-seed/1 refusal vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SeedRefusal {
    UnsupportedSchema,
    MalformedSeed,
    SeedCertificateInvalid,
    WriteFailed,
    ImageTooLarge,
    SnapshotFailed,
}

/// Capture-specific operation refusals on the serving control channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureRefusalCode {
    ImageTooLarge,
    SnapshotFailed,
    UnknownCapture,
    InvalidCaptureState,
}

/// Published serving capture and the confirmation event, never an inferred save.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavePointReport {
    pub start: StartIdentity,
    pub capture_id: DecimalU64,
    pub event_run: crate::RunId,
    pub event_sequence: DecimalU64,
}

/// The phase retained in uncertain or broken publication evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PersistencePhase {
    Queued,
    Beginning,
    Barrier,
    Capturing,
    Hashing,
    Finishing,
    Preparing,
    Confirming,
    Publishing,
    Reporting,
}

/// Closed types of extant artifacts reported after failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetainedArtifactKind {
    Dump,
    Configuration,
    Binding,
    Part,
    Guard,
}

/// An admin-derived relative location with null digest when unknown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetainedArtifact {
    pub location: String,
    pub kind: RetainedArtifactKind,
    #[serde(deserialize_with = "super::required_option")]
    pub sha256: Option<Digest>,
}

/// How the published starting state was authorized.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Confirmation {
    Serving {
        run_id: String,
        capture_id: DecimalU64,
    },
    Seed {
        seed_id: String,
    },
    Rebind {
        parent_artifact_id: String,
    },
}

/// The closed artifact publication causes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PersistenceCause {
    Requested,
    GracefulUnload,
    ForcedUnload,
    Seed,
    Rebind,
}

/// A guard prevents load until a save has completed successfully.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicationState {
    Pending,
    Broken,
}

/// The lifecycle stage that attributed a persistence refusal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PersistenceStage {
    Seed,
    Bind,
    Load,
    Publish,
    Boot,
}

/// The closed persistence refusal vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PersistenceCase {
    UnsupportedLayout,
    SeedExists,
    SeedRequiresUnloaded,
    SeedCertificateInvalid,
    SeedFailed,
    BindingMismatch,
    ConfigurationMismatch,
    SchemeMismatch,
    BrokenSavePoint,
    PublicationFailed,
    BootFailed,
}

impl PersistenceCase {
    /// Reject a well-spelled refusal attributed to a stage that cannot issue it.
    pub fn permits_stage(self, stage: PersistenceStage) -> bool {
        match self {
            Self::SeedExists
            | Self::SeedRequiresUnloaded
            | Self::SeedCertificateInvalid
            | Self::SeedFailed => stage == PersistenceStage::Seed,
            Self::PublicationFailed => stage == PersistenceStage::Publish,
            Self::BootFailed => stage == PersistenceStage::Boot,
            Self::SchemeMismatch => stage == PersistenceStage::Bind,
            Self::UnsupportedLayout
            | Self::BindingMismatch
            | Self::ConfigurationMismatch
            | Self::BrokenSavePoint => {
                matches!(stage, PersistenceStage::Bind | PersistenceStage::Load)
            }
        }
    }
}

impl PromptDuty {
    /// Every seed certificate names all nine duties exactly once.
    pub const ALL: [Self; 9] = [
        Self::Ste100Emission,
        Self::Recall,
        Self::ForwardFlags,
        Self::LockedPlan,
        Self::ReviewClosure,
        Self::Restatement,
        Self::Ste100InputHistory,
        Self::InitialPlan,
        Self::WallSummary,
    ];
}

fn deserialize_duties<'de, D>(deserializer: D) -> Result<Vec<PromptDuty>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let duties = Vec::<PromptDuty>::deserialize(deserializer)?;
    if !valid_duties(&duties) {
        return Err(serde::de::Error::custom(
            "every prompt duty must occur exactly once",
        ));
    }
    Ok(duties)
}

fn valid_duties(duties: &[PromptDuty]) -> bool {
    duties.len() == PromptDuty::ALL.len()
        && PromptDuty::ALL
            .iter()
            .all(|duty| duties.iter().filter(|d| *d == duty).count() == 1)
}

/// Absolute CLOCK_MONOTONIC bounds. Expiry is checked by the receiving actor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SaveWindow {
    pub began_ns: DecimalU64,
    pub deadline_ns: DecimalU64,
}

impl SaveWindow {
    /// A malformed interval is distinct from a valid interval that has expired.
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.began_ns.0 >= self.deadline_ns.0 {
            return Err(ValidationError(
                "save window requires began_ns < deadline_ns",
            ));
        }
        Ok(())
    }
}

impl<'de> Deserialize<'de> for SaveWindow {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            began_ns: DecimalU64,
            deadline_ns: DecimalU64,
        }
        let Fields {
            began_ns,
            deadline_ns,
        } = Fields::deserialize(deserializer)?;
        let window = Self {
            began_ns,
            deadline_ns,
        };
        window.validate().map_err(serde::de::Error::custom)?;
        Ok(window)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digest(byte: char) -> Digest {
        Digest::new(byte.to_string().repeat(64)).unwrap()
    }

    fn seed_fixture() -> (SeedRequest, SchemeDeclaration) {
        let scheme = SchemeDeclaration {
            scheme_id: format!("ste100:{}", digest('4').as_str()),
            artifact_sha256: digest('4'),
            verbs: vec!["read".into()],
        };
        let bindings = vec![Binding {
            statement_span: super::super::Span {
                start: 0.into(),
                end: 10.into(),
            },
            verb: "read".into(),
            verb_span: super::super::Span {
                start: 0.into(),
                end: 4.into(),
            },
            condition: None,
            condition_span: None,
            nouns: vec![super::super::NounBinding {
                value: "item".into(),
                span: super::super::Span {
                    start: 5.into(),
                    end: 9.into(),
                },
            }],
        }];
        let scheme_text = serde_json::json!({
            "format": "ste100-scheme/1",
            "reference": {
                "edition": "fixture",
                "document_sha256": digest('0'),
                "rule_ids": ["fixture-rule"],
                "dictionary_sha256": digest('0')
            },
            "grammar": {
                "format": "fixture-grammar/1",
                "bytes_base64": "Z3JhbW1hcg==",
                "sha256": digest('2')
            },
            "verbs": [{
                "key": "read", "forms": ["Read"], "sense": "fixture",
                "authority": {"rule_id": "fixture-rule", "disposition": "approved_word"}
            }],
            "technical_names": [],
            "conditions": {
                "format": "fixture-conditions/1",
                "bytes_base64": "Y29uZGl0aW9ucw==",
                "sha256": digest('3')
            },
            "recall_keys": ["read"],
            "qualification": {
                "report_sha256": digest('0'),
                "corpus_sha256": digest('0'),
                "checker_abi": "fixture/1"
            }
        })
        .to_string();
        let certificate = SeedCertificate {
            prompt_sha256: digest('1'),
            scheme_id: scheme.scheme_id.clone(),
            checker_artifact_sha256: digest('5'),
            review_reference: "fixture reviewed duties".into(),
            bindings: bindings.clone(),
            duties: PromptDuty::ALL.to_vec(),
        };
        (
            SeedRequest {
                schema_id: "state-content/1".into(),
                seed_id: "a".repeat(32),
                scheme_id: scheme.scheme_id.clone(),
                ste100_scheme: scheme_text,
                checker_artifact_sha256: digest('5'),
                prompt_text: "Read item.".into(),
                raw_block: RawValue::from_string(r#"{"type":"text","text":"Read item."}"#.into())
                    .unwrap(),
                bindings,
                certificate,
            },
            scheme,
        )
    }

    // A deterministic test oracle, not a cryptographic implementation. It lets
    // substitutions fail while the production caller owns the hash implementation.
    fn fixture_hash(bytes: &[u8], scheme_text: &str) -> Digest {
        if bytes == scheme_text.as_bytes() {
            digest('4')
        } else {
            match bytes {
                b"Read item." => digest('1'),
                b"grammar" => digest('2'),
                b"conditions" => digest('3'),
                _ => digest('9'),
            }
        }
    }

    #[test]
    fn seed_verification_checks_independent_identity_and_every_binding() {
        let (seed, scheme) = seed_fixture();
        let verified = seed
            .validate_structure(
                |bytes| fixture_hash(bytes, &seed.ste100_scheme),
                &scheme,
                &digest('5'),
            )
            .unwrap();
        assert_eq!(verified.prompt_sha256(), &digest('1'));
        assert_eq!(verified.scheme_id(), scheme.scheme_id);
        assert_eq!(verified.checker_artifact_sha256(), &digest('5'));
        assert_eq!(verified.bindings(), seed.bindings);

        for field in [
            "prompt", "scheme", "checker", "review", "duties", "noun", "verb",
        ] {
            let mut certificate = seed.certificate.clone();
            match field {
                "prompt" => certificate.prompt_sha256 = digest('8'),
                "scheme" => certificate.scheme_id.push('x'),
                "checker" => certificate.checker_artifact_sha256 = digest('8'),
                "review" => certificate.review_reference.clear(),
                "duties" => certificate.duties[0] = PromptDuty::Recall,
                "noun" => certificate.bindings[0].nouns[0].value = "other".into(),
                "verb" => certificate.bindings[0].verb = "write".into(),
                _ => unreachable!(),
            }
            assert_eq!(
                verify_seed_certificate(
                    &seed.prompt_text,
                    &digest('1'),
                    &certificate,
                    &scheme,
                    &digest('5'),
                ),
                Err(SeedRefusal::SeedCertificateInvalid),
                "{field}"
            );
        }
        assert_eq!(
            seed.validate_structure(
                |bytes| fixture_hash(bytes, &seed.ste100_scheme),
                &scheme,
                &digest('8'),
            ),
            Err(SeedRefusal::SeedCertificateInvalid)
        );
    }

    #[test]
    fn seed_refusal_order_precedes_certificate_checks() {
        let (seed, scheme) = seed_fixture();
        let check = |value: &SeedRequest| {
            value.validate_structure(
                |bytes| fixture_hash(bytes, &seed.ste100_scheme),
                &scheme,
                &digest('5'),
            )
        };
        let mut invalid = seed.clone();
        invalid.schema_id = "other".into();
        invalid.seed_id.clear();
        invalid.certificate.review_reference.clear();
        assert_eq!(check(&invalid), Err(SeedRefusal::UnsupportedSchema));
        invalid.schema_id = seed.schema_id.clone();
        assert_eq!(check(&invalid), Err(SeedRefusal::MalformedSeed));
        invalid.seed_id = seed.seed_id.clone();
        assert_eq!(check(&invalid), Err(SeedRefusal::SeedCertificateInvalid));
        for raw in [
            r#"{"type":"text","text":"Changed item."}"#,
            r#"{"type":"text","text":"Read item.","extra":0}"#,
            r#"{"type":"text","type":"text","text":"Read item."}"#,
            r#"{"type":"tool_use","id":"x","name":"x","input":{}}"#,
        ] {
            invalid = seed.clone();
            invalid.raw_block = RawValue::from_string(raw.into()).unwrap();
            assert_eq!(check(&invalid), Err(SeedRefusal::MalformedSeed), "{raw}");
        }
        invalid = seed.clone();
        invalid.bindings.clear();
        assert_eq!(check(&invalid), Err(SeedRefusal::MalformedSeed));
        invalid = seed.clone();
        invalid.seed_id = "A".repeat(32);
        assert_eq!(check(&invalid), Err(SeedRefusal::MalformedSeed));
    }

    #[test]
    fn seed_scheme_bytes_and_nested_blob_hashes_are_checked() {
        let (seed, scheme) = seed_fixture();
        let mut altered = seed.clone();
        altered.ste100_scheme.push(' ');
        assert_eq!(
            altered.validate_structure(
                |bytes| fixture_hash(bytes, &seed.ste100_scheme),
                &scheme,
                &digest('5'),
            ),
            Err(SeedRefusal::SeedCertificateInvalid)
        );
        altered = seed.clone();
        altered.ste100_scheme = altered.ste100_scheme.replacen(
            "\"format\":\"ste100-scheme/1\"",
            "\"format\":\"ste100-scheme/1\",\"format\":\"ste100-scheme/1\"",
            1,
        );
        assert_eq!(
            altered.validate_structure(
                |bytes| fixture_hash(bytes, &altered.ste100_scheme),
                &scheme,
                &digest('5'),
            ),
            Err(SeedRefusal::SeedCertificateInvalid)
        );
        altered = seed.clone();
        altered.ste100_scheme = altered
            .ste100_scheme
            .replace(digest('2').as_str(), digest('8').as_str());
        assert_eq!(
            altered.validate_structure(
                |bytes| fixture_hash(bytes, &altered.ste100_scheme),
                &scheme,
                &digest('5'),
            ),
            Err(SeedRefusal::SeedCertificateInvalid)
        );
    }

    #[test]
    fn seed_certificate_requires_all_duties_once_and_no_unknown_field() {
        let certificate = SeedCertificate {
            prompt_sha256: Digest::new("0".repeat(64)).unwrap(),
            scheme_id: "fixture-read/1".into(),
            checker_artifact_sha256: Digest::new("1".repeat(64)).unwrap(),
            review_reference: "review fixture".into(),
            bindings: vec![],
            duties: PromptDuty::ALL.to_vec(),
        };
        let valid = serde_json::to_string(&certificate).unwrap();
        assert!(serde_json::from_str::<SeedCertificate>(&valid).is_ok());
        assert!(
            serde_json::from_str::<SeedCertificate>(
                &valid.replace("\"wall_summary\"", "\"recall\"")
            )
            .is_err()
        );
        assert!(
            serde_json::from_str::<SeedCertificate>(&valid.replace(",\"wall_summary\"", ""))
                .is_err()
        );
        let extra = format!("{{\"signature\":null,{}", &valid[1..]);
        assert!(serde_json::from_str::<SeedCertificate>(&extra).is_err());
    }

    #[test]
    fn save_window_refuses_empty_reversed_and_numeric_intervals() {
        assert!(
            serde_json::from_str::<SaveWindow>(r#"{"began_ns":"10","deadline_ns":"11"}"#).is_ok()
        );
        for json in [
            r#"{"began_ns":"10","deadline_ns":"10"}"#,
            r#"{"began_ns":"11","deadline_ns":"10"}"#,
            r#"{"began_ns":10,"deadline_ns":"11"}"#,
            r#"{"began_ns":"10","deadline_ns":"11","duration":"1"}"#,
        ] {
            assert!(serde_json::from_str::<SaveWindow>(json).is_err(), "{json}");
        }
    }

    #[test]
    fn persistence_refusals_cannot_change_stage_meaning() {
        assert!(PersistenceCase::SchemeMismatch.permits_stage(PersistenceStage::Bind));
        assert!(!PersistenceCase::SchemeMismatch.permits_stage(PersistenceStage::Seed));
        assert!(PersistenceCase::BrokenSavePoint.permits_stage(PersistenceStage::Load));
        assert!(!PersistenceCase::BrokenSavePoint.permits_stage(PersistenceStage::Publish));
    }
}
