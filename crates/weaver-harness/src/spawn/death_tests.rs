//! Test-only: the `death_tests` suite of `spawn.rs`, moved out of the production file by #118.

use super::*;

/// The variable that turns the helper below into a stand-in worker. Set
/// only by the test that runs this binary again as that stand-in.
const STAND_IN: &str = "WEAVER_HARNESS_ORGAN_PARENT_STAND_IN";

/// **The stand-in worker**: forks one organ through `fork_organ` from
/// this thread, which then lives until the process is killed, writes the
/// organ's pid to the file the variable names, and waits. Run on its own
/// only by the test below; any other run returns at once.
#[test]
fn organ_parent_stand_in() {
    let Some(report) = std::env::var_os(STAND_IN) else {
        return;
    };
    let pid = unsafe {
        fork_organ(
            std::path::Path::new("/bin/sleep"),
            &[],
            &["60".to_string()],
            None,
        )
    }
    .expect("the fork");
    std::fs::write(&report, pid.to_string()).expect("the report");
    std::thread::sleep(std::time::Duration::from_secs(60));
}

/// **The organs die with the worker**, per `weaver-harness-Spec` section
/// 2.2: a stand-in worker forks an organ and is killed with `SIGKILL`,
/// and the organ is gone within the bound, the kernel's death signal
/// having ended it with nothing of the worker's left to run.
///
/// Perturbation: drop the `prctl` call from the fork's child and the
/// organ, a `sleep 60`, outlives its worker.
#[test]
fn an_organ_dies_with_its_worker() {
    let report = std::env::temp_dir().join(format!("weaver-organ-pid-{}", std::process::id()));
    drop(std::fs::remove_file(&report));
    let mut stand_in = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "spawn::death_tests::organ_parent_stand_in",
            "--test-threads=1",
        ])
        .env(STAND_IN, &report)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("the stand-in worker starts");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let organ: i32 = loop {
        if let Ok(text) = std::fs::read_to_string(&report)
            && let Ok(pid) = text.trim().parse()
        {
            break pid;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "no organ pid reported"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    drop(std::fs::remove_file(&report));
    let alive = |pid: i32| {
        // A zombie has died and waits only for its reaper.
        std::fs::read_to_string(format!("/proc/{pid}/stat"))
            .map(|stat| {
                let state = stat.rsplit(')').next().unwrap_or("").trim_start();
                !state.starts_with('Z')
            })
            .unwrap_or(false)
    };
    assert!(alive(organ), "the organ runs while its worker does");
    stand_in.kill().unwrap();
    stand_in.wait().unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while alive(organ) {
        if std::time::Instant::now() > deadline {
            // Best effort: the organ is ended either way before the panic.
            let _ = nix::sys::signal::kill(
                nix::unistd::Pid::from_raw(organ),
                nix::sys::signal::Signal::SIGKILL,
            );
            panic!("the organ outlived its worker");
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}
