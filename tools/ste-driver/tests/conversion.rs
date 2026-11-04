use serde_json::{Value, json};
use weaver_ste_driver::{
    check::Checker,
    conversion::{check_answer, result_json},
    gate::Capture,
    guard::Judge,
    lexicon::Lexicon,
    literal::LiteralCheck,
    policy::{RequestKind, Terminal, TextConversion},
};

fn judge() -> Judge {
    let cases: Value = serde_json::from_str(include_str!("language-reference.json")).unwrap();
    let lex =
        Lexicon::build(serde_json::from_value(cases["lexicons"][0].clone()).unwrap()).unwrap();
    Judge::new(Checker::new(lex, 20, 25).unwrap(), &[]).unwrap()
}

fn capture(text: Value, kind: &str) -> Capture {
    Capture {
        stdout: serde_json::to_vec(&json!({"kind":kind, "run":"r", "turn":"t", "text":text}))
            .unwrap(),
        stderr: Vec::new(),
        exit_code: Some(0),
        error: None,
    }
}

#[test]
fn language_then_shape_then_accept_shares_three_requests_and_keeps_all_evidence() {
    let judge = judge();
    let source = "Read [[C1]].";
    let mut conversion = TextConversion::new(source.into(), 2);
    for (text, kind) in [
        ("Read [[C1]]\x3b", RequestKind::Initial),
        ("Read the file.", RequestKind::Repair),
        ("Read [[C1]].", RequestKind::ShapeRetry),
    ] {
        let request = conversion.begin_request().unwrap();
        assert_eq!(request.kind, kind);
        if kind == RequestKind::Repair {
            assert!(
                request
                    .text
                    .contains("<<<PREVIOUS_REWRITE\nRead [[C1]]\x3b\n")
            );
            assert!(request.text.contains("- Do not use the semicolon (\x3b)."));
        }
        conversion
            .record_result(
                request.ordinal,
                check_answer(capture(json!(text), "answered"), &judge, source).unwrap(),
            )
            .unwrap();
    }
    assert_eq!(conversion.terminal(), Some(Terminal::Accepted));
    assert_eq!(conversion.requests_used(), 3);
    assert_eq!(conversion.shape_faults(), 1);
    let result = result_json("id", source, &conversion, &LiteralCheck::new().unwrap()).unwrap();
    assert_eq!(result["assembly_eligible"], true);
    assert_eq!(result["attempts"].as_array().unwrap().len(), 3);
    assert_eq!(
        result["attempts"][0]["result"]["response"]["check"]["hard"]["semicolon"],
        json!(["\x3b"])
    );
    assert_eq!(result["attempts"][2]["result"]["response"]["run"], "r");
    assert_eq!(
        result["attempts"][2]["result"]["response"]["recorded_generation_count"],
        Value::Null
    );
    assert!(conversion.begin_request().is_err());
}

#[test]
fn a_lifecycle_stop_keeps_earlier_candidates_only_in_attempts() {
    let judge = judge();
    let mut conversion = TextConversion::new("Read [[C1]].".into(), 2);
    for (text, kind) in [
        (json!("Read [[C1]]\x3b"), "answered"),
        (Value::Null, "stopped"),
    ] {
        let request = conversion.begin_request().unwrap();
        conversion
            .record_result(
                request.ordinal,
                check_answer(capture(text, kind), &judge, "Read [[C1]].").unwrap(),
            )
            .unwrap();
    }
    assert_eq!(conversion.terminal(), Some(Terminal::TransportStopped));
    assert!(conversion.final_output().is_none());
    assert!(conversion.begin_request().is_err());
    assert_eq!(conversion.attempts().len(), 2);
}

#[test]
fn completed_missing_and_lost_placeholders_drop_at_the_shared_limit() {
    let judge = judge();
    for text in [Value::Null, json!("\u{1c}\u{85}"), json!("Read.")] {
        let mut conversion = TextConversion::new("Read [[C1]].".into(), 2);
        for _ in 0..3 {
            let request = conversion.begin_request().unwrap();
            conversion
                .record_result(
                    request.ordinal,
                    check_answer(capture(text.clone(), "answered"), &judge, "Read [[C1]].")
                        .unwrap(),
                )
                .unwrap();
        }
        assert_eq!(conversion.terminal(), Some(Terminal::ShapeDrop));
        assert_eq!(conversion.shape_faults(), 3);
        assert!(conversion.begin_request().is_err());
    }
}

#[test]
fn extra_or_repeated_placeholders_get_notes_and_count_as_language_loss() {
    let judge = judge();
    for (source, output) in [
        ("Read [[C1]].", "Read [[C1]] [[C1]]."),
        ("Read [[C1]].", "Read [[C1]] [[C2]]."),
        ("Read the file.", "Read [[C1]]."),
    ] {
        let mut conversion = TextConversion::new(source.into(), 2);
        for ordinal in 1..=3 {
            let request = conversion.begin_request().unwrap();
            assert_eq!(
                request.kind,
                if ordinal == 1 {
                    RequestKind::Initial
                } else {
                    RequestKind::Repair
                }
            );
            if ordinal > 1 {
                assert!(
                    request
                        .text
                        .contains("These placeholders occur too many times")
                );
                assert!(
                    request
                        .text
                        .contains(&format!("<<<PREVIOUS_REWRITE\n{output}\n"))
                );
            }
            conversion
                .record_result(
                    request.ordinal,
                    check_answer(capture(json!(output), "answered"), &judge, source).unwrap(),
                )
                .unwrap();
        }
        assert_eq!(conversion.terminal(), Some(Terminal::LanguageUnresolved));
        assert_eq!(conversion.shape_faults(), 0);
        assert_eq!(conversion.final_output(), Some(output));
    }
}

#[test]
fn a_lost_occurrence_is_a_shape_fault_even_with_an_extra_marker() {
    let judge = judge();
    for (source, output) in [
        ("Read [[C1]] [[C1]].", "Read [[C1]]."),
        ("Read [[C1]] [[C1]].", "Read [[C1]] [[C2]]."),
        ("Read [[C1]].", "Read [[C2]]."),
    ] {
        let mut conversion = TextConversion::new(source.into(), 2);
        for ordinal in 1..=3 {
            let request = conversion.begin_request().unwrap();
            assert_eq!(
                request.kind,
                if ordinal == 1 {
                    RequestKind::Initial
                } else {
                    RequestKind::ShapeRetry
                }
            );
            assert!(!request.text.contains("<<<PREVIOUS_REWRITE"));
            conversion
                .record_result(
                    request.ordinal,
                    check_answer(capture(json!(output), "answered"), &judge, source).unwrap(),
                )
                .unwrap();
        }
        assert_eq!(conversion.terminal(), Some(Terminal::ShapeDrop));
        assert_eq!(conversion.shape_faults(), 3);
    }
}

#[test]
fn truncation_is_a_language_failure_and_changed_literals_refuse_assembly() {
    let judge = judge();
    let mut truncated = capture(json!("Read the file."), "answered");
    let mut value: Value = serde_json::from_slice(&truncated.stdout).unwrap();
    value["finish"] = json!("length");
    truncated.stdout = serde_json::to_vec(&value).unwrap();
    let result = check_answer(truncated, &judge, "Read the file.").unwrap();
    let mut conversion = TextConversion::new("Read the file.".into(), 0);
    let request = conversion.begin_request().unwrap();
    conversion.record_result(request.ordinal, result).unwrap();
    assert_eq!(conversion.terminal(), Some(Terminal::LanguageUnresolved));

    let source = "Read 145.[[C1]].";
    let mut conversion = TextConversion::new(source.into(), 2);
    let request = conversion.begin_request().unwrap();
    conversion
        .record_result(
            request.ordinal,
            check_answer(
                capture(json!("Read 146.[[C1]]."), "answered"),
                &judge,
                source,
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(conversion.terminal(), Some(Terminal::Accepted));
    let result = result_json(
        "literal",
        source,
        &conversion,
        &LiteralCheck::new().unwrap(),
    )
    .unwrap();
    assert_eq!(result["assembly_eligible"], false);
    assert_eq!(result["altered_literals"], json!(["145.[[C1]]"]));
    assert_eq!(conversion.requests_used(), 1);
}
