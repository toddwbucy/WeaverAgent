//! A partially written Column keeps its identity and delays Stop's answer.
use super::*;

pub(crate) fn prepare(sink: &OwnedFd) {
    nix::sys::socket::setsockopt(sink, nix::sys::socket::sockopt::SndBuf, &4096).unwrap();
}

pub(super) fn preamble(reader: &UnixStream, end: Instant) -> String {
    // Drain the opening rows and the completed Field. Small separate stream
    // writes can exhaust POLLOUT's threshold before the Column starts.
    // Byte reads stop at the Field's newline without consuming any Column.
    let mut bytes = Vec::new();
    let mut start = 0;
    loop {
        assert!(Instant::now() < end, "the elected Field was not recorded");
        let mut byte = [0];
        match recv(reader.as_raw_fd(), &mut byte, MsgFlags::MSG_DONTWAIT) {
            Ok(1) => {
                bytes.push(byte[0]);
                if byte[0] == b'\n' {
                    let row: serde_json::Value = serde_json::from_slice(&bytes[start..]).unwrap();
                    if row["kind"] == "model.field" {
                        return String::from_utf8(bytes).unwrap();
                    }
                    start = bytes.len();
                }
            }
            Err(nix::errno::Errno::EAGAIN) => {
                std::thread::sleep(Duration::from_millis(1));
            }
            other => panic!("record ended before the elected Field: {other:?}"),
        }
    }
}

pub(super) fn column(decode: &OwnedFd, root: &std::path::Path) {
    packet!(
        decode.as_raw_fd(),
        &TokenAnswer::Field {
            position: 5,
            ranked: (0..40)
                .map(|token| weaver_types::Candidate {
                    token,
                    probability: 0.025
                })
                .collect(),
            realized: 3,
        }
    );
    packet!(
        decode.as_raw_fd(),
        &TokenAnswer::Column {
            position: 5,
            layers: vec![vec![0.25; 8192]],
        }
    );
    let end = Instant::now() + Duration::from_secs(5);
    loop {
        let mut queued = 0;
        // SAFETY: TIOCOUTQ writes exactly one integer for this owned socket.
        assert_eq!(
            unsafe { nix::libc::ioctl(decode.as_raw_fd(), nix::libc::TIOCOUTQ, &mut queued) },
            0
        );
        if queued == 0 {
            break;
        }
        assert!(
            Instant::now() < end,
            "the worker did not receive the elected column"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    std::fs::write(root.join("diagnostic-column-held"), b"received\n").unwrap();
}

pub(super) fn held(
    root: &std::path::Path,
    reader: &UnixStream,
    peer: &OwnedFd,
    end: Instant,
) -> String {
    while !root.join("diagnostic-column-held").exists() {
        assert!(Instant::now() < end);
        std::thread::sleep(Duration::from_millis(1));
    }
    observed(peer, 980, AgentState::Active, end);
    let mut bytes = vec![0; 65_536];
    let prefix = loop {
        let count = recv(
            reader.as_raw_fd(),
            &mut bytes,
            MsgFlags::MSG_PEEK | MsgFlags::MSG_DONTWAIT,
        )
        .unwrap();
        let prefix = std::str::from_utf8(&bytes[..count]).unwrap();
        if prefix.contains("\"kind\":\"residual.column\"") {
            break prefix.to_owned();
        }
        assert!(
            Instant::now() < end,
            "the elected Column did not start writing"
        );
        std::thread::sleep(Duration::from_millis(1));
    };
    assert!(prefix.contains("\"kind\":\"residual.column\""));
    assert!(
        !prefix.ends_with('\n'),
        "the Column must remain partially written"
    );
    prefix
}

pub(super) fn cancelled(root: &std::path::Path, peer: &OwnedFd, end: Instant) {
    while !root.join("diagnostic-cancelled").exists() {
        assert!(
            Instant::now() < end,
            "record pressure must not block the exact Cancel"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    let probe = dial(&root.join("admin.sock"));
    observed(&probe, 981, AgentState::Active, end);
    let mut bytes = [0; 4096];
    assert_eq!(
        recv(peer.as_raw_fd(), &mut bytes, MsgFlags::MSG_DONTWAIT),
        Err(nix::errno::Errno::EAGAIN),
        "Stop cannot overtake the original Column or turn close"
    );
}

pub(super) fn rows(rows: &[serde_json::Value]) {
    for kind in ["model.field", "residual.column"] {
        assert_eq!(rows.iter().filter(|row| row["kind"] == kind).count(), 1);
    }
    let column = rows
        .iter()
        .find(|row| row["kind"] == "residual.column")
        .unwrap();
    assert_eq!(column["turn"], "t-1");
    assert_eq!(column["payload"]["position"], 5);
    assert_eq!(column["payload"]["width"], 8192);
    assert_eq!(
        column["payload"]["values"][0].as_array().unwrap(),
        &vec![serde_json::json!(0.25); 8192]
    );
    let column_at = rows
        .iter()
        .position(|row| row["kind"] == "residual.column")
        .unwrap();
    let close_at = rows
        .iter()
        .position(|row| row["kind"] == "turn.closed")
        .unwrap();
    assert!(column_at < close_at);
    for (ordinal, row) in rows.iter().enumerate() {
        assert_eq!(
            row["sequence"],
            ordinal.to_string(),
            "one original rendering per event"
        );
    }
}
