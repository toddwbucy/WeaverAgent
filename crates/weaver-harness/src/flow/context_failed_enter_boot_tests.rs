//! Complete actual Boot owners joined to the same real fork-minted SPU resource
//! carrier used by the failed lifecycle driver. Language/state peers are doubles.
use super::*;
use crate::flow::context::failed_enter_driver::tests as cleanup_fixture;
use crate::flow::context::{FailedEnterCleanup, FailedEnterProgress, FailedEnterResources};
use weaver_trace::ReleaseOutcome;

fn provision() -> (OpenFixture, FailedEnterResources) {
    let mut open = OpenFixture::new();
    let run = &mut open.boot.member.run;
    let mut resources = run.context.failed_enter_resources().unwrap();
    // SAFETY: this test thread retains the actual child until its nonblocking
    // reap completes. The child test peer is never a production adapter.
    unsafe {
        resources.spawn_spu(
            &mut run.context,
            &std::env::current_exe().unwrap(),
            &cleanup_fixture::child_arguments("failed_enter_process_open_then_release_peer"),
            &mut run.clock,
        )
    }
    .unwrap();
    // Borrow the actual producer endpoint through the existing Open fixture;
    // swap it back before transferring cleanup custody. No synthetic PID/port
    // association is installed: the two original ports remain move-owned.
    std::mem::swap(&mut open.decode, resources.decode_mut().unwrap());
    (open, resources)
}

fn restore_resources(open: OpenFixture, resources: &mut FailedEnterResources) -> BootFixture {
    let OpenFixture {
        boot,
        mut decode,
        peer,
    } = open;
    std::mem::swap(&mut decode, resources.decode_mut().unwrap());
    drop((decode, peer)); // Only the fixture's never-used original socketpair.
    boot
}

#[test]
fn failed_enter_cleanup_sent_open_records_owed_account_before_actual_child_release() {
    let (mut open, mut resources) = provision();
    open.begin();
    for _ in 0..128 {
        open.advance().unwrap();
        if !open.decode.open_not_started() {
            break;
        }
    }
    assert!(!open.decode.open_not_started());
    assert!(!open.boot.member.run.context.boot_open_account_settled());
    let first = cleanup_fixture::state_eof(&mut open.boot.member.run);
    let mut boot = restore_resources(open, &mut resources);
    let mut owner = cleanup_fixture::start(&mut boot.member.run, resources);
    cleanup_fixture::drive(&mut owner, &mut boot.member.run.clock);
    assert_eq!(owner.failure(), &first);
    assert!(owner.context().boot_open_account_settled());
    assert!(!owner.context().boot_open_grant_transferred());
    assert!(owner.context().boot_journal.is_none());
    assert_eq!(owner.release_outcome(), ReleaseOutcome::Confirmed);
    assert!(owner.unload_sequence().is_some());
    assert_eq!(
        owner
            .context()
            .barrier
            .recorder()
            .structure()
            .by_kind(Kind::StatePlacement)
            .count(),
        1
    );
    assert_eq!(
        owner
            .context()
            .barrier
            .recorder()
            .structure()
            .by_kind(Kind::Unload)
            .count(),
        1
    );
}

#[test]
fn failed_enter_cleanup_full_trace_keeps_actual_sent_open_and_decoder_until_owed_record() {
    use std::io::{ErrorKind, Read};
    use std::time::{Duration, Instant};
    let (mut open, mut resources) = provision();
    open.begin();
    for _ in 0..128 {
        open.advance().unwrap();
        if !open.decode.open_not_started() {
            break;
        }
    }
    assert!(!open.decode.open_not_started());
    open.boot.member.fill_trace();
    cleanup_fixture::state_eof(&mut open.boot.member.run);
    let mut boot = restore_resources(open, &mut resources);
    let mut owner = cleanup_fixture::start(&mut boot.member.run, resources);
    for _ in 0..3000 {
        owner.advance(&mut boot.member.run.clock);
        if owner.context().pending_author().is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    let ticket = owner
        .context()
        .pending_author()
        .expect("actual owed Open placement blocked by full trace")
        .ticket;
    assert!(!owner.context().boot_open_account_settled());
    assert!(owner.waiting_boot_decode_for_test());
    for _ in 0..4 {
        owner.advance(&mut boot.member.run.clock);
    }
    assert!(owner.waiting_boot_decode_for_test());
    assert_eq!(owner.context().pending_author().unwrap().ticket, ticket);
    boot.member.run._reader.set_nonblocking(true).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut bytes = [0; 65536];
    while owner.context().barrier.recorder().boundary().queued != 0 {
        assert!(Instant::now() < deadline);
        match boot.member.run._reader.read(&mut bytes) {
            Ok(0) => panic!("trace closed before admitted account drained"),
            Ok(_) => {}
            Err(error) if error.kind() == ErrorKind::WouldBlock => std::thread::yield_now(),
            Err(error) => panic!("trace drain: {error}"),
        }
    }
    cleanup_fixture::drive(&mut owner, &mut boot.member.run.clock);
    assert!(owner.context().boot_open_account_settled());
    assert!(owner.context().pending_author().is_none());
    assert_eq!(owner.release_outcome(), ReleaseOutcome::Confirmed);
    assert_eq!(
        owner
            .context()
            .barrier
            .recorder()
            .structure()
            .by_kind(Kind::StatePlacement)
            .count(),
        1
    );
}

fn completed_pair_resources() -> (SummaryFixture, FailedEnterResources) {
    let (mut open, mut resources) = provision();
    open.begin();
    let mut checked = None;
    for _ in 0..3000 {
        open.advance().unwrap();
        let run = &mut open.boot.member.run;
        if let Some(proof) = run.context.take_boot_open(&mut run.clock).unwrap() {
            checked = Some(proof);
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    let checked = checked.expect("actual child must complete checked Open");
    let mut repair =
        RepairFixture::from_checked(restore_resources(open, &mut resources), checked, vec![]);
    repair.begin();
    let BootPairsCompletion::Repaired(repaired) = repair.complete() else {
        panic!("repaired");
    };
    let fx = SummaryFixture {
        boot: repair.into_boot(),
        repaired: Some(repaired),
        _decode: None,
        files: vec![],
    };
    (fx, resources)
}

#[test]
fn failed_enter_cleanup_extracted_summary_must_restore_then_preserves_actual_read_open_and_bounds()
{
    let (mut fx, resources) = completed_pair_resources();
    let proof = fx.boot(None);
    let read_ptr = proof.read() as *const RecordedRead;
    let open_ptr = proof.repaired().checked_open().unwrap() as *const CheckedBootOpen;
    let raw = proof.read().raw_bytes().to_vec();
    let original = proof.repaired().checked_open().unwrap().enter();
    let recalls_before = fx.records(Kind::Recall).len();
    let first = cleanup_fixture::state_eof(fx.run());
    let context = cleanup_fixture::take_context(fx.run());
    let rejected = match context.into_failed_enter_cleanup(resources) {
        Err(error) => error,
        Ok(_) => panic!("external Boot evidence was not restored"),
    };
    assert_eq!(rejected.refusal, FailedEnterRefusal::BootProofExtracted);
    let mut context = rejected.context;
    context
        .restore_boot_plan_summary_for_rollback(proof)
        .unwrap();
    let mut owner: FailedEnterCleanup = match context.into_failed_enter_cleanup(rejected.resources)
    {
        Ok(owner) => owner,
        Err(error) => panic!("restored owner: {:?}", error.refusal),
    };
    cleanup_fixture::drive(&mut owner, &mut fx.run().clock);
    assert_eq!(owner.failure(), &first);
    let account = owner.summary_account().unwrap();
    assert_eq!(
        account.recorded_read().unwrap() as *const RecordedRead,
        read_ptr
    );
    assert_eq!(account.recorded_read().unwrap().raw_bytes(), raw);
    assert_eq!(
        account.repaired().unwrap().checked_open().unwrap() as *const CheckedBootOpen,
        open_ptr
    );
    let disposition = account.disposition_for_test();
    assert!(disposition.complete);
    assert!(!disposition.cancelled);
    assert_eq!(disposition.bound, original);
    assert!(disposition.account_deadline <= original.deadline_ns());
    assert!(owner.context().place_engine.is_none());
    assert_eq!(owner.release_outcome(), ReleaseOutcome::Confirmed);
    assert_eq!(
        owner.advance(&mut fx.run().clock),
        FailedEnterProgress::Closed
    );
    assert_eq!(
        owner
            .context()
            .barrier
            .recorder()
            .structure()
            .by_kind(Kind::Recall)
            .count(),
        recalls_before,
        "cleanup must not reauthor the retained successful reads"
    );
}

#[test]
fn failed_enter_cleanup_real_boot_hold_returns_original_before_cleanup_can_begin() {
    let (mut open, mut resources) = provision();
    open.begin();
    for _ in 0..128 {
        open.advance().unwrap();
        if !open.decode.open_not_started() {
            break;
        }
    }
    let raw =
        serde_json::value::to_raw_value(&serde_json::json!({"unadmitted":"exact-held-original"}))
            .unwrap();
    let pointer = raw.get().as_ptr();
    let original = AuthoredEvent {
        kind: Kind::ModelMeasurement,
        subsystem: Subsystem::Harness,
        turn: Some(weaver_trace::TurnRef("unadmitted".into())),
        source: AuthoredSource::Runtime,
        causal_parent: None,
        payload: Some(Payload::ModelMeasurement(raw)),
    };
    assert_eq!(
        open.boot.member.run.author(original),
        Err(RunFailure::BootOpenOwned)
    );
    cleanup_fixture::state_eof(&mut open.boot.member.run);
    let mut boot = restore_resources(open, &mut resources);
    let context = cleanup_fixture::take_context(&mut boot.member.run);
    let rejected = match context.into_failed_enter_cleanup(resources) {
        Err(error) => error,
        Ok(_) => panic!("held unadmitted original was bypassed"),
    };
    assert_eq!(rejected.refusal, FailedEnterRefusal::LiveDuty);
    let mut context = rejected.context;
    let returned = context.take_held_author().unwrap();
    let Some(Payload::ModelMeasurement(raw)) = &returned.payload else {
        panic!("original");
    };
    assert_eq!(raw.get().as_ptr(), pointer);
    let mut owner = match context.into_failed_enter_cleanup(rejected.resources) {
        Ok(owner) => owner,
        Err(error) => panic!("taken held original: {:?}", error.refusal),
    };
    cleanup_fixture::drive(&mut owner, &mut boot.member.run.clock);
    assert!(owner.context().held_author().is_none());
    assert_eq!(owner.release_outcome(), ReleaseOutcome::Confirmed);
    assert_eq!(
        raw.get().as_ptr(),
        pointer,
        "caller still owns the same rejected input"
    );
}

#[test]
fn failed_enter_cleanup_retires_the_original_summary_rollback_channel_after_a_later_real_failure() {
    let (mut fx, resources) = completed_pair_resources();
    fx.begin_boot();
    fx.release(None);
    for _ in 0..128 {
        fx.advance().unwrap();
        if fx
            .run()
            .context
            .plan_summary
            .as_ref()
            .unwrap()
            .disposition_for_test()
            .complete
        {
            break;
        }
    }
    let run = fx.run();
    assert!(
        run.context
            .plan_summary
            .as_ref()
            .unwrap()
            .disposition_for_test()
            .complete
    );
    let leave = AbsoluteBound::after(run.clock.now().unwrap(), 100_000).unwrap();
    run.context
        .begin_boot_plan_summary_release(leave, &mut run.clock)
        .unwrap();
    assert!(run.context.flow.standing_state().is_none());
    assert!(run.context.flow.has_stopped_plan_read_transport());
    // This is a real classifier EOF after the earlier state rollback, not a
    // forged Failed bit or an assumed empty/healthy retirement.
    fx.boot._classifier.close();
    let run = fx.run();
    let first = run
        .context
        .pump_once(&mut PumpPorts {
            lifecycle: LifecyclePorts::default(),
            gate: None,
            decode: None,
            label: None,
            io: &mut run.clock,
        })
        .unwrap_err();
    assert_eq!(run.context.phase, RecordingPhase::Failed);
    let mut owner = cleanup_fixture::start(fx.run(), resources);
    cleanup_fixture::drive(&mut owner, &mut fx.run().clock);
    assert_eq!(owner.failure(), &first);
    assert!(!owner.context().flow.has_stopped_plan_read_transport());
    assert!(owner.retired_summary_transport_for_test());
    assert!(
        owner
            .summary_account()
            .unwrap()
            .disposition_for_test()
            .complete
    );
    let unload = owner
        .context()
        .barrier
        .recorder()
        .structure()
        .by_kind(Kind::Unload)
        .next()
        .unwrap();
    let unload: serde_json::Value = serde_json::from_str(&unload.line).unwrap();
    assert_eq!(unload["payload"]["grant_surface"], "unreadable");
    assert!(owner.unload_sequence().is_some());
}

#[test]
fn failed_enter_review_actual_summary_account_precedes_unload_and_keeps_member_closed_fact() {
    let (mut fx, resources) = completed_pair_resources();
    fx.begin_boot();
    released_summary_recall(&mut fx, None);
    // The real Released measurement is Recorded before pressure is added.
    // Released provenance is real but its recall author cannot enter the full
    // writer yet. The resulting summary account remains an actual obligation.
    fx.boot.member.fill_trace();
    for _ in 0..128 {
        fx.advance().unwrap();
        if fx.run().context.pending_author().is_some() {
            break;
        }
    }
    let ticket = fx
        .run()
        .context
        .pending_author()
        .expect("real summary recall is blocked")
        .ticket;
    assert_eq!(
        fx.run().context.pending_author().unwrap().event.kind,
        Kind::Recall
    );
    assert!(fx.run().context.plan_summary.is_some());
    let recalls_before = fx.records(Kind::Recall).len();
    // A real classifier EOF preserves the independent healthy member outcome.
    fx.boot._classifier.close();
    let run = fx.run();
    let first = run
        .context
        .pump_once(&mut PumpPorts {
            lifecycle: LifecyclePorts::default(),
            gate: None,
            decode: None,
            label: None,
            io: &mut run.clock,
        })
        .unwrap_err();
    assert_eq!(run.context.phase, RecordingPhase::Failed);
    let mut owner = cleanup_fixture::start(fx.run(), resources);
    for _ in 0..3000 {
        owner.advance(&mut fx.run().clock);
        if owner.release_outcome() == ReleaseOutcome::Confirmed {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert_eq!(owner.release_outcome(), ReleaseOutcome::Confirmed);
    for _ in 0..8 {
        owner.advance(&mut fx.run().clock);
    }
    assert_eq!(owner.context().pending_author().unwrap().ticket, ticket);
    assert!(owner.context().plan_summary.is_some());
    assert!(owner.summary_account().is_none());
    assert!(
        owner.unload_sequence().is_none(),
        "terminal Unload waits for the real summary account"
    );
    assert_eq!(
        owner
            .context()
            .barrier
            .recorder()
            .structure()
            .by_kind(Kind::Unload)
            .count(),
        0
    );
    // Drain actual sink pressure without discarding the context's original
    // ticket, then allow the same stopped account and terminal record to finish.
    use std::io::{ErrorKind, Read};
    fx.run()._reader.set_nonblocking(true).unwrap();
    let end = std::time::Instant::now() + std::time::Duration::from_secs(3);
    let mut bytes = [0u8; 65536];
    while owner.context().barrier.recorder().boundary().queued != 0 {
        assert!(std::time::Instant::now() < end);
        match fx.run()._reader.read(&mut bytes) {
            Ok(0) => panic!("trace closed before owed recall"),
            Ok(_) => {}
            Err(error) if error.kind() == ErrorKind::WouldBlock => std::thread::yield_now(),
            Err(error) => panic!("trace: {error}"),
        }
    }
    cleanup_fixture::drive(&mut owner, &mut fx.run().clock);
    assert_eq!(owner.failure(), &first);
    assert!(owner.summary_account().unwrap().recorded_read().is_some());
    let recall_rows: Vec<_> = owner
        .context()
        .barrier
        .recorder()
        .structure()
        .by_kind(Kind::Recall)
        .collect();
    assert_eq!(recall_rows.len(), recalls_before + 1);
    let recall: serde_json::Value =
        serde_json::from_str(&recall_rows.last().unwrap().line).unwrap();
    let unload_rows: Vec<_> = owner
        .context()
        .barrier
        .recorder()
        .structure()
        .by_kind(Kind::Unload)
        .collect();
    assert_eq!(unload_rows.len(), 1);
    let unload: serde_json::Value = serde_json::from_str(&unload_rows[0].line).unwrap();
    assert!(
        recall["sequence"].as_str().unwrap().parse::<u64>().unwrap()
            < unload["sequence"].as_str().unwrap().parse::<u64>().unwrap()
    );
    assert_eq!(unload["payload"]["release"]["spu"], "confirmed");
    assert_eq!(unload["payload"]["release"]["member"], "closed");
    let done = match owner.finish() {
        Ok(done) => done,
        Err(_) => panic!("closed"),
    };
    assert!(!done.abandoned());
    assert!(done.trace_drained());
    assert!(done.summary_account().unwrap().recorded_read().is_some());
    assert!(done.unload_sequence().is_some());
}
