//! Independent actual admin, worker and normal SQLite member in one circuit.
//! The outer parent owns cleanup and reads the final artifact after both exit.
use super::*;
use std::fs::{self, File};
use std::path::Path;
use std::process::{Child, Command};
use weaver_types::state_flow::enter::{EnterBinding, EnterPayload};

pub(super) fn is_mode(mode: &str) -> bool {
    mode.contains("-forced-save-sqlite-joint")
        || mode.contains("-graceful-save-sqlite-joint")
        || mode.contains("-demand-save-sqlite-joint")
}

pub(super) fn write(root: &Path, name: &str, value: &serde_json::Value) {
    let part = root.join(format!("{name}.part"));
    fs::write(&part, serde_json::to_vec(value).unwrap()).unwrap();
    fs::rename(part, root.join(name)).unwrap();
}

fn wait(root: &Path, name: &str) -> Vec<u8> {
    let end = Instant::now() + Duration::from_secs(25);
    loop {
        match fs::read(root.join(name)) {
            Ok(bytes) => return bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => panic!("{name}: {e}"),
        }
        assert!(Instant::now() < end, "actual admin did not provide {name}");
        std::thread::sleep(Duration::from_millis(1));
    }
}

pub(super) struct Worker {
    root: PathBuf,
    pub(super) socket: PathBuf,
    pub(super) room: PathBuf,
    directory: PathBuf,
    pub(super) lock: Option<File>,
}
impl Worker {
    pub(super) fn begin(root: &Path) -> Self {
        let host: serde_json::Value =
            serde_json::from_slice(&wait(root, "admin-host.json")).unwrap();
        Self {
            root: root.into(),
            socket: host["socket"].as_str().unwrap().into(),
            room: host["room"].as_str().unwrap().into(),
            directory: host["run_directory"].as_str().unwrap().into(),
            lock: None,
        }
    }

    pub(super) fn prepare(&mut self, payload: &mut EnterPayload, image: &[u8]) {
        fs::write(self.root.join("seed.sqlite"), image).unwrap();
        write(
            &self.root,
            "setup.json",
            &serde_json::to_value(&payload).unwrap(),
        );
        let identity: s::StartIdentity =
            serde_json::from_slice(&wait(&self.root, "loaded.json")).unwrap();
        assert_eq!(identity.dump_sha256, normal_member::digest(image));
        let EnterBinding::Serving { start, .. } = &mut payload.binding else {
            panic!("serving");
        };
        *start = identity.clone();
        payload.declaration = identity.config_sha256.to_string();
        let lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(self.directory.join("run.lock"))
            .unwrap();
        let request = nix::libc::flock {
            l_type: nix::libc::F_WRLCK as i16,
            l_whence: nix::libc::SEEK_SET as i16,
            l_start: 0,
            l_len: 0,
            l_pid: 0,
        };
        // SAFETY: the fixture holds this real descriptor through worker exit.
        assert_eq!(
            unsafe { nix::libc::fcntl(lock.as_raw_fd(), nix::libc::F_OFD_SETLK, &request) },
            0
        );
        self.lock = Some(lock);
    }
}

struct Children(Vec<Child>);
impl Drop for Children {
    fn drop(&mut self) {
        for child in &mut self.0 {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let kind = entry.file_type().unwrap();
        if kind.is_dir() {
            copy_tree(&entry.path(), &to.join(entry.file_name()));
        } else if kind.is_file() {
            fs::copy(entry.path(), to.join(entry.file_name())).unwrap();
        }
    }
}

pub(super) fn scenario(name: &str, mode: &str) {
    let layout = crate::tools_cgroup::tests::ScratchLayout::new("joint");
    let root = std::env::temp_dir().join(format!("wj-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let files = Files(root.clone(), true);
    let cgroup = layout.run.strip_suffix("/worker/run").unwrap();
    let agent = cgroup
        .rsplit('/')
        .next()
        .unwrap()
        .strip_prefix("weaver-")
        .unwrap();
    write(
        &root,
        "host.json",
        &serde_json::json!({"cgroup":cgroup,"agent":agent,"mode":mode}),
    );
    let admin = Command::new(
        std::env::var_os("WEAVER_LEAVE_ADMIN_TEST_BIN")
            .expect("build the independent admin test binary"),
    )
    .args([
        "--exact",
        "serving_leave::invocation::tests::joint::forced_leave_joint_actual_admin",
        "--ignored",
        "--nocapture",
    ])
    .env("WEAVER_LEAVE_JOINT_ROOT", &root)
    .spawn()
    .unwrap();
    let mut children = Children(vec![admin]);
    children.0.push(
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", name, "--ignored", "--nocapture"])
            .env("WEAVER_WORKER_STARTUP_CASE", mode)
            .env("WEAVER_GATE_STARTUP_RUN", &layout.run)
            .env("WEAVER_LEAVE_JOINT_ROOT", &root)
            .spawn()
            .unwrap(),
    );
    let end = Instant::now() + Duration::from_secs(40);
    let mut statuses = [None, None];
    while statuses.iter().any(Option::is_none) && Instant::now() < end {
        for (child, status) in children.0.iter_mut().zip(&mut statuses) {
            if status.is_none() {
                *status = child.try_wait().unwrap();
            }
        }
        if statuses.iter().flatten().any(|s| !s.success()) {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    if let Some(path) = std::env::var_os("WEAVER_LEAVE_JOINT_EVIDENCE") {
        copy_tree(&root, &PathBuf::from(path).join(mode));
    }
    assert!(
        statuses.iter().all(|s| s.is_some_and(|s| s.success())),
        "joint admin/worker results: {statuses:?}"
    );
    if mode.ends_with("-bad-release") {
        let failed: serde_json::Value =
            serde_json::from_slice(&fs::read(root.join("failed.json")).unwrap()).unwrap();
        assert!(failed["guarded"].as_bool().unwrap());
        assert!(root.join("damaged-release.json").exists());
        let rows: Vec<serde_json::Value> = fs::read_to_string(root.join("worker-trace.jsonl"))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert!(
            rows.iter()
                .any(|r| r["kind"] == "save_point" && r["payload"]["phase"] == "confirmed")
        );
        assert!(rows.iter().any(|r| r["kind"] == "fault"));
        let fault = rows.iter().find(|r| r["kind"] == "fault").unwrap();
        let unload = rows.iter().find(|r| r["kind"] == "unload").unwrap();
        assert_eq!(unload["payload"]["end_reason"], "component_failure");
        assert_eq!(
            unload["payload"]["state_flow_failure"]["fault_sequence"],
            fault["sequence"]
        );
        assert_eq!(rows.last().unwrap()["kind"], "unload");
        return;
    }
    let concluded: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("concluded.json")).unwrap()).unwrap();
    let report: s::SavePointReport = serde_json::from_value(concluded["report"].clone()).unwrap();
    let image = fs::read(concluded["artifact"].as_str().unwrap()).unwrap();
    let requests = fs::read(root.join("member-requests.jsonl")).unwrap();
    let filings: Vec<s::StateFrame> =
        serde_json::from_slice(&fs::read(root.join("member-filings.json")).unwrap()).unwrap();
    if mode.contains("-graceful-save") {
        super::super::input::generation::session_tests::forced_leave::assert_wind_down_filings(
            &filings,
        );
    }
    normal_member::verify_image(
        &root,
        &image,
        &report,
        &requests,
        &filings,
        usize::from(mode.ends_with("-held-filing") || mode.contains("-demand-save-")),
    );
    let rows: Vec<serde_json::Value> = fs::read_to_string(root.join("worker-trace.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let confirmed = rows
        .iter()
        .find(|r| r["kind"] == "save_point" && r["payload"]["phase"] == "confirmed")
        .unwrap();
    assert_eq!(confirmed["sequence"], report.event_sequence.0.to_string());
    if mode.contains("-demand-save-") {
        assert_eq!(
            rows.iter()
                .filter(|row| row["kind"] == "turn.closed")
                .count(),
            2
        );
        assert!(!rows.iter().any(|row| row["kind"] == "unload"));
        assert_eq!(confirmed["payload"]["cause"], "requested");
        assert!(confirmed["payload"]["tail_window"].is_null());
        return;
    }
    let unload = rows.iter().find(|row| row["kind"] == "unload").unwrap();
    assert_eq!(
        unload["payload"]["forced"],
        !mode.contains("-graceful-save")
    );
    assert_eq!(unload["payload"]["cause"]["uid"], 74);
    assert!(unload["payload"]["forced_by"].is_null());
    if mode.contains("-graceful-save") {
        let close = rows
            .iter()
            .find(|row| row["kind"] == "turn.closed" && row["turn"] == "wind-down-0")
            .unwrap();
        assert_eq!(close["payload"]["close"], "clean");
        let sequence =
            |row: &serde_json::Value| row["sequence"].as_str().unwrap().parse::<u64>().unwrap();
        assert!(sequence(close) < sequence(confirmed));
        assert!(!rows.iter().any(|row| row["kind"] == "message.user"));
    }
    assert_eq!(rows.last().unwrap()["kind"], "unload");
    if let Some(path) = std::env::var_os("WEAVER_LEAVE_JOINT_EVIDENCE") {
        copy_tree(&root, &PathBuf::from(path).join(mode));
    }
    drop(children);
    drop(files);
    drop(layout);
}
