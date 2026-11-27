//! **The gate's door, against the shipped gate**, per `weaver-gate-Spec`
//! section 3 and `weaver-admin-Spec` section 6: admin lays
//! `weaver.run/<name>/gate/` owned by the agent's account and grouped to the
//! gate group, `2750`, and the gate binds `gate.sock` there under its own
//! umask. The socket takes the door's group and not the binder's, so a uid in
//! the gate group and not in the agent's group is admitted, a uid in the
//! agent's group and not in the gate group is turned away at the connect, and
//! the agent's own uid is denied by the gate's construction.
//!
//! The binder holds neither the door's group nor root: the instrument runs
//! inside a user namespace, as root there, lays the door as admin does, starts
//! the gate under a mapped uid whose one group is the agent's, and dials from
//! children of this test binary that drop to each caller's identity first.
//! Admin's own suite pins the door's shape as admin lays it
//! (`the_gates_door_is_laid_setgid_to_the_gate_group`), since no organ links
//! another.

mod common;

use std::io::{Read, Write};
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use common::{ask, bound_receives, place_inherited, seqpacket_pair, wait_bounded};
use nix::sys::socket::{MsgFlags, recv, send};
use weaver_types::{
    AccessRule, GateInstruction, LifecycleAnswer, LifecycleDirective, Opener, OrganEnvelope,
    Payload, Position, TurnFrame,
};

/// The agent's account and its own group, which the gate runs under.
const AGENT_UID: u32 = 1;
const AGENT_GID: u32 = 1;
/// The gate group, the door's.
const GATE_GID: u32 = 2;
/// The gate connector: its own primary group, and the gate group beside it.
const CONNECTOR_UID: u32 = 3;
const CONNECTOR_GID: u32 = 3;
/// A uid in the agent's group and not in the gate group.
const AGENT_GROUP_UID: u32 = 4;

/// The environment that turns this binary's client test into a dialer.
const DIAL: &str = "WEAVER_GATE_DOOR_DIAL";

/// Drops to `uid`, its primary `gid` and the supplementary `groups`, in the
/// child before its exec.
fn drop_to(command: &mut Command, uid: u32, gid: u32, groups: &'static [u32]) {
    unsafe {
        command.pre_exec(move || {
            let list: Vec<nix::libc::gid_t> = groups.to_vec();
            if nix::libc::setgroups(list.len(), list.as_ptr()) != 0
                || nix::libc::setresgid(gid, gid, gid) != 0
                || nix::libc::setresuid(uid, uid, uid) != 0
            {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

fn spawn_gate(child_end: std::os::fd::RawFd, log: &std::path::Path) -> Child {
    let log = std::fs::File::create(log).expect("a child log file");
    let mut command = Command::new(env!("CARGO_BIN_EXE_weaver-gate"));
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(log));
    let tools = common::plain_tools();
    place_inherited(&mut command, &[child_end, tools.as_raw_fd()]);
    drop_to(&mut command, AGENT_UID, AGENT_GID, &[]);
    let child = command.spawn().expect("the binary starts");
    drop(tools);
    child
}

/// A dialer: this test binary's client test, run as the caller named.
fn spawn_dialer(path: &std::path::Path, uid: u32, gid: u32, groups: &'static [u32]) -> Child {
    let exe = std::env::current_exe().expect("the test binary names itself");
    let mut command = Command::new(exe);
    command
        .args([
            "--ignored",
            "--exact",
            "door_client_dials_as_its_caller",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(DIAL, path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    drop_to(&mut command, uid, gid, groups);
    command.spawn().expect("the dialer starts")
}

/// What a dialer saw, from its one `DIAL ` mark, which the test harness
/// prints after the test's name on the same line.
fn outcome(child: Child) -> String {
    let output = child.wait_with_output().expect("the dialer ends");
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .find_map(|line| line.split_once("DIAL ").map(|(_, seen)| seen.to_string()))
        .unwrap_or_else(|| {
            format!(
                "no outcome\nstdout:\n{stdout}\nstderr:\n{}",
                String::from_utf8_lossy(&output.stderr)
            )
        })
}

fn receive(end: &OwnedFd) -> OrganEnvelope {
    let mut buffer = vec![0u8; 128 * 1024];
    let read = recv(end.as_raw_fd(), &mut buffer, MsgFlags::empty()).expect("an envelope arrives");
    assert!(read > 0, "the gate closed the channel");
    buffer.truncate(read);
    serde_json::from_slice(&buffer).expect("an envelope")
}

/// The ids of a process's real uid, real gid and supplementary groups, read
/// from `/proc`.
fn credentials(pid: u32) -> (u32, u32, Vec<u32>) {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).expect("the status");
    let field = |name: &str| -> Vec<u32> {
        status
            .lines()
            .find_map(|line| line.strip_prefix(name))
            .unwrap_or("")
            .split_whitespace()
            .filter_map(|word| word.parse().ok())
            .collect()
    };
    (field("Uid:")[0], field("Gid:")[0], field("Groups:"))
}

/// **The door admits by the gate group, and by it alone**: the gate, running
/// as the agent's uid and holding neither the gate group nor root, binds in a
/// door laid `2750` to the gate group, and its socket comes out `0770` grouped
/// to the gate group. The connector, in the gate group and not in the agent's,
/// is admitted and answered. A uid in the agent's group and not in the gate
/// group is turned away at the connect. The agent's own uid, which the rule
/// names on purpose, connects through the owner's bits and is denied by the
/// gate's construction, closed with nothing written.
///
/// Perturbations: lay the door without its setgid bit, and the socket takes
/// the binder's group, so the connector is turned away at the connect; lay it
/// grouped to the agent's group, and the agent-group uid is admitted where it
/// should be refused; drop the gate's self-deny, and the agent's own uid is
/// answered.
#[test]
#[ignore = "needs root in a user namespace; run by the_door_is_watched_inside_a_user_namespace"]
fn the_door_admits_the_gate_group_and_not_the_agents() {
    assert!(
        nix::unistd::geteuid().is_root(),
        "this instrument needs euid 0"
    );
    let base = std::env::temp_dir().join(format!("weaver-gate-door-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let run = base.join("weaver.run").join("a");
    std::fs::create_dir_all(&run).expect("the run directory");
    for directory in [&base, &base.join("weaver.run"), &run] {
        std::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o755))
            .expect("the run directory opens to traversal");
    }
    // **The door, as admin's load lays it**: the agent's account, the gate
    // group, `2750`, the owner and group set before the mode, since a chown
    // clears a setgid bit.
    let door = run.join("gate");
    std::fs::create_dir(&door).expect("the door");
    std::os::unix::fs::chown(&door, Some(AGENT_UID), Some(GATE_GID)).expect("the door's owner");
    std::fs::set_permissions(&door, std::fs::Permissions::from_mode(0o2750))
        .expect("the door's mode");
    let path = door.join("gate.sock");
    let log = base.join("gate.log");

    let (harness, child_end) = seqpacket_pair();
    bound_receives(&harness, 10);
    let mut gate = spawn_gate(child_end.as_raw_fd(), &log);
    let (uid, gid, groups) = credentials(gate.id());
    assert_eq!(
        (uid, gid),
        (AGENT_UID, AGENT_GID),
        "the gate runs as the agent"
    );
    assert!(
        !groups.contains(&GATE_GID) && !groups.contains(&0),
        "the binder holds neither the door's group nor root: {groups:?}"
    );

    let mut allowed_uids = std::collections::BTreeSet::new();
    for admitted in [CONNECTOR_UID, AGENT_GROUP_UID, AGENT_UID] {
        allowed_uids.insert(admitted);
    }
    let ready = ask(
        &harness,
        1,
        LifecycleDirective::Raise {
            instruction: GateInstruction {
                access_rule: AccessRule {
                    allowed_uids,
                    allowed_gids: Default::default(),
                    denied_uids: Default::default(),
                },
            },
            socket: path.clone(),
            shell: None,
        },
    );
    assert_eq!(
        ready.payload,
        Payload::Answer(LifecycleAnswer::GateReady),
        "{}",
        std::fs::read_to_string(&log).unwrap_or_default()
    );
    let bound = std::fs::symlink_metadata(&path).expect("the socket stands");
    assert_eq!(
        (bound.uid(), bound.gid(), bound.mode() & 0o777),
        (AGENT_UID, GATE_GID, 0o770),
        "the socket takes the door's group, not the binder's"
    );

    // The connector: admitted, its line framed and answered.
    let connector = spawn_dialer(&path, CONNECTOR_UID, CONNECTOR_GID, &[GATE_GID]);
    let envelope = receive(&harness);
    assert_eq!(envelope.exchange.opener, Opener::Gate);
    let Payload::Frame(frame) = &envelope.payload else {
        panic!("a frame, not {:?}", envelope.payload);
    };
    assert_eq!(frame.octets().expect("decodes"), b"through the door");
    let answer = OrganEnvelope {
        exchange: envelope.exchange.clone(),
        position: Position::Close,
        payload: Payload::Frame(TurnFrame::carry(br#"{"kind":"answered"}"#)),
    };
    send(
        harness.as_raw_fd(),
        &serde_json::to_vec(&answer).expect("encodes"),
        MsgFlags::empty(),
    )
    .expect("the answer is sent");
    assert_eq!(
        outcome(connector),
        r#"answered {"kind":"answered"}"#,
        "the gate group's uid is admitted"
    );

    // A uid in the agent's group alone: the door turns it away.
    assert_eq!(
        outcome(spawn_dialer(&path, AGENT_GROUP_UID, AGENT_GID, &[])),
        "refused PermissionDenied",
        "the agent's group reaches nothing in the door"
    );
    // The agent's own uid: through the owner's bits, then denied by the gate.
    assert_eq!(
        outcome(spawn_dialer(&path, AGENT_UID, AGENT_GID, &[])),
        "closed",
        "the agent's own uid stays denied"
    );

    let stopped = ask(&harness, 2, LifecycleDirective::Lower);
    assert_eq!(
        stopped.payload,
        Payload::Answer(LifecycleAnswer::GateStopped)
    );
    drop(harness);
    assert!(wait_bounded(&mut gate, 30, "the lower").success());
    let _ = std::fs::remove_dir_all(&base);
}

/// The dialer the instrument runs as each caller: it connects, sends one
/// line and reads one, and prints what it saw. Without its environment it
/// does nothing, so a plain `--ignored` run passes it.
#[test]
#[ignore = "a dialer the door instrument runs as each caller"]
fn door_client_dials_as_its_caller() {
    let Some(path) = std::env::var_os(DIAL) else {
        return;
    };
    let mut stream = match UnixStream::connect(&path) {
        Ok(stream) => stream,
        Err(error) => {
            println!("DIAL refused {:?}", error.kind());
            return;
        }
    };
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("a bounded read");
    if stream.write_all(b"through the door\n").is_err() {
        println!("DIAL closed");
        return;
    }
    let mut line = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        match stream.read(&mut byte) {
            Ok(0) => break,
            Ok(_) if byte[0] == b'\n' => break,
            Ok(_) => line.push(byte[0]),
            Err(_) => break,
        }
    }
    if line.is_empty() {
        println!("DIAL closed");
    } else {
        println!("DIAL answered {}", String::from_utf8_lossy(&line));
    }
}

/// The watch: re-runs the instrument inside `unshare --map-auto
/// --map-root-user`, where the test is root and the gate and the dialers run
/// as mapped uids; run as root already, it runs it in place. A box with no
/// user namespace says so and skips.
#[test]
fn the_door_is_watched_inside_a_user_namespace() {
    let exe = std::env::current_exe().expect("the test binary names itself");
    let mut command = if nix::unistd::geteuid().is_root() {
        Command::new(&exe)
    } else {
        let mut unshare = Command::new("unshare");
        unshare.args(["--map-auto", "--map-root-user"]).arg(&exe);
        unshare
    };
    let ran = command
        .args([
            "--ignored",
            "--exact",
            "the_door_admits_the_gate_group_and_not_the_agents",
            "--nocapture",
            "--test-threads=1",
        ])
        .stdin(Stdio::null())
        .output();
    let output = match ran {
        Ok(output) => output,
        Err(e) => {
            eprintln!("SKIP door watch: unshare could not run: {e}");
            return;
        }
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.starts_with("unshare:") {
        eprintln!("SKIP door watch: no user namespace here: {}", stderr.trim());
        return;
    }
    assert!(
        output.status.success() && stdout.contains("test result: ok. 1 passed"),
        "the door instrument failed inside the namespace\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
