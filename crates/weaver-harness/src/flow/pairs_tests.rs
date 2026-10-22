//! Private core fixtures obtain commitments through the existing Ledger phases.
//! Only the final recorder proof is a test fixture. Real trace and member socket
//! custody is exercised by the context pair integration tests.

use super::*;
use crate::flow::RecordedTerminal;
use crate::flow::channel::{CommitEvidence, ExchangeProgress};
use crate::flow::coordinator::{RequestScheme, checked_request_fixture as checked};
use crate::flow::intake::{FlowInbox, IntakeOutcome, RetentionBudget};
use crate::flow::ledger::{Ledger, SourceDisposition};
use sha2::{Digest as _, Sha256};
use weaver_trace::Sequence;
use weaver_trace::serving::{
    AdmittedCopy, PairCopy, PathPart, SelectedBlock, SourceBlock, StatementCopy,
};
use weaver_traits::ToolCall;
use weaver_types::state_flow::*;

fn hash(bytes: &[u8]) -> Digest {
    Digest::new(format!("{:x}", Sha256::digest(bytes))).unwrap()
}

pub(super) fn fixture_scheme() -> (String, SchemeDeclaration, RequestScheme) {
    let artifact = serde_json::json!({
        "format": "ste100-scheme/1",
        "reference": {
            "edition": "fixture-read/1", "document_sha256": hash(b"reference"),
            "rule_ids": ["fixture"], "dictionary_sha256": hash(b"dictionary")
        },
        "grammar": {"format": "fixture/1", "bytes_base64": "Zml4dHVyZQ==",
                    "sha256": hash(b"fixture")},
        "verbs": [
            {"key": "read", "forms": ["Read"], "sense": "fixture read",
             "authority": {"rule_id": "fixture", "disposition": "technical_use"}},
            {"key": "recall", "forms": ["Recall"], "sense": "fixture recall",
             "authority": {"rule_id": "fixture", "disposition": "technical_use"}}
        ],
        "technical_names": [],
        "conditions": {"format": "fixture/1", "bytes_base64": "Zml4dHVyZQ==",
                       "sha256": hash(b"fixture")},
        "recall_keys": ["recall"],
        "qualification": {"report_sha256": hash(b"unqualified fixture"),
                          "corpus_sha256": hash(b"fixture corpus"),
                          "checker_abi": "ste100-span/1"}
    });
    let text = serde_json::to_string(&artifact).unwrap();
    let declaration = scheme_declaration(&text);
    let scheme = RequestScheme::from_exact_scheme(&text, &declaration).unwrap();
    (text, declaration, scheme)
}

fn scheme_declaration(text: &str) -> SchemeDeclaration {
    let digest = hash(text.as_bytes());
    SchemeDeclaration {
        scheme_id: format!("ste100:{digest}"),
        artifact_sha256: digest,
        verbs: vec!["read".into(), "recall".into()],
    }
}

pub(super) fn text(value: &str) -> ContentBlock {
    ContentBlock::Text { text: value.into() }
}

pub(super) fn call(arguments: &str) -> ContentBlock {
    ContentBlock::ToolCall(ToolCall {
        name: "sh".into(),
        arguments: arguments.into(),
    })
}

fn unrecovered() -> ContentBlock {
    ContentBlock::UnrecoveredCall {
        fragment: "<unfinished>".into(),
    }
}

fn span(start: usize, end: usize) -> Span {
    Span {
        start: (start as u64).into(),
        end: (end as u64).into(),
    }
}

pub(super) fn binding(value: &str) -> Binding {
    let verb_end = value.find(' ').unwrap();
    Binding {
        statement_span: span(0, value.len()),
        verb: value[..verb_end].to_lowercase(),
        verb_span: span(0, verb_end),
        condition: None,
        condition_span: None,
        nouns: vec![NounBinding {
            value: value[verb_end + 1..value.len() - 1].into(),
            span: span(verb_end + 1, value.len() - 1),
        }],
    }
}

fn shifted(mut binding: Binding, by: usize) -> Binding {
    for span in std::iter::once(&mut binding.statement_span)
        .chain(std::iter::once(&mut binding.verb_span))
        .chain(binding.nouns.iter_mut().map(|noun| &mut noun.span))
        .chain(binding.condition_span.iter_mut())
    {
        span.start.0 += by as u64;
        span.end.0 += by as u64;
    }
    binding
}

pub(super) fn accepted(value: &str) -> Ste100Result {
    Ste100Result::Accepted {
        bindings: vec![binding(value)],
    }
}

fn rejected() -> Ste100Result {
    Ste100Result::NotSte100 {
        code: Ste100DefectCode::NonSte100,
        detail: "fixture language defect".into(),
        spans: Vec::new(),
    }
}

pub(super) fn source(index: usize) -> SourceBlockId {
    SourceBlockId {
        run: "run".into(),
        sequence: 10.into(),
        block: (index as u64).into(),
    }
}

pub(super) fn original(
    content: Vec<ContentBlock>,
    results: Vec<Ste100Result>,
) -> (SchemeDeclaration, OriginalRequests) {
    let (_, declaration, scheme) = fixture_scheme();
    let checked = checked(&declaration, content, results);
    (
        declaration,
        OriginalRequests::from_checked(checked, &scheme).unwrap(),
    )
}

fn one() -> (PairLedger, CallId, SchemeDeclaration) {
    let (scheme, original) = original(
        vec![text("Read files."), call("ls")],
        vec![accepted("Read files.")],
    );
    let call = original.requests()[0].opening().call_id.clone();
    let mut pairs = PairLedger::new("session".into(), "run".into(), scheme.clone()).unwrap();
    pairs.accept(original).unwrap();
    (pairs, call, scheme)
}

pub(super) fn envelope(sequence: u64, kind: &str, channel: &str, origin: &str) -> Arc<str> {
    serde_json::json!({
        "session":"session", "run":"run", "turn":"turn", "sequence":sequence.to_string(),
        "kind":kind, "channel":channel, "origin":origin, "subsystem":"harness",
        "wall_ms":1, "monotonic_ns":"7", "causal_parent":"10"
    })
    .to_string()
    .into()
}

pub(super) fn event_id(sequence: u64) -> SourceBlockId {
    SourceBlockId {
        run: "run".into(),
        sequence: sequence.into(),
        block: 0.into(),
    }
}

pub(super) fn statement_receipt(
    scheme: &SchemeDeclaration,
    source: SourceBlockId,
    value: &str,
    bindings: Vec<Binding>,
) -> CommittedSource {
    let copy = AdmittedCopy::Statement(StatementCopy {
        source_envelope: envelope(source.sequence.0, "message.assistant", "output", "model"),
        partition_rule_id: "statement".into(),
        blocks: vec![SelectedBlock {
            source: SourceBlock {
                run: source.run.clone(),
                sequence: Sequence(source.sequence.0),
                block: source.block.0,
            },
            source_path: vec![
                PathPart::Key("content".into()),
                PathPart::Index(source.block.0),
            ],
            raw: serde_json::to_string(&text(value)).unwrap().into(),
        }],
        context: None,
    });
    committed(scheme, source, copy, Some((value.into(), bindings)))
}

pub(super) fn pair_receipt(
    scheme: &SchemeDeclaration,
    sequence: u64,
    record: PairRecord,
) -> CommittedSource {
    let (kind, origin, raw) = match record {
        PairRecord::Started(record) => (
            "tool.call.started",
            "model",
            serde_json::to_string(&record).unwrap(),
        ),
        PairRecord::Ended(record) => (
            "tool.call.completed",
            "loop",
            serde_json::to_string(&record).unwrap(),
        ),
    };
    let copy = AdmittedCopy::Pair(PairCopy {
        source_envelope: envelope(sequence, kind, "tool", origin),
        partition_rule_id: "pairs".into(),
        block: SelectedBlock {
            source: SourceBlock {
                run: "run".into(),
                sequence: Sequence(sequence),
                block: 0,
            },
            source_path: Vec::new(),
            raw: raw.into(),
        },
    });
    committed(scheme, event_id(sequence), copy, None)
}

pub(super) fn committed(
    scheme: &SchemeDeclaration,
    source: SourceBlockId,
    copy: AdmittedCopy,
    statement: Option<(String, Vec<Binding>)>,
) -> CommittedSource {
    let mut ledger = Ledger::new("run".into(), "session".into(), scheme.clone()).unwrap();
    let mut budget = RetentionBudget::new(FlowBacklog {
        max_pending_blocks: 20.into(),
        max_pending_bytes: 50000.into(),
        lag_alarm_turns: 2.into(),
        measurement_ref: "fixture".into(),
    })
    .unwrap();
    let mut inbox = FlowInbox::new();
    assert!(matches!(
        inbox.accept(copy, &mut budget),
        IntakeOutcome::Accepted
    ));
    ledger
        .install(inbox.pop_front(&budget).unwrap(), 10, None)
        .unwrap();
    let lease = ledger.register_source_lease(vec![source.clone()]).unwrap();
    if let Some((text, bindings)) = statement {
        let request = Ste100Request {
            request_id: 1.into(),
            target: Ste100Target::Source(source.clone()),
            text,
            scheme_id: scheme.scheme_id.clone(),
        };
        ledger.classification_submitted(&source, &request).unwrap();
        ledger
            .classify_recorded(&Ste100Reply {
                request_id: request.request_id,
                target: request.target,
                scheme_id: request.scheme_id,
                result: Ste100Result::Accepted { bindings },
            })
            .unwrap();
    }
    let operation = match ledger.next_file().unwrap() {
        LedgerFile::Statement(_) => StateOperation::File,
        LedgerFile::Pair(_) => StateOperation::FilePair,
    };
    let common = Common {
        session_id: "session".into(),
        run_id: "run".into(),
        request_id: 2.into(),
    };
    ledger.begin_file(&source, &common).unwrap();
    ledger
        .observe_progress(
            &ExchangeProgress {
                common: common.clone(),
                operation,
                source: Some(source.clone()),
                bytes_sent: 100,
                frame_bytes: 100,
                commit: CommitEvidence::Committed,
            },
            Some(11),
        )
        .unwrap();
    ledger
        .receipt_recorded(&StateFrame::Filed {
            common,
            block_id: source.clone(),
            observation: StateOperationObservation {
                operation_id: 2.into(),
                parent_id: None,
                request_id: 2.into(),
                operation,
                began_ns: 15.into(),
                ended_ns: 20.into(),
                phases: vec![
                    OperationPhase {
                        name: PhaseName::Validate,
                        began_ns: 15.into(),
                        ended_ns: 16.into(),
                    },
                    OperationPhase {
                        name: PhaseName::Encode,
                        began_ns: 19.into(),
                        ended_ns: 20.into(),
                    },
                ],
                resource: OperationResource {
                    cpu_ns: None,
                    copied_bytes: Some(1.into()),
                    held_bytes: Some(2.into()),
                    unavailable: vec![ResourceUnavailable {
                        field: ResourceField::CpuNs,
                        reason: ObservationUnavailable::NotSampled,
                    }],
                },
                disk: OperationDisk {
                    enabled: None,
                    before: None,
                    after: None,
                    unavailable_reason: Some(ObservationUnavailable::NotSampled),
                },
            },
        })
        .unwrap();
    ledger
        .terminal_recorded(RecordedTerminal::for_test(source.clone()), &mut budget)
        .unwrap();
    let Some(SourceDisposition::Committed(receipt)) =
        ledger.take_source_receipt(&lease, &source).unwrap()
    else {
        panic!("real ledger committed-source transition")
    };
    receipt
}

pub(super) fn open(
    pairs: &mut PairLedger,
    call: &CallId,
    scheme: &SchemeDeclaration,
    sequence: u64,
) {
    let started = pairs.next_opening().unwrap().1.clone();
    assert_eq!(&started.call_id, call);
    pairs
        .mark_authoring(call, PairRecordPhase::Opening)
        .unwrap();
    pairs
        .mark_recorded(call, PairRecordPhase::Opening, event_id(sequence))
        .unwrap();
    pairs
        .install_opening(
            call,
            pair_receipt(scheme, sequence, PairRecord::Started(started)),
        )
        .unwrap();
}

fn close(pairs: &mut PairLedger, call: &CallId, scheme: &SchemeDeclaration, sequence: u64) {
    let ended = pairs.next_terminal().unwrap().1.clone();
    assert_eq!(&ended.call_id, call);
    pairs
        .mark_authoring(call, PairRecordPhase::Terminal)
        .unwrap();
    pairs
        .mark_recorded(call, PairRecordPhase::Terminal, event_id(sequence))
        .unwrap();
    pairs
        .install_terminal(
            call,
            pair_receipt(scheme, sequence, PairRecord::Ended(ended)),
        )
        .unwrap();
}

#[test]
fn pair_completion_requires_statement_opening_and_exact_terminal_commits() {
    let (mut pairs, call, scheme) = one();
    pairs
        .set_not_dispatched(NonDispatchCause::PassPurpose)
        .unwrap();
    assert_eq!(pairs.next_statement(), Some(&source(0)));
    assert!(pairs.completed(&call).unwrap().is_none());
    assert!(pairs.next_terminal().is_none());
    assert!(pairs.outstanding().is_empty());
    open(&mut pairs, &call, &scheme, 30);
    assert_eq!(pairs.outstanding()[0].state, PairState::Opening);
    assert!(
        pairs.next_terminal().is_none(),
        "opening cannot replace the statement receipt"
    );
    let statement = statement_receipt(
        &scheme,
        source(0),
        "Read files.",
        vec![binding("Read files.")],
    );
    let LedgerFile::Statement(file) = statement.file() else {
        panic!("statement")
    };
    let statement_ptr = file.raw_block.get().as_ptr();
    pairs.install_statement(statement).unwrap();
    assert!(pairs.next_statement().is_none());
    assert_eq!(pairs.outstanding()[0].state, PairState::Opened);
    let ended = pairs.next_terminal().unwrap().1.clone();
    pairs
        .mark_authoring(&call, PairRecordPhase::Terminal)
        .unwrap();
    assert_eq!(pairs.outstanding()[0].state, PairState::Ending);
    assert!(pairs.next_terminal().is_none());
    let receipt = pair_receipt(&scheme, 31, PairRecord::Ended(ended.clone()));
    let (receipt, error) = pairs.install_terminal(&call, receipt).unwrap_err();
    assert_eq!(error, PairError::WrongReceipt);
    assert!(pairs.completed(&call).unwrap().is_none());
    pairs
        .mark_recorded(&call, PairRecordPhase::Terminal, event_id(31))
        .unwrap();
    pairs.install_terminal(&call, receipt).unwrap();
    let pair = pairs.completed(&call).unwrap().unwrap();
    assert_eq!(
        pair.started().request,
        PairRequest::External {
            name: "sh".into(),
            arguments: "ls".into()
        }
    );
    assert_eq!(pair.ended(), &ended);
    assert_eq!(pair.terminal_source(), &event_id(31));
    assert_eq!(pair.receipt().block_id(), &event_id(31));
    assert_eq!(pair.receipt().request_id().0, 2);
    assert_eq!(pair.statement().source(), &source(0));
    let LedgerFile::Statement(file) = pair.statement().file() else {
        panic!("statement")
    };
    assert_eq!(file.raw_block.get().as_ptr(), statement_ptr);
    assert_eq!(pair.opening().source(), &event_id(30));
    assert!(pair.ended().execution.is_none() && pair.ended().outcome.is_none());
    assert!(pairs.outstanding().is_empty());
    assert!(pairs.is_complete());
    let original = pairs.finish_batch().unwrap();
    assert_eq!(original.requests().len(), 1);
    assert!(!pairs.is_complete());
    assert!(pairs.completed(&call).unwrap().is_some());
}

#[test]
fn a_shared_recall_statement_has_one_receipt_for_several_closed_pairs() {
    let value = "Recall files. Recall logs.";
    let bindings = vec![
        binding("Recall files."),
        shifted(binding("Recall logs."), 14),
    ];
    let (scheme, original) = original(
        vec![text(value)],
        vec![Ste100Result::Accepted {
            bindings: bindings.clone(),
        }],
    );
    let calls: Vec<_> = original
        .requests()
        .iter()
        .map(|request| request.opening().call_id.clone())
        .collect();
    assert_eq!(calls.len(), 2);
    let mut pairs = PairLedger::new("session".into(), "run".into(), scheme.clone()).unwrap();
    pairs.accept(original).unwrap();
    pairs
        .set_not_dispatched(NonDispatchCause::RepairExhausted)
        .unwrap();
    pairs
        .install_statement(statement_receipt(
            &scheme,
            source(0),
            value,
            bindings.clone(),
        ))
        .unwrap();
    assert!(pairs.next_statement().is_none());
    assert_eq!(
        pairs
            .install_statement(statement_receipt(&scheme, source(0), value, bindings))
            .unwrap_err()
            .1,
        PairError::DuplicateSource
    );
    for (i, call) in calls.iter().enumerate() {
        open(&mut pairs, call, &scheme, 30 + i as u64);
    }
    close(&mut pairs, &calls[0], &scheme, 32);
    close(&mut pairs, &calls[1], &scheme, 33);
    pairs.finish_batch().unwrap();
    let first = pairs.completed(&calls[0]).unwrap().unwrap();
    let second = pairs.completed(&calls[1]).unwrap().unwrap();
    assert!(std::ptr::eq(first.statement(), second.statement()));
    assert_eq!(first.started().statement.binding.0, 0);
    assert_eq!(second.started().statement.binding.0, 1);
}

#[test]
fn accounting_returns_the_original_and_unresolved_requests_for_repair() {
    let (scheme, original) = original(
        vec![
            text("Read files."),
            call("ls"),
            text("broken"),
            call("pwd"),
            unrecovered(),
        ],
        vec![accepted("Read files."), rejected()],
    );
    assert_eq!(original.unresolved().len(), 1);
    let call = original.requests()[0].opening().call_id.clone();
    let mut pairs = PairLedger::new("session".into(), "run".into(), scheme.clone()).unwrap();
    pairs.accept(original).unwrap();
    pairs
        .set_not_dispatched(NonDispatchCause::PassPurpose)
        .unwrap();
    pairs
        .install_statement(statement_receipt(
            &scheme,
            source(0),
            "Read files.",
            vec![binding("Read files.")],
        ))
        .unwrap();
    open(&mut pairs, &call, &scheme, 30);
    close(&mut pairs, &call, &scheme, 31);
    let original = pairs.finish_batch().unwrap();
    assert_eq!(original.generation().original().message.content.len(), 5);
    assert_eq!(
        original.unresolved()[0].request(),
        &PairRequest::External {
            name: "sh".into(),
            arguments: "pwd".into()
        }
    );
    assert_eq!(original.unresolved()[0].rejected_source(), &source(2));
    assert!(original.attempted_call());
}

#[test]
fn stop_and_unload_override_only_unclaimed_terminal_intent() {
    let (mut pairs, call, scheme) = one();
    pairs
        .set_not_dispatched(NonDispatchCause::PassPurpose)
        .unwrap();
    pairs.set_not_dispatched(NonDispatchCause::Stop).unwrap();
    pairs
        .set_not_dispatched(NonDispatchCause::RoundLimit)
        .unwrap();
    pairs
        .install_statement(statement_receipt(
            &scheme,
            source(0),
            "Read files.",
            vec![binding("Read files.")],
        ))
        .unwrap();
    open(&mut pairs, &call, &scheme, 30);
    assert_eq!(
        pairs.next_terminal().unwrap().1.reason,
        Some(PairFailure::NotDispatched {
            cause: NonDispatchCause::Stop
        })
    );
    pairs
        .mark_authoring(&call, PairRecordPhase::Terminal)
        .unwrap();
    pairs.set_not_dispatched(NonDispatchCause::Unload).unwrap();
    let ending = pairs.entry(&call).unwrap().ending.clone().unwrap();
    assert_eq!(
        ending.reason,
        Some(PairFailure::NotDispatched {
            cause: NonDispatchCause::Stop
        })
    );
    pairs
        .mark_recorded(&call, PairRecordPhase::Terminal, event_id(31))
        .unwrap();
    pairs
        .install_terminal(&call, pair_receipt(&scheme, 31, PairRecord::Ended(ending)))
        .unwrap();
    assert_eq!(
        dominant(Some(NonDispatchCause::Stop), NonDispatchCause::Unload),
        NonDispatchCause::Unload
    );
    assert_eq!(
        dominant(Some(NonDispatchCause::Unload), NonDispatchCause::Stop),
        NonDispatchCause::Unload
    );
}

#[test]
fn author_claim_and_record_identity_cannot_be_repeated_or_invented() {
    let (mut pairs, call, _) = one();
    assert_eq!(
        pairs.mark_recorded(&call, PairRecordPhase::Opening, event_id(30)),
        Err(PairError::WrongPhase)
    );
    assert_eq!(
        pairs.mark_authoring(&call, PairRecordPhase::Terminal),
        Err(PairError::WrongPhase)
    );
    pairs
        .mark_authoring(&call, PairRecordPhase::Opening)
        .unwrap();
    assert!(pairs.next_opening().is_none());
    assert_eq!(
        pairs.mark_authoring(&call, PairRecordPhase::Opening),
        Err(PairError::WrongPhase)
    );
    for bad in [
        source(0),
        SourceBlockId {
            run: "other".into(),
            ..event_id(30)
        },
        SourceBlockId {
            block: 1.into(),
            ..event_id(30)
        },
    ] {
        assert_eq!(
            pairs.mark_recorded(&call, PairRecordPhase::Opening, bad),
            Err(PairError::InvalidIdentity)
        );
    }
    pairs
        .mark_recorded(&call, PairRecordPhase::Opening, event_id(30))
        .unwrap();
    assert_eq!(
        pairs.mark_recorded(&call, PairRecordPhase::Opening, event_id(30)),
        Err(PairError::DuplicateSource)
    );
    assert_eq!(
        pairs.mark_recorded(&call, PairRecordPhase::Opening, event_id(31)),
        Err(PairError::WrongPhase)
    );
    assert_eq!(pairs.outstanding()[0].opening_source, event_id(30));
}

#[test]
fn different_statement_bytes_and_pair_records_return_the_owned_receipt() {
    let (mut pairs, call, scheme) = one();
    let wrong = statement_receipt(
        &scheme,
        source(0),
        "Read logs.",
        vec![binding("Read logs.")],
    );
    let (wrong, error) = pairs.install_statement(wrong).unwrap_err();
    assert_eq!(error, PairError::WrongStatement);
    assert_eq!(wrong.source(), &source(0));
    let mut started = pairs.next_opening().unwrap().1.clone();
    pairs
        .mark_authoring(&call, PairRecordPhase::Opening)
        .unwrap();
    pairs
        .mark_recorded(&call, PairRecordPhase::Opening, event_id(30))
        .unwrap();
    started.request = PairRequest::External {
        name: "sh".into(),
        arguments: "different".into(),
    };
    let (wrong, error) = pairs
        .install_opening(
            &call,
            pair_receipt(&scheme, 30, PairRecord::Started(started)),
        )
        .unwrap_err();
    assert_eq!(error, PairError::WrongReceipt);
    assert_eq!(wrong.source(), &event_id(30));
    assert!(!pairs.outstanding()[0].opening_filed);
}

#[test]
fn terminal_cause_must_match_the_exact_record_that_was_claimed() {
    let (mut pairs, call, scheme) = one();
    pairs
        .set_not_dispatched(NonDispatchCause::PlanBreak)
        .unwrap();
    pairs
        .install_statement(statement_receipt(
            &scheme,
            source(0),
            "Read files.",
            vec![binding("Read files.")],
        ))
        .unwrap();
    open(&mut pairs, &call, &scheme, 30);
    let mut ended = pairs.next_terminal().unwrap().1.clone();
    pairs
        .mark_authoring(&call, PairRecordPhase::Terminal)
        .unwrap();
    pairs
        .mark_recorded(&call, PairRecordPhase::Terminal, event_id(31))
        .unwrap();
    ended.reason = Some(PairFailure::NotDispatched {
        cause: NonDispatchCause::Stop,
    });
    let (receipt, error) = pairs
        .install_terminal(&call, pair_receipt(&scheme, 31, PairRecord::Ended(ended)))
        .unwrap_err();
    assert_eq!(error, PairError::WrongReceipt);
    assert_eq!(receipt.source(), &event_id(31));
    assert!(pairs.completed(&call).unwrap().is_none());
    assert!(pairs.finish_batch().is_err());
}

#[test]
fn freeze_keeps_pending_evidence_and_refuses_every_advance_or_grant() {
    let (mut pairs, call, scheme) = one();
    open(&mut pairs, &call, &scheme, 30);
    pairs.freeze();
    assert_eq!(
        pairs.set_not_dispatched(NonDispatchCause::Stop),
        Err(PairError::Frozen)
    );
    assert_eq!(
        pairs.mark_authoring(&call, PairRecordPhase::Terminal),
        Err(PairError::Frozen)
    );
    assert_eq!(
        pairs.mark_recorded(&call, PairRecordPhase::Terminal, event_id(31)),
        Err(PairError::Frozen)
    );
    assert!(
        pairs.next_opening().is_none()
            && pairs.next_terminal().is_none()
            && pairs.next_statement().is_none()
    );
    assert_eq!(pairs.completed(&call).unwrap_err(), PairError::Frozen);
    assert_eq!(pairs.finish_batch().unwrap_err(), PairError::Frozen);
    assert_eq!(
        pairs
            .install_statement(statement_receipt(
                &scheme,
                source(0),
                "Read files.",
                vec![binding("Read files.")]
            ))
            .unwrap_err()
            .1,
        PairError::Frozen
    );
    assert_eq!(pairs.outstanding()[0].opening_source, event_id(30));
    assert!(pairs.original().is_some());
}

#[test]
fn empty_batches_preserve_original_evidence_and_busy_refusal_returns_custody() {
    let (scheme, original) = original(vec![text("broken")], vec![rejected()]);
    let mut pairs = PairLedger::new("session".into(), "run".into(), scheme).unwrap();
    pairs.accept(original).unwrap();
    let (_, other) = original_fixture();
    let (other, error) = pairs.accept(other).unwrap_err();
    assert_eq!(error, PairError::Busy);
    assert_eq!(other.requests().len(), 1);
    assert!(pairs.is_complete());
    assert_eq!(
        pairs
            .finish_batch()
            .unwrap()
            .generation()
            .original()
            .message
            .content
            .len(),
        1
    );
}

fn original_fixture() -> (SchemeDeclaration, OriginalRequests) {
    original(
        vec![text("Read files."), call("ls")],
        vec![accepted("Read files.")],
    )
}

#[test]
fn completed_call_identity_cannot_be_registered_again() {
    let (mut pairs, call, scheme) = one();
    pairs.set_not_dispatched(NonDispatchCause::NoGate).unwrap();
    pairs
        .install_statement(statement_receipt(
            &scheme,
            source(0),
            "Read files.",
            vec![binding("Read files.")],
        ))
        .unwrap();
    open(&mut pairs, &call, &scheme, 30);
    close(&mut pairs, &call, &scheme, 31);
    pairs.finish_batch().unwrap();
    let (_, original) = original_fixture();
    let (_, error) = pairs.accept(original).unwrap_err();
    assert_eq!(error, PairError::DuplicateCall);
}

#[test]
fn completed_pair_and_filed_receipt_are_not_cloneable_or_default_constructible() {
    trait NotClone<Marker> {
        fn check() {}
    }
    impl<T: ?Sized> NotClone<()> for T {}
    impl<T: Clone> NotClone<u8> for T {}
    let _ = <CompletedPair as NotClone<_>>::check;
    let _ = <FiledReceipt as NotClone<_>>::check;
    trait NotDefault<Marker> {
        fn check() {}
    }
    impl<T: ?Sized> NotDefault<()> for T {}
    impl<T: Default> NotDefault<u8> for T {}
    let _ = <CompletedPair as NotDefault<_>>::check;
    let _ = <FiledReceipt as NotDefault<_>>::check;
}

#[test]
fn batch_identity_and_exact_scheme_are_checked_before_custody_moves() {
    let (scheme, original) = original_fixture();
    let mut other_run = PairLedger::new("session".into(), "other".into(), scheme.clone()).unwrap();
    let (original, error) = other_run.accept(original).unwrap_err();
    assert_eq!(error, PairError::InvalidIdentity);
    let mut other_scheme = scheme;
    other_scheme.verbs.push("other".into());
    let mut other = PairLedger::new("session".into(), "run".into(), other_scheme).unwrap();
    let (original, error) = other.accept(original).unwrap_err();
    assert_eq!(error, PairError::WrongScheme);
    assert_eq!(original.requests().len(), 1);
    assert!(other.original().is_none());
}

#[test]
fn a_terminal_cannot_precede_its_opening_or_reuse_the_opening_source() {
    let (mut pairs, call, scheme) = one();
    pairs.set_not_dispatched(NonDispatchCause::Stop).unwrap();
    pairs
        .install_statement(statement_receipt(
            &scheme,
            source(0),
            "Read files.",
            vec![binding("Read files.")],
        ))
        .unwrap();
    open(&mut pairs, &call, &scheme, 30);
    pairs
        .mark_authoring(&call, PairRecordPhase::Terminal)
        .unwrap();
    assert_eq!(
        pairs.mark_recorded(&call, PairRecordPhase::Terminal, event_id(29)),
        Err(PairError::WrongPhase)
    );
    assert_eq!(
        pairs.mark_recorded(&call, PairRecordPhase::Terminal, event_id(30)),
        Err(PairError::DuplicateSource)
    );
    assert!(pairs.outstanding()[0].terminal_source.is_none());
    pairs
        .mark_recorded(&call, PairRecordPhase::Terminal, event_id(31))
        .unwrap();
}

fn repaired_account() -> (SchemeDeclaration, RepairAccount) {
    use crate::flow::coordinator::checked_request_fixture_at;
    let (_, declaration, scheme) = fixture_scheme();
    let original = OriginalRequests::from_checked(
        checked_request_fixture_at(
            &declaration,
            vec![
                text("broken"),
                call("original opaque bytes"),
                text("Read intact."),
                call("intact opaque bytes"),
                text("still broken"),
            ],
            vec![rejected(), accepted("Read intact."), rejected()],
            "run",
            "turn",
            10,
        ),
        &scheme,
    )
    .unwrap();
    let correction = OriginalRequests::from_checked(
        checked_request_fixture_at(
            &declaration,
            vec![text("Read corrected.")],
            vec![accepted("Read corrected.")],
            "run",
            "turn",
            30,
        ),
        &scheme,
    )
    .unwrap();
    let account = super::super::repair::account_fixture(original, vec![(source(0), correction)]);
    (declaration, account)
}

#[test]
fn repaired_original_requests_use_corrected_statements_and_preserve_original_execution_bytes() {
    let (scheme, account) = repaired_account();
    let starts = account.requests().to_vec();
    assert_eq!(starts.len(), 2);
    assert_eq!(starts[0].call_id.sequence.0, 10);
    assert_eq!(starts[0].call_block.0, 1);
    assert_eq!(starts[0].statement.block_id, event_id(30));
    assert_eq!(
        starts[0].request,
        PairRequest::External {
            name: "sh".into(),
            arguments: "original opaque bytes".into(),
        }
    );
    let mut pairs = PairLedger::new("session".into(), "run".into(), scheme.clone()).unwrap();
    pairs.accept_repair(account).unwrap();
    pairs
        .set_not_dispatched(NonDispatchCause::RepairExhausted)
        .unwrap();
    assert_eq!(pairs.finish_batch().unwrap_err(), PairError::WrongPhase);
    open(&mut pairs, &starts[0].call_id, &scheme, 50);
    open(&mut pairs, &starts[1].call_id, &scheme, 51);
    assert!(pairs.next_terminal().is_none());
    pairs
        .install_statement(statement_receipt(
            &scheme,
            event_id(30),
            "Read corrected.",
            vec![binding("Read corrected.")],
        ))
        .unwrap();
    close(&mut pairs, &starts[0].call_id, &scheme, 60);
    assert_eq!(pairs.next_statement(), Some(&source(2)));
    pairs
        .install_statement(statement_receipt(
            &scheme,
            source(2),
            "Read intact.",
            vec![binding("Read intact.")],
        ))
        .unwrap();
    close(&mut pairs, &starts[1].call_id, &scheme, 61);
    let account = pairs.finish_repair_batch().unwrap();
    assert_eq!(account.requests(), starts);
    assert_eq!(account.generations().count(), 2);
    assert_eq!(account.original().unresolved().len(), 1);
    let completed = pairs.completed(&starts[0].call_id).unwrap().unwrap();
    assert_eq!(completed.statement().source(), &event_id(30));
    assert_eq!(
        completed.ended().reason,
        Some(PairFailure::NotDispatched {
            cause: NonDispatchCause::RepairExhausted,
        })
    );
    // Returning the private repair account preserves its evidence, not a second
    // right to open the same original sends after they have reached terminals.
    let (account, error) = pairs.accept_repair(account).unwrap_err();
    assert_eq!(error, PairError::DuplicateCall);
    assert_eq!(account.requests(), starts);
    assert_eq!(account.generations().count(), 2);
    assert!(pairs.next_opening().is_none());
    assert!(pairs.completed(&starts[0].call_id).unwrap().is_some());
}

#[test]
fn repaired_receipt_requires_its_exact_corrected_source_bytes_and_author_chronology() {
    let (scheme, account) = repaired_account();
    let started = account.requests()[0].clone();
    let mut pairs = PairLedger::new("session".into(), "run".into(), scheme.clone()).unwrap();
    pairs.accept_repair(account).unwrap();
    pairs.set_not_dispatched(NonDispatchCause::Stop).unwrap();
    let (refused, error) = pairs
        .install_statement(statement_receipt(
            &scheme,
            source(0),
            "Read corrected.",
            vec![binding("Read corrected.")],
        ))
        .unwrap_err();
    assert_eq!(error, PairError::WrongStatement);
    assert_eq!(refused.source(), &source(0));
    let (refused, error) = pairs
        .install_statement(statement_receipt(
            &scheme,
            event_id(30),
            "Read changed.",
            vec![binding("Read changed.")],
        ))
        .unwrap_err();
    assert_eq!(error, PairError::WrongStatement);
    assert_eq!(refused.source(), &event_id(30));
    pairs
        .mark_authoring(&started.call_id, PairRecordPhase::Opening)
        .unwrap();
    assert_eq!(
        pairs.mark_recorded(&started.call_id, PairRecordPhase::Opening, event_id(29)),
        Err(PairError::InvalidIdentity)
    );
    assert!(pairs.outstanding().is_empty());
    pairs
        .mark_recorded(&started.call_id, PairRecordPhase::Opening, event_id(50))
        .unwrap();
    assert_eq!(pairs.outstanding()[0].opening_source, event_id(50));
}

#[test]
fn repaired_batch_keeps_its_owner_on_busy_wrong_take_and_freeze() {
    let (scheme, account) = repaired_account();
    let mut pairs = PairLedger::new("session".into(), "run".into(), scheme).unwrap();
    pairs.accept_repair(account).unwrap();
    let (_, second) = repaired_account();
    let (second, error) = pairs.accept_repair(second).unwrap_err();
    assert_eq!(error, PairError::Busy);
    assert_eq!(second.generations().count(), 2);
    assert_eq!(pairs.finish_batch().unwrap_err(), PairError::WrongPhase);
    pairs.freeze();
    assert_eq!(pairs.finish_repair_batch().unwrap_err(), PairError::Frozen);
    assert_eq!(
        pairs
            .original()
            .unwrap()
            .generation()
            .source()
            .block_id
            .sequence
            .0,
        10
    );
}
