//! Whole result grants cross the actual worker/recorder/decoder joins. The
//! model and member remain scripted peers, not qualification artifacts.
use super::emission::tests::{FAMILY, selection, sequence, stop};
use super::tests::{step, until};
use super::*;
use crate::flow::clock::testing::FakePumpIo;
use crate::flow::context::worker_startup::input::management::tests::{Guard, trace};
use crate::flow::context::worker_startup::tests::{Member, worker};
use nix::sys::socket::{MsgFlags, recv, send};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};
use weaver_traits::{ContentBlock, Message, Role};
use weaver_types::{Finish, Generation, TokenAnswer, TokenDirective, state_flow as s};

fn answer(fd: i32, value: TokenAnswer) {
    let bytes = serde_json::to_vec(&value).unwrap();
    assert_eq!(send(fd, &bytes, MsgFlags::MSG_NOSIGNAL), Ok(bytes.len()));
}
fn generation(mode: &str, pass: u64, stopped: bool) -> TokenAnswer {
    TokenAnswer::Generated(Generation {
        emission: "fixture return pass".into(),
        finish: if stopped {
            Finish::Stopped
        } else {
            Finish::Completed
        },
        content: if pass == 0 || mode.contains("-final-return-round-limit") {
            super::emission::tests::content(mode)
        } else {
            vec![ContentBlock::Text {
                text: "Read result.".into(),
            }]
        },
        unrecovered: vec![],
        request: serde_json::value::RawValue::from_string(format!(
            "{{ \"pass\" : {pass}, \"rendered\" : \"fixture\" }}"
        ))
        .unwrap(),
        measurement: serde_json::value::RawValue::from_string(
            "{ \"model\":\"fixture\", \"input_tokens\":[17,18,19], \"output_tokens\":[31] }".into(),
        )
        .unwrap(),
        resident: 8 + 5 * pass,
        capacity: 16384,
    })
}
pub(in crate::flow::context) fn spu(mode: &str, fd: i32) {
    let root = std::path::PathBuf::from(std::env::var("WEAVER_WORKER_INPUT_ROOT").unwrap());
    let mut bytes = vec![0; weaver_types::MAX_ENVELOPE_BYTES];
    let end = Instant::now() + Duration::from_secs(30);
    let mut pass = 0;
    let mut measured = None;
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
                        assert!(measured.is_none());
                        assert_eq!(*keep, 3 + 5 * pass);
                        if mode.contains("-external-") && !mode.contains("no-grant") {
                            assert_eq!(tool_schemas, &[crate::tools::shell_schema().to_owned()]);
                        } else {
                            assert!(tool_schemas.is_empty());
                        }
                        if pass == 0 {
                            assert_eq!(
                                delta,
                                &[Message {
                                    role: Role::User,
                                    content: vec![ContentBlock::Text {
                                        text: "Read caf\u{e9}.".into()
                                    }]
                                }]
                            );
                        } else {
                            assert!(!delta.is_empty());
                            assert!(delta.iter().all(|m| m.role == Role::ToolResult
                                && matches!(m.content.as_slice(), [ContentBlock::ToolResult(_)])));
                        }
                        std::fs::write(
                            root.join(if pass == 0 {
                                "measured.json"
                            } else {
                                "return-measured.json"
                            }),
                            &bytes[..n],
                        )
                        .unwrap();
                        measured = Some((delta.clone(), tool_schemas.clone()));
                        answer(
                            fd,
                            TokenAnswer::DeltaMeasured {
                                resident: *keep,
                                prefix_tokens: 3,
                                keep: *keep,
                                input_tokens: if pass > 0 && mode.contains("fit-refusal") {
                                    20000
                                } else {
                                    3
                                },
                                capacity: 16384,
                            },
                        );
                    }
                    TokenDirective::AppendAndGenerate {
                        delta,
                        tool_schemas,
                        ..
                    } => {
                        if mode.contains("-final-") {
                            std::fs::write(
                                root.join(format!("final-pass-{pass}.json")),
                                &bytes[..n],
                            )
                            .unwrap();
                        }
                        let (original, schemas) = measured.take().unwrap();
                        assert_eq!(delta, &original);
                        assert_eq!(tool_schemas, &schemas);
                        std::fs::write(
                            root.join(if pass == 0 {
                                "appended.json"
                            } else {
                                "return-appended.json"
                            }),
                            &bytes[..n],
                        )
                        .unwrap();
                        if pass > 0 && mode.contains("slow-decode") {
                            while !root.join("release-generation").exists() {
                                assert!(Instant::now() < end);
                                std::thread::sleep(Duration::from_millis(1));
                            }
                        }
                        if pass == 0 || !mode.contains("stop-generating") {
                            answer(fd, generation(mode, pass, false));
                            pass += 1;
                        }
                    }
                    TokenDirective::Cancel { .. } => {
                        assert!(mode.contains("stop-generating") && pass == 1);
                        answer(fd, generation(mode, pass, true));
                        answer(fd, TokenAnswer::AtRest);
                        pass += 1;
                    }
                    other => panic!("unexpected return decoder request: {other:?}"),
                }
            }
            Err(nix::errno::Errno::EAGAIN) => {}
            other => panic!("decoder peer: {other:?}"),
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("return decoder watchdog");
}

#[allow(clippy::too_many_arguments)]
pub(in crate::flow::context::worker_startup) fn exercise(
    mut guard: Guard<'_>,
    member: &mut Member,
    reader: &mut UnixStream,
    clock: &mut FakePumpIo,
    socket: &Path,
    root: &Path,
    mode: &str,
    all: Vec<s::CallId>,
) {
    let input = guard.input.as_mut().unwrap();
    let Some(GenerationCompletion::Generated(previous)) =
        &input.engine.as_ref().unwrap().generation
    else {
        panic!("actual previous generation")
    };
    let previous_raw = previous.raw_bytes().to_vec();
    let mut calls = all.clone();
    if mode.contains("reverse") {
        calls.reverse();
    }
    if mode.contains("subset") {
        calls.truncate(1);
    }
    let context = &input.serving.owner.context;
    let expected: Vec<_> = calls
        .iter()
        .map(|call| Message {
            role: Role::ToolResult,
            content: vec![ContentBlock::ToolResult(
                context.pairs.take_result(call).unwrap().result().block(),
            )],
        })
        .collect();
    let mut foreign = calls[0].clone();
    foreign.call.0 += 100;
    assert_eq!(
        input.place_results(vec![foreign.clone()], "foreign".into(), CutMethod::Flush),
        Err(vec![foreign])
    );
    assert!(
        input
            .place_results(
                vec![calls[0].clone(), calls[0].clone()],
                "duplicate".into(),
                CutMethod::Flush
            )
            .is_err()
    );
    assert_eq!(input.progress(), FoundationInputProgress::PairsComplete);
    if mode.contains("actual-worker") {
        let done = std::sync::atomic::AtomicBool::new(false);
        guard.input = Some(std::thread::scope(|scope| {
            let responder = scope.spawn(|| {
                while !done.load(std::sync::atomic::Ordering::SeqCst) {
                    member.step_live(mode);
                    std::thread::sleep(Duration::from_millis(1));
                }
            });
            let input = worker::place_results(
                guard.input.take().unwrap(),
                calls.clone(),
                "results".into(),
                CutMethod::Flush,
            );
            assert_eq!(
                input.progress(),
                FoundationInputProgress::Placed,
                "{:?}",
                input.detail()
            );
            let input = worker::generate(input);
            done.store(true, std::sync::atomic::Ordering::SeqCst);
            responder.join().unwrap();
            input
        }));
    } else {
        input
            .place_results(calls.clone(), "results".into(), CutMethod::Flush)
            .unwrap();
        assert!(input.generate().is_err());
        if mode.contains("stop-election") {
            stop(&mut guard, member, clock, mode, socket, root);
        } else {
            until(
                guard.input.as_mut().unwrap(),
                member,
                clock,
                mode,
                if mode.contains("fit-refusal") {
                    FoundationInputProgress::Refused
                } else {
                    FoundationInputProgress::Placed
                },
            );
            assert!(!root.join("return-appended.json").exists());
            if mode.contains("stop-placed") {
                stop(&mut guard, member, clock, mode, socket, root);
            } else if !mode.contains("fit-refusal") {
                let input = guard.input.as_mut().unwrap();
                input.generate().unwrap();
                if mode.contains("slow-decode") {
                    super::tests::slow_generation(
                        input,
                        member,
                        clock,
                        root,
                        mode,
                        "return-appended.json",
                    );
                }
                if mode.contains("stop-generating") {
                    for _ in 0..1000 {
                        assert_eq!(
                            step(input, member, clock, mode),
                            FoundationInputProgress::Pending
                        );
                        if root.join("return-appended.json").exists() {
                            break;
                        }
                    }
                    assert!(root.join("return-appended.json").exists());
                    stop(&mut guard, member, clock, mode, socket, root);
                } else {
                    until(
                        input,
                        member,
                        clock,
                        mode,
                        FoundationInputProgress::Generated,
                    );
                    if mode.contains("stop-generated") {
                        stop(&mut guard, member, clock, mode, socket, root);
                    }
                }
            }
        }
    }
    let input = guard.input.as_mut().unwrap();
    let appended = root.join("return-appended.json").exists();
    for call in &all {
        let result = input.serving.owner.context.pairs.take_result(call);
        if appended && calls.contains(call) {
            assert!(matches!(
                result,
                Err(crate::flow::context::pairs::PairError::AlreadyDelivered)
            ));
        } else {
            assert!(result.is_ok());
        }
    }
    if appended {
        let request: TokenDirective =
            serde_json::from_slice(&std::fs::read(root.join("return-appended.json")).unwrap())
                .unwrap();
        let TokenDirective::AppendAndGenerate { delta, .. } = request else {
            unreachable!()
        };
        assert_eq!(delta, expected);
    }
    let rows = trace(reader);
    let results: Vec<_> = rows
        .iter()
        .filter(|r| r["kind"] == "message.tool_result")
        .collect();
    assert_eq!(
        results.len(),
        if mode.contains("fit-refusal") || mode.contains("stop-election") {
            0
        } else {
            calls.len()
        }
    );
    for (record, message) in results.iter().zip(&expected) {
        assert_eq!(record["payload"], serde_json::to_value(message).unwrap());
        assert_eq!(record["origin"], "tool");
        let terminal = record["causal_parent"]
            .as_str()
            .unwrap()
            .parse::<u64>()
            .unwrap();
        assert!(
            rows.iter()
                .any(|r| r["kind"] == "tool.call.completed" && sequence(r) == terminal)
        );
        assert!(sequence(record) > terminal);
    }
    assert_eq!(
        rows.iter()
            .filter(|r| r["kind"] == "tool.call.started")
            .count(),
        all.len()
    );
    assert_eq!(
        rows.iter()
            .filter(|r| r["kind"] == "tool.call.completed")
            .count(),
        all.len()
    );
    if !mode.contains("stop-") && !mode.contains("fit-refusal") {
        assert_eq!(input.progress(), FoundationInputProgress::Generated);
        assert_eq!(input.engine.as_ref().unwrap().rounds.len(), 1);
        let round = &input.engine.as_ref().unwrap().rounds[0];
        let Some(GenerationCompletion::Generated(previous)) = &round._generation else {
            panic!("previous decoder custody retained")
        };
        assert_eq!(previous.raw_bytes(), previous_raw);
        assert_eq!(
            round._emission.as_ref().unwrap().progress(),
            FoundationInputProgress::PairsComplete
        );
        // The next emission goes through the same actual repair/sort/pair path.
        input.repair(&FAMILY).unwrap();
        until(
            input,
            member,
            clock,
            mode,
            FoundationInputProgress::Effective,
        );
        let acts = selection(input.effective_emission().unwrap());
        input.sort_outbound(acts).unwrap();
        until(input, member, clock, mode, FoundationInputProgress::Sorted);
        input.begin_pairs().unwrap();
        until(
            input,
            member,
            clock,
            mode,
            FoundationInputProgress::PairsComplete,
        );
        assert!(
            input
                .place_results(calls, "redelivery".into(), CutMethod::Flush)
                .is_err()
        );
    }
}
