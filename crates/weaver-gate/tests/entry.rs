//! conforms: gate-dumpable-flag-cleared
//! conforms: gate-closure-is-death
//! conforms: gate-agent-uid-denied-by-construction
//!
//! The process facts of `weaver-gate-Spec` section 2, exercised as a process.
//!
//! **The second walk: a same-uid process attaches to the gate.** The adversary
//! is the agent's tool surface, the attack a `ptrace` or `/proc/[pid]/fd` reach
//! into the one process of the worker's family that faces outward. The
//! mechanism is the dumpable flag cleared at entry, a set and not a check.
//!
//! **The third walk: the boundary outlives its interior.** The adversary is
//! timing, a client conversing with a gate whose harness is gone. The test
//! closes the harness end of a standing pair and confirms the gate exits and
//! the listener is gone. **It reaches the closure half and no further:** the
//! parent-death signal's backing needs a double that forks from a thread and
//! ends that thread while its process lives, which this suite does not buy, and
//! killing the double as a process would end the forking thread too, making the
//! perturbation one that can never be seen to fail.

mod common;

use std::io::Write;
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use common::{
    ask, bound_receives, instruction, place_inherited, scratch, seqpacket_pair, tell, wait_bounded,
};
use weaver_types::{LifecycleAnswer, LifecycleDirective, Payload};

/// Start the gate with its one end at descriptor 3. Standard error goes to a
/// file rather than a pipe, since nothing drains a pipe until after the wait.
fn spawn_gate(child_end: std::os::fd::RawFd) -> (Child, common::ScratchFile) {
    let log_path = common::ScratchFile(std::env::temp_dir().join(format!(
        "weaver-gate-entry-{}-{}.log",
        std::process::id(),
        COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )));
    let log = std::fs::File::create(&log_path).expect("a child log file");
    let mut command = Command::new(env!("CARGO_BIN_EXE_weaver-gate"));
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(log));
    place_inherited(&mut command, &[child_end]);
    (command.spawn().expect("the binary starts"), log_path)
}

static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn socket_path(name: &str) -> common::Scratch {
    scratch("entry", name)
}

/// **The dumpable flag is cleared at entry**, read from outside by an observer
/// the process cannot lie to: clearing it transfers ownership of `/proc/[pid]`
/// to root, so the parent reads the fact from the filesystem.
///
/// Perturbation: remove the `set_dumpable` call from `adopt` and this test
/// fails, because `/proc/[pid]/fd` stays owned by the invoking uid. Watched
/// under exactly that removal.
#[test]
fn the_dumpable_flag_is_clear_after_entry() {
    use std::os::unix::fs::MetadataExt;

    // Under uid 0 the observation is vacuous: /proc/[pid]/fd is root-owned
    // either way, so the test would pass with the clear removed.
    if nix::unistd::geteuid().is_root() {
        eprintln!("SKIP the_dumpable_flag_is_clear_after_entry: root owns /proc either way");
        return;
    }

    let (harness, child_end) = seqpacket_pair();
    let (mut child, log_path) = spawn_gate(child_end.as_raw_fd());
    let pid = child.id();

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut owner = None;
    while Instant::now() < deadline {
        if let Ok(meta) = std::fs::metadata(format!("/proc/{pid}/fd")) {
            owner = Some(meta.uid());
            if meta.uid() == 0 {
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(25));
    }

    drop(harness);
    wait_bounded(&mut child, 30, "the_dumpable_flag_is_clear_after_entry");
    std::fs::remove_file(&log_path).ok();

    assert_eq!(
        owner,
        Some(0),
        "a cleared dumpable flag makes /proc/{pid}/fd owned by root, found {owner:?}"
    );
}

/// **Closure is death.** The harness end of a standing pair closes while the
/// gate holds a raised hook, and the gate exits and its listener is gone.
///
/// The double stays alive as a process throughout: it is this test, and it
/// simply drops its end. That matters, because killing a double as a process
/// would end the forking thread too and the parent-death signal would exit the
/// gate whether or not the closure handling stands, which is a perturbation
/// that can never be seen to fail.
///
/// Perturbation: make the `Closed` arm of `serve` continue the loop instead of
/// returning and this test fails, because the gate never exits and the bounded
/// wait kills it. Watched under exactly that change.
#[test]
fn a_closed_channel_ends_the_gate_and_its_listener() {
    let path = socket_path("closure");
    let (harness, child_end) = seqpacket_pair();
    let (mut child, log_path) = spawn_gate(child_end.as_raw_fd());

    let ready = ask(
        &harness,
        1,
        LifecycleDirective::Raise {
            instruction: instruction(),
            socket: path.to_path_buf(),
        },
    );
    assert_eq!(
        ready.payload,
        Payload::Answer(LifecycleAnswer::GateReady),
        "the gate raises"
    );
    // Ready is a fact about a listener: a dial reaches it right now.
    let mut standing = UnixStream::connect(&path).expect("the raised hook accepts a dial");
    standing.write_all(b"x").ok();

    // The interior goes away. Nothing else is done to the gate.
    drop(harness);

    let status = wait_bounded(&mut child, 30, "a_closed_channel_ends_the_gate");
    assert!(
        status.success(),
        "closure is an orderly exit, stderr: {}",
        std::fs::read_to_string(&log_path).unwrap_or_default()
    );
    assert!(
        UnixStream::connect(&path).is_err(),
        "the listener died with the process that held it"
    );
    // The path survives: this crate unlinks nothing, even on the way out.
    assert!(
        path.exists(),
        "the operator's artifact is left where it was"
    );
    std::fs::remove_file(&path).ok();
    std::fs::remove_file(&log_path).ok();
}

/// The raise-then-lower round trip across the real seam, into the real binary.
#[test]
fn raise_then_lower_round_trips_across_the_seam() {
    let path = socket_path("round-trip");
    let (harness, child_end) = seqpacket_pair();
    let (mut child, log_path) = spawn_gate(child_end.as_raw_fd());

    let ready = ask(
        &harness,
        1,
        LifecycleDirective::Raise {
            instruction: instruction(),
            socket: path.to_path_buf(),
        },
    );
    assert_eq!(ready.payload, Payload::Answer(LifecycleAnswer::GateReady));
    assert_eq!(ready.exchange.ordinal, 1);

    let stopped = ask(&harness, 2, LifecycleDirective::Lower);
    assert_eq!(
        stopped.payload,
        Payload::Answer(LifecycleAnswer::GateStopped)
    );
    assert_eq!(stopped.exchange.ordinal, 2);
    assert!(
        UnixStream::connect(&path).is_err(),
        "stopped is answered only after the close returned"
    );

    drop(harness);
    let status = wait_bounded(&mut child, 30, "raise_then_lower_round_trips");
    assert!(status.success());
    std::fs::remove_file(&path).ok();
    std::fs::remove_file(&log_path).ok();
}

/// **The harness's real teardown ends the gate cleanly.**
///
/// `leave()` sends Lower, drops the channel, and reaps, never reading the
/// answer. The gate wakes, closes its listener, and its send of `GateStopped`
/// meets a peer that is already gone. That is the ordinary path every clean
/// session takes, and it must exit zero: an earlier cut returned failure here,
/// so every orderly unload recorded a gate fault for any supervisor reading
/// exit status.
///
/// The existing closure test exercises a gate blocked in `recv`. This one
/// exercises the sequence the real caller performs, which is a different code
/// path and was the one that was wrong.
///
/// Perturbation: return `ExitCode::FAILURE` on the `Closed` arm of the answer
/// send and this test fails. Watched under exactly that change.
#[test]
fn the_harness_teardown_sequence_ends_the_gate_cleanly() {
    let path = socket_path("teardown");
    let (harness, child_end) = seqpacket_pair();
    let (mut child, log_path) = spawn_gate(child_end.as_raw_fd());
    bound_receives(&harness, 30);

    let ready = ask(
        &harness,
        1,
        LifecycleDirective::Raise {
            instruction: instruction(),
            socket: path.to_path_buf(),
        },
    );
    assert_eq!(ready.payload, Payload::Answer(LifecycleAnswer::GateReady));

    // Exactly what the harness does: tell, drop, reap. No answer is read.
    tell(&harness, 2, LifecycleDirective::Lower);
    drop(harness);

    let status = wait_bounded(&mut child, 30, "the_harness_teardown_sequence");
    assert!(
        status.success(),
        "an orderly leave exits zero, stderr: {}",
        std::fs::read_to_string(&log_path).unwrap_or_default()
    );
    assert!(
        UnixStream::connect(&path).is_err(),
        "and the listener is gone"
    );
    std::fs::remove_file(&path).ok();
    std::fs::remove_file(&log_path).ok();
}

/// **The running binary refuses an unauthorized dial**, not only the in-process
/// tests.
///
/// This is the reference walk against the shipped gate: the test process runs
/// as the same uid the gate does, which is the adversary the boundary exists to
/// exclude, and the instruction's rule permits that uid explicitly so the
/// refusal cannot come from the operator's configuration. The gate adds its own
/// uid to the deny set at raise, and denial wins.
///
/// Before this act the serve loop blocked on the channel and never accepted, so
/// a dial sat in the backlog unjudged: `connect` succeeded and the peer waited
/// forever, indistinguishable from an admitted one. What the test reads is that
/// the connection is *closed on*, which is the contract's refusal by closure.
///
/// Perturbation: drop the listener from the poll set in `serve` and this test
/// fails, the dial hanging unjudged until the read bound expires. Watched under
/// exactly that removal.
#[test]
fn the_running_gate_refuses_an_unauthorized_dial() {
    use std::collections::BTreeSet;
    use std::io::Read;
    use weaver_types::{AccessRule, GateInstruction};

    let path = socket_path("refuses-dial");
    let (harness, child_end) = seqpacket_pair();
    let (mut child, log_path) = spawn_gate(child_end.as_raw_fd());
    bound_receives(&harness, 30);

    // A rule that explicitly permits this uid, so only the deny-by-construction
    // can produce the refusal.
    let mut allowed_uids = BTreeSet::new();
    allowed_uids.insert(nix::unistd::getuid().as_raw());
    let permissive = GateInstruction {
        access_rule: AccessRule {
            allowed_uids,
            allowed_gids: BTreeSet::new(),
            denied_uids: BTreeSet::new(),
        },
    };

    let ready = ask(
        &harness,
        1,
        LifecycleDirective::Raise {
            instruction: permissive,
            socket: path.to_path_buf(),
        },
    );
    assert_eq!(ready.payload, Payload::Answer(LifecycleAnswer::GateReady));

    let mut client = UnixStream::connect(&path).expect("the dial reaches the listener");
    client
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("a read bound");
    let mut answer = Vec::new();
    let read = client.read_to_end(&mut answer);
    assert_eq!(
        read.ok(),
        Some(0),
        "the running gate closes on the agent uid with nothing written, got {answer:?}"
    );

    tell(&harness, 2, LifecycleDirective::Lower);
    drop(harness);
    let status = wait_bounded(
        &mut child,
        30,
        "the_running_gate_refuses_an_unauthorized_dial",
    );
    assert!(
        status.success(),
        "an orderly lower ends the gate with a zero status, got {status:?}"
    );
    std::fs::remove_file(&path).ok();
    std::fs::remove_file(&log_path).ok();
}

/// **A peer dialing in a loop cannot exhaust the gate or starve its channel.**
///
/// Retained connections are bounded, and the accept runs whether or not the
/// connection is kept, so the backlog drains and the poll loop does not spin on
/// a permanently readable listener. What the test reads is that the gate is
/// still answering directives after far more dials than it retains.
///
/// **The dial count sits above the gate's retained bound and below the
/// listener's backlog**, which is what lets every dial be asserted rather than
/// hoped for. Rust's `UnixListener::bind` listens with a backlog of 128, so a
/// hundred dials complete without blocking whether or not the gate accepts any
/// of them. Verified by running this test against a gate with its accept
/// removed: the loop finished immediately, the dials sitting in the backlog.
/// An earlier cut dialled three hundred and swallowed failures with `if let
/// Ok`, so it neither proved its own storm nor failed cleanly, since past the
/// backlog a blocking connect against a gate that stopped accepting waits
/// rather than returning, and the test would hang where it should fail.
///
/// **What this test does not demonstrate, stated because the name suggests
/// otherwise.** Removing the `admitted.len() < RETAINED_LIMIT` guard leaves it
/// passing: what the bound prevents is descriptor exhaustion, which needs a
/// storm near the process's own descriptor limit rather than one just past the
/// bound, and that is a heavier fixture than the property is worth here. Nor
/// does removing the accept fail it, the channel being served before the
/// listener, so a gate that never accepts still answers its directives. What is
/// asserted is the narrower thing the name should be read as: a storm of dials
/// leaves the channel answering. The guard's own ground is stated at its
/// constant.
#[test]
fn a_dial_storm_leaves_the_channel_answering() {
    let path = socket_path("dial-storm");
    let (harness, child_end) = seqpacket_pair();
    let (mut child, log_path) = spawn_gate(child_end.as_raw_fd());
    bound_receives(&harness, 30);

    let ready = ask(
        &harness,
        1,
        LifecycleDirective::Raise {
            instruction: instruction(),
            socket: path.to_path_buf(),
        },
    );
    assert_eq!(ready.payload, Payload::Answer(LifecycleAnswer::GateReady));

    // Well past the gate's retained bound of 64 and well under the listener's
    // backlog of 128. Every dial is asserted: a swallowed failure would leave
    // the storm unproven and the test named for something it did not do. These
    // are refused on identity anyway, this process being the agent uid, which
    // is the ordinary case for a storm.
    for dial in 0..100 {
        let stream = UnixStream::connect(&path)
            .unwrap_or_else(|error| panic!("dial {dial} did not reach the listener: {error}"));
        drop(stream);
    }

    // The channel still answers, which is the property: the loop did not wedge
    // on a readable listener and directives are not starved behind dials.
    let stopped = ask(&harness, 2, LifecycleDirective::Lower);
    assert_eq!(
        stopped.payload,
        Payload::Answer(LifecycleAnswer::GateStopped),
        "the gate still serves its channel after a dial storm"
    );

    drop(harness);
    let status = wait_bounded(&mut child, 30, "a_dial_storm_leaves_the_channel_answering");
    assert!(status.success());
    std::fs::remove_file(&path).ok();
    std::fs::remove_file(&log_path).ok();
}
