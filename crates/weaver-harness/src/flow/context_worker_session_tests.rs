//! Multiple real worker turns and management exchanges share one entered
//! context. The member/model peers are scripted and do not qualify deployment.
use super::emission::tests::FAMILY;
use super::*;
use crate::flow::clock::testing::FakePumpIo;
use crate::flow::context::worker_startup::input::management::tests::{Guard, dial, read_packet};
use crate::flow::context::worker_startup::tests::{Member, worker};
use nix::sys::socket::{MsgFlags, recv, send};
use std::io::Read;
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use weaver_traits::{ContentBlock, Message, Role};
use weaver_types::{
    Finish, Generation, LifecycleAnswer, LifecycleDirective, TokenAnswer, TokenDirective,
    state_flow as s,
};
use worker::session::{Progress, Session};

macro_rules! packet {
    ($fd:expr, $value:expr $(,)?) => {{
        let bytes = serde_json::to_vec($value).unwrap();
        assert_eq!(send($fd, &bytes, MsgFlags::MSG_NOSIGNAL), Ok(bytes.len()));
    }};
}
fn wait_file(root: &Path, name: &str) {
    let end = Instant::now() + Duration::from_secs(25);
    while !root.join(name).exists() {
        assert!(Instant::now() < end, "missing {name}");
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn turns(mode: &str) -> usize {
    if mode.ends_with("three") { 3 } else { 2 }
}
fn frame(index: usize, malformed: bool) -> OrganEnvelope {
    OrganEnvelope {
        exchange: weaver_types::ExchangeId {
            opener: Opener::Gate,
            ordinal: 101 + index as u64,
        },
        position: Position::Open,
        payload: Payload::Frame(weaver_types::TurnFrame::carry_from(
            if malformed {
                b"not JSON".as_slice()
            } else {
                br#"{"text":"Read caf\u00e9."}"#
            },
            42,
        )),
    }
}
pub(in crate::flow::context) fn gate(mode: &str, root: &Path, fd: i32) {
    let mut bytes = vec![0; weaver_types::MAX_ENVELOPE_BYTES];
    packet!(fd, &frame(0, mode.ends_with("refused")));
    if mode.ends_with("queued") {
        packet!(fd, &frame(1, false));
        std::fs::write(root.join("session-queued"), b"yes").unwrap();
    }
    for index in 0..turns(mode) {
        if index > 0 && !mode.ends_with("queued") {
            wait_file(root, &format!("session-input-{index}"));
            packet!(fd, &frame(index, false));
        }
        let n = recv(fd, &mut bytes, MsgFlags::empty()).unwrap_or(0);
        if n == 0 {
            return;
        }
        let reply: OrganEnvelope = serde_json::from_slice(&bytes[..n]).unwrap();
        assert_eq!(reply.exchange, frame(index, false).exchange);
        assert_eq!(reply.position, Position::Close);
        let Payload::Frame(frame) = &reply.payload else {
            panic!("actual world close")
        };
        let value: serde_json::Value = serde_json::from_slice(&frame.octets().unwrap()).unwrap();
        assert_eq!(
            value["kind"],
            if index == 0 && mode.ends_with("refused") {
                "refused"
            } else if index == 0 && mode.ends_with("stop-active") {
                "stopped"
            } else {
                "answered"
            }
        );
        if value["kind"] != "refused" {
            assert_eq!(value["turn"], format!("session-{index}"));
        }
        std::fs::write(
            root.join(format!("session-close-{index}.json")),
            &bytes[..n],
        )
        .unwrap();
    }
    // The same gate remains alive until the test's retained owner reaps it.
    let _ = recv(fd, &mut bytes, MsgFlags::empty());
}

pub(in crate::flow::context) fn spu(mode: &str, fd: i32) {
    let root = std::path::PathBuf::from(std::env::var("WEAVER_WORKER_INPUT_ROOT").unwrap());
    let mut bytes = vec![0; weaver_types::MAX_ENVELOPE_BYTES];
    let mut resident = 3;
    let mut measured = None;
    let mut generated = 0;
    let mut pending = None;
    loop {
        let n = recv(fd, &mut bytes, MsgFlags::empty()).unwrap_or(0);
        if n == 0 {
            return;
        }
        let request: TokenDirective = serde_json::from_slice(&bytes[..n]).unwrap();
        match request {
            TokenDirective::MeasureDelta {
                keep,
                delta,
                tool_schemas,
            } => {
                assert!(measured.is_none());
                assert_eq!(keep, resident, "next turn keeps the exact prior argument");
                assert_eq!(
                    delta,
                    [Message {
                        role: Role::User,
                        content: vec![ContentBlock::Text {
                            text: "Read caf\u{e9}.".into()
                        }]
                    }]
                );
                assert!(tool_schemas.is_empty());
                measured = Some(delta);
                packet!(
                    fd,
                    &TokenAnswer::DeltaMeasured {
                        resident,
                        prefix_tokens: 3,
                        keep,
                        input_tokens: 3,
                        capacity: 16384,
                    },
                );
            }
            TokenDirective::AppendAndGenerate {
                turn,
                delta,
                tool_schemas,
            } => {
                assert_eq!(delta, measured.take().unwrap());
                assert!(tool_schemas.is_empty());
                let index = generated + usize::from(mode.ends_with("refused"));
                assert_eq!(turn.0, format!("session-{index}"));
                std::fs::write(
                    root.join(format!("session-generation-{index}.json")),
                    &bytes[..n],
                )
                .unwrap();
                if mode.ends_with("decoder-failure") {
                    return;
                }
                resident += 5;
                let generation = Generation {
                    emission: "Read file.".into(),
                    finish: Finish::Completed,
                    content: vec![ContentBlock::Text {
                        text: "Read file.".into(),
                    }],
                    unrecovered: vec![],
                    request: serde_json::value::RawValue::from_string(format!(
                        "{{\"session_turn\":{index}}}"
                    ))
                    .unwrap(),
                    measurement: serde_json::value::RawValue::from_string(
                        "{\"input_tokens\":[17,18,19],\"output_tokens\":[31]}".into(),
                    )
                    .unwrap(),
                    resident,
                    capacity: 16384,
                };
                generated += 1;
                if index == 0 && mode.ends_with("stop-active") {
                    pending = Some((turn, generation));
                } else {
                    packet!(fd, &TokenAnswer::Generated(generation));
                }
            }
            TokenDirective::Cancel { turn } => {
                let (original, mut generation) = pending.take().expect("actual pending generation");
                assert_eq!(turn, original);
                generation.finish = Finish::Stopped;
                packet!(fd, &TokenAnswer::Generated(generation));
                packet!(fd, &TokenAnswer::AtRest);
            }
            other => panic!("unexpected session decode: {other:?}"),
        }
    }
}

pub(in crate::flow::context) fn marks(filings: &[s::StateFrame]) -> s::QueryResult {
    let held: Vec<_> = filings
        .iter()
        .filter_map(|frame| {
            let s::StateFrame::File { filing, .. } = frame else {
                return None;
            };
            let Some(s::StatementEffect::Mark(s::MarkEffect::BeginTask { task, prompt, .. })) =
                &filing.effect
            else {
                return None;
            };
            Some((filing, task, prompt))
        })
        .collect();
    assert!(held.len() <= 1, "the open task must be reused");
    match held.first() {
        None => s::QueryResult::Marks {
            next_span_id: 1.into(),
            current_task: None,
            open_scopes: vec![],
        },
        Some((filing, task, prompt)) => s::QueryResult::Marks {
            next_span_id: (prompt.local_id.0 + 1).into(),
            current_task: Some(s::TaskScopeRef {
                task: (*task).clone(),
                prompt: (*prompt).clone(),
            }),
            open_scopes: vec![s::SpanInfo {
                span: (*task).clone(),
                parent: None,
                kind: s::SpanKind::Task,
                status: s::SpanStatus::Open,
                opened_source: Some(filing.block_id.clone()),
                ended_source: None,
                replacement: None,
            }],
        },
    }
}

struct StopThreads<'a>(&'a AtomicBool);
impl Drop for StopThreads<'_> {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

struct SessionGuard<'a>(Option<Session<'a>>);
impl Drop for SessionGuard<'_> {
    fn drop(&mut self) {
        let mut guard = Guard {
            input: None,
            management: None,
        };
        if let Some(session) = self.0.take() {
            match session.into_owner() {
                Ok(input) => guard.input = Some(input),
                Err(owner) => guard.management = Some(owner),
            }
        }
    }
}
fn drive(session: &mut Session<'_>, wanted: Progress) {
    let end = Instant::now() + Duration::from_secs(20);
    while session.progress() != wanted && Instant::now() < end {
        assert!(
            matches!(session.progress(), Progress::Pending | Progress::Accepted),
            "{:?}: {:?}",
            session.progress(),
            session.detail()
        );
        session.advance();
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(session.progress(), wanted, "{:?}", session.detail());
}
fn management(socket: &Path, directive: LifecycleDirective) -> std::os::fd::OwnedFd {
    let peer = dial(socket);
    packet!(
        peer.as_raw_fd(),
        &OrganEnvelope {
            exchange: weaver_types::ExchangeId {
                opener: Opener::Admin,
                ordinal: 99,
            },
            position: Position::Open,
            payload: Payload::Directive(directive),
        },
    );
    peer
}

pub(in crate::flow::context::worker_startup) fn exercise(
    serving: FoundationServing<'_>,
    member: &mut Member,
    reader: &mut UnixStream,
    _clock: &mut FakePumpIo,
    socket: &Path,
    root: &Path,
    mode: &str,
) {
    let context_identity = std::ptr::from_ref(&*serving.owner.context);
    let mut guard = SessionGuard(Some(Session::begin(serving, &FAMILY, CutMethod::Flush)));
    let session = guard.0.as_mut().unwrap();
    assert!(session.next_input().is_err());
    assert!(session.start_turn(TurnKey("early".into())).is_err());
    let done = AtomicBool::new(false);
    reader.set_nonblocking(true).unwrap();
    let rows = std::thread::scope(|scope| {
        let responder = scope.spawn(|| {
            let end = Instant::now() + Duration::from_secs(45);
            while !done.load(Ordering::SeqCst) && Instant::now() < end {
                member.step(MonotonicIo.now().unwrap(), mode);
                std::thread::sleep(Duration::from_millis(1));
            }
        });
        let recorder = scope.spawn(|| {
            let end = Instant::now() + Duration::from_secs(45);
            let mut bytes = vec![];
            while !done.load(Ordering::SeqCst) && Instant::now() < end {
                let _ = reader.read_to_end(&mut bytes);
                std::thread::sleep(Duration::from_millis(1));
            }
            let _ = reader.read_to_end(&mut bytes);
            bytes
                .split(|b| *b == b'\n')
                .filter(|s| !s.is_empty())
                .map(|s| serde_json::from_slice::<serde_json::Value>(s).unwrap())
                .collect::<Vec<_>>()
        });
        let _stop_threads = StopThreads(&done);
        for index in 0..turns(mode) {
            if index > 0 {
                if mode.ends_with("observe")
                    || mode.ends_with("stop-idle")
                    || mode.ends_with("leave")
                {
                    let peer = management(
                        socket,
                        if mode.ends_with("stop-idle") {
                            LifecycleDirective::Stop {
                                cause: weaver_types::Cause { uid: 73 },
                            }
                        } else if mode.ends_with("leave") {
                            LifecycleDirective::Leave {
                                cause: weaver_types::Cause { uid: 73 },
                                forced: false,
                                rollback: false,
                            }
                        } else {
                            LifecycleDirective::Observe
                        },
                    );
                    if mode.ends_with("leave") {
                        drive(session, Progress::DirectivePending);
                        assert_eq!(session.next_input(), Err(Progress::DirectivePending));
                        assert!(read_packet(&peer).is_none());
                        break;
                    }
                    let end = Instant::now() + Duration::from_secs(10);
                    let raw = loop {
                        session.advance();
                        if let Some(raw) = read_packet(&peer) {
                            break raw;
                        }
                        assert!(
                            Instant::now() < end,
                            "idle management: {:?}",
                            session.detail()
                        );
                    };
                    if mode.ends_with("observe") {
                        let answer: weaver_types::state_flow::enter::ObservationEnvelope =
                            serde_json::from_slice(&raw).unwrap();
                        assert!(matches!(
                            answer.payload,
                            weaver_types::state_flow::enter::ObservationContent::Answer(
                                weaver_types::state_flow::enter::ObservationAnswer::State {
                                    state: weaver_types::AgentState::Idle,
                                    ..
                                }
                            )
                        ));
                    } else {
                        let answer: OrganEnvelope = serde_json::from_slice(&raw).unwrap();
                        assert!(matches!(
                            answer.payload,
                            Payload::Answer(LifecycleAnswer::AtRest)
                        ));
                    }
                }
                std::fs::write(root.join(format!("session-input-{index}")), b"yes").unwrap();
            }
            if index == 0 && mode.ends_with("refused") {
                drive(session, Progress::Refused);
                wait_file(root, "session-close-0.json");
                assert_eq!(session.next_input(), Ok(Progress::Refused));
                continue;
            }
            drive(session, Progress::Accepted);
            if index == 0 && mode.ends_with("accepted") {
                let peer = management(
                    socket,
                    if mode.ends_with("stop-accepted") {
                        LifecycleDirective::Stop {
                            cause: weaver_types::Cause { uid: 73 },
                        }
                    } else {
                        LifecycleDirective::Observe
                    },
                );
                let end = Instant::now() + Duration::from_secs(10);
                let raw = loop {
                    session.advance();
                    if let Some(raw) = read_packet(&peer) {
                        break raw;
                    }
                    assert!(Instant::now() < end, "accepted management");
                };
                let answer: serde_json::Value = serde_json::from_slice(&raw).unwrap();
                if mode.ends_with("stop-accepted") {
                    let typed: OrganEnvelope = serde_json::from_value(answer).unwrap();
                    assert!(matches!(
                        typed.payload,
                        Payload::Answer(LifecycleAnswer::AtRest)
                    ));
                } else {
                    let typed: weaver_types::state_flow::enter::ObservationEnvelope =
                        serde_json::from_value(answer).unwrap();
                    assert!(matches!(
                        typed.payload,
                        weaver_types::state_flow::enter::ObservationContent::Answer(
                            weaver_types::state_flow::enter::ObservationAnswer::State {
                                state: weaver_types::AgentState::Idle,
                                ..
                            }
                        )
                    ));
                }
                drive(session, Progress::Accepted);
            }
            assert!(session.start_turn(TurnKey(String::new())).is_err());
            assert!(session.next_input().is_err());
            for _ in 0..3 {
                assert_eq!(
                    session.advance(),
                    Progress::Accepted,
                    "acceptance alone never starts a task"
                );
                assert!(
                    !root
                        .join(format!("session-generation-{index}.json"))
                        .exists()
                );
            }
            let duplicate = index == 1 && mode.ends_with("duplicate");
            session
                .start_turn(TurnKey(format!(
                    "session-{}",
                    if duplicate { 0 } else { index }
                )))
                .unwrap();
            assert!(session.start_turn(TurnKey("overlap".into())).is_err());
            assert!(session.next_input().is_err());
            if mode.ends_with("decoder-failure") {
                drive(session, Progress::Stopped);
                assert_eq!(session.next_input(), Err(Progress::Stopped));
                assert!(session.detail().is_some());
                assert!(!root.join("session-close-0.json").exists());
                break;
            }
            if duplicate {
                drive(session, Progress::Refused);
                assert_eq!(
                    session.next_input(),
                    Err(Progress::Refused),
                    "unfinished refusal cannot discard its input"
                );
                assert!(!root.join("session-generation-1.json").exists());
                assert!(!root.join("session-close-1.json").exists());
                break;
            }
            let active_stop = index == 0 && mode.ends_with("stop-active");
            if active_stop {
                let stopper = scope.spawn(|| {
                    wait_file(root, "session-generation-0.json");
                    management(
                        socket,
                        LifecycleDirective::Stop {
                            cause: weaver_types::Cause { uid: 73 },
                        },
                    )
                });
                drive(session, Progress::Aborted);
                let peer = stopper.join().unwrap();
                let reply: OrganEnvelope =
                    serde_json::from_slice(&read_packet(&peer).unwrap()).unwrap();
                assert!(matches!(
                    reply.payload,
                    Payload::Answer(LifecycleAnswer::TurnAborted { .. })
                ));
            } else {
                drive(session, Progress::Completed);
            }
            wait_file(root, &format!("session-close-{index}.json"));
            for _ in 0..3 {
                assert_eq!(
                    session.advance(),
                    if active_stop {
                        Progress::Aborted
                    } else {
                        Progress::Completed
                    }
                );
                assert!(
                    !root
                        .join(format!("session-generation-{}.json", index + 1))
                        .exists()
                );
            }
            assert_eq!(
                session.next_input(),
                Ok(if active_stop {
                    Progress::Aborted
                } else {
                    Progress::Completed
                })
            );
        }
        done.store(true, Ordering::SeqCst);
        responder.join().unwrap();
        recorder.join().unwrap()
    });
    let failed = mode.ends_with("decoder-failure");
    let special = mode.ends_with("leave") || mode.ends_with("duplicate") || failed;
    let expected_turns = if special || mode.ends_with("refused") {
        1
    } else {
        turns(mode)
    };
    let source: Vec<_> = rows
        .iter()
        .filter(|r| r["kind"] == "message.user")
        .collect();
    assert_eq!(source.len(), expected_turns);
    assert_eq!(
        rows.iter().filter(|r| r["kind"] == "turn.closed").count(),
        if failed { 0 } else { expected_turns }
    );
    assert_eq!(
        rows.iter().filter(|r| r["kind"] == "state.mark").count(),
        1,
        "one open task survives turn close"
    );
    assert_eq!(
        member
            .requests
            .iter()
            .filter(|r| r.as_str() == "marks")
            .count(),
        expected_turns
    );
    assert!(
        source
            .windows(2)
            .all(|p| p[0]["sequence"] != p[1]["sequence"] && p[0]["turn"] != p[1]["turn"])
    );
    assert_eq!(
        member
            .requests
            .iter()
            .filter(|r| r.as_str() == "open")
            .count(),
        1
    );
    let marks = marks(&member.filings);
    assert!(
        matches!(marks, s::QueryResult::Marks { current_task: Some(_), open_scopes, .. } if open_scopes.len() == 1)
    );
    if mode.ends_with("leave") {
        assert_eq!(session.progress(), Progress::DirectivePending);
        assert!(!root.join("session-generation-1.json").exists());
    } else if !mode.ends_with("duplicate") && !failed {
        assert_eq!(session.progress(), Progress::Pending);
    }
    let closes: Vec<_> = rows.iter().filter(|r| r["kind"] == "turn.closed").collect();
    let offers: Vec<_> = rows
        .iter()
        .filter(|r| r["kind"] == "input.offered")
        .collect();
    assert_eq!(
        offers.len(),
        if mode.ends_with("leave") || failed {
            1
        } else {
            turns(mode)
        }
    );
    if offers.len() > 1 && !mode.ends_with("refused") {
        assert!(
            super::emission::tests::sequence(offers[1])
                > super::emission::tests::sequence(closes[0]),
            "queued input intake waits for the old world close"
        );
    }
    let session = guard.0.take().unwrap();
    let mut retained = Guard {
        input: None,
        management: None,
    };
    match session.into_owner() {
        Ok(input) => {
            assert_eq!(
                std::ptr::from_ref(&*input.serving.owner.context),
                context_identity
            );
            if failed {
                assert!(input.serving.owner.context.failure().is_some());
                assert!(input.serving.owner.context.generation.is_some());
                assert!(input.received.is_some());
            }
            if mode.ends_with("duplicate") {
                assert!(
                    input.token.is_some(),
                    "refused election retains the new accepted input"
                );
                assert!(input.received.is_some());
            }
            retained.input = Some(input);
        }
        Err(owner) => {
            assert!(mode.ends_with("leave"));
            assert_eq!(
                owner.progress(),
                super::super::management::FoundationManagementProgress::DirectivePending
            );
            retained.management = Some(owner);
        }
    }
}
