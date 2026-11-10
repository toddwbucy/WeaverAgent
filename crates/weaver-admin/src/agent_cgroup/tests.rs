//! The start step's making, the run end and the ladder's read, against a
//! real delegated cgroup subtree, run as `containment`'s instruments are:
//! `systemd-run --user --scope -p Delegate=yes cargo test -p weaver-admin --locked -- --ignored containment`.
//!
//! **The agent's scope is a scratch cgroup this suite marks delegated
//! itself**: the user manager's delegated scope carries no mark (measured on
//! 2026-10-14), and cgroupfs lets a cgroup's owner set a `user.*` extended
//! attribute, so the suite stands the delegated scope the floor asks for
//! beneath its own, and an unmarked one for the refusal. **The scope step's
//! request is a seam** (6.1, the instruments): a fake PID1 makes the scope
//! beneath the suite's cgroup, marks it and moves the process standing in
//! for the start step into it, so the suite never asks the system's PID1.

use super::*;
use crate::containment::tests::{ScratchCgroup, escaper_in, mark_delegated, run_directory};
use crate::scope::Site;
use std::cell::Cell;
use std::os::fd::AsRawFd;
use std::os::unix::fs::PermissionsExt;
use std::time::Instant;

const BOUND: Duration = Duration::from_secs(10);

/// The scratch cgroup as the scope the floor judges.
fn host(scratch: &ScratchCgroup) -> std::io::Result<Cgroup> {
    Cgroup::open(scratch.cgroup.relative())
}

fn owner() -> (u32, u32) {
    (
        nix::unistd::getuid().as_raw(),
        nix::unistd::getgid().as_raw(),
    )
}

/// A delegated scratch scope and a run directory, and the agent's cgroup
/// made beneath it for `run`.
fn made(tag: &str, run: &str) -> (ScratchCgroup, crate::scratch::Scratch, OwnedFd, Made) {
    let scratch = ScratchCgroup::new(tag);
    mark_delegated(&scratch.cgroup);
    let (dir, fd) = run_directory(tag);
    let made = make_beneath(host(&scratch), fd.as_fd(), "alpha", run, owner(), owner().0)
        .expect("the agent's cgroup is made beneath a delegated scope");
    (scratch, dir, fd, made)
}

fn agent(scratch: &ScratchCgroup) -> std::io::Result<Cgroup> {
    scratch.cgroup.child("weaver-alpha")
}

fn alive(child: &mut std::process::Child) -> bool {
    child
        .try_wait()
        .map(|status| status.is_none())
        .unwrap_or(false)
}

/// **The start makes the layout and its record, and a constituent moves
/// itself into its leaf** (6.1, the layout, the record, C4): every directory
/// `0755`, the three files the agent's account's, the record naming the
/// cgroup by path, identity and run, and a child placed through
/// `run_procs` standing in `worker/run/`. Perturbation: drop the record's
/// write, and no record reads.
#[test]
#[ignore = "needs a delegated cgroup scope"]
fn containment_the_start_makes_the_layout_and_its_record() {
    let (scratch, _dir, fd, made) = made("start-makes", "run-1");
    let agent = agent(&scratch).expect("weaver-alpha stands");
    let base = std::path::PathBuf::from(containment::CGROUP_ROOT)
        .join(agent.relative().trim_start_matches('/'));
    for inner in ["", "constituents", "worker", "worker/run", "worker/tools"] {
        let mode = std::fs::metadata(base.join(inner))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o755, "{inner}");
    }
    let record = containment::read_record(fd.as_fd(), "alpha")
        .expect("the record reads")
        .expect("a record stands");
    assert_eq!(record, made.record);
    assert_eq!(record.run, "run-1");
    assert_eq!(record.path, agent.relative());
    assert_eq!(containment::recorded_at(&record), RecordedAt::Same);
    let raw = made.run_procs().as_raw_fd();
    let mut command = std::process::Command::new("sleep");
    command.arg("300");
    // SAFETY: place_self is async-signal-safe.
    unsafe {
        std::os::unix::process::CommandExt::pre_exec(&mut command, move || {
            containment::place_self(raw)
        });
    }
    let mut child = command.spawn().expect("a constituent spawns into run/");
    let run = agent.child("worker/run").unwrap();
    assert_eq!(run.procs().unwrap(), vec![child.id() as i32]);
    let _ = child.kill();
    let _ = child.wait();
}

/// **An undelegated scope makes nothing** (6.1, the floor; the
/// delegation instrument): the start refuses naming the mark, before any
/// `mkdir`, and no record is written. Perturbation: read every scope as
/// delegated, and the cgroup is made.
#[test]
#[ignore = "needs a delegated cgroup scope"]
fn containment_an_undelegated_scope_makes_nothing() {
    let scratch = ScratchCgroup::new("undelegated");
    let (_dir, fd) = run_directory("undelegated");
    let refused = make_beneath(
        host(&scratch),
        fd.as_fd(),
        "alpha",
        "run-1",
        owner(),
        owner().0,
    );
    assert!(
        matches!(&refused, Err(StartRefused::Unverified(why)) if why.contains("not delegated")),
        "{refused:?}"
    );
    assert!(agent(&scratch).is_err(), "nothing made");
    assert_eq!(containment::read_record(fd.as_fd(), "alpha"), Ok(None));
}

/// **A scope that is not root's makes nothing** (6.1, the floor; #166's
/// guard): a delegated scope whose directory and `cgroup.procs` are not
/// the owner the floor requires refuses naming it, before any `mkdir`, and
/// no record is written. The suite's scope is the suite's own, so the
/// floor's owner is a parameter here: requiring another uid stands for
/// production's root. Perturbation: drop the check, and the cgroup is made.
#[test]
#[ignore = "needs a delegated cgroup scope"]
fn containment_a_scope_not_roots_makes_nothing() {
    let scratch = ScratchCgroup::new("not-roots-scope");
    mark_delegated(&scratch.cgroup);
    let (_dir, fd) = run_directory("not-roots-scope");
    let another = owner().0.wrapping_add(1);
    let refused = make_beneath(
        host(&scratch),
        fd.as_fd(),
        "alpha",
        "run-1",
        owner(),
        another,
    );
    assert!(
        matches!(&refused, Err(StartRefused::Unverified(why)) if why.contains("is not root's")),
        "{refused:?}"
    );
    assert!(agent(&scratch).is_err(), "nothing made");
    assert_eq!(containment::read_record(fd.as_fd(), "alpha"), Ok(None));
}

/// **A load over an ended run's leftovers refuses and kills nothing** (6.1,
/// A3; I8): the record's cgroup populated, the start answers `Leftovers`
/// naming them, and they run on. Perturbation: read a populated cgroup as
/// empty, and the start goes on to remove it.
#[test]
#[ignore = "needs a delegated cgroup scope"]
fn containment_a_start_over_leftovers_refuses_and_kills_nothing() {
    let (scratch, _dir, fd, made) = made("leftovers", "run-1");
    drop(made);
    let run = agent(&scratch).unwrap().child("worker/run").unwrap();
    let mut stray = escaper_in(&run);
    let refused = make_beneath(
        host(&scratch),
        fd.as_fd(),
        "alpha",
        "run-2",
        owner(),
        owner().0,
    );
    match refused {
        Err(StartRefused::Leftovers(pids)) => {
            assert!(pids.contains(&(stray.id() as i32)), "{pids:?}");
        }
        other => panic!("refused with the leftovers named, not {other:?}"),
    }
    assert!(alive(&mut stray), "the leftovers run on");
    assert_eq!(run.populated(), Some(true));
    let _ = stray.kill();
    let _ = stray.wait();
}

/// **An empty cgroup standing is removed and made afresh** (6.1, A1), with
/// its record or at the deterministic path without one. Perturbation: skip
/// the removal, and the `mkdir` meets `EEXIST` and refuses.
#[test]
#[ignore = "needs a delegated cgroup scope"]
fn containment_an_empty_cgroup_is_made_afresh() {
    let (scratch, _dir, fd, first) = made("afresh", "run-1");
    drop(first);
    let second = make_beneath(
        host(&scratch),
        fd.as_fd(),
        "alpha",
        "run-2",
        owner(),
        owner().0,
    )
    .expect("A1 with its record is made afresh");
    assert_eq!(second.record.run, "run-2");
    drop(second);
    containment::remove_record(fd.as_fd()).unwrap();
    let third = make_beneath(
        host(&scratch),
        fd.as_fd(),
        "alpha",
        "run-3",
        owner(),
        owner().0,
    )
    .expect("A1 at the path with no record is made afresh");
    assert_eq!(third.record.run, "run-3");
}

/// **A stale record, its path absent, is removed and the cgroup made**
/// (6.1, the row for a recorded path absent): the new record names the new
/// cgroup. Perturbation: refuse a stale record, and the load wedges.
#[test]
#[ignore = "needs a delegated cgroup scope"]
fn containment_a_stale_record_is_removed_and_the_cgroup_made() {
    let (scratch, _dir, fd, first) = made("stale", "run-1");
    drop(first);
    containment::remove_layout(&scratch.cgroup, "weaver-alpha").unwrap();
    let made = make_beneath(
        host(&scratch),
        fd.as_fd(),
        "alpha",
        "run-2",
        owner(),
        owner().0,
    )
    .expect("a stale record wedges nothing");
    let record = containment::read_record(fd.as_fd(), "alpha")
        .unwrap()
        .unwrap();
    assert_eq!(record, made.record);
    assert_eq!(containment::recorded_at(&record), RecordedAt::Same);
}

/// **The run end kills and removes the agent's cgroup, then its record**
/// (6.1, admin's run end; C2): a command that left its session in
/// `worker/tools/` and a stray in `worker/run/` are ended, the cgroup is
/// gone, the record is gone, and the kill names what it found. Perturbation:
/// skip the kill's write, and the wait runs to its bound and refuses.
#[test]
#[ignore = "needs a delegated cgroup scope"]
fn containment_the_run_end_kills_and_removes_the_cgroup() {
    let (scratch, _dir, fd, made) = made("run-end", "run-1");
    drop(made);
    let agent = agent(&scratch).unwrap();
    let mut tool = escaper_in(&agent.child("worker/tools").unwrap());
    let mut stray = escaper_in(&agent.child("worker/run").unwrap());
    let started = Instant::now();
    let ended = run_end(
        fd.as_fd(),
        "alpha",
        Concluding::Run("run-1"),
        Duration::from_secs(5),
    );
    match ended {
        Ok(Ended::Removed {
            standing,
            record_removed: true,
        }) => assert!(standing.contains(&(stray.id() as i32)), "{standing:?}"),
        other => panic!("killed and removed, not {other:?}"),
    }
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the kill emptied it"
    );
    assert!(
        scratch.cgroup.child("weaver-alpha").is_err(),
        "the cgroup is gone"
    );
    assert_eq!(containment::read_record(fd.as_fd(), "alpha"), Ok(None));
    assert!(tool.wait().is_ok() && stray.wait().is_ok());
    assert_eq!(holds(fd.as_fd(), "alpha"), Ok(None));
}

/// **A run end leaves another load's cgroup alone** (6.1, ownership is the
/// record's): concluding a run the record does not name ends nothing, and
/// the ladder's read names what it holds. Perturbation: conclude by the
/// record's run whatever is concluded, and the stray dies.
#[test]
#[ignore = "needs a delegated cgroup scope"]
fn containment_a_run_end_leaves_another_runs_cgroup() {
    let (scratch, _dir, fd, made) = made("not-owned", "run-1");
    drop(made);
    let mut stray = escaper_in(&agent(&scratch).unwrap().child("worker/run").unwrap());
    assert_eq!(
        run_end(fd.as_fd(), "alpha", Concluding::Run("run-2"), BOUND),
        Ok(Ended::NotOwned)
    );
    assert!(alive(&mut stray), "another run's process runs on");
    let held = holds(fd.as_fd(), "alpha").unwrap().unwrap();
    assert!(held.contains(&(stray.id() as i32)), "{held:?}");
    let _ = stray.kill();
    let _ = stray.wait();
}

/// **A removal that fails refuses and keeps the record** (6.1, its
/// failures): a cgroup beneath `tools/` the layout does not name makes
/// `tools/`'s removal fail, the run end refuses naming it, and the record
/// stands for the next `unload`. Perturbation: read the failed removal as
/// done, and the run end concludes over a cgroup still standing.
#[test]
#[ignore = "needs a delegated cgroup scope"]
fn containment_a_failed_removal_refuses_and_keeps_the_record() {
    let (scratch, _dir, fd, made) = made("removal-fails", "run-1");
    drop(made);
    agent(&scratch)
        .unwrap()
        .child("worker/tools")
        .unwrap()
        .make_child("extra")
        .unwrap();
    let ended = run_end(fd.as_fd(), "alpha", Concluding::Run("run-1"), BOUND);
    assert!(matches!(ended, Err(EndRefused::Unverified(_))), "{ended:?}");
    assert!(
        containment::read_record(fd.as_fd(), "alpha")
            .unwrap()
            .is_some()
    );
}

/// **The ladder reads the agent's cgroup through its record** (6.1, C2;
/// rung (iv)): nothing where no record stands or its path is absent, the
/// pids where it holds any, and a refusal where another cgroup stands at
/// the path. Perturbation: read another cgroup at the path as the recorded
/// one, and its pids are named.
#[test]
#[ignore = "needs a delegated cgroup scope"]
fn containment_the_ladder_reads_through_the_record() {
    let scratch = ScratchCgroup::new("ladder");
    mark_delegated(&scratch.cgroup);
    let (_dir, fd) = run_directory("ladder");
    assert_eq!(holds(fd.as_fd(), "alpha"), Ok(None), "no record");
    let made = make_beneath(
        host(&scratch),
        fd.as_fd(),
        "alpha",
        "run-1",
        owner(),
        owner().0,
    )
    .unwrap();
    drop(made);
    assert_eq!(holds(fd.as_fd(), "alpha"), Ok(None), "empty");
    let mut stray = escaper_in(&agent(&scratch).unwrap().child("worker/run").unwrap());
    assert!(
        holds(fd.as_fd(), "alpha")
            .unwrap()
            .unwrap()
            .contains(&(stray.id() as i32))
    );
    let _ = stray.kill();
    let _ = stray.wait();
    let _ = agent(&scratch).unwrap().kill();
    assert!(agent(&scratch).unwrap().wait_emptied(BOUND));
    containment::remove_layout(&scratch.cgroup, "weaver-alpha").unwrap();
    assert_eq!(holds(fd.as_fd(), "alpha"), Ok(None), "absent: A0");
    scratch.cgroup.make_child("weaver-alpha").unwrap();
    assert!(
        holds(fd.as_fd(), "alpha").is_err(),
        "another cgroup at the path"
    );
}

/// **A cgroup that lists nothing while it reads populated is never empty**
/// (6.1, C2, rung (iv); a failed read is read as not empty): the kernel's
/// listing skips a process mid-exit while `populated` stays 1 (measured on
/// 2026-10-15), and a listing that fails names none the same way. Here
/// `worker/tools/` is closed to the suite with a stray inside, so the walk
/// skips it and names no process, and the ladder's read answers populated
/// naming none, never `None`. Perturbation: read an empty listing as empty,
/// and the closed cgroup reads as holding nothing.
#[test]
#[ignore = "needs a delegated cgroup scope"]
fn containment_a_listing_that_fails_never_reads_as_empty() {
    let (scratch, _dir, fd, made) = made("listing-fails", "run-1");
    drop(made);
    let agent = agent(&scratch).unwrap();
    let mut stray = escaper_in(&agent.child("worker/tools").unwrap());
    let tools = std::path::PathBuf::from(containment::CGROUP_ROOT)
        .join(agent.relative().trim_start_matches('/'))
        .join("worker/tools");
    std::fs::set_permissions(&tools, std::fs::Permissions::from_mode(0o000)).unwrap();
    let held = holds(fd.as_fd(), "alpha");
    std::fs::set_permissions(&tools, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(held, Ok(Some(Vec::new())), "populated, naming none");
    let _ = stray.kill();
    let _ = stray.wait();
}

// ------------------------------------------------------------- the scope step

/// The site the fake PID1 stands the agent's scope at, beneath the scratch
/// cgroup: `<scratch>/weaver.slice/weaver-alpha.scope`.
fn site(scratch: &ScratchCgroup) -> Site {
    Site::at(
        &format!("{}/{}", scratch.cgroup.relative(), scope::SLICE),
        "alpha",
    )
}

/// **The fake PID1** (6.1, the instruments): what systemd does for the
/// request, beneath the suite's own cgroup. The slice is made on first use,
/// a second unit of the same name is refused (D7), the scope is made and
/// marked delegated, and the call's pid is moved into it unless `moves` is
/// false, which answers the request with no move.
fn fake_pid1(
    scratch: &ScratchCgroup,
    moves: bool,
) -> impl FnOnce(&scope::Call) -> Result<String, String> + '_ {
    move |call| {
        let slice = match scratch.cgroup.child(scope::SLICE) {
            Ok(slice) => slice,
            Err(_) => scratch
                .cgroup
                .make_child(scope::SLICE)
                .map_err(|error| format!("the slice: {error:?}"))?,
        };
        if slice.child(&call.unit).is_ok() {
            return Err(format!("Call failed: Unit {} already exists.", call.unit));
        }
        let made = slice
            .make_child(&call.unit)
            .map_err(|error| format!("the scope: {error:?}"))?;
        mark_delegated(&made);
        if moves {
            let procs = made
                .procs_for_placement()
                .map_err(|error| error.to_string())?;
            nix::unistd::write(&procs, call.pid.to_string().as_bytes())
                .map_err(|error| error.to_string())?;
        }
        Ok("o \"/org/freedesktop/systemd1/job/1\"".to_string())
    }
}

/// A process standing in for the start step, placed before its exec in an
/// invoker's cgroup the suite makes beneath the scratch, which carries no
/// delegate mark.
fn start_step_in(scratch: &ScratchCgroup) -> (Cgroup, std::process::Child) {
    let invoker = scratch
        .cgroup
        .make_child("invoker")
        .expect("an invoker's cgroup is made");
    let procs = invoker.procs_for_placement().expect("cgroup.procs opens");
    let raw = procs.as_raw_fd();
    let mut command = std::process::Command::new("sleep");
    command.arg("300");
    // SAFETY: place_self is async-signal-safe.
    unsafe {
        std::os::unix::process::CommandExt::pre_exec(&mut command, move || {
            containment::place_self(raw)
        });
    }
    let child = command.spawn().expect("the stand-in start step spawns");
    drop(procs);
    (invoker, child)
}

fn end(mut child: std::process::Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// **The scope step stands the agent beneath its scope** (6.1, the scope
/// step's instrument): a start step in an invoker's cgroup that carries no
/// delegate mark asks for its scope, its own reading names the scope, and
/// the agent's cgroup and its record stand beneath the scope, nothing
/// beneath the invoker's. Perturbation: drop the step and judge the floor on
/// the start step's own cgroup, and the load refuses `BoundaryUnverified`,
/// not delegated.
#[test]
#[ignore = "needs a delegated cgroup scope"]
fn containment_the_scope_step_stands_the_agent_beneath_its_scope() {
    let scratch = ScratchCgroup::new("scope-step");
    let (invoker, start_step) = start_step_in(&scratch);
    let (_dir, fd) = run_directory("scope-step");
    let site = site(&scratch);
    let made = make_in_scope(
        scope::Step {
            site: site.clone(),
            pid: start_step.id(),
            request: fake_pid1(&scratch, true),
            bound: BOUND,
        },
        fd.as_fd(),
        "alpha",
        "run-1",
        owner(),
        owner().0,
    );
    let pid = start_step.id() as i32;
    let moved_to = containment::process_cgroup(pid);
    end(start_step);
    let made = made.expect("the agent's cgroup stands beneath the scope");
    assert_eq!(made.record.path, format!("{}/weaver-alpha", site.path()));
    assert_eq!(moved_to.as_deref(), Some(site.path().as_str()));
    assert_eq!(
        containment::recorded_at(&made.record),
        RecordedAt::Same,
        "the record names the cgroup beneath the scope"
    );
    assert!(
        invoker.child("weaver-alpha").is_err(),
        "nothing beneath the invoker's cgroup"
    );
}

/// **An answer with no move makes nothing** (6.1, the confirm's
/// instrument, D1): the fake PID1 makes the scope, delegated and the
/// suite's, and answers with no move. The start step refuses
/// `BoundaryUnverified`, naming that no move followed, makes nothing
/// beneath the scope and writes no record, and its process stays in the
/// invoker's cgroup. Perturbation: drop the confirm, and the floor judges
/// the scope the fake made and the load stands.
#[test]
#[ignore = "needs a delegated cgroup scope"]
fn containment_the_confirm_refuses_an_answer_with_no_move() {
    let scratch = ScratchCgroup::new("confirm");
    let (invoker, start_step) = start_step_in(&scratch);
    let (_dir, fd) = run_directory("confirm");
    let site = site(&scratch);
    let refused = make_in_scope(
        scope::Step {
            site: site.clone(),
            pid: start_step.id(),
            request: fake_pid1(&scratch, false),
            bound: Duration::from_millis(200),
        },
        fd.as_fd(),
        "alpha",
        "run-1",
        owner(),
        owner().0,
    );
    let stayed = invoker
        .procs()
        .unwrap_or_default()
        .contains(&(start_step.id() as i32));
    end(start_step);
    assert!(
        matches!(&refused, Err(StartRefused::Unverified(why)) if why.contains("no move followed")),
        "{refused:?}"
    );
    let scope = Cgroup::open(&site.path()).expect("the fake PID1 made the scope");
    assert!(scope.child("weaver-alpha").is_err(), "nothing made");
    assert_eq!(containment::read_record(fd.as_fd(), "alpha"), Ok(None));
    assert!(stayed, "the start step was never moved");
}

/// **A populated scope refuses and kills nothing** (6.1, the standing
/// scope's instrument, P3; I8): a scope standing at the agent's site with a
/// process inside answers `AgentRunning` with the processes named, before
/// any request, and the processes run on. Perturbation: drop the read
/// before the request, and the fake PID1's refusal of a second unit answers
/// `BoundaryUnverified` instead.
#[test]
#[ignore = "needs a delegated cgroup scope"]
fn containment_a_populated_scope_refuses_and_kills_nothing() {
    let scratch = ScratchCgroup::new("standing");
    let slice = scratch.cgroup.make_child(scope::SLICE).unwrap();
    let standing = slice.make_child("weaver-alpha.scope").unwrap();
    mark_delegated(&standing);
    let mut stray = escaper_in(&standing);
    let (_invoker, start_step) = start_step_in(&scratch);
    let (_dir, fd) = run_directory("standing");
    let asked = Cell::new(0);
    let refused = make_in_scope(
        scope::Step {
            site: site(&scratch),
            pid: start_step.id(),
            request: |call: &scope::Call| {
                asked.set(asked.get() + 1);
                Err(format!("Call failed: Unit {} already exists.", call.unit))
            },
            bound: BOUND,
        },
        fd.as_fd(),
        "alpha",
        "run-1",
        owner(),
        owner().0,
    );
    end(start_step);
    match refused {
        Err(StartRefused::Leftovers(pids)) => {
            assert!(pids.contains(&(stray.id() as i32)), "{pids:?}");
        }
        other => panic!("refused with the processes named, not {other:?}"),
    }
    assert_eq!(asked.get(), 0, "no request");
    assert!(alive(&mut stray), "the leftovers run on");
    let _ = stray.kill();
    let _ = stray.wait();
}

/// **A scope standing empty is waited out** (6.1, the scope already
/// standing, P4): one PID1 collects within the bound is followed by the
/// request and the load stands, and one still standing at the bound refuses
/// `BoundaryUnverified`, naming it, with no request. Perturbation: refuse an
/// empty scope at once, and the collected one refuses; skip the wait, and the
/// fake PID1 refuses a second unit.
#[test]
#[ignore = "needs a delegated cgroup scope"]
fn containment_an_empty_scope_is_waited_out() {
    let scratch = ScratchCgroup::new("empty-scope");
    let slice = scratch.cgroup.make_child(scope::SLICE).unwrap();
    slice.make_child("weaver-alpha.scope").unwrap();
    let (_invoker, start_step) = start_step_in(&scratch);
    let (_dir, fd) = run_directory("empty-scope");
    let collector = Cgroup::open(slice.relative()).unwrap();
    let collected = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(100));
        nix::unistd::unlinkat(
            collector.fd(),
            "weaver-alpha.scope",
            nix::unistd::UnlinkatFlags::RemoveDir,
        )
    });
    let made = make_in_scope(
        scope::Step {
            site: site(&scratch),
            pid: start_step.id(),
            request: fake_pid1(&scratch, true),
            bound: BOUND,
        },
        fd.as_fd(),
        "alpha",
        "run-1",
        owner(),
        owner().0,
    );
    end(start_step);
    assert!(collected.join().unwrap().is_ok(), "the stand-in collection");
    assert!(
        made.is_ok(),
        "the collected scope is followed by the request: {made:?}"
    );

    let late = ScratchCgroup::new("empty-scope-late");
    let slice = late.cgroup.make_child(scope::SLICE).unwrap();
    slice.make_child("weaver-alpha.scope").unwrap();
    let (_invoker, start_step) = start_step_in(&late);
    let (_dir, fd) = run_directory("empty-scope-late");
    let asked = Cell::new(0);
    let refused = make_in_scope(
        scope::Step {
            site: site(&late),
            pid: start_step.id(),
            request: |_: &scope::Call| {
                asked.set(asked.get() + 1);
                Ok("o \"/org/freedesktop/systemd1/job/1\"".to_string())
            },
            bound: Duration::from_millis(200),
        },
        fd.as_fd(),
        "alpha",
        "run-1",
        owner(),
        owner().0,
    );
    end(start_step);
    assert!(
        matches!(&refused, Err(StartRefused::Unverified(why)) if why.contains("still stands empty at the scope bound")),
        "{refused:?}"
    );
    assert_eq!(asked.get(), 0, "no request");
}

/// **A scope still standing whose `cgroup.events` gives no populated reading
/// refuses unverified** (6.1, the scope already standing): neither a run's
/// leftovers nor ending, so the load refuses `BoundaryUnverified`, naming the
/// read, with no request and nothing killed. The read is failed by the file's
/// mode, which binds the suite's own uid under a user scope and not root, as
/// the failed listing's instrument does. Perturbation: read an unread scope as
/// populated, and the load refuses `AgentRunning`.
#[test]
#[ignore = "needs a delegated cgroup scope"]
fn containment_an_unread_scope_refuses_unverified() {
    let scratch = ScratchCgroup::new("unread-scope");
    let slice = scratch.cgroup.make_child(scope::SLICE).unwrap();
    let standing = slice.make_child("weaver-alpha.scope").unwrap();
    let events = std::path::PathBuf::from(containment::CGROUP_ROOT)
        .join(standing.relative().trim_start_matches('/'))
        .join("cgroup.events");
    std::fs::set_permissions(&events, std::fs::Permissions::from_mode(0o000)).unwrap();
    let (_invoker, start_step) = start_step_in(&scratch);
    let (_dir, fd) = run_directory("unread-scope");
    let asked = Cell::new(0);
    let refused = make_in_scope(
        scope::Step {
            site: site(&scratch),
            pid: start_step.id(),
            request: |_: &scope::Call| {
                asked.set(asked.get() + 1);
                Ok("o \"/org/freedesktop/systemd1/job/1\"".to_string())
            },
            bound: BOUND,
        },
        fd.as_fd(),
        "alpha",
        "run-1",
        owner(),
        owner().0,
    );
    std::fs::set_permissions(&events, std::fs::Permissions::from_mode(0o444)).unwrap();
    end(start_step);
    assert!(
        matches!(&refused, Err(StartRefused::Unverified(why)) if why.contains("gave no populated reading from its cgroup.events")),
        "{refused:?}"
    );
    assert_eq!(asked.get(), 0, "no request");
}

/// **A scope PID1 collects between its open and its read is absent** (6.1,
/// the scope already standing): the read before the request opens the scope,
/// the scope is removed, and its standing reads absent, so the step goes on
/// to the request. A load right after an unload meets that window while PID1
/// collects the emptied scope. Perturbation: read no populated value as
/// unread whatever stands, and the standing reads unread.
#[test]
#[ignore = "needs a delegated cgroup scope"]
fn containment_a_scope_collected_after_its_open_reads_absent() {
    let scratch = ScratchCgroup::new("collected-scope");
    let slice = scratch.cgroup.make_child(scope::SLICE).unwrap();
    slice.make_child("weaver-alpha.scope").unwrap();
    let opened = Cgroup::open(&site(&scratch).path()).expect("the scope opens");
    nix::unistd::unlinkat(
        slice.fd(),
        "weaver-alpha.scope",
        nix::unistd::UnlinkatFlags::RemoveDir,
    )
    .expect("the stand-in collection");
    let (standing, held) = scope::read_standing(opened);
    assert_eq!(standing, scope::ScopeStanding::Absent);
    assert!(
        held.is_none(),
        "nothing is held for a scope gone from its path"
    );
}

/// **A scope replaced after its open reads unread** (6.1, the scope already
/// standing): the opened scope is removed and another directory made at its
/// path, so the failed read meets a path that is not empty, and the start
/// step refuses rather than request over a directory it never read.
/// Perturbation: read a scope gone by any measure as absent (`Cgroup::gone`,
/// the run end's wider reading), and the replaced scope reads absent.
#[test]
#[ignore = "needs a delegated cgroup scope"]
fn containment_a_scope_replaced_after_its_open_reads_unread() {
    let scratch = ScratchCgroup::new("replaced-scope");
    let slice = scratch.cgroup.make_child(scope::SLICE).unwrap();
    slice.make_child("weaver-alpha.scope").unwrap();
    let opened = Cgroup::open(&site(&scratch).path()).expect("the scope opens");
    nix::unistd::unlinkat(
        slice.fd(),
        "weaver-alpha.scope",
        nix::unistd::UnlinkatFlags::RemoveDir,
    )
    .expect("the opened scope removed");
    slice
        .make_child("weaver-alpha.scope")
        .expect("another directory at the path");
    let (standing, held) = scope::read_standing(opened);
    assert_eq!(standing, scope::ScopeStanding::Unread);
    assert!(held.is_some(), "the opened scope is held for its refusal");
}

/// **A scope whose path does not open after a failed read reads unread**
/// (6.1, the scope already standing): the opened scope is removed and its
/// path then refuses its open, so whether anything stands there is not known,
/// and the start step refuses rather than read the path absent. The open is
/// refused by the slice's mode, which binds the suite's own uid under a user
/// scope and not root, as the unread scope's instrument does. Perturbation:
/// read a path that does not open as absent, and the scope reads absent.
#[test]
#[ignore = "needs a delegated cgroup scope"]
fn containment_a_scope_not_opening_after_its_open_reads_unread() {
    let scratch = ScratchCgroup::new("unopened-scope");
    let slice = scratch.cgroup.make_child(scope::SLICE).unwrap();
    slice.make_child("weaver-alpha.scope").unwrap();
    let opened = Cgroup::open(&site(&scratch).path()).expect("the scope opens");
    nix::unistd::unlinkat(
        slice.fd(),
        "weaver-alpha.scope",
        nix::unistd::UnlinkatFlags::RemoveDir,
    )
    .expect("the opened scope removed");
    let directory = std::path::PathBuf::from(containment::CGROUP_ROOT)
        .join(slice.relative().trim_start_matches('/'));
    std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o000)).unwrap();
    let (standing, held) = scope::read_standing(opened);
    std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(standing, scope::ScopeStanding::Unread);
    assert!(held.is_some(), "the opened scope is held for its refusal");
}
