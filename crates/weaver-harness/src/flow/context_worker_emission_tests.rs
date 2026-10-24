//! Actual worker/gate/classifier/decoder owners, with scripted model and member
//! replies. These instruments exercise joins, not language qualification.
use super::super::tests::{step, until};
use super::*;
use crate::flow::clock::testing::FakePumpIo;
use crate::flow::context::startup_composition::tests::CircuitIo;
use crate::flow::context::worker_startup::input::management::{
    FoundationManagementProgress,
    tests::{Guard, dial, read_packet, trace},
};
use crate::flow::context::worker_startup::tests::{Member, worker};
use nix::sys::socket::{MsgFlags, send};
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};
use weaver_traits::{ContentBlock, ToolCall};
use weaver_types::state_flow as s;
use weaver_types::{LifecycleAnswer, LifecycleDirective};

pub(super) static FAMILY: FoundationRestatement = FoundationRestatement::new("fixture.restatement");
static MISSING: FoundationRestatement = FoundationRestatement::new("fixture.missing");

#[test]
fn author_failure_worker_emission_keeps_exact_refusal_after_cancel() {
    use crate::flow::context::HeldInput;
    use crate::flow::context::held_input_tests::hold;
    use crate::flow::context::tests::{Fixture, config, event};

    let mut fx = Fixture::new(config());
    fx.activate();
    fx.context.complete_wait(PumpDeadline::Enter).unwrap();
    let held = hold(&mut fx.context, HeldInput::Both);
    let raw = serde_json::value::to_raw_value(&serde_json::json!({
        "fixture": "exact outbound refusal"
    }))
    .unwrap();
    let original = raw.get().as_ptr();
    let mut work = EmissionWork::new(None);
    work.refusal_author = Some(event(
        serving::Kind::Refusal,
        Some(serving::Payload::Refusal(raw)),
    ));
    let expected = format!("{:?}", work.refusal_author.as_ref().unwrap());
    work.stage = EmissionStage::RefusalAuthor;
    let before = fx.context.barrier.recorder().structure().len();
    let mut ports = PumpPorts {
        lifecycle: LifecyclePorts::default(),
        gate: None,
        decode: None,
        label: None,
        io: &mut fx.clock,
    };
    let failure = work
        .advance(&mut None, &mut None, &mut fx.context, &mut ports)
        .unwrap_err();
    assert_eq!(fx.context.failure(), Some(&failure));
    assert!(!work.refusal_started);
    assert!(work.refusal_record.is_none());
    assert!(work.refusal_author.is_none());
    work.cancel(&mut fx.context, None, ports.io).unwrap();
    assert!(work.stage == EmissionStage::RefusalAuthor);
    assert_eq!(
        work.advance(&mut None, &mut None, &mut fx.context, &mut ports),
        Err(failure)
    );
    let kept = work
        .returned_refusal
        .failure
        .as_ref()
        .unwrap()
        .original
        .as_ref()
        .unwrap();
    assert_eq!(format!("{kept:?}"), expected);
    let Some(serving::Payload::Refusal(raw)) = &kept.payload else {
        panic!("same refusal payload must stay with the emission owner")
    };
    assert_eq!(raw.get().as_ptr(), original);
    assert!(!work.refusal_started);
    assert!(work.refusal_record.is_none());
    assert!(fx.context.pending_author().is_none());
    assert_eq!(fx.context.barrier.recorder().structure().len(), before);
    held.assert_held(&fx.context);
}

pub(in crate::flow::context) fn templates(mode: &str) -> &'static [s::TemplateSpec] {
    static ALL: std::sync::OnceLock<Vec<s::TemplateSpec>> = std::sync::OnceLock::new();
    static NO_SORT: std::sync::OnceLock<Vec<s::TemplateSpec>> = std::sync::OnceLock::new();
    let all = ALL.get_or_init(|| {
        let mut all = super::super::tests::templates().to_vec();
        all.push(s::TemplateSpec {
            template_id: "fixture.restatement".into(),
            kind: "loop.instruction".into(),
            parts: vec![s::TemplatePart::Literal("Read correction.".into())],
        });
        all.push(s::TemplateSpec {
            template_id: "sort.outbound".into(),
            kind: "sort.decision".into(),
            parts: vec![
                s::TemplatePart::Literal("Read ".into()),
                s::TemplatePart::Slot {
                    name: "order".into(),
                    class: s::SlotClass::TechnicalName,
                },
                s::TemplatePart::Literal(".".into()),
            ],
        });
        all
    });
    if mode.contains("no-sort") {
        NO_SORT.get_or_init(|| {
            all.iter()
                .filter(|t| t.template_id != "sort.outbound")
                .cloned()
                .collect()
        })
    } else {
        all
    }
}
pub(in crate::flow::context) fn scheme(
    bootstrap: &mut s::Ste100Bootstrap,
    declaration: &mut s::SchemeDeclaration,
) {
    use sha2::{Digest as _, Sha256};
    let mut artifact: serde_json::Value = serde_json::from_str(&bootstrap.scheme_text).unwrap();
    let mut recall = artifact["verbs"][0].clone();
    recall["key"] = serde_json::json!("recall");
    recall["forms"] = serde_json::json!(["Recall"]);
    artifact["verbs"].as_array_mut().unwrap().push(recall);
    artifact["recall_keys"] = serde_json::json!(["recall"]);
    artifact["technical_names"] = serde_json::json!([{ "text": "identity", "authority": { "rule_id": "test-rule", "disposition": "technical_use" } }]);
    bootstrap.scheme_text = serde_json::to_string(&artifact).unwrap();
    let digest = s::Digest::new(format!(
        "{:x}",
        Sha256::digest(bootstrap.scheme_text.as_bytes())
    ))
    .unwrap();
    *declaration = s::SchemeDeclaration {
        scheme_id: format!("ste100:{digest}"),
        artifact_sha256: digest,
        verbs: vec!["read".into(), "recall".into()],
    };
}
fn text(words: &str) -> ContentBlock {
    ContentBlock::Text { text: words.into() }
}
pub(in crate::flow::context) fn content(mode: &str) -> Vec<ContentBlock> {
    if mode.ends_with("empty") && !mode.contains("correction") {
        return vec![];
    }
    if mode.ends_with("bindings") {
        return vec![text("Read zeta. Read alpha.")];
    }
    vec![
        text(
            if mode.contains("restatement")
                || mode.contains("repair-refusal")
                || mode.contains("correction")
            {
                "Read bad."
            } else {
                "Read blue file."
            },
        ),
        ContentBlock::ToolCall(ToolCall {
            name: "sh".into(),
            arguments: "opaque original command".into(),
        }),
        text("Read caf\u{e9}."),
        text(if mode.contains("invalid-recall") {
            "Recall blue file."
        } else {
            "Recall archive."
        }),
        text("Read blue file."),
    ]
}
pub(in crate::flow::context) fn reply(request: &s::Ste100Request, mode: &str) -> s::Ste100Reply {
    if request.text == "Read zeta. Read alpha." {
        let mut result = crate::flow::context::input_tests::accepted(request);
        let s::Ste100Result::Accepted { bindings } = &mut result.result else {
            unreachable!()
        };
        bindings[0].statement_span.end = 10.into();
        bindings[0].nouns[0].value = "zeta".into();
        bindings[0].nouns[0].span.end = 9.into();
        bindings.push(s::Binding {
            statement_span: s::Span {
                start: 11.into(),
                end: 22.into(),
            },
            verb: "read".into(),
            verb_span: s::Span {
                start: 11.into(),
                end: 15.into(),
            },
            condition: None,
            condition_span: None,
            nouns: vec![s::NounBinding {
                value: "alpha".into(),
                span: s::Span {
                    start: 16.into(),
                    end: 21.into(),
                },
            }],
        });
        return result;
    }
    let rejected = request.text == "Read bad."
        || (mode.contains("sort-refusal") && request.text == "Read identity.")
        || (mode.contains("repair-refusal") && request.text == "Read correction.");
    let result = if rejected {
        s::Ste100Result::NotSte100 {
            code: s::Ste100DefectCode::NonSte100,
            detail: "scripted runtime refusal".into(),
            spans: vec![],
        }
    } else {
        let recall = request.text.starts_with("Recall ");
        let start = if recall { 7 } else { 5 };
        assert!(request.text.ends_with('.'));
        let end = request.text.len() - 1;
        let nouns = if recall && mode.contains("invalid-recall") {
            vec![
                s::NounBinding {
                    value: "blue".into(),
                    span: s::Span {
                        start: 7.into(),
                        end: 11.into(),
                    },
                },
                s::NounBinding {
                    value: "file".into(),
                    span: s::Span {
                        start: 12.into(),
                        end: 16.into(),
                    },
                },
            ]
        } else {
            vec![s::NounBinding {
                value: request.text[start..end].into(),
                span: s::Span {
                    start: (start as u64).into(),
                    end: (end as u64).into(),
                },
            }]
        };
        s::Ste100Result::Accepted {
            bindings: vec![s::Binding {
                statement_span: s::Span {
                    start: 0.into(),
                    end: (request.text.len() as u64).into(),
                },
                verb: if recall {
                    "recall".into()
                } else {
                    "read".into()
                },
                verb_span: s::Span {
                    start: 0.into(),
                    end: (if recall { 6 } else { 4 }).into(),
                },
                condition: None,
                condition_span: None,
                nouns,
            }],
        }
    };
    s::Ste100Reply {
        request_id: request.request_id,
        target: request.target.clone(),
        scheme_id: request.scheme_id.clone(),
        result,
    }
}
pub(super) fn selection(view: FoundationEmission<'_>) -> Vec<SortAct> {
    let mut acts: Vec<_> = view
        .requests()
        .iter()
        .map(|request| match request.lane {
            s::PairLane::External => SortAct::Tool {
                call_id: request.call_id.clone(),
                statement: request.statement.clone(),
            },
            s::PairLane::Recall => SortAct::Recall {
                call_id: request.call_id.clone(),
                statement: request.statement.clone(),
            },
            _ => panic!("model cannot invent a review lane"),
        })
        .collect();
    if let Some(last) = view.statements().last() {
        acts.push(SortAct::Answer {
            source: last.block_id.clone(),
        });
    }
    acts
}
pub(super) fn sequence(row: &serde_json::Value) -> u64 {
    row["sequence"].as_str().unwrap().parse().unwrap()
}

fn await_decision_bound(
    input: &mut FoundationInput<'_>,
    member: &mut Member,
    clock: &mut FakePumpIo,
    mode: &str,
) -> AbsoluteBound {
    for _ in 0..2000 {
        let progress = step(input, member, clock, mode);
        if let Some(bound) = input
            .serving
            .owner
            .context
            .deadlines
            .get(PumpDeadline::Decision)
        {
            return bound;
        }
        assert_eq!(progress, FoundationInputProgress::Pending);
    }
    panic!("the exercised operation must arm a real Decision wait");
}

fn observe_completed_after_bound(
    guard: &mut Guard<'_>,
    clock: &mut FakePumpIo,
    socket: &Path,
    bound: AbsoluteBound,
) {
    clock.advance_to(bound.deadline_ns() + 1).unwrap();
    super::super::tests::observe(guard, clock, socket);
    let input = guard.input.as_ref().unwrap();
    assert!(input.serving.owner.context.failure().is_none());
    assert_eq!(
        input
            .serving
            .owner
            .context
            .deadlines
            .get(PumpDeadline::Decision),
        None
    );
}

pub(in crate::flow::context::worker_startup) fn exercise(
    serving: FoundationServing<'_>,
    member: &mut Member,
    reader: &mut UnixStream,
    clock: &mut FakePumpIo,
    socket: &Path,
    root: &Path,
    mode: &str,
) {
    let mut guard = Guard {
        input: Some(serving.input()),
        management: None,
    };
    let input = guard.input.as_mut().unwrap();
    assert_eq!(
        input.repair(&FAMILY),
        Err(FoundationRepairAdmission::WrongPhase)
    );
    assert!(input.sort_outbound(vec![]).is_err());
    until(
        input,
        member,
        clock,
        mode,
        FoundationInputProgress::Accepted,
    );
    let raw = input.received.as_ref().unwrap().raw_bytes().to_vec();
    input.start_task(TurnKey("generation-turn".into())).unwrap();
    until(input, member, clock, mode, FoundationInputProgress::Task);
    input
        .place_task("worker-task".into(), CutMethod::Flush)
        .unwrap();
    until(input, member, clock, mode, FoundationInputProgress::Placed);
    input.generate().unwrap();
    until(
        input,
        member,
        clock,
        mode,
        FoundationInputProgress::Generated,
    );
    assert!(input.effective_emission().is_none());
    assert!(input.sorted_acts().is_none());
    assert_eq!(
        input.repair(&MISSING),
        Err(FoundationRepairAdmission::UncertifiedFamily)
    );
    assert_eq!(input.progress(), FoundationInputProgress::Generated);
    let before = std::fs::read(root.join("appended.json")).unwrap();
    if mode.contains("correction") {
        return super::correction::tests::exercise(
            guard, member, reader, clock, socket, root, mode,
        );
    }
    if mode.contains("actual-worker") {
        return actual_worker(guard, member, reader, mode, raw);
    }
    if mode.ends_with("stop-generated") || mode.ends_with("stop-generated-invalid-recall") {
        stop(&mut guard, member, clock, mode, socket, root);
        assert_terminals(
            reader,
            if mode.contains("invalid-recall") {
                1
            } else {
                2
            },
        );
        return;
    }
    input.repair(&FAMILY).unwrap();
    if mode.ends_with("stop-begin") {
        stop(&mut guard, member, clock, mode, socket, root);
        assert_terminals(reader, 2);
        return;
    }
    if mode.contains("restatement") || mode.contains("repair-refusal") {
        let completed_bound = mode
            .contains("completed-observe")
            .then(|| await_decision_bound(input, member, clock, mode));
        let target = if mode.contains("repair-refusal") {
            FoundationInputProgress::OperationRefused
        } else {
            FoundationInputProgress::Restatement
        };
        until(input, member, clock, mode, target);
        assert!(input.effective_emission().is_none());
        assert!(input.sorted_acts().is_none());
        assert!(input.sort_outbound(vec![]).is_err());
        if target == FoundationInputProgress::Restatement {
            assert_eq!(input.restatement_source().unwrap().block.0, 0);
            assert!(input.serving.owner.certificate.is_none());
        } else {
            assert!(input.serving.owner.certificate.is_some());
        }
        assert_eq!(std::fs::read(root.join("appended.json")).unwrap(), before);
        if let Some(bound) = completed_bound {
            observe_completed_after_bound(&mut guard, clock, socket, bound);
        }
        stop(&mut guard, member, clock, mode, socket, root);
        let rows = trace(reader);
        assert_eq!(
            rows.iter()
                .filter(|r| r["kind"] == "message.assistant")
                .count(),
            1
        );
        assert_eq!(
            rows.iter()
                .filter(|r| r["kind"] == "loop.instruction")
                .count(),
            1
        );
        assert_eq!(
            rows.iter()
                .filter(|r| r["kind"] == "tool.call.completed")
                .count(),
            1
        );
        assert!(
            guard
                .input
                .as_ref()
                .unwrap()
                .serving
                .owner
                .certificate
                .is_some()
        );
        return;
    }
    until(
        input,
        member,
        clock,
        mode,
        FoundationInputProgress::Effective,
    );
    assert!(input.serving.owner.certificate.is_some());
    let view = input.effective_emission().unwrap();
    assert_eq!(view.finish(), &weaver_types::Finish::Completed);
    if mode.ends_with("empty") {
        assert!(view.statements().is_empty());
        assert!(view.subjects().is_empty());
        assert_eq!(view.text(), "");
        assert!(!view.attempted_call());
    } else if mode.ends_with("bindings") {
        assert_eq!(
            view.statements()
                .iter()
                .map(|s| s.binding.0)
                .collect::<Vec<_>>(),
            [0, 1]
        );
        assert_eq!(view.subjects(), ["alpha", "zeta"]);
        assert_eq!(view.text(), "Read zeta.\nRead alpha.");
        assert!(view.requests().is_empty());
        assert!(!view.attempted_call());
    } else {
        assert_eq!(
            view.statements()
                .iter()
                .map(|s| s.block_id.block.0)
                .collect::<Vec<_>>(),
            [0, 2, 3, 4]
        );
        assert!(view.attempted_call());
        assert_eq!(
            view.subjects(),
            if mode.contains("invalid-recall") {
                vec!["blue", "blue file", "caf\u{e9}", "file"]
            } else {
                vec!["archive", "blue file", "caf\u{e9}"]
            }
        );
        assert_eq!(
            view.requests().len(),
            if mode.contains("invalid-recall") {
                1
            } else {
                2
            }
        );
        assert_eq!(
            view.requests()[0].call_id.sequence,
            view.source().block_id.sequence
        );
        assert_eq!(view.requests()[0].statement.block_id.block.0, 0);
        if mode.contains("invalid-recall") {
            assert_eq!(
                input
                    .engine
                    .as_ref()
                    .unwrap()
                    .emission
                    .as_ref()
                    .unwrap()
                    .effective
                    .as_ref()
                    .unwrap()
                    .account()
                    .recall_refusals()
                    .len(),
                1
            );
        }
    }
    let mut acts = selection(view);
    if mode.ends_with("no-answer") {
        acts.retain(|a| !matches!(a, SortAct::Answer { .. }));
    }
    if mode.ends_with("pair-receipt") {
        member.hold_kind = Some("tool.call.completed");
        stop(&mut guard, member, clock, mode, socket, root);
        assert_terminals(reader, 2);
        return;
    }
    if mode.ends_with("stop-effective") {
        stop(&mut guard, member, clock, mode, socket, root);
        assert_terminals(reader, 2);
        return;
    }
    if mode.ends_with("answer-order") {
        acts.push(SortAct::Answer {
            source: input.effective_emission().unwrap().statements()[0]
                .block_id
                .clone(),
        });
    }
    if mode.ends_with("duplicate") {
        acts.insert(0, acts[0].clone());
    }
    if mode.ends_with("foreign") {
        let SortAct::Tool { call_id, .. } = &mut acts[0] else {
            panic!()
        };
        call_id.run = "foreign".into();
    }
    if mode.ends_with("wrong-statement") {
        let SortAct::Tool { statement, .. } = &mut acts[0] else {
            panic!()
        };
        statement.binding = 9.into();
    }
    if mode.ends_with("wrong-lane") {
        let SortAct::Tool { call_id, statement } = acts.remove(0) else {
            panic!()
        };
        acts.insert(0, SortAct::Recall { call_id, statement });
    }
    let invalid = [
        "duplicate",
        "foreign",
        "wrong-statement",
        "wrong-lane",
        "answer-order",
    ]
    .iter()
    .any(|suffix| mode.ends_with(suffix));
    let expected = acts.clone();
    if mode.contains("held") {
        member.hold_kind = Some("sort.decision");
    }
    input.sort_outbound(acts).unwrap();
    assert!(input.effective_emission().is_none());
    assert!(input.sorted_acts().is_none());
    assert!(input.sort_outbound(vec![]).is_err());
    if mode.ends_with("stop-outbound-begin") {
        stop(&mut guard, member, clock, mode, socket, root);
        assert_terminals(reader, 2);
        return;
    }
    if mode.contains("held") {
        for _ in 0..2000 {
            step(input, member, clock, mode);
            if member.held_file.is_some() {
                break;
            }
        }
        assert!(member.held_file.is_some());
        assert_eq!(input.progress(), FoundationInputProgress::Pending);
        assert!(input.sorted_acts().is_none());
        let bound = input
            .serving
            .owner
            .context
            .deadlines
            .get(PumpDeadline::Decision)
            .unwrap();
        for _ in 0..8 {
            step(input, member, clock, mode);
        }
        assert_eq!(
            input
                .serving
                .owner
                .context
                .deadlines
                .get(PumpDeadline::Decision),
            Some(bound)
        );
        assert!(!root.join("closed.json").exists());
        if mode.ends_with("deadline") {
            clock.advance_to(bound.deadline_ns()).unwrap();
            assert_eq!(
                input.advance_with_io(&mut CircuitIo(clock)),
                FoundationInputProgress::Stopped
            );
            assert!(input.sorted_acts().is_none());
            assert!(input.serving.owner.context.failure().is_some());
            assert!(input.engine.as_ref().unwrap().emission.is_some());
            assert!(member.held_file.is_some());
            return;
        }
        if mode.contains("stop-") {
            stop(&mut guard, member, clock, mode, socket, root);
            assert_terminals(reader, 2);
            return;
        }
        if mode.contains("observe") {
            super::super::tests::observe(&mut guard, clock, socket);
        }
        member.hold_kind = None;
    }
    let input = guard.input.as_mut().unwrap();
    let refused = invalid || mode.contains("sort-refusal") || mode.contains("no-sort");
    let completed_bound = mode
        .contains("completed-observe")
        .then(|| await_decision_bound(input, member, clock, mode));
    until(
        input,
        member,
        clock,
        mode,
        if refused {
            FoundationInputProgress::OperationRefused
        } else {
            FoundationInputProgress::Sorted
        },
    );
    assert_eq!(std::fs::read(root.join("appended.json")).unwrap(), before);
    assert_eq!(input.received.as_ref().unwrap().raw_bytes(), raw);
    assert!(input.serving.owner.context.failure().is_none());
    let rows = trace(reader);
    assert!(
        !rows
            .iter()
            .any(|r| r["kind"] == "turn.closed" || r["kind"] == "tool.call.started")
    );
    if refused {
        assert!(input.sorted_acts().is_none());
        assert!(input.operation_detail().is_some());
        if invalid {
            assert!(!rows.iter().any(|r| r["kind"] == "sort.decision"));
        }
        if mode.contains("no-sort") {
            assert!(rows.iter().any(|r| r["kind"] == "refusal"));
        }
        if mode.contains("sort-refusal") {
            assert!(rows.iter().any(|r| r["kind"] == "fault"));
        }
        if let Some(bound) = completed_bound {
            observe_completed_after_bound(&mut guard, clock, socket, bound);
        }
        stop(&mut guard, member, clock, mode, socket, root);
        assert_terminals(reader, 2);
        return;
    }
    assert_eq!(input.sorted_acts().unwrap(), expected);
    let sort = rows.iter().find(|r| r["kind"] == "sort.decision").unwrap();
    let assistant = rows
        .iter()
        .find(|r| r["kind"] == "message.assistant")
        .unwrap();
    assert!(sequence(sort) > sequence(assistant));
    assert_eq!(sort["payload"]["template_id"], "sort.outbound");
    assert_eq!(member.filings.iter().filter(|f| matches!(f, s::StateFrame::File { filing, .. } if filing.source.kind == "sort.decision")).count(), usize::from(!mode.ends_with("unselected")));
    let FoundationInputResult::Sorted(sorted) = guard.input.take().unwrap().finish().ok().unwrap()
    else {
        panic!()
    };
    assert_eq!(sorted.acts(), expected);
    guard.input = Some(sorted.into_service());
    if let Some(bound) = completed_bound {
        observe_completed_after_bound(&mut guard, clock, socket, bound);
    }
    if mode.ends_with("stop-sorted") || completed_bound.is_some() {
        stop(&mut guard, member, clock, mode, socket, root);
        assert_terminals(reader, 2);
    }
}

pub(super) fn stop(
    guard: &mut Guard<'_>,
    member: &mut Member,
    clock: &mut FakePumpIo,
    mode: &str,
    socket: &Path,
    root: &Path,
) {
    let peer = dial(socket);
    let request = OrganEnvelope {
        exchange: weaver_types::ExchangeId {
            opener: Opener::Admin,
            ordinal: 91,
        },
        position: Position::Open,
        payload: Payload::Directive(LifecycleDirective::Stop {
            cause: weaver_types::Cause { uid: 73 },
        }),
    };
    let bytes = serde_json::to_vec(&request).unwrap();
    assert_eq!(
        send(peer.as_raw_fd(), &bytes, MsgFlags::MSG_NOSIGNAL),
        Ok(bytes.len())
    );
    for _ in 0..500 {
        let input = guard.input.as_mut().unwrap();
        input.observe(&mut CircuitIo(clock)).unwrap();
        if input.progress() == FoundationInputProgress::ManagementPending {
            break;
        }
    }
    guard.management = Some(guard.input.take().unwrap().management().ok().unwrap());
    if member.hold_kind.is_some() {
        for _ in 0..2000 {
            if member.held_file.is_some() {
                break;
            }
            let p = guard
                .management
                .as_mut()
                .unwrap()
                .advance_with_io(&mut CircuitIo(clock));
            assert_eq!(p, FoundationManagementProgress::Pending);
            member.step(clock.now().unwrap(), mode);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(member.held_file.is_some());
        if mode.contains("pair-receipt") {
            assert_eq!(member.filings.iter().filter(|f| matches!(f, s::StateFrame::FilePair { filing, .. } if filing.source.kind == "tool.call.completed")).count(), if mode.contains("second-pair") { 2 } else { 1 });
        }
        for _ in 0..8 {
            guard
                .management
                .as_mut()
                .unwrap()
                .advance_with_io(&mut CircuitIo(clock));
        }
        assert!(read_packet(&peer).is_none());
        assert!(!root.join("closed.json").exists());
        if mode.ends_with("eof") {
            nix::sys::socket::shutdown(peer.as_raw_fd(), nix::sys::socket::Shutdown::Both).unwrap();
        }
        member.hold_kind = None;
    }
    for _ in 0..2500 {
        let management = guard.management.as_mut().unwrap();
        let p = management.advance_with_io(&mut CircuitIo(clock));
        member.step(clock.now().unwrap(), mode);
        if p != FoundationManagementProgress::Pending {
            assert_eq!(
                p,
                if mode.ends_with("eof") {
                    FoundationManagementProgress::Dropped
                } else {
                    FoundationManagementProgress::Answered
                },
                "{:?}",
                management.detail()
            );
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_ne!(
        guard.management.as_ref().unwrap().progress(),
        FoundationManagementProgress::Pending
    );
    guard.input = Some(guard.management.take().unwrap().finish().ok().unwrap());
    assert_eq!(
        guard.input.as_ref().unwrap().progress(),
        if mode.contains("refusal-close") {
            FoundationInputProgress::Refused
        } else {
            FoundationInputProgress::Aborted
        }
    );
    assert!(guard.input.as_ref().unwrap().sorted_acts().is_none());
    assert!(guard.input.as_ref().unwrap().effective_emission().is_none());
    if !mode.ends_with("eof") {
        let answer: OrganEnvelope = serde_json::from_slice(&read_packet(&peer).unwrap()).unwrap();
        assert_eq!(answer.exchange, request.exchange);
        if mode.contains("refusal-close") {
            assert!(matches!(
                answer.payload,
                Payload::Answer(LifecycleAnswer::AtRest)
            ));
        } else {
            assert!(matches!(
                answer.payload,
                Payload::Answer(LifecycleAnswer::TurnAborted { .. })
            ));
        }
    }
}
fn assert_terminals(reader: &mut UnixStream, count: usize) {
    let rows = trace(reader);
    assert_eq!(
        rows.iter()
            .filter(|r| r["kind"] == "tool.call.started")
            .count(),
        count
    );
    assert_eq!(
        rows.iter()
            .filter(|r| r["kind"] == "tool.call.completed")
            .count(),
        count
    );
    let close = rows.iter().find(|r| r["kind"] == "turn.closed").unwrap();
    assert!(
        rows.iter()
            .filter(|r| r["kind"] == "tool.call.completed")
            .all(|r| sequence(r) < sequence(close))
    );
}
fn actual_worker(
    mut guard: Guard<'_>,
    member: &mut Member,
    reader: &mut UnixStream,
    mode: &str,
    raw: Vec<u8>,
) {
    let done = std::sync::atomic::AtomicBool::new(false);
    let input = std::thread::scope(|scope| {
        let responder = scope.spawn(|| {
            let end = Instant::now() + Duration::from_secs(8);
            while !done.load(std::sync::atomic::Ordering::SeqCst) && Instant::now() < end {
                member.step(MonotonicIo.now().unwrap(), mode);
                std::thread::sleep(Duration::from_millis(1));
            }
        });
        let input = worker::repair(guard.input.take().unwrap(), &FAMILY);
        assert_eq!(
            input.progress(),
            FoundationInputProgress::Effective,
            "{:?}",
            input.detail()
        );
        let acts = selection(input.effective_emission().unwrap());
        let input = worker::outbound(input, acts);
        done.store(true, std::sync::atomic::Ordering::SeqCst);
        responder.join().unwrap();
        input
    });
    guard.input = Some(input);
    let input = guard.input.as_ref().unwrap();
    assert_eq!(
        input.progress(),
        FoundationInputProgress::Sorted,
        "{:?} / {:?}",
        input.detail(),
        input.operation_detail()
    );
    assert_eq!(input.received.as_ref().unwrap().raw_bytes(), raw);
    assert_eq!(input.sorted_acts().unwrap().len(), 3);
    let rows = trace(reader);
    assert_eq!(
        rows.iter().filter(|r| r["kind"] == "sort.decision").count(),
        1
    );
    assert!(
        !rows
            .iter()
            .any(|r| r["kind"] == "turn.closed" || r["kind"] == "tool.call.started")
    );
}
