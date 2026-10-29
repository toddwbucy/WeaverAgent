use super::*;
use std::os::fd::AsRawFd;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

// The parent can kill a writer that blocks before its answer timer starts.
// This is a transport regression instrument, with no organ or privileged scope.
fn child_output(mode: &str) -> (std::process::ExitStatus, bool, Vec<u8>, Vec<u8>) {
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "state::tests::diagnostic_writes::diagnostic_write_child",
            "--nocapture",
        ])
        .env("WEAVER_DIAGNOSTIC_WRITE_TEST", mode)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    fn drain(mut pipe: impl Read + Send + 'static) -> std::thread::JoinHandle<Vec<u8>> {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            pipe.read_to_end(&mut bytes).unwrap();
            bytes
        })
    }
    let stdout = drain(child.stdout.take().unwrap());
    let stderr = drain(child.stderr.take().unwrap());
    let deadline = Instant::now() + Duration::from_secs(5);
    let (status, expired) = crate::test_child::wait_until(&mut child, deadline).unwrap();
    (
        status,
        expired,
        stdout.join().unwrap(),
        stderr.join().unwrap(),
    )
}

fn bounded_child(mode: &str) {
    let (status, expired, stdout, stderr) = child_output(mode);
    assert!(!expired, "Diagnostic {mode} blocked before its answer wait");
    assert!(
        status.success(),
        "{mode}: {}{}",
        String::from_utf8_lossy(&stdout),
        String::from_utf8_lossy(&stderr)
    );
}

#[test]
fn diagnostic_write_watchdog_retains_a_large_assertion_without_reporting_a_hang() {
    let (status, expired, stdout, stderr) = child_output("large-failure");
    assert!(
        !expired,
        "a failed assertion must not become a blocked-write verdict"
    );
    assert!(!status.success());
    assert!(stdout.len() >= 1024 * 1024);
    assert!(stderr.len() >= 1024 * 1024);
    assert!(String::from_utf8_lossy(&stderr).contains("diagnostic output drain sentinel"));
}

#[test]
fn diagnostic_opener_pressure_closes_the_incomplete_stream_without_waiting() {
    bounded_child("opener");
}

#[test]
fn diagnostic_ask_pressure_retires_the_seam_before_its_answer_timer() {
    bounded_child("ask");
}

#[test]
fn diagnostic_write_child() {
    let Ok(mode) = std::env::var("WEAVER_DIAGNOSTIC_WRITE_TEST") else {
        return;
    };
    if mode == "large-failure" {
        std::io::stdout()
            .write_all(&vec![b'o'; 1024 * 1024])
            .unwrap();
        std::io::stderr()
            .write_all(&vec![b'e'; 1024 * 1024])
            .unwrap();
        panic!("diagnostic output drain sentinel");
    }
    let (ours, mut peer) = UnixStream::pair().unwrap();
    nix::sys::socket::setsockopt(&ours, nix::sys::socket::sockopt::SndBuf, &4096).unwrap();
    // No fixture sets O_NONBLOCK: the actual constructor must supply it.
    match mode.as_str() {
        "opener" => {
            let frame = weaver_trace::opener(
                &"s".repeat(1024 * 1024),
                &weaver_trace::Election {
                    all_kinds: true,
                    keys: Vec::new(),
                },
            );
            assert!(StateSeam::open_diagnostic(ours, frame.as_bytes()).is_none());
            let mut received = Vec::new();
            peer.read_to_end(&mut received).unwrap();
            assert!(!received.is_empty() && received.len() < frame.len());
            assert!(frame.as_bytes().starts_with(&received));
            assert!(!received.contains(&b'\n'), "no complete opener crossed");
        }
        "ask" => {
            let mut seam = StateSeam::new(ours);
            let mut filled = 0;
            loop {
                match nix::sys::socket::send(
                    seam.channel.as_raw_fd(),
                    &[b'x'; 4096],
                    nix::sys::socket::MsgFlags::MSG_DONTWAIT
                        | nix::sys::socket::MsgFlags::MSG_NOSIGNAL,
                ) {
                    Ok(n) => {
                        assert!(n > 0);
                        filled += n;
                    }
                    Err(nix::errno::Errno::EAGAIN) => break,
                    Err(error) => panic!("fixture fill: {error}"),
                }
            }
            assert!(seam.ask_identity_within(1).is_none());
            assert!(seam.dead());
            assert!(
                seam.ask_replay(1).is_none(),
                "no retry after the unsent ask"
            );
            drop(seam);
            let mut received = Vec::new();
            peer.read_to_end(&mut received).unwrap();
            assert_eq!(received, vec![b'x'; filled]);
        }
        _ => panic!("unknown fixture mode"),
    }
}

#[test]
fn diagnostic_opener_and_identity_ask_preserve_the_same_stream_and_exact_order() {
    let (ours, mut peer) = UnixStream::pair().unwrap();
    peer.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    let frame = weaver_trace::opener(
        "original-session",
        &weaver_trace::Election {
            all_kinds: true,
            keys: Vec::new(),
        },
    );
    let mut seam = StateSeam::open_diagnostic(ours, frame.as_bytes()).unwrap();
    peer.write_all(b"{\"answer\":{\"identity\":{\"messages\":[]}}}\n")
        .unwrap();
    assert!(seam.ask_identity_within(200).unwrap().is_empty());
    assert!(!seam.dead());
    let expected = frame + "{\"ask\":{\"identity\":{}}}\n";
    let mut received = vec![0; expected.len()];
    peer.read_exact(&mut received).unwrap();
    assert_eq!(received, expected.as_bytes());
}
