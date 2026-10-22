use super::*;
use std::sync::Arc;
use weaver_trace::Sequence;
use weaver_trace::serving::{ContextBlock, PairCopy, PathPart, SourceBlock, StatementCopy};
use weaver_types::state_flow::{DecimalU64, SourceBlockId};

fn limits(blocks: u64, bytes: u64) -> FlowBacklog {
    FlowBacklog {
        max_pending_blocks: DecimalU64(blocks),
        max_pending_bytes: DecimalU64(bytes),
        lag_alarm_turns: DecimalU64(2),
        measurement_ref: "fixture-only measured tuple".into(),
    }
}

fn envelope(sequence: u64) -> Arc<str> {
    Arc::from(
        serde_json::json!({
            "session":"session", "run":"run", "turn":"turn",
            "sequence":sequence.to_string(), "kind":"message.user",
            "channel":"input", "origin":"operator", "subsystem":"harness",
            "wall_ms":1, "monotonic_ns":"7"
        })
        .to_string(),
    )
}

fn statement(sequence: u64, indices: &[u64], context: bool) -> AdmittedCopy {
    AdmittedCopy::Statement(StatementCopy {
        source_envelope: envelope(sequence),
        partition_rule_id: "exact source rule".into(),
        blocks: indices
            .iter()
            .map(|index| SelectedBlock {
                source: SourceBlock {
                    run: "run".into(),
                    sequence: Sequence(sequence),
                    block: *index,
                },
                source_path: vec![
                    PathPart::Key("message".into()),
                    PathPart::Key("content".into()),
                    PathPart::Index(*index),
                ],
                raw: Arc::from(format!(
                    "{{ \"type\":\"text\", \"text\":\"Read caf\u{e9} {index}.\\n\" }}"
                )),
            })
            .collect(),
        context: context.then(|| ContextBlock {
            source_path: vec![PathPart::Key("context".into())],
            raw: Arc::from("{ \"step\" : \"step-14\", \"nested\" : [1, 2] }"),
        }),
    })
}

fn pair(sequence: u64) -> AdmittedCopy {
    AdmittedCopy::Pair(PairCopy {
        source_envelope: envelope(sequence),
        partition_rule_id: "pair rule".into(),
        block: SelectedBlock {
            source: SourceBlock {
                run: "run".into(),
                sequence: Sequence(sequence),
                block: 0,
            },
            source_path: Vec::new(),
            raw: Arc::from("{ \"pair\" : \"whole return, including whitespace\" }"),
        },
    })
}

fn total_bytes(copy: &AdmittedCopy) -> u64 {
    let blocks = selected_blocks(copy);
    if blocks.is_empty() {
        return 0;
    }
    let shared = match copy {
        AdmittedCopy::Statement(copy) => {
            copy.source_envelope.len() + copy.context.as_ref().map_or(0, |c| c.raw.len())
        }
        AdmittedCopy::Pair(copy) => copy.source_envelope.len(),
    };
    (shared + blocks.iter().map(|block| block.raw.len()).sum::<usize>()) as u64
}

fn key(sequence: u64) -> SourceEventKey {
    SourceEventKey {
        run: "run".into(),
        sequence,
    }
}

fn recorded(sequence: u64, block: u64) -> RecordedTerminal {
    RecordedTerminal::for_test(SourceBlockId {
        run: "run".into(),
        sequence: DecimalU64(sequence),
        block: DecimalU64(block),
    })
}

fn failed(outcome: IntakeOutcome, expected: IntakeError) -> AdmittedCopy {
    match outcome {
        IntakeOutcome::IntakeFailed { copy, detail } => {
            assert_eq!(detail, expected);
            copy
        }
        other => panic!("expected a whole failed copy, got {other:?}"),
    }
}

#[test]
fn whole_copy_identity_paths_raw_bytes_and_source_order_survive_transfer() {
    let first = statement(41, &[0, 3, 8], true);
    let second = statement(45, &[2], false);
    let expected = [first.clone(), second.clone()];
    let mut budget = RetentionBudget::new(limits(4, 20_000)).unwrap();
    let mut inbox = FlowInbox::new();
    assert!(matches!(
        inbox.accept(first, &mut budget),
        IntakeOutcome::Accepted
    ));
    assert!(matches!(
        inbox.accept(second, &mut budget),
        IntakeOutcome::Accepted
    ));
    let gauges = (4, expected.iter().map(total_bytes).sum());
    assert_eq!(budget.gauges(), gauges);
    assert_eq!(
        inbox.queued().collect::<Vec<_>>(),
        expected.iter().collect::<Vec<_>>()
    );
    for wanted in expected {
        let copy = inbox.pop_front(&budget).unwrap();
        assert_eq!(copy, wanted);
        // Moving into the ledger grants neither a receipt nor a terminal record.
        assert_eq!(budget.gauges(), gauges);
        if let (AdmittedCopy::Statement(actual), AdmittedCopy::Statement(wanted)) = (&copy, &wanted)
        {
            assert!(Arc::ptr_eq(
                &actual.source_envelope,
                &wanted.source_envelope
            ));
            for (actual, wanted) in actual.blocks.iter().zip(&wanted.blocks) {
                assert!(Arc::ptr_eq(&actual.raw, &wanted.raw));
            }
        }
    }
    assert!(inbox.pop_front(&budget).is_none());
    assert_eq!(budget.reservations.len(), 2);
}

#[test]
fn only_recorded_block_disposition_releases_shared_charge_on_the_last_block() {
    let copy = statement(5, &[0, 7], true);
    let bytes = total_bytes(&copy);
    let first_bytes = selected_blocks(&copy)[0].raw.len() as u64;
    let mut budget = RetentionBudget::new(limits(2, bytes)).unwrap();
    let mut inbox = FlowInbox::new();
    assert!(matches!(
        inbox.accept(copy, &mut budget),
        IntakeOutcome::Accepted
    ));
    let held = inbox.pop_front(&budget).unwrap();
    assert_eq!(budget.gauges(), (2, bytes));
    assert_eq!(selected_blocks(&held).len(), 2);
    budget.release(recorded(5, 0)).unwrap();
    assert_eq!(budget.gauges(), (1, bytes - first_bytes));
    let reservation = budget.reservation(&key(5)).unwrap();
    assert_eq!(reservation.blocks.len(), 1);
    assert_eq!(reservation.blocks[0].block, 7);
    assert!(reservation.shared_bytes > 0);
    assert_eq!(
        budget.release(recorded(5, 0)),
        Err(IntakeError::UnknownBlock)
    );
    assert_eq!(budget.gauges(), (1, bytes - first_bytes));
    budget.release(recorded(5, 7)).unwrap();
    assert_eq!(budget.gauges(), (0, 0));
    assert!(budget.reservation(&key(5)).is_none());
    assert_eq!(
        budget.release(recorded(5, 7)),
        Err(IntakeError::UnknownBlock)
    );
}

#[test]
fn pair_is_one_complete_block_and_zero_block_selection_has_no_source_charge() {
    let pair = pair(1);
    let bytes = total_bytes(&pair);
    let empty = statement(2, &[], true);
    let mut budget = RetentionBudget::new(limits(1, bytes)).unwrap();
    let mut inbox = FlowInbox::new();
    assert!(matches!(
        inbox.accept(pair.clone(), &mut budget),
        IntakeOutcome::Accepted
    ));
    assert!(matches!(
        inbox.accept(empty.clone(), &mut budget),
        IntakeOutcome::Accepted
    ));
    assert_eq!(budget.gauges(), (1, bytes));
    assert_eq!(budget.reservation(&key(1)).unwrap().blocks.len(), 1);
    assert!(budget.reservation(&key(2)).is_none());
    assert_eq!(inbox.pop_front(&budget), Some(pair));
    assert_eq!(inbox.pop_front(&budget), Some(empty));
    budget.release(recorded(1, 0)).unwrap();
    assert_eq!(budget.gauges(), (0, 0));
}

#[test]
fn partial_block_fit_waits_whole_and_recorded_release_wakes_the_same_copy() {
    let first = statement(10, &[0], false);
    let waiting = statement(11, &[0, 4], true);
    let first_bytes = total_bytes(&first);
    let waiting_bytes = total_bytes(&waiting);
    let mut budget = RetentionBudget::new(limits(2, 20_000)).unwrap();
    let mut inbox = FlowInbox::new();
    assert!(matches!(
        inbox.accept(first.clone(), &mut budget),
        IntakeOutcome::Accepted
    ));
    assert!(matches!(
        inbox.accept(waiting.clone(), &mut budget),
        IntakeOutcome::Deferred
    ));
    assert_eq!(budget.gauges(), (1, first_bytes));
    assert_eq!(budget.waiting_copy(), Some(&waiting));
    assert!(budget.reservation(&key(11)).is_none());
    assert_eq!(inbox.queued().count(), 1);
    assert!(matches!(
        inbox.resume_waiting(&mut budget),
        Some(IntakeOutcome::Deferred)
    ));
    assert_eq!(budget.waiting_copy(), Some(&waiting));
    assert_eq!(inbox.pop_front(&budget), Some(first));
    assert_eq!(budget.gauges(), (1, first_bytes));
    budget.release(recorded(10, 0)).unwrap();
    assert!(matches!(
        inbox.resume_waiting(&mut budget),
        Some(IntakeOutcome::Accepted)
    ));
    assert_eq!(budget.gauges(), (2, waiting_bytes));
    assert!(budget.waiting_copy().is_none());
    assert_eq!(inbox.pop_front(&budget), Some(waiting));
    assert!(inbox.resume_waiting(&mut budget).is_none());
}

#[test]
fn bytes_are_atomic_and_a_copy_alone_over_maximum_remains_owned_waiting() {
    let first = statement(10, &[0], false);
    let next = statement(11, &[0, 2], true);
    let bytes = total_bytes(&first) + total_bytes(&next) - 1;
    let mut budget = RetentionBudget::new(limits(10, bytes)).unwrap();
    let mut inbox = FlowInbox::new();
    assert!(matches!(
        inbox.accept(first.clone(), &mut budget),
        IntakeOutcome::Accepted
    ));
    assert!(matches!(
        inbox.accept(next.clone(), &mut budget),
        IntakeOutcome::Deferred
    ));
    assert_eq!(budget.gauges(), (1, total_bytes(&first)));
    assert_eq!(budget.waiting_copy(), Some(&next));

    let mut budget = RetentionBudget::new(limits(1, 1)).unwrap();
    let mut inbox = FlowInbox::new();
    assert!(matches!(
        inbox.accept(next.clone(), &mut budget),
        IntakeOutcome::Deferred
    ));
    for _ in 0..3 {
        assert!(matches!(
            inbox.resume_waiting(&mut budget),
            Some(IntakeOutcome::Deferred)
        ));
        assert_eq!(budget.gauges(), (0, 0));
        assert_eq!(budget.waiting_copy(), Some(&next));
        assert!(inbox.failure().is_none());
    }
    let refused = failed(
        inbox.accept(first.clone(), &mut budget),
        IntakeError::WaitingOccupied,
    );
    assert_eq!(refused, first);
    assert_eq!(budget.waiting_copy(), Some(&next));
}

#[test]
fn duplicate_reservation_and_every_block_identity_failure_keep_the_whole_copy() {
    let valid = statement(6, &[0, 3], true);
    let mut budget = RetentionBudget::new(limits(20, 20_000)).unwrap();
    let mut inbox = FlowInbox::new();
    assert!(matches!(
        inbox.accept(valid.clone(), &mut budget),
        IntakeOutcome::Accepted
    ));
    let before = budget.gauges();
    assert_eq!(
        failed(
            inbox.accept(valid.clone(), &mut budget),
            IntakeError::DuplicateReservation
        ),
        valid
    );
    assert_eq!(budget.gauges(), before);
    for (case, expected) in [
        (0, IntakeError::InvalidIdentity),
        (1, IntakeError::InvalidIdentity),
        (2, IntakeError::DuplicateBlock),
        (3, IntakeError::DuplicateBlock),
    ] {
        let mut copy = statement(7, &[1, 5], true);
        let AdmittedCopy::Statement(statement) = &mut copy else {
            unreachable!()
        };
        match case {
            0 => statement.blocks[1].source.run = "other run".into(),
            1 => statement.blocks[1].source.sequence = Sequence(8),
            2 => statement.blocks[1].source.block = 1,
            3 => statement.blocks[1].source.block = 0,
            _ => unreachable!(),
        }
        assert_eq!(
            failed(inbox.accept(copy.clone(), &mut budget), expected),
            copy
        );
        assert_eq!(budget.gauges(), before);
        assert!(budget.reservation(&key(7)).is_none());
        assert_eq!(inbox.queued().count(), 1);
    }
}

#[test]
fn every_reportable_reservation_allocation_failure_returns_the_complete_copy() {
    for point in [
        AllocationPoint::Blocks,
        AllocationPoint::Reservations,
        AllocationPoint::Inbox,
    ] {
        let copy = statement(5, &[0, 2, 4], true);
        let mut budget = RetentionBudget::new(limits(3, 20_000)).unwrap();
        let mut inbox = FlowInbox::new();
        inbox.fail_allocation = Some(point);
        assert_eq!(
            failed(
                inbox.accept(copy.clone(), &mut budget),
                IntakeError::AllocationFailed
            ),
            copy
        );
        assert_eq!(budget.gauges(), (0, 0));
        assert!(budget.reservations.is_empty());
        assert!(budget.waiting_copy().is_none());
        assert_eq!(inbox.queued().count(), 0);
    }
}

#[test]
fn arithmetic_overflow_is_a_preserved_failure_and_never_wrapped_room() {
    for blocks_overflow in [true, false] {
        let mut budget = RetentionBudget::new(limits(u64::MAX, u64::MAX)).unwrap();
        if blocks_overflow {
            budget.blocks = u64::MAX;
        } else {
            budget.bytes = u64::MAX;
        }
        let before = budget.gauges();
        let copy = statement(1, &[0], false);
        let mut inbox = FlowInbox::new();
        assert_eq!(
            failed(
                inbox.accept(copy.clone(), &mut budget),
                IntakeError::CounterOverflow
            ),
            copy
        );
        assert_eq!(budget.gauges(), before);
        assert!(budget.reservations.is_empty());
        assert!(budget.waiting_copy().is_none());
    }
}

#[test]
fn release_validates_all_counters_before_changing_the_reservation() {
    let copy = statement(1, &[0, 5], true);
    let mut budget = RetentionBudget::new(limits(2, 20_000)).unwrap();
    let mut inbox = FlowInbox::new();
    assert!(matches!(
        inbox.accept(copy, &mut budget),
        IntakeOutcome::Accepted
    ));
    budget.bytes = 0;
    assert_eq!(
        budget.release(recorded(1, 0)),
        Err(IntakeError::CounterOverflow)
    );
    assert_eq!(budget.gauges(), (2, 0));
    assert_eq!(budget.reservation(&key(1)).unwrap().blocks.len(), 2);
    assert_eq!(
        budget.release(recorded(1, 2)),
        Err(IntakeError::UnknownBlock)
    );
    assert_eq!(budget.reservation(&key(1)).unwrap().blocks.len(), 2);
}

#[test]
fn halt_freezes_queued_waiting_and_failed_copies_and_never_overwrites_failure() {
    let queued = statement(1, &[0], false);
    let waiting = statement(2, &[0], true);
    let failed_copy = statement(3, &[0], false);
    let mut budget = RetentionBudget::new(limits(1, 20_000)).unwrap();
    let mut inbox = FlowInbox::new();
    assert!(matches!(
        inbox.accept(queued.clone(), &mut budget),
        IntakeOutcome::Accepted
    ));
    assert!(matches!(
        inbox.accept(waiting.clone(), &mut budget),
        IntakeOutcome::Deferred
    ));
    inbox
        .preserve_failure(IntakeFailure {
            copy: failed_copy.clone(),
            detail: IntakeError::AllocationFailed,
        })
        .unwrap();
    let second = statement(4, &[0], false);
    let returned = inbox
        .preserve_failure(IntakeFailure {
            copy: second.clone(),
            detail: IntakeError::InvalidIdentity,
        })
        .unwrap_err();
    assert_eq!(returned.copy, second);
    assert_eq!(returned.detail, IntakeError::InvalidIdentity);
    assert_eq!(inbox.failure().unwrap().copy, failed_copy);
    let before = budget.gauges();
    budget.freeze();
    budget.freeze();
    assert_eq!(budget.release(recorded(1, 0)), Err(IntakeError::Frozen));
    assert!(inbox.pop_front(&budget).is_none());
    assert!(inbox.resume_waiting(&mut budget).is_none());
    assert_eq!(
        failed(
            inbox.accept(second.clone(), &mut budget),
            IntakeError::Frozen
        ),
        second
    );
    assert_eq!(budget.gauges(), before);
    assert_eq!(inbox.queued().collect::<Vec<_>>(), vec![&queued]);
    assert_eq!(budget.waiting_copy(), Some(&waiting));
    assert_eq!(inbox.failure().unwrap().copy, failed_copy);
}

#[test]
fn invalid_limits_and_envelopes_are_refused_without_electing_fallback_values() {
    assert!(matches!(
        RetentionBudget::new(limits(0, 1)),
        Err(IntakeError::InvalidLimits)
    ));
    assert!(matches!(
        RetentionBudget::new(limits(1, 0)),
        Err(IntakeError::InvalidLimits)
    ));
    let declared = limits(4, 5000);
    let mut budget = RetentionBudget::new(declared.clone()).unwrap();
    assert_eq!(budget.limits(), &declared);
    let mut inbox = FlowInbox::new();
    let mut copy = statement(2, &[0], false);
    let AdmittedCopy::Statement(statement) = &mut copy else {
        unreachable!()
    };
    statement.source_envelope = Arc::from("{\"run\":\"run\",\"sequence\":\"2\"}");
    assert_eq!(
        failed(
            inbox.accept(copy.clone(), &mut budget),
            IntakeError::InvalidEnvelope
        ),
        copy
    );
    assert_eq!(budget.gauges(), (0, 0));
    assert_eq!(inbox.queued().count(), 0);
}
