use super::*;
use crate::state_flow::decode_strict;
use serde::de::DeserializeOwned;
use serde_json::{Value, json, to_vec as encode_canonical};

fn source(sequence: u64, block: u64) -> SourceBlockId {
    SourceBlockId {
        run: "run".into(),
        sequence: sequence.into(),
        block: block.into(),
    }
}

fn span(start: u64, end: u64) -> Span {
    Span {
        start: start.into(),
        end: end.into(),
    }
}

fn matched() -> ThesaurusMatch {
    ThesaurusMatch {
        source: source(10, 2),
        previous_change: None,
        sentence_span: span(0, 9),
        word_span: span(4, 8),
        scheme_id: format!("ste100:{}", "b".repeat(64)),
        thesaurus_sha256: Digest::new("a".repeat(64)).unwrap(),
        entry_word: "coal".into(),
    }
}

fn material() -> OfferMaterial {
    OfferMaterial {
        matched: matched(),
        sentence: "Use coal.".into(),
        senses: vec![OfferSense {
            sense: "material".into(),
            r#use: vec!["fuel".into()],
        }],
    }
}

fn splice() -> EmissionChangeRecord {
    EmissionChangeRecord {
        matched: matched(),
        span: span(4, 8),
        replaced: "coal".into(),
        replacement: "fuel".into(),
        method: EmissionChangeMethod::Splice {},
    }
}

fn offer() -> EmissionChangeRecord {
    EmissionChangeRecord {
        span: span(0, 9),
        replaced: "Use coal.".into(),
        replacement: "Use fuel.".into(),
        method: EmissionChangeMethod::Offer {
            instruction: source(12, 0),
            reemission: source(13, 3),
            reemission_span: span(2, 11),
        },
        ..splice()
    }
}

#[test]
fn lookup_and_change_records_keep_canonical_fields_nulls_and_original_indices() {
    for (record, fixture) in [
        (
            splice(),
            include_str!("../../tests/fixtures/ste-emission-splice.json"),
        ),
        (
            offer(),
            include_str!("../../tests/fixtures/ste-emission-offer.json"),
        ),
    ] {
        assert_eq!(
            decode_strict::<EmissionChangeRecord>(fixture).unwrap(),
            record
        );
        assert_eq!(
            encode_canonical(&record).unwrap(),
            fixture.trim_end().as_bytes()
        );
    }
    let mut value = material();
    value.validate().unwrap();
    let encoded = String::from_utf8(encode_canonical(&value).unwrap()).unwrap();
    assert!(encoded.starts_with("{\"matched\":{\"source\":{\"run\":\"run\",\"sequence\":\"10\",\"block\":\"2\"},\"previous_change\":null,"));
    assert!(encoded.ends_with("\"senses\":[{\"sense\":\"material\",\"use\":[\"fuel\"]}]}"));
    value.matched.previous_change = Some(11.into());
    value.senses.push(OfferSense {
        sense: "second".into(),
        r#use: vec!["b".into(), "a".into()],
    });
    let bytes = encode_canonical(&value).unwrap();
    assert_eq!(
        decode_strict::<OfferMaterial>(std::str::from_utf8(&bytes).unwrap()).unwrap(),
        value
    );
}

fn object_paths(value: &Value, at: String, paths: &mut Vec<String>) {
    match value {
        Value::Object(fields) => {
            paths.push(at.clone());
            for (key, child) in fields {
                object_paths(child, format!("{at}/{key}"), paths);
            }
        }
        Value::Array(values) => {
            for (index, child) in values.iter().enumerate() {
                object_paths(child, format!("{at}/{index}"), paths);
            }
        }
        _ => {}
    }
}

fn closed_at_every_depth<T: DeserializeOwned>(value: Value) {
    let original = serde_json::to_string(&value).unwrap();
    decode_strict::<T>(&original).unwrap_or_else(|error| panic!("baseline: {error}"));
    let mut paths = Vec::new();
    object_paths(&value, String::new(), &mut paths);
    for path in paths {
        let object = value.pointer(&path).unwrap().as_object().unwrap();
        let mut changed = value.clone();
        changed
            .pointer_mut(&path)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("surprise".into(), json!(true));
        assert!(
            decode_strict::<T>(&changed.to_string()).is_err(),
            "unknown {path}"
        );
        let mut changed = value.clone();
        *changed.pointer_mut(&path).unwrap() = Value::Array(object.values().cloned().collect());
        assert!(
            decode_strict::<T>(&changed.to_string()).is_err(),
            "array {path}"
        );
        for (key, field) in object {
            let mut changed = value.clone();
            changed
                .pointer_mut(&path)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove(key);
            assert!(
                decode_strict::<T>(&changed.to_string()).is_err(),
                "missing {path}/{key}"
            );
            let fragment = serde_json::to_string(object).unwrap();
            let duplicate = format!(
                "{{{}:{},{}",
                serde_json::to_string(key).unwrap(),
                field,
                &fragment[1..]
            );
            let changed = original.replacen(&fragment, &duplicate, 1);
            assert_ne!(changed, original);
            assert!(
                decode_strict::<T>(&changed).is_err(),
                "duplicate {path}/{key}"
            );
        }
    }
}

#[test]
fn lookup_codecs_are_closed_objects_at_every_depth() {
    closed_at_every_depth::<OfferMaterial>(serde_json::to_value(material()).unwrap());
    closed_at_every_depth::<EmissionChangeRecord>(serde_json::to_value(splice()).unwrap());
    closed_at_every_depth::<EmissionChangeRecord>(serde_json::to_value(offer()).unwrap());
    for bad in ["01", "+1", "-1", "18446744073709551616"] {
        let mut value = serde_json::to_value(offer()).unwrap();
        value["matched"]["source"]["block"] = json!(bad);
        assert!(decode_strict::<EmissionChangeRecord>(&value.to_string()).is_err());
    }
    let mut value = serde_json::to_value(offer()).unwrap();
    value["matched"]["previous_change"] = json!(11);
    assert!(decode_strict::<EmissionChangeRecord>(&value.to_string()).is_err());
    value = serde_json::to_value(offer()).unwrap();
    value["matched"]["thesaurus_sha256"] = json!("A".repeat(64));
    assert!(decode_strict::<EmissionChangeRecord>(&value.to_string()).is_err());
    for method in [
        json!("splice"),
        json!({"other":{}}),
        json!({"splice":{},"offer":{}}),
    ] {
        assert!(decode_strict::<EmissionChangeMethod>(&method.to_string()).is_err());
    }
}

#[test]
fn lookup_span_and_offer_material_checks_keep_utf8_and_nonempty_senses() {
    let mut good = material();
    good.sentence = "Use caf\u{e9}.".into();
    good.matched.sentence_span = span(5, 15);
    good.matched.word_span = span(9, 14);
    good.validate().unwrap();
    for change in [
        (|v: &mut OfferMaterial| v.matched.source.run.clear()) as fn(&mut OfferMaterial),
        |v| v.matched.scheme_id.clear(),
        |v| v.matched.entry_word.clear(),
        |v| v.matched.sentence_span = span(15, 5),
        |v| v.matched.word_span = span(9, 9),
        |v| v.matched.word_span = span(4, 9),
        |v| v.matched.word_span = span(9, 16),
        |v| v.matched.word_span = span(9, 13),
        |v| v.sentence.push(' '),
        |v| v.senses.clear(),
        |v| v.senses[0].sense.clear(),
        |v| v.senses[0].r#use.clear(),
        |v| v.senses[0].r#use[0].clear(),
    ] {
        let mut bad = good.clone();
        change(&mut bad);
        assert!(bad.validate().is_err(), "{bad:?}");
    }
    let mut text = matched();
    text.sentence_span = span(0, 10);
    text.word_span = span(4, 9);
    text.validate_text("Use caf\u{e9}.").unwrap();
    text.word_span.end = 8.into();
    assert!(text.validate_text("Use caf\u{e9}.").is_err());
}

#[test]
fn emission_changes_require_the_exact_current_and_response_slices() {
    splice().validate_texts("Use coal.", None).unwrap();
    offer()
        .validate_texts("Use coal.", Some("xxUse fuel.zz"))
        .unwrap();
    assert!(
        splice()
            .validate_texts("Use coal.", Some("unused"))
            .is_err()
    );
    assert!(offer().validate_texts("Use coal.", None).is_err());
    assert!(
        offer()
            .validate_texts("Use coal.", Some("xxUse fire.zz"))
            .is_err()
    );
    for mut record in [splice(), offer()] {
        let response =
            matches!(record.method, EmissionChangeMethod::Offer { .. }).then_some("xxUse fuel.zz");
        for change in [
            (|v: &mut EmissionChangeRecord| v.replaced.push(' ')) as fn(&mut EmissionChangeRecord),
            |v| v.replaced.clear(),
            |v| v.replacement.clear(),
            |v| v.span = span(1, 9),
        ] {
            let mut bad = record.clone();
            change(&mut bad);
            assert!(
                bad.validate_texts("Use coal.", response).is_err(),
                "{bad:?}"
            );
        }
        record.matched.word_span = span(4, 7);
        if matches!(record.method, EmissionChangeMethod::Splice { .. }) {
            assert!(record.validate_texts("Use coal.", response).is_err());
        }
    }
    for change in [
        (|instruction: &mut SourceBlockId, _: &mut SourceBlockId, _: &mut Span| {
            instruction.block = 1.into()
        }) as fn(&mut SourceBlockId, &mut SourceBlockId, &mut Span),
        |instruction, _, _| instruction.run = "foreign".into(),
        |_, reemission, _| reemission.run = "foreign".into(),
        |_, _, span| span.end = 12.into(),
        |_, _, span| *span = super::tests::span(11, 2),
    ] {
        let mut bad = offer();
        let EmissionChangeMethod::Offer {
            instruction,
            reemission,
            reemission_span,
        } = &mut bad.method
        else {
            unreachable!()
        };
        change(instruction, reemission, reemission_span);
        assert!(
            bad.validate_texts("Use coal.", Some("xxUse fuel.zz"))
                .is_err()
        );
    }
}

#[test]
fn change_text_keeps_unicode_quotes_and_newlines_without_normalization() {
    let mut change = splice();
    change.matched.sentence_span = span(0, 10);
    change.matched.word_span = span(4, 9);
    change.span = span(4, 9);
    change.replaced = "caf\u{e9}".into();
    change.replacement = "\"tea\"\nnext".into();
    change.validate_texts("Use caf\u{e9}.", None).unwrap();
    let bytes = encode_canonical(&change).unwrap();
    assert!(!bytes.contains(&b'\n'));
    let decoded: EmissionChangeRecord =
        decode_strict(std::str::from_utf8(&bytes).unwrap()).unwrap();
    assert_eq!(decoded, change);
    change.span.end = 8.into();
    assert!(change.validate_texts("Use caf\u{e9}.", None).is_err());

    let mut change = offer();
    change.replacement = "Use caf\u{e9}.".into();
    let EmissionChangeMethod::Offer {
        reemission_span, ..
    } = &mut change.method
    else {
        unreachable!()
    };
    *reemission_span = span(2, 12);
    change
        .validate_texts("Use coal.", Some("xxUse caf\u{e9}.zz"))
        .unwrap();
    let EmissionChangeMethod::Offer {
        reemission_span, ..
    } = &mut change.method
    else {
        unreachable!()
    };
    reemission_span.end = 10.into();
    assert!(
        change
            .validate_texts("Use coal.", Some("xxUse caf\u{e9}.zz"))
            .is_err()
    );
}
