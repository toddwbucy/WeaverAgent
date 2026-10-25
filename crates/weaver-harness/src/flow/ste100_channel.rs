//! The separate STE100 SEQPACKET framing from Spec 02 CHANNEL.
//!
//! This transport owns one partial receive and one pending send. JSON decoding,
//! request correlation and duty states belong to the port. One call attempts at
//! most one datagram syscall, including when interrupted; the common pump owns
//! subsequent progress and all clocks.

use std::os::fd::{AsFd, AsRawFd, BorrowedFd, OwnedFd, RawFd};

use nix::errno::Errno;
use nix::fcntl::{FcntlArg, FdFlag, fcntl};
use nix::libc;
use nix::sys::socket::{
    MsgFlags, Shutdown, SockType, UnixAddr, getpeername, getsockname, getsockopt, recv, send,
    shutdown, sockopt,
};

/// This limit includes the header; the loaded logical payload limit does not.
pub(super) const DATAGRAM_BYTES: usize = 65_536;
pub(super) const HEADER_BYTES: usize = 26;
const FRAGMENT_BYTES: usize = DATAGRAM_BYTES - HEADER_BYTES;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(super) enum Tag {
    Bootstrap = 1,
    Request = 2,
    Reply = 3,
    Ready = 4,
    AdmissionRefused = 5,
}

impl Tag {
    fn parse(byte: u8) -> Result<Self, Fault> {
        match byte {
            1 => Ok(Self::Bootstrap),
            2 => Ok(Self::Request),
            3 => Ok(Self::Reply),
            4 => Ok(Self::Ready),
            5 => Ok(Self::AdmissionRefused),
            _ => Err(Fault::Framing("unknown tag")),
        }
    }

    fn validate_id(self, id: u64) -> Result<(), Fault> {
        let zero = matches!(self, Self::Bootstrap | Self::Ready | Self::AdmissionRefused);
        if zero != (id == 0) {
            return Err(Fault::Framing("tag and message ID disagree"));
        }
        Ok(())
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Message {
    pub tag: Tag,
    pub id: u64,
    pub payload: Vec<u8>,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Receive {
    /// No datagram is available, including while a partial message is held.
    Pending,
    /// One fragment arrived. Its payload remains private until completion.
    Fragment,
    Complete(Message),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SendProgress {
    Pending,
    Progress,
    Complete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Fault {
    InvalidLimit,
    WrongSocket,
    Framing(&'static str),
    Allocation,
    Closed,
    Io(Errno),
    Failed,
    /// Local refusal only. The first queued message remains intact.
    Busy,
}

impl std::fmt::Display for Fault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "STE100 channel: {self:?}")
    }
}

impl std::error::Error for Fault {}

#[cfg_attr(test, derive(Debug))]
struct Partial {
    tag: Tag,
    id: u64,
    total: usize,
    bytes: Vec<u8>,
}

#[cfg_attr(test, derive(Debug))]
struct Outbound {
    tag: Tag,
    id: u64,
    bytes: Vec<u8>,
    offset: usize,
    datagram: Vec<u8>,
}

/// A connected, owned Unix SEQPACKET end. Per-call nonblocking flags leave the
/// inherited status flags untouched. Failure retires both directions, retaining
/// partial and outgoing evidence until this owner is dropped. A failed channel
/// never resumes, even though its evidence queries can remain true.
pub(super) struct Channel<E = OwnedFd> {
    end: E,
    limit: usize,
    partial: Option<Partial>,
    outbound: Option<Outbound>,
    // recv(0) can be an empty packet or EOF. Retain it if the immediate status
    // probe is interrupted; never consume another packet before resolving it.
    zero_received: bool,
    // EINTR does not prove that the socket was empty before a send. The pump
    // must perform another receive attempt before allowing outgoing progress.
    receive_interrupted: bool,
    failed: bool,
}

#[cfg(test)]
impl<E> Channel<E> {
    pub(super) fn test_evidence(&self) -> String {
        format!(
            "{:?}",
            (
                self.limit,
                &self.partial,
                self.partial.as_ref().map(|p| p.bytes.as_ptr() as usize),
                &self.outbound,
                self.outbound.as_ref().map(|p| p.bytes.as_ptr() as usize),
                self.zero_received,
                self.receive_interrupted,
                self.failed,
            )
        )
    }
    pub(super) fn test_partial_bytes(&self) -> usize {
        self.partial.as_ref().map_or(0, |p| p.bytes.len())
    }
}

impl Channel {
    /// Retain the exact framing evidence while consuming the sole descriptor.
    pub(super) fn retire_failed(self) -> Channel<()> {
        let Self {
            end,
            limit,
            partial,
            outbound,
            zero_received,
            receive_interrupted,
            failed,
        } = self;
        drop(end);
        Channel {
            end: (),
            limit,
            partial,
            outbound,
            zero_received,
            receive_interrupted,
            failed,
        }
    }

    pub(super) fn from_owned(end: OwnedFd, max_wire_bytes: usize) -> Result<Self, Fault> {
        if max_wire_bytes == 0 || u64::try_from(max_wire_bytes).is_err() {
            return Err(Fault::InvalidLimit);
        }
        if getsockopt(&end, sockopt::SockType).map_err(Fault::Io)? != SockType::SeqPacket
            || getsockname::<UnixAddr>(end.as_raw_fd()).is_err()
            || getpeername::<UnixAddr>(end.as_raw_fd()).is_err()
        {
            return Err(Fault::WrongSocket);
        }
        let flags = fcntl(&end, FcntlArg::F_GETFD).map_err(Fault::Io)?;
        fcntl(
            &end,
            FcntlArg::F_SETFD(FdFlag::from_bits_retain(flags) | FdFlag::FD_CLOEXEC),
        )
        .map_err(Fault::Io)?;
        Ok(Self {
            end,
            limit: max_wire_bytes,
            partial: None,
            outbound: None,
            zero_received: false,
            receive_interrupted: false,
            failed: false,
        })
    }

    pub(super) fn readiness(&self) -> BorrowedFd<'_> {
        self.end.as_fd()
    }

    pub(super) fn has_partial_receive(&self) -> bool {
        self.partial.is_some()
    }

    pub(super) fn has_pending_send(&self) -> bool {
        self.outbound.is_some()
    }

    /// Local work remains after an interrupted receive or zero-read status
    /// probe, even if the socket is no longer readable. The pump must retry
    /// this receive work before waiting for another descriptor event.
    pub(super) fn has_pending_receive_status(&self) -> bool {
        (self.zero_received || self.receive_interrupted) && !self.failed
    }

    /// Retire both directions on a port failure or teardown, retaining evidence.
    pub(super) fn close(&mut self) {
        self.fail(Fault::Failed);
    }

    /// Read at most one datagram. A first fragment is visible as progress even
    /// while sending, so the port can refuse premature traffic immediately.
    pub(super) fn try_receive(&mut self) -> Result<Receive, Fault> {
        self.receive_with(recv)
    }

    // FnOnce makes the single-attempt boundary structural and lets tests inject
    // EINTR without signals or a different framing implementation.
    fn receive_with(
        &mut self,
        receive: impl FnOnce(RawFd, &mut [u8], MsgFlags) -> nix::Result<usize>,
    ) -> Result<Receive, Fault> {
        self.receive_with_status(receive, peer_status)
    }

    fn receive_with_status(
        &mut self,
        receive: impl FnOnce(RawFd, &mut [u8], MsgFlags) -> nix::Result<usize>,
        probe: impl FnOnce(RawFd) -> nix::Result<i16>,
    ) -> Result<Receive, Fault> {
        if self.failed {
            return Err(Fault::Failed);
        }
        let outcome = (|| {
            if self.zero_received {
                return self.resolve_zero(probe);
            }
            let mut datagram = [0_u8; DATAGRAM_BYTES];
            let count = match receive(
                self.end.as_raw_fd(),
                &mut datagram,
                MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_TRUNC,
            ) {
                Ok(count) => {
                    self.receive_interrupted = false;
                    count
                }
                Err(Errno::EINTR) => {
                    self.receive_interrupted = true;
                    return Ok(Receive::Pending);
                }
                Err(Errno::EAGAIN) => {
                    self.receive_interrupted = false;
                    return Ok(Receive::Pending);
                }
                Err(error) => return Err(Fault::Io(error)),
            };
            if count == 0 {
                self.zero_received = true;
                return self.resolve_zero(probe);
            }
            if count > DATAGRAM_BYTES {
                return Err(Fault::Framing("truncated datagram"));
            }
            self.accept_fragment(&datagram[..count])
        })();
        outcome.map_err(|fault| self.fail(fault))
    }

    fn resolve_zero(
        &self,
        probe: impl FnOnce(RawFd) -> nix::Result<i16>,
    ) -> Result<Receive, Fault> {
        let events = match probe(self.end.as_raw_fd()) {
            Ok(events) => events,
            Err(Errno::EINTR) => return Ok(Receive::Pending),
            Err(error) => return Err(Fault::Io(error)),
        };
        if events & libc::POLLNVAL != 0 {
            return Err(Fault::Io(Errno::EBADF));
        }
        if events & libc::POLLERR != 0 {
            return Err(Fault::Io(Errno::EIO));
        }
        if events & (libc::POLLHUP | libc::POLLRDHUP) != 0 {
            return Err(Fault::Closed);
        }
        Err(Fault::Framing("empty datagram from live peer"))
    }

    fn accept_fragment(&mut self, datagram: &[u8]) -> Result<Receive, Fault> {
        if datagram.len() <= HEADER_BYTES {
            return Err(Fault::Framing("missing header or empty fragment"));
        }
        if datagram[0] != 1 {
            return Err(Fault::Framing("unknown framing version"));
        }
        let tag = Tag::parse(datagram[1])?;
        let read_u64 = |start| {
            u64::from_be_bytes(
                datagram[start..start + 8]
                    .try_into()
                    .expect("header checked"),
            )
        };
        let id = read_u64(2);
        tag.validate_id(id)?;
        let total = usize::try_from(read_u64(10))
            .map_err(|_| Fault::Framing("total does not fit allocation index"))?;
        // Judge the entire total before reserving any payload storage.
        if total == 0 || total > self.limit {
            return Err(Fault::Framing("logical message exceeds its declared bound"));
        }
        let offset = usize::try_from(read_u64(18))
            .map_err(|_| Fault::Framing("offset does not fit allocation index"))?;
        let data = &datagram[HEADER_BYTES..];
        let end = offset
            .checked_add(data.len())
            .filter(|end| *end <= total)
            .ok_or(Fault::Framing("fragment exceeds total"))?;
        match &self.partial {
            Some(partial)
                if partial.tag != tag
                    || partial.id != id
                    || partial.total != total
                    || partial.bytes.len() != offset =>
            {
                return Err(Fault::Framing("fragment identity or offset changed"));
            }
            None if offset != 0 => return Err(Fault::Framing("first offset is not zero")),
            _ => {}
        }
        if self.partial.is_none() {
            let mut bytes = Vec::new();
            bytes
                .try_reserve_exact(total)
                .map_err(|_| Fault::Allocation)?;
            self.partial = Some(Partial {
                tag,
                id,
                total,
                bytes,
            });
        }
        let partial = self.partial.as_mut().expect("first fragment initialized");
        partial.bytes.extend_from_slice(data);
        if end == total {
            let partial = self.partial.take().expect("complete message held");
            Ok(Receive::Complete(Message {
                tag: partial.tag,
                id: partial.id,
                payload: partial.bytes,
            }))
        } else {
            Ok(Receive::Fragment)
        }
    }

    /// Own the complete payload before sending any prefix. The caller retains
    /// typed request evidence; this owner holds the serialized bytes and cursor.
    pub(super) fn begin_send(&mut self, tag: Tag, id: u64, payload: Vec<u8>) -> Result<(), Fault> {
        if self.failed {
            return Err(Fault::Failed);
        }
        if self.outbound.is_some() {
            return Err(Fault::Busy);
        }
        let prepare = || {
            tag.validate_id(id)?;
            if payload.is_empty() || payload.len() > self.limit {
                return Err(Fault::Framing("outgoing logical message exceeds its bound"));
            }
            let mut datagram = Vec::new();
            datagram
                .try_reserve_exact(HEADER_BYTES + payload.len().min(FRAGMENT_BYTES))
                .map_err(|_| Fault::Allocation)?;
            Ok(Outbound {
                tag,
                id,
                bytes: payload,
                offset: 0,
                datagram,
            })
        };
        match prepare() {
            Ok(outbound) => {
                self.outbound = Some(outbound);
                Ok(())
            }
            Err(fault) => Err(self.fail(fault)),
        }
    }

    /// Send at most one datagram, retaining its exact bytes across backpressure.
    /// Complete also describes a channel with no queued outgoing message.
    pub(super) fn try_send(&mut self) -> Result<SendProgress, Fault> {
        self.send_with(send)
    }

    fn send_with(
        &mut self,
        transmit: impl FnOnce(RawFd, &[u8], MsgFlags) -> nix::Result<usize>,
    ) -> Result<SendProgress, Fault> {
        if self.failed {
            return Err(Fault::Failed);
        }
        if self.zero_received || self.receive_interrupted {
            return Ok(SendProgress::Pending);
        }
        let outcome = (|| {
            let Some(outbound) = &mut self.outbound else {
                return Ok(SendProgress::Complete);
            };
            if outbound.datagram.is_empty() {
                let remaining = outbound.bytes.len() - outbound.offset;
                let end = outbound.offset + remaining.min(FRAGMENT_BYTES);
                outbound.datagram.push(1);
                outbound.datagram.push(outbound.tag as u8);
                outbound
                    .datagram
                    .extend_from_slice(&outbound.id.to_be_bytes());
                outbound
                    .datagram
                    .extend_from_slice(&(outbound.bytes.len() as u64).to_be_bytes());
                outbound
                    .datagram
                    .extend_from_slice(&(outbound.offset as u64).to_be_bytes());
                outbound
                    .datagram
                    .extend_from_slice(&outbound.bytes[outbound.offset..end]);
            }
            let written = match transmit(
                self.end.as_raw_fd(),
                &outbound.datagram,
                MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_NOSIGNAL,
            ) {
                Ok(written) => written,
                Err(Errno::EINTR | Errno::EAGAIN) => return Ok(SendProgress::Pending),
                Err(error) => return Err(Fault::Io(error)),
            };
            if written != outbound.datagram.len() {
                return Err(Fault::Framing("short SEQPACKET send"));
            }
            outbound.offset += written - HEADER_BYTES;
            outbound.datagram.clear();
            if outbound.offset == outbound.bytes.len() {
                self.outbound = None;
                Ok(SendProgress::Complete)
            } else {
                Ok(SendProgress::Progress)
            }
        })();
        outcome.map_err(|fault| self.fail(fault))
    }

    fn fail(&mut self, fault: Fault) -> Fault {
        self.failed = true;
        let _ = shutdown(self.end.as_raw_fd(), Shutdown::Both);
        fault
    }
}

fn peer_status(fd: RawFd) -> nix::Result<i16> {
    let mut status = libc::pollfd {
        fd,
        // POLLRDHUP observes a write-half shutdown as well as a full close.
        events: libc::POLLRDHUP,
        revents: 0,
    };
    // SAFETY: status is one writable pollfd; the channel owns fd throughout the
    // call. The zero timeout neither blocks nor adds another wait clock.
    if unsafe { libc::poll(&mut status, 1, 0) } < 0 {
        return Err(Errno::last());
    }
    Ok(status.revents)
}

#[cfg(test)]
#[path = "ste100_channel_tests.rs"]
mod tests;
