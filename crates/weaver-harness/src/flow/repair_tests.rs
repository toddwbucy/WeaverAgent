//! Repair-owner fixtures use validated OutputChecks completions and recorded
//! template-certificate states. Socket tests prove the context's recording side.

use super::*;
use crate::flow::coordinator::{
    CheckOperation, RecordedCheck, RequestScheme, checked_request_fixture_at,
};
use crate::flow::templates::{
    CompiledInventory, ExpansionIdentity, RestatementTemplate, TemplateCheckTranscript,
};
use sha2::{Digest as _, Sha256};
use weaver_traits::{ContentBlock, ToolCall};
use weaver_types::state_flow::{
    Binding, Digest, NounBinding, Span, Ste100DefectCode, Ste100Reply, TemplatePart, TemplateSpec,
};

static RESTATEMENT: RestatementTemplate = RestatementTemplate::new("repair-test-restate");

#[path = "sort_tests.rs"]
mod sort_tests;

const _: fn() = || {
    trait NotClone<M> {
        fn check() {}
    }
    impl<T: ?Sized> NotClone<()> for T {}
    impl<T: Clone> NotClone<u8> for T {}
    let _ = <RepairLedger as NotClone<_>>::check;
    let _ = <RepairAccount as NotClone<_>>::check;
    let _ = <RecordedRecallRefusal as NotClone<_>>::check;
    let _ = <EffectiveEmission as NotClone<_>>::check;
    let _ = <RestatementRequest as NotClone<_>>::check;
    trait NotDefault<M> {
        fn check() {}
    }
    impl<T: ?Sized> NotDefault<()> for T {}
    impl<T: Default> NotDefault<u8> for T {}
    let _ = <EffectiveEmission as NotDefault<_>>::check;
};

fn scheme() -> (SchemeDeclaration, RequestScheme) {
    scheme_named("repair-fixture/1")
}

fn scheme_named(edition: &str) -> (SchemeDeclaration, RequestScheme) {
    let seed: weaver_types::state_flow::SeedRequest =
        weaver_types::state_flow::decode_strict(include_str!("fixtures/seed.json")).unwrap();
    let mut artifact: serde_json::Value = serde_json::from_str(&seed.ste100_scheme).unwrap();
    artifact["reference"]["edition"] = serde_json::json!(edition);
    let mut recall = artifact["verbs"][0].clone();
    recall["key"] = serde_json::json!("recall");
    recall["forms"] = serde_json::json!(["Recall"]);
    artifact["verbs"].as_array_mut().unwrap().push(recall);
    artifact["recall_keys"] = serde_json::json!(["recall"]);
    let text = serde_json::to_string(&artifact).unwrap();
    let digest = Digest::new(format!("{:x}", Sha256::digest(text.as_bytes()))).unwrap();
    let declaration = SchemeDeclaration {
        scheme_id: format!("ste100:{digest}"),
        artifact_sha256: digest,
        verbs: vec!["read".into(), "recall".into()],
    };
    let scheme = RequestScheme::from_exact_scheme(&text, &declaration).unwrap();
    (declaration, scheme)
}

fn text(text: &str) -> ContentBlock {
    ContentBlock::Text { text: text.into() }
}
fn call(arguments: &str) -> ContentBlock {
    ContentBlock::ToolCall(ToolCall {
        name: "sh".into(),
        arguments: arguments.into(),
    })
}
fn span(start: usize, end: usize) -> Span {
    Span {
        start: (start as u64).into(),
        end: (end as u64).into(),
    }
}
fn accepted(text: &str) -> Ste100Result {
    let mut offset = 0;
    let bindings = text
        .split_inclusive('.')
        .map(|raw| {
            let sentence = raw.trim_start();
            let start = offset + raw.len() - sentence.len();
            offset += raw.len();
            let verb_end = sentence.find(' ').unwrap();
            Binding {
                statement_span: span(start, offset),
                verb: sentence[..verb_end].to_lowercase(),
                verb_span: span(start, start + verb_end),
                condition: None,
                condition_span: None,
                nouns: vec![NounBinding {
                    value: sentence[verb_end + 1..sentence.len() - 1].into(),
                    span: span(start + verb_end + 1, offset - 1),
                }],
            }
        })
        .collect();
    Ste100Result::Accepted { bindings }
}
fn rejected() -> Ste100Result {
    Ste100Result::NotSte100 {
        code: Ste100DefectCode::NonSte100,
        detail: "fixture defect".into(),
        spans: vec![],
    }
}
fn source(sequence: u64, block: u64) -> SourceBlockId {
    SourceBlockId {
        run: "run".into(),
        sequence: sequence.into(),
        block: block.into(),
    }
}
fn record(sequence: u64) -> SourcePosition {
    SourcePosition {
        block_id: source(sequence, 0),
        turn: Some("turn".into()),
    }
}

fn original_at(
    sequence: u64,
    content: Vec<ContentBlock>,
    results: Vec<Ste100Result>,
) -> OriginalRequests {
    let (declaration, scheme) = scheme();
    OriginalRequests::from_checked(
        checked_request_fixture_at(&declaration, content, results, "run", "turn", sequence),
        &scheme,
    )
    .unwrap()
}

pub(super) fn expansion(run: &str, scheme: &SchemeDeclaration) -> TemplateExpansion {
    let entries = vec![TemplateSpec {
        template_id: "repair-test-restate".into(),
        kind: "loop.instruction".into(),
        parts: vec![TemplatePart::Literal("Restate the statement.".into())],
    }];
    let inventory = CompiledInventory::new(Box::leak(entries.into_boxed_slice())).unwrap();
    let checker = Digest::new("b".repeat(64)).unwrap();
    let load = inventory.record(scheme, &checker);
    let mut transcript =
        TemplateCheckTranscript::begin(inventory, run.into(), scheme.clone(), checker, &load)
            .unwrap();
    let operation = transcript.next_operation().unwrap();
    let CheckOperation::Template { template_id, .. } = &operation else {
        unreachable!()
    };
    let reply = Ste100Reply {
        request_id: 1.into(),
        target: Ste100Target::Template {
            template_id: template_id.clone(),
        },
        scheme_id: scheme.scheme_id.clone(),
        result: Ste100Result::Accepted { bindings: vec![] },
    };
    let result_source = SourcePosition {
        block_id: SourceBlockId {
            run: run.into(),
            sequence: 2.into(),
            block: 0.into(),
        },
        turn: None,
    };
    transcript
        .recorded(RecordedCheck::for_test(operation, reply, result_source))
        .unwrap();
    let certificate = transcript.finish().unwrap();
    RESTATEMENT
        .expand(
            &certificate,
            ExpansionIdentity {
                run_id: run,
                scheme,
                checker_artifact_sha256: certificate.checker_artifact_sha256(),
                inventory_sha256: certificate.inventory_sha256(),
            },
        )
        .unwrap()
}

fn request(ledger: &mut RepairLedger, first: u64) -> RestatementRequest {
    if matches!(ledger.intent(), RepairIntent::Requested { .. }) {
        ledger.recorded_requested(record(first)).unwrap();
    }
    let expanded = expansion("run", ledger.account.as_ref().unwrap().scheme());
    ledger
        .recorded_instruction(expanded, record(first + 1))
        .unwrap()
}

#[test]
fn accepted_generation_releases_one_effective_list_with_original_identity_and_request_order() {
    let original = original_at(
        10,
        vec![
            text("Read first. Read second."),
            text("Read send."),
            call("opaque"),
            text("Recall history."),
        ],
        vec![
            accepted("Read first. Read second."),
            accepted("Read send."),
            accepted("Recall history."),
        ],
    );
    let mut ledger = RepairLedger::begin(original, 2).unwrap();
    assert_eq!(ledger.intent(), RepairIntent::Complete);
    let effective = ledger.take_effective().unwrap().unwrap();
    assert_eq!(effective.source(), &record(10));
    assert_eq!(effective.finish(), &Finish::Stopped);
    assert_eq!(
        effective.text(),
        "Read first.\nRead second.\nRead send.\nRecall history."
    );
    assert!(effective.attempted_call());
    assert_eq!(
        effective
            .statements()
            .iter()
            .map(|statement| (statement.block_id.block.0, statement.binding.0))
            .collect::<Vec<_>>(),
        vec![(0, 0), (0, 1), (1, 0), (3, 0)]
    );
    assert_eq!(
        effective
            .requests()
            .iter()
            .map(|request| (request.call_block.0, request.call_id.call.0))
            .collect::<Vec<_>>(),
        vec![(2, 0), (3, 1)]
    );
    assert!(ledger.take_effective().unwrap().is_none());
    assert_eq!(effective.into_account().generations().count(), 1);
}

#[test]
fn sparse_rejected_sources_repair_in_original_order_and_replace_only_their_logical_slots() {
    let original = original_at(
        10,
        vec![text("bad first"), text("Read middle."), text("bad last")],
        vec![rejected(), accepted("Read middle."), rejected()],
    );
    let mut ledger = RepairLedger::begin(original, 3).unwrap();
    assert_eq!(
        ledger.intent(),
        RepairIntent::Requested {
            source: source(10, 0),
            retry_cap: 3.into()
        }
    );
    let first = request(&mut ledger, 40);
    assert_eq!(first.source(), &source(10, 0));
    assert_eq!(first.attempt().0, 1);
    assert_eq!(first.recorded(), &record(41));
    assert_eq!(first.instruction().text(), "Restate the statement.");
    let correction = original_at(
        100,
        vec![text("Read fixed. Read extra."), text("Read next.")],
        vec![accepted("Read fixed. Read extra."), accepted("Read next.")],
    );
    assert_eq!(
        ledger
            .observe_correction(&source(10, 0), correction)
            .unwrap(),
        CorrectionProgress::Resolved
    );
    assert_eq!(
        ledger.intent(),
        RepairIntent::Requested {
            source: source(10, 2),
            retry_cap: 3.into()
        }
    );
    request(&mut ledger, 140);
    let correction = original_at(
        200,
        vec![text("Read final.")],
        vec![accepted("Read final.")],
    );
    ledger
        .observe_correction(&source(10, 2), correction)
        .unwrap();
    let effective = ledger.take_effective().unwrap().unwrap();
    assert_eq!(
        effective.text(),
        "Read fixed.\nRead extra.\nRead next.\nRead middle.\nRead final."
    );
    assert_eq!(
        effective
            .statements()
            .iter()
            .map(|statement| (
                statement.block_id.sequence.0,
                statement.block_id.block.0,
                statement.binding.0
            ))
            .collect::<Vec<_>>(),
        vec![
            (100, 0, 0),
            (100, 0, 1),
            (100, 1, 0),
            (10, 1, 0),
            (200, 0, 0)
        ]
    );
    assert_eq!(effective.into_account().generations().count(), 3);
}

#[test]
fn failed_complete_correction_counts_once_and_accepted_siblings_do_not_leak_into_effective_text() {
    let original = original_at(10, vec![text("bad")], vec![rejected()]);
    let mut ledger = RepairLedger::begin(original, 2).unwrap();
    request(&mut ledger, 40);
    let correction = original_at(
        100,
        vec![text("bad one"), text("Read unused."), text("bad two")],
        vec![rejected(), accepted("Read unused."), rejected()],
    );
    assert_eq!(
        ledger
            .observe_correction(&source(10, 0), correction)
            .unwrap(),
        CorrectionProgress::Retry
    );
    assert_eq!(ledger.failed_corrections(), Some(1));
    assert_eq!(
        ledger.intent(),
        RepairIntent::Instruction {
            source: source(10, 0),
            attempt: 2.into()
        }
    );
    assert_eq!(
        ledger.recorded_requested(record(140)),
        Err(RepairError::WrongPhase)
    );
    request(&mut ledger, 140);
    let correction = original_at(
        200,
        vec![text("Read corrected.")],
        vec![accepted("Read corrected.")],
    );
    ledger
        .observe_correction(&source(10, 0), correction)
        .unwrap();
    let effective = ledger.take_effective().unwrap().unwrap();
    assert_eq!(effective.text(), "Read corrected.");
    let account = effective.into_account();
    assert_eq!(account.corrections().len(), 2);
    assert_eq!(account.corrections()[0].generation().blocks().len(), 3);
}

#[test]
fn correction_calls_require_recorded_protocol_refusal_and_accounting_before_one_failed_attempt() {
    let original = original_at(10, vec![text("bad")], vec![rejected()]);
    let mut ledger = RepairLedger::begin(original, 2).unwrap();
    request(&mut ledger, 40);
    let correction = original_at(
        100,
        vec![
            ContentBlock::UnrecoveredCall {
                fragment: "<partial>".into(),
            },
            text("bad"),
        ],
        vec![rejected()],
    );
    assert_eq!(
        ledger
            .observe_correction(&source(10, 0), correction)
            .unwrap(),
        CorrectionProgress::ProtocolRefusal
    );
    assert_eq!(ledger.failed_corrections(), Some(0));
    assert!(ledger.take_correction_accounting().is_none());
    ledger.recorded_protocol_refusal(record(140)).unwrap();
    let correction = ledger.take_correction_accounting().unwrap();
    assert!(correction.requests().is_empty());
    assert!(ledger.take_correction_accounting().is_none());
    assert_eq!(
        ledger
            .finish_correction_accounting(correction, &[])
            .unwrap(),
        CorrectionProgress::Retry
    );
    assert_eq!(ledger.failed_corrections(), Some(1));
    assert!(ledger.take_effective().unwrap().is_none());
}

#[test]
fn conforming_forbidden_correction_requests_cannot_be_discarded_as_completed_accounting() {
    let original = original_at(10, vec![text("bad")], vec![rejected()]);
    let mut ledger = RepairLedger::begin(original, 2).unwrap();
    request(&mut ledger, 40);
    let correction = original_at(
        100,
        vec![text("Read send."), call("opaque"), text("Recall history.")],
        vec![accepted("Read send."), accepted("Recall history.")],
    );
    ledger
        .observe_correction(&source(10, 0), correction)
        .unwrap();
    ledger.recorded_protocol_refusal(record(140)).unwrap();
    let correction = ledger.take_correction_accounting().unwrap();
    assert_eq!(correction.requests().len(), 2);
    let (returned, error) = ledger
        .finish_correction_accounting(correction, &[])
        .unwrap_err();
    assert_eq!(error, RepairError::WrongAccounting);
    assert_eq!(returned.generation().source(), &record(100));
    assert_eq!(returned.requests().len(), 2);
    assert_eq!(ledger.failed_corrections(), Some(0));
    assert!(ledger.take_effective().unwrap().is_none());
}

#[test]
fn foreign_and_non_forward_corrections_return_their_complete_owned_evidence() {
    for (run, turn, sequence, expected) in [
        ("foreign", "turn", 100, RepairError::WrongRun),
        ("run", "foreign", 100, RepairError::WrongTurn),
        ("run", "turn", 40, RepairError::OutOfOrder),
    ] {
        let (declaration, scheme) = scheme();
        let original = original_at(10, vec![text("bad")], vec![rejected()]);
        let mut ledger = RepairLedger::begin(original, 2).unwrap();
        request(&mut ledger, 40);
        let correction = OriginalRequests::from_checked(
            checked_request_fixture_at(
                &declaration,
                vec![text("Read fixed.")],
                vec![accepted("Read fixed.")],
                run,
                turn,
                sequence,
            ),
            &scheme,
        )
        .unwrap();
        let before = correction.generation().source().clone();
        let (returned, error) = ledger
            .observe_correction(&source(10, 0), correction)
            .unwrap_err();
        assert_eq!(error, expected);
        assert_eq!(returned.generation().source(), &before);
        assert!(matches!(
            ledger.intent(),
            RepairIntent::AwaitingCorrection { .. }
        ));
        assert_eq!(ledger.failed_corrections(), Some(0));
    }
}

#[test]
fn wrong_source_and_stale_instruction_records_do_not_consume_a_repair_attempt() {
    let original = original_at(10, vec![text("bad")], vec![rejected()]);
    let mut ledger = RepairLedger::begin(original, 2).unwrap();
    assert_eq!(
        ledger.recorded_requested(record(20)),
        Err(RepairError::OutOfOrder)
    );
    ledger.recorded_requested(record(40)).unwrap();
    let expanded = expansion("run", ledger.account.as_ref().unwrap().scheme());
    let (expanded, error) = ledger
        .recorded_instruction(expanded, record(40))
        .unwrap_err();
    assert_eq!(error, RepairError::OutOfOrder);
    assert_eq!(expanded.text(), "Restate the statement.");
    ledger.recorded_instruction(expanded, record(41)).unwrap();
    let correction = original_at(
        100,
        vec![text("Read fixed.")],
        vec![accepted("Read fixed.")],
    );
    let (returned, error) = ledger
        .observe_correction(&source(10, 1), correction)
        .unwrap_err();
    assert_eq!(error, RepairError::WrongSource);
    ledger.observe_correction(&source(10, 0), returned).unwrap();
    assert_eq!(
        ledger.take_effective().unwrap().unwrap().text(),
        "Read fixed."
    );
}

#[test]
fn repaired_external_send_preserves_original_call_identity_and_executable_bytes() {
    let original = original_at(
        10,
        vec![
            text("bad"),
            call("printf 'opaque original'"),
            text("Read sibling."),
        ],
        vec![rejected(), accepted("Read sibling.")],
    );
    let expected = original.unresolved()[0].call_id().clone();
    let mut ledger = RepairLedger::begin(original, 2).unwrap();
    request(&mut ledger, 40);
    let correction = original_at(
        100,
        vec![text("Read corrected.")],
        vec![accepted("Read corrected.")],
    );
    ledger
        .observe_correction(&source(10, 0), correction)
        .unwrap();
    let effective = ledger.take_effective().unwrap().unwrap();
    let request = &effective.requests()[0];
    assert_eq!(request.call_id, expected);
    assert_eq!(request.call_block.0, 1);
    assert_eq!(request.statement.block_id, source(100, 0));
    assert_eq!(request.statement.binding.0, 0);
    assert!(
        matches!(&request.request, weaver_types::state_flow::PairRequest::External { name, arguments } if name == "sh" && arguments == "printf 'opaque original'")
    );
    assert_eq!(effective.text(), "Read corrected.\nRead sibling.");
}

#[test]
fn ambiguous_repaired_external_statement_is_an_operation_refusal_not_another_failed_correction() {
    let original = original_at(10, vec![text("bad"), call("opaque")], vec![rejected()]);
    let mut ledger = RepairLedger::begin(original, 2).unwrap();
    request(&mut ledger, 40);
    let correction = original_at(
        100,
        vec![text("Read first. Read second.")],
        vec![accepted("Read first. Read second.")],
    );
    assert_eq!(
        ledger
            .observe_correction(&source(10, 0), correction)
            .unwrap(),
        CorrectionProgress::Resolved
    );
    assert_eq!(ledger.contexts[0].failed, 0);
    assert_eq!(ledger.intent(), RepairIntent::AssociationRefusals);
    assert_eq!(
        ledger.associations()[0].case(),
        ProtocolRepairCase::AmbiguousToolStatement
    );
    assert!(ledger.take_effective().unwrap().is_none());
    ledger.recorded_association_refusal(0, record(140)).unwrap();
    let effective = ledger.take_effective().unwrap().unwrap();
    assert!(effective.requests().is_empty());
    assert!(effective.attempted_call());
    assert_eq!(effective.text(), "Read first.\nRead second.");
}

#[test]
fn exhaustion_retains_an_earlier_repaired_external_send_for_mandatory_accounting() {
    let original = original_at(
        10,
        vec![text("bad first"), call("opaque"), text("bad last")],
        vec![rejected(), rejected()],
    );
    let mut ledger = RepairLedger::begin(original, 1).unwrap();
    request(&mut ledger, 40);
    ledger
        .observe_correction(
            &source(10, 0),
            original_at(
                100,
                vec![text("Read repaired.")],
                vec![accepted("Read repaired.")],
            ),
        )
        .unwrap();
    request(&mut ledger, 140);
    assert_eq!(
        ledger
            .observe_correction(
                &source(10, 2),
                original_at(200, vec![text("still bad")], vec![rejected()])
            )
            .unwrap(),
        CorrectionProgress::Exhausted
    );
    assert!(ledger.take_exhausted().is_none());
    ledger.recorded_exhausted(record(240)).unwrap();
    let account = ledger.take_exhausted_account().unwrap();
    assert_eq!(account.requests().len(), 1);
    assert_eq!(account.requests()[0].call_id.sequence.0, 10);
    assert_eq!(account.requests()[0].statement.block_id, source(100, 0));
    assert_eq!(account.generations().count(), 3);
    let (account, error) = ledger.finish_exhausted_account(account, &[]).unwrap_err();
    assert_eq!(error, RepairError::WrongAccounting);
    assert_eq!(account.requests().len(), 1);
    assert!(ledger.take_exhausted().is_none());
    assert!(ledger.take_effective().unwrap().is_none());
}

#[test]
fn cap_is_positive_and_exhaustion_requires_recording_then_exact_account_return() {
    let original = original_at(10, vec![text("bad")], vec![rejected()]);
    let (original, error) = RepairLedger::begin(original, 0).unwrap_err();
    assert_eq!(error, RepairError::InvalidCap);
    let mut ledger = RepairLedger::begin(original, 1).unwrap();
    request(&mut ledger, 40);
    ledger
        .observe_correction(
            &source(10, 0),
            original_at(100, vec![text("bad again")], vec![rejected()]),
        )
        .unwrap();
    assert!(ledger.take_exhausted_account().is_none());
    ledger.recorded_exhausted(record(140)).unwrap();
    let account = ledger.take_exhausted_account().unwrap();
    let mut foreign = RepairLedger::begin(
        original_at(
            10,
            vec![text("Read foreign.")],
            vec![accepted("Read foreign.")],
        ),
        1,
    )
    .unwrap();
    let foreign = foreign.take_effective().unwrap().unwrap().into_account();
    let (foreign, error) = ledger.finish_exhausted_account(foreign, &[]).unwrap_err();
    assert_eq!(error, RepairError::WrongAccounting);
    assert_eq!(foreign.source(), &record(10));
    ledger.finish_exhausted_account(account, &[]).unwrap();
    let exhausted = ledger.take_exhausted().unwrap();
    assert_eq!(exhausted.source(), &source(10, 0));
    assert_eq!(exhausted.failed_corrections().0, 1);
    assert_eq!(exhausted.retry_cap().0, 1);
    assert!(ledger.take_exhausted().is_none());
    assert_eq!(exhausted.into_account().generations().count(), 2);
}

#[test]
fn cancellation_and_freeze_preserve_evidence_without_a_success_grant() {
    for frozen in [false, true] {
        let original = original_at(10, vec![text("bad")], vec![rejected()]);
        let mut ledger = RepairLedger::begin(original, 2).unwrap();
        request(&mut ledger, 40);
        let correction = original_at(100, vec![call("unassociated")], vec![]);
        ledger
            .observe_correction(&source(10, 0), correction)
            .unwrap();
        if frozen {
            ledger.freeze();
            ledger.cancel();
            assert_eq!(ledger.intent(), RepairIntent::Frozen);
            assert!(matches!(ledger.take_effective(), Err(RepairError::Frozen)));
            assert!(ledger.take_cancelled().is_none());
            assert!(ledger.pending_correction().is_some());
        } else {
            ledger.cancel();
            assert!(ledger.take_effective().unwrap().is_none());
            assert!(ledger.take_cancelled().is_none());
            ledger.recorded_protocol_refusal(record(140)).unwrap();
            let cancelled = ledger.take_cancelled().unwrap();
            assert_eq!(cancelled.account().source(), &record(10));
            assert_eq!(
                cancelled.pending().unwrap().generation().source(),
                &record(100)
            );
            assert!(ledger.take_cancelled().is_none());
        }
    }
}

#[test]
fn empty_corrections_count_once_after_recorded_refusal_and_reach_the_cap() {
    let original = original_at(10, vec![text("bad")], vec![rejected()]);
    let mut ledger = RepairLedger::begin(original, 2).unwrap();
    request(&mut ledger, 40);
    for (index, correction_sequence) in [100, 200].into_iter().enumerate() {
        let empty = original_at(correction_sequence, vec![], vec![]);
        assert_eq!(
            ledger.observe_correction(&source(10, 0), empty).unwrap(),
            CorrectionProgress::EmptyCorrectionRefusal
        );
        assert_eq!(
            ledger.intent(),
            RepairIntent::EmptyCorrectionRefusal {
                source: source(10, 0),
                correction: record(correction_sequence),
            }
        );
        assert_eq!(ledger.failed_corrections(), Some(index as u32));
        assert!(ledger.take_effective().unwrap().is_none());
        assert!(ledger.take_exhausted().is_none());
        assert!(ledger.take_correction_accounting().is_none());
        assert_eq!(
            ledger.recorded_empty_correction_refusal(record(correction_sequence)),
            Err(RepairError::OutOfOrder)
        );
        assert_eq!(ledger.failed_corrections(), Some(index as u32));
        ledger
            .recorded_empty_correction_refusal(record(correction_sequence + 1))
            .unwrap();
        assert_eq!(ledger.failed_corrections(), Some(index as u32 + 1));
        assert_eq!(
            ledger.recorded_empty_correction_refusal(record(correction_sequence + 2)),
            Err(RepairError::WrongPhase)
        );
        assert_eq!(ledger.failed_corrections(), Some(index as u32 + 1));
        if index == 0 {
            assert!(
                matches!(ledger.intent(), RepairIntent::Instruction { attempt, .. } if attempt.0 == 2)
            );
            let account = ledger.account.as_ref().unwrap();
            let instruction = expansion("run", account.scheme());
            ledger
                .recorded_instruction(instruction, record(140))
                .unwrap();
        }
    }
    assert!(
        matches!(ledger.intent(), RepairIntent::ExhaustedRecord { failed_corrections, .. } if failed_corrections.0 == 2)
    );
    assert_eq!(ledger.account.as_ref().unwrap().corrections().len(), 2);
    assert!(ledger.account.as_ref().unwrap().requests().is_empty());
    assert!(ledger.take_effective().unwrap().is_none());
}

#[test]
fn empty_correction_cancellation_retains_actual_record_and_count_without_retry() {
    let original = original_at(10, vec![text("bad")], vec![rejected()]);
    let mut ledger = RepairLedger::begin(original, 2).unwrap();
    request(&mut ledger, 40);
    let empty = original_at(100, vec![], vec![]);
    ledger.observe_correction(&source(10, 0), empty).unwrap();
    ledger.cancel();
    ledger
        .recorded_empty_correction_refusal(record(101))
        .unwrap();
    assert_eq!(ledger.failed_corrections(), Some(1));
    assert_eq!(ledger.intent(), RepairIntent::Cancelled);
    assert!(ledger.take_effective().unwrap().is_none());
    assert_eq!(
        ledger.recorded_empty_correction_refusal(record(102)),
        Err(RepairError::WrongPhase)
    );
    let cancelled = ledger.take_cancelled().unwrap();
    assert_eq!(cancelled.account().source(), &record(10));
    assert!(cancelled.pending().is_none());
    assert_eq!(cancelled.account().corrections().len(), 1);
    let receipt = &cancelled.account().empty_corrections()[0];
    assert_eq!(receipt.correction(), &record(100));
    assert_eq!(receipt.source(), &record(101));
    assert_eq!(receipt.failed_corrections(), 1);
}

#[test]
fn empty_original_empty_text_and_call_only_corrections_remain_distinct() {
    let mut empty_original = RepairLedger::begin(original_at(10, vec![], vec![]), 2).unwrap();
    assert!(
        empty_original
            .take_effective()
            .unwrap()
            .unwrap()
            .statements()
            .is_empty()
    );
    for (content, results, expected) in [
        (vec![text("")], vec![rejected()], CorrectionProgress::Retry),
        (
            vec![call("opaque")],
            vec![],
            CorrectionProgress::ProtocolRefusal,
        ),
        (
            vec![ContentBlock::UnrecoveredCall {
                fragment: "<partial>".into(),
            }],
            vec![],
            CorrectionProgress::ProtocolRefusal,
        ),
    ] {
        let mut ledger =
            RepairLedger::begin(original_at(10, vec![text("bad")], vec![rejected()]), 2).unwrap();
        request(&mut ledger, 40);
        assert_eq!(
            ledger
                .observe_correction(&source(10, 0), original_at(100, content, results))
                .unwrap(),
            expected
        );
        assert!(!matches!(
            ledger.intent(),
            RepairIntent::EmptyCorrectionRefusal { .. }
        ));
    }
}

#[test]
fn cancellation_recovers_a_lent_forbidden_correction_without_minting_completion() {
    let original = original_at(10, vec![text("bad")], vec![rejected()]);
    let mut ledger = RepairLedger::begin(original, 2).unwrap();
    request(&mut ledger, 40);
    let correction = original_at(
        100,
        vec![text("Read send."), call("opaque")],
        vec![accepted("Read send.")],
    );
    ledger
        .observe_correction(&source(10, 0), correction)
        .unwrap();
    ledger.recorded_protocol_refusal(record(140)).unwrap();
    let correction = ledger.take_correction_accounting().unwrap();
    ledger.cancel();
    assert!(ledger.take_cancelled().is_none());
    // The stopped owner may return before every pair is terminal. Custody still
    // returns, but no success or exhaustion capability is released.
    assert_eq!(
        ledger
            .finish_correction_accounting(correction, &[])
            .unwrap(),
        CorrectionProgress::Cancelled
    );
    assert!(ledger.take_effective().unwrap().is_none());
    assert!(ledger.take_exhausted().is_none());
    assert_eq!(ledger.failed_corrections(), Some(0));
    let cancelled = ledger.take_cancelled().unwrap();
    assert_eq!(cancelled.account().corrections().len(), 1);
    assert_eq!(cancelled.account().corrections()[0].requests().len(), 1);
    assert_eq!(
        cancelled.account().corrections()[0].generation().source(),
        &record(100)
    );
}

#[test]
fn cancellation_recovers_a_lent_exhaustion_account_without_erasing_its_pending_sends() {
    let original = original_at(
        10,
        vec![text("Read send."), call("opaque"), text("bad")],
        vec![accepted("Read send."), rejected()],
    );
    let mut ledger = RepairLedger::begin(original, 1).unwrap();
    request(&mut ledger, 40);
    ledger
        .observe_correction(
            &source(10, 2),
            original_at(100, vec![text("still bad")], vec![rejected()]),
        )
        .unwrap();
    ledger.recorded_exhausted(record(140)).unwrap();
    let account = ledger.take_exhausted_account().unwrap();
    ledger.cancel();
    assert!(ledger.take_cancelled().is_none());
    ledger.finish_exhausted_account(account, &[]).unwrap();
    assert!(ledger.take_exhausted().is_none());
    let cancelled = ledger.take_cancelled().unwrap();
    assert_eq!(cancelled.account().requests().len(), 1);
    assert_eq!(
        cancelled.account().requests()[0].statement.block_id,
        source(10, 0)
    );
    assert_eq!(cancelled.account().generations().count(), 2);
}

#[test]
fn invalid_recall_requires_its_actual_refusal_record_before_transfer() {
    let (declaration, scheme) = scheme();
    let mut result = accepted("Recall first and second.");
    let Ste100Result::Accepted { bindings } = &mut result else {
        unreachable!()
    };
    bindings[0].nouns = vec![
        NounBinding {
            value: "first".into(),
            span: span(7, 12),
        },
        NounBinding {
            value: "second".into(),
            span: span(17, 23),
        },
    ];
    let checked = checked_request_fixture_at(
        &declaration,
        vec![text("Recall first and second.")],
        vec![result],
        "run",
        "turn",
        10,
    );
    let original = OriginalRequests::from_checked(checked, &scheme).unwrap();
    let mut ledger = RepairLedger::begin(original, 2).unwrap();
    let RepairIntent::RecallRefusal(refusal) = ledger.intent() else {
        panic!("checked invalid recall must owe its exact refusal");
    };
    assert_eq!(refusal.call_id.sequence.0, 10);
    assert_eq!(refusal.call_block.0, 0);
    assert_eq!(refusal.noun_count.0, 2);
    assert_eq!(ledger.original_refusals().len(), 1);
    assert!(ledger.associations().is_empty());
    assert_eq!(
        ledger.recorded_association_refusal(0, record(40)),
        Err(RepairError::WrongPhase)
    );
    assert!(ledger.take_effective().unwrap().is_none());
    ledger.cancel();
    assert!(ledger.take_cancelled().is_none());
    ledger
        .recorded_recall_refusal(&refusal, record(40))
        .unwrap();
    assert_eq!(
        ledger.recorded_recall_refusal(&refusal, record(41)),
        Err(RepairError::WrongPhase)
    );
    assert_eq!(
        ledger.account.as_ref().unwrap().recall_refusals()[0].refusal(),
        &refusal
    );
    assert_eq!(
        ledger.account.as_ref().unwrap().recall_refusals()[0]
            .sequence()
            .0,
        40
    );
    assert_eq!(
        ledger
            .take_cancelled()
            .unwrap()
            .account()
            .original()
            .refusals()
            .len(),
        1
    );
}

#[test]
fn correction_from_another_exact_scheme_returns_owned_evidence_without_consuming_attempt() {
    let original = original_at(10, vec![text("bad")], vec![rejected()]);
    let mut ledger = RepairLedger::begin(original, 2).unwrap();
    request(&mut ledger, 40);
    let (foreign_declaration, foreign_scheme) = scheme_named("foreign-fixture/1");
    let checked = checked_request_fixture_at(
        &foreign_declaration,
        vec![text("Read fixed.")],
        vec![accepted("Read fixed.")],
        "run",
        "turn",
        100,
    );
    let correction = OriginalRequests::from_checked(checked, &foreign_scheme).unwrap();
    let (returned, error) = ledger
        .observe_correction(&source(10, 0), correction)
        .unwrap_err();
    assert_eq!(error, RepairError::WrongScheme);
    assert_eq!(returned.generation().scheme(), &foreign_declaration);
    assert_eq!(returned.generation().source(), &record(100));
    assert_eq!(ledger.failed_corrections(), Some(0));
    assert!(matches!(
        ledger.intent(),
        RepairIntent::AwaitingCorrection { .. }
    ));
}

#[test]
fn repair_custody_pending_protocol_flag_clears_at_transfer_and_empty_installation() {
    let original = original_at(10, vec![text("bad")], vec![rejected()]);
    let mut ledger = RepairLedger::begin(original, 3).unwrap();
    request(&mut ledger, 40);
    let correction = original_at(100, vec![call("unassociated")], vec![]);
    ledger
        .observe_correction(&source(10, 0), correction)
        .unwrap();
    ledger.recorded_protocol_refusal(record(140)).unwrap();
    assert!(ledger.pending_protocol_recorded);
    let correction = ledger.take_correction_accounting().unwrap();
    assert!(
        !ledger.pending_protocol_recorded,
        "transferred pending identity leaves no stale purpose bit"
    );
    ledger
        .finish_correction_accounting(correction, &[])
        .unwrap();
    let instruction = expansion("run", ledger.account.as_ref().unwrap().scheme());
    ledger
        .recorded_instruction(instruction, record(200))
        .unwrap();
    // Defensive stale-state probe: every installation owns its purpose flag,
    // even when an earlier implementation forgot the transfer cleanup.
    ledger.pending_protocol_recorded = true;
    let empty = original_at(300, vec![], vec![]);
    ledger.observe_correction(&source(10, 0), empty).unwrap();
    assert!(!ledger.pending_protocol_recorded);
    ledger.cancel();
    let pending = ledger.take_cancelled().unwrap();
    assert_eq!(
        pending.pending().unwrap().generation().source(),
        &record(300)
    );
    assert!(pending.account().empty_corrections().is_empty());
}

// The ledger accepts only its exact frontier index and actual recorded source.
// A caller cannot supply either the refusal payload or a claimed final flag.
const _: fn(&mut RepairLedger, usize, SourcePosition) -> Result<(), RepairError> =
    RepairLedger::recorded_association_refusal;

#[test]
fn repair_review_ledger_derives_ordered_association_payload_and_completion() {
    let original = original_at(
        10,
        vec![call("orphan"), text("bad"), call("linked")],
        vec![rejected()],
    );
    let mut ledger = RepairLedger::begin(original, 2).unwrap();
    request(&mut ledger, 40);
    let correction = original_at(
        100,
        vec![text("Read first. Read second.")],
        vec![accepted("Read first. Read second.")],
    );
    ledger
        .observe_correction(&source(10, 1), correction)
        .unwrap();
    assert_eq!(ledger.intent(), RepairIntent::AssociationRefusals);
    let expected = [
        RefusalRecord::ProtocolRepair {
            source_id: source(10, 0),
            case: ProtocolRepairCase::MissingToolStatement,
            detail: "tool call has no immediately preceding Text block".into(),
        },
        RefusalRecord::ProtocolRepair {
            source_id: source(10, 1),
            case: ProtocolRepairCase::AmbiguousToolStatement,
            detail: "repaired external send requires exactly one non-recall statement".into(),
        },
    ];
    assert_eq!(
        ledger.next_association_refusal(),
        Some((0, expected[0].clone()))
    );
    assert_eq!(
        ledger.recorded_association_refusal(1, record(140)),
        Err(RepairError::WrongAccounting)
    );
    assert!(
        ledger
            .account
            .as_ref()
            .unwrap()
            .association_refusals()
            .is_empty()
    );
    ledger.recorded_association_refusal(0, record(140)).unwrap();
    assert_eq!(ledger.intent(), RepairIntent::AssociationRefusals);
    assert!(
        ledger.take_effective().unwrap().is_none(),
        "one real record cannot claim the remaining frontier is final"
    );
    assert_eq!(
        ledger.next_association_refusal(),
        Some((1, expected[1].clone()))
    );
    assert_eq!(
        ledger.recorded_association_refusal(0, record(141)),
        Err(RepairError::WrongAccounting)
    );
    assert_eq!(
        ledger
            .account
            .as_ref()
            .unwrap()
            .association_refusals()
            .len(),
        1
    );
    ledger.recorded_association_refusal(1, record(141)).unwrap();
    assert!(ledger.next_association_refusal().is_none());
    assert_eq!(
        ledger.recorded_association_refusal(2, record(142)),
        Err(RepairError::WrongPhase)
    );
    let effective = ledger.take_effective().unwrap().unwrap();
    assert!(effective.requests().is_empty());
    assert_eq!(effective.account().association_refusals().len(), 2);
    for (index, receipt) in effective
        .account()
        .association_refusals()
        .iter()
        .enumerate()
    {
        assert_eq!(receipt.refusal(), &expected[index]);
        assert_eq!(receipt.source(), &record(140 + index as u64));
    }
}
