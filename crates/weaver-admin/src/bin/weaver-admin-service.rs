//! The service, `weaver-admin-service`, per `weaver-admin-operator-contract`
//! section 8 and `weaver-admin-Spec` sections 1 and 2: admin's third binary,
//! kept standing by systemd under the weaver-admin account, which holds every
//! agent's sudo rule and the trace reader's role. It answers the admin
//! connector on one local socket, four exchanges: the grants, a verb, the
//! trace from a position, and the agent's `admin.log` lines. It runs the root
//! program `weaver-admin` only through sudo, one fixed line per verb, and
//! reads that verb's grant with `sudo -n -l` in every request. It shares two
//! files with the root program, the name rule and the random source, and
//! links nothing else of it.
//!
//! **What systemd hands it**: the listening socket at descriptor 3, by socket
//! activation (`LISTEN_FDS=1`, `LISTEN_PID` its own). **It runs without
//! no-new-privs**, since sudo gains root by its set-user-id bit, and it
//! refuses to run as root, where every grant read would answer granted.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::fs::{FileExt, MetadataExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use serde_json::Value;

#[path = "../name_rule.rs"]
mod name_rule;
#[path = "../random.rs"]
mod random;

const LISTENER_FD: RawFd = 3;

/// The contract version this service speaks, section 8.2.
const VERSION: u64 = 1;
const PURPOSES: [&str; 3] = ["control", "trace", "log"];

/// The eight verbs, in the order a grants answer names them (section 8.3).
const VERBS: [&str; 8] = [
    "show",
    "validate",
    "load",
    "unload",
    "stop",
    "save-point",
    "restore",
    "force-unload",
];

/// Section 8.7's limits.
const LINE_BOUND: usize = 4096;
const CONTROL_PER_AGENT: usize = 8;
const CONNECTIONS: usize = 1024;
const OBJECT_BOUND: usize = 64 * 1024;
const STDERR_KEPT: usize = 16 * 1024;
const LOG_LINE_BOUND: usize = 8 * 1024;
const LOG_UNSENT: usize = 1024 * 1024;

/// The fixed places of the production service.
const SUDO: &str = "/usr/bin/sudo";
const DEFAULT_BASE: &str = "/etc/weaver/admin";
const CONNECTOR: &str = "weaver-admincon";
const CHILD_PATH: &str = "/usr/sbin:/usr/bin:/sbin:/bin";

/// **How long the service waits for a descendant that holds the root
/// program's output pipes after it exits.** The root program's own bytes are
/// in the pipe by its exit, so this bounds only a process it left holding the
/// pipe, never what it wrote.
const PIPE_DRAIN: Duration = Duration::from_secs(1);

/// Section 8.7's waits, fixed in production. The test constructor shortens
/// them.
#[derive(Clone, Copy)]
struct Timing {
    /// The opening line's arrival, from the connect: 10 seconds.
    opening: Duration,
    /// The log's judging or the relay's dial, from the opening line: 8 seconds.
    judge_cut: Duration,
    /// A trace's `open`, from the dial's connect: 1 second.
    open_after_connect: Duration,
    /// A grant's read, or the eight, from the ask: 10 seconds.
    grant: Duration,
    /// How often a log connection looks at its file, inside section 8.6's 1 second.
    log_tick: Duration,
}

const PRODUCTION: Timing = Timing {
    opening: Duration::from_secs(10),
    judge_cut: Duration::from_secs(8),
    open_after_connect: Duration::from_secs(1),
    grant: Duration::from_secs(10),
    log_tick: Duration::from_millis(100),
};

/// A run machine A holds (section 8.8).
struct Run {
    id: String,
    verb: &'static str,
    started_ms: Option<u64>,
}

/// Machine A and the per-agent bounds, one per agent named at the service.
#[derive(Default)]
struct Agent {
    standing: Vec<Run>,
    /// The last final line of a run that ended, a `show` excepted.
    ended: Option<String>,
    control: usize,
    log_generation: u64,
}

type Ids = Box<dyn Fn() -> std::io::Result<String> + Send + Sync>;
type Clock = Box<dyn Fn() -> u64 + Send + Sync>;

struct Service {
    /// The sudo the service runs, `/usr/bin/sudo` in production.
    sudo: PathBuf,
    /// The root program's path as each agent's rule names it.
    admin: PathBuf,
    /// The base the agents' roots stand under.
    base: PathBuf,
    /// The owner the agent's root, its keys and its `admin.log` must have: root.
    root_owner: u32,
    /// The one uid admitted, the connector account's.
    connector_uid: u32,
    /// The service's own uid, the weaver-admin account's, which section 8.6's
    /// `through_service` reads.
    own_uid: u32,
    /// Drawn at each start of the service's own (section 8.4).
    instance: String,
    timing: Timing,
    connections_bound: usize,
    ids: Ids,
    clock: Clock,
    log: Mutex<Box<dyn Write + Send>>,
    agents: Mutex<HashMap<String, Agent>>,
    connections: AtomicUsize,
}

fn wall_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn main() -> std::process::ExitCode {
    let listener = match take_listener() {
        Ok(listener) => listener,
        Err(detail) => {
            eprintln!("weaver-admin-service: {detail}");
            return std::process::ExitCode::FAILURE;
        }
    };
    let service = match Service::production() {
        Ok(service) => service,
        Err(detail) => {
            eprintln!("weaver-admin-service: {detail}");
            return std::process::ExitCode::FAILURE;
        }
    };
    let error = serve(Arc::new(service), listener);
    eprintln!("weaver-admin-service: the socket stopped accepting: {error}");
    std::process::ExitCode::FAILURE
}

/// **The socket systemd made**, at descriptor 3, judged a listening Unix
/// stream socket and kept close-on-exec, so no root program inherits it.
fn take_listener() -> Result<UnixListener, String> {
    let pid = std::env::var("LISTEN_PID")
        .ok()
        .and_then(|v| v.parse::<u32>().ok());
    let fds = std::env::var("LISTEN_FDS").ok();
    if pid != Some(std::process::id()) || fds.as_deref() != Some("1") {
        return Err(
            "no socket is handed: LISTEN_PID and LISTEN_FDS=1 name none for this process".into(),
        );
    }
    // SAFETY: fcntl on a number this process may hold.
    let flags = unsafe { nix::libc::fcntl(LISTENER_FD, nix::libc::F_GETFD) };
    // SAFETY: as above, on a number that holds a descriptor.
    if flags == -1
        || unsafe {
            nix::libc::fcntl(
                LISTENER_FD,
                nix::libc::F_SETFD,
                flags | nix::libc::FD_CLOEXEC,
            )
        } == -1
    {
        return Err("descriptor 3 is not open".into());
    }
    // SAFETY: descriptor 3 is open, handed to this process, and owned by nothing else.
    let fd = unsafe { OwnedFd::from_raw_fd(LISTENER_FD) };
    use nix::sys::socket::{
        AddressFamily, SockType, SockaddrLike, UnixAddr, getsockname, getsockopt, sockopt,
    };
    let stream = getsockopt(&fd, sockopt::SockType).map(|t| t == SockType::Stream);
    let listening = getsockopt(&fd, sockopt::AcceptConn);
    let unix =
        getsockname::<UnixAddr>(fd.as_raw_fd()).map(|a| a.family() == Some(AddressFamily::Unix));
    match (stream, listening, unix) {
        (Ok(true), Ok(true), Ok(true)) => Ok(UnixListener::from(fd)),
        _ => Err("descriptor 3 is not a listening Unix stream socket".into()),
    }
}

impl Service {
    fn production() -> Result<Service, String> {
        let own_uid = nix::unistd::getuid().as_raw();
        if own_uid == 0 {
            return Err(
                "the service runs under the weaver-admin account, never as root, where every grant read answers granted"
                    .into(),
            );
        }
        let connector_uid = match nix::unistd::User::from_name(CONNECTOR) {
            Ok(Some(user)) => user.uid.as_raw(),
            _ => {
                return Err(format!(
                    "the admin connector's account {CONNECTOR} does not resolve"
                ));
            }
        };
        // **The root program's path is the one the rule names**:
        // `create-agent.sh` writes the rule with the resolved path of the
        // `weaver-admin` beside this binary, so the service resolves the same.
        let exe = std::env::current_exe().map_err(|e| format!("its own path is unread: {e}"))?;
        let beside = exe
            .parent()
            .ok_or("its own path has no directory")?
            .join("weaver-admin");
        let admin = std::fs::canonicalize(&beside).map_err(|e| {
            format!(
                "the root program {} does not resolve: {e}",
                beside.display()
            )
        })?;
        let instance = random::random_hex(8).map_err(|e| format!("no instance is drawn: {e}"))?;
        Ok(Service {
            sudo: PathBuf::from(SUDO),
            admin,
            base: PathBuf::from(DEFAULT_BASE),
            root_owner: 0,
            connector_uid,
            own_uid,
            instance,
            timing: PRODUCTION,
            connections_bound: CONNECTIONS,
            ids: Box::new(|| random::random_hex(8)),
            clock: Box::new(wall_ms),
            log: Mutex::new(Box::new(std::io::stderr())),
            agents: Mutex::new(HashMap::new()),
            connections: AtomicUsize::new(0),
        })
    }

    /// **The service's own log**: one JSON line per event, to its standard
    /// error, which the unit hands the journal. It names refused peers, keeps
    /// sudo's standard error for every grant read that did not answer granted,
    /// and holds one line per run with its principal (section 8.4).
    fn record(&self, event: &str, fields: &[(&str, String)]) {
        let mut pairs = vec![
            ("wall_ms", wall_ms().to_string()),
            ("actor", js("service")),
            ("event", js(event)),
        ];
        pairs.extend(fields.iter().map(|(k, v)| (*k, v.clone())));
        let mut line = object(&pairs);
        line.push('\n');
        if let Ok(mut log) = self.log.lock() {
            let _ = log.write_all(line.as_bytes());
            let _ = log.flush();
        }
    }

    fn open_line(&self) -> String {
        message(
            "open",
            &[
                ("version", VERSION.to_string()),
                ("instance", js(&self.instance)),
            ],
        )
    }

    /// **An agent's entry is made only for an agent that exists**: one whose
    /// root stands as a folder under the base. A well-formed name of no agent
    /// is answered from an empty entry that is not kept, so it leaves nothing
    /// behind in the map.
    fn with_agent<T>(&self, agent: &str, f: impl FnOnce(&mut Agent) -> T) -> T {
        let mut agents = self.agents.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(entry) = agents.get_mut(agent) {
            return f(entry);
        }
        if self.agent_exists(agent) {
            return f(agents.entry(agent.to_string()).or_default());
        }
        f(&mut Agent::default())
    }

    fn agent_exists(&self, agent: &str) -> bool {
        std::fs::symlink_metadata(self.base.join(agent)).is_ok_and(|m| m.is_dir())
    }
}

// ---------------------------------------------------------------------------
// Lines
// ---------------------------------------------------------------------------

/// A string as JSON text.
fn js(text: &str) -> String {
    Value::from(text).to_string()
}

/// An object from members already rendered as JSON text, in the order given,
/// so every line the service sends has one byte form.
fn object(pairs: &[(&str, String)]) -> String {
    let body: Vec<String> = pairs
        .iter()
        .map(|(k, v)| format!("{}:{v}", js(k)))
        .collect();
    format!("{{{}}}", body.join(","))
}

/// An externally tagged message: `{"<tag>":{...}}`.
fn message(tag: &str, pairs: &[(&str, String)]) -> String {
    format!("{{{}:{}}}", js(tag), object(pairs))
}

/// A refusal of the service's own, section 8.2: always this line.
struct Refusal {
    word: &'static str,
    detail: String,
    extra: Option<(&'static str, String)>,
}

impl Refusal {
    fn new(word: &'static str, detail: impl Into<String>) -> Self {
        Refusal {
            word,
            detail: detail.into(),
            extra: None,
        }
    }

    fn line(&self) -> String {
        let mut pairs = vec![("word", js(self.word)), ("detail", js(&self.detail))];
        if let Some((k, v)) = &self.extra {
            pairs.push((k, v.clone()));
        }
        message("refused", &pairs)
    }
}

fn send(stream: &mut UnixStream, line: &str) -> std::io::Result<()> {
    let mut bytes = Vec::with_capacity(line.len() + 1);
    bytes.extend_from_slice(line.as_bytes());
    bytes.push(b'\n');
    stream.write_all(&bytes)
}

/// Sends a refusal and closes the connection.
fn refuse(mut stream: UnixStream, refusal: Refusal) {
    let _ = send(&mut stream, &refusal.line());
    let _ = stream.shutdown(std::net::Shutdown::Both);
}

/// **Removes the whitespace outside strings** from one JSON value's text and
/// nothing else, so an object the root program printed travels as it printed
/// it, member order and number text kept, on one line.
fn compact(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let (mut inside, mut escaped) = (false, false);
    for c in text.chars() {
        if inside {
            out.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                inside = false;
            }
        } else if c == '"' {
            inside = true;
            out.push(c);
        } else if !matches!(c, ' ' | '\t' | '\n' | '\r') {
            out.push(c);
        }
    }
    out
}

enum LineError {
    TooLong,
    Late,
    Closed,
}

/// **One line of at most 4096 bytes with its newline**, from `pending` and
/// then the stream, by `deadline` where one is given. Bytes past the line stay
/// in `pending`.
fn read_line(
    stream: &mut UnixStream,
    pending: &mut Vec<u8>,
    deadline: Option<Instant>,
) -> Result<Vec<u8>, LineError> {
    loop {
        if let Some(at) = pending.iter().position(|b| *b == b'\n') {
            if at + 1 > LINE_BOUND {
                return Err(LineError::TooLong);
            }
            let mut line: Vec<u8> = pending.drain(..=at).collect();
            line.pop();
            return Ok(line);
        }
        if pending.len() >= LINE_BOUND {
            return Err(LineError::TooLong);
        }
        let wait = match deadline {
            None => None,
            Some(at) => match at.checked_duration_since(Instant::now()) {
                Some(left) if !left.is_zero() => Some(left),
                _ => return Err(LineError::Late),
            },
        };
        if stream.set_read_timeout(wait).is_err() {
            return Err(LineError::Closed);
        }
        let mut chunk = [0_u8; LINE_BOUND];
        match stream.read(&mut chunk) {
            Ok(0) => return Err(LineError::Closed),
            Ok(n) => pending.extend_from_slice(&chunk[..n]),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e)
                if deadline.is_some()
                    && matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
            {
                return Err(LineError::Late);
            }
            Err(_) => return Err(LineError::Closed),
        }
    }
}

// ---------------------------------------------------------------------------
// Admission and the opening line
// ---------------------------------------------------------------------------

fn peer_uid(stream: &UnixStream) -> Option<u32> {
    nix::sys::socket::getsockopt(stream, nix::sys::socket::sockopt::PeerCredentials)
        .ok()
        .map(|c| c.uid())
}

/// A connection's place in the service's bound of section 8.1.
struct Slot(Arc<Service>);

impl Slot {
    fn take(service: &Arc<Service>) -> Option<Slot> {
        service
            .connections
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| {
                (n < service.connections_bound).then_some(n + 1)
            })
            .ok()
            .map(|_| Slot(Arc::clone(service)))
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        self.0.connections.fetch_sub(1, Ordering::SeqCst);
    }
}

/// **The accept loop**: the peer judged by its uid before any byte is read
/// or sent, then the service's bound, then a thread per connection.
fn serve(service: Arc<Service>, listener: UnixListener) -> std::io::Error {
    loop {
        let stream = match listener.accept() {
            Ok((stream, _)) => stream,
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::Interrupted | std::io::ErrorKind::ConnectionAborted
                ) =>
            {
                continue;
            }
            Err(e)
                if matches!(
                    e.raw_os_error(),
                    Some(
                        nix::libc::EMFILE
                            | nix::libc::ENFILE
                            | nix::libc::ENOBUFS
                            | nix::libc::ENOMEM
                    )
                ) =>
            {
                service.record("accept_refused", &[("detail", js(&e.to_string()))]);
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }
            Err(e) => return e,
        };
        match peer_uid(&stream) {
            Some(uid) if uid == service.connector_uid => {}
            Some(uid) => {
                service.record("refused_peer", &[("peer_uid", uid.to_string())]);
                continue;
            }
            None => {
                service.record(
                    "refused_peer",
                    &[("detail", js("its credentials are unread"))],
                );
                continue;
            }
        }
        let Some(slot) = Slot::take(&service) else {
            refuse(
                stream,
                Refusal::new("full", "the service holds its bound of connections"),
            );
            continue;
        };
        let accepted = Instant::now();
        let spawned = std::thread::Builder::new()
            .name("connection".into())
            .spawn(move || {
                let service = Arc::clone(&slot.0);
                connection(&service, stream, accepted);
                drop(slot);
            });
        if let Err(e) = spawned {
            service.record("thread_refused", &[("detail", js(&e.to_string()))]);
        }
    }
}

struct Opening {
    purpose: &'static str,
    agent: String,
}

/// **The opening line judged**, section 8.2, in the order of its table:
/// shape and version, then a version above this one, then the purpose, then
/// the name.
fn judge_opening(line: &[u8]) -> Result<Opening, Refusal> {
    let malformed = |detail: &str| Refusal::new("opening_malformed", detail);
    let value: Value =
        serde_json::from_slice(line).map_err(|_| malformed("the line does not parse"))?;
    let Value::Object(top) = value else {
        return Err(malformed("the line is not an object"));
    };
    let body = match (top.len(), top.get("weaver_admin")) {
        (1, Some(Value::Object(body))) => body,
        _ => return Err(malformed("the line's one member is not weaver_admin")),
    };
    let version = body
        .get("version")
        .and_then(Value::as_u64)
        .filter(|v| *v >= 1)
        .ok_or_else(|| malformed("the version is not a positive integer"))?;
    let purpose = body
        .get("purpose")
        .and_then(Value::as_str)
        .ok_or_else(|| malformed("the line lacks its purpose"))?;
    let agent = body
        .get("agent")
        .and_then(Value::as_str)
        .ok_or_else(|| malformed("the line lacks its agent"))?;
    if version > VERSION {
        let mut refusal = Refusal::new(
            "version_unknown",
            format!("this service speaks version {VERSION}"),
        );
        refusal.extra = Some(("versions", format!("[{VERSION}]")));
        return Err(refusal);
    }
    let Some(purpose) = PURPOSES.iter().find(|p| **p == purpose) else {
        let mut refusal = Refusal::new("purpose_unknown", "the purpose is not one of version 1's");
        let list: Vec<String> = PURPOSES.iter().map(|p| js(p)).collect();
        refusal.extra = Some(("purposes", format!("[{}]", list.join(","))));
        return Err(refusal);
    };
    if !name_rule::well_formed(agent) {
        return Err(Refusal::new(
            "name_refused",
            "the name fails the root program's rule",
        ));
    }
    Ok(Opening {
        purpose,
        agent: agent.to_string(),
    })
}

fn connection(service: &Arc<Service>, mut stream: UnixStream, accepted: Instant) {
    let mut pending = Vec::new();
    let line = match read_line(
        &mut stream,
        &mut pending,
        Some(accepted + service.timing.opening),
    ) {
        Ok(line) => line,
        Err(LineError::Late) => {
            return refuse(
                stream,
                Refusal::new(
                    "opening_late",
                    "no complete line within 10 seconds of the connect",
                ),
            );
        }
        Err(LineError::TooLong) => {
            return refuse(
                stream,
                Refusal::new("opening_malformed", "the line is past 4096 bytes"),
            );
        }
        Err(LineError::Closed) => {
            return refuse(
                stream,
                Refusal::new("opening_malformed", "the line ends without a newline"),
            );
        }
    };
    let arrived = Instant::now();
    let opening = match judge_opening(&line) {
        Ok(opening) => opening,
        Err(refusal) => return refuse(stream, refusal),
    };
    let _ = stream.set_read_timeout(None);
    match opening.purpose {
        "control" => control(service, stream, &opening.agent, pending),
        "log" => log(service, stream, &opening.agent, pending, arrived),
        _ => trace(service, stream, &opening.agent, pending, arrived),
    }
}

// ---------------------------------------------------------------------------
// The agent's root
// ---------------------------------------------------------------------------

/// Opens a path without following a link at its last component.
fn open_nofollow(path: &Path, extra: nix::libc::c_int) -> std::io::Result<std::fs::File> {
    let c_path = std::ffi::CString::new(path.as_os_str().as_encoded_bytes())
        .map_err(std::io::Error::other)?;
    let flags = nix::libc::O_RDONLY | nix::libc::O_NOFOLLOW | nix::libc::O_CLOEXEC | extra;
    // SAFETY: open with a NUL-terminated path.
    let fd = unsafe { nix::libc::open(c_path.as_ptr(), flags) };
    if fd == -1 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: the number was just opened and is owned by nothing else.
    Ok(std::fs::File::from(unsafe { OwnedFd::from_raw_fd(fd) }))
}

impl Service {
    /// **One key of the agent's root**, `<base>/<agent>/<key>`, read only from
    /// a root and a file that are root's and that no other principal can
    /// write, as the root program admits a root (`weaver-admin-Spec` section
    /// 9): a value names where the service dials and what it reads.
    fn agent_key(&self, agent: &str, key: &str) -> Result<PathBuf, String> {
        let root = self.base.join(agent);
        let held = |m: &std::fs::Metadata| m.uid() == self.root_owner && m.mode() & 0o022 == 0;
        match std::fs::symlink_metadata(&root) {
            Ok(m) if m.is_dir() && held(&m) => {}
            Ok(_) => {
                return Err(format!(
                    "the agent's root {} is not root's alone",
                    root.display()
                ));
            }
            Err(e) => {
                return Err(format!(
                    "the agent's root {} is unread: {e}",
                    root.display()
                ));
            }
        }
        let path = root.join(key);
        let mut file =
            open_nofollow(&path, 0).map_err(|e| format!("{} is unread: {e}", path.display()))?;
        let meta = file
            .metadata()
            .map_err(|e| format!("{} is unread: {e}", path.display()))?;
        if !meta.is_file() || !held(&meta) {
            return Err(format!("{} is not root's alone", path.display()));
        }
        let mut text = String::new();
        (&mut file)
            .take(LINE_BOUND as u64)
            .read_to_string(&mut text)
            .map_err(|e| format!("{} is unread: {e}", path.display()))?;
        // **Trimmed as the root program trims its keys** (`main.rs`,
        // `load_service_config_from`), so the two name one path.
        let value = PathBuf::from(text.trim());
        if value.is_absolute() {
            Ok(value)
        } else {
            Err(format!("{} names no absolute path", path.display()))
        }
    }
}

// ---------------------------------------------------------------------------
// Control: the grants, a verb, the run
// ---------------------------------------------------------------------------

enum Ask {
    Grants,
    Verb {
        verb: &'static str,
        principal: String,
    },
    Run,
}

/// An ask judged: one that closes the connection, or one that leaves it standing.
enum Judged {
    Close(Refusal),
    Stand(Refusal),
}

/// **A principal is a claim for the record** (section 8.4): judged for its
/// shape alone, rendered for the service's own log, and passed nowhere else.
fn judge_principal(value: Option<&Value>) -> Option<String> {
    let Some(Value::Object(p)) = value else {
        return None;
    };
    match (p.get("kind").and_then(Value::as_str), p.len()) {
        (Some("server"), 1) => Some(object(&[("kind", js("server"))])),
        (Some("person"), 2) => {
            let id = p.get("person_id").and_then(Value::as_str)?;
            let hex = id.strip_prefix("pe-")?;
            (hex.len() == 16
                && hex
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
            .then(|| object(&[("kind", js("person")), ("person_id", js(id))]))
        }
        _ => None,
    }
}

fn judge_ask(line: &[u8]) -> Result<Ask, Judged> {
    let malformed = |detail: &str| Judged::Close(Refusal::new("ask_malformed", detail));
    let value: Value =
        serde_json::from_slice(line).map_err(|_| malformed("the line does not parse"))?;
    let Value::Object(top) = value else {
        return Err(malformed("the line is not an object"));
    };
    let (tag, body) = match top.iter().next() {
        Some((tag, Value::Object(body))) if top.len() == 1 => (tag.as_str(), body),
        _ => return Err(malformed("the line is not one member naming an ask")),
    };
    if body.contains_key("length") {
        return Err(malformed("an ask naming length is not of version 1"));
    }
    match tag {
        "grants" => Ok(Ask::Grants),
        "run" => Ok(Ask::Run),
        "verb" => {
            let principal = judge_principal(body.get("principal"))
                .ok_or_else(|| malformed("the principal is not of its shape"))?;
            let Some(name) = body.get("verb").and_then(Value::as_str) else {
                return Err(malformed("the verb is not named"));
            };
            match VERBS.iter().find(|v| **v == name) {
                Some(verb) => Ok(Ask::Verb { verb, principal }),
                None => Err(Judged::Stand(Refusal::new(
                    "verb_unknown",
                    "the verb is not one of the eight",
                ))),
            }
        }
        _ => Err(Judged::Stand(Refusal::new(
            "ask_unknown",
            "the ask is not one of version 1's",
        ))),
    }
}

/// The control connection's place in its agent's bound of 8.
/// A name of no agent holds no place: its connections are bounded by the
/// service's 1024 alone, and its slot counts nothing.
struct ControlSlot<'a> {
    service: &'a Service,
    agent: &'a str,
    counted: bool,
}

impl Drop for ControlSlot<'_> {
    fn drop(&mut self) {
        if self.counted {
            self.service
                .with_agent(self.agent, |a| a.control = a.control.saturating_sub(1));
        }
    }
}

fn control(service: &Arc<Service>, mut stream: UnixStream, agent: &str, mut pending: Vec<u8>) {
    let counted = service.agent_exists(agent);
    let admitted = !counted
        || service.with_agent(agent, |a| {
            (a.control < CONTROL_PER_AGENT)
                .then(|| a.control += 1)
                .is_some()
        });
    if !admitted {
        return refuse(
            stream,
            Refusal::new("full", "8 control connections stand for this agent"),
        );
    }
    let _slot = ControlSlot {
        service,
        agent,
        counted,
    };
    if send(&mut stream, &service.open_line()).is_err() {
        return;
    }
    loop {
        let line = match read_line(&mut stream, &mut pending, None) {
            Ok(line) => line,
            Err(LineError::TooLong) => {
                return refuse(
                    stream,
                    Refusal::new("ask_malformed", "the line is past 4096 bytes"),
                );
            }
            Err(_) => return,
        };
        let asked = Instant::now();
        let ask = match judge_ask(&line) {
            Ok(ask) => ask,
            Err(Judged::Close(refusal)) => return refuse(stream, refusal),
            Err(Judged::Stand(refusal)) => {
                if send(&mut stream, &refusal.line()).is_err() {
                    return;
                }
                continue;
            }
        };
        let stands = match ask {
            Ask::Run => send(&mut stream, &service.run_answer(agent)).is_ok(),
            Ask::Grants => {
                let (tx, rx) = mpsc::channel();
                let (svc, name) = (Arc::clone(service), agent.to_string());
                std::thread::spawn(move || {
                    let _ = tx.send(svc.grants(&name, asked));
                });
                match outstanding(&mut stream, &pending, &rx) {
                    Waited::Got(line) => send(&mut stream, &line).is_ok(),
                    Waited::Outstanding => return refuse_outstanding(stream),
                    Waited::Closed => false,
                }
            }
            Ask::Verb { verb, principal } => {
                let watch = match stream.try_clone() {
                    Ok(watch) => watch,
                    Err(e) => {
                        let refusal = Refusal::new(
                            "not_started",
                            format!("the connection could not be watched, so nothing ran: {e}"),
                        );
                        if send(&mut stream, &refusal.line()).is_err() {
                            return;
                        }
                        continue;
                    }
                };
                let closed = Arc::new(AtomicBool::new(false));
                let (tx, rx) = mpsc::channel();
                let (svc, name, flag) =
                    (Arc::clone(service), agent.to_string(), Arc::clone(&closed));
                std::thread::spawn(move || {
                    svc.verb(&name, verb, &principal, asked, (&flag, &watch), &tx)
                });
                loop {
                    match outstanding(&mut stream, &pending, &rx) {
                        Waited::Got(Event::Started(line)) => {
                            if send(&mut stream, &line).is_err() {
                                break false;
                            }
                        }
                        Waited::Got(Event::Final(line)) => break send(&mut stream, &line).is_ok(),
                        Waited::Outstanding => {
                            closed.store(true, Ordering::SeqCst);
                            return refuse_outstanding(stream);
                        }
                        Waited::Closed => {
                            closed.store(true, Ordering::SeqCst);
                            break false;
                        }
                    }
                }
            }
        };
        if !stands {
            return;
        }
    }
}

fn refuse_outstanding(stream: UnixStream) {
    refuse(
        stream,
        Refusal::new(
            "ask_outstanding",
            "a line arrived while an ask is outstanding",
        ),
    );
}

enum Waited<T> {
    Got(T),
    Outstanding,
    Closed,
}

/// **While an ask is outstanding**, the connection is watched as well as the
/// work: a byte from the connector is a line sent while an ask is outstanding
/// (section 8.4), and its close is the close of K3 to K6. Bytes already
/// pending past the ask's line are the same.
fn outstanding<T>(stream: &mut UnixStream, pending: &[u8], rx: &mpsc::Receiver<T>) -> Waited<T> {
    if !pending.is_empty() {
        return Waited::Outstanding;
    }
    loop {
        match rx.try_recv() {
            Ok(got) => return Waited::Got(got),
            Err(mpsc::TryRecvError::Disconnected) => return Waited::Closed,
            Err(mpsc::TryRecvError::Empty) => {}
        }
        let mut fd = nix::libc::pollfd {
            fd: stream.as_raw_fd(),
            events: nix::libc::POLLIN,
            revents: 0,
        };
        // SAFETY: poll on one pollfd this frame owns, for a descriptor it borrows.
        match unsafe { nix::libc::poll(&mut fd, 1, 20) } {
            0 => continue,
            -1 if nix::errno::Errno::last() == nix::errno::Errno::EINTR => continue,
            -1 => return Waited::Closed,
            _ => {}
        }
        let mut byte = [0_u8; 1];
        let _ = stream.set_read_timeout(Some(Duration::from_millis(1)));
        let read = stream.read(&mut byte);
        let _ = stream.set_read_timeout(None);
        match read {
            Ok(0) => return Waited::Closed,
            Ok(_) => return Waited::Outstanding,
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock
                        | std::io::ErrorKind::TimedOut
                        | std::io::ErrorKind::Interrupted
                ) => {}
            Err(_) => return Waited::Closed,
        }
    }
}

enum Event {
    Started(String),
    Final(String),
}

/// **The connection's state now**, read before a run is taken (section 8.4,
/// S4). Only a peek that would block is a standing connection: one that
/// finds the connector's close, a byte sent while the ask is outstanding, or
/// an error is not.
fn still_open(stream: &UnixStream) -> bool {
    let mut byte = [0_u8; 1];
    loop {
        // SAFETY: a peek of at most one byte into a buffer this frame owns, on
        // a descriptor it borrows.
        let n = unsafe {
            nix::libc::recv(
                stream.as_raw_fd(),
                byte.as_mut_ptr().cast(),
                1,
                nix::libc::MSG_PEEK | nix::libc::MSG_DONTWAIT,
            )
        };
        if n != -1 {
            return false;
        }
        match nix::errno::Errno::last() {
            nix::errno::Errno::EINTR => continue,
            nix::errno::Errno::EAGAIN => return true,
            _ => return false,
        }
    }
}

enum Grant {
    Granted,
    NotGranted,
    Unread(String),
}

enum Bounded {
    Exited(ExitStatus, String),
    NotStarted(String),
    Late(String),
}

/// The kept head of a stream and its whole length.
#[derive(Default)]
struct Kept {
    bytes: Vec<u8>,
    total: usize,
}

/// Reads a pipe to its end, keeping its first `keep` bytes and counting all.
fn drain(mut pipe: impl Read, keep: usize, into: Arc<Mutex<Kept>>, done: mpsc::Sender<()>) {
    let mut chunk = [0_u8; 8192];
    loop {
        match pipe.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                let mut kept = into.lock().unwrap_or_else(|p| p.into_inner());
                let room = keep.saturating_sub(kept.bytes.len()).min(n);
                kept.bytes.extend_from_slice(&chunk[..room]);
                kept.total += n;
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => break,
        }
    }
    let _ = done.send(());
}

fn child_command(program: &Path) -> Command {
    let mut command = Command::new(program);
    command
        .env_clear()
        .env("PATH", CHILD_PATH)
        .stdin(Stdio::null());
    command
}

impl Service {
    /// **One grant's read**, section 8.4: `sudo -n -l` on the exact fixed
    /// line, run as the service's own account, by `deadline`. Exit 0 is
    /// granted, exit 1 is not, anything else is unread. sudo's standard error
    /// goes to the service's own log for every read that did not answer
    /// granted.
    fn read_grant(&self, agent: &str, verb: &str, deadline: Instant) -> Grant {
        let mut command = child_command(&self.sudo);
        command
            .arg("-n")
            .arg("-l")
            .arg(&self.admin)
            .arg(verb)
            .arg(agent);
        command.stdout(Stdio::null()).stderr(Stdio::piped());
        let answer = match command.spawn() {
            Err(e) => Bounded::NotStarted(e.to_string()),
            Ok(mut child) => {
                let kept = Arc::new(Mutex::new(Kept::default()));
                let (done, _wait) = mpsc::channel();
                if let Some(pipe) = child.stderr.take() {
                    let into = Arc::clone(&kept);
                    std::thread::spawn(move || drain(pipe, STDERR_KEPT, into, done));
                }
                loop {
                    match child.try_wait() {
                        Ok(Some(status)) => {
                            let _ = _wait.recv_timeout(PIPE_DRAIN);
                            let kept = kept.lock().unwrap_or_else(|p| p.into_inner());
                            break Bounded::Exited(
                                status,
                                String::from_utf8_lossy(&kept.bytes).into_owned(),
                            );
                        }
                        Ok(None) if Instant::now() >= deadline => {
                            let _ = child.kill();
                            let _ = child.wait();
                            // **What sudo said before the deadline is kept**:
                            // the bytes drained so far, without waiting for a
                            // descendant that may hold the pipe.
                            let kept = kept.lock().unwrap_or_else(|p| p.into_inner());
                            break Bounded::Late(String::from_utf8_lossy(&kept.bytes).into_owned());
                        }
                        Ok(None) => std::thread::sleep(Duration::from_millis(5)),
                        Err(e) => break Bounded::NotStarted(e.to_string()),
                    }
                }
            }
        };
        let (grant, exit, stderr) = match answer {
            Bounded::Exited(status, stderr) => match status.code() {
                Some(0) => return Grant::Granted,
                Some(1) => (Grant::NotGranted, "1".to_string(), stderr),
                Some(code) => (
                    Grant::Unread(format!("the read exited {code}")),
                    code.to_string(),
                    stderr,
                ),
                None => (
                    Grant::Unread("the read ended by a signal".into()),
                    js("signal"),
                    stderr,
                ),
            },
            Bounded::NotStarted(e) => (
                Grant::Unread(format!("the read did not start: {e}")),
                js("not started"),
                e,
            ),
            Bounded::Late(stderr) => (
                Grant::Unread("the read did not answer within 10 seconds of the ask".into()),
                js("late"),
                stderr,
            ),
        };
        self.record(
            "grant_read",
            &[
                ("agent", js(agent)),
                ("verb", js(verb)),
                ("exit", exit),
                ("stderr", js(&stderr)),
            ],
        );
        grant
    }

    /// **The grants ask**, section 8.3: the eight reads, all by 10 seconds
    /// from the ask, or `grants_unread`, never a shorter list.
    fn grants(&self, agent: &str, asked: Instant) -> String {
        let deadline = asked + self.timing.grant;
        let mut granted = Vec::new();
        for verb in VERBS {
            match self.read_grant(agent, verb, deadline) {
                Grant::Granted => granted.push(js(verb)),
                Grant::NotGranted => {}
                Grant::Unread(detail) => return Refusal::new("grants_unread", detail).line(),
            }
        }
        message("grants", &[("verbs", format!("[{}]", granted.join(",")))])
    }

    /// The `run` ask's answer, machine A read (section 8.4).
    fn run_answer(&self, agent: &str) -> String {
        self.with_agent(agent, |a| {
            let standing: Vec<String> = a
                .standing
                .iter()
                .map(|r| {
                    object(&[
                        ("run", js(&r.id)),
                        ("verb", js(r.verb)),
                        (
                            "started_ms",
                            r.started_ms.map_or("null".into(), |ms| ms.to_string()),
                        ),
                    ])
                })
                .collect();
            message(
                "run",
                &[
                    ("instance", js(&self.instance)),
                    ("standing", format!("[{}]", standing.join(","))),
                    ("ended", a.ended.clone().unwrap_or_else(|| "null".into())),
                ],
            )
        })
    }

    /// **Machine A takes a run**, section 8.8: a `show` beside anything and
    /// held by nothing, a `force-unload` beside a lone `unload`, any other
    /// verb only where none stands. The id is drawn here, before the root
    /// program starts.
    fn take_run(&self, agent: &str, verb: &'static str) -> Result<String, Refusal> {
        self.with_agent(agent, |a| {
            if verb != "show" {
                let admits = match a.standing.as_slice() {
                    [] => true,
                    [only] => only.verb == "unload" && verb == "force-unload",
                    _ => false,
                };
                if !admits {
                    let mut refusal = Refusal::new("busy", "another run of the agent stands");
                    refusal.extra = Some(("run", js(&a.standing[0].id)));
                    return Err(refusal);
                }
            }
            let id = (self.ids)()
                .map_err(|e| Refusal::new("not_started", format!("no run id is drawn: {e}")))?;
            if verb != "show" {
                a.standing.push(Run {
                    id: id.clone(),
                    verb,
                    started_ms: None,
                });
            }
            Ok(id)
        })
    }

    /// **A verb ask**, section 8.4: the grant read in the request, then
    /// machine A, then the root program through sudo, carried to its end
    /// whether or not the connection stands.
    fn verb(
        &self,
        agent: &str,
        verb: &'static str,
        principal: &str,
        asked: Instant,
        (closed, watch): (&AtomicBool, &UnixStream),
        tx: &mpsc::Sender<Event>,
    ) {
        let refused = |refusal: Refusal| {
            let _ = tx.send(Event::Final(refusal.line()));
        };
        match self.read_grant(agent, verb, asked + self.timing.grant) {
            Grant::Granted => {}
            Grant::NotGranted => {
                return refused(Refusal::new(
                    "not_granted",
                    "the verb's grant read answers not granted",
                ));
            }
            Grant::Unread(detail) => return refused(Refusal::new("grant_unread", detail)),
        }
        // **A close before the read answers starts nothing**: the flag the
        // control thread sets, and the connection's own state read now, so a
        // close the control thread has not yet seen starts nothing either.
        if closed.load(Ordering::SeqCst) || !still_open(watch) {
            return;
        }
        let id = match self.take_run(agent, verb) {
            Ok(id) => id,
            Err(refusal) => return refused(refusal),
        };
        let mut command = child_command(&self.sudo);
        command.arg("-n").arg(&self.admin).arg(verb).arg(agent);
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(e) => {
                self.with_agent(agent, |a| a.standing.retain(|r| r.id != id));
                self.record_run(&id, agent, verb, principal, "not_started");
                return refused(Refusal::new(
                    "not_started",
                    format!("the root program could not be started: {e}"),
                ));
            }
        };
        let started_ms = (self.clock)();
        self.with_agent(agent, |a| {
            if let Some(run) = a.standing.iter_mut().find(|r| r.id == id) {
                run.started_ms = Some(started_ms);
            }
        });
        let _ = tx.send(Event::Started(message("started", &[("run", js(&id))])));
        let out = Arc::new(Mutex::new(Kept::default()));
        let err = Arc::new(Mutex::new(Kept::default()));
        let (done, finished) = mpsc::channel();
        for (pipe, keep, into) in [
            (
                child
                    .stdout
                    .take()
                    .map(|p| Box::new(p) as Box<dyn Read + Send>),
                OBJECT_BOUND + 1,
                &out,
            ),
            (
                child
                    .stderr
                    .take()
                    .map(|p| Box::new(p) as Box<dyn Read + Send>),
                STDERR_KEPT,
                &err,
            ),
        ] {
            if let Some(pipe) = pipe {
                let (into, done) = (Arc::clone(into), done.clone());
                std::thread::spawn(move || drain(pipe, keep, into, done));
            }
        }
        drop(done);
        let status = child.wait();
        let until = Instant::now() + PIPE_DRAIN;
        for _ in 0..2 {
            let left = until.saturating_duration_since(Instant::now());
            if finished.recv_timeout(left).is_err() {
                break;
            }
        }
        let out = std::mem::take(&mut *out.lock().unwrap_or_else(|p| p.into_inner()));
        let err = std::mem::take(&mut *err.lock().unwrap_or_else(|p| p.into_inner()));
        let (line, outcome) = final_line(&id, verb, status.ok(), &out, &err);
        if verb != "show" {
            self.with_agent(agent, |a| {
                a.standing.retain(|r| r.id != id);
                a.ended = Some(line.clone());
            });
        }
        self.record_run(&id, agent, verb, principal, &outcome);
        let _ = tx.send(Event::Final(line));
    }

    fn record_run(&self, id: &str, agent: &str, verb: &str, principal: &str, outcome: &str) {
        self.record(
            "run",
            &[
                ("run", js(id)),
                ("agent", js(agent)),
                ("verb", js(verb)),
                ("principal", principal.to_string()),
                ("outcome", js(outcome)),
            ],
        );
    }
}

/// **The run's final line**, section 8.4: `ran` where the root program
/// exited 0 or 1 with one JSON object of at most 64 KiB in UTF-8, carried as
/// it printed it, else `fault`. Returns the line and the outcome for the
/// service's own log.
fn final_line(
    id: &str,
    verb: &str,
    status: Option<ExitStatus>,
    out: &Kept,
    err: &Kept,
) -> (String, String) {
    let fault = |detail: String| {
        let line = message(
            "fault",
            &[("run", js(id)), ("verb", js(verb)), ("detail", js(&detail))],
        );
        (line, format!("fault: {detail}"))
    };
    let code = match status.map(|s| (s.code(), std::os::unix::process::ExitStatusExt::signal(&s))) {
        Some((Some(code), _)) => code,
        Some((None, Some(signal))) => {
            return fault(format!("the root program ended by signal {signal}"));
        }
        _ => return fault("the root program's end is unread".into()),
    };
    if code != 0 && code != 1 {
        return fault(format!("the root program exited {code}"));
    }
    if out.total > OBJECT_BOUND {
        return fault("its standard output is past 64 KiB".into());
    }
    let Ok(text) = std::str::from_utf8(&out.bytes) else {
        return fault("its standard output is not UTF-8".into());
    };
    let mut values = serde_json::Deserializer::from_str(text).into_iter::<Value>();
    match (values.next(), values.next()) {
        (None, _) => return fault("its standard output carries no object".into()),
        (Some(Err(_)), _) => return fault("its standard output is not one JSON object".into()),
        (Some(Ok(_)), Some(_)) => {
            return fault("its standard output carries more than one value".into());
        }
        (Some(Ok(_)), None) => {}
    }
    if !text.trim_start().starts_with('{') {
        return fault("its standard output carries no object".into());
    }
    let stderr = String::from_utf8_lossy(&err.bytes);
    let line = message(
        "ran",
        &[
            ("run", js(id)),
            ("verb", js(verb)),
            ("exit_code", code.to_string()),
            ("answer", compact(text)),
            ("stderr", js(&stderr)),
            ("stderr_cut", (err.total > STDERR_KEPT).to_string()),
        ],
    );
    (line, format!("ran, exit {code}"))
}

// ---------------------------------------------------------------------------
// The trace
// ---------------------------------------------------------------------------

/// **The trace**, section 8.5: the agent's relay dialled before the opening
/// is answered, `open` within 1 second of the dial's connect, then the bytes
/// each way forwarded as they arrive, the service holding none.
fn trace(
    service: &Arc<Service>,
    mut stream: UnixStream,
    agent: &str,
    pending: Vec<u8>,
    arrived: Instant,
) {
    let unreachable = |detail: String| Refusal::new("relay_unreachable", detail);
    let root = match service.agent_key(agent, "coordination-root") {
        Ok(root) => root,
        Err(detail) => return refuse(stream, unreachable(detail)),
    };
    let door = root.join("weaver.run").join(agent).join("trace.sock");
    let (tx, rx) = mpsc::channel();
    let dialled = door.clone();
    std::thread::spawn(move || {
        let _ = tx.send(UnixStream::connect(&dialled));
    });
    let left = (arrived + service.timing.judge_cut).saturating_duration_since(Instant::now());
    let relay = match rx.recv_timeout(left) {
        Ok(Ok(relay)) => relay,
        Ok(Err(e)) => {
            return refuse(
                stream,
                unreachable(format!("no relay answers at the agent's door: {e}")),
            );
        }
        Err(_) => {
            return refuse(
                stream,
                unreachable("the relay's dial did not connect within 8 seconds".into()),
            );
        }
    };
    let connected = Instant::now();
    let window = service.timing.open_after_connect;
    match window.checked_sub(connected.elapsed()) {
        Some(left) if !left.is_zero() => {
            let _ = stream.set_write_timeout(Some(left));
        }
        _ => {
            drop(relay);
            return refuse(
                stream,
                unreachable("open could not be sent within 1 second of the connect".into()),
            );
        }
    }
    if send(&mut stream, &service.open_line()).is_err() {
        return;
    }
    let _ = stream.set_write_timeout(None);
    splice(stream, relay, pending);
}

fn forward(mut from: UnixStream, mut to: UnixStream) {
    let mut chunk = vec![0_u8; 64 * 1024];
    loop {
        match from.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                if to.write_all(&chunk[..n]).is_err() {
                    break;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => break,
        }
    }
    let _ = from.shutdown(std::net::Shutdown::Both);
    let _ = to.shutdown(std::net::Shutdown::Both);
}

/// Forwards each way until either side closes, then closes the other.
fn splice(client: UnixStream, mut relay: UnixStream, pending: Vec<u8>) {
    if !pending.is_empty() && relay.write_all(&pending).is_err() {
        return;
    }
    let (Ok(client_in), Ok(relay_out)) = (client.try_clone(), relay.try_clone()) else {
        return;
    };
    let up = std::thread::spawn(move || forward(client_in, relay_out));
    forward(relay, client);
    let _ = up.join();
}

// ---------------------------------------------------------------------------
// The log
// ---------------------------------------------------------------------------

/// The extended attribute look of the root program's log check: an entry
/// beyond the mode refuses.
fn carries_access_entries(file: &std::fs::File) -> bool {
    for name in [c"system.posix_acl_access", c"system.posix_acl_default"] {
        // SAFETY: a size query with no buffer, on a descriptor this frame
        // borrows and a NUL-terminated name.
        let size = unsafe {
            nix::libc::fgetxattr(file.as_raw_fd(), name.as_ptr(), std::ptr::null_mut(), 0)
        };
        if size >= 0 {
            return true;
        }
        match nix::errno::Errno::last() {
            nix::errno::Errno::ENODATA | nix::errno::Errno::EOPNOTSUPP => {}
            _ => return true,
        }
    }
    false
}

impl Service {
    /// **The log judged**, section 8.6 and `weaver-admin-Spec` section 8:
    /// opened without following a link and non-blocking, a regular file with
    /// one link, root's, and carrying no access entry beyond its mode.
    fn judge_log(&self, path: &Path) -> Result<std::fs::File, String> {
        let file = open_nofollow(path, nix::libc::O_NONBLOCK)
            .map_err(|e| format!("{} is unread: {e}", path.display()))?;
        let meta = file
            .metadata()
            .map_err(|e| format!("{} is unread: {e}", path.display()))?;
        if !meta.is_file() {
            return Err(format!("{} is not a regular file", path.display()));
        }
        if meta.nlink() != 1 {
            return Err(format!("{} has a second link", path.display()));
        }
        if meta.uid() != self.root_owner {
            return Err(format!("{} is not root's", path.display()));
        }
        if carries_access_entries(&file) {
            return Err(format!(
                "{} carries an access entry beyond its mode",
                path.display()
            ));
        }
        Ok(file)
    }

    fn log_path(&self, agent: &str) -> Result<PathBuf, String> {
        Ok(self.agent_key(agent, "territory")?.join("admin.log"))
    }
}

/// **One appended line as its message**, section 8.6.
fn log_message(own_uid: u32, bytes: &[u8]) -> String {
    if bytes.len() > LOG_LINE_BOUND {
        return message("log", &[("oversized", bytes.len().to_string())]);
    }
    if let Ok(text) = std::str::from_utf8(bytes)
        && let Ok(Value::Object(line)) = serde_json::from_str::<Value>(text)
    {
        let own = Some(u64::from(own_uid));
        let through = if line.get("actor").and_then(Value::as_str) == Some("relay") {
            line.get("peer_uid").and_then(Value::as_u64) == own
        } else {
            line.get("uid").and_then(Value::as_u64) == own
        };
        return message(
            "log",
            &[
                ("line", compact(text)),
                ("through_service", through.to_string()),
            ],
        );
    }
    message(
        "log",
        &[
            ("text", js(&String::from_utf8_lossy(bytes))),
            ("through_service", "false".into()),
        ],
    )
}

/// The lines of a file read as it grows: complete lines become messages, a
/// line past 8 KiB is counted and never kept, and where the reading starts
/// inside a line, its rest is skipped.
#[derive(Default)]
struct Lines {
    partial: Vec<u8>,
    oversized: Option<usize>,
    skipping: bool,
}

impl Lines {
    /// **Takes lines while the messages queued stand below `limit`**, so what
    /// is read is bounded by the messages it makes and not by its raw bytes:
    /// a line whose message is six times its length is read no further ahead
    /// than a JSON line. Answers the bytes taken, each piece whole.
    fn take(&mut self, own_uid: u32, bytes: &[u8], out: &mut Vec<u8>, limit: usize) -> usize {
        let mut taken = 0;
        for piece in bytes.split_inclusive(|b| *b == b'\n') {
            if out.len() >= limit {
                break;
            }
            taken += piece.len();
            let (body, complete) = match piece.strip_suffix(b"\n") {
                Some(body) => (body, true),
                None => (piece, false),
            };
            if self.skipping {
                self.skipping = !complete;
                continue;
            }
            if let Some(length) = self.oversized.as_mut() {
                *length += body.len();
            } else if self.partial.len() + body.len() > LOG_LINE_BOUND {
                self.oversized = Some(self.partial.len() + body.len());
                self.partial.clear();
            } else {
                self.partial.extend_from_slice(body);
            }
            if complete {
                let line = match self.oversized.take() {
                    Some(length) => message("log", &[("oversized", length.to_string())]),
                    None => log_message(own_uid, &self.partial),
                };
                out.extend_from_slice(line.as_bytes());
                out.push(b'\n');
                self.partial.clear();
            }
        }
        taken
    }
}

fn log(
    service: &Arc<Service>,
    mut stream: UnixStream,
    agent: &str,
    pending: Vec<u8>,
    arrived: Instant,
) {
    let unread = |detail: String| Refusal::new("log_unread", detail);
    let path = match service.log_path(agent) {
        Ok(path) => path,
        Err(detail) => return refuse(stream, unread(detail)),
    };
    let file = match service.judge_log(&path) {
        Ok(file) => file,
        Err(detail) => return refuse(stream, unread(detail)),
    };
    if arrived.elapsed() > service.timing.judge_cut {
        return refuse(
            stream,
            unread("the log was not judged within 8 seconds".into()),
        );
    }
    // **Lines appended from the moment `open` is answered** (section 8.6):
    // the length is read before `open` is sent, so a line appended after it
    // is carried, and a length that ends inside a line marks that line's rest
    // to be skipped.
    let start = match file.metadata() {
        Ok(meta) => meta.len(),
        Err(e) => return refuse(stream, unread(format!("{} is unread: {e}", path.display()))),
    };
    let mut last = [0_u8; 1];
    let inside_a_line = start > 0
        && file
            .read_at(&mut last, start - 1)
            .is_ok_and(|n| n == 1 && last[0] != b'\n');
    let generation = service.with_agent(agent, |a| {
        a.log_generation += 1;
        a.log_generation
    });
    if send(&mut stream, &service.open_line()).is_err() {
        return;
    }
    if !pending.is_empty() {
        return refuse(
            stream,
            Refusal::new("ask_malformed", "a byte arrived on a log connection"),
        );
    }
    tail(
        service,
        stream,
        (agent, generation),
        &path,
        file,
        (start, inside_a_line),
    );
}

/// **The log's lines as appended**, from `open`, with no history: the file
/// looked at every tick for a newer connection, a byte from the connector, a
/// replacement, a truncation and new bytes, and what is queued written as the
/// connector takes it.
fn tail(
    service: &Arc<Service>,
    mut stream: UnixStream,
    (agent, generation): (&str, u64),
    path: &Path,
    mut file: std::fs::File,
    (start, inside_a_line): (u64, bool),
) {
    let identity = |m: &std::fs::Metadata| (m.dev(), m.ino());
    let Ok(meta) = file.metadata() else { return };
    let mut offset = start;
    let mut ident = identity(&meta);
    let mut lines = Lines {
        skipping: inside_a_line,
        ..Lines::default()
    };
    let mut unsent: Vec<u8> = Vec::new();
    let mut mid_line = false;
    if stream.set_nonblocking(true).is_err() {
        return;
    }
    let discontinuity = |kind: &str, out: &mut Vec<u8>| {
        out.extend_from_slice(message("log", &[("discontinuity", js(kind))]).as_bytes());
        out.push(b'\n');
    };
    loop {
        if service.with_agent(agent, |a| a.log_generation) != generation {
            return;
        }
        let mut byte = [0_u8; 64];
        match stream.read(&mut byte) {
            Ok(0) => return,
            Ok(_) => {
                if !mid_line {
                    let _ = stream.write(
                        format!(
                            "{}\n",
                            Refusal::new("ask_malformed", "a byte arrived on a log connection")
                                .line()
                        )
                        .as_bytes(),
                    );
                }
                return;
            }
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ) => {}
            Err(_) => return,
        }
        let same = std::fs::symlink_metadata(path)
            .map(|m| identity(&m) == ident)
            .unwrap_or(false);
        if !same {
            match service.judge_log(path) {
                Ok(newer) => {
                    file = newer;
                    let Ok(m) = file.metadata() else { return };
                    ident = identity(&m);
                    offset = 0;
                    lines = Lines::default();
                    discontinuity("replaced", &mut unsent);
                }
                Err(_) => return,
            }
        }
        let Ok(len) = file.metadata().map(|m| m.len()) else {
            return;
        };
        if len < offset {
            offset = 0;
            lines = Lines::default();
            discontinuity("truncated", &mut unsent);
        }
        let mut chunk = vec![0_u8; 64 * 1024];
        while offset < len && unsent.len() < LOG_UNSENT {
            let want = ((len - offset) as usize).min(chunk.len());
            match file.read_at(&mut chunk[..want], offset) {
                Ok(0) => break,
                Ok(n) => {
                    let taken = lines.take(service.own_uid, &chunk[..n], &mut unsent, LOG_UNSENT);
                    offset += taken as u64;
                    if taken < n {
                        break;
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(_) => return,
            }
        }
        while !unsent.is_empty() {
            match stream.write(&unsent) {
                Ok(0) => return,
                Ok(n) => {
                    mid_line = unsent[n - 1] != b'\n';
                    unsent.drain(..n);
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(_) => return,
            }
        }
        if unsent.len() >= LOG_UNSENT {
            // **Best effort**: the refusal goes only where the connection
            // takes it whole after the line it is in.
            let behind = format!(
                "{}\n",
                Refusal::new("log_behind", "1 MiB stands unsent").line()
            );
            let rest = if mid_line {
                unsent
                    .iter()
                    .position(|b| *b == b'\n')
                    .map(|at| &unsent[..=at])
            } else {
                Some(&unsent[..0])
            };
            if let Some(rest) = rest
                && stream.write(rest).ok() == Some(rest.len())
            {
                let _ = stream.write(behind.as_bytes());
            }
            return;
        }
        std::thread::sleep(service.timing.log_tick);
    }
}

#[cfg(test)]
#[path = "weaver-admin-service/tests.rs"]
mod tests;
