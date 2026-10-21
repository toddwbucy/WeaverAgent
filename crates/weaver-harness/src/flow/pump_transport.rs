//! Bounded transport owners for HARNESS-PUMP's existing packet seams.
//!
//! These adapters consume a channel rather than cloning its read handle. Each
//! send or receive call attempts at most one datagram operation, retains partial
//! work, and holds at most one typed completion. The caller owns exchange order,
//! deadlines, rotation, trace authoring and completion reduction. In particular,
//! organ channels have two initiators: incoming traffic is legal while sending.
//!
//! Ordinary organ traffic admits no ancillary handles. Preflight Enter's sink
//! and state descriptors still belong to channel.rs::recv_with_descriptor until
//! the paired P1d migration. That owner must finish custody before conversion.
//! Decode retains the existing SegmentPreamble wire, not the STE100 framing.

use std::os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, OwnedFd, RawFd};

use nix::errno::Errno;
use nix::fcntl::{FcntlArg, FdFlag, fcntl};
use nix::libc;
use nix::poll::PollFlags;
use nix::sys::socket::{
    MsgFlags, Shutdown, SockType, UnixAddr, getpeername, getsockname, getsockopt, send, shutdown,
    sockopt,
};
use weaver_types::{
    DECODE_MESSAGE_BOUND, LabelDirective, MAX_ENVELOPE_BYTES, OrganEnvelope, SegmentPreamble,
    TokenDirective,
};

use crate::channel::{ClassifyChannel, ClassifyReply, DecodeChannel, DecodeReply, OrganChannel};
use crate::failure::ChannelFault;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Failure {
    pub fault: ChannelFault,
    pub detail: String,
    pub errno: Option<i32>,
}

impl Failure {
    fn closed(detail: impl Into<String>) -> Self {
        Self {
            fault: ChannelFault::Closed,
            detail: detail.into(),
            errno: None,
        }
    }
    fn invalid(detail: impl Into<String>) -> Self {
        Self {
            fault: ChannelFault::Undecodable,
            detail: detail.into(),
            errno: None,
        }
    }
    fn truncated(bound: usize) -> Self {
        Self {
            fault: ChannelFault::Truncated { bound },
            detail: "packet exceeds its wire bound".into(),
            errno: None,
        }
    }
    fn io(error: Errno) -> Self {
        Self {
            fault: ChannelFault::Closed,
            detail: error.to_string(),
            errno: Some(error as i32),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Interest {
    pub readable: bool,
    pub writable: bool,
    live: bool,
}

impl Interest {
    /// The pump registers every nonempty set. A full completion slot retains
    /// POLLRDHUP without POLLIN, so a queued second packet cannot spin the pump
    /// while a peer's write-half close still wakes its terminal observation.
    pub(super) fn events(self) -> PollFlags {
        if !self.live {
            return PollFlags::empty();
        }
        let mut events = PollFlags::from_bits_retain(libc::POLLRDHUP);
        if self.readable {
            events |= PollFlags::POLLIN;
        }
        if self.writable {
            events |= PollFlags::POLLOUT;
        }
        events
    }
}

#[derive(Debug, PartialEq)]
pub(super) enum Stage<T> {
    Queued,
    Busy(T),
}

#[derive(Debug)]
pub(super) struct StageFailure<T> {
    pub input: T,
    pub failure: Failure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ReceiveProgress {
    Pending,
    Progress,
    Held,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SendProgress {
    Pending,
    Progress,
    Complete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Wire {
    Packet,
    Decode,
}

#[derive(Debug)]
struct Outbound {
    body: Vec<u8>,
    preamble: Option<Vec<u8>>,
    preamble_sent: bool,
    offset: usize,
}

#[derive(Debug)]
struct Series {
    bytes: Vec<u8>,
    total: usize,
    segments: u64,
    received: u64,
}

/// One owner for an existing typed seam. The public aliases below fix the
/// codec and wire election, so callers cannot supply a different framing rule.
pub(super) struct PacketPort<T> {
    end: OwnedFd,
    wire: Wire,
    decode: fn(&[u8]) -> Result<T, Failure>,
    outgoing: Option<Outbound>,
    series: Option<Series>,
    // The last complete packet is retained on decode failure. A held whole
    // message retains its exact wire bytes until its typed completion is taken.
    received_wire: Option<Vec<u8>>,
    held: Option<T>,
    failure: Option<Failure>,
    receive_interrupted: bool,
    send_interrupted: bool,
    zero_received: bool,
}

pub(super) type OrganPort = PacketPort<OrganEnvelope>;
pub(super) type DecodePort = PacketPort<DecodeReply>;
pub(super) type LabelPort = PacketPort<ClassifyReply>;

impl OrganPort {
    pub(super) fn from_channel(channel: OrganChannel) -> Result<Self, Failure> {
        Self::from_owned(channel.into_fd())
    }
    pub(super) fn from_owned(end: OwnedFd) -> Result<Self, Failure> {
        Self::new(end, Wire::Packet, |bytes| {
            serde_json::from_slice(bytes).map_err(|error| Failure::invalid(error.to_string()))
        })
    }
    // Failure returns the intact owned envelope without a second allocation on
    // an already failing path. Boxing this 216-byte error would weaken that.
    #[allow(clippy::result_large_err)]
    pub(super) fn stage(
        &mut self,
        input: OrganEnvelope,
    ) -> Result<Stage<OrganEnvelope>, StageFailure<OrganEnvelope>> {
        self.stage_encoded(input, serde_json::to_vec)
    }
}

impl DecodePort {
    pub(super) fn from_channel(channel: DecodeChannel) -> Result<Self, Failure> {
        Self::from_owned(channel.into_fd())
    }
    pub(super) fn from_owned(end: OwnedFd) -> Result<Self, Failure> {
        Self::new(end, Wire::Decode, |bytes| {
            if let Ok(answer) = serde_json::from_slice(bytes) {
                return Ok(DecodeReply::Answer(answer));
            }
            serde_json::from_slice(bytes)
                .map(DecodeReply::Refusal)
                .map_err(|error| Failure::invalid(error.to_string()))
        })
    }
    pub(super) fn stage(
        &mut self,
        input: TokenDirective,
    ) -> Result<Stage<TokenDirective>, StageFailure<TokenDirective>> {
        self.stage_encoded(input, serde_json::to_vec)
    }
}

impl LabelPort {
    pub(super) fn from_channel(channel: ClassifyChannel) -> Result<Self, Failure> {
        let end = channel.into_pump_fd().map_err(|_retired| {
            Failure::closed("retired label channel cannot acquire a new transport owner")
        })?;
        Self::from_owned(end)
    }
    pub(super) fn from_owned(end: OwnedFd) -> Result<Self, Failure> {
        Self::new(end, Wire::Packet, |bytes| {
            if let Ok(answer) = serde_json::from_slice(bytes) {
                return Ok(ClassifyReply::Answer(answer));
            }
            serde_json::from_slice(bytes)
                .map(ClassifyReply::Refusal)
                .map_err(|error| Failure::invalid(error.to_string()))
        })
    }
    pub(super) fn stage(
        &mut self,
        input: LabelDirective,
    ) -> Result<Stage<LabelDirective>, StageFailure<LabelDirective>> {
        self.stage_encoded(input, serde_json::to_vec)
    }
}

impl<T> PacketPort<T> {
    fn new(
        end: OwnedFd,
        wire: Wire,
        decode: fn(&[u8]) -> Result<T, Failure>,
    ) -> Result<Self, Failure> {
        if getsockopt(&end, sockopt::SockType).map_err(Failure::io)? != SockType::SeqPacket
            || getsockname::<UnixAddr>(end.as_raw_fd()).is_err()
            || getpeername::<UnixAddr>(end.as_raw_fd()).is_err()
        {
            return Err(Failure::closed(
                "transport requires a connected Unix SEQPACKET end",
            ));
        }
        let flags = fcntl(&end, FcntlArg::F_GETFD).map_err(Failure::io)?;
        fcntl(
            &end,
            FcntlArg::F_SETFD(FdFlag::from_bits_retain(flags) | FdFlag::FD_CLOEXEC),
        )
        .map_err(Failure::io)?;
        Ok(Self {
            end,
            wire,
            decode,
            outgoing: None,
            series: None,
            received_wire: None,
            held: None,
            failure: None,
            receive_interrupted: false,
            send_interrupted: false,
            zero_received: false,
        })
    }

    pub(super) fn fd(&self) -> BorrowedFd<'_> {
        self.end.as_fd()
    }

    pub(super) fn interest(&self) -> Interest {
        Interest {
            readable: self.failure.is_none() && self.held.is_none(),
            writable: self.failure.is_none() && self.outgoing.is_some(),
            live: self.failure.is_none(),
        }
    }

    /// Interrupted work is revisited even if the consumed observation no longer
    /// makes the fd readable. A partial series alone waits for its next packet.
    pub(super) fn needs_progress(&self) -> bool {
        self.failure.is_none()
            && (self.receive_interrupted || self.send_interrupted || self.zero_received)
    }

    pub(super) fn failure(&self) -> Option<&Failure> {
        self.failure.as_ref()
    }
    pub(super) fn held(&self) -> Option<&T> {
        self.held.as_ref()
    }

    /// Transfer already received evidence even if a later probe observed EOF.
    /// A final release/refusal can precede an orderly exit. Only the caller can
    /// judge that order. Taking it grants no decision permission and never
    /// clears a latched fault or permits another read or send.
    pub(super) fn take_completion(&mut self) -> Result<Option<T>, Failure> {
        let completion = self.held.take();
        if completion.is_some() {
            self.received_wire = None;
            return Ok(completion);
        }
        self.live()?;
        Ok(None)
    }

    fn live(&self) -> Result<(), Failure> {
        self.failure.clone().map_or(Ok(()), Err)
    }

    fn fail(&mut self, failure: Failure) -> Failure {
        if self.failure.is_none() {
            self.failure = Some(failure);
            let _ = shutdown(self.end.as_raw_fd(), Shutdown::Both);
        }
        self.failure.as_ref().expect("first failure held").clone()
    }

    fn stage_encoded<I>(
        &mut self,
        input: I,
        encode: impl FnOnce(&I) -> Result<Vec<u8>, serde_json::Error>,
    ) -> Result<Stage<I>, StageFailure<I>> {
        if let Err(failure) = self.live() {
            return Err(StageFailure { input, failure });
        }
        if self.outgoing.is_some() {
            return Ok(Stage::Busy(input));
        }
        let prepared = (|| {
            let body = encode(&input).map_err(|error| Failure::invalid(error.to_string()))?;
            let limit = match self.wire {
                Wire::Packet => MAX_ENVELOPE_BYTES,
                Wire::Decode => DECODE_MESSAGE_BOUND,
            };
            if body.is_empty() {
                return Err(Failure::invalid("empty outgoing message"));
            }
            if body.len() > limit {
                return Err(Failure::truncated(limit));
            }
            let preamble = if body.len() > MAX_ENVELOPE_BYTES {
                Some(
                    serde_json::to_vec(&SegmentPreamble {
                        segments: body.len().div_ceil(MAX_ENVELOPE_BYTES) as u64,
                        bytes: body.len() as u64,
                    })
                    .map_err(|error| Failure::invalid(error.to_string()))?,
                )
            } else {
                None
            };
            Ok(Outbound {
                body,
                preamble,
                preamble_sent: false,
                offset: 0,
            })
        })();
        match prepared {
            Ok(outbound) => {
                self.outgoing = Some(outbound);
                Ok(Stage::Queued)
            }
            Err(failure) => Err(StageFailure {
                input,
                failure: self.fail(failure),
            }),
        }
    }

    pub(super) fn try_send(&mut self) -> Result<SendProgress, Failure> {
        self.send_with(send)
    }

    fn send_with(
        &mut self,
        transmit: impl FnOnce(RawFd, &[u8], MsgFlags) -> nix::Result<usize>,
    ) -> Result<SendProgress, Failure> {
        self.live()?;
        // An observed terminal/empty packet is unresolved, not permission to
        // send more bytes while its non-consuming status probe is interrupted.
        if self.zero_received {
            return Ok(SendProgress::Pending);
        }
        let Some(outbound) = self.outgoing.as_mut() else {
            return Ok(SendProgress::Complete);
        };
        let is_preamble = outbound.preamble.is_some() && !outbound.preamble_sent;
        let packet = if is_preamble {
            outbound
                .preamble
                .as_ref()
                .expect("preamble exists")
                .as_slice()
        } else {
            &outbound.body[outbound.offset
                ..outbound
                    .body
                    .len()
                    .min(outbound.offset + MAX_ENVELOPE_BYTES)]
        };
        let sent = match transmit(
            self.end.as_raw_fd(),
            packet,
            MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_NOSIGNAL,
        ) {
            Ok(sent) => {
                self.send_interrupted = false;
                sent
            }
            Err(Errno::EAGAIN) => {
                self.send_interrupted = false;
                return Ok(SendProgress::Pending);
            }
            Err(Errno::EINTR) => {
                self.send_interrupted = true;
                return Ok(SendProgress::Pending);
            }
            Err(error) => return Err(self.fail(Failure::io(error))),
        };
        if sent != packet.len() {
            return Err(self.fail(Failure::closed("SEQPACKET send was not atomic")));
        }
        if is_preamble {
            outbound.preamble_sent = true;
        } else {
            outbound.offset += sent;
        }
        if outbound.offset == outbound.body.len() {
            self.outgoing = None;
            Ok(SendProgress::Complete)
        } else {
            Ok(SendProgress::Progress)
        }
    }

    pub(super) fn try_receive(&mut self) -> Result<ReceiveProgress, Failure> {
        self.receive_with(receive_packet, peer_status)
    }

    fn receive_with(
        &mut self,
        receive: impl FnOnce(RawFd) -> Result<Packet, ReadFault>,
        status: impl FnOnce(RawFd) -> nix::Result<i16>,
    ) -> Result<ReceiveProgress, Failure> {
        self.live()?;
        if self.held.is_some() || self.zero_received {
            return self.resolve_status(status);
        }
        let packet = match receive(self.end.as_raw_fd()) {
            Ok(packet) => {
                self.receive_interrupted = false;
                packet
            }
            Err(ReadFault::Pending) => {
                self.receive_interrupted = false;
                return Ok(ReceiveProgress::Pending);
            }
            Err(ReadFault::Interrupted) => {
                self.receive_interrupted = true;
                return Ok(ReceiveProgress::Pending);
            }
            Err(ReadFault::Failed(failure)) => return Err(self.fail(failure)),
        };
        if packet.bytes.is_empty() {
            self.zero_received = true;
            // No second datagram receive. This immediate status probe decides
            // whether recv(0) was an empty packet or a closed peer.
            return self.resolve_status(status);
        }
        self.received_wire = Some(packet.bytes);
        self.accept_packet().map_err(|failure| self.fail(failure))
    }

    fn resolve_status(
        &mut self,
        status: impl FnOnce(RawFd) -> nix::Result<i16>,
    ) -> Result<ReceiveProgress, Failure> {
        let events = match status(self.end.as_raw_fd()) {
            Ok(events) => {
                self.receive_interrupted = false;
                events
            }
            Err(Errno::EINTR) => {
                self.receive_interrupted = true;
                return Ok(ReceiveProgress::Pending);
            }
            Err(error) => return Err(self.fail(Failure::io(error))),
        };
        if events & libc::POLLNVAL != 0 {
            return Err(self.fail(Failure::io(Errno::EBADF)));
        }
        if events & libc::POLLERR != 0 {
            return Err(self.fail(Failure::io(Errno::EIO)));
        }
        if events & (libc::POLLHUP | libc::POLLRDHUP) != 0 {
            return Err(self.fail(Failure::closed("packet peer closed")));
        }
        if self.zero_received {
            return Err(self.fail(Failure::invalid("empty datagram from live peer")));
        }
        Ok(ReceiveProgress::Held)
    }

    fn accept_packet(&mut self) -> Result<ReceiveProgress, Failure> {
        let packet = self.received_wire.as_ref().expect("one packet received");
        if let Some(series) = &mut self.series {
            let total = series
                .bytes
                .len()
                .checked_add(packet.len())
                .filter(|total| *total <= series.total)
                .ok_or_else(|| Failure::invalid("decode series exceeds declared total"))?;
            if series.received >= series.segments {
                return Err(Failure::invalid("decode series has surplus segments"));
            }
            series.bytes.extend_from_slice(packet);
            series.received += 1;
            self.received_wire = None;
            if series.received < series.segments {
                if total == series.total {
                    return Err(Failure::invalid(
                        "decode series ended before its segment count",
                    ));
                }
                return Ok(ReceiveProgress::Progress);
            }
            if total != series.total {
                return Err(Failure::invalid("decode series ended short"));
            }
            let series = self.series.take().expect("complete series held");
            self.received_wire = Some(series.bytes);
        } else if self.wire == Wire::Decode
            && let Ok(serde_json::Value::Object(object)) = serde_json::from_slice(packet)
            && !object.contains_key("kind")
        {
            let preamble: SegmentPreamble = serde_json::from_slice(packet)
                .map_err(|error| Failure::invalid(error.to_string()))?;
            let total = usize::try_from(preamble.bytes)
                .map_err(|_| Failure::invalid("decode byte total does not fit host"))?;
            if total <= MAX_ENVELOPE_BYTES
                || total > DECODE_MESSAGE_BOUND
                || preamble.segments != total.div_ceil(MAX_ENVELOPE_BYTES) as u64
            {
                return Err(Failure::invalid("invalid decode segment declaration"));
            }
            let mut bytes = Vec::new();
            bytes
                .try_reserve_exact(total)
                .map_err(|error| Failure::invalid(error.to_string()))?;
            self.series = Some(Series {
                bytes,
                total,
                segments: preamble.segments,
                received: 0,
            });
            self.received_wire = None;
            return Ok(ReceiveProgress::Progress);
        }
        let whole = self.received_wire.as_ref().expect("whole message retained");
        self.held = Some((self.decode)(whole)?);
        Ok(ReceiveProgress::Held)
    }
}

struct Packet {
    bytes: Vec<u8>,
}
enum ReadFault {
    Pending,
    Interrupted,
    Failed(Failure),
}

/// Ancillary custody is established before any flag, payload or shape refusal.
/// Ordinary ports accept no handles, but every delivered SCM_RIGHTS descriptor
/// is adopted and closed even on MSG_CTRUNC. The kernel owns any undisclosed tail.
fn receive_packet(fd: RawFd) -> Result<Packet, ReadFault> {
    let mut body = [0u8; MAX_ENVELOPE_BYTES];
    // Machine-word storage satisfies cmsghdr alignment.
    let mut control = [0usize; 32];
    let mut iov = libc::iovec {
        iov_base: body.as_mut_ptr().cast(),
        iov_len: body.len(),
    };
    // SAFETY: All buffers remain live for recvmsg and its returned cmsg walk.
    let mut header: libc::msghdr = unsafe { std::mem::zeroed() };
    header.msg_iov = &mut iov;
    header.msg_iovlen = 1;
    header.msg_control = control.as_mut_ptr().cast();
    header.msg_controllen = std::mem::size_of_val(&control);
    // SAFETY: fd is borrowed from the sole owning transport. The pointers above
    // name correctly aligned writable buffers of their advertised lengths.
    let count =
        unsafe { libc::recvmsg(fd, &mut header, libc::MSG_DONTWAIT | libc::MSG_CMSG_CLOEXEC) };
    if count < 0 {
        return match Errno::last() {
            Errno::EAGAIN => Err(ReadFault::Pending),
            Errno::EINTR => Err(ReadFault::Interrupted),
            error => Err(ReadFault::Failed(Failure::io(error))),
        };
    }
    let mut ancillary = false;
    // SAFETY: Walk only kernel-written headers within this bounded control area.
    unsafe {
        let mut item = libc::CMSG_FIRSTHDR(&header);
        while !item.is_null() {
            ancillary = true;
            let length = (*item).cmsg_len;
            let offset = libc::CMSG_LEN(0) as usize;
            if length < offset || length > header.msg_controllen {
                break;
            }
            if (*item).cmsg_level == libc::SOL_SOCKET && (*item).cmsg_type == libc::SCM_RIGHTS {
                let bytes = libc::CMSG_DATA(item);
                for index in 0..(length - offset) / std::mem::size_of::<RawFd>() {
                    let descriptor = std::ptr::read_unaligned(
                        bytes
                            .add(index * std::mem::size_of::<RawFd>())
                            .cast::<RawFd>(),
                    );
                    if descriptor >= 0 {
                        drop(OwnedFd::from_raw_fd(descriptor));
                    }
                }
            }
            item = libc::CMSG_NXTHDR(&header, item);
        }
    }
    if header.msg_flags & (libc::MSG_TRUNC | libc::MSG_CTRUNC) != 0 {
        return Err(ReadFault::Failed(Failure::truncated(MAX_ENVELOPE_BYTES)));
    }
    if ancillary {
        return Err(ReadFault::Failed(Failure::invalid(
            "ancillary handles belong to preflight Enter, not a pump port",
        )));
    }
    let count = usize::try_from(count)
        .map_err(|_| ReadFault::Failed(Failure::invalid("negative packet size")))?;
    if count > body.len() {
        return Err(ReadFault::Failed(Failure::truncated(MAX_ENVELOPE_BYTES)));
    }
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(count)
        .map_err(|error| ReadFault::Failed(Failure::invalid(error.to_string())))?;
    bytes.extend_from_slice(&body[..count]);
    Ok(Packet { bytes })
}

fn peer_status(fd: RawFd) -> nix::Result<i16> {
    let mut descriptor = libc::pollfd {
        fd,
        events: libc::POLLRDHUP,
        revents: 0,
    };
    // SAFETY: This is one nonblocking probe of a valid stack pollfd.
    let result = unsafe { libc::poll(&mut descriptor, 1, 0) };
    if result < 0 {
        Err(Errno::last())
    } else {
        Ok(descriptor.revents)
    }
}

#[cfg(test)]
#[path = "pump_transport_tests.rs"]
mod tests;
