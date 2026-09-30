//! conforms: harness-truncation-is-a-fault
//! conforms: harness-one-write-is-one-read
//! conforms: harness-atomic-cloexec-at-creation
//! conforms: harness-trace-fd-cloexec-at-receive
//! conforms: harness-organ-ends-from-descriptor-three
//! conforms: harness-dumpable-flag-cleared
//!
//! The threat walks of `weaver-harness-Spec` section 8, made executable, and
//! the channel perturbations beside them. Each names its perturbation: the
//! mutation under which it was watched to fail.
#![cfg(target_os = "linux")]

use std::os::fd::{AsFd, AsRawFd, FromRawFd, OwnedFd};

use weaver_harness::{ChannelFault, FIRST_ORGAN_DESCRIPTOR, Harness, OrganBinaries, OrganChannel};
use weaver_types::{
    AgentName, ExchangeId, LifecycleDirective, MAX_ENVELOPE_BYTES, Opener, OrganEnvelope, Payload,
    Position,
};

/// A path under the temp directory, removed when the test ends, pass or
/// fail: the guard drops on the unwind a failed assertion takes as on a clean
/// return (#690 item C2.9).
struct Scratch(std::path::PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        match std::fs::symlink_metadata(&self.0) {
            Ok(meta) if meta.is_dir() => drop(std::fs::remove_dir_all(&self.0)),
            Ok(_) => drop(std::fs::remove_file(&self.0)),
            Err(_) => {}
        }
    }
}

fn directive(ordinal: u64, agent: &str) -> OrganEnvelope {
    OrganEnvelope {
        exchange: ExchangeId {
            opener: Opener::Admin,
            ordinal,
        },
        position: Position::Open,
        payload: Payload::Directive(LifecycleDirective::Load {
            agent: AgentName(agent.to_string()),
        }),
    }
}

fn cloexec_set(fd: std::os::fd::BorrowedFd<'_>) -> bool {
    // SAFETY: reading F_GETFD on a descriptor this process owns.
    let flags = unsafe { nix::libc::fcntl(fd.as_raw_fd(), nix::libc::F_GETFD) };
    flags != -1 && (flags & nix::libc::FD_CLOEXEC) != 0
}

/// **The second walk: a tool inherits a channel end.** Both ends of every
/// created pair carry close-on-exec from the creating act, by `SOCK_CLOEXEC`
/// in the `socketpair` call rather than by a later `fcntl` - no window between
/// creation and flag.
///
/// Perturbation: drop `SOCK_CLOEXEC` from the `socketpair` call and both
/// assertions fail immediately; with a later `fcntl` they pass while the
/// window stands, which is why the walk below forks to check what crosses.
#[test]
fn pairs_are_close_on_exec_from_creation() {
    let (near, far) = OrganChannel::pair().expect("pair");
    assert!(
        cloexec_set(near.as_fd()),
        "the harness end is flagged at creation"
    );
    assert!(
        cloexec_set(far.as_fd()),
        "the child end is flagged at creation"
    );
}

/// **The second walk, executed.** A forked child that does not run the handoff
/// inherits no channel end: every end this crate creates is flagged, so an
/// exec closes it. The child reports which of the parent's descriptors it can
/// still see after an exec-equivalent flag check.
///
/// Perturbation: dropping the atomic flag to a later `fcntl` leaves a window a
/// fork can land in, which this test's child observes as an inherited end.
#[test]
fn tool_fork_inherits_no_channel_end() {
    let (near, _far) = OrganChannel::pair().expect("pair");
    let watched = near.as_fd().as_raw_fd();
    let (report_r, report_w) = nix::unistd::pipe().expect("pipe");
    // SAFETY: the child writes one byte and _exits; no allocation, no unwind.
    match unsafe { nix::unistd::fork() }.expect("fork") {
        nix::unistd::ForkResult::Child => {
            let flags = unsafe { nix::libc::fcntl(watched, nix::libc::F_GETFD) };
            let inherited_and_clear = if flags != -1 && (flags & nix::libc::FD_CLOEXEC) == 0 {
                1u8
            } else {
                0u8
            };
            let _ = nix::unistd::write(report_w.as_fd(), &[inherited_and_clear]);
            unsafe { nix::libc::_exit(0) };
        }
        nix::unistd::ForkResult::Parent { child } => {
            drop(report_w);
            let mut byte = [0u8; 1];
            let _ = nix::unistd::read(report_r.as_fd(), &mut byte);
            let _ = nix::sys::wait::waitpid(child, None);
            assert_eq!(
                byte[0], 0,
                "no end crosses a fork without its close-on-exec flag"
            );
        }
    }
}

/// **One write is one read**, the boundary property the socket type was
/// elected to buy: two envelopes written back to back arrive as two reads of
/// exactly one envelope each.
///
/// Perturbation: change `SOCK_SEQPACKET` to `SOCK_STREAM` at the `socketpair`
/// call and the first read returns both envelopes concatenated, failing the
/// decode. **Two messages are what make the watch reachable** - a single small
/// envelope crosses a stream socket whole, so a one-message test would pass
/// under the substitution and pin nothing.
#[test]
fn one_write_is_one_read() {
    let (near, far) = OrganChannel::pair().expect("pair");
    let peer = far.into_channel();
    peer.send(&directive(1, "alpha")).expect("first write");
    peer.send(&directive(2, "beta")).expect("second write");
    let first = near.recv().expect("first read decodes on its own");
    let second = near.recv().expect("second read decodes on its own");
    assert_eq!(first.exchange.ordinal, 1);
    assert_eq!(second.exchange.ordinal, 2);
}

/// **Truncation is a fault**, never a message: a read that returns with
/// `MSG_TRUNC` set carries a prefix the kernel shortened and discarded the
/// rest of.
///
/// Perturbation: remove the `MSG_TRUNC` check from `recv` and the shortened
/// octets reach the decoder, which either fails opaquely or - for a payload
/// that happens to remain valid JSON - decodes a silently different directive.
#[test]
fn truncation_is_a_fault() {
    let (near, far) = OrganChannel::pair().expect("pair");
    let oversized = vec![b'x'; MAX_ENVELOPE_BYTES + 4096];
    // SAFETY: a raw write is how the test produces a message larger than the
    // receiver's buffer; the crate's own send refuses to author one.
    let written = unsafe {
        nix::libc::send(
            far.as_fd().as_raw_fd(),
            oversized.as_ptr().cast(),
            oversized.len(),
            0,
        )
    };
    if written < 0 {
        // **The branch under test was not reached**, and a silent return here
        // would let the stated perturbation go undetected on this machine.
        // The kernel refused the oversized datagram outright, which is the
        // same boundary holding one layer down, so this is a skip and says so.
        eprintln!(
            "SKIP truncation_is_a_fault: the kernel refused a {} byte datagram \
             (errno {}), so the MSG_TRUNC branch was not exercised here",
            oversized.len(),
            std::io::Error::last_os_error()
        );
        return;
    }
    match near.recv() {
        Err(ChannelFault::Truncated { bound }) => assert_eq!(bound, MAX_ENVELOPE_BYTES),
        other => panic!("a truncated read must be a fault, got {other:?}"),
    }
}

/// **The first walk: a rogue elected tool reaches for the trace.** The trace
/// descriptor enters this crate at exactly one call, and that call asks for
/// `MSG_CMSG_CLOEXEC` in the receive itself, so the handle arrives flagged and
/// no tool subprocess inherits a writable handle to the record.
///
/// Perturbation: drop `MSG_CMSG_CLOEXEC` from the receive and the received
/// descriptor arrives clear, which this assertion catches directly. The
/// state channel's end rides beside the sink since the ruling of
/// 2026-08-26 and is the second capability the walk names, so both are
/// received here and both must carry the flag.
#[test]
fn received_descriptors_are_close_on_exec() {
    let (near, far) = OrganChannel::pair().expect("pair");
    let peer = far.into_channel();
    let sink_path = Scratch(std::env::temp_dir().join(format!(
        "weaver-harness-walk-one-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    )));
    let sink = std::fs::File::create(&sink_path.0).expect("sink");
    let sink = OwnedFd::from(sink);
    let (state_end, _member_end) =
        std::os::unix::net::UnixStream::pair().expect("the member's pair");
    let state_end = OwnedFd::from(state_end);
    peer.send_with_descriptors(&directive(7, "alpha"), sink.as_fd(), state_end.as_fd())
        .expect("send with descriptors");
    let (envelope, received_sink, received_end) = near.recv_with_descriptor().expect("receive");
    assert_eq!(envelope.exchange.ordinal, 7);
    let received_sink = received_sink.expect("the sink crossed first");
    let received_end = received_end.expect("the state end crossed second");
    assert!(
        cloexec_set(received_sink.as_fd()),
        "the sink arrives close-on-exec, so no tool fork inherits the record"
    );
    assert!(
        cloexec_set(received_end.as_fd()),
        "the state end arrives close-on-exec, so no tool fork seats itself on the member's seam"
    );
}

/// Relocate a descriptor above the range `place_child_ends` overwrites.
///
/// `F_DUPFD_CLOEXEC` answers the lowest free descriptor at or above the floor,
/// so the precondition holds however the surrounding tests have arranged the
/// descriptor table. The original is dropped, closing the low number, which is
/// what leaves the placement's targets free for the placement to take.
fn above_the_targets(fd: OwnedFd) -> OwnedFd {
    // Comfortably clear of 3 and 4, and of the pairs this test already holds.
    const FLOOR: std::os::fd::RawFd = 16;
    if fd.as_raw_fd() >= FLOOR {
        return fd;
    }
    let raw = nix::fcntl::fcntl(fd.as_fd(), nix::fcntl::FcntlArg::F_DUPFD_CLOEXEC(FLOOR))
        .expect("relocate the report end above the placement's targets");
    // SAFETY: `F_DUPFD_CLOEXEC` answered a fresh descriptor this process owns,
    // and `fd` is dropped on return, so the low number is closed exactly once
    // and the new number has exactly one owner.
    unsafe { OwnedFd::from_raw_fd(raw) }
}

/// **The second walk's placement half:** a child finds the ends it is owed at
/// descriptor 3 upward, in the channels' own order, with the flag cleared.
///
/// **This test does not reach the `dup2` no-op corner**, and the honest
/// statement is worth more than a watch that cannot fail: producing it needs
/// a `ChildEnd` whose own descriptor number is already 3, which this crate's
/// private field does not admit from a test. Making the clear conditional
/// leaves this test passing. That is why `weaver-harness-Spec` section 2.2
/// tags the unconditional clear `review` and says so - review's by election
/// and not by impossibility, the shape both seats ran to find the corner.
#[test]
fn child_ends_land_from_descriptor_three() {
    let (_near_a, far_a) = OrganChannel::pair().expect("pair a");
    let (_near_b, far_b) = OrganChannel::pair().expect("pair b");
    let (report_r, report_w) = nix::unistd::pipe().expect("pipe");
    // **The report pipe must sit above the range the placement overwrites.**
    // `place_child_ends` targets 3 and 4, and `dup2` closes whatever occupies
    // its target: a report end at either number would be closed by the very
    // placement this test measures, and the parent would read nothing and
    // report a flag problem that did not occur.
    //
    // **Moved above the range rather than asserted to be above it.** A
    // descriptor is the lowest free number, so the pairs above usually take
    // the low ones, and usually is not a guarantee: a sibling test in this
    // binary closing a descriptor between the pair calls and this one frees a
    // low number for the pipe to take. Asserting the hope made this test fail
    // about once in twenty-five full-file runs while passing forty times out
    // of forty alone, which is a flake reporting a scheduling accident as a
    // placement defect.
    let report_r = above_the_targets(report_r);
    let report_w = above_the_targets(report_w);
    // SAFETY: the child runs the handoff and then reports; async-signal-safe
    // calls only, then _exit.
    match unsafe { nix::unistd::fork() }.expect("fork") {
        nix::unistd::ForkResult::Child => {
            let placed = unsafe { weaver_harness::place_child_ends(&[&far_a, &far_b]) };
            let mut ok = placed.is_ok() as u8;
            for offset in 0..2 {
                let fd = FIRST_ORGAN_DESCRIPTOR + offset;
                let flags = unsafe { nix::libc::fcntl(fd, nix::libc::F_GETFD) };
                if flags == -1 || (flags & nix::libc::FD_CLOEXEC) != 0 {
                    ok = 0;
                }
            }
            let _ = nix::unistd::write(report_w.as_fd(), &[ok]);
            unsafe { nix::libc::_exit(0) };
        }
        nix::unistd::ForkResult::Parent { child } => {
            drop(report_w);
            let mut byte = [0u8; 1];
            let _ = nix::unistd::read(report_r.as_fd(), &mut byte);
            let _ = nix::sys::wait::waitpid(child, None);
            assert_eq!(
                byte[0], 1,
                "both ends sit at 3 and 4 with the flag cleared unconditionally"
            );
        }
    }
}

/// **The third walk: a same-uid process attaches.** The dumpable flag is
/// cleared at adoption, which reparents the proc entries to root and refuses
/// the attach.
///
/// Perturbation: remove the `prctl` from `adopt` and the flag reads 1, which
/// this assertion catches. Run in a forked child so the cleared flag does not
/// outlive the test into the rest of the suite.
#[test]
fn adoption_clears_the_dumpable_flag() {
    let (report_r, report_w) = nix::unistd::pipe().expect("pipe");
    // SAFETY: the child adopts, reads the flag, reports, and _exits.
    // The directory is made and guarded here, before the fork: the child
    // `_exit`s and drops nothing, so the parent's guard is what removes it,
    // pass or fail. Fresh per run by the clock, so a recycled pid never
    // meets a stale socket.
    let dir = Scratch(std::env::temp_dir().join(format!(
        "weaver-dumpable-{}-{:?}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    )));
    std::fs::create_dir_all(&dir.0).expect("scratch");
    match unsafe { nix::unistd::fork() }.expect("fork") {
        nix::unistd::ForkResult::Child => {
            let listener = weaver_harness::bind_coordination(&dir.0.join("c.sock")).expect("bind");
            let constructed = Harness::listen(
                listener,
                OrganBinaries {
                    classify: None,
                    spu: "/nonexistent/spu".into(),
                    gate: "/nonexistent/gate".into(),
                },
                weaver_harness::OrganParameters::default(),
            );
            let dumpable = unsafe { nix::libc::prctl(nix::libc::PR_GET_DUMPABLE, 0, 0, 0, 0) };
            let ok = (constructed.is_ok() && dumpable == 0) as u8;
            let _ = nix::unistd::write(report_w.as_fd(), &[ok]);
            unsafe { nix::libc::_exit(0) };
        }
        nix::unistd::ForkResult::Parent { child } => {
            drop(report_w);
            let mut byte = [0u8; 1];
            let _ = nix::unistd::read(report_r.as_fd(), &mut byte);
            let _ = nix::sys::wait::waitpid(child, None);
            assert_eq!(byte[0], 1, "construction clears the dumpable flag");
        }
    }
}

// **The adopted end's close-on-exec test retired on 2026-08-06** with the
// assertion it read, `harness-coordination-end-close-on-exec`. There is no
// adopted end: this crate creates the coordination listener itself with the
// flag in its creating call, per `weaver-harness-Spec` section 2.3, and every
// accepted connection carries it from `accept4`. A test kept over a deleted
// mechanism would assert that the mechanism still existed. The property that
// replaced it, that the socket this crate creates is flagged atomically, is
// held by `pairs_are_close_on_exec_from_creation` above.
