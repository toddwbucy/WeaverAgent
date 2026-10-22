use super::*;
use crate::flow::author::{AuthoredEvent, AuthoredSource, ServingAuthor};

fn author() -> ServingAuthor {
    ServingAuthor::new("session".into(), "run".into(), 42, 1_000).unwrap()
}

fn authored(text: &str) -> AuthoredEvent {
    AuthoredEvent {
        kind: Kind::MessageUser,
        subsystem: Subsystem::Harness,
        turn: Some(TurnRef("turn".into())),
        source: AuthoredSource::BoundaryInput(42),
        causal_parent: Some(Sequence(0)),
        payload: message(text).payload,
    }
}

fn fault() -> AuthoredEvent {
    AuthoredEvent {
        kind: Kind::Fault,
        subsystem: Subsystem::Harness,
        turn: None,
        source: AuthoredSource::Runtime,
        causal_parent: None,
        payload: Some(Payload::Fault(
            serde_json::value::to_raw_value(
                &serde_json::json!({"case":"state_flow_failed","account":{"fixture":true}}),
            )
            .unwrap(),
        )),
    }
}

fn same_original(actual: &AuthoredEvent, expected: &AuthoredEvent) {
    assert_eq!(format!("{actual:?}"), format!("{expected:?}"));
}

#[test]
fn owed_refusal_queues_only_after_halt_and_cannot_replace_an_original_or_admit_content() {
    let (recorder, _reader) = recorder();
    let mut barrier = RecordBarrier::new(recorder, "run".into());
    let original = AuthoredEvent {
        kind: Kind::Refusal,
        subsystem: Subsystem::SpuDecoder,
        turn: None,
        source: AuthoredSource::Runtime,
        causal_parent: None,
        payload: Some(Payload::Refusal(
            serde_json::value::to_raw_value(&weaver_types::RefusalRecord::Decode {
                asked: weaver_types::TokenAsk::MeasureDelta { keep: 19 },
                refusal: weaver_types::TokenRefusal::NotOpen,
            })
            .unwrap(),
        )),
    };
    let refused = barrier
        .queue_trace_only_authored(original.clone(), &author(), 1_001, 1)
        .unwrap_err();
    same_original(&refused.event, &original);
    assert!(barrier.pending_author().is_none());
    barrier.stop_for_flow_failure();
    let content = authored("Read forbidden content.");
    let refused = barrier
        .queue_trace_only_authored(content.clone(), &author(), 1_002, 1)
        .unwrap_err();
    same_original(&refused.event, &content);
    assert!(barrier.pending_author().is_none());
    barrier
        .queue_trace_only_authored(original.clone(), &author(), 1_003, 1)
        .unwrap();
    let ticket = barrier.pending_author().unwrap().ticket;
    let refused = barrier
        .queue_trace_only_authored(original.clone(), &author(), 1_004, 1)
        .unwrap_err();
    same_original(&refused.event, &original);
    assert_eq!(barrier.pending_author().unwrap().ticket, ticket);
    same_original(&barrier.pending_author().unwrap().event, &original);
    assert!(barrier.resume_trace_only().unwrap().is_some());
    assert_eq!(barrier.resume_trace_only().unwrap(), None);
    assert!(barrier.recorder().tap_stopped());
    assert_eq!(
        barrier
            .recorder()
            .structure()
            .by_kind(Kind::Refusal)
            .count(),
        1
    );
    assert_eq!(
        barrier
            .recorder()
            .structure()
            .by_kind(Kind::MessageUser)
            .count(),
        0
    );
}

#[test]
fn owned_ticket_preserves_source_and_prepares_only_once_under_pressure() {
    let (mut recorder, reader) = recorder();
    fill_capacity(&mut recorder);
    let mut barrier = RecordBarrier::new(recorder, "run".into());
    let mut inbox = FlowInbox::new();
    let mut budget = budget(1);
    let original = authored("Read retained original.");
    assert!(matches!(
        barrier.submit_authored_with_clock(
            original.clone(),
            RecordPurpose::Content,
            &author(),
            777,
            &mut inbox,
            &mut budget,
            &mut || Ok(1_023),
        ),
        RecordProgress::Pending
    ));
    let ticket = barrier.pending_author().expect("original ticket");
    assert_eq!(ticket.ticket, 0);
    same_original(&ticket.event, &original);
    assert!(matches!(
        ticket.event.source,
        AuthoredSource::BoundaryInput(42)
    ));
    let prepared = serde_json::to_string(barrier.pending().unwrap()).unwrap();
    for _ in 0..4 {
        assert!(matches!(
            barrier.resume_with_clock(&mut inbox, &mut budget, &mut || panic!(
                "resuming capacity cannot reprepare"
            )),
            RecordProgress::Pending
        ));
        assert_eq!(
            serde_json::to_string(barrier.pending().unwrap()).unwrap(),
            prepared
        );
    }
    let reader = drain(reader);
    let deadline = Instant::now() + Duration::from_secs(8);
    let sequence = loop {
        match barrier.resume_with_clock(&mut inbox, &mut budget, &mut || Ok(9_000)) {
            RecordProgress::Recorded { sequence, .. } => break sequence,
            RecordProgress::Pending => {
                assert!(Instant::now() < deadline);
                std::thread::yield_now();
            }
            other => panic!("{other:?}"),
        }
    };
    assert!(barrier.pending_author().is_none());
    assert!(barrier.admitted_before_failure().is_none());
    drop(barrier);
    let bytes = reader.join().unwrap();
    let sequence_text = sequence.0.to_string();
    let record: serde_json::Value = std::str::from_utf8(&bytes)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .find(|v| v["sequence"].as_str() == Some(sequence_text.as_str()))
        .unwrap();
    assert_eq!(record["wall_ms"], 777);
    assert_eq!(record["monotonic_ns"], "23");
    assert_eq!(record["causal_parent"], "0");
    assert_eq!(inbox.queued().count(), 1);
}

#[test]
fn second_owned_author_returns_original_without_replacing_first_ticket() {
    let (mut recorder, reader) = recorder();
    fill_capacity(&mut recorder);
    let mut barrier = RecordBarrier::new(recorder, "run".into());
    let mut inbox = FlowInbox::new();
    let mut budget = budget(2);
    let first = authored("Read first.");
    assert!(matches!(
        barrier.submit_authored_with_clock(
            first.clone(),
            RecordPurpose::Content,
            &author(),
            1,
            &mut inbox,
            &mut budget,
            &mut || Ok(1_001),
        ),
        RecordProgress::Pending
    ));
    let mut second = authored("Read second.");
    second.source = AuthoredSource::BoundaryInput(9876);
    match barrier.submit_authored_with_clock(
        second.clone(),
        RecordPurpose::Content,
        &author(),
        2,
        &mut inbox,
        &mut budget,
        &mut || panic!("busy cannot prepare"),
    ) {
        RecordProgress::BusyAuthored {
            event,
            purpose: RecordPurpose::Content,
        } => same_original(&event, &second),
        other => panic!("{other:?}"),
    }
    same_original(&barrier.pending_author().unwrap().event, &first);
    assert!(matches!(
        barrier.failure(),
        Some(RecordFailure::Protocol(_))
    ));
    assert!(budget.is_frozen());
    let reader = drain(reader);
    drop(barrier);
    reader.join().unwrap();
}

#[test]
fn failure_before_admission_keeps_original_but_no_invented_sequence() {
    for clock_fails in [false, true] {
        let (recorder, _reader) = recorder();
        let mut barrier = RecordBarrier::new(recorder, "run".into());
        let mut inbox = FlowInbox::new();
        let mut budget = budget(1);
        let original = authored("Read before clock origin.");
        assert!(matches!(
            barrier.submit_authored_with_clock(
                original.clone(),
                RecordPurpose::Content,
                &author(),
                1,
                &mut inbox,
                &mut budget,
                &mut || if clock_fails {
                    Err(std::io::Error::other("fixture clock failure"))
                } else {
                    Ok(999)
                },
            ),
            RecordProgress::Failed
        ));
        same_original(&barrier.pending_author().unwrap().event, &original);
        assert!(barrier.pending().is_none());
        assert!(barrier.admitted_before_failure().is_none());
        assert!(budget.is_frozen());
        assert_eq!(barrier.recorder().structure().len(), 1);
    }
}

#[test]
fn post_admission_tap_and_intake_failure_keep_sequence_not_pending_author() {
    for tap_fails in [false, true] {
        let (recorder, _reader) = recorder();
        let mut barrier = RecordBarrier::new(recorder, "run".into());
        let mut inbox = FlowInbox::new();
        let mut budget = budget(1);
        let mut original = authored("Read broken intake.");
        if tap_fails {
            original.payload = Some(Payload::Message(
                serde_json::value::to_raw_value(
                    &serde_json::json!({"role":"user","content":"malformed blocks"}),
                )
                .unwrap(),
            ));
        }
        let mut clocks = 0;
        assert!(matches!(
            barrier.submit_authored_with_clock(
                original,
                RecordPurpose::Content,
                &author(),
                1,
                &mut inbox,
                &mut budget,
                &mut || {
                    clocks += 1;
                    if clocks == 1 {
                        Ok(1_001)
                    } else {
                        Err(std::io::Error::other("fixture intake clock failure"))
                    }
                },
            ),
            RecordProgress::Failed
        ));
        assert_eq!(barrier.admitted_before_failure(), Some(Sequence(1)));
        assert!(barrier.pending_author().is_none());
        assert!(barrier.pending().is_none());
        assert_eq!(barrier.recorder().structure().len(), 2);
        assert!(budget.is_frozen());
        if tap_fails {
            assert!(matches!(barrier.failure(), Some(RecordFailure::Tap(_))));
        } else {
            assert!(inbox.failure().is_some());
        }
    }
}

#[test]
fn terminal_reserve_bypasses_ticket_then_trace_only_resume_never_files() {
    let (mut recorder, reader) = recorder();
    fill_capacity(&mut recorder);
    let count = recorder.structure().len();
    let mut barrier = RecordBarrier::new(recorder, "run".into());
    let mut inbox = FlowInbox::new();
    let mut budget = budget(1);
    let original = authored("Read trace-only pending source.");
    assert!(matches!(
        barrier.submit_authored_with_clock(
            original.clone(),
            RecordPurpose::Content,
            &author(),
            1,
            &mut inbox,
            &mut budget,
            &mut || Ok(1_001),
        ),
        RecordProgress::Pending
    ));
    assert!(barrier.resume_trace_only().is_err());
    let premature = barrier
        .submit_terminal_authored(fault(), &author(), 1_002, 2)
        .unwrap_err();
    same_original(&premature.event, &fault());
    assert!(!barrier.recorder().tap_stopped());
    same_original(&barrier.pending_author().unwrap().event, &original);
    assert_eq!(barrier.recorder().structure().len(), count);
    barrier.stop_for_flow_failure();
    budget.freeze();
    let first_fault = barrier
        .submit_terminal_authored(fault(), &author(), 1_002, 2)
        .unwrap();
    assert_eq!(barrier.recorder().structure().len(), count + 1);
    same_original(&barrier.pending_author().unwrap().event, &original);
    let duplicate = barrier
        .submit_terminal_authored(fault(), &author(), 1_003, 3)
        .unwrap_err();
    same_original(&duplicate.event, &fault());
    let reader = drain(reader);
    let deadline = Instant::now() + Duration::from_secs(8);
    let sequence = loop {
        if let Some(sequence) = barrier.resume_trace_only().unwrap() {
            break sequence;
        }
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    };
    assert!(sequence.0 > first_fault.0);
    assert!(barrier.pending_author().is_none());
    assert_eq!(barrier.resume_trace_only().unwrap(), None);
    assert_eq!(budget.gauges(), (0, 0));
    assert_eq!(inbox.queued().count(), 0);
    drop(barrier);
    let bytes = reader.join().unwrap();
    let text = std::str::from_utf8(&bytes).unwrap();
    assert_eq!(text.matches("Read trace-only pending source.").count(), 1);
}

#[test]
fn exhausted_ticket_counter_returns_original_without_preparing_or_wrapping() {
    let (recorder, _reader) = recorder();
    let mut barrier = RecordBarrier::new(recorder, "run".into());
    barrier.next_ticket = Some(u64::MAX);
    let mut inbox = FlowInbox::new();
    let mut budget = budget(1);
    let original = authored("Read counter limit.");
    assert!(matches!(
        barrier.submit_authored_with_clock(
            original.clone(),
            RecordPurpose::Content,
            &author(),
            1,
            &mut inbox,
            &mut budget,
            &mut || Ok(1_001),
        ),
        RecordProgress::Recorded { .. }
    ));
    assert!(barrier.next_ticket.is_none());
    match barrier.submit_authored_with_clock(
        original.clone(),
        RecordPurpose::Content,
        &author(),
        1,
        &mut inbox,
        &mut budget,
        &mut || panic!("exhausted counter cannot prepare"),
    ) {
        RecordProgress::BusyAuthored { event, .. } => same_original(&event, &original),
        other => panic!("{other:?}"),
    }
    assert!(matches!(
        barrier.failure(),
        Some(RecordFailure::Protocol(_))
    ));
    assert!(barrier.pending_author().is_none());
    assert!(budget.is_frozen());
}

#[test]
fn idle_writer_failure_wake_latches_and_freezes_without_pending_author() {
    let (mut recorder, reader) = recorder();
    fill_capacity(&mut recorder);
    drop(reader);
    let mut barrier = RecordBarrier::new(recorder, "run".into());
    let mut budget = budget(1);
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        if barrier.try_capacity_wake(&mut budget).is_err() {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert!(matches!(barrier.failure(), Some(RecordFailure::Trace(_))));
    assert!(budget.is_frozen());
    assert!(barrier.pending_author().is_none());
    assert!(barrier.recorder().tap_stopped());
    assert!(barrier.drain_status(&mut budget).is_err());
}

#[test]
fn whole_copy_backlog_returns_next_original_before_preparation_without_halting() {
    let (recorder, _reader) = recorder();
    let mut barrier = RecordBarrier::new(recorder, "run".into());
    let mut inbox = FlowInbox::new();
    let mut budget = budget(1);
    for text in ["Read charged source.", "Read waiting source."] {
        assert!(matches!(
            barrier.submit_authored_with_clock(
                authored(text),
                RecordPurpose::Content,
                &author(),
                1,
                &mut inbox,
                &mut budget,
                &mut || Ok(1_001),
            ),
            RecordProgress::Recorded { .. }
        ));
    }
    assert!(budget.waiting_copy().is_some());
    let original = authored("Read retained caller input.");
    match barrier.submit_authored_with_clock(
        original.clone(),
        RecordPurpose::Content,
        &author(),
        1,
        &mut inbox,
        &mut budget,
        &mut || panic!("backlog guard precedes preparation"),
    ) {
        RecordProgress::BusyAuthored {
            event,
            purpose: RecordPurpose::Content,
        } => same_original(&event, &original),
        other => panic!("{other:?}"),
    }
    assert!(barrier.failure().is_none());
    assert!(!budget.is_frozen());
    assert!(barrier.pending_author().is_none());
    assert_eq!(barrier.next_ticket, Some(2));
    assert_eq!(barrier.recorder().structure().len(), 3);
}
