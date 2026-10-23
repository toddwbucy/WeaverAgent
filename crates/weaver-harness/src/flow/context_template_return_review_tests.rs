//! A real template-held caller must return before lifecycle release.
use super::*;

#[test]
fn template_return_review_complete_checks_do_not_drop_the_held_caller_at_release() {
    for take_before_release in [false, true] {
        let mut fx = TemplateFixture::new();
        fx.begin();
        let original = AuthoredEvent {
            kind: Kind::MessageAssistant,
            subsystem: weaver_trace::Subsystem::Harness,
            turn: Some(weaver_trace::TurnRef("held-template-release".into())),
            source: crate::flow::author::AuthoredSource::Model,
            causal_parent: Some(Sequence(0)),
            payload: Some(Payload::Message(serde_json::value::to_raw_value(
                &serde_json::json!({"role":"assistant","content":[{"type":"text","text":"Read file."}]})
            ).unwrap())),
        };
        let expected = format!("{original:?}");
        assert_eq!(
            fx.input.run.author(original),
            Err(RunFailure::TemplateBlocked)
        );
        for index in 0..3 {
            fx.accept(index);
        }
        assert_eq!(fx.advance().unwrap(), TemplateProgress::Complete);
        assert!(
            fx.input
                .run
                .context
                .template_transcript()
                .unwrap()
                .is_complete()
        );
        if take_before_release {
            assert_eq!(
                format!(
                    "{:?}",
                    fx.input.run.context.take_template_blocked_author().unwrap()
                ),
                expected
            );
            assert!(fx.certificate().unwrap().is_some());
        } else {
            assert!(fx.certificate().unwrap().is_none());
        }
        let run = &mut fx.input.run;
        run.context.complete_wait(PumpDeadline::Enter).unwrap();
        let leave = AbsoluteBound::after(run.clock.now().unwrap(), 100_000).unwrap();
        let result = run.context.begin_releasing(leave, &mut run.clock);
        if take_before_release {
            result.unwrap();
            assert_eq!(run.context.phase(), RecordingPhase::Releasing);
        } else {
            assert!(matches!(
                result,
                Err(RunFailure::Flow {
                    reason: FlowHaltReason::ProtocolFailed,
                    ..
                })
            ));
            assert_eq!(
                format!("{:?}", run.context.take_template_blocked_author().unwrap()),
                expected
            );
            assert_eq!(run.context.phase(), RecordingPhase::Failed);
        }
    }
}
