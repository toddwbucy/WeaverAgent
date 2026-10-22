use super::payloads::*;
use super::*;
use crate::{Failure, MonotonicNs, RunRef, Sequence, SessionRef, Subsystem, TurnRef};
use serde_json::value::RawValue;

const SEED: &str = "0123456789abcdef0123456789abcdef";

#[path = "recall_fixture_tests.rs"]
mod shared_fixtures;

fn raw(value: &str) -> Box<RawValue> {
    RawValue::from_string(value.into()).unwrap()
}

fn span(local_id: u64) -> SpanRef {
    SpanRef {
        seed_id: SEED.into(),
        local_id: local_id.into(),
        revision: 1.into(),
    }
}

fn source() -> SourceBlockId {
    SourceBlockId {
        run: "run".into(),
        sequence: 7.into(),
        block: 3.into(),
    }
}

fn plan() -> PlanRef {
    PlanRef {
        plan_id: PlanId {
            seed_id: SEED.into(),
            local_id: 4.into(),
        },
        revision: 2.into(),
    }
}

pub(super) fn read() -> RecallRecord {
    let event = ContentRef::Event { block_id: source() };
    let seed = ContentRef::Seed {
        seed_id: SEED.into(),
        ordinal: 0.into(),
    };
    let selected = RecallSpan {
        source: event.clone(),
        selected_span: RecallSelection {
            span_ref: span(9),
            start: 4.into(),
            end: 8.into(),
        },
    };
    RecallRecord::Read {
        selector: ReadSelector::Span {
            seed_id: SEED.into(),
            local_id: 9.into(),
            revision: 1.into(),
        },
        snapshot_id: 9007199254740993.into(),
        sources: vec![
            RecallSource {
                source: event,
                binding_ordinals: vec![2.into(), 10.into()],
            },
            RecallSource {
                source: seed.clone(),
                binding_ordinals: vec![],
            },
        ],
        spans: vec![
            selected.clone(),
            RecallSpan {
                source: seed,
                selected_span: RecallSelection {
                    span_ref: span(9),
                    start: 0.into(),
                    end: 10.into(),
                },
            },
            selected,
        ],
    }
}

pub(super) fn boot() -> RecallRecord {
    let source = ContentRef::Seed {
        seed_id: SEED.into(),
        ordinal: 0.into(),
    };
    RecallRecord::Boot {
        selector: ReadSelector::SystemPrompt {},
        snapshot_id: 1.into(),
        sources: vec![RecallSource {
            source: source.clone(),
            binding_ordinals: vec![0.into(), 2.into()],
        }],
        spans: vec![RecallSpan {
            source,
            selected_span: RecallSelection {
                span_ref: span(0),
                start: 0.into(),
                end: 10.into(),
            },
        }],
        message: raw(r#"{"role":"system","content":[{"type":"text","text":"Read file."}]}"#),
        end_mark: RecallEndMark {
            span: span(0),
            end: 10.into(),
        },
    }
}

pub(super) fn event(record: RecallRecord) -> Event {
    Event {
        envelope: Envelope {
            session: SessionRef("session".into()),
            run: RunRef("run".into()),
            turn: None,
            sequence: Sequence(999),
            kind: Kind::Recall,
            channel: Channel::Internal,
            origin: Origin::Runtime,
            subsystem: Subsystem::Harness,
            causal_parent: None,
            wall_ms: 42,
            monotonic_ns: MonotonicNs(100),
        },
        payload: Some(Payload::RecallRecord(Box::new(record))),
    }
}

fn admit(event: &Event) -> Result<(), Failure> {
    super::event::admit(event, &SessionRef("session".into()), &RunRef("run".into()))
}

fn accepted(record: &RecallRecord) -> bool {
    admit(&event(record.clone())).is_ok()
}

#[test]
fn recall_selector_wire_shapes_are_flat_closed_and_exact() {
    let cases = [
        (
            ReadSelector::History {
                noun: "file".into(),
                verb: None,
                condition: None,
            },
            r#"{"history":{"noun":"file","verb":null,"condition":null}}"#,
        ),
        (
            ReadSelector::History {
                noun: "file".into(),
                verb: Some("read".into()),
                condition: Some("ready".into()),
            },
            r#"{"history":{"noun":"file","verb":"read","condition":"ready"}}"#,
        ),
        (
            ReadSelector::Subjects {
                nouns: vec!["A".into(), "a".into()],
            },
            r#"{"subjects":{"nouns":["A","a"]}}"#,
        ),
        (
            ReadSelector::Statement {
                statement: StatementRef {
                    block_id: source(),
                    binding: 10.into(),
                },
            },
            r#"{"statement":{"statement":{"block_id":{"run":"run","sequence":"7","block":"3"},"binding":"10"}}}"#,
        ),
        (
            ReadSelector::Condition {
                condition: "step:4".into(),
            },
            r#"{"condition":{"condition":"step:4"}}"#,
        ),
        (
            ReadSelector::Span {
                seed_id: SEED.into(),
                local_id: 9.into(),
                revision: 1.into(),
            },
            r#"{"span":{"seed_id":"0123456789abcdef0123456789abcdef","local_id":"9","revision":"1"}}"#,
        ),
        (ReadSelector::SystemPrompt {}, r#"{"system_prompt":{}}"#),
        (
            ReadSelector::Conversation {
                last_turns: u64::MAX.into(),
            },
            r#"{"conversation":{"last_turns":"18446744073709551615"}}"#,
        ),
        (ReadSelector::OpenPairs {}, r#"{"open_pairs":{}}"#),
        (ReadSelector::Marks {}, r#"{"marks":{}}"#),
        (
            ReadSelector::SpanPreview {
                span: span(9),
                through: SourcePosition {
                    block_id: source(),
                    turn: None,
                },
            },
            r#"{"span_preview":{"span":{"seed_id":"0123456789abcdef0123456789abcdef","local_id":"9","revision":"1"},"through":{"block_id":{"run":"run","sequence":"7","block":"3"},"turn":null}}}"#,
        ),
        (
            ReadSelector::SpanPreview {
                span: span(9),
                through: SourcePosition {
                    block_id: source(),
                    turn: Some("old-turn".into()),
                },
            },
            r#"{"span_preview":{"span":{"seed_id":"0123456789abcdef0123456789abcdef","local_id":"9","revision":"1"},"through":{"block_id":{"run":"run","sequence":"7","block":"3"},"turn":"old-turn"}}}"#,
        ),
        (ReadSelector::PlanCurrent {}, r#"{"plan_current":{}}"#),
        (
            ReadSelector::Plan { plan: plan() },
            r#"{"plan":{"plan":{"plan_id":{"seed_id":"0123456789abcdef0123456789abcdef","local_id":"4"},"revision":"2"}}}"#,
        ),
        (
            ReadSelector::StepSuccessor {
                plan: plan(),
                previous_step: None,
            },
            r#"{"step_successor":{"plan":{"plan_id":{"seed_id":"0123456789abcdef0123456789abcdef","local_id":"4"},"revision":"2"},"previous_step":null}}"#,
        ),
        (
            ReadSelector::StepSuccessor {
                plan: plan(),
                previous_step: Some(StepId {
                    seed_id: SEED.into(),
                    local_id: 8.into(),
                }),
            },
            r#"{"step_successor":{"plan":{"plan_id":{"seed_id":"0123456789abcdef0123456789abcdef","local_id":"4"},"revision":"2"},"previous_step":{"seed_id":"0123456789abcdef0123456789abcdef","local_id":"8"}}}"#,
        ),
        (
            ReadSelector::PlanFlags {
                plan: plan(),
                after: source(),
            },
            r#"{"plan_flags":{"plan":{"plan_id":{"seed_id":"0123456789abcdef0123456789abcdef","local_id":"4"},"revision":"2"},"after":{"run":"run","sequence":"7","block":"3"}}}"#,
        ),
        (
            ReadSelector::Observations {
                run: "past-run".into(),
                after: 4.into(),
                through: None,
            },
            r#"{"observations":{"run":"past-run","after":"4","through":null}}"#,
        ),
        (
            ReadSelector::Observations {
                run: "past-run".into(),
                after: 4.into(),
                through: Some(4.into()),
            },
            r#"{"observations":{"run":"past-run","after":"4","through":"4"}}"#,
        ),
    ];
    for (selector, expected) in cases {
        let record = RecallRecord::Read {
            selector,
            snapshot_id: 1.into(),
            sources: vec![],
            spans: vec![],
        };
        assert!(accepted(&record), "{expected}");
        assert_eq!(
            serde_json::to_string(&record).unwrap(),
            format!(
                r#"{{"read":{{"selector":{expected},"snapshot_id":"1","sources":[],"spans":[]}}}}"#
            )
        );
    }
}

#[test]
fn recall_read_preserves_original_bindings_and_repeated_spans() {
    let record = read();
    assert!(accepted(&record));
    assert_eq!(
        serde_json::to_string(&record).unwrap(),
        r#"{"read":{"selector":{"span":{"seed_id":"0123456789abcdef0123456789abcdef","local_id":"9","revision":"1"}},"snapshot_id":"9007199254740993","sources":[{"source":{"event":{"block_id":{"run":"run","sequence":"7","block":"3"}}},"binding_ordinals":["2","10"]},{"source":{"seed":{"seed_id":"0123456789abcdef0123456789abcdef","ordinal":"0"}},"binding_ordinals":[]}],"spans":[{"source":{"event":{"block_id":{"run":"run","sequence":"7","block":"3"}}},"selected_span":{"span_ref":{"seed_id":"0123456789abcdef0123456789abcdef","local_id":"9","revision":"1"},"start":"4","end":"8"}},{"source":{"seed":{"seed_id":"0123456789abcdef0123456789abcdef","ordinal":"0"}},"selected_span":{"span_ref":{"seed_id":"0123456789abcdef0123456789abcdef","local_id":"9","revision":"1"},"start":"0","end":"10"}},{"source":{"event":{"block_id":{"run":"run","sequence":"7","block":"3"}}},"selected_span":{"span_ref":{"seed_id":"0123456789abcdef0123456789abcdef","local_id":"9","revision":"1"},"start":"4","end":"8"}}]}}"#
    );
}

#[test]
fn recall_source_uniqueness_uses_the_complete_tagged_content_identity() {
    let sources = [
        ContentRef::Event { block_id: source() },
        ContentRef::Event {
            block_id: SourceBlockId {
                block: 4.into(),
                ..source()
            },
        },
        ContentRef::Event {
            block_id: SourceBlockId {
                sequence: 8.into(),
                ..source()
            },
        },
        ContentRef::Event {
            block_id: SourceBlockId {
                run: "other-run".into(),
                ..source()
            },
        },
        ContentRef::Seed {
            seed_id: SEED.into(),
            ordinal: 0.into(),
        },
        ContentRef::Seed {
            seed_id: SEED.into(),
            ordinal: 1.into(),
        },
        // The scalar fields equal the preceding seed's untagged key. The
        // variant still makes this a distinct, legally cited content source.
        ContentRef::Event {
            block_id: SourceBlockId {
                run: SEED.into(),
                sequence: 1.into(),
                block: 0.into(),
            },
        },
        ContentRef::Seed {
            seed_id: "a".repeat(32),
            ordinal: 0.into(),
        },
    ]
    .into_iter()
    .map(|source| RecallSource {
        source,
        binding_ordinals: vec![],
    })
    .collect();
    assert!(accepted(&RecallRecord::Read {
        selector: ReadSelector::History {
            noun: "file".into(),
            verb: None,
            condition: None
        },
        snapshot_id: 1.into(),
        sources,
        spans: vec![],
    }));
}

#[test]
fn recall_boot_wire_contains_complete_message_and_stored_end() {
    let record = boot();
    assert!(accepted(&record));
    assert_eq!(
        serde_json::to_string(&record).unwrap(),
        r#"{"boot":{"selector":{"system_prompt":{}},"snapshot_id":"1","sources":[{"source":{"seed":{"seed_id":"0123456789abcdef0123456789abcdef","ordinal":"0"}},"binding_ordinals":["0","2"]}],"spans":[{"source":{"seed":{"seed_id":"0123456789abcdef0123456789abcdef","ordinal":"0"}},"selected_span":{"span_ref":{"seed_id":"0123456789abcdef0123456789abcdef","local_id":"0","revision":"1"},"start":"0","end":"10"}}],"message":{"role":"system","content":[{"type":"text","text":"Read file."}]},"end_mark":{"span":{"seed_id":"0123456789abcdef0123456789abcdef","local_id":"0","revision":"1"},"end":"10"}}}"#
    );
}

#[test]
fn recall_requires_its_envelope_and_accepts_absent_attribution() {
    for record in [read(), boot()] {
        let good = event(record);
        assert!(admit(&good).is_ok());
        let json = serde_json::to_value(&good).unwrap();
        assert!(json.get("turn").is_none());
        assert!(json.get("causal_parent").is_none());
        let mut attributed = good.clone();
        attributed.envelope.causal_parent = Some(Sequence(2));
        assert!(admit(&attributed).is_ok());
        for field in 0..5 {
            let mut wrong = good.clone();
            match field {
                0 => wrong.envelope.turn = Some(TurnRef("turn".into())),
                1 => wrong.envelope.channel = Channel::Input,
                2 => wrong.envelope.origin = Origin::Loop,
                3 => wrong.envelope.subsystem = Subsystem::Tool,
                4 => wrong.envelope.kind = Kind::MessageRestored,
                _ => unreachable!(),
            }
            assert!(
                matches!(admit(&wrong), Err(Failure::RefusedOnSubmit { .. })),
                "field {field}"
            );
        }
    }
}

#[test]
fn recall_refuses_context_free_selector_and_nested_identity_defects() {
    let mut invalid = vec![
        ReadSelector::Subjects { nouns: vec![] },
        ReadSelector::Subjects {
            nouns: vec!["".into()],
        },
        ReadSelector::Subjects {
            nouns: vec!["file".into(), "file".into()],
        },
        ReadSelector::Subjects {
            nouns: vec!["z".into(), "a".into()],
        },
        ReadSelector::Conversation {
            last_turns: 0.into(),
        },
        ReadSelector::Observations {
            run: "run".into(),
            after: 5.into(),
            through: Some(4.into()),
        },
    ];
    for bad_seed in [
        "",
        "seed",
        "0123456789ABCDEF0123456789abcdef",
        "0123456789abcdef0123456789abcdeg",
    ] {
        invalid.push(ReadSelector::Span {
            seed_id: bad_seed.into(),
            local_id: 0.into(),
            revision: 1.into(),
        });
        invalid.push(ReadSelector::SpanPreview {
            span: SpanRef {
                seed_id: bad_seed.into(),
                ..span(0)
            },
            through: SourcePosition {
                block_id: source(),
                turn: None,
            },
        });
        invalid.push(ReadSelector::Plan {
            plan: PlanRef {
                plan_id: PlanId {
                    seed_id: bad_seed.into(),
                    local_id: 1.into(),
                },
                ..plan()
            },
        });
        invalid.push(ReadSelector::StepSuccessor {
            plan: plan(),
            previous_step: Some(StepId {
                seed_id: bad_seed.into(),
                local_id: 1.into(),
            }),
        });
    }
    for revision in [0, 2] {
        invalid.push(ReadSelector::Span {
            seed_id: SEED.into(),
            local_id: 0.into(),
            revision: revision.into(),
        });
        invalid.push(ReadSelector::SpanPreview {
            span: SpanRef {
                revision: revision.into(),
                ..span(0)
            },
            through: SourcePosition {
                block_id: source(),
                turn: None,
            },
        });
    }
    let mut bad_plan = plan();
    bad_plan.plan_id.local_id = 0.into();
    for plan in [
        bad_plan,
        PlanRef {
            revision: 0.into(),
            ..plan()
        },
    ] {
        invalid.push(ReadSelector::Plan { plan: plan.clone() });
        invalid.push(ReadSelector::PlanFlags {
            plan: plan.clone(),
            after: source(),
        });
        invalid.push(ReadSelector::StepSuccessor {
            plan,
            previous_step: None,
        });
    }
    invalid.push(ReadSelector::StepSuccessor {
        plan: plan(),
        previous_step: Some(StepId {
            seed_id: SEED.into(),
            local_id: 0.into(),
        }),
    });
    for selector in invalid {
        let record = RecallRecord::Read {
            selector,
            snapshot_id: 1.into(),
            sources: vec![],
            spans: vec![],
        };
        assert!(!accepted(&record), "{record:?}");
    }
}

#[test]
fn recall_refuses_bad_snapshot_citations_ordinals_and_intervals() {
    for defect in 0..13 {
        let mut record = read();
        let RecallRecord::Read {
            snapshot_id,
            sources,
            spans,
            ..
        } = &mut record
        else {
            unreachable!()
        };
        match defect {
            0 => *snapshot_id = 0.into(),
            1 => sources.push(sources[0].clone()),
            2 => sources[0].binding_ordinals = vec![2.into(), 2.into()],
            3 => sources[0].binding_ordinals = vec![10.into(), 2.into()],
            4 => {
                sources.remove(0);
            }
            5 => spans[0].selected_span.end = 4.into(),
            6 => spans[0].selected_span.end = 3.into(),
            7 => {
                sources[1].source = ContentRef::Seed {
                    seed_id: "seed".into(),
                    ordinal: 0.into(),
                };
                spans[1].source = sources[1].source.clone();
            }
            8 => spans[0].selected_span.span_ref.seed_id = "seed".into(),
            9 => spans[0].selected_span.span_ref.revision = 0.into(),
            10 => spans[0].selected_span.span_ref.revision = 2.into(),
            11 => {
                spans[0].source = ContentRef::Event {
                    block_id: SourceBlockId {
                        block: 4.into(),
                        ..source()
                    },
                }
            }
            12 => sources.push(sources[1].clone()),
            _ => unreachable!(),
        }
        assert!(!accepted(&record), "defect {defect}");
    }
}

#[test]
fn recall_boot_refuses_inconsistent_seed_selection_message_and_end() {
    for defect in 0..15 {
        let mut record = boot();
        let RecallRecord::Boot {
            selector,
            snapshot_id,
            sources,
            spans,
            end_mark,
            ..
        } = &mut record
        else {
            unreachable!()
        };
        match defect {
            0 => *selector = ReadSelector::Marks {},
            1 => *snapshot_id = 0.into(),
            2 => sources.clear(),
            3 => sources.push(RecallSource {
                source: ContentRef::Event { block_id: source() },
                binding_ordinals: vec![],
            }),
            4 => spans.clear(),
            5 => spans.push(spans[0].clone()),
            6 => {
                sources[0].source = ContentRef::Event { block_id: source() };
                spans[0].source = sources[0].source.clone();
            }
            7 => {
                sources[0].source = ContentRef::Seed {
                    seed_id: SEED.into(),
                    ordinal: 1.into(),
                };
                spans[0].source = sources[0].source.clone();
            }
            8 => {
                spans[0].selected_span.span_ref.seed_id = "a".repeat(32);
                end_mark.span = spans[0].selected_span.span_ref.clone();
            }
            9 => {
                spans[0].selected_span.span_ref.local_id = 1.into();
                end_mark.span = spans[0].selected_span.span_ref.clone();
            }
            10 => spans[0].selected_span.start = 1.into(),
            11 => end_mark.span.local_id = 1.into(),
            12 => end_mark.end = 2.into(),
            13 => {
                spans[0].selected_span.end = 2.into();
                end_mark.end = 2.into();
            }
            14 => {
                spans[0].selected_span.span_ref.revision = 2.into();
                end_mark.span = spans[0].selected_span.span_ref.clone();
            }
            _ => unreachable!(),
        }
        assert!(!accepted(&record), "defect {defect}");
    }
}

#[test]
fn recall_boot_message_is_closed_canonical_and_counts_decoded_utf8_bytes() {
    for malformed in [
        r#"{"role":"user","content":[{"type":"text","text":"Read file."}]}"#,
        r#"{"role":"system","content":[]}"#,
        r#"{"role":"system","content":[{"type":"text","text":"Read file."},{"type":"text","text":"Read file."}]}"#,
        r#"{"role":"system","content":[{"type":"tool_use","text":"Read file."}]}"#,
        r#"{"role":"system","content":[{"type":"text","text":"Read file.","extra":true}]}"#,
        r#"{"role":"system","content":[{"type":"text","text":"Read file.","text":"Read file."}]}"#,
        r#"{"role":"system","content":[{"type":"text","text":"Read file."}],"extra":true}"#,
        r#"{"role":"system","role":"system","content":[{"type":"text","text":"Read file."}]}"#,
        r#"{"role":"system","content":[{"type":"text"}]}"#,
        r#"{"role":"system","content":[{"type":"text","text":"Read"}]}"#,
        r#"{"content":[{"type":"text","text":"Read file."}],"role":"system"}"#,
        r#"{"role":"system", "content":[{"type":"text","text":"Read file."}]}"#,
        r#"{"role":"system","content":[{"text":"Read file.","type":"text"}]}"#,
        r#"{"role":"system","content":[{"type":"text","text":"Read\u0020file."}]}"#,
    ] {
        let mut record = boot();
        let RecallRecord::Boot { message, .. } = &mut record else {
            unreachable!()
        };
        *message = raw(malformed);
        assert!(!accepted(&record), "{malformed}");
    }
    let mut record = boot();
    let RecallRecord::Boot {
        message,
        spans,
        end_mark,
        ..
    } = &mut record
    else {
        unreachable!()
    };
    *message = raw(&format!(
        r#"{{"role":"system","content":[{{"type":"text","text":{}}}]}}"#,
        serde_json::to_string("Read \u{e9}.\n").unwrap()
    ));
    spans[0].selected_span.end = 9.into();
    end_mark.end = 9.into();
    assert!(accepted(&record));
    let RecallRecord::Boot {
        spans, end_mark, ..
    } = &mut record
    else {
        unreachable!()
    };
    spans[0].selected_span.end = 8.into();
    end_mark.end = 8.into();
    assert!(!accepted(&record));
}
