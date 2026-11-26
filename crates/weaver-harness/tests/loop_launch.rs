//! The loop's launcher, `weaver-loop-launch`, run as the worker runs it, per
//! `weaver-harness-Spec` section 2.5: a probe in place of `runner.py`, run
//! under the walls the launcher lays, beside the same probe run with none.
//!
//! **Root is the owner the launcher asks for.** The suite runs the launcher
//! inside `unshare --map-root-user`, where the suite's own scratch files are
//! root's. The grant is computed here as the stack computes it: the resolved
//! interpreter, its loader and libraries by `ldd`, every link resolved, and
//! its standard library. No test reaches a privileged program.

use std::io::{BufRead as _, BufReader};
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::os::unix::process::{CommandExt as _, ExitStatusExt as _};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use nix::libc;
use sha2::{Digest, Sha256};

const LAUNCHER: &str = env!("CARGO_BIN_EXE_weaver-loop-launch");

/// The probe, standing as `runner.py`: it tries each wall and writes one JSON
/// line on standard error, then waits for its end at 3 to close.
const PROBE: &str = r#"
import errno, json, os, resource, socket, stat, sys, threading
OUTSIDE = sys.argv[1].rsplit("/", 2)[0] + "/outside"
r = {}
def attempt(name, fn):
    try:
        fn()
        r[name] = "made"
    except OSError as e:
        r[name] = errno.errorcode.get(e.errno, str(e.errno))
    except Exception as e:
        r[name] = type(e).__name__
def fork():
    if os.fork() == 0:
        os._exit(0)
    os.wait()
def thread():
    t = threading.Thread(target=lambda: None)
    t.start()
    t.join()
attempt("socket", lambda: socket.socket(socket.AF_UNIX).close())
attempt("socketpair", lambda: [s.close() for s in socket.socketpair()])
attempt("read_outside", lambda: open(OUTSIDE).read())
attempt("write_loop", lambda: open(sys.argv[1] + ".new", "w").close())
attempt("write_runner", lambda: open(os.path.dirname(sys.argv[0]) + "/new", "w").close())
attempt("fork", fork)
attempt("thread", thread)
attempt("spawn", lambda: os.waitpid(os.posix_spawn(sys.executable, [sys.executable, "-c", "0"], {}), 0))
attempt("allocate", lambda: bytearray(600 << 20))
attempt("raise_limit", lambda: resource.setrlimit(resource.RLIMIT_NOFILE, (4096, 4096)))
r["read_wires"] = open(sys.argv[1]).read()
r["limits"] = {n: list(resource.getrlimit(getattr(resource, "RLIMIT_" + n))) for n in ("AS", "NOFILE", "FSIZE", "CORE")}
r["env"] = dict(os.environ)
fds = []
for fd in range(64):
    try:
        os.fstat(fd)
        fds.append(fd)
    except OSError:
        pass
r["fds"] = fds
r["null_at"] = [fd for fd in (0, 1) if stat.S_ISCHR(os.fstat(fd).st_mode) and os.fstat(fd).st_rdev == os.makedev(1, 3)]
r["socket_at_3"] = stat.S_ISSOCK(os.fstat(3).st_mode)
r["flags"] = [int(sys.flags.isolated), int(sys.flags.dont_write_bytecode), int(sys.flags.no_site)]
r["pycache_prefix"] = sys.pycache_prefix
sys.stderr.write(json.dumps(r) + "\n")
sys.stderr.flush()
os.read(3, 1)
"#;

/// A scratch layout as the stack and the agent's folder lay it, removed when
/// the test ends.
struct Layout {
    root: PathBuf,
    interpreter: PathBuf,
}

impl Layout {
    fn new(tag: &str) -> Layout {
        let root =
            std::env::temp_dir().join(format!("weaver-loop-launch-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for dir in ["lib/weaver-loop", "pycache", "loop"] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
        }
        set_mode(&root, 0o755);
        std::fs::write(root.join("lib/weaver-loop/runner.py"), PROBE).unwrap();
        std::fs::write(root.join("loop/wires.py"), "the wire file\n").unwrap();
        std::fs::write(root.join("outside"), "outside the grant\n").unwrap();
        let interpreter = std::fs::canonicalize(find_python()).unwrap();
        let layout = Layout { root, interpreter };
        layout.write_grant(&layout.grant_text(None));
        layout
    }

    fn grant_file(&self) -> PathBuf {
        self.root.join("lib/weaver-loop/grant")
    }

    fn wires(&self) -> PathBuf {
        self.root.join("loop/wires.py")
    }

    fn runner(&self) -> PathBuf {
        self.root.join("lib/weaver-loop/runner.py")
    }

    /// **The grant as the stack computes it**, the interpreter's digest
    /// replaced where `digest` names one.
    fn grant_text(&self, digest: Option<&str>) -> String {
        let ldd = Command::new("ldd").arg(&self.interpreter).output().unwrap();
        assert!(
            ldd.status.success(),
            "ldd on {}",
            self.interpreter.display()
        );
        let mut execute = Vec::new();
        let mut read = Vec::new();
        for line in String::from_utf8(ldd.stdout).unwrap().lines() {
            let line = line.trim();
            if let Some((_, rest)) = line.split_once(" => ") {
                let path = rest.split(" (").next().unwrap();
                read.push(std::fs::canonicalize(path).unwrap());
            } else if line.starts_with('/') {
                let path = line.split(" (").next().unwrap();
                execute.push(std::fs::canonicalize(path).unwrap());
            }
        }
        let stdlib = Command::new(&self.interpreter)
            .args([
                "-I",
                "-S",
                "-c",
                "import os; print(os.path.dirname(os.__file__))",
            ])
            .output()
            .unwrap();
        let stdlib =
            std::fs::canonicalize(String::from_utf8(stdlib.stdout).unwrap().trim()).unwrap();
        let sha = match digest {
            Some(d) => d.to_string(),
            None => Sha256::digest(std::fs::read(&self.interpreter).unwrap())
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect(),
        };
        let mut text = format!(
            "weaver-loop-grant 1\ninterpreter {sha} {}\n",
            self.interpreter.display()
        );
        for path in execute {
            text.push_str(&format!("execute {}\n", path.display()));
        }
        for path in read {
            text.push_str(&format!("read {}\n", path.display()));
        }
        text.push_str(&format!("beneath {}\n", stdlib.display()));
        text.push_str(&format!(
            "pycache {}\n",
            self.root.join("pycache").display()
        ));
        text
    }

    fn write_grant(&self, text: &str) {
        std::fs::write(self.grant_file(), text).unwrap();
        set_mode(&self.grant_file(), 0o644);
    }
}

impl Drop for Layout {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn set_mode(path: &Path, mode: u32) {
    std::fs::set_permissions(path, std::os::unix::fs::PermissionsExt::from_mode(mode)).unwrap();
}

fn find_python() -> PathBuf {
    std::env::var_os("PATH")
        .and_then(|paths| {
            std::env::split_paths(&paths)
                .map(|dir| dir.join("python3"))
                .find(|p| p.is_file())
        })
        .expect("the suite runs where python3 stands on PATH")
}

/// How a launch is made.
enum Arm {
    /// The launcher, inside a user namespace where the suite's uid is root.
    Launched,
    /// The launcher with `setpriv --pdeathsig KILL` before it, armed after
    /// the namespace is entered, as the organ fork arms it.
    LaunchedWithParentDeath,
    /// The launcher run as the suite's own uid, with no namespace.
    LaunchedUnmapped,
    /// The probe run by the interpreter directly, with no wall: the control.
    Bare,
}

/// A started launch: the child, its standard error, and the near end of its
/// descriptor 3.
struct Started {
    child: Child,
    stderr: BufReader<std::process::ChildStderr>,
    near: UnixStream,
}

/// **Started as the organ fork starts an organ**: the far end of a pair at 3,
/// standard error a pipe, and here also 0 and 1 pipes, a variable the worker
/// would carry, and a leaked descriptor at 9 born without close-on-exec, so
/// the launcher's own steps show.
fn start(layout: &Layout, arm: Arm, with_end: bool) -> Started {
    let (near, far) = UnixStream::pair().unwrap();
    let far: OwnedFd = far.into();
    let far_fd = far.as_raw_fd();
    let leak = std::fs::File::open(&layout.root).unwrap();
    let leak_fd = leak.as_raw_fd();
    let mut command = match arm {
        Arm::Launched => {
            let mut c = Command::new("unshare");
            c.arg("--map-root-user").arg("--").arg(LAUNCHER);
            c
        }
        Arm::LaunchedWithParentDeath => {
            let mut c = Command::new("unshare");
            c.args(["--map-root-user", "--", "setpriv", "--pdeathsig", "KILL"])
                .arg(LAUNCHER);
            c
        }
        Arm::LaunchedUnmapped => Command::new(LAUNCHER),
        Arm::Bare => {
            let mut c = Command::new(&layout.interpreter);
            c.args(["-I", "-B", "-S"]).arg(layout.runner());
            c
        }
    };
    if !matches!(arm, Arm::Bare) {
        command.arg(layout.grant_file());
    }
    command
        .arg(layout.wires())
        .env("WEAVER_PROBE_VARIABLE", "the worker's")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // **Each placed descriptor has its close-on-exec mark cleared**: `dup2`
    // onto the number a descriptor already holds changes nothing, so a far
    // end that the parent opened at 3 would close at the exec.
    // SAFETY: dup2 and fcntl, async-signal-safe, on descriptors read before
    // the fork.
    unsafe {
        command.pre_exec(move || {
            let place = |from: i32, to: i32| {
                if libc::dup2(from, to) != to || libc::fcntl(to, libc::F_SETFD, 0) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            };
            if with_end {
                place(far_fd, 3)?;
            }
            place(leak_fd, 9)
        });
    }
    let mut child = command.spawn().unwrap();
    drop(far);
    drop(leak);
    let stderr = BufReader::new(child.stderr.take().unwrap());
    Started {
        child,
        stderr,
        near,
    }
}

/// The probe's line, read while the probe waits on its end, with its
/// pid's status and limits as the kernel shows them from outside.
struct Probed {
    said: serde_json::Value,
    status: String,
    limits: String,
}

fn probe(layout: &Layout, arm: Arm) -> Probed {
    let mut started = start(layout, arm, true);
    let mut line = String::new();
    started.stderr.read_line(&mut line).unwrap();
    let pid = started.child.id();
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).unwrap_or_default();
    let limits = std::fs::read_to_string(format!("/proc/{pid}/limits")).unwrap_or_default();
    drop(started.near);
    let exit = started.child.wait().unwrap();
    let said =
        serde_json::from_str(&line).unwrap_or_else(|e| panic!("the probe's line {line:?}: {e}"));
    assert!(
        exit.success(),
        "the probe exits 0 when its end closes: {exit:?}"
    );
    Probed {
        said,
        status,
        limits,
    }
}

/// A launch refused before its exec: its exit and its one line.
///
/// **The near end is dropped before the read**, so a launch that wrongly
/// reaches the probe meets its end of file at once and exits, and the test
/// fails on the probe's line instead of waiting on it.
fn refused(layout: &Layout, arm: Arm, with_end: bool) -> (std::process::ExitStatus, String) {
    let Started {
        mut child,
        mut stderr,
        near,
    } = start(layout, arm, with_end);
    drop(near);
    let mut all = String::new();
    std::io::Read::read_to_string(&mut stderr, &mut all).unwrap();
    let exit = child.wait().unwrap();
    (exit, all)
}

/// The reason of a refusal's one JSON line, `{"loop": "launch", ...}`.
fn reason(all: &str) -> String {
    let lines: Vec<&str> = all.lines().collect();
    assert_eq!(lines.len(), 1, "one line: {all:?}");
    let value: serde_json::Value = serde_json::from_str(lines[0]).expect("a JSON line");
    assert_eq!(value["loop"], "launch", "{all}");
    value["reason"].as_str().expect("a reason").to_string()
}

/// **Each wall refuses its probe, and the same probe with no wall is made.**
/// A socket made, a read outside the grant, a write anywhere, a limit passed
/// and a process made, each refused under the launcher. Perturbation: drop a
/// filter row, a limit or the ruleset's restriction and its row reads "made".
#[test]
fn each_wall_refuses_its_probe_and_the_bare_probe_is_made() {
    let layout = Layout::new("walls");
    let bare = probe(&layout, Arm::Bare).said;
    let walled = probe(&layout, Arm::Launched).said;
    let rows = [
        ("socket", "EACCES"),
        ("socketpair", "EACCES"),
        ("read_outside", "EACCES"),
        ("write_loop", "EACCES"),
        ("write_runner", "EACCES"),
        ("fork", "EACCES"),
        ("thread", "RuntimeError"),
        ("spawn", "EACCES"),
        ("allocate", "MemoryError"),
        ("raise_limit", "ValueError"),
    ];
    for (name, want) in rows {
        assert_eq!(
            bare[name], "made",
            "the control: {name} is made with no wall"
        );
        assert_eq!(walled[name], want, "{name} under the walls");
    }
    assert_eq!(
        walled["read_wires"], "the wire file\n",
        "the loop folder is read"
    );
}

/// **The limits, no-new-privs and the filter, as the kernel shows them.**
#[test]
fn the_limits_and_the_filter_stand_on_the_runner() {
    let layout = Layout::new("limits");
    let walled = probe(&layout, Arm::Launched);
    assert_eq!(
        walled.said["limits"],
        serde_json::json!({
            "AS": [512 << 20, 512 << 20],
            "NOFILE": [64, 64],
            "FSIZE": [64 << 20, 64 << 20],
            "CORE": [0, 0],
        })
    );
    assert!(
        walled.status.contains("NoNewPrivs:\t1"),
        "{}",
        walled.status
    );
    assert!(walled.status.contains("Seccomp:\t2"), "{}", walled.status);
    assert!(
        walled
            .limits
            .contains("Max open files            64                   64"),
        "{}",
        walled.limits
    );
}

/// **The cleared environment, the null device at 0 and 1, and exactly
/// descriptors 0 to 3**: the variable, the pipes at 0 and 1 and the leaked 9
/// that the bare probe holds are gone under the launcher. Perturbation: skip
/// the clear, the null device or the close-on-exec mark, and its row fails.
#[test]
fn the_runner_starts_with_the_cleared_environment_and_descriptors_0_to_3() {
    let layout = Layout::new("start");
    let bare = probe(&layout, Arm::Bare).said;
    assert_eq!(bare["env"]["WEAVER_PROBE_VARIABLE"], "the worker's");
    assert!(
        bare["fds"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!(9))
    );
    assert_eq!(bare["null_at"], serde_json::json!([]));
    let walled = probe(&layout, Arm::Launched).said;
    assert_eq!(walled["env"], serde_json::json!({ "LANG": "C.UTF-8" }));
    assert_eq!(walled["fds"], serde_json::json!([0, 1, 2, 3]));
    assert_eq!(walled["null_at"], serde_json::json!([0, 1]));
    assert_eq!(walled["socket_at_3"], true);
    assert_eq!(walled["flags"], serde_json::json!([1, 1, 1]));
    assert_eq!(
        walled["pycache_prefix"],
        layout.root.join("pycache").display().to_string()
    );
    let left: Vec<_> = std::fs::read_dir(layout.root.join("pycache"))
        .unwrap()
        .collect();
    assert!(left.is_empty(), "no bytecode is written");
}

/// **The stack's grant is the grant this suite computes, and a launch under it
/// serves**: `deploy/loop-grant.sh`'s `loop_grant`, run as the stack's scripts
/// run it on the layout's interpreter and pycache folder, writes the same text,
/// and the probe runs walled under it. Perturbation: drop the loader's line or
/// leave a link unresolved in the script, and the texts differ.
#[test]
fn the_stacks_grant_is_the_suites_and_a_launch_under_it_serves() {
    let layout = Layout::new("stack");
    let script = concat!(env!("CARGO_MANIFEST_DIR"), "/../../deploy/loop-grant.sh");
    let out = Command::new("bash")
        .args([
            "-c",
            "set -euo pipefail; source \"$1\"; loop_grant \"$2\" \"$3\"",
            "x",
            script,
        ])
        .arg(&layout.interpreter)
        .arg(layout.root.join("pycache"))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8(out.stdout).unwrap();
    assert_eq!(text, layout.grant_text(None));
    layout.write_grant(&text);
    let walled = probe(&layout, Arm::Launched).said;
    assert_eq!(walled["socket"], "EACCES");
    assert_eq!(walled["read_wires"], "the wire file\n");
}

/// **A stale digest ends the launcher before its exec**, one JSON line naming
/// the interpreter, and no probe runs. Perturbation: skip the comparison.
#[test]
fn a_stale_digest_ends_the_launch_before_its_exec() {
    let layout = Layout::new("stale");
    layout.write_grant(&layout.grant_text(Some(&"0".repeat(64))));
    let (exit, all) = refused(&layout, Arm::Launched, true);
    assert_eq!(exit.code(), Some(1));
    let why = reason(&all);
    assert!(why.contains("not the grant's"), "{why}");
    assert!(
        why.contains(&layout.interpreter.display().to_string()),
        "{why}"
    );
}

/// A row per refusal of the launch, each one JSON line and exit 1 before any
/// exec: a path gone, the runner's folder not root's, no end at 3, the wire
/// file relative, and the grant file absent.
#[test]
fn each_refused_launch_says_one_line_and_runs_nothing() {
    let layout = Layout::new("refusals");
    let good = layout.grant_text(None);

    let gone = layout.root.join("lib/libgone.so");
    layout.write_grant(&good.replace("pycache ", &format!("read {}\npycache ", gone.display())));
    let (exit, all) = refused(&layout, Arm::Launched, true);
    assert_eq!(exit.code(), Some(1));
    let why = reason(&all);
    assert!(
        why.contains(&format!("{} does not open", gone.display())),
        "{why}"
    );
    layout.write_grant(&good);

    let (exit, all) = refused(&layout, Arm::LaunchedUnmapped, true);
    assert_eq!(exit.code(), Some(1));
    let why = reason(&all);
    assert!(
        why.contains("the grant file") && why.contains(", not 0"),
        "{why}"
    );

    let (exit, all) = refused(&layout, Arm::Launched, false);
    assert_eq!(exit.code(), Some(1));
    assert!(
        reason(&all).contains("descriptor 3, the loop's end"),
        "{all}"
    );

    let out = Command::new(LAUNCHER)
        .arg(layout.grant_file())
        .arg("loop/wires.py")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(reason(&String::from_utf8(out.stderr).unwrap()).contains("is not absolute"));

    std::fs::remove_file(layout.grant_file()).unwrap();
    let (exit, all) = refused(&layout, Arm::Launched, true);
    assert_eq!(exit.code(), Some(1));
    assert!(
        reason(&all).contains("the grant file") && all.contains("does not open"),
        "{all}"
    );
}

/// **The parent-death signal survives both execs**, the launcher's and the
/// interpreter's: the runner, armed before the launcher's exec, dies by
/// `SIGKILL` when the thread that forked it ends. The control, not armed,
/// lives on until its end closes. Perturbation: the launcher clearing the
/// signal makes the armed arm live on.
#[test]
fn the_runner_dies_with_its_parent_through_the_launcher() {
    let layout = Layout::new("pdeath");
    for armed in [true, false] {
        let arm = if armed {
            Arm::LaunchedWithParentDeath
        } else {
            Arm::Launched
        };
        let layout_root = layout.root.clone();
        let interpreter = layout.interpreter.clone();
        let (child, near) = std::thread::spawn(move || {
            let layout = Layout {
                root: layout_root,
                interpreter,
            };
            let mut started = start(&layout, arm, true);
            let mut line = String::new();
            started.stderr.read_line(&mut line).unwrap();
            assert!(line.starts_with('{'), "the probe ran: {line:?}");
            std::mem::forget(layout);
            (started.child, started.near)
        })
        .join()
        .unwrap();
        let mut child = child;
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut status = None;
        while Instant::now() < deadline {
            if let Some(s) = child.try_wait().unwrap() {
                status = Some(s);
                break;
            }
            if !armed && Instant::now() + Duration::from_secs(4) > deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        if armed {
            let status = status.unwrap_or_else(|| {
                let _ = child.kill();
                panic!("the armed runner outlived the thread that forked it")
            });
            assert_eq!(status.signal(), Some(libc::SIGKILL));
        } else {
            assert!(status.is_none(), "the control lives on: {status:?}");
            drop(near);
            assert!(child.wait().unwrap().success());
        }
    }
}
