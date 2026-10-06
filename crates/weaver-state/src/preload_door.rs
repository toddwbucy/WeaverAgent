//! conforms: state-serve-restricts-to-the-session
//! conforms: state-replay-answers-at-the-seal
//! conforms: state-preload-door-stands-only-diagnostic
//! Process-level comparison of custody's two doors against an independent record walk.
//!
//! The preload is driven by an in-tree client written to the preload door's
//! wire (an opener, the distillates, the seal), so the suite tests the
//! member and nothing outside this repository. It drove the `weaver-analysis`
//! binary until the operator's ruling of 2026-10-04 put WeaverAnalysis out of
//! this repository's concern.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::os::fd::{AsFd, AsRawFd};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, ExitCode};
use std::time::{Duration, Instant};

use serde_json::{Value, json, value::RawValue};
use weaver_trace::{ElectedKind, Election, Tee};

use weaver_state::Store;
use weaver_state::engine::sqlite::Sqlite;
use weaver_state::save_point::SavePoint;

// This spelling is independent of the distiller. A changed projection must
// fail against it. The message/identity labels describe the asks, not a second kind list.
struct Selection {
    kind: &'static str,
    paths: &'static [&'static str],
    message: bool,
    identity: bool,
    whole: bool,
}
const SELECTION: &[Selection] = &[
    Selection {
        kind: "load",
        paths: &["tee"],
        message: false,
        identity: false,
        whole: false,
    },
    Selection {
        kind: "model.request",
        paths: &["rendered", "template", "sampling"],
        message: false,
        identity: false,
        whole: false,
    },
    Selection {
        kind: "model.measurement",
        paths: &["input_tokens", "output_tokens", "model", "weights_hash"],
        message: false,
        identity: false,
        whole: false,
    },
    Selection {
        kind: "message.system",
        paths: &["role", "content"],
        message: true,
        identity: true,
        whole: true,
    },
    Selection {
        kind: "message.user",
        paths: &["role", "content"],
        message: true,
        identity: false,
        whole: false,
    },
    Selection {
        kind: "message.assistant",
        paths: &["role", "content"],
        message: true,
        identity: false,
        whole: false,
    },
    Selection {
        kind: "message.tool_result",
        paths: &["role", "content"],
        message: true,
        identity: false,
        whole: false,
    },
    // A branch's inherited conversation: served by the recall, distilled
    // whole like the identity, and never answered as identity (#697).
    Selection {
        kind: "message.restored",
        paths: &["role", "content"],
        message: true,
        identity: false,
        whole: true,
    },
];
const SESSION: &str = "s-w5b";
const DESTINATION: &str = "s-e1-diagnostic";
const BRANCH: &str = "s-e1-branch";
const FUTURE: &str = "future.observation";
const FUTURE_PATH: &str = "opaque.unrecognised";
const FOREIGN_SESSION: &str = "s-w5b-foreign";
const CUT: &str = "r-one:2";
const EXCLUDED: &str = "tool.call.started";
const WAIT: Duration = Duration::from_secs(5);

fn election() -> Election {
    Election {
        all_kinds: false,
        keys: SELECTION
            .iter()
            .map(|s| ElectedKind {
                kind: s.kind.into(),
                paths: s.paths.iter().map(|p| (*p).into()).collect(),
            })
            .collect(),
    }
}

fn object(raw: &str) -> BTreeMap<String, Box<RawValue>> {
    serde_json::from_str(raw).expect("canonical JSON object")
}
fn text(raw: &RawValue) -> String {
    serde_json::from_str(raw.get()).expect("canonical string")
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Event {
    envelope: BTreeMap<String, String>,
    pairs: BTreeMap<String, String>,
}

// Enumerate the raw tree first, independently of either producer's path walk.
// Values remain raw slices, including null, strings, arrays and objects.
fn raw_tree(prefix: &str, raw: &RawValue, out: &mut BTreeMap<String, String>) {
    if !prefix.is_empty() {
        out.insert(prefix.into(), raw.get().into());
    }
    if let Ok(members) = serde_json::from_str::<BTreeMap<String, Box<RawValue>>>(raw.get()) {
        for (key, value) in members {
            let path = if prefix.is_empty() {
                key
            } else {
                format!("{prefix}.{key}")
            };
            raw_tree(&path, &value, out);
        }
    }
}

fn expected(lines: &[String], rule: &Election, destination: &str) -> Vec<Event> {
    lines
        .iter()
        .filter_map(|line| {
            let members = object(line);
            let kind = text(&members["kind"]);
            let system = SELECTION.iter().any(|s| s.whole && s.kind == kind)
                && !members.contains_key("turn");
            let selected = rule.keys.iter().find(|s| s.kind == kind);
            if !system && !rule.all_kinds && selected.is_none() {
                return None;
            }
            let mut envelope = BTreeMap::new();
            for name in ["session", "run", "turn", "kind", "sequence"] {
                if let Some(value) = members.get(name) {
                    envelope.insert(name.into(), text(value));
                }
            }
            envelope.insert("session".into(), destination.into());
            let pairs = if system {
                // Enumerate the whole top level, not elected dotted paths.
                members
                    .get("payload")
                    .map(|raw| {
                        object(raw.get())
                            .into_iter()
                            .map(|(key, raw)| (key, raw.get().to_string()))
                            .collect()
                    })
                    .unwrap_or_default()
            } else {
                let mut tree = BTreeMap::new();
                if let Some(payload) = members.get("payload") {
                    raw_tree("", payload, &mut tree);
                }
                selected
                    .into_iter()
                    .flat_map(|entry| &entry.paths)
                    .filter_map(|path| match tree.get(path) {
                        Some(value) => Some((path.clone(), value.clone())),
                        None => {
                            println!("E1 MISSING sequence={} path={path}", envelope["sequence"]);
                            None
                        }
                    })
                    .collect()
            };
            Some(Event { envelope, pairs })
        })
        .collect()
}

// One comparator for every event answer: missing is not null or an empty
// string, and object/number spellings are not normalized to manufacture parity.
fn answer_events(frame: &str, ask: &str) -> Vec<Event> {
    let top = object(frame);
    let answer = object(top["answer"].get());
    let body = object(answer[ask].get());
    let key = if ask == "identity" {
        "messages"
    } else {
        "events"
    };
    let events: Vec<Box<RawValue>> = serde_json::from_str(body[key].get()).unwrap();
    events
        .into_iter()
        .map(|raw| {
            let event = object(raw.get());
            Event {
                envelope: object(event["envelope"].get())
                    .into_iter()
                    .map(|(k, v)| (k, text(&v)))
                    .collect(),
                pairs: object(event["pairs"].get())
                    .into_iter()
                    .map(|(k, v)| (k, v.get().into()))
                    .collect(),
            }
        })
        .collect()
}

fn shape(events: &[Event]) -> Vec<(String, BTreeMap<String, usize>)> {
    let mut runs: Vec<(String, BTreeMap<String, usize>)> = Vec::new();
    for event in events {
        let run = &event.envelope["run"];
        let at = match runs.iter().position(|(name, _)| name == run) {
            Some(at) => at,
            None => {
                runs.push((run.clone(), BTreeMap::new()));
                runs.len() - 1
            }
        };
        *runs[at]
            .1
            .entry(event.envelope["kind"].clone())
            .or_default() += 1;
    }
    runs
}
fn answer_shape(frame: &str) -> Vec<(String, BTreeMap<String, usize>)> {
    let value: Value = serde_json::from_str(frame).unwrap();
    value["answer"]["shape"]["runs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|run| {
            (
                run["run"].as_str().unwrap().into(),
                serde_json::from_value(run["kinds"].clone()).unwrap(),
            )
        })
        .collect()
}
fn subset(events: &[Event], ask: &str, last: Option<usize>) -> Vec<Event> {
    if ask == "replay" {
        return events.to_vec();
    }
    let identity = |event: &&Event| {
        SELECTION
            .iter()
            .any(|s| s.identity && s.kind == event.envelope["kind"])
            && !event.envelope.contains_key("turn")
    };
    if ask == "identity" {
        let run = events
            .iter()
            .rev()
            .find(identity)
            .map(|e| &e.envelope["run"]);
        return events
            .iter()
            .filter(identity)
            .filter(|e| Some(&e.envelope["run"]) == run)
            .cloned()
            .collect();
    }
    let mut turns = Vec::new();
    for event in events {
        if let Some(turn) = event.envelope.get("turn") {
            let key = (event.envelope["run"].clone(), turn.clone());
            turns.retain(|held| held != &key);
            turns.push(key);
        }
    }
    let admitted: BTreeSet<_> = turns
        .into_iter()
        .rev()
        .take(last.unwrap_or(usize::MAX))
        .collect();
    events
        .iter()
        .filter(|event| {
            SELECTION
                .iter()
                .any(|s| s.message && s.kind == event.envelope["kind"])
                && (last.is_none()
                    || event.envelope.get("turn").is_some_and(|turn| {
                        admitted.contains(&(event.envelope["run"].clone(), turn.clone()))
                    }))
        })
        .cloned()
        .collect()
}

struct Record {
    lines: Vec<String>,
    cut: usize,
}
impl Record {
    // Both collisions and a later distinct run/turn matter: the outer row
    // filters and the identity/last-turn selectors must each stay in session.
    fn foreign_lines(&self) -> Vec<String> {
        [false, true]
            .into_iter()
            .flat_map(|distinct| {
                self.lines.iter().map(move |line| {
                    let mut row = object(line);
                    row.insert(
                        "session".into(),
                        serde_json::value::to_raw_value(FOREIGN_SESSION).unwrap(),
                    );
                    if distinct {
                        for name in ["run", "turn"] {
                            if let Some(value) = row.get_mut(name) {
                                *value = serde_json::value::to_raw_value(&format!(
                                    "{}-foreign",
                                    text(value)
                                ))
                                .unwrap();
                            }
                        }
                    }
                    serde_json::to_string(&row).unwrap() + "\n"
                })
            })
            .collect()
    }

    // Only the live input's envelope is renamed. Raw payload values remain
    // the recorded bytes, on the live path and the preload path alike.
    fn destination_lines(&self, destination: &str) -> Vec<String> {
        self.lines
            .iter()
            .map(|line| {
                let mut row = object(line);
                row.insert(
                    "session".into(),
                    serde_json::value::to_raw_value(destination).unwrap(),
                );
                serde_json::to_string(&row).unwrap() + "\n"
            })
            .collect()
    }

    // Read what the canonical record names, independently of the binary.
    fn rule(&self) -> Election {
        let load = object(&self.lines[0]);
        let payload = object(load["payload"].get());
        let rule = object(payload[SELECTION[0].paths[0]].get());
        let keys: Vec<Value> = serde_json::from_str(rule["keys"].get()).unwrap();
        Election {
            all_kinds: serde_json::from_str(rule["all_kinds"].get()).unwrap(),
            keys: keys
                .into_iter()
                .map(|key| ElectedKind {
                    kind: key["kind"].as_str().unwrap().into(),
                    paths: key["paths"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|p| p.as_str().unwrap().into())
                        .collect(),
                })
                .collect(),
        }
    }

    fn new() -> Self {
        Self::under(&Election::default())
    }

    fn under(rule: &Election) -> Self {
        let rule = json!({"all_kinds":rule.all_kinds,"keys":rule.keys.iter()
            .map(|entry| json!({"kind":entry.kind,"paths":entry.paths})).collect::<Vec<_>>()});
        let load = json!({"tee":rule}).to_string();
        let mut lines = Vec::new();
        let mut add = |run: &str, turn: Option<&str>, kind: &str, payload: &str| {
            let n = lines.len();
            let turn = turn
                .map(|t| format!(",\"turn\":{}", json!(t)))
                .unwrap_or_default();
            lines.push(format!("{{\"session\":{},\"run\":{}{turn},\"kind\":{},\"sequence\":\"{n}\",\"subsystem\":\"harness\",\"wall_ms\":1,\"monotonic_ns\":\"{n}\",\"payload\":{payload}}}\n",json!(SESSION),json!(run),json!(kind)));
        };
        for (run, turns) in [("r-one", 2), ("r-two", 1)] {
            add(run, None, SELECTION[0].kind, &load);
            add(
                run,
                None,
                SELECTION[3].kind,
                &format!(
                    r#"{{"content":[{{"text":"prefix {run}","type":"text"}}],"role":"system","unknown":{{"z":1.00, "a":1e3}},"null":null,"empty":""}}"#
                ),
            );
            // The run's inherited conversation, recorded at its open as a
            // branch's is, so the recall and the preload are held to it.
            add(
                run,
                None,
                SELECTION[7].kind,
                &format!(
                    r#"{{"content":[{{"text":"inherited {run}","type":"text"}}],"role":"user"}}"#
                ),
            );
            for index in 1..=turns {
                let turn = format!("t-{index}");
                add(run, Some(&turn), "turn.started", "{}");
                add(
                    run,
                    Some(&turn),
                    SELECTION[4].kind,
                    r#"{"content":"question","role":"user"}"#,
                );
                let template = if run == "r-two" {
                    r#", "template":"""#
                } else if index == 2 {
                    r#", "template":null"#
                } else {
                    ""
                };
                // Raw model payloads can preserve object declaration order.
                // The expectation must not normalize this elected value.
                add(
                    run,
                    Some(&turn),
                    SELECTION[1].kind,
                    &format!(
                        r#"{{"rendered":"prompt","sampling":{{"temperature":0.0,"seed":17}}{template}}}"#
                    ),
                );
                add(
                    run,
                    Some(&turn),
                    SELECTION[2].kind,
                    r#"{"input_tokens":[1,2],"model":"fixture","output_tokens":[3],"weights_hash":"fixture-hash"}"#,
                );
                add(
                    run,
                    Some(&turn),
                    SELECTION[5].kind,
                    r#"{"content":"answer","role":"assistant"}"#,
                );
                if run == "r-one" && index == 1 {
                    add(run, Some(&turn), EXCLUDED, "{}");
                    add(
                        run,
                        Some(&turn),
                        SELECTION[6].kind,
                        r#"{"content":"tool answer","role":"tool_result"}"#,
                    );
                }
                add(
                    run,
                    Some(&turn),
                    FUTURE,
                    r#"{"opaque":{"unrecognised":{"z":1.00, "a":1e3}}}"#,
                );
                add(run, Some(&turn), "turn.closed", r#"{"close":"clean"}"#);
            }
        }
        let cut = lines
            .iter()
            .position(|line| {
                let row: Value = serde_json::from_str(line).unwrap();
                row["run"] == "r-one" && row["turn"] == "t-2" && row["kind"] == "turn.closed"
            })
            .unwrap()
            + 1;
        // Every line is admitted by the producer's envelope parser, including
        // excluded kinds. No imported fixture or projection is an expectation.
        for line in &lines {
            assert!(weaver_trace::distill(line, &Election::default()).is_some());
        }
        Self { lines, cut }
    }
}

static NEXT_DIR: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let at = std::env::temp_dir().join(format!(
            "w5b-{}-{}",
            std::process::id(),
            NEXT_DIR.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir(&at).unwrap();
        Self(at)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
struct Process(Child);
impl Process {
    fn wait(&mut self) -> std::process::ExitStatus {
        let until = Instant::now() + WAIT;
        loop {
            if let Some(status) = self.0.try_wait().unwrap() {
                return status;
            }
            assert!(Instant::now() < until, "child did not exit within {WAIT:?}");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        if !matches!(self.0.try_wait(), Ok(Some(_))) {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}

// The child calls precisely the parameterized production entry. It is neither
// a second server nor a library call to the store.
fn child_entry() {
    if let Ok(vector) = std::env::var("WEAVER_W5B_MEMBER_VECTOR") {
        let args: Vec<String> = serde_json::from_str(&vector).unwrap();
        let fd = std::env::var("WEAVER_W5B_MEMBER_FD")
            .unwrap()
            .parse()
            .unwrap();
        let save_point_fd = std::env::var("WEAVER_W5B_SAVE_POINT_FD")
            .unwrap()
            .parse()
            .unwrap();
        let result = super::member_entry(args.into_iter(), fd, save_point_fd);
        std::process::exit(if result == ExitCode::SUCCESS { 0 } else { 1 });
    }
    assert!(
        nix::unistd::getuid().is_root(),
        "preload requires the operator credential; run these instruments with unshare --map-root-user (no sudo)"
    );
}
struct Member {
    process: Process,
    tee: Option<Tee>,
    wire: UnixStream,
    buffer: Vec<u8>,
    // The member's room, removed with it after Drop stops and reaps the
    // member: its save points live here and nothing else of the store does.
    directory: Directory,
}
impl Member {
    fn new(election: Election, diagnostic: bool, destination: &str) -> Self {
        Self::new_with(election, diagnostic, destination, None)
    }
    /// A member handed a save point at its spawn, the way admin hands one,
    /// at the fixed descriptor the production entry reads.
    fn new_with(
        election: Election,
        diagnostic: bool,
        destination: &str,
        save_point: Option<&std::path::Path>,
    ) -> Self {
        let mut member = Self::spawn_with(diagnostic, save_point);
        member.tee = Some(
            Tee::open(
                member.wire.try_clone().unwrap(),
                destination.into(),
                election,
            )
            .unwrap(),
        );
        // A shape answer proves the production entry reached serve, and an
        // empty one that nothing was restored.
        let shape = answer_shape(&member.ask("shape", None));
        if save_point.is_none() {
            assert!(shape.is_empty());
        }
        assert_eq!(
            member.door().exists(),
            diagnostic,
            "door follows the load's mode"
        );
        member
    }
    // Raw-open tests must reach the same entry before a tee supplies its opener.
    fn spawn_with(diagnostic: bool, save_point: Option<&std::path::Path>) -> Self {
        let thread = std::thread::current();
        let test = thread.name().expect("named test thread");
        let directory = Directory::new();
        let door = directory.0.join("preload.sock");
        let (wire, child) = UnixStream::pair().unwrap();
        let fd = child.as_raw_fd();
        // The save point's descriptor where one is handed, and a number
        // holding nothing where none is, which is an agent's first load.
        let handed =
            save_point.map(|path| std::fs::File::open(path).expect("the save point opens"));
        let save_point_fd = handed.as_ref().map_or(900, |file| file.as_raw_fd());
        let mut args: Vec<String> = vec![directory.0.to_str().unwrap().into()];
        if diagnostic {
            args.push(door.to_str().unwrap().into());
        }
        let log = std::fs::File::create(directory.0.join("member.log")).unwrap();
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", test, "--ignored", "--nocapture"])
            .env(
                "WEAVER_W5B_MEMBER_VECTOR",
                serde_json::to_string(&args).unwrap(),
            )
            .env("WEAVER_W5B_MEMBER_FD", fd.to_string())
            .env("WEAVER_W5B_SAVE_POINT_FD", save_point_fd.to_string())
            .stdout(log.try_clone().unwrap())
            .stderr(log);
        let inherit = handed.is_some();
        // SAFETY: the child owns this socket and, where one is handed, the
        // save point's descriptor. Only descriptor flags are set between
        // fork and exec, and the test entry adopts each exactly once.
        unsafe {
            command.pre_exec(move || {
                let clear = |raw| {
                    let borrowed = std::os::fd::BorrowedFd::borrow_raw(raw);
                    nix::fcntl::fcntl(
                        borrowed,
                        nix::fcntl::FcntlArg::F_SETFD(nix::fcntl::FdFlag::empty()),
                    )
                    .map(|_| ())
                    .map_err(std::io::Error::from)
                };
                clear(fd)?;
                if inherit {
                    clear(save_point_fd)?;
                }
                Ok(())
            });
        }
        let process = Process(command.spawn().unwrap());
        drop(child);
        drop(handed);
        Self {
            process,
            tee: None,
            wire,
            buffer: Vec::new(),
            directory,
        }
    }
    fn log(&self) -> String {
        std::fs::read_to_string(self.directory.0.join("member.log")).unwrap_or_default()
    }
    fn door(&self) -> PathBuf {
        self.directory.0.join("preload.sock")
    }
    fn send(&mut self, frame: &str) {
        let until = Instant::now() + WAIT;
        let mut bytes = frame.as_bytes();
        while !bytes.is_empty() {
            assert!(Instant::now() < until, "harness write exceeded {WAIT:?}");
            match self.wire.write(bytes) {
                Ok(0) => panic!("member closed during harness write"),
                Ok(n) => bytes = &bytes[n..],
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    let mut fds = [nix::poll::PollFd::new(
                        self.wire.as_fd(),
                        nix::poll::PollFlags::POLLOUT,
                    )];
                    let bound = nix::poll::PollTimeout::try_from(
                        until.saturating_duration_since(Instant::now()),
                    )
                    .unwrap();
                    match nix::poll::poll(&mut fds, bound) {
                        Ok(_) | Err(nix::errno::Errno::EINTR) => {}
                        Err(e) => panic!("harness write poll: {e}"),
                    }
                }
                Err(e) => panic!("harness write: {e}"),
            }
        }
    }
    fn connect_preload(&self) -> UnixStream {
        let until = Instant::now() + WAIT;
        loop {
            match UnixStream::connect(self.door()) {
                Ok(stream) => return stream,
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
                    ) && Instant::now() < until =>
                {
                    std::thread::sleep(Duration::from_millis(5))
                }
                Err(e) => panic!("preload connect: {e}; {}", self.log()),
            }
        }
    }
    fn receive(&mut self, timeout: Duration) -> Option<String> {
        let until = Instant::now() + timeout;
        loop {
            if let Some(end) = self.buffer.iter().position(|b| *b == b'\n') {
                let frame: Vec<_> = self.buffer.drain(..=end).collect();
                return Some(String::from_utf8(frame).unwrap());
            }
            let remaining = until.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return None;
            }
            let mut fds = [nix::poll::PollFd::new(
                self.wire.as_fd(),
                nix::poll::PollFlags::POLLIN,
            )];
            let bound = nix::poll::PollTimeout::try_from(remaining).unwrap();
            if nix::poll::poll(&mut fds, bound).unwrap() == 0 {
                return None;
            }
            let mut bytes = [0; 8192];
            match self.wire.read(&mut bytes) {
                Ok(0) => panic!("member closed: {}", self.log()),
                Ok(n) => self.buffer.extend_from_slice(&bytes[..n]),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(e) => panic!("harness read: {e}"),
            }
        }
    }
    fn ask(&mut self, ask: &str, last: Option<usize>) -> String {
        let body = last.map(|n| json!({"last-turns":n})).unwrap_or(json!({}));
        self.send(&format!("{}\n", json!({"ask":{ask:body}})));
        self.receive(WAIT)
            .unwrap_or_else(|| panic!("missing {ask} answer: {}", self.log()))
    }
    /// The live `restore` ask, naming a save point in the member's room.
    fn ask_restore(&mut self, name: &str) -> String {
        self.send(&format!(
            "{}\n",
            json!({"ask":{"restore":{"save-point":name}}})
        ));
        self.receive(WAIT)
            .unwrap_or_else(|| panic!("missing restore answer: {}", self.log()))
    }
    /// The holdings as rows, table by table: a `snapshot` ask writes the
    /// member's save point into its room, and the image is read back here
    /// through the engine's own deserialize into a fresh connection. The
    /// elected indexes are read too, by name.
    fn tables(&mut self) -> Vec<Vec<String>> {
        let answer = stamp_of(&self.ask("snapshot", None), "snapshot");
        let bytes = std::fs::read(self.directory.0.join(&answer.name)).expect("the save point");
        let save_point = SavePoint::parse(&bytes).expect("a sound save point");
        assert_eq!(
            save_point.digest(),
            answer.digest,
            "the digest names the bytes"
        );
        let mut connection = rusqlite::Connection::open_in_memory().unwrap();
        connection
            .deserialize_read_exact(
                rusqlite::MAIN_DB,
                &save_point.image[..],
                save_point.image.len(),
                true,
            )
            .expect("the image deserializes");
        custody_snapshot(&mut connection)
    }
    fn feed(&mut self, lines: &[String]) {
        for line in lines {
            assert!(self.tee.as_mut().unwrap().feed(line), "live tee detached");
        }
    }
    /// **The in-tree preload client**: dials the door, sends the opener
    /// naming `destination` under `rule`, the distillate of every line of the
    /// record through `cut` (the whole record where none), each renamed into
    /// the destination session as the live path renames it, and the seal.
    /// Returns how many distillates crossed.
    fn preload(
        &mut self,
        record: &Record,
        cut: Option<&str>,
        rule: &Election,
        destination: &str,
    ) -> usize {
        let length = if cut.is_some() {
            record.cut
        } else {
            record.lines.len()
        };
        self.preload_lines(
            &record.destination_lines(destination)[..length],
            rule,
            destination,
        )
    }
    /// The same client over an explicit list of canonical lines.
    fn preload_lines(&mut self, lines: &[String], rule: &Election, destination: &str) -> usize {
        let mut driver = self.connect_preload();
        driver
            .write_all(weaver_trace::opener(destination, rule).as_bytes())
            .unwrap();
        let mut crossed = 0;
        for line in lines {
            if let Some(frame) = weaver_trace::distill(line, rule) {
                driver.write_all(frame.as_bytes()).unwrap();
                crossed += 1;
            }
        }
        driver.write_all(b"{}\n").unwrap();
        crossed
    }
}
impl Drop for Member {
    fn drop(&mut self) {
        self.tee.take();
        let _ = self.wire.shutdown(std::net::Shutdown::Both);
        let until = Instant::now() + Duration::from_secs(2);
        while matches!(self.process.0.try_wait(), Ok(None)) && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(5));
        }
        if !matches!(self.process.0.try_wait(), Ok(Some(_))) {
            let _ = self.process.0.kill();
        }
        let _ = self.process.0.wait();
        // The process ends before its directory drops, on both the ordinary
        // path and assertion unwinding.
    }
}

fn compare(cut: &str, live: &mut Member, rebuilt: &mut Member, expected: &[Event]) {
    let wanted_shape = shape(expected);
    let live_shape = answer_shape(&live.ask("shape", None));
    let rebuilt_shape = answer_shape(&rebuilt.ask("shape", None));
    println!(
        "W5B {cut} shape expected={wanted_shape:?} live={live_shape:?} reconstructed={rebuilt_shape:?}"
    );
    assert_eq!(
        (&live_shape, &rebuilt_shape),
        (&wanted_shape, &wanted_shape),
        "three-way shape disagreement at {cut}"
    );
    for (ask, last) in [
        ("replay", None),
        ("identity", None),
        ("recall", None),
        ("recall", Some(1)),
    ] {
        let wanted = subset(expected, ask, last);
        let left = answer_events(&live.ask(ask, last), ask);
        let right = answer_events(&rebuilt.ask(ask, last), ask);
        println!(
            "W5B {cut} {ask} last={last:?} counts expected={} live={} reconstructed={} equal-live={} equal-reconstructed={}",
            wanted.len(),
            left.len(),
            right.len(),
            left == wanted,
            right == wanted
        );
        assert_eq!(
            (&left, &right),
            (&wanted, &wanted),
            "three-way {ask} disagreement at {cut}, last={last:?}"
        );
    }
}

// Each cell has its own live and reconstructed process/database pair. Whole
// ordinary reconstruction is a bare resume; every cut and diagnostic cell
// stands under a source-distinct destination on BOTH paths and in expectation.
fn matched_cuts(record: &Record, rule: &Election, diagnostic: bool, label: &str) {
    for (cut, length) in [(Some(CUT), record.cut), (None, record.lines.len())] {
        let destination = if diagnostic {
            DESTINATION
        } else if cut.is_some() {
            BRANCH
        } else {
            SESSION
        };
        let expected = expected(&record.lines[..length], rule, destination);
        let primary = record.destination_lines(destination);
        let mut live = Member::new(rule.clone(), false, destination);
        live.feed(&primary[..length]);
        live.feed(&record.foreign_lines());
        let mut rebuilt = Member::new(rule.clone(), true, destination);
        let preloaded = rebuilt.preload(record, cut, rule, destination);
        rebuilt.feed(&record.foreign_lines());
        compare(
            &format!("{label}/{}", cut.unwrap_or("whole")),
            &mut live,
            &mut rebuilt,
            &expected,
        );
        assert_eq!(
            preloaded,
            expected.len(),
            "every elected canonical line parsed and projected"
        );
    }
}

#[test]
#[ignore = "needs the preload credential; run inside a user namespace by the watch below"]
fn three_way_at_matched_cuts() {
    child_entry();
    matched_cuts(&Record::new(), &election(), true, "diagnostic");
}

#[test]
#[ignore = "needs the preload credential; run inside a user namespace by the watch below"]
fn dead_driver_retry_replaces_the_prefix() {
    child_entry();
    let record = Record::new();
    let expected = expected(&record.lines, &election(), DESTINATION);
    let primary = record.destination_lines(DESTINATION);
    let mut live = Member::new(election(), false, DESTINATION);
    live.feed(&primary);
    live.feed(&record.foreign_lines());
    let mut rebuilt = Member::new(election(), true, DESTINATION);
    rebuilt.feed(&record.foreign_lines());
    let mut driver = rebuilt.connect_preload();
    driver
        .write_all(weaver_trace::opener(DESTINATION, &election()).as_bytes())
        .unwrap();
    for line in &primary[..2] {
        if let Some(frame) = weaver_trace::distill(line, &election()) {
            driver.write_all(frame.as_bytes()).unwrap();
        }
    }
    drop(driver);
    let prefix = shape(&expected[..2]);
    let until = Instant::now() + WAIT;
    while answer_shape(&rebuilt.ask("shape", None)) != prefix {
        assert!(Instant::now() < until, "dead prefix never landed");
        std::thread::sleep(Duration::from_millis(5));
    }
    rebuilt.send("{\"ask\":{\"replay\":{}}}\n");
    assert!(
        rebuilt.receive(Duration::from_millis(150)).is_none(),
        "unsealed prefix answered replay"
    );
    rebuilt.preload(&record, None, &election(), DESTINATION);
    let answer = rebuilt
        .receive(WAIT)
        .expect("retry must release the parked replay at its seal");
    let left = answer_events(&live.ask("replay", None), "replay");
    let right = answer_events(&answer, "replay");
    println!("E1 retry expected={expected:?} live={left:?} reconstructed={right:?}");
    assert_eq!(
        (&left, &right),
        (&expected, &expected),
        "retry must replace, not append to, the prefix"
    );
    compare("diagnostic/retry", &mut live, &mut rebuilt, &expected);
}

/// Replaces W5b's measurement with the comparison the recorded-rule ruling
/// authorizes. The empty and all-kinds cases protect the system exception;
/// the restrictive case elects unknown material and the absent/null/empty
/// fixtures, while excluding the ordinary event under both preload modes.
#[test]
#[ignore = "needs the preload credential; run inside a user namespace by the watch below"]
fn recorded_rule_three_way_at_matched_cuts() {
    child_entry();
    let original = Record::new();
    matched_cuts(&original, &original.rule(), false, "recorded-all");
    let mut restricted = election();
    // Leave system out of the rule entirely: its whole payload still crosses.
    restricted
        .keys
        .retain(|entry| !SELECTION.iter().any(|s| s.identity && s.kind == entry.kind));
    restricted.keys.push(ElectedKind {
        kind: FUTURE.into(),
        paths: vec![FUTURE_PATH.into()],
    });
    let restricted = Record::under(&restricted);
    matched_cuts(
        &restricted,
        &restricted.rule(),
        false,
        "recorded-restrictive",
    );
    let empty = Record::under(&Election {
        all_kinds: false,
        keys: vec![],
    });
    matched_cuts(&empty, &empty.rule(), false, "recorded-empty");
}

// Seed both the addressed session and a neighbor into a save point the
// member is handed at its spawn, the way admin hands one, so a malformed
// opener is judged against holdings that stand. The rows and index
// definitions are then observed through the member's own save points: a
// rejected opener must not even build indexes from the valid entries
// surrounding an invalid entry.
fn seed_save_point(directory: &std::path::Path) -> PathBuf {
    let mut store = Sqlite::stand().expect("the seed store stands");
    store
        .index_election(&weaver_state::Election {
            all_kinds: true,
            keys: vec![("load".into(), vec!["existing".into()])],
        })
        .unwrap();
    for session in ["target", "neighbor"] {
        store
            .land(&weaver_state::Distillate {
                session: session.into(),
                run: "old-run".into(),
                turn: None,
                kind: "load".into(),
                sequence: 0,
                pairs: vec![("existing".into(), "null".into())],
            })
            .unwrap();
    }
    let save_point = SavePoint::take(
        store.position().unwrap().expect("a position"),
        &store.schema().unwrap(),
        store.image().unwrap(),
    );
    let path = directory.join("seed.save-point");
    std::fs::write(&path, save_point.bytes()).expect("the seed writes");
    path
}
/// The seed's holdings as the member would snapshot them, for a member that
/// exits before it can be asked.
fn seed_tables(path: &std::path::Path) -> Vec<Vec<String>> {
    let save_point = SavePoint::parse(&std::fs::read(path).unwrap()).unwrap();
    let mut connection = rusqlite::Connection::open_in_memory().unwrap();
    connection
        .deserialize_read_exact(
            rusqlite::MAIN_DB,
            &save_point.image[..],
            save_point.image.len(),
            true,
        )
        .unwrap();
    custody_snapshot(&mut connection)
}
fn custody_snapshot(client: &mut rusqlite::Connection) -> Vec<Vec<String>> {
    [
        "SELECT json_object('id', id, 'session', session, 'run', run, 'turn', turn, \
         'kind', kind, 'sequence', sequence) FROM event ORDER BY id",
        "SELECT json_object('event_id', event_id, 'key', key, 'value', value) \
         FROM field ORDER BY event_id, key",
        "SELECT sql FROM sqlite_master WHERE type = 'index' AND sql IS NOT NULL ORDER BY name",
        "SELECT json_object('event_id', event_id, 'role', role, 'parts', parts) \
         FROM message ORDER BY event_id",
        "SELECT json_object('event_id', event_id, 'ordinal', ordinal, 'block', block, \
         'text', text, 'name', name, 'arguments', arguments, 'content', content) \
         FROM part ORDER BY event_id, ordinal",
        "SELECT json_object('event_id', event_id, 'perplexity', perplexity, \
         'entropies', entropies, 'surprisals', surprisals) FROM measurement ORDER BY event_id",
        "SELECT json_object('event_id', event_id, 'member', member, 'ordinal', ordinal, \
         'value', value) FROM series ORDER BY event_id, member, ordinal",
    ]
    .iter()
    .map(|query| {
        let mut statement = client.prepare(query).unwrap();
        statement
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    })
    .collect()
}

#[test]
#[ignore = "needs the preload credential; run inside a user namespace by the watch below"]
fn malformed_first_door_elections_leave_custody_untouched() {
    child_entry();
    for opener in super::tests::malformed_openers() {
        let seed_dir = Directory::new();
        let seed = seed_save_point(&seed_dir.0);
        let before = seed_tables(&seed);
        let mut member = Member::spawn_with(true, Some(&seed));
        member.send(&(opener.clone() + "\n"));
        // Closing the sender also bounds a mutant that silently defaults and
        // begins serving: it exits successfully, which is itself a failure.
        member.wire.shutdown(std::net::Shutdown::Write).unwrap();
        let status = member.process.wait();
        // The handed save point is never written, and a member that refused
        // its opener wrote nothing of its own into its room.
        assert_eq!(seed_tables(&seed), before, "changed custody: {opener}");
        let written: Vec<_> = std::fs::read_dir(&member.directory.0)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .filter(|name| name.contains("save-point"))
            .collect();
        assert!(written.is_empty(), "a refused opener wrote {written:?}");
        assert!(!status.success(), "accepted {opener}");
        assert!(
            member
                .log()
                .contains("malformed election in first-door opener"),
            "{}",
            member.log()
        );
        assert!(
            !member.door().exists(),
            "malformed first opener stood the preload door"
        );
    }
}

#[test]
#[ignore = "needs the preload credential; run inside a user namespace by the watch below"]
fn malformed_preload_elections_leave_custody_untouched() {
    child_entry();
    let mut cases = super::tests::malformed_openers();
    let valid_election = json!({"all_kinds":true,"keys":[]});
    cases.push(json!({"election":valid_election}).to_string());
    for session in [
        Value::Null,
        json!(""),
        json!(false),
        json!(0),
        json!([]),
        json!({}),
    ] {
        cases.push(json!({"session":session,"election":valid_election}).to_string());
    }
    // A well-shaped opener can still be refused by the engine. That refusal
    // belongs to this driver's attempt too, not to the standing member. The
    // embedded engine cannot carry a NUL into its index statement, so an
    // elected path holding one is refused inside the engine, after the
    // opener parsed (the service engine refused an over-long name here).
    cases.push(
        json!({"session":"target","election":{"all_kinds":false,
        "keys":[{"kind":"load","paths":["new.index","held\u{0}path"]}]}})
        .to_string(),
    );
    for opener in cases {
        let seed_dir = Directory::new();
        let seed = seed_save_point(&seed_dir.0);
        let mut member = Member::new_with(
            Election {
                all_kinds: true,
                keys: vec![],
            },
            true,
            "target",
            Some(&seed),
        );
        let before = member.tables();
        assert_eq!(
            before,
            seed_tables(&seed),
            "the seed was restored at the spawn"
        );
        // Shape is a barrier proving the prior replay ask reached parking.
        member.send("{\"ask\":{\"replay\":{}}}\n");
        member.ask("shape", None);
        assert!(member.receive(Duration::from_millis(25)).is_none());
        let retry_opener = weaver_trace::opener(
            "target",
            &Election {
                all_kinds: true,
                keys: vec![],
            },
        );
        let row = concat!(
            r#"{"envelope":{"session":"target","run":"retry","kind":"load","sequence":"1"},"pairs":{}}"#,
            "\n"
        );
        let mut preload = member.connect_preload();
        preload.set_read_timeout(Some(WAIT)).unwrap();
        // Keep the bad driver alive and coalesce trailing valid traffic. No
        // frame after its rejected opener may land or seal a parked answer.
        preload
            .write_all(format!("{opener}\n{retry_opener}{row}{{}}\n").as_bytes())
            .unwrap();
        let mut byte = [0];
        match preload.read(&mut byte) {
            Ok(0) => {}
            Err(error) if error.kind() == std::io::ErrorKind::ConnectionReset => {}
            other => panic!("refused preload was not closed: {other:?}; {opener}"),
        }
        assert!(
            member.process.0.try_wait().unwrap().is_none(),
            "refusal killed member: {}",
            member.log()
        );
        assert_eq!(member.tables(), before, "changed custody: {opener}");
        let fault = if super::parse_session(&opener).is_none_or(|s| s.is_empty()) {
            "missing nonempty session in preload opener"
        } else if super::parse_election(&opener).is_none() {
            "malformed election in preload opener"
        } else {
            "StoreUnavailable"
        };
        assert!(member.log().contains(fault), "{}", member.log());
        member.ask("shape", None); // The harness still serves on the same member.
        assert!(
            member.receive(Duration::from_millis(25)).is_none(),
            "refusal released replay"
        );
        drop(preload);
        // Connect succeeding proves the door re-stood. Do not send a new ask:
        // the successful retry must release the one parked before the refusal.
        let mut retry = member.connect_preload();
        retry
            .write_all(format!("{retry_opener}{row}{{}}\n").as_bytes())
            .unwrap();
        let answer = member
            .receive(WAIT)
            .expect("retry releases original parked replay");
        let value: Value = serde_json::from_str(&answer).unwrap();
        let events = value["answer"]["replay"]["events"].as_array().unwrap();
        assert_eq!(events.len(), 1, "{answer}");
        assert_eq!(events[0]["envelope"]["run"], "retry");
        assert!(
            member.receive(Duration::from_millis(25)).is_none(),
            "duplicate parked answer"
        );
        let after = member.tables();
        assert_eq!(
            after[2], before[2],
            "no new indexes from refused attempt or empty retry"
        );
        assert_eq!(after[0].len(), 2, "retry replaces only the target");
        assert!(after[0].contains(&before[0][1]), "neighbor event unchanged");
        assert_eq!(
            after[1],
            vec![before[1][1].clone()],
            "neighbor field unchanged"
        );
        assert!(member.process.0.try_wait().unwrap().is_none());
    }
}

#[test]
#[ignore = "needs the preload credential; run inside a user namespace by the watch below"]
fn explicit_empty_elections_open_both_doors() {
    child_entry();
    for all_kinds in [false, true] {
        for keys in [json!([]), json!([{"kind":"load","paths":[]}])] {
            let opener = format!(
                "{}\n",
                json!({"session":"target", "election":{
                "all_kinds":all_kinds, "keys":keys}})
            );
            let seed_dir = Directory::new();
            let seed = seed_save_point(&seed_dir.0);
            let before = seed_tables(&seed);
            let mut member = Member::spawn_with(true, Some(&seed));
            member.send(&opener);
            assert!(!answer_shape(&member.ask("shape", None)).is_empty());
            assert_eq!(member.tables(), before, "first door preserves holdings");
            let mut preload = member.connect_preload();
            preload.write_all(opener.as_bytes()).unwrap();
            preload.write_all(b"{}\n").unwrap();
            drop(preload);
            assert!(answer_events(&member.ask("replay", None), "replay").is_empty());
            let after = member.tables();
            assert_eq!(after[2], before[2], "no paths means no new indexes");
            assert_eq!(after[0].len(), 1, "only the addressed session retires");
            assert!(after[0][0].contains("neighbor"));
            assert_eq!(after[1].len(), 1, "neighbor's field remains");
        }
    }
}

/// The stamp members off a `snapshot` or `restore` answer.
struct Stamped {
    name: String,
    run: String,
    sequence: u64,
    turn: u64,
    digest: String,
}
fn stamp_of(frame: &str, ask: &str) -> Stamped {
    let value: Value = serde_json::from_str(frame).unwrap();
    let body = &value["answer"][ask];
    assert!(body.is_object(), "{ask} answered: {frame}");
    Stamped {
        name: body["save-point"].as_str().unwrap().into(),
        run: body["run"].as_str().unwrap().into(),
        sequence: body["sequence"].as_u64().unwrap(),
        turn: body["turn"].as_u64().unwrap(),
        digest: body["digest"].as_str().unwrap().into(),
    }
}

/// The offline builder's rule in miniature, per `weaver-state-Spec` section
/// 3 and `weaver-trace-PRD` section 3: a rebuild reads every `load` event's
/// `reset` and lands nothing of the prior run past the save point the load
/// reset to, or nothing of it at all where the load stood empty, so the
/// rebuild arrives where the store did. Everything else lands in record
/// order.
fn rebuild_plan(lines: &[String]) -> Vec<String> {
    let mut planned: Vec<String> = Vec::new();
    for line in lines {
        let row: Value = serde_json::from_str(line).unwrap();
        if row["kind"] == "load"
            && let Some(reset) = row["payload"].get("reset")
        {
            let prior = reset["prior_run"].as_str().unwrap().to_string();
            let lineage = row["payload"].get("lineage").cloned();
            planned.retain(|held| {
                let held: Value = serde_json::from_str(held).unwrap();
                if held["run"] != prior.as_str() {
                    return true;
                }
                match &lineage {
                    Some(lineage) if lineage["run"] == prior.as_str() => {
                        let sequence: u64 = held["sequence"].as_str().unwrap().parse().unwrap();
                        sequence <= lineage["sequence"].as_u64().unwrap()
                    }
                    _ => false,
                }
            });
        }
        planned.push(line.clone());
    }
    planned
}

/// **A reloaded store equals a full replay**, the store primitive's
/// instrument, per `weaver-state-Spec` section 5 on the rulings of
/// 2026-10-02 on #1 and #58, and the contract's section 8. A live member
/// lands a recorded run part way and takes a save point; a second member is
/// handed that save point at its spawn and answers `restored` with its stamp;
/// a live restore of the same save point on the first member holds what the
/// second held at its restore; the second then lands the next run, whose
/// `load` names the lineage and a reset of the prior run; and a third member
/// rebuilds the whole record through the preload door, honouring the reset,
/// to the same holdings, ask for ask and table by table, the typed tables
/// and the elected index included. Two `snapshot` asks on unchanged holdings
/// give two files.
///
/// Perturbations: stamp the save point one position late or one early in
/// `Sqlite::position` and the stamp no longer names the last event the
/// replay serves, the first assertion failing; make `rebuild_plan` ignore
/// the reset and the rebuilt member holds the prior run's lost tail where
/// the restored one does not, the table comparison failing.
#[test]
#[ignore = "needs the preload credential; run inside a user namespace by the watch below"]
fn a_reloaded_store_equals_a_full_replay() {
    child_entry();
    let record = Record::new();
    let rule = election();
    let lines = record.destination_lines(SESSION);
    let at = |run: &str, turn: &str, kind: &str| {
        lines
            .iter()
            .position(|line| {
                let row: Value = serde_json::from_str(line).unwrap();
                row["run"] == run && row["turn"] == turn && row["kind"] == kind
            })
            .unwrap()
    };
    // The save point falls after the first run's first turn; the first run's
    // second turn is the tail an unclean stop loses.
    let cut = at("r-one", "t-1", "turn.closed") + 1;
    let r_two = lines
        .iter()
        .position(|line| serde_json::from_str::<Value>(line).unwrap()["run"] == "r-two")
        .unwrap();

    let mut live = Member::new(rule.clone(), false, SESSION);
    live.feed(&lines[..cut]);
    let taken = stamp_of(&live.ask("snapshot", None), "snapshot");
    let replayed = answer_events(&live.ask("replay", None), "replay");
    let last = replayed.last().expect("holdings");
    assert_eq!(
        (
            last.envelope["run"].as_str(),
            last.envelope["sequence"].parse::<u64>().unwrap()
        ),
        (taken.run.as_str(), taken.sequence),
        "the stamp names the last event the holdings cover"
    );
    assert_eq!(taken.turn, 1, "and that run's last turn");
    let again = stamp_of(&live.ask("snapshot", None), "snapshot");
    assert_ne!(again.name, taken.name, "two asks give two files");
    assert_eq!(
        (&again.run, again.sequence, again.turn),
        (&taken.run, taken.sequence, taken.turn)
    );
    for name in [&taken.name, &again.name] {
        assert!(live.directory.0.join(name).is_file(), "{name} stands");
    }
    live.feed(&lines[cut..r_two]);

    // The second load restores through the descriptor and says so.
    let save_point = live.directory.0.join(&taken.name);
    let mut restored = Member::new_with(rule.clone(), false, SESSION, Some(&save_point));
    let answered: Value = serde_json::from_str(&restored.ask("restored", None)).unwrap();
    assert_eq!(
        answered["answer"]["restored"]["lineage"],
        json!({"digest": taken.digest, "run": taken.run, "sequence": taken.sequence, "turn": taken.turn}),
        "the restored ask answers the stamp the load restored"
    );
    let at_restore = restored.tables();

    // A live restore of the same save point on the first member holds the
    // same, its tail gone.
    let back = stamp_of(&live.ask_restore(&taken.name), "restore");
    assert_eq!(
        (&back.run, back.sequence, back.turn, &back.digest),
        (&taken.run, taken.sequence, taken.turn, &taken.digest)
    );
    assert_eq!(
        live.tables(),
        at_restore,
        "a live restore holds what the load restored"
    );

    // The next run's load names the lineage and the reset of the prior run.
    let mut next: Vec<String> = lines[r_two..].to_vec();
    {
        let mut load: Value = serde_json::from_str(&next[0]).unwrap();
        assert_eq!(load["kind"], "load");
        load["payload"]["lineage"] = json!({
            "save_point": taken.digest, "run": taken.run, "sequence": taken.sequence,
            "turn": taken.turn, "operator_supplied": false,
        });
        load["payload"]["reset"] = json!({"prior_run": "r-one", "reason": "no-clean-unload"});
        next[0] = serde_json::to_string(&load).unwrap() + "\n";
    }
    restored.feed(&next);

    // The rebuild through the door, honouring the reset.
    let whole: Vec<String> = lines[..r_two]
        .iter()
        .cloned()
        .chain(next.iter().cloned())
        .collect();
    let plan = rebuild_plan(&whole);
    let stamped = lines
        .iter()
        .position(|line| {
            let row: Value = serde_json::from_str(line).unwrap();
            row["run"] == taken.run.as_str()
                && row["sequence"].as_str().and_then(|n| n.parse::<u64>().ok())
                    == Some(taken.sequence)
        })
        .expect("the stamp names a recorded line");
    assert!(stamped < cut, "and one before the cut");
    let through_the_stamp: Vec<String> = lines[..=stamped]
        .iter()
        .cloned()
        .chain(next.iter().cloned())
        .collect();
    assert_eq!(
        plan, through_the_stamp,
        "the plan is the record through the stamped position and the next run, the lost tail left out"
    );
    let mut rebuilt = Member::new(rule.clone(), true, SESSION);
    rebuilt.preload_lines(&plan, &rule, SESSION);
    let expected = expected(&plan, &rule, SESSION);
    compare("restore/rebuild", &mut restored, &mut rebuilt, &expected);
    assert_eq!(
        restored.tables(),
        rebuilt.tables(),
        "the restored store and the rebuilt one hold the same rows, table by table"
    );
}

/// **A save point that fails its check never reaches the holdings, and one
/// under another schema is refused on `restored`**, per `weaver-state-Spec`
/// sections 3 and 5: at a load, a save point taken under a schema the
/// member does not stand answers `refused` naming the mismatch and the
/// member stands empty; at a live restore, a save point truncated part way
/// and one with a byte flipped each go unanswered and leave the holdings
/// standing, and so does a name that is not a plain entry of the room. A
/// torn save point handed at the spawn refuses the member's start, the
/// bytes being admin's to judge at the inventory and a disagreement here a
/// load that did not finish standing.
///
/// Perturbations: skip the schema comparison in `adopt_judged` and the
/// foreign save point restores, the first assertion failing; skip the check
/// in `SavePoint::parse` and the flipped file restores, the holdings moving.
#[test]
#[ignore = "needs the preload credential; run inside a user namespace by the watch below"]
fn a_damaged_or_foreign_save_point_never_reaches_the_holdings() {
    child_entry();
    let rule = election();
    let record = Record::new();
    let lines = record.destination_lines(SESSION);

    // A save point taken under another schema.
    let foreign_dir = Directory::new();
    let foreign = {
        let mut store = Sqlite::stand().expect("stands");
        store
            .land(&weaver_state::Distillate {
                session: SESSION.into(),
                run: "r-foreign".into(),
                turn: None,
                kind: "load".into(),
                sequence: 0,
                pairs: vec![],
            })
            .unwrap();
        let schema = format!(
            "{}\ntable extra\nCREATE TABLE extra (x)\n",
            store.schema().unwrap()
        );
        let save_point = SavePoint::take(
            store.position().unwrap().unwrap(),
            &schema,
            store.image().unwrap(),
        );
        let path = foreign_dir.0.join("foreign.save-point");
        std::fs::write(&path, save_point.bytes()).unwrap();
        path
    };
    let mut member = Member::new_with(rule.clone(), false, SESSION, Some(&foreign));
    let answered: Value = serde_json::from_str(&member.ask("restored", None)).unwrap();
    assert_eq!(
        answered["answer"]["restored"],
        json!({"refused": "schema-mismatch"}),
        "a save point under another schema is refused on restored"
    );
    assert!(
        answer_shape(&member.ask("shape", None)).is_empty(),
        "and the member stands empty"
    );
    // A stamp written to agree cannot carry a foreign image past the rule:
    // the image's own catalog has the extra table, whatever the stamp names.
    // Perturbation: drop the image's schema comparison in `adopt_judged`
    // and this restores.
    let lying = {
        let mut store = Sqlite::stand().expect("stands");
        let standing = store.schema().unwrap();
        store
            .land(&weaver_state::Distillate {
                session: SESSION.into(),
                run: "r-lying".into(),
                turn: None,
                kind: "load".into(),
                sequence: 0,
                pairs: vec![],
            })
            .unwrap();
        let image = store.image().unwrap();
        let mut connection = rusqlite::Connection::open_in_memory().unwrap();
        connection
            .deserialize_read_exact(rusqlite::MAIN_DB, &image[..], image.len(), false)
            .unwrap();
        connection.execute_batch("CREATE TABLE extra (x)").unwrap();
        let extra = connection.serialize(rusqlite::MAIN_DB).unwrap().to_vec();
        let save_point = SavePoint::take(store.position().unwrap().unwrap(), &standing, extra);
        let path = foreign_dir.0.join("lying.save-point");
        std::fs::write(&path, save_point.bytes()).unwrap();
        path
    };
    let mut member = Member::new_with(rule.clone(), false, SESSION, Some(&lying));
    let answered: Value = serde_json::from_str(&member.ask("restored", None)).unwrap();
    assert_eq!(
        answered["answer"]["restored"],
        json!({"refused": "schema-mismatch"}),
        "a stamp naming the standing schema over a foreign image is refused"
    );
    // **The exemption is exact**, per the operator's ruling of 2026-10-05 on
    // #1: a table or a trigger hidden under the elected prefix is schema and
    // refuses, whatever the stamp names. Perturbation: exempt by prefix
    // alone in `schema_of` and both restore.
    for (label, hidden) in [
        ("table", "CREATE TABLE field_elected_7a (x)"),
        (
            "trigger",
            "CREATE TRIGGER field_elected_7b BEFORE INSERT ON field BEGIN SELECT 1; END",
        ),
    ] {
        let mut store = Sqlite::stand().expect("stands");
        let standing = store.schema().unwrap();
        store
            .land(&weaver_state::Distillate {
                session: SESSION.into(),
                run: format!("r-{label}"),
                turn: None,
                kind: "load".into(),
                sequence: 0,
                pairs: vec![],
            })
            .unwrap();
        let image = store.image().unwrap();
        let mut connection = rusqlite::Connection::open_in_memory().unwrap();
        connection
            .deserialize_read_exact(rusqlite::MAIN_DB, &image[..], image.len(), false)
            .unwrap();
        connection.execute_batch(hidden).unwrap();
        let hiding = connection.serialize(rusqlite::MAIN_DB).unwrap().to_vec();
        let save_point = SavePoint::take(store.position().unwrap().unwrap(), &standing, hiding);
        let path = foreign_dir.0.join(format!("hidden-{label}.save-point"));
        std::fs::write(&path, save_point.bytes()).unwrap();
        let mut member = Member::new_with(rule.clone(), false, SESSION, Some(&path));
        let answered: Value = serde_json::from_str(&member.ask("restored", None)).unwrap();
        assert_eq!(
            answered["answer"]["restored"],
            json!({"refused": "schema-mismatch"}),
            "a {label} hidden under the elected prefix is schema"
        );
        assert!(answer_shape(&member.ask("shape", None)).is_empty());
    }
    // A stamp that lies about its position is refused as one that disagrees.
    let misstamped = {
        let mut store = Sqlite::stand().expect("stands");
        store
            .land(&weaver_state::Distillate {
                session: SESSION.into(),
                run: "r-stamped".into(),
                turn: None,
                kind: "load".into(),
                sequence: 3,
                pairs: vec![],
            })
            .unwrap();
        let mut stamp = store.position().unwrap().unwrap();
        stamp.sequence += 1;
        let save_point = SavePoint::take(stamp, &store.schema().unwrap(), store.image().unwrap());
        let path = foreign_dir.0.join("misstamped.save-point");
        std::fs::write(&path, save_point.bytes()).unwrap();
        path
    };
    let mut member = Member::new_with(rule.clone(), false, SESSION, Some(&misstamped));
    let answered: Value = serde_json::from_str(&member.ask("restored", None)).unwrap();
    assert_eq!(
        answered["answer"]["restored"],
        json!({"refused": "stamp disagrees with the image"}),
        "a stamp that lies about its position is refused"
    );

    // A live restore of a damaged save point leaves the holdings standing.
    member.feed(&lines[..record.cut]);
    let taken = stamp_of(&member.ask("snapshot", None), "snapshot");
    let before = member.tables();
    let sound = std::fs::read(member.directory.0.join(&taken.name)).unwrap();
    let mut flipped = sound.clone();
    let last = flipped.len() - 1;
    flipped[last] ^= 0x01;
    std::fs::write(member.directory.0.join("flipped"), &flipped).unwrap();
    std::fs::write(member.directory.0.join("torn"), &sound[..sound.len() / 2]).unwrap();
    for name in ["flipped", "torn", "../flipped", ".part-x", "absent"] {
        member.send(&format!(
            "{}\n",
            json!({"ask":{"restore":{"save-point":name}}})
        ));
        assert!(
            member.receive(Duration::from_millis(250)).is_none(),
            "{name} restored: {}",
            member.log()
        );
    }
    assert_eq!(
        member.tables(),
        before,
        "the holdings stand after every refusal"
    );
    let back = stamp_of(&member.ask_restore(&taken.name), "restore");
    assert_eq!(back.digest, taken.digest, "the sound one still restores");
    // A live restore of a save point taken under another election stands
    // this load's election on the restored holdings. Perturbation: drop the
    // `index_election` from `adopt_judged` and the elected index is gone.
    let unelected = {
        let mut store = Sqlite::stand().expect("stands");
        store
            .land(&weaver_state::Distillate {
                session: SESSION.into(),
                run: "r-unelected".into(),
                turn: None,
                kind: "load".into(),
                sequence: 0,
                pairs: vec![],
            })
            .unwrap();
        SavePoint::take(
            store.position().unwrap().unwrap(),
            &store.schema().unwrap(),
            store.image().unwrap(),
        )
    };
    std::fs::write(member.directory.0.join("unelected"), unelected.bytes()).unwrap();
    stamp_of(&member.ask_restore("unelected"), "restore");
    let after = member.tables();
    assert!(
        after[2].iter().any(|sql| sql.contains("field_elected_")),
        "the active election's indexes stand on the restored holdings: {:?}",
        after[2]
    );

    // A torn save point handed at the spawn refuses the start.
    let torn = foreign_dir.0.join("torn.save-point");
    std::fs::write(&torn, &sound[..sound.len() / 2]).unwrap();
    let mut refused = Member::spawn_with(false, Some(&torn));
    let status = refused.process.wait();
    assert!(
        !status.success(),
        "a torn save point at the descriptor refuses the start"
    );
    assert!(
        refused.log().contains("the save point descriptor refuses"),
        "{}",
        refused.log()
    );
}

/// **The watch for this module's instruments**: re-executes this test binary
/// inside `unshare --map-root-user`, which gives the preload door its
/// operator credential with no sudo, and runs every ignored instrument above
/// against the embedded store. Where no user namespace can be entered, it
/// prints a SKIP naming why and passes. Run as
/// root, it runs the instruments in place.
#[test]
fn the_preload_door_instruments_are_watched_inside_a_user_namespace() {
    let exe = std::env::current_exe().expect("the test binary names itself");
    let mut command = if nix::unistd::getuid().is_root() {
        Command::new(&exe)
    } else {
        let mut unshare = Command::new("unshare");
        unshare.arg("--map-root-user").arg(&exe);
        unshare
    };
    let ran = command
        .args(["preload_door::", "--ignored", "--nocapture"])
        .stdin(std::process::Stdio::null())
        .output();
    let output = match ran {
        Ok(output) => output,
        Err(e) => {
            eprintln!("SKIP preload door watch: unshare could not run: {e}");
            return;
        }
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.starts_with("unshare:") {
        eprintln!(
            "SKIP preload door watch: no user namespace here: {}",
            stderr.trim()
        );
        return;
    }
    assert!(
        output.status.success() && stdout.contains("test result: ok. 8 passed"),
        "the preload door instruments failed inside the namespace\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
