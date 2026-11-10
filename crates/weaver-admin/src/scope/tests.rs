//! The scope step's rows and its call, each a test, and the request's two
//! refusals, which stand nothing and so need no delegated cgroup: a site
//! whose path is absent is read before the request, and the confirm reads
//! this process's own cgroup, which is never that path. The instruments that
//! stand a scope live with the start step's, in `agent_cgroup`'s tests.

use super::*;
use std::cell::Cell;

/// A site beneath a path no cgroup stands at, so the read before the
/// request answers absent.
fn absent_site() -> Site {
    Site::at(
        &format!("/weaver-test-no-such-slice-{}", std::process::id()),
        "alpha",
    )
}

/// **The scope's rows for what stands, transcribed** (6.1, the scope's
/// state table): absent requests, empty is waited out, populated refuses
/// with no request. Perturbation: any arm changed.
#[test]
fn the_scope_rows_answer_what_stands() {
    assert_eq!(
        standing_judgment(ScopeStanding::Absent),
        StandingAction::Request
    );
    assert_eq!(
        standing_judgment(ScopeStanding::Empty),
        StandingAction::AwaitCollection
    );
    assert_eq!(
        standing_judgment(ScopeStanding::Populated),
        StandingAction::RefuseLeftovers
    );
}

/// **Only the start step's own reading of the move goes on** (6.1, D1):
/// an answered request with no move refuses, and a refused request refuses
/// whatever was read. Perturbation: go on at an answer, and the no-move row
/// stands.
#[test]
fn the_scope_rows_go_on_only_at_the_move() {
    assert_eq!(request_judgment(true, true), RequestAction::GoOn);
    assert_eq!(request_judgment(true, false), RequestAction::RefuseNoMove);
    assert_eq!(
        request_judgment(false, false),
        RequestAction::RefuseAnswered
    );
    assert_eq!(request_judgment(false, true), RequestAction::RefuseAnswered);
}

/// **The scope is the agent's, in the agents' slice** (6.1, one scope per
/// agent): `/weaver.slice/weaver-<agent>.scope`. Perturbation: name the
/// scope for anything but the agent, and the path differs.
#[test]
fn the_site_is_the_agents_scope_in_the_slice() {
    assert_eq!(Site::of("alpha").path(), "/weaver.slice/weaver-alpha.scope");
    assert_eq!(unit_name("alpha"), "weaver-alpha.scope");
    assert_eq!(
        Site::at("/user.slice/x/weaver.slice/", "alpha").path(),
        "/user.slice/x/weaver.slice/weaver-alpha.scope"
    );
}

/// **The call carries the four properties in mode `fail`** (6.1, Where):
/// `StartTransientUnit` on the manager, `PIDs` holding the one pid,
/// `Delegate=yes`, the slice and `CollectMode=inactive-or-failed`, no
/// auxiliary units, the system bus and the call's timeout. The vector is the
/// call Part B0 measured on 2026-11-09, with the timeout added.
/// Perturbation: drop or change any property, the mode or the bus, and the
/// vector differs.
#[test]
fn the_call_carries_the_four_properties_in_mode_fail() {
    let call = Call {
        unit: "weaver-alpha.scope".to_string(),
        slice: SLICE.to_string(),
        pid: 4242,
        timeout: SCOPE_BOUND,
    };
    assert_eq!(
        busctl_arguments(&call),
        [
            "--system",
            "--timeout=10000ms",
            "call",
            "org.freedesktop.systemd1",
            "/org/freedesktop/systemd1",
            "org.freedesktop.systemd1.Manager",
            "StartTransientUnit",
            "ssa(sv)a(sa(sv))",
            "weaver-alpha.scope",
            "fail",
            "4",
            "PIDs",
            "au",
            "1",
            "4242",
            "Delegate",
            "b",
            "true",
            "Slice",
            "s",
            "weaver.slice",
            "CollectMode",
            "s",
            "inactive-or-failed",
            "0",
        ]
    );
    assert_eq!(BUSCTL, "/usr/bin/busctl", "a fixed path, never PATH's");
}

/// **The scope bound and its poll are the Spec's** (6.1, the scope bound):
/// a 10 s ceiling read every millisecond. Perturbation: change either.
#[test]
fn the_scope_bound_is_ten_seconds_read_every_millisecond() {
    assert_eq!(SCOPE_BOUND, Duration::from_secs(10));
    assert_eq!(SCOPE_POLL, Duration::from_millis(1));
}

/// **A refused request refuses at once, naming what it answered** (6.1, the
/// request refused): `BoundaryUnverified`, no wait for a move, the request
/// made once with the agent's unit, the slice, the given pid and the bound.
/// Perturbation: read for a move after a refusal, and the bound is spent.
#[test]
fn a_refused_request_refuses_naming_its_answer() {
    let site = absent_site();
    let asked = Cell::new(0);
    let started = Instant::now();
    let refused = enter(
        &site,
        std::process::id(),
        |call| {
            asked.set(asked.get() + 1);
            assert_eq!(call.unit, "weaver-alpha.scope");
            assert_eq!(call.slice, SLICE);
            assert_eq!(call.pid, std::process::id());
            assert_eq!(call.timeout, Duration::from_secs(5));
            Err("Call failed: Unit weaver-alpha.scope already exists.".to_string())
        },
        Duration::from_secs(5),
    );
    match refused {
        Err(ScopeRefused::Unverified(why)) => {
            assert!(why.contains("was refused"), "{why}");
            assert!(why.contains("already exists"), "{why}");
            assert!(why.contains(&site.path()), "{why}");
        }
        other => panic!("refused naming the answer, not {other:?}"),
    }
    assert_eq!(asked.get(), 1);
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "no wait for a move"
    );
}

/// **An answer with no move refuses at the bound, naming that no move
/// followed** (6.1, D1, the confirm): the call's answer is never read as the
/// move. This process never stands at the site's path, so its own reading
/// never names it. Perturbation: read the answer as the move, and the step
/// goes on to open a scope that is not there.
#[test]
fn an_answer_with_no_move_refuses_at_the_bound() {
    let site = absent_site();
    let bound = Duration::from_millis(50);
    let started = Instant::now();
    let refused = enter(
        &site,
        std::process::id(),
        |_| Ok("o \"/org/freedesktop/systemd1/job/1\"".to_string()),
        bound,
    );
    match refused {
        Err(ScopeRefused::Unverified(why)) => {
            assert!(why.contains("no move followed"), "{why}");
            assert!(why.contains("job/1"), "{why}");
        }
        other => panic!("refused, no move, not {other:?}"),
    }
    assert!(started.elapsed() >= bound, "the confirm reads to its bound");
}

/// **The confirm reads the process's own cgroup** (D1): it names this
/// process's cgroup at once and a path it is not in never. Perturbation:
/// answer the confirm without the reading, and the second read passes.
#[test]
fn the_confirm_reads_the_process_cgroup() {
    let own = crate::containment::process_cgroup(std::process::id() as i32)
        .expect("this process's cgroup reads");
    let soon = Instant::now() + Duration::from_secs(1);
    assert!(confirm(std::process::id(), &own, soon));
    let shortly = Instant::now() + Duration::from_millis(20);
    assert!(!confirm(
        std::process::id(),
        &format!("{own}/weaver-test-elsewhere"),
        shortly
    ));
}
