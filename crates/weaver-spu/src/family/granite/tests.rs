use super::*;
use crate::decoder::backend::FlushMechanism;
use crate::family::{ContentBlock, Role, render_generation, render_template, select};

fn message(role: Role, content: &str) -> Message {
    Message {
        role,
        content: vec![ContentBlock::Text {
            text: content.into(),
        }],
    }
}

/// The goldens' messages as canonical messages, `tool` as a tool result.
fn messages_of(case: &serde_json::Value) -> Vec<Message> {
    case["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|input| {
            let content = input["content"].as_str().unwrap();
            match input["role"].as_str().unwrap() {
                "system" => message(Role::System, content),
                "user" => message(Role::User, content),
                "assistant" => message(Role::Assistant, content),
                "tool" => Message {
                    role: Role::ToolResult,
                    content: vec![ContentBlock::ToolResult(weaver_traits::ToolResultBlock {
                        content: content.into(),
                    })],
                },
                other => panic!("unexpected fixture role {other}"),
            }
        })
        .collect()
}

/// The pinned template's rendering of one named case, and its messages. The
/// caller states whether the case ends at the generation opener.
fn golden_with(name: &str, generation: bool) -> (Vec<Message>, String) {
    let goldens: serde_json::Value = serde_json::from_str(include_str!("goldens.json")).unwrap();
    assert_eq!(goldens["source_revision"], SOURCE_REVISION);
    assert_eq!(goldens["template_sha256"], TEMPLATE_SHA256);
    let cases = goldens["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 6, "a case added to regenerate.py needs a test");
    let case = cases
        .iter()
        .find(|case| case["name"] == name)
        .unwrap_or_else(|| panic!("goldens.json has no case {name}"));
    assert_eq!(case["generation"].as_bool().unwrap(), generation, "{name}");
    (
        messages_of(case),
        case["rendered"].as_str().unwrap().to_owned(),
    )
}

/// A case that ends at the generation opener.
fn golden(name: &str) -> (Vec<Message>, String) {
    golden_with(name, true)
}

/// What this module renders for a whole sequence, with the opener.
fn rendered(messages: &[Message]) -> Result<String, RenderRefusal> {
    let mut rendered = renderer().render_identity(messages)?;
    rendered.push_str(GENERATION_OPENER);
    Ok(rendered)
}

/// **The R1 layout renders as the pinned template renders it**, which is the
/// claim that lets this module call qwen2's renderer for its system and user
/// turns. It is read both as one sequence and as the SPU renders it: the system turn as the identity prefix
/// at the open, then the user turn and the opener as the delta at the append.
///
/// Perturbation: render with `qwen2::GENERATION_OPENER` and both comparisons
/// fail at the missing thought.
#[test]
fn the_r1_layout_matches_the_pinned_template_with_thinking_off() {
    let (messages, expected) = golden("system_user");
    assert_eq!(rendered(&messages).unwrap(), expected);

    let prefix = renderer().render_identity(&messages[..1]).unwrap();
    let delta = render_generation(renderer(), &messages[1..], &[], GENERATION_OPENER).unwrap();
    assert_eq!(format!("{prefix}{delta}"), expected);
}

/// **Named limit: no empty system turn is inserted.** The pinned template
/// writes `<|im_start|>system\n<|im_end|>\n` when no system message leads, and
/// this module writes only the turns the messages carry. The rendering differs
/// from the template's by exactly that turn.
///
/// Perturbation: insert the empty system turn in `render_identity` and the
/// first assertion fails, which is the signal to retire this limit.
#[test]
fn limit_no_empty_system_turn_is_inserted() {
    let (messages, expected) = golden("user_only");
    let ours = rendered(&messages).unwrap();
    assert_ne!(ours, expected);
    assert_eq!(
        format!("{}{ours}", render_template(qwen2::TEMPLATE, "system", "")),
        expected
    );
}

/// **An assistant turn carries the empty thought the opener gave it**, per
/// Spec section 5: a turn rebuilt from its message gives the model what it
/// had. Live, the cache holds the opener, the emission and the stop that
/// ended it, and the rebuilt turn opens with exactly those bytes. For text
/// with no thought marker, both of the template's assistant branches agree:
/// the turn before the last user turn and the turn after it.
///
/// The prefix is not trimmed, because the live cache holds the emission
/// untrimmed. Text that holds a thought marker gains the prefix too, because
/// it followed the opener like any other emission, where the template adds
/// none.
///
/// Perturbation: drop the prefix from `render_delta` and every assertion on
/// the empty thought fails.
#[test]
fn an_assistant_turn_carries_the_openers_empty_thought() {
    let (messages, expected) = golden("history");
    assert_eq!(rendered(&messages).unwrap(), expected);
    let (messages, expected) = golden_with("assistant_last", false);
    assert_eq!(renderer().render_identity(&messages).unwrap(), expected);

    let live = format!("{GENERATION_OPENER}Answer.{}", qwen2::TURN_CLOSE);
    let rebuilt = renderer()
        .render_delta(&message(Role::Assistant, "Answer."))
        .unwrap();
    assert!(rebuilt.starts_with(&live), "{rebuilt:?} against {live:?}");

    let spaced = renderer()
        .render_delta(&message(Role::Assistant, " Answer.\n"))
        .unwrap();
    assert_eq!(
        spaced,
        render_template(
            qwen2::TEMPLATE,
            "assistant",
            &format!("{THINK_OPEN}{THINK_CLOSE} Answer.\n")
        )
    );

    for already in ["<think>x</think>Answer.", "</think>Answer.", "<think>x"] {
        assert_eq!(
            renderer()
                .render_delta(&message(Role::Assistant, already))
                .unwrap(),
            render_template(
                qwen2::TEMPLATE,
                "assistant",
                &format!("{THINK_OPEN}{THINK_CLOSE}{already}")
            ),
            "{already}"
        );
    }
}

/// **A marker quoted inside the text does not cost the turn its prefix.** An
/// STE input can quote `</think>` and the rewrite copy it, and the live cache
/// still held the opener's pair before that emission. A condition that
/// skipped the prefix when the text held a marker anywhere would rebuild the
/// turn two tokens short, silently.
///
/// Perturbation: restore a condition skipping the prefix for text holding a
/// marker and this fails.
#[test]
fn a_quoted_marker_keeps_the_turns_prefix() {
    let quoted = "Write the tag </think> as text, then <think> too.";
    let live = format!("{GENERATION_OPENER}{quoted}{}", qwen2::TURN_CLOSE);
    let rebuilt = renderer()
        .render_delta(&message(Role::Assistant, quoted))
        .unwrap();
    assert!(rebuilt.starts_with(&live), "{rebuilt:?} against {live:?}");
}

/// **Named limit: a closed thought in history is kept, not stripped.** The
/// pinned template keeps only what follows the last `</think>` in a turn
/// before the last user turn, so `<think>x</think>A1` renders as
/// `<think></think>A1`. This module keeps the emission as the cache held it,
/// after the opener's pair. The rendering differs from the template's by
/// exactly the stripped thought.
///
/// Perturbation: strip a historical thought in `render_delta` and the first
/// assertion fails, which is the signal to retire this limit.
#[test]
fn limit_a_historical_thought_is_kept() {
    let (messages, expected) = golden("history_with_thought");
    let ours = rendered(&messages).unwrap();
    assert_ne!(ours, expected);
    let kept = render_template(
        qwen2::TEMPLATE,
        "assistant",
        &format!("{THINK_OPEN}{THINK_CLOSE}{THINK_OPEN}x{THINK_CLOSE}A1"),
    );
    let stripped = render_template(
        qwen2::TEMPLATE,
        "assistant",
        &format!("{THINK_OPEN}{THINK_CLOSE}A1"),
    );
    assert_eq!(ours.matches(&kept).count(), 1);
    assert_eq!(ours.replacen(&kept, &stripped, 1), expected);
}

/// **Named limit: admission reads no template for `granite`.** It is the only
/// entry for its architecture, so `select` returns it whatever template the
/// artifact carries, a Granite 3 `<|start_of_role|>` template included. The
/// operator's ruling on judging an uncontested template, or on a measured
/// second row, retires this test.
///
/// Perturbation: make `select` judge a lone candidate's selecting markers
/// against the probe rendering and the Granite 3 template is refused.
#[test]
fn limit_an_uncontested_granite_key_admits_any_template() {
    let granite3 = concat!(
        "{%- for message in messages %}",
        "{{- '<|start_of_role|>' + message['role'] + '<|end_of_role|>' ",
        "+ message['content'] + '<|end_of_text|>\n' }}",
        "{%- endfor %}",
        "{%- if add_generation_prompt %}",
        "{{- '<|start_of_role|>assistant<|end_of_role|>' }}",
        "{%- endif %}"
    );
    let name = FamilyName("granite".into());
    let admitted = select(&name, Some(granite3)).expect("today's selection admits it");
    let entry = lookup(&name).unwrap();
    assert!(std::ptr::eq(admitted, entry));
    assert_eq!(admitted.template, qwen2::TEMPLATE);
}

/// **Refusal: a tool result.** The pinned template closes a tool response
/// with a newline before the turn close, where qwen2's renderer closes it
/// without one, so delegating would render a form this model was not tuned
/// on. The refusal reaches both rendering paths.
///
/// Perturbation: remove the `text_content` guard in `render_delta` and the
/// refusals become qwen2's rendering.
#[test]
fn refusal_a_tool_result() {
    let (messages, expected) = golden("tool_result");
    let result = &messages[2];
    let qwen2_form = qwen2::renderer().render_delta(result).unwrap();
    assert!(expected.contains("\n</tool_response>\n<|im_end|>"));
    assert!(qwen2_form.ends_with("</tool_response><|im_end|>\n"));
    assert_eq!(
        renderer().render_delta(result),
        Err(RenderRefusal::MalformedForFamily)
    );
    assert_eq!(rendered(&messages), Err(RenderRefusal::MalformedForFamily));
}

/// **Refusal and limit: XML tool calls.** This model calls tools as
/// `<function=...>` inside `<tool_call>`. The parse reports such a call as
/// unrecovered under this family's grammar and never as text. A recovered
/// call in a prompt refuses, because qwen2 would write it as JSON. An
/// unrecovered call still renders back as written, per Spec section 5, after
/// the empty thought the opener gave the turn.
///
/// Perturbation: parse with qwen2's grammar and the grammar's family reads
/// `qwen2`; remove the `text_content` guard in `render_delta` and the
/// recovered call renders as qwen2's JSON.
#[test]
fn xml_tool_calls_are_unrecovered_and_a_recovered_call_refuses() {
    let fragment = concat!(
        "<tool_call>\n<function=read>\n<parameter=path>\nsource.txt\n",
        "</parameter>\n</function>\n</tool_call>"
    );
    let parsed = renderer().parse(&format!("Before.{fragment}After."), None);
    assert_eq!(parsed.text(), "Before.After.");
    assert_eq!(parsed.unrecovered.len(), 1);
    assert_eq!(parsed.unrecovered[0].fragment, fragment);
    assert_eq!(parsed.unrecovered[0].grammar.family, "granite");
    assert_eq!(parsed.unrecovered[0].grammar.open, qwen2::CALL_OPEN);

    let recovered = Message {
        role: Role::Assistant,
        content: vec![ContentBlock::ToolCall(weaver_traits::ToolCall {
            name: "read".into(),
            arguments: "{\"path\":\"source.txt\"}".into(),
        })],
    };
    assert!(qwen2::renderer().render_delta(&recovered).is_ok());
    assert_eq!(
        renderer().render_delta(&recovered),
        Err(RenderRefusal::MalformedForFamily)
    );

    let unrecovered = Message {
        role: Role::Assistant,
        content: vec![ContentBlock::UnrecoveredCall {
            fragment: fragment.into(),
        }],
    };
    assert_eq!(
        renderer().render_delta(&unrecovered).unwrap(),
        render_template(
            qwen2::TEMPLATE,
            "assistant",
            &format!("{THINK_OPEN}{THINK_CLOSE}{fragment}")
        )
    );
}

/// **The entry's facts**, per Spec section 5's ownership table: the opener,
/// the marker set and the flush are the entry's, the stop set the renderer's.
///
/// Perturbation: give the opener a trailing newline, or declare the flush by
/// re-establishing, and this fails.
#[test]
fn granite_entry_keeps_its_opener_markers_stop_set_and_truncating_flush() {
    let entry = select(&FamilyName("granite".into()), None).expect("granite is carried");
    assert!(std::ptr::eq(entry, renderer().declaration()));
    assert!(std::ptr::eq((entry.renderer)(), renderer()));
    assert_eq!(entry.family, "granite");
    assert_eq!(entry.template, qwen2::TEMPLATE);
    assert_eq!(entry.generation_opener, GENERATION_OPENER);
    assert_eq!(
        GENERATION_OPENER,
        format!("{}{THINK_OPEN}{THINK_CLOSE}", qwen2::GENERATION_OPENER)
    );
    assert!(!GENERATION_OPENER.ends_with('\n'));
    assert_eq!(entry.selecting_markers, SELECTING_MARKERS);
    assert_eq!(SELECTING_MARKERS, qwen2::RENDERED_MARKERS);
    assert_eq!(&RENDERED_MARKERS[..2], qwen2::RENDERED_MARKERS);
    assert_eq!(&RENDERED_MARKERS[2..], &[THINK_OPEN, THINK_CLOSE]);
    assert_eq!(renderer().stop_conditions(), &[qwen2::TURN_CLOSE]);
    assert_eq!(entry.flush, FlushMechanism::TruncateToPosition);
    assert!(entry.permits_truncation());
    assert!(entry.shards_across(1));
    assert!(entry.shards_across(2));
    assert!(!entry.shards_across(3));
    assert!(!entry.taps_readout);
    assert!(!entry.taps_column);
}
