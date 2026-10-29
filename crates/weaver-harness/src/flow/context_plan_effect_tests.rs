//! Root allocation and closure evidence through actual input filing and reads.
//! Member/classifier answers remain scripted; this is not a SQLite qualification.
use super::*;
#[path = "context_plan_effect_filing_tests.rs"]
mod filing;

fn catalog(current: Option<s::PlanSnapshot>, plan: u64, step: u64) -> Vec<u8> {
    serde_json::to_vec(&s::QueryResult::PlanCurrent {
        current,
        next_plan_id: plan.into(),
        next_step_id: step.into(),
    })
    .unwrap()
}
fn marks(current: Option<&s::PlanSnapshot>, open: bool, next: u64) -> Vec<u8> {
    serde_json::to_vec(&s::QueryResult::Marks {
        next_span_id: next.into(),
        current_task: current.map(|plan| plan.task.clone()),
        open_scopes: if open {
            let plan = current.unwrap();
            vec![s::SpanInfo {
                span: plan.task.task.clone(),
                parent: None,
                kind: s::SpanKind::Task,
                status: s::SpanStatus::Open,
                opened_source: Some(plan.operator_source.clone()),
                ended_source: None,
                replacement: None,
            }]
        } else {
            vec![]
        },
    })
    .unwrap()
}
fn supplied(fx: &mut SummaryFixture, raw: &str, current: Vec<u8>, marks: Vec<u8>) {
    assert_eq!(
        read_catalog_values(fx, raw, current, marks),
        RootPlanProgress::InputReady
    );
    assert_eq!(file_input(fx, false), RootPlanProgress::InputFiled);
}

#[test]
#[ignore = "actual root coordination peer"]
fn root_plan_effect_uses_separate_catalog_counters_and_exact_input_sources() {
    for authorize in [false, true] {
        let (mut fx, _boot) = ready();
        let mut value: serde_json::Value = serde_json::from_str(&filing_document()).unwrap();
        if authorize {
            let mut body = value["operation"]["create"].take();
            body.as_object_mut().unwrap().remove("steps");
            value["operation"] = serde_json::json!({"authorize":body});
        }
        supplied(
            &mut fx,
            &value.to_string(),
            catalog(None, 40, 50),
            marks(None, false, 20),
        );
        assert_eq!(advance(&mut fx), RootPlanProgress::EffectReady);
        let run = fx.run();
        let work = run.context.root_plan_work().unwrap();
        let effect = work.effect().unwrap();
        assert!(effect.belongs_to(&run.context));
        assert_eq!(
            effect.input_sequence(),
            work.input_filing().unwrap().sequence()
        );
        let RootPlanInput::Accepted(input) = work.input() else {
            unreachable!()
        };
        assert_eq!(effect.document_sha256(), &input.context().document_sha256);
        assert!(
            run.context.root_plan_effect_identity(input).is_none(),
            "no second construction outside the owned driver"
        );
        let (plan, task, source) = match effect.effect() {
            s::PlanEffect::Authorize {
                plan, task, source, ..
            } if authorize => (plan, task, source),
            s::PlanEffect::Lock {
                plan,
                task,
                source: s::PlanLockSource::Operator { input },
                steps,
                ..
            } if !authorize => {
                assert_eq!(
                    steps
                        .iter()
                        .map(|step| step.step_id.local_id.0)
                        .collect::<Vec<_>>(),
                    [50, 51]
                );
                assert_eq!(
                    steps
                        .iter()
                        .map(|step| step.instruction.block_id.block.0)
                        .collect::<Vec<_>>(),
                    [1, 2]
                );
                assert!(steps.iter().all(|step| step.instruction.block_id.sequence.0
                    == effect.input_sequence().0
                    && step.instruction.binding.0 == 0));
                (plan, task, input)
            }
            _ => panic!("wrong root effect"),
        };
        assert_eq!(plan.plan_id.local_id.0, 40);
        assert_eq!(plan.revision.0, 1);
        assert_eq!(task.task.local_id.0, 20);
        assert_eq!(task.prompt.local_id.0, 21);
        assert_eq!(source.block.0, 0);
        assert_eq!(source.sequence.0, effect.input_sequence().0);
        assert_eq!(
            fx.records(Kind::PlanEvent).len(),
            1,
            "prepared effect is not yet authored"
        );
    }
}

#[test]
#[ignore = "actual root coordination peer"]
fn root_plan_effect_allows_the_last_step_id_once_but_refuses_wrap_before_effect() {
    for count in [1, 2] {
        let (mut fx, _boot) = ready();
        let mut value: serde_json::Value = serde_json::from_str(&filing_document()).unwrap();
        value["operation"]["create"]["steps"]
            .as_array_mut()
            .unwrap()
            .truncate(count);
        supplied(
            &mut fx,
            &value.to_string(),
            catalog(None, 40, u64::MAX),
            marks(None, false, 20),
        );
        let progress = advance(&mut fx);
        let work = fx.run().context.root_plan_work().unwrap();
        if count == 1 {
            assert_eq!(progress, RootPlanProgress::EffectReady);
            let s::PlanEffect::Lock { steps, .. } = work.effect().unwrap().effect() else {
                panic!("lock")
            };
            assert_eq!(steps[0].step_id.local_id.0, u64::MAX);
        } else {
            assert_eq!(progress, RootPlanProgress::RefusalReady);
            assert_eq!(
                work.refusal().unwrap().case,
                s::PlanUpdateCase::CounterExhausted
            );
            assert!(work.effect().is_none());
        }
        assert_eq!(fx.records(Kind::PlanEvent).len(), 1);
        assert!(fx.run().context.halt().is_none());
    }
}

fn previous(fx: &SummaryFixture) -> s::PlanSnapshot {
    let mut current = fx.snapshot();
    current.status = s::PlanStatus::Complete;
    current.lock_source = Some(current.operator_source.clone());
    current.review.sha256 = s::Digest::new("a".repeat(64)).unwrap();
    current.categories = vec![s::Category {
        id: "review".into(),
        verbs: vec!["read".into()],
        acts: vec![s::PlanAct::Statement],
        review_required: false,
    }];
    let id = s::StepId {
        seed_id: fx.seed().into(),
        local_id: 7.into(),
    };
    let span = |id| s::SpanRef {
        seed_id: fx.seed().into(),
        local_id: id,
        revision: 1.into(),
    };
    let source = |sequence| s::SourceBlockId {
        run: "old-run".into(),
        sequence,
        block: 0.into(),
    };
    current.steps = vec![s::PlanStep {
        step_id: id.clone(),
        ordinal: 1.into(),
        instruction: s::StatementRef {
            block_id: source(9.into()),
            binding: 0.into(),
        },
        category: "review".into(),
        group: None,
        status: s::StepStatus::Closed,
        span: Some(span(3.into())),
        outcome_span: Some(span(4.into())),
        review_call: None,
        work: Some(s::WorkRef {
            plan: current.plan.clone(),
            step_id: id,
            span: span(3.into()),
            through: s::SourcePosition {
                block_id: source(10.into()),
                turn: Some("old-turn".into()),
            },
            content_sha256: s::Digest::new("c".repeat(64)).unwrap(),
        }),
        closure_source: Some(source(11.into())),
    }];
    current
}
fn replacement(current: &s::PlanSnapshot, preserve: bool) -> String {
    let mut value: serde_json::Value = serde_json::from_str(&filing_document()).unwrap();
    let mut body = value["operation"]["create"].take();
    body["task"] = serde_json::Value::Null;
    body["steps"][0]["id"] = serde_json::to_value(&current.steps[0].step_id).unwrap();
    body["preserved_closures"] = if preserve {
        serde_json::json!([current.steps[0].step_id])
    } else {
        serde_json::json!([])
    };
    value["expected"] = serde_json::to_value(&current.plan).unwrap();
    value["operation"] = serde_json::json!({"replace":body});
    value.to_string()
}
fn old_entry(current: &s::PlanSnapshot, text: &str) -> s::ReadEntry {
    let source = current.steps[0].instruction.block_id.clone();
    s::ReadEntry {
        source: s::ReadSource::Event(Box::new(s::EventSource {
            block_id: source.clone(),
            envelope: s::SourceEnvelope {
                session: "old-session".into(),
                run: source.run,
                turn: Some("old-turn".into()),
                sequence: source.sequence,
                kind: "message.assistant".into(),
                channel: "output".into(),
                origin: "model".into(),
                subsystem: "harness".into(),
                causal_parent: None,
                wall_ms: 0,
                monotonic_ns: 10.into(),
            },
            source_path: vec![
                s::SourcePathPart::Key("content".into()),
                s::SourcePathPart::Index { index: 0.into() },
            ],
            partition_rule_id: "text".into(),
        })),
        text: text.into(),
        raw_block: serde_json::value::to_raw_value(&serde_json::json!({"type":"text","text":text}))
            .unwrap(),
        bindings: vec![s::ReadBinding {
            ordinal: 0.into(),
            value: s::Binding {
                statement_span: s::Span {
                    start: 0.into(),
                    end: 10.into(),
                },
                verb: "read".into(),
                verb_span: s::Span {
                    start: 0.into(),
                    end: 4.into(),
                },
                condition: None,
                condition_span: None,
                nouns: vec![s::NounBinding {
                    value: text[5..9].into(),
                    span: s::Span {
                        start: 5.into(),
                        end: 9.into(),
                    },
                }],
            },
        }],
        selected_span: None,
        step_query: None,
        pairs: vec![],
    }
}

#[test]
#[ignore = "actual root coordination peer"]
fn root_plan_effect_preserves_only_exact_statement_bytes_and_complete_category() {
    for changed in ["none", "text", "category"] {
        let (mut fx, _boot) = ready();
        let mut current = previous(&fx);
        if changed == "category" {
            current.categories[0].review_required = true;
        }
        let raw = replacement(&current, true);
        supplied(
            &mut fx,
            &raw,
            catalog(Some(current.clone()), 40, 50),
            marks(Some(&current), false, 20),
        );
        let bound = fx.run().context.root_plan_bound().unwrap();
        let request = next(&mut fx);
        assert!(
            matches!(&request,s::StateFrame::Control { frame:s::StateControl::Read { selector:s::ReadSelector::Statement { statement } },.. } if statement==&current.steps[0].instruction)
        );
        assert_eq!(
            fx.run()
                .context
                .deadlines
                .get(PumpDeadline::StateControl)
                .unwrap()
                .deadline_ns(),
            bound.deadline_ns()
        );
        let text = if changed == "text" {
            "Read data. Read plan."
        } else {
            "Read plan. Read data."
        };
        let bytes = serde_json::to_vec(&s::QueryResult::Entries {
            entries: vec![old_entry(&current, text)],
        })
        .unwrap();
        fx.boot
            .member
            .respond(&request, page(63, 0, bytes.len(), &bytes));
        let release = next(&mut fx);
        fx.boot.member.respond(
            &release,
            s::StateControlAnswer::Released {
                snapshot_id: 63.into(),
            },
        );
        let progress = (0..128)
            .find_map(|_| {
                let progress = advance(&mut fx);
                matches!(
                    progress,
                    RootPlanProgress::EffectReady | RootPlanProgress::RefusalReady
                )
                .then_some(progress)
            })
            .unwrap();
        let work = fx.run().context.root_plan_work().unwrap();
        assert_eq!(work.closures().len(), 1);
        if changed == "none" {
            assert_eq!(progress, RootPlanProgress::EffectReady);
            let s::PlanEffect::Replace {
                expected,
                next,
                task,
                steps,
                ..
            } = work.effect().unwrap().effect()
            else {
                panic!("replacement")
            };
            assert_eq!(expected, &current.plan);
            assert_eq!(next.revision.0, current.plan.revision.0 + 1);
            assert_eq!(
                steps[0].instruction, current.steps[0].instruction,
                "reuse original reference after comparing the binding slice"
            );
            assert_eq!(steps[0].step_id, current.steps[0].step_id);
            assert_eq!(steps[1].step_id.local_id.0, 50);
            assert_eq!(steps[1].instruction.block_id.block.0, 1);
            assert_eq!(task.task.local_id.0, 20, "terminal task gets a new scope");
        } else {
            assert_eq!(progress, RootPlanProgress::RefusalReady);
            assert_eq!(
                work.refusal().unwrap().case,
                s::PlanUpdateCase::ClosureChanged
            );
            assert!(work.effect().is_none());
        }
        assert_eq!(fx.records(Kind::PlanEvent).len(), 1);
        assert!(fx.run().context.halt().is_none());
    }
}

#[test]
#[ignore = "actual root coordination peer"]
fn root_plan_effect_closure_read_refusal_keeps_original_parent_and_no_recall() {
    let (mut fx, _boot) = ready();
    let current = previous(&fx);
    let raw = replacement(&current, true);
    supplied(
        &mut fx,
        &raw,
        catalog(Some(current.clone()), 40, 50),
        marks(Some(&current), false, 20),
    );
    let recalls = fx.records(Kind::Recall).len();
    let request = next(&mut fx);
    fx.boot
        .member
        .respond(&request, refused(s::ReadRefusalCode::NotFound));
    assert!((0..128).any(|_| advance(&mut fx) == RootPlanProgress::RefusalReady));
    let parent = fx.records(Kind::StateStoreMeasurement).last().unwrap()["sequence"].clone();
    let work = fx.run().context.root_plan_work().unwrap();
    assert_eq!(
        work.refusal().unwrap().read_code,
        Some(s::ReadRefusalCode::NotFound)
    );
    assert_eq!(
        serde_json::json!(work.read_refusal().unwrap().measurement_sequence()),
        parent
    );
    assert!(work.effect().is_none());
    assert_eq!(fx.records(Kind::Recall).len(), recalls);
    fx.boot.member.no_request();
}

#[test]
#[ignore = "actual root coordination peer"]
fn root_plan_effect_keeps_an_open_task_but_renews_a_changed_task() {
    for changed in [false, true] {
        let (mut fx, _boot) = ready();
        let mut current = previous(&fx);
        current.status = s::PlanStatus::Locked;
        current.steps[0].status = s::StepStatus::Waiting;
        current.steps[0].span = None;
        current.steps[0].outcome_span = None;
        current.steps[0].work = None;
        current.steps[0].closure_source = None;
        let mut document: serde_json::Value =
            serde_json::from_str(&replacement(&current, false)).unwrap();
        if changed {
            document["operation"]["replace"]["task"] = "Read plan.".into();
        }
        supplied(
            &mut fx,
            &document.to_string(),
            catalog(Some(current.clone()), 40, 50),
            marks(Some(&current), true, 20),
        );
        assert_eq!(advance(&mut fx), RootPlanProgress::EffectReady);
        let s::PlanEffect::Replace { task, steps, .. } = fx
            .run()
            .context
            .root_plan_work()
            .unwrap()
            .effect()
            .unwrap()
            .effect()
        else {
            panic!("replacement")
        };
        assert_eq!(
            task.task.local_id.0,
            if changed {
                20
            } else {
                current.task.task.local_id.0
            }
        );
        assert_eq!(steps[0].step_id, current.steps[0].step_id);
        assert_eq!(steps[0].instruction.block_id.block.0, u64::from(changed));
    }
}

const _: fn() = || {
    trait NoClone<Marker> {
        fn check() {}
    }
    impl<T: ?Sized> NoClone<()> for T {}
    impl<T: Clone> NoClone<u8> for T {}
    let _ = <crate::flow::plan::PreparedRootEffect as NoClone<_>>::check;
};
