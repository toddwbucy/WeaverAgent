//! One test across the two languages (the brief of 2026-11-25, item 10): the relay, and
//! `loop-runner/runner.py` under `/usr/bin/python3 -I -B -S` with the pair at descriptor 3
//! as deployed, relaying to the stand-in seat. Where the interpreter is absent it fails,
//! and it never skips.

use super::*;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::os::fd::BorrowedFd;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};

const WIRES: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/loop-wires-cross.py"
);
const RUNNER: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../loop-runner/runner.py");

fn turned(seat: &Stand) -> String {
    match seat.calls.last() {
        Some(Call::Turn(messages)) => messages[0].1[0].clone(),
        other => panic!("no turn: {other:?}"),
    }
}

#[test]
fn the_relay_and_the_python_runner_cross_passes_a_refusal_and_series_both_ways() {
    let (near, far) = pair();
    let raw = far.end.as_raw_fd();
    let mut command = Command::new("/usr/bin/python3");
    command
        .args(["-I", "-B", "-S", RUNNER, WIRES])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    // SAFETY: place_child_ends runs between fork and exec, on an end this process made.
    unsafe {
        command.pre_exec(move || {
            crate::place_child_ends(&[BorrowedFd::borrow_raw(raw)]).map_err(std::io::Error::from)
        });
    }
    let mut child = command
        .spawn()
        .expect("the interpreter at /usr/bin/python3");
    drop(far);
    let digest = format!("{:x}", Sha256::digest(std::fs::read(WIRES).unwrap()));
    let lines = Lines::default();
    let mut relay = LoopRelay::make(near, &digest, lines.clone()).expect("the runner's hello");

    // Pass 1: an ask and a turn.
    let mut seat = stand();
    let first = relay.pass(&mut seat, "one");
    assert!(first.is_ok(), "{first:?}");
    assert_eq!(turned(&seat), "one, seated True");

    // Pass 2: the wires refuse; the relay answers ChannelLost and stays Idle.
    let mut seat = stand();
    assert!(lost(&relay.pass(&mut seat, "refuse")));
    assert!(seat.calls.is_empty(), "no call: {:?}", seat.calls);
    assert!(
        seat.reads.borrow().is_empty(),
        "no read: {:?}",
        seat.reads.borrow()
    );

    // Pass 3: a recall's return past 64 KiB crosses to the loop as a series, whole, and
    // the turn ask past 64 KiB crosses back as a series, whole. Neither fits one message
    // (the envelope is 64 KiB), so each crossed as a series or not at all.
    let mut seat = stand();
    let recalled = "r".repeat(200_000);
    seat.recall = Some(vec![Recalled {
        kind: "message.user".to_string(),
        run: "r-1".to_string(),
        turn: None,
        sequence: "1".to_string(),
        pairs: vec![("content".to_string(), recalled.clone())],
    }]);
    let third = relay.pass(&mut seat, "recall");
    assert!(third.is_ok(), "{third:?} {}", lines.text());
    assert_eq!(seat.calls[0], Call::Recall(Some(4)));
    let text = turned(&seat);
    assert!(
        text.starts_with("received 200000 "),
        "the loop read the whole return: {}",
        &text[..40]
    );
    assert_eq!(text.len(), "received 200000 ".len() + 100_000);
    assert!(text.len() > MAX_ENVELOPE_BYTES);

    // Pass 4: the relay still serves after all of it.
    let mut seat = stand();
    assert!(relay.pass(&mut seat, "four").is_ok());
    assert_eq!(lines.text(), "", "no break");

    drop(relay);
    // Standard error is read to its end before the wait: a runner that fills the pipe would
    // otherwise never exit.
    let mut said = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut said)
        .unwrap();
    let status = child.wait().unwrap();
    assert!(status.success(), "the runner's exit: {status:?} {said}");
    assert_eq!(
        said, "{\"loop\": \"end\", \"passes\": 4, \"turns\": 3, \"refused\": 1}\n",
        "the runner refused no pass of its own and broke nothing: the wires refused pass 2"
    );
}

/// **A wire file changed between admin's read and the runner's gives two
/// digests, and the load is refused** (the brief of 2026-11-26, section 2,
/// child 4): admin carried the digest of the bytes it read, the file was laid
/// again before the runner read it, and the relay refuses the runner's
/// `hello` naming the digest. Admin's half is
/// `loop_folder::tests::the_election_names_the_file_and_the_digest_of_its_bytes`.
/// Perturbation: let the relay take any digest, and the hello is admitted.
#[test]
fn a_wire_file_changed_between_the_two_reads_refuses_the_hello() {
    let scratch = crate::scratch::dir(format!(
        "weaver-two-digests-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let wires = scratch.join("wires.py");
    let laid = std::fs::read(WIRES).unwrap();
    std::fs::write(&wires, &laid).unwrap();
    let admins = format!("{:x}", Sha256::digest(&laid));
    // Laid again, still a grammar the runner admits, before the runner reads.
    let mut again = laid.clone();
    again.extend_from_slice(b"\n# laid again\n");
    std::fs::write(&wires, &again).unwrap();

    let (near, far) = pair();
    let raw = far.end.as_raw_fd();
    let mut command = Command::new("/usr/bin/python3");
    command
        .args(["-I", "-B", "-S", RUNNER])
        .arg(&wires)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // SAFETY: place_child_ends runs between fork and exec, on an end this process made.
    unsafe {
        command.pre_exec(move || {
            crate::place_child_ends(&[BorrowedFd::borrow_raw(raw)]).map_err(std::io::Error::from)
        });
    }
    let mut child = command
        .spawn()
        .expect("the interpreter at /usr/bin/python3");
    drop(far);
    let lines = Lines::default();
    let refused = LoopRelay::make(near, &admins, lines.clone())
        .err()
        .expect("the hello of the file laid again is refused");
    assert_eq!(
        refused.reason,
        "a hello whose digest is not the wire file's"
    );
    let _ = child.kill();
    let _ = child.wait();
}
