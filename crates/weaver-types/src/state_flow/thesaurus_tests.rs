use super::*;
use serde_json::{Value, json};

fn digest(c: char) -> Digest {
    Digest::new(c.to_string().repeat(64)).unwrap()
}

fn scheme() -> SchemeDeclaration {
    SchemeDeclaration {
        scheme_id: format!("ste100:{}", digest('a')),
        artifact_sha256: digest('a'),
        verbs: vec!["read".into()],
    }
}

fn declared() -> ThesaurusDeclaration {
    ThesaurusDeclaration {
        artifact_sha256: digest('b'),
        scheme_id: scheme().scheme_id,
    }
}

fn wire() -> Value {
    json!({
        "format":"ste100-thesaurus/1",
        "scheme_id":scheme().scheme_id,
        "source":{
            "repository":"fixture-repository",
            "commit":"fixture-commit",
            "path":"fixture.toml",
            "sha256":digest('c'),
        },
        "entries":[
            {"word":"z-word","tag":"offer","senses":[
                {"sense":"second sense first","use":["z use", "a use", "z use"]},
                {"sense":"first sense second","use":["other use"]}
            ]},
            {"word":"a-word","tag":"splice","senses":[
                {"sense":"one sense","use":["one use"]}
            ]}
        ]
    })
}

fn artifact() -> Ste100Thesaurus {
    serde_json::from_value(wire()).unwrap()
}

fn valid(value: &Ste100Thesaurus) -> bool {
    value.validate(&digest('b'), &declared(), &scheme()).is_ok()
}

// SCHEME and THESAURUS: the exact-byte digest belongs to the artifact owner.
#[test]
fn artifact_and_declaration_must_name_the_bound_scheme_and_digest() {
    let value = artifact();
    assert!(valid(&value));
    assert!(
        value
            .validate(&digest('c'), &declared(), &scheme())
            .is_err()
    );
    let mut other_declaration = declared();
    other_declaration.scheme_id = format!("ste100:{}", digest('c'));
    assert!(other_declaration.validate(&scheme()).is_err());
    assert!(
        value
            .validate(&digest('b'), &other_declaration, &scheme())
            .is_err()
    );
    let mut other_scheme = scheme();
    other_scheme.artifact_sha256 = digest('c');
    assert!(
        value
            .validate(&digest('b'), &declared(), &other_scheme)
            .is_err()
    );
    let mut other_artifact = value.clone();
    other_artifact.scheme_id = format!("ste100:{}", digest('c'));
    assert!(!valid(&other_artifact));
    other_artifact = value;
    other_artifact.format = "ste100-thesaurus/2".into();
    assert!(!valid(&other_artifact));
}

#[test]
fn artifact_round_trip_preserves_entry_sense_and_use_order() {
    let value = artifact();
    assert_eq!(serde_json::to_value(&value).unwrap(), wire());
    let encoded = serde_json::to_string(&value).unwrap();
    assert!(encoded.starts_with("{\"format\":\"ste100-thesaurus/1\",\"scheme_id\":"));
    assert!(encoded.contains("\"word\":\"z-word\",\"tag\":\"offer\",\"senses\":"));
    assert!(
        encoded
            .contains("\"sense\":\"second sense first\",\"use\":[\"z use\",\"a use\",\"z use\"]")
    );
    assert!(valid(&serde_json::from_str(&encoded).unwrap()));
}

#[test]
fn offer_allows_one_or_many_senses_but_splice_requires_one_use() {
    let mut value = artifact();
    value.entries[0].tag = ThesaurusTag::Splice;
    value.entries[0].senses[0].r#use.truncate(1);
    assert!(!valid(&value));
    value.entries[0].senses.truncate(1);
    assert!(valid(&value));
    value.entries[0].senses[0].r#use.push("second use".into());
    assert!(!valid(&value));
    value.entries[0].senses[0].r#use.truncate(1);
    assert!(valid(&value));
    value.entries[0].tag = ThesaurusTag::Offer;
    assert!(valid(&value));
}

#[test]
fn duplicate_words_and_empty_source_or_entry_fields_are_refused() {
    let mut value = artifact();
    value.entries.push(value.entries[0].clone());
    assert!(!valid(&value));
    for pointer in [
        "/source/repository",
        "/source/commit",
        "/source/path",
        "/entries/0/word",
        "/entries/0/senses/0/sense",
        "/entries/0/senses/0/use/0",
    ] {
        let mut input = wire();
        *input.pointer_mut(pointer).unwrap() = json!("");
        assert!(!valid(&serde_json::from_value(input).unwrap()), "{pointer}");
    }
    for pointer in ["/entries/0/senses", "/entries/0/senses/0/use"] {
        let mut input = wire();
        *input.pointer_mut(pointer).unwrap() = json!([]);
        assert!(!valid(&serde_json::from_value(input).unwrap()), "{pointer}");
    }
}

// THESAURUS requires unique words but does not elect a minimum entry count.
#[test]
fn empty_artifact_has_no_lookup_entries_and_remains_well_formed() {
    let mut value = artifact();
    value.entries.clear();
    assert!(valid(&value));
}

#[test]
fn artifact_objects_refuse_unknown_duplicate_missing_and_array_fields() {
    for (pointer, fields) in [
        ("", &["format", "scheme_id", "source", "entries"][..]),
        ("/source", &["repository", "commit", "path", "sha256"][..]),
        ("/entries/0", &["word", "tag", "senses"][..]),
        ("/entries/0/senses/0", &["sense", "use"][..]),
    ] {
        let original = wire();
        let object = original.pointer(pointer).unwrap();
        let mut unknown = original.clone();
        unknown
            .pointer_mut(pointer)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unexpected".into(), json!(1));
        assert!(
            serde_json::from_value::<Ste100Thesaurus>(unknown).is_err(),
            "{pointer}"
        );
        let mut array = original.clone();
        *array.pointer_mut(pointer).unwrap() =
            Value::Array(fields.iter().map(|field| object[*field].clone()).collect());
        assert!(
            serde_json::from_value::<Ste100Thesaurus>(array).is_err(),
            "{pointer}"
        );
        for (key, child) in object.as_object().unwrap() {
            let mut missing = original.clone();
            missing
                .pointer_mut(pointer)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove(key);
            assert!(
                serde_json::from_value::<Ste100Thesaurus>(missing).is_err(),
                "{pointer}/{key}"
            );
            let obj = object.to_string();
            let duplicate = format!("{{{}:{},{}", json!(key), child, &obj[1..]);
            let text = original.to_string().replacen(&obj, &duplicate, 1);
            assert!(
                serde_json::from_str::<Ste100Thesaurus>(&text).is_err(),
                "{pointer}/{key}"
            );
        }
    }
}

#[test]
fn declaration_and_artifact_reject_invalid_digest_and_tag_spellings() {
    for pointer in ["/source/sha256", "/entries/0/tag"] {
        for bad in [
            json!(""),
            json!("B".repeat(64)),
            json!(null),
            json!(1),
            json!({"offer": null}),
            json!({"splice": null}),
            json!(["offer"]),
        ] {
            let mut input = wire();
            *input.pointer_mut(pointer).unwrap() = bad;
            assert!(serde_json::from_value::<Ste100Thesaurus>(input).is_err());
        }
    }
    let value = declared();
    let text = serde_json::to_string(&value).unwrap();
    assert!(text.starts_with("{\"artifact_sha256\":"));
    assert_eq!(
        serde_json::from_str::<ThesaurusDeclaration>(&text).unwrap(),
        value
    );
    for bad in [
        text.replacen('{', "{\"scheme_id\":\"x\",", 1),
        text.replacen('{', "{\"unknown\":null,", 1),
        format!(
            "[{},{}]",
            json!(value.artifact_sha256),
            json!(value.scheme_id)
        ),
    ] {
        assert!(serde_json::from_str::<ThesaurusDeclaration>(&bad).is_err());
    }
}
