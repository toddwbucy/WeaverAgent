//! Invalid cleanup and publication cross the public failed-run handoffs with
//! the original capture account, then close the actual member and children.
use super::*;
use crate::flow::context::worker_startup::FoundationRun;
use crate::flow::context::worker_startup::input::halt::{
    FoundationHaltProgress,
    exit::{FoundationExitProgress, FoundationFailedHandoff},
};
use crate::flow::report::CaptureState;
use weaver_trace::Sequence;

pub(in crate::flow::context::worker_startup) fn is_mode(mode: &str) -> bool {
    super::is_mode(mode) && (mode.ends_with("-bad-published") || mode.ends_with("-bad-aborted"))
}

thread_local! {
    static FAILED: std::cell::RefCell<Option<(Box<FoundationFailedHandoff>, Vec<serde_json::Value>)>> = const { std::cell::RefCell::new(None) };
}

pub(super) fn retain(handoff: Box<FoundationFailedHandoff>, rows: Vec<serde_json::Value>) {
    FAILED.with(|slot| assert!(slot.borrow_mut().replace((handoff, rows)).is_none()));
}

pub(super) fn handoff(
    input: Box<FoundationInput<'_>>,
    root: &Path,
    mode: &str,
) -> (Box<FoundationFailedHandoff>, Sequence) {
    let context = &input.serving.owner.context;
    let save = input
        .serving
        .save
        .as_ref()
        .expect("original save remains owned");
    let facts = save.capture.at_halt(context);
    assert_eq!(facts.commit_sent, mode.ends_with("-bad-published"));
    assert!(facts.published.is_none());
    assert_eq!(
        facts.state,
        if facts.commit_sent {
            CaptureState::AwaitPublished
        } else {
            CaptureState::Aborting
        }
    );
    assert!(context.failure().is_some());
    if mode.ends_with("-bad-aborted") {
        assert!(matches!(
            context.failure(),
            Some(RunFailure::Flow {
                reason: FlowHaltReason::ProtocolFailed,
                point: HaltPoint::StateReply,
                ..
            })
        ));
    }
    assert!(context.member_capture.is_some());
    let first = context.failure().unwrap().clone();
    let mut halt = input
        .halt()
        .ok()
        .expect("actual failed capture can enter halt");
    let end = Instant::now() + Duration::from_secs(5);
    loop {
        assert!(
            Instant::now() < end,
            "failed demand halt: {:?}",
            halt.detail()
        );
        if matches!(
            halt.advance(),
            FoundationHaltProgress::Prepared
                | FoundationHaltProgress::NeedsContainment
                | FoundationHaltProgress::Expired
        ) {
            break;
        }
    }
    let context = &halt.test_input().serving.owner.context;
    assert_eq!(context.failure(), Some(&first));
    assert_eq!(
        context.halt().unwrap().account().unwrap().capture.as_ref(),
        Some(&facts)
    );
    let through = context.barrier.recorder().boundary().admitted.unwrap();
    let handoff = halt
        .handoff()
        .ok()
        .expect("capture and request transfer together");
    std::fs::write(
        root.join("demand-failed-handoff"),
        b"retained failed capture",
    )
    .unwrap();
    (handoff, through)
}

pub(in crate::flow::context::worker_startup) fn finish(
    run: Box<FoundationRun>,
    member: &mut Member,
    reader: &mut UnixStream,
    root: &Path,
    mode: &str,
) {
    let (handoff, mut rows) = FAILED.with(|slot| slot.borrow_mut().take().unwrap());
    let facts = run
        .context
        .halt()
        .unwrap()
        .account()
        .unwrap()
        .capture
        .clone()
        .unwrap();
    let committed = mode.ends_with("-bad-published");
    assert_eq!(facts.commit_sent, committed);
    assert!(facts.published.is_none());
    let mut owner = run
        .failed_exit(handoff)
        .ok()
        .expect("original run accepts capture handoff");
    let done = AtomicBool::new(false);
    reader.set_nonblocking(true).unwrap();
    let bytes = std::thread::scope(|scope| {
        let responder = scope.spawn(|| {
            let end = Instant::now() + Duration::from_secs(15);
            while !done.load(Ordering::SeqCst) && Instant::now() < end {
                member.step_live(mode);
                std::thread::sleep(Duration::from_millis(1));
            }
        });
        let recorder = scope.spawn(|| {
            let mut bytes = vec![];
            while !done.load(Ordering::SeqCst) {
                let _ = reader.read_to_end(&mut bytes);
                std::thread::sleep(Duration::from_millis(1));
            }
            let _ = reader.read_to_end(&mut bytes);
            bytes
        });
        let _stop = StopThreads(&done);
        let end = Instant::now() + Duration::from_secs(12);
        while owner.progress() != FoundationExitProgress::Closed {
            assert!(Instant::now() < end, "failed demand exit did not finish");
            owner.advance();
            if owner.progress() == FoundationExitProgress::Pending {
                owner.wait_for_progress();
            }
        }
        done.store(true, Ordering::SeqCst);
        responder.join().unwrap();
        recorder.join().unwrap()
    });
    assert!(!owner.abandoned());
    assert!(matches!(
        owner.channel_fault(),
        Some(crate::ChannelFault::Closed)
    ));
    let through = Sequence(owner.unload_sequence().expect("actual failed unload"));
    rows.extend(
        trace_through(
            reader,
            bytes,
            through,
            Instant::now() + Duration::from_secs(3),
        )
        .unwrap(),
    );
    member.finish_normal();
    assert!(root.join("demand-failed-lower.json").exists());
    assert_eq!(
        member
            .requests
            .iter()
            .filter(|request| *request == "snapshot")
            .count(),
        1
    );
    assert!(
        !member
            .requests
            .iter()
            .any(|request| request == "release_capture"),
        "failure grants no healthy capture release"
    );
    assert_eq!(
        rows.iter()
            .filter(|row| row["kind"] == "turn.closed")
            .count(),
        1
    );
    let saves: Vec<_> = rows
        .iter()
        .filter(|row| row["kind"] == "save_point")
        .collect();
    assert_eq!(saves.len(), if committed { 2 } else { 1 });
    assert_eq!(saves[0]["payload"]["phase"], "requested");
    if committed {
        assert_eq!(saves[1]["payload"]["phase"], "confirmed");
    } else {
        assert!(root.join("demand-bad-aborted.jsonl").exists());
        assert_eq!(
            member
                .requests
                .iter()
                .filter(|request| *request == "abort")
                .count(),
            1
        );
    }
    assert_eq!(rows.last().unwrap()["kind"], "unload");
    assert_eq!(
        rows.last().unwrap()["payload"]["end_reason"],
        "component_failure"
    );
    assert_eq!(rows.iter().filter(|row| row["kind"] == "fault").count(), 1);
}
