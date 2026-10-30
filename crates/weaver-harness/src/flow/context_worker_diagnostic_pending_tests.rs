//! Decode remains owned while preload or an earlier record is pending.
use super::*;

pub(super) fn report() -> weaver_types::FaultReport {
    weaver_types::FaultReport {
        case: weaver_types::FaultCase::ResidencyDegraded,
        account: serde_json::value::RawValue::from_string(format!(
            "{{\"pending\": true, \"detail\":\"{}\"}}",
            "idle decode report ".repeat(2048)
        ))
        .unwrap(),
    }
}

pub(super) fn spu(root: &std::path::Path, mode: &str, decode: &OwnedFd) {
    let end = Instant::now() + Duration::from_secs(5);
    while !root.join("diagnostic-pending-action").exists() {
        assert!(Instant::now() < end);
        std::thread::sleep(Duration::from_millis(1));
    }
    if mode.contains("death") {
        nix::sys::socket::shutdown(decode.as_raw_fd(), nix::sys::socket::Shutdown::Both).unwrap();
        std::thread::sleep(Duration::from_secs(90));
        panic!("the worker did not reap its dead decode peer");
    }
    packet!(decode.as_raw_fd(), &TokenAnswer::Fault(report()));
    std::fs::write(root.join("diagnostic-pending-sent"), b"report sent\n").unwrap();
}

pub(super) fn held_report(
    root: &std::path::Path,
    peer: &OwnedFd,
    reader: &UnixStream,
    end: Instant,
) -> String {
    while !root.join("diagnostic-pending-sent").exists() {
        assert!(Instant::now() < end);
        std::thread::sleep(Duration::from_millis(1));
    }
    observed(peer, 993, AgentState::Active, end);
    let prefix = closing::hold_after(reader, end, "turn.closed", "\"kind\":\"fault\"");
    closing::pending(root, peer, end, false);
    prefix
}

pub(super) fn answer(root: &std::path::Path) {
    let end = Instant::now() + Duration::from_secs(5);
    while !root.join("diagnostic-preload-answer").exists() {
        assert!(Instant::now() < end);
        std::thread::sleep(Duration::from_millis(1));
    }
}

pub(super) fn preload(
    root: &std::path::Path,
    mode: &str,
    peer: &OwnedFd,
    reader: &mut UnixStream,
    prefix: &mut String,
    end: Instant,
) {
    observed(peer, 991, AgentState::Idle, end);
    std::fs::write(
        root.join("diagnostic-pending-action"),
        b"preload is pending\n",
    )
    .unwrap();
    reader.set_nonblocking(false).unwrap();
    reader
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let mut line = Vec::new();
    let fault = loop {
        let mut byte = [0];
        reader
            .read_exact(&mut byte)
            .expect("idle decode must wake the pending preload");
        line.push(byte[0]);
        if byte[0] == b'\n' {
            prefix.push_str(std::str::from_utf8(&line).unwrap());
            let row: serde_json::Value = serde_json::from_slice(&line).unwrap();
            assert!(row["kind"] != "turn.started" && row["kind"] != "replay.closed");
            if row["kind"] == "fault" {
                break row;
            }
            line.clear();
        }
    };
    assert!(fault.get("turn").is_none());
    let death = mode.contains("-preload-death-");
    if death {
        assert_eq!(fault["payload"]["case"], "organ_death_observed");
        assert_eq!(fault["payload"]["account"]["organ"], "spu");
    } else {
        assert_eq!(fault["payload"], serde_json::to_value(report()).unwrap());
        assert!(prefix.contains(report().account.get()));
    }
    if death || mode.ends_with("-resume") {
        // A late valid answer after death must not restart the replay.
        std::fs::write(
            root.join("diagnostic-preload-answer"),
            b"answer the original ask\n",
        )
        .unwrap();
    }
    if death {
        let pid = std::fs::read_to_string(root.join("diagnostic-interrupt-spu-pid")).unwrap();
        let proc = format!("/proc/{}", pid.trim());
        let began = Instant::now();
        while std::path::Path::new(&proc).exists() {
            assert!(began.elapsed() < Duration::from_millis(weaver_types::LOWER_BOUND_MS + 2_000));
            observed(
                peer,
                992,
                AgentState::Idle,
                end.min(Instant::now() + Duration::from_secs(1)),
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

pub(super) fn rows(rows: &[serde_json::Value], mode: &str) {
    let faults: Vec<_> = rows.iter().filter(|row| row["kind"] == "fault").collect();
    assert_eq!(faults.len(), 1);
    if !mode.contains("-preload-death-") {
        assert_eq!(
            faults[0]["payload"],
            serde_json::to_value(report()).unwrap()
        );
    }
    if mode.ends_with("-close-report") {
        let close = rows
            .iter()
            .position(|row| row["kind"] == "turn.closed")
            .unwrap();
        let report = rows.iter().position(|row| row["kind"] == "fault").unwrap();
        assert!(
            close < report,
            "the held close finishes before the later report"
        );
        assert!(faults[0].get("turn").is_none());
    }
}

pub(super) fn death(
    root: &std::path::Path,
    child: &mut std::process::Child,
    peer: &OwnedFd,
    reader: &mut UnixStream,
    prefix: &mut String,
    end: Instant,
) {
    observed(peer, 994, AgentState::Active, end);
    std::fs::write(root.join("diagnostic-pending-action"), b"turn open\n").unwrap();
    std::thread::sleep(Duration::from_millis(50));
    reader.set_nonblocking(false).unwrap();
    reader
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    reader.read_to_string(prefix).unwrap();
    faults::finish(root, child, peer, end);
    let rows: Vec<serde_json::Value> = prefix
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let faults: Vec<_> = rows.iter().filter(|row| row["kind"] == "fault").collect();
    assert_eq!(faults.len(), 1, "observed decode death must be recorded");
    assert_eq!(faults[0]["payload"]["case"], "organ_death_observed");
    assert_eq!(faults[0]["payload"]["account"]["organ"], "spu");
    let closes: Vec<_> = rows
        .iter()
        .filter(|row| row["kind"] == "turn.closed")
        .collect();
    assert_eq!(closes.len(), 1);
    assert_eq!(closes[0]["payload"]["reason"], "fault");
    assert!(!rows.iter().any(|row| row["kind"] == "replay.closed"));
}
