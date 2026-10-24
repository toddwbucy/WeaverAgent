//! Real declared files and complete floor values for mechanical duty tests only.
//! These bytes never qualify a grammar or a span model.
use sha2::{Digest as _, Sha256};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use weaver_types::state_flow::{
    ArtifactFile, DecimalU64, Digest, EncoderArtifact, GrammarBlob, MeasuredWait, RepairCap,
    RuleDisposition, RuleReference, STE100_BINDER_ABI, STE100_LABELS, STE100_PROTOCOL,
    SchemeQualification, SchemeReference, Ste100Bootstrap, Ste100Instruction, Ste100ModelManifest,
    Ste100Scheme, TechnicalNameDefinition, VerbDefinition,
};
use weaver_types::{ArtifactRef, DeviceOrdinal, ModelBinding};

static NEXT: AtomicU64 = AtomicU64::new(0);
pub const LIMIT: usize = 65_536;
pub const INPUTS: [(&str, &[u8]); 5] = [
    ("config.json", b"original runtime config"),
    ("weights/first", b"original first runtime weight"),
    ("weights/second", b"original second runtime weight"),
    ("tokenizer.json", b"original runtime tokenizer"),
    ("head", b"original seven-column runtime head"),
];
pub fn sha(bytes: &[u8]) -> Digest {
    Digest::new(format!("{:x}", Sha256::digest(bytes))).unwrap()
}

pub struct Fixture {
    pub root: PathBuf,
    pub bootstrap: Ste100Bootstrap,
}
impl Fixture {
    pub fn new(fixture: bool) -> Self {
        let root = std::env::temp_dir().join(format!(
            "weaver-ste100-runtime-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("weights")).unwrap();
        for (name, bytes) in INPUTS {
            std::fs::write(root.join(name), bytes).unwrap();
        }
        let format = if fixture {
            "fixture-read/1"
        } else {
            "unqualified-test-candidate/1"
        };
        let authority = RuleReference {
            rule_id: "test-rule".into(),
            disposition: RuleDisposition::TechnicalUse,
        };
        let scheme = Ste100Scheme {
            format: "ste100-scheme/1".into(),
            reference: SchemeReference {
                edition: format.into(),
                document_sha256: sha(b"unqualified test document"),
                rule_ids: vec!["test-rule".into()],
                dictionary_sha256: sha(b"unqualified test dictionary"),
            },
            grammar: GrammarBlob {
                format: format.into(),
                bytes_base64: "YQ==".into(),
                sha256: sha(b"a"),
            },
            verbs: vec![VerbDefinition {
                key: "read".into(),
                forms: vec!["Read".into(), "read".into()],
                sense: "test-only".into(),
                authority: authority.clone(),
            }],
            technical_names: vec![TechnicalNameDefinition {
                text: "item".into(),
                authority,
            }],
            conditions: GrammarBlob {
                format: format.into(),
                bytes_base64: "Yg==".into(),
                sha256: sha(b"b"),
            },
            recall_keys: vec!["read".into()],
            qualification: SchemeQualification {
                report_sha256: sha(b"unqualified test report"),
                corpus_sha256: sha(b"unqualified test corpus"),
                checker_abi: STE100_BINDER_ABI.into(),
            },
        };
        let reference = |index: usize| ArtifactFile {
            name: INPUTS[index].0.into(),
            sha256: sha(INPUTS[index].1),
        };
        let manifest = Ste100ModelManifest {
            format: "ste100-model/1".into(),
            checker_abi: STE100_BINDER_ABI.into(),
            encoder: EncoderArtifact {
                family: format.into(),
                config: reference(0),
                weights: vec![reference(1), reference(2)],
            },
            tokenizer: reference(3),
            span_head: reference(4),
            alignment: format.into(),
            max_sentence_tokens: 32,
            labels: STE100_LABELS.into_iter().map(str::to_owned).collect(),
        };
        let manifest = serde_json::to_vec(&manifest).unwrap();
        let path = root.join("declared-manifest.json");
        std::fs::write(&path, &manifest).unwrap();
        Self {
            root,
            bootstrap: Ste100Bootstrap {
                protocol: STE100_PROTOCOL.into(),
                scheme_text: serde_json::to_string(&scheme).unwrap(),
                instruction: Ste100Instruction {
                    model_binding: ModelBinding {
                        artifact: ArtifactRef(path.to_str().unwrap().into()),
                        devices: vec![DeviceOrdinal(0)],
                    },
                    model_artifact_sha256: sha(&manifest),
                    instances: 2,
                    max_wire_bytes: DecimalU64(LIMIT as u64),
                    measurement_ref: "unqualified-runtime-fixture".into(),
                    repair_cap: RepairCap {
                        attempts: 2,
                        measurement_ref: "unqualified-runtime-fixture".into(),
                    },
                    decision_wait: MeasuredWait {
                        elapsed_ns: DecimalU64(1),
                        measurement_ref: "unqualified-runtime-fixture".into(),
                    },
                },
            },
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
