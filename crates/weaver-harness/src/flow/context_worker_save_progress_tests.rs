//! Actual capture/member replies and decode EOF with controlled non-ready wakes.
use super::*;
use crate::flow::clock::testing::{NoReadyIo, NonblockingIo};
use crate::flow::context::RunFailure;
use crate::flow::context::worker_startup::input::FoundationInput;
use crate::flow::pump::PumpSource;

/// Only the actual Requested author waiting on recorder capacity releases the
/// SPU. Its real EOF interrupts that same author before the recorder is drained.
struct RequestedEofIo<'a> {
    root: &'a Path,
    waited: bool,
    resume_claim:
        Option<std::sync::Arc<crate::flow::context::serving_observation::ServingObservationAuthor>>,
}

impl crate::flow::clock::PumpIo for RequestedEofIo<'_> {
    fn now(&mut self) -> std::io::Result<u64> {
        crate::flow::clock::PumpIo::now(&mut crate::flow::clock::MonotonicIo)
    }

    fn wait(
        &mut self,
        interests: &[crate::flow::clock::WaitInterest<'_>],
        deadline: Option<u64>,
    ) -> std::io::Result<crate::flow::clock::WaitOutcome> {
        assert!(
            self.resume_claim
                .as_ref()
                .is_none_or(|claim| claim.sequence().is_none()),
            "Requested was Recorded; return to the owed organ observation before another wait"
        );
        if !self.waited {
            self.waited = true;
            std::fs::write(
                self.root.join("organ-loss-requested-author-waiting"),
                b"actual Requested author is waiting for recorder capacity",
            )?;
        }
        crate::flow::clock::PumpIo::wait(&mut crate::flow::clock::MonotonicIo, interests, deadline)
    }
}

pub(super) fn keep_requested_author_behind_observation(
    input: &mut FoundationInput<'_>,
    root: &Path,
    hold_trace: &AtomicBool,
    trace_paused: &AtomicBool,
) {
    use crate::flow::context::serving_organ_loss::ObservationProgress;

    let end = Instant::now() + Duration::from_secs(10);
    hold_trace.store(true, Ordering::SeqCst);
    while !trace_paused.load(Ordering::SeqCst) {
        assert!(Instant::now() < end, "actual trace reader did not pause");
        std::thread::yield_now();
    }
    let context = &mut input.serving.owner.context;
    let window = context.deadlines.get(PumpDeadline::SaveQueued);
    assert!(window.is_some());
    let mut clock = crate::flow::clock::testing::FakePumpIo::new(context.last_ns);
    crate::flow::context::worker_startup::input::management::stop::tests::fill_trace(
        context, &mut clock,
    );
    assert_eq!(
        context.barrier.recorder().boundary().queued,
        serving::ORDINARY_CAPACITY
    );
    let mut io = RequestedEofIo {
        root,
        waited: false,
        resume_claim: None,
    };
    let progress = input.advance_with_io(&mut io);
    assert!(
        input.detail().is_none(),
        "{progress:?}: {:?}",
        input.detail()
    );
    assert!(
        io.waited,
        "Requested must encounter actual recorder pressure"
    );
    let context = &input.serving.owner.context;
    assert!(
        context.serving_organs.no_residency(),
        "actual SPU EOF must interrupt the pending Requested author"
    );
    let pending = context.pending_author().expect("original Requested author");
    assert_eq!(pending.event.kind, serving::Kind::SavePoint);
    let claim = context.serving_observation_author.as_ref().unwrap().clone();
    assert_eq!(claim.ticket(), Some(pending.ticket));
    assert!(claim.sequence().is_none());
    assert!(
        !input
            .serving
            .save
            .as_ref()
            .unwrap()
            .capture
            .boundary_pending()
    );

    while !input
        .serving
        .owner
        .resident
        .spu
        .last_word_snapshot()
        .is_some_and(|word| word.eof)
    {
        assert!(Instant::now() < end, "actual SPU stderr did not finish");
        std::thread::yield_now();
    }
    loop {
        match input.advance_organ_observations(None, &mut io) {
            Ok(ObservationProgress::EarlierAuthor) => break,
            Ok(_) => panic!("the original Requested author must keep its claim"),
            Err(failure) => {
                assert!(matches!(
                    failure,
                    RunFailure::Transport {
                        source: PumpSource::Residency,
                        ..
                    }
                ));
                // The real process exits after closing decode. Its separate
                // residency EOF follows the normal input owner's retention
                // path before we inspect the still-pending author claim.
                assert!(input.retain_organ_end(&failure, &mut io).unwrap());
            }
        }
        assert!(Instant::now() < end, "actual residency EOF did not settle");
    }
    assert!(std::sync::Arc::ptr_eq(
        input
            .serving
            .owner
            .context
            .serving_observation_author
            .as_ref()
            .unwrap(),
        &claim
    ));
    hold_trace.store(false, Ordering::SeqCst);
    io.resume_claim = Some(std::sync::Arc::clone(&claim));
    let progress = input.advance_with_io(&mut io);
    assert!(
        input.detail().is_none(),
        "{progress:?}: {:?}",
        input.detail()
    );
    assert!(
        claim.sequence().is_some(),
        "resume the same Requested author"
    );
    assert!(
        input
            .serving
            .save
            .as_ref()
            .unwrap()
            .capture
            .boundary_pending()
    );
    let context = &input.serving.owner.context;
    assert!(context.serving_observation_author.is_none());
    assert!(context.has_owed_organ_observation());
    assert_eq!(context.deadlines.get(PumpDeadline::SaveQueued), window);
    assert!(
        context.deadlines.get(PumpDeadline::CaptureWindow).is_none(),
        "EarlierAuthor permits its owner to finish, not a capture boundary"
    );
    assert!(context.capture_snapshot().is_none());
}

pub(super) fn reduce_while_last_word_is_pending(input: &mut FoundationInput<'_>, root: &Path) {
    wait_file(root, "organ-loss-decode-closed");
    wait_file(root, "organ-loss-member-paused");
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
    assert!(
        !root.join("organ-loss-finished-carried").exists(),
        "the actual member has not answered Acknowledge yet"
    );
    let mut io = NonblockingIo { waits: io.waits };
    std::fs::write(
        root.join("organ-loss-member-resume"),
        b"allow actual Finished",
    )
    .unwrap();
    while !root.join("organ-loss-finished-carried").exists() {
        assert!(
            !input.serving.save.as_ref().unwrap().reply_held(),
            "Prepared remains withheld while member completion progresses"
        );
        let before_waits = io.waits;
        let progress = input.advance_with_io(&mut io);
        assert!(
            input.detail().is_none(),
            "{progress:?}: {:?}",
            input.detail()
        );
        assert!(io.waits - before_waits <= 1);
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
            "member completion must progress before stderr settlement finishes"
        );
        assert!(
            Instant::now() < end,
            "no admin packet is held, but the original member Finished must reach admin"
        );
        std::thread::yield_now();
    }
    assert!(!input.serving.save.as_ref().unwrap().reply_held());
    std::fs::write(
        root.join("organ-loss-held-reply-reduced"),
        b"original capture progressed",
    )
    .unwrap();
}

/// Keep either Requested or QueueBoundary behind real EOF settlement. Requested
/// must finish accounting without gaining permission to begin in the same call.
pub(super) fn keep_queued_boundary_behind_settlement(
    input: &mut FoundationInput<'_>,
    root: &Path,
    requested: bool,
) {
    let end = Instant::now() + Duration::from_secs(3);
    if !requested {
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
    }
    assert_eq!(
        input
            .serving
            .save
            .as_ref()
            .unwrap()
            .capture
            .boundary_pending(),
        !requested
    );
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
        b"original queued save retained, boundary not elected",
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
    loop {
        input.advance_with_io(&mut io);
        assert!(input.detail().is_none(), "{:?}", input.detail());
        assert_eq!(
            input
                .serving
                .owner
                .context
                .deadlines
                .get(PumpDeadline::SaveQueued),
            original,
            "queued capture began during organ settlement"
        );
        assert!(
            input
                .serving
                .owner
                .context
                .deadlines
                .get(PumpDeadline::CaptureWindow)
                .is_none(),
            "finishing Requested cannot begin capture in the same advance call"
        );
        assert!(input.serving.owner.context.capture_snapshot().is_none());
        assert!(!input.advance_organ_faults(None, &mut io).unwrap());
        if input
            .serving
            .save
            .as_ref()
            .unwrap()
            .capture
            .boundary_pending()
        {
            break;
        }
        assert!(
            Instant::now() < end,
            "Requested accounting must finish while organ settlement remains pending"
        );
        std::thread::yield_now();
    }
    std::fs::write(
        root.join("organ-loss-queued-boundary-checked"),
        b"original queued allowance retained",
    )
    .unwrap();
}
