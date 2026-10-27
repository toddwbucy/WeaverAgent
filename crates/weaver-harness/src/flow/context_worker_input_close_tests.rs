//! The refusal's actual gate send has one bound and an observed whole receipt.
use super::*;
use crate::flow::clock::{WaitInterest, WaitOutcome};
use crate::flow::pump::PumpDeadline;

pub(super) fn failed_send(
    input: &mut FoundationInput<'_>,
    clock: &mut FakePumpIo,
    root: &Path,
    mode: &str,
) {
    let bound = input
        .serving
        .owner
        .context
        .deadlines
        .get(PumpDeadline::Decision)
        .expect("required send must be bounded");
    let request = input.received.as_ref().unwrap().envelope().clone();
    for _ in 0..3 {
        assert_eq!(
            input.advance_with_io(&mut CircuitIo(clock)),
            FoundationInputProgress::Pending
        );
        assert_eq!(
            input
                .serving
                .owner
                .context
                .deadlines
                .get(PumpDeadline::Decision),
            Some(bound)
        );
        assert!(!input.send.as_ref().unwrap().sent());
    }
    assert!(!root.join("closed.json").exists());
    if mode.ends_with("deadline") {
        // A non-ready transport waits to the original bound, without the
        // all-interests-ready fixture concealing an unbounded wait.
        assert_eq!(
            input.advance_with_io(clock),
            FoundationInputProgress::Stopped
        );
        assert!(matches!(
            input.serving.owner.context.failure(),
            Some(RunFailure::Flow {
                reason: FlowHaltReason::DecisionBound,
                ..
            })
        ));
        assert_eq!(clock.now().unwrap(), bound.deadline_ns());
        assert!(!input.send.as_ref().unwrap().sent());
        assert!(!root.join("closed.json").exists());
    } else {
        struct BrokenClock<'a> {
            clock: &'a mut FakePumpIo,
            sent: Box<dyn Fn() -> bool>,
        }
        impl PumpIo for BrokenClock<'_> {
            fn now(&mut self) -> std::io::Result<u64> {
                if (self.sent)() {
                    Err(std::io::Error::other("input send clock fixture"))
                } else {
                    self.clock.now()
                }
            }
            fn wait(
                &mut self,
                interests: &[WaitInterest<'_>],
                deadline: Option<u64>,
            ) -> std::io::Result<WaitOutcome> {
                CircuitIo(self.clock).wait(interests, deadline)
            }
        }
        let sent = input.send.as_ref().unwrap().sent_probe_for_test();
        std::fs::write(root.join("drain"), b"yes").unwrap();
        let mut io = BrokenClock { clock, sent };
        let end = Instant::now() + Duration::from_secs(3);
        while input.advance_with_io(&mut io) != FoundationInputProgress::Stopped {
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!((io.sent)());
        assert!(matches!(
            input.serving.owner.context.failure(),
            Some(RunFailure::Flow {
                reason: FlowHaltReason::TraceFailed,
                point: HaltPoint::Author,
                ..
            })
        ));
        assert!(input.send.as_ref().unwrap().sent_at().is_none());
    }
    assert_eq!(input.received.as_ref().unwrap().envelope(), &request);
    assert!(
        !input
            .serving
            .owner
            .context
            .input
            .snapshot()
            .unwrap()
            .response_sent
    );
    std::fs::write(root.join("drain"), b"yes").unwrap();
}
