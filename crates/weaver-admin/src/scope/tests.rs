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
/// state table): absent requests, empty is waited out, populated refuses as
/// a run's leftovers and unread refuses as unverified, both with no request.
/// Perturbation: any arm changed.
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
    assert_eq!(
        standing_judgment(ScopeStanding::Unread),
        StandingAction::RefuseUnread
    );
}

/// **Only the start step's own reading of the move goes on** (6.1, D1):
/// an answered request with no move refuses, and a refused request refuses.
/// Perturbation: go on at an answer, and the no-move row stands.
#[test]
fn the_scope_rows_go_on_only_at_the_move() {
    assert_eq!(request_judgment(RequestOutcome::Moved), RequestAction::GoOn);
    assert_eq!(
        request_judgment(RequestOutcome::AnsweredNoMove),
        RequestAction::RefuseNoMove
    );
    assert_eq!(
        request_judgment(RequestOutcome::Refused),
        RequestAction::RefuseAnswered
    );
}

/// **Production's step is the agent's site, this process and the scope
/// bound** (6.1, Where): the pid the request moves is the start step's own,
/// so the confirm reads `/proc/self/cgroup`. Perturbation: pass any other
/// pid, site or bound, and the step differs.
#[test]
fn production_step_moves_this_process_to_the_agents_scope() {
    let step = Step::production("alpha");
    assert_eq!(step.site, Site::of("alpha"));
    assert_eq!(step.pid, std::process::id());
    assert_eq!(step.bound, SCOPE_BOUND);
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
        deadline: Instant::now() + SCOPE_BOUND,
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

/// **The scope bound, its poll and the helper's grace are the Spec's** (6.1,
/// the scope bound): a 10 s ceiling read every millisecond, and a helper
/// waited for 50 ms past the deadline and its output 50 ms past its end.
/// Perturbation: change any.
#[test]
fn the_scope_bound_is_ten_seconds_read_every_millisecond() {
    assert_eq!(SCOPE_BOUND, Duration::from_secs(10));
    assert_eq!(SCOPE_POLL, Duration::from_millis(1));
    assert_eq!(HELPER_GRACE, Duration::from_millis(50));
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
    let refused = enter(Step {
        site: site.clone(),
        pid: std::process::id(),
        request: |call: &Call| {
            asked.set(asked.get() + 1);
            assert_eq!(call.unit, "weaver-alpha.scope");
            assert_eq!(call.slice, SLICE);
            assert_eq!(call.pid, std::process::id());
            assert_eq!(call.timeout, Duration::from_secs(5));
            assert!(call.deadline > Instant::now(), "the deadline is ahead");
            assert!(
                call.deadline <= Instant::now() + Duration::from_secs(5),
                "the deadline is the bound counted from the request"
            );
            Err("Call failed: Unit weaver-alpha.scope already exists.".to_string())
        },
        bound: Duration::from_secs(5),
    });
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
    let refused = enter(Step {
        site,
        pid: std::process::id(),
        request: |_: &Call| Ok("o \"/org/freedesktop/systemd1/job/1\"".to_string()),
        bound,
    });
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

// ------------------------------------------------- the helper, held (#522)

/// A shell command, the helper the tests hold to a deadline in place of
/// `busctl`.
fn shell(command: &str) -> Vec<String> {
    vec!["-c".to_string(), command.to_string()]
}

/// A file a test's shell records a pid in, emptied first, and its path
/// single-quoted for the shell.
fn pid_record(test: &str) -> (std::path::PathBuf, String) {
    let record =
        std::env::temp_dir().join(format!("weaver-test-{test}-pid-{}", std::process::id()));
    let _ = std::fs::remove_file(&record);
    let quoted = format!("'{}'", record.display().to_string().replace('\'', r"'\''"));
    (record, quoted)
}

/// **A helper that answers is read and reaped at once** (6.1, the scope
/// bound): its standard output, trimmed, is the reply, with no wait for the
/// deadline. Perturbation: answer without reading its output, and the reply
/// is empty.
#[test]
fn a_helper_that_answers_is_read_and_reaped() {
    let started = Instant::now();
    let answered = helper(
        "/bin/sh",
        &shell("echo 'o /org/freedesktop/systemd1/job/7'"),
        Instant::now() + Duration::from_secs(5),
        Duration::from_secs(5),
    );
    assert_eq!(
        answered.as_deref(),
        Ok("o /org/freedesktop/systemd1/job/7"),
        "{answered:?}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "no wait for the deadline"
    );
}

/// **A helper that fails names its status and what it said** (6.1, the
/// request refused). Perturbation: drop the status or the standard error from
/// the refusal, and it no longer names them.
#[test]
fn a_helper_that_fails_names_its_status_and_what_it_said() {
    let refused = helper(
        "/bin/sh",
        &shell("echo 'Call failed: Access denied' >&2; exit 3"),
        Instant::now() + Duration::from_secs(5),
        Duration::from_secs(5),
    );
    let why = refused.expect_err("a failed helper refuses");
    assert!(why.contains("/bin/sh"), "{why}");
    assert!(why.contains("exit status: 3"), "{why}");
    assert!(why.contains("Access denied"), "{why}");
}

/// **A helper that does not run is named at once** (6.1, the request
/// refused): a start that fails answers as it fails, not at the wait's end.
/// Perturbation: wait out the deadline before reading a failed start, and
/// the call takes the 5 s.
#[test]
fn a_helper_that_does_not_run_is_named() {
    let started = Instant::now();
    let refused = helper(
        "/weaver-test-no-such-helper",
        &[],
        Instant::now() + Duration::from_secs(5),
        Duration::from_secs(5),
    );
    let why = refused.expect_err("an absent helper refuses");
    assert!(
        why.contains("/weaver-test-no-such-helper does not run"),
        "{why}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "at once, not at the wait's end, {:?}",
        started.elapsed()
    );
}

/// **A helper whose start blocks is held to the wait** (6.1, the scope
/// bound, #522): a spawn returns only once the helper has exec'd, so an exec
/// blocked on its filesystem would hold the call before the wait began. The
/// start runs on a thread of its own, the call refuses at the wait's end,
/// and the start that returns later is killed and reaped by its thread. Here
/// the start is held 1 s past a 300 ms deadline, then starts a long helper
/// and records its pid.
/// Perturbations: start inline, and the call takes the held second; drop the
/// late start's kill, and its pid stands.
#[test]
fn a_helper_whose_start_blocks_is_held_to_the_wait() {
    let (record, _) = pid_record("late");
    let written = record.clone();
    let deadline = Duration::from_millis(300);
    let started = Instant::now();
    let refused = helper_from(
        "/bin/sh",
        move || {
            std::thread::sleep(Duration::from_secs(1));
            let child = std::process::Command::new("/bin/sh")
                .args(shell("exec sleep 5"))
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()?;
            std::fs::write(&written, child.id().to_string())?;
            Ok(child)
        },
        Instant::now() + deadline,
        deadline,
    );
    let elapsed = started.elapsed();
    let why = refused.expect_err("a blocked start refuses");
    assert!(
        why.contains("did not start within the scope bound (300 ms)"),
        "{why}"
    );
    assert!(
        elapsed >= deadline + HELPER_GRACE,
        "held to the wait's end, {elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_millis(900),
        "not to the start's return, {elapsed:?}"
    );
    let pid = (0..300)
        .find_map(|_| {
            let pid = std::fs::read_to_string(&record)
                .ok()
                .filter(|pid| !pid.trim().is_empty());
            if pid.is_none() {
                std::thread::sleep(Duration::from_millis(10));
            }
            pid
        })
        .expect("the late start recorded its pid");
    let _ = std::fs::remove_file(&record);
    let pid = pid.trim().to_string();
    let reaped = (0..200).any(|_| {
        let gone = !std::path::Path::new(&format!("/proc/{pid}")).exists();
        if !gone {
            std::thread::sleep(Duration::from_millis(10));
        }
        gone
    });
    assert!(reaped, "the late helper {pid} is killed and reaped");
}

/// **A helper still running past the deadline is killed and reaped** (6.1,
/// the scope bound, #522): the call returns at the grace past the deadline,
/// not at the helper's own end, refusing with the bound and what it had said,
/// and the helper's pid is gone from `/proc`, reaped, not a zombie. The shell
/// records its pid and execs the sleep under it.
/// Perturbations: wait for the helper with no deadline, and the call takes
/// the sleep's 5 s; kill it without reaping, and its pid stands as a zombie.
#[test]
fn a_helper_still_running_at_the_deadline_is_killed_and_reaped() {
    let (record, quoted) = pid_record("helper");
    let deadline = Duration::from_millis(300);
    let started = Instant::now();
    let refused = helper(
        "/bin/sh",
        &shell(&format!(
            "echo $$ > {quoted}; echo 'connecting to the bus' >&2; exec sleep 5"
        )),
        Instant::now() + deadline,
        deadline,
    );
    let elapsed = started.elapsed();
    let why = refused.expect_err("a hung helper refuses");
    assert!(
        why.contains("still running past the scope bound (300 ms)"),
        "{why}"
    );
    assert!(why.contains("killed and reaped"), "{why}");
    assert!(why.contains("connecting to the bus"), "{why}");
    assert!(
        elapsed >= deadline + HELPER_GRACE,
        "held to its deadline and the grace, {elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_secs(2),
        "not to its own end, {elapsed:?}"
    );
    let pid = std::fs::read_to_string(&record).expect("the helper recorded its pid");
    let _ = std::fs::remove_file(&record);
    let pid = pid.trim();
    assert!(
        !std::path::Path::new(&format!("/proc/{pid}")).exists(),
        "the killed helper {pid} is reaped, not left a zombie"
    );
}

/// **A process the helper started cannot hold the call** (6.1, the scope
/// bound): the helper answers and ends, a process it started keeps its
/// output open, and the call returns at the wait's end, the grace past the
/// deadline, refusing with the helper's status whatever it answered. The
/// started process records its pid and is killed after.
/// Perturbations: wait for the pipes with no bound, and the call takes the
/// started process's 5 s; drop the status, and the refusal no longer names
/// it.
#[test]
fn a_process_the_helper_started_cannot_hold_the_call() {
    let (record, quoted) = pid_record("started");
    let deadline = Duration::from_millis(300);
    let started = Instant::now();
    let refused = helper(
        "/bin/sh",
        &shell(&format!("sleep 5 & echo $! > {quoted}; echo reply")),
        Instant::now() + deadline,
        deadline,
    );
    let elapsed = started.elapsed();
    if let Some(pid) = std::fs::read_to_string(&record)
        .ok()
        .and_then(|pid| pid.trim().parse::<i32>().ok())
    {
        // SAFETY: a plain kill(2) of the sleep this test's shell started.
        let _ = unsafe { nix::libc::kill(pid, nix::libc::SIGKILL) };
    }
    let _ = std::fs::remove_file(&record);
    let why = refused.expect_err("an output still held refuses");
    assert!(
        why.contains("a process it started still holds its output"),
        "{why}"
    );
    assert!(why.contains("exit status: 0"), "{why}");
    assert!(
        elapsed >= deadline + HELPER_GRACE,
        "its pipes left the whole wait, {elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_secs(2),
        "not held by it, {elapsed:?}"
    );
}

/// **A helper that answers early leaves its pipes the whole wait** (6.1, the
/// scope bound): its output is collected until the wait's end, not only the
/// grace past the helper's own end, so a reader that is late to its pipe's
/// close, on a loaded box, never refuses an answer. Here a process the
/// helper started holds its output 300 ms past the helper's end and then
/// ends, and the reply is read.
/// Perturbation: collect the output only the grace past the helper's end,
/// and the call refuses the reply as held.
#[test]
fn a_helper_that_answers_early_leaves_its_pipes_the_whole_wait() {
    let started = Instant::now();
    let answered = helper(
        "/bin/sh",
        &shell("sleep 0.3 & echo 'o /org/freedesktop/systemd1/job/7'"),
        Instant::now() + Duration::from_secs(5),
        Duration::from_secs(5),
    );
    assert_eq!(
        answered.as_deref(),
        Ok("o /org/freedesktop/systemd1/job/7"),
        "{answered:?}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "read at the pipe's close, not the deadline"
    );
}

/// **A helper that ends within the grace keeps its own answer** (6.1, the
/// scope bound, #522): where PID1 stalls, `busctl`'s own timeout counts from
/// its start, a little after the deadline's, so the helper is waited for the
/// grace past the deadline and its own refusal names the stall. Here the
/// deadline is already reached as the helper starts.
/// Perturbation: kill the helper at the deadline, and the refusal names the
/// kill, not its status and what it said.
#[test]
fn a_helper_that_ends_within_the_grace_keeps_its_own_answer() {
    let refused = helper(
        "/bin/sh",
        &shell("echo 'Call failed: Connection timed out' >&2; exit 1"),
        Instant::now(),
        Duration::from_secs(5),
    );
    let why = refused.expect_err("a failed helper refuses");
    assert!(why.contains("exit status: 1"), "{why}");
    assert!(why.contains("Connection timed out"), "{why}");
    assert!(!why.contains("killed"), "{why}");
}

/// **A hung request refuses at the scope bound** (6.1, the request refused,
/// #522): the step hands the request its own deadline, the scope bound
/// counted from the request, and a helper held to it ends the load the grace
/// past it, `BoundaryUnverified` naming the hang. Perturbation: give the call
/// a later deadline than the step's, and the helper runs to its own end.
#[test]
fn a_hung_request_refuses_at_the_scope_bound() {
    let site = absent_site();
    let bound = Duration::from_millis(300);
    let started = Instant::now();
    let refused = enter(Step {
        site: site.clone(),
        pid: std::process::id(),
        request: |call: &Call| {
            helper(
                "/bin/sh",
                &shell("exec sleep 5"),
                call.deadline,
                call.timeout,
            )
        },
        bound,
    });
    let elapsed = started.elapsed();
    match refused {
        Err(ScopeRefused::Unverified(why)) => {
            assert!(why.contains("was refused"), "{why}");
            assert!(why.contains("killed and reaped"), "{why}");
            assert!(why.contains(&site.path()), "{why}");
        }
        other => panic!("refused at the bound, not {other:?}"),
    }
    assert!(
        elapsed >= bound + HELPER_GRACE,
        "held to the bound and the grace, {elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_secs(2),
        "not to the helper's end, {elapsed:?}"
    );
}
