//! PlanUpdate is refused at the worker's real management seam during an open turn.
use super::*;

pub(super) fn open_turn(
    serving: FoundationServing<'_>,
    member: &mut Member,
    reader: &mut UnixStream,
    socket: &Path,
) {
    let mut guard = Guard {
        input: Some(serving.input()),
        management: None,
    };
    let done = AtomicBool::new(false);
    std::thread::scope(|scope| {
        let responder = scope.spawn(|| {
            let end = Instant::now() + Duration::from_secs(10);
            while !done.load(Ordering::SeqCst) && Instant::now() < end {
                member.step_live("ready");
                std::thread::sleep(Duration::from_millis(1));
            }
        });
        let end = Instant::now() + Duration::from_secs(5);
        while guard.input.as_ref().unwrap().progress() != FoundationInputProgress::Accepted {
            let input = guard.input.as_mut().unwrap();
            assert_ne!(
                input.advance(),
                FoundationInputProgress::Stopped,
                "{:?}",
                input.detail()
            );
            assert!(Instant::now() < end, "input never accepted");
        }
        guard
            .input
            .as_mut()
            .unwrap()
            .start_turn(TurnKey("plan-active-turn".into()))
            .unwrap();
        while guard.input.as_ref().unwrap().progress() != FoundationInputProgress::Turn {
            let input = guard.input.as_mut().unwrap();
            assert_ne!(
                input.advance(),
                FoundationInputProgress::Stopped,
                "{:?}",
                input.detail()
            );
            assert!(Instant::now() < end, "turn never opened");
        }
        let original_turn = guard
            .input
            .as_ref()
            .unwrap()
            .serving
            .owner
            .context
            .open_turn
            .clone();
        assert_eq!(
            original_turn,
            Some(weaver_trace::TurnRef("plan-active-turn".into()))
        );
        let old_summary = guard
            .input
            .as_mut()
            .unwrap()
            .serving
            .owner
            .context
            .committed_plan_summary()
            .unwrap()
            .cloned();
        let before = trace(reader);
        assert_eq!(
            before
                .iter()
                .filter(|row| row["kind"] == "turn.started")
                .count(),
            1
        );
        for _ in 0..2 {
            let peer = request(socket, &document());
            let answer: OrganEnvelope = s::decode_strict(
                std::str::from_utf8(&management_answer(&mut guard, &peer)).unwrap(),
            )
            .unwrap();
            assert_eq!(answer.exchange.ordinal, 2);
            assert_eq!(answer.position, Position::Close);
            assert_eq!(
                answer.payload,
                Payload::Refusal(LifecycleRefusal::OutOfOrder)
            );
            let input = guard.input.as_mut().unwrap();
            assert_eq!(input.progress(), FoundationInputProgress::Turn);
            assert!(!input.plan_pending());
            assert!(input.serving.owner.context.root_plan_work().is_none());
            assert!(input.serving.owner.context.failure().is_none());
            assert_eq!(input.serving.owner.context.open_turn, original_turn);
            assert_eq!(
                input
                    .serving
                    .owner
                    .context
                    .committed_plan_summary()
                    .unwrap()
                    .cloned(),
                old_summary
            );
        }
        let after = trace(reader);
        assert!(
            !after
                .iter()
                .any(|row| row["kind"] == "plan.event" || row["kind"] == "turn.closed")
        );
        let refusals: Vec<_> = after
            .iter()
            .filter(|row| row["kind"] == "refusal")
            .collect();
        assert_eq!(refusals.len(), 2);
        for row in refusals {
            assert_eq!(row["payload"]["seam"], "lifecycle");
            assert_eq!(
                row["payload"]["asked"],
                serde_json::json!({"ask":"plan_update"})
            );
            assert_eq!(
                row["payload"]["refusal"],
                serde_json::json!({"kind":"out_of_order"})
            );
            assert!(row.get("turn").is_none());
            assert!(row.get("causal_parent").is_none());
        }
        done.store(true, Ordering::SeqCst);
        responder.join().unwrap();
    });
}
