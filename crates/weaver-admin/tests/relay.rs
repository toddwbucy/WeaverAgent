//! The trace relay exercised as a process, per `weaver-admin-Spec` section 6
//! and `weaver-types-Spec` section 3.1. Each test starts the built relay the
//! way the start step does, its descriptors at their fixed numbers, and
//! dials its door as this test's own uid, which the relay is told is the
//! declared reader unless a test says otherwise.
#![cfg(target_os = "linux")]

use std::io::{BufRead, BufReader, Write};
use std::os::fd::{AsRawFd, OwnedFd, RawFd};
use std::os::unix::net::{UnixListener, UnixStream};
use std::os::unix::process::CommandExt;
use std::time::Duration;

struct Scratch(std::path::PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        drop(std::fs::remove_dir_all(&self.0));
    }
}

/// A relay under test, killed when the test ends.
struct Relay {
    child: std::process::Child,
    socket: std::path::PathBuf,
    sink: std::path::PathBuf,
    log: std::path::PathBuf,
    /// The lifetime pipe's write end, the worker's in production.
    lifetime: Option<OwnedFd>,
    _scratch: Scratch,
}

impl Drop for Relay {
    fn drop(&mut self) {
        drop(self.child.kill());
        drop(self.child.wait());
    }
}

/// A close-on-exec copy at 100 or above, so no source is a target number.
fn high(fd: RawFd) -> OwnedFd {
    // SAFETY: F_DUPFD_CLOEXEC on a descriptor this test owns.
    let raw = unsafe { nix::libc::fcntl(fd, nix::libc::F_DUPFD_CLOEXEC, 100) };
    assert!(raw >= 100);
    // SAFETY: a fresh descriptor this test owns.
    unsafe { std::os::fd::FromRawFd::from_raw_fd(raw) }
}

/// Starts the relay over a sink holding `contents`, telling it the reader is
/// `reader`, at production timing.
fn start(tag: &str, contents: &[u8], reader: u32) -> Relay {
    start_timed(tag, contents, reader, None)
}

/// As `start`, the relay's three waits set to `wait_ms` where given.
fn start_timed(tag: &str, contents: &[u8], reader: u32, wait_ms: Option<u64>) -> Relay {
    let dir = std::env::temp_dir().join(format!("weaver-relay-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let scratch = Scratch(dir.clone());
    let socket = dir.join("trace.sock");
    let sink = dir.join("trace.ndjson");
    let log = dir.join("admin.log");
    std::fs::write(&sink, contents).unwrap();
    let listener = high(UnixListener::bind(&socket).unwrap().as_raw_fd());
    let sink_fd = high(std::fs::File::open(&sink).unwrap().as_raw_fd());
    let log_fd = high(
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log)
            .unwrap()
            .as_raw_fd(),
    );
    let lock_fd = high(
        std::fs::File::create(dir.join("run.lock"))
            .unwrap()
            .as_raw_fd(),
    );
    let (read_end, write_end) = nix::unistd::pipe2(nix::fcntl::OFlag::O_CLOEXEC).unwrap();
    let read_fd = high(read_end.as_raw_fd());
    drop(read_end);
    let gifts = [
        (listener.as_raw_fd(), 3),
        (sink_fd.as_raw_fd(), 4),
        (log_fd.as_raw_fd(), 5),
        (read_fd.as_raw_fd(), 6),
        (lock_fd.as_raw_fd(), 9),
    ];
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_weaver-trace-relay"));
    if let Some(ms) = wait_ms {
        command.env("WEAVER_TRACE_RELAY_TEST_MS", ms.to_string());
    }
    command
        .args([reader.to_string(), "alpha".to_string(), "b0b0".to_string()])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    // SAFETY: dup2 is async-signal-safe and clears the flag at each target.
    unsafe {
        command.pre_exec(move || {
            for (source, target) in gifts {
                if nix::libc::dup2(source, target) == -1 {
                    return Err(std::io::Error::last_os_error());
                }
            }
            Ok(())
        });
    }
    let child = command.spawn().expect("the relay starts");
    Relay {
        child,
        socket,
        sink,
        log,
        lifetime: Some(write_end),
        _scratch: scratch,
    }
}

fn me() -> u32 {
    nix::unistd::getuid().as_raw()
}

/// Dials the door and sends one request line.
fn dial(relay: &Relay, request: &str) -> BufReader<UnixStream> {
    let mut stream = UnixStream::connect(&relay.socket).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    stream.write_all(request.as_bytes()).unwrap();
    BufReader::new(stream)
}

/// One line, or the empty string at end-of-file. A refused connection the
/// relay closed with the request unread reads as a reset, which is the same
/// nothing received.
fn line(reader: &mut BufReader<UnixStream>) -> String {
    let mut line = String::new();
    match reader.read_line(&mut line) {
        Ok(_) => line,
        Err(e) if e.kind() == std::io::ErrorKind::ConnectionReset => String::new(),
        Err(e) => panic!("reading the door: {e}"),
    }
}

fn digest(bytes: &[u8]) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn logged(relay: &Relay) -> String {
    std::fs::read_to_string(&relay.log).unwrap_or_default()
}

/// **The reader gets the header, then the record exactly as written, then
/// what is appended**, per `weaver-types-Spec` section 3.1. Perturbation: skip
/// the header and the first line read is a record.
#[test]
fn the_reader_gets_the_header_then_the_record_then_what_follows() {
    let relay = start("follow", b"{\"a\":1}\n{\"b\":2}\n", me());
    let mut reader = dial(&relay, "{\"offset\":0}\n");
    let header: serde_json::Value = serde_json::from_str(&line(&mut reader)).unwrap();
    assert!(
        header["trace_stream"]["header"]["inode"].as_u64().is_some(),
        "{header}"
    );
    assert_eq!(line(&mut reader), "{\"a\":1}\n");
    assert_eq!(line(&mut reader), "{\"b\":2}\n");
    let mut sink = std::fs::OpenOptions::new()
        .append(true)
        .open(&relay.sink)
        .unwrap();
    sink.write_all(b"{\"c\":3}\n").unwrap();
    assert_eq!(
        line(&mut reader),
        "{\"c\":3}\n",
        "a line appended later follows"
    );
    assert!(logged(&relay).contains("\"event\":\"connect\""));
}

/// **A position resumes only where it verifies**: the offset after the first
/// record with that record's digest streams from the second, and a wrong
/// digest is refused and logged before a byte is sent. Perturbation: skip the
/// digest comparison and the wrong digest streams.
#[test]
fn a_position_resumes_only_where_it_verifies() {
    let first = b"{\"a\":1}\n";
    let relay = start("resume", b"{\"a\":1}\n{\"b\":2}\n", me());
    let good = format!(
        "{{\"offset\":{},\"prior_digest\":\"{}\"}}\n",
        first.len(),
        digest(first)
    );
    let mut reader = dial(&relay, &good);
    let _header = line(&mut reader);
    assert_eq!(line(&mut reader), "{\"b\":2}\n");
    let bad = format!(
        "{{\"offset\":{},\"prior_digest\":\"{}\"}}\n",
        first.len(),
        "0".repeat(64)
    );
    let mut refused = dial(&relay, &bad);
    assert_eq!(line(&mut refused), "", "refused before a byte is sent");
    let off_boundary = format!("{{\"offset\":3,\"prior_digest\":\"{}\"}}\n", digest(first));
    let mut refused = dial(&relay, &off_boundary);
    assert_eq!(line(&mut refused), "");
    let log = logged(&relay);
    assert!(log.contains("the position does not verify"), "{log}");
    assert!(log.contains("not on a record boundary"), "{log}");
}

/// **Only the declared reader is admitted**, by the kernel's peer credential
/// before a byte of the request is read: told the reader is another uid, the
/// relay refuses this test's connection and logs the uid it refused.
/// Perturbation: admit any peer and the stranger receives the header.
#[test]
fn only_the_declared_reader_is_admitted() {
    let relay = start("stranger", b"{\"a\":1}\n", me() + 1);
    let mut refused = dial(&relay, "{\"offset\":0}\n");
    assert_eq!(line(&mut refused), "", "a stranger receives nothing");
    std::thread::sleep(Duration::from_millis(200));
    let log = logged(&relay);
    assert!(log.contains("\"event\":\"refused\""), "{log}");
    assert!(log.contains(&format!("\"peer_uid\":{}", me())), "{log}");
}

/// **The newest connection from the reader replaces the old**: the first
/// follower is ended and the replacement logged. Perturbation: keep both and
/// the first never reads end-of-file.
#[test]
fn the_newest_reader_connection_replaces_the_old() {
    let relay = start("replace", b"{\"a\":1}\n", me());
    let mut first = dial(&relay, "{\"offset\":0}\n");
    let _ = line(&mut first);
    let _ = line(&mut first);
    let mut second = dial(&relay, "{\"offset\":0}\n");
    let _ = line(&mut second);
    assert_eq!(line(&mut first), "", "the old follower ends");
    assert!(logged(&relay).contains("\"event\":\"replaced\""));
}

/// **A truncation ends the stream with `truncated`**: a file shrunk below the
/// follower's position sends the size and closes. Perturbation: continue from
/// the old position and no `truncated` line arrives.
#[test]
fn a_truncation_ends_the_stream() {
    let relay = start("truncate", b"{\"a\":1}\n{\"b\":2}\n", me());
    let mut reader = dial(&relay, "{\"offset\":0}\n");
    for _ in 0..3 {
        let _ = line(&mut reader);
    }
    std::fs::OpenOptions::new()
        .write(true)
        .open(&relay.sink)
        .unwrap()
        .set_len(3)
        .unwrap();
    assert_eq!(
        line(&mut reader),
        "{\"trace_stream\":{\"truncated\":{\"size\":3}}}\n"
    );
    assert_eq!(line(&mut reader), "", "and the stream ends");
}

/// **A request past its bound is refused**: a line longer than 4096 bytes
/// gets nothing, and the refusal is logged. Perturbation: drop the length
/// bound and the relay reads the long line whole.
#[test]
fn a_request_past_its_bound_is_refused() {
    let relay = start("long", b"{\"a\":1}\n", me());
    let long = format!("{{\"offset\":0,\"pad\":\"{}\"}}\n", "x".repeat(5000));
    let mut stream = UnixStream::connect(&relay.socket).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let _ = stream.write_all(long.as_bytes());
    let mut reader = BufReader::new(stream);
    assert_eq!(line(&mut reader), "");
    assert!(logged(&relay).contains("longer than its bound"));
}

/// **The relay dies with the worker**: the lifetime pipe's last write end
/// closing, as at the worker's death, ends the relay and its follower.
/// Perturbation: ignore the pipe and the relay outlives its worker.
#[test]
fn the_relay_dies_with_the_worker() {
    let mut relay = start("lifetime", b"{\"a\":1}\n", me());
    let mut reader = dial(&relay, "{\"offset\":0}\n");
    let _ = line(&mut reader);
    drop(relay.lifetime.take());
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let status = loop {
        if let Some(status) = relay.child.try_wait().unwrap() {
            break status;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the relay outlived its worker"
        );
        std::thread::sleep(Duration::from_millis(20));
    };
    assert!(status.success());
    let _ = line(&mut reader);
    assert_eq!(line(&mut reader), "", "the follower is ended");
    assert!(logged(&relay).contains("the worker exited"));
}

/// A sink of many short lines, about `bytes` long.
fn backlog(bytes: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes + 64);
    let mut n = 0u64;
    while out.len() < bytes {
        out.extend_from_slice(format!("{{\"n\":{n},\"pad\":\"xxxxxxxxxxxxxxxx\"}}\n").as_bytes());
        n += 1;
    }
    out
}

/// A reader that takes 4 KiB every 200 ms in its own thread, as a slow
/// connection resuming from zero on a large trace does.
fn trickle(relay: &Relay) -> std::thread::JoinHandle<()> {
    let mut stream = UnixStream::connect(&relay.socket).unwrap();
    stream.write_all(b"{\"offset\":0}\n").unwrap();
    std::thread::spawn(move || {
        use std::io::Read;
        let mut buffer = [0u8; 4096];
        let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
        for _ in 0..200 {
            match stream.read(&mut buffer) {
                Ok(0) | Err(_) => return,
                Ok(_) => std::thread::sleep(Duration::from_millis(200)),
            }
        }
    })
}

/// **The relay dies with its worker however slow its reader**, per Spec
/// section 6: with a large backlog and a reader trickling it, the lifetime
/// pipe's end still ends the relay within a bound, the loop returning to the
/// pipe every tick and never draining the backlog in one pass. Perturbation:
/// copy the whole backlog before returning to poll and the relay outlives
/// its worker past the bound, holding the run lock.
#[test]
fn the_relay_dies_with_its_worker_behind_a_slow_reader() {
    let mut relay = start("slow-lifetime", &backlog(8 << 20), me());
    let reader = trickle(&relay);
    std::thread::sleep(Duration::from_millis(300));
    drop(relay.lifetime.take());
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while relay.child.try_wait().unwrap().is_none() {
        assert!(
            std::time::Instant::now() < deadline,
            "the relay outlived its worker behind a slow reader"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    // The reader thread ends on its own once the closed socket drains.
    drop(reader);
}

/// **A reader that does not take a queued write in time is dropped**: with
/// the write wait shortened, a trickling reader misses it and is logged.
/// Perturbation: reset the wait on every partial write and the trickler is
/// never dropped.
#[test]
fn a_reader_too_slow_for_the_write_wait_is_dropped() {
    let relay = start_timed("slow-reader", &backlog(8 << 20), me(), Some(300));
    let reader = trickle(&relay);
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while !logged(&relay).contains("did not take a write in time") {
        assert!(std::time::Instant::now() < deadline, "{}", logged(&relay));
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = reader.join();
}

/// **A request that arrives late is refused**, the wait shortened for the
/// test. Perturbation: drop the request's deadline and the silent connection
/// holds the door.
#[test]
fn a_late_request_is_refused() {
    let relay = start_timed("late", b"{\"a\":1}\n", me(), Some(300));
    let stream = UnixStream::connect(&relay.socket).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut reader = BufReader::new(stream);
    assert_eq!(line(&mut reader), "", "closed without a byte");
    assert!(logged(&relay).contains("arrived late"));
}

/// **A heartbeat follows the idle wait**, shortened for the test, once the
/// record is streamed. Perturbation: drop the heartbeat and the read times
/// out.
#[test]
fn a_heartbeat_follows_while_idle() {
    let relay = start_timed("heartbeat", b"{\"a\":1}\n", me(), Some(300));
    let mut reader = dial(&relay, "{\"offset\":0}\n");
    let _header = line(&mut reader);
    assert_eq!(line(&mut reader), "{\"a\":1}\n");
    let heartbeat = line(&mut reader);
    assert!(
        heartbeat.starts_with("{\"trace_stream\":{\"heartbeat\":"),
        "{heartbeat}"
    );
}

/// **The position's edges refuse**: an offset past the end, offset zero
/// carrying a digest, and an offset past zero carrying none. Perturbation:
/// skip any one check and its case streams.
#[test]
fn the_positions_edges_refuse() {
    let first = b"{\"a\":1}\n";
    let relay = start("edges", first, me());
    for (request, why) in [
        (
            format!(
                "{{\"offset\":999,\"prior_digest\":\"{}\"}}\n",
                digest(first)
            ),
            "past the end",
        ),
        (
            format!("{{\"offset\":0,\"prior_digest\":\"{}\"}}\n", digest(first)),
            "carries no prior digest",
        ),
        (
            format!("{{\"offset\":{}}}\n", first.len()),
            "needs its prior digest",
        ),
    ] {
        let mut refused = dial(&relay, &request);
        assert_eq!(line(&mut refused), "", "{why}");
        std::thread::sleep(Duration::from_millis(100));
        assert!(logged(&relay).contains(why), "{why}: {}", logged(&relay));
    }
}

/// **A line longer than one chunk streams whole**, unbroken by any line of
/// the relay's own. Perturbation: hold every chunk without a newline and the
/// long line never arrives.
#[test]
fn a_line_longer_than_a_chunk_streams_whole() {
    let mut long = format!("{{\"pad\":\"{}\"}}", "y".repeat(100 * 1024)).into_bytes();
    long.push(b'\n');
    let relay = start("long-line", &long, me());
    let mut reader = dial(&relay, "{\"offset\":0}\n");
    let _header = line(&mut reader);
    assert_eq!(line(&mut reader).as_bytes(), long.as_slice());
}

/// **The header carries the birth time exactly where the filesystem reports
/// one**: compared against this test's own `statx` of the sink.
/// Perturbation: always write zero and the absent case carries a value.
#[test]
fn the_header_carries_the_birth_time_where_reported() {
    let relay = start("birth", b"{\"a\":1}\n", me());
    let mut reader = dial(&relay, "{\"offset\":0}\n");
    let header: serde_json::Value = serde_json::from_str(&line(&mut reader)).unwrap();
    let c_path = std::ffi::CString::new(relay.sink.as_os_str().as_encoded_bytes()).unwrap();
    // SAFETY: statx into a buffer this test owns.
    let mut buffer: nix::libc::statx = unsafe { std::mem::zeroed() };
    let rc = unsafe {
        nix::libc::statx(
            nix::libc::AT_FDCWD,
            c_path.as_ptr(),
            0,
            nix::libc::STATX_BTIME,
            &mut buffer,
        )
    };
    let reported = rc == 0 && buffer.stx_mask & nix::libc::STATX_BTIME != 0;
    let birth = &header["trace_stream"]["header"]["birth_ns"];
    if reported {
        let expected =
            buffer.stx_btime.tv_sec as i128 * 1_000_000_000 + buffer.stx_btime.tv_nsec as i128;
        assert_eq!(birth.as_i64().map(i128::from), Some(expected), "{header}");
    } else {
        assert!(birth.is_null(), "absent where unreported: {header}");
    }
}
