use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use weaver_ste_driver::{
    check::{CheckResult, Checker},
    guard::Judge,
    lexicon::{Lexicon, LexiconInput},
};

#[test]
fn checks_and_notes_match_the_pinned_source_tests() {
    let fixture: Value = serde_json::from_str(include_str!("language-reference.json")).unwrap();
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 75);
    let mut checkers = BTreeMap::new();
    for case in fixture["cases"].as_array().unwrap() {
        let key = (
            case["lexicon"].as_u64().unwrap() as usize,
            case["limits"][0].as_u64().unwrap() as usize,
            case["limits"][1].as_u64().unwrap() as usize,
        );
        let checker = checkers.entry(key).or_insert_with(|| {
            let input = serde_json::from_value(fixture["lexicons"][key.0].clone()).unwrap();
            Checker::new(Lexicon::build(input).unwrap(), key.1, key.2).unwrap()
        });
        let output = case["output"].as_str().unwrap();
        let source = case["source"].as_str().unwrap();
        let names: BTreeSet<String> = serde_json::from_value(case["names"].clone()).unwrap();
        let result = checker.check(output, source, &names).unwrap();
        assert_eq!(result.to_json(), case["result"], "{}", case["name"]);
        assert_eq!(
            checker.repair_note(&result),
            case["note"].as_str().unwrap(),
            "{}",
            case["name"]
        );
        assert_eq!(
            serde_json::to_value(checker.sentences(output).unwrap()).unwrap(),
            case["sentences"]
        );
        assert_eq!(
            checker.count_words(output).unwrap() as u64,
            case["count_words"].as_u64().unwrap()
        );
    }
}

#[test]
fn guards_and_complete_repair_notes_match_the_pinned_reference() {
    let fixture: Value = serde_json::from_str(include_str!("language-reference.json")).unwrap();
    assert_eq!(fixture["guards"].as_array().unwrap().len(), 11);
    for (index, case) in fixture["guards"].as_array().unwrap().iter().enumerate() {
        let input: LexiconInput = serde_json::from_value(
            fixture["lexicons"][case["lexicon"].as_u64().unwrap() as usize].clone(),
        )
        .unwrap();
        let checker = Checker::new(Lexicon::build(input).unwrap(), 20, 25).unwrap();
        let examples: Vec<String> = serde_json::from_value(case["examples"].clone()).unwrap();
        let judge = Judge::new(checker, &examples).unwrap();
        let result = judge
            .check(
                case["output"].as_str(),
                case["source"].as_str().unwrap(),
                &BTreeSet::new(),
            )
            .unwrap();
        assert_eq!(result.to_json(), case["result"], "case {index}");
        assert_eq!(
            judge.checker.repair_notes_with_guards(&result),
            case["notes"].as_str().unwrap(),
            "case {index}"
        );
    }
}

fn checker() -> Checker {
    let fixture: Value = serde_json::from_str(include_str!("language-reference.json")).unwrap();
    let input = serde_json::from_value(fixture["lexicons"][0].clone()).unwrap();
    Checker::new(Lexicon::build(input).unwrap(), 20, 25).unwrap()
}

#[test]
fn ratio_guard_keeps_exact_boundaries_and_the_short_input_exception() {
    let judge = Judge::new(checker(), &[]).unwrap();
    for (input_count, output_count, failed) in [
        (8, 4, false),
        (8, 20, false),
        (8, 3, true),
        (8, 21, true),
        (7, 0, false),
    ] {
        let source = vec!["word"; input_count].join(" ");
        let output = vec!["word"; output_count].join(" ");
        let result = judge
            .check(Some(&output), &source, &BTreeSet::new())
            .unwrap();
        assert_eq!(result.hard.contains_key("word_ratio"), failed);
    }
}

#[test]
fn soft_vocabulary_findings_do_not_fail_the_check() {
    let result = checker()
        .check("Now I will examine the zebra.", "text", &BTreeSet::new())
        .unwrap();
    assert_eq!(result.soft["unknown_words"], [serde_json::json!("zebra")]);
    assert_eq!(result.soft["unapproved_words"], [serde_json::json!("now")]);
    assert!(result.ok());
}

#[test]
fn empty_guard_notes_have_the_ruled_fallback() {
    assert_eq!(
        checker().repair_notes_with_guards(&CheckResult::default()),
        "- Obey the rules."
    );
}

#[test]
fn literal_repair_note_keeps_the_pinned_bytes() {
    let mut result = CheckResult::default();
    result
        .hard
        .insert("altered_literals", vec![serde_json::json!("1.[[C3]]")]);
    assert_eq!(
        checker().repair_note(&result),
        concat!(
            "- Your rewrite changed text that is joined to code or a name. Keep this text ",
            "exactly as it is in the input: 1.[[C3]]."
        )
    );
}
