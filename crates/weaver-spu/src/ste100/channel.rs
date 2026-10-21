//! The separate STE100 SEQPACKET framing, per Spec 02 CHANNEL.
//!
//! This transport owns one partial receive and one pending send. It does not
//! parse JSON, decide duty states, or compare the header ID with a JSON ID.

use std::os::fd::{AsFd, AsRawFd, BorrowedFd, OwnedFd};

use nix::errno::Errno;
use nix::fcntl::{FcntlArg, FdFlag, fcntl};
use nix::sys::socket::{
    MsgFlags, Shutdown, SockType, UnixAddr, getpeername, getsockname, getsockopt, recv, send,
    shutdown, sockopt,
};

/// The limit includes the header, unlike the declared logical payload limit.
pub const DATAGRAM_BYTES: usize = 65_536;
pub const HEADER_BYTES: usize = 26;
const FRAGMENT_BYTES: usize = DATAGRAM_BYTES - HEADER_BYTES;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Tag {
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
pub struct Message {
    pub tag: Tag,
    pub id: u64,
    pub payload: Vec<u8>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Receive {
    /// No datagram is available, including while a partial message is held.
    Pending,
    /// One fragment arrived. No part of its payload is available to the caller.
    Fragment,
    Complete(Message),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendProgress {
    Pending,
    Progress,
    Complete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fault {
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

struct Partial {
    tag: Tag,
    id: u64,
    total: usize,
    bytes: Vec<u8>,
}

struct Outbound {
    tag: Tag,
    id: u64,
    bytes: Vec<u8>,
    offset: usize,
    datagram: Vec<u8>,
}

/// A connected, owned Unix SEQPACKET end. Entry owns descriptor-count and
/// dumpable checks. Per-call nonblocking flags leave the inherited status flags
/// untouched. A failed transport is shut down and never resumes.
pub struct Channel {
    end: OwnedFd,
    limit: usize,
    partial: Option<Partial>,
    outbound: Option<Outbound>,
    failed: bool,
}

impl Channel {
    pub fn from_owned(end: OwnedFd, max_wire_bytes: usize) -> Result<Self, Fault> {
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
            failed: false,
        })
    }

    pub fn readiness(&self) -> BorrowedFd<'_> {
        self.end.as_fd()
    }

    pub fn max_wire_bytes(&self) -> usize {
        self.limit
    }

    pub fn has_partial_receive(&self) -> bool {
        self.partial.is_some()
    }

    pub fn has_pending_send(&self) -> bool {
        self.outbound.is_some()
    }

    /// Retire both directions on a duty-level protocol failure or teardown.
    pub fn close(&mut self) {
        self.fail(Fault::Failed);
    }

    /// Read at most one datagram. The dispatcher can observe a first fragment
    /// while Checking without blocking for that unwanted message's remainder.
    pub fn try_receive(&mut self) -> Result<Receive, Fault> {
        if self.failed {
            return Err(Fault::Failed);
        }
        match self.receive_one() {
            Ok(progress) => Ok(progress),
            Err(fault) => Err(self.fail(fault)),
        }
    }

    fn receive_one(&mut self) -> Result<Receive, Fault> {
        let mut datagram = [0_u8; DATAGRAM_BYTES];
        let count = loop {
            match recv(
                self.end.as_raw_fd(),
                &mut datagram,
                MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_TRUNC,
            ) {
                Ok(count) => break count,
                Err(Errno::EINTR) => continue,
                Err(Errno::EAGAIN) => return Ok(Receive::Pending),
                Err(error) => return Err(Fault::Io(error)),
            }
        };
        if count == 0 {
            return Err(Fault::Closed);
        }
        if count > DATAGRAM_BYTES {
            return Err(Fault::Framing("truncated datagram"));
        }
        self.accept_fragment(&datagram[..count])
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
        // Judge the whole total before reserving any payload storage.
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

    /// Queue complete JSON bytes. No prefix is sent until the entire outgoing
    /// payload and its bounded datagram scratch allocation have been accepted.
    pub fn begin_send(&mut self, tag: Tag, id: u64, payload: Vec<u8>) -> Result<(), Fault> {
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
    pub fn try_send(&mut self) -> Result<SendProgress, Fault> {
        if self.failed {
            return Err(Fault::Failed);
        }
        match self.send_one() {
            Ok(progress) => Ok(progress),
            Err(fault) => Err(self.fail(fault)),
        }
    }

    fn send_one(&mut self) -> Result<SendProgress, Fault> {
        let Some(outbound) = &mut self.outbound else {
            return Ok(SendProgress::Complete);
        };
        if outbound.datagram.is_empty() {
            let end = outbound
                .offset
                .saturating_add(FRAGMENT_BYTES)
                .min(outbound.bytes.len());
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
        let written = loop {
            match send(
                self.end.as_raw_fd(),
                &outbound.datagram,
                MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_NOSIGNAL,
            ) {
                Ok(written) => break written,
                Err(Errno::EINTR) => continue,
                Err(Errno::EAGAIN) => return Ok(SendProgress::Pending),
                Err(error) => return Err(Fault::Io(error)),
            }
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
    }

    fn fail(&mut self, fault: Fault) -> Fault {
        self.failed = true;
        self.partial = None;
        self.outbound = None;
        let _ = shutdown(self.end.as_raw_fd(), Shutdown::Both);
        fault
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nix::sys::socket::{AddressFamily, SockFlag, socket, socketpair};

    fn pair() -> (OwnedFd, OwnedFd) {
        socketpair(
            AddressFamily::Unix,
            SockType::SeqPacket,
            None,
            SockFlag::SOCK_CLOEXEC,
        )
        .expect("SEQPACKET pair")
    }

    fn packet(tag: Tag, id: u64, total: u64, offset: u64, data: &[u8]) -> Vec<u8> {
        let mut bytes = vec![1, tag as u8];
        bytes.extend_from_slice(&id.to_be_bytes());
        bytes.extend_from_slice(&total.to_be_bytes());
        bytes.extend_from_slice(&offset.to_be_bytes());
        bytes.extend_from_slice(data);
        bytes
    }

    fn raw_send(end: &OwnedFd, bytes: &[u8]) {
        assert_eq!(
            send(end.as_raw_fd(), bytes, MsgFlags::MSG_NOSIGNAL).unwrap(),
            bytes.len()
        );
    }

    fn assert_failed(channel: &mut Channel) {
        assert!(!channel.has_partial_receive());
        assert!(!channel.has_pending_send());
        assert_eq!(channel.try_receive(), Err(Fault::Failed));
        assert_eq!(channel.try_send(), Err(Fault::Failed));
        assert_eq!(
            channel.begin_send(Tag::Reply, 1, b"{}".to_vec()),
            Err(Fault::Failed)
        );
    }

    #[test]
    fn exact_header_and_all_tags_use_their_declared_id_domain() {
        for (tag, id) in [
            (Tag::Bootstrap, 0),
            (Tag::Request, u64::MAX),
            (Tag::Reply, 1),
            (Tag::Ready, 0),
            (Tag::AdmissionRefused, 0),
        ] {
            let (writer, reader) = pair();
            let mut channel = Channel::from_owned(writer, 3).unwrap();
            channel.begin_send(tag, id, b"abc".to_vec()).unwrap();
            assert_eq!(channel.try_send(), Ok(SendProgress::Complete));
            let mut received = [0_u8; 64];
            let count = recv(reader.as_raw_fd(), &mut received, MsgFlags::empty()).unwrap();
            assert_eq!(&received[..count], packet(tag, id, 3, 0, b"abc"));
            assert!(!channel.has_pending_send());
        }
    }

    #[test]
    fn exact_datagram_and_logical_bounds_round_trip_without_partial_results() {
        for length in [
            1,
            FRAGMENT_BYTES,
            FRAGMENT_BYTES + 1,
            2 * FRAGMENT_BYTES + 17,
        ] {
            let payload: Vec<u8> = (0..length).map(|index| (index % 251) as u8).collect();
            let (writer, reader) = pair();
            let mut sender = Channel::from_owned(writer, length).unwrap();
            let mut receiver = Channel::from_owned(reader, length).unwrap();
            assert_eq!(receiver.max_wire_bytes(), length);
            assert_eq!(receiver.try_receive(), Ok(Receive::Pending));
            sender.begin_send(Tag::Reply, 37, payload.clone()).unwrap();
            let mut fragments = 0;
            loop {
                let progress = sender.try_send().unwrap();
                assert_ne!(progress, SendProgress::Pending);
                match receiver.try_receive().unwrap() {
                    Receive::Fragment => {
                        fragments += 1;
                        assert!(receiver.has_partial_receive());
                        assert_eq!(receiver.try_receive(), Ok(Receive::Pending));
                        assert_ne!(progress, SendProgress::Complete);
                    }
                    Receive::Complete(message) => {
                        assert_eq!(
                            message,
                            Message {
                                tag: Tag::Reply,
                                id: 37,
                                payload
                            }
                        );
                        assert_eq!(progress, SendProgress::Complete);
                        assert_eq!(fragments, length.div_ceil(FRAGMENT_BYTES) - 1);
                        assert!(!receiver.has_partial_receive());
                        break;
                    }
                    Receive::Pending => panic!("sent packet is available"),
                }
            }
            sender.begin_send(Tag::Reply, 38, vec![b'z']).unwrap();
            assert_eq!(sender.try_send(), Ok(SendProgress::Complete));
            assert_eq!(
                receiver.try_receive(),
                Ok(Receive::Complete(Message {
                    tag: Tag::Reply,
                    id: 38,
                    payload: vec![b'z']
                }))
            );
        }
    }

    #[test]
    fn malformed_first_fragments_fail_before_a_partial_is_exposed() {
        let valid = packet(Tag::Request, 1, 3, 0, b"abc");
        let mut bad_version = valid.clone();
        bad_version[0] = 2;
        let mut bad_tag = valid.clone();
        bad_tag[1] = 6;
        let cases = vec![
            vec![1; HEADER_BYTES - 1],
            packet(Tag::Request, 1, 1, 0, b""),
            bad_version,
            bad_tag,
            packet(Tag::Bootstrap, 1, 3, 0, b"abc"),
            packet(Tag::Ready, 1, 3, 0, b"abc"),
            packet(Tag::AdmissionRefused, 1, 3, 0, b"abc"),
            packet(Tag::Request, 0, 3, 0, b"abc"),
            packet(Tag::Reply, 0, 3, 0, b"abc"),
            packet(Tag::Request, 1, 0, 0, b"abc"),
            packet(Tag::Request, 1, 9, 0, b"abc"),
            packet(Tag::Request, 1, u64::MAX, 0, b"abc"),
            packet(Tag::Request, 1, 2, 0, b"abc"),
            packet(Tag::Request, 1, 4, 1, b"abc"),
            packet(Tag::Request, 1, 4, u64::MAX, b"abc"),
        ];
        for bytes in cases {
            let (writer, reader) = pair();
            let mut channel = Channel::from_owned(reader, 8).unwrap();
            raw_send(&writer, &bytes);
            assert!(channel.try_receive().is_err(), "accepted {bytes:?}");
            assert_failed(&mut channel);
        }
    }

    #[test]
    fn a_continuation_cannot_change_identity_total_tag_or_offset() {
        for continuation in [
            packet(Tag::Request, 2, 6, 3, b"def"),
            packet(Tag::Reply, 1, 6, 3, b"def"),
            packet(Tag::Request, 1, 7, 3, b"def"),
            packet(Tag::Request, 1, 6, 2, b"def"),
            packet(Tag::Request, 1, 6, 4, b"de"),
            packet(Tag::Request, 1, 6, 0, b"abc"),
            packet(Tag::Request, 1, 6, 3, b"defg"),
        ] {
            let (writer, reader) = pair();
            let mut channel = Channel::from_owned(reader, 8).unwrap();
            raw_send(&writer, &packet(Tag::Request, 1, 6, 0, b"abc"));
            assert_eq!(channel.try_receive(), Ok(Receive::Fragment));
            raw_send(&writer, &continuation);
            assert!(channel.try_receive().is_err(), "accepted {continuation:?}");
            assert_failed(&mut channel);
        }
    }

    #[test]
    fn oversized_datagram_fails_even_when_its_copied_prefix_would_complete() {
        let (writer, reader) = pair();
        let mut channel = Channel::from_owned(reader, FRAGMENT_BYTES).unwrap();
        let mut bytes = packet(
            Tag::Request,
            1,
            FRAGMENT_BYTES as u64,
            0,
            &vec![b'x'; FRAGMENT_BYTES],
        );
        bytes.push(b'y');
        raw_send(&writer, &bytes);
        assert_eq!(
            channel.try_receive(),
            Err(Fault::Framing("truncated datagram"))
        );
        assert_failed(&mut channel);
    }

    #[test]
    fn eof_during_a_partial_message_is_never_a_completion() {
        for partial in [false, true] {
            let (writer, reader) = pair();
            let mut channel = Channel::from_owned(reader, 8).unwrap();
            if partial {
                raw_send(&writer, &packet(Tag::Request, 1, 6, 0, b"abc"));
                assert_eq!(channel.try_receive(), Ok(Receive::Fragment));
                assert!(channel.has_partial_receive());
            }
            drop(writer);
            assert_eq!(channel.try_receive(), Err(Fault::Closed));
            assert_failed(&mut channel);
        }
    }

    #[test]
    fn outgoing_bounds_refuse_whole_and_busy_keeps_the_first_message() {
        for (tag, id, payload) in [
            (Tag::Reply, 1, vec![]),
            (Tag::Reply, 1, vec![0; 4]),
            (Tag::Reply, 0, vec![0]),
            (Tag::Ready, 1, vec![0]),
        ] {
            let (writer, reader) = pair();
            let mut sender = Channel::from_owned(writer, 3).unwrap();
            assert!(sender.begin_send(tag, id, payload).is_err());
            let mut bytes = [0; 64];
            assert_eq!(
                recv(reader.as_raw_fd(), &mut bytes, MsgFlags::MSG_DONTWAIT),
                Ok(0)
            );
            assert_failed(&mut sender);
        }
        let (writer, reader) = pair();
        let mut sender = Channel::from_owned(writer, 8).unwrap();
        let mut receiver = Channel::from_owned(reader, 8).unwrap();
        sender.begin_send(Tag::Ready, 0, b"first".to_vec()).unwrap();
        assert_eq!(
            sender.begin_send(Tag::Reply, 1, b"second".to_vec()),
            Err(Fault::Busy)
        );
        assert_eq!(sender.try_send(), Ok(SendProgress::Complete));
        assert_eq!(
            receiver.try_receive(),
            Ok(Receive::Complete(Message {
                tag: Tag::Ready,
                id: 0,
                payload: b"first".to_vec()
            }))
        );
    }

    #[test]
    fn backpressure_retries_the_exact_pending_datagram_and_can_still_receive() {
        let length = 32 * FRAGMENT_BYTES + 7;
        let payload: Vec<_> = (0..length).map(|index| (index % 251) as u8).collect();
        let (writer, reader) = pair();
        nix::sys::socket::setsockopt(&writer, sockopt::SndBuf, &131_072_usize).unwrap();
        let mut sender = Channel::from_owned(writer, length).unwrap();
        let mut receiver = Channel::from_owned(reader, length).unwrap();
        sender.begin_send(Tag::Reply, 1, payload.clone()).unwrap();
        let mut progress = SendProgress::Progress;
        for _ in 0..32 {
            progress = sender.try_send().unwrap();
            if progress == SendProgress::Pending {
                break;
            }
            assert_ne!(
                progress,
                SendProgress::Complete,
                "test must reach backpressure"
            );
        }
        assert_eq!(progress, SendProgress::Pending);
        let held = sender.outbound.as_ref().unwrap();
        let held_offset = held.offset;
        let held_bytes = held.datagram.clone();
        assert_eq!(sender.try_send(), Ok(SendProgress::Pending));
        assert_eq!(sender.outbound.as_ref().unwrap().offset, held_offset);
        assert_eq!(sender.outbound.as_ref().unwrap().datagram, held_bytes);

        // The duty can observe an out-of-order peer request while its answer is blocked.
        receiver
            .begin_send(Tag::Request, 2, b"overlap".to_vec())
            .unwrap();
        assert_eq!(receiver.try_send(), Ok(SendProgress::Complete));
        assert!(matches!(
            sender.try_receive(),
            Ok(Receive::Complete(Message { id: 2, .. }))
        ));
        for _ in 0..128 {
            match receiver.try_receive().unwrap() {
                Receive::Complete(message) => {
                    assert_eq!(message.payload, payload);
                    assert_eq!(message.id, 1);
                    assert_eq!(message.tag, Tag::Reply);
                    assert!(!sender.has_pending_send());
                    return;
                }
                Receive::Fragment | Receive::Pending => {}
            }
            sender.try_send().unwrap();
        }
        panic!("bounded drain did not complete");
    }

    #[test]
    fn ownership_requires_a_connected_unix_seqpacket_and_sets_cloexec() {
        let (end, _peer) = pair();
        fcntl(&end, FcntlArg::F_SETFD(FdFlag::empty())).unwrap();
        let channel = Channel::from_owned(end, 1).unwrap();
        let flags = fcntl(channel.readiness(), FcntlArg::F_GETFD).unwrap();
        assert!(FdFlag::from_bits_retain(flags).contains(FdFlag::FD_CLOEXEC));
        let (end, _peer) = pair();
        assert!(matches!(
            Channel::from_owned(end, 0),
            Err(Fault::InvalidLimit)
        ));
        let (stream, _peer) = std::os::unix::net::UnixStream::pair().unwrap();
        assert!(matches!(
            Channel::from_owned(stream.into(), 8),
            Err(Fault::WrongSocket)
        ));
        let unconnected = socket(
            AddressFamily::Unix,
            SockType::SeqPacket,
            SockFlag::SOCK_CLOEXEC,
            None,
        )
        .unwrap();
        assert!(matches!(
            Channel::from_owned(unconnected, 8),
            Err(Fault::WrongSocket)
        ));
    }

    #[test]
    fn explicit_close_and_send_failure_retire_both_directions() {
        let (writer, reader) = pair();
        let mut channel = Channel::from_owned(reader, 8).unwrap();
        raw_send(&writer, &packet(Tag::Request, 1, 6, 0, b"abc"));
        assert_eq!(channel.try_receive(), Ok(Receive::Fragment));
        channel
            .begin_send(Tag::Reply, 1, b"reply".to_vec())
            .unwrap();
        channel.close();
        channel.close();
        assert_failed(&mut channel);
        let (writer, reader) = pair();
        let mut channel = Channel::from_owned(writer, 8).unwrap();
        channel
            .begin_send(Tag::Reply, 1, b"reply".to_vec())
            .unwrap();
        drop(reader);
        assert!(matches!(channel.try_send(), Err(Fault::Io(_))));
        assert_failed(&mut channel);
    }
}
