//! Caller custody at each actual input author stage, including resumed intake.
use super::*;
use crate::flow::clock::{WaitInterest, WaitOutcome};
use crate::flow::context::held_input_tests::{Originals, hold};
use crate::flow::context::input_tests::InputFixture;
use crate::flow::pump::LifecyclePorts;

struct CrossingClock {
    reads: usize,
    before: u64,
    after: u64,
}
impl PumpIo for CrossingClock {
    fn now(&mut self) -> std::io::Result<u64> {
        if self.reads == 0 {
            Ok(self.after)
        } else {
            self.reads -= 1;
            Ok(self.before)
        }
    }
    fn wait(&mut self, _: &[WaitInterest<'_>], _: Option<u64>) -> std::io::Result<WaitOutcome> {
        panic!("initial admission must retain before a transport wait")
    }
}
fn ports(io: &mut impl PumpIo) -> PumpPorts<'_, impl PumpIo> {
    PumpPorts {
        lifecycle: LifecyclePorts::default(),
        gate: None,
        decode: None,
        label: None,
        io,
    }
}
fn exact(event: &AuthoredEvent) -> (usize, String) {
    let pointer = match &event.payload {
        Some(Payload::InputOffered(value)) => &**value as *const _ as usize,
        Some(Payload::Refusal(value) | Payload::Message(value)) => value.get().as_ptr() as usize,
        None => event.turn.as_ref().unwrap().0.as_ptr() as usize,
        _ => panic!("input author fixture"),
    };
    (pointer, format!("{event:?}"))
}
fn crossing(fx: &mut InputFixture, reads: usize) -> CrossingClock {
    let before = fx.run.clock.now().unwrap();
    let bound = AbsoluteBound::after(before, 5).unwrap();
    fx.run
        .context
        .arm(PumpDeadline::Leave, bound, &mut fx.run.clock)
        .unwrap();
    CrossingClock {
        reads,
        before,
        after: bound.deadline_ns(),
    }
}
fn held_retry(fx: &mut InputFixture) -> (Originals, (usize, String), AbsoluteBound) {
    let original = exact(&fx.run.context.waiting_content.as_ref().unwrap().event);
    assert!(fx.run.context.author_admission_pending);
    assert!(fx.run.context.pending_author().is_none());
    let bound = fx.run.context.complete_wait(PumpDeadline::Leave).unwrap();
    fx.run.clock.advance_to(bound.deadline_ns()).unwrap();
    let held = hold(&mut fx.run.context, HeldInput::Both);
    (held, original, bound)
}

#[test]
fn author_failure_input_offer_and_refusal_keep_stage_and_exact_return() {
    for refusal in [false, true] {
        let mut fx = InputFixture::new();
        fx.begin(TurnFrame::carry_from(b"not JSON", 43));
        if refusal {
            assert_eq!(
                fx.run
                    .context
                    .advance_input(&mut ports(&mut fx.run.clock))
                    .unwrap(),
                InputProgress::Progress
            );
        }
        let mut clock = crossing(&mut fx, 1);
        assert!(matches!(
            fx.run.context.advance_input(&mut ports(&mut clock)),
            Err(RunFailure::Wait { .. })
        ));
        let (held, original, _) = held_retry(&mut fx);
        let reason = fx
            .run
            .context
            .advance_input(&mut ports(&mut fx.run.clock))
            .unwrap_err();
        let author = fx.run.context.input_author.as_ref().unwrap();
        assert_eq!(matches!(author.stage, InputAuthorStage::Refusal), refusal);
        assert_eq!(
            author
                .returned
                .failure
                .as_ref()
                .unwrap()
                .original
                .as_ref()
                .map(exact),
            Some(original.clone())
        );
        assert!(fx.run.context.waiting_content.is_none());
        assert!(fx.run.context.pending_author().is_none());
        assert_eq!(
            fx.run
                .context
                .advance_input(&mut ports(&mut fx.run.clock))
                .unwrap_err(),
            reason
        );
        assert_eq!(
            fx.run
                .context
                .input_author
                .as_ref()
                .unwrap()
                .returned
                .failure
                .as_ref()
                .unwrap()
                .original
                .as_ref()
                .map(exact),
            Some(original)
        );
        held.assert_held(&fx.run.context);
        assert!(
            fx.records(if refusal {
                Kind::Refusal
            } else {
                Kind::InputOffered
            })
            .is_empty()
        );
    }
}

#[test]
fn author_failure_input_turn_and_message_keep_original_source_without_restart() {
    for (reads, kind, turn_recorded) in
        [(2, Kind::TurnStarted, false), (5, Kind::MessageUser, true)]
    {
        let mut fx = InputFixture::new();
        fx.begin(TurnFrame::carry_from(br#"{"text":"Read file."}"#, 42));
        let (_, _, token) = fx.accepted_token();
        let mut clock = crossing(&mut fx, reads);
        let failure = fx
            .run
            .context
            .consume_input(
                token,
                TurnRef("returned-input".into()),
                &mut ports(&mut clock),
            )
            .unwrap_err();
        assert!(failure.token.is_none());
        assert!(matches!(failure.failure, RunFailure::Wait { .. }));
        assert_eq!(fx.run.context.waiting_content.as_ref().unwrap().kind, kind);
        let (held, original, _) = held_retry(&mut fx);
        let reason = fx
            .run
            .context
            .resume_input_turn(&mut ports(&mut fx.run.clock))
            .unwrap_err();
        let work = fx.run.context.input_turn.as_ref().unwrap();
        assert!(!work.author_started);
        assert_eq!(work.turn_recorded, turn_recorded);
        assert_eq!(work.turn.0, "returned-input");
        assert_eq!(work.input.dialer, 42);
        assert_eq!(
            work.returned_author
                .failure
                .as_ref()
                .unwrap()
                .original
                .as_ref()
                .map(exact),
            Some(original.clone())
        );
        assert_eq!(
            fx.run
                .context
                .resume_input_turn(&mut ports(&mut fx.run.clock))
                .unwrap_err(),
            reason
        );
        assert_eq!(
            fx.run
                .context
                .input_turn
                .as_ref()
                .unwrap()
                .returned_author
                .failure
                .as_ref()
                .unwrap()
                .original
                .as_ref()
                .map(exact),
            Some(original)
        );
        held.assert_held(&fx.run.context);
        assert!(fx.records(kind).is_empty());
        assert!(fx.run.context.pending_author().is_none());
    }
}
