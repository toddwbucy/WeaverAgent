//! conforms: spu-descriptor-count-check
//! conforms: spu-dumpable-flag-cleared
//!
//! The process facts of `weaver-spu-Spec` section 2, exercised as a process.
//!
//! These run the built binary rather than calling into it, because what they
//! assert is what the process holds and what the kernel says about it at the
//! moment after entry. A unit test inside the crate can reach neither: the
//! count check is about descriptors this process was handed, and the dumpable
//! flag is read from outside by an observer the process cannot lie to.

use std::fs::File;
use std::os::fd::{AsRawFd, RawFd};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

mod common;
use common::{place_inherited, seqpacket_pair, wait_bounded};

/// Start the binary holding exactly the descriptors named, placed at 3, 4, and
/// upward in the order given.
///
/// Standard error goes to a file rather than a pipe. These tests read what the
/// binary writes there, and a pipe nobody drains until after the wait can fill
/// and wedge the child mid-write, which is the deadlock the loaded suite met.
/// A file has no such buffer and leaves the account on disk for a failure that
/// needs it.
fn spawn_holding(child_ends: Vec<RawFd>) -> (Child, PathBuf) {
    let log_path = std::env::temp_dir().join(format!(
        "weaver-spu-entry-{}-{}.log",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let log = File::create(&log_path).expect("a child log file");
    let mut command = Command::new(env!("CARGO_BIN_EXE_weaver-spu"));
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(log));
    place_inherited(&mut command, &child_ends);
    (command.spawn().expect("the binary starts"), log_path)
}

/// Distinguishes the log files of children spawned within one test process.
static COUNTER: AtomicU32 = AtomicU32::new(0);

fn complaint(log_path: &PathBuf) -> String {
    std::fs::read_to_string(log_path).unwrap_or_default()
}

/// **Entry verifies that it holds exactly two descriptors beyond the standard
/// streams, and refuses to serve if it holds more.**
///
/// A count above two means the harness's fork discipline failed upstream and
/// this process is not the one to continue past it. The check is a count rather
/// than an identification, since this crate cannot know what a stray descriptor
/// refers to, so this test leaks a third socket rather than anything meaningful.
///
/// Perturbation: remove the `held != 2` branch from `adopt` and this test
/// fails, because the process proceeds to serve, meets its closed lifecycle
/// end, and exits clean. The parent's ends are dropped before the wait for
/// exactly that reason: an earlier version held them open, so the perturbed
/// binary blocked in its first read and the property was caught by a hang
/// rather than a failure. Watched under exactly that removal.
#[test]
fn a_leaked_descriptor_refuses_to_serve() {
    let (lifecycle, child_lifecycle) = seqpacket_pair();
    let (decode, child_decode) = seqpacket_pair();
    let (leaked, child_leaked) = seqpacket_pair();

    let (mut child, log_path) = spawn_holding(vec![
        child_lifecycle.as_raw_fd(),
        child_decode.as_raw_fd(),
        child_leaked.as_raw_fd(),
    ]);
    drop((lifecycle, decode, leaked));
    let status = wait_bounded(&mut child, 30, "a_leaked_descriptor_refuses_to_serve");

    assert!(
        !status.success(),
        "a process holding a leaked descriptor refuses to serve"
    );
    let complaint = complaint(&log_path);
    assert!(
        complaint.contains("held_beyond_standard_streams\":3"),
        "the refusal reports the count it found, got {complaint}"
    );
    std::fs::remove_file(&log_path).ok();
}

/// The clean case: exactly two descriptors, so the count check passes and the
/// process serves. Without this, the test above would pass against a binary
/// that refused unconditionally.
#[test]
fn exactly_two_descriptors_serve() {
    let (lifecycle, child_lifecycle) = seqpacket_pair();
    let (_decode, child_decode) = seqpacket_pair();

    let (mut child, log_path) =
        spawn_holding(vec![child_lifecycle.as_raw_fd(), child_decode.as_raw_fd()]);

    // A served process blocks on its first read rather than exiting. Closing
    // the parent's end is what ends it, and the clean exit below is the whole
    // proof it got past entry: an immediate liveness poll would race the exec
    // and assert nothing either way.
    drop(lifecycle);
    let status = wait_bounded(&mut child, 30, "exactly_two_descriptors_serve");
    assert!(
        status.success(),
        "a process holding exactly two descriptors serves and exits clean, stderr: {}",
        complaint(&log_path)
    );
    std::fs::remove_file(&log_path).ok();
}

/// **The dumpable flag is cleared at entry.**
///
/// The adversary is a same-uid process attaching by `ptrace` or reaching
/// through `/proc/[pid]/fd` into the process holding the weights and both
/// channels. The mechanism is a set and not a check.
///
/// **The observation is the kernel's rather than the process's.** Clearing the
/// flag transfers ownership of `/proc/[pid]` to `root:root`, so the parent reads
/// the fact from the filesystem and the child has no way to report it falsely.
/// A test that asked the child to attest would pass against a child that lied.
///
/// Perturbation: remove the `clear_dumpable` call from `adopt` and this test
/// fails, because `/proc/[pid]/fd` stays owned by the invoking uid. Watched
/// under exactly that removal.
#[test]
fn the_dumpable_flag_is_clear_after_entry() {
    use std::os::unix::fs::MetadataExt;

    // Under uid 0 the observation is vacuous: /proc/[pid]/fd is root-owned
    // whether or not the flag was cleared, so the test would pass with the
    // clear removed. The suite skips loudly rather than attesting to nothing.
    if nix::unistd::geteuid().is_root() {
        eprintln!("SKIP the_dumpable_flag_is_clear_after_entry: root owns /proc either way");
        return;
    }

    let (lifecycle, child_lifecycle) = seqpacket_pair();
    let (_decode, child_decode) = seqpacket_pair();

    let (mut child, log_path) =
        spawn_holding(vec![child_lifecycle.as_raw_fd(), child_decode.as_raw_fd()]);
    let pid = child.id();

    // Entry runs and then blocks on the first read. Poll rather than sleep a
    // fixed span, so the test is not a race on a loaded machine.
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

    drop(lifecycle);
    wait_bounded(&mut child, 30, "the_dumpable_flag_is_clear_after_entry");
    std::fs::remove_file(&log_path).ok();

    assert_eq!(
        owner,
        Some(0),
        "a cleared dumpable flag makes /proc/{pid}/fd owned by root, found {owner:?}"
    );
}

/// **A failed exec still reaches the parent.**
///
/// `place_inherited` marks the descriptors above its range close-on-exec
/// rather than closing them, because Rust reports a failed exec through a
/// close-on-exec pipe it opens above that range. Closing it makes the parent
/// read EOF and conclude the spawn succeeded, which would let this suite spawn
/// a missing binary and test nothing.
///
/// Perturbation: pass `0` instead of `CLOSE_RANGE_CLOEXEC` in `place_inherited`
/// and this test fails, because `spawn` returns `Ok` for a path that does not
/// exist. Watched under exactly that change.
#[test]
fn a_failed_exec_is_reported_to_the_parent() {
    let (_parent, child_end) = seqpacket_pair();
    let mut command = Command::new("/nonexistent/weaver-spu-that-is-not-there");
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    place_inherited(&mut command, &[child_end.as_raw_fd()]);
    assert!(
        command.spawn().is_err(),
        "a missing executable is an error, never a spawn the parent believes"
    );
}
