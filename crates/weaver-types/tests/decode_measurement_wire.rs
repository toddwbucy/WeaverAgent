//! Exact TYPES-DECODE shapes at the coupled serving cutover. These tests
//! certify the shared representation, not a successful session, a cut,
//! proposal custody or the SPU's measured values.

use serde_json::{Value, json};
use weaver_traits::{ContentBlock, Message, Role};
use weaver_types::{RefusalRecord, TokenAnswer, TokenAsk, TokenDirective, TokenRefusal, TurnKey};

fn delta() -> Vec<Message> {
    vec![Message {
        role: Role::User,
        content: vec![ContentBlock::Text {
            text: "Read item.".into(),
        }],
    }]
}

fn append() -> TokenDirective {
    TokenDirective::AppendAndGenerate {
        turn: TurnKey("turn-7".into()),
        delta: delta(),
        tool_schemas: vec!["second".into(), "first\nlast".into()],
    }
}

fn measure() -> TokenDirective {
    TokenDirective::MeasureDelta {
        keep: u64::MAX,
        delta: delta(),
        tool_schemas: vec!["second".into(), "first\nlast".into()],
    }
}

fn opened() -> TokenAnswer {
    TokenAnswer::Opened(weaver_types::Opened {
        session: weaver_types::SessionId("s".into()),
        prefix_tokens: 13,
        capacity: u64::MAX,
        generation_limit: 42,
        terminal_tokens: 1,
        request: serde_json::value::RawValue::from_string(r#"{"opaque_request":true}"#.into())
            .unwrap(),
        measurement: serde_json::value::RawValue::from_string(
            r#"{"opaque_measurement":true}"#.into(),
        )
        .unwrap(),
    })
}

fn measured() -> TokenAnswer {
    TokenAnswer::DeltaMeasured {
        resident: 127,
        prefix_tokens: 13,
        keep: 91,
        input_tokens: u64::MAX,
        capacity: 32768,
    }
}

#[test]
fn append_keeps_internal_tag_and_exact_ordered_schemas() {
    let value = append();
    let bytes = serde_json::to_string(&value).unwrap();
    assert_eq!(
        bytes,
        concat!(
            r#"{"kind":"append_and_generate","turn":"turn-7","delta":["#,
            r#"{"role":"user","content":[{"type":"text","text":"Read item."}]}],"#,
            r#""tool_schemas":["second","first\nlast"]}"#,
        )
    );
    assert_eq!(
        serde_json::from_str::<TokenDirective>(&bytes).unwrap(),
        value
    );
}

#[test]
fn measure_keeps_internal_tag_and_full_numeric_keep() {
    let value = measure();
    let bytes = serde_json::to_string(&value).unwrap();
    assert_eq!(
        bytes,
        concat!(
            r#"{"kind":"measure_delta","keep":18446744073709551615,"delta":["#,
            r#"{"role":"user","content":[{"type":"text","text":"Read item."}]}],"#,
            r#""tool_schemas":["second","first\nlast"]}"#,
        )
    );
    assert_eq!(
        serde_json::from_str::<TokenDirective>(&bytes).unwrap(),
        value
    );
}

#[test]
fn opened_has_exact_required_numeric_body() {
    let value = opened();
    let bytes = serde_json::to_string(&value).unwrap();
    assert_eq!(
        bytes,
        concat!(
            r#"{"kind":"opened","body":{"session":"s","prefix_tokens":13,"#,
            r#""capacity":18446744073709551615,"generation_limit":42,"terminal_tokens":1,"#,
            r#""request":{"opaque_request":true},"measurement":{"opaque_measurement":true}}}"#,
        )
    );
    assert_eq!(serde_json::from_str::<TokenAnswer>(&bytes).unwrap(), value);
}

#[test]
fn shared_open_corpus_keeps_nested_boxes_opaque_and_checks_the_closed_outer_wire() {
    #[derive(serde::Deserialize)]
    struct Envelope {
        body: Box<serde_json::value::RawValue>,
    }
    #[derive(serde::Deserialize)]
    struct Boxes {
        request: Box<serde_json::value::RawValue>,
        measurement: Box<serde_json::value::RawValue>,
    }
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/decode-open-evidence.json")).unwrap();
    for case in cases {
        let bytes = case["wire"].as_str().unwrap();
        let answer = serde_json::from_str::<TokenAnswer>(bytes);
        assert_eq!(
            answer.is_ok(),
            case["outer_valid"].as_bool().unwrap(),
            "{}",
            case["name"]
        );
        if let Ok(TokenAnswer::Opened(opened)) = answer {
            let original: Envelope = serde_json::from_str(bytes).unwrap();
            let original: Boxes = serde_json::from_str(original.body.get()).unwrap();
            assert_eq!(
                opened.request.get(),
                original.request.get(),
                "{}",
                case["name"]
            );
            assert_eq!(
                opened.measurement.get(),
                original.measurement.get(),
                "{}",
                case["name"]
            );
            let emitted = serde_json::to_string(&TokenAnswer::Opened(opened.clone())).unwrap();
            assert!(emitted.contains(opened.request.get()));
            assert!(emitted.contains(opened.measurement.get()));
            assert_eq!(
                serde_json::from_str::<TokenAnswer>(&emitted).unwrap(),
                TokenAnswer::Opened(opened)
            );
        }
    }
}

#[test]
fn opened_equality_checks_every_fact_and_each_raw_box_byte() {
    let TokenAnswer::Opened(original) = opened() else {
        unreachable!()
    };
    let mut changed = original.clone();
    changed.session.0.push('x');
    assert_ne!(changed, original);
    for field in [
        "prefix_tokens",
        "capacity",
        "generation_limit",
        "terminal_tokens",
    ] {
        let mut changed = original.clone();
        let value = match field {
            "prefix_tokens" => &mut changed.prefix_tokens,
            "capacity" => &mut changed.capacity,
            "generation_limit" => &mut changed.generation_limit,
            _ => &mut changed.terminal_tokens,
        };
        *value = value.wrapping_add(1);
        assert_ne!(changed, original, "{field}");
    }
    for request in [true, false] {
        let mut changed = original.clone();
        let raw = if request {
            &mut changed.request
        } else {
            &mut changed.measurement
        };
        *raw = serde_json::value::RawValue::from_string(raw.get().replacen('{', "{ ", 1)).unwrap();
        assert_ne!(changed, original, "raw whitespace is evidence");
    }
}

#[test]
fn opened_has_one_public_named_object_decoder_without_an_array_alias() {
    use serde::Deserialize as _;
    let TokenAnswer::Opened(expected) = opened() else {
        unreachable!()
    };
    let object = serde_json::to_string(&expected).unwrap();
    let positional =
        r#"["s",13,18446744073709551615,42,1,{"opaque_request":true},{"opaque_measurement":true}]"#;
    let direct =
        weaver_types::Opened::deserialize(&mut serde_json::Deserializer::from_str(&object))
            .unwrap();
    assert_eq!(direct, expected);
    assert!(
        weaver_types::Opened::deserialize(&mut serde_json::Deserializer::from_str(positional))
            .is_err()
    );
    assert!(serde_json::from_reader::<_, weaver_types::Opened>(positional.as_bytes()).is_err());
    for envelope in [
        format!("{{\"kind\":\"opened\",\"body\":{positional}}}"),
        format!("{{\"body\":{positional},\"kind\":\"opened\"}}"),
    ] {
        assert!(serde_json::from_str::<TokenAnswer>(&envelope).is_err());
    }
    for rejected in [
        object.replacen("{", "{\"extra\":null,", 1),
        object.replacen(
            "\"session\":\"s\"",
            "\"session\":\"s\",\"session\":\"s\"",
            1,
        ),
        object.replace("\"session\":\"s\",", ""),
    ] {
        assert!(serde_json::from_str::<weaver_types::Opened>(&rejected).is_err());
    }
}

#[test]
fn measured_has_exact_required_numeric_body_without_capping_count() {
    let value = measured();
    let bytes = serde_json::to_string(&value).unwrap();
    assert_eq!(
        bytes,
        concat!(
            r#"{"kind":"delta_measured","body":{"resident":127,"prefix_tokens":13,"#,
            r#""keep":91,"input_tokens":18446744073709551615,"capacity":32768}}"#,
        )
    );
    assert_eq!(serde_json::from_str::<TokenAnswer>(&bytes).unwrap(), value);
}

#[test]
fn every_proposal_member_is_required_and_null_is_not_a_default() {
    for proposal in [append(), measure()] {
        let canonical = serde_json::to_value(&proposal).unwrap();
        for field in canonical.as_object().unwrap().keys() {
            let mut missing = canonical.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(
                serde_json::from_str::<TokenDirective>(&missing.to_string()).is_err(),
                "absent {field} must refuse: {missing}"
            );
            let mut null = canonical.clone();
            null[field] = Value::Null;
            assert!(
                serde_json::from_str::<TokenDirective>(&null.to_string()).is_err(),
                "null {field} must refuse: {null}"
            );
        }
    }
    for proposal in [
        TokenDirective::AppendAndGenerate {
            turn: TurnKey("turn-7".into()),
            delta: vec![],
            tool_schemas: vec![],
        },
        TokenDirective::MeasureDelta {
            keep: 0,
            delta: vec![],
            tool_schemas: vec![],
        },
    ] {
        let bytes = serde_json::to_string(&proposal).unwrap();
        assert!(bytes.contains(r#""tool_schemas":[]"#));
        assert_eq!(
            serde_json::from_str::<TokenDirective>(&bytes).unwrap(),
            proposal
        );
    }
}

#[test]
fn every_answer_fact_is_required_and_null_is_not_a_default() {
    for answer in [opened(), measured()] {
        let canonical = serde_json::to_value(&answer).unwrap();
        for field in canonical["body"].as_object().unwrap().keys() {
            let mut missing = canonical.clone();
            missing["body"].as_object_mut().unwrap().remove(field);
            assert!(
                serde_json::from_str::<TokenAnswer>(&missing.to_string()).is_err(),
                "absent {field} must refuse: {missing}"
            );
            // The floor carries opaque boxes. Their null/object/content check
            // belongs to HARNESS-OPEN, not this shared vocabulary.
            if matches!(field.as_str(), "request" | "measurement") {
                continue;
            }
            let mut null = canonical.clone();
            null["body"][field] = Value::Null;
            assert!(
                serde_json::from_str::<TokenAnswer>(&null.to_string()).is_err(),
                "null {field} must refuse: {null}"
            );
        }
    }
}

#[test]
fn legacy_or_retagged_decode_shapes_are_not_aliases() {
    for bytes in [
        r#"{"kind":"opened"}"#,
        r#"{"kind":"opened","body":null}"#,
        r#"{"kind":"opened","body":{}}"#,
        r#"{"kind":"opened","prefix_tokens":13,"capacity":32768,"generation_limit":42,"terminal_tokens":1}"#,
        r#"{"opened":{"prefix_tokens":13,"capacity":32768,"generation_limit":42,"terminal_tokens":1}}"#,
        r#"{"kind":"delta_measured"}"#,
        r#"{"kind":"delta_measured","body":null}"#,
        r#"{"kind":"delta_measured","body":{}}"#,
        r#"{"kind":"delta_measured","resident":127,"prefix_tokens":13,"keep":91,"input_tokens":42,"capacity":32768}"#,
    ] {
        assert!(
            serde_json::from_str::<TokenAnswer>(bytes).is_err(),
            "{bytes}"
        );
    }
    for bytes in [
        r#"{"kind":"append_and_generate","turn":"t","delta":[]}"#,
        r#"{"kind":"measure_delta","keep":0,"delta":[]}"#,
        r#"{"kind":"measure_delta","body":{"keep":0,"delta":[],"tool_schemas":[]}}"#,
        r#"{"measure_delta":{"keep":0,"delta":[],"tool_schemas":[]}}"#,
    ] {
        assert!(
            serde_json::from_str::<TokenDirective>(bytes).is_err(),
            "{bytes}"
        );
    }
}

#[test]
fn all_measurement_counts_keep_the_u64_number_domain() {
    for answer in [opened(), measured()] {
        let canonical = serde_json::to_value(&answer).unwrap();
        for (field, value) in canonical["body"].as_object().unwrap() {
            if !value.is_number() {
                continue;
            }
            for valid in [0, 1, u64::MAX] {
                let mut candidate = canonical.clone();
                candidate["body"][field] = json!(valid);
                assert!(
                    serde_json::from_str::<TokenAnswer>(&candidate.to_string()).is_ok(),
                    "u64 {field} remains numeric: {candidate}"
                );
            }
            for invalid in [json!(-1), json!(1.5), json!("1"), json!(true), json!([])] {
                let mut candidate = canonical.clone();
                candidate["body"][field] = invalid;
                assert!(
                    serde_json::from_str::<TokenAnswer>(&candidate.to_string()).is_err(),
                    "invalid u64 {field} must refuse: {candidate}"
                );
            }
            let mut overflow = canonical.clone();
            overflow["body"][field] = json!("wire-overflow");
            let overflow = overflow
                .to_string()
                .replace("\"wire-overflow\"", "18446744073709551616");
            assert!(
                serde_json::from_str::<TokenAnswer>(&overflow).is_err(),
                "{overflow}"
            );
        }
    }
    for invalid in ["-1", "1.5", "\"1\"", "true", "[]", "18446744073709551616"] {
        let bytes = format!(
            "{{\"kind\":\"measure_delta\",\"keep\":{invalid},\"delta\":[],\"tool_schemas\":[]}}"
        );
        assert!(
            serde_json::from_str::<TokenDirective>(&bytes).is_err(),
            "{bytes}"
        );
    }
}

#[test]
fn measurement_refusals_carry_the_exact_requested_keep() {
    for (refusal, kind) in [
        (TokenRefusal::NotOpen, "not_open"),
        (TokenRefusal::OutOfOrder, "out_of_order"),
        (TokenRefusal::MalformedDelta, "malformed_delta"),
    ] {
        let value = RefusalRecord::Decode {
            asked: TokenAsk::MeasureDelta { keep: u64::MAX },
            refusal,
        };
        let bytes = serde_json::to_string(&value).unwrap();
        assert_eq!(
            bytes,
            format!(
                concat!(
                    "{{\"seam\":\"decode\",\"asked\":{{\"ask\":\"measure_delta\",",
                    "\"keep\":18446744073709551615}},\"refusal\":{{\"kind\":\"{}\"}}}}"
                ),
                kind,
            )
        );
        assert_eq!(
            serde_json::from_str::<RefusalRecord>(&bytes).unwrap(),
            value
        );
    }
    for bytes in [
        r#"{"ask":"measure_delta"}"#,
        r#"{"ask":"measure_delta","keep":null}"#,
        r#"{"ask":"measure_delta","keep":"1"}"#,
        r#"{"ask":"measure_delta","keep":-1}"#,
        r#"{"ask":"measure_delta","keep":18446744073709551616}"#,
        r#"{"kind":"measure_delta","keep":0}"#,
    ] {
        assert!(serde_json::from_str::<TokenAsk>(bytes).is_err(), "{bytes}");
    }
}

#[test]
fn refeed_keeps_its_recorded_rendering_and_path_without_schema_member() {
    let directive = TokenDirective::ReFeed {
        turn: TurnKey("replay-1".into()),
        rendered: "recorded\nbytes".into(),
        path: vec![0, u32::MAX],
    };
    let bytes = serde_json::to_string(&directive).unwrap();
    assert_eq!(
        bytes,
        concat!(
            r#"{"kind":"re_feed","turn":"replay-1","rendered":"recorded\nbytes","#,
            r#""path":[0,4294967295]}"#,
        )
    );
    assert_eq!(
        serde_json::from_str::<TokenDirective>(&bytes).unwrap(),
        directive
    );
}
