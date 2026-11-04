use serde_json::Value;
use weaver_ste_driver::literal::LiteralCheck;
use weaver_ste_driver::mask::{Masker, unmask};

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn masking_and_restoration_match_the_pinned_reference() {
    let fixture: Value = serde_json::from_str(include_str!("reference.json")).unwrap();
    let masker = Masker::new().unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 76);
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let got = masker.mask(case["source"].as_str().unwrap()).unwrap();
        assert_eq!(got.text, case["masked"].as_str().unwrap(), "{name}");
        assert_eq!(got.codes, strings(&case["codes"]), "{name}");
        assert_eq!(unmask(&got.text, &got.codes), case["restored"], "{name}");
    }
}

#[test]
fn literal_checks_match_the_pinned_reference() {
    let fixture: Value = serde_json::from_str(include_str!("reference.json")).unwrap();
    let check = LiteralCheck::new().unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let source = case["source"].as_str().unwrap();
        let output = case["masked"].as_str();
        assert_eq!(
            check.glued(source).unwrap(),
            strings(&case["glued"]),
            "{name}"
        );
        assert_eq!(
            check.altered(output, source).unwrap(),
            strings(&case["altered"]),
            "{name}"
        );
        assert_eq!(
            check.altered(None, source).unwrap(),
            strings(&case["altered_none"]),
            "{name}"
        );
        assert!(
            check.altered(Some(source), source).unwrap().is_empty(),
            "{name}"
        );
    }
}

#[test]
fn nested_spans_restore_the_exact_source_bytes() {
    let source = "Before\r\nUse d[`k`] = [1, [2]].\r\nAfter\n";
    let masked = Masker::new().unwrap().mask(source).unwrap();
    assert_eq!(masked.text, "Before\r\nUse [[C2]] = [[C3]].\r\nAfter\n");
    assert_eq!(masked.codes, ["`k`", "d[[[C1]]]", "[1, [2]]"]);
    assert_eq!(unmask(&masked.text, &masked.codes), source);
}

#[test]
fn literal_changes_refuse_assembly_but_slash_expansion_does_not() {
    let check = LiteralCheck::new().unwrap();
    let source = "Keep 145.[[C8]] and [[C4]]([[C5]]. Use [[C1]]/[[C2]].";
    let output = "Keep 145.[[C8]] and [[C4]]([[C5]]. Use [[C1]] and [[C2]].";
    assert!(check.altered(Some(output), source).unwrap().is_empty());
    let output = "Keep 145 [[C8]] and [[C4]] [[C5]]. Use [[C1]] and [[C2]].";
    assert_eq!(
        check.altered(Some(output), source).unwrap(),
        ["145.[[C8]]", "[[C4]]([[C5]]"]
    );
}
