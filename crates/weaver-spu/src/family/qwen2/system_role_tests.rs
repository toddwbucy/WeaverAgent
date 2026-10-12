//! Test-only: the `system_role_tests` suite of `family/qwen2.rs`,
//! moved out of the production file by #118.

use super::super::Role;
use weaver_traits::{ContentBlock, Message};

/// The system turn renders in the family's own template shape, per the
/// system role act: the loop's framing enters the slot the template
/// carries for it rather than riding user role.
#[test]
fn a_system_message_renders_in_the_system_slot() {
    let message = Message {
        role: Role::System,
        content: vec![ContentBlock::Text {
            text: "You are a careful assistant.".to_string(),
        }],
    };
    let rendered = super::renderer()
        .render_delta(&message)
        .expect("the slot exists in this family");
    assert_eq!(
        rendered,
        "<|im_start|>system\nYou are a careful assistant.<|im_end|>\n"
    );
}
