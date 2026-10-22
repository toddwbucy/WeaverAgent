//! Material is constructed only from the actual paged/read/recorder consumer.

use super::*;
use crate::flow::material::{HeldMaterial, MaterialFailure, SelectedUnit};
use serde_json::value::to_raw_value;
use weaver_traits::{ContentBlock, Role};

const _: fn() = || {
    trait MustNotClone<Marker> {
        fn check() {}
    }
    impl<T: ?Sized> MustNotClone<()> for T {}
    impl<T: Clone> MustNotClone<u8> for T {}
    let _ = <HeldMaterial as MustNotClone<_>>::check;
    let _ = <SelectedUnit as MustNotClone<_>>::check;
    trait MustNotFromRecord<Marker> {
        fn check() {}
    }
    impl<T: ?Sized> MustNotFromRecord<()> for T {}
    impl<T: From<weaver_traits::Message>> MustNotFromRecord<u8> for T {}
    impl<T: From<s::QueryResult>> MustNotFromRecord<u16> for T {}
    let _ = <HeldMaterial as MustNotFromRecord<_>>::check;
    let _ = <SelectedUnit as MustNotFromRecord<_>>::check;
};

fn span(start: u64, end: u64) -> s::Span {
    s::Span {
        start: start.into(),
        end: end.into(),
    }
}
fn block(sequence: u64) -> s::SourceBlockId {
    s::SourceBlockId {
        run: "old-run".into(),
        sequence: sequence.into(),
        block: 2.into(),
    }
}
fn event(sequence: u64, kind: &str) -> s::EventSource {
    s::EventSource {
        block_id: block(sequence),
        envelope: s::SourceEnvelope {
            session: "old-session".into(),
            run: "old-run".into(),
            turn: Some("turn".into()),
            sequence: sequence.into(),
            kind: kind.into(),
            channel: "output".into(),
            origin: "model".into(),
            subsystem: "harness".into(),
            causal_parent: None,
            wall_ms: 0,
            monotonic_ns: 10.into(),
        },
        source_path: vec![
            s::SourcePathPart::Key("content".into()),
            s::SourcePathPart::Index { index: 2.into() },
        ],
        partition_rule_id: "text".into(),
    }
}
fn binding(
    ordinal: u64,
    statement: s::Span,
    verb: &str,
    verb_span: s::Span,
    noun: &str,
    noun_span: s::Span,
) -> s::ReadBinding {
    s::ReadBinding {
        ordinal: ordinal.into(),
        value: s::Binding {
            statement_span: statement,
            verb: verb.into(),
            verb_span,
            condition: None,
            condition_span: None,
            nouns: vec![s::NounBinding {
                value: noun.into(),
                span: noun_span,
            }],
        },
    }
}
fn entry() -> s::ReadEntry {
    let text = "Read record. Write file.";
    s::ReadEntry {
        source: s::ReadSource::Event(Box::new(event(5, "message.assistant"))),
        raw_block: to_raw_value(&serde_json::json!({"type":"text","text":text})).unwrap(),
        text: text.into(),
        bindings: vec![
            binding(0, span(0, 12), "read", span(0, 4), "record", span(5, 11)),
            binding(1, span(13, 24), "write", span(13, 18), "file", span(19, 23)),
        ],
        selected_span: None,
        step_query: None,
        pairs: vec![],
    }
}
fn filtered() -> s::ReadEntry {
    let mut entry = entry();
    entry.bindings.remove(0);
    entry
}
fn history() -> s::ReadSelector {
    s::ReadSelector::History {
        noun: "file".into(),
        verb: None,
        condition: None,
    }
}
fn entries(entries: Vec<s::ReadEntry>) -> s::QueryResult {
    s::QueryResult::Entries { entries }
}
fn pair(call: u64, binding: u64, request: s::PairRequest) -> s::ReadPair {
    let started = s::PairStarted {
        call_id: s::CallId {
            run: "old-run".into(),
            sequence: 5.into(),
            call: call.into(),
        },
        call_block: (3 + call).into(),
        statement: s::StatementRef {
            block_id: block(5),
            binding: binding.into(),
        },
        lane: request.lane(),
        request,
        retry_of: None,
    };
    let ended = s::PairEnded {
        call_id: started.call_id.clone(),
        execution: None,
        fate: s::TerminalPairFate::Failed,
        reason: Some(s::PairFailure::NotDispatched {
            cause: s::NonDispatchCause::Stop,
        }),
        outcome: None,
        loaded_from: None,
    };
    fn source(sequence: u64, kind: &str) -> s::EventSource {
        let mut source = event(sequence, kind);
        source.block_id.block = 0.into();
        source.source_path.clear();
        source
    }
    s::ReadPair {
        call_id: started.call_id.clone(),
        statement: started.statement.clone(),
        lane: started.lane,
        request: started.request.clone(),
        fate: s::PairFate::Failed,
        reason: ended.reason.clone(),
        opening_source: s::PairReadSource {
            source: source(6 + call * 2, "tool.call.started"),
            raw_block: to_raw_value(&started).unwrap(),
        },
        terminal_source: Some(s::PairReadSource {
            source: source(7 + call * 2, "tool.call.completed"),
            raw_block: to_raw_value(&ended).unwrap(),
        }),
    }
}
fn external(call: u64, binding: u64) -> s::ReadPair {
    pair(
        call,
        binding,
        s::PairRequest::External {
            name: "held-tool".into(),
            arguments: "exact 'args'\nline".into(),
        },
    )
}
fn read(member: &mut Member, selector: s::ReadSelector, result: s::QueryResult) -> RecordedRead {
    let bytes = serde_json::to_vec(&result).unwrap();
    member
        .run
        .context
        .begin_read(selector, None, &mut member.run.clock)
        .unwrap();
    let request = member.next_request();
    member.respond(&request, page(7, 0, bytes.len(), &bytes));
    let release = member.next_request();
    member.respond(
        &release,
        s::StateControlAnswer::Released {
            snapshot_id: 7.into(),
        },
    );
    let read = member.finish().unwrap();
    assert_eq!(read.raw_bytes(), bytes);
    assert!(member.records(Kind::Recall).iter().any(|record| {
        record["sequence"]
            .as_str()
            .and_then(|value| value.parse::<u64>().ok())
            == Some(read.recall_sequence().0)
    }));
    read
}
fn hold(member: &mut Member, read: RecordedRead) -> HeldMaterial {
    member
        .run
        .context
        .hold_material(read, &mut member.run.clock)
        .unwrap()
}
fn text(unit: &SelectedUnit, index: usize) -> &str {
    let [ContentBlock::Text { text }] = unit.messages()[index].content.as_slice() else {
        panic!("not one canonical Text");
    };
    text
}
fn span_ref() -> s::SpanRef {
    s::SpanRef {
        seed_id: "0123456789abcdef0123456789abcdef".into(),
        local_id: 4.into(),
        revision: 1.into(),
    }
}
fn span_selector(span: &s::SpanRef) -> s::ReadSelector {
    s::ReadSelector::Span {
        seed_id: span.seed_id.clone(),
        local_id: span.local_id,
        revision: span.revision,
    }
}
fn selected(mut entry: s::ReadEntry, span_ref: &s::SpanRef, interval: s::Span) -> s::ReadEntry {
    entry.selected_span = Some(s::SelectedSpan {
        span_ref: span_ref.clone(),
        start: interval.start,
        end: interval.end,
    });
    entry
}

#[test]
fn material_filtered_statement_retains_original_ordinal_and_utf8_interval() {
    let mut member = Member::new();
    let mut entry = filtered();
    entry.text = "Read record. Read \u{e9}.".into();
    entry.raw_block = to_raw_value(&serde_json::json!({"type":"text","text":entry.text})).unwrap();
    entry.bindings = vec![binding(
        7,
        span(13, 21),
        "read",
        span(13, 17),
        "\u{e9}",
        span(18, 20),
    )];
    let proof = read(
        &mut member,
        s::ReadSelector::History {
            noun: "\u{e9}".into(),
            verb: None,
            condition: None,
        },
        entries(vec![entry]),
    );
    let sequence = proof.recall_sequence();
    let held = hold(&mut member, proof);
    assert_eq!(held.recall_sequences().collect::<Vec<_>>(), vec![sequence]);
    let units = held.into_units();
    let [unit] = units.as_slice() else {
        panic!("one statement unit");
    };
    assert!(unit.belongs_to(&member.run.context));
    assert_eq!(
        unit.unit_id(),
        &s::UnitId::Statement {
            statement: s::StatementRef {
                block_id: block(5),
                binding: 7.into()
            }
        }
    );
    assert_eq!(text(unit, 0), "Read \u{e9}.");
    assert_eq!(unit.messages()[0].role, Role::Assistant);
    assert_eq!(unit.source_map()[0].start, Some(0.into()));
    assert_eq!(unit.source_map()[0].end, Some(8.into()));
    assert_eq!(
        unit.sources(),
        &[s::MaterialRef::Text {
            source: s::ContentRef::Event { block_id: block(5) },
            span: span(13, 21)
        }]
    );
    assert_eq!(unit.reads()[0].recall_sequence(), sequence);
}

#[test]
fn material_whole_block_keeps_unmatched_binding_pairs_and_exact_tool_messages() {
    let mut member = Member::new();
    let mut entry = entry();
    let first = external(0, 0);
    let mut second = external(1, 1);
    let mut ended: s::PairEnded =
        s::decode_strict(second.terminal_source.as_ref().unwrap().raw_block.get()).unwrap();
    ended.execution = Some(weaver_types::ExchangeId {
        opener: weaver_types::Opener::Harness,
        ordinal: 3,
    });
    ended.reason = None;
    ended.outcome = Some(weaver_types::ToolOutcome::Killed {
        partial: Some("part\n\u{e9}".into()),
        by: weaver_types::KillCause::Clock,
    });
    second.reason = None;
    second.terminal_source.as_mut().unwrap().raw_block = to_raw_value(&ended).unwrap();
    entry.pairs = vec![first.clone(), second.clone()];
    let proof = read(
        &mut member,
        s::ReadSelector::Subjects {
            nouns: vec!["record".into()],
        },
        entries(vec![entry]),
    );
    let held = hold(&mut member, proof);
    let unit = &held.units()[0];
    assert_eq!(text(unit, 0), "Read record. Write file.");
    assert_eq!(unit.messages().len(), 5);
    for index in [1, 3] {
        assert_eq!(unit.messages()[index].role, Role::Assistant);
        assert_eq!(
            unit.messages()[index].content,
            vec![ContentBlock::ToolCall(weaver_traits::ToolCall {
                name: "held-tool".into(),
                arguments: "exact 'args'\nline".into()
            })]
        );
    }
    let ContentBlock::ToolResult(first_content) = &unit.messages()[2].content[0] else {
        panic!()
    };
    assert_eq!(
        first_content.content,
        first.terminal_source.as_ref().unwrap().raw_block.get()
    );
    let ContentBlock::ToolResult(second_content) = &unit.messages()[4].content[0] else {
        panic!()
    };
    assert_eq!(
        second_content.content,
        "the command ran past the clock and was killed. Partial output before the kill:\npart\n\u{e9}"
    );
    assert_eq!(
        unit.sources()[1],
        s::MaterialRef::Request {
            call_id: first.call_id
        }
    );
    assert_eq!(
        unit.sources()[3],
        s::MaterialRef::Request {
            call_id: second.call_id
        }
    );
    assert_eq!(unit.source_map().len(), 5);
    assert!(
        unit.source_map()[1..]
            .iter()
            .all(|map| map.start.is_none() && map.end.is_none())
    );
}

#[test]
fn material_inward_pair_uses_request_statement_text_and_open_pair_refuses_whole_unit() {
    let mut member = Member::new();
    let mut entry = filtered();
    let pair = pair(
        0,
        1,
        s::PairRequest::Recall {
            subject: "file".into(),
        },
    );
    entry.pairs.push(pair.clone());
    let proof = read(&mut member, history(), entries(vec![entry.clone()]));
    let held = hold(&mut member, proof);
    let unit = &held.units()[0];
    assert_eq!(unit.messages().len(), 2);
    assert_eq!(unit.source_map()[1].message, 0.into());
    assert_eq!(unit.source_map()[1].start, Some(0.into()));
    assert_eq!(unit.source_map()[1].end, Some(11.into()));
    assert_eq!(
        unit.sources()[1],
        s::MaterialRef::Request {
            call_id: pair.call_id
        }
    );
    entry.pairs[0].fate = s::PairFate::Opened;
    entry.pairs[0].reason = None;
    entry.pairs[0].terminal_source = None;
    let proof = read(&mut member, history(), entries(vec![entry]));
    let exact = proof.raw_bytes().to_vec();
    let error = member
        .run
        .context
        .hold_material(proof, &mut member.run.clock)
        .unwrap_err();
    assert_eq!(error.kind, MaterialFailure::IncompleteCandidates);
    assert!(error.failure.is_none());
    assert_eq!(error.reads[0].raw_bytes(), exact);
    assert!(member.run.context.halt().is_none());
}

#[test]
fn material_span_preserves_repeated_locations_and_expands_each_pair_once() {
    let mut member = Member::new();
    let span = span_ref();
    let mut entry = selected(filtered(), &span, self::span(13, 24));
    entry.pairs.push(external(0, 1));
    let proof = read(
        &mut member,
        span_selector(&span),
        entries(vec![entry.clone(), entry]),
    );
    let held = hold(&mut member, proof);
    let unit = &held.units()[0];
    assert_eq!(unit.unit_id(), &s::UnitId::Span { span: span.clone() });
    assert_eq!(unit.spans(), &[span]);
    assert_eq!(unit.messages().len(), 4);
    assert_eq!(text(unit, 0), "Write file.");
    assert_eq!(text(unit, 3), "Write file.");
    assert_eq!(unit.sources().len(), 3);
    assert_eq!(unit.source_map().len(), 4);
    assert_eq!(unit.source_map()[0].material, unit.source_map()[3].material);
    assert_eq!(unit.source_map()[0].message, 0.into());
    assert_eq!(unit.source_map()[3].message, 3.into());
}

#[test]
fn material_span_keeps_exact_step_query_and_source_kind_roles() {
    let mut member = Member::new();
    let span = span_ref();
    let mut entries_out = vec![];
    for (index, kind) in [
        "message.assistant",
        "message.user",
        "loop.instruction",
        "plan.event",
        "step.outcome",
        "sort.decision",
        "state.mark",
    ]
    .into_iter()
    .enumerate()
    {
        let mut entry = selected(filtered(), &span, self::span(13, 24));
        entry.source = s::ReadSource::Event(Box::new(event(index as u64 + 20, kind)));
        if kind == "loop.instruction" {
            entry.step_query = Some(s::StepQueryRef {
                plan: s::PlanRef {
                    plan_id: s::PlanId {
                        seed_id: "0123456789abcdef0123456789abcdef".into(),
                        local_id: 1.into(),
                    },
                    revision: 2.into(),
                },
                previous_step: None,
                step_id: s::StepId {
                    seed_id: "0123456789abcdef0123456789abcdef".into(),
                    local_id: 3.into(),
                },
                instruction: s::StatementRef {
                    block_id: block(index as u64 + 20),
                    binding: 1.into(),
                },
            });
        }
        entries_out.push(entry);
    }
    let reference = entries_out[2].step_query.clone().unwrap();
    let proof = read(&mut member, span_selector(&span), entries(entries_out));
    let held = hold(&mut member, proof);
    let unit = &held.units()[0];
    assert_eq!(unit.messages().len(), 7);
    assert_eq!(unit.messages()[0].role, Role::Assistant);
    assert!(
        unit.messages()[1..]
            .iter()
            .all(|message| message.role == Role::User)
    );
    assert_eq!(unit.step_queries().len(), 1);
    assert_eq!(unit.step_queries()[0].message, 2.into());
    assert_eq!(unit.step_queries()[0].reference, reference);
    assert_eq!(text(unit, 2), "Write file.");
}

#[test]
fn material_pair_requires_second_recorded_statement_with_original_binding() {
    let mut member = Member::new();
    let mut entry = filtered();
    entry.pairs.push(external(0, 1));
    let proof = read(&mut member, history(), entries(vec![entry.clone()]));
    let statement = read(
        &mut member,
        s::ReadSelector::Statement {
            statement: entry.pairs[0].statement.clone(),
        },
        entries(vec![entry.clone()]),
    );
    let sequences = vec![proof.recall_sequence(), statement.recall_sequence()];
    let held = member
        .run
        .context
        .hold_pair(proof, 0, 0, statement, &mut member.run.clock)
        .unwrap();
    assert_eq!(held.recall_sequences().collect::<Vec<_>>(), sequences);
    let unit = &held.units()[0];
    assert_eq!(
        unit.unit_id(),
        &s::UnitId::Pair {
            call_id: entry.pairs[0].call_id.clone()
        }
    );
    assert_eq!(unit.messages().len(), 3);
    assert_eq!(text(unit, 0), "Write file.");
    assert_eq!(unit.reads().len(), 2);
}

#[test]
fn material_pair_mismatched_statement_is_component_failure_and_retains_both_reads() {
    let mut member = Member::new();
    let mut entry = filtered();
    entry.pairs.push(external(0, 1));
    let proof = read(&mut member, history(), entries(vec![entry]));
    let mut other = filtered();
    other.source = s::ReadSource::Event(Box::new(event(99, "message.assistant")));
    let statement = read(
        &mut member,
        s::ReadSelector::Statement {
            statement: s::StatementRef {
                block_id: block(99),
                binding: 1.into(),
            },
        },
        entries(vec![other]),
    );
    let error = member
        .run
        .context
        .hold_pair(proof, 0, 0, statement, &mut member.run.clock)
        .unwrap_err();
    assert!(matches!(error.kind, MaterialFailure::Invalid(_)));
    assert!(matches!(
        error.failure,
        Some(RunFailure::Flow {
            reason: FlowHaltReason::ProtocolFailed,
            ..
        })
    ));
    assert_eq!(error.reads.len(), 2);
}

#[test]
fn material_rejects_equal_recall_sequences_from_another_context() {
    let mut first = Member::new();
    let mut second = Member::new();
    let first_read = read(&mut first, history(), entries(vec![filtered()]));
    let second_read = read(&mut second, history(), entries(vec![filtered()]));
    assert_eq!(first_read.recall_sequence(), second_read.recall_sequence());
    assert_eq!(first_read.snapshot_id(), second_read.snapshot_id());
    assert_eq!(first_read.raw_bytes(), second_read.raw_bytes());
    let held = hold(&mut first, first_read);
    assert!(held.belongs_to(&first.run.context));
    assert!(!held.belongs_to(&second.run.context));
    assert!(!held.units()[0].belongs_to(&second.run.context));
    let error = first
        .run
        .context
        .hold_material(second_read, &mut first.run.clock)
        .unwrap_err();
    assert_eq!(error.kind, MaterialFailure::ForeignContext);
    assert!(error.failure.is_some());
    assert!(error.reads[0].belongs_to(&second.run.context));
    // The rejected original is still consumable by its actual owner.
    let held = hold(&mut second, error.reads.into_iter().next().unwrap());
    assert!(held.belongs_to(&second.run.context));
}

#[test]
fn material_lifecycle_wait_keeps_the_owned_read_for_one_retry() {
    let mut member = Member::new();
    let proof = read(&mut member, history(), entries(vec![filtered()]));
    let sequence = proof.recall_sequence();
    let bound = AbsoluteBound::after(member.now(), 10).unwrap();
    member
        .run
        .context
        .arm(PumpDeadline::Leave, bound, &mut member.run.clock)
        .unwrap();
    member.run.clock.advance_to(bound.deadline_ns()).unwrap();
    let error = member
        .run
        .context
        .hold_material(proof, &mut member.run.clock)
        .unwrap_err();
    assert_eq!(error.kind, MaterialFailure::Unavailable);
    assert_eq!(
        error.failure,
        Some(RunFailure::Wait {
            key: PumpDeadline::Leave,
            bound
        })
    );
    member
        .run
        .context
        .complete_wait(PumpDeadline::Leave)
        .unwrap();
    let held = hold(&mut member, error.reads.into_iter().next().unwrap());
    assert_eq!(held.recall_sequences().collect::<Vec<_>>(), vec![sequence]);
    assert_eq!(member.records(Kind::Recall).len(), 1);
    assert_eq!(member.requests.len(), 2);
}

#[test]
fn material_unknown_source_kind_fails_with_exact_read_evidence() {
    let mut member = Member::new();
    let mut entry = filtered();
    entry.source = s::ReadSource::Event(Box::new(event(5, "state.observation")));
    let proof = read(&mut member, history(), entries(vec![entry]));
    let bytes = proof.raw_bytes().to_vec();
    let error = member
        .run
        .context
        .hold_material(proof, &mut member.run.clock)
        .unwrap_err();
    assert_eq!(
        error.kind,
        MaterialFailure::Invalid("unknown material source kind")
    );
    assert!(error.failure.is_some());
    assert_eq!(error.reads[0].raw_bytes(), bytes);
}

#[test]
fn material_preview_and_metadata_never_become_units() {
    let mut member = Member::new();
    let span = span_ref();
    let through = s::SourcePosition {
        block_id: block(10),
        turn: Some("turn".into()),
    };
    let proof = read(
        &mut member,
        s::ReadSelector::SpanPreview {
            span: span.clone(),
            through: through.clone(),
        },
        s::QueryResult::SpanPreview {
            span: span.clone(),
            through,
            entries: vec![selected(filtered(), &span, self::span(13, 24))],
        },
    );
    let error = member
        .run
        .context
        .hold_material(proof, &mut member.run.clock)
        .unwrap_err();
    assert_eq!(error.kind, MaterialFailure::NotMaterial);
    assert!(error.failure.is_none());
    let proof = read(
        &mut member,
        s::ReadSelector::PlanCurrent {},
        s::QueryResult::PlanCurrent {
            current: None,
            next_plan_id: 1.into(),
            next_step_id: 1.into(),
        },
    );
    let error = member
        .run
        .context
        .hold_material(proof, &mut member.run.clock)
        .unwrap_err();
    assert_eq!(error.kind, MaterialFailure::NotMaterial);
    assert!(member.run.context.halt().is_none());
}

#[test]
fn material_system_seed_read_is_inspectable_but_cannot_append_a_second_head() {
    let mut member = Member::new();
    let system = s::SpanRef {
        seed_id: "0123456789abcdef0123456789abcdef".into(),
        local_id: 0.into(),
        revision: 1.into(),
    };
    let mut entry = selected(entry(), &system, span(0, 24));
    entry.source = s::ReadSource::Seed(s::SeedSource {
        seed_id: "0123456789abcdef0123456789abcdef".into(),
        ordinal: 0.into(),
    });
    let proof = read(
        &mut member,
        s::ReadSelector::SystemPrompt {},
        entries(vec![entry.clone()]),
    );
    let error = member
        .run
        .context
        .hold_material(proof, &mut member.run.clock)
        .unwrap_err();
    assert_eq!(error.kind, MaterialFailure::InvalidSpan);
    assert!(error.failure.is_none());
    let proof = read(&mut member, span_selector(&system), entries(vec![entry]));
    let error = member
        .run
        .context
        .hold_material(proof, &mut member.run.clock)
        .unwrap_err();
    assert_eq!(error.kind, MaterialFailure::InvalidSpan);
    assert!(member.run.context.halt().is_none());
}

#[test]
fn material_review_lane_uses_exact_command_and_shared_outcome_formatting() {
    let mut member = Member::new();
    let mut entry = filtered();
    let plan = s::PlanRef {
        plan_id: s::PlanId {
            seed_id: "0123456789abcdef0123456789abcdef".into(),
            local_id: 1.into(),
        },
        revision: 1.into(),
    };
    let review = s::PairRequest::Review {
        setup: s::ReviewSetupRef {
            setup_id: "review".into(),
            sha256: s::Digest::new("a".repeat(64)).unwrap(),
        },
        work: Box::new(s::WorkRef {
            plan,
            step_id: s::StepId {
                seed_id: "0123456789abcdef0123456789abcdef".into(),
                local_id: 2.into(),
            },
            span: span_ref(),
            through: s::SourcePosition {
                block_id: block(4),
                turn: None,
            },
            content_sha256: s::Digest::new("b".repeat(64)).unwrap(),
        }),
        command: "review --held 'exact'\ncommand".into(),
    };
    let outcomes = [
        (
            weaver_types::ToolOutcome::Result {
                content: "whole result\n\u{e9}".into(),
            },
            "whole result\n\u{e9}",
        ),
        (
            weaver_types::ToolOutcome::Refused {
                reason: "retained refusal".into(),
            },
            "the call was refused: retained refusal",
        ),
        (
            weaver_types::ToolOutcome::Errored {
                detail: "retained error".into(),
            },
            "the tool machinery failed: retained error",
        ),
        (
            weaver_types::ToolOutcome::Killed {
                partial: None,
                by: weaver_types::KillCause::Cancel,
            },
            "the command was killed by the operator's stop, with no output before the kill",
        ),
    ];
    for (index, (outcome, _)) in outcomes.iter().enumerate() {
        let mut pair = pair(index as u64, 1, review.clone());
        let mut ended: s::PairEnded =
            s::decode_strict(pair.terminal_source.as_ref().unwrap().raw_block.get()).unwrap();
        ended.execution = Some(weaver_types::ExchangeId {
            opener: weaver_types::Opener::Harness,
            ordinal: index as u64,
        });
        ended.reason = None;
        ended.outcome = Some(outcome.clone());
        if matches!(outcome, weaver_types::ToolOutcome::Result { .. }) {
            ended.fate = s::TerminalPairFate::Completed;
            pair.fate = s::PairFate::Completed;
        }
        pair.reason = None;
        pair.terminal_source.as_mut().unwrap().raw_block = to_raw_value(&ended).unwrap();
        entry.pairs.push(pair);
    }
    let proof = read(&mut member, history(), entries(vec![entry]));
    let held = hold(&mut member, proof);
    let unit = &held.units()[0];
    assert_eq!(unit.messages().len(), 9);
    for (index, (_, expected)) in outcomes.iter().enumerate() {
        assert_eq!(
            unit.messages()[index * 2 + 1].content,
            vec![ContentBlock::ToolCall(weaver_traits::ToolCall {
                name: "sh".into(),
                arguments: "review --held 'exact'\ncommand".into()
            })]
        );
        let ContentBlock::ToolResult(result) = &unit.messages()[index * 2 + 2].content[0] else {
            panic!()
        };
        assert_eq!(&result.content, expected);
    }
}

#[test]
fn material_same_statement_identity_cannot_substitute_changed_bytes() {
    let mut member = Member::new();
    let mut original = filtered();
    original.pairs.push(external(0, 1));
    let proof = read(&mut member, history(), entries(vec![original.clone()]));
    let mut changed = original.clone();
    changed.text = "Read record. Write file!".into();
    changed.raw_block =
        to_raw_value(&serde_json::json!({"type":"text","text":changed.text})).unwrap();
    let statement = read(
        &mut member,
        s::ReadSelector::Statement {
            statement: original.pairs[0].statement.clone(),
        },
        entries(vec![changed]),
    );
    let error = member
        .run
        .context
        .hold_pair(proof, 0, 0, statement, &mut member.run.clock)
        .unwrap_err();
    assert_eq!(
        error.kind,
        MaterialFailure::Invalid("pair statement differs between held snapshots")
    );
    assert!(error.failure.is_some());
    assert_eq!(error.reads.len(), 2);
    assert_ne!(error.reads[0].raw_bytes(), error.reads[1].raw_bytes());
}

#[test]
fn material_pair_rejects_a_foreign_statement_proof_even_with_equal_metadata() {
    let mut owner = Member::new();
    let mut foreign = Member::new();
    let mut entry = filtered();
    entry.pairs.push(external(0, 1));
    let proof = read(&mut owner, history(), entries(vec![entry.clone()]));
    let statement = read(
        &mut foreign,
        s::ReadSelector::Statement {
            statement: entry.pairs[0].statement.clone(),
        },
        entries(vec![entry]),
    );
    let error = owner
        .run
        .context
        .hold_pair(proof, 0, 0, statement, &mut owner.run.clock)
        .unwrap_err();
    assert_eq!(error.kind, MaterialFailure::ForeignContext);
    assert_eq!(error.reads.len(), 2);
    assert!(error.reads[0].belongs_to(&owner.run.context));
    assert!(error.reads[1].belongs_to(&foreign.run.context));
}

#[test]
fn material_condition_and_conversation_choose_statement_and_whole_block_units() {
    let mut member = Member::new();
    let whole = entry();
    let proof = read(
        &mut member,
        s::ReadSelector::Conversation {
            last_turns: 1.into(),
        },
        entries(vec![whole]),
    );
    let held = hold(&mut member, proof);
    assert!(matches!(held.units()[0].unit_id(), s::UnitId::Block { .. }));
    assert_eq!(text(&held.units()[0], 0), "Read record. Write file.");
    let mut conditioned = filtered();
    conditioned.bindings[0].value.condition = Some("file".into());
    conditioned.bindings[0].value.condition_span = Some(span(19, 23));
    let proof = read(
        &mut member,
        s::ReadSelector::Condition {
            condition: "file".into(),
        },
        entries(vec![conditioned]),
    );
    let held = hold(&mut member, proof);
    assert_eq!(text(&held.units()[0], 0), "Write file.");
    assert_eq!(
        held.units()[0].unit_id(),
        &s::UnitId::Statement {
            statement: s::StatementRef {
                block_id: block(5),
                binding: 1.into()
            }
        }
    );
}

#[test]
fn material_repeated_pair_cannot_substitute_equal_bytes_from_another_event() {
    let mut member = Member::new();
    let span = span_ref();
    let mut first = selected(filtered(), &span, self::span(13, 24));
    first.pairs.push(external(0, 1));
    let mut second = first.clone();
    let source = &mut second.pairs[0].terminal_source.as_mut().unwrap().source;
    source.block_id.sequence = 99.into();
    source.envelope.sequence = 99.into();
    let proof = read(
        &mut member,
        span_selector(&span),
        entries(vec![first, second]),
    );
    let bytes = proof.raw_bytes().to_vec();
    let error = member
        .run
        .context
        .hold_material(proof, &mut member.run.clock)
        .unwrap_err();
    assert_eq!(
        error.kind,
        MaterialFailure::Invalid("repeated pair changes its held sources")
    );
    assert!(error.failure.is_some());
    assert_eq!(error.reads[0].raw_bytes(), bytes);
}

#[test]
fn material_repeated_text_cannot_substitute_bytes_or_event_attribution() {
    for changed_bytes in [true, false] {
        let mut member = Member::new();
        let span = span_ref();
        let first = selected(filtered(), &span, self::span(13, 24));
        let mut second = first.clone();
        if changed_bytes {
            second.text = "Read record. Write file!".into();
            second.raw_block =
                to_raw_value(&serde_json::json!({"type":"text","text":second.text})).unwrap();
        } else {
            let s::ReadSource::Event(source) = &mut second.source else {
                panic!()
            };
            source.envelope.session = "another-session".into();
        }
        let proof = read(
            &mut member,
            span_selector(&span),
            entries(vec![first, second]),
        );
        let bytes = proof.raw_bytes().to_vec();
        let error = member
            .run
            .context
            .hold_material(proof, &mut member.run.clock)
            .unwrap_err();
        assert_eq!(
            error.kind,
            MaterialFailure::Invalid("one source identity has conflicting held content")
        );
        assert!(error.failure.is_some());
        assert_eq!(error.reads[0].raw_bytes(), bytes);
    }
}

#[test]
fn material_repeated_binding_ordinal_cannot_name_two_different_statements() {
    let mut member = Member::new();
    let span = span_ref();
    let mut first = selected(entry(), &span, self::span(0, 12));
    first.bindings.truncate(1);
    let mut second = selected(filtered(), &span, self::span(13, 24));
    second.bindings[0].ordinal = 0.into();
    let proof = read(
        &mut member,
        span_selector(&span),
        entries(vec![first, second]),
    );
    let bytes = proof.raw_bytes().to_vec();
    let error = member
        .run
        .context
        .hold_material(proof, &mut member.run.clock)
        .unwrap_err();
    assert_eq!(
        error.kind,
        MaterialFailure::Invalid("repeated binding ordinal changes its original value")
    );
    assert!(error.failure.is_some());
    assert_eq!(error.reads[0].raw_bytes(), bytes);
}

#[test]
fn material_pair_identity_is_consistent_across_separate_statement_units() {
    let mut member = Member::new();
    let mut first = filtered();
    first.pairs.push(external(0, 1));
    let mut second = first.clone();
    let source = &mut second.pairs[0].terminal_source.as_mut().unwrap().source;
    source.block_id.sequence = 99.into();
    source.envelope.sequence = 99.into();
    let proof = read(&mut member, history(), entries(vec![first, second]));
    let bytes = proof.raw_bytes().to_vec();
    let error = member
        .run
        .context
        .hold_material(proof, &mut member.run.clock)
        .unwrap_err();
    assert_eq!(
        error.kind,
        MaterialFailure::Invalid("repeated pair changes its held sources")
    );
    assert!(error.failure.is_some());
    assert_eq!(error.reads[0].raw_bytes(), bytes);
}

#[test]
fn material_different_calls_cannot_reuse_one_source_event_for_different_payloads() {
    let mut member = Member::new();
    let mut entry = filtered();
    let first = external(0, 1);
    let mut second = external(1, 1);
    second.opening_source.source = first.opening_source.source.clone();
    entry.pairs = vec![first, second];
    let proof = read(&mut member, history(), entries(vec![entry]));
    let bytes = proof.raw_bytes().to_vec();
    let error = member
        .run
        .context
        .hold_material(proof, &mut member.run.clock)
        .unwrap_err();
    assert_eq!(
        error.kind,
        MaterialFailure::Invalid("one source identity has conflicting held content")
    );
    assert!(error.failure.is_some());
    assert_eq!(error.reads[0].raw_bytes(), bytes);
}

#[test]
fn journal_retains_real_read_provenance_and_keeps_repeated_pairs_separate() {
    let mut member = Member::new();
    let mut entry = filtered();
    entry.pairs.push(external(0, 1));
    let proof = read(&mut member, history(), entries(vec![entry.clone()]));
    let held = hold(&mut member, proof);
    let mut other = Member::new();
    let proof = read(&mut other, history(), entries(vec![entry]));
    let mut foreign = hold(&mut other, proof).into_units();
    crate::flow::journal::exercise_material(held.into_units(), foreign.remove(0));
}

fn step_query_entry() -> s::ReadEntry {
    let mut entry = selected(filtered(), &span_ref(), span(13, 24));
    let mut source = event(5, "loop.instruction");
    source.block_id.block = 0.into();
    source.envelope.channel = "internal".into();
    source.envelope.origin = "loop".into();
    source.envelope.turn = None;
    source.source_path = vec![
        s::SourcePathPart::Key("message".into()),
        s::SourcePathPart::Key("content".into()),
        s::SourcePathPart::Index { index: 0.into() },
    ];
    entry.step_query = Some(s::StepQueryRef {
        plan: s::PlanRef {
            plan_id: s::PlanId {
                seed_id: span_ref().seed_id,
                local_id: 1.into(),
            },
            revision: 2.into(),
        },
        previous_step: None,
        step_id: s::StepId {
            seed_id: span_ref().seed_id,
            local_id: 3.into(),
        },
        instruction: s::StatementRef {
            block_id: source.block_id.clone(),
            binding: 1.into(),
        },
    });
    entry.source = s::ReadSource::Event(Box::new(source));
    entry
}

fn step_review(entry: &s::ReadEntry) -> s::ReadPair {
    let reference = entry.step_query.as_ref().unwrap();
    let mut pair = pair(
        0,
        reference.instruction.binding.0,
        s::PairRequest::Review {
            setup: s::ReviewSetupRef {
                setup_id: "review".into(),
                sha256: s::Digest::new("a".repeat(64)).unwrap(),
            },
            work: Box::new(s::WorkRef {
                plan: reference.plan.clone(),
                step_id: reference.step_id.clone(),
                span: span_ref(),
                through: s::SourcePosition {
                    block_id: reference.instruction.block_id.clone(),
                    turn: None,
                },
                content_sha256: s::Digest::new("b".repeat(64)).unwrap(),
            }),
            command: "review --held-step".into(),
        },
    );
    pair.statement = reference.instruction.clone();
    let mut started: s::PairStarted =
        s::decode_strict(pair.opening_source.raw_block.get()).unwrap();
    started.statement = pair.statement.clone();
    started.call_block = 0.into();
    pair.opening_source.raw_block = to_raw_value(&started).unwrap();
    pair.opening_source.source.envelope.channel = "internal".into();
    pair.opening_source.source.envelope.origin = "loop".into();
    pair.opening_source.source.envelope.turn = None;
    let terminal = pair.terminal_source.as_mut().unwrap();
    let mut ended: s::PairEnded = s::decode_strict(terminal.raw_block.get()).unwrap();
    ended.execution = Some(weaver_types::ExchangeId {
        opener: weaver_types::Opener::Harness,
        ordinal: 0,
    });
    ended.fate = s::TerminalPairFate::Completed;
    ended.reason = None;
    ended.outcome = Some(weaver_types::ToolOutcome::Result {
        content: "review completed".into(),
    });
    terminal.raw_block = to_raw_value(&ended).unwrap();
    terminal.source.envelope.channel = "internal".into();
    terminal.source.envelope.origin = "runtime".into();
    terminal.source.envelope.turn = None;
    pair.fate = s::PairFate::Completed;
    pair.reason = None;
    pair
}

#[test]
fn material_step_query_retains_review_evidence_but_expands_only_the_instruction() {
    for ordinary in [false, true] {
        let mut member = Member::new();
        let mut entry = step_query_entry();
        let reference = entry.step_query.clone().unwrap();
        let attachment = step_review(&entry);
        entry.pairs.push(attachment.clone());
        if ordinary {
            entry.step_query = None;
        }
        let proof = read(
            &mut member,
            span_selector(&span_ref()),
            entries(vec![entry]),
        );
        let raw = proof.raw_bytes().to_vec();
        let held = hold(&mut member, proof);
        let unit = &held.units()[0];
        assert_eq!(text(unit, 0), "Write file.");
        assert_eq!(unit.messages()[0].role, Role::User);
        assert_eq!(unit.reads()[0].raw_bytes(), raw);
        let s::QueryResult::Entries { entries } = unit.reads()[0].result() else {
            panic!("retained the exact entry result");
        };
        assert_eq!(entries[0].pairs.len(), 1);
        assert_eq!(
            entries[0].pairs[0]
                .terminal_source
                .as_ref()
                .unwrap()
                .raw_block
                .get(),
            attachment.terminal_source.as_ref().unwrap().raw_block.get()
        );
        if ordinary {
            assert_eq!(unit.messages().len(), 3);
            assert_eq!(unit.sources().len(), 3);
            assert!(unit.step_queries().is_empty());
            assert!(matches!(
                unit.messages()[1].content[0],
                ContentBlock::ToolCall(_)
            ));
            assert!(matches!(
                unit.messages()[2].content[0],
                ContentBlock::ToolResult(_)
            ));
        } else {
            assert_eq!(unit.messages().len(), 1);
            assert_eq!(unit.sources().len(), 1);
            assert_eq!(unit.source_map().len(), 1);
            assert!(matches!(unit.sources()[0], s::MaterialRef::Text { .. }));
            assert_eq!(unit.step_queries().len(), 1);
            assert_eq!(unit.step_queries()[0].reference, reference);
            assert_eq!(unit.step_queries()[0].message, 0.into());
        }
        assert!(member.run.context.halt().is_none());
    }
}

#[test]
fn material_step_query_rejects_a_second_binding_before_selecting_its_instruction() {
    let mut member = Member::new();
    let mut entry = step_query_entry();
    let mut extra = entry.bindings[0].clone();
    extra.ordinal = 2.into();
    entry.bindings.push(extra);
    let proof = read(
        &mut member,
        span_selector(&span_ref()),
        entries(vec![entry]),
    );
    let raw = proof.raw_bytes().to_vec();
    let error = member
        .run
        .context
        .hold_material(proof, &mut member.run.clock)
        .unwrap_err();
    assert_eq!(
        error.kind,
        MaterialFailure::Invalid("step query is not its exact instruction")
    );
    assert!(error.failure.is_some());
    assert_eq!(error.reads[0].raw_bytes(), raw);
}

#[test]
fn material_checks_later_entries_before_refusing_an_open_pair() {
    for open_first in [true, false] {
        let mut member = Member::new();
        let mut open = filtered();
        let mut attachment = external(0, 1);
        attachment.fate = s::PairFate::Opened;
        attachment.reason = None;
        attachment.terminal_source = None;
        open.pairs.push(attachment);
        let mut invalid = filtered();
        invalid.source = s::ReadSource::Event(Box::new(event(99, "unknown.content")));
        let values = if open_first {
            vec![open, invalid]
        } else {
            vec![invalid, open]
        };
        let proof = read(&mut member, history(), entries(values));
        let raw = proof.raw_bytes().to_vec();
        let error = member
            .run
            .context
            .hold_material(proof, &mut member.run.clock)
            .unwrap_err();
        assert_eq!(
            error.kind,
            MaterialFailure::Invalid("unknown material source kind")
        );
        assert!(error.failure.is_some());
        assert_eq!(error.reads[0].raw_bytes(), raw);
    }
}

#[test]
fn material_pair_compares_matching_attachments_across_both_recorded_snapshots() {
    for changed in ["opening", "terminal", "payload"] {
        let mut member = Member::new();
        let mut entry = filtered();
        entry.pairs.push(external(0, 1));
        let proof = read(&mut member, history(), entries(vec![entry.clone()]));
        let pair = &mut entry.pairs[0];
        match changed {
            "opening" => pair.opening_source.source.envelope.wall_ms += 1,
            "terminal" => {
                let source = &mut pair.terminal_source.as_mut().unwrap().source;
                source.block_id.sequence = 99.into();
                source.envelope.sequence = 99.into();
            }
            "payload" => {
                let terminal = pair.terminal_source.as_mut().unwrap();
                let mut ended: s::PairEnded = s::decode_strict(terminal.raw_block.get()).unwrap();
                ended.reason = Some(s::PairFailure::NotDispatched {
                    cause: s::NonDispatchCause::Unload,
                });
                pair.reason = ended.reason.clone();
                terminal.raw_block = to_raw_value(&ended).unwrap();
            }
            _ => unreachable!(),
        }
        let selector = s::ReadSelector::Statement {
            statement: pair.statement.clone(),
        };
        let statement = read(&mut member, selector, entries(vec![entry]));
        let original = [proof.raw_bytes().to_vec(), statement.raw_bytes().to_vec()];
        let error = member
            .run
            .context
            .hold_pair(proof, 0, 0, statement, &mut member.run.clock)
            .unwrap_err();
        assert_eq!(
            error.kind,
            MaterialFailure::Invalid("pair attachment differs between held snapshots")
        );
        assert!(error.failure.is_some());
        assert_eq!(error.reads.len(), 2);
        assert_eq!(error.reads[0].raw_bytes(), original[0]);
        assert_eq!(error.reads[1].raw_bytes(), original[1]);
    }
}

#[test]
fn material_recall_request_offsets_follow_the_statement_inside_block_and_span_units() {
    for as_span in [false, true] {
        let mut member = Member::new();
        let mut entry = entry();
        entry.text = "Read record. Read file.".into();
        entry.raw_block =
            to_raw_value(&serde_json::json!({"type":"text","text":entry.text})).unwrap();
        entry.bindings[1] = binding(1, span(13, 23), "read", span(13, 17), "file", span(18, 22));
        let attachment = pair(
            0,
            1,
            s::PairRequest::Recall {
                subject: "file".into(),
            },
        );
        let request = s::MaterialRef::Request {
            call_id: attachment.call_id.clone(),
        };
        entry.pairs.push(attachment);
        let selector = if as_span {
            entry = selected(entry, &span_ref(), span(0, 23));
            span_selector(&span_ref())
        } else {
            s::ReadSelector::Subjects {
                nouns: vec!["file".into()],
            }
        };
        let proof = read(&mut member, selector, entries(vec![entry]));
        let held = hold(&mut member, proof);
        let unit = &held.units()[0];
        assert_eq!(unit.messages().len(), 2);
        assert_eq!(text(unit, 0), "Read record. Read file.");
        let map = unit
            .source_map()
            .iter()
            .find(|map| map.material == request)
            .unwrap();
        assert_eq!(map.message, 0.into());
        assert_eq!(map.block, 0.into());
        assert_eq!(map.start, Some(13.into()));
        assert_eq!(map.end, Some(23.into()));
    }
}

#[test]
fn material_empty_span_and_event_backed_system_span_refuse_without_halting() {
    for empty in [true, false] {
        let mut member = Member::new();
        let mut span_ref = span_ref();
        let entries_out = if empty {
            vec![]
        } else {
            span_ref.local_id = 0.into();
            vec![selected(filtered(), &span_ref, span(13, 24))]
        };
        let proof = read(&mut member, span_selector(&span_ref), entries(entries_out));
        let raw = proof.raw_bytes().to_vec();
        let error = member
            .run
            .context
            .hold_material(proof, &mut member.run.clock)
            .unwrap_err();
        assert_eq!(error.kind, MaterialFailure::InvalidSpan);
        assert!(error.failure.is_none());
        assert!(member.run.context.halt().is_none());
        assert_eq!(error.reads[0].raw_bytes(), raw);
    }
}

#[test]
fn material_open_pair_does_not_hide_a_conflicting_statement_snapshot() {
    let mut member = Member::new();
    let mut entry = filtered();
    let mut attachment = external(0, 1);
    attachment.fate = s::PairFate::Opened;
    attachment.reason = None;
    attachment.terminal_source = None;
    let selector = s::ReadSelector::Statement {
        statement: attachment.statement.clone(),
    };
    entry.pairs.push(attachment);
    let proof = read(&mut member, history(), entries(vec![entry.clone()]));
    entry.text = "Read record. Write file!".into();
    entry.raw_block = to_raw_value(&serde_json::json!({"type":"text","text":entry.text})).unwrap();
    let statement = read(&mut member, selector, entries(vec![entry]));
    let original = [proof.raw_bytes().to_vec(), statement.raw_bytes().to_vec()];
    let error = member
        .run
        .context
        .hold_pair(proof, 0, 0, statement, &mut member.run.clock)
        .unwrap_err();
    assert_eq!(
        error.kind,
        MaterialFailure::Invalid("pair statement differs between held snapshots")
    );
    assert!(error.failure.is_some());
    assert_eq!(error.reads[0].raw_bytes(), original[0]);
    assert_eq!(error.reads[1].raw_bytes(), original[1]);
}

#[test]
fn material_raw_text_and_pair_interval_disagreements_cannot_become_recorded_reads() {
    for raw_text in [true, false] {
        let mut member = Member::new();
        let mut entry = selected(filtered(), &span_ref(), span(13, 24));
        entry.pairs.push(external(0, 1));
        if raw_text {
            // The material pair comparison's text guard is redundant with
            // exact raw equality after this actual read-boundary validation.
            entry.text = "Read record. Write file!".into();
        } else {
            // A selected pair cannot reach material with its retained binding
            // outside the selected Text interval: the read owner rejects it.
            entry.selected_span.as_mut().unwrap().start = 14.into();
        }
        let bytes = serde_json::to_vec(&entries(vec![entry])).unwrap();
        member
            .run
            .context
            .begin_read(span_selector(&span_ref()), None, &mut member.run.clock)
            .unwrap();
        let request = member.next_request();
        member.respond_protocol_failure(&request, page(7, 0, bytes.len(), &bytes));
        assert!(member.records(Kind::Recall).is_empty());
    }
}

#[test]
fn material_statement_step_query_is_refused_before_a_pair_proof_can_be_formed() {
    let mut member = Member::new();
    let mut statement_entry = step_query_entry();
    statement_entry.selected_span = None;
    let attachment = step_review(&statement_entry);
    let selector = s::ReadSelector::Statement {
        statement: attachment.statement.clone(),
    };
    statement_entry.pairs.push(attachment);
    let mut original = statement_entry.clone();
    original.step_query = None;
    let proof = read(&mut member, history(), entries(vec![original]));
    let raw = proof.raw_bytes().to_vec();
    let recalls = member.records(Kind::Recall).len();
    // Statement uses require_unfiltered_text after require_statement. That
    // existing boundary refuses span metadata before another RecordedRead
    // can be offered to hold_pair. Do not manufacture a second proof.
    member
        .run
        .context
        .begin_read(selector, None, &mut member.run.clock)
        .unwrap();
    let request = member.next_request();
    let bytes = serde_json::to_vec(&entries(vec![statement_entry])).unwrap();
    member.respond_protocol_failure(&request, page(7, 0, bytes.len(), &bytes));
    assert_eq!(member.records(Kind::Recall).len(), recalls);
    assert_eq!(proof.raw_bytes(), raw);
    assert!(proof.belongs_to(&member.run.context));
}
