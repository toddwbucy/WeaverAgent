//! Actual worker startup, gate input, classifier and decoder processes with a
//! scripted state member/model. This proves joins, not artifact qualification.
use super::super::management::{
    FoundationManagementProgress,
    tests::{Guard, dial, read_packet, trace},
};
use super::*;
use crate::flow::clock::testing::FakePumpIo;
use crate::flow::context::startup_composition::tests::CircuitIo;
use crate::flow::context::worker_startup::tests::{Member, worker};
use nix::sys::socket::{MsgFlags, recv, send};
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};
use weaver_types::state_flow as s;
use weaver_types::{Finish, Generation, LifecycleAnswer, TokenAnswer, TokenDirective};

pub(in crate::flow::context) fn templates() -> &'static [s::TemplateSpec] {
    static ALL: std::sync::OnceLock<Vec<s::TemplateSpec>> = std::sync::OnceLock::new();
    ALL.get_or_init(|| {
        let mut all = crate::flow::context::template_tests::templates().to_vec();
        all.push(s::TemplateSpec {
            template_id: "mark.begin_task".into(),
            kind: "state.mark".into(),
            parts: vec![s::TemplatePart::Literal("Read task.".into())],
        });
        all
    })
}
fn packet(fd: i32, value: &TokenAnswer) {
    let bytes = serde_json::to_vec(value).unwrap();
    assert_eq!(send(fd, &bytes, MsgFlags::MSG_NOSIGNAL), Ok(bytes.len()));
}
fn generated(finish: Finish) -> TokenAnswer {
    TokenAnswer::Generated(Generation {
        emission: "Read result.".into(), finish,
        content: vec![weaver_traits::ContentBlock::Text { text: "Read result.".into() }],
        unrecovered: vec![],
        request: serde_json::value::RawValue::from_string("{ \"rendered\": \"fixture family render\", \"template\": \"fixture\" }".into()).unwrap(),
        measurement: serde_json::value::to_raw_value(&serde_json::json!({"model":"fixture","weights_hash":"fixture","input_tokens":[17,18,19],"output_tokens":[31]})).unwrap(),
        resident: 8, capacity: 16384,
    })
}
pub(in crate::flow::context) fn spu_after_open(mode: &str, fd: i32) {
    let root = std::path::PathBuf::from(std::env::var("WEAVER_WORKER_INPUT_ROOT").unwrap());
    let mut bytes = vec![0; weaver_types::MAX_ENVELOPE_BYTES];
    let end = Instant::now() + Duration::from_secs(10);
    let mut measured = None;
    let mut appended = false;
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
                        assert!(measured.is_none() && !appended);
                        assert_eq!(*keep, 3);
                        assert_eq!(
                            delta,
                            &[weaver_traits::Message {
                                role: weaver_traits::Role::User,
                                content: vec![weaver_traits::ContentBlock::Text {
                                    text: "Read café.".into()
                                }]
                            }]
                        );
                        assert!(tool_schemas.is_empty());
                        std::fs::write(root.join("measured.json"), &bytes[..n]).unwrap();
                        if mode.contains("stop-measure") || mode.ends_with("observe-measure") {
                            while !root.join("release-measure").exists() {
                                assert!(Instant::now() < end);
                                std::thread::sleep(Duration::from_millis(1));
                            }
                        }
                        if mode.ends_with("decoder-eof") {
                            return;
                        }
                        measured = Some((delta.clone(), tool_schemas.clone()));
                        packet(
                            fd,
                            &TokenAnswer::DeltaMeasured {
                                resident: if mode.ends_with("wrong-counts") { 4 } else { 3 },
                                prefix_tokens: 3,
                                keep: *keep,
                                input_tokens: if mode.ends_with("fit") { 20000 } else { 3 },
                                capacity: 16384,
                            },
                        );
                        if mode.ends_with("stop-unsent") {
                            while !root.join("release-unsent").exists() {
                                assert!(Instant::now() < end);
                                std::thread::sleep(Duration::from_millis(1));
                            }
                        }
                    }
                    TokenDirective::AppendAndGenerate {
                        turn,
                        delta,
                        tool_schemas,
                    } => {
                        assert!(!appended);
                        assert_eq!(turn.0, "generation-turn");
                        let (original, schemas) = measured.as_ref().expect("measure before append");
                        assert_eq!(delta, original);
                        assert_eq!(tool_schemas, schemas);
                        std::fs::write(root.join("appended.json"), &bytes[..n]).unwrap();
                        appended = true;
                        if !mode.ends_with("stop-generation") {
                            packet(fd, &generated(Finish::Completed));
                        }
                    }
                    TokenDirective::Cancel { turn } => {
                        assert!(appended && mode.ends_with("stop-generation"));
                        assert_eq!(turn.0, "generation-turn");
                        std::fs::write(root.join("cancelled.json"), &bytes[..n]).unwrap();
                        packet(fd, &generated(Finish::Stopped));
                        packet(fd, &TokenAnswer::AtRest);
                    }
                    _ => panic!("unexpected serving decoder request {request:?}"),
                }
            }
            Err(nix::errno::Errno::EAGAIN) => {}
            other => panic!("decode fixture: {other:?}"),
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("generation child watchdog");
}
fn step(
    input: &mut FoundationInput<'_>,
    member: &mut Member,
    clock: &mut FakePumpIo,
    mode: &str,
) -> FoundationInputProgress {
    let p = if mode == "input-worker-generation" {
        input.advance()
    } else {
        input.advance_with_io(&mut CircuitIo(clock))
    };
    member.step(
        if mode == "input-worker-generation" {
            MonotonicIo.now().unwrap()
        } else {
            clock.now().unwrap()
        },
        mode,
    );
    std::thread::sleep(Duration::from_millis(1));
    p
}
fn until(
    input: &mut FoundationInput<'_>,
    member: &mut Member,
    clock: &mut FakePumpIo,
    mode: &str,
    target: FoundationInputProgress,
) {
    for _ in 0..2000 {
        let p = step(input, member, clock, mode);
        if p == target {
            return;
        }
        assert_eq!(
            p,
            FoundationInputProgress::Pending,
            "{:?} / {:?}",
            input.detail(),
            input.operation_detail()
        );
    }
    panic!(
        "generation join did not reach {target:?}: {:?}",
        input.operation_detail()
    );
}
fn stop(
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
            ordinal: 81,
        },
        position: Position::Open,
        payload: Payload::Directive(weaver_types::LifecycleDirective::Stop {
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
        // A ready operation is still obliged to observe the actual root socket.
        input.observe(&mut CircuitIo(clock)).unwrap();
        if input.progress() == FoundationInputProgress::ManagementPending {
            break;
        }
    }
    guard.management = Some(guard.input.take().unwrap().management().ok().unwrap());
    if mode.ends_with("stop-mark") {
        for _ in 0..8 {
            guard
                .management
                .as_mut()
                .unwrap()
                .advance_with_io(&mut CircuitIo(clock));
        }
        assert!(
            read_packet(&peer).is_none(),
            "Stop still owes the admitted mark filing"
        );
        assert!(member.held_file.is_some());
        member.hold_mark = false;
    }
    if mode.contains("stop-measure") {
        for _ in 0..8 {
            guard
                .management
                .as_mut()
                .unwrap()
                .advance_with_io(&mut CircuitIo(clock));
        }
        assert!(
            read_packet(&peer).is_none(),
            "measurement still owes its exact exchange"
        );
        assert!(!root.join("closed.json").exists());
        if mode.ends_with("eof") {
            nix::sys::socket::shutdown(peer.as_raw_fd(), nix::sys::socket::Shutdown::Both).unwrap();
        }
        std::fs::write(root.join("release-measure"), b"release").unwrap();
    }
    for _ in 0..2000 {
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
    guard.input = Some(
        guard
            .management
            .take()
            .unwrap()
            .finish()
            .ok()
            .expect("whole Stop"),
    );
    if mode.ends_with("stop-fit") {
        until(
            guard.input.as_mut().unwrap(),
            member,
            clock,
            mode,
            FoundationInputProgress::Refused,
        );
    } else {
        assert_eq!(
            guard.input.as_ref().unwrap().progress(),
            FoundationInputProgress::Aborted
        );
    }
    if mode.ends_with("eof") {
        return;
    }
    let answer: OrganEnvelope =
        serde_json::from_slice(&read_packet(&peer).expect("actual root answer")).unwrap();
    assert_eq!(answer.exchange, request.exchange);
    assert_eq!(
        answer.payload,
        Payload::Answer(if mode.ends_with("stop-fit") {
            LifecycleAnswer::AtRest
        } else {
            LifecycleAnswer::TurnAborted {
                turn: TurnKey("generation-turn".into()),
            }
        })
    );
    assert!(read_packet(&peer).is_none());
    if mode.ends_with("stop-generation") {
        assert!(root.join("cancelled.json").exists());
    }
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
    if mode == "input-worker-generation" {
        return exercise_worker(serving, member, reader, clock, mode);
    }
    let mut guard = Guard {
        input: Some(serving.input()),
        management: None,
    };
    let input = guard.input.as_mut().unwrap();
    assert!(input.start_task(TurnKey("early".into())).is_err());
    assert!(input.generate().is_err());
    until(
        input,
        member,
        clock,
        mode,
        FoundationInputProgress::Accepted,
    );
    let original = input.received.as_ref().unwrap().raw_bytes().to_vec();
    input.start_task(TurnKey("generation-turn".into())).unwrap();
    assert!(input.start_turn(TurnKey("replacement".into())).is_err());
    assert!(input.start_task(TurnKey("replacement".into())).is_err());
    if mode.ends_with("held-input") || mode.ends_with("stop-task") {
        for _ in 0..50 {
            input.advance_with_io(&mut CircuitIo(clock));
            if input.serving.owner.context.open_turn.is_some() {
                break;
            }
        }
        let bound = input
            .serving
            .owner
            .context
            .deadlines
            .get(PumpDeadline::Decision);
        assert!(bound.is_some());
        for _ in 0..8 {
            input.advance_with_io(&mut CircuitIo(clock));
        }
        assert_eq!(
            input
                .serving
                .owner
                .context
                .deadlines
                .get(PumpDeadline::Decision),
            bound
        );
        assert_eq!(input.progress(), FoundationInputProgress::Pending);
        assert!(input.generate().is_err());
        assert!(!root.join("measured.json").exists());
        if mode.ends_with("stop-task") {
            stop(&mut guard, member, clock, mode, socket, root);
            assert!(!root.join("measured.json").exists());
            let rows = trace(reader);
            assert!(!rows.iter().any(|r| r["kind"] == "state.mark"));
            assert!(!member.requests.iter().any(|r| r == "marks"));
            return;
        }
    }
    if mode.ends_with("held-mark") || mode.ends_with("stop-mark") {
        member.hold_mark = true;
        for _ in 0..2000 {
            step(input, member, clock, mode);
            if member.held_file.is_some() {
                break;
            }
        }
        assert!(member.held_file.is_some());
        assert_eq!(input.progress(), FoundationInputProgress::Pending);
        assert!(!root.join("measured.json").exists());
        if mode.ends_with("stop-mark") {
            stop(&mut guard, member, clock, mode, socket, root);
            assert!(member.held_file.is_none());
            assert!(!root.join("measured.json").exists());
            return;
        }
        member.hold_mark = false;
    }
    let task_target = if mode.ends_with("read-refusal") {
        FoundationInputProgress::OperationRefused
    } else {
        FoundationInputProgress::Task
    };
    until(input, member, clock, mode, task_target);
    if mode.ends_with("read-refusal") {
        assert_eq!(
            input.task_read_refusal(),
            Some((s::ReadRefusalCode::InvalidSelector, "worker prompt refusal"))
        );
        assert!(input.generate().is_err());
        assert!(!root.join("measured.json").exists());
        assert!(input.serving.owner.context.failure().is_none());
        return;
    }
    assert!(
        input
            .serving
            .owner
            .context
            .deadlines
            .get(PumpDeadline::Decision)
            .is_none()
    );
    assert!(
        member
            .requests
            .iter()
            .filter(|r| r.as_str() == "file")
            .count()
            >= 2
    );
    assert_eq!(input.received.as_ref().unwrap().raw_bytes(), original);
    assert!(input.place_task(String::new(), CutMethod::Flush).is_err());
    input
        .place_task(
            "worker-task".into(),
            if mode.ends_with("elide") {
                CutMethod::Elide
            } else {
                CutMethod::Flush
            },
        )
        .unwrap();
    if mode.ends_with("observe-measure") {
        for _ in 0..2000 {
            step(input, member, clock, mode);
            if root.join("measured.json").exists() {
                break;
            }
        }
        assert!(root.join("measured.json").exists());
        observe(&mut guard, clock, socket);
        std::fs::write(root.join("release-measure"), b"release").unwrap();
    }
    let input = guard.input.as_mut().unwrap();
    if mode.contains("stop-measure") {
        for _ in 0..2000 {
            step(input, member, clock, mode);
            if root.join("measured.json").exists() {
                break;
            }
        }
        assert!(root.join("measured.json").exists());
        stop(&mut guard, member, clock, mode, socket, root);
        assert!(!root.join("appended.json").exists());
        return;
    }
    if mode.ends_with("stop-fit") {
        for _ in 0..2000 {
            step(input, member, clock, mode);
            if input.serving.owner.context.placement_refusing() {
                break;
            }
        }
        assert!(input.serving.owner.context.placement_refusing());
        assert!(input.serving.owner.context.open_turn.is_some());
        stop(&mut guard, member, clock, mode, socket, root);
        let rows = trace(reader);
        assert_eq!(
            rows.iter().filter(|r| r["kind"] == "turn.closed").count(),
            1
        );
        assert!(!root.join("appended.json").exists());
        return;
    }
    let placed_target = if mode.ends_with("fit") {
        FoundationInputProgress::Refused
    } else if mode.ends_with("wrong-counts") || mode.ends_with("decoder-eof") {
        FoundationInputProgress::Stopped
    } else {
        FoundationInputProgress::Placed
    };
    until(input, member, clock, mode, placed_target);
    if placed_target != FoundationInputProgress::Placed {
        assert!(!root.join("appended.json").exists());
        if placed_target == FoundationInputProgress::Stopped {
            assert!(input.serving.owner.context.failure().is_some());
            assert!(input.engine.as_ref().unwrap().generation.is_none());
        } else {
            assert!(input.serving.owner.context.failure().is_none());
            assert!(input.serving.owner.context.open_turn.is_none());
            let end = Instant::now() + Duration::from_secs(1);
            while !root.join("closed.json").exists() && Instant::now() < end {
                std::thread::sleep(Duration::from_millis(1));
            }
            assert!(
                root.join("closed.json").exists(),
                "only complete delivery returns the serving seat"
            );
        }
        return;
    }
    assert!(
        !root.join("appended.json").exists(),
        "no append before worker's explicit decision"
    );
    let rows = trace(reader);
    let placement = rows
        .iter()
        .find(|r| r["kind"] == "state.placement" && r["payload"]["phase"] == "append")
        .expect("Recorded before grant");
    assert_eq!(placement["payload"]["context"]["label"], "worker-task");
    assert_eq!(placement["payload"]["input_tokens"], 3);
    assert!(placement["payload"]["cut"].is_null());
    let source = rows.iter().find(|r| r["kind"] == "message.user").unwrap();
    let map = &placement["payload"]["unit_map"][0]["source_map"][0];
    assert_eq!(
        map["material"]["text"]["source"]["event"]["block_id"]["sequence"],
        source["sequence"]
    );
    assert_eq!(map["end"], "11"); // Complete UTF-8 bytes of Read café.
    assert_eq!(map["message_id"], placement["payload"]["message_ids"][0]);
    assert_eq!(
        rows.iter().filter(|r| r["kind"] == "message.user").count(),
        1
    );
    assert_eq!(rows.iter().filter(|r| r["kind"] == "state.mark").count(), 1);
    if mode.ends_with("stop-grant") {
        stop(&mut guard, member, clock, mode, socket, root);
        assert!(!root.join("appended.json").exists());
        return;
    }
    if mode.ends_with("stop-unsent") {
        loop {
            match send(
                input.serving.owner.decode.fd().as_raw_fd(),
                &[b'x'; 4096],
                MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_NOSIGNAL,
            ) {
                Ok(4096) => {}
                Err(nix::errno::Errno::EAGAIN) => break,
                other => panic!("fill decode: {other:?}"),
            }
        }
    }
    input.generate().unwrap();
    if mode.ends_with("stop-unsent") {
        input.advance_engine(None, &mut CircuitIo(clock)).unwrap();
        assert!(input.serving.owner.context.generation.is_some());
        stop(&mut guard, member, clock, mode, socket, root);
        assert!(!root.join("appended.json").exists());
        return;
    }
    if mode.ends_with("stop-generation") {
        for _ in 0..2000 {
            step(input, member, clock, mode);
            if root.join("appended.json").exists() {
                break;
            }
        }
        assert!(root.join("appended.json").exists());
        stop(&mut guard, member, clock, mode, socket, root);
        return;
    }
    until(
        input,
        member,
        clock,
        mode,
        FoundationInputProgress::Generated,
    );
    assert!(
        input.generate().is_err(),
        "same grant cannot generate twice"
    );
    assert_eq!(input.received.as_ref().unwrap().raw_bytes(), original);
    assert!(
        !root.join("closed.json").exists(),
        "generation is no final-answer election"
    );
    let work = input.engine.as_mut().unwrap();
    let Some(GenerationCompletion::Generated(proof)) = &mut work.generation else {
        panic!("actual proof")
    };
    assert_eq!(proof.generation().resident, 8);
    let output = proof.take_output().expect("checked OUTPUT retained");
    assert_eq!(output.original().turn.0, "generation-turn");
    assert!(output.receipt_lease().is_some());
    assert_eq!(
        matches!(
            output.blocks()[0].reply().result,
            s::Ste100Result::NotSte100 { .. }
        ),
        mode.ends_with("output-refusal")
    );
    proof.output = Some(output);
    let rows = trace(reader);
    assert_eq!(
        rows.iter()
            .filter(|r| r["kind"] == "message.assistant")
            .count(),
        1
    );
    assert_eq!(
        rows.iter().filter(|r| r["kind"] == "turn.closed").count(),
        0
    );
    let result = guard.input.take().unwrap().finish().ok().unwrap();
    let FoundationInputResult::Generated(output) = result else {
        panic!("generated handoff")
    };
    assert_eq!(output.turn().0, "generation-turn");
    guard.input = Some(output.into_service());
    assert_eq!(
        guard
            .input
            .as_ref()
            .unwrap()
            .received
            .as_ref()
            .unwrap()
            .raw_bytes(),
        original
    );
    if mode.ends_with("stop-generated") {
        stop(&mut guard, member, clock, mode, socket, root);
    }
}

fn exercise_worker(
    serving: FoundationServing<'_>,
    member: &mut Member,
    reader: &mut UnixStream,
    clock: &mut FakePumpIo,
    mode: &str,
) {
    let mut guard = Guard {
        input: Some(serving.input()),
        management: None,
    };
    until(
        guard.input.as_mut().unwrap(),
        member,
        clock,
        mode,
        FoundationInputProgress::Accepted,
    );
    let done = std::sync::atomic::AtomicBool::new(false);
    let input = std::thread::scope(|scope| {
        let responder = scope.spawn(|| {
            let end = Instant::now() + Duration::from_secs(8);
            while !done.load(std::sync::atomic::Ordering::SeqCst) && Instant::now() < end {
                member.step(MonotonicIo.now().unwrap(), mode);
                std::thread::sleep(Duration::from_millis(1));
            }
        });
        let input = worker::task(
            guard.input.take().unwrap(),
            TurnKey("generation-turn".into()),
        );
        assert_eq!(
            input.progress(),
            FoundationInputProgress::Task,
            "{:?} / {:?}",
            input.detail(),
            input.operation_detail()
        );
        let input = worker::place_task(input, "worker-task".into(), CutMethod::Flush);
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
    });
    guard.input = Some(input);
    assert_eq!(
        guard.input.as_ref().unwrap().progress(),
        FoundationInputProgress::Generated,
        "{:?}",
        guard.input.as_ref().unwrap().detail()
    );
    let rows = trace(reader);
    for kind in [
        "turn.started",
        "message.user",
        "state.mark",
        "message.assistant",
    ] {
        assert_eq!(
            rows.iter().filter(|r| r["kind"] == kind).count(),
            1,
            "{kind}"
        );
    }
    assert!(!rows.iter().any(|r| r["kind"] == "turn.closed"));
}

fn observe(guard: &mut Guard<'_>, clock: &mut FakePumpIo, socket: &Path) {
    use weaver_types::state_flow::enter::{
        ObservationAnswer, ObservationContent, ObservationEnvelope,
    };
    let bound = guard
        .input
        .as_ref()
        .unwrap()
        .serving
        .owner
        .context
        .deadlines
        .get(PumpDeadline::Decision)
        .unwrap();
    let peer = dial(socket);
    let request = OrganEnvelope {
        exchange: weaver_types::ExchangeId {
            opener: Opener::Admin,
            ordinal: 82,
        },
        position: Position::Open,
        payload: Payload::Directive(weaver_types::LifecycleDirective::Observe),
    };
    let raw = serde_json::to_vec(&request).unwrap();
    assert_eq!(
        send(peer.as_raw_fd(), &raw, MsgFlags::MSG_NOSIGNAL),
        Ok(raw.len())
    );
    for _ in 0..500 {
        let input = guard.input.as_mut().unwrap();
        input.observe(&mut CircuitIo(clock)).unwrap();
        if input.progress() == FoundationInputProgress::ManagementPending {
            break;
        }
    }
    guard.management = Some(guard.input.take().unwrap().management().ok().unwrap());
    for _ in 0..500 {
        let management = guard.management.as_mut().unwrap();
        let p = management.advance_with_io(&mut CircuitIo(clock));
        if p != FoundationManagementProgress::Pending {
            assert_eq!(p, FoundationManagementProgress::Answered);
            break;
        }
    }
    guard.input = Some(guard.management.take().unwrap().finish().ok().unwrap());
    let input = guard.input.as_ref().unwrap();
    assert_eq!(
        input
            .serving
            .owner
            .context
            .deadlines
            .get(PumpDeadline::Decision),
        Some(bound)
    );
    assert_eq!(input.progress(), FoundationInputProgress::Pending);
    let answer = ObservationEnvelope::decode(&read_packet(&peer).unwrap()).unwrap();
    assert_eq!(answer.exchange, request.exchange);
    assert!(matches!(
        answer.payload,
        ObservationContent::Answer(ObservationAnswer::State {
            state: weaver_types::AgentState::Active,
            ..
        })
    ));
}
