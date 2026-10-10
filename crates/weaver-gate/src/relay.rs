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
mod tests {
    use super::*;
    use weaver_types::PeerIdentity;

    /// A served connection over a socketpair, the accept bypassed: the
    /// predicate's own tests live with the hook, and the gate denies its
    /// own uid by construction, so relay tests build the admitted state
    /// directly.
    fn served_pair() -> (Served, UnixStream) {
        let (near, far) = UnixStream::pair().expect("pair");
        let served = Served::admit(Admitted {
            stream: near,
            peer: PeerIdentity {
                uid: 12345,
                gid: 12345,
                pid: 1,
            },
        })
        .expect("admit");
        (served, far)
    }

    /// **A quiesce closes every connection owed nothing and reads no more**,
    /// per `weaver-gate-Spec` section 4 (draining): a connection with no open
    /// exchange closes, its client reading the end; one owed a response stays,
    /// reads nothing, opens no exchange for a line already in its residual or
    /// sent after, delivers what it is owed, and then owes nothing.
    /// Perturbation: keep the idle connection, or let a draining one frame its
    /// residual, and this fails.
    #[test]
    fn a_quiesce_closes_what_is_owed_nothing_and_reads_no_more() {
        use std::io::Read as _;
        let (owed, mut owed_client) = served_pair();
        let (idle, mut idle_client) = served_pair();
        let mut relay = Relay::new();
        relay.served.push(owed);
        relay.served.push(idle);
        owed_client
            .write_all(b"the request\nthe next line\n")
            .expect("writes");
        let Framed::Opened(opened) = relay.read_one(0).expect("reads") else {
            panic!("the request frames");
        };

        relay.quiesce();

        assert_eq!(relay.served.len(), 1, "the idle connection closed");
        let mut got = [0u8; 8];
        assert_eq!(idle_client.read(&mut got).expect("the end"), 0);
        let draining = &mut relay.served[0];
        assert!(draining.owes(), "the open exchange is owed");
        assert!(
            !draining.wants_read(),
            "a draining connection is never read"
        );
        let mut ordinal = opened.exchange.ordinal;
        draining
            .on_response(&TurnFrame::carry(b"the answer"))
            .expect("routes");
        draining.on_writable().expect("drains");
        let n = owed_client.read(&mut [0u8; 32]).expect("the answer");
        assert_eq!(n, b"the answer\n".len());
        assert!(
            matches!(
                draining.frame_one(&mut ordinal).expect("scans"),
                Framed::Waiting
            ),
            "the residual's line never opens an exchange while draining"
        );
        assert!(!draining.owes(), "delivered, it owes nothing");
        assert!(!draining.wants_read(), "and is still never read");
    }

    /// **At the lower, a request never answered closes at once, and only a
    /// response the harness sent is waited on** (`weaver-harness-gate-contract`
    /// section 2: "every response the harness sent has been written"): the
    /// channel is ordered, so an exchange still open when the `Lower` is read
    /// will never be answered, and its connection closes unanswered, the
    /// harness having recorded the request refused. Perturbation: keep the
    /// open exchange, and its client never reads the end.
    #[test]
    fn at_the_lower_a_request_never_answered_closes_at_once() {
        use std::io::Read as _;
        let (open, mut client) = served_pair();
        let mut relay = Relay::new();
        relay.served.push(open);
        client.write_all(b"never answered\n").expect("writes");
        let Framed::Opened(_) = relay.read_one(0).expect("reads") else {
            panic!("the request frames");
        };
        relay.quiesce();
        assert_eq!(relay.served.len(), 1, "the quiesce keeps an open exchange");
        relay.lower();
        assert!(relay.served.is_empty(), "the lower closes it");
        assert_eq!(client.read(&mut [0u8; 8]).expect("the end"), 0);
        assert_eq!(relay.settle(false), Settled::Done { lost: vec![] });
    }

    /// **A response written but unread is waited on, and named lost at the
    /// settle instant** (`weaver-gate-Spec` section 4): with the gate's send
    /// buffer smaller than the response, the write leaves part of it owed; the
    /// lower keeps the connection, settle is open inside the instant and names
    /// the dialer past it. At default buffers a single response always fits
    /// (a 64 KiB envelope against about 176 KiB the kernel takes unread), so
    /// this is reachable only where the gate's send buffer is small; the test
    /// shrinks it. Perturbation: settle while a response is unwritten inside
    /// the instant, or name no dialer past it, and this fails.
    #[test]
    fn a_response_written_but_unread_is_named_lost_at_the_settle_instant() {
        use std::os::fd::AsFd as _;
        let (near, mut client) = UnixStream::pair().expect("pair");
        nix::sys::socket::setsockopt(&near.as_fd(), nix::sys::socket::sockopt::SndBuf, &4096)
            .expect("a small send buffer");
        let mut served = Served::admit(Admitted {
            stream: near,
            peer: PeerIdentity {
                uid: 12345,
                gid: 12345,
                pid: 1,
            },
        })
        .expect("admit");
        let mut ordinal = 0u64;
        client.write_all(b"the request\n").expect("writes");
        let Framed::Opened(_) = served.on_readable(&mut ordinal).expect("reads") else {
            panic!("the request frames");
        };
        let mut relay = Relay::new();
        relay.served.push(served);
        relay.quiesce();
        relay.served[0]
            .on_response(&TurnFrame::carry(&vec![b'x'; 60 * 1024]))
            .expect("routes");
        relay.served[0].on_writable().expect("writes what fits");
        assert!(
            relay.served[0].wants_write(),
            "part of the response is unwritten"
        );
        relay.lower();
        assert_eq!(
            relay.served.len(),
            1,
            "a response the harness sent is waited on"
        );
        assert_eq!(relay.settle(false), Settled::Open);
        assert_eq!(relay.settle(true), Settled::Done { lost: vec![12345] });
        drop(client);
    }

    /// **The envelopes waiting on the channel are flushed in order**, so a
    /// quiesce's or a forced lower's answer follows every frame this crate
    /// admitted (`weaver-harness-gate-contract` section 2). Perturbation:
    /// flush only the first, and the second never arrives before the end.
    #[test]
    fn the_pending_envelopes_flush_in_order() {
        let (near, far) = nix::sys::socket::socketpair(
            nix::sys::socket::AddressFamily::Unix,
            nix::sys::socket::SockType::SeqPacket,
            None,
            nix::sys::socket::SockFlag::SOCK_CLOEXEC,
        )
        .expect("pair");
        let (gate, harness) = (
            crate::channel::from_owned(near),
            crate::channel::from_owned(far),
        );
        let mut relay = Relay::new();
        for ordinal in [7, 8] {
            relay.pending.push_back(OrganEnvelope {
                exchange: ExchangeId {
                    opener: Opener::Gate,
                    ordinal,
                },
                position: Position::Open,
                payload: Payload::Frame(TurnFrame::carry(b"admitted")),
            });
        }
        relay.flush_pending(&gate).expect("flushes");
        assert!(relay.pending.is_empty());
        for ordinal in [7, 8] {
            assert_eq!(harness.recv().expect("arrives").exchange.ordinal, ordinal);
        }
    }

    /// **Two lines in one write open two exchanges in order, one at a
    /// time.** The scan bounds frames at the delimiter, the cap holds the
    /// second line in the residual while the first exchange is open, and
    /// the scan resumes when the response returns. The lines are not JSON,
    /// because the relay does not care.
    #[test]
    fn two_lines_frame_in_order_one_exchange_at_a_time() {
        let (mut served, mut client) = served_pair();
        let mut ordinal = 0u64;
        client
            .write_all(b"first line, plain text\nsecond line, also plain\n")
            .expect("client writes");

        let Framed::Opened(first) = served.on_readable(&mut ordinal).expect("reads") else {
            panic!("the first line frames");
        };
        assert_eq!(first.exchange.ordinal, 1);
        let Payload::Frame(frame) = &first.payload else {
            panic!("a frame");
        };
        assert_eq!(
            frame.octets().expect("canonical"),
            b"first line, plain text",
            "the frame carries the line's octets, unread"
        );
        // The dialer rides the frame, the uid the accept read, per
        // `weaver-harness-gate-contract` section 2. Perturbation: carry the
        // frame without it and every seeding line refuses at the harness,
        // whoever dialed.
        assert_eq!(frame.dialer, Some(12345), "the frame names its dialer");

        // The cap: the second line stands in the residual and no second
        // exchange opens while the first is unanswered.
        assert!(!served.wants_read(), "an open exchange leaves the read set");
        assert!(matches!(
            served.frame_one(&mut ordinal).expect("scans"),
            Framed::Waiting
        ));

        // The response returns and queues, and the scan still waits: the
        // cap admits the next line only after the drain, the outbound
        // buffer holding at most one response.
        served
            .on_response(&TurnFrame::carry(b"the first answer"))
            .expect("routes");
        assert!(matches!(
            served.frame_one(&mut ordinal).expect("scans"),
            Framed::Waiting
        ));

        // The queued response drains to the client, delimiter appended,
        // and only then does the residual's line frame in its turn.
        served.on_writable().expect("drains");
        let mut got = [0u8; 64];
        use std::io::Read as _;
        let n = client.read(&mut got).expect("client reads");
        assert_eq!(&got[..n], b"the first answer\n");
        let Framed::Opened(second) = served.frame_one(&mut ordinal).expect("scans") else {
            panic!("the residual's line frames after the drain");
        };
        assert_eq!(second.exchange.ordinal, 2);
    }

    /// **The response routes by the exchange's identity**, two clients
    /// speaking at once and each answered on its own connection, in
    /// whatever order the answers return.
    #[test]
    fn responses_route_by_the_exchange_identity() {
        let (served_a, mut client_a) = served_pair();
        let (served_b, mut client_b) = served_pair();
        let mut relay = Relay::new();
        relay.served.push(served_a);
        relay.served.push(served_b);

        client_a.write_all(b"from a\n").expect("a writes");
        client_b.write_all(b"from b\n").expect("b writes");
        let mut next = 0u64;
        let Framed::Opened(env_a) = relay.served[0].on_readable(&mut next).expect("a") else {
            panic!("a frames");
        };
        let Framed::Opened(env_b) = relay.served[1].on_readable(&mut next).expect("b") else {
            panic!("b frames");
        };

        // Answered in reverse order, routed by identity alone.
        relay
            .route(env_b.exchange.ordinal)
            .expect("b is owed")
            .on_response(&TurnFrame::carry(b"answer b"))
            .expect("routes");
        relay
            .route(env_a.exchange.ordinal)
            .expect("a is owed")
            .on_response(&TurnFrame::carry(b"answer a"))
            .expect("routes");
        for served in &mut relay.served {
            served.on_writable().expect("drains");
        }

        use std::io::Read as _;
        let mut got = [0u8; 32];
        let n = client_a.read(&mut got).expect("a reads");
        assert_eq!(&got[..n], b"answer a\n", "a receives a's answer");
        let n = client_b.read(&mut got).expect("b reads");
        assert_eq!(&got[..n], b"answer b\n", "b receives b's answer");
    }

    /// **A half-closed peer is a finished speaker, not a departed one.**
    /// The read side ends, the line already sent still frames, the response
    /// still delivers, and the connection is spent only when nothing
    /// remains: the shape every piped client takes.
    #[test]
    fn a_half_closed_peer_still_receives_its_answer() {
        let (mut served, mut client) = served_pair();
        let mut ordinal = 0u64;
        client
            .write_all(b"one line, then silence\n")
            .expect("writes");
        client
            .shutdown(std::net::Shutdown::Write)
            .expect("half-close");

        let Framed::Opened(envelope) = served.on_readable(&mut ordinal).expect("reads") else {
            panic!("the line frames despite the half-close");
        };
        let Payload::Frame(frame) = &envelope.payload else {
            panic!("a frame");
        };
        assert_eq!(
            frame.octets().expect("canonical"),
            b"one line, then silence"
        );
        assert!(!served.spent(), "an open exchange is not spent");

        served
            .on_response(&TurnFrame::carry(b"the answer"))
            .expect("routes");
        served.on_writable().expect("drains");
        use std::io::Read as _;
        let mut got = [0u8; 32];
        let n = client.read(&mut got).expect("client reads");
        assert_eq!(&got[..n], b"the answer\n");

        // The end of the read side is its own wake: the poll re-wakes on
        // the pending close, the read records it, and with nothing left
        // the connection is spent.
        assert!(matches!(
            served.on_readable(&mut ordinal).expect("reads the close"),
            Framed::Waiting
        ));
        assert!(
            served.spent(),
            "nothing remains and the connection is spent"
        );
    }

    /// **The bound is inclusive, and one octet more closes the
    /// connection.** A line of exactly the bound followed by its delimiter
    /// opens its exchange, and a connection fed one undelimited octet past
    /// the bound is gone with no exchange opened.
    #[test]
    fn the_bound_is_inclusive_and_one_more_closes() {
        let (mut served, mut client) = served_pair();
        let mut ordinal = 0u64;

        // Exactly the bound, then the delimiter: legal.
        let exact = vec![b'x'; LINE_BOUND];
        client.write_all(&exact).expect("writes");
        client.write_all(b"\n").expect("delimiter");
        let mut framed = false;
        for _ in 0..=(LINE_BOUND / 4096 + 2) {
            match served.on_readable(&mut ordinal).expect("reads") {
                Framed::Opened(envelope) => {
                    let Payload::Frame(frame) = &envelope.payload else {
                        panic!("a frame");
                    };
                    assert_eq!(frame.octets().expect("canonical").len(), LINE_BOUND);
                    framed = true;
                    break;
                }
                Framed::Waiting => continue,
            }
        }
        assert!(framed, "a bound-exact line frames within its reads");

        // One undelimited octet past the bound: the framing layer closes
        // the connection, no exchange opened.
        let (mut served, mut client) = served_pair();
        let over = vec![b'x'; LINE_BOUND + 1];
        client.write_all(&over).expect("writes");
        let mut outcome = Ok(Framed::Waiting);
        for _ in 0..=(LINE_BOUND / 4096 + 1) {
            outcome = served.on_readable(&mut ordinal);
            if outcome.is_err() {
                break;
            }
        }
        assert_eq!(
            outcome.err(),
            Some(Gone::PastTheBound),
            "past the bound is below any turn"
        );
    }
}
