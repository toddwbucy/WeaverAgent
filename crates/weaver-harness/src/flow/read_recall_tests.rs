use super::*;
use crate::flow::read::{PageRead, ReadAdvance};
use serde_json::{Value, json, value::to_raw_value};
use weaver_types::state_flow as s;

const SEED: &str = "0123456789abcdef0123456789abcdef";

#[test]
fn shared_checked_result_corpus_produces_exact_recall_after_release() {
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/recall-sources.json")).unwrap();
    assert_eq!(cases.len(), 18);
    for case in cases {
        let selector: s::ReadSelector = serde_json::from_value(case["selector"].clone())
            .unwrap_or_else(|error| panic!("{} selector: {error}", case["case"]));
        let result: s::QueryResult = serde_json::from_value(case["result"].clone())
            .unwrap_or_else(|error| panic!("{} result: {error}", case["case"]));
        let completed = checked_named(case["case"].as_str().unwrap(), selector, result);
        assert_eq!(
            serde_json::to_value(completed.recall().unwrap()).unwrap(),
            case["recall"],
            "fixture {}",
            case["case"]
        );
    }
}

fn block(sequence: u64) -> s::SourceBlockId {
    s::SourceBlockId {
        run: "run".into(),
        sequence: sequence.into(),
        block: 2.into(),
    }
}
fn statement(sequence: u64, ordinal: u64) -> s::StatementRef {
    s::StatementRef {
        block_id: block(sequence),
        binding: ordinal.into(),
    }
}
fn held_span(local_id: u64) -> s::SpanRef {
    s::SpanRef {
        seed_id: SEED.into(),
        local_id: local_id.into(),
        revision: 1.into(),
    }
}
fn position(sequence: u64) -> s::SourcePosition {
    s::SourcePosition {
        block_id: block(sequence),
        turn: Some("uncited-turn".into()),
    }
}
fn plan_ref() -> s::PlanRef {
    s::PlanRef {
        plan_id: s::PlanId {
            seed_id: SEED.into(),
            local_id: 3.into(),
        },
        revision: 2.into(),
    }
}
fn step_id(local_id: u64) -> s::StepId {
    s::StepId {
        seed_id: SEED.into(),
        local_id: local_id.into(),
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
fn interval(start: u64, end: u64) -> s::Span {
    s::Span {
        start: start.into(),
        end: end.into(),
    }
}
fn entry(sequence: u64, ordinal: u64) -> s::ReadEntry {
    s::ReadEntry {
        source: s::ReadSource::Event(Box::new(event_source(sequence, "message.assistant"))),
        raw_block: to_raw_value(&json!({"type":"text","text":"Read file."})).unwrap(),
        text: "Read file.".into(),
        bindings: vec![s::ReadBinding {
            ordinal: ordinal.into(),
            value: s::Binding {
                statement_span: interval(0, 10),
                verb: "read".into(),
                verb_span: interval(0, 4),
                condition: None,
                condition_span: None,
                nouns: vec![s::NounBinding {
                    value: "file".into(),
                    span: interval(5, 9),
                }],
            },
        }],
        selected_span: None,
        step_query: None,
        pairs: vec![],
    }
}
fn pair(terminal: bool) -> s::ReadPair {
    let started = s::PairStarted {
        call_id: s::CallId {
            run: "run".into(),
            sequence: 5.into(),
            call: 0.into(),
        },
        call_block: 3.into(),
        statement: statement(5, 2),
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
    let actual = |sequence, kind| {
        let mut source = event_source(sequence, kind);
        source.block_id.block = 0.into();
        source.source_path.clear();
        source
    };
    s::ReadPair {
        call_id: started.call_id.clone(),
        statement: started.statement.clone(),
        lane: started.lane,
        request: started.request.clone(),
        fate: if terminal {
            s::PairFate::Failed
        } else {
            s::PairFate::Opened
        },
        reason: if terminal { ended.reason.clone() } else { None },
        opening_source: s::PairReadSource {
            source: actual(6, "tool.call.started"),
            raw_block: to_raw_value(&started).unwrap(),
        },
        terminal_source: terminal.then(|| s::PairReadSource {
            source: actual(7, "tool.call.completed"),
            raw_block: to_raw_value(&ended).unwrap(),
        }),
    }
}
fn encode(bytes: &[u8]) -> String {
    const DIGITS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();
    for chunk in bytes.chunks(3) {
        let a = chunk[0];
        let b = chunk.get(1).copied().unwrap_or(0);
        let c = chunk.get(2).copied().unwrap_or(0);
        result.push(DIGITS[usize::from(a >> 2)] as char);
        result.push(DIGITS[usize::from((a & 3) << 4 | b >> 4)] as char);
        result.push(if chunk.len() > 1 {
            DIGITS[usize::from((b & 15) << 2 | c >> 6)] as char
        } else {
            '='
        });
        result.push(if chunk.len() > 2 {
            DIGITS[usize::from(c & 63)] as char
        } else {
            '='
        });
    }
    result
}
// Every expected provenance below passes through the actual strict page decoder,
// selector validation and matching release. No CompletedRead is manufactured.
fn checked(selector: s::ReadSelector, result: s::QueryResult) -> CompletedRead {
    checked_named("inline recall fixture", selector, result)
}

fn checked_named(case: &str, selector: s::ReadSelector, result: s::QueryResult) -> CompletedRead {
    let raw = serde_json::to_vec(&result).unwrap();
    let mut read = PageRead::new(selector, 1_000_000.into()).unwrap();
    let request = read.next_control().unwrap();
    read.bind_request(1.into(), &request).unwrap();
    assert_eq!(
        read.accept(
            1.into(),
            &request,
            &s::StateControlAnswer::ReadPage(s::ReadPage {
                snapshot_id: 19.into(),
                offset: 0.into(),
                total_bytes: (raw.len() as u64).into(),
                octets: encode(&raw),
                done: true,
            })
        )
        .unwrap_or_else(|error| panic!("{case}: {error}")),
        ReadAdvance::Pending
    );
    assert!(read.take().is_none());
    let release = read.next_control().unwrap();
    read.bind_request(2.into(), &release).unwrap();
    assert_eq!(
        read.accept(
            2.into(),
            &release,
            &s::StateControlAnswer::Released {
                snapshot_id: 19.into(),
            }
        )
        .unwrap(),
        ReadAdvance::Complete
    );
    read.take().unwrap().unwrap()
}
fn record(selector: s::ReadSelector, result: s::QueryResult) -> Value {
    let record = checked(selector.clone(), result).recall().unwrap();
    let wire = serde_json::to_value(record).unwrap();
    assert_eq!(wire.as_object().unwrap().len(), 1);
    assert!(wire.get("boot").is_none());
    let value = wire["read"].clone();
    assert_eq!(value["snapshot_id"], "19");
    assert_eq!(value["selector"], serde_json::to_value(selector).unwrap());
    assert!(value.get("message").is_none());
    assert!(value.get("end_mark").is_none());
    value
}
fn citation(sequence: u64, block: u64, ordinals: &[u64]) -> Value {
    json!({"source":{"event":{"block_id":{"run":"run",
        "sequence":sequence.to_string(),"block":block.to_string()}}},
        "binding_ordinals":ordinals.iter().map(u64::to_string).collect::<Vec<_>>()})
}
fn entries(entries: Vec<s::ReadEntry>) -> s::QueryResult {
    s::QueryResult::Entries { entries }
}
fn history() -> s::ReadSelector {
    s::ReadSelector::History {
        noun: "file".into(),
        verb: Some("read".into()),
        condition: None,
    }
}

#[test]
fn recall_content_selectors_keep_original_bindings_and_actual_pair_sources() {
    let mut first = entry(5, 2);
    first.pairs.push(pair(true));
    let result = record(
        history(),
        entries(vec![first, entry(5, 7), entry(9, 1), entry(5, 2)]),
    );
    assert_eq!(
        result["sources"],
        json!([
            citation(5, 2, &[2, 7]),
            citation(6, 0, &[]),
            citation(7, 0, &[]),
            citation(9, 2, &[1]),
        ])
    );
    assert_eq!(result["spans"], json!([]));
    for selector in [
        s::ReadSelector::Subjects {
            nouns: vec!["file".into()],
        },
        s::ReadSelector::Conversation {
            last_turns: 2.into(),
        },
        s::ReadSelector::Statement {
            statement: statement(5, 0),
        },
    ] {
        let result = record(selector, entries(vec![entry(5, 0)]));
        assert_eq!(result["sources"], json!([citation(5, 2, &[0])]));
    }
    let mut conditional = entry(5, 4);
    conditional.text = "Read file if ready.".into();
    conditional.raw_block = to_raw_value(&json!({"type":"text","text":conditional.text})).unwrap();
    let binding = &mut conditional.bindings[0].value;
    binding.statement_span = interval(0, 19);
    binding.condition = Some("ready".into());
    binding.condition_span = Some(interval(13, 18));
    let result = record(
        s::ReadSelector::Condition {
            condition: "ready".into(),
        },
        entries(vec![conditional]),
    );
    assert_eq!(result["sources"], json!([citation(5, 2, &[4])]));
    let result = record(
        s::ReadSelector::OpenPairs {},
        s::QueryResult::OpenPairs {
            pairs: vec![pair(false)],
        },
    );
    assert_eq!(
        result["sources"],
        json!([citation(5, 2, &[2]), citation(6, 0, &[])])
    );
}

#[test]
fn recall_span_occurrences_remain_ordered_while_sources_merge_exactly() {
    let span = held_span(4);
    let mut first = entry(5, 2);
    first.selected_span = Some(s::SelectedSpan {
        span_ref: span.clone(),
        start: 0.into(),
        end: 10.into(),
    });
    first.pairs.push(pair(true));
    let mut second = first.clone();
    second.pairs.clear();
    let selectors = [
        s::ReadSelector::Span {
            seed_id: SEED.into(),
            local_id: 4.into(),
            revision: 1.into(),
        },
        s::ReadSelector::SpanPreview {
            span: span.clone(),
            through: position(99),
        },
    ];
    for selector in selectors {
        let input = match &selector {
            s::ReadSelector::SpanPreview { span, through } => s::QueryResult::SpanPreview {
                span: span.clone(),
                through: through.clone(),
                entries: vec![first.clone(), second.clone()],
            },
            _ => entries(vec![first.clone(), second.clone()]),
        };
        let result = record(selector, input);
        assert_eq!(
            result["sources"],
            json!([
                citation(5, 2, &[2]),
                citation(6, 0, &[]),
                citation(7, 0, &[])
            ])
        );
        assert_eq!(
            result["spans"],
            json!([
                {"source":citation(5,2,&[])["source"],"selected_span":{
                    "span_ref":span,"start":"0","end":"10"}},
                {"source":citation(5,2,&[])["source"],"selected_span":{
                    "span_ref":span,"start":"0","end":"10"}},
            ])
        );
        assert!(!result["sources"].to_string().contains("99"));
    }
}

#[test]
fn recall_system_prompt_is_a_read_and_cites_only_the_retained_seed() {
    let mut first = entry(5, 0);
    first.source = s::ReadSource::Seed(s::SeedSource {
        seed_id: SEED.into(),
        ordinal: 0.into(),
    });
    first.selected_span = Some(s::SelectedSpan {
        span_ref: held_span(0),
        start: 0.into(),
        end: 10.into(),
    });
    let result = record(s::ReadSelector::SystemPrompt {}, entries(vec![first]));
    let source = json!({"seed":{"seed_id":SEED,"ordinal":"0"}});
    assert_eq!(
        result["sources"],
        json!([{"source":source,"binding_ordinals":["0"]}])
    );
    assert_eq!(
        result["spans"],
        json!([{"source":source,"selected_span":{
        "span_ref":held_span(0),"start":"0","end":"10"}}])
    );
}

fn plan_snapshot() -> s::PlanSnapshot {
    let step = |local_id, ordinal, source, binding| s::PlanStep {
        step_id: step_id(local_id),
        ordinal: s::DecimalU64(ordinal),
        instruction: statement(source, binding),
        category: "work".into(),
        group: None,
        status: s::StepStatus::Closed,
        span: Some(held_span(30)),
        outcome_span: Some(held_span(31)),
        review_call: Some(s::CallId {
            run: "uncited".into(),
            sequence: 100.into(),
            call: 0.into(),
        }),
        work: Some(s::WorkRef {
            plan: plan_ref(),
            step_id: step_id(local_id),
            span: held_span(32),
            through: position(source + 1),
            content_sha256: s::Digest::new("a".repeat(64)).unwrap(),
        }),
        closure_source: Some(block(source + 2)),
    };
    s::PlanSnapshot {
        plan: plan_ref(),
        task: s::TaskScopeRef {
            task: held_span(1),
            prompt: held_span(2),
        },
        status: s::PlanStatus::Broken,
        review: s::ReviewSetupRef {
            setup_id: "review".into(),
            sha256: s::Digest::new("b".repeat(64)).unwrap(),
        },
        operator_source: block(10),
        lock_source: Some(block(11)),
        last_source: block(12),
        categories: vec![],
        group_drafts: vec![],
        groups: vec![],
        steps: vec![step(2, 0, 13, 7), step(3, 1, 16, 2)],
        broken: Some(s::PlanBreakRecord {
            case: s::PlanBreakCase::WorkChanged,
            step_id: Some(step_id(3)),
            proposed_source: Some(block(19)),
            source: block(20),
        }),
    }
}

#[test]
fn recall_plan_and_current_walk_only_documented_sources_in_first_encounter_order() {
    let snapshot = plan_snapshot();
    for selector in [
        s::ReadSelector::PlanCurrent {},
        s::ReadSelector::Plan { plan: plan_ref() },
    ] {
        let input = match selector {
            s::ReadSelector::PlanCurrent {} => s::QueryResult::PlanCurrent {
                current: Some(snapshot.clone()),
                next_plan_id: 4.into(),
                next_step_id: 4.into(),
            },
            _ => s::QueryResult::Plan {
                snapshot: snapshot.clone(),
            },
        };
        let result = record(selector, input);
        assert_eq!(
            result["sources"],
            json!([
                citation(10, 2, &[]),
                citation(11, 2, &[]),
                citation(12, 2, &[]),
                citation(13, 2, &[7]),
                citation(14, 2, &[]),
                citation(15, 2, &[]),
                citation(16, 2, &[2]),
                citation(17, 2, &[]),
                citation(18, 2, &[]),
                citation(19, 2, &[]),
                citation(20, 2, &[]),
            ])
        );
        assert_eq!(result["spans"], json!([]));
    }
    let mut shared = snapshot;
    shared.operator_source = block(13);
    shared.lock_source = None;
    shared.last_source = block(13);
    shared.steps[0].work = None;
    shared.steps[0].closure_source = None;
    shared.steps[1].instruction = statement(13, 2);
    shared.steps[1].work = None;
    shared.steps[1].closure_source = None;
    shared.broken = None;
    let result = record(
        s::ReadSelector::Plan { plan: plan_ref() },
        s::QueryResult::Plan { snapshot: shared },
    );
    assert_eq!(result["sources"], json!([citation(13, 2, &[2, 7])]));
    let empty = record(
        s::ReadSelector::PlanCurrent {},
        s::QueryResult::PlanCurrent {
            current: None,
            next_plan_id: 1.into(),
            next_step_id: 1.into(),
        },
    );
    assert_eq!(empty["sources"], json!([]));
}

#[test]
fn recall_successor_marks_and_flags_keep_sources_without_invented_content() {
    let reference = s::StepQueryRef {
        plan: plan_ref(),
        previous_step: Some(step_id(2)),
        step_id: step_id(3),
        instruction: statement(5, 2),
    };
    let mut instruction = entry(5, 2);
    instruction.step_query = Some(reference.clone());
    instruction.pairs.push(pair(true));
    let result = record(
        s::ReadSelector::StepSuccessor {
            plan: plan_ref(),
            previous_step: Some(step_id(2)),
        },
        s::QueryResult::StepSuccessor {
            reference,
            instruction: Box::new(instruction),
        },
    );
    assert_eq!(
        result["sources"],
        json!([
            citation(5, 2, &[2]),
            citation(6, 0, &[]),
            citation(7, 0, &[])
        ])
    );
    let scope = |local_id, opened_source, ended_source| s::SpanInfo {
        span: held_span(local_id),
        parent: Some(held_span(0)),
        kind: s::SpanKind::Task,
        status: s::SpanStatus::Retired,
        opened_source,
        ended_source,
        replacement: Some(held_span(99)),
    };
    let result = record(
        s::ReadSelector::Marks {},
        s::QueryResult::Marks {
            next_span_id: 100.into(),
            current_task: Some(s::TaskScopeRef {
                task: held_span(1),
                prompt: held_span(2),
            }),
            open_scopes: vec![
                scope(1, None, None),
                scope(2, Some(block(8)), Some(block(9))),
                scope(3, Some(block(9)), Some(block(10))),
            ],
        },
    );
    assert_eq!(
        result["sources"],
        json!([
            citation(8, 2, &[]),
            citation(9, 2, &[]),
            citation(10, 2, &[])
        ])
    );
    assert_eq!(result["spans"], json!([]));
    for through in [position(8), position(9)] {
        let result = record(
            s::ReadSelector::PlanFlags {
                plan: plan_ref(),
                after: block(8),
            },
            s::QueryResult::PlanFlags {
                snapshot: s::PlanFlagSnapshot {
                    plan: plan_ref(),
                    after: block(8),
                    through: through.clone(),
                    total_flags: 2.into(),
                    matched_flags: 1.into(),
                    unmatched_flags: 1.into(),
                },
            },
        );
        let expected = if through.block_id == block(8) {
            json!([citation(8, 2, &[])])
        } else {
            json!([citation(8, 2, &[]), citation(9, 2, &[])])
        };
        assert_eq!(result["sources"], expected);
        assert_eq!(result["spans"], json!([]));
    }
}

#[test]
fn recall_empty_content_and_observation_metadata_are_successful_empty_accounts() {
    let empty = record(history(), entries(vec![]));
    assert_eq!(empty["sources"], json!([]));
    assert_eq!(empty["spans"], json!([]));
    let result = record(
        s::ReadSelector::Observations {
            run: "run".into(),
            after: 0.into(),
            through: Some(0.into()),
        },
        s::QueryResult::Observations {
            run: "run".into(),
            after: 0.into(),
            through: 0.into(),
            member_observation: s::MemberObservationIdentity {
                instrument_version: "state-observation/1".into(),
                pid: 10,
                start_ticks: Some(1.into()),
                ticks_per_second: Some(100.into()),
                unavailable_reason: None,
            },
            records: vec![],
        },
    );
    assert_eq!(result["sources"], json!([]));
    assert_eq!(result["spans"], json!([]));
}

#[test]
fn read_validation_rejects_noncanonical_seed_before_release_and_conversion_checks_selectors() {
    let mut prompt = entry(5, 0);
    prompt.source = s::ReadSource::Seed(s::SeedSource {
        seed_id: "seed".into(),
        ordinal: 0.into(),
    });
    let mut selected = held_span(0);
    selected.seed_id = "seed".into();
    prompt.selected_span = Some(s::SelectedSpan {
        span_ref: selected,
        start: 0.into(),
        end: 10.into(),
    });
    let raw = serde_json::to_vec(&entries(vec![prompt])).unwrap();
    let mut read = PageRead::new(s::ReadSelector::SystemPrompt {}, 1_000_000.into()).unwrap();
    let request = read.next_control().unwrap();
    read.bind_request(1.into(), &request).unwrap();
    assert!(
        read.accept(
            1.into(),
            &request,
            &s::StateControlAnswer::ReadPage(s::ReadPage {
                snapshot_id: 19.into(),
                offset: 0.into(),
                total_bytes: (raw.len() as u64).into(),
                octets: encode(&raw),
                done: true,
            })
        )
        .is_err()
    );
    assert!(read.next_control().is_none());
    assert!(read.take().is_none());
    for invalid in 0..3 {
        let mut reference = plan_ref();
        match invalid {
            0 => reference.plan_id.local_id = 0.into(),
            1 => reference.revision = 0.into(),
            _ => reference.plan_id.seed_id = "seed".into(),
        }
        assert!(selector(&s::ReadSelector::Plan { plan: reference }).is_err());
    }
}
