use super::*;
use weaver_traits::Message;

fn pair(key: &str, value: &str) -> (String, String) {
    (key.to_string(), value.to_string())
}

/// A message's role and content land typed, one part row per block, and
/// render back to the bytes the floor type spells.
#[test]
fn a_message_lands_typed_and_renders_back_exactly() {
    let message = Message {
        role: Role::Assistant,
        content: vec![
            ContentBlock::Text {
                text: "four \"quoted\" words\n".into(),
            },
            ContentBlock::ToolCall(ToolCall {
                name: "calculator".into(),
                arguments: r#"{"a":1}"#.into(),
            }),
            ContentBlock::ToolResult(ToolResultBlock {
                content: "2".into(),
            }),
        ],
    };
    let role = serde_json::to_string(&message.role).unwrap();
    let content = serde_json::to_string(&message.content).unwrap();
    let pairs = vec![pair("content", &content), pair("role", &role)];
    let (typed, verbatim) = split("message.assistant", &pairs);
    assert!(verbatim.is_empty(), "{verbatim:?}");
    assert_eq!(
        typed.message.as_ref().unwrap().role.as_deref(),
        Some("assistant")
    );
    assert_eq!(typed.parts.len(), 3);
    assert_eq!(typed.parts[1].name.as_deref(), Some("calculator"));
    assert_eq!(render(&typed).unwrap(), pairs);
}

/// An empty content is a member holding no block, not an absent member.
#[test]
fn an_empty_content_is_held_rather_than_absent() {
    let (typed, verbatim) = split("message.user", &[pair("content", "[]")]);
    assert!(verbatim.is_empty());
    assert_eq!(typed.message.as_ref().unwrap().parts, Some(0));
    assert_eq!(render(&typed).unwrap(), [pair("content", "[]")]);
}

/// A value the vocabulary does not name, or names and cannot type
/// exactly, lands verbatim: a non-message kind's content, a content that
/// is not the floor's shape, and a spelling the floor type would change.
#[test]
fn what_cannot_land_typed_exactly_lands_verbatim() {
    for (kind, key, value) in [
        ("model.output", "content", "[]"),
        ("message.user", "content", r#"{"z": 1.00}"#),
        ("message.user", "content", r#"[{"text":"a","type":"text"}]"#),
        (
            "message.user",
            "content",
            r#"[ {"type":"text","text":"a"} ]"#,
        ),
        ("message.user", "role", r#""narrator""#),
        ("message.user", "turn_note", r#""x""#),
        ("model.measurement", "perplexity", "1.50"),
        ("model.measurement", "entropies", "[1e0]"),
        ("model.measurement", "input_tokens", "[1,2]"),
    ] {
        let pairs = [pair(key, value)];
        let (typed, verbatim) = split(kind, &pairs);
        assert!(typed.is_empty(), "{kind} {key} {value} typed");
        assert_eq!(verbatim, pairs, "{kind} {key} {value}");
    }
}

/// **The measurement's absent readings stay absent**: a measurement that
/// crossed with a perplexity and no series holds no series length and
/// renders no series member, and one with no perplexity holds none
/// rather than zero.
#[test]
fn an_absent_reading_is_absent_and_not_zero() {
    let (typed, _) = split(
        "model.measurement",
        &[pair("perplexity", "1.503386244415742")],
    );
    let row = typed.measurement.as_ref().unwrap();
    assert_eq!(row.perplexity, Some(1.503386244415742));
    assert_eq!((row.entropies, row.surprisals), (None, None));
    assert_eq!(
        render(&typed).unwrap(),
        [pair("perplexity", "1.503386244415742")]
    );

    let pairs = [pair("entropies", "[0.5,1.25]")];
    let (typed, verbatim) = split("model.measurement", &pairs);
    assert!(verbatim.is_empty());
    let row = typed.measurement.as_ref().unwrap();
    assert_eq!(row.perplexity, None, "an absent perplexity is not zero");
    assert_eq!(row.entropies, Some(2));
    assert_eq!(render(&typed).unwrap(), pairs);
}

/// **A string carrying U+0000 lands verbatim**, because the service
/// engine's `TEXT` cannot hold it, and the rule is one rule for both
/// engines. The escaped JSON holds it, so custody keeps the pair whole.
#[test]
fn a_nul_in_a_string_lands_verbatim() {
    for (key, value) in [
        ("content", r#"[{"type":"text","text":"a\u0000b"}]"#),
        (
            "content",
            r#"[{"type":"tool_call","name":"n\u0000","arguments":"{}"}]"#,
        ),
        ("content", r#"[{"type":"tool_result","content":"\u0000"}]"#),
    ] {
        let pairs = [pair(key, value)];
        let (typed, verbatim) = split("message.user", &pairs);
        assert!(typed.is_empty(), "{value} typed");
        assert_eq!(verbatim, pairs, "{value}");
    }
}

/// **A reading the SPU rendered lands typed and renders back exactly**,
/// which holds only because the manifest turns on serde_json's
/// `float_roundtrip`. The value is a perplexity from the HeroBench run of
/// 2026-09-29, one of 780 readings that landed verbatim under the default
/// parse, which reads it as a neighbouring double that renders as
/// `1.4226275797516237`. Perturbation: drop the feature and this reading
/// and the series beside it land verbatim, and the test fails on typed.
#[test]
fn a_rendered_reading_lands_typed_and_renders_back() {
    let pairs = [
        pair("perplexity", "1.4226275797516235"),
        pair("entropies", "[1.4226275797516235,0.000011356166396581102]"),
    ];
    let (typed, verbatim) = split("model.measurement", &pairs);
    assert!(verbatim.is_empty(), "landed verbatim: {verbatim:?}");
    let row = typed.measurement.as_ref().expect("typed");
    assert_eq!(row.perplexity, Some(1.4226275797516235));
    assert_eq!(row.entropies, Some(2));
    let mut served = render(&typed).unwrap();
    served.sort();
    let mut sent = pairs.to_vec();
    sent.sort();
    assert_eq!(served, sent);
}

/// Rows missing from their count are refused at read rather than served
/// short, the one error the renderer has.
#[test]
fn a_count_its_rows_do_not_meet_is_refused() {
    let (mut typed, _) = split(
        "message.user",
        &[pair("content", r#"[{"type":"text","text":"a"}]"#)],
    );
    typed.parts.clear();
    assert!(render(&typed).is_err());
}
