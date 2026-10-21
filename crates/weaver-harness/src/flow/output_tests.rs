use super::*;
use weaver_types::state_flow::{Binding, NounBinding, Span, Ste100ComponentCode, Ste100DefectCode};

// Integration tests obtain these capabilities from real recorder completions.
// Unit fixtures below exercise the private owner's checks, not that provenance.
const _: fn() = || {
    trait NotClone<Marker> {
        fn check() {}
    }
    impl<T: ?Sized> NotClone<()> for T {}
    impl<T: Clone> NotClone<u8> for T {}
    let _ = <CheckedGeneration as NotClone<_>>::check;
    let _ = <RecordedOutputBlock as NotClone<_>>::check;
    trait NotDefault<Marker> {
        fn check() {}
    }
    impl<T: ?Sized> NotDefault<()> for T {}
    impl<T: Default> NotDefault<u8> for T {}
    let _ = <CheckedGeneration as NotDefault<_>>::check;
    let _ = <RecordedOutputBlock as NotDefault<_>>::check;
};

fn original() -> OutputOriginal {
    OutputOriginal {
        message: Message {
            role: Role::Assistant,
            content: vec![
                ContentBlock::Text {
                    text: "Read first.".into(),
                },
                ContentBlock::ToolCall(weaver_traits::ToolCall {
                    name: "sh".into(),
                    arguments: "opaque command".into(),
                }),
                ContentBlock::Text {
                    text: "Read second.".into(),
                },
                ContentBlock::UnrecoveredCall {
                    fragment: "opaque fragment".into(),
                },
                ContentBlock::Text {
                    text: "Read third.".into(),
                },
            ],
        },
        finish: Finish::Length,
        turn: TurnRef("turn".into()),
    }
}

fn source(block: u64) -> SourceBlockId {
    SourceBlockId {
        run: "run".into(),
        sequence: 10.into(),
        block: block.into(),
    }
}

fn owner(selected: &[u64]) -> OutputChecks {
    let mut owner = OutputChecks::new(
        "run",
        &crate::flow::classifier::scripted::fixture_scheme(),
        original(),
    )
    .unwrap();
    owner
        .bind(
            Sequence(10),
            100,
            &selected
                .iter()
                .map(|index| source(*index))
                .collect::<Vec<_>>(),
        )
        .unwrap();
    owner
}

fn request(owner: &OutputChecks, block: u64, id: u64) -> Ste100Request {
    let ContentBlock::Text { text } = &owner.original.message.content[block as usize] else {
        panic!("fixture Text block");
    };
    Ste100Request {
        request_id: id.into(),
        target: Ste100Target::Source(source(block)),
        text: text.clone(),
        scheme_id: owner.scheme.scheme_id.clone(),
    }
}

fn accepted(request: &Ste100Request) -> Ste100Reply {
    let len = request.text.len() as u64;
    Ste100Reply {
        request_id: request.request_id,
        target: request.target.clone(),
        scheme_id: request.scheme_id.clone(),
        result: Ste100Result::Accepted {
            bindings: vec![Binding {
                statement_span: Span {
                    start: 0.into(),
                    end: len.into(),
                },
                verb: "read".into(),
                verb_span: Span {
                    start: 0.into(),
                    end: 4.into(),
                },
                condition: None,
                condition_span: None,
                nouns: vec![NounBinding {
                    value: request.text[5..request.text.len() - 1].into(),
                    span: Span {
                        start: 5.into(),
                        end: (len - 1).into(),
                    },
                }],
            }],
        },
    }
}

fn result_source(id: u64) -> SourcePosition {
    SourcePosition {
        block_id: SourceBlockId {
            run: "run".into(),
            sequence: (20 + id).into(),
            block: 0.into(),
        },
        turn: Some("turn".into()),
    }
}

fn record(owner: &mut OutputChecks, block: u64, id: u64, selected: bool, now: u64) {
    let request = request(owner, block, id);
    if !selected {
        owner.submitted(request.clone()).unwrap();
    }
    let reply = accepted(&request);
    owner
        .record(request, reply, result_source(id), now, selected)
        .unwrap();
}

#[test]
fn complete_output_preserves_full_original_indices_and_recorded_checks() {
    let mut owner = owner(&[2]);
    assert_eq!(
        owner
            .pending()
            .map(|source| source.block.0)
            .collect::<Vec<_>>(),
        vec![0, 2, 4]
    );
    record(&mut owner, 2, 1, true, 110);
    assert!(!owner.is_complete());
    let mut owner = owner.complete().unwrap_err();
    record(&mut owner, 0, 2, false, 120);
    assert!(!owner.is_complete());
    record(&mut owner, 4, 3, false, 130);
    let checked = owner.complete().unwrap();
    assert_eq!(checked.original().message, original().message);
    assert_eq!(checked.original().finish, Finish::Length);
    assert_eq!(checked.source().block_id, source(0));
    assert_eq!(checked.recorded_ns(), 130);
    assert_eq!(
        checked
            .blocks()
            .iter()
            .map(|block| {
                let Ste100Target::Source(source) = &block.request().target else {
                    panic!("source")
                };
                (source.block.0, block.selected())
            })
            .collect::<Vec<_>>(),
        vec![(0, false), (2, true), (4, false)]
    );
    assert_eq!(checked.blocks()[0].recorded_ns(), 120);
    assert_eq!(checked.blocks()[0].result_source(), &result_source(2));
}

#[test]
fn bound_source_requires_exact_sparse_text_selection_and_one_binding() {
    for selected in [
        vec![source(1)],
        vec![source(2), source(0)],
        vec![source(2), source(2)],
    ] {
        let mut owner = OutputChecks::new(
            "run",
            &crate::flow::classifier::scripted::fixture_scheme(),
            original(),
        )
        .unwrap();
        assert!(owner.bind(Sequence(10), 100, &selected).is_err());
        assert!(!owner.is_bound());
    }
    let mut owner = owner(&[2]);
    assert!(owner.bind(Sequence(11), 101, &[source(2)]).is_err());
    assert_eq!(owner.source().unwrap().block_id.sequence.0, 10);
}

#[test]
fn recorded_check_refuses_changed_text_scheme_reply_and_selected_disposition() {
    for mutation in 0..6 {
        let mut owner = owner(&[2]);
        let mut request = request(&owner, 2, 1);
        let mut reply = accepted(&request);
        let mut result_source = result_source(1);
        let mut selected = true;
        match mutation {
            0 => request.text = "Read changed.".into(),
            1 => request.scheme_id = "other".into(),
            2 => reply.request_id = 9.into(),
            3 => result_source.turn = Some("foreign".into()),
            4 => selected = false,
            5 => {
                reply.result = Ste100Result::ComponentFailed {
                    code: Ste100ComponentCode::NoResult,
                    detail: "fixture fault".into(),
                }
            }
            _ => unreachable!(),
        }
        assert!(
            owner
                .record(request, reply, result_source, 110, selected)
                .is_err()
        );
        assert_eq!(owner.pending().count(), 3);
    }
}

#[test]
fn selected_result_cannot_be_observed_twice() {
    let mut owner = owner(&[2]);
    record(&mut owner, 2, 1, true, 110);
    let request = request(&owner, 2, 1);
    let reply = accepted(&request);
    assert!(
        owner
            .record(request, reply, result_source(1), 110, true)
            .is_err()
    );
    assert_eq!(owner.pending().count(), 2);
}

#[test]
fn unselected_result_requires_exact_submitted_request() {
    let mut owner = owner(&[2]);
    let request = request(&owner, 0, 1);
    let reply = accepted(&request);
    assert!(
        owner
            .record(request.clone(), reply.clone(), result_source(1), 110, false)
            .is_err()
    );
    owner.submitted(request.clone()).unwrap();
    assert!(owner.next_unselected().is_none());
    owner
        .record(request, reply, result_source(1), 110, false)
        .unwrap();
    assert_eq!(owner.next_unselected().unwrap().0.block.0, 4);
}

#[test]
fn cancelled_output_retains_submitted_check_until_recorded_without_grant() {
    let mut owner = owner(&[2]);
    let request = request(&owner, 0, 1);
    let reply = accepted(&request);
    owner.submitted(request.clone()).unwrap();
    owner.cancel();
    assert!(!owner.is_settled());
    assert!(owner.next_unselected().is_none());
    let mut owner = owner.cancellation().unwrap_err();
    owner
        .record(request, reply, result_source(1), 110, false)
        .unwrap();
    assert!(owner.is_settled());
    assert!(!owner.is_complete());
    let owner = owner.complete().unwrap_err();
    let cancelled = owner.cancellation().unwrap();
    assert_eq!(cancelled.source.unwrap().block_id, source(0));
    assert_eq!(cancelled.recorded.len(), 1);
    assert_eq!(cancelled.original.message, original().message);
}

#[test]
fn correct_defect_completes_check_without_becoming_component_failure() {
    let mut owner = owner(&[0, 2, 4]);
    record(&mut owner, 0, 1, true, 110);
    let request = request(&owner, 2, 2);
    let mut reply = accepted(&request);
    reply.result = Ste100Result::NotSte100 {
        code: Ste100DefectCode::NonSte100,
        detail: "fixture defect".into(),
        spans: Vec::new(),
    };
    owner
        .record(request, reply, result_source(2), 120, true)
        .unwrap();
    record(&mut owner, 4, 3, true, 130);
    let checked = owner.complete().unwrap();
    assert!(matches!(
        checked.blocks()[1].reply().result,
        Ste100Result::NotSte100 { .. }
    ));
    assert_eq!(checked.blocks().len(), 3);
}

#[test]
fn cancelled_or_frozen_complete_output_cannot_transfer_a_grant() {
    for freeze in [false, true] {
        let mut owner = owner(&[0, 2, 4]);
        for (index, block) in [0, 2, 4].into_iter().enumerate() {
            record(
                &mut owner,
                block,
                index as u64 + 1,
                true,
                110 + index as u64,
            );
        }
        assert!(owner.is_complete());
        if freeze {
            owner.freeze();
        } else {
            owner.cancel();
        }
        assert!(!owner.is_complete());
        assert!(owner.complete().is_err());
    }
}

#[test]
fn textless_original_is_checked_only_after_its_source_is_bound() {
    let mut original = original();
    original
        .message
        .content
        .retain(|block| !matches!(block, ContentBlock::Text { .. }));
    let mut owner = OutputChecks::new(
        "run",
        &crate::flow::classifier::scripted::fixture_scheme(),
        original,
    )
    .unwrap();
    assert!(!owner.is_complete());
    owner.bind(Sequence(10), 100, &[]).unwrap();
    let checked = owner.complete().unwrap();
    assert!(checked.blocks().is_empty());
    assert_eq!(checked.original.message.content.len(), 2);
    assert_eq!(checked.recorded_ns(), 100);
}

#[test]
fn cancellation_before_author_returns_the_unchanged_original() {
    let mut owner = OutputChecks::new(
        "run",
        &crate::flow::classifier::scripted::fixture_scheme(),
        original(),
    )
    .unwrap();
    owner.cancel();
    let cancelled = owner.cancellation().unwrap();
    assert!(cancelled.source.is_none());
    assert!(cancelled.recorded.is_empty());
    assert_eq!(cancelled.original.message, original().message);
}
