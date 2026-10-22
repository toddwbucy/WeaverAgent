//! SPU-RENDER fixtures use each admitted registry entry's actual renderer.
//! They establish rendering mechanics, not model or tokenizer qualification.

use super::*;
use weaver_traits::{ToolCall, ToolResultBlock};

fn text(role: Role, body: &str) -> Message {
    Message {
        role,
        content: vec![ContentBlock::Text { text: body.into() }],
    }
}

#[test]
fn no_schemas_preserve_the_legacy_generation_bytes_for_every_registry_entry() {
    let inputs = [
        vec![],
        vec![text(Role::User, "Read the file.")],
        vec![
            text(Role::System, "Use the declared tools."),
            text(Role::User, "Read the file."),
            text(Role::Assistant, "The file exists."),
            text(Role::System, "Report the result."),
        ],
    ];
    for entry in REGISTRY {
        let family = (entry.renderer)();
        for delta in &inputs {
            let original = delta.clone();
            let mut expected = render_each(family, delta).unwrap();
            expected.push_str(entry.generation_opener);
            assert_eq!(
                render_generation(family, delta, &[], entry.generation_opener).unwrap(),
                expected,
                "{} / {}",
                entry.family,
                entry.template,
            );
            assert_eq!(*delta, original);
        }
    }
}

#[test]
fn schemas_are_one_ordered_message_in_the_complete_family_fold() {
    let delta = vec![
        text(Role::User, "Read the file."),
        text(Role::Assistant, "The file exists."),
        text(Role::System, "Use these declarations."),
    ];
    let schemas = vec![
        "first schema\nwith two lines".to_string(),
        String::new(),
        "third schema\r\nwith original bytes".to_string(),
    ];
    let original = (delta.clone(), schemas.clone());
    let mut complete = delta.clone();
    complete.push(text(
        Role::User,
        "first schema\nwith two lines\n\nthird schema\r\nwith original bytes",
    ));
    for entry in REGISTRY {
        let family = (entry.renderer)();
        let mut expected = render_each(family, &complete).unwrap();
        expected.push_str(entry.generation_opener);
        assert_eq!(
            render_generation(family, &delta, &schemas, entry.generation_opener).unwrap(),
            expected,
            "{} / {}",
            entry.family,
            entry.template,
        );
    }
    assert_eq!((delta, schemas), original);
}

#[test]
fn trailing_system_and_schema_fold_together_without_an_identity_preamble() {
    let delta = vec![text(Role::System, "Use this tool.")];
    let schemas = vec!["Read a file.".into(), "Write a file.".into()];
    // These exact strings distinguish one complete fold from separately
    // rendered delta/schema fragments, which would leave two user turns.
    assert_eq!(
        render_generation(mistral3::renderer(), &delta, &schemas, "<generation>").unwrap(),
        "[INST]Use this tool.\n\nRead a file.\nWrite a file.[/INST]<generation>",
    );
    assert_eq!(
        render_generation(gemma4::renderer(), &delta, &schemas, "<generation>").unwrap(),
        "<|turn>user\nUse this tool.\n\nRead a file.\nWrite a file.<turn|>\n<generation>",
    );
}

#[test]
fn empty_delta_and_empty_schema_text_keep_the_declared_message_and_opener() {
    for entry in REGISTRY {
        let family = (entry.renderer)();
        assert_eq!(
            render_generation(family, &[], &[], entry.generation_opener).unwrap(),
            entry.generation_opener,
            "{} / {}",
            entry.family,
            entry.template,
        );
        let mut expected = render_each(family, &[text(Role::User, "")]).unwrap();
        expected.push_str(entry.generation_opener);
        assert_eq!(
            render_generation(family, &[], &[String::new()], entry.generation_opener).unwrap(),
            expected,
            "{} / {}",
            entry.family,
            entry.template,
        );
    }
}

#[test]
fn tool_blocks_reach_each_family_intact_or_refuse_without_text_filtering() {
    let inputs = [
        vec![Message {
            role: Role::Assistant,
            content: vec![
                ContentBlock::Text {
                    text: "Read the file.".into(),
                },
                ContentBlock::ToolCall(ToolCall {
                    name: "read_file".into(),
                    arguments: "{\"path\":\"source.txt\"}".into(),
                }),
            ],
        }],
        vec![Message {
            role: Role::ToolResult,
            content: vec![ContentBlock::ToolResult(ToolResultBlock {
                content: "The file contains the result.".into(),
            })],
        }],
        vec![Message {
            role: Role::Assistant,
            content: vec![ContentBlock::UnrecoveredCall {
                fragment: "<attempted-call>unrecovered bytes".into(),
            }],
        }],
        vec![Message {
            role: Role::User,
            content: vec![ContentBlock::ToolCall(ToolCall {
                name: "read_file".into(),
                arguments: "{}".into(),
            })],
        }],
    ];
    for entry in REGISTRY {
        let family = (entry.renderer)();
        for delta in &inputs {
            let original = delta.clone();
            let mut complete = delta.clone();
            complete.push(text(Role::User, "Tool schema."));
            let expected = render_each(family, &complete).map(|mut rendered| {
                rendered.push_str(entry.generation_opener);
                rendered
            });
            assert_eq!(
                render_generation(
                    family,
                    delta,
                    &["Tool schema.".into()],
                    entry.generation_opener
                ),
                expected,
                "{} / {}",
                entry.family,
                entry.template,
            );
            assert_eq!(*delta, original);
        }
    }
    let supported = render_generation(
        qwen2::renderer(),
        &inputs[0],
        &["Tool schema.".into()],
        qwen2::GENERATION_OPENER,
    )
    .unwrap();
    assert!(supported.contains("read_file"));
    assert!(supported.contains("source.txt"));
    assert!(supported.contains(qwen2::CALL_OPEN));
    assert_eq!(
        render_generation(
            gemma4::renderer(),
            &inputs[1],
            &[],
            gemma4::GENERATION_OPENER
        ),
        Err(RenderRefusal::MalformedForFamily),
    );
}
