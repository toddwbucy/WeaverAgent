//! Filing admission can yield a nonfatal lifecycle wait after building its event.
//! Each refused original must return without replacing an earlier caller's evidence.
use super::*;
use crate::flow::author::AuthoredSource;
use crate::flow::clock::testing::FakePumpIo;
use crate::flow::clock::{WaitInterest, WaitOutcome};
use weaver_trace::serving::payloads as trace;

struct FilingBoundaryClock<'a> {
    clock: &'a mut FakePumpIo,
    samples: usize,
    deadline: u64,
}
impl PumpIo for FilingBoundaryClock<'_> {
    fn now(&mut self) -> std::io::Result<u64> {
        self.samples += 1;
        if self.samples == 2 {
            self.clock.advance_to(self.deadline)?;
        }
        self.clock.now()
    }
    fn wait(
        &mut self,
        interests: &[WaitInterest<'_>],
        deadline: Option<u64>,
    ) -> std::io::Result<WaitOutcome> {
        self.clock.wait(interests, deadline)
    }
}

fn terminal_ready() -> (PairFixture, s::CallId, weaver_trace::TurnRef) {
    let mut fx = PairFixture::new();
    let original = fx.generation(vec![text("Read files."), call("ls")]);
    let call = original.requests()[0].opening().call_id.clone();
    let turn = original.generation().original().turn.clone();
    fx.begin(original, s::NonDispatchCause::NoGate);
    for _ in 0..512 {
        fx.step(Hold::None);
        let context = &fx.output.input.run.context;
        if context.pair_work.as_ref().unwrap().leg.is_none()
            && context.pairs.next_terminal().is_some()
        {
            return (fx, call, turn);
        }
    }
    panic!("terminal filing did not become ready");
}

fn expire_filing(fx: &mut PairFixture, deadline: u64) -> AuthorFailure {
    let run = &mut fx.output.input.run;
    let mut clock = FilingBoundaryClock {
        clock: &mut run.clock,
        samples: 0,
        deadline,
    };
    let failure = run
        .context
        .advance_pairs(&mut ports(&mut clock))
        .unwrap_err();
    assert!(
        failure.original.is_some(),
        "expiry must reach filing admission"
    );
    assert!(run.context.pair_work.as_ref().unwrap().leg.is_none());
    assert!(run.context.pending_author().is_none());
    failure
}

fn expire_leave(fx: &mut PairFixture) -> (AuthorFailure, AbsoluteBound) {
    let run = &mut fx.output.input.run;
    let leave = AbsoluteBound::after(run.clock.now().unwrap(), 10).unwrap();
    run.context
        .arm(PumpDeadline::Leave, leave, &mut run.clock)
        .unwrap();
    let failure = expire_filing(fx, leave.deadline_ns());
    assert_eq!(
        failure.reason,
        RunFailure::Wait {
            key: PumpDeadline::Leave,
            bound: leave
        }
    );
    assert!(fx.output.input.run.context.failure().is_none());
    (failure, leave)
}

fn assert_original(
    failure: &AuthorFailure,
    call: &s::CallId,
    turn: &weaver_trace::TurnRef,
    cause: trace::NonDispatchCause,
) {
    let event = failure.original.as_ref().unwrap();
    assert_eq!(event.kind, Kind::ToolCallCompleted);
    assert_eq!(event.subsystem, weaver_trace::Subsystem::Harness);
    assert_eq!(event.turn.as_ref(), Some(turn));
    assert!(matches!(event.source, AuthoredSource::Loop));
    assert_eq!(event.causal_parent, Some(Sequence(call.sequence.0)));
    let Some(Payload::PairEnded(terminal)) = &event.payload else {
        panic!("returned original lost its terminal payload");
    };
    assert_eq!(
        serde_json::to_value(&terminal.call_id).unwrap(),
        serde_json::to_value(call).unwrap()
    );
    assert_eq!(
        terminal.reason,
        Some(trace::PairFailure::NotDispatched { cause })
    );
    assert_eq!(terminal.fate, trace::TerminalPairFate::Failed);
    assert!(terminal.execution.is_none());
    assert!(terminal.outcome.is_none());
    assert!(terminal.loaded_from.is_none());
}

#[test]
fn pair_filing_returns_each_original_across_two_leave_expiries() {
    let (mut fx, call, turn) = terminal_ready();
    let bound = fx
        .output
        .input
        .run
        .context
        .deadlines
        .get(PumpDeadline::Decision);
    let (first, first_leave) = expire_leave(&mut fx);
    assert_original(&first, &call, &turn, trace::NonDispatchCause::NoGate);
    assert_eq!(
        fx.output
            .input
            .run
            .context
            .complete_wait(PumpDeadline::Leave),
        Some(first_leave)
    );

    // The first unadmitted terminal is still owned by this caller. Stop changes
    // only the unclaimed ledger candidate, not that earlier returned original.
    let run = &mut fx.output.input.run;
    run.context
        .bar_pending_pairs(s::NonDispatchCause::Stop, &mut run.clock)
        .unwrap();
    let (second, second_leave) = expire_leave(&mut fx);
    assert_original(&first, &call, &turn, trace::NonDispatchCause::NoGate);
    assert_original(&second, &call, &turn, trace::NonDispatchCause::Stop);
    assert_eq!(
        fx.output
            .input
            .run
            .context
            .complete_wait(PumpDeadline::Leave),
        Some(second_leave)
    );
    assert_eq!(
        fx.output
            .input
            .run
            .context
            .deadlines
            .get(PumpDeadline::Decision),
        bound
    );
    assert!(fx.output.input.records(Kind::ToolCallCompleted).is_empty());

    // No failed attempt claimed the terminal or acquired a ticket. Once the
    // independent Leave wait is retired, only the current Stop terminal files.
    fx.complete();
    let run = &mut fx.output.input.run;
    let completed = run
        .context
        .completed_pair(&call, &mut run.clock)
        .unwrap()
        .unwrap();
    assert_eq!(
        completed.ended().reason,
        Some(s::PairFailure::NotDispatched {
            cause: s::NonDispatchCause::Stop
        })
    );
    assert_eq!(fx.output.input.records(Kind::ToolCallCompleted).len(), 1);
    assert_original(&first, &call, &turn, trace::NonDispatchCause::NoGate);
    assert_original(&second, &call, &turn, trace::NonDispatchCause::Stop);
}

#[test]
fn pair_filing_fatal_refusal_keeps_earlier_returned_original_and_first_cause() {
    let (mut fx, call, turn) = terminal_ready();
    let (first, leave) = expire_leave(&mut fx);
    assert_eq!(
        fx.output
            .input
            .run
            .context
            .complete_wait(PumpDeadline::Leave),
        Some(leave)
    );
    let decision = fx
        .output
        .input
        .run
        .context
        .deadlines
        .get(PumpDeadline::Decision)
        .unwrap();
    let fatal = expire_filing(&mut fx, decision.deadline_ns());
    assert!(matches!(
        fatal.reason,
        RunFailure::Flow {
            reason: FlowHaltReason::DecisionBound,
            ..
        }
    ));
    assert_original(&first, &call, &turn, trace::NonDispatchCause::NoGate);
    assert_original(&fatal, &call, &turn, trace::NonDispatchCause::NoGate);
    let run = &mut fx.output.input.run;
    let later = run
        .context
        .advance_pairs(&mut ports(&mut run.clock))
        .unwrap_err();
    assert!(
        later.original.is_none(),
        "latched failure starts no new attempt"
    );
    assert_eq!(later.reason, fatal.reason);
    assert_eq!(run.context.failure(), Some(&fatal.reason));
    assert!(fx.output.input.records(Kind::ToolCallCompleted).is_empty());
}
