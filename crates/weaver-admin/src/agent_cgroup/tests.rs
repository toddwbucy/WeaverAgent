//! The start step's making, the run end and the ladder's read, against a
//! real delegated cgroup subtree, run as `containment`'s instruments are:
//! `systemd-run --user --scope -p Delegate=yes cargo test -p weaver-admin --locked -- --ignored containment`.
//!
//! **The invoker is a scratch cgroup this suite marks delegated itself**:
//! the user manager's delegated scope carries no mark (measured on
//! 2026-10-14), and cgroupfs lets a cgroup's owner set a `user.*` extended
//! attribute, so the suite stands the delegated invoker the floor asks for
//! beneath its own scope, and an unmarked one for the refusal.

use super::*;
use crate::containment::tests::{ScratchCgroup, escaper_in, mark_delegated, run_directory};
use std::os::fd::AsRawFd;
use std::os::unix::fs::PermissionsExt;
use std::time::Instant;

const BOUND: Duration = Duration::from_secs(10);

fn invoker(scratch: &ScratchCgroup) -> std::io::Result<Cgroup> {
    Cgroup::open(scratch.cgroup.relative())
}

fn owner() -> (u32, u32) {
    (
        nix::unistd::getuid().as_raw(),
        nix::unistd::getgid().as_raw(),
    )
}

/// A delegated scratch invoker and a run directory, and the agent's cgroup
/// made beneath it for `run`.
fn made(tag: &str, run: &str) -> (ScratchCgroup, crate::scratch::Scratch, OwnedFd, Made) {
    let scratch = ScratchCgroup::new(tag);
    mark_delegated(&scratch.cgroup);
    let (dir, fd) = run_directory(tag);
    let made = make_beneath(invoker(&scratch), fd.as_fd(), "alpha", run, owner())
        .expect("the agent's cgroup is made beneath a delegated invoker");
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

/// **An undelegated invoker makes nothing** (6.1, the floor; the
/// delegation instrument): the start refuses naming the mark, before any
/// `mkdir`, and no record is written. Perturbation: read every invoker as
/// delegated, and the cgroup is made.
#[test]
#[ignore = "needs a delegated cgroup scope"]
fn containment_an_undelegated_invoker_makes_nothing() {
    let scratch = ScratchCgroup::new("undelegated");
    let (_dir, fd) = run_directory("undelegated");
    let refused = make_beneath(invoker(&scratch), fd.as_fd(), "alpha", "run-1", owner());
    assert!(
        matches!(&refused, Err(StartRefused::Unverified(why)) if why.contains("not delegated")),
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
    let refused = make_beneath(invoker(&scratch), fd.as_fd(), "alpha", "run-2", owner());
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
    let second = make_beneath(invoker(&scratch), fd.as_fd(), "alpha", "run-2", owner())
        .expect("A1 with its record is made afresh");
    assert_eq!(second.record.run, "run-2");
    drop(second);
    containment::remove_record(fd.as_fd()).unwrap();
    let third = make_beneath(invoker(&scratch), fd.as_fd(), "alpha", "run-3", owner())
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
    let made = make_beneath(invoker(&scratch), fd.as_fd(), "alpha", "run-2", owner())
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
    assert_eq!(holds(fd.as_fd(), "alpha"), Ok(Vec::new()));
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
    let held = holds(fd.as_fd(), "alpha").unwrap();
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
    assert_eq!(holds(fd.as_fd(), "alpha"), Ok(Vec::new()), "no record");
    let made = make_beneath(invoker(&scratch), fd.as_fd(), "alpha", "run-1", owner()).unwrap();
    drop(made);
    assert_eq!(holds(fd.as_fd(), "alpha"), Ok(Vec::new()), "empty");
    let mut stray = escaper_in(&agent(&scratch).unwrap().child("worker/run").unwrap());
    assert!(
        holds(fd.as_fd(), "alpha")
            .unwrap()
            .contains(&(stray.id() as i32))
    );
    let _ = stray.kill();
    let _ = stray.wait();
    let _ = agent(&scratch).unwrap().kill();
    assert!(agent(&scratch).unwrap().wait_empty(BOUND));
    containment::remove_layout(&scratch.cgroup, "weaver-alpha").unwrap();
    assert_eq!(holds(fd.as_fd(), "alpha"), Ok(Vec::new()), "absent: A0");
    scratch.cgroup.make_child("weaver-alpha").unwrap();
    assert!(
        holds(fd.as_fd(), "alpha").is_err(),
        "another cgroup at the path"
    );
}
