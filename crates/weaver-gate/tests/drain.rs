//! **The unload's drain, against the shipped gate**, per `weaver-gate-Spec`
//! section 4 and `weaver-admin-Spec` 3.0 (the graceful unload, steps 2 to 5;
//! the forced unload): the gate accepts no further input at `Quiesce`, every
//! request it admitted before the quiesce is answered on its own connection,
//! and `Lower` from draining answers `GateStopped` only after those answers
//! are written; `Lower` from raised closes at once.
//!
//! This suite drives the real binary and plays the harness over the channel,
//! so the ordering it reads is the gate's own and not a stand-in's. The gate
//! denies its own uid by construction (`hook.rs`, the raise's deny set), so a
//! client of the test's uid could never be admitted: the instrument runs
//! inside a user namespace, as root there, starts the gate under a mapped uid
//! and dials as root, whom the rule admits.

mod common;

use std::io::{Read, Write};
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use common::{ask, bound_receives, place_inherited, scratch, seqpacket_pair, wait_bounded};
use nix::sys::socket::{MsgFlags, recv, send};
use weaver_types::{
    AccessRule, ExchangeId, GateInstruction, LifecycleAnswer, LifecycleDirective, Opener,
    OrganEnvelope, Payload, Position, TurnFrame,
};

/// The uid the gate runs under inside the namespace: mapped by `--map-auto`,
/// and not root, so the gate's self-deny excludes it and not the client.
const GATE_UID: u32 = 1;

fn spawn_gate_as(child_end: std::os::fd::RawFd, log: &std::path::Path) -> Child {
    let log = std::fs::File::create(log).expect("a child log file");
    let mut command = Command::new(env!("CARGO_BIN_EXE_weaver-gate"));
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(log));
    place_inherited(&mut command, &[child_end]);
    unsafe {
        command.pre_exec(|| {
            if nix::libc::setgroups(0, std::ptr::null()) != 0
                || nix::libc::setresgid(GATE_UID, GATE_UID, GATE_UID) != 0
                || nix::libc::setresuid(GATE_UID, GATE_UID, GATE_UID) != 0
            {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    command.spawn().expect("the binary starts")
}

fn root_admitted() -> GateInstruction {
    let mut allowed_uids = std::collections::BTreeSet::new();
    allowed_uids.insert(0);
    GateInstruction {
        access_rule: AccessRule {
            allowed_uids,
            allowed_gids: Default::default(),
            denied_uids: Default::default(),
        },
    }
}

fn receive(end: &OwnedFd) -> OrganEnvelope {
    let mut buffer = vec![0u8; 128 * 1024];
    let read = recv(end.as_raw_fd(), &mut buffer, MsgFlags::empty()).expect("an envelope arrives");
    assert!(read > 0, "the gate closed the channel");
    buffer.truncate(read);
    serde_json::from_slice(&buffer).expect("an envelope")
}

fn respond(end: &OwnedFd, exchange: &ExchangeId, line: &str) {
    let envelope = OrganEnvelope {
        exchange: exchange.clone(),
        position: Position::Close,
        payload: Payload::Frame(TurnFrame::carry(line.as_bytes())),
    };
    let body = serde_json::to_vec(&envelope).expect("encodes");
    send(end.as_raw_fd(), &body, MsgFlags::empty()).expect("the response is sent");
}

fn dial(path: &std::path::Path) -> UnixStream {
    let stream = UnixStream::connect(path).expect("an admitted dial");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("a bounded read");
    stream
}

fn read_line(stream: &mut UnixStream) -> String {
    let mut line = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        match stream.read(&mut byte) {
            Ok(0) => return String::from_utf8_lossy(&line).into_owned(),
            Ok(_) if byte[0] == b'\n' => return String::from_utf8_lossy(&line).into_owned(),
            Ok(_) => line.push(byte[0]),
            Err(error) => panic!("the read failed: {error}"),
        }
    }
}

/// The frame a client's line opened, read off the channel.
fn frame_of(end: &OwnedFd, text: &str) -> ExchangeId {
    let envelope = receive(end);
    assert_eq!(envelope.exchange.opener, Opener::Gate);
    let Payload::Frame(frame) = envelope.payload else {
        panic!("a frame, not {:?}", envelope.payload);
    };
    assert_eq!(frame.octets().expect("decodes"), text.as_bytes());
    envelope.exchange
}

/// **The drain, read from the shipped gate**: two requests admitted before
/// the quiesce are answered on their own connections after it, the one
/// refused and the other given its final output; an idle connection closes
/// at the quiesce, never having been received as a request; a dial after the
/// quiesce finds no listener; and `GateStopped` comes only after both
/// answers are written, each read by its client.
///
/// The responses and the `Lower` are sent back to back and the clients read
/// only after `GateStopped`, so the answers must already be written when the
/// gate answers stopped.
///
/// Perturbations: drop the relay at `Quiesce` and both clients read the end
/// of the connection instead of their answers; answer `Lower` from a
/// draining gate without waiting on the writes and the clients read the end
/// where their answers should be.
#[test]
#[ignore = "needs root in a user namespace; run by the_drain_is_watched_inside_a_user_namespace"]
fn the_quiesce_answers_every_admitted_request_and_lowers_after_the_writes() {
    assert!(
        nix::unistd::geteuid().is_root(),
        "this instrument needs euid 0"
    );
    let path = scratch("drain", "quiesce");
    let dir = path.parent().expect("a scratch dir").to_path_buf();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o777)).expect("the dir opens");
    let log = dir.join("gate.log");
    let (harness, child_end) = seqpacket_pair();
    bound_receives(&harness, 10);
    let mut child = spawn_gate_as(child_end.as_raw_fd(), &log);

    let ready = ask(
        &harness,
        1,
        LifecycleDirective::Raise {
            instruction: root_admitted(),
            socket: path.to_path_buf(),
        },
    );
    assert_eq!(
        ready.payload,
        Payload::Answer(LifecycleAnswer::GateReady),
        "{}",
        std::fs::read_to_string(&log).unwrap_or_default()
    );

    let mut in_flight = dial(&path);
    let mut received = dial(&path);
    let mut idle = dial(&path);
    in_flight.write_all(b"in flight\n").expect("writes");
    let in_flight_exchange = frame_of(&harness, "in flight");
    received.write_all(b"received\n").expect("writes");
    let received_exchange = frame_of(&harness, "received");

    let quiesced = ask(&harness, 2, LifecycleDirective::Quiesce);
    assert_eq!(
        quiesced.payload,
        Payload::Answer(LifecycleAnswer::GateQuiesced)
    );
    assert!(
        UnixStream::connect(&path).is_err(),
        "no new connection after the quiesce"
    );
    assert_eq!(
        read_line(&mut idle),
        "",
        "the idle connection closes at the quiesce"
    );

    // The received-but-unstarted request is refused, the turn in flight
    // gives its final output, each on its own still-standing connection.
    respond(
        &harness,
        &received_exchange,
        r#"{"kind":"refused","reason":"the agent is unloading"}"#,
    );
    respond(
        &harness,
        &in_flight_exchange,
        r#"{"kind":"answered","text":"final"}"#,
    );
    let stopped = ask(&harness, 3, LifecycleDirective::Lower);
    assert_eq!(
        stopped.payload,
        Payload::Answer(LifecycleAnswer::GateStopped)
    );
    assert_eq!(
        read_line(&mut received),
        r#"{"kind":"refused","reason":"the agent is unloading"}"#,
        "written before stopped was answered"
    );
    assert_eq!(
        read_line(&mut in_flight),
        r#"{"kind":"answered","text":"final"}"#,
        "written before stopped was answered"
    );
    assert_eq!(
        read_line(&mut in_flight),
        "",
        "the connections close with the lower"
    );

    drop(harness);
    assert!(wait_bounded(&mut child, 30, "the drain").success());
    let _ = std::fs::remove_dir_all(&dir);
}

/// **A client that hung up does not hold the lower** (self-check on B1): a
/// draining connection whose client closed fully while its answer was owed
/// is written once, the write fails, and the connection leaves, so
/// `GateStopped` comes at once rather than at the lower bound, and the gate
/// never reads a draining connection. Perturbation: take the read branch on
/// a hang-up again, and stopped waits for the bound.
#[test]
#[ignore = "needs root in a user namespace; run by the_drain_is_watched_inside_a_user_namespace"]
fn a_client_that_hung_up_does_not_hold_the_lower() {
    assert!(
        nix::unistd::geteuid().is_root(),
        "this instrument needs euid 0"
    );
    let path = scratch("drain", "hung-up");
    let dir = path.parent().expect("a scratch dir").to_path_buf();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o777)).expect("the dir opens");
    let log = dir.join("gate.log");
    let (harness, child_end) = seqpacket_pair();
    bound_receives(&harness, 20);
    let mut child = spawn_gate_as(child_end.as_raw_fd(), &log);
    let ready = ask(
        &harness,
        1,
        LifecycleDirective::Raise {
            instruction: root_admitted(),
            socket: path.to_path_buf(),
        },
    );
    assert_eq!(ready.payload, Payload::Answer(LifecycleAnswer::GateReady));
    let mut gone = dial(&path);
    gone.write_all(b"then gone\n").expect("writes");
    let exchange = frame_of(&harness, "then gone");
    let quiesced = ask(&harness, 2, LifecycleDirective::Quiesce);
    assert_eq!(
        quiesced.payload,
        Payload::Answer(LifecycleAnswer::GateQuiesced)
    );
    drop(gone);
    respond(
        &harness,
        &exchange,
        r#"{"kind":"answered","text":"unread"}"#,
    );
    let started = std::time::Instant::now();
    let stopped = ask(&harness, 3, LifecycleDirective::Lower);
    assert_eq!(
        stopped.payload,
        Payload::Answer(LifecycleAnswer::GateStopped)
    );
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "stopped waited {:?} on a client that hung up",
        started.elapsed()
    );
    drop(harness);
    assert!(wait_bounded(&mut child, 30, "the hung-up drain").success());
    let _ = std::fs::remove_dir_all(&dir);
}

/// **A request never answered closes at the lower at once**
/// (`weaver-harness-gate-contract` section 2: the lower waits on "every
/// response the harness sent"): the channel is ordered, so an exchange still
/// open when the `Lower` is read will never be answered; its connection
/// closes unanswered, the harness having recorded it refused, and no lost
/// delivery is named. A response sent and left unread is pinned in
/// `relay.rs`, a single response always fitting the kernel's buffer here.
/// Perturbation: count the open exchange as owed, and stopped waits for the
/// settle instant.
#[test]
#[ignore = "needs root in a user namespace; run by the_drain_is_watched_inside_a_user_namespace"]
fn a_request_never_answered_closes_at_the_lower_at_once() {
    assert!(
        nix::unistd::geteuid().is_root(),
        "this instrument needs euid 0"
    );
    let path = scratch("drain", "never-answered");
    let dir = path.parent().expect("a scratch dir").to_path_buf();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o777)).expect("the dir opens");
    let log = dir.join("gate.log");
    let (harness, child_end) = seqpacket_pair();
    bound_receives(&harness, 20);
    let mut child = spawn_gate_as(child_end.as_raw_fd(), &log);
    let ready = ask(
        &harness,
        1,
        LifecycleDirective::Raise {
            instruction: root_admitted(),
            socket: path.to_path_buf(),
        },
    );
    assert_eq!(ready.payload, Payload::Answer(LifecycleAnswer::GateReady));
    let mut waiting = dial(&path);
    waiting.write_all(b"never answered\n").expect("writes");
    frame_of(&harness, "never answered");
    let quiesced = ask(&harness, 2, LifecycleDirective::Quiesce);
    assert_eq!(
        quiesced.payload,
        Payload::Answer(LifecycleAnswer::GateQuiesced)
    );
    let started = std::time::Instant::now();
    let stopped = ask(&harness, 3, LifecycleDirective::Lower);
    let waited = started.elapsed();
    assert_eq!(
        stopped.payload,
        Payload::Answer(LifecycleAnswer::GateStopped)
    );
    assert!(
        waited < Duration::from_secs(2),
        "stopped waited {waited:?} on a request that will never be answered"
    );
    assert_eq!(read_line(&mut waiting), "", "closed unanswered");
    let named = std::fs::read_to_string(&log).unwrap_or_default();
    assert!(
        !named.contains("lost_delivery"),
        "nothing the harness sent was lost: {named}"
    );
    drop(waiting);
    drop(harness);
    assert!(wait_bounded(&mut child, 30, "the never-answered lower").success());
    let _ = std::fs::remove_dir_all(&dir);
}

/// **The forced path lowers at once** (`weaver-admin-Spec` 3.0, the forced
/// unload (sole)): the admitted request's frame reaches the harness ahead of
/// `GateStopped`, so the harness records it refused `Unloading`, and its
/// connection closes unanswered, the operator having chosen no time to
/// finish.
#[test]
#[ignore = "needs root in a user namespace; run by the_drain_is_watched_inside_a_user_namespace"]
fn a_lower_from_a_raised_gate_drops_the_request_in_flight() {
    assert!(
        nix::unistd::geteuid().is_root(),
        "this instrument needs euid 0"
    );
    let path = scratch("drain", "forced");
    let dir = path.parent().expect("a scratch dir").to_path_buf();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o777)).expect("the dir opens");
    let log = dir.join("gate.log");
    let (harness, child_end) = seqpacket_pair();
    bound_receives(&harness, 10);
    let mut child = spawn_gate_as(child_end.as_raw_fd(), &log);
    let ready = ask(
        &harness,
        1,
        LifecycleDirective::Raise {
            instruction: root_admitted(),
            socket: path.to_path_buf(),
        },
    );
    assert_eq!(ready.payload, Payload::Answer(LifecycleAnswer::GateReady));
    let mut in_flight = dial(&path);
    in_flight.write_all(b"in flight\n").expect("writes");
    frame_of(&harness, "in flight");
    let stopped = ask(&harness, 2, LifecycleDirective::Lower);
    assert_eq!(
        stopped.payload,
        Payload::Answer(LifecycleAnswer::GateStopped)
    );
    assert_eq!(read_line(&mut in_flight), "", "dropped unanswered");
    drop(harness);
    assert!(wait_bounded(&mut child, 30, "the forced lower").success());
    let _ = std::fs::remove_dir_all(&dir);
}

/// The watch: re-runs the four instruments inside `unshare --map-auto
/// --map-root-user`, where the test is root and the gate runs as a mapped
/// uid; run as root already, it runs them in place, root having what the
/// namespace grants (Codex on #114: it returned a pass there having run
/// nothing), as the preload door's watch does. A box with no user namespace
/// says so and skips.
#[test]
fn the_drain_is_watched_inside_a_user_namespace() {
    let exe = std::env::current_exe().expect("the test binary names itself");
    let mut command = if nix::unistd::geteuid().is_root() {
        Command::new(&exe)
    } else {
        let mut unshare = Command::new("unshare");
        unshare.args(["--map-auto", "--map-root-user"]).arg(&exe);
        unshare
    };
    let ran = command
        .args(["--ignored", "--nocapture", "--test-threads=1"])
        .stdin(Stdio::null())
        .output();
    let output = match ran {
        Ok(output) => output,
        Err(e) => {
            eprintln!("SKIP drain watch: unshare could not run: {e}");
            return;
        }
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.starts_with("unshare:") {
        eprintln!(
            "SKIP drain watch: no user namespace here: {}",
            stderr.trim()
        );
        return;
    }
    assert!(
        output.status.success() && stdout.contains("test result: ok. 4 passed"),
        "the drain instruments failed inside the namespace\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
