//! conforms: gate-client-content-unread
//! conforms: gate-frame-bounded-at-the-delimiter
//! conforms: gate-one-exchange-per-line-by-identity
//! conforms: gate-line-bound-closes-the-connection
//! conforms: gate-one-exchange-open-per-connection
//!
//! The relay, per `weaver-gate-Spec` section 4: the pass-through between the
//! world and the harness, octets in order, nothing read, nothing retained
//! past the answer.
//!
//! **The relay reads no content.** A line is bounded by a byte scan for the
//! delimiter and carried encoded inside the frame, and both are carriage
//! rather than reading: no field is parsed on either leg. The suite holds
//! the same rule, driving lines that are not JSON at all, so a test that
//! parsed one would fail on its face.
//!
//! **One exchange is open per connection, and the cap is the flow control.**
//! A connection with an exchange open leaves the read set until its response
//! returns, further lines waiting in the socket's own buffer, and everything
//! this module holds is bounded by the cap: one input residual, one outbound
//! buffer, and one exchange entry per connection. Bytes a read already
//! delivered past the first delimiter wait in the residual, at most one
//! read's worth by construction, and the scan resumes over the residual when
//! the response returns, so a line it already holds is served in its turn
//! and never skipped.

use std::collections::VecDeque;
use std::io::{Read, Write};
use std::os::fd::{AsFd, BorrowedFd};
use std::os::unix::net::UnixStream;

use weaver_types::{ExchangeId, Opener, OrganEnvelope, Payload, Position, TurnFrame};

use crate::channel::{Channel, ChannelFault};
use crate::hook::Admitted;

/// The client line's bound: 32 kibibytes of octets before the delimiter,
/// inclusive, per `weaver-gate-Spec` section 4. A line of exactly the bound
/// followed by its delimiter is legal, and a connection holding one octet
/// more with no delimiter found has left the protocol at the framing layer,
/// below any turn.
pub const LINE_BOUND: usize = 32 * 1024;

/// The delimiter, the world contract's framing: one request per line. The
/// scan compares bytes against it and parses nothing.
const DELIMITER: u8 = b'\n';

/// The raised window's relay state: the served connections, the envelopes
/// the channel could not take yet, and the ordinal this crate numbers its
/// exchanges from. Dropping it closes every connection, which is what the
/// lower's ordering relies on.
pub struct Relay {
    pub served: Vec<Served>,
    /// Envelopes waiting on the channel's writability, bounded at one per
    /// connection by the cap: a connection contributes an envelope only by
    /// opening an exchange, and it opens at most one.
    pub pending: VecDeque<OrganEnvelope>,
    next_ordinal: u64,
}

impl Default for Relay {
    fn default() -> Relay {
        Relay::new()
    }
}

impl Relay {
    pub fn new() -> Relay {
        Relay {
            served: Vec::new(),
            pending: VecDeque::new(),
            next_ordinal: 0,
        }
    }

    /// The connection owed the response on this exchange, by the identity
    /// the channel already gives: the ordinal is the table this crate does
    /// not mint, per charter section 13.1.
    pub fn route(&mut self, ordinal: u64) -> Option<&mut Served> {
        self.served
            .iter_mut()
            .find(|served| served.exchange == Some(ordinal))
    }

    /// Frames the next line on a connection if one stands and no exchange
    /// is open, minting the exchange under this crate's own ordinal.
    pub fn frame_one(&mut self, at: usize) -> Result<Framed, Gone> {
        let Some(served) = self.served.get_mut(at) else {
            return Ok(Framed::Waiting);
        };
        served.frame_one(&mut self.next_ordinal)
    }

    /// A readable wake on one connection: read once and frame if a line
    /// stands.
    pub fn read_one(&mut self, at: usize) -> Result<Framed, Gone> {
        let Some(served) = self.served.get_mut(at) else {
            return Ok(Framed::Waiting);
        };
        served.on_readable(&mut self.next_ordinal)
    }

    /// **The quiesce's closes**, per `weaver-gate-Spec` section 4: every
    /// connection owed nothing closes now, its input never received as a
    /// request, and every one owed something drains, read no more.
    pub fn quiesce(&mut self) {
        self.served.retain(Served::owes);
        for served in &mut self.served {
            served.draining = true;
        }
    }

    /// **Sends every envelope waiting on the channel, in order, blocking**,
    /// so the answer that follows reaches the harness after every frame this
    /// crate admitted (`weaver-harness-gate-contract` section 2, the quiesce
    /// and the forced lower).
    pub fn flush_pending(&mut self, channel: &Channel) -> Result<(), ChannelFault> {
        while let Some(front) = self.pending.front() {
            channel.send(front)?;
            self.pending.pop_front();
        }
        Ok(())
    }

    /// **The lower's closes from draining** (`weaver-harness-gate-contract`
    /// section 2): the channel is ordered, so when the `Lower` is read every
    /// response the harness will send has arrived, and a connection whose
    /// exchange is still open will never be answered: it closes now,
    /// unanswered, the harness having recorded its request refused. Only a
    /// connection with a response still to write stays.
    pub fn lower(&mut self) {
        self.served.retain(Served::wants_write);
    }

    /// **Whether a lower from draining can answer stopped** (`weaver-gate-Spec`
    /// section 4): done once every response the harness sent is written;
    /// still open while one is not and the settle instant has not passed;
    /// and, past it, done with the dialer of each connection still writing
    /// named, its delivery lost.
    pub fn settle(&self, past_the_bound: bool) -> Settled {
        // Walked once per wake while a lower waits, so the dialers are
        // collected only when they will be named.
        if !self.served.iter().any(Served::wants_write) {
            return Settled::Done { lost: Vec::new() };
        }
        if !past_the_bound {
            return Settled::Open;
        }
        let owed: Vec<u32> = self
            .served
            .iter()
            .filter(|served| served.wants_write())
            .map(|served| served.dialer)
            .collect();
        Settled::Done { lost: owed }
    }

    /// **The lower's flush, bounded by the settle instant** (Codex on #114):
    /// sends what waits on the channel in order, waiting on its writability
    /// no later than `until`, so a harness reading slowly cannot hold the gate
    /// past the instant it must answer by. Frames still waiting then are
    /// dropped, and the dialer of each is returned to be named lost.
    pub fn flush_pending_until(
        &mut self,
        channel: &Channel,
        until: std::time::Instant,
    ) -> Result<Vec<u32>, ChannelFault> {
        use nix::poll::{PollFd, PollFlags, PollTimeout, poll};
        while let Some(front) = self.pending.front() {
            if channel.try_send(front)? {
                self.pending.pop_front();
                continue;
            }
            let remaining = until.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                break;
            }
            let timeout = PollTimeout::try_from(
                remaining.saturating_add(std::time::Duration::from_micros(999)),
            )
            .unwrap_or(PollTimeout::MAX);
            let mut fds = [PollFd::new(channel.as_fd(), PollFlags::POLLOUT)];
            match poll(&mut fds, timeout) {
                Ok(_) | Err(nix::errno::Errno::EINTR) => {}
                Err(_) => return Err(ChannelFault::Closed),
            }
        }
        Ok(self
            .pending
            .drain(..)
            .filter_map(|envelope| match envelope.payload {
                Payload::Frame(frame) => frame.dialer,
                _ => None,
            })
            .collect())
    }

    /// The index of the connection owed this exchange's response, or none
    /// where the connection already left and the delivery is lost.
    pub fn owed(&self, ordinal: u64) -> Option<usize> {
        self.served
            .iter()
            .position(|served| served.exchange == Some(ordinal))
    }
}

/// One admitted connection under relay: the stream, the undelimited
/// residual, the responses not yet written, and the open exchange if one
/// stands. Nothing else survives here, per the retention rule: an entry
/// lives exactly as long as what it serves.
pub struct Served {
    stream: UnixStream,
    input: Vec<u8>,
    outbound: Vec<u8>,
    exchange: Option<u64>,
    /// The dialer's uid, read at accept from the kernel's `SO_PEERCRED`
    /// answer and carried on every frame this connection sends inward, per
    /// `weaver-harness-gate-contract` section 2: the harness admits a
    /// `system` line from the operator alone, and the gate is the one
    /// process that knows who dialed.
    dialer: u32,
    /// The peer closed its writing half. A half-closed connection is a
    /// client that said its piece and awaits the answer, so the read side
    /// ends while everything owed still delivers, and the connection leaves
    /// only when it is spent.
    read_closed: bool,
    /// The gate quiesced: this connection is never read again and opens no
    /// exchange, and it leaves once what it is owed is written
    /// (`weaver-gate-Spec` section 4, draining).
    draining: bool,
}

/// Whether a draining relay has delivered what it owes, for the lower.
#[derive(Debug, PartialEq, Eq)]
pub enum Settled {
    /// Something is still owed and the lower bound has not passed.
    Open,
    /// The lower may answer stopped; `lost` names the dialer of each
    /// connection whose delivery the bound outran.
    Done { lost: Vec<u32> },
}

/// Why a connection left the relay. The name travels to standard error for
/// the operator, never to the peer, who is refused by closure.
#[derive(Debug, PartialEq)]
pub enum Gone {
    /// The peer closed or its stream failed. A response owed to it is a
    /// lost delivery and not a lost turn, per the world contract.
    PeerLeft,
    /// More than the bound stood undelimited, or a line exceeded it: the
    /// peer left the protocol at the framing layer, below any turn. There
    /// is no line to refuse and no turn to open.
    PastTheBound,
    /// A response frame's member was not the canonical carriage. The
    /// harness is the only party that writes it, so this names a broken
    /// interior, and the connection cannot be answered truthfully.
    Unanswerable,
}

/// What a scan produced: nothing yet, or a frame opened toward the harness.
pub enum Framed {
    /// No complete line stands. The connection stays in the read set.
    Waiting,
    /// A line became an exchange: the envelope to send, the ordinal
    /// recorded on the connection for the response's routing. Boxed for the
    /// same reason the harness boxes its entered run, the variants' sizes
    /// being a hundredfold apart.
    Opened(Box<OrganEnvelope>),
}

impl Served {
    /// Admits a judged connection into the relay, nonblocking end to end so
    /// the loop never waits on a client, per `weaver-gate-Spec` section 4.
    pub fn admit(admitted: Admitted) -> std::io::Result<Served> {
        admitted.stream.set_nonblocking(true)?;
        Ok(Served {
            dialer: admitted.peer.uid,
            stream: admitted.stream,
            input: Vec::new(),
            outbound: Vec::new(),
            exchange: None,
            read_closed: false,
            draining: false,
        })
    }

    /// Whether the read set wants this connection: only while no exchange
    /// is open and no response stands undelivered, which is the cap doing
    /// the flow control, the outbound buffer holding at most one response
    /// per the Spec's own sentence. A read-closed connection wants nothing
    /// read again.
    pub fn wants_read(&self) -> bool {
        !self.read_closed && !self.draining && self.exchange.is_none() && self.outbound.is_empty()
    }

    /// Whether this connection is owed anything: an open exchange, or a
    /// response not yet written.
    pub fn owes(&self) -> bool {
        self.exchange.is_some() || !self.outbound.is_empty()
    }

    /// Whether the write set wants this connection: only while a response
    /// stands undelivered.
    pub fn wants_write(&self) -> bool {
        !self.outbound.is_empty()
    }

    /// The descriptor, for the wait's registration.
    pub fn as_fd(&self) -> BorrowedFd<'_> {
        self.stream.as_fd()
    }

    /// Whether nothing remains to serve: the read side ended, no exchange
    /// is open, nothing is owed, and no complete line waits. A spent
    /// connection leaves the relay quietly, its conversation finished.
    pub fn spent(&self) -> bool {
        (self.draining && !self.owes())
            || self.read_closed
                && self.exchange.is_none()
                && self.outbound.is_empty()
                && !self.input.contains(&DELIMITER)
    }

    /// A readable wake: read what is there, once, and try to frame a line.
    /// One read per wake keeps one loud client from starving the round,
    /// the poll being level-triggered and re-waking on what remains.
    ///
    /// A read of nothing is the peer's half-close, not its departure: the
    /// line it already sent still frames, the response it is owed still
    /// delivers, and the connection leaves when it is spent.
    pub fn on_readable(&mut self, next_ordinal: &mut u64) -> Result<Framed, Gone> {
        let mut chunk = [0u8; 4096];
        loop {
            match self.stream.read(&mut chunk) {
                Ok(0) => {
                    self.read_closed = true;
                    break;
                }
                Ok(n) => {
                    self.input.extend_from_slice(&chunk[..n]);
                    break;
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => return Err(Gone::PeerLeft),
            }
        }
        self.frame_one(next_ordinal)
    }

    /// Scan the residual for the delimiter and open an exchange if a line
    /// stands and none is open. Called on a readable wake and again when a
    /// response returns, so a line the residual already holds is served in
    /// its turn and never skipped.
    fn frame_one(&mut self, next_ordinal: &mut u64) -> Result<Framed, Gone> {
        // The cap holds on both legs: no second exchange opens while one is
        // open or while its response stands undelivered, so the outbound
        // buffer carries at most one response.
        if self.draining || self.exchange.is_some() || !self.outbound.is_empty() {
            return Ok(Framed::Waiting);
        }
        match self.input.iter().position(|byte| *byte == DELIMITER) {
            Some(at) => {
                // The bound is inclusive: a line of exactly the bound's
                // octets followed by its delimiter is legal.
                if at > LINE_BOUND {
                    return Err(Gone::PastTheBound);
                }
                let line: Vec<u8> = self.input.drain(..=at).take(at).collect();
                *next_ordinal += 1;
                self.exchange = Some(*next_ordinal);
                Ok(Framed::Opened(Box::new(OrganEnvelope {
                    exchange: ExchangeId {
                        opener: Opener::Gate,
                        ordinal: *next_ordinal,
                    },
                    position: Position::Open,
                    payload: Payload::Frame(TurnFrame::carry_from(&line, self.dialer)),
                })))
            }
            // The connection closes when more than the bound stands
            // undelimited, however many reads delivered it.
            None if self.input.len() > LINE_BOUND => Err(Gone::PastTheBound),
            None => Ok(Framed::Waiting),
        }
    }

    /// A response frame routed here: decode the line, queue it with its
    /// delimiter appended, and close the exchange entry, the retention rule
    /// enforced at the moment it names.
    pub fn on_response(&mut self, frame: &TurnFrame) -> Result<(), Gone> {
        let Some(line) = frame.octets() else {
            return Err(Gone::Unanswerable);
        };
        self.outbound.extend_from_slice(&line);
        self.outbound.push(DELIMITER);
        self.exchange = None;
        Ok(())
    }

    /// A writable wake: drain what the connection will take, blocking on
    /// nothing.
    pub fn on_writable(&mut self) -> Result<(), Gone> {
        while !self.outbound.is_empty() {
            match self.stream.write(&self.outbound) {
                Ok(0) => return Err(Gone::PeerLeft),
                Ok(n) => {
                    self.outbound.drain(..n);
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => return Err(Gone::PeerLeft),
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
