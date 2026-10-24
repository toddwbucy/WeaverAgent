//! Shared held-boundary instruments. These unaccepted inputs are not proofs,
//! tickets or recorded events. The boundary fixtures establish their real
//! prerequisites first, then isolate each possible occupied custody slot.
use super::*;

pub(super) const HOLDS: [HeldInput; 3] = [HeldInput::Caller, HeldInput::Template, HeldInput::Both];

pub(super) struct Originals {
    caller: Option<(*const u8, String)>,
    template: Option<(*const u8, String)>,
}

fn identity(event: &AuthoredEvent) -> (*const u8, String) {
    let Some(Payload::Refusal(raw)) = &event.payload else {
        panic!("fixture original has its exact raw payload")
    };
    (raw.get().as_ptr(), format!("{event:?}"))
}

pub(super) fn hold(context: &mut RecordingContext, held: HeldInput) -> Originals {
    assert!(!context.caller_author_held());
    let event = |detail| {
        tests::event(
            Kind::Refusal,
            Some(Payload::Refusal(
                serde_json::value::to_raw_value(&serde_json::json!({"held": detail})).unwrap(),
            )),
        )
    };
    if matches!(held, HeldInput::Caller | HeldInput::Both) {
        context.held_author = Some(event("caller"));
    }
    if matches!(held, HeldInput::Template | HeldInput::Both) {
        context.template_blocked_author = Some(event("template"));
    }
    Originals {
        caller: context.held_author.as_ref().map(identity),
        template: context.template_blocked_author.as_ref().map(identity),
    }
}

impl Originals {
    pub(super) fn assert_held(&self, context: &RecordingContext) {
        assert_eq!(context.held_author.as_ref().map(identity), self.caller);
        assert_eq!(
            context.template_blocked_author.as_ref().map(identity),
            self.template
        );
    }
    pub(super) fn take(self, context: &mut RecordingContext) {
        assert_eq!(
            context.take_held_author().as_ref().map(identity),
            self.caller
        );
        assert_eq!(
            context
                .take_template_blocked_author()
                .as_ref()
                .map(identity),
            self.template
        );
        assert!(!context.caller_author_held());
    }
}

#[test]
fn held_boundary_private_continuation_requires_the_existing_owner() {
    for held in HOLDS {
        for owned in [false, true] {
            let mut run = tests::Fixture::new(tests::config());
            let originals = hold(&mut run.context, held);
            assert_eq!(run.context.held_input(), Some(held));
            assert_eq!(
                run.context.caller_author_blocks_continuation(|| owned),
                held != HeldInput::Caller || !owned
            );
            originals.assert_held(&run.context);
            originals.take(&mut run.context);
            assert!(!run.context.caller_author_blocks_continuation(|| false));
        }
    }
}
