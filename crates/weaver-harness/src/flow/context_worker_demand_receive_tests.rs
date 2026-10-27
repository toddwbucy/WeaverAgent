//! Admin dials first, then sends during the worker's pre-receive clock call.
//! Both clock and socket waits remain real; only scheduling is controlled.
use super::*;
use crate::flow::clock::{MonotonicIo, PumpIo, WaitInterest, WaitOutcome};
use crate::flow::context::worker_startup::input::FoundationInputProgress;
use crate::flow::context::worker_startup::input::management::FoundationManagementProgress;
use std::os::fd::OwnedFd;

struct SendAfterClock {
    peer: OwnedFd,
    duration: u64,
    calls: usize,
    before: Option<u64>,
    window: Option<s::SaveWindow>,
}
impl PumpIo for SendAfterClock {
    fn now(&mut self) -> std::io::Result<u64> {
        let before = MonotonicIo.now()?;
        self.calls += 1;
        // The read is already armed. Management checks liveness first and
        // the common receiving pump checks it second. Delay that second
        // sample's return, allowing admin to send after the sample but before
        // the recvmsg which the very same pump invocation performs.
        if self.calls == 2 {
            std::thread::sleep(Duration::from_millis(2));
            let began = MonotonicIo.now()?;
            assert!(began > before);
            let window = s::SaveWindow {
                began_ns: began.into(),
                deadline_ns: (began + self.duration).into(),
            };
            let packet = CaptureEnvelope {
                exchange: weaver_types::ExchangeId {
                    opener: Opener::Admin,
                    ordinal: 701,
                },
                position: Position::Open,
                payload: CapturePayload::Directive(CaptureDirective::SavePoint {
                    cause: weaver_types::Cause { uid: 74 },
                    window,
                }),
            }
            .encode(CaptureSender::Admin)
            .unwrap();
            assert_eq!(
                send(self.peer.as_raw_fd(), &packet, MsgFlags::MSG_NOSIGNAL),
                Ok(packet.len())
            );
            self.before = Some(before);
            self.window = Some(window);
        }
        Ok(before)
    }
    fn wait(
        &mut self,
        interests: &[WaitInterest<'_>],
        deadline: Option<u64>,
    ) -> std::io::Result<WaitOutcome> {
        MonotonicIo.wait(interests, deadline)
    }
}

pub(in crate::flow::context::worker_startup) fn exercise(
    serving: FoundationServing<'_>,
    socket: &Path,
    close_before_projection: bool,
) {
    let duration = serving
        .owner
        .context
        .load_duty
        .as_ref()
        .unwrap()
        .persistence
        .save_wait
        .elapsed_ns
        .0;
    let mut guard = Guard {
        input: Some(serving.input()),
        management: None,
    };
    let peer = dial(socket);
    let end = Instant::now() + Duration::from_secs(3);
    while guard.input.as_mut().unwrap().advance() != FoundationInputProgress::ManagementPending {
        assert!(Instant::now() < end, "actual admin dial must be retained");
    }
    guard.management = Some(guard.input.take().unwrap().management().ok().unwrap());
    assert_eq!(
        guard.management.as_mut().unwrap().advance(),
        FoundationManagementProgress::Pending
    );
    let mut clock = SendAfterClock {
        peer,
        duration,
        calls: 0,
        before: None,
        window: None,
    };
    assert_eq!(
        guard
            .management
            .as_mut()
            .unwrap()
            .advance_with_io(&mut clock),
        FoundationManagementProgress::DirectivePending
    );
    let window = clock
        .window
        .expect("the same receiving pump carried the request");
    assert!(clock.before.unwrap() < window.began_ns.0);
    let owner = guard.management.take().unwrap();
    if close_before_projection {
        drop(clock.peer);
        guard.input = Some(
            crate::flow::context::worker_startup::input::management::tests::closed_save_projection(
                owner,
            ),
        );
        assert!(guard.input.as_ref().unwrap().serving.save.is_none());
        assert!(
            guard
                .input
                .as_ref()
                .unwrap()
                .serving
                .owner
                .context
                .failure()
                .is_none()
        );
        return;
    }
    match owner.queue_save() {
        Ok(input) => guard.input = Some(input),
        Err(owner) => {
            let detail = owner.detail().map(str::to_owned);
            guard.management = Some(owner);
            panic!(
                "valid save sampled after dial and received after send was rejected: {detail:?}"
            );
        }
    }
    let input = guard.input.as_ref().unwrap();
    assert!(input.serving.owner.context.failure().is_none());
    let save = input.serving.save.as_ref().unwrap();
    let CapturePayload::Directive(CaptureDirective::SavePoint {
        window: retained, ..
    }) = save.request.message.envelope.payload
    else {
        panic!("original SavePoint");
    };
    assert_eq!(retained, window);
}
