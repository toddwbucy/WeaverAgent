//! conforms: admin-one-write-is-one-read
//! conforms: admin-dial-retries-within-a-bound
//! conforms: admin-truncation-is-a-channel-fault
//! conforms: admin-enter-carries-descriptor-in-one-message
//! conforms: admin-run-reference-distinguishes
//!
//! The coordination channel, per `weaver-admin-Spec` section 7: reached in one
//! act, the dial, `SOCK_SEQPACKET`, carrying the election of
//! `weaver-types-Spec` section 4 rather than re-deciding it.
//!
//! **The channel is dialed and never bound.** Any socket connecting to the
//! harness is an internal connection, so the harness binds inside the agent's
//! sandbox and this crate connects, per `weaver-admin-harness-contract`
//! section 2. What refuses a stranger dialing in is the credential the
//! harness reads at its accept. **What refuses a stranger dialed is the
//! credential this crate reads after its connect** (#98, #140): the socket
//! stands in a directory the agent owns, so the peer must be the agent's
//! worker, its account and its process, or the name was replaced and the
//! dial is refused.
//!
//! **The dial retries within a bound.** The load starts the unit and then
//! dials, and the bind is the worker's first act, so the race is structural.
//! A bound exceeded refuses rather than waiting without end.

use std::os::fd::{AsRawFd, BorrowedFd, OwnedFd};
use std::path::Path;
use std::time::{Duration, Instant};

use nix::sys::socket::{
    AddressFamily, ControlMessage, MsgFlags, SockFlag, SockType, UnixAddr, connect, sendmsg, socket,
};
use weaver_types::{MAX_ENVELOPE_BYTES, OrganEnvelope};

#[allow(dead_code)]
#[path = "channel_capture.rs"]
mod capture;
pub(crate) use capture::{CaptureSendFailure, ReceivedCapture};
#[cfg(test)]
pub(crate) use capture::{TestCaptureSend, with_before_capture_send};

/// A fault below the exchange layer on the coordination channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChannelFault {
    Truncated {
        bound: usize,
    },
    Undecodable,
    Closed,
    /// The dial found no listener within the bound. The worker never bound its
    /// socket, and section 7's ceiling is what turns that into an answer
    /// rather than a wait.
    NotDialable,
    /// **Positive evidence that no worker listens**, per `weaver-admin-Spec`
    /// section 3: through the dial's whole bound the name stood bound to
    /// nothing or the connection was refused, never a full backlog, which is
    /// a busy worker and so not evidence.
    NoListener,
    /// An answer did not arrive within its exchange's bound, per section 3.
    Unanswered,
    /// **The peer is not this agent's worker** (#98, #140;
    /// `weaver-admin-Spec` section 7): the listener's uid, read by
    /// `SO_PEERCRED` after the connect, is not the worker account's, or the
    /// listening process, read through its pidfd, is not the one holding the
    /// run lock. The socket's directory is the agent's, so a name replaced
    /// there could aim this invocation at another agent's worker, at any
    /// listener on the box, or at an organ or tool under the worker's own
    /// uid. Refused at once and never retried, a wrong peer being no
    /// transient absence. `pid` is `SO_PEERCRED`'s, the listener's at
    /// `listen`.
    WrongPeer {
        uid: u32,
        pid: i32,
    },
}

/// The connected end this invocation holds for the life of one verb.
#[derive(Debug)]
pub struct Coordination {
    fd: OwnedFd,
    ordinal: u64,
}

/// The dial's ceiling and its interval, this Spec's election. One second of
/// attempts at ten milliseconds, per section 7: the bind is the worker's first
/// act and the load's dial may arrive first, so a bounded retry covers the
/// race, and the ceiling is what keeps a worker that never binds from hanging
/// the operator's terminal.
const DIAL_CEILING: Duration = Duration::from_secs(1);
const DIAL_INTERVAL: Duration = Duration::from_millis(10);

/// Connects to the socket the worker bound, retrying within the bound above.
///
/// Close-on-exec is asked for in the socket call itself, so no window exists
/// between creation and flag. The connection is this invocation's and closes
/// when the verb answers, there being no standing end to keep.
///
/// **The socket is nonblocking for the connect and blocking afterwards, and
/// the reason is that a blocking connect defeats the ceiling.** Measured
/// 2026-08-06: with the listener's backlog full, a blocking `connect` on an
/// `AF_UNIX` socket was still blocked after three seconds, three times the
/// bound this election exists to keep, while the same connect on a nonblocking
/// socket returned `EAGAIN` at once. A full backlog is reachable here rather
/// than theoretical, because the harness serves one connection at a time, so a
/// verb arriving while another is in flight meets exactly that. The flag is
/// cleared once connected, because everything after the dial, the enter
/// directive and the answer it waits for, is blocking work.
pub fn dial(
    socket_path: &Path,
    worker_uid: u32,
    run_directory: &Path,
) -> Result<Coordination, ChannelFault> {
    let deadline = Instant::now() + DIAL_CEILING;
    loop {
        match dial_once(socket_path, worker_uid, run_directory) {
            Ok(connection) => return Ok(connection),
            // Not a transient absence: the name itself is unusable, so the
            // ceiling would spend a second reaching the same answer.
            Err(ChannelFault::Undecodable) => return Err(ChannelFault::Undecodable),
            Err(fault @ ChannelFault::WrongPeer { .. }) => return Err(fault),
            Err(fault) => {
                // **The wait never outlives the deadline.** Sleeping a fixed
                // interval would overshoot the bound by up to one interval,
                // and a bound that is approximately kept is not the bound the
                // Spec states.
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return Err(fault);
                }
                std::thread::sleep(DIAL_INTERVAL.min(remaining));
            }
        }
    }
}

pub(crate) fn dial_for_enter(
    socket_path: &Path,
    worker_uid: u32,
    run_directory: &Path,
    window: &crate::serving_load::window::LoadWindow,
) -> Result<Coordination, ChannelFault> {
    let ceiling = Instant::now() + DIAL_CEILING;
    loop {
        window.check_channel()?;
        let result = dial_once(socket_path, worker_uid, run_directory);
        match result {
            Ok(peer) => {
                window.check_channel()?;
                return Ok(peer);
            }
            Err(fault @ (ChannelFault::Undecodable | ChannelFault::WrongPeer { .. })) => {
                return Err(fault);
            }
            Err(fault) => {
                let remaining = window.remaining_channel()?;
                let dial_remaining = ceiling.saturating_duration_since(Instant::now());
                if dial_remaining.is_zero() {
                    return Err(fault);
                }
                std::thread::sleep(DIAL_INTERVAL.min(remaining).min(dial_remaining));
            }
        }
    }
}

fn dial_once(
    socket_path: &Path,
    worker_uid: u32,
    run_directory: &Path,
) -> Result<Coordination, ChannelFault> {
    let fd = socket(
        AddressFamily::Unix,
        SockType::SeqPacket,
        SockFlag::SOCK_CLOEXEC | SockFlag::SOCK_NONBLOCK,
        None,
    )
    .map_err(|_| ChannelFault::NotDialable)?;
    // **A path a Unix address cannot be built from is not a race**, so it
    // returns a fault the caller does not retry.
    let address = UnixAddr::new(socket_path).map_err(|_| ChannelFault::Undecodable)?;
    match connect(fd.as_raw_fd(), &address) {
        Ok(()) => {}
        // The listener's backlog is full: a transient absence like any other,
        // and the one this crate would otherwise block on.
        Err(nix::errno::Errno::EAGAIN) | Err(nix::errno::Errno::EINPROGRESS) => {
            return Err(ChannelFault::NotDialable);
        }
        Err(nix::errno::Errno::ENOENT) | Err(nix::errno::Errno::ECONNREFUSED) => {
            return Err(ChannelFault::NoListener);
        }
        Err(_) => return Err(ChannelFault::NotDialable),
    }
    // **The peer is judged before a byte is sent** (#98): a Unix socket's
    // peer credential is the listener's, set when it listened, so it reads
    // at once, accepted or still in the backlog.
    let peer = nix::sys::socket::getsockopt(&fd, nix::sys::socket::sockopt::PeerCredentials)
        .map_err(|_| ChannelFault::NotDialable)?;
    if peer.uid() != worker_uid {
        diag!(
            "weaver-admin: the coordination socket's listener is uid {}, not the agent's worker (uid {worker_uid}), so it is not dialed",
            peer.uid()
        );
        return Err(ChannelFault::WrongPeer {
            uid: peer.uid(),
            pid: peer.pid(),
        });
    }
    // **And the process, not only the account** (#140): the gate, the SPU
    // and every tool run under the worker's uid, so the listener must be the
    // process holding the run lock's description.
    if let Err(why) = listener_is_the_worker(&fd, peer.pid(), run_directory) {
        diag!(
            "weaver-admin: the coordination socket's listener, pid {}, {why}, so it is not dialed",
            peer.pid()
        );
        return Err(ChannelFault::WrongPeer {
            uid: peer.uid(),
            pid: peer.pid(),
        });
    }
    clear_nonblocking(&fd)?;
    Ok(Coordination { fd, ordinal: 0 })
}

/// **Whether the listener is the worker's process**, per section 7: the
/// listener's pidfd (`SO_PEERPIDFD`) is read, `SO_PEERCRED`'s pid holds the
/// run lock's description, and the pidfd still reads that pid after the
/// scan. `SO_PEERCRED`'s pid is fixed at `listen` and outlives its process,
/// so a reaped listener's number, reused by the next run's worker, would
/// pass the scan; its pidfd reads `-1` once it is gone, and a process cannot
/// give up its pid while it lives, so one read after the scan shows the pid
/// named the listener throughout. Answers why not, each cause named apart
/// for `admin.log`: a kernel without `SO_PEERPIDFD` (before Linux 6.5), a
/// descriptor table root cannot read, a process holding no run lock, and a
/// listener gone.
fn listener_is_the_worker(
    fd: &OwnedFd,
    pid: i32,
    run_directory: &Path,
) -> Result<(), &'static str> {
    let pidfd = nix::sys::socket::getsockopt(fd, nix::sys::socket::sockopt::PeerPidfd)
        .map_err(|_| "cannot be judged: the kernel answers no pidfd for it (SO_PEERPIDFD, Linux 6.5 or later)")?;
    // #118: a test-only hook, kept beside the production code it observes:
    // the suite's stand-in workers are its own threads, so a listener that
    // is this process is taken for the worker. Production admin never
    // listens on this channel.
    #[cfg(test)]
    if pid == std::process::id() as i32 {
        return Ok(());
    }
    match crate::start::holds_run_lock(run_directory, pid) {
        Err(()) => {
            return Err("cannot be judged: its descriptor table or the run lock does not read");
        }
        Ok(false) => return Err("is not the process holding the agent's run lock"),
        Ok(true) => {}
    }
    if pidfd_pid(&pidfd) != Some(pid) {
        return Err("is gone, its pid no longer naming the process that listened");
    }
    Ok(())
}

/// The pid a pidfd names, from its `Pid:` line in `/proc/self/fdinfo`, `-1`
/// once the process is gone.
fn pidfd_pid(pidfd: &OwnedFd) -> Option<i32> {
    std::fs::read_to_string(format!("/proc/self/fdinfo/{}", pidfd.as_raw_fd()))
        .ok()?
        .lines()
        .find_map(|line| line.strip_prefix("Pid:"))
        .and_then(|value| value.trim().parse().ok())
}

/// Returns the connected end to blocking mode. Everything after the dial waits
/// on the harness, and a nonblocking read would report an empty channel as a
/// fault rather than waiting for the answer.
fn clear_nonblocking(fd: &OwnedFd) -> Result<(), ChannelFault> {
    // SAFETY: fcntl on a descriptor this frame owns.
    let flags = unsafe { nix::libc::fcntl(fd.as_raw_fd(), nix::libc::F_GETFL) };
    if flags == -1 {
        return Err(ChannelFault::NotDialable);
    }
    // SAFETY: as above.
    let rc = unsafe {
        nix::libc::fcntl(
            fd.as_raw_fd(),
            nix::libc::F_SETFL,
            flags & !nix::libc::O_NONBLOCK,
        )
    };
    if rc == -1 {
        return Err(ChannelFault::NotDialable);
    }
    Ok(())
}

impl Coordination {
    /// Observe and Leave use separate exchanges/connections. The invocation
    /// keeps the first socket until this check: kernel pidfds, not a reused
    /// numeric PID, pin the worker whose loaded facts authorized the directive.
    pub(crate) fn require_same_worker(&self, observed: &Self) -> Result<(), ChannelFault> {
        use nix::sys::socket::{getsockopt, sockopt};
        let peer = getsockopt(&self.fd, sockopt::PeerCredentials)
            .map_err(|_| ChannelFault::Undecodable)?;
        let prior = getsockopt(&observed.fd, sockopt::PeerCredentials)
            .map_err(|_| ChannelFault::Undecodable)?;
        let current_pidfd =
            getsockopt(&self.fd, sockopt::PeerPidfd).map_err(|_| ChannelFault::Undecodable)?;
        let observed_pidfd =
            getsockopt(&observed.fd, sockopt::PeerPidfd).map_err(|_| ChannelFault::Undecodable)?;
        if peer.pid() != prior.pid()
            || peer.uid() != prior.uid()
            || pidfd_pid(&current_pidfd) != Some(peer.pid())
            || pidfd_pid(&observed_pidfd) != Some(prior.pid())
        {
            return Err(ChannelFault::WrongPeer {
                uid: peer.uid(),
                pid: peer.pid(),
            });
        }
        Ok(())
    }

    /// The next exchange identity: ordinals assigned serially by this crate,
    /// per `weaver-organ-channel` section 1.
    pub fn next_ordinal(&mut self) -> u64 {
        self.ordinal += 1;
        self.ordinal
    }

    /// Writes one envelope as one message, asserting the same bound the
    /// receiver holds, because a bound only the receiver holds is a bound the
    /// sender discovers in production.
    /// Sends one directive on this channel, opening its exchange.
    pub fn send_directive(
        &self,
        ordinal: u64,
        directive: weaver_types::LifecycleDirective,
    ) -> Result<(), ChannelFault> {
        self.send(&OrganEnvelope {
            exchange: weaver_types::ExchangeId {
                opener: weaver_types::Opener::Admin,
                ordinal,
            },
            position: weaver_types::Position::Open,
            payload: weaver_types::Payload::Directive(directive),
        })
    }

    pub fn send(&self, envelope: &OrganEnvelope) -> Result<(), ChannelFault> {
        let body = serde_json::to_vec(envelope).map_err(|_| ChannelFault::Undecodable)?;
        if body.len() > MAX_ENVELOPE_BYTES {
            return Err(ChannelFault::Truncated {
                bound: MAX_ENVELOPE_BYTES,
            });
        }
        let slices = [std::io::IoSlice::new(&body)];
        let sent = sendmsg::<()>(self.fd.as_raw_fd(), &slices, &[], MsgFlags::empty(), None)
            .map_err(|_| ChannelFault::Closed)?;
        if sent != body.len() {
            return Err(ChannelFault::Closed);
        }
        Ok(())
    }

    /// **The enter directive and its ancillary payload are one message.** The
    /// envelope is rendered and sent with the sink's descriptor, and the
    /// state channel's end where the member stands, as `SCM_RIGHTS` control
    /// data on the same `sendmsg`, which is what makes each descriptor cross
    /// once, in the enter exchange, with no separate delivery to order
    /// against anything, the receiver telling the two apart by position per
    /// the contract's supply order.
    pub fn send_with_sink(
        &self,
        envelope: &OrganEnvelope,
        sink: BorrowedFd<'_>,
        state_end: Option<BorrowedFd<'_>>,
    ) -> Result<(), ChannelFault> {
        let body = serde_json::to_vec(envelope).map_err(|_| ChannelFault::Undecodable)?;
        if body.len() > MAX_ENVELOPE_BYTES {
            return Err(ChannelFault::Truncated {
                bound: MAX_ENVELOPE_BYTES,
            });
        }
        self.send_descriptor_bytes(&body, sink, state_end)
    }

    /// Dormant paired TYPES-ENTER sender. No active command selects this codec.
    #[cfg(test)]
    pub(crate) fn send_replacement_enter(
        &self,
        envelope: &weaver_types::state_flow::enter::EnterEnvelope,
        sink: BorrowedFd<'_>,
        state_end: Option<BorrowedFd<'_>>,
    ) -> Result<(), ChannelFault> {
        use weaver_types::state_flow::enter::{CodecError, EnterBinding};
        if matches!(envelope.enter().binding, EnterBinding::Serving { .. }) && state_end.is_none() {
            return Err(ChannelFault::Undecodable);
        }
        let body = envelope.encode().map_err(|error| match error {
            CodecError::TooLarge => ChannelFault::Truncated {
                bound: MAX_ENVELOPE_BYTES,
            },
            _ => ChannelFault::Undecodable,
        })?;
        self.send_descriptor_bytes(&body, sink, state_end)
    }

    /// The replacement Enter retains one original raw deadline through encode,
    /// socket pressure and carry. SCM_RIGHTS is retried only after EAGAIN/EINTR,
    /// never after the kernel accepted the complete seqpacket.
    pub(crate) fn send_replacement_enter_within(
        &self,
        envelope: &weaver_types::state_flow::enter::EnterEnvelope,
        sink: BorrowedFd<'_>,
        state: BorrowedFd<'_>,
        window: &crate::serving_load::window::LoadWindow,
    ) -> Result<(), ChannelFault> {
        use weaver_types::state_flow::enter::{CodecError, EnterBinding};
        let EnterBinding::Serving { enter_window, .. } = &envelope.enter().binding else {
            return Err(ChannelFault::Undecodable);
        };
        if *enter_window != window.original() {
            return Err(ChannelFault::Undecodable);
        }
        window.check_channel()?;
        let body = envelope.encode().map_err(|error| match error {
            CodecError::TooLarge => ChannelFault::Truncated {
                bound: MAX_ENVELOPE_BYTES,
            },
            _ => ChannelFault::Undecodable,
        })?;
        window.check_channel()?;
        let fds = [sink.as_raw_fd(), state.as_raw_fd()];
        loop {
            window.check_channel()?;
            match sendmsg::<()>(
                self.fd.as_raw_fd(),
                &[std::io::IoSlice::new(&body)],
                &[ControlMessage::ScmRights(&fds)],
                MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_NOSIGNAL,
                None,
            ) {
                Ok(sent) if sent == body.len() => return Ok(()),
                Ok(_) => return Err(ChannelFault::Closed),
                Err(nix::errno::Errno::EINTR) => continue,
                Err(nix::errno::Errno::EAGAIN) => self.wait_enter(nix::libc::POLLOUT, window)?,
                Err(_) => return Err(ChannelFault::Closed),
            }
        }
    }

    pub(crate) fn recv_enter_answer(
        &self,
        window: &crate::serving_load::window::LoadWindow,
    ) -> Result<OrganEnvelope, ChannelFault> {
        self.recv_enter_answer_with(window, |bytes| {
            serde_json::from_slice(bytes).map_err(|_| ChannelFault::Undecodable)
        })
    }
    fn recv_enter_answer_with(
        &self,
        window: &crate::serving_load::window::LoadWindow,
        decode: impl FnOnce(&[u8]) -> Result<OrganEnvelope, ChannelFault>,
    ) -> Result<OrganEnvelope, ChannelFault> {
        loop {
            self.wait_enter(nix::libc::POLLIN, window)?;
            let mut bytes = vec![0; MAX_ENVELOPE_BYTES];
            let mut slices = [std::io::IoSliceMut::new(&mut bytes)];
            let message = match nix::sys::socket::recvmsg::<()>(
                self.fd.as_raw_fd(),
                &mut slices,
                None,
                MsgFlags::MSG_DONTWAIT,
            ) {
                Ok(message) => message,
                Err(nix::errno::Errno::EINTR | nix::errno::Errno::EAGAIN) => continue,
                Err(_) => return Err(ChannelFault::Closed),
            };
            if message
                .flags
                .intersects(MsgFlags::MSG_TRUNC | MsgFlags::MSG_CTRUNC)
            {
                return Err(ChannelFault::Truncated {
                    bound: MAX_ENVELOPE_BYTES,
                });
            }
            if message.bytes == 0 {
                return Err(ChannelFault::Closed);
            }
            let count = message.bytes;
            let answer = decode(&bytes[..count])?;
            window.check_channel()?;
            return Ok(answer);
        }
    }

    fn wait_enter(
        &self,
        events: i16,
        window: &crate::serving_load::window::LoadWindow,
    ) -> Result<(), ChannelFault> {
        loop {
            let remaining = window.remaining_channel()?;
            let millis = remaining.as_nanos().div_ceil(1_000_000).min(poll_cap_ms()) as i32;
            let mut fd = nix::libc::pollfd {
                fd: self.fd.as_raw_fd(),
                events,
                revents: 0,
            };
            // SAFETY: one valid pollfd; the original owner holds the descriptor.
            let result = unsafe { nix::libc::poll(&mut fd, 1, millis) };
            if result < 0
                && std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted
            {
                return Err(ChannelFault::Closed);
            }
            window.check_channel()?;
            if result > 0 {
                return Ok(());
            }
        }
    }

    fn send_descriptor_bytes(
        &self,
        body: &[u8],
        sink: BorrowedFd<'_>,
        state_end: Option<BorrowedFd<'_>>,
    ) -> Result<(), ChannelFault> {
        if body.len() > MAX_ENVELOPE_BYTES {
            return Err(ChannelFault::Truncated {
                bound: MAX_ENVELOPE_BYTES,
            });
        }
        let mut fds = vec![sink.as_raw_fd()];
        if let Some(end) = state_end {
            fds.push(end.as_raw_fd());
        }
        let control = [ControlMessage::ScmRights(&fds)];
        let slices = [std::io::IoSlice::new(body)];
        let sent = sendmsg::<()>(
            self.fd.as_raw_fd(),
            &slices,
            &control,
            MsgFlags::empty(),
            None,
        )
        .map_err(|_| ChannelFault::Closed)?;
        if sent != body.len() {
            return Err(ChannelFault::Closed);
        }
        Ok(())
    }

    /// Reads one envelope. The buffer is sized to the envelope bound, and a
    /// read returning `MSG_TRUNC` **is a channel fault and never a message**:
    /// the kernel returns the truncated prefix with the flag set and discards
    /// the remainder, so an unchecked flag turns a long answer into a silently
    /// shortened one.
    pub fn recv(&self) -> Result<OrganEnvelope, ChannelFault> {
        let mut buffer = vec![0u8; MAX_ENVELOPE_BYTES];
        let mut slices = [std::io::IoSliceMut::new(&mut buffer)];
        let message = nix::sys::socket::recvmsg::<()>(
            self.fd.as_raw_fd(),
            &mut slices,
            None,
            MsgFlags::empty(),
        )
        .map_err(|_| ChannelFault::Closed)?;
        if message.flags.contains(MsgFlags::MSG_TRUNC) {
            return Err(ChannelFault::Truncated {
                bound: MAX_ENVELOPE_BYTES,
            });
        }
        let read = message.bytes;
        if read == 0 {
            return Err(ChannelFault::Closed);
        }
        serde_json::from_slice(&buffer[..read]).map_err(|_| ChannelFault::Undecodable)
    }

    /// **Reads one envelope within `bound`**, per section 3's rule that no verb
    /// holds the invocation lock past a bound it states: blocking is not
    /// unbounded, so the read waits on the descriptor for at most `bound` and
    /// answers `Unanswered` past it.
    pub fn recv_within(&self, bound: Duration) -> Result<OrganEnvelope, ChannelFault> {
        self.wait_readable(bound)?;
        self.recv()
    }

    /// The replacement observation keeps this invocation's sole channel reader.
    pub(crate) fn recv_observation_within(
        &self,
        bound: Duration,
    ) -> Result<weaver_types::state_flow::enter::ObservationEnvelope, ChannelFault> {
        self.wait_readable(bound)?;
        let mut buffer = vec![0u8; MAX_ENVELOPE_BYTES];
        let mut slices = [std::io::IoSliceMut::new(&mut buffer)];
        let message = nix::sys::socket::recvmsg::<()>(
            self.fd.as_raw_fd(),
            &mut slices,
            None,
            MsgFlags::empty(),
        )
        .map_err(|_| ChannelFault::Closed)?;
        if message.flags.contains(MsgFlags::MSG_TRUNC) {
            return Err(ChannelFault::Truncated {
                bound: MAX_ENVELOPE_BYTES,
            });
        }
        if message.flags.contains(MsgFlags::MSG_CTRUNC) {
            return Err(ChannelFault::Undecodable);
        }
        let read = message.bytes;
        if read == 0 {
            return Err(ChannelFault::Closed);
        }
        weaver_types::state_flow::enter::ObservationEnvelope::decode(&buffer[..read])
            .map_err(|_| ChannelFault::Undecodable)
    }

    fn wait_readable(&self, bound: Duration) -> Result<(), ChannelFault> {
        // A bound past what the clock can hold never panics: it waits as long
        // as poll's longest wait, again and again, which is the bound read
        // as written.
        let deadline = Instant::now().checked_add(bound);
        loop {
            let remaining = match deadline {
                Some(deadline) => deadline.saturating_duration_since(Instant::now()),
                None => Duration::MAX,
            };
            if remaining.is_zero() {
                return Err(ChannelFault::Unanswered);
            }
            // Rounded up, so less than a millisecond left waits one rather
            // than spinning on a zero timeout.
            let millis = remaining.as_nanos().div_ceil(1_000_000).min(poll_cap_ms()) as i32;
            let mut poll = nix::libc::pollfd {
                fd: self.fd.as_raw_fd(),
                events: nix::libc::POLLIN,
                revents: 0,
            };
            // SAFETY: poll over one pollfd this frame owns.
            match unsafe { nix::libc::poll(&mut poll, 1, millis) } {
                // **A timeout is read against the deadline, never alone**:
                // poll's longest wait is about 24.8 days, shorter than a
                // declared drain and wind-down can make the leave's window,
                // so a timeout with time left waits again; the loop's head
                // answers `Unanswered` once none is left.
                0 => {}
                n if n > 0 => return Ok(()),
                _ if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted => {}
                _ => return Err(ChannelFault::Closed),
            }
        }
    }

    /// Adopts a connected end, for a test peer standing in for the worker.
    // #118: a test-only hook, kept beside the production code it observes.
    #[cfg(test)]
    pub fn adopt(fd: OwnedFd) -> Self {
        Coordination { fd, ordinal: 0 }
    }
}

/// poll's longest wait, in milliseconds.
fn poll_cap_ms() -> u128 {
    // #118: a test-only hook, kept beside the production code it observes.
    #[cfg(test)]
    if let Some(cap) = TEST_POLL_CAP_MS.with(std::cell::Cell::get) {
        return cap;
    }
    i32::MAX as u128
}

// A shorter cap on poll's wait, so a test meets the cap inside a bound
// it can wait out.
// #118: a test-only hook, kept beside the production code it observes.
#[cfg(test)]
thread_local! {
    static TEST_POLL_CAP_MS: std::cell::Cell<Option<u128>> = const { std::cell::Cell::new(None) };
}

#[cfg(test)]
mod tests;

/// **Mint the run's reference**, per `weaver-admin-Spec` section 7.
///
/// Three parts, each doing one job: an instant so the reference reads as a
/// date and sorts into calendar order, the validated agent name so a reader
/// sees whose run it is without joining anything, and eight bytes from the
/// operating system's randomness, which is what carries the distinctness the
/// contract asks for.
///
/// **The clock does not carry the guarantee.** Wall-clock time is adjustable
/// and an adjustment can move it backwards, so no resolution makes two
/// instants certainly different. A builder who dropped the random part and
/// leaned on a finer clock would be trading a guarantee for a probability.
///
/// **A load that cannot read randomness refuses rather than weakening.** The
/// alternative is a reference that looks like the others and is not
/// guaranteed, which is worse than a refused load because nothing downstream
/// could tell.
pub fn mint_run_reference(agent: &str) -> Option<weaver_types::RunId> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?;
    let nonce = crate::random::random_hex(8).ok()?;
    Some(weaver_types::RunId(format!(
        "{}-{agent}-{nonce}",
        rfc3339_millis(now.as_millis() as i64)
    )))
}

/// An RFC 3339 instant in UTC at millisecond resolution, rendered without a
/// date library because this crate's dependency set is two crates and a
/// calendar is arithmetic. The civil-date conversion is the standard one from
/// days since the epoch.
fn rfc3339_millis(epoch_millis: i64) -> String {
    let (days, ms_of_day) = (
        epoch_millis.div_euclid(86_400_000),
        epoch_millis.rem_euclid(86_400_000),
    );
    let (hh, mm, ss, ms) = (
        ms_of_day / 3_600_000,
        (ms_of_day / 60_000) % 60,
        (ms_of_day / 1_000) % 60,
        ms_of_day % 1_000,
    );
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}.{ms:03}Z")
}

#[cfg(test)]
mod reference_tests;
