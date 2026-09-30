//! conforms: gate-agent-uid-denied-by-construction
//!
//! The scaffolding the gate's integration suites share.
//!
//! It lives once because `scratch` existed three times and `instruction` twice,
//! already disagreeing on their directory prefixes, which is how a fixture fix
//! lands in one copy and a later suite inherits the stale-socket flake the
//! pre-clean exists to prevent.
//!
//! **This is the gate's own copy, and a fourth copy of `weaver-spu`'s
//! `tests/common` is not what it is.** The two crates duplicate
//! `seqpacket_pair`, a bounded wait, and a descriptor placement, and sharing
//! those across crates means a dev-only support crate, which is an eighth
//! member of a set the corpus fixes at seven and a change the graph would see.
//! That is a decision for the seats rather than a patch, and it is named in the
//! PR that carries this file.

#![allow(dead_code)]

use std::collections::BTreeSet;
use std::os::fd::{AsRawFd, OwnedFd, RawFd};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus};
use std::time::{Duration, Instant};

use nix::libc;
use nix::sys::socket::{AddressFamily, MsgFlags, SockFlag, SockType, recv, send, socketpair};
use weaver_types::{
    AccessRule, ExchangeId, GateInstruction, LifecycleDirective, Opener, OrganEnvelope, Payload,
    Position,
};

pub fn seqpacket_pair() -> (OwnedFd, OwnedFd) {
    socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .expect("a socketpair")
}

/// A socket path inside a directory of its own, the directory and everything
/// in it removed when the test ends, pass or fail: the guard drops on the
/// unwind a failed assertion takes as on a clean return (#690 item C2.9). It
/// reads as the socket's path, and a caller handing the path to a directive
/// holds the guard for as long as the socket is wanted.
pub struct Scratch {
    dir: PathBuf,
    file: PathBuf,
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

impl std::ops::Deref for Scratch {
    type Target = std::path::Path;
    fn deref(&self) -> &std::path::Path {
        &self.file
    }
}

impl AsRef<std::path::Path> for Scratch {
    fn as_ref(&self) -> &std::path::Path {
        &self.file
    }
}

/// A file under the temp directory, removed when the test ends, pass or fail,
/// as [`Scratch`]'s directory is (#690 item C2.9).
pub struct ScratchFile(pub PathBuf);

impl Drop for ScratchFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

impl std::ops::Deref for ScratchFile {
    type Target = std::path::Path;
    fn deref(&self) -> &std::path::Path {
        &self.0
    }
}

impl AsRef<std::path::Path> for ScratchFile {
    fn as_ref(&self) -> &std::path::Path {
        &self.0
    }
}

/// A scratch socket path under a directory named for the suite and the test,
/// pre-cleaned so a previous run's leftover cannot make a raise refuse.
pub fn scratch(suite: &str, name: &str) -> Scratch {
    let dir =
        std::env::temp_dir().join(format!("weaver-gate-{suite}-{}-{name}", std::process::id()));
    // **Created under the umask lock, because the umask is the process's.**
    // A sibling test inside `Umask::deny_others` has every other-bit masked
    // off, and a `create_dir_all` interleaving with it lands a directory at a
    // mode nobody elected - the same race the guard exists to close, reaching
    // a file the guard was never about. The lock is reentrant per thread, so
    // a caller already holding it may call this.
    weaver_gate::hook::with_umask_held(|| {
        std::fs::create_dir_all(&dir).expect("a scratch dir");
    });
    let path = dir.join("gate.sock");
    std::fs::remove_file(&path).ok();
    Scratch { dir, file: path }
}

/// An instruction with an empty rule: nothing is permitted, and the raise adds
/// this process's own uid to the deny set whatever the rule says.
pub fn instruction() -> GateInstruction {
    GateInstruction {
        access_rule: AccessRule {
            allowed_uids: BTreeSet::new(),
            allowed_gids: BTreeSet::new(),
            denied_uids: BTreeSet::new(),
        },
    }
}

/// An instruction whose rule permits this very uid, which is the operator
/// mistake the deny-by-construction has to survive.
pub fn permissive_instruction() -> GateInstruction {
    let mut allowed_uids = BTreeSet::new();
    allowed_uids.insert(nix::unistd::getuid().as_raw());
    let mut instruction = instruction();
    instruction.access_rule.allowed_uids = allowed_uids;
    instruction
}

/// Place the child's ends at 3 upward and mark everything above close-on-exec.
///
/// Marked, never closed: Rust reports a failed exec to the parent through a
/// close-on-exec pipe it opens above this range, and closing it makes the
/// parent read EOF and conclude the spawn succeeded. Nothing here allocates,
/// since it runs in the child of a multithreaded test process.
pub fn place_inherited(command: &mut Command, child_ends: &[RawFd]) {
    let mut sources = [0 as RawFd; 8];
    let count = child_ends.len();
    assert!(
        count <= sources.len(),
        "at most {} descriptors",
        sources.len()
    );
    sources[..count].copy_from_slice(child_ends);
    unsafe {
        command.pre_exec(move || {
            let mut lifted = [0 as RawFd; 8];
            for (slot, source) in lifted[..count].iter_mut().zip(&sources[..count]) {
                let high = libc::fcntl(*source, libc::F_DUPFD, 32);
                if high < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                *slot = high;
            }
            for (offset, high) in lifted[..count].iter().enumerate() {
                if libc::dup2(*high, 3 + offset as RawFd) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
            }
            // `F_DUPFD` clears close-on-exec on the copy, so these would survive
            // the exec and inflate the count the child checks.
            for high in &lifted[..count] {
                libc::close(*high);
            }
            let first = 3 + count as libc::c_uint;
            if libc::syscall(
                libc::SYS_close_range,
                first,
                libc::c_uint::MAX,
                libc::CLOSE_RANGE_CLOEXEC,
            ) != 0
            {
                // Older kernels lack the flag; set it per descriptor instead,
                // on top of what each already carries.
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

/// Wait for a child to exit, bounded, killing and reaping it on expiry.
pub fn wait_bounded(child: &mut Child, seconds: u64, what: &str) -> ExitStatus {
    let deadline = Instant::now() + Duration::from_secs(seconds);
    loop {
        match child.try_wait().expect("a wait succeeds") {
            Some(status) => return status,
            None if Instant::now() > deadline => {
                child.kill().ok();
                child.wait().ok();
                panic!("{what}: the child did not exit within {seconds}s");
            }
            None => std::thread::sleep(Duration::from_millis(25)),
        }
    }
}

/// Bound every receive on this end, so a gate that never answers is a failure
/// naming the exchange rather than a hang.
pub fn bound_receives(end: &OwnedFd, seconds: i64) {
    nix::sys::socket::setsockopt(
        end,
        nix::sys::socket::sockopt::ReceiveTimeout,
        &nix::sys::time::TimeVal::new(seconds, 0),
    )
    .expect("the receive timeout sets");
}

/// Send one directive as the harness and read the one answer.
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
    assert!(
        read > 0,
        "the gate closed without answering ordinal {ordinal}"
    );
    buffer.truncate(read);
    serde_json::from_slice(&buffer).expect("the answer is an envelope")
}

/// Send a directive and do not read an answer, which is what the harness's
/// leave does.
pub fn tell(end: &OwnedFd, ordinal: u64, directive: LifecycleDirective) {
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
}
