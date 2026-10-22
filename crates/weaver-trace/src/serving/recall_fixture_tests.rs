//! Shared literal read/recall cases, without a dependency on the harness or types.
//! The bridge below is test-only: production trace payloads stay serialize-only.
use super::*;
use serde_json::Value;

const CORPUS: &str = include_str!("../../../weaver-harness/src/flow/fixtures/recall-sources.json");

fn number(value: &Value) -> DecimalU64 {
    DecimalU64(value.as_str().unwrap().parse().unwrap())
}
fn text(value: &Value) -> String {
    value.as_str().unwrap().into()
}
fn optional_text(value: &Value) -> Option<String> {
    (!value.is_null()).then(|| text(value))
}
fn block(value: &Value) -> SourceBlockId {
    SourceBlockId {
        run: text(&value["run"]),
        sequence: number(&value["sequence"]),
        block: number(&value["block"]),
    }
}
fn statement(value: &Value) -> StatementRef {
    StatementRef {
        block_id: block(&value["block_id"]),
        binding: number(&value["binding"]),
    }
}
fn held_span(value: &Value) -> SpanRef {
    SpanRef {
        seed_id: text(&value["seed_id"]),
        local_id: number(&value["local_id"]),
        revision: number(&value["revision"]),
    }
}
fn plan(value: &Value) -> PlanRef {
    PlanRef {
        plan_id: PlanId {
            seed_id: text(&value["plan_id"]["seed_id"]),
            local_id: number(&value["plan_id"]["local_id"]),
        },
        revision: number(&value["revision"]),
    }
}
fn step(value: &Value) -> StepId {
    StepId {
        seed_id: text(&value["seed_id"]),
        local_id: number(&value["local_id"]),
    }
}
fn position(value: &Value) -> SourcePosition {
    SourcePosition {
        block_id: block(&value["block_id"]),
        turn: optional_text(&value["turn"]),
    }
}
fn source(value: &Value) -> ContentRef {
    let object = value.as_object().unwrap();
    assert_eq!(object.len(), 1);
    if let Some(value) = object.get("seed") {
        ContentRef::Seed {
            seed_id: text(&value["seed_id"]),
            ordinal: number(&value["ordinal"]),
        }
    } else {
        ContentRef::Event {
            block_id: block(&object["event"]["block_id"]),
        }
    }
}
fn selector(value: &Value) -> ReadSelector {
    let object = value.as_object().unwrap();
    assert_eq!(object.len(), 1);
    let (name, value) = object.iter().next().unwrap();
    match name.as_str() {
        "history" => ReadSelector::History {
            noun: text(&value["noun"]),
            verb: optional_text(&value["verb"]),
            condition: optional_text(&value["condition"]),
        },
        "subjects" => ReadSelector::Subjects {
            nouns: value["nouns"]
                .as_array()
                .unwrap()
                .iter()
                .map(text)
                .collect(),
        },
        "statement" => ReadSelector::Statement {
            statement: statement(&value["statement"]),
        },
        "condition" => ReadSelector::Condition {
            condition: text(&value["condition"]),
        },
        "span" => ReadSelector::Span {
            seed_id: text(&value["seed_id"]),
            local_id: number(&value["local_id"]),
            revision: number(&value["revision"]),
        },
        "system_prompt" => ReadSelector::SystemPrompt {},
        "conversation" => ReadSelector::Conversation {
            last_turns: number(&value["last_turns"]),
        },
        "open_pairs" => ReadSelector::OpenPairs {},
        "marks" => ReadSelector::Marks {},
        "span_preview" => ReadSelector::SpanPreview {
            span: held_span(&value["span"]),
            through: position(&value["through"]),
        },
        "plan_current" => ReadSelector::PlanCurrent {},
        "plan" => ReadSelector::Plan {
            plan: plan(&value["plan"]),
        },
        "step_successor" => ReadSelector::StepSuccessor {
            plan: plan(&value["plan"]),
            previous_step: (!value["previous_step"].is_null())
                .then(|| step(&value["previous_step"])),
        },
        "plan_flags" => ReadSelector::PlanFlags {
            plan: plan(&value["plan"]),
            after: block(&value["after"]),
        },
        "observations" => ReadSelector::Observations {
            run: text(&value["run"]),
            after: number(&value["after"]),
            through: (!value["through"].is_null()).then(|| number(&value["through"])),
        },
        _ => panic!("unknown shared recall fixture selector: {name}"),
    }
}

#[test]
fn shared_checked_result_corpus_has_admissible_exact_trace_representations() {
    let cases: Vec<Value> = serde_json::from_str(CORPUS).unwrap();
    assert_eq!(cases.len(), 18);
    for case in cases {
        assert!(
            case["result"].is_object(),
            "fixture lost its complete typed result"
        );
        let wire = &case["recall"]["read"];
        let record = RecallRecord::Read {
            selector: selector(&case["selector"]),
            snapshot_id: number(&wire["snapshot_id"]),
            sources: wire["sources"]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| RecallSource {
                    source: source(&value["source"]),
                    binding_ordinals: value["binding_ordinals"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(number)
                        .collect(),
                })
                .collect(),
            spans: wire["spans"]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| RecallSpan {
                    source: source(&value["source"]),
                    selected_span: RecallSelection {
                        span_ref: held_span(&value["selected_span"]["span_ref"]),
                        start: number(&value["selected_span"]["start"]),
                        end: number(&value["selected_span"]["end"]),
                    },
                })
                .collect(),
        };
        assert_eq!(
            serde_json::to_value(&record).unwrap(),
            case["recall"],
            "fixture bridge changed {}",
            case["case"]
        );
        let authored = event(record);
        assert!(
            admit(&authored).is_ok(),
            "fixture {} failed trace admission",
            case["case"]
        );
        let wire = serde_json::to_value(&authored).unwrap();
        assert!(wire.get("turn").is_none());
        assert!(wire.get("causal_parent").is_none());
        assert_eq!(wire["payload"], case["recall"]);
    }
}
