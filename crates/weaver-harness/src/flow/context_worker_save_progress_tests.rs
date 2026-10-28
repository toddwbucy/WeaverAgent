//! Actual capture reply and decode EOF; one non-ready scheduling step is controlled.
use super::*;
use crate::flow::clock::{PumpIo, WaitInterest, WaitOutcome};
use crate::flow::context::RunFailure;
use crate::flow::context::worker_startup::input::FoundationInput;
use crate::flow::pump::PumpSource;

struct NoReadyIo {
    now: u64,
    waits: usize,
}
impl PumpIo for NoReadyIo {
    fn now(&mut self) -> std::io::Result<u64> {
        Ok(self.now)
    }

    fn wait(&mut self, _: &[WaitInterest<'_>], _: Option<u64>) -> std::io::Result<WaitOutcome> {
        self.waits += 1;
        // No descriptor becomes ready and no deadline expires during this
        // controlled step. Held work must progress without another readiness.
        Ok(WaitOutcome::Ready(Vec::new()))
    }
}

pub(super) fn reduce_while_last_word_is_pending(input: &mut FoundationInput<'_>, root: &Path) {
    wait_file(root, "organ-loss-decode-closed");
    let end = Instant::now() + Duration::from_secs(3);
    let failure = loop {
        if let Err(failure) = input.serving.owner.decode.try_receive() {
            break failure;
        }
        assert!(
            Instant::now() < end,
            "the actual decode EOF must be observed"
        );
        std::thread::yield_now();
    };
    std::fs::write(root.join("organ-loss-pin-ready"), b"reader ready").unwrap();
    let save = input.serving.save.as_mut().unwrap();
    while save.peer.capture_held().is_none() {
        save.peer.try_receive().unwrap();
        assert!(
            Instant::now() < end,
            "admin must supply its actual ImageChecked"
        );
        std::thread::yield_now();
    }
    assert!(matches!(
        save.peer.capture_held().unwrap().envelope.payload,
        CapturePayload::SavePointImageChecked(_)
    ));
    let mut io = NoReadyIo {
        now: crate::flow::clock::monotonic_ns().unwrap(),
        waits: 0,
    };
    assert!(
        input
            .retain_organ_end(
                &RunFailure::Transport {
                    source: PumpSource::Decode,
                    failure,
                },
                &mut io
            )
            .unwrap()
    );
    assert!(
        !input.advance_organ_faults(None, &mut io).unwrap(),
        "the real SPU keeps stderr open until the held capture reply is reduced"
    );
    let window = input
        .serving
        .owner
        .context
        .deadlines
        .get(PumpDeadline::CaptureWindow);
    assert!(window.is_some());
    let progress = input.advance_with_io(&mut io);
    assert!(
        input.detail().is_none(),
        "{progress:?}: {:?}",
        input.detail()
    );
    assert!(
        input
            .serving
            .save
            .as_ref()
            .unwrap()
            .peer
            .capture_held()
            .is_none(),
        "pending organ settlement must not strand an already-held ImageChecked"
    );
    assert!(
        io.waits <= 1,
        "save progress must not be followed by another blocking wait"
    );
    assert_eq!(
        input
            .serving
            .owner
            .context
            .deadlines
            .get(PumpDeadline::CaptureWindow),
        window
    );
    assert!(
        !input.advance_organ_faults(None, &mut io).unwrap(),
        "save progress did not renew or expire the organ settlement interval"
    );
    std::fs::write(
        root.join("organ-loss-held-reply-reduced"),
        b"original capture progressed",
    )
    .unwrap();
}
