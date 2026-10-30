//! After an interrupted pass, idle decode still belongs to the entered owner.
use super::*;

fn report() -> weaver_types::FaultReport {
    weaver_types::FaultReport {
        case: weaver_types::FaultCase::ResidencyDegraded,
        account: serde_json::value::RawValue::from_string(
            "{\"z\": 7, \"detail\":\"idle residency report\"}".into(),
        )
        .unwrap(),
    }
}

pub(super) fn spu(root: &std::path::Path, mode: &str, decode: &OwnedFd) {
    let end = Instant::now() + Duration::from_secs(5);
    while !root.join("diagnostic-idle-action").exists() {
        assert!(Instant::now() < end);
        std::thread::sleep(Duration::from_millis(1));
    }
    if mode.ends_with("-death") {
        nix::sys::socket::shutdown(decode.as_raw_fd(), nix::sys::socket::Shutdown::Both).unwrap();
    } else if mode.ends_with("-idle-invalid") {
        // Its accepted Cancel already settled. A second AtRest is not a report.
        packet!(decode.as_raw_fd(), &TokenAnswer::AtRest);
    } else {
        packet!(decode.as_raw_fd(), &TokenAnswer::Fault(report()));
    }
    if !mode.ends_with("-report") {
        // A closed channel need not mean an exited process. Only the worker
        // can end this child, and management must work while it waits to reap.
        std::thread::sleep(Duration::from_secs(90));
        panic!("the worker did not end its owned child")
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn control(
    root: &std::path::Path,
    mode: &str,
    child: &mut std::process::Child,
    peer: &OwnedFd,
    reader: &mut UnixStream,
    prefix: &mut String,
    end: Instant,
) -> bool {
    let record_failure = mode.ends_with("-idle-write-failed");
    if record_failure {
        reader.shutdown(std::net::Shutdown::Both).unwrap();
    }
    let began = Instant::now();
    std::fs::write(root.join("diagnostic-idle-action"), b"after Stop\n").unwrap();
    if record_failure || mode.ends_with("-idle-invalid") {
        faults::finish(root, child, peer, end);
        return true;
    }
    reader.set_nonblocking(false).unwrap();
    reader
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let fault = loop {
        let row = next_record(reader, prefix);
        if row["kind"] == "fault" {
            break row;
        }
    };
    assert!(
        fault.get("turn").is_none(),
        "no new turn for an idle report"
    );
    if mode.ends_with("-report") {
        assert_eq!(fault["payload"], serde_json::to_value(report()).unwrap());
        assert!(
            prefix.contains(report().account.get()),
            "retain opaque account bytes"
        );
    } else {
        assert_eq!(fault["payload"]["case"], "organ_death_observed");
        assert_eq!(fault["payload"]["account"]["organ"], "spu");
        let pid = std::fs::read_to_string(root.join("diagnostic-interrupt-spu-pid")).unwrap();
        let proc = format!("/proc/{}", pid.trim());
        let reap_end = began + Duration::from_millis(weaver_types::LOWER_BOUND_MS + 2_000);
        while std::path::Path::new(&proc).exists() {
            assert!(
                Instant::now() < reap_end,
                "closed but living SPU must be reaped"
            );
            observed(
                peer,
                980,
                AgentState::Idle,
                end.min(Instant::now() + Duration::from_secs(1)),
            );
            assert!(child.try_wait().unwrap().is_none());
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(began.elapsed() >= Duration::from_millis(weaver_types::LOWER_BOUND_MS));
    }
    observed(peer, 981, AgentState::Idle, end);
    send(
        peer,
        982,
        LifecycleDirective::Stop {
            cause: Cause { uid: 42 },
        },
    );
    let stopped: OrganEnvelope = serde_json::from_slice(&receive(peer, end)).unwrap();
    assert_eq!(stopped.exchange.ordinal, 982);
    assert_eq!(stopped.payload, Payload::Answer(LifecycleAnswer::AtRest));
    assert!(child.try_wait().unwrap().is_none());
    false
}

fn next_record(reader: &mut UnixStream, prefix: &mut String) -> serde_json::Value {
    let mut line = Vec::new();
    loop {
        let mut byte = [0];
        reader
            .read_exact(&mut byte)
            .expect("idle decode must reach the record");
        line.push(byte[0]);
        if byte[0] == b'\n' {
            break;
        }
    }
    prefix.push_str(std::str::from_utf8(&line).unwrap());
    serde_json::from_slice(&line).unwrap()
}

pub(super) fn await_certificate(reader: &mut UnixStream, prefix: &mut String) {
    reader.set_nonblocking(false).unwrap();
    reader
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    loop {
        let row = next_record(reader, prefix);
        if row["kind"] == "replay.closed" {
            assert_eq!(row["payload"]["outcome"]["kind"], "certified");
            break;
        }
    }
}
