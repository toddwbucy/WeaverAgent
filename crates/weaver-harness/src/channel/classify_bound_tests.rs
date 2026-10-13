//! Test-only: the `classify_bound_tests` suite of `channel.rs`,
//! moved out of the production file by #118.

use super::*;
use nix::sys::socket::{MsgFlags, send};

/// **A late readiness frame spends the answer's bound rather than
/// renewing it.** The peer sends readiness partway into the bound and
/// then nothing, holding its end open so the wait is the deadline's and
/// not a closed peer's. One deadline ends the ask at the bound. A
/// deadline renewed per receive would end it at the frame's arrival plus
/// a whole second bound.
///
/// Perturbation: renew the deadline at each receive in `recv_answer_by`,
/// which is what the engine's loop did before, and the ask runs to about
/// 700 ms against this 400 ms bound and fails the assertion.
#[test]
fn a_late_readiness_spends_the_bound_rather_than_renewing_it() {
    let (near, far) = ClassifyChannel::pair().expect("the pair");
    let bound = 400u64;
    let peer = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(300));
        let ready = serde_json::to_vec(&weaver_types::LabelAnswer::Ready).expect("ready renders");
        send(far.end.as_raw_fd(), &ready, MsgFlags::empty()).expect("ready sent");
        std::thread::sleep(std::time::Duration::from_millis(1500));
        drop(far);
    });
    let began = std::time::Instant::now();
    let reply = near.recv_answer_by(bound);
    let took = began.elapsed();
    assert!(
        reply.is_err(),
        "no answer came, so the ask expires: {reply:?}"
    );
    assert!(
        took < std::time::Duration::from_millis(550),
        "the ask ended at {took:?}, past its {bound} ms bound"
    );
    // The poll's timeout is whole milliseconds, so the last wait may end
    // a fraction of one short of the deadline.
    assert!(
        took >= std::time::Duration::from_millis(bound - 2),
        "the ask waited its bound, took {took:?}"
    );
    peer.join().expect("the peer");
}
