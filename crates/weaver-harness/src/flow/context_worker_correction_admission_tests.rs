//! Generation admission starts after real repair, filing and placement. No
//! ready descriptor may hide an unbounded Busy retry or replace its grant.
use super::*;
use crate::flow::clock::{WaitInterest, WaitOutcome};
use crate::flow::context::held_input_tests::{HOLDS, hold};
use std::sync::Arc;

struct Unready<'a> {
    clock: &'a mut FakePumpIo,
    bound: Option<u64>,
    expire: bool,
    waits: usize,
}
impl PumpIo for Unready<'_> {
    fn now(&mut self) -> std::io::Result<u64> {
        self.clock.now()
    }

    fn wait(
        &mut self,
        _interests: &[WaitInterest<'_>],
        deadline: Option<u64>,
    ) -> std::io::Result<WaitOutcome> {
        let now = self.clock.now()?;
        // The readiness-only observation is not the blocking admission wait.
        if deadline != Some(now) {
            let bound = deadline.expect("restatement admission must have a bound");
            if let Some(original) = self.bound {
                assert_eq!(bound, original, "Busy must retain its original bound");
            }
            self.bound = Some(bound);
            self.waits += 1;
            self.clock
                .advance_to(if self.expire { bound } else { now + 1 })?;
            if self.expire {
                return Ok(WaitOutcome::Timeout);
            }
        }
        Ok(WaitOutcome::Ready(vec![]))
    }
}

/// True means the expired owner has halted; otherwise the real generation
/// has accepted its original measured grant and the caller continues repair.
pub(super) fn exercise(
    input: &mut FoundationInput<'_>,
    clock: &mut FakePumpIo,
    root: &Path,
    mode: &str,
) -> bool {
    let correction = |input: &FoundationInput<'_>| {
        let work = input.engine.as_ref().unwrap().emission.as_ref().unwrap();
        let correction = work.corrections.last().unwrap();
        let Some(PlacementCompletion::Granted(grant)) = &correction.placement else {
            panic!("retain the actual measured grant until admission")
        };
        (
            Arc::clone(&grant.identity),
            Arc::clone(&grant.units[0]),
            format!("{grant:?}"),
            correction.restatement.source().clone(),
            correction.restatement.recorded().clone(),
        )
    };
    let original = correction(input);
    let measured = std::fs::read(root.join("correction-measured-1.json")).unwrap();
    let sequence = input.serving.owner.context.last_author_sequence;
    let inherited = mode.ends_with("inherited").then(|| {
        let bound = AbsoluteBound::after(clock.now().unwrap(), 1_000).unwrap();
        input
            .serving
            .owner
            .context
            .arm(PumpDeadline::Decision, bound, clock)
            .unwrap();
        bound.deadline_ns()
    });
    let mut io = Unready {
        clock,
        bound: inherited,
        expire: mode.ends_with("deadline"),
        waits: 0,
    };
    let slots = if io.expire {
        &[crate::flow::context::HeldInput::Both][..]
    } else {
        &HOLDS[..]
    };
    for &slot in slots {
        let held = hold(input.serving.owner.context, slot);
        for _ in 0..3 {
            let prior = io.waits;
            let progress = input.advance_with_io(&mut io);
            assert!(io.waits > prior, "Busy must enter the common bounded wait");
            held.assert_held(input.serving.owner.context);
            let retained = correction(input);
            assert!(Arc::ptr_eq(&retained.0, &original.0));
            assert!(Arc::ptr_eq(&retained.1, &original.1));
            assert_eq!(retained.2, original.2);
            assert_eq!(retained.3, original.3);
            assert_eq!(retained.4, original.4);
            assert!(input.serving.owner.context.generation.is_none());
            assert!(input.serving.owner.context.measure.is_none());
            assert!(!root.join("correction-appended-1.json").exists());
            assert_eq!(
                std::fs::read(root.join("correction-measured-1.json")).unwrap(),
                measured
            );
            if io.expire {
                assert_eq!(progress, FoundationInputProgress::Stopped);
                assert!(matches!(
                    input.serving.owner.context.failure(),
                    Some(RunFailure::Flow {
                        reason: FlowHaltReason::DecisionBound,
                        ..
                    })
                ));
                assert!(input.effective_emission().is_none());
                held.take(input.serving.owner.context);
                return true;
            }
            assert_eq!(progress, FoundationInputProgress::Pending);
            assert_eq!(input.serving.owner.context.last_author_sequence, sequence);
            assert!(input.serving.owner.context.failure().is_none());
            assert_eq!(
                input
                    .serving
                    .owner
                    .context
                    .deadlines
                    .get(PumpDeadline::Decision)
                    .map(|bound| bound.deadline_ns()),
                io.bound
            );
        }
        held.take(input.serving.owner.context);
    }
    assert_eq!(
        input.advance_with_io(&mut io),
        FoundationInputProgress::Pending
    );
    let context = &input.serving.owner.context;
    assert!(context.generation.is_some(), "retry admits generation");
    assert!(Arc::ptr_eq(
        context
            .place_engine
            .as_ref()
            .unwrap()
            .outstanding
            .as_ref()
            .unwrap(),
        &original.0
    ));
    assert!(
        input
            .engine
            .as_ref()
            .unwrap()
            .emission
            .as_ref()
            .unwrap()
            .corrections
            .last()
            .unwrap()
            .placement
            .is_none(),
        "the admitted generation owns the grant"
    );
    assert!(
        context.deadlines.get(PumpDeadline::Decision).is_none(),
        "accepted generation retires the admission bound"
    );
    // The admission clock cannot limit the actual correction decode. Its
    // checked output will receive a new Decision budget when it arrives.
    io.clock.advance_to(io.bound.unwrap() + 1).unwrap();
    false
}
