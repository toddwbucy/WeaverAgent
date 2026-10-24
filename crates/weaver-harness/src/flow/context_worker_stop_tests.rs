use super::super::tests::{Guard, dial, read_packet, trace};
use super::*;
use crate::flow::clock::testing::FakePumpIo;
use crate::flow::context::startup_composition::tests::CircuitIo;
use crate::flow::context::worker_startup::tests::{Member, worker};
use crate::flow::pump::PumpDeadline;
use nix::sys::socket::{MsgFlags, Shutdown, send, shutdown};
use std::io::Read;
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};
use weaver_types::ExchangeId;

pub(in crate::flow::context::worker_startup) fn fill_trace(
    context: &mut crate::flow::context::RecordingContext,
    clock: &mut FakePumpIo,
) -> usize {
    use crate::flow::recording::{RecordProgress, RecordPurpose};
    let payload =
        serde_json::value::to_raw_value(&serde_json::json!({"fixture":"x".repeat(32_768)}))
            .unwrap();
    for count in 0..serving::ORDINARY_CAPACITY + 64 {
        if context.barrier.recorder().boundary().queued == serving::ORDINARY_CAPACITY {
            return count;
        }
        let now = clock.now().unwrap();
        assert!(matches!(
            context.barrier.submit_authored_with_clock(
                AuthoredEvent {
                    kind: serving::Kind::ModelMeasurement,
                    subsystem: Subsystem::Harness,
                    turn: Some(TurnRef("stopped-turn".into())),
                    source: AuthoredSource::Runtime,
                    causal_parent: None,
                    payload: Some(serving::Payload::ModelMeasurement(payload.clone()))
                },
                RecordPurpose::Observation,
                &context.author,
                1,
                &mut context.inbox,
                &mut context.budget,
                &mut || Ok(now),
            ),
            RecordProgress::Recorded { .. }
        ));
    }
    panic!("ordinary queue did not fill")
}

struct TraceWait<'a> {
    clock: &'a mut FakePumpIo,
    reader: &'a mut UnixStream,
    peer: &'a std::os::fd::OwnedFd,
    root: &'a Path,
    count: usize,
    eof: bool,
    seen: bool,
}
impl PumpIo for TraceWait<'_> {
    fn now(&mut self) -> std::io::Result<u64> {
        self.clock.now()
    }
    fn wait(
        &mut self,
        interests: &[crate::flow::clock::WaitInterest<'_>],
        deadline: Option<u64>,
    ) -> std::io::Result<crate::flow::clock::WaitOutcome> {
        if !self.seen {
            assert!(
                read_packet(self.peer).is_none(),
                "Stop cannot answer before Recorded close"
            );
            assert!(
                !self.root.join("closed.json").exists(),
                "gate cannot close before Recorded close"
            );
            if self.eof {
                shutdown(self.peer.as_raw_fd(), Shutdown::Both).unwrap();
            }
            let mut bytes = [0; 65_536];
            let mut records = 0;
            let mut last = None;
            let end = Instant::now() + Duration::from_secs(3);
            while records < self.count {
                assert!(Instant::now() < end, "trace drain stalled");
                match self.reader.read(&mut bytes) {
                    Ok(0) => panic!("trace closed while draining"),
                    Ok(n) => {
                        records += bytes[..n].iter().filter(|b| **b == b'\n').count();
                        last = Some(bytes[n - 1]);
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => (),
                    Err(e) => panic!("trace drain: {e}"),
                }
                std::thread::yield_now();
            }
            assert_eq!(last, Some(b'\n'));
            self.seen = true;
        }
        CircuitIo(self.clock).wait(interests, deadline)
    }
}

pub(in crate::flow::context::worker_startup) fn fill(fd: i32) {
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

fn input_step(
    input: &mut FoundationInput<'_>,
    clock: &mut FakePumpIo,
    real: bool,
) -> FoundationInputProgress {
    if real {
        input.advance()
    } else {
        input.advance_with_io(&mut CircuitIo(clock))
    }
}

fn prepare(
    input: &mut FoundationInput<'_>,
    member: &mut Member,
    clock: &mut FakePumpIo,
    real: bool,
    target: Stage,
) {
    for _ in 0..5000 {
        if input.stage == target {
            return;
        }
        if input.stage == Stage::Accepted {
            input.start_turn(TurnKey("stopped-turn".into())).unwrap();
            if target == Stage::Source {
                return;
            }
        }
        assert_ne!(
            input_step(input, clock, real),
            FoundationInputProgress::Stopped,
            "{:?}",
            input.detail()
        );
        if real {
            member.step_live("ready");
        } else {
            member.step(clock.now().unwrap(), "ready");
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("input never reached target");
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
    let real = mode.starts_with("input-worker");
    let before_turn = ["waiting", "checking", "accepted", "source"]
        .iter()
        .any(|s| mode.ends_with(s));
    let target = if mode.ends_with("waiting") {
        Stage::Waiting
    } else if mode.ends_with("checking") {
        Stage::Checking
    } else if mode.ends_with("accepted") {
        Stage::Accepted
    } else if mode.ends_with("source") {
        Stage::Source
    } else {
        Stage::Turn
    };
    let mut guard = Guard {
        input: Some(serving.input()),
        management: None,
    };
    prepare(guard.input.as_mut().unwrap(), member, clock, real, target);
    if mode.ends_with("transfer") || real {
        let input = guard.input.take().unwrap();
        let source = input.seated.as_ref().unwrap().source.clone();
        let FoundationInputResult::Turn(turn) = input.finish().ok().unwrap() else {
            panic!("turn owner")
        };
        assert_eq!(turn.source(), &source);
        guard.input = Some(turn.into_service());
        assert_eq!(
            guard
                .input
                .as_ref()
                .unwrap()
                .seated
                .as_ref()
                .unwrap()
                .source,
            source
        );
    }
    let input = guard.input.as_ref().unwrap();
    let snapshot = input.serving.owner.context.input.snapshot();
    let deadline = input
        .serving
        .owner
        .context
        .deadlines
        .get(PumpDeadline::Gate);
    let run = input.serving.owner.context.flow.run().to_owned();
    let original = input.received.as_ref().map(|r| r.envelope().clone());
    let mut records = trace(reader);

    // Hold the actual whole-file OFD lock so the separate admin test process
    // takes the production pidfd/UID/lock authentication path.
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(root.join("run.lock"))
        .unwrap();
    let request = nix::libc::flock {
        l_type: nix::libc::F_WRLCK as i16,
        l_whence: nix::libc::SEEK_SET as i16,
        l_start: 0,
        l_len: 0,
        l_pid: 0,
    };
    assert_eq!(
        unsafe { nix::libc::fcntl(lock.as_raw_fd(), nix::libc::F_OFD_SETLK, &request) },
        0
    );
    let mut admin = if real {
        Some(
            std::process::Command::new(
                std::env::var("WEAVER_STOP_ADMIN_TEST_BIN").expect("current admin artifact"),
            )
            .args([
                "--exact",
                "stop_exchange::tests::stop_exchange_actual_admin_process",
                "--nocapture",
            ])
            .env("WEAVER_STOP_CIRCUIT", root)
            .spawn()
            .unwrap(),
        )
    } else {
        None
    };
    let peer = (!real).then(|| dial(socket));
    if let Some(peer) = &peer {
        let request = OrganEnvelope {
            exchange: ExchangeId {
                opener: Opener::Admin,
                ordinal: 73,
            },
            position: Position::Open,
            payload: Payload::Directive(LifecycleDirective::Stop {
                cause: Cause { uid: 73 },
            }),
        };
        let bytes = serde_json::to_vec(&request).unwrap();
        assert_eq!(
            send(peer.as_raw_fd(), &bytes, MsgFlags::MSG_NOSIGNAL).unwrap(),
            bytes.len()
        );
    }
    for _ in 0..5000 {
        let p = input_step(guard.input.as_mut().unwrap(), clock, real);
        if p == FoundationInputProgress::ManagementPending {
            break;
        }
        assert_ne!(p, FoundationInputProgress::Stopped);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        guard.input.as_ref().unwrap().progress(),
        FoundationInputProgress::ManagementPending
    );
    if real {
        let done = std::sync::atomic::AtomicBool::new(false);
        let result = std::thread::scope(|scope| {
            let responder = scope.spawn(|| {
                let end = Instant::now() + Duration::from_secs(8);
                while !done.load(std::sync::atomic::Ordering::SeqCst) && Instant::now() < end {
                    member.step_live("ready");
                    std::thread::sleep(Duration::from_millis(1));
                }
            });
            let result = worker::management(guard.input.take().unwrap());
            done.store(true, std::sync::atomic::Ordering::SeqCst);
            responder.join().unwrap();
            result
        });
        match result {
            Ok(input) => guard.input = Some(input),
            Err(m) => guard.management = Some(m),
        }
        assert!(
            guard.input.is_some(),
            "{:?}",
            guard.management.as_ref().and_then(|m| m.detail())
        );
        assert!(admin.as_mut().unwrap().wait().unwrap().success());
        let answer: LifecycleAnswer =
            serde_json::from_slice(&std::fs::read(root.join("admin-stopped.json")).unwrap())
                .unwrap();
        assert_eq!(
            answer,
            LifecycleAnswer::TurnAborted {
                turn: TurnKey("stopped-turn".into())
            }
        );
    } else {
        guard.management = Some(guard.input.take().unwrap().management().ok().unwrap());
        let m = guard.management.as_mut().unwrap();
        if mode.ends_with("admin-pressure") {
            fill(m.peer.fd().as_raw_fd());
        }
        if mode.ends_with("gate-pressure") {
            fill(m.input.serving.owner.gate.fd().as_raw_fd());
        }
        if mode.ends_with("early-eof") {
            shutdown(peer.as_ref().unwrap().as_raw_fd(), Shutdown::Both).unwrap();
        }
        if mode.ends_with("gate-eof") {
            shutdown(m.input.serving.owner.gate.fd().as_raw_fd(), Shutdown::Both).unwrap();
        }
        if mode.ends_with("trace-failure") {
            m.input
                .serving
                .owner
                .context
                .barrier
                .fail_next_recorded_observer_for_test();
        }
        if mode.ends_with("trace-pressure") || mode.ends_with("trace-eof") {
            let count = fill_trace(m.input.serving.owner.context, clock);
            let mut io = TraceWait {
                clock,
                reader,
                peer: peer.as_ref().unwrap(),
                root,
                count,
                eof: mode.ends_with("trace-eof"),
                seen: false,
            };
            assert_eq!(
                m.advance_with_io(&mut io),
                FoundationManagementProgress::Pending
            );
            assert!(io.seen, "actual recorder backpressure must be reached");
        }
        let mut pressured = false;
        let mut departed = false;
        for _ in 0..5000 {
            let m = guard.management.as_mut().unwrap();
            let p = m.advance_with_io(&mut CircuitIo(clock));
            if m.send.is_some() && !before_turn {
                assert!(
                    m.stop.as_ref().unwrap().close_sequence.is_some(),
                    "no answer before Recorded close"
                );
                assert!(m.input.serving.owner.context.open_turn.is_none());
            }
            if !departed && mode.ends_with("accepted-eof") && m.stop.is_some() {
                shutdown(peer.as_ref().unwrap().as_raw_fd(), Shutdown::Both).unwrap();
                departed = true;
            }
            if !pressured
                && mode.ends_with("admin-pressure")
                && m.send.as_ref().is_some_and(|s| !s.sent())
            {
                for _ in 0..3 {
                    assert_eq!(
                        m.advance_with_io(&mut CircuitIo(clock)),
                        FoundationManagementProgress::Pending
                    );
                }
                assert!(!m.send.as_ref().unwrap().sent());
                while let Some(bytes) = read_packet(peer.as_ref().unwrap()) {
                    assert!(bytes.iter().all(|b| *b == b'x'));
                }
                pressured = true;
            }
            if !pressured
                && mode.ends_with("gate-pressure")
                && m.stop
                    .as_ref()
                    .and_then(|s| s.gate_send.as_ref())
                    .is_some_and(|s| !s.sent())
            {
                for _ in 0..3 {
                    assert_eq!(
                        m.advance_with_io(&mut CircuitIo(clock)),
                        FoundationManagementProgress::Pending
                    );
                }
                assert!(!m.stop.as_ref().unwrap().gate_done);
                assert!(m.input.stage == Stage::Turn);
                std::fs::write(root.join("drain"), b"yes").unwrap();
                pressured = true;
            }
            if p != FoundationManagementProgress::Pending {
                break;
            }
            member.step(clock.now().unwrap(), "ready");
            std::thread::sleep(Duration::from_millis(1));
        }
        let m = guard.management.as_mut().unwrap();
        if mode.ends_with("gate-eof") || mode.ends_with("trace-failure") {
            assert_eq!(
                m.progress(),
                FoundationManagementProgress::Stopped,
                "{:?}",
                m.detail()
            );
            assert!(m.input.stage == Stage::Stopped);
            assert_eq!(
                m.input.received.as_ref().map(|r| r.envelope()),
                original.as_ref()
            );
            let detail = m.detail().unwrap().to_owned();
            assert_eq!(
                m.advance_with_io(&mut CircuitIo(clock)),
                FoundationManagementProgress::Stopped
            );
            assert_eq!(m.detail(), Some(detail.as_str()));
            assert!(read_packet(peer.as_ref().unwrap()).is_none());
            guard.management = Some(guard.management.take().unwrap().finish().err().unwrap());
            return;
        }
        let dropped = mode.ends_with("early-eof")
            || mode.ends_with("accepted-eof")
            || mode.ends_with("trace-eof");
        assert_eq!(
            m.progress(),
            if dropped {
                FoundationManagementProgress::Dropped
            } else {
                FoundationManagementProgress::Answered
            },
            "{:?}",
            m.detail()
        );
        if mode.ends_with("admin-pressure") || mode.ends_with("gate-pressure") {
            assert!(pressured);
        }
        if !dropped {
            let answer: OrganEnvelope =
                serde_json::from_slice(&read_packet(peer.as_ref().unwrap()).unwrap()).unwrap();
            assert_eq!(
                answer.exchange,
                ExchangeId {
                    opener: Opener::Admin,
                    ordinal: 73
                }
            );
            assert_eq!(answer.position, Position::Close);
            assert_eq!(
                answer.payload,
                Payload::Answer(if before_turn {
                    LifecycleAnswer::AtRest
                } else {
                    LifecycleAnswer::TurnAborted {
                        turn: TurnKey("stopped-turn".into()),
                    }
                })
            );
        }
        let progress = m.progress();
        for _ in 0..3 {
            assert_eq!(m.advance_with_io(&mut CircuitIo(clock)), progress);
        }
        guard.input = Some(guard.management.take().unwrap().finish().ok().unwrap());
    }
    if before_turn {
        let input = guard.input.as_ref().unwrap();
        assert!(input.stage == target);
        assert_eq!(input.serving.owner.context.input.snapshot(), snapshot);
        assert_eq!(
            input
                .serving
                .owner
                .context
                .deadlines
                .get(PumpDeadline::Gate),
            deadline
        );
        assert_eq!(
            input.received.as_ref().map(|r| r.envelope()),
            original.as_ref()
        );
        assert!(trace(reader).is_empty(), "AtRest changes nothing");
        assert!(!root.join("closed.json").exists());
        prepare(
            guard.input.as_mut().unwrap(),
            member,
            clock,
            real,
            Stage::Turn,
        );
    } else {
        assert_eq!(
            guard.input.as_ref().unwrap().progress(),
            FoundationInputProgress::Aborted
        );
        for _ in 0..1000 {
            if root.join("closed.json").exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        let close: OrganEnvelope =
            serde_json::from_slice(&std::fs::read(root.join("closed.json")).unwrap()).unwrap();
        assert_eq!(close.exchange, original.as_ref().unwrap().exchange);
        let Payload::Frame(frame) = close.payload else {
            panic!()
        };
        let close: serde_json::Value = serde_json::from_slice(&frame.octets().unwrap()).unwrap();
        assert_eq!(
            close,
            serde_json::json!({"kind":"stopped","reason":"the operator stopped the turn","run":run,"turn":"stopped-turn"})
        );
        records.extend(trace(reader));
        let closes: Vec<_> = records
            .iter()
            .filter(|r| r["kind"] == "turn.closed")
            .collect();
        assert_eq!(closes.len(), 1);
        assert_eq!(closes[0]["turn"], "stopped-turn");
        assert_eq!(
            closes[0]["payload"],
            serde_json::json!({"close":"stopped","reason":"directive","cause":{"uid":73}})
        );
        let FoundationInputResult::Aborted(serving) =
            guard.input.take().unwrap().finish().ok().unwrap()
        else {
            panic!("same serving seat")
        };
        guard.input = Some(serving.input());
        assert!(
            guard
                .input
                .as_ref()
                .unwrap()
                .serving
                .owner
                .context
                .open_turn
                .is_none()
        );
        if mode.ends_with("next") {
            prepare(
                guard.input.as_mut().unwrap(),
                member,
                clock,
                real,
                Stage::Accepted,
            );
            let input = guard.input.as_mut().unwrap();
            assert_eq!(
                input.received.as_ref().unwrap().envelope().exchange.ordinal,
                102
            );
            input.start_turn(TurnKey("next-turn".into())).unwrap();
            prepare(input, member, clock, real, Stage::Turn);
            assert_eq!(input.seated.as_ref().unwrap().input_id.0, 2);
            assert_eq!(input.seated.as_ref().unwrap().turn.0, "next-turn");
        }
    }
    records.extend(trace(reader));
    let count = if mode.ends_with("next") { 2 } else { 1 };
    for kind in ["input.offered", "turn.started", "message.user"] {
        assert_eq!(
            records.iter().filter(|r| r["kind"] == kind).count(),
            count,
            "{kind}"
        );
    }
}
