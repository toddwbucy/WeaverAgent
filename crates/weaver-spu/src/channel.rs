//! conforms: spu-descriptors-owned-types
//! conforms: spu-dumpable-flag-cleared
//! conforms: spu-channel-ends-close-on-exec
//! conforms: spu-descriptor-count-check
//! conforms: spu-envelope-on-lifecycle-only
//! conforms: spu-truncation-is-a-fault
//!
//! The two channel ends and the process facts, per `weaver-spu-Spec` section 2.
//!
//! Both ends arrive inherited rather than re-decided: **3 is the lifecycle
//! channel and 4 is the decode socket**, the numbering and its order elected by
//! `weaver-harness-Spec` section 2.2 and owed to this document, which takes it.
//! Lifecycle sits first because it is the channel every organ has and the one an
//! organ with a single end already places there, so the gate's placement is
//! unchanged and this crate's first end sits where the gate's does.
//!
//! The envelope crosses the lifecycle end only. Per charter section 13.2 the
//! decode socket is not an organ channel, so [`DecodeSocket`] carries octets and
//! offers no envelope method at all: the confinement is the absence of the
//! surface rather than a rule a caller is asked to keep.

use std::os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, OwnedFd, RawFd};

use nix::dir::Dir;
use nix::fcntl::{FcntlArg, FdFlag, OFlag, fcntl};
use nix::sys::socket::{MsgFlags, recv, send};
use nix::sys::stat::Mode;
use weaver_types::{DECODE_MESSAGE_BOUND, MAX_ENVELOPE_BYTES, OrganEnvelope, SegmentPreamble};

/// The lifecycle channel's inherited number. Descriptor 3 is the channel every
/// organ has, which is why it is first.
pub const LIFECYCLE_FD: RawFd = 3;

/// The decode socket's inherited number, the second end the decoder cut gave
/// this crate.
pub const DECODE_FD: RawFd = 4;

/// What entry refuses on, before it serves anything.
#[derive(Debug, PartialEq)]
pub enum EntryFault {
    /// The count check failed: this process holds descriptors beyond the two it
    /// was given and the standard streams. A count above two means the harness's
    /// fork discipline failed upstream, and this process is not the one to
    /// continue past it. The count is reported rather than the identity, since
    /// this crate cannot know what a stray descriptor refers to.
    DescriptorCountWrong { found: usize },
    /// An inherited number is not an open descriptor, or the enumeration the
    /// count check needs could not be performed.
    DescriptorsUnusable,
    /// A hygiene set failed. Both sets are sets and never checks, so a failure
    /// here is a refusal rather than a report: a step that finds a flag wrong
    /// and continues leaves the process attachable, which is the condition the
    /// set exists to prevent.
    HygieneFailed,
}

/// The two ends, adopted and owned.
///
/// Descriptors are owned types end to end in this crate, so no raw number
/// outlives the thing it names and no close happens twice. The wrap is not a
/// formality: it is what `spu-descriptors-owned-types` pins.
#[derive(Debug)]
pub struct Inherited {
    pub lifecycle: LifecycleChannel,
    pub decode: DecodeSocket,
}

/// The first end: the organ channel with the harness, carrying the envelope.
#[derive(Debug)]
pub struct LifecycleChannel {
    end: OwnedFd,
}

/// The second end: the decode socket, which is not an organ channel.
///
/// It carries the token trio in whatever encoding the measurement of Spec
/// section 11 elects, so this type offers octets and no envelope. The election
/// is open and code filling it would be invention.
#[derive(Debug)]
pub struct DecodeSocket {
    end: OwnedFd,
}

/// Adopt both ends and perform the two sets, before the first read.
///
/// The order is the Spec's: the count check answers first, because a process
/// that failed the discipline upstream should refuse rather than adopt; then
/// the wrap; then close-on-exec on both ends; then the dumpable clear.
///
/// The close-on-exec set matters even though this crate forks nothing, because
/// `execve` clears the flag and the requirement is stated against the last exec.
pub fn adopt() -> Result<Inherited, EntryFault> {
    let held = descriptors_beyond_the_standard_streams()?;
    if held != 2 {
        return Err(EntryFault::DescriptorCountWrong { found: held });
    }

    // Both numbers must name open descriptors before either is wrapped: wrapping
    // first and failing second would drop a half-adopted pair through the
    // OwnedFd close, which is a close of a descriptor this process may not own.
    for fd in [LIFECYCLE_FD, DECODE_FD] {
        let borrowed = unsafe { BorrowedFd::borrow_raw(fd) };
        fcntl(borrowed, FcntlArg::F_GETFD).map_err(|_| EntryFault::DescriptorsUnusable)?;
    }

    // SAFETY: both numbers were confirmed open above, the count check confirmed
    // there are exactly two beyond the standard streams, and this is the one
    // place in the crate that constructs an owned handle from an inherited
    // number. Every later use flows from these two values.
    let lifecycle = unsafe { OwnedFd::from_raw_fd(LIFECYCLE_FD) };
    let decode = unsafe { OwnedFd::from_raw_fd(DECODE_FD) };

    set_close_on_exec(lifecycle.as_fd())?;
    set_close_on_exec(decode.as_fd())?;
    clear_dumpable()?;

    Ok(Inherited {
        lifecycle: LifecycleChannel { end: lifecycle },
        decode: DecodeSocket { end: decode },
    })
}

/// A set and never a check, per `weaver-organ-channel` section 2.
fn set_close_on_exec(end: BorrowedFd<'_>) -> Result<(), EntryFault> {
    fcntl(end, FcntlArg::F_SETFD(FdFlag::FD_CLOEXEC)).map_err(|_| EntryFault::HygieneFailed)?;
    Ok(())
}

/// The second walk's mechanism: a same-uid process attaching by `ptrace` or
/// through `/proc/[pid]/fd` reaches a process holding the weights and both
/// channels. Clearing the flag is what closes it, and it is a set rather than a
/// check for the same reason the neighbour above is.
fn clear_dumpable() -> Result<(), EntryFault> {
    nix::sys::prctl::set_dumpable(false).map_err(|_| EntryFault::HygieneFailed)
}

/// Count the open descriptors this process holds beyond the standard streams.
///
/// The enumeration itself opens a descriptor, so the directory handle is
/// excluded by its own number. Counting it would make the check answer three
/// where the truth is two, which is a count check that can never pass.
fn descriptors_beyond_the_standard_streams() -> Result<usize, EntryFault> {
    let mut dir = Dir::open(
        "/proc/self/fd",
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_CLOEXEC,
        Mode::empty(),
    )
    .map_err(|_| EntryFault::DescriptorsUnusable)?;
    let own = dir.as_raw_fd();

    let mut held = 0usize;
    for entry in dir.iter() {
        let entry = entry.map_err(|_| EntryFault::DescriptorsUnusable)?;
        // `.` and `..` fail the parse and are skipped by it rather than by name.
        let Ok(number) = entry.file_name().to_string_lossy().parse::<RawFd>() else {
            continue;
        };
        if number <= 2 || number == own {
            continue;
        }
        held += 1;
    }
    Ok(held)
}

/// What a channel operation faults on, per `weaver-spu-Spec` section 9, which
/// declares this enum's three cases and no fourth. Refusals are the floor's and
/// faults are below the exchange layer, so nothing here carries an exchange.
#[derive(Debug, PartialEq)]
pub enum ChannelFault {
    /// The message exceeded the envelope bound, in either direction: a read
    /// returning `MSG_TRUNC`, or this crate's own write exceeding the bound it
    /// asserts on its reads. A silently shortened directive is the failure mode
    /// the boundary property was elected to prevent, and a writer that can
    /// exceed the receiver's buffer produces the truncation the receiver is
    /// obliged to fault on.
    Truncated { bound: usize },
    /// The octets read are not an envelope.
    Undecodable,
    /// The peer's end is closed.
    ///
    /// On a `SOCK_SEQPACKET` socket a shutdown peer reads as a zero-length
    /// datagram rather than as an error, so it is distinguished here rather
    /// than left to look like a message. Every message in this protocol is a
    /// JSON envelope and none is empty, so a zero-length read is unambiguous.
    /// A send the socket refuses lands here too, the peer being unreachable
    /// either way.
    Closed,
}

impl LifecycleChannel {
    /// Send one envelope. The bound is asserted on this crate's own writes, not
    /// only on what it reads, because a writer that can exceed the receiver's
    /// buffer produces the truncation the receiver is obliged to fault on.
    pub fn send(&self, envelope: &OrganEnvelope) -> Result<(), ChannelFault> {
        let body = serde_json::to_vec(envelope).map_err(|_| ChannelFault::Undecodable)?;
        if body.len() > MAX_ENVELOPE_BYTES {
            return Err(ChannelFault::Truncated {
                bound: MAX_ENVELOPE_BYTES,
            });
        }
        send_octets(self.end.as_fd(), &body)
    }

    /// Receive one envelope, faulting on truncation.
    pub fn recv(&self) -> Result<OrganEnvelope, ChannelFault> {
        let body = recv_octets(self.end.as_fd())?;
        serde_json::from_slice(&body).map_err(|_| ChannelFault::Undecodable)
    }

    pub fn as_fd(&self) -> BorrowedFd<'_> {
        self.end.as_fd()
    }
}

impl DecodeSocket {
    /// Send one message, segmenting past the envelope per the decode
    /// contract's segment series: a frame within the envelope crosses as it
    /// always did, a larger one crosses as the preamble and its counted raw
    /// slices, and one past the total bound is this side's own fault. No
    /// envelope method exists on this type: the decode socket is not an
    /// organ channel and the confinement is the absent surface.
    pub fn send_octets(&self, body: &[u8]) -> Result<(), ChannelFault> {
        if body.len() <= MAX_ENVELOPE_BYTES {
            return send_octets(self.end.as_fd(), body);
        }
        if body.len() > DECODE_MESSAGE_BOUND {
            return Err(ChannelFault::Truncated {
                bound: DECODE_MESSAGE_BOUND,
            });
        }
        let preamble = SegmentPreamble {
            segments: body.len().div_ceil(MAX_ENVELOPE_BYTES) as u64,
            bytes: body.len() as u64,
        };
        let preamble = serde_json::to_vec(&preamble).map_err(|_| ChannelFault::Undecodable)?;
        send_octets(self.end.as_fd(), &preamble)?;
        for slice in body.chunks(MAX_ENVELOPE_BYTES) {
            send_octets(self.end.as_fd(), slice)?;
        }
        Ok(())
    }

    /// Receive one message, faulting on truncation on the same grounds the
    /// lifecycle end does, and reassembling a segment series into the one
    /// frame it carries: the series is the channel's fact and no caller
    /// sees a segment.
    pub fn recv_octets(&self) -> Result<Vec<u8>, ChannelFault> {
        let first = recv_octets(self.end.as_fd())?;
        reassemble_if_series(first, || recv_octets(self.end.as_fd()))
    }

    /// Receive one message without blocking, for the cancel poll at token
    /// boundaries: a boundary check that blocked would defeat the bound it
    /// exists to provide, per `weaver-spu-Spec` section 4.3's polled cancel.
    /// `None` is the quiet channel, distinct from every fault. A preamble
    /// arriving here consumes its declared slices before returning, the
    /// series being one message to every rule: once the first frame is in
    /// hand the remainder is already sent and ordered, so the completion
    /// blocks only on datagrams in flight.
    pub fn try_recv_octets(&self) -> Result<Option<Vec<u8>>, ChannelFault> {
        match recv_with(self.end.as_fd(), MsgFlags::MSG_DONTWAIT)? {
            None => Ok(None),
            Some(first) => reassemble_if_series(first, || recv_octets(self.end.as_fd())).map(Some),
        }
    }

    pub fn as_fd(&self) -> BorrowedFd<'_> {
        self.end.as_fd()
    }
}

/// The classify process's one end, per `weaver-spu-Spec` section 11: the
/// label seam at descriptor 3, the count check at one, the same hygiene
/// sets, the same receive obligation. A count above one means the fork
/// discipline upstream failed and this process is not the one to continue
/// past it.
pub const CLASSIFY_FD: RawFd = 3;

/// The label seam's end, per `weaver-harness-spu-classify-contract`
/// section 1: not an organ channel, so this type offers octets and no
/// envelope, the confinement being the absent surface.
#[derive(Debug)]
pub struct ClassifySocket {
    end: OwnedFd,
}

impl ClassifySocket {
    /// Send one message, the bound asserted on this side's own writes.
    pub fn send_octets(&self, body: &[u8]) -> Result<(), ChannelFault> {
        if body.len() > MAX_ENVELOPE_BYTES {
            return Err(ChannelFault::Truncated {
                bound: MAX_ENVELOPE_BYTES,
            });
        }
        send_octets(self.end.as_fd(), body)
    }

    /// Receive one message, `MSG_TRUNC` a channel fault and never a message.
    pub fn recv_octets(&self) -> Result<Vec<u8>, ChannelFault> {
        recv_octets(self.end.as_fd())
    }
}

/// Adopt the classify process's one end and perform the same sets the
/// two-end adoption performs, in the same order: count, wrap, close-on-exec,
/// dumpable clear.
pub fn adopt_classify() -> Result<ClassifySocket, EntryFault> {
    let held = descriptors_beyond_the_standard_streams()?;
    if held != 1 {
        return Err(EntryFault::DescriptorCountWrong { found: held });
    }
    let borrowed = unsafe { BorrowedFd::borrow_raw(CLASSIFY_FD) };
    fcntl(borrowed, FcntlArg::F_GETFD).map_err(|_| EntryFault::DescriptorsUnusable)?;
    // SAFETY: the number was confirmed open above, the count check confirmed
    // exactly one beyond the standard streams, and this is the classify
    // binary's one construction of an owned handle from an inherited number.
    let end = unsafe { OwnedFd::from_raw_fd(CLASSIFY_FD) };
    set_close_on_exec(end.as_fd())?;
    clear_dumpable()?;
    Ok(ClassifySocket { end })
}

/// Reassemble a segment series where the first frame is its preamble, per
/// `weaver-types-Spec` section 4.4. The kindless shape is reserved rather
/// than guessed: a JSON object without a `kind` member is either exactly
/// the two-integer preamble or a channel fault, and the preamble validates
/// whole before the first slice is read - the byte length past the envelope
/// and within the total bound, the count exactly what the length requires
/// at the envelope size, the slices totaling exactly the declared bytes.
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
    let Ok(preamble) = serde_json::from_slice::<SegmentPreamble>(&first) else {
        return Err(ChannelFault::Undecodable);
    };
    let Ok(bytes) = usize::try_from(preamble.bytes) else {
        return Err(ChannelFault::Undecodable);
    };
    if bytes <= MAX_ENVELOPE_BYTES || bytes > DECODE_MESSAGE_BOUND {
        return Err(ChannelFault::Undecodable);
    }
    if preamble.segments != bytes.div_ceil(MAX_ENVELOPE_BYTES) as u64 {
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

fn send_octets(end: BorrowedFd<'_>, body: &[u8]) -> Result<(), ChannelFault> {
    loop {
        match send(end.as_raw_fd(), body, MsgFlags::empty()) {
            Ok(_) => return Ok(()),
            // A signal landing mid-call is not the peer's doing: retry rather
            // than report a closed channel that is not closed.
            Err(nix::errno::Errno::EINTR) => continue,
            Err(_) => return Err(ChannelFault::Closed),
        }
    }
}

/// The receive obligation the `SOCK_SEQPACKET` election attaches: a buffer sized
/// to the envelope bound, and a read returning `MSG_TRUNC` treated as a channel
/// fault.
///
/// The buffer is deliberately exactly the bound rather than the bound plus one.
/// `MSG_TRUNC` is what reports the overflow, so a message of exactly the bound
/// fits and reads clean, and anything past it sets the flag.
fn recv_octets(end: BorrowedFd<'_>) -> Result<Vec<u8>, ChannelFault> {
    recv_with(end, MsgFlags::empty())
        .map(|frame| frame.expect("a blocking receive returns a frame or a fault"))
}

/// The one receive, parameterized by blocking: the buffer, the retry on a
/// signal, the truncation report, and the zero-length closure reading live
/// here once, and `Ok(None)` is the quiet channel on the non-blocking path
/// alone.
fn recv_with(end: BorrowedFd<'_>, flags: MsgFlags) -> Result<Option<Vec<u8>>, ChannelFault> {
    let mut buffer = vec![0u8; MAX_ENVELOPE_BYTES];
    let read = loop {
        match recv(end.as_raw_fd(), &mut buffer, MsgFlags::MSG_TRUNC | flags) {
            Ok(read) => break read,
            Err(nix::errno::Errno::EINTR) => continue,
            Err(nix::errno::Errno::EAGAIN) if flags.contains(MsgFlags::MSG_DONTWAIT) => {
                return Ok(None);
            }
            Err(_) => return Err(ChannelFault::Closed),
        }
    };
    if read > buffer.len() {
        // With MSG_TRUNC on a SEQPACKET socket the kernel returns the real
        // datagram length rather than the copied length, so a return above the
        // buffer is the truncation report.
        return Err(ChannelFault::Truncated {
            bound: MAX_ENVELOPE_BYTES,
        });
    }
    if read == 0 {
        return Err(ChannelFault::Closed);
    }
    buffer.truncate(read);
    Ok(Some(buffer))
}

/// Construct a lifecycle end from an already-owned descriptor.
///
/// This exists for the suite, which needs a channel over a socketpair it made
/// rather than over an inherited number. It is not an adoption path: [`adopt`]
/// remains the one place an inherited number becomes owned.
#[doc(hidden)]
pub fn lifecycle_from_owned(end: OwnedFd) -> LifecycleChannel {
    LifecycleChannel { end }
}

/// The decode-end counterpart of [`lifecycle_from_owned`], on the same grounds.
#[doc(hidden)]
pub fn decode_from_owned(end: OwnedFd) -> DecodeSocket {
    DecodeSocket { end }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nix::sys::socket::{AddressFamily, SockFlag, SockType, socketpair};

    fn seqpacket_pair() -> (OwnedFd, OwnedFd) {
        socketpair(
            AddressFamily::Unix,
            SockType::SeqPacket,
            None,
            SockFlag::SOCK_CLOEXEC,
        )
        .expect("a socketpair")
    }

    /// **A read returning `MSG_TRUNC` is a channel fault and never a message.**
    ///
    /// A message larger than the envelope bound is written directly to the
    /// socket, bypassing this crate's own send-side bound, which is exactly the
    /// case a misbehaving or mismatched peer produces. The receiver must fault
    /// rather than hand a silently shortened body to a parser.
    ///
    /// Perturbation: delete the `read > buffer.len()` branch from `recv_octets`
    /// and this test fails, because the oversized message then returns as a
    /// truncated body instead of a fault. Watched under exactly that removal.
    #[test]
    fn an_oversized_message_is_a_fault_rather_than_a_short_body() {
        let (writer, reader) = seqpacket_pair();
        let socket = decode_from_owned(reader);

        let oversized = vec![b'x'; MAX_ENVELOPE_BYTES + 1];
        // Written past this crate's own send bound on purpose: the receive
        // obligation is what is under test, not the send assertion.
        send(writer.as_raw_fd(), &oversized, MsgFlags::empty()).expect("the datagram is sent");

        assert_eq!(
            socket.recv_octets(),
            Err(ChannelFault::Truncated {
                bound: MAX_ENVELOPE_BYTES
            }),
            "an oversized datagram faults rather than arriving short, naming the bound"
        );
    }

    /// A message of exactly the bound fits and reads clean, which is what makes
    /// the buffer exactly the bound rather than the bound plus one.
    #[test]
    fn a_message_of_exactly_the_bound_reads_clean() {
        let (writer, reader) = seqpacket_pair();
        let socket = decode_from_owned(reader);

        let exact = vec![b'y'; MAX_ENVELOPE_BYTES];
        send(writer.as_raw_fd(), &exact, MsgFlags::empty()).expect("the datagram is sent");

        assert_eq!(socket.recv_octets(), Ok(exact));
    }

    /// **This crate asserts the bound on its own writes too**, and since the
    /// segment series the bound it asserts is the total one: a write past
    /// the envelope segments rather than refusing, per the decode
    /// contract's amendment on issue #236, and a write past
    /// `DECODE_MESSAGE_BOUND` is this side's own fault, refused before the
    /// socket.
    ///
    /// Perturbation: delete the total-bound branch from
    /// `DecodeSocket::send_octets` and this test fails. Watched under
    /// exactly that removal.
    #[test]
    fn a_write_past_the_bound_refuses_before_it_reaches_the_socket() {
        let (writer, _reader) = seqpacket_pair();
        let socket = decode_from_owned(writer);
        let oversized = vec![b'z'; DECODE_MESSAGE_BOUND + 1];
        assert_eq!(
            socket.send_octets(&oversized),
            Err(ChannelFault::Truncated {
                bound: DECODE_MESSAGE_BOUND
            })
        );
    }
}

#[cfg(test)]
mod series_tests {
    use super::*;
    use nix::sys::socket::{AddressFamily, SockFlag, SockType, socketpair};
    use std::os::fd::OwnedFd;

    fn seqpacket_pair() -> (OwnedFd, OwnedFd) {
        let (a, b) = socketpair(
            AddressFamily::Unix,
            SockType::SeqPacket,
            None,
            SockFlag::empty(),
        )
        .expect("pair");
        (a, b)
    }

    /// **A close past the envelope arrives byte-identical**, the contract's
    /// checkable: the series is the channel's fact and the caller sees the
    /// one frame, watched to fail when the carriage is removed.
    #[test]
    fn an_oversized_frame_round_trips_byte_identical() {
        let (a, b) = seqpacket_pair();
        let sender = DecodeSocket { end: a };
        let receiver = DecodeSocket { end: b };
        let body: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
        sender.send_octets(&body).expect("segments");
        let back = receiver.recv_octets().expect("reassembles");
        assert_eq!(back, body, "byte-identical through the series");
        // And a small frame still crosses as one write.
        sender
            .send_octets(b"{\"kind\":\"at_rest\"}")
            .expect("plain");
        assert_eq!(
            receiver.recv_octets().expect("plain"),
            b"{\"kind\":\"at_rest\"}"
        );
    }

    /// **An interrupted series faults whole**: a peer closing before its
    /// declared count leaves no partial frame, the contract's checkable.
    #[test]
    fn an_interrupted_series_faults_whole() {
        let (a, b) = seqpacket_pair();
        let receiver = DecodeSocket { end: b };
        {
            let sender = DecodeSocket { end: a };
            let preamble = format!("{{\"segments\":4,\"bytes\":{}}}", 200_000);
            super::send_octets(sender.end.as_fd(), preamble.as_bytes()).expect("preamble");
            super::send_octets(sender.end.as_fd(), &[7u8; MAX_ENVELOPE_BYTES]).expect("one");
            // the sender dies here, two slices short
        }
        assert!(matches!(receiver.recv_octets(), Err(ChannelFault::Closed)));
    }

    /// The preamble validates whole before any slice is read: the kindless
    /// shape is reserved, and every inconsistent spelling refuses.
    #[test]
    fn the_preamble_validates_before_any_slice() {
        let fault = |first: &str| {
            matches!(
                reassemble_if_series(first.as_bytes().to_vec(), || panic!("no slice read")),
                Err(ChannelFault::Undecodable)
            )
        };
        // a kindful frame passes through untouched
        let through = reassemble_if_series(b"{\"kind\":\"at_rest\"}".to_vec(), || {
            panic!("no slice read")
        })
        .expect("passes");
        assert_eq!(through, b"{\"kind\":\"at_rest\"}");
        // non-JSON and non-object pass through for the caller to judge
        assert!(reassemble_if_series(b"not json".to_vec(), || panic!()).is_ok());
        assert!(reassemble_if_series(b"[1,2]".to_vec(), || panic!()).is_ok());
        // kindless and not the exact preamble: refused, never guessed
        assert!(fault("{\"segments\":2}"), "one member");
        assert!(
            fault("{\"segments\":2,\"bytes\":200000,\"extra\":1}"),
            "three members"
        );
        assert!(
            fault("{\"segments\":-2,\"bytes\":200000}"),
            "negative count"
        );
        // bytes within one envelope: the sender had no series to send
        assert!(fault("{\"segments\":1,\"bytes\":100}"));
        // bytes past the total bound
        assert!(fault(&format!(
            "{{\"segments\":129,\"bytes\":{}}}",
            DECODE_MESSAGE_BOUND + 1
        )));
        // a count inconsistent with the length
        assert!(fault("{\"segments\":5,\"bytes\":200000}"), "too many");
        assert!(fault("{\"segments\":2,\"bytes\":200000}"), "too few");
    }

    /// A series whose slices exceed or fall short of the declared bytes is
    /// a channel fault, never a partial frame.
    #[test]
    fn a_series_totals_exactly_its_declared_bytes() {
        let slices = vec![vec![1u8; MAX_ENVELOPE_BYTES], vec![2u8; 10]];
        let mut feed = slices.clone().into_iter();
        let preamble = format!("{{\"segments\":2,\"bytes\":{}}}", MAX_ENVELOPE_BYTES + 10);
        let whole =
            reassemble_if_series(preamble.into_bytes(), || Ok(feed.next().expect("scripted")))
                .expect("exact total reassembles");
        assert_eq!(whole.len(), MAX_ENVELOPE_BYTES + 10);
        let mut feed = vec![vec![1u8; MAX_ENVELOPE_BYTES], vec![2u8; 11]].into_iter();
        let preamble = format!("{{\"segments\":2,\"bytes\":{}}}", MAX_ENVELOPE_BYTES + 10);
        assert!(matches!(
            reassemble_if_series(preamble.into_bytes(), || Ok(feed.next().expect("scripted"))),
            Err(ChannelFault::Undecodable)
        ));
    }
}
