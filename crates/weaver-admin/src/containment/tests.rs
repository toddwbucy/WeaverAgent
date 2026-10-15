//! 6.1's rows that admin owns, each a test, and the primitives' instruments
//! against a real delegated cgroup subtree.
//!
//! **The instruments run in the user manager's delegated scope**: they are
//! `#[ignore]` and run as
//! `systemd-run --user --scope -p Delegate=yes cargo test -p weaver-admin --locked -- --ignored containment`.
//! Each makes a scratch cgroup beneath the scope it runs in, and outside a
//! delegated scope it fails naming why, never passing by skip. The suite runs
//! as one account, so the owner the start step gives the agent's account is
//! the suite's own here.

use super::*;
use std::os::unix::fs::PermissionsExt;

// ------------------------------------------------------------- the rows

fn facts(at_path: Standing, recorded: RecordAtStart) -> StartFacts {
    StartFacts {
        run_lock_held: false,
        cgroup2: true,
        delegated: true,
        at_path,
        recorded,
    }
}

/// 6.1's start rows, transcribed: the run lock first, then the floor's
/// mount and mark, then what stands. Perturbation: any arm changed.
#[test]
fn the_start_step_judges_by_its_rows_in_order() {
    // A2 or A4 with the lock held: AgentRunning at the run lock, before the
    // cgroup is looked at, even where the floor would also refuse.
    let mut held = facts(Standing::Populated, RecordAtStart::None);
    held.run_lock_held = true;
    held.cgroup2 = false;
    assert_eq!(
        start_judgment(held),
        StartAction::Refuse(StartRefusal::RunLockHeld)
    );
    // The floor's mount, then the mark, before any mkdir.
    let mut no_mount = facts(Standing::Absent, RecordAtStart::None);
    no_mount.cgroup2 = false;
    no_mount.delegated = false;
    assert_eq!(
        start_judgment(no_mount),
        StartAction::Refuse(StartRefusal::NoCgroup2)
    );
    let mut undelegated = facts(Standing::Empty, RecordAtStart::None);
    undelegated.delegated = false;
    assert_eq!(
        start_judgment(undelegated),
        StartAction::Refuse(StartRefusal::NotDelegated)
    );
    // A0: made afresh.
    assert_eq!(
        start_judgment(facts(Standing::Absent, RecordAtStart::None)),
        StartAction::Make
    );
    // A1 at the path, or where a record names one: removed and remade.
    assert_eq!(
        start_judgment(facts(Standing::Empty, RecordAtStart::None)),
        StartAction::RemoveThenMake
    );
    assert_eq!(
        start_judgment(facts(
            Standing::Absent,
            RecordAtStart::Same(Standing::Empty)
        )),
        StartAction::RemoveThenMake
    );
    // A3 (or A4 with the lock free) at either: AgentRunning, killing nothing.
    assert_eq!(
        start_judgment(facts(Standing::Populated, RecordAtStart::None)),
        StartAction::Refuse(StartRefusal::Leftovers)
    );
    assert_eq!(
        start_judgment(facts(
            Standing::Empty,
            RecordAtStart::Same(Standing::Populated)
        )),
        StartAction::Refuse(StartRefusal::Leftovers)
    );
    // Not root's at either: BoundaryUnverified, before leftovers.
    assert_eq!(
        start_judgment(facts(Standing::NotRoots, RecordAtStart::None)),
        StartAction::Refuse(StartRefusal::NotRoots)
    );
    assert_eq!(
        start_judgment(facts(
            Standing::Populated,
            RecordAtStart::Same(Standing::NotRoots)
        )),
        StartAction::Refuse(StartRefusal::NotRoots)
    );
    // A record that does not read, or another cgroup at its path:
    // BoundaryUnverified, acting on nothing, before what stands at the path.
    assert_eq!(
        start_judgment(facts(Standing::Empty, RecordAtStart::Unreadable)),
        StartAction::Refuse(StartRefusal::RecordUnverified)
    );
    assert_eq!(
        start_judgment(facts(Standing::Absent, RecordAtStart::Other)),
        StartAction::Refuse(StartRefusal::RecordUnverified)
    );
    // A record naming a cgroup absent at its path is A0, stale: the path
    // alone is judged.
    assert_eq!(
        start_judgment(facts(Standing::Absent, RecordAtStart::Absent)),
        StartAction::Make
    );
}

/// 6.1's rows for the floor's `cgroup.kill` and the start's `fchmod`,
/// owner and record writes. Perturbation: any arm changed.
#[test]
fn a_failed_step_of_the_making_removes_and_refuses() {
    for (step, answer) in [
        (MadeStep::LayoutMade(true), MadeAction::GoOn),
        (MadeStep::LayoutMade(false), MadeAction::RemoveAndRefuse),
        (MadeStep::KillFilePresent(true), MadeAction::GoOn),
        (MadeStep::ModesSet(true), MadeAction::GoOn),
        (MadeStep::OwnersSet(true), MadeAction::GoOn),
        (MadeStep::RecordWritten(true), MadeAction::GoOn),
        (
            MadeStep::KillFilePresent(false),
            MadeAction::RemoveAndRefuse,
        ),
        (MadeStep::ModesSet(false), MadeAction::RemoveAndRefuse),
        (MadeStep::OwnersSet(false), MadeAction::RemoveAndRefuse),
        (MadeStep::RecordWritten(false), MadeAction::RemoveAndRefuse),
    ] {
        assert_eq!(made_judgment(step), answer, "{step:?}");
    }
}

/// Ownership is the record's, by its run and its identity: a run end ends
/// only the cgroup its record names for the run being concluded, standing at
/// the recorded path as the recorded cgroup. Perturbation: drop either half.
#[test]
fn a_run_end_owns_only_its_records_run_and_cgroup() {
    assert!(owns("run-7", "run-7", RecordedAt::Same));
    assert!(
        !owns("run-8", "run-7", RecordedAt::Same),
        "another load's cgroup is left alone"
    );
    for at in [
        RecordedAt::Other,
        RecordedAt::Absent,
        RecordedAt::Unreadable,
    ] {
        assert!(
            !owns("run-7", "run-7", at),
            "{at:?} is not the recorded cgroup"
        );
    }
}

/// 6.1's rows for a later verb's record. Perturbation: any arm changed.
#[test]
fn a_later_verb_judges_its_record_by_its_rows() {
    assert_eq!(record_judgment(RecordedAt::Same), RecordAction::Proceed);
    assert_eq!(
        record_judgment(RecordedAt::Absent),
        RecordAction::RemoveStaleAndGoOn
    );
    assert_eq!(record_judgment(RecordedAt::Other), RecordAction::Refuse);
    assert_eq!(
        record_judgment(RecordedAt::Unreadable),
        RecordAction::Refuse
    );
}

/// 6.1's run-end rows, each step's success and failure, a rollback's
/// included. Perturbation: any arm changed.
#[test]
fn the_run_end_steps_by_its_rows() {
    for (step, next) in [
        (RunEndStep::Kill { written: true }, RunEndAction::Wait),
        (RunEndStep::Kill { written: false }, RunEndAction::Fallback),
        (RunEndStep::Fallback { emptied: true }, RunEndAction::Remove),
        (
            RunEndStep::Fallback { emptied: false },
            RunEndAction::RefuseWorkerWouldNotExit,
        ),
        (RunEndStep::Wait { emptied: true }, RunEndAction::Remove),
        (
            RunEndStep::Wait { emptied: false },
            RunEndAction::RefuseWorkerWouldNotExit,
        ),
        (
            RunEndStep::Remove { removed: true },
            RunEndAction::RemoveRecord,
        ),
        (
            RunEndStep::Remove { removed: false },
            RunEndAction::RefuseBoundaryUnverified,
        ),
        (
            RunEndStep::RemoveRecord { removed: true },
            RunEndAction::Conclude,
        ),
        (
            RunEndStep::RemoveRecord { removed: false },
            RunEndAction::LogAndConclude,
        ),
    ] {
        assert_eq!(run_end_next(step), next, "{step:?}");
    }
}

// ------------------------------------------------------------- the record

fn run_directory(tag: &str) -> (crate::scratch::Scratch, OwnedFd) {
    let dir = crate::scratch::Scratch(
        std::env::temp_dir().join(format!("weaver-admin-record-{tag}-{}", std::process::id())),
    );
    let _ = std::fs::remove_dir_all(&dir.0);
    std::fs::create_dir_all(&dir.0).unwrap();
    let fd = nix::fcntl::open(
        &dir.0,
        OFlag::O_DIRECTORY | OFlag::O_RDONLY | OFlag::O_CLOEXEC,
        Mode::empty(),
    )
    .unwrap();
    (dir, fd)
}

/// **The record is written atomically and read back**: a temporary left
/// behind is removed, the file is `0600`, a record naming anything but a
/// `weaver-<agent>` directory, or whose path leaves the mount, does not read,
/// and none standing is `None`. Perturbation: drop the name check, or the
/// component check, and the foreign record reads.
#[test]
fn the_record_round_trips_and_refuses_a_foreign_path() {
    let (dir, fd) = run_directory("round");
    assert_eq!(read_record(fd.as_fd(), "alpha"), Ok(None));
    std::fs::write(dir.0.join(RECORD_TEMPORARY), b"torn").unwrap();
    let record = Record {
        path: "/system.slice/admin-con.service/weaver-alpha".into(),
        dev: 42,
        ino: 7,
        run: "run-1".into(),
    };
    write_record(fd.as_fd(), &record).unwrap();
    assert!(
        !dir.0.join(RECORD_TEMPORARY).exists(),
        "the temporary renamed away"
    );
    assert_eq!(
        std::fs::metadata(dir.0.join(RECORD))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert_eq!(read_record(fd.as_fd(), "alpha"), Ok(Some(record.clone())));
    assert_eq!(
        read_record(fd.as_fd(), "beta"),
        Err(()),
        "a record naming another agent's cgroup does not read"
    );
    let escaping = Record {
        path: "/../../tmp/weaver-alpha".into(),
        ..record.clone()
    };
    write_record(fd.as_fd(), &escaping).unwrap();
    assert_eq!(
        read_record(fd.as_fd(), "alpha"),
        Err(()),
        "a record whose path leaves the mount does not read"
    );
    std::fs::write(dir.0.join(RECORD), b"{not json").unwrap();
    assert_eq!(read_record(fd.as_fd(), "alpha"), Err(()));
    remove_record(fd.as_fd()).unwrap();
    remove_record(fd.as_fd()).unwrap();
    assert_eq!(read_record(fd.as_fd(), "alpha"), Ok(None));
}

// ------------------------------------------------------------- the instruments

/// A scratch cgroup beneath the delegated scope this suite runs in, killed
/// and removed when dropped.
struct ScratchCgroup {
    parent: Cgroup,
    name: String,
    cgroup: Cgroup,
}

impl ScratchCgroup {
    fn new(tag: &str) -> Self {
        let own = own_cgroup().expect("/proc/self/cgroup reads");
        let parent = Cgroup::open(&own).expect("the suite's own cgroup opens");
        // The user manager's delegated scope carries no delegate mark of its
        // own (measured 2026-10-14: systemd's system manager marks its
        // delegated units, its user manager does not), so the guard is what
        // the instruments need: a scope this account owns and may write.
        let owner = fstat(parent.fd()).map(|stat| stat.st_uid).ok();
        assert!(
            owner == Some(nix::unistd::getuid().as_raw()) && parent.has_file("cgroup.procs"),
            "the cgroup instruments need a delegated scope this account owns, and {own} is not one: run them as \
             `systemd-run --user --scope -p Delegate=yes cargo test -p weaver-admin --locked -- --ignored containment`"
        );
        let name = format!("weaver-test-{tag}-{}", std::process::id());
        let _ = remove_tree(&parent, &name);
        let cgroup = parent.make_child(&name).expect("a scratch cgroup is made");
        ScratchCgroup {
            parent,
            name,
            cgroup,
        }
    }
}

impl Drop for ScratchCgroup {
    fn drop(&mut self) {
        let _ = self.cgroup.kill();
        let _ = self.cgroup.wait_empty(Duration::from_secs(5));
        let _ = remove_tree(&self.parent, &self.name);
    }
}

/// Removes any tree beneath `parent` deepest first, for the suite's cleanup.
fn remove_tree(parent: &Cgroup, name: &str) -> nix::Result<()> {
    let Ok(root) = parent.child(name) else {
        return Ok(());
    };
    if let Ok(all) = root.subtree() {
        let base = root.relative().to_string();
        let mut relatives: Vec<String> = all
            .iter()
            .map(|cgroup| {
                cgroup
                    .relative()
                    .trim_start_matches(&base)
                    .trim_start_matches('/')
                    .to_string()
            })
            .filter(|relative| !relative.is_empty())
            .collect();
        relatives.sort_by_key(|relative| std::cmp::Reverse(relative.matches('/').count()));
        for relative in relatives {
            let _ = unlinkat(root.fd(), relative.as_str(), UnlinkatFlags::RemoveDir);
        }
    }
    unlinkat(parent.fd(), name, UnlinkatFlags::RemoveDir)
}

/// A process started in `cgroup`: a shell that starts a new session and
/// double-forks a sleeper, then sleeps itself. **The child moves itself in
/// before its exec**, as 6.1's constituents and shells do: admitted after
/// the spawn, the shell could fork its escapers first and leave them outside.
fn escaper_in(cgroup: &Cgroup) -> std::process::Child {
    use std::os::unix::process::CommandExt;
    let procs = cgroup.procs_for_placement().expect("cgroup.procs opens");
    let raw = procs.as_raw_fd();
    let mut command = std::process::Command::new("bash");
    command
        .arg("-c")
        .arg("setsid bash -c '(sleep 300 &) ; sleep 300' & sleep 300");
    // SAFETY: place_self is async-signal-safe.
    unsafe {
        command.pre_exec(move || place_self(raw));
    }
    let child = command.spawn().expect("bash spawns in the cgroup");
    drop(procs);
    let deadline = Instant::now() + Duration::from_secs(5);
    while cgroup.procs().map(|pids| pids.len()).unwrap_or(0) < 4 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    child
}

/// **The floor's mount, the delegate mark and the cgroup's interface** on
/// this host: `cgroup2` at the mount; the user manager's unit, which systemd's
/// system manager delegated, carrying the mark; a cgroup this suite made
/// carrying none; and a scratch cgroup carrying `cgroup.kill`. Perturbation:
/// read the mark as always set, and the scratch cgroup reads delegated;
/// read it as never set, and the user manager's unit reads undelegated.
#[test]
#[ignore = "needs a delegated cgroup scope"]
fn containment_the_floor_reads_on_this_host() {
    assert!(is_cgroup2(), "the mount is cgroup2");
    let scratch = ScratchCgroup::new("floor");
    assert!(scratch.cgroup.has_file("cgroup.kill"), "cgroup.kill stands");
    assert!(
        !delegated(&scratch.cgroup),
        "a cgroup made beneath the scope carries no mark of its own"
    );
    let own = own_cgroup().unwrap();
    let manager = own
        .find("/user@")
        .and_then(|at| {
            own[at..]
                .find(".service")
                .map(|end| &own[..at + end + ".service".len()])
        })
        .expect("the suite runs beneath a user manager's unit");
    assert!(
        delegated(&Cgroup::open(manager).unwrap()),
        "{manager}, a delegated unit, carries the mark"
    );
    assert!(!scratch.cgroup.has_file("no-such-interface-file"));
}

/// **The layout's modes are set by `fchmod`, never by the umask** (6.1,
/// C3's modes): every directory is made `0700` and opened up to `0755`
/// through its descriptor, so a root umask of `077`, which `mkdir` would
/// apply, cannot narrow it, and no test here needs to change the
/// process-wide umask. The three delegated files take the given owner.
/// Perturbation: drop the `fchmod`, and the directories stay `0700`.
#[test]
#[ignore = "needs a delegated cgroup scope"]
fn containment_the_layout_is_0755_by_fchmod() {
    let scratch = ScratchCgroup::new("modes");
    let uid = nix::unistd::getuid().as_raw();
    let gid = nix::unistd::getgid().as_raw();
    make_layout(&scratch.cgroup, uid, gid).expect("the layout is made");
    for inner in LAYOUT_DEEPEST_FIRST {
        let mode = fstat(scratch.cgroup.child(inner).unwrap().fd())
            .unwrap()
            .st_mode
            & 0o777;
        assert_eq!(mode, DIRECTORY_MODE, "{inner}");
    }
    for file in DELEGATED_FILES {
        let stat = nix::sys::stat::fstatat(scratch.cgroup.fd(), file, AtFlags::AT_SYMLINK_NOFOLLOW)
            .unwrap();
        assert_eq!((stat.st_uid, stat.st_gid), (uid, gid), "{file}");
    }
}

/// **The kill reaches a command that left its session**: a shell that
/// starts a new session and double-forks is ended whole by `cgroup.kill`,
/// and the wait reads `populated 0` within the bound. Perturbation: write
/// the kill to another file, and the wait runs out.
#[test]
#[ignore = "needs a delegated cgroup scope"]
fn containment_the_kill_ends_a_double_forked_new_session() {
    let scratch = ScratchCgroup::new("kill");
    let mut child = escaper_in(&scratch.cgroup);
    assert!(
        scratch.cgroup.procs().unwrap().len() >= 3,
        "the escapers stand"
    );
    scratch.cgroup.kill().unwrap();
    assert!(
        scratch.cgroup.wait_empty(CONTAINMENT_BOUND),
        "emptied within the bound"
    );
    let _ = child.wait();
}

/// **The pinned fallback empties the cgroup without its kill file**, each
/// pid confirmed in the cgroup after its pidfd opens. Perturbation: compare
/// against another path, and nothing is signalled.
#[test]
#[ignore = "needs a delegated cgroup scope"]
fn containment_the_pinned_fallback_empties_without_the_kill() {
    let scratch = ScratchCgroup::new("fallback");
    let leaf = scratch.cgroup.make_child("leaf").unwrap();
    let mut child = escaper_in(&leaf);
    assert!(
        leaf.procs().unwrap().len() >= 3,
        "the escapers stand before the fallback"
    );
    assert!(
        scratch.cgroup.fallback_until_empty(Duration::from_secs(5)),
        "emptied"
    );
    let _ = child.wait();
}

/// **A pid not standing in the named cgroup is never signalled**, the check
/// read after the pidfd opens. Perturbation: drop the check, and the
/// process outside dies.
#[test]
#[ignore = "needs a delegated cgroup scope"]
fn containment_a_pid_outside_the_cgroup_is_not_signalled() {
    let scratch = ScratchCgroup::new("pinned");
    let mut outside = std::process::Command::new("sleep")
        .arg("300")
        .spawn()
        .unwrap();
    assert!(!signal_pinned(
        outside.id() as i32,
        scratch.cgroup.relative()
    ));
    assert_eq!(
        outside.try_wait().unwrap(),
        None,
        "the process outside still runs"
    );
    let _ = outside.kill();
    let _ = outside.wait();
}

/// **A failed read of `cgroup.events` is read as not yet empty**, so the
/// wait runs to its bound. Perturbation: read a failed read as empty.
#[test]
#[ignore = "needs a delegated cgroup scope"]
fn containment_a_failed_events_read_runs_the_wait_to_its_bound() {
    let scratch = ScratchCgroup::new("events");
    let leaf = scratch.cgroup.make_child("leaf").unwrap();
    unlinkat(scratch.cgroup.fd(), "leaf", UnlinkatFlags::RemoveDir).unwrap();
    assert_eq!(
        leaf.populated(),
        None,
        "the removed cgroup's events do not read"
    );
    let started = Instant::now();
    assert!(!leaf.wait_empty(Duration::from_millis(200)));
    assert!(started.elapsed() >= Duration::from_millis(200));
}

/// **The removal goes deepest first and reads `ENOENT` as removed**: a
/// layout already partly removed is finished, and a second removal is a
/// no-op. Perturbation: answer `ENOENT` as a failure.
#[test]
#[ignore = "needs a delegated cgroup scope"]
fn containment_the_removal_finishes_a_partial_one() {
    let scratch = ScratchCgroup::new("remove");
    let agent = scratch.cgroup.make_child("weaver-alpha").unwrap();
    let uid = nix::unistd::getuid().as_raw();
    let gid = nix::unistd::getgid().as_raw();
    make_layout(&agent, uid, gid).unwrap();
    unlinkat(agent.fd(), "worker/run", UnlinkatFlags::RemoveDir).unwrap();
    remove_layout(&scratch.cgroup, "weaver-alpha").expect("finished");
    assert!(scratch.cgroup.child("weaver-alpha").is_err(), "gone");
    remove_layout(&scratch.cgroup, "weaver-alpha").expect("a second removal is a no-op");
}

/// **A later verb finds the recorded cgroup by its identity**: the same
/// cgroup reads `Same`, a removed one `Absent`, and one remade at the same
/// path, with another inode, `Other`. Perturbation: compare the path alone.
#[test]
#[ignore = "needs a delegated cgroup scope"]
fn containment_the_record_tells_its_cgroup_by_identity() {
    let scratch = ScratchCgroup::new("identity");
    let agent = scratch.cgroup.make_child("weaver-alpha").unwrap();
    let (dev, ino) = agent.identity().unwrap();
    let record = Record {
        path: agent.relative().to_string(),
        dev,
        ino,
        run: "run-1".into(),
    };
    assert_eq!(recorded_at(&record), RecordedAt::Same);
    drop(agent);
    unlinkat(
        scratch.cgroup.fd(),
        "weaver-alpha",
        UnlinkatFlags::RemoveDir,
    )
    .unwrap();
    assert_eq!(recorded_at(&record), RecordedAt::Absent);
    let remade = scratch.cgroup.make_child("weaver-alpha").unwrap();
    assert_ne!(remade.identity().unwrap(), (dev, ino));
    assert_eq!(recorded_at(&record), RecordedAt::Other);
}
