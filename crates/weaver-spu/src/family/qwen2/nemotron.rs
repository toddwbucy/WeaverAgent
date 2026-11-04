//! The Nemotron 3 Nano 4B ChatML variant, per SPU Spec 5 and workflow 9.02, 9.05.
//! The R0 path disables thinking and carries no tool schemas or tool messages.
//! The pinned template is reproduced by Rust, not evaluated during service.

use super::super::{
    ContentBlock, Declaration, Family, FamilyName, Markers, Message, Parsed, RenderRefusal, Role,
    lookup, render_each, render_template, scan, text_content,
};
use super::{CALL_CLOSE, CALL_OPEN, TEMPLATE, TURN_CLOSE, TURN_OPEN};

pub const SOURCE_REVISION: &str = "3eab5a7bdfb5e669104f0014242972da1db50771";
pub const TEMPLATE_SHA256: &str =
    "ab7813c3abdd9cb655905a410728b26c7884eca45ddfab8d9f931553485a7862";
pub const THINK_OPEN: &str = "<think>";
pub const THINK_CLOSE: &str = "</think>";
pub const RENDERED_MARKERS: &[&str] = &[TURN_OPEN, TURN_CLOSE, THINK_OPEN, THINK_CLOSE];
pub const GENERATION_OPENER: &str = "<|im_start|>assistant\n<think></think>";

pub fn renderer() -> &'static dyn Family {
    &Nemotron
}

pub struct Nemotron;

fn assistant_text(text: &str, historical: bool) -> String {
    let content = if !text.contains(THINK_OPEN) && !text.contains(THINK_CLOSE) {
        format!("{THINK_OPEN}{THINK_CLOSE}{text}")
    } else {
        text.to_owned()
    };
    let content = if historical && content.contains(THINK_OPEN) && content.contains(THINK_CLOSE) {
        format!(
            "{THINK_OPEN}{THINK_CLOSE}{}",
            content.rsplit_once(THINK_CLOSE).unwrap().1
        )
    } else {
        content
    };
    // Jinja's trim uses Python whitespace, which includes these four controls.
    content
        .trim_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
        .to_owned()
}

impl Family for Nemotron {
    fn render_identity(&self, messages: &[Message]) -> Result<String, RenderRefusal> {
        let mut rendered = String::new();
        if !messages
            .first()
            .is_some_and(|message| message.role == Role::System)
        {
            rendered.push_str(&render_template(TEMPLATE, "system", ""));
        }
        rendered.push_str(&render_each(self, messages)?);
        Ok(rendered)
    }

    fn render_delta(&self, message: &Message) -> Result<String, RenderRefusal> {
        let text = text_content(message)?;
        let (role, text) = match message.role {
            Role::System => ("system", text),
            Role::User => ("user", text),
            Role::Assistant => ("assistant", assistant_text(&text, false)),
            _ => return Err(RenderRefusal::MalformedForFamily),
        };
        Ok(render_template(TEMPLATE, role, &text))
    }

    fn fold_for_template(&self, messages: &[Message]) -> Result<Vec<Message>, RenderRefusal> {
        let last_user = messages
            .iter()
            .rposition(|message| message.role == Role::User);
        messages
            .iter()
            .enumerate()
            .map(|(index, message)| {
                if message.role != Role::Assistant {
                    return Ok(message.clone());
                }
                let text = text_content(message)?;
                Ok(Message {
                    role: Role::Assistant,
                    content: vec![ContentBlock::Text {
                        text: assistant_text(&text, last_user.is_some_and(|last| index < last)),
                    }],
                })
            })
            .collect()
    }

    fn parse(&self, emission: &str, ended_by: Option<&str>) -> Parsed {
        scan(
            emission,
            ended_by,
            Markers {
                family: "nemotron_h",
                call_open: CALL_OPEN,
                call_close: Some(CALL_CLOSE),
            },
            |_| Err("Nemotron tool-call recovery is not implemented".into()),
        )
    }

    fn stop_conditions(&self) -> &'static [&'static str] {
        &[TURN_CLOSE]
    }

    fn declaration(&self) -> &'static Declaration {
        lookup(&FamilyName("nemotron_h".into())).expect("registry carries nemotron_h")
    }
}

#[cfg(test)]
mod tests;
