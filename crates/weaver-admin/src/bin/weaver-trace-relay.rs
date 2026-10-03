//! The trace relay, per `weaver-admin-Spec` sections 1 and 6 and
//! `weaver-types-Spec` section 3.1: admin's second binary, started by the
//! start step under the relay account for a file sink, holding the trace door
//! for exactly one declared reader. It parses no event: it hashes one record's
//! bytes to verify a position and copies bytes.
//!
//! **What the start step hands it, at fixed numbers**: the bound listener at
//! 3, a read-only descriptor of the loaded run's sink at 4, the operations log
//! at 5, the read end of the lifetime pipe whose write end the worker holds at
//! 6, and the run lock's description at 9. Its vector is the reader's uid, the
//! agent's name and the boundary file's digest, for its log lines.

use std::io::Write as _;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::time::{Duration, Instant};

use weaver_types::{TraceControl, TraceHeader, TraceLine, TraceRequest};

const LISTENER_FD: RawFd = 3;
const SINK_FD: RawFd = 4;
const LOG_FD: RawFd = 5;
const LIFETIME_FD: RawFd = 6;
const RUN_LOCK_FD: RawFd = 9;

/// The request's bound, per `weaver-types-Spec` section 3.1: one line of at
/// most 4096 bytes, newline included.
const REQUEST_BOUND: usize = 4096;

/// How much of the sink one tick copies, and how often the loop looks. One
/// chunk per tick at most, so the loop always returns to the lifetime pipe,
/// the door and the reader's hangup, however large the backlog.
const CHUNK: usize = 64 * 1024;
const TICK_MS: i32 = 50;

/// How much of a position's verification one pass does: at most this many
/// blocks of `CHUNK` bytes read back or hashed, so a resume after a record of
/// any length never holds the loop away from the lifetime pipe, the door or
/// the follower.
const VERIFY_BLOCKS: usize = 16;

/// The relay's three waits, per `weaver-admin-Spec` section 6: the request
/// within five seconds of the connection, a heartbeat after five seconds
/// idle, and five seconds for the reader to take each queued write, a reader
/// that misses it being dropped. Fixed in production; a test shortens them
/// through `WEAVER_TRACE_RELAY_TEST_MS`, which the start step never sets, its
/// spawn clearing the environment.
#[derive(Clone, Copy)]
struct Timing {
    request: Duration,
    heartbeat: Duration,
    write: Duration,
}

impl Timing {
    fn read() -> Self {
        let production = Duration::from_secs(5);
        let wait = std::env::var("WEAVER_TRACE_RELAY_TEST_MS")
            .ok()
            .and_then(|ms| ms.parse::<u64>().ok())
            .map(Duration::from_millis)
            .unwrap_or(production);
        Timing {
            request: wait,
            heartbeat: wait,
            write: wait,
        }
    }
}

fn main() -> std::process::ExitCode {
    // **The first act**: every handed descriptor is kept close-on-exec, the
    // run lock among them, held for the relay's life and never closed.
    for fd in [LISTENER_FD, SINK_FD, LOG_FD, LIFETIME_FD, RUN_LOCK_FD] {
        // SAFETY: fcntl on a number this process may hold.
        let flags = unsafe { nix::libc::fcntl(fd, nix::libc::F_GETFD) };
        // SAFETY: as above, on a number that holds a descriptor.
        if flags == -1
            || unsafe { nix::libc::fcntl(fd, nix::libc::F_SETFD, flags | nix::libc::FD_CLOEXEC) }
                == -1
        {
            let _ = writeln!(
                std::io::stderr(),
                "weaver-trace-relay: descriptor {fd} was not handed: not started by admin"
            );
            return std::process::ExitCode::FAILURE;
        }
    }
    let mut args = std::env::args().skip(1);
    let (Some(reader), Some(agent), Some(boundary)) = (args.next(), args.next(), args.next())
    else {
        let _ = writeln!(
            std::io::stderr(),
            "weaver-trace-relay <reader-uid> <agent> <boundary-digest>"
        );
        return std::process::ExitCode::FAILURE;
    };
    let Ok(reader) = reader.parse::<u32>() else {
        let _ = writeln!(
            std::io::stderr(),
            "weaver-trace-relay: the reader is not a uid"
        );
        return std::process::ExitCode::FAILURE;
    };
    // SAFETY: each number was judged present above and is adopted once.
    let (listener, sink, log, lifetime) = unsafe {
        (
            OwnedFd::from_raw_fd(LISTENER_FD),
            OwnedFd::from_raw_fd(SINK_FD),
            std::fs::File::from_raw_fd(LOG_FD),
            OwnedFd::from_raw_fd(LIFETIME_FD),
        )
    };
    let mut relay = Relay {
        reader,
        timing: Timing::read(),
        log: Log {
            file: log,
            agent,
            boundary,
        },
        sink: std::fs::File::from(sink),
        follower: None,
        candidate: None,
    };
    relay.serve(&listener, &lifetime);
    std::process::ExitCode::SUCCESS
}

/// The relay's own operations-log lines, per `weaver-admin-Spec` section 8 and
/// #73's second item: a schema of its own, `actor` naming the relay, the
/// peer's uid where a peer stood, and none of an invocation's fields, since no
/// command line and no sudo cause exist in a process that outlived its load.
/// Each line is one `write`, so it never interleaves with an invocation's.
struct Log {
    file: std::fs::File,
    agent: String,
    boundary: String,
}

impl Log {
    fn record(&mut self, event: &str, peer_uid: Option<u32>, detail: &str) {
        let mut line = serde_json::Map::new();
        line.insert("wall_ms".into(), wall_ms().into());
        line.insert("actor".into(), "relay".into());
        line.insert("agent".into(), self.agent.clone().into());
        line.insert("event".into(), event.into());
        if let Some(uid) = peer_uid {
            line.insert("peer_uid".into(), uid.into());
        }
        line.insert("boundary".into(), self.boundary.clone().into());
        if !detail.is_empty() {
            line.insert("detail".into(), detail.into());
        }
        let mut rendered = serde_json::Value::Object(line).to_string();
        rendered.push('\n');
        let _ = self.file.write_all(rendered.as_bytes());
    }
}

fn wall_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// The one reader currently following the record. Its socket is
/// non-blocking, and what the relay owes it waits in `pending`, written as the
/// reader takes it within the write wait.
struct Follower {
    socket: std::os::unix::net::UnixStream,
    uid: u32,
    /// The sink offset through which bytes have been queued.
    position: u64,
    /// Bytes queued and not yet taken, and when the oldest was queued.
    pending: Vec<u8>,
    pending_since: Option<Instant>,
    /// When the reader last took a write, for the heartbeat.
    last_write: Instant,
    /// Bytes of an unfinished line have been queued, so no line of the
    /// relay's own may follow until that line completes.
    mid_line: bool,
    /// The stream's last line, `truncated`, is queued: the follower is kept
    /// only until the reader takes it whole or misses the write wait, and the
    /// connection then closes.
    closing: bool,
}

impl Follower {
    fn queue(&mut self, bytes: &[u8]) {
        if self.pending.is_empty() {
            self.pending_since = Some(Instant::now());
        }
        self.pending.extend_from_slice(bytes);
    }

    fn queue_line(&mut self, control: TraceControl) {
        let mut line = serde_json::to_string(&TraceLine {
            trace_stream: control,
        })
        .unwrap_or_default();
        line.push('\n');
        self.queue(line.as_bytes());
    }

    /// Writes what the reader will take now without blocking. Answers
    /// `false` where the reader has gone, or missed the write wait on a
    /// queued write.
    fn flush(&mut self, wait: Duration) -> bool {
        while !self.pending.is_empty() {
            match self.socket.write(&self.pending) {
                Ok(0) => return false,
                Ok(written) => {
                    self.pending.drain(..written);
                    self.last_write = Instant::now();
                    // The wait is the whole queued write's, from its queuing
                    // to its last byte, so a trickling reader misses it.
                    if self.pending.is_empty() {
                        self.pending_since = None;
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(_) => return false,
            }
        }
        self.pending_since
            .is_none_or(|since| since.elapsed() < wait)
    }
}

/// A connection from the declared reader whose request has not arrived
/// whole. Its socket is non-blocking and read as bytes arrive, inside the
/// loop, so a reader withholding its newline never holds the loop away from
/// the lifetime pipe or the follower.
struct Candidate {
    socket: std::os::unix::net::UnixStream,
    uid: u32,
    line: Vec<u8>,
    deadline: Instant,
    /// The request arrived whole and its position is being verified, a
    /// bounded step per pass.
    verifying: Option<Verification>,
}

/// What reading a candidate's request came to on one pass.
enum Request {
    Waiting,
    Whole(TraceRequest),
    Refused(&'static str),
}

impl Candidate {
    /// **Reads what has arrived of the one request**, per `weaver-types-Spec`
    /// section 3.1: a `TraceRequest` JSON object on one newline-terminated
    /// line of at most 4096 bytes, within five seconds of the connection,
    /// read a byte at a time so nothing after the newline is taken.
    fn read(&mut self) -> Request {
        use std::io::Read as _;
        let mut byte = [0u8; 1];
        loop {
            match self.socket.read(&mut byte) {
                Ok(0) => return Request::Refused("the request ended without a newline"),
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    return if Instant::now() >= self.deadline {
                        Request::Refused("the request arrived late")
                    } else {
                        Request::Waiting
                    };
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => return Request::Refused("the request could not be read"),
            }
            if byte[0] == b'\n' {
                return serde_json::from_slice(&self.line).map_or(
                    Request::Refused("the request does not parse"),
                    Request::Whole,
                );
            }
            self.line.push(byte[0]);
            if self.line.len() >= REQUEST_BOUND {
                return Request::Refused("the request is longer than its bound");
            }
        }
    }
}

struct Relay {
    reader: u32,
    timing: Timing,
    log: Log,
    sink: std::fs::File,
    follower: Option<Follower>,
    candidate: Option<Candidate>,
}

impl Relay {
    /// The loop: accept on the door, read a pending request as it arrives,
    /// follow the sink one chunk per tick at most, and end on the lifetime
    /// pipe's end-of-file, which is the worker's death, a slow reader or a
    /// withheld request never holding the loop away from it.
    fn serve(&mut self, listener: &OwnedFd, lifetime: &OwnedFd) {
        loop {
            let (follower_fd, follower_events) = match &self.follower {
                Some(follower) => (
                    follower.socket.as_raw_fd(),
                    nix::libc::POLLRDHUP
                        | if follower.pending.is_empty() {
                            0
                        } else {
                            nix::libc::POLLOUT
                        },
                ),
                None => (-1, 0),
            };
            let candidate_fd = self
                .candidate
                .as_ref()
                .map_or(-1, |candidate| candidate.socket.as_raw_fd());
            let mut polls = [
                nix::libc::pollfd {
                    fd: listener.as_raw_fd(),
                    events: nix::libc::POLLIN,
                    revents: 0,
                },
                nix::libc::pollfd {
                    fd: lifetime.as_raw_fd(),
                    events: nix::libc::POLLIN,
                    revents: 0,
                },
                nix::libc::pollfd {
                    fd: follower_fd,
                    events: follower_events,
                    revents: 0,
                },
                nix::libc::pollfd {
                    fd: candidate_fd,
                    events: nix::libc::POLLIN,
                    revents: 0,
                },
            ];
            // A verification in progress takes its next step without waiting.
            let wait = if self
                .candidate
                .as_ref()
                .is_some_and(|candidate| candidate.verifying.is_some())
            {
                0
            } else {
                TICK_MS
            };
            // SAFETY: poll over four pollfds this frame owns.
            let ready = unsafe { nix::libc::poll(polls.as_mut_ptr(), 4, wait) };
            if ready < 0 {
                if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                return;
            }
            if polls[1].revents != 0 {
                // The worker's death: the follower's connection is closed and
                // the reason logged, no stream line carrying one.
                if let Some(follower) = self.follower.take() {
                    self.log
                        .record("ended", Some(follower.uid), "the worker exited");
                } else {
                    self.log.record("ended", None, "the worker exited");
                }
                return;
            }
            if polls[2].revents & (nix::libc::POLLRDHUP | nix::libc::POLLHUP | nix::libc::POLLERR)
                != 0
                && let Some(follower) = self.follower.take()
            {
                self.log.record("disconnect", Some(follower.uid), "");
            }
            if polls[0].revents & nix::libc::POLLIN != 0 {
                self.accept(listener);
            }
            self.admit();
            self.follow();
        }
    }

    /// **Accepts one connection**: the peer's uid, from the kernel, before a
    /// byte is read, and a connection from anyone but the declared reader is
    /// refused, logged and closed. The reader's becomes the candidate, whose
    /// request the loop reads as it arrives, a newer connection replacing a
    /// candidate still waiting.
    fn accept(&mut self, listener: &OwnedFd) {
        // SAFETY: accept4 on the listener this frame borrows, close-on-exec.
        let raw = unsafe {
            nix::libc::accept4(
                listener.as_raw_fd(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                nix::libc::SOCK_CLOEXEC,
            )
        };
        if raw < 0 {
            return;
        }
        // SAFETY: accept4 answered a fresh descriptor this process owns.
        let socket = unsafe { std::os::unix::net::UnixStream::from_raw_fd(raw) };
        let uid =
            match nix::sys::socket::getsockopt(&socket, nix::sys::socket::sockopt::PeerCredentials)
            {
                Ok(credentials) => credentials.uid(),
                Err(_) => return,
            };
        if uid != self.reader {
            self.log
                .record("refused", Some(uid), "not the declared trace reader");
            return;
        }
        if socket.set_nonblocking(true).is_err() {
            return;
        }
        if let Some(old) = self.candidate.take() {
            self.log.record(
                "refused",
                Some(old.uid),
                "a newer connection from the reader before the request",
            );
        }
        self.candidate = Some(Candidate {
            socket,
            uid,
            line: Vec::with_capacity(REQUEST_BOUND),
            deadline: Instant::now() + self.timing.request,
            verifying: None,
        });
    }

    /// **Admits exactly one reader**: the candidate's one request line under
    /// its bounds and a position that verifies, a bounded step of the
    /// verification per pass, else the connection is refused, logged and
    /// closed. The newest admitted connection replaces the old.
    fn admit(&mut self) {
        let Some(mut candidate) = self.candidate.take() else {
            return;
        };
        if candidate.verifying.is_none() {
            let request = match candidate.read() {
                Request::Waiting => {
                    self.candidate = Some(candidate);
                    return;
                }
                Request::Refused(why) => {
                    self.log.record("refused", Some(candidate.uid), why);
                    return;
                }
                Request::Whole(request) => request,
            };
            match Verification::begin(&self.sink, request) {
                Ok(verification) => candidate.verifying = Some(verification),
                Err(why) => {
                    self.log.record("refused", Some(candidate.uid), why);
                    return;
                }
            }
        }
        let Some(verification) = candidate.verifying.as_mut() else {
            return;
        };
        match verification.step(&self.sink, VERIFY_BLOCKS) {
            None => {
                self.candidate = Some(candidate);
                return;
            }
            Some(Err(why)) => {
                self.log.record("refused", Some(candidate.uid), why);
                return;
            }
            Some(Ok(())) => {}
        }
        let offset = verification.offset;
        let Candidate { socket, uid, .. } = candidate;
        if let Some(old) = self.follower.take() {
            self.log.record(
                "replaced",
                Some(old.uid),
                "a newer connection from the reader",
            );
        }
        let mut follower = Follower {
            socket,
            uid,
            position: offset,
            pending: Vec::new(),
            pending_since: None,
            last_write: Instant::now(),
            mid_line: false,
            closing: false,
        };
        follower.queue_line(TraceControl::Header(identity(&self.sink)));
        self.log
            .record("connect", Some(uid), &format!("from offset {offset}"));
        self.follower = Some(follower);
    }

    /// **Follows the sink, one chunk per tick at most**: the reader's queued
    /// bytes are written as it takes them, a reader missing the write wait is
    /// dropped, and only an empty queue takes the next chunk of whole lines, a
    /// heartbeat while idle, or `truncated` then the end where the file shrank
    /// below the position. A truncation met mid-line closes without the line,
    /// since a line of the relay's own may not land inside a record.
    fn follow(&mut self) {
        let Some(mut follower) = self.follower.take() else {
            return;
        };
        if !follower.flush(self.timing.write) {
            self.log.record(
                "disconnect",
                Some(follower.uid),
                "the reader did not take a write in time",
            );
            return;
        }
        if !follower.pending.is_empty() {
            self.follower = Some(follower);
            return;
        }
        if follower.closing {
            // The `truncated` line is taken whole, and the stream ends.
            return;
        }
        let size = match self.sink.metadata() {
            Ok(metadata) => metadata.len(),
            Err(_) => {
                self.log
                    .record("ended", Some(follower.uid), "the sink cannot be read");
                return;
            }
        };
        if size < follower.position {
            self.log
                .record("truncated", Some(follower.uid), &format!("to {size} bytes"));
            if follower.mid_line {
                return;
            }
            // **The last line is written whole before the connection
            // closes**: the follower stays, closing, while the reader takes
            // it within the write wait, a full send buffer never cutting it.
            follower.queue_line(TraceControl::Truncated { size });
            follower.closing = true;
            if follower.flush(self.timing.write) && !follower.pending.is_empty() {
                self.follower = Some(follower);
            }
            return;
        }
        if follower.position < size {
            let want = ((size - follower.position) as usize).min(CHUNK);
            let mut buffer = vec![0u8; want];
            if let Ok(read) =
                std::os::unix::fs::FileExt::read_at(&self.sink, &mut buffer, follower.position)
                && read > 0
            {
                buffer.truncate(read);
                // Whole lines only, unless one line outruns a chunk.
                let send = match buffer.iter().rposition(|b| *b == b'\n') {
                    Some(last) => Some(last + 1),
                    None if read == CHUNK => Some(read),
                    None => None,
                };
                if let Some(send) = send {
                    follower.queue(&buffer[..send]);
                    follower.position += send as u64;
                    follower.mid_line = buffer[send - 1] != b'\n';
                }
            }
        } else if !follower.mid_line && follower.last_write.elapsed() >= self.timing.heartbeat {
            follower.queue_line(TraceControl::Heartbeat { wall_ms: wall_ms() });
        }
        if !follower.flush(self.timing.write) {
            self.log.record(
                "disconnect",
                Some(follower.uid),
                "the reader did not take a write in time",
            );
            return;
        }
        self.follower = Some(follower);
    }
}

/// **A position's verification, in bounded steps**, before a byte is sent:
/// offset zero carries no digest, and any other offset must fall just after a
/// newline, inside the file, with the record ending there hashing to the
/// digest, the record's bytes from the start of its line through its newline.
/// The record is found by reading back from the offset and hashed forward,
/// one block at a time, a record of any length costing one block of memory
/// and each step at most a fixed number of blocks.
struct Verification {
    offset: u64,
    digest: String,
    phase: Phase,
    block: Vec<u8>,
}

enum Phase {
    /// Reading back for the start of the record that ends at the offset.
    Back { start: u64 },
    /// Hashing the record forward from its start.
    Forward { at: u64, hasher: sha2::Sha256 },
    /// Nothing to read: offset zero.
    Verified,
}

impl Verification {
    /// The checks a single read answers, made at once: the digest's presence,
    /// the offset inside the file, and a newline just before it.
    fn begin(sink: &std::fs::File, request: TraceRequest) -> Result<Self, &'static str> {
        use std::os::unix::fs::FileExt;
        if request.offset == 0 {
            return match request.prior_digest {
                None => Ok(Verification {
                    offset: 0,
                    digest: String::new(),
                    phase: Phase::Verified,
                    block: Vec::new(),
                }),
                Some(_) => Err("offset zero carries no prior digest"),
            };
        }
        let Some(digest) = request.prior_digest else {
            return Err("a position past zero needs its prior digest");
        };
        let size = sink
            .metadata()
            .map_err(|_| "the sink cannot be read")?
            .len();
        if request.offset > size {
            return Err("the position is past the end of the file");
        }
        let mut last = [0u8; 1];
        sink.read_exact_at(&mut last, request.offset - 1)
            .map_err(|_| "the sink cannot be read")?;
        if last[0] != b'\n' {
            return Err("the position is not on a record boundary");
        }
        Ok(Verification {
            offset: request.offset,
            digest,
            // The walk back starts before the record's own newline.
            phase: Phase::Back {
                start: request.offset - 1,
            },
            block: vec![0u8; CHUNK],
        })
    }

    /// At most `budget` blocks of the walk back and the hash. Answers `None`
    /// while work remains, else whether the position verifies.
    fn step(&mut self, sink: &std::fs::File, budget: usize) -> Option<Result<(), &'static str>> {
        use sha2::Digest;
        use std::os::unix::fs::FileExt;
        let Verification {
            offset,
            digest,
            phase,
            block,
        } = self;
        for _ in 0..budget {
            match phase {
                Phase::Verified => return Some(Ok(())),
                Phase::Back { start } => {
                    let from = start.saturating_sub(block.len() as u64);
                    let len = (*start - from) as usize;
                    if sink.read_exact_at(&mut block[..len], from).is_err() {
                        return Some(Err("the sink cannot be read"));
                    }
                    match block[..len].iter().rposition(|b| *b == b'\n') {
                        Some(at) => {
                            *phase = Phase::Forward {
                                at: from + at as u64 + 1,
                                hasher: sha2::Sha256::new(),
                            }
                        }
                        None if from == 0 => {
                            *phase = Phase::Forward {
                                at: 0,
                                hasher: sha2::Sha256::new(),
                            }
                        }
                        None => *start = from,
                    }
                }
                Phase::Forward { at, hasher } => {
                    if *at >= *offset {
                        let found: String = hasher
                            .clone()
                            .finalize()
                            .iter()
                            .map(|b| format!("{b:02x}"))
                            .collect();
                        return Some(if &found == digest {
                            Ok(())
                        } else {
                            Err("the position does not verify")
                        });
                    }
                    let len = ((*offset - *at) as usize).min(block.len());
                    if sink.read_exact_at(&mut block[..len], *at).is_err() {
                        return Some(Err("the sink cannot be read"));
                    }
                    hasher.update(&block[..len]);
                    *at += len as u64;
                }
            }
        }
        None
    }
}

/// The served file's identity: device, inode, and the birth time where the
/// filesystem reports one, absent otherwise, per #73's fourth item.
fn identity(sink: &std::fs::File) -> TraceHeader {
    use std::os::unix::fs::MetadataExt;
    let (device, inode) = sink
        .metadata()
        .map(|metadata| (metadata.dev(), metadata.ino()))
        .unwrap_or((0, 0));
    TraceHeader {
        device,
        inode,
        birth_ns: birth_ns(sink.as_raw_fd()),
    }
}

fn birth_ns(fd: RawFd) -> Option<i128> {
    // SAFETY: statx with an empty path on a descriptor this process holds,
    // into a buffer this frame owns.
    let mut buffer: nix::libc::statx = unsafe { std::mem::zeroed() };
    let rc = unsafe {
        nix::libc::statx(
            fd,
            c"".as_ptr(),
            nix::libc::AT_EMPTY_PATH,
            nix::libc::STATX_BTIME,
            &mut buffer,
        )
    };
    if rc != 0 || buffer.stx_mask & nix::libc::STATX_BTIME == 0 {
        return None;
    }
    Some(buffer.stx_btime.tv_sec as i128 * 1_000_000_000 + buffer.stx_btime.tv_nsec as i128)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Scratch(std::path::PathBuf);

    impl Drop for Scratch {
        fn drop(&mut self) {
            drop(std::fs::remove_dir_all(&self.0));
        }
    }

    fn scratch(tag: &str) -> Scratch {
        let path =
            std::env::temp_dir().join(format!("weaver-relay-unit-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Scratch(path)
    }

    fn hex(bytes: &[u8]) -> String {
        use sha2::Digest;
        sha2::Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    /// A verification run to its end.
    fn verify_position(sink: &std::fs::File, request: &TraceRequest) -> Result<(), &'static str> {
        let mut verification = Verification::begin(sink, request.clone())?;
        loop {
            if let Some(answer) = verification.step(sink, 1) {
                return answer;
            }
        }
    }

    /// **A record longer than many blocks verifies, hashed whole**: the
    /// position after a 300 KiB record resumes with that record's digest and
    /// refuses another's. Perturbation: hash only the last block read and the
    /// true digest refuses.
    #[test]
    fn a_record_longer_than_a_block_verifies_whole() {
        let dir = scratch("long-record");
        let path = dir.0.join("trace.ndjson");
        let mut long = vec![b'x'; 300 * 1024];
        long.push(b'\n');
        let mut file = b"{\"a\":1}\n".to_vec();
        file.extend_from_slice(&long);
        std::fs::write(&path, &file).unwrap();
        let sink = std::fs::File::open(&path).unwrap();
        let at = |digest: String| TraceRequest {
            offset: file.len() as u64,
            prior_digest: Some(digest),
        };
        assert_eq!(verify_position(&sink, &at(hex(&long))), Ok(()));
        assert_eq!(
            verify_position(&sink, &at(hex(&long[long.len() - CHUNK..]))),
            Err("the position does not verify")
        );
    }

    fn relay_over(dir: &Scratch, path: &std::path::Path) -> Relay {
        Relay {
            reader: 0,
            timing: Timing {
                request: Duration::from_secs(5),
                heartbeat: Duration::from_secs(5),
                write: Duration::from_secs(5),
            },
            log: Log {
                file: std::fs::File::create(dir.0.join("admin.log")).unwrap(),
                agent: "alpha".into(),
                boundary: "b0b0".into(),
            },
            sink: std::fs::File::open(path).unwrap(),
            follower: None,
            candidate: None,
        }
    }

    /// **A resume's verification is spread over passes**: after a record far
    /// longer than one pass's blocks, a pass of `admit` leaves the candidate
    /// waiting, the loop free to look at the lifetime pipe and the follower,
    /// and later passes promote it. Perturbation: verify the whole record in
    /// one pass and the first `admit` promotes it.
    #[test]
    fn a_resume_after_a_long_record_is_verified_across_passes() {
        let dir = scratch("verify-passes");
        let path = dir.0.join("trace.ndjson");
        let mut long = vec![b'x'; 3 * VERIFY_BLOCKS * CHUNK];
        long.push(b'\n');
        std::fs::write(&path, &long).unwrap();
        let mut relay = relay_over(&dir, &path);
        let (ours, _theirs) = std::os::unix::net::UnixStream::pair().unwrap();
        ours.set_nonblocking(true).unwrap();
        let request = TraceRequest {
            offset: long.len() as u64,
            prior_digest: Some(hex(&long)),
        };
        relay.candidate = Some(Candidate {
            socket: ours,
            uid: 0,
            line: Vec::new(),
            deadline: Instant::now() + Duration::from_secs(5),
            verifying: Some(Verification::begin(&relay.sink, request).unwrap()),
        });
        relay.admit();
        assert!(
            relay.follower.is_none(),
            "one pass does not finish the record"
        );
        assert!(
            relay.candidate.is_some(),
            "and the candidate waits for the next"
        );
        let mut passes = 1;
        while relay.follower.is_none() {
            assert!(relay.candidate.is_some(), "the verification refused");
            relay.admit();
            passes += 1;
            assert!(passes < 100, "the verification never finished");
        }
        assert!(passes >= 3, "{passes} passes");
    }

    /// **The `truncated` line reaches a reader whose send buffer is full**:
    /// a truncation met while the socket takes nothing more keeps the
    /// follower, closing, until the reader drains and takes the line whole,
    /// and only then does the connection close. Perturbation: drop the
    /// follower right after one flush, as the first form did, and the reader
    /// reads end-of-file with no `truncated` line.
    #[test]
    fn the_truncated_line_reaches_a_reader_behind_a_full_buffer() {
        use std::io::Read as _;
        let dir = scratch("truncate-full");
        let path = dir.0.join("trace.ndjson");
        std::fs::write(&path, b"{\"a\":1}\n").unwrap();
        let (ours, mut theirs) = std::os::unix::net::UnixStream::pair().unwrap();
        ours.set_nonblocking(true).unwrap();
        // Fill the send buffer until it takes nothing more.
        let filler = [b'.'; 4096];
        let mut filled = 0usize;
        loop {
            match (&ours).write(&filler) {
                Ok(n) => filled += n,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => panic!("{e}"),
            }
        }
        let mut relay = Relay {
            reader: 0,
            timing: Timing {
                request: Duration::from_secs(5),
                heartbeat: Duration::from_secs(5),
                write: Duration::from_secs(5),
            },
            log: Log {
                file: std::fs::File::create(dir.0.join("admin.log")).unwrap(),
                agent: "alpha".into(),
                boundary: "b0b0".into(),
            },
            sink: std::fs::File::open(&path).unwrap(),
            follower: Some(Follower {
                socket: ours,
                uid: 0,
                position: 8,
                pending: Vec::new(),
                pending_since: None,
                last_write: Instant::now(),
                mid_line: false,
                closing: false,
            }),
            candidate: None,
        };
        std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(3)
            .unwrap();
        relay.follow();
        assert!(
            relay.follower.as_ref().is_some_and(|f| f.closing),
            "the follower is kept, closing, behind the full buffer"
        );
        // The reader drains, and the relay finishes the line and closes.
        let reader = std::thread::spawn(move || {
            let mut all = Vec::new();
            theirs.read_to_end(&mut all).unwrap();
            all
        });
        let deadline = Instant::now() + Duration::from_secs(5);
        while relay.follower.is_some() {
            assert!(
                Instant::now() < deadline,
                "the relay never finished the line"
            );
            relay.follow();
            std::thread::sleep(Duration::from_millis(5));
        }
        let all = reader.join().unwrap();
        assert_eq!(
            all.len(),
            filled + b"{\"trace_stream\":{\"truncated\":{\"size\":3}}}\n".len()
        );
        assert!(
            all.ends_with(b"{\"trace_stream\":{\"truncated\":{\"size\":3}}}\n"),
            "the stream ends with the truncated line"
        );
    }
}
