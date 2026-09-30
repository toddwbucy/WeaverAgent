//! conforms: admin-stop-answer-relayed-unchanged
//! conforms: admin-rollback-logs-its-account
//!
//! The verbs' shared parts, per `weaver-admin-Spec` section 3: what a failed
//! load leaves standing, the rollback that walks it, and the stop relay.
//! **Admin authorizes and does not execute**: what a run does after the enter
//! directive is the harness's, and every verb stops at the seam.

use weaver_types::LifecycleAnswer;

// **The fleet map retired with the service account on 2026-08-06.**
//
// A per-invocation crate has nowhere to keep a map across verbs, and the map
// is not missed for what it truly knew: whether an agent's unit is running is
// a question the init system answers authoritatively, per Spec section 3, and
// a map of admin's own would be a second account of a fact the process
// manager already holds.
//
// **What the in-flight flag held is delegated rather than dropped.** Starting
// a transient unit whose name already exists fails at the init system, so two
// concurrent loads of one agent cannot both start a worker. Two concurrent
// unloads reach a worker that answers leave once and refuses the second by
// the channel's own ordering. Neither race is prevented by a lock of this
// crate's, and the honest statement is that the ordering is delegated to the
// two parties that already serialize: the process manager and the worker.

/// What a load left standing, walked in reverse by the rollback.
#[derive(Debug, Default, Clone)]
pub struct Standing {
    pub entered: bool,
    pub unit_started: bool,
    pub sink_opened: bool,
}

/// One rollback act's outcome, for the account the log carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Undone {
    pub act: &'static str,
    pub succeeded: bool,
}

/// **Rollback is the reap plus one directive, as data.** It walks what stands:
/// direct leave where a run was entered, stop the unit where a unit started,
/// close the sink where one opened. Each act's outcome enters the account, the
/// rollback reports what it could not undo, and **no state is published on any
/// partial outcome**.
pub fn rollback<L, U, S>(
    standing: &Standing,
    mut direct_leave: L,
    mut stop_unit: U,
    mut close_sink: S,
) -> Vec<Undone>
where
    L: FnMut() -> bool,
    U: FnMut() -> bool,
    S: FnMut() -> bool,
{
    let mut account = Vec::new();
    if standing.entered {
        account.push(Undone {
            act: "leave",
            succeeded: direct_leave(),
        });
    }
    if standing.unit_started {
        account.push(Undone {
            act: "unit-stop",
            succeeded: stop_unit(),
        });
    }
    if standing.sink_opened {
        account.push(Undone {
            act: "sink-close",
            succeeded: close_sink(),
        });
    }
    account
}

/// **`stop` is a conveyance and its answer is a relay.**
///
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

    /// **The rollback's account:** a load failed after each step leaves
    /// exactly what the charter names, and the account records what was undone
    /// and what could not be.
    ///
    /// Perturbation: move the account off the rollback path and it goes
    /// silent, which the emptiness assertion catches.
    #[test]
    fn the_rollback_walks_only_what_stands() {
        // Nothing stood up: nothing is undone.
        let empty = rollback(&Standing::default(), || true, || true, || true);
        assert!(
            empty.is_empty(),
            "a load that stood nothing up undoes nothing"
        );

        // A sink opened and a unit started, but no run entered.
        let partial = rollback(
            &Standing {
                entered: false,
                unit_started: true,
                sink_opened: true,
            },
            || panic!("no leave is directed where no run entered"),
            || true,
            || true,
        );
        assert_eq!(
            partial,
            vec![
                Undone {
                    act: "unit-stop",
                    succeeded: true
                },
                Undone {
                    act: "sink-close",
                    succeeded: true
                },
            ]
        );

        // Everything stood, and the leave could not be undone: the account
        // reports what it could not do rather than going quiet.
        let full = rollback(
            &Standing {
                entered: true,
                unit_started: true,
                sink_opened: true,
            },
            || false,
            || true,
            || true,
        );
        assert_eq!(
            full[0],
            Undone {
                act: "leave",
                succeeded: false
            }
        );
        assert_eq!(full.len(), 3, "the reverse order is walked whole");
    }

    // **The two fleet-map tests retired here on 2026-08-06** with the map they
    // read: one asserted a second transition refused while one was in flight,
    // and one asserted a failed transition published nothing. Both properties
    // survive, delegated per this module's head, and neither is this crate's
    // to hold any more. A test kept over a deleted mechanism would assert the
    // mechanism still existed.

    /// The stop answer is relayed unchanged: admin holds no opinion about what
    /// a stop found.
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
