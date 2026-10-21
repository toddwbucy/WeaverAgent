use super::*;
use weaver_trace::serving::payloads::{CopyContentType, CopySourceEvent};

fn observation() -> Observation {
    Observation::new(
        "run".into(),
        100,
        MeasuredInterval {
            elapsed_ns: 10.into(),
            measurement_ref: "test-only cadence".into(),
        },
        2,
    )
    .unwrap()
}

fn id(sequence: u64, block: u64) -> SourceBlockId {
    SourceBlockId {
        run: "run".into(),
        sequence: sequence.into(),
        block: block.into(),
    }
}

fn source<'a>(
    id: &'a SourceBlockId,
    turn: Option<&'a str>,
    raw: u64,
    context: u64,
    intake_ns: u64,
) -> PendingSource<'a> {
    PendingSource {
        source: id,
        turn,
        raw_bytes: raw,
        context_bytes: context,
        intake_ns,
    }
}

fn gauges(input_pending: u64) -> GaugeSnapshot {
    GaugeSnapshot::from_sources("run", [], input_pending, (0, 0), None).unwrap()
}

fn waiting(sequence: u64) -> CopyAtHalt {
    CopyAtHalt {
        source_event: CopySourceEvent {
            run: "run".into(),
            sequence: sequence.into(),
        },
        partition_rule_id: "text".into(),
        content_type: CopyContentType::Statement,
        sources: vec![TraceSource {
            run: "run".into(),
            sequence: sequence.into(),
            block: 0.into(),
        }],
    }
}

fn prepare(
    owner: &mut Observation,
    phase: FlowPhase,
    gauges: GaugeSnapshot,
    at: u64,
) -> FlowObservation {
    assert_eq!(
        owner.prepare(Transition::new(phase), gauges, at),
        Ok(Prepared::Prepared)
    );
    owner.pending().unwrap().clone()
}

fn confirm(owner: &mut Observation) -> Option<RecordedTerminal> {
    owner
        .recorded(RecordedFlow::for_test("run", owner.pending().unwrap()))
        .unwrap()
}

#[test]
fn traffic_reservations_age_and_waiting_turns_keep_their_distinct_meanings() {
    let a = id(1, 0);
    let b = id(1, 2);
    let c = id(2, 0);
    let wait = waiting(3);
    let snapshot = GaugeSnapshot::from_sources(
        "run",
        [
            source(&a, Some("turn-a"), 10, 7, 101),
            source(&b, Some("turn-a"), 20, 7, 103),
            source(&c, None, 30, 0, 105),
        ],
        1,
        (3, 900),
        Some(WaitingSource {
            copy: &wait,
            turn: Some("turn-b"),
        }),
    )
    .unwrap();
    assert!(snapshot.work_pending());
    let mut owner = observation();
    owner.note_input_started().unwrap();
    let sample = prepare(&mut owner, FlowPhase::BacklogWait, snapshot, 120);
    assert_eq!(sample.pending_count.0, 3);
    assert_eq!(
        sample.pending_bytes.0, 74,
        "shared context counts once per obligation for traffic"
    );
    assert_eq!(sample.retained_blocks.0, 3);
    assert_eq!(
        sample.retained_bytes.0, 900,
        "reservation includes its own shared envelope accounting"
    );
    assert_eq!(
        sample.oldest_age_ns.0, 19,
        "age starts at intake, before classification or send"
    );
    assert_eq!(
        sample.pending_turns.0, 2,
        "waiting turn is present, turnless work adds no turn"
    );
    assert_eq!(sample.waiting_copy, Some(wait));
    assert!(sample.backlog_waiting && sample.lag_alarm);
    assert_eq!(sample.input_pending.0, 1);
    assert_eq!(sample.input_started.0, 1);
    assert_eq!((sample.sample_start_ns.0, sample.sampled_ns.0), (100, 120));
}

#[test]
fn waiting_copy_has_identity_and_turn_without_ledger_admission_or_charge() {
    let wait = waiting(1);
    let snapshot = GaugeSnapshot::from_sources(
        "run",
        [],
        0,
        (0, 0),
        Some(WaitingSource {
            copy: &wait,
            turn: Some("one"),
        }),
    )
    .unwrap();
    assert!(snapshot.work_pending());
    let mut owner = observation();
    let sample = prepare(&mut owner, FlowPhase::BacklogWait, snapshot, 110);
    assert_eq!(sample.pending_count.0, 0);
    assert_eq!(sample.pending_bytes.0, 0);
    assert_eq!(sample.retained_blocks.0, 0);
    assert_eq!(sample.retained_bytes.0, 0);
    assert_eq!(sample.oldest_age_ns.0, 0);
    assert_eq!(sample.pending_turns.0, 1);
    assert!(sample.backlog_waiting && sample.lag_alarm);
    confirm(&mut owner);
    let resumed = prepare(&mut owner, FlowPhase::BacklogResumed, gauges(0), 120);
    assert!(!resumed.backlog_waiting && !resumed.lag_alarm);
    assert!(resumed.waiting_copy.is_none());
}

#[test]
fn turn_alarm_deduplicates_ids_and_clears_without_becoming_a_deadline() {
    let a = id(1, 0);
    let b = id(1, 1);
    let c = id(2, 0);
    let mut owner = observation();
    for (turn_c, want_alarm, want_turns, at) in [("other", true, 2, 110), ("one", false, 1, 120)] {
        let snapshot = GaugeSnapshot::from_sources(
            "run",
            [
                source(&a, Some("one"), 1, 0, 101),
                source(&b, Some("one"), 1, 0, 101),
                source(&c, Some(turn_c), 1, 0, 101),
            ],
            0,
            (3, 100),
            None,
        )
        .unwrap();
        let sample = prepare(&mut owner, FlowPhase::Sampled, snapshot, at);
        assert_eq!(sample.pending_turns.0, want_turns);
        assert_eq!(sample.lag_alarm, want_alarm);
        assert_eq!(sample.blocked_stage, None);
        confirm(&mut owner);
    }
    assert_eq!(owner.next_tick(true), Ok(Some(130)));
}

#[test]
fn duplicate_foreign_or_malformed_snapshot_identities_are_refused() {
    let a = id(1, 0);
    assert!(matches!(
        GaugeSnapshot::from_sources(
            "run",
            [source(&a, None, 1, 0, 101), source(&a, None, 1, 0, 101),],
            0,
            (2, 20),
            None
        ),
        Err(ObservationError::DuplicateSource)
    ));
    let mut foreign = a.clone();
    foreign.run = "foreign".into();
    assert!(matches!(
        GaugeSnapshot::from_sources("run", [source(&foreign, None, 1, 0, 101)], 0, (1, 10), None),
        Err(ObservationError::InvalidIdentity)
    ));
    let wait = waiting(1);
    assert!(matches!(
        GaugeSnapshot::from_sources(
            "run",
            [source(&a, None, 1, 0, 101)],
            0,
            (1, 10),
            Some(WaitingSource {
                copy: &wait,
                turn: None
            })
        ),
        Err(ObservationError::DuplicateSource)
    ));
    for defect in ["run", "sequence", "duplicate", "rule"] {
        let mut wait = waiting(2);
        match defect {
            "run" => wait.source_event.run = "foreign".into(),
            "sequence" => wait.sources[0].sequence = 3.into(),
            "duplicate" => wait.sources.push(wait.sources[0].clone()),
            "rule" => wait.partition_rule_id.clear(),
            _ => unreachable!(),
        }
        assert!(
            GaugeSnapshot::from_sources(
                "run",
                [],
                0,
                (0, 0),
                Some(WaitingSource {
                    copy: &wait,
                    turn: None
                })
            )
            .is_err(),
            "{defect}"
        );
    }
    assert!(matches!(
        GaugeSnapshot::from_sources("run", [source(&a, Some(""), 1, 0, 101)], 0, (1, 10), None),
        Err(ObservationError::InvalidIdentity)
    ));
    let foreign = GaugeSnapshot::from_sources("foreign", [], 0, (0, 0), None).unwrap();
    assert_eq!(
        observation().prepare(Transition::new(FlowPhase::Sampled), foreign, 110),
        Err(ObservationError::InvalidIdentity)
    );
}

#[test]
fn pending_sample_freezes_counts_and_only_its_exact_proof_moves_the_baseline() {
    let mut owner = observation();
    owner.note_filed(17).unwrap();
    owner.note_rejected().unwrap();
    owner.note_input_started().unwrap();
    let frozen = prepare(&mut owner, FlowPhase::Sampled, gauges(1), 110);
    owner.note_filed(23).unwrap();
    owner.note_input_completed().unwrap();
    assert_eq!(
        owner.prepare(Transition::new(FlowPhase::Sampled), gauges(0), 140),
        Ok(Prepared::Busy)
    );
    assert_eq!(owner.pending(), Some(&frozen));
    assert_eq!(owner.next_tick(true), Ok(None));
    assert!(!owner.tick_due(140, true).unwrap());
    let mut wrong = frozen.clone();
    wrong.filed_bytes = 999.into();
    for proof in [
        RecordedFlow::for_test("foreign", &frozen),
        RecordedFlow::for_test("run", &wrong),
    ] {
        assert!(matches!(
            owner.recorded(proof),
            Err(ObservationError::ProofMismatch)
        ));
        assert_eq!(owner.pending(), Some(&frozen));
        assert_eq!(owner.baseline_ns, 100);
    }
    assert!(confirm(&mut owner).is_none());
    assert_eq!(owner.next_tick(true), Ok(Some(120)));
    assert!(owner.tick_due(140, true).unwrap());
    let coalesced = prepare(&mut owner, FlowPhase::Sampled, gauges(0), 140);
    assert_eq!(
        (coalesced.sample_start_ns.0, coalesced.sampled_ns.0),
        (110, 140)
    );
    assert_eq!(coalesced.filed_count.0, 1);
    assert_eq!(coalesced.filed_bytes.0, 23);
    assert_eq!(coalesced.rejected_count.0, 0);
    assert_eq!(coalesced.input_started.0, 0);
    assert_eq!(coalesced.input_completed.0, 1);
    confirm(&mut owner);
    assert!(matches!(
        owner.recorded(RecordedFlow::for_test("run", &coalesced)),
        Err(ObservationError::NoPendingSample)
    ));
}

#[test]
fn queued_points_preserve_transition_facts_but_use_each_recorded_interval() {
    let mut owner = observation();
    owner.note_filed(7).unwrap();
    let mut filed = Transition::new(FlowPhase::Filed);
    filed.source_id = Some(id(1, 0));
    let first = owner
        .capture(
            filed,
            GaugeSnapshot::from_sources("run", [], 0, (1, 90), None).unwrap(),
            110,
        )
        .unwrap();
    owner.prepare_captured(&first).unwrap();
    let pending = owner.pending().unwrap().clone();
    let waiting = waiting(2);
    let wait = owner
        .capture(
            Transition::new(FlowPhase::BacklogWait),
            GaugeSnapshot::from_sources(
                "run",
                [],
                0,
                (1, 90),
                Some(WaitingSource {
                    copy: &waiting,
                    turn: Some("turn"),
                }),
            )
            .unwrap(),
            120,
        )
        .unwrap();
    owner.note_filed(13).unwrap();
    let resumed = owner
        .capture(Transition::new(FlowPhase::BacklogResumed), gauges(0), 130)
        .unwrap();
    assert_eq!(owner.pending(), Some(&pending));
    assert_eq!(owner.baseline_ns, 100);
    assert_eq!(owner.next_tick(true), Ok(None));
    assert_eq!(owner.prepare_captured(&wait), Ok(Prepared::Busy));
    confirm(&mut owner);
    assert_eq!(owner.next_tick(true), Ok(Some(120)));
    owner.prepare_captured(&wait).unwrap();
    let sample = owner.pending().unwrap();
    assert_eq!((sample.sample_start_ns.0, sample.sampled_ns.0), (110, 120));
    assert_eq!(sample.filed_count.0, 0);
    assert_eq!(sample.filed_bytes.0, 0);
    assert!(sample.backlog_waiting);
    assert_eq!(sample.waiting_copy, Some(waiting));
    assert_eq!(sample.retained_blocks.0, 1);
    confirm(&mut owner);
    owner.prepare_captured(&resumed).unwrap();
    let sample = owner.pending().unwrap();
    assert_eq!((sample.sample_start_ns.0, sample.sampled_ns.0), (120, 130));
    assert_eq!(sample.filed_count.0, 1);
    assert_eq!(sample.filed_bytes.0, 13);
    assert!(!sample.backlog_waiting);
    assert_eq!(sample.retained_blocks.0, 0);
    confirm(&mut owner);
    assert_eq!(
        owner.prepare_captured(&first),
        Err(ObservationError::ClockReversed)
    );
}

#[test]
fn halt_supersedes_queued_points_without_consuming_their_unrecorded_counts() {
    let mut owner = observation();
    owner.note_filed(7).unwrap();
    let first = owner
        .capture(Transition::new(FlowPhase::Sampled), gauges(0), 110)
        .unwrap();
    owner.note_filed(13).unwrap();
    let second = owner
        .capture(Transition::new(FlowPhase::Sampled), gauges(0), 120)
        .unwrap();
    assert!(owner.pending().is_none());
    assert!(matches!(
        owner.final_sample(gauges(0), 119),
        Err(ObservationError::ClockReversed)
    ));
    let final_sample = owner.final_sample(gauges(0), 130).unwrap();
    assert_eq!(
        (final_sample.sample_start_ns.0, final_sample.sampled_ns.0),
        (100, 130)
    );
    assert_eq!(final_sample.filed_count.0, 2);
    assert_eq!(final_sample.filed_bytes.0, 20);
    assert_eq!(
        owner.prepare_captured(&first),
        Err(ObservationError::Finalized)
    );
    assert_eq!(
        owner.prepare_captured(&second),
        Err(ObservationError::Finalized)
    );
}

#[test]
fn ticks_and_boundaries_preserve_zero_receipt_intervals_without_idle_ticks() {
    let mut owner = observation();
    assert_eq!(owner.next_tick(false), Ok(None));
    assert!(!owner.tick_due(109, true).unwrap());
    assert!(owner.tick_due(110, true).unwrap());
    let a = id(1, 0);
    let snapshot =
        GaugeSnapshot::from_sources("run", [source(&a, None, 12, 3, 101)], 0, (1, 90), None)
            .unwrap();
    for at in [110, 150] {
        let sample = prepare(&mut owner, FlowPhase::Sampled, snapshot.clone(), at);
        assert_eq!(sample.filed_count.0, 0);
        assert_eq!(sample.filed_bytes.0, 0);
        assert_eq!(sample.pending_count.0, 1);
        assert_eq!(sample.oldest_age_ns.0, at - 101);
        confirm(&mut owner);
    }
    let boundary = prepare(&mut owner, FlowPhase::Sampled, gauges(0), 155);
    assert_eq!(
        (boundary.sample_start_ns.0, boundary.sampled_ns.0),
        (150, 155)
    );
    confirm(&mut owner);
    assert_eq!(owner.next_tick(false), Ok(None));
}

#[test]
fn halt_sample_supersedes_pending_interval_without_discarding_its_counts() {
    let mut owner = observation();
    owner.note_filed(11).unwrap();
    prepare(&mut owner, FlowPhase::Sampled, gauges(0), 110);
    confirm(&mut owner);
    owner.note_rejected().unwrap();
    owner.note_filed(7).unwrap();
    let pending = prepare(&mut owner, FlowPhase::Sampled, gauges(0), 120);
    owner.note_filed(13).unwrap();
    let final_sample = owner.final_sample(gauges(0), 130).unwrap();
    assert_eq!(final_sample.phase, FlowPhase::Sampled);
    assert_eq!(
        (final_sample.sample_start_ns.0, final_sample.sampled_ns.0),
        (110, 130)
    );
    assert_eq!(final_sample.filed_count.0, 2);
    assert_eq!(final_sample.filed_bytes.0, 20);
    assert_eq!(final_sample.rejected_count.0, 1);
    assert!(owner.pending().is_none());
    assert_eq!(owner.note_rejected(), Err(ObservationError::Finalized));
    assert_eq!(owner.next_tick(true), Err(ObservationError::Finalized));
    assert!(matches!(
        owner.recorded(RecordedFlow::for_test("run", &pending)),
        Err(ObservationError::Finalized)
    ));
    assert!(matches!(
        owner.final_sample(gauges(0), 140),
        Err(ObservationError::Finalized)
    ));
}

#[test]
fn every_transition_keeps_its_correlations_and_repairs_require_their_fields() {
    for phase in [
        FlowPhase::Copied,
        FlowPhase::ClassificationStarted,
        FlowPhase::Accepted,
        FlowPhase::CorrectlyRejected,
        FlowPhase::FileStarted,
        FlowPhase::Filed,
        FlowPhase::WaitFinished,
        FlowPhase::RepairRequested,
        FlowPhase::RepairExhausted,
        FlowPhase::Sampled,
    ] {
        let mut owner = observation();
        let mut transition = Transition::new(phase);
        transition.source_id = Some(id(9, 2));
        transition.request_id = Some(5);
        transition.input_id = Some(3);
        transition.elapsed_ns = Some(8);
        transition.blocked_stage = Some(BlockedStage::Cut);
        if matches!(
            phase,
            FlowPhase::RepairRequested | FlowPhase::RepairExhausted
        ) {
            transition.failed_corrections = Some(2);
            transition.retry_cap = Some(3);
        }
        assert_eq!(
            owner.prepare(transition.clone(), gauges(0), 110),
            Ok(Prepared::Prepared)
        );
        let sample = owner.pending().unwrap();
        assert_eq!(sample.phase, phase);
        assert_eq!(sample.source_id.as_ref().unwrap().block.0, 2);
        assert_eq!(sample.request_id.unwrap().0, 5);
        assert_eq!(sample.input_id.unwrap().0, 3);
        assert_eq!(sample.elapsed_ns.unwrap().0, 8);
        assert_eq!(sample.blocked_stage, Some(BlockedStage::Cut));
        let terminal = confirm(&mut owner);
        assert_eq!(
            terminal.is_some(),
            matches!(phase, FlowPhase::Filed | FlowPhase::CorrectlyRejected)
        );
        if let Some(terminal) = terminal {
            assert_eq!(terminal.into_source(), id(9, 2));
        }
    }
    for phase in [
        FlowPhase::Filed,
        FlowPhase::CorrectlyRejected,
        FlowPhase::RepairRequested,
        FlowPhase::RepairExhausted,
    ] {
        assert_eq!(
            observation().prepare(Transition::new(phase), gauges(0), 110),
            Err(ObservationError::InvalidTransition)
        );
    }
    let mut transition = Transition::new(FlowPhase::Sampled);
    transition.failed_corrections = Some(1);
    transition.retry_cap = Some(3);
    assert_eq!(
        observation().prepare(transition, gauges(0), 110),
        Err(ObservationError::InvalidTransition)
    );
    assert_eq!(
        observation().prepare(Transition::new(FlowPhase::BacklogWait), gauges(0), 110),
        Err(ObservationError::InvalidTransition)
    );
}

#[test]
fn checked_arithmetic_never_wraps_a_gauge_counter_clock_or_cadence() {
    let a = id(1, 0);
    let b = id(2, 0);
    assert!(matches!(
        GaugeSnapshot::from_sources("run", [source(&a, None, u64::MAX, 1, 101)], 0, (1, 1), None),
        Err(ObservationError::CounterOverflow)
    ));
    assert!(matches!(
        GaugeSnapshot::from_sources(
            "run",
            [
                source(&a, None, u64::MAX, 0, 101),
                source(&b, None, 1, 0, 101)
            ],
            0,
            (2, 1),
            None
        ),
        Err(ObservationError::CounterOverflow)
    ));
    let mut owner = observation();
    assert_eq!(
        owner.note_input_completed(),
        Err(ObservationError::InputUnderflow)
    );
    owner.totals.filed_bytes = u64::MAX;
    assert_eq!(owner.note_filed(1), Err(ObservationError::CounterOverflow));
    assert_eq!(
        owner.totals.filed, 0,
        "failed multi-counter update is atomic"
    );
    owner.totals.filed = u64::MAX;
    assert_eq!(owner.note_filed(0), Err(ObservationError::CounterOverflow));
    owner.totals.rejected = u64::MAX;
    assert_eq!(
        owner.note_rejected(),
        Err(ObservationError::CounterOverflow)
    );
    owner.totals.input_started = u64::MAX;
    assert_eq!(
        owner.note_input_started(),
        Err(ObservationError::CounterOverflow)
    );
    let mut owner = observation();
    assert_eq!(
        owner.tick_due(99, true),
        Err(ObservationError::ClockReversed)
    );
    assert_eq!(
        owner.prepare(Transition::new(FlowPhase::Sampled), gauges(0), 99),
        Err(ObservationError::ClockReversed)
    );
    let future = GaugeSnapshot::from_sources(
        "run",
        [source(&a, None, 1, 0, 90), source(&b, None, 1, 0, 120)],
        0,
        (2, 10),
        None,
    )
    .unwrap();
    assert_eq!(
        owner.prepare(Transition::new(FlowPhase::Sampled), future, 110),
        Err(ObservationError::ClockReversed),
        "future newer intake is checked even when oldest is valid"
    );
    prepare(&mut owner, FlowPhase::Sampled, gauges(0), 120);
    assert_eq!(
        owner.tick_due(119, true),
        Err(ObservationError::ClockReversed)
    );
    assert!(matches!(
        owner.final_sample(gauges(0), 119),
        Err(ObservationError::ClockReversed)
    ));
    owner.baseline_ns = u64::MAX;
    owner.pending = None;
    assert_eq!(
        owner.next_tick(true),
        Err(ObservationError::CounterOverflow)
    );
}

use crate::flow::recording_fixture as fixture;

#[test]
fn actual_trace_admission_alone_can_confirm_a_sample_and_release_terminal_proof() {
    use crate::flow::intake::{FlowInbox, RetentionBudget};
    use crate::flow::recording::{RecordBarrier, RecordProgress, RecordProof, RecordPurpose};
    use std::os::unix::net::UnixStream;
    use std::time::Duration;
    use weaver_trace::serving::{
        Channel, Envelope, Event, Kind, Origin, Payload, PreflightAttempt, PreflightRecorder,
        validate_partition,
    };
    use weaver_trace::{MonotonicNs, RunRef, Sequence, SessionRef, Subsystem};
    use weaver_types::state_flow::FlowBacklog;
    let envelope = |kind, channel| Envelope {
        session: SessionRef("session".into()),
        run: RunRef("run".into()),
        turn: None,
        sequence: Sequence(999),
        kind,
        channel,
        origin: Origin::Runtime,
        subsystem: Subsystem::Harness,
        causal_parent: None,
        wall_ms: 1,
        monotonic_ns: MonotonicNs(7),
    };
    let (sink, _reader) = UnixStream::pair().unwrap();
    sink.set_write_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let recorder = PreflightRecorder::receive(
        sink.into(),
        RunRef("run".into()),
        SessionRef("session".into()),
    )
    .unwrap();
    let PreflightAttempt::Recorded { recorder, .. } = recorder
        .record_load(Event {
            envelope: envelope(Kind::Load, Channel::Internal),
            payload: Some(Payload::Elections(Box::new(fixture::load_record()))),
        })
        .unwrap_or_else(|error| panic!("load failed: {:?}", error.failure))
    else {
        panic!("empty recorder capacity");
    };
    let mut barrier = RecordBarrier::new(
        recorder.activate(validate_partition(&fixture::partition()).unwrap()),
        "run".into(),
    );
    let mut inbox = FlowInbox::new();
    let mut budget = RetentionBudget::new(FlowBacklog {
        max_pending_blocks: 10.into(),
        max_pending_bytes: 10000.into(),
        lag_alarm_turns: 2.into(),
        measurement_ref: "test-only".into(),
    })
    .unwrap();
    let mut owner = observation();
    owner.note_filed(13).unwrap();
    let mut transition = Transition::new(FlowPhase::Filed);
    transition.source_id = Some(id(1, 0));
    assert_eq!(
        owner.prepare(transition, gauges(0), 110),
        Ok(Prepared::Prepared)
    );
    let sample = owner.pending().unwrap().clone();
    let RecordProgress::Recorded {
        proof: Some(RecordProof::Flow(proof)),
        ..
    } = barrier.submit(
        Event {
            envelope: envelope(Kind::StateFlow, Channel::Measurement),
            payload: Some(Payload::FlowObservation(Box::new(sample.clone()))),
        },
        RecordPurpose::Flow(sample.clone()),
        &mut inbox,
        &mut budget,
    )
    else {
        panic!("real sample admission");
    };
    assert_eq!(
        owner.baseline_ns, 100,
        "admission alone does not mutate the accounting owner"
    );
    assert_eq!(owner.pending(), Some(&sample));
    assert_eq!(
        owner.recorded(proof).unwrap().unwrap().into_source(),
        id(1, 0)
    );
    assert_eq!(owner.baseline_ns, 110);
    assert!(owner.pending().is_none());
    assert_eq!(budget.gauges(), (0, 0));
    assert_eq!(
        inbox.queued().count(),
        0,
        "flow observations cannot tap themselves"
    );
}
