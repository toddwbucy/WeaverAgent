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

fn returned_attempt(context: &mut RecordingContext, template: bool, detail: &str) -> AuthorFailure {
    let event = tests::event(
        Kind::Refusal,
        Some(Payload::Refusal(
            serde_json::value::RawValue::from_string(format!("{{ \"held\" : \"{detail}\" }}"))
                .unwrap(),
        )),
    );
    if template {
        RecordingContext::hold_author_failure(
            &mut context.template_blocked_author,
            &mut context.template_blocked_identity,
            HeldInput::Template,
            RunFailure::TemplateBlocked,
            event,
        )
    } else {
        RecordingContext::hold_author_failure(
            &mut context.held_author,
            &mut context.held_author_identity,
            HeldInput::Caller,
            RunFailure::InspectionOwned,
            event,
        )
    }
}

#[test]
fn held_author_return_preserves_other_context_slot_and_first_fatal() {
    for template in [false, true] {
        let mut run = tests::Fixture::new(tests::config());
        let mut foreign = tests::Fixture::new(tests::config());
        let mut owner = returned_attempt(&mut run.context, template, "owner");
        let mut other_slot = returned_attempt(&mut run.context, !template, "other-slot");
        let mut other_context = returned_attempt(&mut foreign.context, template, "foreign");
        assert_eq!(run.context.held_input(), Some(HeldInput::Both));
        let expected = if template {
            identity(run.context.template_blocked_author.as_ref().unwrap())
        } else {
            identity(run.context.held_author.as_ref().unwrap())
        };
        assert!(!owner.reclaim_held(&mut foreign.context));
        assert!(owner.original.is_none());
        let failure = run.context.fail(
            &mut run.clock,
            FlowHaltReason::ProtocolFailed,
            HaltPoint::Author,
            "original failure before caller reclaims input",
        );
        let first = format!("{failure:?}");
        let waits = run.context.deadlines.entries().collect::<Vec<_>>();
        assert!(owner.reclaim_held(&mut run.context));
        assert_eq!(identity(owner.original.as_ref().unwrap()), expected);
        assert!(!owner.reclaim_held(&mut run.context));
        assert_eq!(
            run.context.held_input(),
            Some(if template {
                HeldInput::Caller
            } else {
                HeldInput::Template
            })
        );
        assert_eq!(run.context.deadlines.entries().collect::<Vec<_>>(), waits);
        assert_eq!(
            format!("{:?}", run.context.live(&mut run.clock).unwrap_err()),
            first
        );
        assert!(other_slot.reclaim_held(&mut run.context));
        assert!(other_context.reclaim_held(&mut foreign.context));
        assert!(!run.context.caller_author_held());
        assert!(run.context.live(&mut run.clock).is_err());
    }
}

#[test]
fn stale_held_author_return_cannot_take_a_replacement_original() {
    for template in [false, true] {
        let mut run = tests::Fixture::new(tests::config());
        let mut old = returned_attempt(&mut run.context, template, "first");
        let original = if template {
            run.context.take_template_blocked_author().unwrap()
        } else {
            run.context.take_held_author().unwrap()
        };
        let mut new = returned_attempt(&mut run.context, template, "replacement");
        assert!(!old.reclaim_held(&mut run.context));
        assert!(old.original.is_none());
        assert!(new.reclaim_held(&mut run.context));
        assert_ne!(
            identity(&original),
            identity(new.original.as_ref().unwrap())
        );
        assert!(!run.context.caller_author_held());
        assert!(run.context.pending_author().is_none());
        assert!(run.context.failure().is_none());
    }
}
