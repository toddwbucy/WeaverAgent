//! Actual refused placement hands its original Boot proof to bounded rollback.
//! Requires the separate returned-Boot adapter; this overlay changes no policy.
use super::*;
use crate::flow::context::plan_summary_driver::PlanSummaryDisposition;

#[test]
fn place_returned_boot_install_refusal_transfers_exact_cleanup_owner_through_closed() {
    for command_case in 0..3 {
        let mut summary = SummaryFixture::new(vec![]);
        let proof = summary.boot(None);
        let raw = proof.read().raw_bytes().to_vec();
        let read_address = std::ptr::from_ref(proof.read()) as usize;
        let open_address = std::ptr::from_ref(proof.repaired().checked_open().unwrap()) as usize;
        let recall = proof.read().recall_sequence();
        let enter = proof.repaired().checked_open().unwrap().enter();
        let initial = proof.repaired().initial().recall_sequence();
        let recheck = proof.repaired().recheck().recall_sequence();
        let before_placement = summary.records(Kind::StatePlacement);
        let before_recall = summary.records(Kind::Recall);
        let journal = summary
            .run()
            .context
            .boot_journal
            .as_ref()
            .unwrap()
            .boot_snapshot();
        let (mut decode, decoder_peer) = summary._decode.take().unwrap();
        let command = directive(match command_case {
            0 => weaver_types::LifecycleDirective::Stop {
                cause: weaver_types::Cause { uid: 0 },
            },
            1 => weaver_types::LifecycleDirective::Leave {
                cause: weaver_types::Cause { uid: 0 },
                forced: true,
                rollback: true,
            },
            2 => weaver_types::LifecycleDirective::JoinLeave {
                cause: weaver_types::Cause { uid: 0 },
            },
            _ => unreachable!(),
        });
        let (channel, lifecycle_peer) = OrganChannel::pair().unwrap();
        let mut admin = LifecyclePeer::from_channel(channel).unwrap();
        send_bytes(
            lifecycle_peer.as_fd(),
            &serde_json::to_vec(&command).unwrap(),
        );
        let run = summary.run();
        let mut clock = SummaryReadyClock(&mut run.clock);
        run.context
            .pump_once(&mut PumpPorts {
                lifecycle: LifecyclePorts {
                    listener: None,
                    peer: Some(&mut admin),
                    residency: None,
                    dial: None,
                    leave_peer: None,
                },
                gate: None,
                decode: Some(&mut decode),
                label: None,
                io: &mut clock,
            })
            .unwrap();
        assert_eq!(admin.take_completion().unwrap().unwrap().envelope, command);
        assert!(run.context.boot_enter_cancelled);
        let error = run
            .context
            .install_place(proof, &mut run.clock)
            .unwrap_err();
        assert_eq!(error.refusal, PlaceError::WrongProof);
        assert!(error.failure.is_none());
        assert_eq!(
            std::ptr::from_ref(error.prerequisite.read()) as usize,
            read_address
        );
        assert!(run.context.loaded_place_schemas().is_none());
        run.context
            .restore_boot_plan_summary_for_rollback(error.prerequisite)
            .unwrap();
        assert_eq!(
            run.context.take_plan_summary(&mut run.clock).unwrap_err(),
            RunFailure::PlanSummaryOwned
        );
        assert!(run.context.loaded_place_schemas().is_none());
        assert!(receive(decoder_peer.as_fd()).is_none());
        assert!(receive(lifecycle_peer.as_fd()).is_none());
        let leave = AbsoluteBound::after(run.clock.now().unwrap(), 1_000).unwrap();
        run.context
            .begin_boot_plan_summary_release(leave, &mut run.clock)
            .unwrap();
        let account = run.context.take_stopped_plan_summary().unwrap().unwrap();
        assert_eq!(
            account.disposition_for_test(),
            PlanSummaryDisposition {
                complete: true,
                cancelled: false,
                key: PumpDeadline::Enter,
                bound: enter,
                account_deadline: enter.deadline_ns().min(leave.deadline_ns())
            }
        );
        assert_eq!(
            std::ptr::from_ref(account.recorded_read().unwrap()) as usize,
            read_address
        );
        assert_eq!(account.recorded_read().unwrap().raw_bytes(), raw);
        assert_eq!(account.recorded_read().unwrap().recall_sequence(), recall);
        let repaired = account.repaired().unwrap();
        assert_eq!(
            std::ptr::from_ref(repaired.checked_open().unwrap()) as usize,
            open_address
        );
        assert_eq!(repaired.initial().recall_sequence(), initial);
        assert_eq!(repaired.recheck().recall_sequence(), recheck);
        assert_eq!(
            run.context.boot_journal.as_ref().unwrap().boot_snapshot(),
            journal
        );
        assert!(run.context.retire_boot_decode(decode).is_ok());
        run.context.retire_classifier(&mut run.clock).unwrap();
        run.context
            .author_and_wait(&mut ports(&mut run.clock), unload())
            .map_err(|failure| failure.retained_reason())
            .unwrap();
        assert_eq!(summary.records(Kind::StatePlacement), before_placement);
        assert_eq!(summary.records(Kind::Recall), before_recall);
        assert!(summary.records(Kind::SavePoint).is_empty());
        assert!(summary.records(Kind::TurnStarted).is_empty());
        assert!(summary.records(Kind::MessageUser).is_empty());
        summary.drain();
        let run = summary.run();
        run.context
            .close_after_drain(&mut ports(&mut run.clock))
            .unwrap();
        assert_eq!(run.context.phase(), RecordingPhase::Closed);
        assert!(run.context.failure().is_none());
        assert_eq!(account.recorded_read().unwrap().raw_bytes(), raw);
    }
}

#[test]
fn place_returned_boot_success_is_owned_by_engine_and_foreign_return_cannot_replace_it() {
    let mut summary = SummaryFixture::new(vec![]);
    let proof = summary.boot(None);
    let read_address = std::ptr::from_ref(proof.read()) as usize;
    let run = summary.run();
    run.context.install_place(proof, &mut run.clock).unwrap();
    assert_eq!(
        std::ptr::from_ref(run.context.place_engine.as_ref().unwrap().boot.read()) as usize,
        read_address
    );
    assert!(run.context.loaded_place_schemas().is_some());
    let mut other = SummaryFixture::new(vec![]);
    let foreign = other.boot(None);
    let foreign_address = std::ptr::from_ref(foreign.read()) as usize;
    let error = summary
        .run()
        .context
        .restore_boot_plan_summary_for_rollback(foreign)
        .unwrap_err();
    assert_eq!(error.refusal, PlanSummaryBeginRefusal::WrongProof);
    assert!(error.failure.is_none());
    assert_eq!(
        std::ptr::from_ref(error.prerequisite.read()) as usize,
        foreign_address
    );
    let run = summary.run();
    assert_eq!(
        std::ptr::from_ref(run.context.place_engine.as_ref().unwrap().boot.read()) as usize,
        read_address
    );
    assert!(run.context.plan_summary.is_none());
    assert!(run.context.committed_plan_summary().is_ok());
    // There is no owned Boot proof available after successful installation:
    // the engine retains it, and its normal accessor supplies only a borrow.
    // A separate compile-rejection probe pins that this borrow cannot restore.
}
