use super::*;
use crate::flow::clock::testing::FakePumpIo;
use crate::flow::context::startup_composition::tests::CircuitIo;
use crate::flow::context::worker_startup::tests::{Member, reap};
use nix::sys::socket::{MsgFlags, recv, send};
use std::io::Read;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};
use weaver_types::{ExchangeId, LifecycleAnswer, LifecycleDirective, TurnFrame};

use super::super::tests::worker;

fn packet(fd: i32, value: &OrganEnvelope) {
    let bytes = serde_json::to_vec(value).unwrap();
    assert_eq!(
        send(fd, &bytes, MsgFlags::MSG_NOSIGNAL).unwrap(),
        bytes.len()
    );
}
fn frame(mode: &str, ordinal: u64) -> OrganEnvelope {
    let mode = if mode == "input-two" && ordinal == 101 {
        "input-malformed"
    } else {
        mode
    };
    let bytes = match mode {
        "input-malformed" | "input-pressure" | "input-duplicate" | "input-worker" => {
            b"not JSON".as_slice()
        }
        "input-role" => br#"{"text":"Read file.","role":"system"}"#,
        _ => br#"{"text":"Read caf\u00e9."}"#,
    };
    let mut input = TurnFrame::carry_from(bytes, if mode == "input-external" { 43 } else { 42 });
    if mode == "input-no-dialer" {
        input.dialer = None;
    }
    OrganEnvelope {
        exchange: ExchangeId {
            opener: if mode == "input-opener" {
                Opener::Harness
            } else {
                Opener::Gate
            },
            ordinal,
        },
        position: if mode == "input-position" {
            Position::Continue
        } else {
            Position::Open
        },
        payload: Payload::Frame(input),
    }
}

#[test]
fn worker_input_gate_child() {
    let Ok(mode) = std::env::var("WEAVER_GATE_STARTUP_CHILD") else {
        return;
    };
    if !mode.starts_with("input-") {
        return;
    }
    // SAFETY: actual fork_organ exclusively mapped these fixture endpoints.
    let port = unsafe { OwnedFd::from_raw_fd(3) };
    let tools = unsafe { OwnedFd::from_raw_fd(4) };
    assert_eq!(
        nix::sys::statfs::fstatfs(&tools).unwrap().filesystem_type(),
        nix::sys::statfs::CGROUP2_SUPER_MAGIC
    );
    let root = std::path::PathBuf::from(std::env::var("WEAVER_WORKER_INPUT_ROOT").unwrap());
    let mut bytes = vec![0; weaver_types::MAX_ENVELOPE_BYTES];
    let n = recv(port.as_raw_fd(), &mut bytes, MsgFlags::empty()).unwrap();
    let raise: OrganEnvelope = serde_json::from_slice(&bytes[..n]).unwrap();
    let Payload::Directive(LifecycleDirective::Raise { socket, .. }) = &raise.payload else {
        panic!("Raise first")
    };
    let _listener = std::os::unix::net::UnixListener::bind(socket).unwrap();
    packet(
        3,
        &OrganEnvelope {
            exchange: raise.exchange,
            position: Position::Close,
            payload: Payload::Answer(LifecycleAnswer::GateReady),
        },
    );
    if mode == "input-fault" {
        packet(
            3,
            &OrganEnvelope {
                exchange: ExchangeId {
                    opener: Opener::Gate,
                    ordinal: 90,
                },
                position: Position::Close,
                payload: Payload::Fault(weaver_types::FaultReport {
                    case: weaver_types::FaultCase::ClientConnectionFailedMidTurn,
                    account: serde_json::value::RawValue::from_string("{\"fixture\": 7}".into())
                        .unwrap(),
                }),
            },
        );
    }
    if mode.contains("-session-") {
        return super::generation::session_tests::gate(&mode, &root, 3);
    }
    if mode == "input-report-queued" {
        let watch = Instant::now() + Duration::from_secs(10);
        while !root.join("report-queued").exists() {
            assert!(Instant::now() < watch);
            std::thread::sleep(Duration::from_millis(1));
        }
        for ordinal in [201, 202] {
            packet(
                3,
                &OrganEnvelope {
                    exchange: ExchangeId {
                        opener: Opener::Gate,
                        ordinal,
                    },
                    position: Position::Close,
                    payload: Payload::Fault(weaver_types::FaultReport {
                        case: weaver_types::FaultCase::ClientConnectionFailedMidTurn,
                        account: serde_json::value::to_raw_value(
                            &serde_json::json!({"report": ordinal}),
                        )
                        .unwrap(),
                    }),
                },
            );
        }
        while !root.join("reports-recorded").exists() {
            assert!(Instant::now() < watch);
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    packet(3, &frame(&mode, 101));
    if mode == "input-report-held" {
        for (ordinal, stage) in [(201, "accepted"), (202, "turn")] {
            let watch = Instant::now() + Duration::from_secs(10);
            while !root.join(format!("report-{stage}")).exists() {
                assert!(Instant::now() < watch, "worker must reach {stage}");
                std::thread::sleep(Duration::from_millis(1));
            }
            packet(
                3,
                &OrganEnvelope {
                    exchange: ExchangeId {
                        opener: Opener::Gate,
                        ordinal,
                    },
                    position: Position::Close,
                    payload: Payload::Fault(weaver_types::FaultReport {
                        case: weaver_types::FaultCase::ClientConnectionFailedMidTurn,
                        account: serde_json::value::to_raw_value(
                            &serde_json::json!({"held_report": stage}),
                        )
                        .unwrap(),
                    }),
                },
            );
        }
    }
    if mode.contains("-external-") {
        return super::generation::emission::execution::external_tests::gate(&mode, &root, 3);
    }
    if mode == "input-queued" {
        packet(3, &frame(&mode, 102));
    }
    if mode.contains("election-wait-held") || mode.contains("election-wait-gate-failure") {
        let end = Instant::now() + Duration::from_secs(5);
        while !root.join("election-wake").exists() && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(root.join("election-wake").exists());
        if mode.contains("gate-failure") {
            return;
        }
        let raw = serde_json::to_vec(&frame(&mode, 102)).unwrap();
        std::fs::write(root.join("election-queued.json"), &raw).unwrap();
        assert_eq!(send(3, &raw, MsgFlags::MSG_NOSIGNAL), Ok(raw.len()));
    }
    if mode == "input-pressure"
        || mode == "input-stop-gate-pressure"
        || mode.contains("final-manual-gate-pressure")
        || mode.contains("final-manual-stop-after")
        || mode.contains("final-manual-deadline")
        || mode.contains("final-manual-send-clock")
        || mode.contains("refusal-boundary-gate-")
        || mode.contains("task-admission-operation-refusal-gate-")
    {
        let end = Instant::now() + Duration::from_secs(5);
        while !root.join("drain").exists() && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(root.join("drain").exists());
    }
    let mut next_after_refusal = false;
    loop {
        let n = recv(port.as_raw_fd(), &mut bytes, MsgFlags::empty()).unwrap_or(0);
        if n == 0 {
            break;
        }
        if bytes[0] == b'x' {
            continue;
        }
        let reply: OrganEnvelope = serde_json::from_slice(&bytes[..n]).unwrap();
        assert_eq!(reply.exchange, frame(&mode, 101).exchange);
        assert_eq!(reply.position, Position::Close);
        if mode.contains("generation") {
            let Payload::Frame(frame) = &reply.payload else {
                panic!("actual operation close")
            };
            let value: serde_json::Value =
                serde_json::from_slice(&frame.octets().unwrap()).unwrap();
            assert_eq!(
                value["kind"],
                if mode.contains("-final-")
                    && !mode.contains("stop-before")
                    && !mode.contains("operation-refusal")
                {
                    "answered"
                } else {
                    "stopped"
                }
            );
            if mode.contains("no-task") {
                assert!(value.get("turn").is_none());
                assert!(value.get("run").is_none());
            } else {
                assert_eq!(value["turn"], "generation-turn");
            }
        } else if mode.contains("stop-") {
            let Payload::Frame(frame) = &reply.payload else {
                panic!("actual stopped frame")
            };
            let value: serde_json::Value =
                serde_json::from_slice(&frame.octets().unwrap()).unwrap();
            assert_eq!(value["kind"], "stopped");
            assert_eq!(value["reason"], "the operator stopped the turn");
            assert_eq!(value["turn"], "stopped-turn");
            assert!(value["run"].as_str().is_some_and(|run| !run.is_empty()));
        } else {
            assert_eq!(
                reply.payload,
                Payload::Frame(TurnFrame::carry(
                    br#"{"kind":"refused","reason":"Restate the input in STE100."}"#
                ))
            );
        }
        std::fs::write(root.join("closed.json"), &bytes[..n]).unwrap();
        if mode.contains("operation-refusal") && !next_after_refusal {
            next_after_refusal = true;
            packet(3, &frame(&mode, 102));
        }
        if mode == "input-duplicate" {
            packet(3, &frame(&mode, 101));
        }
        if mode == "input-two" || mode == "input-stop-next" {
            packet(3, &frame(&mode, 102));
        }
    }
}

enum Held<'a> {
    Input(Box<FoundationInput<'a>>),
    Turn(Box<FoundationTurn<'a>>),
    Serving(FoundationServing<'a>),
}
struct Guard<'a>(Option<Held<'a>>);
impl Drop for Guard<'_> {
    fn drop(&mut self) {
        let serving = match self.0.as_mut() {
            Some(Held::Input(input)) => &mut input.serving,
            Some(Held::Turn(turn)) => &mut turn.serving,
            Some(Held::Serving(serving)) => serving,
            None => return,
        };
        reap(&mut serving.owner.gate_child);
    }
}
impl<'a> Guard<'a> {
    fn input(&mut self) -> &mut FoundationInput<'a> {
        let Some(Held::Input(input)) = self.0.as_mut() else {
            panic!("input owner")
        };
        input
    }
}
fn trace(reader: &mut UnixStream) -> Vec<serde_json::Value> {
    reader.set_nonblocking(true).unwrap();
    let end = Instant::now() + Duration::from_millis(30);
    let mut bytes = vec![];
    while Instant::now() < end {
        let _ = reader.read_to_end(&mut bytes);
        std::thread::sleep(Duration::from_millis(1));
    }
    bytes
        .split(|b| *b == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice(line).unwrap())
        .collect()
}
fn rows<'a>(trace: &'a [serde_json::Value], kind: &str) -> Vec<&'a serde_json::Value> {
    trace.iter().filter(|r| r["kind"] == kind).collect()
}
fn seq(row: &serde_json::Value) -> u64 {
    row["sequence"].as_str().unwrap().parse().unwrap()
}

fn held_report_at_rest(
    owner: &mut FoundationInput<'_>,
    clock: &mut FakePumpIo,
    root: &Path,
    name: &str,
) {
    use crate::flow::context::HeldInput;
    use crate::flow::context::held_input_tests::hold;
    let stage = owner.stage;
    let expected = owner.progress();
    let before = owner
        .serving
        .owner
        .context
        .barrier
        .recorder()
        .structure()
        .by_kind(serving::Kind::Fault)
        .count();
    let held = hold(owner.serving.owner.context, HeldInput::Caller);
    std::fs::write(root.join(format!("report-{name}")), b"ready").unwrap();
    let watch = Instant::now() + Duration::from_secs(5);
    while owner.report.is_none() {
        owner.advance_with_io(&mut CircuitIo(clock));
        assert!(owner.failure.is_none(), "{:?}", owner.detail());
        assert!(
            Instant::now() < watch,
            "gate must deliver its actual report"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    for _ in 0..3 {
        assert_eq!(
            owner.advance_with_io(&mut CircuitIo(clock)),
            FoundationInputProgress::Pending
        );
        assert!(owner.stage == stage);
        assert!(owner.report.is_some());
        held.assert_held(owner.serving.owner.context);
        assert_eq!(
            owner
                .serving
                .owner
                .context
                .barrier
                .recorder()
                .structure()
                .by_kind(serving::Kind::Fault)
                .count(),
            before
        );
    }
    held.take(owner.serving.owner.context);
    assert_eq!(owner.advance_with_io(&mut CircuitIo(clock)), expected);
    assert!(owner.stage == stage);
    assert!(owner.report.is_none());
    assert_eq!(
        owner
            .serving
            .owner
            .context
            .barrier
            .recorder()
            .structure()
            .by_kind(serving::Kind::Fault)
            .count(),
        before + 1
    );
}

pub(in super::super) fn exercise(
    serving: FoundationServing<'_>,
    member: &mut Member,
    reader: &mut UnixStream,
    clock: &mut FakePumpIo,
    management: &Path,
    root: &Path,
    mode: &str,
) {
    let ordinal = if mode == "input-next" { 102 } else { 101 };
    let input_id = if mode == "input-next" { 2 } else { 1 };
    let mut guard = Guard(Some(Held::Input(serving.input())));
    assert!(guard.input().received.is_none());
    assert!(
        guard
            .input()
            .start_turn(TurnKey("too-early".into()))
            .is_err()
    );
    let held = guard.0.take().unwrap();
    let Held::Input(input) = held else {
        unreachable!()
    };
    guard.0 = Some(Held::Input(
        input.finish().err().expect("no fabricated completion"),
    ));
    let mut prefix_records = vec![];
    if mode == "input-report-queued" {
        let owner = guard.input();
        let held = crate::flow::context::held_input_tests::hold(
            owner.serving.owner.context,
            crate::flow::context::HeldInput::Caller,
        );
        std::fs::write(root.join("report-queued"), b"ready").unwrap();
        let watch = Instant::now() + Duration::from_secs(5);
        while owner.report.is_none() || owner.serving.owner.gate.held().is_none() {
            if owner.report.is_some() && owner.serving.owner.gate.held().is_none() {
                owner.serving.owner.gate.try_receive().unwrap();
            }
            if owner.report.is_some() && owner.serving.owner.gate.held().is_some() {
                break;
            }
            owner.advance_with_io(&mut CircuitIo(clock));
            assert!(owner.failure.is_none(), "{:?}", owner.detail());
            assert!(
                Instant::now() < watch,
                "both actual reports must be retained"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        for _ in 0..3 {
            assert_eq!(
                owner.advance_with_io(&mut CircuitIo(clock)),
                FoundationInputProgress::Pending
            );
            assert!(owner.stage == Stage::Waiting);
            assert!(owner.report.is_some());
            assert_eq!(
                owner.serving.owner.gate.held().unwrap().exchange.ordinal,
                202
            );
            held.assert_held(owner.serving.owner.context);
        }
        held.take(owner.serving.owner.context);
        for _ in 0..4 {
            owner.advance_with_io(&mut CircuitIo(clock));
        }
        assert!(owner.failure.is_none(), "{:?}", owner.detail());
        let records = trace(reader);
        let faults = rows(&records, "fault");
        assert_eq!(faults.len(), 2);
        assert_eq!(faults[0]["payload"]["account"]["report"], 201);
        assert_eq!(faults[1]["payload"]["account"]["report"], 202);
        prefix_records = records;
        std::fs::write(root.join("reports-recorded"), b"continue").unwrap();
    }
    if mode == "input-worker" || mode == "input-worker-accepted" {
        let Some(Held::Input(input)) = guard.0.take() else {
            unreachable!()
        };
        let result = std::thread::scope(|scope| {
            let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let flag = done.clone();
            let responder = scope.spawn(move || {
                let end = Instant::now() + Duration::from_secs(8);
                while !flag.load(std::sync::atomic::Ordering::SeqCst) && Instant::now() < end {
                    member.step_live("ready");
                    std::thread::sleep(Duration::from_millis(1));
                }
            });
            let result = worker::input(input.serving, TurnKey("t-1".into()));
            done.store(true, std::sync::atomic::Ordering::SeqCst);
            responder.join().unwrap();
            result
        });
        if let Err(owner) = &result {
            eprintln!("worker input stopped: {:?}", owner.detail());
        }
        guard.0 = Some(match result {
            Ok(FoundationInputResult::Refused(serving)) => Held::Serving(serving),
            Ok(FoundationInputResult::Aborted(serving)) => Held::Serving(serving),
            Ok(FoundationInputResult::Completed(serving)) => Held::Serving(serving),
            Ok(FoundationInputResult::Turn(turn)) => Held::Turn(turn),
            Ok(FoundationInputResult::Generated(output)) => Held::Input(output.into_service()),
            Ok(FoundationInputResult::Sorted(output)) => Held::Input(output.into_service()),
            Err(input) => Held::Input(input),
        });
        assert!(matches!(
            (&guard.0, mode),
            (Some(Held::Serving(_)), "input-worker")
                | (Some(Held::Turn(_)), "input-worker-accepted")
        ));
        let records = trace(reader);
        assert_eq!(rows(&records, "input.offered").len(), 1);
        assert_eq!(
            rows(&records, "message.user").len(),
            usize::from(mode == "input-worker-accepted")
        );
        return;
    }
    let mut pressure = false;
    if mode == "input-pressure" {
        let fd = guard.input().serving.owner.gate.fd().as_raw_fd();
        let mut count = 0;
        loop {
            match send(
                fd,
                &[b'x'; 4096],
                MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_NOSIGNAL,
            ) {
                Ok(4096) => count += 1,
                Err(nix::errno::Errno::EAGAIN) => break,
                other => panic!("fill sender: {other:?}"),
            }
        }
        assert!(count > 0);
    }
    let mut management_peer = None;
    let mut expired = false;
    for _ in 0..5000 {
        let owner = guard.input();
        let progress = owner.advance_with_io(&mut CircuitIo(clock));
        if mode == "input-pressure" && owner.send.as_ref().is_some_and(|s| !s.sent()) {
            assert_eq!(progress, FoundationInputProgress::Pending);
            assert!(
                !owner
                    .serving
                    .owner
                    .context
                    .input
                    .snapshot()
                    .unwrap()
                    .response_sent
            );
            assert!(
                owner
                    .serving
                    .owner
                    .context
                    .deadlines
                    .get(super::super::super::PumpDeadline::Gate)
                    .is_none()
            );
            pressure = true;
            std::fs::write(root.join("drain"), b"yes").unwrap();
        }
        if mode == "input-silent"
            && !expired
            && owner
                .serving
                .owner
                .context
                .input
                .snapshot()
                .is_some_and(|s| s.request_id.is_some())
        {
            let bound = owner
                .serving
                .owner
                .context
                .deadlines
                .get(super::super::super::PumpDeadline::Gate)
                .unwrap();
            clock.advance_to(bound.deadline_ns()).unwrap();
            expired = true;
        }
        if mode == "input-management" && management_peer.is_none() && owner.stage == Stage::Checking
        {
            use nix::sys::socket::{AddressFamily, SockFlag, SockType, UnixAddr, connect, socket};
            let fd = socket(
                AddressFamily::Unix,
                SockType::SeqPacket,
                SockFlag::SOCK_CLOEXEC,
                None,
            )
            .unwrap();
            connect(fd.as_raw_fd(), &UnixAddr::new(management).unwrap()).unwrap();
            management_peer = Some(fd);
        }
        if progress != FoundationInputProgress::Pending {
            break;
        }
        member.step(clock.now().unwrap(), "ready");
        std::thread::sleep(Duration::from_millis(1));
    }
    let stopped = matches!(
        mode,
        "input-position" | "input-opener" | "input-silent" | "input-check-eof" | "input-no-dialer"
    );
    let refused = matches!(
        mode,
        "input-malformed"
            | "input-role"
            | "input-language"
            | "input-pressure"
            | "input-duplicate"
            | "input-two"
    );
    let owner = guard.input();
    let expected = if stopped {
        FoundationInputProgress::Stopped
    } else if refused {
        FoundationInputProgress::Refused
    } else if mode == "input-management" {
        FoundationInputProgress::ManagementPending
    } else {
        FoundationInputProgress::Accepted
    };
    assert_eq!(owner.progress(), expected, "{:?}", owner.detail());
    if mode == "input-report-held" {
        held_report_at_rest(owner, clock, root, "accepted");
    }
    prefix_records.extend(trace(reader));
    let mut records = prefix_records;
    assert!(rows(&records, "turn.started").is_empty());
    assert!(rows(&records, "message.user").is_empty());
    if stopped || mode == "input-management" {
        let detail = owner.detail();
        assert_eq!(owner.advance_with_io(clock), expected);
        assert_eq!(owner.detail(), detail);
        assert!(owner.received.is_some());
        if mode == "input-management" {
            assert!(owner.serving.owner.listener.held().is_some());
            assert!(owner.serving.owner.context.failure().is_none());
            assert!(
                !owner
                    .serving
                    .owner
                    .context
                    .input
                    .snapshot()
                    .unwrap()
                    .cancelled
            );
        }
        let Some(Held::Input(input)) = guard.0.take() else {
            unreachable!()
        };
        guard.0 = Some(Held::Input(
            input
                .finish()
                .err()
                .expect("retained stopped/pending custody"),
        ));
        return;
    }
    assert_eq!(rows(&records, "input.offered").len(), 1);
    let offered = rows(&records, "input.offered")[0].clone();
    let Payload::Frame(original) = frame(mode, ordinal).payload else {
        unreachable!()
    };
    assert_eq!(offered["payload"]["octets"], original.octets);
    assert_eq!(offered["payload"]["input_id"], input_id.to_string());
    if refused {
        if mode == "input-pressure" {
            assert!(pressure);
        }
        assert_eq!(rows(&records, "refusal").len(), 1);
        assert!(seq(&offered) < seq(rows(&records, "refusal")[0]));
        let end = Instant::now() + Duration::from_secs(2);
        while !root.join("closed.json").exists() && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(
            root.join("closed.json").exists(),
            "actual gate must receive whole response"
        );
        assert!(owner.send.as_ref().unwrap().sent());
        let Some(Held::Input(input)) = guard.0.take() else {
            unreachable!()
        };
        let FoundationInputResult::Refused(serving) = input.finish().ok().expect("sent refusal")
        else {
            panic!()
        };
        guard.0 = Some(Held::Serving(serving));
        if mode == "input-two" {
            let Some(Held::Serving(serving)) = guard.0.take() else {
                unreachable!()
            };
            exercise(
                serving,
                member,
                reader,
                clock,
                management,
                root,
                "input-next",
            );
            return;
        }
        if mode == "input-duplicate" {
            let Some(Held::Serving(serving)) = guard.0.take() else {
                unreachable!()
            };
            guard.0 = Some(Held::Input(serving.input()));
            for _ in 0..1000 {
                if guard.input().advance_with_io(&mut CircuitIo(clock))
                    != FoundationInputProgress::Pending
                {
                    break;
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            assert_eq!(guard.input().progress(), FoundationInputProgress::Stopped);
            assert!(rows(&trace(reader), "input.offered").is_empty());
        }
        return;
    }
    if mode == "input-fault" {
        assert_eq!(rows(&records, "fault").len(), 1);
        assert!(seq(rows(&records, "fault")[0]) < seq(&offered));
    }
    assert!(owner.token.is_some());
    assert_eq!(
        rows(&records, "ste100.result").len(),
        if mode == "input-next" { 1 } else { 4 }
    ); // Three templates plus input.
    if mode == "input-source-eof" {
        reap(&mut owner.serving.owner.gate_child);
    }
    owner.start_turn(TurnKey("t-1".into())).unwrap();
    for _ in 0..3000 {
        if owner.advance_with_io(&mut CircuitIo(clock)) != FoundationInputProgress::Pending {
            break;
        }
        member.step(clock.now().unwrap(), "ready");
    }
    assert_eq!(
        owner.progress(),
        FoundationInputProgress::Turn,
        "{:?}",
        owner.detail()
    );
    if mode == "input-report-held" {
        held_report_at_rest(owner, clock, root, "turn");
    }
    records.extend(trace(reader));
    if mode == "input-source-eof" {
        assert!(owner.serving.owner.context.failure().is_none());
        assert!(owner.failure.is_none());
        assert!(
            owner
                .serving
                .owner
                .context
                .serving_organs
                .gate_delivery_lost(&owner.serving.owner.gate)
        );
        assert_eq!(owner.serving.owner.tools.populated(), Some(false));
        let faults = rows(&records, "fault");
        assert_eq!(
            faults.len(),
            1,
            "one fault, accepted input still has a turn"
        );
        assert_eq!(faults[0]["payload"]["case"], "organ_death_observed");
        assert!(owner.serving.owner.context.input.snapshot().is_none());
        assert!(
            owner.seated.is_some(),
            "accepted token became the original source"
        );
        assert!(!root.join("closed.json").exists());
    }
    let Some(Held::Input(input)) = guard.0.take() else {
        unreachable!()
    };
    let FoundationInputResult::Turn(turn) = input.finish().ok().expect("recorded input") else {
        panic!()
    };
    guard.0 = Some(Held::Turn(turn));
    let Some(Held::Turn(turn)) = guard.0.as_mut() else {
        unreachable!()
    };
    assert_eq!(turn.message().role, weaver_traits::Role::User);
    assert_eq!(
        turn.message().content,
        vec![weaver_traits::ContentBlock::Text {
            text: "Read caf\u{e9}.".into()
        }]
    );
    assert_eq!(turn.turn(), TurnKey("t-1".into()));
    assert!(turn.received.belongs_to(&turn.serving.owner.gate));
    let users = rows(&records, "message.user");
    assert_eq!(users.len(), 1);
    assert_eq!(rows(&records, "turn.started").len(), 1);
    assert_eq!(users[0]["causal_parent"], seq(&offered).to_string());
    assert_eq!(
        users[0]["origin"],
        if mode == "input-external" {
            "external"
        } else {
            "operator"
        }
    );
    assert_eq!(turn.source().sequence.0, seq(users[0]));
    assert_eq!(turn.input.input_id.0, input_id);
    assert_eq!(turn.input.offered_sequence.0, seq(&offered));
    let context = &mut turn.serving.owner.context;
    for _ in 0..100 {
        if context.flow.ledger().next_file().is_some() {
            break;
        }
        context.reduce_flow(clock).unwrap();
    }
    let crate::flow::ledger::LedgerFile::Statement(file) =
        context.flow.ledger().next_file().unwrap()
    else {
        panic!()
    };
    assert_eq!(&file.block_id, turn.source());
    assert_eq!(file.bindings[0].nouns[0].value, "caf\u{e9}");
    if mode == "input-queued" {
        assert_eq!(
            turn.serving.owner.gate.held().unwrap().exchange.ordinal,
            102
        );
        assert!(
            turn.serving
                .owner
                .context
                .deadlines
                .get(super::super::super::PumpDeadline::Gate)
                .is_none()
        );
        assert_eq!(rows(&records, "input.offered").len(), 1);
    }
}
