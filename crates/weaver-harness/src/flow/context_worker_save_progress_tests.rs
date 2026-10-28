//! Actual capture reply and decode EOF; one non-ready scheduling step is controlled.
use super::*;
use crate::flow::clock::testing::NoReadyIo;
use crate::flow::context::RunFailure;
use crate::flow::context::worker_startup::input::FoundationInput;
use crate::flow::pump::PumpSource;

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
    let before_waits = io.waits;
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
        io.waits - before_waits <= 1,
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

/// Finish the actual queued Requested author while leaving boundary election to
/// the outer input owner. Then observe real EOF with stderr held open and prove
/// that the next input step cannot begin this queued capture before settlement.
pub(super) fn keep_queued_boundary_behind_settlement(input: &mut FoundationInput<'_>, root: &Path) {
    let end = Instant::now() + Duration::from_secs(3);
    loop {
        let save = input.serving.save.as_mut().unwrap();
        if save.capture.boundary_pending() {
            break;
        }
        let owner = &mut input.serving.owner;
        save.advance(
            owner.context,
            crate::flow::context::PumpPorts {
                lifecycle: crate::flow::pump::LifecyclePorts {
                    listener: Some(&mut owner.listener),
                    residency: Some(owner.resident.residency),
                    ..Default::default()
                },
                gate: Some(&mut owner.gate),
                decode: Some(owner.decode),
                label: None,
                io: &mut crate::flow::clock::MonotonicIo,
            },
            false,
        )
        .unwrap();
        assert!(
            Instant::now() < end,
            "queued Requested author did not settle"
        );
    }
    let original = input
        .serving
        .owner
        .context
        .deadlines
        .get(PumpDeadline::SaveQueued);
    assert!(original.is_some());
    assert!(!input.serving.save.as_ref().unwrap().reply_held());
    assert!(
        input
            .serving
            .owner
            .context
            .deadlines
            .get(PumpDeadline::CaptureWindow)
            .is_none()
    );
    std::fs::write(
        root.join("organ-loss-save-queued"),
        b"Requested recorded, boundary not elected",
    )
    .unwrap();
    wait_file(root, "organ-loss-decode-closed");
    let failure = loop {
        if let Err(failure) = input.serving.owner.decode.try_receive() {
            break failure;
        }
        assert!(
            Instant::now() < end,
            "the actual queued-save EOF must be observed"
        );
        std::thread::yield_now();
    };
    let mut io = NoReadyIo {
        now: crate::flow::clock::monotonic_ns().unwrap(),
        waits: 0,
    };
    assert!(
        input
            .retain_organ_end(
                &RunFailure::Transport {
                    source: PumpSource::Decode,
                    failure
                },
                &mut io
            )
            .unwrap()
    );
    assert!(!input.advance_organ_faults(None, &mut io).unwrap());
    input.advance_with_io(&mut io);
    assert!(input.detail().is_none(), "{:?}", input.detail());
    assert!(
        input
            .serving
            .save
            .as_ref()
            .unwrap()
            .capture
            .boundary_pending(),
        "queued capture began during organ settlement"
    );
    assert_eq!(
        input
            .serving
            .owner
            .context
            .deadlines
            .get(PumpDeadline::SaveQueued),
        original
    );
    assert!(
        input
            .serving
            .owner
            .context
            .deadlines
            .get(PumpDeadline::CaptureWindow)
            .is_none()
    );
    assert!(input.serving.owner.context.capture_snapshot().is_none());
    assert!(!input.advance_organ_faults(None, &mut io).unwrap());
    std::fs::write(
        root.join("organ-loss-queued-boundary-checked"),
        b"original queued allowance retained",
    )
    .unwrap();
}
