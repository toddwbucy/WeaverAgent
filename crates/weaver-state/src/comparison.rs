//! conforms: state-serve-restricts-to-the-session
//! conforms: state-replay-answers-at-the-seal
//! conforms: state-preload-door-stands-only-diagnostic
//! Process-level comparison of custody's two doors against an independent record walk.

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

use super::postgres_scratch::Scratch;

// This spelling is independent of analysis. A changed binary must fail against
// it. The message/identity labels describe the asks, not a second kind list.
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
    // the recorded bytes; reconstruction reads the unchanged source record.
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
        let result = super::member_entry(args.into_iter(), fd);
        std::process::exit(if result == ExitCode::SUCCESS { 0 } else { 1 });
    }
    analysis_binary(); // Refuse missing/stale analysis before any child or database stands.
    assert!(
        nix::unistd::getuid().is_root(),
        "preload requires the operator credential; run these scratch tests with unshare -Ur (no sudo)"
    );
}
fn analysis_binary() -> PathBuf {
    let exe = std::env::current_exe().unwrap();
    let path = exe
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("weaver-analysis");
    assert!(
        path.is_file(),
        "missing {}: run cargo build -p weaver-analysis --locked before this suite",
        path.display()
    );
    fn newest_source(directory: &std::path::Path) -> std::time::SystemTime {
        std::fs::read_dir(directory)
            .expect("analysis source directory")
            .map(|entry| {
                let entry = entry.unwrap();
                if entry.file_type().unwrap().is_dir() {
                    newest_source(&entry.path())
                } else {
                    entry.metadata().unwrap().modified().unwrap()
                }
            })
            .max()
            .expect("analysis sources")
    }
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../weaver-analysis/src");
    assert!(
        std::fs::metadata(&path).unwrap().modified().unwrap() >= newest_source(&source),
        "stale {}: run cargo build -p weaver-analysis --locked before this suite",
        path.display()
    );
    path
}
struct Member {
    process: Process,
    tee: Option<Tee>,
    wire: UnixStream,
    buffer: Vec<u8>,
    directory: Directory,
    // Fields drop in declaration order, after Drop stops/reaps the member.
    // Keep database custody last, after process and socket handles.
    _scratch: Scratch,
}
impl Member {
    fn new(election: Election, diagnostic: bool, destination: &str) -> Self {
        let mut member = Self::spawn(diagnostic);
        member.tee = Some(
            Tee::open(
                member.wire.try_clone().unwrap(),
                destination.into(),
                election,
            )
            .unwrap(),
        );
        // A shape answer proves the production entry reached serve.
        assert!(answer_shape(&member.ask("shape", None)).is_empty());
        assert_eq!(
            member.door().exists(),
            diagnostic,
            "door follows the load's mode"
        );
        member
    }
    // Raw-open tests must reach the same entry before a tee supplies its opener.
    fn spawn(diagnostic: bool) -> Self {
        let thread = std::thread::current();
        let test = thread.name().expect("named test thread");
        let scratch = Scratch::new();
        // Establish the schema before the child and a seeding observer connect.
        // Otherwise their first opens can race PostgreSQL catalog creation.
        drop(scratch.open());
        let directory = Directory::new();
        let door = directory.0.join("preload.sock");
        let (wire, child) = UnixStream::pair().unwrap();
        let fd = child.as_raw_fd();
        let mut args = vec![
            "--engine".into(),
            "postgres".into(),
            "--store-socket".into(),
            scratch.socket.clone(),
            "--database".into(),
            scratch.database.clone(),
            "--role".into(),
            scratch.role.clone(),
            directory.0.to_str().unwrap().into(),
        ];
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
            .stdout(log.try_clone().unwrap())
            .stderr(log);
        // SAFETY: the child owns this socket. Only a descriptor flag is set
        // between fork and exec, and the test entry adopts it exactly once.
        unsafe {
            command.pre_exec(move || {
                let borrowed = std::os::fd::BorrowedFd::borrow_raw(fd);
                nix::fcntl::fcntl(
                    borrowed,
                    nix::fcntl::FcntlArg::F_SETFD(nix::fcntl::FdFlag::empty()),
                )
                .map(|_| ())
                .map_err(std::io::Error::from)
            });
        }
        let process = Process(command.spawn().unwrap());
        drop(child);
        Self {
            process,
            tee: None,
            wire,
            buffer: Vec::new(),
            directory,
            _scratch: scratch,
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
    fn feed(&mut self, lines: &[String]) {
        for line in lines {
            assert!(self.tee.as_mut().unwrap().feed(line), "live tee detached");
        }
    }
    fn reconstruct(
        &mut self,
        record: &Record,
        cut: Option<&str>,
        diagnostic: bool,
        destination: &str,
    ) -> usize {
        let trace = self.directory.0.join("record.ndjson");
        std::fs::write(&trace, record.lines.concat()).unwrap();
        let stdout = self.directory.0.join("analysis.json");
        let stderr = self.directory.0.join("analysis.log");
        let binary = analysis_binary();
        let mut command = Command::new(&binary);
        command.arg("preload").arg(trace).arg(self.door());
        if diagnostic {
            command.arg("--diagnostic");
        }
        if diagnostic || cut.is_some() || destination != SESSION {
            command.args(["--as", destination]);
        }
        if let Some(cut) = cut {
            command.args(["--through", cut]);
        }
        let until = Instant::now() + WAIT;
        loop {
            command
                .stdout(std::fs::File::create(&stdout).unwrap())
                .stderr(std::fs::File::create(&stderr).unwrap());
            let mut process = Process(command.spawn().unwrap());
            if process.wait().success() {
                break;
            }
            let refusal = std::fs::read_to_string(&stderr).unwrap();
            // Only retry a dial that could not have sent an opener. Every
            // parse, projection or post-connect failure remains a refusal.
            let dial_unavailable = [
                "No such file or directory (os error 2)",
                "Connection refused (os error 111)",
            ]
            .iter()
            .any(|error| refusal.contains(&format!("the preload died: {error}")));
            assert!(
                dial_unavailable && Instant::now() < until,
                "analysis refused: {refusal}"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        let report: Value =
            serde_json::from_str(&std::fs::read_to_string(stdout).unwrap()).unwrap();
        assert_eq!(report["sealed"], true);
        assert_eq!(report["source_session"], SESSION);
        assert_eq!(report["destination_session"], destination);
        assert_eq!(
            report["mode"],
            if diagnostic { "diagnostic" } else { "recorded" }
        );
        println!(
            "W5B analysis={} preloaded={}",
            binary.display(),
            report["preloaded"]
        );
        report["preloaded"].as_u64().unwrap() as usize
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
        // The process and all its PostgreSQL connections end before Scratch
        // drops, on both the ordinary path and assertion unwinding.
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
        let preloaded = rebuilt.reconstruct(record, cut, diagnostic, destination);
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
#[ignore = "needs WEAVER_STATE_TEST_PG scratch PostgreSQL and prebuilt weaver-analysis; run in unshare -Ur for the preload credential"]
fn three_way_at_matched_cuts() {
    child_entry();
    matched_cuts(&Record::new(), &election(), true, "diagnostic");
}

#[test]
#[ignore = "needs WEAVER_STATE_TEST_PG scratch PostgreSQL and prebuilt weaver-analysis; run in unshare -Ur for the preload credential"]
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
    rebuilt.reconstruct(&record, None, true, DESTINATION);
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
#[ignore = "needs WEAVER_STATE_TEST_PG scratch PostgreSQL and prebuilt weaver-analysis; run in unshare -Ur for the preload credential"]
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

// Seed both the addressed session and a neighbor before a malformed opener.
// Observe rows and index definitions directly: a rejected opener must not even
// build indexes from the valid entries surrounding an invalid entry.
fn seeded_custody(member: &Member) -> postgres::Client {
    use weaver_state::Store;
    let mut store = member._scratch.open();
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
    postgres::Config::new()
        .host_path(&member._scratch.socket)
        .user(&member._scratch.role)
        .dbname(&member._scratch.database)
        .connect(postgres::NoTls)
        .unwrap()
}
fn custody_snapshot(client: &mut postgres::Client) -> Vec<Vec<String>> {
    [
        "SELECT row_to_json(e)::text FROM event e ORDER BY id",
        "SELECT row_to_json(f)::text FROM field f ORDER BY event_id, key",
        "SELECT indexdef FROM pg_indexes WHERE schemaname = 'public' ORDER BY indexname",
    ]
    .iter()
    .map(|query| {
        client
            .query(*query, &[])
            .unwrap()
            .iter()
            .map(|row| row.get(0))
            .collect()
    })
    .collect()
}

#[test]
#[ignore = "needs WEAVER_STATE_TEST_PG naming a scratch PostgreSQL socket directory, unshare -Ur, and a built weaver-analysis binary"]
fn malformed_first_door_elections_leave_custody_untouched() {
    child_entry();
    for opener in super::tests::malformed_openers() {
        let mut member = Member::spawn(true);
        let mut client = seeded_custody(&member);
        let before = custody_snapshot(&mut client);
        member.send(&(opener.clone() + "\n"));
        // Closing the sender also bounds a mutant that silently defaults and
        // begins serving: it exits successfully, which is itself a failure.
        member.wire.shutdown(std::net::Shutdown::Write).unwrap();
        let status = member.process.wait();
        assert_eq!(
            custody_snapshot(&mut client),
            before,
            "changed custody: {opener}"
        );
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
#[ignore = "needs WEAVER_STATE_TEST_PG naming a scratch PostgreSQL socket directory, unshare -Ur, and a built weaver-analysis binary"]
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
    // belongs to this driver's attempt too, not to the standing member.
    let long_path = "x".repeat(1024);
    cases.push(
        json!({"session":"target","election":{"all_kinds":false,
        "keys":[{"kind":"load","paths":["new.index",long_path]}]}})
        .to_string(),
    );
    for opener in cases {
        let mut member = Member::new(
            Election {
                all_kinds: true,
                keys: vec![],
            },
            true,
            "target",
        );
        let mut client = seeded_custody(&member);
        let before = custody_snapshot(&mut client);
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
        assert_eq!(
            custody_snapshot(&mut client),
            before,
            "changed custody: {opener}"
        );
        let fault = if super::parse_session(&opener).is_none_or(|s| s.is_empty()) {
            "missing nonempty session in preload opener"
        } else if super::parse_election(&opener).is_none() {
            "malformed election in preload opener"
        } else {
            "the elected key path"
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
        let after = custody_snapshot(&mut client);
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
#[ignore = "needs WEAVER_STATE_TEST_PG naming a scratch PostgreSQL socket directory, unshare -Ur, and a built weaver-analysis binary"]
fn explicit_empty_elections_open_both_doors() {
    child_entry();
    for all_kinds in [false, true] {
        for keys in [json!([]), json!([{"kind":"load","paths":[]}])] {
            let opener = format!(
                "{}\n",
                json!({"session":"target", "election":{
                "all_kinds":all_kinds, "keys":keys}})
            );
            let mut member = Member::spawn(true);
            let mut client = seeded_custody(&member);
            let before = custody_snapshot(&mut client);
            member.send(&opener);
            assert!(!answer_shape(&member.ask("shape", None)).is_empty());
            assert_eq!(
                custody_snapshot(&mut client),
                before,
                "first door preserves holdings"
            );
            let mut preload = member.connect_preload();
            preload.write_all(opener.as_bytes()).unwrap();
            preload.write_all(b"{}\n").unwrap();
            drop(preload);
            assert!(answer_events(&member.ask("replay", None), "replay").is_empty());
            let after = custody_snapshot(&mut client);
            assert_eq!(after[2], before[2], "no paths means no new indexes");
            assert_eq!(after[0].len(), 1, "only the addressed session retires");
            assert!(after[0][0].contains("neighbor"));
            assert_eq!(after[1].len(), 1, "neighbor's field remains");
        }
    }
}
