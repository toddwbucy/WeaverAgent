use super::*;
use weaver_trace::TurnRef;
use weaver_traits::{Message, ToolResultBlock};
use weaver_types::Finish;
use weaver_types::state_flow::{Digest, EmissionChangeMethod, Span, ThesaurusMatch};

fn source(block: u64) -> SourceBlockId {
    SourceBlockId {
        run: "run".into(),
        sequence: 10.into(),
        block: block.into(),
    }
}

fn span(start: usize, end: usize) -> Span {
    Span {
        start: (start as u64).into(),
        end: (end as u64).into(),
    }
}

fn original() -> OutputOriginal {
    OutputOriginal {
        message: Message {
            role: Role::Assistant,
            content: vec![
                ContentBlock::Text {
                    text: "Use coal.".into(),
                },
                ContentBlock::UnrecoveredCall {
                    fragment: "raw call".into(),
                },
                ContentBlock::Text {
                    text: String::new(),
                },
                ContentBlock::Text {
                    text: "Use coal.".into(),
                },
            ],
        },
        finish: Finish::Length,
        turn: TurnRef("turn".into()),
    }
}

fn position() -> SourcePosition {
    SourcePosition {
        block_id: source(0),
        turn: Some("turn".into()),
    }
}

fn book() -> ChangeBook {
    ChangeBook::new(original(), position()).unwrap()
}

fn change(block: u64) -> EmissionChangeRecord {
    EmissionChangeRecord {
        matched: ThesaurusMatch {
            source: source(block),
            previous_change: None,
            sentence_span: span(0, 9),
            word_span: span(4, 8),
            scheme_id: format!("ste100:{}", "b".repeat(64)),
            thesaurus_sha256: Digest::new("a".repeat(64)).unwrap(),
            entry_word: "coal".into(),
        },
        span: span(4, 8),
        replaced: "coal".into(),
        replacement: "fuel".into(),
        method: EmissionChangeMethod::Splice {},
    }
}

fn unchanged(owner: &ChangeBook) {
    assert_eq!(owner.text_snapshot(), "Use coal.\n\nUse coal.");
    assert_eq!(owner.current(&source(0)), Ok(("Use coal.", None)));
    assert_eq!(owner.current(&source(3)), Ok(("Use coal.", None)));
    assert_eq!(owner.original().0.message, original().message);
    assert_eq!(owner.original().0.finish, Finish::Length);
    assert_eq!(owner.original().1, &position());
}

#[test]
fn change_book_initial_text_keeps_indices_empty_blocks_and_original_evidence() {
    let owner = book();
    unchanged(&owner);
    assert_eq!(owner.current(&source(2)), Ok(("", None)));
    assert_eq!(owner.current(&source(1)), Err(ChangeError::WrongSource));
    assert_eq!(owner.current(&source(4)), Err(ChangeError::WrongSource));
}

#[test]
fn change_book_invalid_original_returns_the_exact_message() {
    for case in 0..7 {
        let mut original = original();
        let mut source = position();
        match case {
            0 => source.block_id.run.clear(),
            1 => source.block_id.block = 1.into(),
            2 => original.turn.0.clear(),
            3 => source.turn = None,
            4 => source.turn = Some("foreign".into()),
            5 => original.message.role = Role::User,
            6 => original
                .message
                .content
                .push(ContentBlock::ToolResult(ToolResultBlock {
                    content: "not model output".into(),
                })),
            _ => unreachable!(),
        }
        let message = original.message.clone();
        let turn = original.turn.clone();
        let Err(returned) = ChangeBook::new(original, source) else {
            panic!("invalid original accepted in case {case}");
        };
        assert_eq!(returned.message, message);
        assert_eq!(returned.turn, turn);
        assert_eq!(returned.finish, Finish::Length);
    }
}

#[test]
fn change_book_matching_empty_turns_do_not_supply_attribution() {
    let mut original = original();
    original.turn.0.clear();
    let mut source = position();
    source.turn = Some(String::new());
    let Err(returned) = ChangeBook::new(original, source) else {
        panic!("matching empty turns accepted");
    };
    assert!(returned.turn.0.is_empty());
    assert_eq!(returned.message, self::original().message);
    assert_eq!(returned.finish, Finish::Length);
}

#[test]
fn change_book_only_recorded_applies_once_and_pressure_keeps_the_exact_record() {
    let mut owner = book();
    let record = change(0);
    let reservation = owner.reserve(record.clone(), None).unwrap();
    let address = owner.pending_record(&reservation).unwrap() as *const _;
    for _ in 0..3 {
        assert_eq!(owner.pending_record(&reservation), Ok(&record));
        assert_eq!(
            owner.pending_record(&reservation).unwrap() as *const _,
            address
        );
        unchanged(&owner);
    }
    assert_eq!(
        owner.recorded(&reservation, Sequence(20)),
        Err(ChangeError::NotAdmitted)
    );
    unchanged(&owner);
    owner.admitted(&reservation).unwrap();
    assert_eq!(
        owner.admitted(&reservation),
        Err(ChangeError::AlreadyAdmitted)
    );
    assert_eq!(owner.pending_record(&reservation), Ok(&record));
    unchanged(&owner);
    assert_eq!(
        owner.recorded(&reservation, Sequence(20)),
        Ok(RecordedChange::Applied)
    );
    assert_eq!(
        owner.current(&source(0)),
        Ok(("Use fuel.", Some(20.into())))
    );
    assert_eq!(owner.text_snapshot(), "Use fuel.\n\nUse coal.");
    assert_eq!(owner.original().0.message, original().message);
    assert_eq!(owner.original().0.finish, Finish::Length);
    assert_eq!(owner.original().1, &position());
    assert_eq!(
        owner.recorded(&reservation, Sequence(21)),
        Err(ChangeError::UnknownReservation)
    );
    assert_eq!(
        owner.current(&source(0)),
        Ok(("Use fuel.", Some(20.into())))
    );
}

#[test]
fn change_book_length_changes_use_current_offsets_and_current_heads() {
    let mut original = original();
    original.message.content[0] = ContentBlock::Text {
        text: "Use coal. Keep it.".into(),
    };
    let mut owner = ChangeBook::new(original, position()).unwrap();
    let mut first = change(0);
    first.replacement = "liquid fuel".into();
    let reservation = owner.reserve(first, None).unwrap();
    owner.admitted(&reservation).unwrap();
    owner.recorded(&reservation, Sequence(20)).unwrap();
    assert_eq!(
        owner.current(&source(0)),
        Ok(("Use liquid fuel. Keep it.", Some(20.into())))
    );
    assert_eq!(
        owner.reserve(change(0), None).unwrap_err(),
        ChangeError::StaleSource
    );

    let text = owner.current(&source(0)).unwrap().0;
    let start = text.find("Keep").unwrap();
    let mut next = change(0);
    next.matched.previous_change = Some(20.into());
    next.matched.sentence_span = span(start, text.len());
    next.matched.word_span = span(start, start + 4);
    next.span = next.matched.word_span;
    next.replaced = "Keep".into();
    next.replacement = "Hold".into();
    let mut old_offsets = next.clone();
    old_offsets.span = span(10, 14);
    old_offsets.matched.word_span = old_offsets.span;
    old_offsets.matched.sentence_span = span(10, 17);
    assert_eq!(
        owner.reserve(old_offsets, None).unwrap_err(),
        ChangeError::InvalidText
    );
    let reservation = owner.reserve(next.clone(), None).unwrap();
    owner.admitted(&reservation).unwrap();
    owner.recorded(&reservation, Sequence(22)).unwrap();
    assert_eq!(
        owner.current(&source(0)),
        Ok(("Use liquid fuel. Hold it.", Some(22.into())))
    );
    assert_eq!(
        owner.reserve(next, None).unwrap_err(),
        ChangeError::StaleSource
    );
}

#[test]
fn change_book_pending_proposals_cannot_branch_or_overwrite_the_same_source() {
    let mut owner = book();
    let first = owner.reserve(change(0), None).unwrap();
    let mut replacement = change(0);
    replacement.replacement = "wood".into();
    assert_eq!(
        owner.reserve(replacement, None).unwrap_err(),
        ChangeError::PendingChange
    );
    assert_eq!(owner.pending_record(&first), Ok(&change(0)));
    let other = owner.reserve(change(3), None).unwrap();
    owner.admitted(&other).unwrap();
    owner.recorded(&other, Sequence(20)).unwrap();
    assert_eq!(owner.current(&source(0)), Ok(("Use coal.", None)));
    assert_eq!(
        owner.current(&source(3)),
        Ok(("Use fuel.", Some(20.into())))
    );
    owner.admitted(&first).unwrap();
    owner.recorded(&first, Sequence(21)).unwrap();
    assert_eq!(
        owner.current(&source(0)),
        Ok(("Use fuel.", Some(21.into())))
    );
    assert_eq!(
        owner.current(&source(3)),
        Ok(("Use fuel.", Some(20.into())))
    );
}

#[test]
fn change_book_wrong_sources_cannot_reserve_or_read_another_block() {
    for case in 0..4 {
        let mut owner = book();
        let mut record = change(0);
        match case {
            0 => record.matched.source.run = "foreign".into(),
            1 => record.matched.source.sequence = 9.into(),
            2 => record.matched.source.block = 1.into(),
            3 => record.matched.source.block = u64::MAX.into(),
            _ => unreachable!(),
        }
        assert_eq!(
            owner.current(&record.matched.source),
            Err(ChangeError::WrongSource)
        );
        assert_eq!(
            owner.reserve(record, None).unwrap_err(),
            ChangeError::WrongSource
        );
        unchanged(&owner);
        let _next = owner.reserve(change(0), None).unwrap();
    }
}

#[test]
fn change_book_rejects_wrong_bytes_and_requires_utf8_boundaries() {
    for case in 0..4 {
        let mut owner = book();
        let mut record = change(0);
        match case {
            0 => record.replaced = "wood".into(),
            1 => record.replacement.clear(),
            2 => record.span = span(4, 100),
            3 => record.matched.word_span = span(0, 0),
            _ => unreachable!(),
        }
        assert_eq!(
            owner.reserve(record, None).unwrap_err(),
            ChangeError::InvalidText
        );
        unchanged(&owner);
    }
    let mut original = original();
    original.message.content[0] = ContentBlock::Text {
        text: "Use caf\u{e9}.".into(),
    };
    let mut owner = ChangeBook::new(original, position()).unwrap();
    let mut record = change(0);
    record.matched.sentence_span = span(0, 10);
    record.replaced = "caf\u{e9}".into();
    assert_eq!(
        owner.reserve(record.clone(), None).unwrap_err(),
        ChangeError::InvalidText
    );
    record.span = span(4, 9);
    record.matched.word_span = record.span;
    let reservation = owner.reserve(record, None).unwrap();
    owner.admitted(&reservation).unwrap();
    owner.recorded(&reservation, Sequence(20)).unwrap();
    assert_eq!(
        owner.current(&source(0)),
        Ok(("Use fuel.", Some(20.into())))
    );
}

#[test]
fn change_book_opaque_reservations_cannot_cross_owners() {
    let mut owner = book();
    let reservation = owner.reserve(change(0), None).unwrap();
    let mut other = book();
    let other_reservation = other.reserve(change(0), None).unwrap();
    other.admitted(&other_reservation).unwrap();
    assert_eq!(
        other.pending_record(&reservation),
        Err(ChangeError::UnknownReservation)
    );
    assert_eq!(
        other.admitted(&reservation),
        Err(ChangeError::UnknownReservation)
    );
    assert_eq!(
        other.recorded(&reservation, Sequence(20)),
        Err(ChangeError::UnknownReservation)
    );
    assert_eq!(
        other.cancel(&reservation),
        Err(ChangeError::UnknownReservation)
    );
    unchanged(&other);
    assert_eq!(other.pending_record(&other_reservation), Ok(&change(0)));
}

#[test]
fn change_book_unadmitted_refusal_releases_without_changing_y() {
    let mut owner = book();
    let reservation = owner.reserve(change(0), None).unwrap();
    assert_eq!(owner.cancel(&reservation), Ok(Cancellation::Released));
    assert_eq!(
        owner.pending_record(&reservation),
        Err(ChangeError::UnknownReservation)
    );
    assert_eq!(
        owner.admitted(&reservation),
        Err(ChangeError::UnknownReservation)
    );
    assert_eq!(
        owner.recorded(&reservation, Sequence(20)),
        Err(ChangeError::UnknownReservation)
    );
    unchanged(&owner);
    let _next = owner.reserve(change(0), None).unwrap();
}

#[test]
fn change_book_admitted_cancellation_accounts_late_record_without_continuation() {
    let mut owner = book();
    let reservation = owner.reserve(change(0), None).unwrap();
    let unadmitted = owner.reserve(change(3), None).unwrap();
    owner.admitted(&reservation).unwrap();
    assert_eq!(owner.cancel(&reservation), Ok(Cancellation::AwaitingRecord));
    assert_eq!(owner.cancel(&reservation), Ok(Cancellation::AwaitingRecord));
    assert_eq!(owner.pending_record(&reservation), Ok(&change(0)));
    assert_eq!(
        owner.pending_record(&unadmitted),
        Err(ChangeError::UnknownReservation)
    );
    assert_eq!(
        owner.admitted(&reservation),
        Err(ChangeError::AlreadyAdmitted)
    );
    unchanged(&owner);
    assert_eq!(
        owner.reserve(change(3), None).unwrap_err(),
        ChangeError::Stopped
    );
    assert_eq!(
        owner.recorded(&reservation, Sequence(20)),
        Ok(RecordedChange::AccountedAfterCancellation)
    );
    unchanged(&owner);
    assert_eq!(
        owner.recorded(&reservation, Sequence(21)),
        Err(ChangeError::UnknownReservation)
    );
    assert_eq!(
        owner.reserve(change(0), None).unwrap_err(),
        ChangeError::Stopped
    );
}

#[test]
fn change_book_turn_cancellation_retains_only_admitted_records() {
    let mut owner = book();
    let admitted = owner.reserve(change(0), None).unwrap();
    let unadmitted = owner.reserve(change(3), None).unwrap();
    owner.admitted(&admitted).unwrap();
    owner.cancel_all();
    owner.cancel_all();
    assert_eq!(owner.pending_record(&admitted), Ok(&change(0)));
    assert_eq!(
        owner.pending_record(&unadmitted),
        Err(ChangeError::UnknownReservation)
    );
    assert_eq!(
        owner.recorded(&unadmitted, Sequence(20)),
        Err(ChangeError::UnknownReservation)
    );
    assert_eq!(
        owner.recorded(&admitted, Sequence(20)),
        Ok(RecordedChange::AccountedAfterCancellation)
    );
    unchanged(&owner);
    assert_eq!(
        owner.reserve(change(3), None).unwrap_err(),
        ChangeError::Stopped
    );
}

#[test]
fn change_book_record_sequences_must_advance_across_blocks() {
    let mut owner = book();
    let first = owner.reserve(change(0), None).unwrap();
    owner.admitted(&first).unwrap();
    for sequence in [0, 9, 10] {
        assert_eq!(
            owner.recorded(&first, Sequence(sequence)),
            Err(ChangeError::InvalidSequence)
        );
        unchanged(&owner);
        assert_eq!(owner.pending_record(&first), Ok(&change(0)));
    }
    owner.recorded(&first, Sequence(20)).unwrap();
    let second = owner.reserve(change(3), None).unwrap();
    owner.admitted(&second).unwrap();
    for sequence in [11, 19, 20] {
        assert_eq!(
            owner.recorded(&second, Sequence(sequence)),
            Err(ChangeError::InvalidSequence)
        );
        assert_eq!(owner.current(&source(3)), Ok(("Use coal.", None)));
    }
    owner.recorded(&second, Sequence(u64::MAX)).unwrap();
    assert_eq!(
        owner.current(&source(3)),
        Ok(("Use fuel.", Some(u64::MAX.into())))
    );
}

#[test]
fn change_book_offer_copies_only_selected_response_bytes_into_the_original() {
    let mut owner = book();
    let mut record = change(0);
    record.span = span(0, 9);
    record.replaced = "Use coal.".into();
    record.replacement = "Use fuel.".into();
    let mut instruction = source(0);
    instruction.sequence = 11.into();
    let mut reemission = source(3);
    reemission.sequence = 12.into();
    record.method = EmissionChangeMethod::Offer {
        instruction,
        reemission,
        reemission_span: span(2, 11),
    };
    assert_eq!(
        owner.reserve(record.clone(), None).unwrap_err(),
        ChangeError::InvalidText
    );
    assert_eq!(
        owner
            .reserve(record.clone(), Some("  Use wood. tail"))
            .unwrap_err(),
        ChangeError::InvalidText
    );
    let reservation = owner.reserve(record, Some("  Use fuel. tail")).unwrap();
    unchanged(&owner);
    owner.admitted(&reservation).unwrap();
    owner.recorded(&reservation, Sequence(13)).unwrap();
    assert_eq!(owner.text_snapshot(), "Use fuel.\n\nUse coal.");
    assert_eq!(owner.original().0.message, original().message);
    assert_eq!(owner.original().0.finish, Finish::Length);
    assert_eq!(owner.original().1, &position());
}
