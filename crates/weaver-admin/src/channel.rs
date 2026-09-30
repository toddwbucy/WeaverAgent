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
//! section 2. What refuses a stranger is the credential the harness reads at
//! its accept, not anything here.
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
pub fn dial(socket_path: &Path) -> Result<Coordination, ChannelFault> {
    let deadline = Instant::now() + DIAL_CEILING;
    loop {
        match dial_once(socket_path) {
            Ok(connection) => return Ok(connection),
            // Not a transient absence: the name itself is unusable, so the
            // ceiling would spend a second reaching the same answer.
            Err(ChannelFault::Undecodable) => return Err(ChannelFault::Undecodable),
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

fn dial_once(socket_path: &Path) -> Result<Coordination, ChannelFault> {
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
        Err(_) => return Err(ChannelFault::NotDialable),
    }
    clear_nonblocking(&fd)?;
    Ok(Coordination { fd, ordinal: 0 })
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
        sendmsg::<()>(self.fd.as_raw_fd(), &slices, &[], MsgFlags::empty(), None)
            .map_err(|_| ChannelFault::Closed)?;
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
        let mut fds = vec![sink.as_raw_fd()];
        if let Some(end) = state_end {
            fds.push(end.as_raw_fd());
        }
        let control = [ControlMessage::ScmRights(&fds)];
        let slices = [std::io::IoSlice::new(&body)];
        sendmsg::<()>(
            self.fd.as_raw_fd(),
            &slices,
            &control,
            MsgFlags::empty(),
            None,
        )
        .map_err(|_| ChannelFault::Closed)?;
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

    /// Adopts a connected end, for a test peer standing in for the worker.
    #[cfg(test)]
    pub fn adopt(fd: OwnedFd) -> Self {
        Coordination { fd, ordinal: 0 }
    }
}

#[cfg(test)]
mod tests {
    //! The dial's bound and the channel's framing, run from inside the crate
    //! because this crate publishes no library surface.
    //!
    //! **The harness is stood in for here.** Under the inversion the worker
    //! binds and this crate dials, so every test below plays the harness's
    //! part with a listener of its own and exercises `dial` as the code under
    //! test.

    use super::*;
    use nix::sys::socket::{bind, listen};
    use weaver_types::{ExchangeId, LifecycleDirective, Opener, Payload, Position};

    fn scratch(tag: &str) -> crate::scratch::Scratch {
        let dir = crate::scratch::Scratch(std::env::temp_dir().join(format!(
            "weaver-admin-chan-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        )));
        let _ = std::fs::remove_dir_all(&dir.0);
        std::fs::create_dir_all(&dir.0).expect("scratch");
        dir
    }

    /// The harness's part: bind the per-agent name and listen. The real one
    /// does this inside the unit's runtime directory as its first act.
    fn harness_listener(dir: &std::path::Path) -> (OwnedFd, std::path::PathBuf) {
        let path = dir.join("alpha.sock");
        let fd = socket(
            AddressFamily::Unix,
            SockType::SeqPacket,
            SockFlag::SOCK_CLOEXEC,
            None,
        )
        .expect("socket");
        let address = UnixAddr::new(&path).expect("address");
        bind(fd.as_raw_fd(), &address).expect("bind");
        listen(&fd, nix::sys::socket::Backlog::new(1).unwrap()).expect("listen");
        (fd, path)
    }

    fn harness_accepts(listener: &OwnedFd) -> OwnedFd {
        let fd = nix::sys::socket::accept4(listener.as_raw_fd(), SockFlag::SOCK_CLOEXEC)
            .expect("accept");
        // SAFETY: the kernel just created this descriptor and no other owner
        // exists.
        unsafe { <OwnedFd as std::os::fd::FromRawFd>::from_raw_fd(fd) }
    }

    fn directive(ordinal: u64, agent: &str) -> OrganEnvelope {
        OrganEnvelope {
            exchange: ExchangeId {
                opener: Opener::Admin,
                ordinal,
            },
            position: Position::Open,
            payload: Payload::Directive(LifecycleDirective::Load {
                agent: weaver_types::AgentName(agent.to_string()),
            }),
        }
    }

    /// **The dial refuses within its bound rather than waiting.** A name
    /// nothing bound is the worker that never came up, and section 7's ceiling
    /// is what turns that into an answer.
    ///
    /// Perturbation: remove the deadline test from `dial`'s loop and this test
    /// never returns. Watched under exactly that removal.
    #[test]
    fn the_dial_refuses_within_its_bound() {
        let dir = scratch("bound");
        let path = dir.join("nothing-listens-here.sock");
        let started = std::time::Instant::now();
        let refused = dial(&path);
        let elapsed = started.elapsed();
        assert!(
            matches!(refused, Err(ChannelFault::NotDialable)),
            "a dial against an unbound name refuses, got {refused:?}"
        );
        assert!(
            elapsed >= DIAL_CEILING,
            "the dial retried for the whole bound, took {elapsed:?}"
        );
        assert!(
            elapsed < DIAL_CEILING * 4,
            "the dial stopped at its ceiling rather than waiting on, took {elapsed:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **A full backlog does not defeat the ceiling.** The harness serves one
    /// connection at a time, so a verb arriving while another is in flight
    /// meets a full backlog, and a blocking connect on an `AF_UNIX` socket
    /// blocks there without bound: measured 2026-08-06, still blocked after
    /// three seconds against a one second ceiling. The nonblocking connect is
    /// what turns that into a refusal.
    ///
    /// Perturbation: drop `SOCK_NONBLOCK` from `dial_once` and this test
    /// blocks past its own assertion rather than failing an equality, which is
    /// the hang the flag prevents. Watched under exactly that removal.
    #[test]
    fn a_full_backlog_refuses_within_the_bound() {
        let dir = scratch("backlog");
        let (listener, path) = harness_listener(&dir);
        // Fill the backlog: the listener never accepts, so these queue and
        // then the queue closes.
        let mut held = Vec::new();
        for _ in 0..8 {
            match dial(&path) {
                Ok(c) => held.push(c),
                Err(_) => break,
            }
        }
        let started = std::time::Instant::now();
        let refused = dial(&path);
        let elapsed = started.elapsed();
        assert!(
            matches!(refused, Err(ChannelFault::NotDialable)),
            "a full backlog refuses rather than blocking, got {refused:?}"
        );
        assert!(
            elapsed < DIAL_CEILING * 3,
            "and it refuses within the bound, took {elapsed:?}"
        );
        drop(held);
        drop(listener);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **The dial succeeds once the harness binds, which is the race the
    /// bound covers.** The listener appears after the dial has begun.
    #[test]
    fn the_dial_wins_the_race_when_the_bind_is_late() {
        let dir = scratch("race");
        let path = dir.join("alpha.sock");
        let bind_dir = dir.to_path_buf();
        let late = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(120));
            harness_listener(&bind_dir)
        });
        let connected = dial(&path);
        let (listener, _) = late.join().expect("thread");
        assert!(
            connected.is_ok(),
            "a bind inside the bound is reached, got {connected:?}"
        );
        drop(listener);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **One write is one read**, the boundary the socket type buys, tested
    /// where the connection is made.
    ///
    /// Perturbation: create the socket as `SOCK_STREAM` in `dial_once` and the
    /// first read returns both envelopes. **Two messages are what make the
    /// watch reachable** - a single small envelope crosses a stream socket
    /// whole, so a one-message test would pass under the substitution.
    #[test]
    fn one_write_is_one_read() {
        let dir = scratch("framing");
        let (listener, path) = harness_listener(&dir);
        let admin = dial(&path).expect("dial");
        let worker = harness_accepts(&listener);
        let worker = Coordination::adopt(worker);
        worker.send(&directive(1, "alpha")).expect("first write");
        worker.send(&directive(2, "beta")).expect("second write");
        let first = admin.recv().expect("first read decodes on its own");
        let second = admin.recv().expect("second read decodes on its own");
        assert_eq!(first.exchange.ordinal, 1);
        assert_eq!(second.exchange.ordinal, 2);
        drop(worker);
        drop(listener);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **Truncation is a fault**, never a message: a read returning with
    /// `MSG_TRUNC` set carries a prefix the kernel shortened.
    ///
    /// Perturbation: remove the `MSG_TRUNC` check from `recv` and the
    /// shortened octets reach the decoder.
    #[test]
    fn truncation_is_a_channel_fault() {
        let dir = scratch("truncate");
        let (listener, path) = harness_listener(&dir);
        let admin = dial(&path).expect("dial");
        let worker = harness_accepts(&listener);
        let oversized = vec![b'x'; MAX_ENVELOPE_BYTES + 4096];
        // SAFETY: a raw send is how the test produces a message larger than
        // the receiver's buffer; this crate's own send refuses to author one.
        let written = unsafe {
            nix::libc::send(
                worker.as_raw_fd(),
                oversized.as_ptr().cast(),
                oversized.len(),
                0,
            )
        };
        if written < 0 {
            eprintln!(
                "SKIP truncation_is_a_channel_fault: the kernel refused the oversized \
                 datagram (errno {}), so the MSG_TRUNC branch was not exercised here",
                std::io::Error::last_os_error()
            );
        } else {
            match admin.recv() {
                Err(ChannelFault::Truncated { bound }) => assert_eq!(bound, MAX_ENVELOPE_BYTES),
                other => panic!("a truncated read must be a fault, got {other:?}"),
            }
        }
        drop(worker);
        drop(listener);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The enter directive and its ancillary payload are one message: the
    /// descriptor crosses once, in the enter exchange, with no separate
    /// delivery to order against anything.
    #[test]
    fn the_enter_carries_its_descriptor_in_one_message() {
        use std::os::fd::AsFd;
        let dir = scratch("ancillary");
        let (listener, path) = harness_listener(&dir);
        let admin = dial(&path).expect("dial");
        let worker_fd = harness_accepts(&listener);

        let sink = std::fs::File::create(dir.join("sink")).expect("sink");
        let sink = OwnedFd::from(sink);
        admin
            .send_with_sink(&directive(7, "alpha"), sink.as_fd(), None)
            .expect("one message");

        // The worker reads one message and finds the descriptor on it.
        let mut buffer = vec![0u8; MAX_ENVELOPE_BYTES];
        let mut control = nix::cmsg_space!([std::os::fd::RawFd; 4]);
        let mut slices = [std::io::IoSliceMut::new(&mut buffer)];
        let message = nix::sys::socket::recvmsg::<()>(
            worker_fd.as_raw_fd(),
            &mut slices,
            Some(&mut control),
            MsgFlags::MSG_CMSG_CLOEXEC,
        )
        .expect("one message read");
        let mut received = 0;
        for cmsg in message.cmsgs().expect("control data") {
            if let nix::sys::socket::ControlMessageOwned::ScmRights(fds) = cmsg {
                received += fds.len();
                for fd in fds {
                    // SAFETY: adopting the descriptors the kernel installed so
                    // the test closes them.
                    drop(unsafe { <OwnedFd as std::os::fd::FromRawFd>::from_raw_fd(fd) });
                }
            }
        }
        assert_eq!(
            received, 1,
            "the sink crosses on the directive's own message"
        );
        assert!(message.bytes > 0, "the envelope crossed with it");
        drop(worker_fd);
        drop(listener);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

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
    let mut bytes = [0u8; 8];
    let mut file = std::fs::File::open("/dev/urandom").ok()?;
    std::io::Read::read_exact(&mut file, &mut bytes).ok()?;
    let nonce: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
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
mod reference_tests {
    use super::*;

    /// **Two references minted back to back differ**, which is the whole of
    /// what the contract's distinctness guarantee asks for. The instants can
    /// be equal and the agent is the same, so what this watches is the random
    /// part carrying the guarantee alone.
    ///
    /// Perturbation: drop the nonce from `mint_run_reference` and this fails,
    /// because two mints inside one millisecond render the same string.
    /// Watched under exactly that removal.
    #[test]
    fn two_references_for_one_agent_differ() {
        let a = mint_run_reference("alpha").expect("randomness reads");
        let b = mint_run_reference("alpha").expect("randomness reads");
        assert_ne!(a.0, b.0, "the reference is not distinct: {} {}", a.0, b.0);
    }

    /// The reference reads as a date and names its agent, which is what makes
    /// an artifact legible without a join.
    #[test]
    fn the_reference_reads_as_a_date_and_names_its_agent() {
        let r = mint_run_reference("alpha").expect("randomness reads").0;
        assert!(r.contains("-alpha-"), "the agent is not named: {r}");
        assert_eq!(&r[4..5], "-", "not a rendered date: {r}");
        assert!(r.contains('T') && r.contains("Z-"), "not RFC 3339: {r}");
    }

    /// The civil-date arithmetic is hand-rolled, so it is pinned against
    /// instants a reader can check by eye, including a leap day and the epoch.
    #[test]
    fn the_instant_renders_against_known_values() {
        assert_eq!(rfc3339_millis(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(
            rfc3339_millis(1_709_164_800_000),
            "2024-02-29T00:00:00.000Z"
        );
        assert_eq!(
            rfc3339_millis(1_786_665_731_482),
            "2026-08-14T00:02:11.482Z"
        );
    }
}
