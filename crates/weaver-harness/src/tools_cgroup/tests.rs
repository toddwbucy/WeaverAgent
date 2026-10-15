//! The worker's rows of `weaver-admin-Spec` 6.1, each a test, and the
//! primitives' instruments against a real delegated cgroup subtree.
//!
//! **The instruments run in the user manager's delegated scope**: they are
//! `#[ignore]` and run as
//! `systemd-run --user --scope -p Delegate=yes cargo test -p weaver-harness --locked -- --ignored containment`.
//! Each makes a scratch `worker/{run,tools}` beneath the scope it runs in,
//! and outside a delegated scope it fails naming why, never passing by skip.

use super::*;
use std::os::fd::AsRawFd;
use std::path::PathBuf;

const BOUND: Duration = Duration::from_secs(10);

// ------------------------------------------------------------- the rows

/// **The worker's rows, one judgment** (6.1, rows 2677 to 2684): serving, an
/// emptied `tools/` is `Emptied`, a kill delivered past the bound
/// `PastBound`, and the write and the fallback both failing `Undelivered`;
/// inside a leave, a kill delivered by either is `Delivered` and none is
/// `Undelivered`, never waiting. Perturbation: change any arm's answer, and
/// its row fails.
#[test]
fn the_worker_contains_by_its_rows() {
    use Containment::*;
    use Context::*;
    // Serving: the write, then the wait (2677).
    assert_eq!(step(Serving, true, false, true), Emptied);
    // Serving: the write failing, the fallback emptying it (2681).
    assert_eq!(step(Serving, false, true, true), Emptied);
    // Serving: the bound passing with a kill delivered (2680).
    assert_eq!(step(Serving, true, false, false), PastBound);
    assert_eq!(step(Serving, false, true, false), PastBound);
    // Serving: the write and the fallback both failing (2682).
    assert_eq!(step(Serving, false, false, false), Undelivered);
    // A leave and the unwind: delivered by the write or the pass (2678,
    // 2683), never waiting.
    assert_eq!(step(Leave, true, false, false), Delivered);
    assert_eq!(step(Leave, false, true, false), Delivered);
    // Both failing (2679, 2684).
    assert_eq!(step(Leave, false, false, false), Undelivered);
}

/// **`tools/` is found only beside a `worker/run` leaf** (6.1, the layout):
/// any other cgroup the worker starts in names no `tools/`, and the enter
/// refuses. Perturbation: accept any cgroup, and `/a/b` opens `/a/tools`.
#[test]
fn tools_is_found_only_beside_run() {
    for own in ["/a/b", "/a/run", "/worker/run/x", "/"] {
        assert!(
            matches!(
                ToolsCgroup::open_beside_run(own),
                Err(OpenRefused::NotInRun(_))
            ),
            "{own}"
        );
    }
}

/// The unified line, `/` first and no trailing `/`.
#[test]
fn the_unified_line_reads() {
    assert_eq!(
        unified_line("1:name=x:/y\n0::/a/b/\n").as_deref(),
        Some("/a/b")
    );
    assert_eq!(unified_line("0::/\n").as_deref(), Some("/"));
    assert_eq!(unified_line("1:cpu:/a\n"), None);
}

// ------------------------------------------------------------- the instruments

/// A scratch `worker/{run,tools}` beneath the delegated scope this suite runs
/// in, killed and removed when dropped.
pub(crate) struct ScratchLayout {
    root: PathBuf,
    /// `.../worker/run`, as `/proc/self/cgroup` would name it.
    pub(crate) run: String,
}

impl ScratchLayout {
    pub(crate) fn new(tag: &str) -> Self {
        Self::with(tag, true)
    }

    pub(crate) fn with(tag: &str, tools: bool) -> Self {
        let own = own_cgroup().expect("/proc/self/cgroup reads");
        let parent = PathBuf::from(format!("{CGROUP_ROOT}{own}"));
        let owner = std::fs::metadata(&parent).map(|meta| {
            use std::os::unix::fs::MetadataExt;
            meta.uid()
        });
        assert!(
            owner.ok() == Some(nix::unistd::getuid().as_raw())
                && parent.join("cgroup.procs").exists(),
            "the cgroup instruments need a delegated scope this account owns, and {own} is not one: run them as \
             `systemd-run --user --scope -p Delegate=yes cargo test -p weaver-harness --locked -- --ignored containment`"
        );
        let name = format!("weaver-test-{tag}-{}", std::process::id());
        let root = parent.join(&name);
        remove(&root);
        std::fs::create_dir_all(root.join("worker/run")).expect("run/ is made");
        if tools {
            std::fs::create_dir_all(root.join("worker/tools")).expect("tools/ is made");
        }
        Self {
            root,
            run: format!("{own}/{name}/worker/run"),
        }
    }

    /// A leaf's directory beneath the layout's root.
    pub(crate) fn leaf(&self, leaf: &str) -> PathBuf {
        self.root.join(leaf)
    }
}

impl Drop for ScratchLayout {
    fn drop(&mut self) {
        let _ = std::fs::write(self.root.join("cgroup.kill"), b"1");
        let deadline = Instant::now() + Duration::from_secs(5);
        while std::fs::read_to_string(self.root.join("cgroup.events"))
            .is_ok_and(|events| events.contains("populated 1"))
            && Instant::now() < deadline
        {
            std::thread::sleep(WAIT_SLICE);
        }
        remove(&self.root);
    }
}

fn remove(root: &std::path::Path) {
    for leaf in ["worker/run", "worker/tools", "worker", ""] {
        let _ = std::fs::remove_dir(root.join(leaf));
    }
}

/// A shell started in `leaf` that starts a new session and double-forks a
/// sleeper, then sleeps itself. The child moves itself in before its exec.
pub(crate) fn escaper_in(leaf: &std::path::Path) -> std::process::Child {
    use std::os::unix::process::CommandExt;
    let procs = std::fs::OpenOptions::new()
        .write(true)
        .open(leaf.join("cgroup.procs"))
        .expect("cgroup.procs opens");
    let raw = procs.as_raw_fd();
    let mut command = std::process::Command::new("bash");
    command
        .arg("-c")
        .arg("setsid bash -c '(sleep 300 &) ; sleep 300' & sleep 300");
    // SAFETY: one write on a descriptor opened before the fork.
    unsafe {
        command.pre_exec(move || {
            let procs = BorrowedFd::borrow_raw(raw);
            nix::unistd::write(procs, b"0")
                .map(drop)
                .map_err(std::io::Error::from)
        });
    }
    let child = command.spawn().expect("bash spawns in the leaf");
    drop(procs);
    let deadline = Instant::now() + Duration::from_secs(5);
    while listed(leaf) < 4 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    child
}

fn listed(leaf: &std::path::Path) -> usize {
    std::fs::read_to_string(leaf.join("cgroup.procs"))
        .map(|text| text.lines().count())
        .unwrap_or(0)
}

fn alive(child: &mut std::process::Child) -> bool {
    child
        .try_wait()
        .map(|status| status.is_none())
        .unwrap_or(false)
}

/// **`tools/` opens beside `run/` and is judged at once** (6.1;
/// `weaver-harness-Spec` section 2.2): a directory, with `cgroup.kill` and
/// `cgroup.procs` the worker can write; with no `tools/` beside `run/`, the
/// open refuses. Perturbation: open `run/` in its place, and the relative
/// path names the wrong leaf.
#[test]
#[ignore = "needs a delegated cgroup scope"]
fn containment_tools_opens_beside_run_and_is_judged() {
    let layout = ScratchLayout::new("open");
    let tools = ToolsCgroup::open_beside_run(&layout.run).expect("tools/ opens");
    assert_eq!(
        tools.relative(),
        layout.run.replace("/worker/run", "/worker/tools")
    );
    assert_eq!(tools.populated(), Some(false));
    let bare = ScratchLayout::with("open-bare", false);
    assert!(matches!(
        ToolsCgroup::open_beside_run(&bare.run),
        Err(OpenRefused::Open(_))
    ));
}

/// **A `tools/` the worker cannot kill through refuses at the open** (6.1,
/// the worker's row for `tools/` not opened or judged): `cgroup.kill` closed
/// to the worker's account refuses naming it, so no gate stands over a
/// containment the worker could not deliver. Perturbation: drop the judgment
/// of the files, and the open stands.
#[test]
#[ignore = "needs a delegated cgroup scope"]
fn containment_an_unwritable_kill_refuses_the_open() {
    use std::os::unix::fs::PermissionsExt;
    let layout = ScratchLayout::new("unwritable");
    let kill = layout.leaf("worker/tools/cgroup.kill");
    std::fs::set_permissions(&kill, std::fs::Permissions::from_mode(0o444)).unwrap();
    let opened = ToolsCgroup::open_beside_run(&layout.run);
    std::fs::set_permissions(&kill, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(
        matches!(opened, Err(OpenRefused::File("cgroup.kill", _))),
        "{opened:?}"
    );
}

/// **Serving, the worker's kill empties `tools/` within the bound** (6.1, row
/// 2677; C1, the gate's death): a command that left its session and
/// double-forked is ended whole. Perturbation: drop the kill's write, and
/// the bound passes with the escapers standing.
#[test]
#[ignore = "needs a delegated cgroup scope"]
fn containment_serving_the_kill_empties_tools() {
    let layout = ScratchLayout::new("serving");
    let tools = ToolsCgroup::open_beside_run(&layout.run).unwrap();
    let mut shell = escaper_in(&layout.leaf("worker/tools"));
    assert!(
        listed(&layout.leaf("worker/tools")) >= 3,
        "the escapers stand"
    );
    assert_eq!(
        contain(&tools, Context::Serving, BOUND),
        Containment::Emptied
    );
    assert_eq!(tools.populated(), Some(false));
    let _ = shell.wait();
}

/// **Inside a leave the worker kills and does not wait** (6.1, rows 2678 and
/// 2683): `cgroup.events` closed to the worker, so any wait would read
/// "not yet empty" to its bound, the containment still answers `Delivered`
/// at once, and `tools/` empties on the kill alone. Perturbation: wait the
/// bound inside a leave, and the answer takes the whole bound.
#[test]
#[ignore = "needs a delegated cgroup scope"]
fn containment_a_leave_kills_without_waiting() {
    use std::os::unix::fs::PermissionsExt;
    let layout = ScratchLayout::new("leave");
    let tools = ToolsCgroup::open_beside_run(&layout.run).unwrap();
    let mut shell = escaper_in(&layout.leaf("worker/tools"));
    let events = layout.leaf("worker/tools/cgroup.events");
    std::fs::set_permissions(&events, std::fs::Permissions::from_mode(0o000)).unwrap();
    let began = Instant::now();
    let contained = contain(&tools, Context::Leave, Duration::from_secs(3));
    let took = began.elapsed();
    std::fs::set_permissions(&events, std::fs::Permissions::from_mode(0o444)).unwrap();
    assert_eq!(contained, Containment::Delivered);
    assert!(took < Duration::from_secs(1), "no wait: {took:?}");
    assert!(tools.wait_empty(BOUND), "the kill alone empties it");
    let _ = shell.wait();
}

/// **Where the write of `cgroup.kill` fails, the pinned fallback empties
/// `tools/`** (6.1, row 2681): `cgroup.kill` closed after the open, each
/// pid is pinned, confirmed and killed, and the serving containment answers
/// `Emptied`. Perturbation: skip the fallback, and the containment answers
/// `Undelivered`.
#[test]
#[ignore = "needs a delegated cgroup scope"]
fn containment_the_pinned_fallback_empties_tools() {
    use std::os::unix::fs::PermissionsExt;
    let layout = ScratchLayout::new("fallback");
    let tools = ToolsCgroup::open_beside_run(&layout.run).unwrap();
    let mut shell = escaper_in(&layout.leaf("worker/tools"));
    let kill = layout.leaf("worker/tools/cgroup.kill");
    std::fs::set_permissions(&kill, std::fs::Permissions::from_mode(0o444)).unwrap();
    let contained = contain(&tools, Context::Serving, BOUND);
    std::fs::set_permissions(&kill, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(contained, Containment::Emptied);
    let _ = shell.wait();
}

/// **A pid outside `tools/` is never signalled** (6.1, the pinned fallback):
/// a process in `run/`, its pid handed to the pinned signal, is confirmed
/// outside by `/proc/<pid>/cgroup` after the pidfd's open and runs on.
/// Perturbation: drop the confirmation, and the process dies.
#[test]
#[ignore = "needs a delegated cgroup scope"]
fn containment_a_pid_outside_tools_is_never_signalled() {
    let layout = ScratchLayout::new("outside");
    let tools = ToolsCgroup::open_beside_run(&layout.run).unwrap();
    let mut outside = escaper_in(&layout.leaf("worker/run"));
    assert!(!signal_pinned(outside.id() as i32, tools.relative()));
    std::thread::sleep(Duration::from_millis(100));
    assert!(alive(&mut outside), "the process in run/ runs on");
    let _ = outside.kill();
    let _ = outside.wait();
}

/// **A worker started in `run/` opens `tools/` from its own cgroup** (6.1;
/// `weaver-harness-Spec` section 2.2): the suite's own binary, re-run as a
/// stand-in moved into `run/` before its exec, reads `/proc/self/cgroup`
/// and opens the `tools/` beside it. Perturbation: read another leaf's
/// sibling, and the stand-in reports a refusal.
#[test]
#[ignore = "needs a delegated cgroup scope"]
fn containment_a_worker_in_run_opens_tools_from_its_own_cgroup() {
    use std::os::unix::process::CommandExt;
    let layout = ScratchLayout::new("own");
    let report = std::env::temp_dir().join(format!("weaver-tools-own-{}", std::process::id()));
    let _ = std::fs::remove_file(&report);
    let procs = std::fs::OpenOptions::new()
        .write(true)
        .open(layout.leaf("worker/run/cgroup.procs"))
        .unwrap();
    let raw = procs.as_raw_fd();
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "tools_cgroup::tests::stand_in_opens_from_own",
            "--ignored",
            "--nocapture",
        ])
        .env(STAND_IN, &report);
    // SAFETY: one write on a descriptor opened before the fork.
    unsafe {
        command.pre_exec(move || {
            nix::unistd::write(BorrowedFd::borrow_raw(raw), b"0")
                .map(drop)
                .map_err(std::io::Error::from)
        });
    }
    let status = command.status().expect("the stand-in runs");
    drop(procs);
    assert!(status.success());
    let reported = std::fs::read_to_string(&report).unwrap_or_default();
    let _ = std::fs::remove_file(&report);
    assert_eq!(
        reported,
        layout.run.replace("/worker/run", "/worker/tools"),
        "the stand-in opened the tools/ beside its run/"
    );
}

const STAND_IN: &str = "WEAVER_TOOLS_STAND_IN";

/// The stand-in the test above runs: it does nothing unless the test named
/// it, and then reports what it opened from its own cgroup.
#[test]
#[ignore = "a stand-in, run by containment_a_worker_in_run_opens_tools_from_its_own_cgroup"]
fn stand_in_opens_from_own() {
    let Some(report) = std::env::var_os(STAND_IN) else {
        return;
    };
    let answer = match ToolsCgroup::open_from_own() {
        Ok(tools) => tools.relative().to_string(),
        Err(refused) => format!("refused: {refused:?}"),
    };
    std::fs::write(report, answer).unwrap();
}
