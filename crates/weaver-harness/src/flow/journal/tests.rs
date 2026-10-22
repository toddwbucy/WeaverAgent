use super::*;
use std::cmp::Ordering;
use weaver_traits::{ContentBlock, ToolCall, ToolResultBlock};
use weaver_types::state_flow::{ContentRef, SourceBlockId, Span};

const _: fn() = || {
    trait NoConstruction<T> {
        fn pin() {}
    }
    impl<T: ?Sized> NoConstruction<()> for T {}
    impl<T: Default> NoConstruction<u8> for T {}
    impl<T: From<u64>> NoConstruction<u16> for T {}
    impl<T: Clone> NoConstruction<u32> for T {}
    let _ = <Journal as NoConstruction<_>>::pin;
    let _ = <PrefixProjection as NoConstruction<_>>::pin;
    let _ = <PendingAppend as NoConstruction<_>>::pin;
};

fn journal() -> Journal {
    Journal {
        identity: Arc::new(()),
        origin: Arc::new(()),
        run: "run".into(),
        revision: 0,
        next_message: 1,
        next_assembly: 1,
        next_mark: 1,
        prefix: 10,
        resident: 10,
        capacity: 1000,
        layout: vec![],
        boundaries: vec![],
        pairs: vec![],
        marks: vec![],
        pending: None,
        rebuild_pending: false,
    }
}
fn text(value: &str) -> Message {
    Message {
        role: Role::Assistant,
        content: vec![ContentBlock::Text { text: value.into() }],
    }
}
fn cursor(id: u64, block: u64, byte: Option<u64>) -> ArgumentCursor {
    ArgumentCursor::Message {
        message_id: id.into(),
        block_end: block.into(),
        byte_end: byte.map(Into::into),
    }
}
fn assistant(journal: &mut Journal, message: Message, resident: u64) -> DecimalU64 {
    let prefix = journal.project(None).unwrap();
    let pending = journal.reserve(prefix, &[]).unwrap();
    let id = pending.assistant_id;
    journal
        .complete(
            &pending,
            ConfirmedAppend {
                assembly: pending.assembly,
                assistant: message,
                resident,
                capacity: 1000,
            },
        )
        .unwrap();
    id
}
fn cut(journal: &mut Journal, end: ArgumentCursor) {
    let prefix = journal.project(Some(&end)).unwrap();
    journal
        .cut(
            &prefix,
            ConfirmedCut {
                before: journal.resident,
                after: prefix.n,
            },
        )
        .unwrap();
}
fn source(sequence: u64) -> ContentRef {
    ContentRef::Event {
        block_id: SourceBlockId {
            run: "source-run".into(),
            sequence: sequence.into(),
            block: 0.into(),
        },
    }
}
fn span(start: u64, end: u64) -> Span {
    Span {
        start: start.into(),
        end: end.into(),
    }
}
fn mapped(id: u64, sequence: u64, local_start: u64, local_end: u64, absolute: u64) -> PlacedSource {
    PlacedSource {
        material: MaterialRef::Text {
            source: source(sequence),
            span: span(absolute, absolute + local_end - local_start),
        },
        message_id: id.into(),
        block: 0.into(),
        start: Some(local_start.into()),
        end: Some(local_end.into()),
    }
}
fn mark_span() -> SpanRef {
    SpanRef {
        seed_id: "0123456789abcdef0123456789abcdef".into(),
        local_id: 2.into(),
        revision: 1.into(),
    }
}

#[test]
fn journal_cursors_use_layout_order_and_utf8_without_token_offsets() {
    let mut j = journal();
    assistant(&mut j, text("a\u{e9}z"), 40);
    assistant(&mut j, text("later"), 80);
    // IDs are identities. Deliberately reverse their numerical order in this
    // private layout fixture to make a numerical comparison fail the assertion.
    j.layout[0].id = 90.into();
    j.layout[1].id = 2.into();
    assert_eq!(
        layout::compare(&j.layout, &cursor(90, 1, Some(3)), &cursor(2, 1, None)),
        Ok(Ordering::Less)
    );
    assert_eq!(
        layout::compare(&j.layout, &cursor(90, 1, Some(4)), &cursor(90, 1, None)),
        Ok(Ordering::Equal)
    );
    for invalid in [
        cursor(90, 0, None),
        cursor(90, 2, None),
        cursor(90, 1, Some(2)),
        cursor(90, 1, Some(5)),
        cursor(999, 1, None),
    ] {
        assert_eq!(
            layout::position(&j.layout, &invalid),
            Err(JournalError::InvalidCursor)
        );
    }
    let kept = layout::prefix(&j.layout, &cursor(90, 1, Some(3))).unwrap();
    assert_eq!(kept[0].message, text("a\u{e9}"));
    assert_eq!(kept[0].id.0, 90);
}

#[test]
fn journal_empty_answers_are_distinct_confirmed_boundaries() {
    let mut j = journal();
    let empty = Message {
        role: Role::Assistant,
        content: vec![],
    };
    assistant(&mut j, empty.clone(), 11);
    assistant(&mut j, text(""), 12);
    assert_eq!(j.layout[0].message, empty);
    assert_eq!(j.layout[1].message, text(""));
    assert_eq!(j.boundaries.len(), 2);
    assert_eq!(j.project(Some(&cursor(1, 0, None))).unwrap().n, 11);
    assert_eq!(
        layout::position(&j.layout, &cursor(2, 0, None)),
        Err(JournalError::InvalidCursor)
    );
    assert_eq!(
        layout::compare(&j.layout, &cursor(2, 1, Some(0)), &cursor(2, 1, None)),
        Ok(Ordering::Equal)
    );
    cut(&mut j, cursor(1, 0, None));
    assert_eq!(j.layout.len(), 1);
    assert_eq!(j.resident, 11);
}

#[test]
fn journal_rejects_byte_cursors_on_whole_call_return_and_unrecovered_blocks() {
    let mut j = journal();
    let message = Message {
        role: Role::Assistant,
        content: vec![
            ContentBlock::ToolCall(ToolCall {
                name: "x".into(),
                arguments: "{}".into(),
            }),
            ContentBlock::UnrecoveredCall {
                fragment: "broken".into(),
            },
        ],
    };
    assistant(&mut j, message.clone(), 30);
    assert_eq!(j.layout[0].message, message);
    for block in 1..=2 {
        assert_eq!(
            layout::position(&j.layout, &cursor(1, block, Some(0))),
            Err(JournalError::InvalidCursor)
        );
        assert!(layout::position(&j.layout, &cursor(1, block, None)).is_ok());
    }
    // A placed result is a different role; only the cursor rule is under test.
    j.layout.push(RetainedMessage {
        id: 2.into(),
        message: Message {
            role: Role::ToolResult,
            content: vec![ContentBlock::ToolResult(ToolResultBlock {
                content: "done".into(),
            })],
        },
        sources: vec![],
        material: None,
        occurrence: None,
        step_queries: vec![],
    });
    assert_eq!(
        layout::position(&j.layout, &cursor(2, 1, Some(0))),
        Err(JournalError::InvalidCursor)
    );
    assert!(layout::position(&j.layout, &cursor(2, 1, None)).is_ok());
}

#[test]
fn journal_cancel_burns_ids_but_installs_nothing_and_rejects_duplicate_completion() {
    let mut j = journal();
    let before = j.project(None).unwrap();
    let pending = j.reserve(j.project(None).unwrap(), &[]).unwrap();
    assert_eq!(pending.assistant_id.0, 1);
    assert_eq!(
        j.reserve(j.project(None).unwrap(), &[]).unwrap_err(),
        JournalError::Busy
    );
    j.cancel(&pending).unwrap();
    assert!(j.layout.is_empty() && j.boundaries.is_empty());
    assert_eq!(j.revision, before.revision);
    assert_eq!(j.cancel(&pending), Err(JournalError::Stale));
    let id = assistant(&mut j, text("answer"), 30);
    assert_eq!(id.0, 2);
    assert_eq!(j.next_assembly, 3);
    assert_eq!(
        j.complete(
            &pending,
            ConfirmedAppend {
                assembly: pending.assembly,
                assistant: text("late"),
                resident: 40,
                capacity: 1000
            }
        ),
        Err(JournalError::Stale)
    );
}

#[test]
fn journal_overflow_and_bad_confirmation_leave_pending_state_unchanged() {
    let mut j = journal();
    j.next_message = u64::MAX;
    assert_eq!(
        j.reserve(j.project(None).unwrap(), &[]).unwrap_err(),
        JournalError::Exhausted
    );
    assert_eq!(j.next_assembly, 1);
    assert!(j.pending.is_none());
    j.next_message = 1;
    j.next_assembly = u64::MAX;
    assert_eq!(
        j.reserve(j.project(None).unwrap(), &[]).unwrap_err(),
        JournalError::Exhausted
    );
    assert_eq!(j.next_message, 1);
    j.next_assembly = 1;
    let pending = j.reserve(j.project(None).unwrap(), &[]).unwrap();
    for (assembly, resident, capacity) in
        [(99, 20, 1000), (1, 10, 1000), (1, 1001, 1000), (1, 20, 99)]
    {
        assert_eq!(
            j.complete(
                &pending,
                ConfirmedAppend {
                    assembly,
                    assistant: text("kept"),
                    resident,
                    capacity
                }
            ),
            Err(JournalError::InvalidFacts)
        );
        assert!(j.layout.is_empty());
        assert_eq!(j.pending, Some(1));
    }
    j.revision = u64::MAX;
    let mut pending = pending;
    pending.revision = u64::MAX;
    pending.prefix.revision = u64::MAX;
    assert_eq!(
        j.complete(
            &pending,
            ConfirmedAppend {
                assembly: 1,
                assistant: text("kept"),
                resident: 20,
                capacity: 1000
            }
        ),
        Err(JournalError::Exhausted)
    );
    assert!(j.layout.is_empty());
    assert_eq!(j.pending, Some(1));
}

#[test]
fn journal_conservative_n_and_successive_cuts_never_reuse_removed_boundaries() {
    let mut j = journal();
    assistant(&mut j, text("first"), 30);
    assistant(&mut j, text("second"), 70);
    assistant(&mut j, text("third"), 100);
    let prefix = j.project(Some(&cursor(2, 1, Some(3)))).unwrap();
    assert_eq!(prefix.n, 30);
    assert_eq!(prefix.rebuild.len(), 1);
    assert_eq!(prefix.rebuild[0].id.0, 2);
    assert_eq!(prefix.rebuild[0].message, text("sec"));
    assert_eq!(j.layout[1].message, text("second"));
    j.cut(
        &prefix,
        ConfirmedCut {
            before: 100,
            after: 30,
        },
    )
    .unwrap();
    assert_eq!(j.boundaries.len(), 1);
    assert_eq!(j.project(None).unwrap().n, 30);
    cut(&mut j, cursor(2, 1, Some(2)));
    assert_eq!(j.project(None).unwrap().rebuild[0].message, text("se"));
    assert_eq!(j.project(None).unwrap().n, 30);
    assistant(&mut j, text("fresh"), 60);
    assert_eq!(
        j.layout.iter().map(|m| m.id.0).collect::<Vec<_>>(),
        vec![1, 2, 4]
    );
    assert_eq!(j.layout[1].message, text("se"));
    assert!(!j.rebuild_pending);
    assert!(j.project(None).unwrap().rebuild.is_empty());
    let prefix = j.project(Some(&cursor(1, 1, Some(2)))).unwrap();
    assert_eq!(prefix.n, 10);
    assert_eq!(prefix.rebuild[0].message, text("fi"));
}

#[test]
fn journal_projection_clips_absolute_coverage_but_preserves_source_identity_and_repeats() {
    let mut j = journal();
    assistant(&mut j, text("a\u{e9}z"), 30);
    let placed = mapped(1, 7, 0, 4, 100);
    j.annotate(ConfirmedSources {
        sources: vec![placed.clone()],
    })
    .unwrap();
    // A repeated location in another canonical occurrence remains separate.
    assistant(&mut j, text("a\u{e9}z"), 60);
    let mut repeated = placed.clone();
    repeated.message_id = 2.into();
    j.annotate(ConfirmedSources {
        sources: vec![repeated],
    })
    .unwrap();
    assert_eq!(j.project(None).unwrap().coverage.len(), 1);
    assert_eq!(j.layout.iter().flat_map(|m| &m.sources).count(), 2);
    let prefix = j.project(Some(&cursor(2, 1, Some(3)))).unwrap();
    assert_eq!(
        prefix.kept[1].sources[0].material,
        MaterialRef::Text {
            source: source(7),
            span: span(100, 103)
        }
    );
    assert_eq!(prefix.kept[1].sources[0].end, Some(3.into()));
    assert_eq!(prefix.coverage.len(), 2);
    assert!(layout::covers(
        &prefix.coverage,
        &MaterialRef::Text {
            source: source(7),
            span: span(101, 104)
        }
    ));
    assert!(!layout::covers(
        &prefix.coverage,
        &MaterialRef::Text {
            source: source(8),
            span: span(101, 104)
        }
    ));
    let first = j.project(Some(&cursor(1, 1, Some(3)))).unwrap();
    assert!(!layout::covers(
        &first.coverage,
        &MaterialRef::Text {
            source: source(7),
            span: span(100, 104)
        }
    ));
    assert_eq!(j.layout[0].sources[0], placed);
}

#[test]
fn journal_revisions_invalidate_projections_and_foreign_owners_cannot_substitute() {
    let mut j = journal();
    let stale = j.project(None).unwrap();
    assistant(&mut j, text("answer"), 30);
    assert_eq!(j.check_projection(&stale), Err(JournalError::Stale));
    let stale = j.project(None).unwrap();
    j.annotate(ConfirmedSources {
        sources: vec![mapped(1, 1, 0, 6, 0)],
    })
    .unwrap();
    assert_eq!(j.check_projection(&stale), Err(JournalError::Stale));
    let stale = j.project(None).unwrap();
    assert_eq!(
        j.mark(ConfirmedMark {
            span: mark_span(),
            end: cursor(1, 1, Some(3))
        }),
        Ok(1)
    );
    assert_eq!(j.check_projection(&stale), Err(JournalError::Stale));
    let stale = j.project(None).unwrap();
    cut(&mut j, cursor(1, 1, Some(3)));
    assert_eq!(j.check_projection(&stale), Err(JournalError::Stale));
    let other = journal().project(None).unwrap();
    assert_eq!(j.check_projection(&other), Err(JournalError::Stale));
    assert_eq!(j.marks[0].ordinal, 1);
    assert_eq!(j.marks[0].span, mark_span());
    cut(&mut j, ArgumentCursor::Head {});
    assert!(j.marks.is_empty());
    assert_eq!(j.next_mark, 2);
}

#[test]
fn journal_bad_annotations_cuts_and_mark_overflow_are_atomic() {
    let mut j = journal();
    assistant(&mut j, text("a\u{e9}z"), 30);
    let revision = j.revision;
    assert_eq!(
        j.annotate(ConfirmedSources {
            sources: vec![mapped(1, 1, 0, 1, 0), mapped(1, 2, 0, 2, 0)]
        }),
        Err(JournalError::InvalidMap)
    );
    assert!(j.layout[0].sources.is_empty());
    assert_eq!(j.revision, revision);
    let prefix = j.project(Some(&cursor(1, 1, Some(3)))).unwrap();
    assert_eq!(
        j.cut(
            &prefix,
            ConfirmedCut {
                before: 29,
                after: 10
            }
        ),
        Err(JournalError::InvalidFacts)
    );
    assert_eq!(j.layout[0].message, text("a\u{e9}z"));
    j.next_mark = u64::MAX;
    assert_eq!(
        j.mark(ConfirmedMark {
            span: mark_span(),
            end: cursor(1, 1, None)
        }),
        Err(JournalError::Exhausted)
    );
    assert!(j.marks.is_empty());
    assert_eq!(j.revision, revision);
    j.prefix = 31;
    assert_eq!(j.project(None).unwrap_err(), JournalError::InvalidFacts);
}

// Invoked by a real paged-read/recorder fixture in context_material_tests.
// Initialization here is test-only and cannot stand a production journal.
pub(super) fn exercise_material(units: Vec<SelectedUnit>, foreign: SelectedUnit) {
    let mut j = journal();
    let unit = Arc::new(units.into_iter().next().unwrap());
    j.origin = unit.origin().clone();
    let original = unit.messages().to_vec();
    let original_maps = unit.source_map().to_vec();
    let provenance = unit.reads()[0].raw_bytes().to_vec();
    let read_sequence = unit.reads()[0].recall_sequence();
    let foreign = Arc::new(foreign);
    assert_eq!(
        j.reserve(j.project(None).unwrap(), &[foreign]).unwrap_err(),
        JournalError::ForeignMaterial
    );
    assert_eq!(j.next_message, 1);
    let pending = j
        .reserve(j.project(None).unwrap(), &[unit.clone(), unit.clone()])
        .unwrap();
    assert_eq!(pending.new_messages.len(), original.len() * 2);
    assert_eq!(pending.new_pairs.len(), 2);
    assert_eq!(
        pending.new_messages[0]
            .occurrence
            .as_ref()
            .unwrap()
            .unit_index
            .0,
        0
    );
    assert_eq!(
        pending.new_messages[original.len()]
            .occurrence
            .as_ref()
            .unwrap()
            .unit_index
            .0,
        1
    );
    assert!(j.layout.is_empty() && j.boundaries.is_empty());
    j.complete(
        &pending,
        ConfirmedAppend {
            assembly: pending.assembly,
            assistant: text(""),
            resident: 100,
            capacity: 1000,
        },
    )
    .unwrap();
    assert_eq!(
        j.layout[..original.len()]
            .iter()
            .map(|m| m.message.clone())
            .collect::<Vec<_>>(),
        original
    );
    assert_eq!(j.project(None).unwrap().coverage, unit.sources());
    assert_eq!(
        j.layout.iter().flat_map(|m| &m.sources).count(),
        original_maps.len() * 2
    );
    for message in &j.layout[..original.len() * 2] {
        let proof = message.material.as_ref().unwrap();
        assert!(Arc::ptr_eq(proof, &unit));
        assert_eq!(proof.reads()[0].raw_bytes(), provenance);
        assert_eq!(proof.reads()[0].recall_sequence(), read_sequence);
    }
    for pair in &j.pairs {
        let end = cursor(
            pair.request.message_id.0,
            pair.request.block.0 + 1,
            pair.request.end.map(|n| n.0),
        );
        assert_eq!(
            j.project(Some(&end)).unwrap_err(),
            JournalError::InvalidPair
        );
    }
    // Earlier return coverage with the same CallId must not license keeping the
    // later request while removing that later occurrence's return.
    let second = j.pairs[1].clone();
    assert_eq!(
        j.project(Some(&cursor(second.request.message_id.0, 1, None)))
            .unwrap_err(),
        JournalError::InvalidPair
    );
    let first_end = cursor(j.pairs[0].returned.message_id.0, 1, None);
    let projected = j.project(Some(&first_end)).unwrap();
    assert_eq!(projected.n, 10);
    assert!(projected.rebuild.iter().all(|m| {
        m.material
            .as_ref()
            .is_some_and(|held| Arc::ptr_eq(held, &unit))
    }));
    j.cut(
        &projected,
        ConfirmedCut {
            before: 100,
            after: 10,
        },
    )
    .unwrap();
    assert_eq!(j.pairs.len(), 1);
    assert_eq!(unit.source_map(), original_maps);
    assert_eq!(unit.messages(), original);
}

// Real read-backed material enters here from context_material_tests. Corrupted
// inspection copies below exercise only the pure map check, never mint a unit.
pub(super) fn exercise_material_map_order(unit: SelectedUnit) {
    let unit = Arc::new(unit);
    let maps = unit.source_map().to_vec();
    let sources = unit.sources().to_vec();
    let messages = unit.messages().to_vec();
    let raw = unit.reads()[0].raw_bytes().to_vec();
    let recall = unit.reads()[0].recall_sequence();
    assert_eq!(sources.len(), 5);
    layout::validate_coverage(&maps, &sources).unwrap();
    let mut repeated_location = maps.clone();
    repeated_location.push(maps[0].clone());
    layout::validate_coverage(&repeated_location, &sources).unwrap();
    for fault in 0..4 {
        let mut declared = sources.clone();
        match fault {
            0 => {
                declared.remove(2);
            }
            1 => declared.swap(1, 3),
            2 => declared.push(sources[0].clone()),
            3 => declared.push(MaterialRef::Text {
                source: source(999),
                span: span(0, 1),
            }),
            _ => unreachable!(),
        }
        assert_eq!(
            layout::validate_coverage(&maps, &declared),
            Err(JournalError::InvalidMap),
            "coverage fault {fault}"
        );
    }

    let mut j = journal();
    j.origin = unit.origin().clone();
    let pending = j
        .reserve(j.project(None).unwrap(), std::slice::from_ref(&unit))
        .unwrap();
    assert_eq!(pending.new_pairs.len(), 2);
    // The counterexample is real: message walking relocates the later recall
    // request before earlier pair returns. That order is not MESSAGE-MAP's.
    assert_ne!(layout::coverage(&pending.new_messages), sources);
    assert_eq!(layout::coverage(&pending.new_messages).len(), sources.len());
    assert_eq!(
        pending
            .new_messages
            .iter()
            .map(|m| m.sources.len())
            .sum::<usize>(),
        maps.len()
    );
    for mapping in &maps {
        let placed = &pending.new_messages[mapping.message.0 as usize];
        assert!(placed.sources.contains(&PlacedSource {
            material: mapping.material.clone(),
            message_id: placed.id,
            block: mapping.block,
            start: mapping.start,
            end: mapping.end,
        }));
    }
    j.complete(
        &pending,
        ConfirmedAppend {
            assembly: pending.assembly,
            assistant: text(""),
            resident: 100,
            capacity: 1000,
        },
    )
    .unwrap();
    assert_eq!(j.pairs.len(), 2);
    assert_eq!(unit.source_map(), maps);
    assert_eq!(unit.sources(), sources);
    for (placed, original) in j.layout.iter().zip(&messages) {
        assert_eq!(&placed.message, original);
        let retained = placed.material.as_ref().unwrap();
        assert!(Arc::ptr_eq(retained, &unit));
        assert_eq!(retained.reads()[0].raw_bytes(), raw);
        assert_eq!(retained.reads()[0].recall_sequence(), recall);
    }
}

#[test]
fn journal_mark_identity_is_exact_and_repeated_reminders_do_not_replace_it() {
    let mut j = journal();
    assistant(&mut j, text("first"), 30);
    let span = mark_span();
    j.mark(ConfirmedMark {
        span: span.clone(),
        end: cursor(1, 1, None),
    })
    .unwrap();
    assistant(&mut j, text("second"), 60);
    j.mark(ConfirmedMark {
        span: span.clone(),
        end: cursor(2, 1, None),
    })
    .unwrap();
    assert_eq!(j.project_mark("run", 1, &span).unwrap().n, 30);
    assert_eq!(j.project_mark("run", 2, &span).unwrap().n, 60);
    assert_eq!(
        j.project_mark("foreign", 1, &span).unwrap_err(),
        JournalError::InvalidCursor
    );
    let mut changed = span.clone();
    changed.local_id = 99.into();
    assert_eq!(
        j.project_mark("run", 1, &changed).unwrap_err(),
        JournalError::InvalidCursor
    );
    cut(&mut j, cursor(1, 1, None));
    assert!(j.project_mark("run", 1, &span).is_ok());
    assert_eq!(
        j.project_mark("run", 2, &span).unwrap_err(),
        JournalError::InvalidCursor
    );
}

#[test]
fn journal_request_text_is_indivisible_and_numeric_counts_never_round_n_forward() {
    let mut j = journal();
    assistant(&mut j, text("Do work. Then wait."), 90);
    let call = CallId {
        run: "run".into(),
        sequence: 1.into(),
        call: 0.into(),
    };
    j.annotate(ConfirmedSources {
        sources: vec![PlacedSource {
            material: MaterialRef::Request {
                call_id: call.clone(),
            },
            message_id: 1.into(),
            block: 0.into(),
            start: Some(0.into()),
            end: Some(8.into()),
        }],
    })
    .unwrap();
    assert_eq!(
        j.project(Some(&cursor(1, 1, Some(4)))).unwrap_err(),
        JournalError::InvalidPair
    );
    let prefix = j.project(Some(&cursor(1, 1, Some(8)))).unwrap();
    assert_eq!(prefix.n, 10);
    assert!(layout::covers(
        &prefix.coverage,
        &MaterialRef::Request {
            call_id: call.clone()
        }
    ));
    assert!(!layout::covers(
        &prefix.coverage,
        &MaterialRef::Returned { call_id: call }
    ));
    assert_eq!(
        j.reserve(prefix, &[]).unwrap_err(),
        JournalError::InvalidFacts
    );
    assert!(j.pending.is_none());
    assert_eq!(j.layout[0].message, text("Do work. Then wait."));
}

#[test]
fn journal_coverage_does_not_merge_partial_sources_or_equal_bytes() {
    let a = MaterialRef::Text {
        source: source(1),
        span: span(0, 4),
    };
    let b = MaterialRef::Text {
        source: source(1),
        span: span(4, 8),
    };
    assert!(!layout::covers(
        &[a.clone(), b],
        &MaterialRef::Text {
            source: source(1),
            span: span(0, 8)
        }
    ));
    assert!(layout::covers(
        std::slice::from_ref(&a),
        &MaterialRef::Text {
            source: source(1),
            span: span(1, 3)
        }
    ));
    assert!(!layout::covers(
        &[a],
        &MaterialRef::Text {
            source: source(2),
            span: span(1, 3)
        }
    ));
}

#[test]
fn journal_equal_facts_do_not_authorize_another_journals_projection() {
    let first = journal();
    let mut second = journal();
    assert!(!Arc::ptr_eq(&first.identity, &second.identity));
    assert_eq!(format!("{first:?}"), format!("{second:?}"));
    let before = format!("{second:?}");
    assert_eq!(
        second
            .reserve(first.project(None).unwrap(), &[])
            .unwrap_err(),
        JournalError::Stale
    );
    assert_eq!(format!("{second:?}"), before);
    let pending = second.reserve(second.project(None).unwrap(), &[]).unwrap();
    second.cancel(&pending).unwrap();
}

#[test]
fn journal_cut_rejects_wrong_confirmed_n_without_changing_any_state() {
    let mut j = journal();
    assistant(&mut j, text("first"), 30);
    assistant(&mut j, text("second"), 70);
    assistant(&mut j, text("third"), 100);
    j.annotate(ConfirmedSources {
        sources: vec![mapped(2, 2, 0, 6, 50)],
    })
    .unwrap();
    j.mark(ConfirmedMark {
        span: mark_span(),
        end: cursor(3, 1, None),
    })
    .unwrap();
    let prefix = j.project(Some(&cursor(2, 1, Some(3)))).unwrap();
    assert_eq!(prefix.n, 30);
    let before = format!("{j:?}");
    assert_eq!(
        j.cut(
            &prefix,
            ConfirmedCut {
                before: 100,
                after: 31
            }
        ),
        Err(JournalError::InvalidFacts)
    );
    assert_eq!(format!("{j:?}"), before);
    j.cut(
        &prefix,
        ConfirmedCut {
            before: 100,
            after: 30,
        },
    )
    .unwrap();
    assert_eq!(j.resident, 30);
    assert_eq!(j.layout.len(), 2);
    assert_eq!(j.layout[1].message, text("sec"));
    assert_eq!(j.boundaries.len(), 1);
    assert!(j.marks.is_empty());
}

#[test]
fn journal_assistant_tool_result_cannot_consume_the_pending_completion() {
    let mut j = journal();
    let pending = j.reserve(j.project(None).unwrap(), &[]).unwrap();
    let before = format!("{j:?}");
    assert_eq!(
        j.complete(
            &pending,
            ConfirmedAppend {
                assembly: pending.assembly,
                assistant: Message {
                    role: Role::Assistant,
                    content: vec![ContentBlock::ToolResult(ToolResultBlock {
                        content: "done".into()
                    })],
                },
                resident: 30,
                capacity: 1000,
            }
        ),
        Err(JournalError::InvalidFacts)
    );
    assert_eq!(format!("{j:?}"), before);
    j.complete(
        &pending,
        ConfirmedAppend {
            assembly: pending.assembly,
            assistant: text("accepted"),
            resident: 30,
            capacity: 1000,
        },
    )
    .unwrap();
    assert!(j.pending.is_none());
    assert_eq!(j.layout[0].message, text("accepted"));
}

#[test]
fn journal_cut_cannot_change_a_matching_active_reservation() {
    let mut j = journal();
    assistant(&mut j, text("first"), 30);
    assistant(&mut j, text("second"), 70);
    let prefix = j.project(Some(&cursor(1, 1, None))).unwrap();
    let pending = j.reserve(j.project(None).unwrap(), &[]).unwrap();
    assert_eq!(j.check_projection(&prefix), Ok(()));
    assert_eq!(prefix.n, 30);
    let before = format!("{j:?}");
    assert_eq!(
        j.cut(
            &prefix,
            ConfirmedCut {
                before: 70,
                after: 30
            }
        ),
        Err(JournalError::Busy)
    );
    assert_eq!(format!("{j:?}"), before);
    j.cancel(&pending).unwrap();
    j.cut(
        &prefix,
        ConfirmedCut {
            before: 70,
            after: 30,
        },
    )
    .unwrap();
    assert_eq!(j.resident, 30);
    assert_eq!(j.layout.len(), 1);
}

#[test]
fn journal_n_uses_the_latest_of_multiple_eligible_completed_boundaries() {
    let mut j = journal();
    assistant(&mut j, text("first"), 30);
    assistant(&mut j, text("second"), 70);
    assistant(&mut j, text("third"), 100);
    let prefix = j.project(Some(&cursor(3, 1, Some(3)))).unwrap();
    assert_eq!(prefix.n, 70);
    assert_eq!(prefix.rebuild.len(), 1);
    assert_eq!(prefix.rebuild[0].id.0, 3);
    assert_eq!(prefix.rebuild[0].message, text("thi"));
    assert_eq!(j.layout[2].message, text("third"));
}

#[test]
fn journal_annotation_requires_equal_local_and_absolute_utf8_byte_lengths() {
    let mut j = journal();
    assistant(&mut j, text("a\u{e9}z"), 30);
    let valid = mapped(1, 1, 0, 1, 50);
    let correct = mapped(1, 2, 0, 3, 100);
    let mut unequal = correct.clone();
    let MaterialRef::Text { span, .. } = &mut unequal.material else {
        unreachable!()
    };
    span.end = 104.into();
    let before = format!("{j:?}");
    assert_eq!(
        j.annotate(ConfirmedSources {
            sources: vec![valid.clone(), unequal]
        }),
        Err(JournalError::InvalidMap)
    );
    assert_eq!(format!("{j:?}"), before);
    j.annotate(ConfirmedSources {
        sources: vec![valid.clone(), correct.clone()],
    })
    .unwrap();
    assert_eq!(j.layout[0].sources, vec![valid, correct]);
}

#[test]
fn journal_equivalent_whole_text_ends_reuse_the_same_physical_boundary() {
    let mut j = journal();
    assistant(&mut j, text("a\u{e9}z"), 30);
    for end in [cursor(1, 1, None), cursor(1, 1, Some(4))] {
        let prefix = j.project(Some(&end)).unwrap();
        assert_eq!(prefix.n, 30);
        assert!(prefix.rebuild.is_empty());
        assert_eq!(prefix.kept[0].message, text("a\u{e9}z"));
    }
}

// Inputs come from actual paged-read/recorder material fixtures. Only the pure
// checks receive changed inspection copies. No copy becomes a SelectedUnit.
pub(super) fn exercise_material_guard(unit: SelectedUnit, guard: MaterialGuard) {
    let unit = Arc::new(unit);
    let mut j = journal();
    j.origin = unit.origin().clone();
    let pending = j
        .reserve(j.project(None).unwrap(), std::slice::from_ref(&unit))
        .unwrap();
    let sources: Vec<_> = pending
        .new_messages
        .iter()
        .flat_map(|m| m.sources.iter().cloned())
        .collect();
    let references: Vec<_> = sources.iter().collect();
    assert_eq!(unit_pair_links(&references).unwrap().len(), 2);
    match guard {
        MaterialGuard::SystemMessage => {
            for message in unit.messages() {
                validate_delta_message(message).unwrap();
            }
            let mut inspected = unit.messages()[0].clone();
            inspected.role = Role::System;
            assert_eq!(
                validate_delta_message(&inspected),
                Err(JournalError::InvalidMap)
            );
        }
        MaterialGuard::MissingRequest => {
            let call = pending.new_pairs[0].call.clone();
            let inspected: Vec<_> = sources.iter().filter(|source|
                !matches!(&source.material, MaterialRef::Request { call_id } if *call_id == call)
            ).collect();
            assert_eq!(
                unit_pair_links(&inspected).unwrap_err(),
                JournalError::InvalidPair
            );
        }
        MaterialGuard::DuplicateReturn => {
            let mut inspected = references;
            inspected.push(&pending.new_pairs[0].returned);
            assert_eq!(
                unit_pair_links(&inspected).unwrap_err(),
                JournalError::InvalidPair
            );
        }
        MaterialGuard::ReturnedAnnotation => {
            j.complete(
                &pending,
                ConfirmedAppend {
                    assembly: pending.assembly,
                    assistant: text(""),
                    resident: 100,
                    capacity: 1000,
                },
            )
            .unwrap();
            let returned = j.pairs[0].returned.clone();
            let message = j
                .layout
                .iter()
                .find(|m| m.id == returned.message_id)
                .unwrap();
            assert!(matches!(
                message.message.content[returned.block.0 as usize],
                ContentBlock::ToolResult(_)
            ));
            layout::validate_source(message, &returned).unwrap();
            assert!(message.sources.contains(&returned));
            let before = format!("{j:?}");
            assert_eq!(
                j.annotate(ConfirmedSources {
                    sources: vec![returned]
                }),
                Err(JournalError::InvalidPair)
            );
            assert_eq!(format!("{j:?}"), before);
        }
    }
}

pub(super) fn exercise_step_query(unit: SelectedUnit) {
    let unit = Arc::new(unit);
    assert_eq!(unit.messages().len(), 1);
    assert_eq!(
        unit.messages()[0].content,
        vec![ContentBlock::Text {
            text: "Write file.".into()
        }]
    );
    let query = unit.step_queries()[0].reference.clone();
    let messages = unit.messages().to_vec();
    let maps = unit.source_map().to_vec();
    let raw = unit.reads()[0].raw_bytes().to_vec();
    let sequence = unit.reads()[0].recall_sequence();
    let mut j = journal();
    j.origin = unit.origin().clone();
    let pending = j
        .reserve(j.project(None).unwrap(), std::slice::from_ref(&unit))
        .unwrap();
    let id = pending.new_messages[0].id.0;
    j.complete(
        &pending,
        ConfirmedAppend {
            assembly: pending.assembly,
            assistant: text(""),
            resident: 100,
            capacity: 1000,
        },
    )
    .unwrap();
    for end in [cursor(id, 1, None), cursor(id, 1, Some(11))] {
        let prefix = j.project(Some(&end)).unwrap();
        assert_eq!(prefix.kept[0].step_queries, vec![query.clone()]);
        assert_eq!(prefix.kept[0].message, messages[0]);
    }
    let prefix = j.project(Some(&cursor(id, 1, Some(5)))).unwrap();
    assert!(prefix.kept[0].step_queries.is_empty());
    let mut shortened = messages[0].clone();
    shortened.content = vec![ContentBlock::Text {
        text: "Write".into(),
    }];
    assert_eq!(prefix.kept[0].message, shortened);
    let MaterialRef::Text {
        span: original_span,
        ..
    } = &maps[0].material
    else {
        unreachable!()
    };
    let mut clipped = maps[0].material.clone();
    let MaterialRef::Text { span, .. } = &mut clipped else {
        unreachable!()
    };
    span.end = (original_span.start.0 + 5).into();
    assert_eq!(prefix.coverage, vec![clipped.clone()]);
    assert_eq!(prefix.kept[0].sources[0].material, clipped);
    assert_eq!(prefix.kept[0].sources[0].end, Some(5.into()));
    assert_eq!(prefix.n, 10);
    j.cut(
        &prefix,
        ConfirmedCut {
            before: 100,
            after: 10,
        },
    )
    .unwrap();
    assert!(j.layout[0].step_queries.is_empty());
    assert_eq!(j.layout[0].message, shortened);
    assert!(Arc::ptr_eq(j.layout[0].material.as_ref().unwrap(), &unit));
    assert_eq!(unit.messages(), messages);
    assert_eq!(unit.source_map(), maps);
    assert_eq!(unit.step_queries()[0].reference, query);
    assert_eq!(unit.reads()[0].raw_bytes(), raw);
    assert_eq!(unit.reads()[0].recall_sequence(), sequence);
}
