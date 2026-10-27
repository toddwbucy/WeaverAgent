//! Actual reused or accepted serving input followed by root rollback.
use super::*;
use crate::flow::clock::AbsoluteBound;
use crate::flow::context::failed_enter_driver::ChildState;
use crate::flow::context::worker_startup::FoundationRun;
use crate::flow::pump::PumpDeadline;
use std::os::fd::OwnedFd;
use std::sync::Arc;
use weaver_harness::{FoundationEnteredRollbackHandoff, FoundationEnteredRollbackProgress};

pub(in crate::flow::context::worker_startup) fn is_mode(mode: &str) -> bool {
    mode.contains("-session-serving-rollback-")
}

pub(super) fn holds_generation(mode: &str) -> bool {
    mode.contains("-active-generation")
}

pub(super) fn is_correction(mode: &str) -> bool {
    is_mode(mode) && mode.contains("-correction-")
}

fn active_turn(mode: &str) -> bool {
    mode.ends_with("-source-file")
        || mode.ends_with("-source-pending")
        || mode.ends_with("-task-read")
        || mode.ends_with("-mark-file")
        || mode.contains("-placement")
        || mode.ends_with("-placed-clock-failure")
        || holds_generation(mode)
        || is_correction(mode)
        || mode.contains("-external-")
}

fn clean_turn(mode: &str) -> bool {
    mode.ends_with("-idle") || mode.ends_with("-final-close") || mode.ends_with("-world-blocked")
}

#[test]
fn classifier_child() {
    use crate::flow::context::edge_tests::classifier_fixture;
    use crate::flow::ste100_channel::{Channel, Receive, Tag};
    use sha2::{Digest as _, Sha256};
    use std::os::fd::FromRawFd;
    let Ok(mode) = std::env::var("WEAVER_STARTUP_COMPOSITION_CHILD") else {
        return;
    };
    if !is_mode(&mode) {
        return;
    }
    let root = std::path::PathBuf::from(std::env::var("WEAVER_WORKER_INPUT_ROOT").unwrap());
    // SAFETY: the fixture producer exclusively maps this child channel to fd 3.
    let fd = unsafe { OwnedFd::from_raw_fd(3) };
    let mut channel = Channel::from_owned(fd, 262144).unwrap();
    let end = Instant::now() + Duration::from_secs(30);
    loop {
        assert!(Instant::now() < end, "serving rollback classifier watchdog");
        match channel.try_receive() {
            Ok(Receive::Complete(message)) if message.tag == Tag::Bootstrap => {
                let bootstrap: s::Ste100Bootstrap =
                    serde_json::from_slice(&message.payload).unwrap();
                let (_, _, mut ready) = classifier_fixture();
                ready.max_wire_bytes = bootstrap.instruction.max_wire_bytes;
                let digest = s::Digest::new(format!(
                    "{:x}",
                    Sha256::digest(bootstrap.scheme_text.as_bytes())
                ))
                .unwrap();
                ready.scheme_id = format!("ste100:{digest}");
                ready.artifact_sha256 = digest;
                channel
                    .begin_send(Tag::Ready, 0, serde_json::to_vec(&ready).unwrap())
                    .unwrap();
            }
            Ok(Receive::Complete(message)) => {
                assert_eq!(message.tag, Tag::Request);
                let request: s::Ste100Request = serde_json::from_slice(&message.payload).unwrap();
                let input = matches!(request.target, s::Ste100Target::Input { .. });
                if input && mode.ends_with("-checking") {
                    std::fs::write(
                        root.join("serving-rollback-checking.json"),
                        &message.payload,
                    )
                    .unwrap();
                    wait_file(&root, "serving-rollback-accepted");
                }
                let reply = if input {
                    crate::flow::context::input_tests::accepted(&request)
                } else if matches!(request.target, s::Ste100Target::Source(_)) {
                    super::super::emission::tests::reply(&request, &mode)
                } else {
                    s::Ste100Reply {
                        request_id: request.request_id,
                        target: request.target,
                        scheme_id: request.scheme_id,
                        result: s::Ste100Result::Accepted { bindings: vec![] },
                    }
                };
                channel
                    .begin_send(Tag::Reply, message.id, serde_json::to_vec(&reply).unwrap())
                    .unwrap();
            }
            Ok(_) => (),
            Err(_) => return,
        }
        if channel.has_pending_send() {
            channel.try_send().unwrap();
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

pub(super) fn gate(mode: &str, root: &Path, fd: i32) {
    packet!(fd, &frame(0, false));
    if mode.ends_with("-world-blocked") {
        wait_file(root, "serving-rollback-world-drain");
    }
    if holds_generation(mode) && mode.ends_with("-queued") {
        packet!(fd, &frame(1, false));
        std::fs::write(
            root.join("serving-rollback-queued"),
            b"actual second frame sent",
        )
        .unwrap();
    }
    let mut bytes = vec![0; weaver_types::MAX_ENVELOPE_BYTES];
    let mut execution: Option<OrganEnvelope> = None;
    let mut interrupted = false;
    let mut crossed = false;
    let end = Instant::now() + Duration::from_secs(30);
    loop {
        assert!(Instant::now() < end, "serving rollback gate watchdog");
        if mode.ends_with("-external-crossed")
            && let Some(original_execution) = execution.as_ref()
            && !crossed
            && root.join("serving-rollback-accepted").exists()
        {
            let answer = OrganEnvelope {
                exchange: original_execution.exchange.clone(),
                position: Position::Close,
                payload: Payload::ToolAnswer(weaver_types::ToolOutcome::Result {
                    content: "original crossed result".into(),
                }),
            };
            packet!(fd, &answer);
            std::fs::write(
                root.join("serving-rollback-tool-answer.json"),
                serde_json::to_vec(&answer).unwrap(),
            )
            .unwrap();
            crossed = true;
        }
        let n = match recv(fd, &mut bytes, MsgFlags::MSG_DONTWAIT) {
            Ok(n) => n,
            Err(nix::errno::Errno::EAGAIN) => {
                std::thread::sleep(Duration::from_millis(1));
                continue;
            }
            Err(error) => panic!("serving rollback gate receive: {error}"),
        };
        if n == 0 {
            return;
        }
        if (mode.ends_with("-world-blocked") || mode.ends_with("-external-interrupt-blocked"))
            && bytes[..n].iter().all(|byte| *byte == b'x')
        {
            continue;
        }
        let reply: OrganEnvelope = serde_json::from_slice(&bytes[..n]).unwrap();
        if matches!(reply.payload, Payload::Tool(_)) {
            assert!(mode.contains("-external-"));
            assert!(
                execution.is_none(),
                "rollback cannot dispatch a second execution"
            );
            assert_eq!(reply.exchange.opener, Opener::Harness);
            assert_eq!(reply.exchange.ordinal, 2);
            assert_eq!(reply.position, Position::Open);
            std::fs::write(root.join("serving-rollback-execution.json"), &bytes[..n]).unwrap();
            execution = Some(reply);
            if mode.ends_with("-external-rejection-pending") {
                wait_file(root, "serving-rollback-send-rejection");
                let rejected = OrganEnvelope {
                    exchange: weaver_types::ExchangeId {
                        opener: Opener::Harness,
                        ordinal: 999,
                    },
                    position: Position::Close,
                    payload: Payload::ToolAnswer(weaver_types::ToolOutcome::Result {
                        content: "unrelated \"result\" with original whitespace".into(),
                    }),
                };
                let raw = format!(
                    " \t\r\n{}\n\t ",
                    serde_json::to_string_pretty(&rejected).unwrap()
                );
                std::fs::write(root.join("serving-rollback-rejection.json"), &raw).unwrap();
                assert_eq!(
                    send(fd, raw.as_bytes(), MsgFlags::MSG_NOSIGNAL),
                    Ok(raw.len())
                );
            }
            if mode.ends_with("-external-interrupt-blocked") {
                wait_file(root, "serving-rollback-interrupt-drain");
            }
            continue;
        }
        if matches!(reply.payload, Payload::ToolInterrupt { .. }) {
            assert_eq!(reply.exchange, execution.as_ref().unwrap().exchange);
            assert_eq!(reply.position, Position::Continue);
            assert_eq!(
                reply.payload,
                Payload::ToolInterrupt {
                    by: weaver_types::KillCause::Unload
                }
            );
            assert!(!interrupted, "one interrupt for the original execution");
            interrupted = true;
            std::fs::write(root.join("serving-rollback-interrupt.json"), &bytes[..n]).unwrap();
            if mode.ends_with("-external-unload-queued") {
                packet!(fd, &frame(1, false));
                std::fs::write(
                    root.join("serving-rollback-queued"),
                    b"actual frame crossed after Unload interrupt",
                )
                .unwrap();
            }
            continue;
        }
        if reply.payload == Payload::Directive(LifecycleDirective::Lower) {
            assert_eq!(reply.exchange.opener, Opener::Harness);
            assert_eq!(reply.position, Position::Open);
            if mode.ends_with("-idle") {
                assert!(root.join("serving-rollback-world.json").exists());
            }
            std::fs::write(root.join("serving-rollback-lower.json"), &bytes[..n]).unwrap();
            if mode.ends_with("-external-unload-queued") {
                wait_file(root, "serving-rollback-gate-burst");
            }
            if let Some(execution) = &execution {
                assert_eq!(reply.exchange.ordinal, execution.exchange.ordinal + 1);
                if !crossed {
                    assert!(
                        interrupted,
                        "Unload interrupt must precede Lower without waiting for its answer"
                    );
                }
                if !crossed && !mode.ends_with("-external-silent") {
                    let answer = OrganEnvelope {
                        exchange: execution.exchange.clone(),
                        position: Position::Close,
                        payload: Payload::ToolAnswer(weaver_types::ToolOutcome::Killed {
                            partial: Some("original partial result".into()),
                            by: weaver_types::KillCause::Unload,
                        }),
                    };
                    packet!(fd, &answer);
                    std::fs::write(
                        root.join("serving-rollback-tool-answer.json"),
                        serde_json::to_vec(&answer).unwrap(),
                    )
                    .unwrap();
                }
            }
            packet!(
                fd,
                &OrganEnvelope {
                    exchange: reply.exchange,
                    position: Position::Close,
                    payload: Payload::Answer(LifecycleAnswer::GateStopped),
                }
            );
            if mode.ends_with("-external-silent") {
                std::fs::write(
                    root.join("serving-rollback-silent-pid"),
                    std::process::id().to_string(),
                )
                .unwrap();
                // GateStopped closes the exchange while this exact process
                // remains alive. The owner must observe that distinction.
                wait_file(root, "serving-rollback-silent-release");
            }
            return;
        }
        assert_eq!(reply.exchange, frame(0, false).exchange);
        assert_eq!(reply.position, Position::Close);
        let Payload::Frame(world) = reply.payload else {
            panic!("actual original world close")
        };
        let value: serde_json::Value = serde_json::from_slice(&world.octets().unwrap()).unwrap();
        if clean_turn(mode) {
            assert_eq!(value["kind"], "answered");
            assert_eq!(value["turn"], "session-0");
        } else {
            assert_eq!(value["kind"], "stopped");
            assert_eq!(value["reason"], "The agent is leaving.");
            if active_turn(mode) {
                assert_eq!(value["turn"], "session-0");
            } else {
                assert!(value.get("turn").is_none());
                assert!(value.get("run").is_none());
            }
        }
        assert!(!root.join("serving-rollback-world.json").exists());
        std::fs::write(root.join("serving-rollback-world.json"), &bytes[..n]).unwrap();
    }
}

// The ordinary session decoder returns on actual decode EOF. These modes keep
// the same SPU process alive for its original residency channel's Release.
pub(super) fn release_peer() {
    let mut bytes = vec![0; weaver_types::MAX_ENVELOPE_BYTES];
    let end = Instant::now() + Duration::from_secs(25);
    loop {
        match recv(3, &mut bytes, MsgFlags::MSG_DONTWAIT) {
            Ok(0) => return,
            Ok(n) => {
                let request: OrganEnvelope = serde_json::from_slice(&bytes[..n]).unwrap();
                assert_eq!(request.exchange.opener, Opener::Harness);
                assert_eq!(request.position, Position::Open);
                assert_eq!(
                    request.payload,
                    Payload::Directive(LifecycleDirective::Release)
                );
                packet!(
                    3,
                    &OrganEnvelope {
                        exchange: request.exchange,
                        position: Position::Close,
                        payload: Payload::Answer(LifecycleAnswer::Released),
                    }
                );
                return;
            }
            Err(nix::errno::Errno::EAGAIN) => {
                assert!(Instant::now() < end, "serving rollback Release watchdog");
                std::thread::sleep(Duration::from_millis(1));
            }
            other => panic!("serving rollback residency receive: {other:?}"),
        }
    }
}

pub(in crate::flow::context::worker_startup) struct Handoff {
    actual: Box<FoundationEnteredRollbackHandoff>,
    peer: OwnedFd,
    socket: std::path::PathBuf,
    origin: Arc<()>,
    enter: AbsoluteBound,
    cause: weaver_types::Cause,
    before: Vec<u8>,
    original: Vec<u8>,
    original_identity: Option<usize>,
    engine_identity: Option<usize>,
    input_bound: Option<AbsoluteBound>,
    world_recorded: Option<weaver_trace::Sequence>,
    world_probe: Option<Box<dyn Fn() -> (bool, bool) + Send>>,
    pending_turn_ticket: Option<u64>,
    pending_rejection: Option<(u64, usize, AbsoluteBound)>,
}

pub(in crate::flow::context::worker_startup) fn handoff(
    serving: FoundationServing<'_>,
    socket: &Path,
    root: &Path,
    mode: &str,
    member: &mut Member,
    reader: &mut UnixStream,
) -> Handoff {
    let origin = Arc::clone(&serving.owner.context.read_origin);
    let enter = serving.owner.enter;
    let cause = serving
        .owner
        .resident
        .evidence
        .startup
        .original_enter
        .as_ref()
        .unwrap()
        .envelope()
        .enter()
        .cause;
    assert_eq!(cause.uid, 73);
    assert!(serving.owner.ready.sent());
    if mode.ends_with("-source-file") {
        member.hold_kind = Some("message.user");
    }
    if mode.ends_with("-mark-file") {
        member.hold_mark = true;
    }
    if mode.ends_with("-task-read") {
        member.hold_control = Some("marks");
    }
    let mut guard = SessionGuard(Some(Session::begin(serving, &FAMILY, CutMethod::Flush)));
    let mut direct = None;
    let done = AtomicBool::new(false);
    let trace_pause = AtomicBool::new(false);
    let trace_paused = AtomicBool::new(false);
    let mut pending_peer = None;
    let leave_at_held_member = mode.ends_with("-source-file")
        || mode.ends_with("-task-read")
        || mode.ends_with("-mark-file");
    let leave_at_held_measurement =
        is_mode(mode) && mode.contains("-placement") && !is_correction(mode);
    let root_leave = || {
        management(
            socket,
            LifecycleDirective::Leave {
                cause: weaver_types::Cause { uid: 74 },
                forced: true,
                rollback: true,
            },
        )
    };
    reader.set_nonblocking(true).unwrap();
    let (peer, before) = std::thread::scope(|scope| {
        let responder = scope.spawn(|| {
            let mut peer = None;
            while !done.load(Ordering::SeqCst) {
                member.step_live(mode);
                let member_held = member.held_file.is_some() || member.held_control.is_some();
                if member_held {
                    std::fs::write(
                        root.join("serving-rollback-member-held"),
                        b"original response held",
                    )
                    .unwrap();
                }
                // The driver can already be waiting on either held answer.
                // Deliver root Leave from this independent peer at that boundary.
                if peer.is_none()
                    && ((leave_at_held_member && member_held)
                        || (leave_at_held_measurement
                            && root.join("serving-rollback-measured").exists()))
                {
                    peer = Some(root_leave());
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            peer
        });
        let recorder = scope.spawn(|| {
            let mut bytes = vec![];
            while !done.load(Ordering::SeqCst) {
                if trace_pause.load(Ordering::SeqCst) {
                    trace_paused.store(true, Ordering::SeqCst);
                } else {
                    let _ = reader.read_to_end(&mut bytes);
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            if !trace_pause.load(Ordering::SeqCst) {
                let _ = reader.read_to_end(&mut bytes);
            }
            bytes
        });
        let _stop = StopThreads(&done);
        if mode.ends_with("-checking") {
            drive_until_file(
                guard.0.as_mut().unwrap(),
                root,
                "serving-rollback-checking.json",
            );
        } else {
            let session = guard.0.as_mut().unwrap();
            drive(session, Progress::Accepted);
            if mode.ends_with("-idle") || mode.ends_with("-final-close") {
                session.start_turn(TurnKey("session-0".into())).unwrap();
                drive(session, Progress::Completed);
                wait_file(root, "serving-rollback-world.json");
                if mode.ends_with("-idle") {
                    assert_eq!(session.next_input(), Ok(Progress::Completed));
                    assert_eq!(session.progress(), Progress::Pending);
                }
            } else if active_turn(mode)
                && !mode.ends_with("-placed-clock-failure")
                && !mode.ends_with("-source-pending")
            {
                session.start_turn(TurnKey("session-0".into())).unwrap();
                let marker = if is_correction(mode) {
                    if mode.ends_with("-placement") {
                        "serving-rollback-correction-measured.json"
                    } else {
                        "serving-rollback-correction-generation.json"
                    }
                } else if mode.contains("-external-") {
                    "serving-rollback-execution.json"
                } else if holds_generation(mode) {
                    "session-generation-0.json"
                } else if mode.contains("-placement") {
                    "serving-rollback-measured"
                } else {
                    "serving-rollback-member-held"
                };
                drive_until_file(session, root, marker);
            } else {
                assert!(!root.join("session-generation-0.json").exists());
            }
        }
        if mode.ends_with("-source-elected")
            || mode.ends_with("-source-pending")
            || mode.ends_with("-final-close")
            || mode.ends_with("-placed-clock-failure")
            || mode.ends_with("-world-blocked")
            || mode.ends_with("-external-interrupt-blocked")
            || mode.ends_with("-external-unload-queued")
            || mode.ends_with("-external-rejection-pending")
        {
            let input = match guard.0.take().unwrap().into_owner() {
                Ok(input) => input,
                Err(management) => {
                    let _guard = Guard {
                        input: None,
                        management: Some(management),
                    };
                    panic!("no root directive before fixture positions input");
                }
            };
            direct = Some(Guard {
                input: Some(input),
                management: None,
            });
            if mode.ends_with("-source-elected") || mode.ends_with("-source-pending") {
                let input = direct.as_mut().unwrap().input.as_mut().unwrap();
                if mode.ends_with("-source-pending") {
                    trace_pause.store(true, Ordering::SeqCst);
                    let end = Instant::now() + Duration::from_secs(3);
                    while !trace_paused.load(Ordering::SeqCst) {
                        assert!(
                            Instant::now() < end,
                            "trace reader must acknowledge its pause"
                        );
                        std::thread::sleep(Duration::from_millis(1));
                    }
                    let mut clock = FakePumpIo::new(input.serving.owner.context.last_ns);
                    assert!(
                        super::super::super::management::stop::tests::fill_trace(
                            input.serving.owner.context,
                            &mut clock,
                        ) > 0
                    );
                }
                input.start_turn(TurnKey("session-0".into())).unwrap();
                if mode.ends_with("-source-pending") {
                    let mut io = RootDuringAuthorWait { socket, peer: None };
                    assert_eq!(
                        input.advance_with_io(&mut io),
                        FoundationInputProgress::ManagementPending
                    );
                    pending_peer = Some(
                        io.peer
                            .expect("actual root Leave sent from the pending author wait"),
                    );
                    let pending = input.serving.owner.context.pending_author().unwrap();
                    assert_eq!(pending.event.kind, serving::Kind::TurnStarted);
                    assert!(input.serving.owner.context.input_turn.is_some());
                    assert!(input.serving.owner.context.open_turn.is_none());
                    assert!(input.token.is_none());
                }
            } else if mode.ends_with("-placed-clock-failure") || mode.ends_with("-world-blocked") {
                let input = direct.as_mut().unwrap().input.as_mut().unwrap();
                input.start_task(TurnKey("session-0".into())).unwrap();
                drive_input(input, FoundationInputProgress::Task);
                input
                    .place_task("turn input".into(), CutMethod::Flush)
                    .unwrap();
                drive_input(input, FoundationInputProgress::Placed);
                if mode.ends_with("-world-blocked") {
                    input.generate().unwrap();
                    drive_input(input, FoundationInputProgress::Generated);
                    input.repair(&FAMILY).unwrap();
                    drive_input(input, FoundationInputProgress::Effective);
                    input
                        .sort_outbound(super::super::emission::tests::selection(
                            input.effective_emission().unwrap(),
                        ))
                        .unwrap();
                    drive_input(input, FoundationInputProgress::Sorted);
                    input.begin_pairs().unwrap();
                    drive_input(input, FoundationInputProgress::PairsComplete);
                    super::super::super::management::stop::tests::fill(
                        input.serving.owner.gate.fd().as_raw_fd(),
                    );
                    input.close_answer().unwrap();
                    assert_eq!(input.advance(), FoundationInputProgress::Pending);
                    let close = input.engine.as_ref().unwrap().answer.as_ref().unwrap();
                    assert!(close.recorded().is_some());
                    assert!(!close.send().unwrap().sent());
                    assert!(!close.send().unwrap().withdrawn());
                    assert!(input.serving.owner.context.open_turn.is_none());
                }
            }
            if mode.ends_with("-external-interrupt-blocked") {
                let input = direct.as_mut().unwrap().input.as_mut().unwrap();
                let fd = input.serving.owner.gate.fd().as_raw_fd();
                super::super::super::management::stop::tests::fill(fd);
                // Exhaust remaining packet slots, so the small actual
                // ToolInterrupt also receives EAGAIN.
                loop {
                    match send(fd, b"x", MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_NOSIGNAL) {
                        Ok(1) => (),
                        Err(nix::errno::Errno::EAGAIN) => break,
                        other => panic!("fill final gate packet slots: {other:?}"),
                    }
                }
            }
            if mode.ends_with("-external-unload-queued")
                || mode.ends_with("-external-rejection-pending")
            {
                let input = direct.as_mut().unwrap().input.as_mut().unwrap();
                trace_pause.store(true, Ordering::SeqCst);
                let end = Instant::now() + Duration::from_secs(3);
                while !trace_paused.load(Ordering::SeqCst) {
                    assert!(
                        Instant::now() < end,
                        "queued gate fixture must pause its real trace reader"
                    );
                    std::thread::sleep(Duration::from_millis(1));
                }
                assert!(input.serving.owner.context.pending_author().is_none());
                let mut clock = FakePumpIo::new(input.serving.owner.context.last_ns);
                assert!(
                    super::super::super::management::stop::tests::fill_trace(
                        input.serving.owner.context,
                        &mut clock
                    ) > 0
                );
                if mode.ends_with("-external-rejection-pending") {
                    std::fs::write(
                        root.join("serving-rollback-send-rejection"),
                        b"trace writer is blocked",
                    )
                    .unwrap();
                    wait_file(root, "serving-rollback-rejection.json");
                    let end = Instant::now() + Duration::from_secs(5);
                    while input
                        .serving
                        .owner
                        .context
                        .pairs
                        .pending_rejection()
                        .is_none()
                    {
                        assert!(
                            Instant::now() < end,
                            "actual wrong return must reach its original pair owner"
                        );
                        assert_eq!(input.advance(), FoundationInputProgress::Pending);
                    }
                    let mut io = RootDuringAuthorWait { socket, peer: None };
                    assert_eq!(
                        input.advance_with_io(&mut io),
                        FoundationInputProgress::ManagementPending
                    );
                    pending_peer = Some(
                        io.peer
                            .expect("actual root Leave heard during rejected-return author wait"),
                    );
                    assert_eq!(
                        input
                            .serving
                            .owner
                            .context
                            .pending_author()
                            .unwrap()
                            .event
                            .kind,
                        serving::Kind::ToolReturnRejected
                    );
                }
            }
        }
        if leave_at_held_measurement {
            assert!(
                root.join("serving-rollback-measured").exists(),
                "independent Leave requires the held measurement boundary"
            );
        }
        let peer = pending_peer
            .take()
            .or_else(|| (!leave_at_held_member && !leave_at_held_measurement).then(&root_leave));
        if let Some(held) = &mut direct {
            let end = Instant::now() + Duration::from_secs(10);
            let input = held.input.as_mut().unwrap();
            while input.progress() != FoundationInputProgress::ManagementPending {
                assert!(
                    Instant::now() < end,
                    "root must interrupt the exact retained input"
                );
                input.observe(&mut MonotonicIo).unwrap();
                std::thread::sleep(Duration::from_millis(1));
            }
            held.management = Some(held.input.take().unwrap().management().ok().unwrap());
            while held.management.as_ref().unwrap().progress()
                != super::super::management::FoundationManagementProgress::DirectivePending
            {
                assert!(Instant::now() < end, "actual root directive must complete");
                held.management.as_mut().unwrap().advance();
            }
        } else {
            drive(guard.0.as_mut().unwrap(), Progress::DirectivePending);
        }
        done.store(true, Ordering::SeqCst);
        let peer = peer.or(responder.join().unwrap()).unwrap();
        assert!(read_packet(&peer).is_none());
        (peer, recorder.join().unwrap())
    });
    let management = match direct.as_mut().and_then(|held| held.management.take()) {
        Some(management) => management,
        None => match guard.0.take().unwrap().into_owner() {
            Err(owner) => owner,
            Ok(input) => {
                let _guard = Guard {
                    input: Some(input),
                    management: None,
                };
                panic!("actual serving rollback must retain its management owner");
            }
        },
    };
    if mode.ends_with("-idle") {
        assert!(management.input.stage == super::super::Stage::Waiting);
        assert!(management.input.serving.owner.context.ever_started_turn);
        assert!(management.input.token.is_none());
        assert!(!management.input.serving.gate_exchanges.is_empty());
    } else if mode.ends_with("-accepted") {
        assert!(management.input.stage == super::super::Stage::Accepted);
        assert!(management.input.token.is_some());
        assert!(management.input.received.is_some());
        assert!(!management.input.serving.owner.context.ever_started_turn);
    } else if mode.ends_with("-checking") {
        assert!(management.input.stage == super::super::Stage::Checking);
        assert!(management.input.token.is_none());
        assert!(
            management
                .input
                .serving
                .owner
                .context
                .deadlines
                .get(PumpDeadline::Gate)
                .is_some()
        );
    } else if mode.ends_with("-source-elected") {
        assert!(management.input.stage == super::super::Stage::Source);
        assert!(management.input.token.is_some());
        assert!(management.input.serving.owner.context.open_turn.is_none());
    } else if mode.ends_with("-source-pending") {
        assert!(management.input.stage == super::super::Stage::Source);
        assert!(management.input.token.is_none());
        assert!(management.input.serving.owner.context.input_turn.is_some());
        assert!(management.input.serving.owner.context.open_turn.is_none());
        assert!(!management.input.serving.owner.context.ever_started_turn);
        assert_eq!(
            management
                .input
                .serving
                .owner
                .context
                .pending_author()
                .unwrap()
                .event
                .kind,
            serving::Kind::TurnStarted
        );
    } else {
        assert!(management.input.serving.owner.context.ever_started_turn);
        assert!(management.input.engine.is_some());
        if holds_generation(mode) && mode.ends_with("-queued") {
            assert_eq!(
                management.input.serving.queued_inputs.len(),
                1,
                "the actual second frame is retained before Leave admission"
            );
            assert_eq!(
                management.input.serving.queued_inputs[0]
                    .envelope()
                    .exchange,
                frame(1, false).exchange
            );
        }
        if mode.ends_with("-final-close") {
            let close = management
                .input
                .engine
                .as_ref()
                .unwrap()
                .answer
                .as_ref()
                .unwrap();
            assert!(close.recorded().is_some());
            assert!(close.send().unwrap().sent());
        } else if mode.ends_with("-world-blocked") {
            let close = management
                .input
                .engine
                .as_ref()
                .unwrap()
                .answer
                .as_ref()
                .unwrap();
            assert!(close.recorded().is_some());
            assert!(!close.send().unwrap().sent());
            assert!(!close.send().unwrap().withdrawn());
            assert!(!root.join("serving-rollback-world.json").exists());
        } else if mode.ends_with("-placed-clock-failure") {
            let Some(crate::flow::context::PlacementCompletion::Granted(grant)) =
                management.input.engine.as_ref().unwrap().placement.as_ref()
            else {
                panic!("the actual engine must still own its uncancelled placement grant")
            };
            assert!(Arc::ptr_eq(
                &grant.identity,
                management
                    .input
                    .serving
                    .owner
                    .context
                    .place_engine
                    .as_ref()
                    .unwrap()
                    .outstanding
                    .as_ref()
                    .unwrap(),
            ));
            assert!(management.input.serving.owner.context.place.is_none());
            assert!(management.input.serving.owner.context.generation.is_none());
            assert!(!root.join("session-generation-0.json").exists());
        }
    }
    let pending_turn_ticket = mode.ends_with("-source-pending").then(|| {
        management
            .input
            .serving
            .owner
            .context
            .pending_author()
            .unwrap()
            .ticket
    });
    let pending_rejection = mode.ends_with("-external-rejection-pending").then(|| {
        let context = &management.input.serving.owner.context;
        let rejected = context.pairs.pending_rejection().unwrap();
        assert_eq!(
            rejected.raw_bytes(),
            std::fs::read(root.join("serving-rollback-rejection.json")).unwrap()
        );
        assert!(context.pair_rejected_authoring);
        (
            context.pending_author().unwrap().ticket,
            rejected.raw_bytes().as_ptr() as usize,
            context.deadlines.get(PumpDeadline::Decision).unwrap(),
        )
    });
    if is_correction(mode) {
        assert!(!root.join("serving-rollback-lower.json").exists());
        assert!(root.join("session-generation-0.json").exists());
        let context = &management.input.serving.owner.context;
        assert!(context.open_turn.is_some());
        if mode.ends_with("-placement") {
            assert!(
                context.measure.is_some(),
                "the original correction measurement is in flight"
            );
            assert!(context.generation.is_none());
            assert!(
                !root
                    .join("serving-rollback-correction-generation.json")
                    .exists()
            );
        } else {
            assert!(
                context.generation.is_some(),
                "the original correction generation is in flight"
            );
            assert!(!root.join("serving-rollback-cancel.json").exists());
        }
    }
    let world_recorded = management
        .input
        .engine
        .as_ref()
        .and_then(|engine| engine.answer.as_ref())
        .and_then(|close| close.recorded());
    let world_probe = management
        .input
        .engine
        .as_ref()
        .and_then(|engine| engine.answer.as_ref())
        .and_then(|close| close.send())
        .map(|send| send.state_probe_for_test());
    let original = management
        .input
        .received
        .as_ref()
        .map(|received| received.raw_bytes().to_vec())
        .unwrap_or_default();
    let input_bound = management
        .input
        .serving
        .owner
        .context
        .deadlines
        .get(PumpDeadline::Gate);
    let original_identity = management
        .input
        .received
        .as_ref()
        .map(|received| received.raw_bytes().as_ptr() as usize);
    let engine_identity = management
        .input
        .engine
        .as_deref()
        .map(|engine| std::ptr::from_ref(engine) as usize);
    let actual = match management.rollback_handoff() {
        Ok(handoff) => handoff,
        Err(management) => {
            let _guard = Guard {
                input: None,
                management: Some(management),
            };
            panic!("the actual serving position must yield its rollback handoff: {mode}");
        }
    };
    assert!(Arc::ptr_eq(&actual.origin, &origin));
    assert_eq!(actual.enter, enter);
    assert_original(&actual.input, &original, original_identity, engine_identity);
    assert!(actual.ready.as_ref().unwrap().sent());
    assert_eq!(actual.message.envelope.exchange.opener, Opener::Admin);
    assert_eq!(actual.message.envelope.exchange.ordinal, 99);
    assert_eq!(
        actual.message.envelope.payload,
        Payload::Directive(LifecycleDirective::Leave {
            cause: weaver_types::Cause { uid: 74 },
            forced: true,
            rollback: true,
        })
    );
    Handoff {
        actual,
        peer,
        socket: socket.to_owned(),
        origin,
        enter,
        cause,
        before,
        original,
        original_identity,
        engine_identity,
        input_bound,
        world_recorded,
        world_probe,
        pending_turn_ticket,
        pending_rejection,
    }
}

struct RootDuringAuthorWait<'a> {
    socket: &'a Path,
    peer: Option<OwnedFd>,
}
impl crate::flow::clock::PumpIo for RootDuringAuthorWait<'_> {
    fn now(&mut self) -> std::io::Result<u64> {
        MonotonicIo.now()
    }
    fn wait(
        &mut self,
        interests: &[crate::flow::clock::WaitInterest<'_>],
        deadline: Option<u64>,
    ) -> std::io::Result<crate::flow::clock::WaitOutcome> {
        if self.peer.is_none() {
            self.peer = Some(management(
                self.socket,
                LifecycleDirective::Leave {
                    cause: weaver_types::Cause { uid: 74 },
                    forced: true,
                    rollback: true,
                },
            ));
        }
        MonotonicIo.wait(interests, deadline)
    }
}

fn drive_input(input: &mut FoundationInput<'_>, target: FoundationInputProgress) {
    let end = Instant::now() + Duration::from_secs(10);
    while input.progress() != target {
        assert!(
            Instant::now() < end,
            "input did not reach {target:?}: {:?}",
            input.detail()
        );
        assert_eq!(
            input.progress(),
            FoundationInputProgress::Pending,
            "{:?}",
            input.detail()
        );
        input.advance();
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn assert_original(
    input: &super::super::super::rollback::RollbackInputWork,
    original: &[u8],
    original_identity: Option<usize>,
    engine_identity: Option<usize>,
) {
    assert_eq!(
        input
            .test_received()
            .map(|received| received.raw_bytes().as_ptr() as usize),
        original_identity
    );
    assert_eq!(
        input
            .test_received()
            .map(|received| received.raw_bytes())
            .unwrap_or_default(),
        original
    );
    assert_eq!(input.test_engine_identity(), engine_identity);
}

fn drive_until_file(session: &mut Session<'_>, root: &Path, marker: &str) {
    let end = Instant::now() + Duration::from_secs(15);
    while !root.join(marker).exists() {
        assert!(
            Instant::now() < end,
            "missing {marker}: {:?}",
            session.detail()
        );
        assert!(
            matches!(session.progress(), Progress::Pending | Progress::Accepted),
            "{:?}: {:?}",
            session.progress(),
            session.detail()
        );
        session.advance();
        std::thread::sleep(Duration::from_millis(1));
    }
}

struct RollbackGuard(
    Box<crate::flow::context::worker_startup::entered_rollback::FoundationEnteredRollback>,
);
impl Drop for RollbackGuard {
    fn drop(&mut self) {
        self.0.test_reap();
    }
}

fn drive_silent_reap(
    owner: &mut crate::flow::context::worker_startup::entered_rollback::FoundationEnteredRollback,
    root: &Path,
) {
    let execution: OrganEnvelope = serde_json::from_slice(
        &std::fs::read(root.join("serving-rollback-execution.json")).unwrap(),
    )
    .unwrap();
    let end = Instant::now() + Duration::from_secs(3);
    while owner
        .test_gate()
        .is_none_or(|gate| gate.test_replies().1.is_none())
    {
        assert!(Instant::now() < end, "actual GateStopped must arrive");
        owner.advance();
        assert!(
            owner.test_context().failure().is_none(),
            "{:?}",
            owner.detail()
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    wait_file(root, "serving-rollback-silent-pid");
    let pid = nix::unistd::Pid::from_raw(
        std::fs::read_to_string(root.join("serving-rollback-silent-pid"))
            .unwrap()
            .parse()
            .unwrap(),
    );
    assert_eq!(nix::sys::signal::kill(pid, None), Ok(()));
    for _ in 0..8 {
        assert_eq!(
            owner.test_account().1.arms.child.as_ref().unwrap().state(),
            ChildState::Running
        );
        assert!(
            owner
                .test_context()
                .pairs
                .executing()
                .is_some_and(|(_, exchange, sent)| { sent && *exchange == execution.exchange }),
            "GateStopped cannot invent Killed/Fault while the actual gate child is alive"
        );
        owner.advance();
    }
    assert!(owner.test_context().pairs.executing().is_some());
    std::fs::write(root.join("serving-rollback-silent-release"), b"exit now").unwrap();
    let end = Instant::now() + Duration::from_secs(15);
    while owner.progress() != FoundationEnteredRollbackProgress::Closed {
        assert!(
            Instant::now() < end,
            "silent gate cleanup: {:?}",
            owner.detail()
        );
        owner.advance();
        if owner.test_context().pairs.executing().is_none() {
            assert!(matches!(
                owner.test_account().1.arms.child.as_ref().unwrap().state(),
                ChildState::Reaped(nix::sys::wait::WaitStatus::Exited(_, 0))
            ));
            assert_eq!(
                nix::sys::signal::kill(pid, None),
                Err(nix::errno::Errno::ESRCH)
            );
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn drive_blocked_world(
    owner: &mut crate::flow::context::worker_startup::entered_rollback::FoundationEnteredRollback,
    root: &Path,
    recorded: weaver_trace::Sequence,
    probe: Box<dyn Fn() -> (bool, bool) + Send>,
) {
    std::thread::scope(|scope| {
        let drainer = scope.spawn(move || {
            let end = Instant::now() + Duration::from_secs(3);
            loop {
                let (sent, withdrawn) = probe();
                assert!(!sent, "the original blocked world reply must remain unsent");
                if withdrawn {
                    break;
                }
                assert!(
                    Instant::now() < end,
                    "rollback must withdraw the exact blocked world ticket"
                );
                std::thread::sleep(Duration::from_millis(1));
            }
            std::fs::write(
                root.join("serving-rollback-world-drain"),
                b"drain filler for Lower",
            )
            .unwrap();
        });
        let end = Instant::now() + Duration::from_secs(15);
        while owner.progress() != FoundationEnteredRollbackProgress::Closed {
            assert!(
                Instant::now() < end,
                "blocked world rollback: {:?}",
                owner.detail()
            );
            owner.advance();
            assert!(
                owner.test_context().failure().is_none(),
                "{:?}",
                owner.detail()
            );
            assert_eq!(
                owner.test_input().test_world(),
                Some((Some(recorded), false, true))
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        drainer.join().unwrap();
    });
    assert!(!root.join("serving-rollback-world.json").exists());
}

struct DrainInterruptOnWait<'a> {
    root: &'a Path,
    socket: &'a Path,
    gate_fd: i32,
    released: bool,
    secondary: Option<OwnedFd>,
    read_bound: Option<AbsoluteBound>,
    observe_sent: bool,
}
impl crate::flow::clock::PumpIo for DrainInterruptOnWait<'_> {
    fn now(&mut self) -> std::io::Result<u64> {
        MonotonicIo.now()
    }
    fn wait(
        &mut self,
        interests: &[crate::flow::clock::WaitInterest<'_>],
        deadline: Option<u64>,
    ) -> std::io::Result<crate::flow::clock::WaitOutcome> {
        if !self.released {
            assert!(
                interests
                    .iter()
                    .any(|interest| interest.fd.as_raw_fd() == self.gate_fd
                        && interest.events.contains(nix::poll::PollFlags::POLLOUT)),
                "actual queued control must wait on the original full gate socket"
            );
            assert!(!self.root.join("serving-rollback-interrupt.json").exists());
            assert!(!self.root.join("serving-rollback-lower.json").exists());
            if self.secondary.is_none() {
                self.secondary = Some(dial(self.socket));
            }
            if let Some(bytes) = read_packet(self.secondary.as_ref().unwrap()) {
                let reply: OrganEnvelope = serde_json::from_slice(&bytes).unwrap();
                assert!(self.observe_sent);
                assert_eq!(
                    reply.exchange,
                    weaver_types::ExchangeId {
                        opener: Opener::Admin,
                        ordinal: 100
                    }
                );
                assert_eq!(reply.position, Position::Close);
                assert_eq!(
                    reply.payload,
                    Payload::Answer(LifecycleAnswer::InTransition {
                        constituents: vec![]
                    })
                );
                assert!(!self.read_bound.unwrap().expired(MonotonicIo.now()?));
                std::fs::write(
                    self.root.join("serving-rollback-interrupt-drain"),
                    b"actual secondary Observe answered while gate remained blocked",
                )
                .unwrap();
                self.released = true;
            }
        }
        // A fixture wake exposes the retained read bound to its caller while
        // the peer is still silent. It never edits any owner deadline.
        let wake = if self.released {
            deadline
        } else {
            let local = MonotonicIo.now()?.saturating_add(5_000_000);
            Some(deadline.map_or(local, |deadline| deadline.min(local)))
        };
        MonotonicIo.wait(interests, wake)
    }
}

fn drive_blocked_interrupt(
    owner: &mut crate::flow::context::worker_startup::entered_rollback::FoundationEnteredRollback,
    root: &Path,
    socket: &Path,
) {
    let mut io = DrainInterruptOnWait {
        root,
        socket,
        gate_fd: owner
            .test_account()
            .1
            .arms
            .port
            .as_ref()
            .unwrap()
            .fd()
            .as_raw_fd(),
        released: false,
        secondary: None,
        read_bound: None,
        observe_sent: false,
    };
    let leave = owner.test_account().1.leave;
    let mut read_observations = 0;
    let end = Instant::now() + Duration::from_secs(15);
    while owner.progress() != FoundationEnteredRollbackProgress::Closed {
        assert!(
            Instant::now() < end,
            "backpressured interrupt rollback: {:?}",
            owner.detail()
        );
        owner.advance_with_io(&mut io);
        assert!(
            owner.test_context().failure().is_none(),
            "{:?}",
            owner.detail()
        );
        if let Some(bound) = owner.test_context().deadlines.get(PumpDeadline::AdminRead) {
            if let Some(original) = io.read_bound {
                assert_eq!(
                    bound, original,
                    "secondary read never receives a new window"
                );
            } else {
                assert_eq!(
                    bound.deadline_ns(),
                    bound
                        .began_ns()
                        .saturating_add(crate::state::ANSWER_BOUND_MS * 1_000_000)
                        .min(leave.deadline_ns())
                );
                io.read_bound = Some(bound);
            }
            read_observations += 1;
            if read_observations >= 2 && !io.observe_sent {
                packet!(
                    io.secondary.as_ref().unwrap().as_raw_fd(),
                    &OrganEnvelope {
                        exchange: weaver_types::ExchangeId {
                            opener: Opener::Admin,
                            ordinal: 100
                        },
                        position: Position::Open,
                        payload: Payload::Directive(LifecycleDirective::Observe),
                    }
                );
                io.observe_sent = true;
            }
        } else if io.read_bound.is_some() && !io.observe_sent {
            panic!("the silent secondary peer lost its original AdminRead bound");
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(
        io.released,
        "actual control EAGAIN must reach the common pump wait"
    );
    assert!(read_observations >= 2 && io.observe_sent);
    assert!(
        read_packet(io.secondary.as_ref().unwrap()).is_none(),
        "secondary Observe answers once"
    );
    let (control, send) = owner.test_input().cancel_control().unwrap();
    assert_eq!(
        control.payload,
        Payload::ToolInterrupt {
            by: weaver_types::KillCause::Unload
        }
    );
    assert_eq!(control.position, Position::Continue);
    assert!(send.sent() && send.sent_at().is_some() && !send.withdrawn());
    assert_eq!(
        serde_json::from_slice::<OrganEnvelope>(
            &std::fs::read(root.join("serving-rollback-interrupt.json")).unwrap()
        )
        .unwrap(),
        *control
    );
}

struct BrokenClock<'a> {
    clock: &'a mut FakePumpIo,
    broken: bool,
}

struct QueueBurstOnWait<'a> {
    root: &'a Path,
    waits: usize,
    released: bool,
}
impl crate::flow::clock::PumpIo for QueueBurstOnWait<'_> {
    fn now(&mut self) -> std::io::Result<u64> {
        MonotonicIo.now()
    }
    fn wait(
        &mut self,
        interests: &[crate::flow::clock::WaitInterest<'_>],
        deadline: Option<u64>,
    ) -> std::io::Result<crate::flow::clock::WaitOutcome> {
        loop {
            let now = MonotonicIo.now()?;
            if deadline.is_some_and(|deadline| now >= deadline) {
                return Ok(crate::flow::clock::WaitOutcome::Timeout);
            }
            if !self.released
                && self.root.join("serving-rollback-queued").exists()
                && self.root.join("serving-rollback-lower.json").exists()
            {
                self.waits += 1;
                if self.waits >= 2 {
                    std::fs::write(
                        self.root.join("serving-rollback-gate-burst"),
                        b"deliver original ToolAnswer and GateStopped under real trace pressure",
                    )
                    .unwrap();
                    self.released = true;
                }
            }
            if self.released {
                return MonotonicIo.wait(interests, deadline);
            }
            let local = now.saturating_add(5_000_000);
            match MonotonicIo.wait(
                interests,
                Some(deadline.map_or(local, |deadline| deadline.min(local))),
            )? {
                ready @ crate::flow::clock::WaitOutcome::Ready(_) => return Ok(ready),
                // Probe wakes stay inside this wait. The common pump may see
                // Timeout only when its original requested deadline expires.
                crate::flow::clock::WaitOutcome::Timeout => (),
            }
        }
    }
}

fn drive_queued_pressure(
    owner: &mut crate::flow::context::worker_startup::entered_rollback::FoundationEnteredRollback,
    root: &Path,
    trace_release: &AtomicBool,
) {
    let mut io = QueueBurstOnWait {
        root,
        waits: 0,
        released: false,
    };
    let end = Instant::now() + Duration::from_secs(15);
    loop {
        assert!(
            Instant::now() < end,
            "actual pending gate record must yield its held packet"
        );
        owner.advance_with_io(&mut io);
        assert!(
            owner.test_context().failure().is_none(),
            "{:?}",
            owner.detail()
        );
        let pending = owner.test_context().pending_author();
        let held = owner
            .test_account()
            .1
            .arms
            .port
            .as_ref()
            .and_then(|port| port.held());
        if pending.is_some_and(|pending| pending.event.kind == serving::Kind::Refusal)
            && held.is_some_and(|frame| matches!(frame.payload, Payload::ToolAnswer(_)))
        {
            break;
        }
        assert_ne!(
            owner.progress(),
            FoundationEnteredRollbackProgress::Closed,
            "cleanup cannot pass the blocked original GateRecord"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(io.released && io.waits >= 2);
    assert!(!trace_release.load(Ordering::SeqCst));
    let pending = owner.test_context().pending_author().unwrap();
    let ticket = pending.ticket;
    let Some(serving::Payload::Refusal(payload)) = &pending.event.payload else {
        panic!("actual typed Unloading record")
    };
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(payload.get()).unwrap(),
        serde_json::to_value(weaver_types::RefusalRecord::Lifecycle {
            asked: weaver_types::LifecycleAsk::Leave,
            refusal: weaver_types::LifecycleRefusal::Unloading,
        })
        .unwrap()
    );
    assert!(pending.event.turn.is_none());
    let expected = serde_json::to_vec(&frame(1, false)).unwrap();
    assert_eq!(
        owner.test_gate().unwrap().test_pending_frame(),
        Some(expected.as_slice())
    );
    assert!(owner.test_gate().unwrap().test_recorded().is_none());
    let execution: OrganEnvelope = serde_json::from_slice(
        &std::fs::read(root.join("serving-rollback-execution.json")).unwrap(),
    )
    .unwrap();
    let held = owner
        .test_account()
        .1
        .arms
        .port
        .as_ref()
        .unwrap()
        .held()
        .unwrap();
    assert_eq!(held.exchange, execution.exchange);
    assert_eq!(
        held.payload,
        Payload::ToolAnswer(weaver_types::ToolOutcome::Killed {
            partial: Some("original partial result".into()),
            by: weaver_types::KillCause::Unload,
        })
    );
    assert_eq!(
        owner.test_context().pending_author().unwrap().ticket,
        ticket
    );
    trace_release.store(true, Ordering::SeqCst);
    while owner.progress() != FoundationEnteredRollbackProgress::Closed {
        assert!(
            Instant::now() < end,
            "queued record cleanup must settle its original debt"
        );
        owner.advance();
        assert!(
            owner.test_context().failure().is_none(),
            "{:?}",
            owner.detail()
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(owner.test_gate().unwrap().test_recorded().is_some());
    assert!(owner.test_gate().unwrap().test_pending_frame().is_none());
}
struct ReleaseRejectedTrace<'a> {
    root: &'a Path,
    trace_release: &'a AtomicBool,
    end: Instant,
    released: bool,
}
impl crate::flow::clock::PumpIo for ReleaseRejectedTrace<'_> {
    fn now(&mut self) -> std::io::Result<u64> {
        MonotonicIo.now()
    }
    fn wait(
        &mut self,
        interests: &[crate::flow::clock::WaitInterest<'_>],
        deadline: Option<u64>,
    ) -> std::io::Result<crate::flow::clock::WaitOutcome> {
        loop {
            if Instant::now() >= self.end {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "rejected-return rollback fixture watchdog",
                ));
            }
            let now = MonotonicIo.now()?;
            if deadline.is_some_and(|deadline| now >= deadline) {
                return Ok(crate::flow::clock::WaitOutcome::Timeout);
            }
            if !self.released
                && self.root.join("serving-rollback-lower.json").exists()
                && self.root.join("serving-rollback-tool-answer.json").exists()
            {
                self.trace_release.store(true, Ordering::SeqCst);
                self.released = true;
            }
            let local = now.saturating_add(5_000_000);
            match MonotonicIo.wait(
                interests,
                Some(deadline.map_or(local, |deadline| deadline.min(local))),
            )? {
                ready @ crate::flow::clock::WaitOutcome::Ready(_) => return Ok(ready),
                // Probe the child marker without reporting a synthetic expiry
                // to the worker's real wait owner.
                crate::flow::clock::WaitOutcome::Timeout => (),
            }
        }
    }
}

fn drive_rejected_pressure(
    owner: &mut crate::flow::context::worker_startup::entered_rollback::FoundationEnteredRollback,
    root: &Path,
    trace_release: &AtomicBool,
    original: (u64, usize, AbsoluteBound),
) {
    let mut io = ReleaseRejectedTrace {
        root,
        trace_release,
        end: Instant::now() + Duration::from_secs(15),
        released: false,
    };
    assert!(!trace_release.load(Ordering::SeqCst));
    while owner.progress() != FoundationEnteredRollbackProgress::Closed {
        assert!(
            Instant::now() < io.end,
            "rejected return must settle after actual Lower"
        );
        if !io.released {
            let context = owner.test_context();
            assert_eq!(context.pending_author().unwrap().ticket, original.0);
            assert_eq!(
                context
                    .pairs
                    .pending_rejection()
                    .unwrap()
                    .raw_bytes()
                    .as_ptr() as usize,
                original.1
            );
            assert_eq!(
                context.deadlines.get(PumpDeadline::Decision),
                Some(original.2)
            );
        }
        owner.advance_with_io(&mut io);
        assert!(
            owner.test_context().failure().is_none(),
            "{:?}",
            owner.detail()
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(
        io.released,
        "pending trace must release only after actual Lower and ToolAnswer"
    );
}

impl crate::flow::clock::PumpIo for BrokenClock<'_> {
    fn now(&mut self) -> std::io::Result<u64> {
        if !self.broken {
            self.broken = true;
            Err(std::io::Error::other(
                "actual active serving rollback clock failure",
            ))
        } else {
            self.clock.now()
        }
    }
    fn wait(
        &mut self,
        _: &[crate::flow::clock::WaitInterest<'_>],
        _: Option<u64>,
    ) -> std::io::Result<crate::flow::clock::WaitOutcome> {
        panic!("first rollback clock failure cannot wait")
    }
}

fn finish_failed(
    run: Box<FoundationRun>,
    handoff: Handoff,
    member: &mut Member,
    reader: &mut UnixStream,
    root: &Path,
    mode: &str,
) {
    let Handoff {
        actual,
        peer,
        socket: _,
        origin,
        enter,
        cause,
        before,
        original,
        original_identity,
        engine_identity,
        input_bound,
        world_recorded: _,
        world_probe: _,
        pending_rejection: _,
        pending_turn_ticket: _,
    } = handoff;
    let leave = actual.leave;
    assert!(
        run.context.generation.is_some()
            || mode.ends_with("-decision-bound")
            || mode.ends_with("-placed-clock-failure"),
        "the original model operation is still active"
    );
    assert!(!original.is_empty());
    let recorded_before = run.context.last_author_sequence;
    let requests_before = member.requests.clone();
    let mut guard = RollbackGuard(match run.entered_rollback(actual) {
        Ok(owner) => owner,
        Err((mut run, mut actual)) => {
            crate::flow::context::worker_startup::tests::reap(actual.arms.child.as_mut().unwrap());
            run.resources.reap_startup_test_children();
            panic!("actual active serving rollback must join its original run");
        }
    });
    let owner = &mut guard.0;
    let mut clock = FakePumpIo::new(owner.test_context().last_ns);
    assert_eq!(
        owner.test_context().deadlines.get(PumpDeadline::Gate),
        input_bound
    );
    if mode.ends_with("-cap") {
        assert_eq!(
            owner.test_context().deadlines.get(PumpDeadline::Decision),
            None,
            "staged AppendAndGenerate retired its admission Decision; no wait is edited by the test"
        );
        assert_eq!(
            owner
                .test_context()
                .deadlines
                .expired(leave.deadline_ns())
                .min_by_key(|(_, bound)| bound.deadline_ns()),
            Some((PumpDeadline::Leave, leave)),
            "actual Leave must be the earliest expired bound"
        );
        clock.advance_to(leave.deadline_ns()).unwrap();
        assert_eq!(
            owner.advance_with_io(&mut clock),
            FoundationEnteredRollbackProgress::Closed
        );
        assert!(owner.abandoned());
        assert!(owner.test_context().failure().is_none());
        assert!(owner.test_context().flow.halt().is_none());
        assert!(
            owner.test_failed().is_none(),
            "healthy cap cannot mint failed-owner authority"
        );
        assert_eq!(owner.test_context().last_author_sequence, recorded_before);
        assert!(owner.unload_sequence().is_none());
        assert_eq!(member.requests, requests_before);
        assert!(!root.join("serving-rollback-cancel.json").exists());
        assert!(!root.join("serving-rollback-lower.json").exists());
    } else if mode.ends_with("-trace-failure") {
        reader.shutdown(std::net::Shutdown::Read).unwrap();
        let end = Instant::now() + Duration::from_secs(15);
        let mut first = None;
        let mut failed_deadline = None;
        while owner.progress() != FoundationEnteredRollbackProgress::Closed {
            assert!(
                Instant::now() < end,
                "actual trace failure cleanup watchdog: {:?}",
                owner.detail()
            );
            owner.advance();
            member.step_live(mode);
            if first.is_none()
                && let Some(failure) = owner.test_context().failure()
            {
                assert!(matches!(
                    failure,
                    RunFailure::Flow {
                        reason: FlowHaltReason::TraceFailed,
                        ..
                    }
                ));
                first = Some(failure.clone());
                let halt = owner
                    .test_context()
                    .flow
                    .halt()
                    .unwrap()
                    .deadline()
                    .unwrap();
                failed_deadline = Some(halt.min(leave.deadline_ns()));
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(
            first.is_some(),
            "closing the real trace reader must latch a real trace failure"
        );
        assert_eq!(owner.test_context().failure(), first.as_ref());
        assert_eq!(owner.test_failed().unwrap().deadline(), failed_deadline);
        assert!(
            !owner.test_context().place_owed(),
            "failed rollback settles its original decoder account with an unavailable trace"
        );
        assert!(owner.test_context().halt_report().unwrap().attempted());
        assert!(
            owner.abandoned(),
            "unwritable trace cannot certify terminal drain"
        );
    } else if mode.ends_with("-decision-bound") {
        let bound = owner
            .test_context()
            .deadlines
            .get(PumpDeadline::Decision)
            .unwrap();
        assert!(bound.deadline_ns() < leave.deadline_ns());
        clock.advance_to(bound.deadline_ns()).unwrap();
        owner.advance_with_io(&mut clock);
        let first = owner.test_context().failure().unwrap().clone();
        assert!(matches!(
            first,
            RunFailure::Flow {
                reason: FlowHaltReason::DecisionBound,
                ..
            }
        ));
        let deadline = owner.test_failed().unwrap().deadline().unwrap();
        let halt = owner
            .test_context()
            .flow
            .halt()
            .unwrap()
            .deadline()
            .unwrap();
        assert_eq!(deadline, halt.min(leave.deadline_ns()));
        assert!(owner.unload_sequence().is_none());
        clock.advance_to(deadline).unwrap();
        while owner.progress() != FoundationEnteredRollbackProgress::Closed {
            owner.advance_with_io(&mut clock);
        }
        assert_eq!(owner.test_context().failure(), Some(&first));
        assert!(owner.abandoned());
        assert!(owner.unload_sequence().is_none());
    } else {
        assert_eq!(
            owner.advance_with_io(&mut BrokenClock {
                clock: &mut clock,
                broken: false
            }),
            FoundationEnteredRollbackProgress::Progress
        );
        let first = owner.test_context().failure().unwrap().clone();
        assert!(matches!(
            first,
            RunFailure::Flow {
                reason: FlowHaltReason::TraceFailed,
                ..
            }
        ));
        let halt = owner
            .test_context()
            .flow
            .halt()
            .unwrap()
            .deadline()
            .unwrap();
        assert_eq!(
            owner.test_failed().unwrap().deadline(),
            Some(halt.min(leave.deadline_ns()))
        );
        assert!(
            owner
                .test_gate()
                .unwrap()
                .test_lower()
                .is_none_or(|(_, sent, _)| !sent)
        );
        let done = AtomicBool::new(false);
        let bytes = std::thread::scope(|scope| {
            let responder = scope.spawn(|| {
                while !done.load(Ordering::SeqCst) {
                    member.step_live(mode);
                    std::thread::sleep(Duration::from_millis(1));
                }
            });
            let recorder = scope.spawn(|| {
                let mut bytes = before.clone();
                while !done.load(Ordering::SeqCst) {
                    let _ = reader.read_to_end(&mut bytes);
                    std::thread::sleep(Duration::from_millis(1));
                }
                let _ = reader.read_to_end(&mut bytes);
                bytes
            });
            let _stop = StopThreads(&done);
            owner.advance();
            assert!(owner.test_context().halt_report().unwrap().attempted());
            assert!(
                owner
                    .test_gate()
                    .unwrap()
                    .test_lower()
                    .is_none_or(|(_, sent, _)| !sent),
                "fault attempt precedes Lower"
            );
            let end = Instant::now() + Duration::from_secs(15);
            while owner.progress() != FoundationEnteredRollbackProgress::Closed {
                assert!(
                    Instant::now() < end,
                    "active failure cleanup watchdog: {:?}",
                    owner.detail()
                );
                owner.advance();
                std::thread::sleep(Duration::from_millis(1));
            }
            done.store(true, Ordering::SeqCst);
            responder.join().unwrap();
            recorder.join().unwrap()
        });
        assert_eq!(owner.test_context().failure(), Some(&first));
        let last = owner.test_context().last_author_sequence.unwrap();
        let rows =
            trace_through(reader, bytes, last, Instant::now() + Duration::from_secs(3)).unwrap();
        assert_eq!(rows.iter().filter(|row| row["kind"] == "fault").count(), 1);
        if let Some(unload) = rows.iter().find(|row| row["kind"] == "unload") {
            assert_eq!(
                unload["payload"]["cause"],
                serde_json::to_value(cause).unwrap()
            );
            assert_eq!(rows.last().unwrap()["kind"], "unload");
        }
        assert_eq!(
            rows.iter()
                .filter(|row| row["kind"] == "turn.started")
                .count(),
            1
        );
    }
    assert!(Arc::ptr_eq(&owner.test_context().read_origin, &origin));
    assert_eq!(owner.test_context().retired_enter, Some(enter));
    assert_original(
        owner.test_input(),
        &original,
        original_identity,
        engine_identity,
    );
    assert_eq!(
        owner.test_context().deadlines.get(PumpDeadline::Leave),
        Some(leave)
    );
    assert!(!root.join("session-generation-1.json").exists());
    if mode.ends_with("-placed-clock-failure") {
        assert!(!root.join("session-generation-0.json").exists());
        assert!(
            owner
                .test_context()
                .place_engine
                .as_ref()
                .unwrap()
                .outstanding
                .is_none()
        );
        assert!(!owner.test_context().place_owed());
    }
    assert!(
        read_packet(&peer).is_none(),
        "cap/failure never claims Left"
    );
    let sequence = owner.unload_sequence();
    assert_eq!(
        owner.advance_with_io(&mut clock),
        FoundationEnteredRollbackProgress::Closed
    );
    assert_eq!(owner.unload_sequence(), sequence);
}

pub(in crate::flow::context::worker_startup) fn finish(
    run: Box<FoundationRun>,
    handoff: Handoff,
    member: &mut Member,
    reader: &mut UnixStream,
    root: &Path,
    mode: &str,
) {
    if mode.ends_with("-cap")
        || mode.ends_with("-clock-failure")
        || mode.ends_with("-trace-failure")
        || mode.ends_with("-decision-bound")
    {
        return finish_failed(run, handoff, member, reader, root, mode);
    }
    let Handoff {
        actual,
        peer,
        socket,
        origin,
        enter,
        cause,
        before,
        original,
        original_identity,
        engine_identity,
        input_bound,
        world_recorded,
        world_probe,
        pending_turn_ticket,
        pending_rejection,
    } = handoff;
    let leave = actual.leave;
    let outgoing = actual.gate_outgoing;
    assert!(Arc::ptr_eq(&run.context.read_origin, &origin));
    assert_eq!(run.context.retired_enter, Some(enter));
    assert_eq!(run.context.deadlines.get(PumpDeadline::Leave), Some(leave));
    if let Some(ticket) = pending_turn_ticket {
        let pending = run.context.pending_author().unwrap();
        assert_eq!(pending.ticket, ticket);
        assert_eq!(pending.event.kind, serving::Kind::TurnStarted);
        assert!(run.context.input_turn.is_some());
        assert!(run.context.open_turn.is_none());
    }
    if let Some((ticket, original, bound)) = pending_rejection {
        assert_eq!(run.context.pending_author().unwrap().ticket, ticket);
        assert_eq!(
            run.context
                .pairs
                .pending_rejection()
                .unwrap()
                .raw_bytes()
                .as_ptr() as usize,
            original
        );
        assert_eq!(
            run.context.deadlines.get(PumpDeadline::Decision),
            Some(bound)
        );
    }
    let requests = member.requests.clone();
    assert_eq!(run.context.deadlines.get(PumpDeadline::Gate), input_bound);
    std::fs::write(
        root.join("serving-rollback-accepted"),
        b"actual Leave accepted",
    )
    .unwrap();
    member.hold_kind = None;
    member.hold_mark = false;
    member.hold_control = None;
    let done = AtomicBool::new(false);
    let trace_release =
        AtomicBool::new(!mode.ends_with("-external-unload-queued") && pending_rejection.is_none());
    let (mut owner, bytes) = std::thread::scope(|scope| {
        let responder = scope.spawn(|| {
            while !done.load(Ordering::SeqCst) {
                member.step_live(mode);
                std::thread::sleep(Duration::from_millis(1));
            }
        });
        let recorder = scope.spawn(|| {
            let mut bytes = before;
            while !done.load(Ordering::SeqCst) {
                if trace_release.load(Ordering::SeqCst) {
                    let _ = reader.read_to_end(&mut bytes);
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            if trace_release.load(Ordering::SeqCst) {
                let _ = reader.read_to_end(&mut bytes);
            }
            bytes
        });
        let _stop = StopThreads(&done);
        let manual = mode.ends_with("-external-silent")
            || mode.ends_with("-world-blocked")
            || mode.ends_with("-external-interrupt-blocked")
            || mode.ends_with("-external-unload-queued")
            || pending_rejection.is_some();
        let result = if manual {
            run.entered_rollback(actual)
        } else {
            worker::entered_rollback(run, actual)
        };
        let mut owner = match result {
            Ok(owner) => owner,
            Err((mut run, mut handoff)) => {
                crate::flow::context::worker_startup::tests::reap(
                    handoff.arms.child.as_mut().unwrap(),
                );
                run.resources.reap_startup_test_children();
                panic!("the original run rejected its actual serving rollback handoff");
            }
        };
        if manual {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                if let Some(original) = pending_rejection {
                    drive_rejected_pressure(&mut owner, root, &trace_release, original);
                } else if mode.ends_with("-external-silent") {
                    drive_silent_reap(&mut owner, root);
                } else if mode.ends_with("-external-interrupt-blocked") {
                    drive_blocked_interrupt(&mut owner, root, &socket);
                } else if mode.ends_with("-external-unload-queued") {
                    drive_queued_pressure(&mut owner, root, &trace_release);
                } else {
                    drive_blocked_world(
                        &mut owner,
                        root,
                        world_recorded.unwrap(),
                        world_probe.unwrap(),
                    );
                }
            }));
            if let Err(panic) = result {
                owner.test_reap();
                std::panic::resume_unwind(panic);
            }
        }
        done.store(true, Ordering::SeqCst);
        responder.join().unwrap();
        (owner, recorder.join().unwrap())
    });
    assert_eq!(owner.progress(), FoundationEnteredRollbackProgress::Closed);
    assert!(!owner.abandoned(), "{:?}", owner.detail());
    assert!(owner.detail().is_none(), "{:?}", owner.detail());
    let (run, account) = owner.test_account();
    assert!(Arc::ptr_eq(&run.context.read_origin, &origin));
    assert!(Arc::ptr_eq(&account.origin, &origin));
    assert_eq!(account.enter, enter);
    assert_original(
        &account.input,
        &original,
        original_identity,
        engine_identity,
    );
    assert!(
        account.input.test_token_revoked(),
        "cancelled input never retains a usable token"
    );
    assert_eq!(account.leave, leave);
    assert_eq!(run.context.retired_enter, Some(enter));
    assert!(run.context.deadlines.get(PumpDeadline::Enter).is_none());
    assert!(run.context.failure().is_none());
    assert!(run.context.flow.halt().is_none());
    assert!(run.context.phase == crate::flow::context::RecordingPhase::Closed);
    assert_eq!(
        run.context.ever_started_turn,
        clean_turn(mode) || active_turn(mode)
    );
    for state in [
        Some(account.arms.child.as_ref().unwrap().state()),
        run.resources.classifier_child(),
        run.resources.spu_child(),
    ] {
        assert!(matches!(
            state,
            Some(ChildState::Reaped(nix::sys::wait::WaitStatus::Exited(_, 0)))
        ));
    }
    let gate = owner.test_gate().unwrap();
    let lower = gate.bound();
    assert_eq!(lower.began_ns(), leave.began_ns());
    assert_eq!(
        lower.deadline_ns(),
        (leave.began_ns() + weaver_types::LOWER_BOUND_MS * 1_000_000).min(leave.deadline_ns())
    );
    let (request, sent, sent_at) = gate.test_lower().unwrap();
    assert!(sent);
    assert!(sent_at.is_some_and(|at| at >= leave.began_ns() && !lower.expired(at)));
    assert_eq!(request.exchange.ordinal, outgoing + 1);
    assert_eq!(
        serde_json::from_slice::<OrganEnvelope>(
            &std::fs::read(root.join("serving-rollback-lower.json")).unwrap()
        )
        .unwrap(),
        *request
    );
    assert_eq!(
        gate.test_replies().1.unwrap().envelope().payload,
        Payload::Answer(LifecycleAnswer::GateStopped)
    );
    assert!(member.requests.starts_with(&requests));
    assert_eq!(member.requests.last().map(String::as_str), Some("grants"));
    member.assert_closed_without_request();
    let answer: OrganEnvelope = serde_json::from_slice(
        &read_packet(&peer).expect("actual serving rollback must return Left"),
    )
    .unwrap();
    assert_eq!(answer.exchange, account.message.envelope.exchange);
    assert_eq!(answer.position, Position::Close);
    assert_eq!(
        answer.payload,
        Payload::Answer(LifecycleAnswer::Left {
            save_point: None,
            forced: true,
            no_state: false
        })
    );
    let recorded = weaver_trace::Sequence(owner.unload_sequence().expect("actual Unload receipt"));
    let rows = trace_through(
        reader,
        bytes,
        recorded,
        Instant::now() + Duration::from_secs(3),
    )
    .unwrap();
    let completed = usize::from(clean_turn(mode) || active_turn(mode));
    for kind in ["input.offered", "load", "unload"] {
        assert_eq!(
            rows.iter().filter(|row| row["kind"] == kind).count(),
            1,
            "{kind}"
        );
    }
    for kind in ["message.user", "turn.started", "turn.closed"] {
        assert_eq!(
            rows.iter().filter(|row| row["kind"] == kind).count(),
            completed,
            "{kind}"
        );
    }
    if pending_turn_ticket.is_some() {
        assert!(run.context.input_turn.is_none());
        assert!(!rows.iter().any(|row| row["kind"] == "state.mark"));
        assert!(!root.join("session-generation-0.json").exists());
        let offered = rows
            .iter()
            .find(|row| row["kind"] == "input.offered")
            .unwrap();
        let source = rows
            .iter()
            .find(|row| row["kind"] == "message.user")
            .unwrap();
        assert_eq!(source["causal_parent"], offered["sequence"]);
        let filings: Vec<_> = member
            .filings
            .iter()
            .filter_map(|frame| match frame {
                s::StateFrame::File { filing, .. } if filing.source.kind == "message.user" => {
                    Some(filing)
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            filings.len(),
            1,
            "the original MessageUser must file exactly once"
        );
        assert_eq!(
            serde_json::to_value(filings[0].source.sequence).unwrap(),
            source["sequence"]
        );
        let source_id = serde_json::to_value(&filings[0].block_id).unwrap();
        let filed: Vec<_> = rows
            .iter()
            .enumerate()
            .filter(|(_, row)| {
                row["kind"] == "state.flow"
                    && row["payload"]["phase"] == "filed"
                    && row["payload"]["source_id"] == source_id
            })
            .map(|(index, _)| index)
            .collect();
        assert_eq!(
            filed.len(),
            1,
            "the exact original source needs its Recorded filed terminal"
        );
        let close = rows
            .iter()
            .position(|row| row["kind"] == "turn.closed")
            .unwrap();
        let unload = rows.iter().position(|row| row["kind"] == "unload").unwrap();
        assert!(
            filed[0] < close && close < unload,
            "source custody settles before closing the turn and load"
        );
    }
    if mode.ends_with("-queued") {
        assert!(root.join("serving-rollback-queued").exists());
        let refusals: Vec<_> = rows.iter().filter(|row| row["kind"] == "refusal").collect();
        assert_eq!(refusals.len(), 1);
        assert_eq!(
            refusals[0]["payload"],
            serde_json::to_value(weaver_types::RefusalRecord::Lifecycle {
                asked: weaver_types::LifecycleAsk::Leave,
                refusal: weaver_types::LifecycleRefusal::Unloading,
            })
            .unwrap()
        );
        assert!(
            refusals[0]
                .get("turn")
                .is_none_or(serde_json::Value::is_null)
        );
    }
    if active_turn(mode) {
        let close = rows
            .iter()
            .find(|row| row["kind"] == "turn.closed")
            .unwrap();
        assert_eq!(close["turn"], "session-0");
        assert_eq!(
            close["payload"],
            serde_json::json!({"close":"stopped", "reason":"unload", "cause":{"uid":74}})
        );
    } else if completed == 1 {
        let close = rows
            .iter()
            .find(|row| row["kind"] == "turn.closed")
            .unwrap();
        assert_eq!(
            close["payload"]["close"], "clean",
            "the original clean close is never replaced"
        );
        if mode.ends_with("-world-blocked") {
            assert_eq!(
                close["sequence"],
                serde_json::to_value(world_recorded.unwrap()).unwrap()
            );
            assert_eq!(
                account.input.test_world(),
                Some((world_recorded, false, true))
            );
            assert!(!root.join("serving-rollback-world.json").exists());
        }
    }
    if mode.contains("-external-") {
        if pending_rejection.is_some() {
            let rejected: Vec<_> = rows
                .iter()
                .filter(|row| row["kind"] == "tool.return_rejected")
                .collect();
            assert_eq!(rejected.len(), 1);
            assert_eq!(rejected[0]["payload"]["reason"], "unexpected_exchange");
            assert_eq!(
                rejected[0]["payload"]["raw_envelope"].as_str().unwrap(),
                std::fs::read_to_string(root.join("serving-rollback-rejection.json")).unwrap()
            );
            assert!(!run.context.pair_rejected_authoring);
            assert!(run.context.pairs.pending_rejection().is_none());
        }
        let ends: Vec<_> = member
            .filings
            .iter()
            .filter_map(|frame| {
                let s::StateFrame::FilePair { filing, .. } = frame else {
                    return None;
                };
                let s::PairRecord::Ended(end) = &filing.pair else {
                    return None;
                };
                Some(end)
            })
            .collect();
        assert_eq!(
            ends.len(),
            2,
            "both actual pair terminal filings settle before Unload"
        );
        let expected = if mode.ends_with("-external-crossed") {
            weaver_types::ToolOutcome::Result {
                content: "original crossed result".into(),
            }
        } else if mode.ends_with("-external-silent") {
            assert!(!root.join("serving-rollback-tool-answer.json").exists());
            weaver_types::ToolOutcome::Killed {
                partial: None,
                by: weaver_types::KillCause::Fault,
            }
        } else {
            weaver_types::ToolOutcome::Killed {
                partial: Some("original partial result".into()),
                by: weaver_types::KillCause::Unload,
            }
        };
        let executed: OrganEnvelope = serde_json::from_slice(
            &std::fs::read(root.join("serving-rollback-execution.json")).unwrap(),
        )
        .unwrap();
        let sent = ends
            .iter()
            .find(|end| end.execution.as_ref() == Some(&executed.exchange))
            .unwrap();
        let unstarted = ends.iter().find(|end| end.execution.is_none()).unwrap();
        assert_eq!(sent.outcome, Some(expected));
        if mode.ends_with("-external-silent") {
            assert_eq!(sent.reason, Some(s::PairFailure::GateFault {}));
        }
        assert_eq!(
            unstarted.reason,
            Some(s::PairFailure::NotDispatched {
                cause: s::NonDispatchCause::Unload
            })
        );
        assert!(unstarted.outcome.is_none());
        assert!(
            !rows.iter().any(|row| row["kind"] == "message.tool_result"),
            "rollback never elects result placement or another generation"
        );
    }
    assert!(
        rows.iter()
            .all(|row| !matches!(row["kind"].as_str(), Some("save_point" | "fault")))
    );
    let load = rows.iter().find(|row| row["kind"] == "load").unwrap();
    let unload = rows.last().unwrap();
    assert_eq!(unload["kind"], "unload");
    assert_eq!(unload["sequence"], serde_json::to_value(recorded).unwrap());
    assert_eq!(
        unload["payload"]["cause"],
        serde_json::to_value(cause).unwrap()
    );
    assert_eq!(unload["payload"]["cause"], load["payload"]["cause"]);
    assert_eq!(unload["payload"]["forced"], true);
    assert_eq!(unload["payload"]["release"]["spu"], "confirmed");
    assert_eq!(unload["payload"]["release"]["member"], "closed");
    assert!(!root.join("session-generation-1.json").exists());
    assert_eq!(
        root.join("session-generation-0.json").exists(),
        clean_turn(mode)
            || holds_generation(mode)
            || is_correction(mode)
            || mode.contains("-external-")
    );
    if is_correction(mode) {
        assert_eq!(
            rows.iter()
                .filter(|row| row["kind"] == "loop.instruction")
                .count(),
            1
        );
        assert_eq!(
            root.join("serving-rollback-correction-generation.json")
                .exists(),
            mode.ends_with("-generation"),
            "a cancelled correction measurement cannot append its instruction",
        );
        assert_eq!(
            root.join("serving-rollback-cancel.json").exists(),
            mode.ends_with("-generation")
        );
        assert!(run.context.measure.is_none());
        assert!(run.context.generation.is_none());
    }
    assert_eq!(owner.advance(), FoundationEnteredRollbackProgress::Closed);
    assert!(read_packet(&peer).is_none(), "Left is sent only once");
}
