use super::*;
use serde_json::value::to_raw_value;
use weaver_types::state_flow as s;

fn block(sequence: u64) -> s::SourceBlockId {
    s::SourceBlockId {
        run: "run".into(),
        sequence: sequence.into(),
        block: 2.into(),
    }
}
fn span(start: u64, end: u64) -> s::Span {
    s::Span {
        start: start.into(),
        end: end.into(),
    }
}
fn span_ref() -> s::SpanRef {
    s::SpanRef {
        seed_id: "seed".into(),
        local_id: 4.into(),
        revision: 1.into(),
    }
}
fn event_source(sequence: u64, kind: &str) -> s::EventSource {
    s::EventSource {
        block_id: block(sequence),
        envelope: s::SourceEnvelope {
            session: "session".into(),
            run: "run".into(),
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
fn binding() -> s::ReadBinding {
    s::ReadBinding {
        ordinal: 0.into(),
        value: s::Binding {
            statement_span: span(0, 8),
            verb: "read".into(),
            verb_span: span(0, 4),
            condition: None,
            condition_span: None,
            nouns: vec![s::NounBinding {
                value: "\u{e9}".into(),
                span: span(5, 7),
            }],
        },
    }
}
fn entry() -> s::ReadEntry {
    s::ReadEntry {
        source: s::ReadSource::Event(Box::new(event_source(5, "message.assistant"))),
        raw_block: to_raw_value(&serde_json::json!({"type":"text","text":"Read \u{e9}."})).unwrap(),
        text: "Read \u{e9}.".into(),
        bindings: vec![binding()],
        selected_span: None,
        step_query: None,
        pairs: vec![],
    }
}
fn result(entry: s::ReadEntry) -> s::QueryResult {
    s::QueryResult::Entries {
        entries: vec![entry],
    }
}
fn history() -> s::ReadSelector {
    s::ReadSelector::History {
        noun: "\u{e9}".into(),
        verb: Some("read".into()),
        condition: None,
    }
}
fn pair() -> s::ReadPair {
    let started = s::PairStarted {
        call_id: s::CallId {
            run: "run".into(),
            sequence: 5.into(),
            call: 0.into(),
        },
        call_block: 3.into(),
        statement: s::StatementRef {
            block_id: block(5),
            binding: 0.into(),
        },
        lane: s::PairLane::External,
        request: s::PairRequest::External {
            name: "sh".into(),
            arguments: "cat file".into(),
        },
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
        let mut source = event_source(sequence, kind);
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
            source: source(6, "tool.call.started"),
            raw_block: to_raw_value(&started).unwrap(),
        },
        terminal_source: Some(s::PairReadSource {
            source: source(7, "tool.call.completed"),
            raw_block: to_raw_value(&ended).unwrap(),
        }),
    }
}
fn plan_ref() -> s::PlanRef {
    s::PlanRef {
        plan_id: s::PlanId {
            seed_id: "seed".into(),
            local_id: 1.into(),
        },
        revision: 1.into(),
    }
}
fn plan() -> s::PlanSnapshot {
    s::PlanSnapshot {
        plan: plan_ref(),
        task: s::TaskScopeRef {
            task: span_ref(),
            prompt: span_ref(),
        },
        status: s::PlanStatus::Authorized,
        review: s::ReviewSetupRef {
            setup_id: "review".into(),
            sha256: s::Digest::new("a".repeat(64)).unwrap(),
        },
        operator_source: block(1),
        lock_source: None,
        last_source: block(1),
        categories: vec![],
        group_drafts: vec![],
        groups: vec![],
        steps: vec![],
        broken: None,
    }
}
fn identity() -> s::MemberObservationIdentity {
    s::MemberObservationIdentity {
        instrument_version: "state-observation/1".into(),
        pid: 10,
        start_ticks: Some(1.into()),
        ticks_per_second: Some(100.into()),
        unavailable_reason: None,
    }
}
fn observation(id: u64) -> s::StoredOperationObservation {
    s::StoredOperationObservation {
        request_source: None,
        observation: s::StateOperationObservation {
            operation_id: id.into(),
            parent_id: None,
            request_id: id.into(),
            operation: s::StateOperation::Read,
            began_ns: 20.into(),
            ended_ns: 22.into(),
            phases: vec![
                s::OperationPhase {
                    name: s::PhaseName::Validate,
                    began_ns: 20.into(),
                    ended_ns: 21.into(),
                },
                s::OperationPhase {
                    name: s::PhaseName::Encode,
                    began_ns: 21.into(),
                    ended_ns: 22.into(),
                },
            ],
            resource: s::OperationResource {
                cpu_ns: None,
                copied_bytes: None,
                held_bytes: None,
                unavailable: vec![
                    s::ResourceUnavailable {
                        field: s::ResourceField::CpuNs,
                        reason: s::ObservationUnavailable::NotSampled,
                    },
                    s::ResourceUnavailable {
                        field: s::ResourceField::CopiedBytes,
                        reason: s::ObservationUnavailable::Uninstrumented,
                    },
                    s::ResourceUnavailable {
                        field: s::ResourceField::HeldBytes,
                        reason: s::ObservationUnavailable::Uninstrumented,
                    },
                ],
            },
            disk: s::OperationDisk {
                enabled: None,
                before: None,
                after: None,
                unavailable_reason: Some(s::ObservationUnavailable::NotSampled),
            },
        },
    }
}

#[test]
fn empty_success_is_distinct_from_wrong_result_tag_and_single_source_omission() {
    let empty = s::QueryResult::Entries { entries: vec![] };
    assert!(validate(&history(), &empty).is_ok());
    assert!(validate(&s::ReadSelector::OpenPairs {}, &empty).is_err());
    assert!(validate(&s::ReadSelector::SystemPrompt {}, &empty).is_err());
    assert!(
        validate(
            &s::ReadSelector::Statement {
                statement: s::StatementRef {
                    block_id: block(5),
                    binding: 0.into()
                }
            },
            &empty
        )
        .is_err()
    );
}

#[test]
fn original_raw_text_and_event_index_survive_without_normalization() {
    let mut original = entry();
    original.raw_block = serde_json::value::RawValue::from_string(
        r#"{ "text":"Read \u00e9.", "type":"text" }"#.into(),
    )
    .unwrap();
    let before = original.raw_block.get().to_owned();
    assert!(validate(&history(), &result(original.clone())).is_ok());
    assert_eq!(original.raw_block.get(), before);
    let mut bad = original.clone();
    bad.text = "Read e.".into();
    assert!(validate(&history(), &result(bad)).is_err());
    let mut bad = original.clone();
    bad.raw_block = serde_json::value::RawValue::from_string(
        r#"{"type":"text","text":"Read \u00e9.","text":"Read \u00e9."}"#.into(),
    )
    .unwrap();
    assert!(validate(&history(), &result(bad)).is_err());
    let mut bad = original.clone();
    let s::ReadSource::Event(source) = &mut bad.source else {
        unreachable!()
    };
    source.source_path[1] = s::SourcePathPart::Index { index: 0.into() };
    assert!(validate(&history(), &result(bad)).is_err());
    let mut bad = original;
    let s::ReadSource::Event(source) = &mut bad.source else {
        unreachable!()
    };
    source.envelope.sequence = 4.into();
    assert!(validate(&history(), &result(bad)).is_err());
}

#[test]
fn binding_byte_boundaries_and_substrings_cannot_change_under_a_read() {
    assert!(validate(&history(), &result(entry())).is_ok());
    for span_value in [span(5, 6), span(5, 100), span(7, 5)] {
        let mut bad = entry();
        bad.bindings[0].value.nouns[0].span = span_value;
        assert!(validate(&history(), &result(bad)).is_err());
    }
    let mut bad = entry();
    bad.bindings[0].value.nouns[0].value = "different".into();
    assert!(validate(&history(), &result(bad)).is_err());
    let mut bad = entry();
    bad.bindings[0].value.condition = Some("when ready".into());
    assert!(validate(&history(), &result(bad)).is_err());
}

#[test]
fn filtered_reads_keep_original_ordinals_and_exact_selector_values() {
    let mut original = entry();
    original.bindings[0].ordinal = 7.into();
    assert!(validate(&history(), &result(original.clone())).is_ok());
    let selected = s::ReadSelector::Statement {
        statement: s::StatementRef {
            block_id: block(5),
            binding: 7.into(),
        },
    };
    assert!(validate(&selected, &result(original.clone())).is_ok());
    original.bindings[0].ordinal = 0.into();
    assert!(validate(&selected, &result(original)).is_err());
    let changed = s::ReadSelector::History {
        noun: "other".into(),
        verb: None,
        condition: None,
    };
    assert!(validate(&changed, &result(entry())).is_err());
    let changed = s::ReadSelector::History {
        noun: "\u{e9}".into(),
        verb: Some("write".into()),
        condition: None,
    };
    assert!(validate(&changed, &result(entry())).is_err());
    let changed = s::ReadSelector::Condition {
        condition: "when ready".into(),
    };
    assert!(validate(&changed, &result(entry())).is_err());
}

#[test]
fn filtered_span_bindings_keep_unique_increasing_original_ordinals() {
    let mut selected = entry();
    selected.text = "Read \u{e9}. Write it.".into();
    selected.raw_block =
        to_raw_value(&serde_json::json!({"type":"text","text":selected.text})).unwrap();
    selected.bindings[0].ordinal = 2.into();
    selected.bindings.push(s::ReadBinding {
        ordinal: 7.into(),
        value: s::Binding {
            statement_span: span(9, 18),
            verb: "write".into(),
            verb_span: span(9, 14),
            nouns: vec![s::NounBinding {
                value: "it".into(),
                span: span(15, 17),
            }],
            condition: None,
            condition_span: None,
        },
    });
    selected.selected_span = Some(s::SelectedSpan {
        span_ref: span_ref(),
        start: 0.into(),
        end: 18.into(),
    });
    let selector = s::ReadSelector::Span {
        seed_id: "seed".into(),
        local_id: 4.into(),
        revision: 1.into(),
    };
    // A filtered span can retain gaps in the original ordinals. It cannot
    // duplicate or reverse them, even though each binding is byte-valid and
    // the complete-sequence check used by conversation/subjects does not apply.
    assert!(validate(&selector, &result(selected.clone())).is_ok());
    for ordinal in [2, 1] {
        let mut bad = selected.clone();
        bad.bindings[1].ordinal = ordinal.into();
        assert!(
            validate(&selector, &result(bad)).is_err(),
            "ordinal {ordinal}"
        );
    }
}

#[test]
fn subjects_and_conversation_require_complete_unique_binding_and_source_sequences() {
    let mut full = entry();
    full.text = "Read \u{e9}. Write it.".into();
    full.raw_block = to_raw_value(&serde_json::json!({"type":"text","text":full.text})).unwrap();
    full.bindings.push(s::ReadBinding {
        ordinal: 1.into(),
        value: s::Binding {
            statement_span: span(9, 18),
            verb: "write".into(),
            verb_span: span(9, 14),
            nouns: vec![s::NounBinding {
                value: "it".into(),
                span: span(15, 17),
            }],
            condition: None,
            condition_span: None,
        },
    });
    let subjects = s::ReadSelector::Subjects {
        nouns: vec!["\u{e9}".into()],
    };
    let conversation = s::ReadSelector::Conversation {
        last_turns: 1.into(),
    };
    for selector in [&subjects, &conversation] {
        assert!(validate(selector, &result(full.clone())).is_ok());
        for ordinal in [0, 2] {
            let mut bad = full.clone();
            bad.bindings[1].ordinal = ordinal.into();
            assert!(validate(selector, &result(bad)).is_err());
        }
        assert!(
            validate(
                selector,
                &s::QueryResult::Entries {
                    entries: vec![full.clone(), full.clone()]
                }
            )
            .is_err()
        );
    }
}

#[test]
fn selected_span_checks_exact_echo_utf8_and_whole_binding_membership() {
    let mut selected = entry();
    selected.selected_span = Some(s::SelectedSpan {
        span_ref: span_ref(),
        start: 0.into(),
        end: 8.into(),
    });
    let selector = s::ReadSelector::Span {
        seed_id: "seed".into(),
        local_id: 4.into(),
        revision: 1.into(),
    };
    assert!(validate(&selector, &result(selected.clone())).is_ok());
    let mut bad = selected.clone();
    bad.selected_span.as_mut().unwrap().start = 5.into();
    assert!(validate(&selector, &result(bad)).is_err());
    let mut bad = selected.clone();
    bad.selected_span.as_mut().unwrap().span_ref.revision = 2.into();
    assert!(validate(&selector, &result(bad)).is_err());
    let through = s::SourcePosition {
        block_id: block(8),
        turn: Some("turn".into()),
    };
    let selector = s::ReadSelector::SpanPreview {
        span: span_ref(),
        through: through.clone(),
    };
    let mut preview = s::QueryResult::SpanPreview {
        span: span_ref(),
        through,
        entries: vec![selected],
    };
    assert!(validate(&selector, &preview).is_ok());
    let s::QueryResult::SpanPreview { through, .. } = &mut preview else {
        unreachable!()
    };
    through.turn = None;
    assert!(validate(&selector, &preview).is_err());
}

#[test]
fn system_prompt_is_one_full_seed_interval_without_event_refiling() {
    let mut prompt = entry();
    prompt.source = s::ReadSource::Seed(s::SeedSource {
        seed_id: "seed".into(),
        ordinal: 0.into(),
    });
    prompt.selected_span = Some(s::SelectedSpan {
        span_ref: s::SpanRef {
            seed_id: "seed".into(),
            local_id: 0.into(),
            revision: 1.into(),
        },
        start: 0.into(),
        end: 8.into(),
    });
    assert!(validate(&s::ReadSelector::SystemPrompt {}, &result(prompt.clone())).is_ok());
    let mut bad = prompt.clone();
    bad.bindings.clear();
    assert!(validate(&s::ReadSelector::SystemPrompt {}, &result(bad)).is_err());
    let mut bad = prompt.clone();
    bad.bindings[0].ordinal = 1.into();
    assert!(validate(&s::ReadSelector::SystemPrompt {}, &result(bad)).is_err());
    let mut bad = prompt.clone();
    bad.source = entry().source;
    assert!(validate(&s::ReadSelector::SystemPrompt {}, &result(bad)).is_err());
    prompt.selected_span.as_mut().unwrap().span_ref.local_id = 1.into();
    assert!(validate(&s::ReadSelector::SystemPrompt {}, &result(prompt)).is_err());
}

#[test]
fn pair_attachments_keep_exact_binding_and_complete_raw_opening_and_terminal() {
    let mut original = entry();
    original.pairs.push(pair());
    assert!(validate(&history(), &result(original.clone())).is_ok());
    let mut bad = original.clone();
    bad.pairs[0].statement.binding = 1.into();
    assert!(validate(&history(), &result(bad)).is_err());
    let mut bad = original.clone();
    bad.pairs[0].opening_source.raw_block = to_raw_value(&serde_json::json!({})).unwrap();
    assert!(validate(&history(), &result(bad)).is_err());
    let mut bad = original.clone();
    let mut terminal: s::PairEnded = decode_strict(
        bad.pairs[0]
            .terminal_source
            .as_ref()
            .unwrap()
            .raw_block
            .get(),
    )
    .unwrap();
    terminal.call_id.call = 1.into();
    bad.pairs[0].terminal_source.as_mut().unwrap().raw_block = to_raw_value(&terminal).unwrap();
    assert!(validate(&history(), &result(bad)).is_err());
    let mut bad = original.clone();
    bad.pairs[0].terminal_source = None;
    assert!(validate(&history(), &result(bad)).is_err());
    original.pairs.push(pair());
    assert!(validate(&history(), &result(original)).is_err());
}

#[test]
fn open_pairs_are_valid_history_but_never_a_closed_span_or_terminal_open_query() {
    let mut opened = pair();
    opened.fate = s::PairFate::Opened;
    opened.reason = None;
    opened.terminal_source = None;
    let mut original = entry();
    original.pairs.push(opened.clone());
    assert!(validate(&history(), &result(original.clone())).is_ok());
    assert!(
        validate(
            &s::ReadSelector::OpenPairs {},
            &s::QueryResult::OpenPairs {
                pairs: vec![opened]
            }
        )
        .is_ok()
    );
    assert!(
        validate(
            &s::ReadSelector::OpenPairs {},
            &s::QueryResult::OpenPairs {
                pairs: vec![pair()]
            }
        )
        .is_err()
    );
    original.selected_span = Some(s::SelectedSpan {
        span_ref: span_ref(),
        start: 0.into(),
        end: 8.into(),
    });
    assert!(
        validate(
            &s::ReadSelector::Span {
                seed_id: "seed".into(),
                local_id: 4.into(),
                revision: 1.into()
            },
            &result(original)
        )
        .is_err()
    );
}

#[test]
fn plan_metadata_preserves_revision_and_flags_do_not_order_unrelated_run_names() {
    let selector = s::ReadSelector::Plan { plan: plan_ref() };
    let mut snapshot = plan();
    assert!(
        validate(
            &selector,
            &s::QueryResult::Plan {
                snapshot: snapshot.clone()
            }
        )
        .is_ok()
    );
    snapshot.plan.revision = 2.into();
    assert!(validate(&selector, &s::QueryResult::Plan { snapshot }).is_err());
    let selector = s::ReadSelector::PlanFlags {
        plan: plan_ref(),
        after: block(5),
    };
    let mut snapshot = s::PlanFlagSnapshot {
        plan: plan_ref(),
        after: block(5),
        through: s::SourcePosition {
            block_id: s::SourceBlockId {
                run: "a-new-run".into(),
                sequence: 1.into(),
                block: 0.into(),
            },
            turn: None,
        },
        total_flags: 3.into(),
        matched_flags: 2.into(),
        unmatched_flags: 1.into(),
    };
    assert!(
        validate(
            &selector,
            &s::QueryResult::PlanFlags {
                snapshot: snapshot.clone()
            }
        )
        .is_ok()
    );
    snapshot.after.sequence = 4.into();
    assert!(
        validate(
            &selector,
            &s::QueryResult::PlanFlags {
                snapshot: snapshot.clone()
            }
        )
        .is_err()
    );
    snapshot.after = block(5);
    snapshot.matched_flags = u64::MAX.into();
    assert!(validate(&selector, &s::QueryResult::PlanFlags { snapshot }).is_err());
}

#[test]
fn step_successor_keeps_requested_predecessor_and_exact_instruction_binding() {
    let reference = s::StepQueryRef {
        plan: plan_ref(),
        previous_step: None,
        step_id: s::StepId {
            seed_id: "seed".into(),
            local_id: 3.into(),
        },
        instruction: s::StatementRef {
            block_id: block(5),
            binding: 0.into(),
        },
    };
    let mut instruction = entry();
    instruction.step_query = Some(reference.clone());
    let selector = s::ReadSelector::StepSuccessor {
        plan: plan_ref(),
        previous_step: None,
    };
    let valid = s::QueryResult::StepSuccessor {
        reference: reference.clone(),
        instruction: Box::new(instruction),
    };
    assert!(validate(&selector, &valid).is_ok());
    let mut bad = valid.clone();
    let s::QueryResult::StepSuccessor { instruction, .. } = &mut bad else {
        unreachable!()
    };
    instruction.step_query = None;
    assert!(validate(&selector, &bad).is_err());
    let mut bad = valid;
    let s::QueryResult::StepSuccessor {
        reference,
        instruction,
    } = &mut bad
    else {
        unreachable!()
    };
    reference.instruction.binding = 1.into();
    instruction.step_query = Some(reference.clone());
    assert!(validate(&selector, &bad).is_err());
}

#[test]
fn observation_queries_validate_echoes_order_frontier_identity_and_source_nullability() {
    let selector = s::ReadSelector::Observations {
        run: "run".into(),
        after: 1.into(),
        through: Some(4.into()),
    };
    let valid = s::QueryResult::Observations {
        run: "run".into(),
        after: 1.into(),
        through: 4.into(),
        member_observation: identity(),
        records: vec![observation(2), observation(4)],
    };
    assert!(validate(&selector, &valid).is_ok());
    for id in [1, 2, 5] {
        let mut bad = valid.clone();
        let s::QueryResult::Observations { records, .. } = &mut bad else {
            unreachable!()
        };
        records[1].observation.operation_id = id.into();
        assert!(validate(&selector, &bad).is_err());
    }
    let mut bad = valid.clone();
    let s::QueryResult::Observations { records, .. } = &mut bad else {
        unreachable!()
    };
    records[0].request_source = Some(block(5));
    assert!(validate(&selector, &bad).is_err());
    let mut bad = valid.clone();
    let s::QueryResult::Observations {
        member_observation, ..
    } = &mut bad
    else {
        unreachable!()
    };
    member_observation.pid = 0;
    assert!(validate(&selector, &bad).is_err());
    let mut bad = valid;
    let s::QueryResult::Observations { through, .. } = &mut bad else {
        unreachable!()
    };
    *through = 5.into();
    assert!(validate(&selector, &bad).is_err());
}

#[test]
fn conversation_cannot_turn_an_admitted_statement_into_an_empty_binding_list() {
    let selector = s::ReadSelector::Conversation {
        last_turns: 1.into(),
    };
    let mut original = entry();
    assert!(validate(&selector, &result(original.clone())).is_ok());
    original.bindings.clear();
    assert!(validate(&selector, &result(original)).is_err());
}

#[test]
fn historical_observation_reads_accept_both_producers_without_relabeling() {
    let selector = s::ReadSelector::Observations {
        run: "run".into(),
        after: 1.into(),
        through: Some(2.into()),
    };
    for (version, accepted) in [
        ("state-observation/1", true),
        ("state-observation/2", true),
        ("state-observation/3", false),
    ] {
        let mut member = identity();
        member.instrument_version = version.into();
        let result = s::QueryResult::Observations {
            run: "run".into(),
            after: 1.into(),
            through: 2.into(),
            member_observation: member,
            records: vec![observation(2)],
        };
        let original = serde_json::to_vec(&result).unwrap();
        assert_eq!(validate(&selector, &result).is_ok(), accepted, "{version}");
        assert_eq!(serde_json::to_vec(&result).unwrap(), original);
    }
}
