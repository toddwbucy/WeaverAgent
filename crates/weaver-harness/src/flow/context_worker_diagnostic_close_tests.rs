//! An admitted close keeps its outcome, and interruption admits no later work.
use super::*;

pub(super) fn events(mode: &str, turn: &str, pressure: bool) -> Vec<serde_json::Value> {
    let mut events = vec![
        serde_json::json!({"envelope":{"kind":"model.request","run":"r-1","turn":turn,"sequence":"4"},"pairs":{"rendered":if mode.contains("-send-") {send_pressure::rendered()} else {"hi".into()},"template":"tmpl","sampling":{"seed":37}}}),
        serde_json::json!({"envelope":{"kind":"model.measurement","run":"r-1","turn":turn,"sequence":"6"},"pairs":{"input_tokens":[1,2],"output_tokens":if pressure {vec![3,4,5]} else {vec![3]},"model":"art","weights_hash":"h"}}),
    ];
    if mode.ends_with("-ticket-leave") {
        events[1]["pairs"]["model"] = serde_json::json!(foreign_artifact());
    }
    if mode.contains("-later-") || mode.contains("-close-") {
        if !mode.ends_with("-later-turn") {
            events.push(serde_json::json!({"envelope":{"kind":"elision","run":"r-1","sequence":"7"},"pairs":{"from":4,"to":5,"resident_before":6,"resident_after":5}}));
        }
        events.extend([
            serde_json::json!({"envelope":{"kind":"model.request","run":"r-1","turn":"t-2","sequence":"8"},"pairs":{"rendered":"hi","template":"tmpl","sampling":{"seed":37}}}),
            serde_json::json!({"envelope":{"kind":"model.measurement","run":"r-1","turn":"t-2","sequence":"10"},"pairs":{"input_tokens":[1,2],"output_tokens":[4],"model":"art","weights_hash":"h"}}),
        ]);
    }
    events
}

pub(super) fn finish_source(decode: &OwnedFd) {
    let edit: TokenDirective = take!(decode.as_raw_fd());
    assert_eq!(edit, TokenDirective::Elide { from: 4, to: 5 });
    packet!(
        decode.as_raw_fd(),
        &TokenAnswer::Elided {
            resident_before: 6,
            resident_after: 5
        }
    );
    let request: TokenDirective = take!(decode.as_raw_fd());
    assert_eq!(
        request,
        TokenDirective::ReFeed {
            turn: weaver_types::TurnKey("t-2".into()),
            rendered: "hi".into(),
            path: vec![4],
        }
    );
    let mut answer = serde_json::to_value(completed()).unwrap();
    answer["body"]["measurement"]["output_tokens"] = serde_json::json!([4]);
    answer["body"]["resident"] = serde_json::json!(7);
    let answer: TokenAnswer = serde_json::from_value(answer).unwrap();
    packet!(decode.as_raw_fd(), &answer);
}

pub(super) fn duplicate(root: &std::path::Path, decode: &OwnedFd) -> ! {
    let end = Instant::now() + Duration::from_secs(5);
    while !root.join("diagnostic-close-duplicate").exists() {
        assert!(Instant::now() < end);
        std::thread::sleep(Duration::from_millis(1));
    }
    packet!(decode.as_raw_fd(), &completed());
    std::thread::sleep(Duration::from_secs(90));
    panic!("the failed close did not end its owned SPU");
}

pub(super) fn held(reader: &UnixStream, end: Instant) -> String {
    // Drain through the final complete generation row, byte-exactly. The
    // next event is turn.closed, whose long identity holds a partial write.
    hold_after(reader, end, "message.assistant", "\"turn\":\"turn-turn-")
}

pub(super) fn foreign_artifact() -> String {
    "different-artifact-".repeat(2048)
}

pub(super) fn ticket(reader: &UnixStream, end: Instant) -> String {
    hold_after(reader, end, "recall", "\"kind\":\"replay.closed\"")
}

pub(crate) fn hold_after(reader: &UnixStream, end: Instant, kind: &str, prefix: &str) -> String {
    let mut bytes = Vec::new();
    let mut start = 0;
    loop {
        assert!(Instant::now() < end, "record must reach {kind}");
        let mut byte = [0];
        match recv(reader.as_raw_fd(), &mut byte, MsgFlags::MSG_DONTWAIT) {
            Ok(1) => {
                bytes.push(byte[0]);
                if byte[0] == b'\n' {
                    let row: serde_json::Value = serde_json::from_slice(&bytes[start..]).unwrap();
                    if row["kind"] == kind {
                        break;
                    }
                    assert!(
                        row["kind"] != "replay.closed",
                        "replay ended before {kind}: {row}"
                    );
                    start = bytes.len();
                }
            }
            Err(nix::errno::Errno::EAGAIN) => std::thread::sleep(Duration::from_millis(1)),
            other => panic!("record ended before the complete generation: {other:?}"),
        }
    }
    let mut held = vec![0; 65_536];
    loop {
        assert!(Instant::now() < end, "the close did not start writing");
        match recv(
            reader.as_raw_fd(),
            &mut held,
            MsgFlags::MSG_PEEK | MsgFlags::MSG_DONTWAIT,
        ) {
            Ok(n) if n > 0 => {
                let actual = std::str::from_utf8(&held[..n]).unwrap();
                assert!(actual.contains(prefix));
                assert!(
                    !actual.ends_with('\n'),
                    "the close must remain partly written"
                );
                break;
            }
            Err(nix::errno::Errno::EAGAIN) => std::thread::sleep(Duration::from_millis(1)),
            other => panic!("record ended before the close: {other:?}"),
        }
    }
    String::from_utf8(bytes).unwrap()
}

pub(super) fn pending(root: &std::path::Path, peer: &OwnedFd, end: Instant, leaving: bool) {
    if leaving {
        send(peer, 987, LifecycleDirective::Observe);
        let answer = ObservationEnvelope::decode(&receive(peer, end)).unwrap();
        assert_eq!(answer.exchange.ordinal, 987);
        assert!(matches!(
            answer.payload,
            ObservationContent::Answer(ObservationAnswer::InTransition { .. })
        ));
    } else {
        observed(peer, 987, AgentState::Active, end);
    }
    let mut bytes = [0; 4096];
    assert_eq!(
        recv(peer.as_raw_fd(), &mut bytes, MsgFlags::MSG_DONTWAIT),
        Err(nix::errno::Errno::EAGAIN),
        "the accepted close must record before Stop or Left"
    );
    assert!(
        !root.join("diagnostic-cancelled").exists(),
        "no active ReFeed remains to cancel"
    );
}
