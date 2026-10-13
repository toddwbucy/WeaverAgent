use super::*;

struct Scratch(std::path::PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        drop(std::fs::remove_dir_all(&self.0));
    }
}

fn scratch(tag: &str) -> Scratch {
    let path = std::env::temp_dir().join(format!("weaver-relay-unit-{tag}-{}", std::process::id()));
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
