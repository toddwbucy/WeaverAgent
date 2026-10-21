//! Real stream fixtures for framing and the original invocation deadline.
//! The synthetic request exercises transport, not STE100 qualification.

use super::*;
use std::io::{Read, Write};
use std::os::fd::AsFd;
use std::os::unix::net::UnixStream;
use std::time::Duration;
use weaver_types::state_flow::{
    DecimalU64, Digest, PromptDuty, STATE_CONTENT_SCHEMA, STATE_REQUEST_LINE_BOUND, SeedCertificate,
};

const TEST_BOUND: Duration = Duration::from_secs(3);

fn request() -> SeedRequest {
    let digest = Digest::new("a".repeat(64)).unwrap();
    SeedRequest {
        schema_id: STATE_CONTENT_SCHEMA.into(),
        seed_id: "b".repeat(32),
        scheme_id: "synthetic-transport-only".into(),
        ste100_scheme: "synthetic-transport-only".into(),
        checker_artifact_sha256: digest.clone(),
        prompt_text: "Read item.".into(),
        raw_block: serde_json::value::RawValue::from_string(
            r#"{"type":"text","text":"Read item."}"#.into(),
        )
        .unwrap(),
        bindings: vec![],
        certificate: SeedCertificate {
            prompt_sha256: digest.clone(),
            scheme_id: "synthetic-transport-only".into(),
            checker_artifact_sha256: digest,
            review_reference: "transport fixture only".into(),
            bindings: vec![],
            duties: PromptDuty::ALL.to_vec(),
        },
    }
}

fn created(request: &SeedRequest) -> SeedCreated {
    SeedCreated {
        schema_id: request.schema_id.clone(),
        seed_id: request.seed_id.clone(),
        room_name: format!("seed-{}.sqlite", request.seed_id),
        size_bytes: DecimalU64(4096),
    }
}

fn response(request: &SeedRequest) -> Vec<u8> {
    encode_seed_line(
        &SeedFrame::Created {
            created: created(request),
        },
        FrameDirection::Response,
    )
    .unwrap()
}

fn pair() -> (UnixStream, UnixStream) {
    let (ours, peer) = UnixStream::pair().unwrap();
    peer.set_read_timeout(Some(TEST_BOUND)).unwrap();
    peer.set_write_timeout(Some(TEST_BOUND)).unwrap();
    (ours, peer)
}

fn read_request(peer: &mut UnixStream) -> Vec<u8> {
    let mut result = Vec::new();
    let mut buffer = [0u8; 997];
    loop {
        let count = peer.read(&mut buffer).unwrap();
        assert_ne!(count, 0, "admin closed before sending its request");
        result.extend_from_slice(&buffer[..count]);
        if result.last() == Some(&b'\n') {
            assert!(matches!(
                decode_seed_line(&result, FrameDirection::Request).unwrap(),
                SeedFrame::Seed { .. }
            ));
            return result;
        }
    }
}

fn hold_until_close(peer: &mut UnixStream) {
    let mut byte = [0u8; 1];
    assert_eq!(peer.read(&mut byte).unwrap(), 0, "no second seed request");
}

fn with_answer(answer: Vec<u8>) -> Result<SeedCreated, ExchangeFailure> {
    let (ours, mut peer) = pair();
    // Queue the complete small answer first, so trailing-frame checks do not
    // depend on the relative scheduling of the reader and writer.
    peer.write_all(&answer).unwrap();
    let result = exchange(ours.as_fd(), &request(), Instant::now() + TEST_BOUND);
    read_request(&mut peer);
    drop(ours);
    result
}

#[test]
fn seed_transport_preserves_request_bytes_and_reads_a_fragmented_answer() {
    let request = request();
    let expected_request = encode_seed_line(
        &SeedFrame::Seed {
            request: Box::new(request.clone()),
        },
        FrameDirection::Request,
    )
    .unwrap();
    let answer = response(&request);
    let (ours, mut peer) = pair();
    let server = std::thread::spawn(move || {
        assert_eq!(read_request(&mut peer), expected_request);
        for piece in answer.chunks(31) {
            peer.write_all(piece).unwrap();
            std::thread::sleep(Duration::from_millis(2));
        }
        hold_until_close(&mut peer);
    });
    let outcome = exchange(ours.as_fd(), &request, Instant::now() + TEST_BOUND).unwrap();
    assert_eq!(outcome, created(&request));
    // The per-call MSG_DONTWAIT flag must not alter a borrowed socket's mode.
    // SAFETY: F_GETFL reads the live descriptor and takes no third argument.
    let flags = unsafe { nix::libc::fcntl(ours.as_raw_fd(), nix::libc::F_GETFL) };
    assert!(flags >= 0);
    assert_eq!(flags & nix::libc::O_NONBLOCK, 0);
    drop(ours);
    server.join().unwrap();
}

fn small_send_buffer(stream: &UnixStream) {
    let size: nix::libc::c_int = 4096;
    // SAFETY: this fixture owns the socket and passes one correctly sized integer.
    assert_eq!(
        unsafe {
            nix::libc::setsockopt(
                stream.as_raw_fd(),
                nix::libc::SOL_SOCKET,
                nix::libc::SO_SNDBUF,
                (&size as *const nix::libc::c_int).cast(),
                std::mem::size_of_val(&size) as nix::libc::socklen_t,
            )
        },
        0
    );
}

#[test]
fn seed_transport_finishes_partial_sends_without_changing_the_request() {
    let mut request = request();
    request.prompt_text = "x".repeat(1024 * 1024);
    let expected = encode_seed_line(
        &SeedFrame::Seed {
            request: Box::new(request.clone()),
        },
        FrameDirection::Request,
    )
    .unwrap();
    let answer = response(&request);
    let (ours, mut peer) = pair();
    small_send_buffer(&ours);
    let server = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(30));
        assert_eq!(read_request(&mut peer), expected);
        peer.write_all(&answer).unwrap();
        hold_until_close(&mut peer);
    });
    assert_eq!(
        exchange(ours.as_fd(), &request, Instant::now() + TEST_BOUND).unwrap(),
        created(&request)
    );
    drop(ours);
    server.join().unwrap();
}

#[test]
fn seed_transport_refusal_preserves_its_code_and_detail() {
    let answer = encode_seed_line(
        &SeedFrame::Refused {
            code: SeedRefusal::SeedCertificateInvalid,
            detail: "the exact certificate does not match".into(),
        },
        FrameDirection::Response,
    )
    .unwrap();
    assert!(matches!(
        with_answer(answer),
        Err(ExchangeFailure::Refused {code: SeedRefusal::SeedCertificateInvalid, detail})
            if detail == "the exact certificate does not match"
    ));
}

#[test]
fn seed_transport_refuses_wrong_protocol_id_direction_identity_and_duplicate_fields() {
    let valid = String::from_utf8(response(&request())).unwrap();
    let wrong_schema = valid.replace(STATE_CONTENT_SCHEMA, "other-schema");
    let wrong_seed = valid.replace(&"b".repeat(32), &"c".repeat(32));
    let request_frame = encode_seed_line(
        &SeedFrame::Seed {
            request: Box::new(request()),
        },
        FrameDirection::Request,
    )
    .unwrap();
    for bytes in [
        valid.replace("state-seed/1", "state-flow/1").into_bytes(),
        valid
            .replace("\"request_id\":\"1\"", "\"request_id\":\"2\"")
            .into_bytes(),
        wrong_schema.into_bytes(),
        wrong_seed.into_bytes(),
        valid
            .replace("\"created\":", "\"op\":\"created\",\"created\":")
            .into_bytes(),
        request_frame,
        b"not-json\n".to_vec(),
    ] {
        assert!(matches!(
            with_answer(bytes),
            Err(ExchangeFailure::Malformed(_))
        ));
    }
}

#[test]
fn seed_transport_refuses_a_second_frame_or_any_trailing_byte() {
    let valid = response(&request());
    for tail in [valid.clone(), b"x".to_vec()] {
        let mut bytes = valid.clone();
        bytes.extend(tail);
        assert!(matches!(
            with_answer(bytes),
            Err(ExchangeFailure::Malformed(_))
        ));
    }
    // Put the LF exactly on the receive-buffer edge. The extra byte is left in
    // the socket, so the post-frame check, not the same-buffer guard, must catch it.
    let mut boundary = valid;
    boundary.pop();
    boundary.resize(8191, b' ');
    boundary.extend_from_slice(b"\nx");
    assert!(matches!(
        with_answer(boundary),
        Err(ExchangeFailure::Malformed(_))
    ));
}

#[test]
fn seed_transport_refuses_empty_or_partial_eof_and_created_followed_by_eof() {
    for answer in [vec![], b"{\"protocol\":".to_vec(), response(&request())] {
        let (ours, mut peer) = pair();
        peer.write_all(&answer).unwrap();
        peer.shutdown(std::net::Shutdown::Write).unwrap();
        let result = exchange(ours.as_fd(), &request(), Instant::now() + TEST_BOUND);
        assert!(matches!(result, Err(ExchangeFailure::Io(_))), "{result:?}");
        read_request(&mut peer);
        drop(ours);
    }
}

#[test]
fn seed_transport_refuses_oversized_answers_before_waiting_for_more() {
    let (ours, mut peer) = pair();
    let server = std::thread::spawn(move || {
        read_request(&mut peer);
        // The peer may observe admin's early close as soon as the bounded line
        // has no possible terminating LF. That close is the behavior under test.
        let _ = peer.write_all(&vec![b'x'; STATE_ANSWER_LINE_BOUND + 1]);
    });
    let result = exchange(ours.as_fd(), &request(), Instant::now() + TEST_BOUND);
    assert!(matches!(result, Err(ExchangeFailure::Malformed(_))));
    drop(ours);
    server.join().unwrap();
}

#[test]
fn seed_transport_accepts_a_complete_answer_at_the_exact_line_bound() {
    let (ours, mut peer) = pair();
    let mut answer = response(&request());
    answer.pop();
    answer.resize(STATE_ANSWER_LINE_BOUND - 1, b' ');
    answer.push(b'\n');
    let server = std::thread::spawn(move || {
        read_request(&mut peer);
        peer.write_all(&answer).unwrap();
        hold_until_close(&mut peer);
    });
    assert_eq!(
        exchange(ours.as_fd(), &request(), Instant::now() + TEST_BOUND).unwrap(),
        created(&request())
    );
    drop(ours);
    server.join().unwrap();
}

#[test]
fn seed_transport_expired_or_oversized_requests_send_nothing() {
    for expired in [true, false] {
        let (ours, mut peer) = pair();
        peer.set_nonblocking(true).unwrap();
        let mut request = request();
        if !expired {
            request.prompt_text = "x".repeat(STATE_REQUEST_LINE_BOUND);
        }
        let deadline = if expired {
            Instant::now()
        } else {
            Instant::now() + TEST_BOUND
        };
        let result = exchange(ours.as_fd(), &request, deadline);
        if expired {
            assert!(matches!(result, Err(ExchangeFailure::Deadline)));
        } else {
            assert!(matches!(result, Err(ExchangeFailure::Malformed(_))));
        }
        assert_eq!(
            peer.read(&mut [0u8; 1]).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
    }
}

#[test]
fn seed_transport_deadline_covers_both_blocked_send_and_answer() {
    let (ours, mut peer) = pair();
    small_send_buffer(&ours);
    let mut request = request();
    request.prompt_text = "x".repeat(256 * 1024);
    let answer = response(&request);
    let server = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(200));
        read_request(&mut peer);
        std::thread::sleep(Duration::from_millis(200));
        if peer.write_all(&answer).is_ok() {
            hold_until_close(&mut peer);
        }
    });
    let result = exchange(
        ours.as_fd(),
        &request,
        Instant::now() + Duration::from_millis(350),
    );
    assert!(
        matches!(result, Err(ExchangeFailure::Deadline)),
        "{result:?}"
    );
    drop(ours);
    server.join().unwrap();
}

#[test]
fn seed_transport_partial_answers_do_not_restart_the_deadline() {
    let (ours, mut peer) = pair();
    let server = std::thread::spawn(move || {
        read_request(&mut peer);
        for piece in response(&request()).chunks(64) {
            if peer.write_all(piece).is_err() {
                break;
            }
            std::thread::sleep(Duration::from_millis(30));
        }
    });
    let result = exchange(
        ours.as_fd(),
        &request(),
        Instant::now() + Duration::from_millis(75),
    );
    assert!(matches!(result, Err(ExchangeFailure::Deadline)));
    drop(ours);
    server.join().unwrap();
}

#[test]
fn seed_transport_closed_peer_never_delivers_sigpipe() {
    const CHILD: &str = "WEAVER_SEED_TRANSPORT_SIGPIPE_CHILD";
    if std::env::var_os(CHILD).is_some() {
        // SAFETY: this branch runs in its own test process. No other test shares
        // the process, and the default action is restored only for this fixture.
        unsafe { nix::libc::signal(nix::libc::SIGPIPE, nix::libc::SIG_DFL) };
        let (ours, peer) = pair();
        drop(peer);
        assert!(matches!(
            exchange(ours.as_fd(), &request(), Instant::now() + TEST_BOUND),
            Err(ExchangeFailure::Io(_))
        ));
        return;
    }
    let module = module_path!().split_once("::").unwrap().1;
    let name = format!("{module}::seed_transport_closed_peer_never_delivers_sigpipe");
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", &name, "--nocapture"])
        .env(CHILD, "1")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
}
