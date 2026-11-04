use super::*;
use sha2::{Digest as _, Sha256};

fn message(role: Role, content: &str) -> Message {
    Message {
        role,
        content: vec![ContentBlock::Text {
            text: content.into(),
        }],
    }
}

#[test]
fn nemotron_render_matches_the_pinned_template_with_thinking_disabled() {
    let goldens: serde_json::Value = serde_json::from_str(include_str!("goldens.json")).unwrap();
    assert_eq!(goldens["source_revision"], SOURCE_REVISION);
    assert_eq!(goldens["template_sha256"], TEMPLATE_SHA256);
    let cases = goldens["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 16);
    for case in cases {
        let messages: Vec<_> = case["messages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|input| {
                message(
                    match input["role"].as_str().unwrap() {
                        "system" => Role::System,
                        "user" => Role::User,
                        "assistant" => Role::Assistant,
                        other => panic!("unexpected fixture role {other}"),
                    },
                    input["content"].as_str().unwrap(),
                )
            })
            .collect();
        let original = messages.clone();
        let mut rendered = renderer().render_identity(&messages).unwrap();
        if case["generation"].as_bool().unwrap() {
            rendered.push_str(GENERATION_OPENER);
        }
        assert_eq!(
            rendered,
            case["rendered"].as_str().unwrap(),
            "{}",
            case["name"]
        );
        assert_eq!(messages, original, "input messages must remain unchanged");
    }
}

#[test]
fn nemotron_folded_prompt_boot_matches_the_measured_reference_hash() {
    let prompt = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/crates/weaver-harness/Loops/ste-conversion-prompt.txt"
    ));
    assert_eq!(
        format!("{:x}", Sha256::digest(prompt.as_bytes())),
        "0fe2ee35a59f6b1e30734af4d7a72e32bd6dbc0e728abc98e32e2c609dad07ed"
    );
    let rendered = renderer()
        .render_identity(&[message(Role::System, prompt)])
        .unwrap();
    assert_eq!(rendered.len(), 47_244);
    assert_eq!(
        format!("{:x}", Sha256::digest(rendered.as_bytes())),
        "e9cbd70b8b36420271473bc0cbc822632d0e3cdd960cb1a7b1c00181e9f12627"
    );
    for (request, expected) in [
        (
            "<<<TEXT\nRead the file.\nTEXT>>>",
            "e55446ee5796bcbac523cf37ab0362e43502d2c231e5d2ca4868712c9d1cb604",
        ),
        (
            concat!(
                "<<<TEXT\nRead the file.\nTEXT>>>\n\n",
                "<<<PREVIOUS_REWRITE\nRead the file.\nPREVIOUS_REWRITE>>>\n\n",
                "Your rewrite breaks these rules:\n- Obey the rules.\n\n",
                "Write the complete rewrite again with these problems corrected. ",
                "Keep the meaning. Output only the rewritten text."
            ),
            "2c61f621f3dd866d7987f522397335c61b99598c8ce392efc5b49df34adfedcb",
        ),
    ] {
        let delta = crate::family::render_generation(
            renderer(),
            &[message(Role::User, request)],
            &[],
            GENERATION_OPENER,
        )
        .unwrap();
        assert_eq!(
            format!(
                "{:x}",
                Sha256::digest(format!("{rendered}{delta}").as_bytes())
            ),
            expected
        );
    }
}

#[test]
fn nemotron_entry_keeps_its_opener_stop_set_and_hybrid_flush() {
    let entry = renderer().declaration();
    assert_eq!(entry.family, "nemotron_h");
    assert_eq!(entry.generation_opener, GENERATION_OPENER);
    assert_eq!(entry.selecting_markers, RENDERED_MARKERS);
    assert_eq!(renderer().stop_conditions(), &[TURN_CLOSE]);
    assert_eq!(
        entry.flush,
        crate::decoder::backend::FlushMechanism::ReestablishAndReprefill
    );
    assert!(!entry.permits_truncation());
    assert!(entry.shards_across(1));
    assert!(!entry.shards_across(3));
}

#[test]
fn nemotron_unsupported_calls_remain_unrecovered_and_cannot_turn_into_text() {
    let fragment = "<tool_call>\n<function=read>\n</function>\n</tool_call>";
    let parsed = renderer().parse(&format!("Before.{fragment}After."), None);
    assert_eq!(parsed.text(), "Before.After.");
    assert_eq!(parsed.unrecovered.len(), 1);
    assert_eq!(parsed.unrecovered[0].fragment, fragment);
    assert_eq!(parsed.unrecovered[0].grammar.family, "nemotron_h");
    assert!(
        renderer()
            .render_delta(&Message {
                role: Role::Assistant,
                content: vec![ContentBlock::ToolCall(weaver_traits::ToolCall {
                    name: "read".into(),
                    arguments: "{}".into(),
                })],
            })
            .is_err()
    );
    assert!(
        renderer()
            .render_delta(&message(Role::ToolResult, "Result"))
            .is_err()
    );
}
