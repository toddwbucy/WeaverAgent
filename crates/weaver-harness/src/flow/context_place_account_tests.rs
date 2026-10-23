//! Failed and releasing accounts retain exact wire and recorder custody.
use super::*;

impl PlaceFixture {
    pub(super) fn account_step(&mut self) -> Result<PlaceProgress, RunFailure> {
        let run = &mut self.summary.boot.member.run;
        run.context.advance_place_account(&mut PumpPorts {
            lifecycle: LifecyclePorts::default(),
            gate: None,
            decode: Some(&mut self.decode),
            label: None,
            io: &mut run.clock,
        })
    }
    pub(super) fn first_fault_attempt(&mut self) {
        let run = self.run();
        assert!(run.context.failure().is_some());
        run.context
            .freeze_halt_account(ExternalFacts {
                outstanding_pairs: vec![],
                capture: None,
                wait: run.context.halt_wait().cloned(),
                input_id: None,
            })
            .unwrap();
        run.context.begin_halt_report().unwrap();
        run.context.advance_halt_trace(&mut run.clock).unwrap();
    }
    pub(super) fn settled_account(&mut self) -> PlaceAccount {
        for _ in 0..128 {
            self.account_step().unwrap();
            if let Some(account) = self.run().context.take_place_account() {
                return account;
            }
        }
        panic!("bounded place account did not settle");
    }
}

#[test]
fn place_releasing_measurement_drains_actual_response_without_placement_or_generation() {
    let mut fx = PlaceFixture::new();
    let units = fx.text(8, "record");
    fx.turn("release-measure");
    let before = fx.journal();
    let request = fx.request(units);
    fx.begin(request);
    let TokenDirective::MeasureDelta { keep, .. } = fx.next_measure() else {
        unreachable!()
    };
    let original = fx
        .run()
        .context
        .deadlines
        .get(PumpDeadline::Decision)
        .unwrap();
    let run = fx.run();
    let leave = AbsoluteBound::after(run.clock.now().unwrap(), PLACE_WAIT / 2).unwrap();
    run.context
        .begin_place_release(leave, &mut run.clock)
        .unwrap();
    assert_eq!(run.context.phase(), RecordingPhase::Releasing);
    assert!(run.context.failure().is_none());
    assert_ne!(fx.account_step().unwrap(), PlaceProgress::Complete);
    assert!(fx.run().context.take_place_account().is_none());
    let raw = fx.answer(&fx.measured(keep, 3));
    let account = fx.settled_account();
    assert_eq!(account.raw_bytes(), Some(raw.as_slice()));
    assert!(account.incomplete().is_none());
    assert_eq!(fx.journal().1, before.1);
    assert_eq!(fx.journal().6, 0);
    assert_eq!(fx.records(Kind::StatePlacement).len(), 1);
    assert_eq!(
        fx.run().context.deadlines.get(PumpDeadline::Decision),
        Some(original)
    );
    assert!(receive(fx.peer.as_fd()).is_none());
    assert!(fx.run().context.completed_author.is_none());
}

#[test]
fn place_releasing_pending_record_keeps_exact_ticket_and_completes_only_trace_duty() {
    let mut fx = PlaceFixture::new();
    let units = fx.text(8, "record");
    fx.turn("release-placement");
    let request = fx.request(units);
    fx.begin(request);
    let TokenDirective::MeasureDelta { keep, .. } = fx.next_measure() else {
        unreachable!()
    };
    let raw = fx.answer(&fx.measured(keep, 3));
    fx.summary.boot.member.fill_trace();
    let mut ticket = None;
    for _ in 0..128 {
        fx.advance_placement_pressure().unwrap();
        if let Some(author) = fx.run().context.pending_author() {
            ticket = Some(author.ticket);
            break;
        }
    }
    let ticket = ticket.expect("actual pending placement");
    let run = fx.run();
    let leave = AbsoluteBound::after(run.clock.now().unwrap(), PLACE_WAIT / 2).unwrap();
    run.context
        .begin_place_release(leave, &mut run.clock)
        .unwrap();
    for _ in 0..4 {
        assert_eq!(fx.account_step().unwrap(), PlaceProgress::Pending);
        assert_eq!(fx.run().context.pending_author().unwrap().ticket, ticket);
        assert!(fx.run().context.take_place_account().is_none());
    }
    fx.summary.drain();
    let account = fx.settled_account();
    assert!(account.incomplete().is_none());
    assert_eq!(account.raw_bytes(), Some(raw.as_slice()));
    assert_eq!(fx.records(Kind::StatePlacement).len(), 2);
    assert_eq!(
        fx.journal().6,
        0,
        "Recorded append proposal never installs prospective IDs"
    );
    assert!(receive(fx.peer.as_fd()).is_none());
    assert!(fx.run().context.completed_author.is_none());
    assert!(fx.run().context.failure().is_none());
}

#[test]
fn place_failed_terminal_preserves_first_failure_exact_raw_and_original_account_bound() {
    let mut fx = PlaceFixture::new();
    let units = fx.text(8, "record");
    fx.turn("failed-account");
    let before = fx.journal();
    let grant = fx.grant(units, 3);
    fx.start_generation(grant);
    fx.sent_generation();
    let run = fx.run();
    let original = AbsoluteBound::after(run.clock.now().unwrap(), PLACE_WAIT / 2).unwrap();
    run.context
        .arm(PumpDeadline::Leave, original, &mut run.clock)
        .unwrap();
    let raw = fx.answer(&TokenAnswer::AtRest);
    let first = fx.generation_failure();
    assert_eq!(
        fx.account_step().unwrap(),
        PlaceProgress::Pending,
        "owed account waits for first terminal fault attempt"
    );
    assert!(fx.run().context.take_place_account().is_none());
    fx.first_fault_attempt();
    let run = fx.run();
    run.clock.advance_to(original.deadline_ns()).unwrap();
    let account = fx.settled_account();
    assert!(account.incomplete().is_some());
    assert_eq!(account.raw_bytes(), Some(raw.as_slice()));
    assert_eq!(fx.run().context.failure(), Some(&first));
    assert_eq!(fx.journal().1, before.1);
    assert_eq!(fx.journal().6, 0);
    assert_eq!(fx.records(Kind::Fault).len(), 1);
    assert!(receive(fx.peer.as_fd()).is_none());
}

#[test]
fn place_releasing_generation_before_first_byte_withdraws_without_append_or_false_residency() {
    let mut fx = PlaceFixture::new();
    let units = fx.text(8, "record");
    fx.turn("release-unsent");
    let before = fx.journal();
    let grant = fx.grant(units, 3);
    fx.start_generation(grant);
    let run = fx.run();
    let leave = AbsoluteBound::after(run.clock.now().unwrap(), PLACE_WAIT / 2).unwrap();
    run.context
        .begin_place_release(leave, &mut run.clock)
        .unwrap();
    let account = fx.settled_account();
    assert!(account.incomplete().is_none());
    assert!(account.raw_bytes().is_none());
    assert!(receive(fx.peer.as_fd()).is_none());
    assert_eq!(fx.journal().1, before.1);
    assert_eq!(fx.journal().6, 0);
    assert!(fx.run().context.failure().is_none());
}

#[test]
fn place_full_releasing_account_retires_decode_state_classifier_then_unloads_and_drains() {
    let mut fx = PlaceFixture::new();
    let units = fx.text(8, "record");
    fx.turn("full-release");
    let request = fx.request(units);
    fx.begin(request);
    let TokenDirective::MeasureDelta { keep, .. } = fx.next_measure() else {
        unreachable!()
    };
    let run = fx.run();
    let leave = AbsoluteBound::after(run.clock.now().unwrap(), PLACE_WAIT / 2).unwrap();
    run.context
        .begin_place_release(leave, &mut run.clock)
        .unwrap();
    assert_eq!(
        run.context.retire_state(&mut run.clock),
        Err(RunFailure::PlaceOwned)
    );
    assert_eq!(
        run.context.retire_classifier(&mut run.clock),
        Err(RunFailure::PlaceOwned)
    );
    let raw = fx.answer(&fx.measured(keep, 3));
    let account = fx.settled_account();
    assert_eq!(account.raw_bytes(), Some(raw.as_slice()));
    assert!(account.incomplete().is_none());
    assert!(receive(fx.peer.as_fd()).is_none());
    let (foreign, foreign_peer) = DecodeChannel::pair().unwrap();
    let foreign = DecodePort::from_channel(foreign).unwrap();
    let foreign_fd = foreign.fd().as_raw_fd();
    let foreign = fx
        .run()
        .context
        .retire_place_decode(foreign)
        .expect_err("settled owner must still reject a different actual decoder");
    assert_eq!(
        foreign.fd().as_raw_fd(),
        foreign_fd,
        "refusal returns the exact unrelated descriptor"
    );
    assert!(receive(foreign_peer.as_fd()).is_none());
    let PlaceFixture {
        mut summary,
        decode,
        ..
    } = fx;
    let run = summary.run();
    assert!(run.context.retire_place_decode(decode).is_ok());
    assert_eq!(
        run.context.retire_state(&mut run.clock).unwrap(),
        Advance::Progress
    );
    assert_eq!(
        run.context.retire_classifier(&mut run.clock).unwrap(),
        Advance::Progress
    );
    assert!(run.context.flow.standing_state().is_none());
    assert!(run.context.flow.standing_classifier().is_none());
    run.context
        .author_and_wait(&mut ports(&mut run.clock), unload())
        .unwrap();
    assert!(run.context.barrier.is_closed());
    assert!(run.context.failure().is_none());
    summary.drain();
    let run = summary.run();
    run.context
        .close_after_drain(&mut ports(&mut run.clock))
        .unwrap();
    assert_eq!(run.context.phase(), RecordingPhase::Closed);
    assert_eq!(
        run.context
            .author_and_wait(&mut ports(&mut run.clock), unload()),
        Err(RunFailure::Closed)
    );
    assert_eq!(
        account.raw_bytes(),
        Some(raw.as_slice()),
        "retained exchange evidence outlives complete teardown"
    );
}

#[test]
fn place_join_owed_account_blocks_release_scope_without_consuming_shutdown_original() {
    let mut fx = PlaceFixture::new();
    let units = fx.text(8, "record");
    fx.turn("release-owed-join");
    let request = fx.request(units);
    fx.begin(request);
    let TokenDirective::MeasureDelta { keep, .. } = fx.next_measure() else {
        unreachable!()
    };
    let run = fx.run();
    let leave = AbsoluteBound::after(run.clock.now().unwrap(), PLACE_WAIT / 2).unwrap();
    run.context
        .begin_place_release(leave, &mut run.clock)
        .unwrap();
    assert_eq!(run.context.phase(), RecordingPhase::Releasing);
    assert!(run.context.place_owed());
    assert!(run.context.pending_author().is_none());
    let event = crate::flow::context::tests::event(
        Kind::Refusal,
        Some(Payload::Refusal(
            serde_json::value::to_raw_value(&weaver_types::LifecycleRefusal::OutOfOrder).unwrap(),
        )),
    );
    let Payload::Refusal(original) = event.payload.as_ref().unwrap() else {
        unreachable!()
    };
    let pointer = original.get().as_ptr();
    let original_text = original.get().to_owned();
    let prior_sequence = run.context.last_author_sequence;
    // Exercise the real context-issued private release scope directly, so the
    // new barrier debt guard is not masked by the outer PlaceOwned admission or
    // the older Unload-only guard. This Refusal has valid shutdown shape.
    let RecordProgress::BusyAuthored { event, purpose } =
        run.context
            .submit_context_author(event, RecordPurpose::Observation, 1, &mut run.clock)
    else {
        panic!("owed placement evidence must retain the shutdown original");
    };
    assert!(matches!(purpose, RecordPurpose::Observation));
    let Payload::Refusal(returned) = event.payload.as_ref().unwrap() else {
        unreachable!()
    };
    assert_eq!(returned.get().as_ptr(), pointer);
    assert_eq!(returned.get(), original_text);
    assert_eq!(run.context.last_author_sequence, prior_sequence);
    assert!(run.context.pending_author().is_none());
    assert!(run.context.barrier.failure().is_none());
    assert!(run.context.failure().is_none());
    // The private call returned its event before any ticket existed. Retire that
    // unadmitted attempt; the actual placement account remains untouched.
    run.context.finish_release_author();
    assert!(fx.records(Kind::Refusal).is_empty());
    let raw = fx.answer(&fx.measured(keep, 3));
    let account = fx.settled_account();
    assert_eq!(account.raw_bytes(), Some(raw.as_slice()));
    assert!(account.incomplete().is_none());
    assert!(!fx.run().context.place_owed());
    let run = fx.run();
    run.context
        .author_and_wait(&mut ports(&mut run.clock), event)
        .unwrap();
    assert_eq!(fx.records(Kind::Refusal).len(), 1);
    let PlaceFixture {
        mut summary,
        decode,
        ..
    } = fx;
    let run = summary.run();
    assert!(run.context.retire_place_decode(decode).is_ok());
    assert_eq!(
        run.context.retire_state(&mut run.clock).unwrap(),
        Advance::Progress
    );
    assert_eq!(
        run.context.retire_classifier(&mut run.clock).unwrap(),
        Advance::Progress
    );
    run.context
        .author_and_wait(&mut ports(&mut run.clock), unload())
        .unwrap();
    summary.drain();
    let run = summary.run();
    run.context
        .close_after_drain(&mut ports(&mut run.clock))
        .unwrap();
    assert_eq!(run.context.phase(), RecordingPhase::Closed);
    assert_eq!(account.raw_bytes(), Some(raw.as_slice()));
}
