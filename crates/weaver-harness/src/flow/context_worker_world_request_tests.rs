//! Later world requests use the actual dead-SPU run, recorder and gate.
use super::*;
use crate::flow::clock::{MonotonicIo, PumpIo, WaitInterest, WaitOutcome};
use weaver_types::{ExchangeId, TurnFrame};

pub(super) fn request(index: u64) -> OrganEnvelope {
    OrganEnvelope {
        exchange: ExchangeId {
            opener: Opener::Gate,
            ordinal: 300 + index,
        },
        position: Position::Open,
        payload: Payload::Frame(TurnFrame::carry_from(
            if index == 0 {
                b"\xffnot JSON"
            } else {
                br#"{"text":"Read file."}"#
            },
            42,
        )),
    }
}

struct ReleaseTrace<'a> {
    hold: &'a AtomicBool,
    root: &'a Path,
    deadline: u64,
    waited: bool,
}
impl PumpIo for ReleaseTrace<'_> {
    fn now(&mut self) -> std::io::Result<u64> {
        MonotonicIo.now()
    }
    fn wait(
        &mut self,
        interests: &[WaitInterest<'_>],
        deadline: Option<u64>,
    ) -> std::io::Result<WaitOutcome> {
        if !self.waited {
            assert!(
                !self.root.join("organ-loss-world-0").exists(),
                "world close preceded Recorded refusal"
            );
            assert!(
                deadline.is_some_and(|deadline| deadline <= self.deadline),
                "trace pressure renewed or lost gate_wait"
            );
            self.waited = true;
            self.hold.store(false, Ordering::SeqCst);
        }
        MonotonicIo.wait(interests, deadline)
    }
}

pub(super) fn under_pressure(
    input: &mut crate::flow::context::worker_startup::input::FoundationInput<'_>,
    hold: &AtomicBool,
    root: &Path,
) -> FoundationInputProgress {
    let context = &mut input.serving.owner.context;
    assert!(context.serving_organs.no_residency());
    assert!(hold.load(Ordering::SeqCst));
    let before = context.input.snapshot().unwrap();
    assert!(before.request_id.is_none());
    let mut clock = crate::flow::clock::testing::FakePumpIo::new(context.last_ns);
    crate::flow::context::worker_startup::input::management::stop::tests::fill_trace(
        context, &mut clock,
    );
    assert_eq!(
        context.barrier.recorder().boundary().queued,
        serving::ORDINARY_CAPACITY
    );
    assert!(context.input_close().is_none());
    let original = input.received.as_ref().unwrap().envelope().clone();
    let mut io = ReleaseTrace {
        hold,
        root,
        deadline: before.bound.deadline_ns(),
        waited: false,
    };
    let progress = input.advance_with_io(&mut io);
    assert!(
        io.waited,
        "exercise the real full recorder before releasing it"
    );
    assert_eq!(input.received.as_ref().unwrap().envelope(), &original);
    assert!(
        input.serving.owner.context.failure().is_none(),
        "{:?}",
        input.detail()
    );
    assert!(
        input
            .serving
            .owner
            .context
            .input
            .snapshot()
            .unwrap()
            .request_id
            .is_none()
    );
    progress
}

pub(super) fn queue_during_leave(
    input: &mut crate::flow::context::worker_startup::input::FoundationInput<'_>,
    root: &Path,
) {
    assert!(input.has_graceful_leave());
    assert_eq!(input.progress(), FoundationInputProgress::LeavePending);
    std::fs::write(root.join("organ-loss-leave-ready"), b"accepted Leave").unwrap();
    wait_file(root, "organ-loss-leave-world-queued");
    let end = Instant::now() + Duration::from_secs(3);
    while input.serving.owner.gate.held().is_none() {
        input.serving.owner.gate.try_receive().unwrap();
        assert!(Instant::now() < end);
        std::thread::yield_now();
    }
    input.serving.retain_queued_input(&mut MonotonicIo).unwrap();
    assert_eq!(input.advance(), FoundationInputProgress::LeavePending);
    assert!(input.serving.owner.context.input.snapshot().is_none());
    assert_eq!(
        input.serving.queued_inputs.front().unwrap().envelope(),
        &request(2)
    );
}

pub(super) fn assert_requests(rows: &[serde_json::Value], count: usize) {
    let offered: Vec<_> = rows
        .iter()
        .filter(|row| row["kind"] == "input.offered")
        .collect();
    assert_eq!(offered.len(), count);
    for (index, offered) in offered.iter().enumerate() {
        let Payload::Frame(frame) = request(index as u64).payload else {
            unreachable!()
        };
        assert_eq!(offered["payload"]["octets"], frame.octets);
        assert_eq!(offered["payload"]["input_id"], (index + 1).to_string());
        let refused: Vec<_> = rows
            .iter()
            .filter(|row| row["kind"] == "refusal" && row["causal_parent"] == offered["sequence"])
            .collect();
        assert_eq!(refused.len(), 1);
        let refused = refused[0];
        assert_eq!(refused["run"], offered["run"]);
        assert!(refused["turn"].is_null());
        assert_eq!(refused["channel"], "internal");
        assert_eq!(refused["origin"], "runtime");
        assert_eq!(refused["subsystem"], "harness");
        assert_eq!(
            refused["payload"],
            serde_json::json!({"seam":"lifecycle",
            "asked":{"ask":"world_request"}, "refusal":{"kind":"no_residency"}})
        );
    }
    assert!(
        !rows
            .iter()
            .any(|row| row["kind"] == "ste100.result"
                && row["payload"]["target"]["input"].is_object())
    );
}
