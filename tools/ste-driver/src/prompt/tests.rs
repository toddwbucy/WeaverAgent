use super::{decode, quoted};

#[test]
fn source_json_spelling_keeps_python_defaults() {
    assert_eq!(
        quoted("a\"\\\n\t\r\x08\x0c\x1f\x7f\u{e9}\u{1f600}"),
        "\"a\\\"\\\\\\n\\t\\r\\b\\f\\u001f\\u007f\\u00e9\\ud83d\\ude00\""
    );
}

#[test]
fn source_bundle_checks_hashes_roles_and_limits_before_returning_examples() {
    let bytes = include_bytes!("../../tests/prompt-reference.json");
    let system = "ce1ba678d9b6d99aaf4bf8facee81e9e6f8b657bc688a37f05c4375a06382a54";
    let bundle = "77081eb4aed62d4982737ec46f0c711772ef3d529189e3f9fa1ea299dff20dd1";
    let examples = decode(bytes, system, bundle).unwrap();
    assert_eq!(examples, ["one\n", "two", "three", "four"]);
    assert!(decode(bytes, "wrong", bundle).is_err());
    assert!(decode(bytes, system, "wrong").is_err());
    let mut value: serde_json::Value = serde_json::from_slice(bytes).unwrap();
    value["examples"][0]["role"] = serde_json::json!("assistant");
    assert!(decode(&serde_json::to_vec(&value).unwrap(), system, bundle).is_err());
    value["examples"][0]["role"] = serde_json::json!("user");
    value["meta"]["limit_instruction"] = serde_json::json!(21);
    assert!(decode(&serde_json::to_vec(&value).unwrap(), system, bundle).is_err());
}
