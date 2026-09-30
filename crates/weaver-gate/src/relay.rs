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
    /// The peer closed its writing half. A half-closed connection is a
    /// client that said its piece and awaits the answer, so the read side
    /// ends while everything owed still delivers, and the connection leaves
    /// only when it is spent.
    read_closed: bool,
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
            stream: admitted.stream,
            input: Vec::new(),
            outbound: Vec::new(),
            exchange: None,
            read_closed: false,
        })
    }

    /// Whether the read set wants this connection: only while no exchange
    /// is open and no response stands undelivered, which is the cap doing
    /// the flow control, the outbound buffer holding at most one response
    /// per the Spec's own sentence. A read-closed connection wants nothing
    /// read again.
    pub fn wants_read(&self) -> bool {
        !self.read_closed && self.exchange.is_none() && self.outbound.is_empty()
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
        self.read_closed
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
        if self.exchange.is_some() || !self.outbound.is_empty() {
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
                    payload: Payload::Frame(TurnFrame::carry(&line)),
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
