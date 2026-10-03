//! conforms: admin-stop-answer-relayed-unchanged
//!
//! The verbs' shared parts, per `weaver-admin-Spec` section 3: the stop relay.
//! **Admin authorizes and does not execute**: what a run does after the enter
//! directive is the harness's, and every verb stops at the seam.

use weaver_types::LifecycleAnswer;

// **The fleet map retired with the service account on 2026-08-06**, and the
// init system that held the ordering after it left on 2026-10-03 (#50). What
// orders two invocations of one agent now is the invocation lock, and what
// says a run stands is the run lock, both of `start.rs` per Spec section 3,
// and the load's rollback lives beside the load it undoes.

/// The harness's answer returns to the operator as received: admin holds no
/// opinion about which, because authorizing a stop and deciding what a stop
/// found are different acts, and the second is the harness's. A relay that
/// translated the answer would be admin ruling on a run it does not conduct.
///
/// **This was `#[cfg(test)]` until 2026-08-06 and is now on the verb's own
/// path.** The routing gap it waited on was the operator surface having no
/// target to convey a stop to, since the floor's `Stop` carries no agent name.
/// The recut closed that from the other side: the operator names the agent as
/// an argument and the invocation dials that agent's own socket, so the
/// directive reaches one worker by construction. A relay reachable only from
/// its own test pinned nothing, which is why it moved rather than staying.
pub fn relay_stop_answer(from_harness: LifecycleAnswer) -> LifecycleAnswer {
    from_harness
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stop_answer_is_relayed_unchanged() {
        let aborted = LifecycleAnswer::TurnAborted {
            turn: weaver_types::TurnKey("t-1".into()),
        };
        assert_eq!(relay_stop_answer(aborted.clone()), aborted);
        assert_eq!(
            relay_stop_answer(LifecycleAnswer::AtRest),
            LifecycleAnswer::AtRest
        );
    }
}
