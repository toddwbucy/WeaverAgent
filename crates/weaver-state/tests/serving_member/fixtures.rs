//! Synthetic byte-contract fixtures. No artifact here is language qualification.

use sha2::{Digest as _, Sha256};
use weaver_types::state_flow::*;

pub fn digest(bytes: &[u8]) -> Digest {
    Digest::new(format!("{:x}", Sha256::digest(bytes))).unwrap()
}

pub fn seed() -> SeedRequest {
    let artifact = serde_json::json!({
        "format":"ste100-scheme/1",
        "reference":{"edition":"synthetic-process-test",
            "document_sha256":digest(b"reference"),"rule_ids":["test-rule"],
            "dictionary_sha256":digest(b"dictionary")},
        "grammar":{"format":"test-grammar/1","bytes_base64":"Z3JhbW1hcg==",
            "sha256":digest(b"grammar")},
        "verbs":[{"key":"read","forms":["Read"],"sense":"test",
            "authority":{"rule_id":"test-rule","disposition":"approved_word"}}],
        "technical_names":[],
        "conditions":{"format":"test-conditions/1","bytes_base64":"Y29uZGl0aW9ucw==",
            "sha256":digest(b"conditions")},
        "recall_keys":["read"],
        "qualification":{"report_sha256":digest(b"test-only report"),
            "corpus_sha256":digest(b"test-only corpus"),"checker_abi":"test/1"}
    })
    .to_string();
    let scheme_id = format!("ste100:{}", digest(artifact.as_bytes()));
    let text = "Read item.";
    let bindings = vec![binding(text)];
    SeedRequest {
        schema_id: STATE_CONTENT_SCHEMA.into(),
        seed_id: "a".repeat(32),
        scheme_id: scheme_id.clone(),
        ste100_scheme: artifact,
        checker_artifact_sha256: digest(b"synthetic checker"),
        prompt_text: text.into(),
        raw_block: raw_text(text),
        bindings: bindings.clone(),
        certificate: SeedCertificate {
            prompt_sha256: digest(text.as_bytes()),
            scheme_id,
            checker_artifact_sha256: digest(b"synthetic checker"),
            review_reference: "synthetic process fixture, not production qualification".into(),
            bindings,
            duties: PromptDuty::ALL.to_vec(),
        },
    }
}

pub fn raw_text(text: &str) -> Box<serde_json::value::RawValue> {
    serde_json::value::RawValue::from_string(
        serde_json::json!({"type":"text","text":text}).to_string(),
    )
    .unwrap()
}

pub fn binding(text: &str) -> Binding {
    Binding {
        statement_span: Span {
            start: 0.into(),
            end: (text.len() as u64).into(),
        },
        verb: "read".into(),
        verb_span: Span {
            start: 0.into(),
            end: 4.into(),
        },
        condition: None,
        condition_span: None,
        nouns: vec![NounBinding {
            value: text[5..text.len() - 1].into(),
            span: Span {
                start: 5.into(),
                end: (text.len() as u64 - 1).into(),
            },
        }],
    }
}

pub fn common(request_id: u64) -> Common {
    Common {
        session_id: "fixture-session".into(),
        run_id: "fixture-run".into(),
        request_id: request_id.into(),
    }
}

pub fn start(image: &[u8], seed: &SeedRequest) -> StartIdentity {
    StartIdentity {
        seed_id: seed.seed_id.clone(),
        artifact_id: "synthetic-seed".into(),
        dump_sha256: digest(image),
        config_sha256: digest(b"admin owns config"),
        parent_artifact_id: None,
        position: None,
    }
}

pub fn limits(max_result_bytes: u64) -> ReadLimits {
    ReadLimits {
        max_result_bytes: max_result_bytes.into(),
        measurement_ref: "synthetic byte limit, not measured production capacity".into(),
    }
}

pub fn statement(seed: &SeedRequest, sequence: u64, text: &str) -> StatementFile {
    StatementFile {
        block_id: SourceBlockId {
            run: "fixture-run".into(),
            sequence: sequence.into(),
            block: 0.into(),
        },
        source: SourceEnvelope {
            session: "fixture-session".into(),
            run: "fixture-run".into(),
            turn: Some("fixture-turn".into()),
            sequence: sequence.into(),
            kind: "message.assistant".into(),
            channel: "output".into(),
            origin: "model".into(),
            subsystem: "harness".into(),
            causal_parent: None,
            wall_ms: 0,
            monotonic_ns: sequence.into(),
        },
        partition_rule_id: "statements".into(),
        source_path: vec![
            SourcePathPart::Key("content".into()),
            SourcePathPart::Index { index: 0.into() },
        ],
        admission: Admission::Admitted,
        raw_block: raw_text(text),
        text: text.into(),
        scheme_id: seed.scheme_id.clone(),
        bindings: vec![binding(text)],
        context: None,
        effect: None,
    }
}

pub fn opening(statement: &StatementFile) -> PairStarted {
    PairStarted {
        call_id: CallId {
            run: statement.block_id.run.clone(),
            sequence: statement.block_id.sequence,
            call: 0.into(),
        },
        call_block: 1.into(),
        statement: StatementRef {
            block_id: statement.block_id.clone(),
            binding: 0.into(),
        },
        lane: PairLane::External,
        request: PairRequest::External {
            name: "sh".into(),
            arguments: "echo fixture".into(),
        },
        retry_of: None,
    }
}

pub fn pair(seed: &SeedRequest, sequence: u64, record: PairRecord) -> PairFile {
    let mut base = statement(seed, sequence, "Read item.");
    base.source.channel = "tool".into();
    let bytes = match &record {
        PairRecord::Started(value) => {
            base.source.kind = "tool.call.started".into();
            serde_json::to_string(value).unwrap()
        }
        PairRecord::Ended(value) => {
            base.source.kind = "tool.call.completed".into();
            base.source.origin = "tool".into();
            serde_json::to_string(value).unwrap()
        }
    };
    PairFile {
        block_id: base.block_id,
        source: base.source,
        partition_rule_id: "pairs".into(),
        source_path: vec![],
        admission: Admission::Admitted,
        raw_block: serde_json::value::RawValue::from_string(bytes).unwrap(),
        scheme_id: seed.scheme_id.clone(),
        pair: record,
    }
}
