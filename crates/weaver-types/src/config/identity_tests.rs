//! Test-only: the `identity_tests` suite of `config.rs`, moved out of the production file by #118.

use super::check_identity_roles;
use weaver_traits::{ContentBlock, Message, Role, ToolCall};

fn text(text: &str) -> ContentBlock {
    ContentBlock::Text {
        text: text.to_string(),
    }
}

fn refused_at(identity: &[Message]) -> String {
    check_identity_roles(identity)
        .expect_err("refuses")
        .field
        .expect("names its field")
        .0
}

/// **A role the door does not write refuses, naming its index.**
/// Perturbation: drop the role arm and all three roles pass.
#[test]
fn a_non_system_identity_role_refuses() {
    for role in [Role::User, Role::Assistant, Role::ToolResult] {
        let identity = [Message {
            role: role.clone(),
            content: vec![text("You answer briefly.")],
        }];
        assert_eq!(refused_at(&identity), "identity.0.role", "{role:?}");
    }
}

/// **A `system` message carrying a block it may not carry refuses**, per
/// `weaver-traits-Spec` section 3, which licenses `Text` alone there.
/// Perturbation: drop the block loop and a `tool_call` passes.
#[test]
fn an_unlicensed_identity_block_refuses() {
    let identity = [Message {
        role: Role::System,
        content: vec![ContentBlock::ToolCall(ToolCall {
            name: "calculator".into(),
            arguments: "{}".into(),
        })],
    }];
    assert_eq!(refused_at(&identity), "identity.0.content.0");
}

/// **A message carrying nothing refuses**, though an empty list does not.
/// Perturbation: drop the empty-content check and it passes.
#[test]
fn an_identity_message_carrying_nothing_refuses() {
    let identity = [Message {
        role: Role::System,
        content: vec![],
    }];
    assert_eq!(refused_at(&identity), "identity.0.content");
    assert!(check_identity_roles(&[]).is_ok(), "an empty list is lawful");
}

/// **A text block carrying no text refuses**, the empty turn by a second
/// route. Perturbation: drop the empty-text arm and it passes.
#[test]
fn an_identity_text_block_carrying_no_text_refuses() {
    let identity = [Message {
        role: Role::System,
        content: vec![text("")],
    }];
    assert_eq!(refused_at(&identity), "identity.0.content.0.text");
}
