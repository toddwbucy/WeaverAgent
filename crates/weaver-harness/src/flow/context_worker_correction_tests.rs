//! Actual worker correction joins with scripted model/member peers. This is
//! process and custody evidence, not language or production qualification.
use super::super::tests::{FAMILY, selection, sequence, stop};
use super::*;
use crate::flow::clock::testing::FakePumpIo;
use crate::flow::context::worker_startup::input::generation::tests::{step, until};
use crate::flow::context::worker_startup::input::management::tests::{Guard, trace};
use crate::flow::context::worker_startup::tests::{Member, worker};
use nix::sys::socket::{MsgFlags, recv, send};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};
use weaver_traits::{ContentBlock, Message, Role};
use weaver_types::{Finish, Generation, TokenAnswer, TokenDirective};

fn generated(mode: &str, attempt: usize, stopped: bool) -> TokenAnswer {
    let content = if attempt == 0 {
        super::super::tests::content(mode)
    } else if attempt == 1 && mode.contains("retry-empty") {
        vec![]
    } else if attempt == 1 && mode.contains("retry-call") {
        vec![
            ContentBlock::Text {
                text: "Read file.".into(),
            },
            ContentBlock::ToolCall(weaver_traits::ToolCall {
                name: "sh".into(),
                arguments: "forbidden correction command".into(),
            }),
        ]
    } else {
        vec![ContentBlock::Text {
            text: if (attempt == 1 && mode.contains("retry-text")) || mode.ends_with("cap") {
                "Read bad."
            } else {
                "Read fixed."
            }
            .into(),
        }]
    };
    TokenAnswer::Generated(Generation {
        emission: "fixture correction".into(),
        finish: if stopped {
            Finish::Stopped
        } else {
            Finish::Completed
        },
        content,
        unrecovered: vec![],
        request: serde_json::value::RawValue::from_string(format!(
            "{{ \"attempt\" : {attempt}, \"rendered\" : \"fixture\" }}"
        ))
        .unwrap(),
        measurement: serde_json::value::RawValue::from_string(
            "{ \"model\":\"fixture\", \"input_tokens\":[17,18,19], \"output_tokens\":[31] }".into(),
        )
        .unwrap(),
        resident: 8 + 5 * attempt as u64,
        capacity: 16384,
    })
}
fn packet(fd: i32, answer: TokenAnswer) {
    let bytes = serde_json::to_vec(&answer).unwrap();
    assert_eq!(send(fd, &bytes, MsgFlags::MSG_NOSIGNAL), Ok(bytes.len()));
}
pub(in crate::flow::context) fn spu(mode: &str, fd: i32) {
    let root = std::path::PathBuf::from(std::env::var("WEAVER_WORKER_INPUT_ROOT").unwrap());
    let mut bytes = vec![0; weaver_types::MAX_ENVELOPE_BYTES];
    let end = Instant::now() + Duration::from_secs(30);
    let mut attempt = 0;
    let mut measured = None;
    let mut pending = None;
    while Instant::now() < end {
        match recv(fd, &mut bytes, MsgFlags::MSG_DONTWAIT) {
            Ok(0) => return,
            Ok(n) => {
                if bytes[0] == b'x' {
                    continue;
                }
                let request: TokenDirective = serde_json::from_slice(&bytes[..n]).unwrap();
                match &request {
                    TokenDirective::MeasureDelta {
                        keep,
                        delta,
                        tool_schemas,
                    } => {
                        assert!(measured.is_none() && pending.is_none());
                        assert_eq!(
                            *keep,
                            if attempt == 0 {
                                3
                            } else {
                                3 + 5 * attempt as u64
                            }
                        );
                        assert_eq!(
                            delta,
                            &[Message {
                                role: Role::User,
                                content: vec![ContentBlock::Text {
                                    text: if attempt == 0 {
                                        "Read caf\u{e9}."
                                    } else {
                                        "Read correction."
                                    }
                                    .into()
                                }]
                            }]
                        );
                        assert!(tool_schemas.is_empty());
                        let file = if attempt == 0 {
                            "measured.json".into()
                        } else {
                            format!("correction-measured-{attempt}.json")
                        };
                        std::fs::write(root.join(file), &bytes[..n]).unwrap();
                        measured = Some((delta.clone(), tool_schemas.clone()));
                        if attempt == 1 && mode.ends_with("measure-refusal") {
                            let bytes =
                                serde_json::to_vec(&weaver_types::TokenRefusal::MalformedDelta)
                                    .unwrap();
                            assert_eq!(send(fd, &bytes, MsgFlags::MSG_NOSIGNAL), Ok(bytes.len()));
                            continue;
                        }
                        packet(
                            fd,
                            TokenAnswer::DeltaMeasured {
                                resident: *keep,
                                prefix_tokens: 3,
                                keep: *keep,
                                input_tokens: if attempt == 1 && mode.contains("fit-refusal") {
                                    20000
                                } else {
                                    3
                                },
                                capacity: 16384,
                            },
                        );
                    }
                    TokenDirective::AppendAndGenerate {
                        turn,
                        delta,
                        tool_schemas,
                    } => {
                        assert_eq!(turn.0, "generation-turn");
                        let (original, schemas) =
                            measured.take().expect("exact measurement precedes decode");
                        assert_eq!(*delta, original);
                        assert_eq!(*tool_schemas, schemas);
                        let file = if attempt == 0 {
                            "appended.json".into()
                        } else {
                            format!("correction-appended-{attempt}.json")
                        };
                        std::fs::write(root.join(file), &bytes[..n]).unwrap();
                        if attempt == 1 && mode.ends_with("stop-generation") {
                            pending = Some(attempt);
                        } else {
                            packet(fd, generated(mode, attempt, false));
                        }
                        attempt += 1;
                    }
                    TokenDirective::Cancel { turn } => {
                        assert_eq!(turn.0, "generation-turn");
                        let attempt = pending
                            .take()
                            .expect("only an outstanding decode can be cancelled");
                        std::fs::write(root.join("correction-cancelled.json"), &bytes[..n])
                            .unwrap();
                        packet(fd, generated(mode, attempt, true));
                        packet(fd, TokenAnswer::AtRest);
                    }
                    _ => panic!("unexpected correction decoder request {request:?}"),
                }
            }
            Err(nix::errno::Errno::EAGAIN) => {}
            other => panic!("correction decode fixture: {other:?}"),
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("correction child watchdog");
}

pub(in crate::flow::context) fn exercise(
    mut guard: Guard<'_>,
    member: &mut Member,
    reader: &mut UnixStream,
    clock: &mut FakePumpIo,
    socket: &Path,
    root: &Path,
    mode: &str,
) {
    let input = guard.input.as_mut().unwrap();
    assert!(
        input
            .place_restatement("too early".into(), CutMethod::Flush)
            .is_err()
    );
    assert!(input.generate_correction().is_err());
    if mode.contains("actual-worker") {
        return actual_worker(guard, member, reader, mode, root);
    }
    if mode.contains("held-filing") {
        member.hold_kind = Some("loop.instruction");
    }
    input.repair(&FAMILY).unwrap();
    until(
        input,
        member,
        clock,
        mode,
        FoundationInputProgress::Restatement,
    );
    let source = input.restatement_source().unwrap().clone();
    assert_eq!(source.block.0, 0);
    assert!(!root.join("correction-measured-1.json").exists());
    assert!(!root.join("correction-appended-1.json").exists());
    if mode.ends_with("stop-restatement") {
        stop(&mut guard, member, clock, mode, socket, root);
        return;
    }
    let count = if mode.contains("retry-") || mode.ends_with("cap") {
        2
    } else {
        1
    };
    for attempt in 1..=count {
        let input = guard.input.as_mut().unwrap();
        input
            .place_restatement("worker-correction".into(), CutMethod::Flush)
            .unwrap();
        assert!(
            input
                .place_restatement("duplicate".into(), CutMethod::Flush)
                .is_err()
        );
        if mode.contains("refusal") {
            until(
                input,
                member,
                clock,
                mode,
                FoundationInputProgress::OperationRefused,
            );
            assert!(input.serving.owner.context.failure().is_none());
            assert!(input.serving.owner.context.open_turn.is_some());
            assert!(input.serving.owner.context.repair.is_some());
            assert!(input.serving.owner.context.place_close().is_none());
            assert!(!root.join("correction-appended-1.json").exists());
            input.close_correction_refusal().unwrap();
            if mode.contains("refusal-close") {
                member.hold_kind = Some("tool.call.completed");
                stop(&mut guard, member, clock, mode, socket, root);
            } else {
                until(input, member, clock, mode, FoundationInputProgress::Refused);
            }
            let rows = trace(reader);
            let close = sequence(rows.iter().find(|r| r["kind"] == "turn.closed").unwrap());
            assert_eq!(
                rows.iter()
                    .filter(|r| r["kind"] == "tool.call.completed")
                    .count(),
                1
            );
            if !mode.contains("refusal-close") {
                assert!(
                    rows.iter()
                        .filter(|r| r["kind"] == "tool.call.completed")
                        .all(|r| r.to_string().contains("pass_purpose"))
                );
            }
            assert_eq!(
                rows.iter().filter(|r| r["kind"] == "turn.closed").count(),
                1
            );
            assert!(
                rows.iter()
                    .filter(|r| r["kind"] == "tool.call.completed")
                    .all(|r| sequence(r) < close)
            );
            return;
        }
        if attempt == 1 && mode.contains("held-filing") {
            for _ in 0..80 {
                assert_eq!(
                    step(input, member, clock, mode),
                    FoundationInputProgress::Pending
                );
            }
            assert!(member.held_file.is_some());
            assert!(!root.join("correction-measured-1.json").exists());
            assert!(!root.join("correction-appended-1.json").exists());
            member.hold_kind = None;
        }
        until(
            input,
            member,
            clock,
            mode,
            FoundationInputProgress::CorrectionPlaced,
        );
        assert!(
            root.join(format!("correction-measured-{attempt}.json"))
                .exists()
        );
        assert!(
            !root
                .join(format!("correction-appended-{attempt}.json"))
                .exists()
        );
        assert!(
            input
                .serving
                .owner
                .context
                .deadlines
                .get(PumpDeadline::Decision)
                .is_none(),
            "no classifier/filing wait runs during decode"
        );
        if mode.ends_with("stop-placed") {
            stop(&mut guard, member, clock, mode, socket, root);
            return;
        }
        input.generate_correction().unwrap();
        assert!(input.generate_correction().is_err());
        if mode.ends_with("held-decode") {
            use crate::flow::context::held_input_tests::hold;
            let held = hold(
                input.serving.owner.context,
                crate::flow::context::HeldInput::Both,
            );
            for _ in 0..24 {
                assert_eq!(
                    step(input, member, clock, mode),
                    FoundationInputProgress::Pending
                );
                held.assert_held(input.serving.owner.context);
            }
            assert!(!root.join("correction-appended-1.json").exists());
            held.take(input.serving.owner.context);
        }
        if mode.ends_with("stop-generation") || mode.ends_with("stop-checks") {
            let end = Instant::now() + Duration::from_secs(5);
            loop {
                assert!(Instant::now() < end);
                assert_eq!(
                    step(input, member, clock, mode),
                    FoundationInputProgress::Pending
                );
                if (mode.ends_with("stop-generation")
                    && root.join("correction-appended-1.json").exists())
                    || (mode.ends_with("stop-checks")
                        && input.serving.owner.context.output.is_some())
                {
                    break;
                }
            }
            stop(&mut guard, member, clock, mode, socket, root);
            let work = guard
                .input
                .as_ref()
                .unwrap()
                .engine
                .as_ref()
                .unwrap()
                .emission
                .as_ref()
                .unwrap();
            let account = work.cancelled_repair.as_ref().unwrap().account();
            let expected = usize::from(mode.contains("retry-") && mode.ends_with("stop-checks"));
            assert_eq!(account.failed_corrections().len(), expected);
            if expected == 1 {
                assert_eq!(account.failed_corrections()[0].source(), &source);
                assert_eq!(account.failed_corrections()[0].failed_corrections(), 1);
                assert!(
                    account.failed_corrections()[0]
                        .correction()
                        .block_id
                        .sequence
                        > source.sequence
                );
            }
            let rows = trace(reader);
            assert!(!root.join("correction-appended-2.json").exists());
            assert!(rows.iter().any(|row| row["kind"] == "turn.closed"));
            return;
        }
        until(
            input,
            member,
            clock,
            mode,
            if attempt < count {
                FoundationInputProgress::Restatement
            } else if mode.ends_with("cap") {
                FoundationInputProgress::OperationRefused
            } else {
                FoundationInputProgress::Effective
            },
        );
        if attempt < count {
            assert_eq!(input.restatement_source(), Some(&source));
        }
    }
    let input = guard.input.as_mut().unwrap();
    if mode.ends_with("cap") {
        let work = input.engine.as_ref().unwrap().emission.as_ref().unwrap();
        assert_eq!(work.exhausted.as_ref().unwrap().failed_corrections().0, 2);
        assert!(input.effective_emission().is_none());
        assert!(!root.join("correction-appended-3.json").exists());
        stop(&mut guard, member, clock, mode, socket, root);
        return;
    }
    let view = input.effective_emission().unwrap();
    assert!(view.text().starts_with("Read fixed."));
    assert_eq!(view.requests().len(), 2);
    assert_eq!(view.requests()[0].call_id.run, source.run);
    assert_eq!(view.requests()[0].call_id.sequence, source.sequence);
    assert_eq!(view.requests()[0].call_id.call.0, 0);
    assert_ne!(view.requests()[0].statement.block_id, source);
    assert_eq!(view.requests()[0].statement, view.statements()[0]);
    let corrected_requests = view.requests().to_vec();
    let mut acts = selection(view);
    if mode.ends_with("correction-inward") {
        acts.retain(|act| !matches!(act, SortAct::Tool { .. }));
    }
    input.sort_outbound(acts).unwrap();
    until(input, member, clock, mode, FoundationInputProgress::Sorted);
    assert_eq!(
        input
            .engine
            .as_ref()
            .unwrap()
            .emission
            .as_ref()
            .unwrap()
            .corrections
            .len(),
        count
    );
    if mode.ends_with("correction-inward") {
        input.begin_pairs().unwrap();
        until(
            input,
            member,
            clock,
            mode,
            FoundationInputProgress::PairReady,
        );
        assert_eq!(input.ready_pair(), Some(&corrected_requests[1]));
        input.recall(corrected_requests[1].call_id.clone()).unwrap();
        until(
            input,
            member,
            clock,
            mode,
            FoundationInputProgress::PairsComplete,
        );
        assert!(
            input
                .pair_outcome(&corrected_requests[0].call_id)
                .unwrap()
                .outcome
                .is_none()
        );
        assert!(
            input
                .pair_outcome(&corrected_requests[1].call_id)
                .unwrap()
                .outcome
                .is_some()
        );
    }
    stop(&mut guard, member, clock, mode, socket, root);
    let rows = trace(reader);
    if mode.ends_with("correction-inward") {
        let starts: Vec<_> = rows
            .iter()
            .filter(|r| r["kind"] == "tool.call.started")
            .collect();
        assert_eq!(starts.len(), corrected_requests.len());
        for (row, request) in starts.iter().zip(corrected_requests) {
            assert_eq!(
                row["payload"]["call_id"],
                serde_json::to_value(request.call_id).unwrap()
            );
            assert_eq!(
                row["payload"]["statement"],
                serde_json::to_value(request.statement).unwrap()
            );
        }
    }
    let instructions: Vec<_> = rows
        .iter()
        .filter(|r| r["kind"] == "loop.instruction")
        .collect();
    assert_eq!(instructions.len(), count);
    assert_eq!(
        rows.iter()
            .filter(|r| r["kind"] == "message.assistant")
            .count(),
        count + 1
    );
    for instruction in instructions {
        let id = sequence(instruction);
        let placement = rows
            .iter()
            .find(|r| {
                r["kind"] == "state.placement"
                    && r.to_string().contains("worker-correction")
                    && r.to_string().contains(&format!("\"sequence\":\"{id}\""))
            })
            .expect("placement retains actual instruction source");
        assert!(id < sequence(placement));
    }
    assert!(rows.iter().any(|r| r["kind"] == "sort.decision"));
}

fn actual_worker(
    mut guard: Guard<'_>,
    member: &mut Member,
    reader: &mut UnixStream,
    mode: &str,
    root: &Path,
) {
    let done = std::sync::atomic::AtomicBool::new(false);
    guard.input = Some(std::thread::scope(|scope| {
        let responder = scope.spawn(|| {
            let end = Instant::now() + Duration::from_secs(8);
            while !done.load(std::sync::atomic::Ordering::SeqCst) && Instant::now() < end {
                member.step_live(mode);
                std::thread::sleep(Duration::from_millis(1));
            }
        });
        let input = worker::repair(guard.input.take().unwrap(), &FAMILY);
        assert_eq!(
            input.progress(),
            FoundationInputProgress::Restatement,
            "{:?}",
            input.detail()
        );
        let input = worker::place_restatement(input, "worker-correction".into(), CutMethod::Elide);
        if mode.contains("fit-refusal") {
            if input.progress() != FoundationInputProgress::Refused {
                let work = input.engine.as_ref().unwrap().emission.as_ref().unwrap();
                eprintln!(
                    "refusal stuck at {:?}, correction {:?}, account {}, close {}, pairs {}, pending author {}, pending flow {}",
                    work.stage,
                    work.corrections.last().unwrap().stage,
                    work.account.is_some(),
                    work.correction_close.is_some(),
                    input.serving.owner.context.pair_work.is_some(),
                    input.serving.owner.context.pending_author().is_some(),
                    input.serving.owner.context.flow.has_pending_record()
                );
            }
            assert_eq!(
                input.progress(),
                FoundationInputProgress::Refused,
                "{:?}",
                input.detail()
            );
            done.store(true, std::sync::atomic::Ordering::SeqCst);
            responder.join().unwrap();
            return input;
        }
        assert_eq!(
            input.progress(),
            FoundationInputProgress::CorrectionPlaced,
            "{:?}",
            input.detail()
        );
        assert!(!root.join("correction-appended-1.json").exists());
        let input = worker::generate_correction(input);
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
    }));
    if mode.contains("fit-refusal") {
        assert!(!root.join("correction-appended-1.json").exists());
        let rows = trace(reader);
        let close = sequence(rows.iter().find(|r| r["kind"] == "turn.closed").unwrap());
        assert_eq!(
            rows.iter()
                .filter(|r| r["kind"] == "tool.call.completed")
                .count(),
            1
        );
        assert!(
            rows.iter()
                .filter(|r| r["kind"] == "tool.call.completed")
                .all(|r| sequence(r) < close)
        );
        return;
    }
    assert_eq!(
        guard.input.as_ref().unwrap().progress(),
        FoundationInputProgress::Sorted
    );
    let rows = trace(reader);
    assert_eq!(
        rows.iter()
            .filter(|r| r["kind"] == "message.assistant")
            .count(),
        2
    );
    assert_eq!(
        rows.iter()
            .filter(|r| r["kind"] == "loop.instruction")
            .count(),
        1
    );
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
