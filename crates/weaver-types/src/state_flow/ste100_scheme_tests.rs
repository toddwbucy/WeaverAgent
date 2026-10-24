//! Scheme objects remain named maps through every public serde entry point.
//! Synthetic values exercise representation, not language qualification.

use super::*;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::fmt::Debug;

fn scheme() -> Value {
    let digest = "0".repeat(64);
    let authority = json!({"rule_id":"fixture-only", "disposition":"technical_use"});
    let blob = json!({"format":"fixture-only", "bytes_base64":"YQ==", "sha256":digest});
    json!({
        "format":"ste100-scheme/1",
        "reference":{"edition":"fixture-only", "document_sha256":digest,
            "rule_ids":["fixture-only"], "dictionary_sha256":digest},
        "grammar":blob,
        "verbs":[{"key":"read", "forms":["Read","read"], "sense":"fixture-only",
            "authority":authority}],
        "technical_names":[{"text":"item", "authority":authority}],
        "conditions":blob,
        "recall_keys":["read"],
        "qualification":{"report_sha256":digest, "corpus_sha256":digest,
            "checker_abi":STE100_BINDER_ABI}
    })
}

fn assert_valid<T>(value: &Value)
where
    T: DeserializeOwned + Serialize + PartialEq + Debug,
{
    let text = serde_json::to_string(value).unwrap();
    let expected: T = serde_json::from_str(&text).unwrap();
    assert_eq!(serde_json::to_value(&expected).unwrap(), *value);
    assert_eq!(
        serde_json::from_slice::<T>(text.as_bytes()).unwrap(),
        expected
    );
    assert_eq!(
        serde_json::from_reader::<_, T>(text.as_bytes()).unwrap(),
        expected
    );
    assert_eq!(
        serde_json::from_value::<T>(value.clone()).unwrap(),
        expected
    );
    assert_eq!(T::deserialize(value.clone()).unwrap(), expected);
    let mut deserializer = serde_json::Deserializer::from_str(&text);
    assert_eq!(T::deserialize(&mut deserializer).unwrap(), expected);
    deserializer.end().unwrap();
    assert_eq!(super::super::decode_strict::<T>(&text).unwrap(), expected);
}

fn assert_bad_json<T: DeserializeOwned>(text: &str) {
    assert!(
        serde_json::from_str::<T>(text).is_err(),
        "from_str accepted {text}"
    );
    assert!(
        serde_json::from_slice::<T>(text.as_bytes()).is_err(),
        "from_slice accepted {text}"
    );
    assert!(
        serde_json::from_reader::<_, T>(text.as_bytes()).is_err(),
        "from_reader accepted {text}"
    );
    let mut deserializer = serde_json::Deserializer::from_str(text);
    assert!(
        T::deserialize(&mut deserializer).is_err(),
        "Deserialize trait accepted {text}"
    );
    assert!(
        super::super::decode_strict::<T>(text).is_err(),
        "strict decode accepted {text}"
    );
}

fn assert_bad_value<T: DeserializeOwned>(value: &Value) {
    assert_bad_json::<T>(&serde_json::to_string(value).unwrap());
    assert!(
        serde_json::from_value::<T>(value.clone()).is_err(),
        "from_value accepted {value}"
    );
    assert!(
        T::deserialize(value.clone()).is_err(),
        "Value deserializer accepted {value}"
    );
}

fn sequence(object: &Value, fields: &[&str]) -> Value {
    Value::Array(fields.iter().map(|field| object[*field].clone()).collect())
}

fn assert_object<T>(object: Value, fields: &[&str])
where
    T: DeserializeOwned + Serialize + PartialEq + Debug,
{
    assert_valid::<T>(&object);
    assert_bad_value::<T>(&sequence(&object, fields));
    for scalar in [
        Value::Null,
        json!(true),
        json!(1),
        json!("object"),
        json!([]),
    ] {
        assert_bad_value::<T>(&scalar);
    }
    let mut unknown = object.clone();
    unknown
        .as_object_mut()
        .unwrap()
        .insert("unexpected".into(), json!(0));
    assert_bad_value::<T>(&unknown);
    let encoded = serde_json::to_string(&object).unwrap();
    for field in fields {
        let mut missing = object.clone();
        assert!(missing.as_object_mut().unwrap().remove(*field).is_some());
        assert_bad_value::<T>(&missing);
        let mut null = object.clone();
        null[*field] = Value::Null;
        assert_bad_value::<T>(&null);
        // Build the duplicate in text; a Value would erase the very defect.
        let duplicate = format!(
            "{{{}:{},{}",
            serde_json::to_string(field).unwrap(),
            serde_json::to_string(&object[*field]).unwrap(),
            &encoded[1..]
        );
        assert_bad_json::<T>(&duplicate);
    }
}

const SCHEME_FIELDS: &[&str] = &[
    "format",
    "reference",
    "grammar",
    "verbs",
    "technical_names",
    "conditions",
    "recall_keys",
    "qualification",
];
const REFERENCE_FIELDS: &[&str] = &[
    "edition",
    "document_sha256",
    "rule_ids",
    "dictionary_sha256",
];
const BLOB_FIELDS: &[&str] = &["format", "bytes_base64", "sha256"];
const VERB_FIELDS: &[&str] = &["key", "forms", "sense", "authority"];
const NAME_FIELDS: &[&str] = &["text", "authority"];
const AUTHORITY_FIELDS: &[&str] = &["rule_id", "disposition"];
const QUALIFICATION_FIELDS: &[&str] = &["report_sha256", "corpus_sha256", "checker_abi"];

#[test]
fn scheme_object_is_closed_through_all_deserializers() {
    assert_object::<Ste100Scheme>(scheme(), SCHEME_FIELDS);
}
#[test]
fn scheme_reference_is_closed_through_all_deserializers() {
    assert_object::<SchemeReference>(scheme()["reference"].clone(), REFERENCE_FIELDS);
}
#[test]
fn scheme_grammar_blob_is_closed_through_all_deserializers() {
    assert_object::<GrammarBlob>(scheme()["grammar"].clone(), BLOB_FIELDS);
}
#[test]
fn scheme_verb_is_closed_through_all_deserializers() {
    assert_object::<VerbDefinition>(scheme()["verbs"][0].clone(), VERB_FIELDS);
}
#[test]
fn scheme_technical_name_is_closed_through_all_deserializers() {
    assert_object::<TechnicalNameDefinition>(scheme()["technical_names"][0].clone(), NAME_FIELDS);
}
#[test]
fn scheme_rule_reference_is_closed_through_all_deserializers() {
    assert_object::<RuleReference>(scheme()["verbs"][0]["authority"].clone(), AUTHORITY_FIELDS);
}
#[test]
fn scheme_qualification_is_closed_through_all_deserializers() {
    assert_object::<SchemeQualification>(scheme()["qualification"].clone(), QUALIFICATION_FIELDS);
}

#[test]
fn scheme_rejects_each_nested_array_alias_without_changing_legitimate_vectors() {
    let valid = scheme();
    assert_valid::<Ste100Scheme>(&valid);
    for (pointer, fields) in [
        ("", SCHEME_FIELDS),
        ("/reference", REFERENCE_FIELDS),
        ("/grammar", BLOB_FIELDS),
        ("/conditions", BLOB_FIELDS),
        ("/verbs/0", VERB_FIELDS),
        ("/technical_names/0", NAME_FIELDS),
        ("/verbs/0/authority", AUTHORITY_FIELDS),
        ("/technical_names/0/authority", AUTHORITY_FIELDS),
        ("/qualification", QUALIFICATION_FIELDS),
    ] {
        let mut malformed = valid.clone();
        let object = malformed.pointer_mut(pointer).unwrap();
        *object = sequence(object, fields);
        assert_bad_value::<Ste100Scheme>(&malformed);
    }
    // Actual vector fields and the scalar disposition keep their wire meanings.
    let mut empty_names = valid.clone();
    empty_names["technical_names"] = json!([]);
    assert_valid::<Ste100Scheme>(&empty_names);
    assert!(serde_json::from_value::<RuleDisposition>(json!("technical_use")).is_ok());
}

#[test]
fn scheme_rule_disposition_requires_its_scalar_string_in_every_position() {
    for spelling in ["approved_word", "technical_use"] {
        assert_valid::<RuleDisposition>(&json!(spelling));
        for pointer in [
            "/verbs/0/authority/disposition",
            "/technical_names/0/authority/disposition",
        ] {
            let mut valid = scheme();
            *valid.pointer_mut(pointer).unwrap() = json!(spelling);
            assert_valid::<Ste100Scheme>(&valid);
            for payload in [Value::Null, json!({}), json!([])] {
                let alias = json!({(spelling):payload});
                assert_bad_value::<RuleDisposition>(&alias);
                let mut malformed = valid.clone();
                *malformed.pointer_mut(pointer).unwrap() = alias;
                assert_bad_value::<Ste100Scheme>(&malformed);
            }
        }
    }
    for invalid in [
        Value::Null,
        json!(false),
        json!(1),
        json!("unknown"),
        json!(["technical_use"]),
    ] {
        assert_bad_value::<RuleDisposition>(&invalid);
    }
}

thread_local! {
    static BLOB_DECODES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

pub(super) fn note_blob_decode() {
    BLOB_DECODES.with(|calls| calls.set(calls.get() + 1));
}

fn counted<T>(operation: impl FnOnce() -> T) -> (T, usize) {
    BLOB_DECODES.with(|calls| calls.set(0));
    let result = operation();
    (result, BLOB_DECODES.with(std::cell::Cell::get))
}

fn bootstrap() -> Value {
    json!({
        "protocol":STE100_PROTOCOL,
        "scheme_text":serde_json::to_string(&scheme()).unwrap(),
        "instruction":{
            "model_binding":{"artifact":"/synthetic/manifest.json","devices":[2,0]},
            "model_artifact_sha256":"0".repeat(64),
            "instances":2,
            "max_wire_bytes":"65536",
            "measurement_ref":"synthetic-only",
            "repair_cap":{"attempts":2,"measurement_ref":"synthetic-only"},
            "decision_wait":{"elapsed_ns":"1000","measurement_ref":"synthetic-only"}
        }
    })
}

const BOOTSTRAP_FIELDS: &[&str] = &["protocol", "scheme_text", "instruction"];
const INSTRUCTION_FIELDS: &[&str] = &[
    "model_binding",
    "model_artifact_sha256",
    "instances",
    "max_wire_bytes",
    "measurement_ref",
    "repair_cap",
    "decision_wait",
];
const REPAIR_CAP_FIELDS: &[&str] = &["attempts", "measurement_ref"];
const MODEL_BINDING_FIELDS: &[&str] = &["artifact", "devices"];

#[test]
fn ste100_bootstrap_object_is_closed_through_all_deserializers() {
    let value = bootstrap();
    let decoded: Ste100Bootstrap = serde_json::from_value(value.clone()).unwrap();
    decoded.validate(DecimalU64(65536)).unwrap();
    assert_object::<Ste100Bootstrap>(value, BOOTSTRAP_FIELDS);
}

#[test]
fn ste100_instruction_object_is_closed_through_all_deserializers() {
    assert_object::<Ste100Instruction>(bootstrap()["instruction"].clone(), INSTRUCTION_FIELDS);
}

#[test]
fn ste100_repair_cap_object_is_closed_through_all_deserializers() {
    assert_object::<RepairCap>(
        bootstrap()["instruction"]["repair_cap"].clone(),
        REPAIR_CAP_FIELDS,
    );
}

#[test]
fn ste100_model_binding_object_is_closed_through_all_deserializers() {
    assert_object::<ModelBinding>(
        bootstrap()["instruction"]["model_binding"].clone(),
        MODEL_BINDING_FIELDS,
    );
    // This shared carrier still accepts the legacy relative locator and ordered devices.
    assert_valid::<ModelBinding>(&json!({"artifact":"legacy-decoder-fixture","devices":[2,0]}));
}

#[test]
fn ste100_bootstrap_rejects_each_newly_closed_nested_array_alias() {
    let valid = bootstrap();
    assert_valid::<Ste100Bootstrap>(&valid);
    for (pointer, fields) in [
        ("", BOOTSTRAP_FIELDS),
        ("/instruction", INSTRUCTION_FIELDS),
        ("/instruction/repair_cap", REPAIR_CAP_FIELDS),
        ("/instruction/model_binding", MODEL_BINDING_FIELDS),
    ] {
        let mut malformed = valid.clone();
        let object = malformed.pointer_mut(pointer).unwrap();
        *object = sequence(object, fields);
        assert_bad_value::<Ste100Bootstrap>(&malformed);
    }
    let decoded: Ste100Bootstrap = serde_json::from_value(valid.clone()).unwrap();
    assert_eq!(
        decoded.instruction.model_binding.devices,
        vec![crate::DeviceOrdinal(2), crate::DeviceOrdinal(0)]
    );
    assert_eq!(decoded.scheme_text, valid["scheme_text"].as_str().unwrap());
}

#[test]
fn scheme_disposition_rejects_unknown_spellings_directly_and_nested() {
    for (variant, spelling) in [
        (RuleDisposition::ApprovedWord, "approved_word"),
        (RuleDisposition::TechnicalUse, "technical_use"),
    ] {
        assert_eq!(serde_json::to_value(variant).unwrap(), json!(spelling));
        assert_eq!(
            serde_json::from_value::<RuleDisposition>(json!(spelling)).unwrap(),
            variant
        );
    }
    for spelling in [
        "technical",
        "approved",
        "technical-use",
        "approved-word",
        "TechnicalUse",
        "",
        "technical_use\0",
    ] {
        let invalid = json!(spelling);
        assert_bad_value::<RuleDisposition>(&invalid);
        for pointer in [
            "/verbs/0/authority/disposition",
            "/technical_names/0/authority/disposition",
        ] {
            let mut malformed = scheme();
            *malformed.pointer_mut(pointer).unwrap() = invalid.clone();
            assert_bad_value::<Ste100Scheme>(&malformed);
        }
    }
}

fn declared_scheme() -> (Ste100Scheme, Digest, SchemeDeclaration) {
    let digest = Digest::new("0".repeat(64)).unwrap();
    let declared = SchemeDeclaration {
        scheme_id: format!("ste100:{}", digest.as_str()),
        artifact_sha256: digest.clone(),
        verbs: vec!["read".into()],
    };
    let mut scheme: Ste100Scheme = serde_json::from_value(scheme()).unwrap();
    scheme.grammar.bytes_base64 = "YWJj".into();
    scheme.conditions.bytes_base64 = "AAEC/w==".into();
    (scheme, digest, declared)
}

#[test]
fn scheme_validate_and_decode_returns_each_blob_once_with_legacy_validation_parity() {
    let (scheme, digest, declared) = declared_scheme();
    let (decoded, calls) = counted(|| scheme.validate_and_decode(&digest, &declared));
    let (grammar, conditions) = decoded.unwrap();
    assert_eq!(grammar, b"abc");
    assert_eq!(conditions, [0, 1, 2, 255]);
    assert_eq!(calls, 2, "each blob must be decoded exactly once");
    let (old_result, old_calls) = counted(|| scheme.validate(&digest, &declared));
    old_result.unwrap();
    assert_eq!(old_calls, 2);
    // Metadata/encoding validation does not pretend to verify cryptographic content.
    let wrong = Digest::new("1".repeat(64)).unwrap();
    assert!(scheme.verify_blobs(|_| wrong.clone()).is_err());
    scheme.verify_blobs(|_| digest.clone()).unwrap();
}

#[test]
fn scheme_validate_and_decode_rejects_metadata_before_allocating_blobs() {
    let (scheme, digest, declared) = declared_scheme();
    let mut cases = Vec::new();
    let mut bad = scheme.clone();
    bad.format = "wrong".into();
    cases.push(bad);
    let mut bad = scheme.clone();
    bad.reference.edition.clear();
    cases.push(bad);
    let mut bad = scheme.clone();
    bad.reference.rule_ids.clear();
    cases.push(bad);
    let mut bad = scheme.clone();
    bad.qualification.checker_abi.clear();
    cases.push(bad);
    let mut bad = scheme.clone();
    bad.recall_keys = vec!["foreign".into()];
    cases.push(bad);
    let mut bad = scheme.clone();
    bad.verbs[0].forms.push("read".into());
    cases.push(bad);
    let mut bad = scheme.clone();
    bad.technical_names[0].text.clear();
    cases.push(bad);
    for bad in cases {
        let (result, calls) = counted(|| bad.validate_and_decode(&digest, &declared));
        let error = result.unwrap_err();
        assert_eq!(calls, 0);
        assert_eq!(
            bad.validate(&digest, &declared).unwrap_err().to_string(),
            error.to_string()
        );
    }
    let wrong_digest = Digest::new("1".repeat(64)).unwrap();
    let (result, calls) = counted(|| scheme.validate_and_decode(&wrong_digest, &declared));
    assert!(result.is_err());
    assert_eq!(calls, 0);
    let mut bad_declaration = declared;
    bad_declaration.scheme_id = "ste100:foreign".into();
    let (result, calls) = counted(|| scheme.validate_and_decode(&digest, &bad_declaration));
    assert!(result.is_err());
    assert_eq!(calls, 0);
}

#[test]
fn scheme_validate_and_decode_preserves_canonical_encoding_failures() {
    let (scheme, digest, declared) = declared_scheme();
    for bad_encoding in ["", "YQ", "YR==", "YWJ=", "YQ==AAAA", "YQ?="] {
        for conditions in [false, true] {
            let mut bad = scheme.clone();
            let blob = if conditions {
                &mut bad.conditions
            } else {
                &mut bad.grammar
            };
            blob.bytes_base64 = bad_encoding.into();
            let (result, calls) = counted(|| bad.validate_and_decode(&digest, &declared));
            let error = result.unwrap_err();
            assert_eq!(calls, if conditions { 2 } else { 1 });
            assert_eq!(
                bad.validate(&digest, &declared).unwrap_err().to_string(),
                error.to_string()
            );
        }
    }
}

super::super::codec::closed_object! {
    /// Test the supported container spelling with a field which actually changes.
    #[derive(Debug, PartialEq, Serialize)]
    #[serde(rename_all = "kebab-case")]
    struct RenamedCarrier {
        exact_value: String,
        #[serde(deserialize_with = "super::super::required_option")]
        prior_value: Option<String>,
    }
}

#[test]
fn closed_object_container_rename_is_shared_by_writer_and_named_map_reader() {
    let value = json!({"exact-value":"retained", "prior-value":null});
    assert_valid::<RenamedCarrier>(&value);
    for bad in [
        json!({"exact_value":"retained", "prior_value":null}),
        json!({"exact-value":"retained"}),
        json!({"exact-value":"retained", "prior-value":null, "extra":1}),
        json!(["retained", null]),
    ] {
        assert_bad_value::<RenamedCarrier>(&bad);
    }
    assert_bad_json::<RenamedCarrier>(
        r#"{"exact-value":"a","exact-value":"b","prior-value":null}"#,
    );
    assert_valid::<RenamedCarrier>(&json!({"exact-value":"retained", "prior-value":"previous"}));
}
