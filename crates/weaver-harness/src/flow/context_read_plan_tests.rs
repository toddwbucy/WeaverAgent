//! Root preflight needs the refusing leg's actual Recorded parent after cleanup.
use super::*;

fn finish(member: &mut Member) -> super::super::read_driver::RetainedReadCompletion {
    for _ in 0..128 {
        member.advance().unwrap();
        if let Some(result) = member
            .run
            .context
            .take_read_retained(&mut member.run.clock)
            .unwrap()
        {
            return result;
        }
    }
    panic!("retained read did not settle");
}

#[test]
fn plan_read_initial_refusal_retains_original_measurement_without_recall_or_release() {
    let mut member = Member::new();
    let initial = member.begin();
    let bound = member.bound();
    let prior_recalls = member.records(Kind::Recall).len();
    member.respond(&initial, refused(s::ReadRefusalCode::NotFound));
    let retained = finish(&mut member);
    assert!(matches!(
        retained.completion,
        ReadCompletion::Refused(super::super::super::read::ReadFailure::Refused {
            code: s::ReadRefusalCode::NotFound,
            ..
        })
    ));
    let proof = retained
        .refusal
        .expect("finalized refusal retains its measured evidence");
    assert!(proof.belongs_to(&member.run.context));
    assert_eq!(proof.selector(), &selector());
    assert_eq!(
        proof.request(),
        &s::StateControl::Read {
            selector: selector()
        }
    );
    assert_eq!(proof.answer().common(), initial.common());
    assert_eq!(proof.bound(), bound);
    assert_eq!(proof.snapshot(), None);
    assert!(proof.release().is_none());
    assert!(proof.partial_bytes().is_empty());
    let records = member.records(Kind::StateStoreMeasurement);
    assert_eq!(
        records.last().unwrap()["sequence"],
        serde_json::json!(proof.measurement_sequence())
    );
    assert_eq!(member.records(Kind::Recall).len(), prior_recalls);
    member.no_request();
    let other = Member::new();
    assert!(!proof.belongs_to(&other.run.context));
}

#[test]
fn plan_read_next_refusal_keeps_original_parent_and_partial_bytes_through_real_release() {
    for cancel in [false, true] {
        let mut member = Member::new();
        let initial = member.begin();
        let bound = member.bound();
        let recalls = member.records(Kind::Recall).len();
        member.respond(&initial, page(11, 0, EMPTY.len(), &EMPTY[..8]));
        let next = member.next_request();
        member.respond(&next, refused(s::ReadRefusalCode::ResultUnavailable));
        if cancel {
            member
                .run
                .context
                .cancel_read(&mut member.run.clock)
                .unwrap();
        }
        assert!(
            member
                .run
                .context
                .take_read_retained(&mut member.run.clock)
                .unwrap()
                .is_none()
        );
        let release = member.next_request();
        let refusing_sequence =
            member.records(Kind::StateStoreMeasurement).last().unwrap()["sequence"].clone();
        assert!(
            member
                .run
                .context
                .take_read_retained(&mut member.run.clock)
                .unwrap()
                .is_none()
        );
        member.respond(
            &release,
            s::StateControlAnswer::Released {
                snapshot_id: 11.into(),
            },
        );
        let retained = finish(&mut member);
        assert_eq!(
            matches!(retained.completion, ReadCompletion::Cancelled(_)),
            cancel
        );
        let proof = retained.refusal.unwrap();
        assert_eq!(
            serde_json::json!(proof.measurement_sequence()),
            refusing_sequence
        );
        assert_eq!(
            proof.request(),
            &s::StateControl::Next {
                snapshot_id: 11.into(),
                offset: 8.into()
            }
        );
        assert_eq!(proof.answer().common(), next.common());
        assert_eq!(proof.selector(), &selector());
        assert_eq!(proof.partial_bytes(), &EMPTY[..8]);
        assert_eq!(proof.snapshot(), Some(11.into()));
        assert_eq!(proof.bound(), bound);
        let cleanup = proof.release().unwrap();
        assert_eq!(cleanup.answer.common(), release.common());
        assert_ne!(cleanup.measurement_sequence, proof.measurement_sequence());
        assert_eq!(
            member.records(Kind::StateStoreMeasurement).last().unwrap()["sequence"],
            serde_json::json!(cleanup.measurement_sequence)
        );
        assert_eq!(member.records(Kind::Recall).len(), recalls);
        assert!(member.run.context.halt().is_none());
        member.no_request();
    }
}

#[test]
fn plan_read_failed_release_never_transfers_a_finalized_operation_refusal() {
    let mut member = Member::new();
    let initial = member.begin();
    member.respond(&initial, page(11, 0, EMPTY.len(), &EMPTY[..8]));
    let next = member.next_request();
    member.respond(&next, refused(s::ReadRefusalCode::ResultUnavailable));
    let release = member.next_request();
    member.respond_protocol_failure(&release, refused(s::ReadRefusalCode::UnknownSnapshot));
    assert!(
        member
            .run
            .context
            .take_read_retained(&mut member.run.clock)
            .is_err()
    );
    assert_eq!(
        member
            .run
            .context
            .read
            .as_ref()
            .unwrap()
            .refused_answer()
            .unwrap()
            .common(),
        next.common()
    );
}

fn plan_selectors() -> [s::ReadSelector; 3] {
    [
        s::ReadSelector::PlanCurrent {},
        s::ReadSelector::Marks {},
        s::ReadSelector::Statement {
            statement: s::StatementRef {
                block_id: s::SourceBlockId {
                    run: "prior".into(),
                    sequence: 3.into(),
                    block: 0.into(),
                },
                binding: 0.into(),
            },
        },
    ]
}

#[test]
fn plan_read_preflight_initial_table_preserves_typed_detail_and_rejects_foreign_proof() {
    use crate::flow::plan::preflight_read_refusal;
    use s::ReadRefusalCode as Code;
    let common = [
        Code::CounterExhausted,
        Code::ResultTooLarge,
        Code::ResultUnavailable,
        Code::AnswerTooLarge,
    ];
    let excluded = [
        Code::UnsupportedOperation,
        Code::InvalidSelector,
        Code::WrongSeed,
        Code::UnknownSpan,
        Code::SpanOpen,
        Code::SpanPending,
        Code::PartialPair,
    ];
    for selected in plan_selectors().into_iter().chain([selector()]) {
        for code in common.into_iter().chain([Code::NotFound]).chain(excluded) {
            let mut member = Member::new();
            member
                .run
                .context
                .begin_read(selected.clone(), None, &mut member.run.clock)
                .unwrap();
            let request = member.next_request();
            let detail = format!("original {code:?} detail\nwith escaped \\\"text\\\"");
            member.respond(
                &request,
                s::StateControlAnswer::ControlRefused {
                    code: s::ControlRefusalCode::Read(code),
                    detail: detail.clone(),
                },
            );
            let proof = finish(&mut member).refusal.unwrap();
            let result = preflight_read_refusal(&member.run.context, &selected, &proof);
            let expected = selected != selector()
                && (common.contains(&code)
                    || (matches!(selected, s::ReadSelector::Statement { .. })
                        && code == Code::NotFound));
            assert_eq!(result.is_ok(), expected, "{selected:?}: {code:?}");
            if let Ok(result) = result {
                assert_eq!(result.case, s::PlanUpdateCase::ReadRefused);
                assert_eq!(result.read_code, Some(code));
                assert_eq!(result.detail, detail);
                assert!(result.rejected_blocks.is_empty());
            }
            let other = Member::new();
            assert!(preflight_read_refusal(&other.run.context, &selected, &proof).is_err());
            let foreign_selector = if matches!(selected, s::ReadSelector::Marks {}) {
                s::ReadSelector::PlanCurrent {}
            } else {
                s::ReadSelector::Marks {}
            };
            assert!(
                preflight_read_refusal(&member.run.context, &foreign_selector, &proof).is_err()
            );
            assert!(member.records(Kind::Recall).is_empty());
            assert!(member.run.context.halt().is_none());
            member.no_request();
        }
    }
}

#[test]
fn plan_read_preflight_next_table_requires_the_actual_acquired_release() {
    use crate::flow::plan::preflight_read_refusal;
    use s::ReadRefusalCode as Code;
    for selected in plan_selectors() {
        for code in [
            Code::UnknownSnapshot,
            Code::WrongOffset,
            Code::ResultUnavailable,
            Code::AnswerTooLarge,
        ] {
            let mut member = Member::new();
            member
                .run
                .context
                .begin_read(selected.clone(), None, &mut member.run.clock)
                .unwrap();
            let initial = member.next_request();
            member.respond(&initial, page(11, 0, 20, b"{"));
            let next = member.next_request();
            member.respond(&next, refused(code));
            let release = member.next_request();
            assert!(
                member
                    .run
                    .context
                    .take_read_retained(&mut member.run.clock)
                    .unwrap()
                    .is_none()
            );
            let parent =
                member.records(Kind::StateStoreMeasurement).last().unwrap()["sequence"].clone();
            member.respond(
                &release,
                s::StateControlAnswer::Released {
                    snapshot_id: 11.into(),
                },
            );
            let proof = finish(&mut member).refusal.unwrap();
            let outcome = preflight_read_refusal(&member.run.context, &selected, &proof).unwrap();
            assert_eq!(outcome.read_code, Some(code));
            assert_eq!(serde_json::json!(proof.measurement_sequence()), parent);
            assert_eq!(proof.partial_bytes(), b"{");
            assert!(proof.release().is_some());
            assert!(member.records(Kind::Recall).is_empty());
            assert!(member.run.context.halt().is_none());
            member.no_request();
        }
    }
}

#[test]
fn plan_read_consumed_refusal_keeps_evidence_across_post_accept_leave() {
    let mut member = Member::new();
    let initial = member.begin();
    member.respond(&initial, refused(s::ReadRefusalCode::ResultUnavailable));
    member.record_completion_only();
    let parent = member.records(Kind::StateStoreMeasurement).last().unwrap()["sequence"].clone();
    let now = member.now();
    let leave = AbsoluteBound::after(now, 10).unwrap();
    member
        .run
        .context
        .arm(PumpDeadline::Leave, leave, &mut member.run.clock)
        .unwrap();
    let mut clock = ValidationClock {
        before: now,
        deadline: leave.deadline_ns(),
        reads: 0,
    };
    assert_eq!(
        member
            .run
            .context
            .settle_recorded_read(&mut clock)
            .unwrap_err(),
        RunFailure::Wait {
            key: PumpDeadline::Leave,
            bound: leave
        }
    );
    assert!(member.run.context.flow.operation_completion().is_none());
    assert_eq!(
        member
            .run
            .context
            .read
            .as_ref()
            .unwrap()
            .refused_answer()
            .unwrap()
            .common(),
        initial.common()
    );
    assert_eq!(
        member.run.context.complete_wait(PumpDeadline::Leave),
        Some(leave)
    );
    member.run.clock.advance_to(leave.deadline_ns()).unwrap();
    let proof = finish(&mut member).refusal.unwrap();
    assert_eq!(serde_json::json!(proof.measurement_sequence()), parent);
    assert!(member.records(Kind::Recall).is_empty());
    member.no_request();
}
