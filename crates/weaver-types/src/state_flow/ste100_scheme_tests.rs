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
