//! Stop's original exchange survives a later graceful or forced Leave.
use super::*;

pub(super) fn terminal(root: &std::path::Path) {
    let end = Instant::now() + Duration::from_secs(5);
    while !root.join("diagnostic-stop-joined").exists() {
        assert!(Instant::now() < end, "Leave must join the pending Cancel");
        std::thread::sleep(Duration::from_millis(1));
    }
}

pub(super) fn leave(
    root: &std::path::Path,
    original: &OwnedFd,
    end: Instant,
    forced: bool,
) -> OwnedFd {
    while !root.join("diagnostic-cancelled").exists() {
        assert!(Instant::now() < end);
        std::thread::sleep(Duration::from_millis(1));
    }
    observed(original, 988, AgentState::Active, end);
    let peer = dial(&root.join("admin.sock"));
    send(
        &peer,
        972,
        LifecycleDirective::Leave {
            cause: Cause { uid: 71 },
            forced,
            rollback: false,
        },
    );
    send(&peer, 989, LifecycleDirective::Observe);
    let answer = ObservationEnvelope::decode(&receive(&peer, end)).unwrap();
    assert_eq!(answer.exchange.ordinal, 989);
    assert!(matches!(
        answer.payload,
        ObservationContent::Answer(ObservationAnswer::InTransition { .. })
    ));
    for held in [original, &peer] {
        assert_eq!(
            recv(held.as_raw_fd(), &mut [0; 4096], MsgFlags::MSG_DONTWAIT),
            Err(nix::errno::Errno::EAGAIN),
            "neither original request completes before the terminal and AtRest"
        );
    }
    std::fs::write(root.join("diagnostic-stop-joined"), b"Leave retained\n").unwrap();
    peer
}
