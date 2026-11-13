use super::read_pinned;
use crate::{check::Checker, lexicon::Lexicon};
use serde_json::json;
use std::collections::BTreeSet;

#[test]
fn source_mapping_reaches_checks_and_notes_with_every_lexicon_source() {
    let words = json!({
        "approved_words":{"read":["v"],"the":["art"],"a":["art"]},
        "unapproved_words":{"odd":["adj"],"inspect":["v"]},
        "unapproved_verbs":[
            {"word":"inspect","alternatives":["read"]},
            {"word":"blob","alternatives":["read"]},
            {"word":"buffer","alternatives":["read"]}
        ],
        "technical_noun_examples":["blob"],
        "verbs":[{"key":"read"}]
    });
    let lex = Lexicon::from_sources(
        words,
        json!({"verbs":{"stream":{}}}),
        json!({"nouns":["buffer"]}),
        json!({"domain":["READ"],"products":["ODD"]}),
        json!({"mappings":{"inspect":"read"}}),
    )
    .unwrap();
    let checker = Checker::new(lex, 3, 25).unwrap();
    let names = BTreeSet::new();
    let result = checker
        .check(
            "READ the blob. Blob a buffer. Buffer the blob.",
            "text",
            &names,
        )
        .unwrap();
    assert!(result.ok(), "{:?}", result.hard);
    assert!(result.soft.is_empty());
    let technical = checker
        .check("Stream the buffer. Streaming a buffer.", "text", &names)
        .unwrap();
    assert!(technical.ok(), "{:?}", technical.hard);
    assert!(technical.soft.is_empty());
    let odd = checker.check("The odd zebra.", "text", &names).unwrap();
    assert_eq!(odd.soft["unapproved_words"], [json!("odd")]);
    assert_eq!(odd.soft["unknown_words"], [json!("zebra")]);
    let proper = checker.check("ODD.", "text", &names).unwrap();
    assert!(proper.ok());
    assert!(proper.soft.is_empty());
    let verb = checker
        .check("Inspected the blob.", "text", &names)
        .unwrap();
    assert_eq!(verb.hard["verbs"], [json!("inspected")]);
    assert!(
        checker
            .repair_note(&verb)
            .contains("Write \"read\" for it (a fixed rule).")
    );
    assert_eq!(checker.lex.unapproved_verbs["inspect"], ["read"]);
    let instruction = checker
        .check("Read the buffer a blob.", "text", &names)
        .unwrap();
    assert_eq!(instruction.hard["length"][0]["limit"], 3);
}

#[test]
fn pinned_reads_keep_exact_bytes_and_refuse_changed_source() {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("ste-pinned-read-{}-{nanos}", std::process::id()));
    std::fs::create_dir(&dir).unwrap();
    let path = dir.join("source");
    std::fs::write(&path, b"one\r\n").unwrap();
    let expected = "5259d46a49644bf76792231ef7315b5293677c49ddd7e69d95557013e10320d4";
    assert_eq!(read_pinned(&path, expected).unwrap(), b"one\r\n");
    std::fs::write(&path, b"one\n").unwrap();
    let error = read_pinned(&path, expected).unwrap_err();
    assert!(error.to_string().contains("source hash mismatch"));
    std::fs::remove_file(path).unwrap();
    std::fs::remove_dir(dir).unwrap();
}
