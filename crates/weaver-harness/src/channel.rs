//! conforms: harness-truncation-is-a-fault
//! conforms: harness-one-write-is-one-read
//! conforms: harness-atomic-cloexec-at-creation
//! conforms: harness-binds-coordination-socket-first
//! conforms: harness-coordination-accept-refuses-non-root
//! conforms: harness-bind-never-unlinks
//! conforms: harness-organ-ends-from-descriptor-three
//! conforms: harness-child-flag-clear-unconditional
//! conforms: harness-fork-to-exec-bounded-calls
//! conforms: harness-trace-fd-cloexec-at-receive
//! conforms: harness-no-path-taken
//! conforms: harness-os-surface-nix
//! conforms: harness-descriptors-owned-types
//!
//! Organ-channel I/O and descriptor custody, per `weaver-harness-Spec`
//! section 2. The pairs this crate creates are `SOCK_SEQPACKET`, carrying the
//! election of `weaver-types-Spec` section 4 rather than re-deciding it, and
//! the election arrives with its obligation: the receive buffer is sized to
//! the maximum envelope and a read returning `MSG_TRUNC` is a channel fault
//! and never a message.
//!
//! **One path is taken, and it is not the trace's.** The coordination socket
//! is bound to a name the composition root supplies, per section 2.3 and the
//! inversion ruling of 2026-08-05, which is a deployment fact of the same kind
//! as the organ binaries of section 3. The prohibition `weaver-harness-PRD`
//! section 5 states is about the trace, and the trace still reaches this crate
//! as a descriptor and never as a name.

use std::io::IoSlice;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, OwnedFd, RawFd};

use nix::sys::socket::{
    AddressFamily, ControlMessage, MsgFlags, SockFlag, SockType, UnixAddr, accept4, bind,
    getsockopt, listen, recvmsg, sendmsg, socket, socketpair, sockopt,
};
use weaver_types::{MAX_ENVELOPE_BYTES, OrganEnvelope};

use crate::failure::ChannelFault;

/// The control buffer's size, generous against the one descriptor loop 0
/// legitimately carries so an honest message never truncates. A message that
/// truncates it anyway is a fault, and every descriptor it did deliver is
/// adopted and closed before that fault returns.
const CONTROL_BYTES: usize = 256;

/// The operator line an undecodable envelope leaves behind, per
/// `weaver-harness-Spec` section 7 and issue #404: the decoder's own error
/// names the specifics - a mixed-version deployment refuses on
/// `unknown field: column-permission`, the exact member two builds disagree
/// about - and the map to the closed case discarded it, so the death said
/// nothing an operator could act on. The case stays one case on the wire;
/// the specifics belong to the journal.
///
/// Built and returned so a watch can hold the line without capturing
/// stderr, printed by the caller at the refusing site.
///
/// conforms: harness-undecodable-names-its-detail
pub fn undecodable_detail(channel: &str, error: &serde_json::Error) -> String {
    serde_json::json!({
        "channel_undecodable": {
            "channel": channel,
            "detail": error.to_string(),
        }
    })
    .to_string()
}

/// The first descriptor after the standard streams: where an organ finds its
/// lifecycle channel from its first instruction, per section 2.2. An organ's
/// further channel takes the next number, so the gate's single end sits where
/// the SPU's first does.
pub const FIRST_ORGAN_DESCRIPTOR: RawFd = 3;

/// Where the worker finds the run lock's open file description admin's start
/// step took before its first fork, per `weaver-admin-Spec` section 3 and
/// `weaver-harness-Spec` section 2.2: one fixed number for every constituent,
/// the worker, the state member and the trace relay alike, elected by the act
/// that lands the start step. An organ child may place its ends over 8 and 9,
/// since `MAX_PLACED_ENDS` reaches 3 to 10, which is harmless: the worker's
/// own 8 and 9 are close-on-exec, and a placement only replaces the child's
/// copy of a number the exec would have closed anyway.
pub const RUN_LOCK_DESCRIPTOR: RawFd = 9;

/// Where the worker finds the write end of the trace relay's lifetime pipe,
/// per `weaver-admin-Spec` section 6, present only where a file sink stood a
/// relay.
pub const RELAY_LIFETIME_DESCRIPTOR: RawFd = 8;

/// **The worker's first act**, per `weaver-harness-Spec` section 2.2: the two
/// descriptors the start step hands across its exec, the run lock's
/// description and the relay's lifetime pipe, are marked close-on-exec before
/// any thread starts or anything forks, so no organ inherits a writable
/// descriptor to root's lock file or a write end that would keep the relay
/// alive, and no window exists in which one could.
///
/// **Neither is ever closed.** The run lock is held for the worker's life and
/// released by its death, and the relay reads end-of-file only when the last
/// write end is gone. A number holding nothing, a worker started by hand or
/// by a stack that predates the start step, is skipped: there is nothing to
/// keep. Answers which of the two were held, and **an error where a present
/// descriptor could not be marked**, which the worker refuses to start on,
/// since serving with it inheritable would hand an organ root's lock file.
pub fn keep_start_step_descriptors() -> std::io::Result<(bool, bool)> {
    let held = (
        keep_close_on_exec(RUN_LOCK_DESCRIPTOR)?,
        keep_close_on_exec(RELAY_LIFETIME_DESCRIPTOR)?,
    );
    // **Said, never silent**: a worker serving without a run lock is one a
    // later load cannot see, so its absence, or a number holding something
    // other than a lock file, is named on stderr, the worker's log.
    if !held.0 {
        eprintln!(
            "worker: no run lock at descriptor {RUN_LOCK_DESCRIPTOR}; not started by admin's \
             start step, so no later load can see this worker"
        );
    } else if !is_regular_file(RUN_LOCK_DESCRIPTOR) {
        eprintln!("worker: descriptor {RUN_LOCK_DESCRIPTOR} is not a regular file, not a run lock");
    }
    Ok(held)
}

/// Whether the number holds a regular file, read by `fstat`.
fn is_regular_file(fd: RawFd) -> bool {
    // SAFETY: the borrow is used for one fstat and adopts nothing.
    let borrowed = unsafe { BorrowedFd::borrow_raw(fd) };
    nix::sys::stat::fstat(borrowed)
        .is_ok_and(|stat| stat.st_mode & nix::libc::S_IFMT == nix::libc::S_IFREG)
}

/// Marks one inherited number close-on-exec and leaves it open, answering
/// whether a descriptor stood there. **An empty number is `EBADF` and only
/// `EBADF`**: any other failure of either call, a seccomp policy's rejection
/// among them, is an error, never an absence, because a present descriptor
/// left inheritable is the leak this exists to prevent.
pub fn keep_close_on_exec(fd: RawFd) -> std::io::Result<bool> {
    // SAFETY: fcntl on a number this process may or may not hold; F_GETFD on
    // an empty number fails with EBADF and touches nothing.
    let flags = unsafe { nix::libc::fcntl(fd, nix::libc::F_GETFD) };
    if flags == -1 {
        let error = std::io::Error::last_os_error();
        return match error.raw_os_error() {
            Some(nix::libc::EBADF) => Ok(false),
            _ => Err(error),
        };
    }
    // SAFETY: as above, on a number that holds a descriptor.
    if unsafe { nix::libc::fcntl(fd, nix::libc::F_SETFD, flags | nix::libc::FD_CLOEXEC) } == -1 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(true)
}

/// One end of an organ channel: a `SOCK_SEQPACKET` socket carrying one
/// `OrganEnvelope` per message, framing coming from the socket type rather
/// than from a length prefix this crate would otherwise have to write.
#[derive(Debug)]
pub struct OrganChannel {
    end: OwnedFd,
}

/// The decode end takes its own type rather than `OrganChannel`, because
/// `weaver-spu-PRD` section 13.2 rules that socket not an organ channel and a
/// shared name would carry the envelope's assumptions onto a seam that does
/// not take them. Its traffic is the token workflow's to shape.
#[derive(Debug)]
pub struct DecodeChannel {
    end: OwnedFd,
}

/// The far end of a created pair, held until the fork that hands it to a
/// child. It is an owned descriptor and nothing else: there is no call that
/// turns it into a path, and dropping it closes it.
#[derive(Debug)]
pub struct ChildEnd {
    end: OwnedFd,
}

/// The coordination listener, bound inside the agent's own runtime directory
/// and held for the worker's whole life.
#[derive(Debug)]
pub struct CoordinationListener {
    fd: OwnedFd,
    /// The path this listener bound, retained so the gate's socket can be
    /// named beside it, per `weaver-harness-Spec` section 3.
    ///
    /// **The one path this crate holds beyond the organ binaries**, and the
    /// exception is argued the same way: a deployment fact supplied at
    /// construction rather than a name resolved at use. Nothing here opens it
    /// again, the listener already being bound when this is stored.
    path: std::path::PathBuf,
}

impl CoordinationListener {
    /// The gate's socket, named beside the coordination socket this listener
    /// bound.
    ///
    /// **A sibling, so the manager's lifecycle covers both.** The runtime
    /// directory is created at start and destroyed with the unit, per
    /// `weaver-admin-systemd-contract` section 2, so a pathname inside it
    /// cannot outlive the worker and the next bind meets no stale name. A
    /// derivation reaching anywhere else would need the cleanup this program
    /// refuses to write, an unlink racing a live successor.
    ///
    /// **The leaf is fixed, so a coordination socket named `gate.sock` would
    /// collide with what this returns.** Unguarded on purpose. The collision
    /// is not reachable by configuring anything: admin fixes the coordination
    /// leaf in code and what an operator places is the directory above it, so
    /// producing one takes a composition root that names the coordination
    /// socket `gate.sock` by hand. What it produces then is the gate refusing
    /// its raise with `bind_failed` and the path carried, which is the
    /// documented answer for an occupied path rather than a silence.
    ///
    /// A derivation that cannot collide is available and was declined: it
    /// would have to carry the coordination socket's own name into the gate's,
    /// giving every operator who dials the door a name like
    /// `coordination-gate.sock` to explain, which is a permanent cost against
    /// a failure nobody reaches. Named here rather than fixed so the next
    /// composition root's author meets it before choosing a name.
    pub fn gate_socket(&self) -> std::path::PathBuf {
        self.path
            .parent()
            .unwrap_or(std::path::Path::new("/"))
            .join("gate.sock")
    }
}

/// A process umask held for one bind and restored on every path out.
///
/// The gate and the state member carry the same type for the same reason, per
/// `weaver-gate-Spec` section 3: a Unix socket's mode comes from the umask at
/// creation, and setting it afterwards leaves the name live at whatever was
/// inherited for the window between. Serialized on the one process umask, so
/// two binds cannot interleave their save and restore.
struct CoordinationUmask {
    previous: nix::sys::stat::Mode,
    _serialized: CoordinationSerialized,
}

static COORDINATION_UMASK: std::sync::Mutex<()> = std::sync::Mutex::new(());

thread_local! {
    /// Whether this thread already holds [`COORDINATION_UMASK`].
    ///
    /// **The mutex is not reentrant, and the guard alone does not make it
    /// so.** A caller that holds it and then does anything taking it again
    /// waits on itself, with no second party involved and no deadlock
    /// detector to say so. The lock serializes threads, and a thread cannot
    /// interleave with itself, so a nested acquire on the holding thread is
    /// a no-op. `weaver-gate-Spec` section 3 argues the same shape for the
    /// gate's guard.
    static COORDINATION_HELD: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// The umask lock, held for a scope, reentrant within one thread.
///
/// `None` inside means the outer holder on this thread releases it. The
/// umask nests correctly regardless, each guard restoring what it found in
/// LIFO order.
struct CoordinationSerialized(Option<std::sync::MutexGuard<'static, ()>>);

impl CoordinationSerialized {
    fn acquire() -> Self {
        if COORDINATION_HELD.with(std::cell::Cell::get) {
            return CoordinationSerialized(None);
        }
        // A poisoned lock still hands back the guard: the umask is restored
        // on every path out including a panic, so the value behind it is
        // sound whatever happened to the thread that held it last.
        let guard = COORDINATION_UMASK
            .lock()
            .unwrap_or_else(|held| held.into_inner());
        COORDINATION_HELD.with(|held| held.set(true));
        CoordinationSerialized(Some(guard))
    }
}

impl Drop for CoordinationSerialized {
    fn drop(&mut self) {
        if self.0.is_some() {
            COORDINATION_HELD.with(|held| held.set(false));
        }
    }
}

/// Runs `work` with the process umask held, for a caller that must read or
/// set it around this module's own use.
///
/// **Exposed to this crate's tests because the resource is the process's.** A
/// test reading the ambient umask, or electing a known one so a mode
/// assertion is not vacuous, races the bind's own guard otherwise.
// Test-only hook kept here (#118): beside the coordination guard it holds.
#[cfg(test)]
fn with_umask_held<T>(work: impl FnOnce() -> T) -> T {
    let _serialized = CoordinationSerialized::acquire();
    work()
}

impl CoordinationUmask {
    /// Deny every bit to group and other, so the name lands at `0700`.
    fn deny_all_but_owner() -> Self {
        let serialized = CoordinationSerialized::acquire();
        CoordinationUmask {
            previous: nix::sys::stat::umask(
                nix::sys::stat::Mode::S_IRWXG | nix::sys::stat::Mode::S_IRWXO,
            ),
            _serialized: serialized,
        }
    }
}

impl Drop for CoordinationUmask {
    fn drop(&mut self) {
        nix::sys::stat::umask(self.previous);
    }
}

/// Binds the coordination socket and listens, which is the worker's first act.
///
/// **The name is the caller's, and `gate.sock` is the one to avoid.**
/// [`CoordinationListener::gate_socket`] derives the gate's name as a sibling
/// with that fixed leaf, so a coordination socket carrying it would be the
/// path the gate is later told to bind. Nothing here refuses it, and that
/// function carries why.
///
/// **Any socket connecting to the harness is an internal connection**, per
/// `weaver-admin-harness-contract` section 2, so this crate creates and binds
/// it rather than adopting a handed end. The socket carries close-on-exec from
/// its creating call, so no window exists between creation and flag.
///
/// **The bind never unlinks.** A Unix socket's pathname outlives the process
/// that bound it, and the directory this socket lives in is created by the init
/// system at the unit's start and removed with the unit, per
/// `weaver-admin-systemd-contract` sections 2 and 5, so the name cannot be
/// inherited from a previous run and there is nothing to clear. **A bind that
/// finds its name occupied is a fault and never a thing to remove**, because
/// the only ways a name is occupied are that a live worker holds it, where
/// unlinking would strand the running agent's supervisor, or that the manager
/// did not honor the directory, where this crate's assumption is wrong and it
/// should say so rather than repair.
pub fn bind_coordination(
    socket_path: &std::path::Path,
) -> Result<CoordinationListener, ChannelFault> {
    let fd = socket(
        AddressFamily::Unix,
        SockType::SeqPacket,
        SockFlag::SOCK_CLOEXEC,
        None,
    )
    .map_err(|_| ChannelFault::Closed)?;
    // The path faults carry their own case: `Undecodable` is about envelope
    // octets, and reusing it for a name that cannot be a Unix address would
    // report one thing as another.
    let address = UnixAddr::new(socket_path)
        .map_err(|e| ChannelFault::SocketPathUnusable { errno: e as i32 })?;
    // **The mode is elected in the creating call**, the reasoning of
    // `weaver-gate-Spec` section 3 applied to the third named socket in the
    // agent's runtime directory. `bind` sets none, so this name would land at
    // `0777 & ~umask` and the door's permissions would be whatever umask this
    // process inherited.
    //
    // **`0700` here rather than the gate's `0770`.** `accept_root` admits
    // `uid() == 0` and no other, so no group reaches this door and a mode
    // granting one would describe access it does not offer. The credential
    // check is the lock that decides; this is the lock that keeps a stranger
    // from arriving at it, and the directory's `0750` narrows who may
    // traverse rather than who may connect.
    //
    // conforms: harness-coordination-door-states-its-mode
    let bound = {
        let _mask = CoordinationUmask::deny_all_but_owner();
        bind(fd.as_raw_fd(), &address)
    };
    bound.map_err(|e| ChannelFault::SocketPathUnusable { errno: e as i32 })?;
    listen(&fd, nix::sys::socket::Backlog::new(1).unwrap())
        .map_err(|e| ChannelFault::SocketPathUnusable { errno: e as i32 })?;
    Ok(CoordinationListener {
        fd,
        path: socket_path.to_path_buf(),
    })
}

impl AsFd for CoordinationListener {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.fd.as_fd()
    }
}

impl CoordinationListener {
    /// Accepts one connection, reading the peer credential before any byte.
    ///
    /// **Every accept refuses what is not root.** The socket is reachable from
    /// inside the sandbox by construction, so an elected tool at the agent uid
    /// can dial it, and what refuses that tool is that it is not root. The
    /// earlier design expected the agent's own uid here, which every tool of
    /// the agent's satisfies, and leaned on a listener closed after one accept
    /// to do the refusing.
    ///
    /// **The listener is not closed after an accept**, because admin is
    /// per-invocation and every later verb dials again.
    pub fn accept_root(&self) -> Result<OrganChannel, ChannelFault> {
        // **A signal is not an ending.** `accept4` returns `EINTR` when one
        // arrives while it waits, and a worker that read that as a closed
        // listener would stop serving because something unrelated happened.
        let fd = loop {
            match accept4(self.fd.as_raw_fd(), SockFlag::SOCK_CLOEXEC) {
                Ok(fd) => break fd,
                Err(nix::errno::Errno::EINTR) => continue,
                Err(_) => return Err(ChannelFault::Closed),
            }
        };
        // SAFETY: the kernel just created this descriptor and no other owner
        // exists.
        let connection = unsafe { OwnedFd::from_raw_fd(fd) };
        let peer =
            getsockopt(&connection, sockopt::PeerCredentials).map_err(|_| ChannelFault::Closed)?;
        if peer.uid() != 0 {
            // Refused at the connection rather than written to: no answer
            // crosses to a peer the check rejected.
            drop(connection);
            return Err(ChannelFault::WrongPeer { uid: peer.uid() });
        }
        Ok(OrganChannel { end: connection })
    }

    /// **An accept that does not wait**, for the leave's sweeps of the
    /// backlog (`weaver-admin-Spec` section 3, the seal, step 3): `None`
    /// where no dial is pending, else [`Self::accept_root`]'s answer.
    pub fn try_accept_root(&self) -> Result<Option<OrganChannel>, ChannelFault> {
        use nix::poll::{PollFd, PollFlags, PollTimeout, poll};
        let mut fds = [PollFd::new(self.fd.as_fd(), PollFlags::POLLIN)];
        match poll(&mut fds, PollTimeout::ZERO) {
            Ok(0) => Ok(None),
            Ok(_) => self.accept_root().map(Some),
            Err(nix::errno::Errno::EINTR) => Ok(None),
            Err(_) => Err(ChannelFault::Closed),
        }
    }

    /// **The seal's unlink** (`weaver-admin-Spec` section 3, the seal, step
    /// 2; `weaver-harness-Spec` section 2.3): the worker's own pathname
    /// removed once the leave's outcome is fixed, so no new dial reaches it.
    /// The listening descriptor stands, and the backlog already queued is
    /// still accepted.
    pub fn unlink(&self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

impl OrganChannel {
    /// A second handle on the same connection, so a leave carried out at
    /// rest holds its dialer's connection as one held from a turn does.
    pub fn try_clone(&self) -> Result<OrganChannel, ChannelFault> {
        self.end
            .try_clone()
            .map(|end| OrganChannel { end })
            .map_err(|_| ChannelFault::Closed)
    }

    /// Creates a pair with close-on-exec set atomically at creation by
    /// `SOCK_CLOEXEC` in the `socketpair` call rather than by a later `fcntl`.
    ///
    /// An unflagged end would cross every later exec: this process forks its
    /// organs, the gate among them, and the gate forks a shell per tool call,
    /// so an end of the residency seam left unflagged would reach the gate's
    /// image and from it every shell, a release directive handed to the tool
    /// surface. The atomic form is elected because a separate `fcntl` on the
    /// creating thread is safe only by an ordering no type enforces
    /// (`weaver-harness-Spec` section 2.2).
    pub fn pair() -> Result<(OrganChannel, ChildEnd), ChannelFault> {
        let (near, far) = socketpair(
            AddressFamily::Unix,
            SockType::SeqPacket,
            None,
            SockFlag::SOCK_CLOEXEC,
        )
        .map_err(|_| ChannelFault::Closed)?;
        Ok((OrganChannel { end: near }, ChildEnd { end: far }))
    }

    /// Writes one envelope as one message. The bound the receiver holds is
    /// asserted at the write site too, because a bound only the receiver holds
    /// is a bound the sender discovers in production.
    pub fn send(&self, envelope: &OrganEnvelope) -> Result<(), ChannelFault> {
        let body = serde_json::to_vec(envelope).map_err(|_| ChannelFault::Undecodable)?;
        if body.len() > MAX_ENVELOPE_BYTES {
            return Err(ChannelFault::Truncated {
                bound: MAX_ENVELOPE_BYTES,
            });
        }
        send_octets(self.end.as_fd(), &body)
    }

    /// Reads one envelope. The buffer is sized to the maximum envelope, and a
    /// read returning `MSG_TRUNC` is a channel fault: the kernel returns the
    /// truncated prefix with the flag set and discards the remainder, so an
    /// unchecked flag turns a long directive into a silently shortened one.
    pub fn recv(&self) -> Result<OrganEnvelope, ChannelFault> {
        let octets = recv_octets(self.end.as_fd())?;
        serde_json::from_slice(&octets).map_err(|error| {
            use std::io::Write as _;
            let _ = writeln!(std::io::stderr(), "{}", undecodable_detail("organ", &error));
            ChannelFault::Undecodable
        })
    }

    /// The one receive site that takes descriptors: the trace sink crosses
    /// once as ancillary data on the enter directive's own message, and the
    /// state channel's end rides beside it where the member stands, per the
    /// ruling of 2026-08-26, the two told apart by position per the
    /// contract's supply order. The call asks for `MSG_CMSG_CLOEXEC` in the
    /// receive itself. A descriptor received without the flag arrives clear,
    /// which is the window `weaver-organ-channel` section 2 describes and
    /// the reason the obligation is the receiver's.
    pub fn recv_with_descriptor(
        &self,
    ) -> Result<(OrganEnvelope, Option<OwnedFd>, Option<OwnedFd>), ChannelFault> {
        let mut buffer = vec![0u8; MAX_ENVELOPE_BYTES];
        let mut control = [0u8; CONTROL_BYTES];

        // **The receive is issued through `libc` rather than `nix` here, and
        // the reason is custody.** `nix`'s `RecvMsg::cmsgs` refuses to iterate
        // when `MSG_CTRUNC` is set, returning `ENOBUFS`, which is exactly the
        // case where descriptors were installed and must be closed: an
        // iterator that will not run on the truncated path leaves every
        // delivered descriptor unowned. Owning the `msghdr` is what lets the
        // walk below happen on every path, truncated or not.
        let mut iov = nix::libc::iovec {
            iov_base: buffer.as_mut_ptr().cast(),
            iov_len: buffer.len(),
        };
        // SAFETY: a zeroed msghdr is a valid empty header; every field the
        // call reads is set below.
        let mut mhdr: nix::libc::msghdr = unsafe { std::mem::zeroed() };
        mhdr.msg_iov = &mut iov;
        mhdr.msg_iovlen = 1;
        mhdr.msg_control = control.as_mut_ptr().cast();
        mhdr.msg_controllen = control.len() as _;

        // SAFETY: the header points at buffers this frame owns for the call's
        // duration, and the descriptor is this channel's own.
        let read = unsafe {
            nix::libc::recvmsg(self.end.as_raw_fd(), &mut mhdr, nix::libc::MSG_CMSG_CLOEXEC)
        };
        if read < 0 {
            return Err(ChannelFault::Closed);
        }

        // **Custody is established before the fault checks, not after them.**
        // The kernel can install ancillary descriptors on the same call that
        // returns a truncated body or a truncated control buffer, so a fault
        // check that returns first leaks descriptors this process now owns
        // and nothing closes. A peer sending oversized descriptor-bearing
        // messages would exhaust the table and the harness could then create
        // no organ channels. Adopting first makes every path drop what it
        // owns.
        let mut sink = None;
        let mut state_end = None;
        let mut surplus = Vec::new();
        // SAFETY: the walk reads only headers the kernel wrote into this
        // frame's control buffer, bounded by the length it reported, and each
        // descriptor is adopted exactly once.
        unsafe {
            let mut cmsg = nix::libc::CMSG_FIRSTHDR(&mhdr);
            while !cmsg.is_null() {
                // Both kernel lengths are `size_t` on glibc Linux, which is
                // where this crate runs, per `weaver-harness-Spec` section
                // 2.3 as of 2026-09-07: no width cast, and a libc where the
                // field is `socklen_t` re-enters at that clause first.
                let header_len = (*cmsg).cmsg_len;
                let payload_offset = nix::libc::CMSG_LEN(0) as usize;
                if header_len < payload_offset || header_len > mhdr.msg_controllen {
                    // A header the kernel could not complete: nothing after it
                    // can be trusted, and nothing in it names a descriptor.
                    break;
                }
                if (*cmsg).cmsg_level == nix::libc::SOL_SOCKET
                    && (*cmsg).cmsg_type == nix::libc::SCM_RIGHTS
                {
                    let data = nix::libc::CMSG_DATA(cmsg);
                    let width = std::mem::size_of::<RawFd>();
                    let count = (header_len - payload_offset) / width;
                    for index in 0..count {
                        let mut fd: RawFd = -1;
                        std::ptr::copy_nonoverlapping(
                            data.add(index * width),
                            (&mut fd as *mut RawFd).cast::<u8>(),
                            width,
                        );
                        if fd < 0 {
                            continue;
                        }
                        // The kernel just installed this descriptor for this
                        // process and no other owner exists, so adopting it
                        // into an OwnedFd is what makes its close a type
                        // property.
                        let owned = OwnedFd::from_raw_fd(fd);
                        if sink.is_none() {
                            sink = Some(owned);
                        } else if state_end.is_none() {
                            state_end = Some(owned);
                        } else {
                            surplus.push(owned);
                        }
                    }
                }
                cmsg = nix::libc::CMSG_NXTHDR(&mhdr, cmsg);
            }
        }
        drop(surplus);

        // A truncated control buffer means the kernel dropped descriptors the
        // sender authored, so the message is not the one that was sent and
        // cannot be repaired by reading what survived. What did arrive is
        // already owned and closed by the drop above.
        if mhdr.msg_flags & nix::libc::MSG_CTRUNC != 0 || mhdr.msg_flags & nix::libc::MSG_TRUNC != 0
        {
            return Err(ChannelFault::Truncated {
                bound: MAX_ENVELOPE_BYTES,
            });
        }
        let read = read as usize;
        if read == 0 {
            return Err(ChannelFault::Closed);
        }
        let envelope = serde_json::from_slice(&buffer[..read]).map_err(|error| {
            use std::io::Write as _;
            let _ = writeln!(
                std::io::stderr(),
                "{}",
                undecodable_detail("coordination", &error)
            );
            ChannelFault::Undecodable
        })?;
        Ok((envelope, sink, state_end))
    }

    /// Sends one envelope carrying one descriptor as ancillary data, the
    /// sending side of the receive above. Used by the tests that stand a
    /// coordination peer up, and by any composition root that speaks this
    /// seam.
    pub fn send_with_descriptor(
        &self,
        envelope: &OrganEnvelope,
        descriptor: BorrowedFd<'_>,
    ) -> Result<(), ChannelFault> {
        let body = serde_json::to_vec(envelope).map_err(|_| ChannelFault::Undecodable)?;
        if body.len() > MAX_ENVELOPE_BYTES {
            return Err(ChannelFault::Truncated {
                bound: MAX_ENVELOPE_BYTES,
            });
        }
        let fds = [descriptor.as_raw_fd()];
        let control = [ControlMessage::ScmRights(&fds)];
        let slices = [IoSlice::new(&body)];
        sendmsg::<()>(
            self.end.as_raw_fd(),
            &slices,
            &control,
            MsgFlags::empty(),
            None,
        )
        .map_err(|_| ChannelFault::Closed)?;
        Ok(())
    }

    /// Sends one envelope carrying two descriptors as ancillary data, the
    /// sending side of an enter that stands the state leg: the sink first
    /// and the state channel's end second, per the contract's supply order.
    /// Used by the tests that stand a coordination peer up, and by any
    /// composition root that speaks this seam.
    pub fn send_with_descriptors(
        &self,
        envelope: &OrganEnvelope,
        sink: BorrowedFd<'_>,
        state_end: BorrowedFd<'_>,
    ) -> Result<(), ChannelFault> {
        let body = serde_json::to_vec(envelope).map_err(|_| ChannelFault::Undecodable)?;
        if body.len() > MAX_ENVELOPE_BYTES {
            return Err(ChannelFault::Truncated {
                bound: MAX_ENVELOPE_BYTES,
            });
        }
        let fds = [sink.as_raw_fd(), state_end.as_raw_fd()];
        let control = [ControlMessage::ScmRights(&fds)];
        let slices = [IoSlice::new(&body)];
        sendmsg::<()>(
            self.end.as_raw_fd(),
            &slices,
            &control,
            MsgFlags::empty(),
            None,
        )
        .map_err(|_| ChannelFault::Closed)?;
        Ok(())
    }

    /// Yields the owned descriptor, for a composition root that created a pair
    /// here and hands one end to its consumer. Ownership moves with it.
    pub fn into_fd(self) -> OwnedFd {
        self.end
    }
}

impl AsFd for OrganChannel {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.end.as_fd()
    }
}

impl DecodeChannel {
    /// Creates the decode pair, `SOCK_SEQPACKET` with the same atomic flag.
    /// The seam's traffic shape is the token workflow's, so this type carries
    /// custody today and no vocabulary.
    pub fn pair() -> Result<(DecodeChannel, ChildEnd), ChannelFault> {
        let (near, far) = socketpair(
            AddressFamily::Unix,
            SockType::SeqPacket,
            None,
            SockFlag::SOCK_CLOEXEC,
        )
        .map_err(|_| ChannelFault::Closed)?;
        Ok((DecodeChannel { end: near }, ChildEnd { end: far }))
    }
}

impl DecodeChannel {
    /// Send one token directive as bare JSON, per the decode contract: the
    /// seam carries the token trio and no envelope, one write one message,
    /// and a frame past the envelope crosses as the segment series the
    /// contract admits for either end - the directives are small today and
    /// the law is symmetric anyway.
    pub fn send_directive(
        &self,
        directive: &weaver_types::TokenDirective,
    ) -> Result<(), ChannelFault> {
        let body = serde_json::to_vec(directive).map_err(|_| ChannelFault::Undecodable)?;
        if body.len() <= weaver_types::MAX_ENVELOPE_BYTES {
            return send_octets(self.end.as_fd(), &body);
        }
        if body.len() > weaver_types::DECODE_MESSAGE_BOUND {
            return Err(ChannelFault::Truncated {
                bound: weaver_types::DECODE_MESSAGE_BOUND,
            });
        }
        let preamble = weaver_types::SegmentPreamble {
            segments: body.len().div_ceil(weaver_types::MAX_ENVELOPE_BYTES) as u64,
            bytes: body.len() as u64,
        };
        let preamble = serde_json::to_vec(&preamble).map_err(|_| ChannelFault::Undecodable)?;
        send_octets(self.end.as_fd(), &preamble)?;
        for slice in body.chunks(weaver_types::MAX_ENVELOPE_BYTES) {
            send_octets(self.end.as_fd(), slice)?;
        }
        Ok(())
    }

    /// Receive one decode frame, blocking, reassembling a segment series
    /// into the one frame it carries: the series is the channel's fact and
    /// no layer above it sees a segment, per the decode contract. A frame
    /// is a `TokenAnswer` or a `TokenRefusal`, the two sharing the seam
    /// with disjoint tag vocabularies, so the receive distinguishes them
    /// rather than reading only the answer and losing a refusal to an
    /// undecodable fault.
    pub fn recv_reply(&self) -> Result<DecodeReply, ChannelFault> {
        let first = recv_octets(self.end.as_fd())?;
        let octets = reassemble_if_series(first, || recv_octets(self.end.as_fd()))?;
        if let Ok(answer) = serde_json::from_slice::<weaver_types::TokenAnswer>(&octets) {
            return Ok(DecodeReply::Answer(answer));
        }
        if let Ok(refusal) = serde_json::from_slice::<weaver_types::TokenRefusal>(&octets) {
            return Ok(DecodeReply::Refusal(refusal));
        }
        Err(ChannelFault::Undecodable)
    }
}

/// One frame off the decode seam: an answer to advance the exchange or a typed
/// refusal that declines the ask, per the decode contract's section 5.
#[derive(Debug)]
pub enum DecodeReply {
    Answer(weaver_types::TokenAnswer),
    Refusal(weaver_types::TokenRefusal),
}

/// The label seam's near end, per `weaver-harness-spu-classify-contract`
/// section 1: not an organ channel, so this type carries the label trio and
/// no envelope, its own name keeping the envelope's assumptions off a seam
/// that does not take them, the way [`DecodeChannel`] does for the token
/// seam.
#[derive(Debug)]
pub struct ClassifyChannel {
    end: OwnedFd,
    /// One-strike retirement, the state seam's economics on this seam: a
    /// timeout or a channel fault retires the arm, and every later ask
    /// answers the missing leg's absence immediately rather than paying
    /// the bound again against a peer already proven unresponsive. A
    /// `Cell` because the seat holds the channel by shared reference on
    /// the one serving thread.
    retired: std::cell::Cell<bool>,
}

impl ClassifyChannel {
    /// Creates the classify pair, `SOCK_SEQPACKET` per `weaver-spu-Spec`
    /// section 11, beside the SPU's two the way those are made before the
    /// fork that carries them.
    pub fn pair() -> Result<(ClassifyChannel, ChildEnd), ChannelFault> {
        let (near, far) = socketpair(
            AddressFamily::Unix,
            SockType::SeqPacket,
            None,
            SockFlag::SOCK_CLOEXEC,
        )
        .map_err(|_| ChannelFault::Closed)?;
        Ok((
            ClassifyChannel {
                end: near,
                retired: std::cell::Cell::new(false),
            },
            ChildEnd { end: far },
        ))
    }

    /// Whether the arm has been retired, per the one-strike economics.
    pub fn retired(&self) -> bool {
        self.retired.get()
    }

    /// Retire the arm: the peer missed its bound or the channel faulted,
    /// and nothing asks it again.
    pub fn retire(&self) {
        self.retired.set(true);
    }

    /// Receive one label frame inside a bound: the wait is a poll deadline,
    /// so a hung peer costs the bound once rather than the serving thread
    /// forever, and the expiry is a channel fault the caller converts.
    pub fn recv_reply_within(&self, bound_ms: u64) -> Result<ClassifyReply, ChannelFault> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(bound_ms);
        loop {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                return Err(ChannelFault::Closed);
            }
            let wait = remaining.as_millis().min(u128::from(u16::MAX)) as u16;
            let mut fds = [nix::poll::PollFd::new(
                self.end.as_fd(),
                nix::poll::PollFlags::POLLIN,
            )];
            match nix::poll::poll(&mut fds, wait) {
                Ok(0) => return Err(ChannelFault::Closed),
                Ok(_) => return self.recv_reply(),
                Err(nix::errno::Errno::EINTR) => continue,
                Err(_) => return Err(ChannelFault::Closed),
            }
        }
    }

    /// **Receive the answer to one ask by one deadline**, per
    /// `weaver-harness-Spec` section 6: the deadline is taken once, before
    /// the first receive, and a late readiness frame is skipped against it
    /// rather than renewing it, so a peer that sends readiness and then
    /// nothing costs the bound once and a stream of readiness frames cannot
    /// postpone the retirement the bound exists to force. What returns is
    /// never a readiness frame: the exchange's answer, its refusal, or the
    /// expiry as a channel fault.
    pub fn recv_answer_by(&self, bound_ms: u64) -> Result<ClassifyReply, ChannelFault> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(bound_ms);
        loop {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            // Rounded up, so the last receive reaches the deadline rather
            // than stopping short of it by the truncated fraction.
            let remaining_ms =
                u64::try_from(remaining.as_micros().div_ceil(1000)).unwrap_or(u64::MAX);
            match self.recv_reply_within(remaining_ms)? {
                ClassifyReply::Answer(weaver_types::LabelAnswer::Ready) => continue,
                reply => return Ok(reply),
            }
        }
    }

    /// Send one classify ask as bare JSON, the bound asserted on this side's
    /// own writes per the contract: a frame that would exceed it fails here,
    /// at the send, and the caller converts the failure to the standing
    /// absence.
    pub fn send_directive(
        &self,
        directive: &weaver_types::LabelDirective,
    ) -> Result<(), ChannelFault> {
        let body = serde_json::to_vec(directive).map_err(|_| ChannelFault::Undecodable)?;
        if body.len() > weaver_types::MAX_ENVELOPE_BYTES {
            return Err(ChannelFault::Truncated {
                bound: weaver_types::MAX_ENVELOPE_BYTES,
            });
        }
        send_octets(self.end.as_fd(), &body)
    }

    /// Receive one label frame, blocking: an answer or a typed refusal, the
    /// two sharing the seam with disjoint tag vocabularies the way the
    /// decode seam's do.
    pub fn recv_reply(&self) -> Result<ClassifyReply, ChannelFault> {
        let octets = recv_octets(self.end.as_fd())?;
        if let Ok(answer) = serde_json::from_slice::<weaver_types::LabelAnswer>(&octets) {
            return Ok(ClassifyReply::Answer(answer));
        }
        if let Ok(refusal) = serde_json::from_slice::<weaver_types::LabelRefusal>(&octets) {
            return Ok(ClassifyReply::Refusal(refusal));
        }
        Err(ChannelFault::Undecodable)
    }
}

/// One frame off the label seam: an answer or a typed refusal, per the
/// classify contract's section 2.
#[derive(Debug)]
pub enum ClassifyReply {
    Answer(weaver_types::LabelAnswer),
    Refusal(weaver_types::LabelRefusal),
}

impl AsFd for DecodeChannel {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.end.as_fd()
    }
}

/// Wrap an already-owned descriptor as a decode end, for the suite that scripts
/// a decode peer over a socketpair rather than forking a real SPU. Test-only
/// and not an adoption path.
// Test-only hook kept here (#118): other modules' suites reach it as `crate::channel`.
#[cfg(test)]
pub(crate) fn decode_from_owned(end: OwnedFd) -> DecodeChannel {
    DecodeChannel { end }
}

impl ChildEnd {
    /// The descriptor a child will find this end at once the handoff runs.
    pub fn as_fd(&self) -> BorrowedFd<'_> {
        self.end.as_fd()
    }

    /// What an organ does with the end it was handed: adopt it as its channel.
    /// The organ crates reach this after finding their end at descriptor 3.
    pub fn into_channel(self) -> OrganChannel {
        OrganChannel { end: self.end }
    }
}

fn send_octets(end: BorrowedFd<'_>, body: &[u8]) -> Result<(), ChannelFault> {
    let slices = [IoSlice::new(body)];
    sendmsg::<()>(end.as_raw_fd(), &slices, &[], MsgFlags::empty(), None)
        .map_err(|_| ChannelFault::Closed)?;
    Ok(())
}

/// Reassemble a segment series where the first frame is its preamble, per
/// `weaver-types-Spec` section 4.4, the mirror of the SPU side's: the
/// kindless shape is reserved rather than guessed, the preamble validates
/// whole before the first slice is read, and a short, long, or inconsistent
/// series is a channel fault, never a partial frame.
fn reassemble_if_series(
    first: Vec<u8>,
    mut next: impl FnMut() -> Result<Vec<u8>, ChannelFault>,
) -> Result<Vec<u8>, ChannelFault> {
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(&first) else {
        return Ok(first);
    };
    let Some(object) = value.as_object() else {
        return Ok(first);
    };
    if object.contains_key("kind") {
        return Ok(first);
    }
    let Ok(preamble) = serde_json::from_slice::<weaver_types::SegmentPreamble>(&first) else {
        return Err(ChannelFault::Undecodable);
    };
    let Ok(bytes) = usize::try_from(preamble.bytes) else {
        return Err(ChannelFault::Undecodable);
    };
    if bytes <= weaver_types::MAX_ENVELOPE_BYTES || bytes > weaver_types::DECODE_MESSAGE_BOUND {
        return Err(ChannelFault::Undecodable);
    }
    if preamble.segments != bytes.div_ceil(weaver_types::MAX_ENVELOPE_BYTES) as u64 {
        return Err(ChannelFault::Undecodable);
    }
    let mut whole = Vec::with_capacity(bytes);
    for _ in 0..preamble.segments {
        whole.extend_from_slice(&next()?);
        if whole.len() > bytes {
            return Err(ChannelFault::Undecodable);
        }
    }
    if whole.len() != bytes {
        return Err(ChannelFault::Undecodable);
    }
    Ok(whole)
}

fn recv_octets(end: BorrowedFd<'_>) -> Result<Vec<u8>, ChannelFault> {
    let mut buffer = vec![0u8; MAX_ENVELOPE_BYTES];
    let mut slices = [std::io::IoSliceMut::new(&mut buffer)];
    // A signal landing mid-call is not the peer's doing, the same rule the
    // send side runs: retry rather than report a closed channel that is not
    // closed.
    let message = loop {
        match recvmsg::<()>(end.as_raw_fd(), &mut slices, None, MsgFlags::empty()) {
            Ok(message) => break message,
            Err(nix::errno::Errno::EINTR) => continue,
            Err(_) => return Err(ChannelFault::Closed),
        }
    };
    if message.flags.contains(MsgFlags::MSG_TRUNC) {
        return Err(ChannelFault::Truncated {
            bound: MAX_ENVELOPE_BYTES,
        });
    }
    let read = message.bytes;
    if read == 0 {
        return Err(ChannelFault::Closed);
    }
    buffer.truncate(read);
    Ok(buffer)
}

/// The child's side of the handoff, between fork and exec: `dup2` each end it
/// is given to a descriptor from 3 upward, `fcntl` the flag clear on each
/// unconditionally, and `execve`. Nothing else - all three are
/// async-signal-safe, and the bound is the safety argument rather than a
/// style, because the worker holds the writer's thread at every fork.
///
/// The clear is unconditional because the duplicate law has a corner: a
/// duplicate made by `dup2` is born with the flag clear, but only when the two
/// descriptors differ, and `dup2` onto the same number is a no-op that returns
/// the descriptor with its flag untouched. A child whose end already sat at
/// descriptor 3 would keep the flag, lose the end at `execve`, and start the
/// organ with no channel, silently.
///
/// # Safety
///
/// Must be called only in a forked child, before its exec, and only with ends
/// this process created. An end is any descriptor the organ is owed: a
/// channel's [`ChildEnd`], or the gate's handle on `tools/` beside it
/// (`weaver-harness-Spec` section 2.2, #111).
pub unsafe fn place_child_ends(ends: &[BorrowedFd<'_>]) -> Result<(), nix::Error> {
    if ends.len() > MAX_PLACED_ENDS {
        return Err(nix::Error::EINVAL);
    }
    let count = ends.len() as RawFd;
    let target_end = FIRST_ORGAN_DESCRIPTOR + count;

    // **A source sitting inside the target range is moved out of it first,
    // because `dup2` onto a target closes whatever already occupies that
    // number.** With ends at 4 and 3, placing the first at 3 would close the
    // second before its own turn, and placing the second at 4 would then
    // duplicate the first: both numbers would name one channel and the other
    // would be gone. Descriptor numbers are the lowest free number, so a
    // descending layout is reachable whenever an earlier close left a hole.
    let mut sources = [0 as RawFd; MAX_PLACED_ENDS];
    for (index, end) in ends.iter().enumerate() {
        let mut source = end.as_raw_fd();
        if source >= FIRST_ORGAN_DESCRIPTOR && source < target_end {
            // SAFETY: fcntl is async-signal-safe. F_DUPFD_CLOEXEC returns the
            // lowest free descriptor at or above the floor, which is outside
            // the range every dup2 below will overwrite.
            let moved = unsafe { nix::libc::fcntl(source, nix::libc::F_DUPFD_CLOEXEC, target_end) };
            if moved == -1 {
                return Err(nix::Error::last());
            }
            source = moved;
        }
        sources[index] = source;
    }

    for (index, source) in sources.iter().take(ends.len()).enumerate() {
        let target = FIRST_ORGAN_DESCRIPTOR + index as RawFd;
        if *source != target {
            // SAFETY: dup2 is async-signal-safe and both numbers are this
            // child's own. The duplicate is born with the flag clear.
            if unsafe { nix::libc::dup2(*source, target) } == -1 {
                // **A failed placement must not reach the exec.** The organ
                // would start with no channel at 3, or with whatever
                // unrelated descriptor sat there, which is the silent
                // no-channel start this whole design exists to prevent.
                return Err(nix::Error::last());
            }
        }
        // The unconditional clear: the no-op case above leaves the flag set,
        // so this runs whether or not a duplication moved anything.
        // SAFETY: fcntl is async-signal-safe.
        if unsafe { nix::libc::fcntl(target, nix::libc::F_SETFD, 0) } == -1 {
            return Err(nix::Error::last());
        }
    }
    Ok(())
}

/// The most ends any organ is handed: the SPU's two. A fixed array keeps the
/// pre-move bookkeeping allocation-free, which the async-signal-safe bound
/// between fork and exec requires.
const MAX_PLACED_ENDS: usize = 8;

#[cfg(test)]
mod series_tests;

#[cfg(test)]
mod door_mode_tests;

#[cfg(test)]
mod undecodable_tests;

#[cfg(test)]
mod classify_bound_tests;

#[cfg(test)]
mod start_step_descriptor_tests;
