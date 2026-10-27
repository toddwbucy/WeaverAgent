//! Actual Quiesce, joining root connections, Lower and normal SQLite capture.
//! A blocked original refusal author resumes after force, with its ticket intact.
use super::*;
use crate::flow::clock::{PumpIo, WaitInterest, WaitOutcome};

pub(in crate::flow::context::worker_startup) fn is_mode(mode: &str) -> bool {
    mode.contains("-session-graceful-save-sqlite-quiesce-")
}
fn pending(mode: &str) -> bool {
    mode.ends_with("pending-record")
        || mode.ends_with("pending-author")
        || mode.ends_with("accepted")
        || mode.ends_with("queued-close")
}
fn accepted(mode: &str) -> bool {
    mode.ends_with("accepted") || mode.ends_with("delivered") || mode.ends_with("queued-close")
}

pub(super) struct Evidence {
    ticket: Option<u64>,
    original: Option<serde_json::Value>,
    pub(super) probe: Option<Box<dyn Fn() -> (bool, bool) + Send>>,
}
impl Evidence {
    pub(super) fn assert_run(&self, run: &FoundationRun) {
        if let Some(ticket) = self.ticket {
            assert_eq!(run.context.pending_author().unwrap().ticket, ticket);
            assert_eq!(
                Some(serde_json::to_value(run.context.barrier.pending().unwrap()).unwrap()),
                self.original
            );
        }
        assert!(run.context.failure().is_none());
        assert!(run.context.open_turn.is_none());
        assert!(
            run.context
                .deadlines
                .get(PumpDeadline::GracefulDrain)
                .is_none()
        );
    }
}

pub(in crate::flow::context::worker_startup) fn gate(mode: &str, root: &Path, fd: i32) {
    if accepted(mode) {
        packet!(fd, &frame(0, false));
    }
    let mut bytes = vec![0; weaver_types::MAX_ENVELOPE_BYTES];
    if mode.ends_with("queued-control") {
        wait_file(root, "quiesce-convert-drain");
        let lower = read_after_filler(fd, &mut bytes);
        assert_eq!(lower.payload, Payload::Directive(LifecycleDirective::Lower));
        assert_eq!(lower.exchange.ordinal, 3);
        std::fs::write(
            root.join("ready-rollback-lower.json"),
            serde_json::to_vec(&lower).unwrap(),
        )
        .unwrap();
        packet!(
            fd,
            &OrganEnvelope {
                exchange: lower.exchange,
                position: Position::Close,
                payload: Payload::Answer(LifecycleAnswer::GateStopped)
            }
        );
        return;
    }
    let n = recv(fd, &mut bytes, MsgFlags::empty()).unwrap();
    let quiesce: OrganEnvelope = serde_json::from_slice(&bytes[..n]).unwrap();
    assert_eq!(
        quiesce.payload,
        Payload::Directive(LifecycleDirective::Quiesce)
    );
    assert_eq!(quiesce.exchange.ordinal, 2);
    std::fs::write(root.join("quiesce-convert-request.json"), &bytes[..n]).unwrap();
    if mode.contains("-secondary-") && mode.ends_with("gate-end") {
        wait_file(root, "secondary-leave-read-armed");
        return;
    }
    if mode.ends_with("queued-close") {
        wait_file(root, "quiesce-convert-drain");
    }
    if pending(mode) && !accepted(mode) {
        for index in 0..2 {
            packet!(fd, &frame(index, false));
        }
    }
    if mode.ends_with("prelower") {
        wait_file(root, "quiesce-convert-handoff");
        packet!(
            fd,
            &OrganEnvelope {
                exchange: quiesce.exchange.clone(),
                position: Position::Close,
                payload: Payload::Answer(LifecycleAnswer::GateQuiesced)
            }
        );
        std::fs::write(
            root.join("quiesce-convert-ack-sent"),
            b"actual acknowledgement",
        )
        .unwrap();
    }
    loop {
        let n = recv(fd, &mut bytes, MsgFlags::empty()).unwrap_or(0);
        if n == 0 {
            return;
        }
        if mode.ends_with("queued-close") && bytes[..n].iter().all(|byte| *byte == b'x') {
            continue;
        }
        let request: OrganEnvelope = serde_json::from_slice(&bytes[..n]).unwrap();
        if let Payload::Frame(close) = &request.payload {
            assert!(accepted(mode));
            assert_eq!(request.exchange, frame(0, false).exchange);
            assert_eq!(request.position, Position::Close);
            let close: serde_json::Value =
                serde_json::from_slice(&close.octets().unwrap()).unwrap();
            assert_eq!(close["kind"], "refused");
            assert_eq!(close["reason"], "the agent is unloading");
            assert!(
                !root.join("quiesce-convert-world.json").exists(),
                "world close duplicated"
            );
            std::fs::write(root.join("quiesce-convert-world.json"), &bytes[..n]).unwrap();
            continue;
        }
        assert_eq!(
            request.payload,
            Payload::Directive(LifecycleDirective::Lower)
        );
        assert_eq!(request.exchange.ordinal, 3);
        assert_eq!(request.position, Position::Open);
        std::fs::write(root.join("ready-rollback-lower.json"), &bytes[..n]).unwrap();
        if mode.ends_with("wrong-ack") {
            packet!(
                fd,
                &OrganEnvelope {
                    exchange: weaver_types::ExchangeId {
                        opener: Opener::Harness,
                        ordinal: 94
                    },
                    position: Position::Close,
                    payload: Payload::Answer(LifecycleAnswer::GateQuiesced)
                }
            );
            return;
        }
        if mode.ends_with("duplicate-ack") {
            for _ in 0..2 {
                packet!(
                    fd,
                    &OrganEnvelope {
                        exchange: quiesce.exchange.clone(),
                        position: Position::Close,
                        payload: Payload::Answer(LifecycleAnswer::GateQuiesced)
                    }
                );
            }
            return;
        }
        if !mode.ends_with("prelower") && !mode.ends_with("expiry") {
            packet!(
                fd,
                &OrganEnvelope {
                    exchange: quiesce.exchange,
                    position: Position::Close,
                    payload: Payload::Answer(LifecycleAnswer::GateQuiesced)
                }
            );
        }
        packet!(
            fd,
            &OrganEnvelope {
                exchange: request.exchange,
                position: Position::Close,
                payload: Payload::Answer(LifecycleAnswer::GateStopped)
            }
        );
        return;
    }
}

fn read_after_filler(fd: i32, bytes: &mut [u8]) -> OrganEnvelope {
    loop {
        let n = recv(fd, bytes, MsgFlags::empty()).unwrap();
        assert!(n > 0);
        if !bytes[..n].iter().all(|byte| *byte == b'x') {
            return serde_json::from_slice(&bytes[..n]).unwrap();
        }
    }
}

fn fill_gate(fd: i32) {
    crate::flow::context::worker_startup::input::management::stop::tests::fill(fd);
    loop {
        match send(fd, b"x", MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_NOSIGNAL) {
            Ok(1) => (),
            Err(nix::errno::Errno::EAGAIN) => return,
            other => panic!("fill final gate slots: {other:?}"),
        }
    }
}

struct JoinDuringWait<'a> {
    socket: &'a Path,
    peer: Option<OwnedFd>,
}
impl PumpIo for JoinDuringWait<'_> {
    fn now(&mut self) -> std::io::Result<u64> {
        MonotonicIo.now()
    }
    fn wait(
        &mut self,
        interests: &[WaitInterest<'_>],
        deadline: Option<u64>,
    ) -> std::io::Result<WaitOutcome> {
        if self.peer.is_none() && deadline.is_none_or(|end| end > self.now().unwrap()) {
            self.peer = Some(joining(self.socket, 75, 100));
        }
        MonotonicIo.wait(interests, deadline)
    }
}

pub(super) fn handoff(
    serving: FoundationServing<'_>,
    socket: &Path,
    root: &Path,
    mode: &str,
    member: &mut Member,
    reader: &mut UnixStream,
) -> Handoff {
    let expiry = mode.ends_with("expiry");
    let secondary = mode.contains("-secondary-");
    let pending = pending(mode);
    let author = mode.ends_with("pending-author");
    let queued = mode.ends_with("queued-control") || mode.ends_with("queued-close");
    let mut session = SessionGuard(Some(Session::begin(serving, &FAMILY, CutMethod::Flush)));
    let done = AtomicBool::new(false);
    let pause = AtomicBool::new(false);
    let paused = AtomicBool::new(false);
    reader.set_nonblocking(true).unwrap();
    let (actual, primary, before, joins, evidence) = std::thread::scope(|scope| {
        let responder = scope.spawn(|| {
            let mut joined = None;
            while !done.load(Ordering::SeqCst) {
                member.step_live(mode);
                let trigger = if mode.ends_with("delivered") {
                    "quiesce-convert-world.json"
                } else {
                    "quiesce-convert-request.json"
                };
                if !pending
                    && !expiry
                    && !secondary
                    && !queued
                    && joined.is_none()
                    && root.join(trigger).exists()
                {
                    joined = Some(joining(socket, 75, 100));
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            joined
        });
        let recorder = scope.spawn(|| {
            let mut bytes = Vec::new();
            while !done.load(Ordering::SeqCst) {
                if pause.load(Ordering::SeqCst) {
                    paused.store(true, Ordering::SeqCst);
                } else {
                    let _ = reader.read_to_end(&mut bytes);
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            let _ = reader.read_to_end(&mut bytes);
            bytes
        });
        let _stop = StopThreads(&done);
        let worker = session.0.as_mut().unwrap();
        if accepted(mode) {
            drive(worker, Progress::Accepted);
        }
        let primary = management(
            socket,
            LifecycleDirective::Leave {
                cause: weaver_types::Cause { uid: 74 },
                forced: false,
                rollback: false,
            },
        );
        drive(worker, Progress::DirectivePending);
        assert!(worker.defer_graceful_leave());
        assert!(worker.begin_graceful_quiesce());
        let mut guard = Guard {
            input: Some(session.0.take().unwrap().into_owner().ok().unwrap()),
            management: None,
        };
        let mut first = None;
        let mut evidence = Evidence {
            ticket: None,
            original: None,
            probe: None,
        };
        if secondary {
            let end = Instant::now() + Duration::from_secs(4);
            while !root.join("quiesce-convert-request.json").exists() {
                assert!(Instant::now() < end, "actual Quiesce was not sent");
                assert_ne!(
                    guard.input.as_mut().unwrap().advance(),
                    FoundationInputProgress::Stopped
                );
                std::thread::sleep(Duration::from_millis(1));
            }
            first = secondary::convert(
                &mut guard,
                socket,
                root,
                mode,
                expiry.then_some(PumpDeadline::GracefulDrain),
            );
        }
        if pending {
            let end = Instant::now() + Duration::from_secs(4);
            while !guard
                .input
                .as_ref()
                .unwrap()
                .serving
                .graceful
                .as_ref()
                .unwrap()
                .quiesce
                .as_ref()
                .unwrap()
                .has_record()
            {
                assert!(
                    Instant::now() < end,
                    "the original Quiesce never retained a frame"
                );
                let input = guard.input.as_mut().unwrap();
                assert_ne!(
                    input.advance(),
                    FoundationInputProgress::Stopped,
                    "{:?}",
                    input.detail()
                );
            }
            if author {
                pause.store(true, Ordering::SeqCst);
                while !paused.load(Ordering::SeqCst) {
                    assert!(Instant::now() < end);
                    std::thread::sleep(Duration::from_millis(1));
                }
                let input = guard.input.as_mut().unwrap();
                let context = &mut input.serving.owner.context;
                let mut clock = FakePumpIo::new(context.last_ns);
                assert!(
                    crate::flow::context::worker_startup::input::management::stop::tests::fill_trace(
                        context, &mut clock
                    ) > 0
                );
                let mut io = JoinDuringWait { socket, peer: None };
                assert_eq!(
                    input.advance_with_io(&mut io),
                    FoundationInputProgress::ManagementPending
                );
                let pending = input
                    .serving
                    .owner
                    .context
                    .pending_author()
                    .expect("same blocked refusal author");
                assert_eq!(pending.event.kind, serving::Kind::Refusal);
                evidence.ticket = Some(pending.ticket);
                evidence.original = Some(
                    serde_json::to_value(input.serving.owner.context.barrier.pending().unwrap())
                        .unwrap(),
                );
                first = io.peer;
            } else if !queued {
                first = Some(joining(socket, 75, 100));
            }
        }
        if queued {
            let input = guard.input.as_mut().unwrap();
            fill_gate(input.serving.owner.gate.fd().as_raw_fd());
            let mut io = JoinDuringWait { socket, peer: None };
            assert_eq!(
                input.advance_with_io(&mut io),
                FoundationInputProgress::ManagementPending
            );
            first = io.peer;
            let phase = input
                .serving
                .graceful
                .as_ref()
                .unwrap()
                .quiesce
                .as_ref()
                .unwrap();
            let send = if mode.ends_with("queued-control") {
                phase.control().unwrap().1
            } else {
                phase.test_record_send().unwrap()
            };
            assert!(!send.sent() && !send.withdrawn() && send.sent_at().is_none());
            evidence.probe = Some(send.state_probe_for_test());
        }
        let end = Instant::now() + Duration::from_secs(4);
        if secondary {
            assert!(
                guard
                    .owned_input()
                    .serving
                    .graceful
                    .as_ref()
                    .unwrap()
                    .forced
            );
        } else if expiry {
            while !guard
                .input
                .as_ref()
                .unwrap()
                .serving
                .graceful
                .as_ref()
                .unwrap()
                .forced
            {
                assert!(Instant::now() < end, "declared Quiesce bound was renewed");
                let input = guard.input.as_mut().unwrap();
                assert_ne!(
                    input.advance(),
                    FoundationInputProgress::Stopped,
                    "{:?}",
                    input.detail()
                );
            }
        } else {
            while guard.input.as_ref().unwrap().progress()
                != FoundationInputProgress::ManagementPending
            {
                assert!(Instant::now() < end, "JoinLeave did not interrupt Quiesce");
                let input = guard.input.as_mut().unwrap();
                let progress = if pending {
                    input.wait_for_wake()
                } else {
                    input.advance()
                };
                assert_ne!(
                    progress,
                    FoundationInputProgress::Stopped,
                    "{:?}",
                    input.detail()
                );
            }
            guard.management = Some(guard.input.take().unwrap().management().ok().unwrap());
            while guard.management.as_ref().unwrap().progress()
                == weaver_harness::FoundationManagementProgress::Pending
            {
                assert!(Instant::now() < end);
                guard.management.as_mut().unwrap().advance();
            }
            guard.input = Some(
                match guard.management.take().unwrap().join_graceful_leave() {
                    Ok(input) => input,
                    Err(owner) => {
                        guard.management = Some(owner);
                        panic!("actual Quiesce join refused");
                    }
                },
            );
        }
        let input = guard.input.as_ref().unwrap();
        let leave = input.serving.graceful.as_ref().unwrap();
        assert!(leave.forced);
        assert_eq!(
            leave.forced_by,
            (!expiry && !secondary).then_some(weaver_types::Cause { uid: 75 })
        );
        let quiesce = leave.quiesce.as_ref().unwrap();
        assert!(!quiesce.test_accounts().0, "no invented Quiesced grant");
        let (request, send) = quiesce.control().unwrap();
        assert_eq!(request.exchange.ordinal, 2);
        assert_eq!(send.sent(), !mode.ends_with("queued-control"));
        assert_eq!(send.sent_at().is_some(), !mode.ends_with("queued-control"));
        assert_eq!(quiesce.has_record(), pending);
        if mode.ends_with("delivered") {
            assert!(input.failed_world_was_sent());
        }
        assert!(!root.join("ready-rollback-lower.json").exists());
        std::fs::write(
            root.join("quiesce-convert-handoff"),
            b"original force retained",
        )
        .unwrap();
        if mode.ends_with("prelower") {
            wait_file(root, "quiesce-convert-ack-sent");
        }
        let actual = match guard.input.take().unwrap().graceful_leave_handoff() {
            Ok(actual) => actual,
            Err(input) => {
                let detail = input.detail();
                guard.input = Some(input);
                panic!("converted Quiesce handoff refused: {detail:?}");
            }
        };
        assert_eq!(
            actual.inner.message.envelope.payload,
            Payload::Directive(LifecycleDirective::Leave {
                cause: weaver_types::Cause { uid: 74 },
                forced: false,
                rollback: false,
            })
        );
        pause.store(false, Ordering::SeqCst);
        done.store(true, Ordering::SeqCst);
        let joined = responder.join().unwrap();
        let joins = first
            .or(joined)
            .into_iter()
            .map(|peer| {
                assert!(read_packet(&peer).is_none());
                JoinReply {
                    peer,
                    ordinal: 100,
                    answer: None,
                }
            })
            .collect();
        (actual, primary, recorder.join().unwrap(), joins, evidence)
    });
    assert!(read_packet(&primary).is_none());
    Handoff {
        actual: LeaveHandoff::Graceful(actual),
        peer: Some(primary),
        socket: socket.into(),
        before,
        joins,
        conversion: None,
        quiesce: Some(evidence),
        wind_down: None,
    }
}

pub(super) fn assert_tail(
    owner: &mut ForcedGuard,
    rows: &[serde_json::Value],
    joins: &[JoinReply],
    filings: &[s::StateFrame],
    root: &Path,
    mode: &str,
) {
    let expiry = mode.ends_with("expiry");
    let secondary = mode.contains("-secondary-");
    let gate_end = secondary && mode.ends_with("gate-end");
    let expected = if accepted(mode) {
        1
    } else if pending(mode) {
        2
    } else {
        0
    };
    let unload = rows.iter().find(|row| row["kind"] == "unload").unwrap();
    assert_eq!(
        unload["payload"]["forced_by"],
        if expiry || secondary {
            serde_json::Value::Null
        } else {
            serde_json::json!({"uid":75})
        }
    );
    assert_eq!(
        joins.len(),
        if secondary {
            usize::from(mode.contains("-secondary-joiner-"))
        } else {
            usize::from(!expiry)
        }
    );
    assert!(joins.iter().all(|join| join.answer.is_some()));
    assert!(filings.is_empty());
    assert!(!rows.iter().any(|row| matches!(
        row["kind"].as_str(),
        Some("turn.started" | "turn.closed" | "loop.instruction" | "message.user")
    )));
    let refusals: Vec<_> = rows
        .iter()
        .filter(|row| {
            row["kind"] == "refusal"
                && matches!(
                    serde_json::from_value::<weaver_types::RefusalRecord>(row["payload"].clone()),
                    Ok(weaver_types::RefusalRecord::Lifecycle {
                        asked: weaver_types::LifecycleAsk::Leave,
                        refusal: weaver_types::LifecycleRefusal::Unloading
                    })
                )
        })
        .collect();
    assert_eq!(
        refusals.len(),
        expected,
        "every admitted frame owes exactly one refusal"
    );
    let sequence =
        |row: &serde_json::Value| row["sequence"].as_str().unwrap().parse::<u64>().unwrap();
    let confirmed = rows
        .iter()
        .find(|row| row["kind"] == "save_point" && row["payload"]["phase"] == "confirmed")
        .unwrap();
    assert!(
        refusals
            .iter()
            .all(|row| sequence(row) < sequence(confirmed))
    );
    let inner = owner.0.test_inner();
    let (_, handoff) = inner.test_account();
    let phase = handoff
        .graceful_tail
        .as_ref()
        .unwrap()
        .quiesce
        .as_ref()
        .unwrap();
    let (complete, recorded) = phase.test_accounts();
    assert!(!complete && !phase.has_record());
    assert_eq!(
        recorded.len(),
        usize::from(pending(mode) || mode.ends_with("delivered"))
    );
    assert!(
        recorded
            .iter()
            .all(|seq| refusals.iter().any(|row| sequence(row) == seq.0))
    );
    assert!(inner.test_context().input.snapshot().is_none());
    let gate = inner.test_gate().unwrap();
    if gate_end {
        assert!(gate.test_quiesce_reply().is_none());
        assert!(gate.test_replies().1.is_none());
        assert!(!root.join("ready-rollback-lower.json").exists());
        return;
    }
    let bad_ack = mode.ends_with("wrong-ack") || mode.ends_with("duplicate-ack");
    assert_eq!(
        gate.refusal(),
        bad_ack.then_some(&weaver_types::LifecycleRefusal::Malformed)
    );
    assert_eq!(
        gate.test_quiesce_reply().is_some(),
        !expiry && !mode.ends_with("wrong-ack") && !mode.ends_with("queued-control")
    );
    if let Some(reply) = gate.test_quiesce_reply() {
        assert_eq!(reply.envelope().exchange.ordinal, 2);
    }
    let last = gate.test_replies().1.unwrap().envelope();
    if bad_ack {
        assert_eq!(
            last.exchange.ordinal,
            if mode.ends_with("wrong-ack") { 94 } else { 2 }
        );
        assert_eq!(last.payload, Payload::Answer(LifecycleAnswer::GateQuiesced));
    } else {
        assert_eq!(last.exchange.ordinal, 3);
        assert_eq!(last.payload, Payload::Answer(LifecycleAnswer::GateStopped));
    }
    if mode.ends_with("queued-control") || mode.ends_with("queued-close") {
        assert!(root.join("quiesce-convert-drain").exists());
        assert!(!root.join("quiesce-convert-world.json").exists());
    }
    if mode.ends_with("delivered") {
        assert!(root.join("quiesce-convert-world.json").exists());
    }
}
