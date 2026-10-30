//! A started ReFeed series finishes whole before its one Cancel can cross.
use super::*;

pub(super) fn rendered() -> String {
    // Below the state answer's one-MiB bound, above the decode socket's
    // ordinary send capacity, and large enough to require at least twelve segments.
    "segment ".repeat(96 * 1024)
}

pub(super) fn receive(root: &std::path::Path, mode: &str, decode: &OwnedFd) -> TokenDirective {
    let preamble: weaver_types::SegmentPreamble = take!(decode.as_raw_fd());
    let expected = TokenDirective::ReFeed {
        turn: weaver_types::TurnKey("t-1".into()),
        rendered: rendered(),
        path: vec![3],
    };
    let original = serde_json::to_vec(&expected).unwrap();
    assert_eq!(preamble.bytes, original.len() as u64);
    assert_eq!(
        preamble.segments,
        original.len().div_ceil(weaver_types::MAX_ENVELOPE_BYTES) as u64
    );
    assert!(preamble.segments > 10);
    std::fs::write(root.join("diagnostic-refeed"), b"preamble only\n").unwrap();
    if mode.ends_with("-send-silent") {
        // Keep the actual child and unread series alive through the complete
        // cancellation interval. Cleanup, not this fixture, must end it.
        std::thread::sleep(Duration::from_secs(90));
        panic!("the original cancellation interval did not end the partial send");
    }
    let end = Instant::now()
        + Duration::from_secs(if mode.ends_with("-delayed-silent") {
            30
        } else {
            5
        });
    while !root.join("diagnostic-send-drain").exists() {
        assert!(Instant::now() < end);
        std::thread::sleep(Duration::from_millis(1));
    }
    let mut whole = Vec::new();
    let mut packet = vec![0; weaver_types::MAX_ENVELOPE_BYTES];
    for slice in original.chunks(weaver_types::MAX_ENVELOPE_BYTES) {
        let n = recv(decode.as_raw_fd(), &mut packet, MsgFlags::empty()).unwrap();
        assert_eq!(n, slice.len(), "no Cancel or restart inside the series");
        assert_eq!(&packet[..n], slice, "retain each original segment");
        whole.extend_from_slice(&packet[..n]);
    }
    assert_eq!(whole, original);
    let actual = serde_json::from_slice(&whole).unwrap();
    assert_eq!(actual, expected);
    std::fs::write(
        root.join("diagnostic-send-complete"),
        b"exact whole ReFeed\n",
    )
    .unwrap();
    actual
}

pub(super) fn stopped(root: &std::path::Path, peer: &OwnedFd, end: Instant, mode: &str) {
    // On the original ordered peer, this Observe follows Stop. It must be
    // answered while the ReFeed's partially sent bytes are still retained.
    observed(peer, 984, AgentState::Active, end);
    let mut bytes = [0; 4096];
    assert_eq!(
        recv(peer.as_raw_fd(), &mut bytes, MsgFlags::MSG_DONTWAIT),
        Err(nix::errno::Errno::EAGAIN),
        "a partial send cannot grant Stop"
    );
    assert!(!root.join("diagnostic-cancelled").exists());
    assert!(!root.join("diagnostic-send-complete").exists());
    if mode.ends_with("-delayed-silent") {
        let held = Instant::now();
        while held.elapsed() < Duration::from_secs(20) {
            observed(peer, 986, AgentState::Active, end);
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    if !mode.ends_with("-send-silent") {
        std::fs::write(root.join("diagnostic-send-drain"), b"Stop is retained\n").unwrap();
    }
}

pub(super) fn turn_before_send() -> String {
    // A legitimate identity below the coordination envelope bound, but larger
    // than the elected small record socket's send capacity.
    "turn-".repeat(6 * 1024)
}

pub(super) fn hold_turn_start(reader: &UnixStream, end: Instant) -> String {
    let mut bytes = Vec::new();
    let mut start = 0;
    loop {
        assert!(
            Instant::now() < end,
            "replay identity must reach the record"
        );
        let mut byte = [0];
        match recv(reader.as_raw_fd(), &mut byte, MsgFlags::MSG_DONTWAIT) {
            Ok(1) => {
                bytes.push(byte[0]);
                if byte[0] == b'\n' {
                    let row: serde_json::Value = serde_json::from_slice(&bytes[start..]).unwrap();
                    if row["kind"] == "replay.identity" {
                        break;
                    }
                    start = bytes.len();
                }
            }
            Err(nix::errno::Errno::EAGAIN) => std::thread::sleep(Duration::from_millis(1)),
            other => panic!("record ended before replay identity: {other:?}"),
        }
    }
    let mut held = vec![0; 65_536];
    loop {
        assert!(Instant::now() < end, "turn.started did not start writing");
        match recv(
            reader.as_raw_fd(),
            &mut held,
            MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_PEEK,
        ) {
            Ok(n) if n > 0 => {
                let prefix = std::str::from_utf8(&held[..n]).unwrap();
                // The long turn field precedes kind in the envelope. Keep
                // that original identity partially written, then verify the
                // complete turn.started row after the reader is released.
                assert!(prefix.contains("\"turn\":\"turn-turn-"));
                assert!(
                    !prefix.ends_with('\n'),
                    "turn.started must remain partly written"
                );
                break;
            }
            Err(nix::errno::Errno::EAGAIN) => std::thread::sleep(Duration::from_millis(1)),
            other => panic!("record ended before turn.started: {other:?}"),
        }
    }
    String::from_utf8(bytes).unwrap()
}

pub(super) fn stopped_before_send(root: &std::path::Path, peer: &OwnedFd, end: Instant) {
    observed(peer, 985, AgentState::Active, end);
    let mut bytes = [0; 4096];
    assert_eq!(
        recv(peer.as_raw_fd(), &mut bytes, MsgFlags::MSG_DONTWAIT),
        Err(nix::errno::Errno::EAGAIN)
    );
    assert!(!root.join("diagnostic-refeed").exists());
    assert!(!root.join("diagnostic-cancelled").exists());
}
