//! Invalid terminals cannot satisfy an accepted Stop or certify a replay.
use super::*;
use std::os::fd::AsFd;

pub(super) struct UnwindMember(pub(super) UnixStream);

impl Drop for UnwindMember {
    fn drop(&mut self) {
        if std::thread::panicking() {
            // A failed assertion must wake the scoped member reader before
            // its join. The full-duration success cases keep their timeout.
            let _ = self.0.shutdown(std::net::Shutdown::Both);
        }
    }
}

pub(super) fn spu(mode: &str, turn: &weaver_types::TurnKey, decode: &OwnedFd) -> ! {
    let mut received = turn.clone();
    let (mut consumed, mut resident, mut capacity) = (0, 5, 64);
    if mode.ends_with("-foreign-turn") {
        received.0 = "foreign".into();
    } else if mode.ends_with("-whole-path") {
        consumed = 1;
    } else if mode.ends_with("-resident-small") {
        resident = 0;
    } else if mode.ends_with("-capacity-small") {
        capacity = 4;
    } else if mode.ends_with("-count-overflow") {
        consumed = u64::MAX;
        resident = u64::MAX;
        capacity = u64::MAX;
    }
    let terminal = TokenAnswer::RefeedInterrupted {
        turn: received,
        consumed,
        resident,
        capacity,
    };
    if mode.ends_with("-silent") {
        // The owner must enforce its own original interval without any reply.
    } else if mode.ends_with("-early-rest") {
        packet!(decode.as_raw_fd(), &TokenAnswer::AtRest);
    } else if mode.ends_with("-cancel-refused") {
        packet!(decode.as_raw_fd(), &weaver_types::TokenRefusal::OutOfOrder);
    } else if mode.ends_with("-malformed") {
        let bytes = b"{not-a-terminal";
        assert_eq!(
            nix::sys::socket::send(decode.as_raw_fd(), bytes, MsgFlags::MSG_NOSIGNAL).unwrap(),
            bytes.len()
        );
    } else if mode.ends_with("-decode-eof") {
        nix::sys::socket::shutdown(decode.as_raw_fd(), nix::sys::socket::Shutdown::Both).unwrap();
    } else {
        packet!(decode.as_raw_fd(), &terminal);
        if mode.ends_with("-duplicate") {
            packet!(decode.as_raw_fd(), &terminal);
        }
    }
    if !mode.ends_with("-silent")
        && !mode.ends_with("-no-rest")
        && !mode.ends_with("-decode-eof")
        && !mode.ends_with("-early-rest")
    {
        // Supply the otherwise valid completion too. Removing a terminal
        // check must expose a false Stop success, not merely another timeout.
        let bytes = serde_json::to_vec(&TokenAnswer::AtRest).unwrap();
        match crate::channel::managed::try_send(decode.as_fd(), &bytes) {
            Ok(true) | Err(crate::ChannelFault::Closed) => (),
            other => panic!("small trailing AtRest: {other:?}"),
        }
    }
    // Keep this actual child alive. A protocol failure must close and reap
    // the owned process, not depend on a scripted successful child exit.
    std::thread::sleep(Duration::from_secs(90));
    panic!("the failed worker did not terminate its owned SPU")
}

pub(super) fn finish(
    root: &std::path::Path,
    child: &mut std::process::Child,
    peer: &OwnedFd,
    end: Instant,
) {
    let end = end.min(Instant::now() + Duration::from_secs(5));
    // This helper rejects every byte. Even an attributed TurnAborted is
    // forbidden when the original decode obligations did not settle.
    crate::flow::fixture_eof::await_eof_until(peer.as_raw_fd(), end, || {});
    let (status, expired) = crate::test_child::wait_until(child, end).unwrap();
    assert!(
        !expired && !status.success(),
        "protocol failure must end the worker"
    );
    let pid = std::fs::read_to_string(root.join("diagnostic-interrupt-spu-pid")).unwrap();
    assert!(
        !std::path::Path::new(&format!("/proc/{}", pid.trim())).exists(),
        "the actual SPU must be reaped"
    );
    assert!(
        !root.join("diagnostic-released").exists(),
        "failed-run cleanup is not successful Leave"
    );
}

/// The real interval includes progress, requester EOF and later force/join.
pub(super) fn timed_finish(
    root: &std::path::Path,
    mode: &str,
    child: &mut std::process::Child,
    peer: &OwnedFd,
    end: Instant,
    began: Instant,
) {
    let probe = dial(&root.join("admin.sock"));
    let in_leave = mode.ends_with("-graceful-silent");
    while began.elapsed() < Duration::from_secs(20) {
        send(&probe, 974, LifecycleDirective::Observe);
        let answer = ObservationEnvelope::decode(&receive(&probe, end)).unwrap();
        assert_eq!(answer.exchange.ordinal, 974);
        assert!(match answer.payload {
            ObservationContent::Answer(ObservationAnswer::State {
                state: AgentState::Active,
                ..
            }) => !in_leave,
            ObservationContent::Answer(ObservationAnswer::InTransition { .. }) => in_leave,
            _ => false,
        });
        assert!(child.try_wait().unwrap().is_none());
        std::thread::sleep(Duration::from_millis(100));
    }
    // Acceptance was observed. Loss of the original answer destination must
    // retire delivery alone, and the new request cannot renew cancellation.
    nix::sys::socket::shutdown(peer.as_raw_fd(), nix::sys::socket::Shutdown::Both).unwrap();
    send(
        &probe,
        975,
        if in_leave {
            LifecycleDirective::JoinLeave {
                cause: Cause { uid: 73 },
            }
        } else {
            LifecycleDirective::Leave {
                cause: Cause { uid: 74 },
                forced: true,
                rollback: false,
            }
        },
    );
    send(&probe, 976, LifecycleDirective::Observe);
    let observed = ObservationEnvelope::decode(&receive(&probe, end)).unwrap();
    assert_eq!(observed.exchange.ordinal, 976);
    assert!(matches!(
        observed.payload,
        ObservationContent::Answer(ObservationAnswer::InTransition { .. })
    ));
    let original_end = began + Duration::from_millis(weaver_types::DIAGNOSTIC_CANCEL_BOUND_MS);
    // An erroneous renewal would add the twenty seconds already spent.
    crate::flow::fixture_eof::await_eof_until(
        probe.as_raw_fd(),
        end.min(original_end + Duration::from_secs(5)),
        || {},
    );
    assert!(
        Instant::now() >= original_end,
        "the production cancellation bound was shortened"
    );
    finish(root, child, &probe, end);
}
