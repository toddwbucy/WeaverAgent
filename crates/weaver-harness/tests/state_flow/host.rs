use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant};

fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

pub(super) fn organ(root: &Path, kind: &str) -> PathBuf {
    let path = root.join(kind);
    fs::write(&path, format!(
        "#!/bin/sh\nexport WEAVER_EXTERNAL_STATE_ORGAN={}\nexec {} --exact state_flow_fixture_organ --nocapture\n",
        quote(kind), quote(std::env::current_exe().unwrap().to_str().unwrap()),
    )).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
    path
}

struct Watched(Child);
impl Watched {
    fn run(mut command: Command) {
        let mut child = Self(command.spawn().expect("start external circuit process"));
        let end = Instant::now() + Duration::from_secs(45);
        let status = loop {
            if let Some(status) = child.0.try_wait().unwrap() {
                break status;
            }
            assert!(
                Instant::now() < end,
                "external circuit process exceeded watchdog"
            );
            std::thread::sleep(Duration::from_millis(10));
        };
        assert!(
            status.success(),
            "external circuit process failed: {status}"
        );
    }
}
impl Drop for Watched {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct Layout {
    files: PathBuf,
    cgroup: PathBuf,
    test: String,
}
impl Layout {
    fn new(test: &str) -> Self {
        assert!(
            nix::unistd::getuid().is_root(),
            "mapped root is required for the real admin credential check"
        );
        let own = fs::read_to_string("/proc/self/cgroup").unwrap();
        let own = own
            .lines()
            .find_map(|line| line.strip_prefix("0::"))
            .unwrap();
        let cgroup = PathBuf::from("/sys/fs/cgroup")
            .join(own.trim_start_matches('/'))
            .join(format!("weaver-state-circuit-{}", std::process::id()));
        let files = std::env::temp_dir().join(format!("wsc-{}", std::process::id()));
        fs::create_dir(&files).unwrap();
        let layout = Self {
            files,
            cgroup,
            test: test.into(),
        };
        fs::create_dir(&layout.cgroup).expect("requires a writable delegated cgroup scope");
        fs::create_dir(layout.cgroup.join("worker")).unwrap();
        fs::create_dir(layout.cgroup.join("worker/run")).unwrap();
        fs::create_dir(layout.cgroup.join("worker/tools")).unwrap();
        layout
    }

    fn finish(self) {
        let events = fs::read_to_string(self.cgroup.join("cgroup.events")).unwrap();
        assert!(
            events.lines().any(|line| line == "populated 0"),
            "runtime left a child in the fixture cgroup: {events}"
        );
        if let Some(destination) = std::env::var_os("WEAVER_EXTERNAL_STATE_EVIDENCE") {
            copy(&self.files, &PathBuf::from(destination).join(&self.test)).unwrap();
        }
        for suffix in ["worker/run", "worker/tools", "worker", ""] {
            fs::remove_dir(self.cgroup.join(suffix)).unwrap();
        }
        fs::remove_dir_all(&self.files).unwrap();
        assert!(!self.cgroup.exists() && !self.files.exists());
    }
}

fn copy(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_dir() {
            copy(&entry.path(), &to.join(entry.file_name()))?;
        } else if kind.is_file() {
            fs::copy(entry.path(), to.join(entry.file_name()))?;
        }
    }
    Ok(())
}

impl Drop for Layout {
    fn drop(&mut self) {
        let _ = fs::write(self.cgroup.join("cgroup.kill"), b"1");
        let end = Instant::now() + Duration::from_secs(5);
        while fs::read_to_string(self.cgroup.join("cgroup.events"))
            .is_ok_and(|s| s.contains("populated 1"))
            && Instant::now() < end
        {
            std::thread::sleep(Duration::from_millis(5));
        }
        if self.files.exists()
            && let Some(destination) = std::env::var_os("WEAVER_EXTERNAL_STATE_EVIDENCE")
            && let Err(error) = copy(&self.files, &PathBuf::from(destination).join(&self.test))
        {
            eprintln!("external evidence copy failed: {error}");
        }
        for suffix in ["worker/run", "worker/tools", "worker", ""] {
            let _ = fs::remove_dir(self.cgroup.join(suffix));
        }
        let _ = fs::remove_dir_all(&self.files);
    }
}

pub(super) fn scenario(name: &str, case: super::Case, circuit: fn(&Path, super::Case)) {
    let mode = std::env::var("WEAVER_EXTERNAL_STATE_PROCESS").ok();
    if mode.as_deref() == Some("worker") {
        let root = PathBuf::from(std::env::var_os("WEAVER_EXTERNAL_STATE_ROOT").unwrap());
        let run = PathBuf::from(std::env::var_os("WEAVER_EXTERNAL_STATE_RUN").unwrap());
        fs::write(run.join("cgroup.procs"), std::process::id().to_string()).unwrap();
        circuit(&root, case);
    } else if mode.as_deref() == Some("parent") {
        let layout = Layout::new(name);
        let mut child = Command::new(std::env::current_exe().unwrap());
        child
            .args(["--exact", name, "--nocapture", "--test-threads=1"])
            .env("WEAVER_EXTERNAL_STATE_PROCESS", "worker")
            .env("WEAVER_EXTERNAL_STATE_ROOT", &layout.files)
            .env(
                "WEAVER_EXTERNAL_STATE_RUN",
                layout.cgroup.join("worker/run"),
            )
            .env(
                "WEAVER_EXTERNAL_STATE_HELD",
                if case.has_input() { "yes" } else { "no" },
            );
        Watched::run(child);
        layout.finish();
    } else {
        // No sudo or production service is touched. Scope creation and namespace
        // support are test prerequisites; their failure is an instrument failure.
        let mut command = Command::new("systemd-run");
        command
            .args([
                "--user",
                "--quiet",
                "--scope",
                "-p",
                "Delegate=yes",
                "unshare",
                "--map-root-user",
            ])
            .arg(std::env::current_exe().unwrap())
            .args(["--exact", name, "--nocapture", "--test-threads=1"])
            .env("WEAVER_EXTERNAL_STATE_PROCESS", "parent");
        Watched::run(command);
    }
}
