//! **The worker starts the loop**, per `weaver-harness-Spec` section 2.5: the
//! loop arm forked by the organ fork as it stands, through the real
//! `weaver-loop-launch` on the real interpreter, `loop-runner/runner.py` or a
//! probe standing as it, and stopped as every leave stops it.
//!
//! **Each case runs in a child process inside `unshare --map-root-user`**,
//! where the suite's scratch files are root's, as the launcher asks, and where
//! the fork's own `PR_SET_PDEATHSIG` is armed after the namespace is entered,
//! so the parent-death case measures the organ fork's arming and not one a
//! wrapper laid. The case's standard error is the worker's, where the last
//! word copies the launcher's and the runner's lines. The grant is computed by
//! `deploy/loop-grant.sh`, the stack's own computation. No test reaches a
//! privileged program, and none skips: a box with no user namespace fails.

use std::io::{BufReader, Read as _};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};
use weaver_harness::{LoopArm, LoopLaunch, Seat, StartRefused, TurnError, TurnOutcome};
use weaver_types::{TurnContext, TurnKey, WiresElection};

const LAUNCHER: &str = env!("CARGO_BIN_EXE_weaver-loop-launch");
const RUNNER_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../loop-runner");
const GRANT_SCRIPT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../deploy/loop-grant.sh");
const CROSS_WIRES: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/loop-wires-cross.py"
);

/// The variable naming the case a child process runs.
const CASE: &str = "WEAVER_LOOP_ARM_CASE";
/// The variable naming the scratch layout the case runs over.
const ROOT: &str = "WEAVER_LOOP_ARM_ROOT";
/// The bound on one case: a case that hangs is killed and fails here.
const CASE_BOUND: Duration = Duration::from_secs(60);

/// **A probe standing as `runner.py`**: it writes what it holds on standard
/// error as one JSON line, sends the `hello` with its wire file's digest, then
/// reads its end until the end of file and exits 0.
const PROBE: &str = r#"
import hashlib, json, os, socket, stat, sys
fds = []
for fd in range(64):
    try:
        os.fstat(fd)
        fds.append(fd)
    except OSError:
        pass
null = [fd for fd in (0, 1) if stat.S_ISCHR(os.fstat(fd).st_mode) and os.fstat(fd).st_rdev == os.makedev(1, 3)]
r = {"probe": "held", "fds": fds, "env": dict(os.environ), "null_at": null,
     "socket_at_3": stat.S_ISSOCK(os.fstat(3).st_mode)}
sys.stderr.write(json.dumps(r) + "\n")
sys.stderr.flush()
digest = hashlib.sha256(open(sys.argv[1], "rb").read()).hexdigest()
s = socket.socket(fileno=3)
s.send(json.dumps({"kind": "hello", "version": "weaver-loop/1", "wires_sha256": digest},
                  separators=(",", ":")).encode())
while s.recv(65536):
    pass
"#;

/// **A probe that names another digest and never reads its end**: the relay
/// refuses its `hello`, and only the kill ends it.
const STUCK: &str = r#"
import json, socket, sys, time
sys.stderr.write(json.dumps({"loop": "start", "reason": "the stuck probe"}) + "\n")
sys.stderr.flush()
s = socket.socket(fileno=3)
s.send(json.dumps({"kind": "hello", "version": "weaver-loop/1", "wires_sha256": "0" * 64},
                  separators=(",", ":")).encode())
time.sleep(3600)
"#;

/// **A probe that sends a good `hello` and never reads its end again**, so
/// neither the end of file nor the relay ends it: only a signal does.
const DEAF: &str = r#"
import hashlib, json, socket, sys, time
digest = hashlib.sha256(open(sys.argv[1], "rb").read()).hexdigest()
s = socket.socket(fileno=3)
s.send(json.dumps({"kind": "hello", "version": "weaver-loop/1", "wires_sha256": digest},
                  separators=(",", ":")).encode())
time.sleep(3600)
"#;

/// How long the `settles` case holds the tee before freeing it.
const TEE_HELD: Duration = Duration::from_millis(100);

/// A scratch layout as the stack and the agent's folder lay it.
struct Layout {
    root: PathBuf,
}

impl Layout {
    /// The runner's folder with `runner` standing as `runner.py` beside the
    /// board, an empty pycache folder, the loop folder holding `wires`, and
    /// the grant the stack computes.
    fn new(tag: &str, runner: Option<&str>, wires: &[u8]) -> Layout {
        let root =
            std::env::temp_dir().join(format!("weaver-loop-arm-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for dir in ["lib/weaver-loop", "pycache", "loop"] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
        }
        let folder = root.join("lib/weaver-loop");
        match runner {
            Some(text) => std::fs::write(folder.join("runner.py"), text).unwrap(),
            None => {
                std::fs::copy(
                    Path::new(RUNNER_DIR).join("runner.py"),
                    folder.join("runner.py"),
                )
                .unwrap();
            }
        }
        std::fs::copy(
            Path::new(RUNNER_DIR).join("weaver_board.py"),
            folder.join("weaver_board.py"),
        )
        .unwrap();
        std::fs::write(root.join("loop/wires.py"), wires).unwrap();
        for (path, mode) in [
            (root.clone(), 0o755),
            (folder.clone(), 0o755),
            (root.join("pycache"), 0o755),
            (folder.join("runner.py"), 0o644),
            (folder.join("weaver_board.py"), 0o644),
        ] {
            set_mode(&path, mode);
        }
        let layout = Layout { root };
        let grant = Command::new("bash")
            .args([
                "-c",
                r#"source "$1"; loop_grant "$(command -v "$2")" "$3""#,
                "loop-grant",
                GRANT_SCRIPT,
                "python3",
            ])
            .arg(layout.root.join("pycache"))
            .output()
            .unwrap();
        assert!(
            grant.status.success(),
            "the stack's grant: {}",
            String::from_utf8_lossy(&grant.stderr)
        );
        std::fs::write(layout.grant(), &grant.stdout).unwrap();
        set_mode(&layout.grant(), 0o644);
        layout
    }

    fn grant(&self) -> PathBuf {
        self.root.join("lib/weaver-loop/grant")
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

fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// What a case's child said: its status, standard output and standard error.
struct Ran {
    success: bool,
    stdout: String,
    stderr: String,
}

/// **Runs `case` over `layout` in a child process inside the namespace**,
/// bounded: a case that hangs is killed and fails at the bound, naming what
/// it had said. `during` is handed the child's pid and its standard output as
/// it arrives, for the case that must act on the child while it runs.
fn run_case(
    case: &str,
    layout: &Layout,
    during: impl FnOnce(u32, &mut dyn std::io::BufRead),
) -> Ran {
    run_case_within(case, layout, CASE_BOUND, during)
}

/// [`run_case`] under `bound` in place of [`CASE_BOUND`].
fn run_case_within(
    case: &str,
    layout: &Layout,
    bound: Duration,
    during: impl FnOnce(u32, &mut dyn std::io::BufRead),
) -> Ran {
    let exe = std::env::current_exe().unwrap();
    let mut child = Command::new("unshare")
        .arg("--map-root-user")
        .arg(&exe)
        .args([
            "--exact",
            "a_case_in_the_namespace",
            "--include-ignored",
            "--nocapture",
        ])
        .env(CASE, case)
        .env(ROOT, &layout.root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("unshare runs: the suite needs a user namespace and never skips");
    let pid = child.id();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut stderr = child.stderr.take().unwrap();
    let errors = std::thread::spawn(move || {
        let mut held = String::new();
        let _ = stderr.read_to_string(&mut held);
        held
    });
    // **The bound holds through `during`**: its clock starts before it, and a
    // watchdog kills the child at the bound, which ends a read `during` would
    // otherwise block on forever where the case stalls before saying what it
    // waits for. The child is shared, so the kill never reaches a pid that
    // was reaped and reused.
    let started = Instant::now();
    let child = std::sync::Arc::new(std::sync::Mutex::new(child));
    let (finished, watched) = std::sync::mpsc::channel::<()>();
    let watchdog = {
        let child = std::sync::Arc::clone(&child);
        std::thread::spawn(move || {
            let timed_out = matches!(
                watched.recv_timeout(bound),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout)
            );
            if timed_out {
                let _ = child.lock().unwrap().kill();
            }
            timed_out
        })
    };
    let during_ran =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| during(pid, &mut stdout)));
    let outs = std::thread::spawn(move || {
        let mut held = String::new();
        let _ = stdout.read_to_string(&mut held);
        held
    });
    // A `during` that panicked leaves a child no one waits on: it is killed
    // and reaped at once, and the panic carried on below.
    if during_ran.is_err() {
        let _ = child.lock().unwrap().kill();
    }
    let status = loop {
        if let Some(status) = child.lock().unwrap().try_wait().unwrap() {
            break status;
        }
        if started.elapsed() > bound {
            let mut child = child.lock().unwrap();
            let _ = child.kill();
            break child.wait().unwrap();
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let _ = finished.send(());
    let timed_out = watchdog.join().unwrap() || started.elapsed() > bound;
    if timed_out {
        panic!(
            "the case {case} did not end within {bound:?} and was killed\nstderr:\n{}",
            errors.join().unwrap()
        );
    }
    if let Err(panicked) = during_ran {
        std::panic::resume_unwind(panicked);
    }
    Ran {
        success: status.success(),
        stdout: outs.join().unwrap(),
        stderr: errors.join().unwrap(),
    }
}

/// How many descriptors this process holds.
fn open_descriptors() -> usize {
    std::fs::read_dir("/proc/self/fd").unwrap().count()
}

/// The JSON lines of `text` whose member `key` is `value`.
fn lines_where(text: &str, key: &str, value: &str) -> Vec<serde_json::Value> {
    text.lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line.trim()).ok())
        .filter(|line| line[key] == value)
        .collect()
}

/// **A seat that records each turn's text** and answers it with a refusal
/// this suite names, which the relay never makes, so a pass that answers it
/// shows the loop's `turn` reached the seat and came back unchanged.
#[derive(Default)]
struct Recording {
    turned: Vec<String>,
}

impl Seat for Recording {
    fn first_turn(&self) -> bool {
        true
    }
    fn seated(&self) -> bool {
        true
    }
    fn turn_context(&self) -> TurnContext {
        TurnContext::Continuous
    }
    fn fullness(&self) -> Option<(u64, u64)> {
        None
    }
    fn flush(&mut self, _keep: u64) -> Option<(u64, u64)> {
        None
    }
    fn recall(&mut self, _last_turns: Option<u64>) -> Option<Vec<weaver_harness::Recalled>> {
        None
    }
    fn session_shape(&mut self) -> Option<weaver_harness::SessionShape> {
        None
    }
    fn turn(&mut self, delta: Vec<weaver_traits::Message>) -> Result<TurnOutcome, TurnError> {
        for message in delta {
            for block in message.content {
                if let weaver_traits::ContentBlock::Text { text } = block {
                    self.turned.push(text);
                }
            }
        }
        Err(TurnError::Unlicensed {
            turn: TurnKey("t-recorded".into()),
        })
    }
}

/// **The case bodies**, run only inside the namespace by [`run_case`]; ignored
/// in a plain run, where it has no case to run.
#[test]
#[ignore = "a case body, run inside the namespace by the tests below"]
fn a_case_in_the_namespace() {
    let case = std::env::var(CASE).expect("run by run_case");
    let root = PathBuf::from(std::env::var(ROOT).expect("run by run_case"));
    let launch = LoopLaunch {
        launcher: PathBuf::from(LAUNCHER),
        grant: root.join("lib/weaver-loop/grant"),
    };
    let wires_path = root.join("loop/wires.py");
    let election = WiresElection {
        file: wires_path.to_str().unwrap().to_owned(),
        sha256: digest(&std::fs::read(&wires_path).unwrap()),
    };
    let before = open_descriptors();
    // SAFETY: the case runs on this test's thread for the whole of its arm's
    // life: every arm is stopped, or the process ends, before the thread does.
    let started = unsafe { LoopArm::start(&launch, &election) };
    match case.as_str() {
        "serve" => {
            let mut arm = started.expect("the loop starts");
            // **The worker holds the near end and the last word's reader, and
            // nothing more**: its copies of the far end and of the pipe's
            // write end dropped with the fork.
            assert_eq!(
                open_descriptors() - before,
                2,
                "the start leaves the worker two descriptors"
            );
            let mut seat = Recording::default();
            let answered = arm.pass(&mut seat, "hello");
            assert!(
                matches!(&answered, Err(TurnError::Unlicensed { turn }) if turn.0 == "t-recorded"),
                "the seat's answer comes back unchanged: {answered:?}"
            );
            assert_eq!(seat.turned, ["hello, seated True"]);
            // **A child forked since holds a copy of the near end**, as a fork
            // the worker makes later would until its exec: the stop's shutdown
            // stands at once all the same, where a close alone would wait on
            // the copy.
            // SAFETY: the child calls sleep and _exit alone, both
            // async-signal-safe.
            let holder = match unsafe { nix::unistd::fork() }.expect("fork") {
                nix::unistd::ForkResult::Child => unsafe {
                    nix::libc::sleep(20);
                    nix::libc::_exit(0)
                },
                nix::unistd::ForkResult::Parent { child } => child,
            };
            let stopping = Instant::now();
            let status = arm.stop(Some(Instant::now() + Duration::from_secs(30)));
            let _ = nix::sys::signal::kill(holder, nix::sys::signal::Signal::SIGKILL);
            let _ = nix::sys::wait::waitpid(holder, None);
            assert!(
                matches!(status, Some(nix::sys::wait::WaitStatus::Exited(_, 0))),
                "the runner exits 0 on its end of file: {status:?}"
            );
            assert!(
                stopping.elapsed() < Duration::from_secs(5),
                "the runner went at its end of file, not at the deadline: {:?}",
                stopping.elapsed()
            );
        }
        "probe" => {
            let arm = started.expect("the probe's hello is accepted");
            let status = arm.stop(Some(Instant::now() + Duration::from_secs(30)));
            assert!(
                matches!(status, Some(nix::sys::wait::WaitStatus::Exited(_, 0))),
                "{status:?}"
            );
        }
        "stuck" => {
            let refused = started
                .err()
                .expect("a hello naming another digest refuses");
            assert!(
                matches!(&refused, StartRefused::Hello { reason, .. } if reason.contains("digest")),
                "{refused:?}"
            );
            // **Killed and reaped at once**: this process forked nothing else,
            // so no child of it remains, a zombie included.
            assert_eq!(
                nix::sys::wait::waitpid(
                    nix::unistd::Pid::from_raw(-1),
                    Some(nix::sys::wait::WaitPidFlag::WNOHANG)
                ),
                Err(nix::errno::Errno::ECHILD),
                "the refused child is reaped before the start returns"
            );
        }
        "launcher-refuses" => {
            let refused = started.err().expect("a stale grant refuses the launch");
            match refused {
                StartRefused::Hello {
                    last_word: Some(line),
                    ..
                } => {
                    let said: serde_json::Value = serde_json::from_str(&line).unwrap();
                    assert_eq!(said["loop"], "launch", "{line}");
                }
                other => panic!("the launcher's line is the last word: {other:?}"),
            }
        }
        "settles" => {
            use nix::libc;
            let mut arm = started.expect("the loop starts");
            let mut seat = Recording::default();
            let _ = arm.pass(&mut seat, "hello");
            // **The worker's standard error onto a full pipe**, so the tee
            // that carries the runner's end line blocks on it until a drainer
            // frees the pipe, `TEE_HELD` from now.
            // SAFETY: plain descriptor calls on numbers this process owns,
            // checked as they return.
            let (saved, read_end, filled) = unsafe {
                let saved = libc::dup(2);
                assert!(saved >= 0, "dup");
                let mut ends = [0; 2];
                assert_eq!(
                    libc::pipe2(ends.as_mut_ptr(), libc::O_CLOEXEC | libc::O_NONBLOCK),
                    0
                );
                let mut filled = 0usize;
                while libc::write(ends[1], b".".as_ptr().cast(), 1) == 1 {
                    filled += 1;
                }
                assert_eq!(libc::fcntl(ends[1], libc::F_SETFL, 0), 0);
                assert_eq!(libc::dup2(ends[1], 2), 2);
                libc::close(ends[1]);
                (saved, ends[0], filled)
            };
            let drainer = std::thread::spawn(move || {
                std::thread::sleep(TEE_HELD);
                let mut sink = [0u8; 4096];
                let mut drained = 0;
                while drained < filled {
                    // SAFETY: the read end is this process's and outlives the
                    // thread, which is joined before it closes.
                    let n = unsafe {
                        libc::read(
                            read_end,
                            sink.as_mut_ptr().cast(),
                            sink.len().min(filled - drained),
                        )
                    };
                    if n > 0 {
                        drained += n as usize;
                    } else {
                        std::thread::sleep(Duration::from_millis(1));
                    }
                }
            });
            let stopping = Instant::now();
            let status = arm.stop(Some(Instant::now() + Duration::from_secs(30)));
            let stopped_after = stopping.elapsed();
            drainer.join().unwrap();
            let mut after = Vec::new();
            let mut chunk = [0u8; 4096];
            // SAFETY: as above, then the worker's own standard error put back.
            unsafe {
                loop {
                    let n = libc::read(read_end, chunk.as_mut_ptr().cast(), chunk.len());
                    if n <= 0 {
                        break;
                    }
                    after.extend_from_slice(&chunk[..n as usize]);
                }
                libc::dup2(saved, 2);
                libc::close(saved);
                libc::close(read_end);
            }
            let after = String::from_utf8_lossy(&after);
            assert!(
                matches!(status, Some(nix::sys::wait::WaitStatus::Exited(_, 0))),
                "{status:?}"
            );
            assert!(
                stopped_after >= TEE_HELD,
                "the stop returned in {stopped_after:?}, before the tee holding the runner's end line was freed"
            );
            assert_eq!(
                lines_where(&after, "loop", "end").len(),
                1,
                "the runner's end line went through the tee: {after}"
            );
        }
        "dies-with-worker" => {
            let arm = started.expect("the loop starts");
            println!("{}", serde_json::json!({ "runner": arm.pid().as_raw() }));
            std::thread::sleep(Duration::from_secs(50));
            panic!("the parent kills this process before here");
        }
        // **Says nothing and does not end**, for the runner's own bound.
        "silent" => {
            drop(started);
            std::thread::sleep(Duration::from_secs(45));
        }
        other => panic!("no case {other}"),
    }
}

/// **A started loop serves its pass and stops on its end of file**: the real
/// runner, the cross test's wire file, the `hello` read, one pass whose `turn`
/// reaches the seat and whose answer comes back unchanged, then the stop,
/// where the runner exits 0 well inside the deadline, and its end-of-file line
/// naming the one pass and the one turn reaches the worker's standard error
/// through the last word. The worker holds two new descriptors after the
/// start, and a child forked before the stop holding a copy of the near end
/// does not delay it. Perturbation: drop the relay's shutdown, and the stop
/// waits on the copy; keep the worker's copy of the far end or of the pipe's
/// write end, and it holds three.
#[test]
fn a_started_loop_serves_its_pass_and_stops_on_its_end_of_file() {
    let wires = std::fs::read(CROSS_WIRES).unwrap();
    let layout = Layout::new("serve", None, &wires);
    let ran = run_case("serve", &layout, |_, _| {});
    assert!(
        ran.success,
        "the case failed\nstdout:\n{}\nstderr:\n{}",
        ran.stdout, ran.stderr
    );
    let ended = lines_where(&ran.stderr, "loop", "end");
    assert_eq!(ended.len(), 1, "one end line\nstderr:\n{}", ran.stderr);
    assert_eq!(
        (&ended[0]["passes"], &ended[0]["turns"]),
        (&serde_json::json!(1), &serde_json::json!(1))
    );
}

/// **The runner holds descriptors 0 to 3 and one variable under the worker's
/// fork**: the organ fork as the worker calls it, from a process holding a
/// descriptor born without close-on-exec and its whole environment, and the
/// probe sees the null device at 0 and 1, the last word's pipe at 2, the
/// loop's end at 3, nothing else, and `LANG=C.UTF-8` alone. Perturbation:
/// place the far end anywhere but 3, and the probe never reaches its `hello`.
#[test]
fn the_runner_holds_descriptors_0_to_3_and_the_cleared_environment_under_the_worker_fork() {
    let layout = Layout::new("probe", Some(PROBE), b"the wire file\n");
    // A descriptor born without close-on-exec, which the case inherits
    // across unshare's exec and holds when it forks.
    let leak = std::fs::File::open(&layout.root).unwrap();
    nix::fcntl::fcntl(
        &leak,
        nix::fcntl::FcntlArg::F_SETFD(nix::fcntl::FdFlag::empty()),
    )
    .unwrap();
    let ran = run_case("probe", &layout, |_, _| {});
    drop(leak);
    assert!(ran.success, "the case failed\nstderr:\n{}", ran.stderr);
    let held = lines_where(&ran.stderr, "probe", "held");
    assert_eq!(held.len(), 1, "the probe's line\nstderr:\n{}", ran.stderr);
    assert_eq!(held[0]["fds"], serde_json::json!([0, 1, 2, 3]));
    assert_eq!(held[0]["null_at"], serde_json::json!([0, 1]));
    assert_eq!(held[0]["socket_at_3"], serde_json::json!(true));
    assert_eq!(held[0]["env"], serde_json::json!({"LANG": "C.UTF-8"}));
}

/// **A case's bound holds through `during`** (review of #655): a case that
/// says nothing while `during` waits on its standard output is killed at the
/// bound and fails naming it, never hanging the suite for as long as the case
/// stalls. Perturbation: start the bound's clock after `during`, or drop the
/// watchdog, and the run waits out the case's 45 seconds.
#[test]
fn a_case_silent_while_during_reads_is_killed_at_its_bound() {
    let wires = std::fs::read(CROSS_WIRES).unwrap();
    let layout = Layout::new("silent", None, &wires);
    let begun = Instant::now();
    let ran = std::panic::catch_unwind(|| {
        // `during` waits on a line the case never says, past the lines the
        // test harness itself prints, until the standard output ends.
        run_case_within("silent", &layout, Duration::from_secs(3), |_, stdout| {
            let mut line = String::new();
            while stdout.read_line(&mut line).is_ok_and(|n| n > 0) {
                assert!(!line.contains("never said"), "the case said nothing");
                line.clear();
            }
        })
    });
    let panicked = match ran {
        Ok(_) => panic!("a silent case cannot pass its bound"),
        Err(panicked) => panicked,
    };
    let said = panicked
        .downcast_ref::<String>()
        .cloned()
        .unwrap_or_default();
    assert!(said.contains("did not end within"), "{said}");
    assert!(
        begun.elapsed() < Duration::from_secs(30),
        "the bound held through during: {:?}",
        begun.elapsed()
    );
}

/// **A `hello` naming another digest refuses the start, and the runner is
/// killed and reaped at once**: the probe sends a wrong digest and never reads
/// its end again, so only the kill ends it. The relay's reason names the
/// digest, and no child of the worker remains when the start returns.
/// Perturbation: drop the kill, and the reap waits on a runner that never
/// exits, and the case is killed at its bound.
#[test]
fn a_hello_naming_another_digest_kills_and_reaps_the_runner() {
    let layout = Layout::new("stuck", Some(STUCK), b"the wire file\n");
    let ran = run_case("stuck", &layout, |_, _| {});
    assert!(ran.success, "the case failed\nstderr:\n{}", ran.stderr);
    assert!(
        ran.stderr
            .contains("the loop relay refused the loop's hello"),
        "the relay names the refusal on the worker's standard error\n{}",
        ran.stderr
    );
}

/// **A launch the launcher refuses refuses the start, its line the last
/// word**: a grant naming another interpreter digest ends the launcher before
/// its exec with its one typed line, and the start's refusal carries it.
#[test]
fn a_launch_the_launcher_refuses_refuses_the_start_with_its_line() {
    let wires = std::fs::read(CROSS_WIRES).unwrap();
    let layout = Layout::new("refuses", None, &wires);
    let grant = std::fs::read_to_string(layout.grant()).unwrap();
    let stale: String = grant
        .lines()
        .map(|line| match line.strip_prefix("interpreter ") {
            Some(rest) => format!("interpreter {}{}\n", "0".repeat(64), &rest[64..]),
            None => format!("{line}\n"),
        })
        .collect();
    std::fs::write(layout.grant(), stale).unwrap();
    let ran = run_case("launcher-refuses", &layout, |_, _| {});
    assert!(ran.success, "the case failed\nstderr:\n{}", ran.stderr);
}

/// **The runner dies with the worker, through the launcher**: the case starts
/// the loop by the organ fork, which arms `PR_SET_PDEATHSIG` to `SIGKILL`
/// inside the namespace, names the runner's pid and waits. The runner sent a
/// good `hello` and never reads its end again, so the end of file the case's
/// death brings does not end it. Killed by `SIGKILL`, the case leaves the
/// runner dead within seconds, through both execs, the launcher's and the
/// interpreter's. Perturbation: drop the fork's arming, and the runner
/// outlives the case.
#[test]
fn the_runner_dies_with_the_worker_through_the_launcher() {
    let layout = Layout::new("dies", Some(DEAF), b"the wire file\n");
    let mut runner = 0;
    let ran = run_case("dies-with-worker", &layout, |pid, stdout| {
        // libtest's own lines come first: the runner's is the first that
        // parses with a `runner` member.
        runner = loop {
            let mut line = String::new();
            assert!(
                stdout.read_line(&mut line).unwrap() > 0,
                "the case ended without naming the runner"
            );
            if let Ok(said) = serde_json::from_str::<serde_json::Value>(line.trim())
                && let Some(pid) = said["runner"].as_i64()
            {
                break pid as i32;
            }
        };
        assert!(alive(runner), "the runner stands before the worker dies");
        nix::sys::signal::kill(
            nix::unistd::Pid::from_raw(pid as i32),
            nix::sys::signal::Signal::SIGKILL,
        )
        .unwrap();
    });
    assert!(!ran.success, "the case was killed");
    let deadline = Instant::now() + Duration::from_secs(10);
    while alive(runner) {
        if Instant::now() >= deadline {
            // Ended here, so a failed run leaves no runner sleeping its hour.
            let _ = nix::sys::signal::kill(
                nix::unistd::Pid::from_raw(runner),
                nix::sys::signal::Signal::SIGKILL,
            );
            panic!("the runner {runner} outlived the worker by 10 s");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// **The stop returns once the runner's end line is through the tee**: the
/// case holds the worker's standard error on a full pipe, so the tee carrying
/// the runner's end line blocks until a drainer frees it `TEE_HELD` later.
/// The runner exits at once on its end of file, and the stop still returns
/// only after the hold, the end line then in the pipe. Perturbation: drop the
/// stop's settle of the last word, and it returns as soon as the runner is
/// reaped, the line still held.
#[test]
fn the_stop_returns_once_the_runners_end_line_is_through_the_tee() {
    let wires = std::fs::read(CROSS_WIRES).unwrap();
    let layout = Layout::new("settles", None, &wires);
    let ran = run_case("settles", &layout, |_, _| {});
    assert!(
        ran.success,
        "the case failed\nstdout:\n{}\nstderr:\n{}",
        ran.stdout, ran.stderr
    );
}

/// Whether `pid` runs: present in `/proc` and not a zombie.
fn alive(pid: i32) -> bool {
    match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(stat) => stat
            .rsplit_once(") ")
            .is_some_and(|(_, rest)| !rest.starts_with('Z')),
        Err(_) => false,
    }
}
