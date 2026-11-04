use super::*;
use serde_json::json;

fn checked(output: &str, failures: &[&str], notes: &str) -> AttemptResult {
    AttemptResult::Checked {
        output: output.into(),
        failures: failures.iter().map(|s| (*s).into()).collect(),
        repair_notes: notes.into(),
        response: json!({"run_id":"run-a", "turn":4, "finish":"stop"}),
    }
}

fn shape(output: Option<&str>) -> AttemptResult {
    AttemptResult::ShapeFault {
        output: output.map(str::to_owned),
        failures: vec!["placeholder".into()],
        response: json!({"run_id":"run-a", "turn":5}),
    }
}

fn stopped() -> AttemptResult {
    AttemptResult::TransportStopped {
        detail: "gate closed before the answer".into(),
        response: json!({"run_id":"run-a", "turn":6}),
    }
}

fn answer(text: &mut TextConversion, result: AttemptResult) -> Request {
    let request = text.begin_request().unwrap();
    text.record_result(request.ordinal, result).unwrap();
    request
}

// Workflow 4.05 and 6.08: the exact layouts preserve payload bytes.
#[test]
fn layouts_preserve_source_candidate_and_newlines_without_a_final_lf() {
    let source = "\nSource [[C0]] \u{b0}\n";
    let previous = "\nLast Y [[C0]]\n";
    let mut text = TextConversion::new(source.into(), 2);
    let first = answer(
        &mut text,
        checked(previous, &["verbs"], " \n- Use the verb.\n "),
    );
    assert_eq!(first.ordinal, 1);
    assert_eq!(first.kind, RequestKind::Initial);
    assert_eq!(first.text, format!("<<<TEXT\n{source}\nTEXT>>>"));
    let repair = text.begin_request().unwrap();
    assert_eq!(repair.kind, RequestKind::Repair);
    assert_eq!(
        repair.text,
        format!(
            "<<<TEXT\n{source}\nTEXT>>>\n\n<<<PREVIOUS_REWRITE\n{previous}\nPREVIOUS_REWRITE>>>\n\nYour rewrite breaks these rules:\n- Use the verb.\n\nWrite the complete rewrite again with these problems corrected. Keep the meaning. Output only the rewritten text."
        )
    );
    assert!(!repair.text.ends_with('\n'));
}

#[test]
fn empty_notes_use_the_ruled_fallback() {
    let mut text = TextConversion::new("Source".into(), 2);
    answer(&mut text, checked("Last Y", &["word_ratio"], " \n\t"));
    assert!(
        text.begin_request()
            .unwrap()
            .text
            .contains("\n- Obey the rules.\n")
    );
}

// Workflow 6.03 and 6.07: all request kinds spend one shared allowance.
#[test]
fn language_shape_language_exhausts_one_budget_and_keeps_the_last_y() {
    let mut text = TextConversion::new("Source [[C0]]".into(), 2);
    let initial = answer(&mut text, checked("First Y [[C0]]", &["verbs"], "- Verb."));
    let repair = answer(&mut text, shape(Some("Malformed Y")));
    let retry = answer(&mut text, checked("Last Y [[C0]]", &["ing"], "- Ing."));
    assert_eq!(repair.kind, RequestKind::Repair);
    assert_eq!(retry.kind, RequestKind::ShapeRetry);
    assert_eq!(retry.text, initial.text);
    assert_eq!(text.requests_used(), 3);
    assert_eq!(text.shape_faults(), 1);
    assert_eq!(text.terminal(), Some(Terminal::LanguageUnresolved));
    assert_eq!(text.final_output(), Some("Last Y [[C0]]"));
    assert_eq!(text.begin_request(), Err(ConversionError::Finished));
    assert_eq!(text.attempts().len(), 3);
    assert_eq!(text.attempts()[1].request.text, repair.text);
}

#[test]
fn a_final_shape_fault_drops_even_with_only_one_strike() {
    let mut text = TextConversion::new("Source".into(), 2);
    answer(&mut text, checked("First", &["verbs"], "- One."));
    answer(&mut text, checked("Second", &["verbs"], "- Two."));
    answer(&mut text, shape(Some("Last invalid text")));
    assert_eq!(text.terminal(), Some(Terminal::ShapeDrop));
    assert_eq!(text.shape_faults(), 1);
    assert_eq!(text.final_output(), Some("Last invalid text"));
    assert!(!text.assembly_eligible());
}

#[test]
fn three_shape_faults_drop_even_with_more_request_capacity() {
    let mut text = TextConversion::new("Source".into(), 8);
    for _ in 0..3 {
        answer(&mut text, shape(None));
    }
    assert_eq!(text.requests_used(), 3);
    assert_eq!(text.shape_faults(), 3);
    assert_eq!(text.terminal(), Some(Terminal::ShapeDrop));
    assert_eq!(text.final_output(), None);
    assert_eq!(text.begin_request(), Err(ConversionError::Finished));
}

#[test]
fn an_empty_completed_answer_is_a_shape_fault() {
    for output in ["", " \n\t"] {
        let mut text = TextConversion::new("Source".into(), 0);
        answer(&mut text, checked(output, &[], ""));
        assert_eq!(text.terminal(), Some(Terminal::ShapeDrop));
        assert_eq!(text.shape_faults(), 1);
    }
}

#[test]
fn accepted_output_finishes_without_retry_or_an_extra_shape_strike() {
    let mut text = TextConversion::new("Source".into(), 2);
    answer(&mut text, shape(None));
    answer(&mut text, checked("Complete Y", &[], ""));
    assert_eq!(text.terminal(), Some(Terminal::Accepted));
    assert_eq!(text.requests_used(), 2);
    assert_eq!(text.shape_faults(), 1);
    assert!(text.assembly_eligible());
    assert_eq!(
        text.scorer_output("segment-a"),
        Some(json!({"id":"segment-a", "output":"Complete Y"}))
    );
    assert_eq!(text.begin_request(), Err(ConversionError::Finished));
}

// Workflow 6.03 and 7.07: a stopped text never promotes an earlier answer.
#[test]
fn transport_stop_keeps_evidence_but_never_an_earlier_output_or_retry() {
    for earlier in [
        shape(Some("Bad shape")),
        checked("Earlier Y", &["verbs"], "- Verb."),
    ] {
        let mut text = TextConversion::new("Source".into(), 2);
        answer(&mut text, earlier.clone());
        answer(&mut text, stopped());
        assert_eq!(text.terminal(), Some(Terminal::TransportStopped));
        assert_eq!(text.attempts()[0].result, earlier);
        assert_eq!(text.attempts().len(), 2);
        assert_eq!(text.final_output(), None);
        assert_eq!(text.scorer_output("segment-a"), None);
        assert_eq!(text.begin_request(), Err(ConversionError::Finished));
    }
}

#[test]
fn requests_are_counted_before_send_and_only_the_pending_request_can_complete() {
    let mut text = TextConversion::new("Source".into(), 2);
    assert_eq!(
        text.record_result(1, stopped()),
        Err(ConversionError::NoPendingRequest)
    );
    let request = text.begin_request().unwrap();
    assert_eq!(text.requests_used(), 1);
    assert_eq!(text.begin_request(), Err(ConversionError::RequestPending));
    assert_eq!(
        text.record_result(2, stopped()),
        Err(ConversionError::WrongRequest)
    );
    assert_eq!(text.pending_request(), Some(&request));
    assert_eq!(text.requests_used(), 1);
    text.record_result(1, checked("Y", &[], "")).unwrap();
    assert_eq!(
        text.record_result(1, stopped()),
        Err(ConversionError::Finished)
    );
    assert_eq!(text.final_output(), Some("Y"));
}

#[test]
fn request_limit_addition_does_not_wrap_at_the_largest_explicit_cap() {
    let text = TextConversion::new("Source".into(), u32::MAX);
    assert_eq!(text.request_limit(), u64::from(u32::MAX) + 1);
}

// Workflow 7.04, 7.05 and 7.07: exact counts, the strict bound, and no verdict.
#[test]
fn round_counts_stops_outside_loss_and_retains_never_attempted_ids() {
    let mut accepted = TextConversion::new("A".into(), 0);
    answer(&mut accepted, checked("Y", &[], ""));
    let mut drop = TextConversion::new("B".into(), 0);
    answer(&mut drop, shape(None));
    let mut unresolved = TextConversion::new("C".into(), 0);
    answer(&mut unresolved, checked("Bad Y", &["verbs"], "- Verb."));
    let mut stop = TextConversion::new("D".into(), 0);
    answer(&mut stop, stopped());
    let planned = ["a", "b", "c", "d", "e"];
    let results = [
        ("a", &accepted),
        ("b", &drop),
        ("c", &unresolved),
        ("d", &stop),
    ];
    let report = RoundReport::collect(&planned, &results, 0).unwrap();
    assert_eq!(report.attempted_text_count, 4);
    assert_eq!(report.accepted_count, 1);
    assert_eq!(report.shape_drop_count, 1);
    assert_eq!(report.language_unresolved_count, 1);
    assert_eq!(report.transport_stopped_count, 1);
    assert_eq!(report.conversion_loss_count, 2);
    assert_eq!(report.conversion_loss_rate, Some(0.5));
    assert_eq!(report.never_attempted, ["e"]);
    assert!(report.void);
    assert!(!report.r0_score_allowed());
    let at_bound = RoundReport::collect(&planned, &results, 1).unwrap();
    assert!(!at_bound.void);
    assert!(!at_bound.r0_score_allowed());
    assert_eq!(
        at_bound.to_json()["kill_line_verdict"],
        serde_json::Value::Null
    );
}

#[test]
fn score_requires_a_complete_nonvoid_evaluation_and_zero_has_no_rate() {
    let empty = RoundReport::collect(&["a"], &[], 0).unwrap();
    assert_eq!(empty.conversion_loss_rate, None);
    assert_eq!(empty.never_attempted, ["a"]);
    assert!(!empty.r0_score_allowed());
    let mut text = TextConversion::new("A".into(), 0);
    answer(&mut text, checked("Unresolved", &["verbs"], "- Verb."));
    let report = RoundReport::collect(&["a"], &[("a", &text)], 0).unwrap();
    assert!(report.r0_score_allowed());
    assert_eq!(report.conversion_loss_rate, Some(1.0));
    assert_eq!(
        report.to_json()["kill_line_verdict"],
        serde_json::Value::Null
    );
}

#[test]
fn round_refuses_duplicate_unknown_and_unfinished_results() {
    let mut text = TextConversion::new("A".into(), 0);
    assert_eq!(
        RoundReport::collect(&["a"], &[("a", &text)], 0),
        Err(ConversionError::Unfinished)
    );
    answer(&mut text, checked("Y", &[], ""));
    for (planned, results, expected) in [
        (vec!["a", "a"], vec![], ConversionError::DuplicateId),
        (vec!["a"], vec![("b", &text)], ConversionError::UnknownId),
        (
            vec!["a"],
            vec![("a", &text), ("a", &text)],
            ConversionError::DuplicateId,
        ),
    ] {
        assert_eq!(RoundReport::collect(&planned, &results, 0), Err(expected));
    }
}
