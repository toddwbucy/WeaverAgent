//! The scaffolding the three integration suites share: the socket pair, the
//! descriptor placement, and the one-directive exchange. It lives once because
//! it existed three times, and the third copy had dropped the load-bearing
//! comment on why the lifted descriptors must be closed. A descriptor-protocol
//! fix applied to one copy and missed by the others is exactly the divergence a
//! shared module prevents.
//!
//! Each test binary compiles its own copy and uses its own subset, so the
//! unused-code lint is silenced here once rather than per-consumer: a helper
//! unused by one suite is not dead, it is another suite's.
#![allow(dead_code)]

use std::os::fd::{AsRawFd, OwnedFd, RawFd};
use std::os::unix::process::CommandExt;
use std::process::{Command, ExitStatus};
use std::time::{Duration, Instant};

use nix::libc;
use nix::sys::socket::{AddressFamily, MsgFlags, SockFlag, SockType, recv, send, socketpair};
use weaver_types::{ExchangeId, LifecycleDirective, Opener, OrganEnvelope, Payload, Position};

pub fn seqpacket_pair() -> (OwnedFd, OwnedFd) {
    socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .expect("a socketpair")
}

/// Arrange for the child to hold exactly the given descriptors at 3, 4, and
/// upward, in order, and nothing else.
///
/// The placement runs in two passes because a source can already sit on a
/// number about to be written, and writing targets directly would clobber a
/// source not yet placed. The first pass lifts every source above the target
/// range with `F_DUPFD`, the second writes the targets with `dup2`, which
/// clears close-on-exec on what it writes so exactly these survive the exec.
///
/// Nothing in the closure allocates: it runs in the child of a multithreaded
/// test process, where another thread may hold the allocator lock at the
/// moment of the fork.
pub fn place_inherited(command: &mut Command, child_ends: &[RawFd]) {
    let mut sources = [0 as RawFd; 8];
    let count = child_ends.len();
    assert!(
        count <= sources.len(),
        "place_inherited takes at most {} descriptors, got {count}",
        sources.len()
    );
    sources[..count].copy_from_slice(child_ends);
    unsafe {
        command.pre_exec(move || {
            // Pass one: lift every source clear of the target range.
            let mut lifted = [0 as RawFd; 8];
            for (slot, source) in lifted[..count].iter_mut().zip(&sources[..count]) {
                let high = libc::fcntl(*source, libc::F_DUPFD, 32);
                if high < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                *slot = high;
            }
            // Pass two: write the targets, which also clears close-on-exec.
            for (offset, high) in lifted[..count].iter().enumerate() {
                if libc::dup2(*high, 3 + offset as RawFd) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
            }
            // `F_DUPFD` CLEARS close-on-exec on the copy rather than
            // inheriting it, so the lifted copies would survive the exec and
            // inflate the very count the child checks. Closing them is
            // required, not tidiness.
            for high in &lifted[..count] {
                libc::close(*high);
            }
            // Mark everything above the placed range close-on-exec, which is
            // the fork discipline the harness performs in production. The test
            // process is not a clean parent: an in-process CUDA or llama.cpp
            // load in a sibling test leaves driver descriptors open without
            // close-on-exec, the child inherits them, and its own count check
            // then refuses to serve - correctly. Found live: a child spawned
            // after a sibling's in-process load reported 24 descriptors held.
            //
            // **Marked, never closed.** Rust reports a failed exec to the
            // parent through a close-on-exec pipe it opened above this range,
            // and closing that pipe makes the parent read EOF and conclude the
            // spawn succeeded. Measured: with the descriptors closed, spawning
            // a nonexistent binary returns Ok and the child dies on a signal.
            // Marking leaves the pipe alive until exec, which is exactly when
            // it should go.
            let first = 3 + count as libc::c_uint;
            if libc::syscall(
                libc::SYS_close_range,
                first,
                libc::c_uint::MAX,
                libc::CLOSE_RANGE_CLOEXEC,
            ) != 0
            {
                // The flag is Linux 5.11+; this workshop is far past it, but a
                // fallback keeps the helper honest elsewhere. It runs to the
                // process's own descriptor limit rather than a round number,
                // because a descriptor above that number is what the child
                // would then count and refuse on, and it sets the flag on top
                // of what each descriptor already carries rather than
                // replacing it.
                let mut limit = libc::rlimit {
                    rlim_cur: 0,
                    rlim_max: 0,
                };
                let ceiling = if libc::getrlimit(libc::RLIMIT_NOFILE, &mut limit) == 0 {
                    limit.rlim_cur as RawFd
                } else {
                    4096
                };
                for fd in first as RawFd..ceiling {
                    let flags = libc::fcntl(fd, libc::F_GETFD);
                    if flags >= 0 {
                        libc::fcntl(fd, libc::F_SETFD, flags | libc::FD_CLOEXEC);
                    }
                }
            }
            Ok(())
        });
    }
}

/// Bound every receive on this end, so a child that wedges turns into a test
/// failure naming the stage rather than a hang the harness cannot end.
pub fn bound_receives(end: &OwnedFd, seconds: i64) {
    use nix::sys::socket::sockopt::ReceiveTimeout;
    use nix::sys::time::TimeVal;
    nix::sys::socket::setsockopt(end, ReceiveTimeout, &TimeVal::new(seconds, 0))
        .expect("the receive timeout sets");
}

/// Wait for a child to exit, bounded, killing and reaping it on expiry.
///
/// An unbounded wait on a child that never exits is a hang the harness cannot
/// end, and a hang reports nothing about which stage wedged.
pub fn wait_bounded(child: &mut std::process::Child, seconds: u64, what: &str) -> ExitStatus {
    let deadline = Instant::now() + Duration::from_secs(seconds);
    loop {
        match child.try_wait().expect("a wait succeeds") {
            Some(status) => return status,
            None if Instant::now() > deadline => {
                child.kill().ok();
                child.wait().ok();
                panic!("{what}: the child did not exit within {seconds}s");
            }
            None => std::thread::sleep(Duration::from_millis(50)),
        }
    }
}

/// Send one directive as the harness and read the one answer, panicking with
/// the stage name if the child never answers within the bound set by
/// [`bound_receives`].
pub fn ask(end: &OwnedFd, ordinal: u64, directive: LifecycleDirective) -> OrganEnvelope {
    let envelope = OrganEnvelope {
        exchange: ExchangeId {
            opener: Opener::Harness,
            ordinal,
        },
        position: Position::Open,
        payload: Payload::Directive(directive),
    };
    let body = serde_json::to_vec(&envelope).expect("an envelope encodes");
    send(end.as_raw_fd(), &body, MsgFlags::empty()).expect("the directive is sent");

    let mut buffer = vec![0u8; 64 * 1024];
    let read = recv(end.as_raw_fd(), &mut buffer, MsgFlags::empty())
        .unwrap_or_else(|errno| panic!("no answer to ordinal {ordinal}: {errno}"));
    buffer.truncate(read);
    serde_json::from_slice(&buffer).expect("the answer is an envelope")
}

/// The marker a masked child carries: the name of the test it runs. A test
/// binary sets it for its own child, so it is not a fixture, and it stays
/// outside the `WEAVER_` names `weaver-spu-Spec` section 1.2 enumerates.
pub const MASKED_CHILD: &str = "SPU_MASKED_CHILD";

/// The prefix of a line a masked child reports to its parent on stdout.
pub const MASKED_RESULT: &str = "MASKED-RESULT ";

/// **Whether this test process may take `devices`**, read from its own
/// `CUDA_VISIBLE_DEVICES`, the box's lease rule. A set and empty mask hides
/// the cards, so nothing is leased. A list leases its ordinals, read in PCI
/// bus order. No mask leases every device the driver counts. `Err` carries
/// the reason, for a skip line.
///
/// A device test admits under the device pair amendment PAIR-1 mask, which
/// one process can hold for one binding only, so a test of several bindings
/// runs each in its own child ([`run_masked`]) and the parent reads the lease
/// here rather than touching a device.
pub fn lease(devices: &[u32]) -> Result<(), String> {
    match std::env::var("CUDA_VISIBLE_DEVICES") {
        Ok(mask) if mask.trim().is_empty() => Err("the cards are hidden".into()),
        Ok(mask) => {
            if std::env::var("CUDA_DEVICE_ORDER").as_deref() != Ok("PCI_BUS_ID") {
                return Err(format!(
                    "CUDA_VISIBLE_DEVICES={mask} without CUDA_DEVICE_ORDER=PCI_BUS_ID \
                     names no ordinals"
                ));
            }
            let held: Vec<&str> = mask.split(',').map(str::trim).collect();
            match devices
                .iter()
                .find(|d| !held.contains(&d.to_string().as_str()))
            {
                Some(missing) => Err(format!("ordinal {missing} is not leased ({mask})")),
                None => Ok(()),
            }
        }
        Err(_) => {
            let count = cudarc_device_count();
            match devices.iter().find(|d| **d as usize >= count) {
                Some(missing) => Err(format!(
                    "ordinal {missing} is past the {count} devices the driver counts"
                )),
                None => Ok(()),
            }
        }
    }
}

/// The driver's device count with no context created, zero where it cannot
/// answer.
#[cfg(feature = "cuda")]
fn cudarc_device_count() -> usize {
    cudarc::driver::CudaContext::device_count().map_or(0, |n| n.max(0) as usize)
}

#[cfg(not(feature = "cuda"))]
fn cudarc_device_count() -> usize {
    0
}

/// The binding this process holds as the masked child for test `name`, the
/// ordinals of its mask in order, or `None` when it is not that child.
pub fn masked_child(name: &str) -> Option<Vec<weaver_types::DeviceOrdinal>> {
    if std::env::var(MASKED_CHILD).as_deref() != Ok(name) {
        return None;
    }
    let mask = std::env::var("CUDA_VISIBLE_DEVICES").expect("a masked child holds a mask");
    Some(
        mask.split(',')
            .map(|ordinal| weaver_types::DeviceOrdinal(ordinal.parse().expect("a decimal ordinal")))
            .collect(),
    )
}

/// Report a value from a masked child to its parent.
pub fn report(value: &serde_json::Value) {
    println!("{MASKED_RESULT}{value}");
}

/// The values a masked child reported, in order, from its stdout. Under
/// `--nocapture` the harness prints `test <name> ... ` and the test's first
/// line follows on the same line, so a report is found anywhere in a line,
/// not only at its start: the first live run of the masked children, on
/// 2026-11-08, failed a passing child on exactly that.
pub fn reported(stdout: &str) -> Vec<serde_json::Value> {
    stdout
        .lines()
        .filter_map(|line| line.split_once(MASKED_RESULT).map(|(_, json)| json))
        .map(|json| serde_json::from_str(json).expect("a reported value"))
        .collect()
}

/// **Run test `name` of this binary in its own process under the PAIR-1 mask
/// for `devices`**, the way the harness launches the SPU, and answer the
/// values it reported. A child that fails, or that the filter did not reach,
/// fails the parent with its output. The child's stderr is passed through,
/// so its emissions read in the parent's output.
pub fn run_masked(name: &str, devices: &[u32]) -> Vec<serde_json::Value> {
    let mask = devices
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let output = Command::new(std::env::current_exe().expect("this test binary"))
        .args([
            "--exact",
            name,
            "--include-ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("CUDA_DEVICE_ORDER", "PCI_BUS_ID")
        .env("CUDA_VISIBLE_DEVICES", &mask)
        .env(MASKED_CHILD, name)
        .output()
        .expect("the masked child starts");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "masked child {name} on {mask} failed:\n{stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    eprint!("{}", String::from_utf8_lossy(&output.stderr));
    let values = reported(&stdout);
    // `--nocapture` interleaves the child's own lines with the harness's
    // per-test line, so the summary is what says the one test ran and passed.
    assert!(
        stdout.contains("test result: ok. 1 passed"),
        "masked child {name} did not run:\n{stdout}"
    );
    // A child returns early unless it is the child named, so one that ran
    // and reported nothing never reached its body.
    assert!(
        !values.is_empty(),
        "masked child {name} reported nothing:\n{stdout}"
    );
    values
}
