use super::*;
use crate::flow::sort::{OutboundInput, OutboundRefusal, identity_sort, noun_keys};
use weaver_types::state_flow::SortAct;

fn effective() -> EffectiveEmission {
    let original = original_at(
        10,
        vec![
            text("Read files."),
            call("ls"),
            text("Recall files."),
            text("Read answer."),
        ],
        vec![
            accepted("Read files."),
            accepted("Recall files."),
            accepted("Read answer."),
        ],
    );
    RepairLedger::begin(original, 2)
        .unwrap()
        .take_effective()
        .unwrap()
        .unwrap()
}

fn input() -> OutboundInput {
    let effective = effective();
    let acts = effective
        .requests()
        .iter()
        .map(|request| match request.lane {
            PairLane::External => SortAct::Tool {
                call_id: request.call_id.clone(),
                statement: request.statement.clone(),
            },
            PairLane::Recall => SortAct::Recall {
                call_id: request.call_id.clone(),
                statement: request.statement.clone(),
            },
            PairLane::Review => panic!("model cannot create review"),
        })
        .chain(std::iter::once(SortAct::Answer {
            source: effective.statements().last().unwrap().block_id.clone(),
        }))
        .collect();
    OutboundInput {
        source: effective.source().clone(),
        effective,
        acts,
    }
}

#[test]
fn outbound_preserves_exact_complete_order_without_opening_pairs_or_answer_policy() {
    let value = input();
    let acts = value.acts.clone();
    let source = value.source.clone();
    let text = value.effective.text().to_owned();
    let sorted = identity_sort(value).unwrap().into_input();
    assert_eq!(sorted.acts, acts);
    assert_eq!(sorted.source, source);
    assert_eq!(sorted.effective.text(), text);
    assert!(sorted.effective.attempted_call());
    assert_eq!(sorted.effective.requests().len(), 2);
    // Empty caller choice remains empty, even when requests and text exist.
    let mut empty = input();
    empty.acts.clear();
    assert!(identity_sort(empty).unwrap().input().acts.is_empty());
}

#[test]
fn outbound_refuses_unknown_duplicate_foreign_and_wrong_lane_identities_unchanged() {
    for change in 0..8 {
        let mut value = input();
        match change {
            0 => value.source.turn = Some("foreign".into()),
            1 => value.source.block_id.sequence = 99.into(),
            2 => value.acts.push(value.acts[0].clone()),
            3 => value.acts.push(value.acts.last().unwrap().clone()),
            4 => {
                if let SortAct::Tool { call_id, .. } = &mut value.acts[0] {
                    call_id.run = "foreign".into();
                }
            }
            5 => {
                if let SortAct::Tool { statement, .. } = &mut value.acts[0] {
                    statement.binding = 99.into();
                }
            }
            6 => {
                if let SortAct::Tool { call_id, statement } = &value.acts[0] {
                    value.acts[0] = SortAct::Recall {
                        call_id: call_id.clone(),
                        statement: statement.clone(),
                    };
                }
            }
            7 => value.acts.push(SortAct::Answer {
                source: source(99, 0),
            }),
            _ => unreachable!(),
        }
        let expected = value.acts.clone();
        let original = value.source.clone();
        let (returned, refusal) = identity_sort(value).unwrap_err();
        assert_eq!(refusal, OutboundRefusal::InvalidCandidate);
        assert_eq!(returned.acts, expected);
        assert_eq!(returned.source, original);
        assert_eq!(returned.effective.requests().len(), 2);
    }
}

#[test]
fn outbound_answers_use_effective_block_order_and_never_binding_indices() {
    let mut value = input();
    value.acts = value
        .effective
        .statements()
        .iter()
        .map(|statement| SortAct::Answer {
            source: statement.block_id.clone(),
        })
        .collect();
    assert!(identity_sort(value).is_ok());
    let mut value = input();
    value.acts = value
        .effective
        .statements()
        .iter()
        .rev()
        .map(|statement| SortAct::Answer {
            source: statement.block_id.clone(),
        })
        .collect();
    assert_eq!(
        identity_sort(value).unwrap_err().1,
        OutboundRefusal::InvalidCandidate
    );
}

#[test]
fn subjects_preserve_compound_nouns_and_sort_unique_exact_utf8_bytes() {
    let words = "Read fuel pump. Read Zebra. Read fuel pump. Read apple. Read \u{00e9}clair.";
    let Ste100Result::Accepted { bindings } = accepted(words) else {
        unreachable!()
    };
    for binding in &bindings {
        binding.validate(words, &["read".into()]).unwrap();
    }
    assert_eq!(
        noun_keys(&bindings),
        vec!["Zebra", "apple", "fuel pump", "\u{00e9}clair"]
    );
    assert!(noun_keys(&[]).is_empty());
}

#[test]
fn outbound_keeps_original_call_ids_and_corrected_statement_refs_in_logical_order() {
    let original = original_at(
        10,
        vec![text("bad"), call("opaque command"), text("Read sibling.")],
        vec![rejected(), accepted("Read sibling.")],
    );
    let expected = original.unresolved()[0].call_id().clone();
    let mut ledger = RepairLedger::begin(original, 2).unwrap();
    request(&mut ledger, 40);
    ledger
        .observe_correction(
            &source(10, 0),
            original_at(
                100,
                vec![text("Read corrected.")],
                vec![accepted("Read corrected.")],
            ),
        )
        .unwrap();
    let effective = ledger.take_effective().unwrap().unwrap();
    let statement = effective.requests()[0].statement.clone();
    let acts = vec![
        SortAct::Tool {
            call_id: expected.clone(),
            statement: statement.clone(),
        },
        SortAct::Answer {
            source: source(100, 0),
        },
        SortAct::Answer {
            source: source(10, 2),
        },
    ];
    let sorted = identity_sort(OutboundInput {
        source: effective.source().clone(),
        effective,
        acts: acts.clone(),
    })
    .unwrap()
    .into_input();
    assert_eq!(sorted.acts, acts);
    assert_eq!(sorted.effective.requests()[0].call_id, expected);
    assert_eq!(statement.block_id, source(100, 0));
    assert_eq!(sorted.effective.text(), "Read corrected.\nRead sibling.");
    assert_eq!(sorted.effective.account().generations().count(), 2);
}

const _: fn() = || {
    trait NotClone<M> {
        fn check() {}
    }
    impl<T: ?Sized> NotClone<()> for T {}
    impl<T: Clone> NotClone<u8> for T {}
    let _ = <crate::flow::sort::CheckedOutbound as NotClone<_>>::check;
    let _ = <OutboundInput as NotClone<_>>::check;
    trait NotDefault<M> {
        fn check() {}
    }
    impl<T: ?Sized> NotDefault<()> for T {}
    impl<T: Default> NotDefault<u8> for T {}
    let _ = <crate::flow::sort::CheckedOutbound as NotDefault<_>>::check;
};

#[test]
fn outbound_recall_keeps_its_original_nonzero_binding_ordinal() {
    for changed in [false, true] {
        let original = original_at(
            10,
            vec![text("Read item. Recall files.")],
            vec![accepted("Read item. Recall files.")],
        );
        let effective = RepairLedger::begin(original, 2)
            .unwrap()
            .take_effective()
            .unwrap()
            .unwrap();
        let request = &effective.requests()[0];
        assert_eq!(request.statement.binding.0, 1);
        let mut statement = request.statement.clone();
        if changed {
            statement.binding = 0.into();
        }
        let acts = vec![SortAct::Recall {
            call_id: request.call_id.clone(),
            statement,
        }];
        let result = identity_sort(OutboundInput {
            source: effective.source().clone(),
            effective,
            acts,
        });
        assert_eq!(result.is_err(), changed);
    }
}
