//! Private-owner fixtures use OutputChecks' validated completion path. The real
//! recorder/socket proof is exercised by the output transaction integration tests.

use super::*;
use crate::flow::coordinator::{OutputChecks, OutputOriginal};
use weaver_trace::{Sequence, TurnRef};
use weaver_traits::{Message, ToolCall};
use weaver_types::Finish;
use weaver_types::state_flow::{
    Binding, NounBinding, SourcePosition, Span, Ste100DefectCode, Ste100Reply, Ste100Request,
};

const _: fn() = || {
    trait NotClone<Marker> {
        fn check() {}
    }
    impl<T: ?Sized> NotClone<()> for T {}
    impl<T: Clone> NotClone<u8> for T {}
    let _ = <OriginalRequests as NotClone<_>>::check;
    let _ = <ModelRequestCandidate as NotClone<_>>::check;
    let _ = <PendingAssociation as NotClone<_>>::check;
    trait NotDefault<Marker> {
        fn check() {}
    }
    impl<T: ?Sized> NotDefault<()> for T {}
    impl<T: Default> NotDefault<u8> for T {}
    let _ = <OriginalRequests as NotDefault<_>>::check;
    let _ = <ModelRequestCandidate as NotDefault<_>>::check;
};

fn fixture_scheme() -> (String, SchemeDeclaration, RequestScheme) {
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

fn text(value: &str) -> ContentBlock {
    ContentBlock::Text { text: value.into() }
}

fn call(arguments: &str) -> ContentBlock {
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

fn binding(value: &str) -> Binding {
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

fn accepted(value: &str) -> Ste100Result {
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

pub(in crate::flow) fn checked(
    declaration: &SchemeDeclaration,
    content: Vec<ContentBlock>,
    results: Vec<Ste100Result>,
) -> CheckedGeneration {
    checked_at(declaration, content, results, "run", "turn", 10)
}

pub(in crate::flow) fn checked_at(
    declaration: &SchemeDeclaration,
    content: Vec<ContentBlock>,
    results: Vec<Ste100Result>,
    run: &str,
    turn: &str,
    sequence: u64,
) -> CheckedGeneration {
    let source = |index: usize| SourceBlockId {
        run: run.into(),
        sequence: sequence.into(),
        block: (index as u64).into(),
    };
    let indices: Vec<_> = content
        .iter()
        .enumerate()
        .filter_map(|(index, block)| matches!(block, ContentBlock::Text { .. }).then_some(index))
        .collect();
    assert_eq!(indices.len(), results.len());
    let original = OutputOriginal {
        message: Message {
            role: Role::Assistant,
            content,
        },
        finish: Finish::Stopped,
        turn: TurnRef(turn.into()),
    };
    let mut owner = OutputChecks::new(run, declaration, original).unwrap();
    owner
        .bind(
            Sequence(sequence),
            100,
            &indices
                .iter()
                .map(|index| source(*index))
                .collect::<Vec<_>>(),
        )
        .unwrap();
    for (ordinal, (index, result)) in indices.into_iter().zip(results).enumerate() {
        let ContentBlock::Text { text } = &owner.original().message.content[index] else {
            panic!("fixture Text")
        };
        let request = Ste100Request {
            request_id: (ordinal as u64 + 1).into(),
            target: Ste100Target::Source(source(index)),
            text: text.clone(),
            scheme_id: declaration.scheme_id.clone(),
        };
        let reply = Ste100Reply {
            request_id: request.request_id,
            target: request.target.clone(),
            scheme_id: request.scheme_id.clone(),
            result,
        };
        owner
            .record(
                request,
                reply,
                SourcePosition {
                    block_id: SourceBlockId {
                        run: run.into(),
                        sequence: (sequence + 10 + ordinal as u64).into(),
                        block: 0.into(),
                    },
                    turn: Some(turn.into()),
                },
                110 + ordinal as u64,
                true,
            )
            .unwrap();
    }
    owner.complete().unwrap()
}

fn source(index: usize) -> SourceBlockId {
    SourceBlockId {
        run: "run".into(),
        sequence: 10.into(),
        block: (index as u64).into(),
    }
}

#[test]
fn external_request_keeps_opaque_bytes_source_and_exact_statement_dependency() {
    let (_, declaration, scheme) = fixture_scheme();
    let arguments = "printf '%s\\n' 'opaque { not STE100 }'\n";
    let generation = checked(
        &declaration,
        vec![text("Read files."), call(arguments)],
        vec![accepted("Read files.")],
    );
    let requests = OriginalRequests::from_checked(generation, &scheme).unwrap();
    assert!(requests.attempted_call());
    assert!(requests.refusals().is_empty());
    assert!(requests.unresolved().is_empty());
    let candidate = &requests.requests()[0];
    assert_eq!(
        candidate.opening(),
        &PairStarted {
            call_id: CallId {
                run: "run".into(),
                sequence: 10.into(),
                call: 0.into()
            },
            call_block: 1.into(),
            statement: StatementRef {
                block_id: source(0),
                binding: 0.into()
            },
            lane: PairLane::External,
            request: PairRequest::External {
                name: "sh".into(),
                arguments: arguments.into()
            },
            retry_of: None,
        }
    );
    assert_eq!(
        requests.dependencies().collect::<Vec<_>>(),
        vec![candidate.dependency()]
    );
    assert_eq!(requests.generation().original().finish, Finish::Stopped);
}

#[test]
fn repeated_equal_requests_keep_distinct_recovered_ordinals_and_sources() {
    let (_, declaration, scheme) = fixture_scheme();
    let generation = checked(
        &declaration,
        vec![
            text("Read files."),
            call("same"),
            text("Read files."),
            call("same"),
        ],
        vec![accepted("Read files."), accepted("Read files.")],
    );
    let requests = OriginalRequests::from_checked(generation, &scheme).unwrap();
    assert_eq!(
        requests
            .requests()
            .iter()
            .map(|x| x.opening().call_id.call.0)
            .collect::<Vec<_>>(),
        vec![0, 1]
    );
    assert_eq!(requests.requests()[0].dependency().block_id, source(0));
    assert_eq!(requests.requests()[1].dependency().block_id, source(2));
    assert_eq!(
        requests.requests()[0].opening().request,
        requests.requests()[1].opening().request
    );
}

#[test]
fn recalls_number_after_all_external_calls_but_dispatch_order_is_source_order() {
    let (_, declaration, scheme) = fixture_scheme();
    let generation = checked(
        &declaration,
        vec![
            text("Recall subject."),
            text("Read files."),
            call("command"),
            text("Recall second."),
        ],
        vec![
            accepted("Recall subject."),
            accepted("Read files."),
            accepted("Recall second."),
        ],
    );
    let requests = OriginalRequests::from_checked(generation, &scheme).unwrap();
    assert_eq!(
        requests
            .requests()
            .iter()
            .map(|x| (
                x.opening().call_id.call.0,
                x.opening().call_block.0,
                x.opening().lane
            ))
            .collect::<Vec<_>>(),
        vec![
            (1, 0, PairLane::Recall),
            (0, 2, PairLane::External),
            (2, 3, PairLane::Recall)
        ]
    );
    assert_eq!(
        requests.requests()[0].opening().request,
        PairRequest::Recall {
            subject: "subject".into()
        }
    );
}

#[test]
fn recall_binding_ordinals_include_ordinary_siblings_and_keep_exact_subjects() {
    let (_, declaration, scheme) = fixture_scheme();
    let one = "Read files.";
    let two = "Recall MixedCase.";
    let three = "Recall other.";
    let value = format!("{one} {two} {three}");
    let generation = checked(
        &declaration,
        vec![text(&value)],
        vec![Ste100Result::Accepted {
            bindings: vec![
                binding(one),
                shifted(binding(two), one.len() + 1),
                shifted(binding(three), one.len() + two.len() + 2),
            ],
        }],
    );
    let requests = OriginalRequests::from_checked(generation, &scheme).unwrap();
    assert_eq!(
        requests
            .requests()
            .iter()
            .map(|x| x.dependency().binding.0)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert_eq!(
        requests.requests()[0].opening().request,
        PairRequest::Recall {
            subject: "MixedCase".into()
        }
    );
    assert_eq!(requests.requests()[1].opening().call_id.call.0, 1);
}

#[test]
fn unrecovered_blocks_never_consume_call_ids_and_bare_calls_are_not_guessed() {
    let (_, declaration, scheme) = fixture_scheme();
    let generation = checked(
        &declaration,
        vec![
            unrecovered(),
            call("bare"),
            text("Read files."),
            call("valid"),
            text("Recall item."),
        ],
        vec![accepted("Read files."), accepted("Recall item.")],
    );
    let requests = OriginalRequests::from_checked(generation, &scheme).unwrap();
    assert!(
        matches!(&requests.refusals()[0], AssociationRefusal::External {
        call_id, call_block: 1, source: id, case: ProtocolRepairCase::MissingToolStatement, ..
    } if call_id.call.0 == 0 && *id == source(1))
    );
    assert_eq!(
        requests
            .requests()
            .iter()
            .map(|x| x.opening().call_id.call.0)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert_eq!(requests.requests()[0].opening().call_block.0, 3);
}

#[test]
fn rejected_adjacent_text_stays_unresolved_with_original_call_bytes() {
    let (_, declaration, scheme) = fixture_scheme();
    let generation = checked(
        &declaration,
        vec![text("bad statement"), call("opaque original")],
        vec![rejected()],
    );
    let requests = OriginalRequests::from_checked(generation, &scheme).unwrap();
    assert!(requests.requests().is_empty());
    assert!(requests.refusals().is_empty());
    assert!(requests.attempted_call());
    let unresolved = &requests.unresolved()[0];
    assert_eq!(unresolved.call_id().call.0, 0);
    assert_eq!(unresolved.call_block(), 1);
    assert_eq!(unresolved.rejected_source(), &source(0));
    assert_eq!(
        unresolved.request(),
        &PairRequest::External {
            name: "sh".into(),
            arguments: "opaque original".into()
        }
    );
}

#[test]
fn accepted_requests_survive_rejected_siblings_for_forbidden_pass_accounting() {
    let (_, declaration, scheme) = fixture_scheme();
    let generation = checked(
        &declaration,
        vec![
            text("bad"),
            call("pending"),
            text("Read files."),
            call("valid"),
            text("Recall item."),
        ],
        vec![
            rejected(),
            accepted("Read files."),
            accepted("Recall item."),
        ],
    );
    let requests = OriginalRequests::from_checked(generation, &scheme).unwrap();
    assert_eq!(requests.unresolved().len(), 1);
    assert_eq!(
        requests
            .requests()
            .iter()
            .map(|x| x.opening().call_id.call.0)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert_eq!(requests.generation().blocks().len(), 3);
    assert!(matches!(
        requests.generation().blocks()[0].reply().result,
        Ste100Result::NotSte100 { .. }
    ));
}

#[test]
fn accepted_multi_statement_adjacency_is_refused_without_picking_a_sentence() {
    let (_, declaration, scheme) = fixture_scheme();
    let one = "Read first.";
    let two = "Read second.";
    let value = format!("{one} {two}");
    let generation = checked(
        &declaration,
        vec![text(&value), call("command")],
        vec![Ste100Result::Accepted {
            bindings: vec![binding(one), shifted(binding(two), one.len() + 1)],
        }],
    );
    let requests = OriginalRequests::from_checked(generation, &scheme).unwrap();
    assert!(requests.requests().is_empty());
    assert!(requests.unresolved().is_empty());
    assert!(matches!(
        requests.refusals()[0],
        AssociationRefusal::External {
            case: ProtocolRepairCase::AmbiguousToolStatement,
            ..
        }
    ));
}

#[test]
fn a_recall_binding_never_doubles_as_the_adjacent_external_send() {
    let (_, declaration, scheme) = fixture_scheme();
    let generation = checked(
        &declaration,
        vec![text("Recall item."), call("command")],
        vec![accepted("Recall item.")],
    );
    let requests = OriginalRequests::from_checked(generation, &scheme).unwrap();
    assert_eq!(requests.requests().len(), 1);
    assert_eq!(requests.requests()[0].opening().lane, PairLane::Recall);
    assert_eq!(requests.requests()[0].opening().call_id.call.0, 1);
    assert!(
        matches!(&requests.refusals()[0], AssociationRefusal::External {
        call_id, case: ProtocolRepairCase::AmbiguousToolStatement, ..
    } if call_id.call.0 == 0)
    );
}

#[test]
fn invalid_recall_shape_retains_exact_evidence_and_reserves_its_ordinal() {
    let (_, declaration, scheme) = fixture_scheme();
    let compound = "Recall first and second.";
    let mut bad = binding(compound);
    bad.nouns = vec![
        NounBinding {
            value: "first".into(),
            span: span(7, 12),
        },
        NounBinding {
            value: "second".into(),
            span: span(17, 23),
        },
    ];
    let conditional = "Recall item if ready.";
    let mut condition = binding(conditional);
    condition.nouns = vec![NounBinding {
        value: "item".into(),
        span: span(7, 11),
    }];
    condition.condition = Some("if ready".into());
    condition.condition_span = Some(span(12, 20));
    let generation = checked(
        &declaration,
        vec![text(compound), text(conditional), text("Recall valid.")],
        vec![
            Ste100Result::Accepted {
                bindings: vec![bad],
            },
            Ste100Result::Accepted {
                bindings: vec![condition],
            },
            accepted("Recall valid."),
        ],
    );
    let requests = OriginalRequests::from_checked(generation, &scheme).unwrap();
    assert_eq!(requests.requests().len(), 1);
    assert_eq!(requests.requests()[0].opening().call_id.call.0, 2);
    assert!(
        matches!(&requests.refusals()[0], AssociationRefusal::InvalidRecall { call_id, call_block: 0, statement, noun_count: 2, has_condition: false }
        if call_id.call.0 == 0 && statement.block_id == source(0))
    );
    assert!(
        matches!(&requests.refusals()[1], AssociationRefusal::InvalidRecall { call_id, call_block: 1, statement, noun_count: 1, has_condition: true }
        if call_id.call.0 == 1 && statement.block_id == source(1))
    );
}

#[test]
fn rejected_text_never_infers_a_recall_request_from_its_spelling() {
    let (_, declaration, scheme) = fixture_scheme();
    let generation = checked(&declaration, vec![text("Recall maybe")], vec![rejected()]);
    let requests = OriginalRequests::from_checked(generation, &scheme).unwrap();
    assert!(requests.requests().is_empty());
    assert!(requests.refusals().is_empty());
    assert!(!requests.attempted_call());
}

#[test]
fn only_unrecovered_blocks_still_report_attempt_without_inventing_a_pair() {
    let (_, declaration, scheme) = fixture_scheme();
    let generation = checked(&declaration, vec![unrecovered()], Vec::new());
    let requests = OriginalRequests::from_checked(generation, &scheme).unwrap();
    assert!(requests.attempted_call());
    assert!(requests.requests().is_empty());
    assert!(requests.refusals().is_empty());
    assert!(requests.unresolved().is_empty());
}

#[test]
fn ordinary_text_has_no_requests_and_keeps_its_original_proof() {
    let (_, declaration, scheme) = fixture_scheme();
    let generation = checked(
        &declaration,
        vec![text("Read files.")],
        vec![accepted("Read files.")],
    );
    let requests = OriginalRequests::from_checked(generation, &scheme).unwrap();
    assert!(!requests.attempted_call());
    assert!(requests.requests().is_empty());
    let generation = requests.into_generation();
    assert_eq!(
        generation.original().message.content,
        vec![text("Read files.")]
    );
    assert_eq!(generation.source().block_id, source(0));
}

#[test]
fn request_scheme_requires_exact_artifact_bytes_and_valid_blob_hashes() {
    let (text, declaration, _) = fixture_scheme();
    assert!(matches!(
        RequestScheme::from_exact_scheme(&(text.clone() + "\n"), &declaration),
        Err(AssociationError::InvalidScheme)
    ));
    let mut artifact: serde_json::Value = serde_json::from_str(&text).unwrap();
    artifact["grammar"]["sha256"] = serde_json::json!(hash(b"wrong"));
    let changed = serde_json::to_string(&artifact).unwrap();
    assert!(matches!(
        RequestScheme::from_exact_scheme(&changed, &scheme_declaration(&changed)),
        Err(AssociationError::InvalidScheme)
    ));
}

#[test]
fn request_scheme_refuses_unknown_recall_keys_and_duplicate_json_members() {
    let (text, _, _) = fixture_scheme();
    let mut artifact: serde_json::Value = serde_json::from_str(&text).unwrap();
    artifact["recall_keys"] = serde_json::json!(["invented"]);
    let changed = serde_json::to_string(&artifact).unwrap();
    assert!(RequestScheme::from_exact_scheme(&changed, &scheme_declaration(&changed)).is_err());
    let duplicate = text.replacen('{', "{\"format\":\"ste100-scheme/1\",", 1);
    assert!(RequestScheme::from_exact_scheme(&duplicate, &scheme_declaration(&duplicate)).is_err());
}

#[test]
fn foreign_loaded_scheme_returns_the_original_checked_generation() {
    let (scheme_text, declaration, _) = fixture_scheme();
    let generation = checked(
        &declaration,
        vec![text("Read files."), call("command")],
        vec![accepted("Read files.")],
    );
    let changed = scheme_text + "\n";
    let foreign =
        RequestScheme::from_exact_scheme(&changed, &scheme_declaration(&changed)).unwrap();
    let failure = OriginalRequests::from_checked(generation, &foreign).unwrap_err();
    assert_eq!(failure.error, AssociationError::WrongScheme);
    assert_eq!(
        failure.generation.original().message.content,
        vec![text("Read files."), call("command")]
    );
}

#[test]
fn textless_generation_still_requires_the_exact_loaded_scheme() {
    let (scheme_text, declaration, _) = fixture_scheme();
    let generation = checked(&declaration, vec![unrecovered()], Vec::new());
    let changed = scheme_text + "\n";
    let foreign =
        RequestScheme::from_exact_scheme(&changed, &scheme_declaration(&changed)).unwrap();
    let failure = OriginalRequests::from_checked(generation, &foreign).unwrap_err();
    assert_eq!(failure.error, AssociationError::WrongScheme);
    assert_eq!(
        failure.generation.original().message.content,
        vec![unrecovered()]
    );
}

#[test]
fn same_scheme_id_does_not_hide_a_different_artifact_digest() {
    let (scheme_text, declaration, scheme) = fixture_scheme();
    let mut corrupt = declaration.clone();
    corrupt.artifact_sha256 = hash(b"different artifact");
    assert_eq!(corrupt.scheme_id, declaration.scheme_id);
    // The production constructor refuses this declaration before association.
    assert!(matches!(
        RequestScheme::from_exact_scheme(&scheme_text, &corrupt),
        Err(AssociationError::InvalidScheme)
    ));
    // The private owner fixture also probes association's exact-identity guard.
    let generation = checked(&corrupt, vec![unrecovered()], Vec::new());
    let failure = OriginalRequests::from_checked(generation, &scheme).unwrap_err();
    assert_eq!(failure.error, AssociationError::WrongScheme);
    assert_eq!(failure.generation.scheme(), &corrupt);
}

#[test]
fn same_scheme_id_does_not_hide_a_different_closed_verb_list() {
    let (_, declaration, scheme) = fixture_scheme();
    let mut changed = declaration.clone();
    changed.verbs.push("invented".into());
    assert_eq!(changed.scheme_id, declaration.scheme_id);
    let generation = checked(
        &changed,
        vec![text("Read files."), call("command")],
        vec![accepted("Read files.")],
    );
    let failure = OriginalRequests::from_checked(generation, &scheme).unwrap_err();
    assert_eq!(failure.error, AssociationError::WrongScheme);
    assert_eq!(failure.generation.scheme(), &changed);
}
