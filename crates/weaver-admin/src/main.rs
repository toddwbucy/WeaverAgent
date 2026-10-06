//! conforms: admin-no-library-surface
//! conforms: admin-one-floor-link-types-config
//! conforms: admin-no-direct-traits-line
//! conforms: admin-no-runtime-no-bus-no-logging
//! conforms: admin-descriptors-owned-types
//! conforms: admin-runs-as-root-or-performs-nothing
//! conforms: admin-answer-and-exit-status-agree
//! conforms: admin-residency-is-not-lifecycle-state
//! conforms: admin-preload-name-follows-the-kind
//! conforms: admin-cloexec-atomic-at-creation
//! conforms: admin-publishes-only-on-ready
//! conforms: admin-validate-starts-no-process
//! conforms: admin-inventory-one-function
//! conforms: admin-unload-answers-after-confirmed-stop
//! conforms: admin-failed-dial-consults-unit-state
//!
//! `weaver-admin`: the lifecycle tool the admin role runs with root. One
//! binary and no library surface, per `weaver-admin-Spec` section 1 - nothing
//! links admin, and a library target would be an API for a consumer the
//! topology forbids.
//!
//! **One invocation, one verb, then exit.** The operator socket, its
//! accept-time predicate, and the fleet map retired with the service account
//! on 2026-08-05, and what replaces them is the process boundary the operating
//! system already draws around an executed program, per section 2.

/// **A diagnostic line on standard error that can never end the verb**, per
/// `weaver-admin-Spec` section 2: the invocation ignores `SIGPIPE`, so a write
/// to a closed or broken standard error fails with `EPIPE` instead of killing
/// the process, and `eprintln!` would panic on that failure part way through a
/// verb. This writes and discards the error, the line being diagnostics no
/// caller parses.
macro_rules! diag {
    ($($arg:tt)*) => {{
        use std::io::Write as _;
        let _ = writeln!(std::io::stderr(), $($arg)*);
    }};
}

mod channel;
mod inventory;
mod log;
mod sink;
mod stack;
mod start;
mod surface;
mod verbs;

/// A path under the temp directory for this crate's tests, removed when the
/// test ends, pass or fail: the guard drops on the unwind a failed assertion
/// takes as on a clean return, so no run leaves a directory behind (#690
/// item C2.9).
#[cfg(test)]
mod scratch {
    pub(crate) struct Scratch(pub(crate) std::path::PathBuf);

    impl Drop for Scratch {
        fn drop(&mut self) {
            match std::fs::symlink_metadata(&self.0) {
                Ok(meta) if meta.is_dir() => drop(std::fs::remove_dir_all(&self.0)),
                Ok(_) => drop(std::fs::remove_file(&self.0)),
                Err(_) => {}
            }
        }
    }

    impl std::ops::Deref for Scratch {
        type Target = std::path::Path;
        fn deref(&self) -> &std::path::Path {
            &self.0
        }
    }

    impl AsRef<std::path::Path> for Scratch {
        fn as_ref(&self) -> &std::path::Path {
            &self.0
        }
    }
}

use std::path::PathBuf;

use weaver_types::{AgentName, LifecycleAnswer, LifecycleDirective, LifecycleRefusal};

/// One agent's operator-installed configuration, read from that agent's own
/// root, per Spec section 9: the coordination root, the agent's binaries, the
/// optional values, the operator's declaration directory and uid, and the
/// boundary file. **These are deployment facts the operator installs** -
/// crossing no seam, and **none of them discovered at runtime by searching**.
/// Admin is one agent's organ, on the operator's ruling of 2026-10-01, so it
/// reads this agent's root and nothing shared.
struct ServiceConfig {
    /// The agent this configuration is, named by the root it was read from.
    agent: String,
    coordination_root: PathBuf,
    worker: PathBuf,
    spu: PathBuf,
    gate: PathBuf,
    headroom_bytes: Option<String>,
    /// The engine libraries' directory, judged as the root is, per section 6.
    library_path: Option<PathBuf>,
    /// The bound on the enter's answer, per section 2: 900 seconds, or the
    /// root's `load-bound-seconds`.
    load_bound: std::time::Duration,
    /// The operator's directory holding `agent.toml`, `admin.log` and
    /// `worker.log`, canonical as judged, per sections 8 and 9.
    declaration_directory: PathBuf,
    /// The operator's uid, the box's own fact about whose data defines the
    /// agent, per section 9.
    operator: u32,
    /// The declaration directory's group, the group the operator's logs take.
    operator_gid: u32,
    /// The boundary file, `roles.toml`, as read: its sha256 hex and its one
    /// trace reader, or why it did not read. **Required at `validate` and
    /// `load` alone**, per section 9: a damaged file never takes `unload`,
    /// `stop` or `show` from a running agent, whose log lines then carry no
    /// digest.
    boundary: Result<BoundaryRead, String>,
}

/// A boundary file that read and parsed.
#[derive(Debug, Clone)]
struct BoundaryRead {
    digest: String,
    reader: String,
}

/// The enter's bound where the root names none, per Spec section 2.
const DEFAULT_LOAD_BOUND: std::time::Duration = std::time::Duration::from_secs(900);

impl ServiceConfig {
    /// The per-agent socket the harness binds inside the agent's runtime
    /// directory. Admin resolves the same name to dial it, which is the one
    /// value that reaches two crates and the reason the operator's file is
    /// where they agree.
    fn coordination_socket(&self) -> PathBuf {
        start::runtime_directory(&self.coordination_root, &self.agent).join("coordination.sock")
    }

    /// The agent's root-owned run directory of section 3.
    fn run_directory(&self) -> PathBuf {
        start::run_directory(&self.coordination_root, &self.agent)
    }

    /// The boundary file's digest where it read, for the operations log.
    fn boundary_digest(&self) -> Option<&str> {
        self.boundary.as_ref().ok().map(|read| read.digest.as_str())
    }

    /// **The boundary file, required**, at `validate` and `load`: missing or
    /// malformed refuses `ConfigInvalid` naming `roles.toml`, per section 9.
    fn require_boundary(&self) -> Result<&BoundaryRead, LifecycleRefusal> {
        self.boundary.as_ref().map_err(|why| {
            diag!("weaver-admin: {why}");
            LifecycleRefusal::ConfigInvalid {
                field: Some(weaver_types::FieldName(BOUNDARY_FILE.to_string())),
            }
        })
    }

    /// The operations log, per section 8.
    fn admin_log(&self) -> PathBuf {
        self.declaration_directory.join("admin.log")
    }

    /// The worker's own log, per section 6, never the operations log.
    fn worker_log(&self) -> PathBuf {
        self.declaration_directory.join("worker.log")
    }

    /// Who owns the operator's logs: the `operator` uid and the declaration
    /// directory's group, set through the open descriptor, per section 8.
    fn operator_owner(&self) -> (u32, u32) {
        (self.operator, self.operator_gid)
    }
}

/// The one answer object on standard output, written and flushed with the
/// error discarded, so a closed or broken standard output leaves the exit
/// status, which still agrees with the object, as the answer's carrier.
fn say(object: &str) {
    use std::io::Write as _;
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{object}");
    let _ = out.flush();
}

fn main() {
    // **The first instruction ignores every catchable terminating signal**,
    // per Spec section 2, so a caller cancelling, timing out or hanging up,
    // which sudo relays, cannot end a verb part way.
    start::ignore_terminating_signals();
    let outcome = run();
    // **The answer is one JSON object on standard output and the exit status
    // agrees with it.** Zero exits an answer and a non-zero status exits a
    // refusal, so a shell reads the status and a tool reads the object and the
    // two never disagree.
    match outcome {
        Ok(answer) => {
            // The answer's write is discarded on failure, a caller that went
            // away leaving the verb's outcome in `admin.log`, per Spec section 2.
            say(&surface::render_answer(&answer));
            std::process::exit(0);
        }
        Err(refusal) => {
            say(&surface::render_refusal(&refusal));
            std::process::exit(surface::EXIT_REFUSED);
        }
    }
}

/// Who asked for this invocation's change, per `weaver-types-Spec` section
/// 3.1 and `weaver-admin-Spec` section 2: the uid sudo reports and nothing
/// else. `run` has already refused a malformed `SUDO_UID` before any verb, so
/// the read here cannot fail on the same environment.
fn invocation_cause() -> weaver_types::Cause {
    cause_from(std::env::var_os("SUDO_UID").as_deref())
        .expect("run refuses a malformed SUDO_UID before any verb")
}

/// **`SUDO_UID` parsed strictly as a decimal uid**, per `weaver-admin-Spec`
/// section 2: ASCII digits only, no sign, no space and no leading zero, within
/// `u32`. Absent, it is a root shell and the cause is uid 0. Malformed, it
/// refuses `Malformed` and never falls back, because a fallback would record
/// a cause nobody gave.
fn cause_from(sudo_uid: Option<&std::ffi::OsStr>) -> Result<weaver_types::Cause, LifecycleRefusal> {
    let Some(value) = sudo_uid else {
        return Ok(weaver_types::Cause { uid: 0 });
    };
    let bytes = value.as_encoded_bytes();
    let canonical = !bytes.is_empty()
        && bytes.iter().all(u8::is_ascii_digit)
        && (bytes == b"0" || bytes[0] != b'0');
    if !canonical {
        return Err(LifecycleRefusal::Malformed);
    }
    std::str::from_utf8(bytes)
        .ok()
        .and_then(|digits| digits.parse::<u32>().ok())
        .map(|uid| weaver_types::Cause { uid })
        .ok_or(LifecycleRefusal::Malformed)
}

fn run() -> Result<LifecycleAnswer, LifecycleRefusal> {
    // **Authorization is the kernel's, and what this crate checks is the
    // name.** The invocation runs as root or performs nothing: no predicate,
    // no allow set, and no deny set, the operator being the party the kernel
    // already admitted. This refusal is enacted before any verb touches
    // anything.
    if !surface::running_as_root() {
        return Err(LifecycleRefusal::Unauthorized);
    }
    // The cause is judged before any verb, so a malformed one refuses having
    // touched nothing.
    cause_from(std::env::var_os("SUDO_UID").as_deref())?;
    let request = surface::parse_arguments(std::env::args().skip(1))?;
    // **A refusal before the agent's root is admitted has no admin.log to
    // reach**, per Spec section 8: it goes to standard error alone.
    let config = load_service_config(request.agent()).inspect_err(|refusal| {
        diag!(
            "weaver-admin: refused before admission: {}",
            surface::render_refusal(refusal)
        );
    })?;
    dispatch(&config, request)
}

fn dispatch(
    config: &ServiceConfig,
    request: surface::Request,
) -> Result<LifecycleAnswer, LifecycleRefusal> {
    admissible(config, request.agent())?;
    // **The run directory is made before either lock is taken**, every verb's
    // first act after the root's admission, per Spec section 3, under a
    // coordination root judged closed first.
    prepare_run_directory(config, 0)?;
    let (verb, outcome) = match request {
        surface::Request::Validate(agent) => ("validate", validate(config, &agent)),
        surface::Request::Load(agent) => ("load", load(config, &agent)),
        surface::Request::Unload(_) => ("unload", unload(config)),
        surface::Request::Stop(_) => ("stop", stop(config)),
        surface::Request::Show(_) => ("show", show(config)),
    };
    record(
        config,
        verb,
        &match &outcome {
            Ok(answer) => surface::render_answer(answer),
            Err(refusal) => surface::render_refusal(refusal),
        },
    );
    outcome
}

/// **The coordination root is held closed before the run directory is made in
/// it**, per Spec section 3: the root itself owned by `owner`, uid 0 in
/// production, or by uid 0, and writable by no group or other, sticky or not,
/// since a sticky world-writable root would let any local user pre-create
/// `weaver.run` and squat the agent; every directory above it held closed by
/// section 9's ancestor rule. Otherwise the verb refuses `BoundaryUnverified`, since a
/// principal that could write the coordination root could rename `weaver.run/`
/// away and leave the next `load` a fresh `run.lock` while a run still holds
/// the old one. Then the run directory is made and judged, owned by `owner`.
fn prepare_run_directory(config: &ServiceConfig, owner: u32) -> Result<PathBuf, LifecycleRefusal> {
    use std::os::unix::fs::MetadataExt;
    let root = &config.coordination_root;
    let metadata = std::fs::symlink_metadata(root).map_err(|_| {
        diag!(
            "weaver-admin: the coordination root {} does not exist",
            root.display()
        );
        LifecycleRefusal::BoundaryUnverified
    })?;
    let closed = metadata.mode() & 0o022 == 0;
    let held = metadata.uid() == owner || metadata.uid() == 0;
    if !metadata.is_dir() || !held || !closed {
        diag!(
            "weaver-admin: the coordination root {} is not a directory {owner} holds closed",
            root.display()
        );
        return Err(LifecycleRefusal::BoundaryUnverified);
    }
    let canonical = judge_ancestors(root, &[owner, 0])?;
    start::prepare_run_directory(&canonical, &config.agent, owner)
}

/// **`validate` is the load's front half**, and it stops at the report: it
/// touches no seam and starts no process, which is what makes `Validated`
/// mean an outcome rather than a transition.
fn validate(
    config: &ServiceConfig,
    agent: &AgentName,
) -> Result<LifecycleAnswer, LifecycleRefusal> {
    let _invocation = start::take_invocation_lock(&config.run_directory())?;
    take_inventory(config, agent)?;
    Ok(LifecycleAnswer::Validated)
}

/// **The name is authorized before it reaches the filesystem**, and every verb
/// that builds a path from it calls this.
///
/// `AgentName` wraps a bare string, so a name carrying `/` or `..` would
/// otherwise be interpolated into a config path or a socket path and traverse
/// out of the directory the operator placed. A name that is not the agent this
/// configuration was read for refuses too, so what a verb may name has one
/// answer rather than one per verb.
fn admissible(config: &ServiceConfig, agent: &AgentName) -> Result<(), LifecycleRefusal> {
    if !well_formed(&agent.0) || agent.0 != config.agent {
        return Err(LifecycleRefusal::NoSuchAgent);
    }
    Ok(())
}

/// The name's shape, judged before any path is built from it: non-empty
/// ASCII letters, digits, `-` and `_`, so `.`, `..` and anything carrying `/`
/// are refused. **A name ending in a reserved suffix is refused too**, per
/// Spec section 4, since the agent's derived accounts and groups append
/// `-state`, `-trace`, `-relay`, `-admin` and `-admincon` to `weaver-<name>`,
/// and an agent named `x-relay` would collide with agent `x`'s relay.
fn well_formed(agent: &str) -> bool {
    const RESERVED: &[&str] = &["-state", "-trace", "-relay", "-admin", "-admincon"];
    !agent.is_empty()
        && agent
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        && !RESERVED.iter().any(|suffix| agent.ends_with(suffix))
}

/// The one inventory, called by both `validate` and `load`, so the two cannot
/// drift.
/// Stands the state member for this load, per `weaver-harness-state-contract`
/// as ruled 2026-08-26 and `weaver-state-Spec` section 2: the custodian's
/// territory sits on the operator's side of the wall the worker's identity
/// cannot cross, so the party that opens the trace sink is the party that
/// stands the member, and that party now creates the first door's transport
/// too. This crate makes the socketpair, arms the member's end onto the fixed
/// number in the spawn path itself, and returns the harness's end for the
/// enter directive to courier, speaking on neither, per `weaver-admin-PRD`
/// section 2. The binary is discovered beside the worker's own, so a
/// deployment without it simply has no leg. Every failure here is absorbed:
/// the leg is optional by presence, a load is never refused over its
/// derivative, and a `None` return is the leg not standing.
fn stand_state_member(
    config: &ServiceConfig,
    inventory: &inventory::Inventory,
    run_lock: &start::RunLock,
) -> Option<std::os::fd::OwnedFd> {
    // `none` declines the member, per `weaver-state-PRD` section 4 as of
    // 2026-09-04: nothing is stood, no territory is made, and the harness's
    // end is absent from the enter by the declaration's own word.
    let store = inventory.config.state_store.clone().unwrap_or_default();
    if store.engine == weaver_types::StoreEngine::None {
        return None;
    }
    let binary_directory = config.worker.parent()?;
    let binary = binary_directory.join("weaver-state");
    if !binary.exists() {
        return None;
    }
    // The member's account, which section 4 required of every election but
    // `none` before this load reached here.
    let member_account = inventory.member_account?;
    let territory_root = inventory::sink_directory(&inventory.config.trace_sink);
    let territory = prepare_territory(territory_root, member_account)?;
    // **The first door is a socketpair this crate creates and speaks on
    // never**, per the operator's ruling of 2026-08-26: both ends
    // close-on-exec atomically at creation like every descriptor this crate
    // holds, and the member's end is re-armed onto the fixed number in the
    // spawn path itself, the one deliberate gift the third walk of
    // `weaver-admin-Spec` section 10 names.
    let (harness_end, member_end) = nix::sys::socket::socketpair(
        nix::sys::socket::AddressFamily::Unix,
        nix::sys::socket::SockType::Stream,
        None,
        nix::sys::socket::SockFlag::SOCK_CLOEXEC,
    )
    .ok()?;
    // **Opened as root in the member's own room, so never through a link**:
    // the member could otherwise aim root's append at any file, and a FIFO
    // could hold the load.
    let log = log::open_append(&territory.join("state.log"), None);
    let mut member = std::process::Command::new(&binary);
    member
        .args(member_vector(
            &territory,
            &inventory.binding,
            inventory.lineage.is_some(),
        ))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null());
    if let Ok(log) = log {
        member.stderr(log);
    } else {
        member.stderr(std::process::Stdio::null());
    }
    // The member's end arrives at the fixed number `weaver-state` adopts, and
    // the run lock's description at 9. **Both sources are copied above 100
    // first**, so neither is a number the child places, the spawn placing
    // the standard streams onto 0 through 2 before any pre-exec runs and a
    // `dup2` onto its own number leaving the close-on-exec flag standing.
    let member_end = start::high({
        use std::os::fd::AsRawFd;
        member_end.as_raw_fd()
    })
    .ok()?;
    let lock = start::high(run_lock.raw()).ok()?;
    let (raw_member_end, raw_lock) = {
        use std::os::fd::AsRawFd;
        (member_end.as_raw_fd(), lock.as_raw_fd())
    };
    // SAFETY: every call below is async-signal-safe, run in the child
    // between fork and exec.
    unsafe {
        use std::os::unix::process::CommandExt;
        member.pre_exec(move || {
            // **The member takes its own session and resets the invocation's
            // ignored signals**, per Spec section 6, and holds the run lock's
            // description for its life, per section 3.
            start::place(raw_lock, start::RUN_LOCK_FD)?;
            // The member's allowlist: its first door's end at 3, armed below,
            // and the run lock at 9.
            start::seal_except(&[3, start::RUN_LOCK_FD])?;
            start::detach_and_reset()?;
            become_member(member_account)?;
            arm_member_end(raw_member_end)
        });
    }
    let spawned = member.spawn().is_ok();
    // This side's copy of the member's end closes either way: the member
    // holds the armed number, and a spawn that failed leaves no holder, the
    // harness end below then reading as the closed pair it is.
    drop(member_end);
    if !spawned {
        return None;
    }
    Some(harness_end)
}

/// **The member's territory, which the member owns.** One subdirectory of
/// the operator-side directory the sink already stands in, made if absent and
/// repaired if present, `0700` and owned by the member's own account, per
/// `weaver-state-PRD` section 4 and `weaver-admin-Spec` section 6.
///
/// **Owned rather than shared, as of 2026-09-15**, per issue #545. It stood
/// at `0750` with the sink directory's group and this crate's uid as owner,
/// which made the room admin's and the member a guest in it, and rewrote both
/// on every load - so a member-owned room did not survive one load, and the
/// charter's "a uid of its own over one subdirectory" was a sentence no
/// filesystem fact answered. The repair is unconditional for the same reason
/// the mode was: a room that widened between loads is a wall that stopped
/// being one, and this crate is the party that owns saying so.
///
/// The agent's uid is walled out twice over and neither wall rests on the
/// other: the containing directory denies it the search bit, which section 4
/// verified before this ran, and this directory grants it nothing through
/// owner, group, or other.
///
/// **Absorbed rather than refused**, like every other failure on this path:
/// a territory this crate could not make or could not hand to the member is
/// the leg not standing, and a member spawned into a room it cannot write is
/// worse than an absent one.
///
/// conforms: admin-member-territory-is-the-members-own
fn prepare_territory(
    root: &std::path::Path,
    member: inventory::MemberAccount,
) -> Option<std::path::PathBuf> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    let territory = root.join("state");
    // The mode rides the creation itself, so the directory never stands a
    // moment wider than it ends.
    if std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&territory)
        .is_err()
        && !territory.is_dir()
    {
        return None;
    }
    std::os::unix::fs::chown(&territory, Some(member.uid), Some(member.gid)).ok()?;
    std::fs::set_permissions(&territory, std::fs::Permissions::from_mode(0o700)).ok()?;
    Some(territory)
}

/// **The privilege drop at the member's spawn**, run in the pre-exec while
/// the fork still holds this crate's root: the supplementary set becomes the
/// member's own group alone, then the gids, then the uids, each of the three
/// values set so no saved id survives for the member to return to.
///
/// **The order is not interchangeable.** `setgroups` and `setresgid` both
/// need the privilege `setresuid` gives away, so a drop that took the uid
/// first would fail its group call with `EPERM`, and the member would not
/// spawn. Issue #675 was that failure in the store probe.
///
/// Async-signal-safe throughout, per the pre-exec contract: three syscalls
/// and no allocation. A failure returns the error, which fails the spawn, so
/// a member this crate could not unprivilege does not run at all.
///
/// conforms: admin-member-spawn-drops-to-its-account
fn become_member(member: inventory::MemberAccount) -> std::io::Result<()> {
    // The drop lives in `inventory::drop_to` since issue #675, so the order
    // described above is implemented once.
    inventory::drop_to(member.uid, &[member.gid as nix::libc::gid_t])
}

/// **The arming, the one deliberate gift**, per `weaver-admin-Spec` section
/// 6: `dup2` onto the member's fixed number, then an unconditional
/// close-on-exec clear, because `dup2` onto the same number is a no-op that
/// leaves the flag standing - the corner `weaver-harness-Spec` section 2.2
/// records - and a cleared flag on the armed number alone is what makes the
/// inheritance an act at one site while the atomic flag stands everywhere
/// else. Async-signal-safe throughout, per the pre-exec contract.
fn arm_member_end(raw_member_end: std::os::fd::RawFd) -> std::io::Result<()> {
    if raw_member_end != 3 && unsafe { nix::libc::dup2(raw_member_end, 3) } < 0 {
        return Err(std::io::Error::last_os_error());
    }
    let flags = unsafe { nix::libc::fcntl(3, nix::libc::F_GETFD) };
    if flags < 0 {
        return Err(std::io::Error::last_os_error());
    }
    if unsafe { nix::libc::fcntl(3, nix::libc::F_SETFD, flags & !nix::libc::FD_CLOEXEC) } < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

/// **The member's vector, both directions this crate's alone**, per
/// `weaver-admin-Spec` section 6 as ruled 2026-08-26: the territory, and the
/// preload socket path exactly where the resolved kind is diagnostic, derived
/// as the territory with a fixed leaf so no invocation input composes it. One
/// value on a serving load and two on a diagnostic one, the first door riding
/// no argument at all, its end inherited at the fixed number the spawn arms.
fn member_vector(
    territory: &std::path::Path,
    binding: &weaver_types::EnterBinding,
    restoring: bool,
) -> Vec<std::ffi::OsString> {
    // **The territory leads and no flag rides**, per Spec section 6: the one
    // engine is the embedded one, so the engine flag left the vector with
    // the service engine on the operator's ruling of 2026-10-02 on #1.
    let mut vector: Vec<std::ffi::OsString> = vec![territory.as_os_str().to_owned()];
    // The door's name rides the vector under a diagnostic binding and,
    // since 2026-09-04, under a serving load that elects a restore, per Spec
    // section 6 and issue #432: the member binds the name only where this
    // value is there, and this crate names the door and dials it never.
    if matches!(binding, weaver_types::EnterBinding::Diagnostic) || restoring {
        vector.push(territory.join("preload.sock").into_os_string());
    }
    vector
}

/// The digests of the organ binaries this crate starts and hands the worker,
/// keyed by the binary's name, per `weaver-admin-harness-contract` section 3:
/// the worker the unit runs, the state member beside it where it stood, and
/// the agent's own SPU and the gate the worker forks from the paths section 6
/// hands it, per Spec section 9. Each is sha256 hex, and the empty string where
/// the file does not read. The names cannot collide, section 9's read having
/// refused a root naming two binaries under one file name.
fn stack_digests(
    config: &ServiceConfig,
    member_started: bool,
    classify: Option<&std::path::Path>,
) -> std::collections::BTreeMap<String, String> {
    let worker = config.worker.as_path();
    let member = worker
        .parent()
        .map(|directory| directory.join("weaver-state"))
        .unwrap_or_else(|| std::path::PathBuf::from("weaver-state"));
    let spu = config.spu.as_path();
    let mut binaries = vec![worker];
    if member_started {
        binaries.push(member.as_path());
    }
    binaries.push(spu);
    binaries.push(config.gate.as_path());
    // The classify arm's binary where it is handed, so a run that classifies
    // names the program that did.
    if let Some(classify) = classify {
        binaries.push(classify);
    }
    let mut stack = std::collections::BTreeMap::new();
    for binary in binaries {
        let name = binary
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        stack.insert(name, inventory::file_digest(binary));
    }
    stack
}

fn take_inventory(
    config: &ServiceConfig,
    agent: &AgentName,
) -> Result<inventory::Inventory, LifecycleRefusal> {
    admissible(config, agent)?;
    judge_reader(&config.require_boundary()?.reader, agent)?;
    let identity = inventory::identity_for(agent);
    let source_path = config.declaration_directory.join("agent.toml");
    let source =
        std::fs::read_to_string(&source_path).map_err(|_| LifecycleRefusal::NoSuchAgent)?;
    // The home comes from the account database rather than from a constructed
    // path: an operator who placed the agent elsewhere would otherwise have
    // the boundary checked against a directory that is not the agent's.
    let user = nix::unistd::User::from_name(&identity)
        .ok()
        .flatten()
        .ok_or(LifecycleRefusal::BoundaryUnverified)?;
    let boundary = inventory::Boundary {
        agent_uid: user.uid.as_raw(),
        // Custody is held by whoever opens the sink, and that is this
        // invocation under root, the role's principal.
        admin_uid: nix::unistd::getuid().as_raw(),
        // **Every gid the worker will actually hold, not the passwd primary
        // alone.** The unit sets `Group={identity}`, so the running egid is
        // `weaver-<agent>` whatever passwd says - and where the operator
        // provisioned a shared primary group, which is the case the mode's
        // own justification cites, the two disagree. A denial walk reading
        // only passwd would pass a sink at `root:weaver-<agent>` mode `0710`
        // that the worker can traverse into and rewrite the record admin
        // holds custody of.
        //
        // Over-approximated on purpose: this asks what the agent could reach,
        // so a gid too many refuses a boundary that might have held, and a
        // gid too few admits one that does not.
        agent_gids: agent_gids(&user),
        home: user.dir.clone(),
        // The box fact the store rule reads, per Spec section 4: the member's
        // binary beside the worker's.
        member_binary: config
            .worker
            .parent()
            .map(|directory| directory.join("weaver-state"))
            // A binary and not a path: a directory or an unexecutable file
            // under that name would pass an existence look and fail at the
            // spawn, the leg then down under a declaration that never
            // declined it, which is the #381 class this rule exists to
            // refuse at the inventory.
            .filter(|binary| {
                use std::os::unix::fs::PermissionsExt;
                std::fs::metadata(binary)
                    .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
                    .unwrap_or(false)
            }),
        // **The member's own account, looked up from the derived name.** It
        // is read here rather than constructed, for the reason the home is:
        // the uid the spawn drops to and the uid that owns the territory are
        // the account database's fact and not this crate's. An absent account is a box
        // the provisioning has not finished, which section 4 refuses for
        // every election but `none`.
        member_account: nix::unistd::User::from_name(&inventory::member_identity_for(agent))
            .ok()
            .flatten()
            .map(|user| inventory::MemberAccount {
                uid: user.uid.as_raw(),
                gid: user.gid.as_raw(),
            }),
    };
    inventory::take_inventory(
        agent,
        &source,
        |name| read_prompt(&config.declaration_directory, config.operator, name),
        &boundary,
    )
}

/// Every gid the worker may run under: the group the unit sets, the passwd
/// primary, and the supplementary memberships the user holds.
///
/// `unreachable_peer` reads `Group::from_name(identity)` for the same
/// boundary, and the denial walk follows it rather than reading passwd alone.
///
/// conforms: admin-boundary-reads-every-gid-the-worker-holds
fn agent_gids(user: &nix::unistd::User) -> Vec<u32> {
    // The gid `--property=Group={identity}` gives the unit, which is what the
    // worker's egid actually is.
    let named = match nix::unistd::Group::from_name(&user.name) {
        Ok(Some(group)) => Some(group.gid.as_raw()),
        // A lookup that failed leaves the primary and the supplementary set,
        // which is the narrower answer and so the one that refuses more.
        _ => None,
    };
    // And whatever else the user is a member of, a supplementary group being
    // reachable by the running process too.
    let supplementary: Vec<u32> = std::ffi::CString::new(user.name.as_str())
        .ok()
        .and_then(|name| nix::unistd::getgrouplist(&name, user.gid).ok())
        .map(|groups| groups.into_iter().map(|gid| gid.as_raw()).collect())
        .unwrap_or_default();
    merge_gids(user.gid.as_raw(), named, &supplementary)
}

/// The three sources joined, deduped, with the passwd primary first.
///
/// **Separate from the host lookups so the join is watched.** The failure the
/// denial walk cannot survive is a gid dropped on the floor, and that is a
/// property of this merge rather than of `getgrnam_r`. Reading the host
/// inside the same function would have made any watch on it vacuous wherever
/// no agent is provisioned, which is this box and CI both.
fn merge_gids(primary: u32, named: Option<u32>, supplementary: &[u32]) -> Vec<u32> {
    let mut gids = vec![primary];
    for gid in named.into_iter().chain(supplementary.iter().copied()) {
        if !gids.contains(&gid) {
            gids.push(gid);
        }
    }
    gids
}

#[cfg(test)]
mod gid_tests {
    use super::merge_gids;

    /// **The group the unit sets is in the set, and the passwd primary alone
    /// is not the set.**
    ///
    /// This is the custody hole olympus found on 2026-08-29: the unit forces
    /// `Group={identity}` so the worker's egid is `weaver-<agent>`, while the
    /// denial walk read the passwd primary. Where the operator provisioned a
    /// shared primary - `users`, `nogroup` - the two disagree, and a sink at
    /// `root:weaver-karl` mode `0710` passes a walk the running worker can
    /// then traverse to rewrite the trace admin holds custody of.
    ///
    /// Perturbation: returning `vec![primary]` fails this. Watched failing
    /// 2026-08-29.
    ///
    /// conforms: admin-boundary-reads-every-gid-the-worker-holds
    #[test]
    fn the_walk_reads_the_group_the_unit_sets_and_not_passwd_alone() {
        // The shared-primary case the mode's own justification cites.
        let gids = merge_gids(100, Some(2001), &[100]);
        assert!(
            gids.contains(&2001),
            "the egid the unit sets is walked: {gids:?}"
        );
        assert!(gids.contains(&100), "and the passwd primary too: {gids:?}");

        // Supplementary memberships reach the sink as well.
        let gids = merge_gids(100, Some(2001), &[100, 27, 998]);
        assert!(
            [100, 2001, 27, 998].iter().all(|gid| gids.contains(gid)),
            "every gid the worker holds is walked: {gids:?}"
        );

        // Deduped, and the primary stays first: the walk asks `contains`, so
        // a repeat is only noise, but a set that grows per call is a leak.
        assert_eq!(merge_gids(100, Some(100), &[100, 100]), vec![100]);
        assert_eq!(merge_gids(100, None, &[7]), vec![100, 7]);
    }
}

/// The observation's bound, per Spec section 3: five seconds from the
/// `Observe`, after the dial's bound, which covers only the connect.
const OBSERVE_BOUND: std::time::Duration = std::time::Duration::from_secs(5);

/// The leave's own bound, per Spec section 3: sixty seconds from the
/// directive.
const LEAVE_BOUND: std::time::Duration = std::time::Duration::from_secs(60);

/// The stop's bound, per Spec section 3: sixty seconds from the directive.
const STOP_BOUND: std::time::Duration = std::time::Duration::from_secs(60);

/// What an observation of a held run found, per Spec section 3. **Silence is
/// not evidence**: only `Unloaded` or no listener at all says no run entered.
enum Observation {
    State(
        weaver_types::AgentState,
        Option<Box<weaver_types::LoadFacts>>,
    ),
    /// No worker listens: no name bound, or the connection refused, through
    /// the dial's whole bound.
    NoListener,
    /// A worker accepted, or the backlog was full, and nothing answered
    /// inside the bounds: a busy run, never a stranded one.
    Silent,
}

/// **Observes the run through the harness's own word**, per Spec section 3
/// and `weaver-admin-harness-contract` section 3, within the dial's bound and
/// the observation's.
fn observe(config: &ServiceConfig) -> Result<Observation, LifecycleRefusal> {
    let mut coordination = match channel::dial(&config.coordination_socket()) {
        Ok(coordination) => coordination,
        Err(channel::ChannelFault::NoListener) => return Ok(Observation::NoListener),
        Err(_) => return Ok(Observation::Silent),
    };
    let ordinal = coordination.next_ordinal();
    if coordination
        .send_directive(ordinal, LifecycleDirective::Observe)
        .is_err()
    {
        return Ok(Observation::Silent);
    }
    match coordination.recv_within(OBSERVE_BOUND) {
        Ok(answer) => match answer.payload {
            weaver_types::Payload::Answer(LifecycleAnswer::State { state, load, .. }) => {
                Ok(Observation::State(state, load))
            }
            weaver_types::Payload::Refusal(refusal) => Err(refusal),
            _ => Err(LifecycleRefusal::Malformed),
        },
        Err(_) => Ok(Observation::Silent),
    }
}

/// What a load has stood up, for its rollback: the run lock it took, whether
/// it forked any constituent, and whether the enter reached the worker.
#[derive(Default)]
struct Standing {
    run_lock: Option<start::RunLock>,
    forked: bool,
    entered: bool,
    sink_opened: bool,
}

/// **`load` keeps one promise**, per Spec section 3 and the operator's ruling
/// of 2026-10-03 on #72: a run started by this load, or a refusal with nothing
/// changed. A held run lock refuses `AgentRunning`, or `Unanswered` where the
/// worker is silent, and a load never ends an existing run.
fn load(config: &ServiceConfig, agent: &AgentName) -> Result<LifecycleAnswer, LifecycleRefusal> {
    let _invocation = start::take_invocation_lock(&config.run_directory())?;
    let mut standing = Standing::default();
    match run_load(config, agent, &mut standing) {
        Ok(()) => Ok(LifecycleAnswer::State {
            state: weaver_types::AgentState::Idle,
            load: None,
            constituents: Vec::new(),
        }),
        Err(refusal) => {
            let account = roll_back(config, &mut standing);
            if !account.is_empty() {
                record(config, "load", &format!("rolled back: {account}"));
            }
            Err(refusal)
        }
    }
}

/// The load's order, per Spec section 3: the run lock taken by this
/// invocation before anything forks, then the inventory, the sink, the
/// runtime directory, the member and the worker, each child inheriting the
/// run lock's description, then the dial and the enter under its bound.
fn run_load(
    config: &ServiceConfig,
    agent: &AgentName,
    standing: &mut Standing,
) -> Result<(), LifecycleRefusal> {
    let run_directory = config.run_directory();
    let Some(run_lock) = start::take_run_lock(&run_directory)? else {
        // **Any answer, a refusal among them, is a run that stands**: only
        // silence is `Unanswered`, per Spec section 3.
        return Err(match observe(config) {
            Ok(Observation::Silent) => LifecycleRefusal::Unanswered,
            _ => LifecycleRefusal::AgentRunning,
        });
    };
    let inventory = take_inventory(config, agent)?;
    let custody = sink::FileCustody {
        owners: vec![0, nix::unistd::geteuid().as_raw()],
        trace_group: format!("{}-trace", inventory::identity_for(agent)),
    };
    let sink = sink::open(&inventory.config.trace_sink, &custody)?;
    standing.sink_opened = true;
    let account = AgentAccount::resolve(&inventory.identity)?;
    start::prepare_runtime_directory(
        &config.coordination_root,
        &agent.0,
        account.uid,
        account.gid,
    )?;
    let worker_log = start::open_log(&config.worker_log(), Some(config.operator_owner()))
        .map_err(|_| LifecycleRefusal::BoundaryUnverified)?;

    // **The trace door stands only for a file sink**, per Spec section 6: a
    // stale door is cleared on every load, and for a file sink the relay is
    // stood before the member and the worker, the lifetime pipe made before
    // either fork.
    start::clear_trace_door(&config.run_directory())?;
    let relay_write = match &inventory.config.trace_sink {
        weaver_types::TraceSink::File { .. } => {
            let (_relay, write) = stand_relay(config, agent, &sink, &run_lock)?;
            standing.forked = true;
            Some(write)
        }
        _ => None,
    };
    let state_end = stand_state_member(config, &inventory, &run_lock);
    standing.forked |= state_end.is_some();
    let classify = config
        .worker
        .parent()
        .map(|directory| directory.join("weaver-spu-classify"))
        .filter(|binary| binary.is_file());
    let stack = stack_digests(config, state_end.is_some(), classify.as_deref());
    let socket_path = config.coordination_socket();
    let mut worker = start::spawn_worker(start::WorkerStart {
        binary: &config.worker,
        arguments: start::worker_arguments(
            &socket_path,
            &config.spu,
            &config.gate,
            config.headroom_bytes.as_deref(),
            inventory.config.loop_file.as_deref(),
            classify.as_deref(),
        ),
        uid: account.uid,
        gid: account.gid,
        home: &account.home,
        library_path: config.library_path.as_deref(),
        log: &worker_log,
        run_lock: &run_lock,
        relay_write: relay_write.as_ref().map(std::os::fd::AsRawFd::as_raw_fd),
    })
    .map_err(|_| LifecycleRefusal::BindFailed)?;
    standing.forked = true;
    // **The start step closes its own copy of the write end once both
    // children hold theirs**, per Spec section 6, so the relay reads
    // end-of-file at the worker's death and not at this invocation's.
    drop(relay_write);
    standing.run_lock = Some(run_lock);

    let mut coordination = match channel::dial(&socket_path) {
        Ok(coordination) => coordination,
        Err(_) => return Err(refusal_from_worker(&mut worker)),
    };
    let ordinal = coordination.next_ordinal();
    // **The session is read and the run is minted**, per Spec section 7. The
    // session is the operator's, declared in the config and carried
    // uninterpreted. The reference is this crate's, and a load that cannot
    // read the randomness it rests on refuses rather than carrying a
    // reference that looks like the others and is not guaranteed.
    let run_reference =
        channel::mint_run_reference(&agent.0).ok_or(LifecycleRefusal::BoundaryUnverified)?;
    let envelope = weaver_types::OrganEnvelope {
        exchange: weaver_types::ExchangeId {
            opener: weaver_types::Opener::Admin,
            ordinal,
        },
        position: weaver_types::Position::Open,
        payload: weaver_types::Payload::Directive(LifecycleDirective::Enter {
            payload: Box::new(weaver_types::EnterPayload {
                session: inventory.config.session.clone(),
                run: run_reference,
                // The permission member is written from the resolved kind,
                // per `weaver-admin-Spec` section 7: granted under a
                // diagnostic enter and cleared under a serving one, never
                // read from the file, whose grant the inventory refused.
                spu_instruction: {
                    let mut instruction = inventory.config.spu_instruction.clone();
                    let diagnostic =
                        matches!(inventory.binding, weaver_types::EnterBinding::Diagnostic);
                    instruction.decoder.refeed_permission = diagnostic;
                    instruction.decoder.column_permission = diagnostic;
                    instruction
                },
                binding: inventory.binding.clone(),
                state_election: inventory.config.state_election.clone().unwrap_or_default(),
                state_store: inventory.config.state_store.clone().unwrap_or_default(),
                declaration: inventory.declaration.clone(),
                identity_file: inventory.identity_file.clone(),
                restore: inventory.lineage.clone(),
                // No reset until admin's clean-unload marker lands with its
                // save-point act (A3.2), per `weaver-admin-Spec` section 4.
                reset: None,
                stack,
                // The boundary file's digest, the cause and the judged
                // libraries, per `weaver-types-Spec` section 4 as of
                // 2026-10-03, which the harness records on the load event.
                boundary: config.require_boundary()?.digest.clone(),
                cause: invocation_cause(),
                library_path: config
                    .library_path
                    .as_ref()
                    .map(|path| path.display().to_string()),
            }),
        }),
    };
    use std::os::fd::AsFd;
    coordination
        .send_with_sink(
            &envelope,
            sink.as_fd(),
            state_end.as_ref().map(|end| end.as_fd()),
        )
        .map_err(|_| LifecycleRefusal::DescriptorsUnusable)?;
    standing.entered = true;
    match coordination.recv_within(config.load_bound) {
        Ok(answer) => match answer.payload {
            weaver_types::Payload::Answer(LifecycleAnswer::Ready) => Ok(()),
            weaver_types::Payload::Refusal(refusal) => Err(refusal),
            _ => Err(LifecycleRefusal::Malformed),
        },
        Err(_) => Err(LifecycleRefusal::NoResidency),
    }
}

/// **Stands the trace relay for a file sink**, per Spec section 6: the relay
/// account, its trace group and the agent's access group resolved by name,
/// each refusing `BoundaryUnverified` where the box does not carry it; the
/// declared reader's uid; the binary beside the worker's, found as the member's
/// is; the door bound; the sink reopened read-only and confirmed; the
/// operations log opened for the relay's lines; and the lifetime pipe made.
/// Answers the relay and the pipe's write end, which the worker inherits.
fn stand_relay(
    config: &ServiceConfig,
    agent: &AgentName,
    sink: &std::os::fd::OwnedFd,
    run_lock: &start::RunLock,
) -> Result<(std::process::Child, std::os::fd::OwnedFd), LifecycleRefusal> {
    let base = inventory::identity_for(agent);
    let missing = |what: &str| {
        diag!("weaver-admin: {what} is not provisioned, so the trace relay cannot stand");
        LifecycleRefusal::BoundaryUnverified
    };
    let relay_name = format!("{base}-relay");
    let relay_user = nix::unistd::User::from_name(&relay_name)
        .ok()
        .flatten()
        .ok_or_else(|| missing(&relay_name))?;
    let trace_name = format!("{base}-trace");
    let trace_group = nix::unistd::Group::from_name(&trace_name)
        .ok()
        .flatten()
        .ok_or_else(|| missing(&trace_name))?;
    let access_name = format!("{base}-admin");
    let access_group = nix::unistd::Group::from_name(&access_name)
        .ok()
        .flatten()
        .ok_or_else(|| missing(&access_name))?;
    let boundary = config.require_boundary()?;
    let reader = nix::unistd::User::from_name(&boundary.reader)
        .ok()
        .flatten()
        .ok_or_else(|| missing(&boundary.reader))?;
    let binary = config
        .worker
        .parent()
        .map(|directory| directory.join("weaver-trace-relay"))
        .filter(|binary| binary.is_file())
        .ok_or_else(|| missing("weaver-trace-relay beside the worker binary"))?;
    let listener = start::bind_trace_door(&config.run_directory(), access_group.gid.as_raw())?;
    let read_only = start::reopen_read_only(sink)?;
    let log = log::open_append(&config.admin_log(), Some(config.operator_owner()))
        .map_err(|_| LifecycleRefusal::BoundaryUnverified)?;
    let (lifetime_read, lifetime_write) = nix::unistd::pipe2(nix::fcntl::OFlag::O_CLOEXEC)
        .map_err(|_| LifecycleRefusal::DescriptorsUnusable)?;
    let relay = start::spawn_relay(start::RelayStart {
        binary: &binary,
        reader_uid: reader.uid.as_raw(),
        agent: &agent.0,
        boundary_digest: &boundary.digest,
        uid: relay_user.uid.as_raw(),
        gid: trace_group.gid.as_raw(),
        listener: &listener,
        sink: &read_only,
        log: &log,
        lifetime_read: &lifetime_read,
        run_lock,
    })
    .map_err(|_| LifecycleRefusal::BindFailed)?;
    Ok((relay, lifetime_write))
}

/// **A failed dial is answered from the worker's own exit**, per Spec
/// section 6: a worker that exited refuses naming its status on standard
/// error, and one still running with no socket is a bind that failed.
fn refusal_from_worker(worker: &mut std::process::Child) -> LifecycleRefusal {
    match worker.try_wait() {
        Ok(Some(status)) => {
            diag!("weaver-admin: the worker exited before binding: {status}");
            LifecycleRefusal::NoResidency
        }
        _ => LifecycleRefusal::BindFailed,
    }
}

/// **Rollback is the reap plus one directive, as data**, per Spec section 3:
/// leave where a run was entered, then this invocation's own copy of the run
/// lock's description closed and every constituent it started ended by the
/// escalation, which never signals this invocation, then the sink closed.
/// Answers the account, empty where nothing stood.
fn roll_back(config: &ServiceConfig, standing: &mut Standing) -> String {
    let mut account = Vec::new();
    if standing.entered {
        let left = direct_leave(config).is_ok();
        account.push(format!("leave {}", if left { "undone" } else { "held" }));
    }
    standing.run_lock = None;
    if standing.forked {
        let ended = start::escalate(&config.run_directory());
        account.push(format!(
            "constituents {}",
            match ended {
                Ok(()) => "ended".to_string(),
                Err(refusal) => format!("held: {}", surface::render_refusal(&refusal)),
            }
        ));
    }
    if standing.sink_opened {
        account.push("sink closed".to_string());
    }
    account.join(", ")
}

/// The agent's account, resolved by name as the start step needs it: the
/// uid, the agent's own group by name, never a provisioned primary, and the
/// home, per Spec section 6.
struct AgentAccount {
    uid: u32,
    gid: u32,
    home: PathBuf,
}

impl AgentAccount {
    fn resolve(identity: &str) -> Result<Self, LifecycleRefusal> {
        let user = nix::unistd::User::from_name(identity)
            .ok()
            .flatten()
            .ok_or(LifecycleRefusal::BoundaryUnverified)?;
        let group = nix::unistd::Group::from_name(identity)
            .ok()
            .flatten()
            .ok_or(LifecycleRefusal::BoundaryUnverified)?;
        Ok(AgentAccount {
            uid: user.uid.as_raw(),
            gid: group.gid.as_raw(),
            home: user.dir,
        })
    }
}

/// **`show` answers through the observation exchange**, per Spec section 3,
/// holding the invocation lock shared: `InTransition` where an exclusive
/// holder stands, `Unloaded` without dialing where the run lock is free, and
/// otherwise the harness's word with the run's constituents beside it,
/// `Unanswered` where the worker is silent.
fn show(config: &ServiceConfig) -> Result<LifecycleAnswer, LifecycleRefusal> {
    let run_directory = config.run_directory();
    let _shared = match start::take_shared_invocation_lock(&run_directory)? {
        start::Shared::Held(lock) => lock,
        start::Shared::InTransition => return Ok(LifecycleAnswer::InTransition),
    };
    if !start::run_lock_held(&run_directory)? {
        return Ok(LifecycleAnswer::State {
            state: weaver_types::AgentState::Unloaded,
            load: None,
            constituents: Vec::new(),
        });
    }
    let (state, load) = match observe(config)? {
        Observation::State(state, load) => (state, load),
        // The lock held with no worker listening: a run that never entered,
        // whose constituents the caller may end with `unload`.
        Observation::NoListener => (weaver_types::AgentState::Unloaded, None),
        Observation::Silent => return Err(LifecycleRefusal::Unanswered),
    };
    Ok(LifecycleAnswer::State {
        state,
        load,
        constituents: constituents(&run_directory),
    })
}

/// The run's constituent pids, every holder of the run lock's description,
/// per toddwbucy/WeaverWeb#15, sorted and each named once.
fn constituents(run_directory: &std::path::Path) -> Vec<u32> {
    let mut pids: Vec<u32> = start::holders(run_directory)
        .into_iter()
        .map(|holder| holder.pid as u32)
        .collect();
    pids.sort_unstable();
    pids.dedup();
    pids
}

/// **`unload` ends whatever holds the run lock**, per Spec section 3: its own
/// promise, acting on the lock and not on a classification. Where the
/// observation answers `Idle`, or the worker is silent, it directs leave under
/// the leave's bound; where it answers `Unloaded`, or no worker listens, no run
/// was entered and it goes straight to the escalation. It answers
/// provisioned-and-unloaded only once the lock is free.
fn unload(config: &ServiceConfig) -> Result<LifecycleAnswer, LifecycleRefusal> {
    unload_within(config, UNLOAD_BOUNDS)
}

/// The unload's four waits, fixed in production by Spec section 3 and
/// passed in so a test can run the same path at a test's pace.
#[derive(Clone, Copy)]
struct UnloadBounds {
    leave: std::time::Duration,
    after_left: std::time::Duration,
    term: std::time::Duration,
    kill: std::time::Duration,
}

/// Spec section 3's values: the leave's sixty seconds, thirty after `left`,
/// ten from `SIGTERM` to `SIGKILL` and five for the last read, at most 105
/// seconds in all, a number WeaverWeb builds against.
const UNLOAD_BOUNDS: UnloadBounds = UnloadBounds {
    leave: LEAVE_BOUND,
    after_left: start::AFTER_LEFT,
    term: start::TERM_GRACE,
    kill: start::KILL_GRACE,
};

fn unload_within(
    config: &ServiceConfig,
    bounds: UnloadBounds,
) -> Result<LifecycleAnswer, LifecycleRefusal> {
    let run_directory = config.run_directory();
    let _invocation = start::take_invocation_lock(&run_directory)?;
    // **The leave's budget runs from here**, per Spec section 3: the
    // observation and both dials spend it, so the verb holds the invocation
    // lock at most the leave's sixty seconds and the escalation's forty-five.
    let leave_deadline = std::time::Instant::now() + bounds.leave;
    let unloaded = Ok(LifecycleAnswer::State {
        state: weaver_types::AgentState::Unloaded,
        load: None,
        constituents: Vec::new(),
    });
    if !start::run_lock_held(&run_directory)? {
        return unloaded;
    }
    // **A refused observation is silence**, `unload`'s promise being to end
    // whatever holds the lock: it directs leave as for any silent run.
    let entered = match observe(config).unwrap_or(Observation::Silent) {
        Observation::State(weaver_types::AgentState::Unloaded, _) | Observation::NoListener => {
            false
        }
        Observation::State(..) | Observation::Silent => true,
    };
    if entered {
        match direct_leave_within(config, leave_deadline) {
            Ok(()) => {
                if start::wait_free(&run_directory, bounds.after_left) {
                    return unloaded;
                }
            }
            // A refusal on leave, `ActivityNotAtRest` above all, returns to
            // the operator unchanged and answers nothing further.
            Err(LeaveFault::Refused(refusal)) => return Err(refusal),
            // The leave went unanswered inside its bound: a worker that
            // would not exit, so the escalation follows.
            Err(LeaveFault::Unanswered) => {}
        }
    }
    start::escalate_within(&run_directory, bounds.term, bounds.kill)?;
    unloaded
}

/// Why a directed leave did not answer `Left`.
enum LeaveFault {
    Refused(LifecycleRefusal),
    Unanswered,
}

/// **Directs leave under the leave's own bound**, per Spec section 3.
fn direct_leave(config: &ServiceConfig) -> Result<(), LeaveFault> {
    direct_leave_within(config, std::time::Instant::now() + LEAVE_BOUND)
}

/// Directs leave and waits for its answer until `deadline`, the dial spending
/// the same budget.
fn direct_leave_within(
    config: &ServiceConfig,
    deadline: std::time::Instant,
) -> Result<(), LeaveFault> {
    let Ok(mut coordination) = channel::dial(&config.coordination_socket()) else {
        return Err(LeaveFault::Unanswered);
    };
    let ordinal = coordination.next_ordinal();
    coordination
        .send_directive(
            ordinal,
            LifecycleDirective::Leave {
                cause: invocation_cause(),
            },
        )
        .map_err(|_| LeaveFault::Unanswered)?;
    match coordination.recv_within(deadline.saturating_duration_since(std::time::Instant::now())) {
        Ok(answer) => match answer.payload {
            weaver_types::Payload::Answer(LifecycleAnswer::Left) => Ok(()),
            weaver_types::Payload::Refusal(refusal) => Err(LeaveFault::Refused(refusal)),
            _ => Err(LeaveFault::Refused(LifecycleRefusal::Malformed)),
        },
        Err(_) => Err(LeaveFault::Unanswered),
    }
}

/// **`stop` is a conveyance and its answer is a relay**, per Spec section 3,
/// under the stop's own bound: a worker that accepts stop and answers nothing
/// is not ended, a stop being no unload, and the verb refuses `Unanswered`.
fn stop(config: &ServiceConfig) -> Result<LifecycleAnswer, LifecycleRefusal> {
    let _invocation = start::take_invocation_lock(&config.run_directory())?;
    let mut coordination =
        channel::dial(&config.coordination_socket()).map_err(|_| LifecycleRefusal::NoResidency)?;
    let ordinal = coordination.next_ordinal();
    coordination
        .send_directive(
            ordinal,
            LifecycleDirective::Stop {
                cause: invocation_cause(),
            },
        )
        .map_err(|_| LifecycleRefusal::NoResidency)?;
    match coordination.recv_within(STOP_BOUND) {
        Ok(answer) => match answer.payload {
            weaver_types::Payload::Answer(answer) => Ok(verbs::relay_stop_answer(answer)),
            weaver_types::Payload::Refusal(refusal) => Err(refusal),
            _ => Err(LifecycleRefusal::Malformed),
        },
        Err(channel::ChannelFault::Unanswered) => Err(LifecycleRefusal::Unanswered),
        Err(_) => Err(LifecycleRefusal::NoResidency),
    }
}

/// **One line of the operations log**, per Spec section 8: the wall time,
/// the command line, the caller's uid, what was asked, the boundary file's
/// digest in force, and the outcome. A log that cannot open costs the line,
/// never the verb.
fn record(config: &ServiceConfig, verb: &'static str, outcome: &str) {
    let Ok(mut operations) =
        log::OperationsLog::open(&config.admin_log(), Some(config.operator_owner()))
    else {
        diag!("weaver-admin: admin.log did not open; the {verb} line is lost");
        return;
    };
    let _ = operations.record(&log::Act {
        verb,
        agent: config.agent.clone(),
        command: format!("weaver-admin {verb} {}", config.agent),
        uid: invocation_cause().uid,
        boundary: config.boundary_digest().map(str::to_string),
        outcome: outcome.to_string(),
        spu: (verb == "load").then(|| config.spu.display().to_string()),
    });
}

/// The base the agents' roots stand under where `WEAVER_ADMIN_CONFIG` is unset.
const DEFAULT_BASE: &str = "/etc/weaver/admin";

/// **One agent's configuration, from that agent's root and nothing shared**,
/// per Spec section 9 and the operator's ruling of 2026-10-01 that admin is
/// one agent's organ. `WEAVER_ADMIN_CONFIG` names the base, `/etc/weaver/admin`
/// where it is unset, and the agent's root is `<base>/<agent>/`. The name is
/// judged before the path is built, the root existing is the admission, and
/// the root must be root's and closed to every other writer before a value is
/// read from it, since what it names runs under the agent's identity.
fn load_service_config(agent: &AgentName) -> Result<ServiceConfig, LifecycleRefusal> {
    if !well_formed(&agent.0) {
        return Err(LifecycleRefusal::NoSuchAgent);
    }
    let base = std::env::var_os("WEAVER_ADMIN_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_BASE));
    load_service_config_at(&base, &agent.0, 0)
}

/// The judgments in Spec section 9's order, against `owner` for the root's
/// files, which production fixes at uid 0 and a test sets to its own uid: the
/// root admitted and closed, its ancestors closed, its entries closed, its
/// values read, then the operator's declaration directory judged against the
/// `operator` the root names and `library-path` judged as the root is.
fn load_service_config_at(
    base: &std::path::Path,
    agent: &str,
    owner: u32,
) -> Result<ServiceConfig, LifecycleRefusal> {
    let root = base.join(agent);
    judge_root(&root, owner)?;
    let root = judge_ancestors(&root, &[owner, 0])?;
    judge_entries(&root, owner)?;
    let mut config = load_service_config_from(&root, agent).map_err(|failure| {
        diag!("weaver-admin: {failure}");
        // A value of the root's failing names no field; the boundary file is
        // named where it is required, at `validate` and `load`.
        LifecycleRefusal::ConfigInvalid { field: None }
    })?;
    config.declaration_directory =
        judge_declaration_directory(&config.declaration_directory, config.operator)?;
    config.operator_gid = {
        use std::os::unix::fs::MetadataExt;
        std::fs::metadata(&config.declaration_directory)
            .map_err(|_| LifecycleRefusal::BoundaryUnverified)?
            .gid()
    };
    if let Some(libraries) = &config.library_path {
        config.library_path = Some(judge_library_path(libraries, owner)?);
    }
    Ok(config)
}

/// **The boundary file's reader, judged at `validate` and `load`**, per Spec
/// section 9: a user the box carries, never the agent's account or its state
/// member's, and holding the agent's access group `weaver-<agent>-admin`, so the
/// agent never reaches its own record through the boundary and the declared
/// reader can always reach the door. A missing access group is the box's
/// provisioning and refuses `BoundaryUnverified`; every fault of the reader
/// itself refuses `ConfigInvalid` naming `roles.toml`.
fn judge_reader(reader: &str, agent: &AgentName) -> Result<(), LifecycleRefusal> {
    let group_name = format!("{}-admin", inventory::identity_for(agent));
    let user = nix::unistd::User::from_name(reader)
        .ok()
        .flatten()
        .map(|user| user.gid.as_raw());
    let group = nix::unistd::Group::from_name(&group_name)
        .ok()
        .flatten()
        .map(|group| (group.gid.as_raw(), group.mem));
    reader_verdict(reader, agent, user, group)
}

/// The reader's verdict over what the box answered, separated so a test
/// reaches every case without provisioning accounts.
fn reader_verdict(
    reader: &str,
    agent: &AgentName,
    user_primary_gid: Option<u32>,
    access_group: Option<(u32, Vec<String>)>,
) -> Result<(), LifecycleRefusal> {
    let named = || LifecycleRefusal::ConfigInvalid {
        field: Some(weaver_types::FieldName(BOUNDARY_FILE.to_string())),
    };
    if reader == inventory::identity_for(agent) || reader == inventory::member_identity_for(agent) {
        diag!("weaver-admin: roles.toml names the agent's own account as its trace reader");
        return Err(named());
    }
    let Some((gid, members)) = access_group else {
        diag!(
            "weaver-admin: the access group {}-admin is not provisioned",
            inventory::identity_for(agent)
        );
        return Err(LifecycleRefusal::BoundaryUnverified);
    };
    let Some(primary) = user_primary_gid else {
        diag!("weaver-admin: roles.toml names {reader}, which is no user on this box");
        return Err(named());
    };
    if primary != gid && !members.iter().any(|member| member == reader) {
        diag!("weaver-admin: the trace reader {reader} does not hold the agent's access group");
        return Err(named());
    }
    Ok(())
}

/// The boundary file's name in the agent's root, per Spec section 9.
const BOUNDARY_FILE: &str = "roles.toml";

/// **Every entry of the root is held closed**: a regular file, never a link,
/// owned by `owner` and writable by no group or other, since the values name
/// programs this invocation runs as root.
fn judge_entries(root: &std::path::Path, owner: u32) -> Result<(), LifecycleRefusal> {
    use std::os::unix::fs::MetadataExt;
    let entries = std::fs::read_dir(root).map_err(|_| LifecycleRefusal::BoundaryUnverified)?;
    for entry in entries {
        let entry = entry.map_err(|_| LifecycleRefusal::BoundaryUnverified)?;
        let metadata = std::fs::symlink_metadata(entry.path())
            .map_err(|_| LifecycleRefusal::BoundaryUnverified)?;
        if !metadata.file_type().is_file()
            || metadata.uid() != owner
            || metadata.mode() & 0o022 != 0
        {
            diag!(
                "weaver-admin: {} in the agent's root is not a closed regular file",
                entry.path().display()
            );
            return Err(LifecycleRefusal::BoundaryUnverified);
        }
    }
    Ok(())
}

/// **Every directory above a judged one is held closed**, as sshd's
/// StrictModes judges a path: resolved once to its canonical path, each
/// directory from its parent up to `/` owned by one of `owners` and writable
/// by no group or other unless its sticky bit is set. Answers the canonical
/// path, through which every later read goes.
fn judge_ancestors(
    path: &std::path::Path,
    owners: &[u32],
) -> Result<std::path::PathBuf, LifecycleRefusal> {
    use std::os::unix::fs::MetadataExt;
    let canonical =
        std::fs::canonicalize(path).map_err(|_| LifecycleRefusal::BoundaryUnverified)?;
    let mut above = canonical.parent();
    while let Some(directory) = above {
        let metadata = std::fs::symlink_metadata(directory)
            .map_err(|_| LifecycleRefusal::BoundaryUnverified)?;
        let held = owners.contains(&metadata.uid());
        let closed = metadata.mode() & 0o022 == 0 || metadata.mode() & 0o1000 != 0;
        if !held || !closed {
            diag!(
                "weaver-admin: {} above {} is held or writable by another principal, so it \
                 cannot be vouched for",
                directory.display(),
                path.display()
            );
            return Err(LifecycleRefusal::BoundaryUnverified);
        }
        above = directory.parent();
    }
    Ok(canonical)
}

/// **The agent's root is the admission**: no root, or one that is not a
/// directory, is no agent, and the look does not follow a link at the root
/// itself. A root is root's and writable by no group or other.
fn judge_root(root: &std::path::Path, owner: u32) -> Result<(), LifecycleRefusal> {
    use std::os::unix::fs::MetadataExt;
    let metadata = match std::fs::symlink_metadata(root) {
        Ok(metadata) => metadata,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(LifecycleRefusal::NoSuchAgent);
        }
        Err(_) => return Err(LifecycleRefusal::BoundaryUnverified),
    };
    if !metadata.is_dir() {
        return Err(LifecycleRefusal::NoSuchAgent);
    }
    if metadata.uid() != owner || metadata.mode() & 0o022 != 0 {
        return Err(LifecycleRefusal::BoundaryUnverified);
    }
    Ok(())
}

/// **The operator's declaration directory, judged before a value in it is
/// read**, per Spec section 9 and the operator's ruling of 2026-10-02: not a
/// link, a directory owned by exactly the `operator` uid, granting nothing to
/// group or other and carrying no access-control entry beyond its mode, every
/// directory above it owned by uid 0 or the operator and closed, and the
/// `agent.toml` in it a regular file, never a link, owned by the operator or
/// uid 0 and writable by no group or other. A directory with no `agent.toml`
/// is no agent. Answers the canonical directory.
fn judge_declaration_directory(
    directory: &std::path::Path,
    operator: u32,
) -> Result<std::path::PathBuf, LifecycleRefusal> {
    use std::os::unix::fs::MetadataExt;
    let refuse = |what: &str| {
        diag!(
            "weaver-admin: the declaration directory {} {what}",
            directory.display()
        );
        LifecycleRefusal::BoundaryUnverified
    };
    let metadata = std::fs::symlink_metadata(directory).map_err(|_| refuse("does not exist"))?;
    if !metadata.is_dir() {
        return Err(refuse("is not a directory"));
    }
    if metadata.uid() != operator {
        return Err(refuse("is not the operator's"));
    }
    if metadata.mode() & 0o077 != 0 {
        return Err(refuse("grants a permission to group or other"));
    }
    if carries_access_entries(directory) {
        return Err(refuse("carries an access-control entry beyond its mode"));
    }
    let canonical = judge_ancestors(directory, &[operator, 0])?;
    let declaration = canonical.join("agent.toml");
    match std::fs::symlink_metadata(&declaration) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(LifecycleRefusal::NoSuchAgent);
        }
        Err(_) => return Err(refuse("holds an agent.toml that cannot be read")),
        Ok(file) => {
            if !file.file_type().is_file()
                || (file.uid() != operator && file.uid() != 0)
                || file.mode() & 0o022 != 0
            {
                return Err(refuse(
                    "holds an agent.toml that is not a closed regular file",
                ));
            }
        }
    }
    Ok(canonical)
}

/// **The prompt file's reader**, per Spec section 4 as of 2026-10-02: the
/// bare name the declaration's `identity-file` carries, opened inside the
/// declaration directory section 9 judged and never by a pathname
/// re-resolved after it. The directory is held by a descriptor opened
/// without following a link and judged again on that descriptor, the
/// operator's and closed to group and other, and the name is opened through
/// it, so no path component can move between the judgment and the read. The
/// file is judged on its own descriptor: a regular file, never a symbolic
/// link, **the operator's own and with link count one**, writable by no
/// group or other. The uid-0 admission `agent.toml` carries is not extended
/// to the prompt and a second link refuses, because a hard link is not a
/// symbolic link: an operator-placed link to a root-owned file elsewhere
/// would otherwise pass, and admin, as root, would read that file's bytes
/// and seat them as the prompt (the question on #76, answered as its option
/// (a) pending the operator's word). **The read is bounded**: a file past
/// `PROMPT_CEILING` refuses on its size before any byte is read, and the
/// read takes no more than the ceiling, so a declaration naming a large
/// file never has it read whole into the enter. A failed judgment refuses
/// `BoundaryUnverified` naming the file on stderr. **Only an absent file
/// answers nothing**, which the parse refuses `ConfigInvalid` naming the
/// field, the omission being the declaration's; every other failure to
/// open or to read is the boundary's fault and refuses `BoundaryUnverified`.
fn read_prompt(
    directory: &std::path::Path,
    operator: u32,
    name: &str,
) -> Result<Option<Vec<u8>>, LifecycleRefusal> {
    use nix::fcntl::OFlag;
    use nix::sys::stat::Mode;
    use std::io::Read;
    let path = directory.join(name);
    let refuse = |what: &str| {
        diag!("weaver-admin: the prompt file {} {what}", path.display());
        LifecycleRefusal::BoundaryUnverified
    };
    let held = nix::fcntl::open(
        directory,
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::empty(),
    )
    .map_err(|e| refuse(&format!("sits in a directory admin cannot hold: {e}")))?;
    let directory_stat = nix::sys::stat::fstat(&held)
        .map_err(|e| refuse(&format!("sits in a directory that does not stat: {e}")))?;
    if directory_stat.st_uid != operator || directory_stat.st_mode & 0o077 != 0 {
        return Err(refuse(
            "sits in a directory no longer as section 9 judged it",
        ));
    }
    let file = match nix::fcntl::openat(
        &held,
        name,
        OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC | OFlag::O_NONBLOCK,
        Mode::empty(),
    ) {
        Ok(file) => file,
        Err(nix::errno::Errno::ENOENT) => {
            diag!("weaver-admin: the prompt file {} is absent", path.display());
            return Ok(None);
        }
        Err(nix::errno::Errno::ELOOP) => {
            return Err(refuse("is a link, and admin reads no link as the prompt"));
        }
        Err(e) => return Err(refuse(&format!("does not open: {e}"))),
    };
    let stat = nix::sys::stat::fstat(&file).map_err(|e| refuse(&format!("does not stat: {e}")))?;
    if stat.st_mode & nix::libc::S_IFMT != nix::libc::S_IFREG
        || stat.st_uid != operator
        || stat.st_mode & 0o022 != 0
    {
        return Err(refuse("is not a closed regular file of the operator's own"));
    }
    if stat.st_nlink != 1 {
        return Err(refuse(
            "carries a second link, and admin reads no linked file as the prompt",
        ));
    }
    let size = u64::try_from(stat.st_size).unwrap_or(u64::MAX);
    if size > PROMPT_CEILING {
        return Err(refuse(&format!(
            "is {size} bytes, past the prompt ceiling of {PROMPT_CEILING}"
        )));
    }
    let mut bytes = Vec::new();
    std::fs::File::from(file)
        .take(PROMPT_CEILING + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| refuse(&format!("does not read: {e}")))?;
    if bytes.len() as u64 > PROMPT_CEILING {
        return Err(refuse(&format!(
            "grew past the prompt ceiling of {PROMPT_CEILING} while read"
        )));
    }
    Ok(Some(bytes))
}

/// The most a prompt file may hold, one mebibyte, this act's election per
/// Spec section 4 pending the operator's word: the same order as the state
/// seam's answer ceiling, far past any prompt an operator writes as prose,
/// and small enough that a declaration pointing at the wrong file costs
/// admin, reading as root, one bounded read and never a whole file.
const PROMPT_CEILING: u64 = 1024 * 1024;

/// Whether a path carries a POSIX access-control list, access or default,
/// read without following a link.
fn carries_access_entries(path: &std::path::Path) -> bool {
    let Ok(c_path) = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()) else {
        return true;
    };
    [c"system.posix_acl_access", c"system.posix_acl_default"]
        .iter()
        .any(|name| {
            // SAFETY: a size query with no buffer, on NUL-terminated strings.
            let size = unsafe {
                nix::libc::lgetxattr(c_path.as_ptr(), name.as_ptr(), std::ptr::null_mut(), 0)
            };
            size >= 0
        })
}

/// **`library-path` is judged as the root is**, per Spec section 6: a
/// directory, never a link at its own name, owned by `owner` and writable by
/// no group or other, its ancestors closed, because whatever it holds is
/// loaded into the worker and its organs. Answers the canonical directory.
fn judge_library_path(
    libraries: &std::path::Path,
    owner: u32,
) -> Result<std::path::PathBuf, LifecycleRefusal> {
    use std::os::unix::fs::MetadataExt;
    let refuse = |what: &str| {
        diag!("weaver-admin: library-path {} {what}", libraries.display());
        LifecycleRefusal::BoundaryUnverified
    };
    let metadata = std::fs::symlink_metadata(libraries).map_err(|_| refuse("does not exist"))?;
    if !metadata.is_dir() {
        return Err(refuse("is not a directory"));
    }
    if metadata.uid() != owner || metadata.mode() & 0o022 != 0 {
        return Err(refuse("is not root's and closed"));
    }
    judge_ancestors(libraries, &[owner, 0])
}

/// Reads the boundary file, answering why where it does not read or parse.
fn read_boundary(root: &std::path::Path) -> Result<BoundaryRead, String> {
    let bytes = std::fs::read(root.join(BOUNDARY_FILE))
        .map_err(|e| format!("{BOUNDARY_FILE}: the boundary file does not read: {e}"))?;
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| format!("{BOUNDARY_FILE}: the boundary file is not UTF-8"))?;
    let parsed = weaver_types::parse_boundary(text).map_err(|e| format!("{BOUNDARY_FILE}: {e}"))?;
    let digest = {
        use sha2::Digest;
        sha2::Sha256::digest(&bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    };
    Ok(BoundaryRead {
        digest,
        reader: parsed.trace_reader,
    })
}

/// Reads the root's values, per Spec section 9. Required: `worker-binary`,
/// `spu-binary`, `gate-binary`, `coordination-root`, `declaration-directory`,
/// `operator` and `roles.toml`. Optional: `headroom-bytes`, `library-path` and
/// `load-bound-seconds`. A failure names the value;
/// a failure of the boundary file starts with its name, which the caller
/// carries as the refusal's field.
fn load_service_config_from(root: &std::path::Path, agent: &str) -> Result<ServiceConfig, String> {
    // **A required value fails whether absent or unreadable**, and the
    // message says which, per Spec section 9.
    let read = |name: &str| -> Result<String, String> {
        std::fs::read_to_string(root.join(name))
            .map(|s| s.trim().to_string())
            .map_err(|e| match std::fs::symlink_metadata(root.join(name)) {
                Err(absent) if absent.kind() == std::io::ErrorKind::NotFound => {
                    format!("the service configuration has no {name}")
                }
                _ => format!("the service configuration's {name} does not read: {e}"),
            })
    };
    // **An optional value is absent only where nothing stands at its path**,
    // asked of the link and never of its target, per Spec section 9: a
    // dangling link, a directory, bytes that are not UTF-8 or a refused read
    // is the operator's file failing to read, never a default.
    let optional = |name: &str| -> Result<Option<String>, String> {
        match std::fs::symlink_metadata(root.join(name)) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => {
                return Err(format!(
                    "the service configuration's {name} does not read: {e}"
                ));
            }
            Ok(_) => {}
        }
        std::fs::read_to_string(root.join(name))
            .map(|s| Some(s.trim().to_string()))
            .map_err(|e| format!("the service configuration's {name} does not read: {e}"))
    };
    // **Every path a key names is absolute**, per Spec section 9, so no read
    // resolves against the working directory a caller ran sudo from and two
    // invocations of one root always name the same files.
    let absolute = |name: &str, value: String| -> Result<PathBuf, String> {
        let path = PathBuf::from(value);
        if path.is_absolute() {
            Ok(path)
        } else {
            Err(format!(
                "the service configuration's {name} is not an absolute path"
            ))
        }
    };
    let path = |name: &str| -> Result<PathBuf, String> { absolute(name, read(name)?) };
    let optional_path = |name: &str| -> Result<Option<PathBuf>, String> {
        optional(name)?
            .filter(|v| !v.is_empty())
            .map(|v| absolute(name, v))
            .transpose()
    };
    let worker = path("worker-binary")?;
    let spu = path("spu-binary")?;
    let gate = path("gate-binary")?;
    stack::judge_names(&worker, &gate, &spu)?;
    let operator = read("operator")?
        .parse::<u32>()
        .map_err(|_| "the service configuration's operator is not a uid".to_string())?;
    let load_bound = match optional("load-bound-seconds")?.filter(|v| !v.is_empty()) {
        None => DEFAULT_LOAD_BOUND,
        Some(text) => match text.parse::<u64>() {
            // A count that cannot form a deadline on this box refuses here,
            // never at the enter's wait.
            Ok(seconds)
                if seconds > 0
                    && std::time::Instant::now()
                        .checked_add(std::time::Duration::from_secs(seconds))
                        .is_some() =>
            {
                std::time::Duration::from_secs(seconds)
            }
            _ => return Err(
                "the service configuration's load-bound-seconds is not a positive count of seconds"
                    .to_string(),
            ),
        },
    };
    let boundary = read_boundary(root);
    Ok(ServiceConfig {
        agent: agent.to_string(),
        coordination_root: path("coordination-root")?,
        worker,
        spu,
        gate,
        headroom_bytes: optional("headroom-bytes")?.filter(|v| !v.is_empty()),
        library_path: optional_path("library-path")?,
        load_bound,
        declaration_directory: path("declaration-directory")?,
        operator,
        operator_gid: 0,
        boundary,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **The cause is the uid sudo reports, parsed strictly**, per
    /// `weaver-admin-Spec` section 2: absent is a root shell and uid 0, a
    /// decimal uid is that uid, and anything else refuses `Malformed` rather
    /// than falling back. Perturbation: answer uid 0 for a value that does
    /// not parse and the malformed cases record a cause nobody gave.
    #[test]
    fn the_cause_is_sudo_uid_parsed_strictly() {
        use std::ffi::OsStr;
        assert_eq!(cause_from(None), Ok(weaver_types::Cause { uid: 0 }));
        assert_eq!(
            cause_from(Some(OsStr::new("1000"))),
            Ok(weaver_types::Cause { uid: 1000 })
        );
        assert_eq!(
            cause_from(Some(OsStr::new("0"))),
            Ok(weaver_types::Cause { uid: 0 })
        );
        for malformed in [
            "",
            "-1",
            "+5",
            " 1000",
            "1000 ",
            "01000",
            "1e3",
            "4294967296",
            "x",
        ] {
            assert_eq!(
                cause_from(Some(OsStr::new(malformed))),
                Err(LifecycleRefusal::Malformed),
                "{malformed:?} refuses"
            );
        }
    }

    /// Spawn `readlink` over the named child descriptors with the given
    /// pre-exec arming, and return what each resolved to - the socket's
    /// inode identity, or an empty string where the number held nothing.
    /// Identity rather than existence, because a child's own bookkeeping
    /// can land a descriptor on a number this test is watching.
    fn probe_child_fds<F>(arming: F, numbers: &[i32]) -> Vec<String>
    where
        F: FnMut() -> std::io::Result<()> + Send + Sync + 'static,
    {
        let mut probe = std::process::Command::new("/usr/bin/readlink");
        for number in numbers {
            probe.arg(format!("/proc/self/fd/{number}"));
        }
        probe
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null());
        unsafe {
            use std::os::unix::process::CommandExt;
            probe.pre_exec(arming);
        }
        let held = probe.output().expect("the probe runs");
        std::str::from_utf8(&held.stdout)
            .expect("fd targets are ascii")
            .lines()
            .map(str::to_string)
            .collect()
    }

    /// This process's own reading of a descriptor's identity, the string
    /// the child's probe must match for the same open file description.
    fn own_fd_identity(raw: i32) -> String {
        std::fs::read_link(format!("/proc/self/fd/{raw}"))
            .expect("own descriptor resolves")
            .to_string_lossy()
            .into_owned()
    }

    /// **The third walk's enumerate half, on the real arming path**, per
    /// `weaver-admin-Spec` section 10 as amended 2026-08-26: spawn a
    /// subprocess with the member's spawn arming, enumerate its descriptors,
    /// and confirm none of this process's crossed but the one the spawn
    /// deliberately arms. Two layouts, because the arming has two paths:
    /// the ordinary one, the end above the target and `dup2` moving it, and
    /// the equal-number corner, the end already at three with close-on-exec
    /// set, where `dup2` is a no-op and the unconditional flag clear is the
    /// whole repair, per `weaver-harness-Spec` section 2.2.
    ///
    /// Perturbation: remove the flag clear from `arm_member_end` and the
    /// corner layout fails, the armed number closing at exec. Remove the
    /// `dup2` and the ordinary layout fails.
    #[test]
    fn the_spawn_arms_the_gift_and_nothing_else_crosses() {
        // Layout one, ordinary: the end relocated above the child's own
        // low numbers, so the listing below cannot alias it.
        let (harness_end, member_end) = nix::sys::socket::socketpair(
            nix::sys::socket::AddressFamily::Unix,
            nix::sys::socket::SockType::Stream,
            None,
            nix::sys::socket::SockFlag::SOCK_CLOEXEC,
        )
        .expect("the pair");
        let relocate = |end: std::os::fd::OwnedFd| -> std::os::fd::OwnedFd {
            let raw = nix::fcntl::fcntl(&end, nix::fcntl::FcntlArg::F_DUPFD_CLOEXEC(16))
                .expect("relocates");
            // SAFETY: a fresh descriptor this process owns, adopted once,
            // the low original dropping with `end`.
            unsafe {
                use std::os::fd::FromRawFd;
                std::os::fd::OwnedFd::from_raw_fd(raw)
            }
        };
        let harness_end = relocate(harness_end);
        let member_end = relocate(member_end);
        let harness_raw = {
            use std::os::fd::AsRawFd;
            harness_end.as_raw_fd()
        };
        let member_raw = {
            use std::os::fd::AsRawFd;
            member_end.as_raw_fd()
        };
        let member_identity = own_fd_identity(member_raw);
        let listed = probe_child_fds(move || arm_member_end(member_raw), &[3, harness_raw]);
        assert_eq!(
            listed.first().map(String::as_str),
            Some(member_identity.as_str()),
            "the gift stands at the fixed number as the member's own end: {listed:?}"
        );
        assert_eq!(
            listed.len(),
            1,
            "this process's other end did not cross: {listed:?}"
        );
        drop(harness_end);
        drop(member_end);

        // Layout two, the corner: the end seated at the target with the
        // flag set before the arming runs, so `dup2` is a no-op and only
        // the unconditional clear keeps the gift alive across exec.
        let (own_end, corner_end) = nix::sys::socket::socketpair(
            nix::sys::socket::AddressFamily::Unix,
            nix::sys::socket::SockType::Stream,
            None,
            nix::sys::socket::SockFlag::SOCK_CLOEXEC,
        )
        .expect("the corner pair");
        let corner_raw = {
            use std::os::fd::AsRawFd;
            corner_end.as_raw_fd()
        };
        let corner_identity = own_fd_identity(corner_raw);
        let listed = probe_child_fds(
            move || {
                // Seat the end at three with close-on-exec set, which is
                // the layout an unlucky descriptor table hands the real
                // spawn.
                if unsafe { nix::libc::dup2(corner_raw, 3) } < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if unsafe { nix::libc::fcntl(3, nix::libc::F_SETFD, nix::libc::FD_CLOEXEC) } < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                arm_member_end(3)
            },
            &[3],
        );
        assert_eq!(
            listed.first().map(String::as_str),
            Some(corner_identity.as_str()),
            "the corner's gift survives exec on the flag clear alone: {listed:?}"
        );
        drop(own_end);
        drop(corner_end);
    }

    /// **The vector this crate composes, in both directions**, per
    /// `weaver-admin-Spec` section 6 as ruled 2026-08-26: a serving
    /// inventory puts one value on it and a diagnostic inventory puts two,
    /// the preload path the territory with a fixed leaf. Two watches rather
    /// than one, because one would not fail on both directions.
    ///
    /// Perturbation: make the arm that appends the preload path
    /// unconditional and the serving half fails on a load carrying two.
    /// Remove the arm and the diagnostic half fails on a load carrying one.
    #[test]
    fn the_vector_follows_the_kind_in_both_directions() {
        let territory = std::path::Path::new("/dbpool/agents/alpha/state");
        let serving_binding = weaver_types::EnterBinding::Serving {
            gate_instruction: weaver_types::GateInstruction {
                access_rule: weaver_types::AccessRule {
                    allowed_uids: Default::default(),
                    allowed_gids: Default::default(),
                    denied_uids: Default::default(),
                },
            },
        };
        let serving = member_vector(territory, &serving_binding, false);
        assert_eq!(
            serving.len(),
            1,
            "a serving load carries the territory alone"
        );
        assert_eq!(serving[0], territory.as_os_str());
        let diagnostic = member_vector(territory, &weaver_types::EnterBinding::Diagnostic, false);
        assert_eq!(
            diagnostic.len(),
            2,
            "a diagnostic load carries the preload path"
        );
        assert_eq!(diagnostic[0], territory.as_os_str());
        assert_eq!(
            diagnostic[1],
            territory.join("preload.sock").into_os_string(),
            "the territory with the fixed leaf, no invocation input composing it"
        );
        // **A serving load that elects a restore names the door too**, per
        // Spec section 6 as of 2026-09-04 and issue #432, the same arm.
        let restoring = member_vector(territory, &serving_binding, true);
        assert_eq!(
            restoring.len(),
            2,
            "a restoring serving load carries the preload path"
        );
        assert_eq!(
            restoring[1],
            territory.join("preload.sock").into_os_string()
        );
    }

    /// **The stack names the binaries this crate started and handed the
    /// worker**, per `weaver-admin-harness-contract` section 3 and Spec
    /// section 9: the worker on every load, the member only where it stood, and
    /// the root's SPU and the gate always.
    ///
    /// Perturbations, each failing an assertion here: name the member whatever
    /// stood; leave the SPU out; leave the gate out.
    #[test]
    fn the_stack_names_what_was_started_and_handed() {
        let config = unread_config();
        let without = stack_digests(&config, false, None);
        let names: Vec<&str> = without.keys().map(String::as_str).collect();
        assert_eq!(names, ["python-spu.pyz", "weaver-gate", "worker"]);
        let with = stack_digests(&config, true, None);
        assert_eq!(
            with.len(),
            4,
            "the worker, the member, the SPU and the gate"
        );
        assert!(with.contains_key("weaver-state"));
        // The classify arm's binary is named where it is handed.
        // Perturbation: drop it from the set and the key is absent.
        let classified = stack_digests(
            &config,
            false,
            Some(std::path::Path::new("/nonexistent/bin/weaver-spu-classify")),
        );
        assert!(classified.contains_key("weaver-spu-classify"));
    }

    /// **The vector carries the root's values and the declaration's loop
    /// file, and nothing composed from the invocation's input**, per Spec
    /// section 6: the socket, the SPU the root names and the gate, then each
    /// named flag only where its value stands. Perturbation: substitute a
    /// fixed SPU path in `worker_arguments` and the first assertion fails;
    /// emit `--classify-binary` unconditionally and the bare vector carries it.
    #[test]
    fn the_roots_spu_reaches_the_vector() {
        let config = unread_config();
        let socket = config.coordination_socket();
        let bare = start::worker_arguments(&socket, &config.spu, &config.gate, None, None, None);
        assert_eq!(
            bare,
            vec![
                socket.display().to_string(),
                "/nonexistent/python/python-spu.pyz".to_string(),
                "/nonexistent/bin/weaver-gate".to_string(),
            ]
        );
        let full = start::worker_arguments(
            &socket,
            &config.spu,
            &config.gate,
            Some("4096"),
            Some(std::path::Path::new("/home/op/loop.py")),
            Some(std::path::Path::new("/opt/weaver/bin/weaver-spu-classify")),
        );
        for (flag, value) in [
            ("--headroom-bytes", "4096"),
            ("--loop-file", "/home/op/loop.py"),
            ("--classify-binary", "/opt/weaver/bin/weaver-spu-classify"),
        ] {
            let at = full.iter().position(|a| a == flag).expect(flag);
            assert_eq!(full[at + 1], value);
        }
    }

    /// The values every root carries, written into a scratch root, with the
    /// operator's declaration directory beside it, `<root>.decl`, owned by
    /// this test's uid as the root's `operator`, closed, and holding an empty
    /// `agent.toml`. Answers the declaration directory.
    fn write_root(root: &std::path::Path) -> std::path::PathBuf {
        use std::os::unix::fs::PermissionsExt;
        std::fs::create_dir_all(root).unwrap();
        let declarations = root.with_extension("decl");
        let _ = std::fs::remove_dir_all(&declarations);
        std::fs::create_dir_all(&declarations).unwrap();
        std::fs::set_permissions(&declarations, std::fs::Permissions::from_mode(0o700)).unwrap();
        std::fs::write(declarations.join("agent.toml"), "").unwrap();
        std::fs::set_permissions(
            declarations.join("agent.toml"),
            std::fs::Permissions::from_mode(0o600),
        )
        .unwrap();
        let operator = nix::unistd::getuid().as_raw().to_string();
        let declaration_directory = declarations.display().to_string();
        for (name, text) in [
            ("coordination-root", "/run/weaver"),
            ("worker-binary", "/opt/weaver/bin/worker"),
            ("spu-binary", "/opt/weaver/bin/weaver-spu"),
            ("gate-binary", "/opt/weaver/bin/weaver-gate"),
            ("declaration-directory", declaration_directory.as_str()),
            ("operator", operator.as_str()),
            ("roles.toml", "trace-reader = \"weaver-alpha-admincon\"\n"),
        ] {
            std::fs::write(root.join(name), text).unwrap();
        }
        declarations
    }

    /// **A root naming two binaries under one file name fails the read**,
    /// before any verb, per Spec section 9, and a sound root reads as this
    /// agent's, its declaration beside it. Perturbation: drop the
    /// `judge_names` call from the read and the second assertion fails.
    #[test]
    fn a_root_naming_two_binaries_under_one_name_fails_the_read() {
        let root =
            std::env::temp_dir().join(format!("weaver-admin-root-read-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        write_root(&root);
        let config = load_service_config_from(&root, "alpha").expect("a sound root reads");
        assert_eq!(config.agent, "alpha");
        assert_eq!(
            config.boundary.as_ref().unwrap().digest.len(),
            64,
            "the boundary file's sha256 hex"
        );
        std::fs::write(root.join("gate-binary"), "/opt/other/worker").unwrap();
        let failure = load_service_config_from(&root, "alpha")
            .err()
            .unwrap_or_default();
        assert!(failure.contains("share the file name"), "{failure:?}");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// **The agent's root is admitted only as a directory root holds that no
    /// other principal may write**, per Spec section 9. Judged here against
    /// this test's own uid, the one parameter production fixes at root's.
    ///
    /// Perturbations: drop the owner comparison and the first refusal
    /// admits; drop the mode test and the group- and world-writable cases
    /// admit.
    #[test]
    fn the_root_is_admitted_only_owned_and_closed() {
        use std::os::unix::fs::PermissionsExt;
        let me = nix::unistd::getuid().as_raw();
        let base =
            std::env::temp_dir().join(format!("weaver-admin-root-judge-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let root = base.join("alpha");
        std::fs::create_dir_all(&root).unwrap();
        let mode = |bits| std::fs::set_permissions(&root, std::fs::Permissions::from_mode(bits));
        mode(0o755).unwrap();
        assert_eq!(judge_root(&root, me), Ok(()));
        if me != 0 {
            assert_eq!(
                judge_root(&root, 0),
                Err(LifecycleRefusal::BoundaryUnverified),
                "a root another uid holds"
            );
        }
        for open in [0o775, 0o757, 0o777] {
            mode(open).unwrap();
            assert_eq!(
                judge_root(&root, me),
                Err(LifecycleRefusal::BoundaryUnverified),
                "{open:o} lets another principal write"
            );
        }
        mode(0o755).unwrap();
        assert_eq!(
            judge_root(&base.join("beta"), me),
            Err(LifecycleRefusal::NoSuchAgent),
            "no root is no agent"
        );
        std::fs::write(base.join("gamma"), "a file").unwrap();
        assert_eq!(
            judge_root(&base.join("gamma"), me),
            Err(LifecycleRefusal::NoSuchAgent),
            "a file is not a root"
        );
        std::os::unix::fs::symlink(&root, base.join("delta")).unwrap();
        assert_eq!(
            judge_root(&base.join("delta"), me),
            Err(LifecycleRefusal::NoSuchAgent),
            "a link at the root is not followed"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    /// **Every directory above the root is closed as the root is**, per Spec
    /// section 9: a directory a group or the world may write refuses
    /// `BoundaryUnverified` unless it is sticky, and a closed or sticky one
    /// admits. Codex on #45, round 9: a writable ancestor let another principal
    /// swap the judged root before the reads. Perturbation: drop the
    /// `judge_ancestors` call, and the open base reads.
    #[test]
    fn every_directory_above_the_root_is_closed() {
        use std::os::unix::fs::PermissionsExt;
        let me = nix::unistd::getuid().as_raw();
        let base =
            std::env::temp_dir().join(format!("weaver-admin-ancestors-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let root = base.join("alpha");
        write_root(&root);
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
        let base_mode =
            |bits| std::fs::set_permissions(&base, std::fs::Permissions::from_mode(bits)).unwrap();
        for open in [0o775, 0o757, 0o777] {
            base_mode(open);
            assert_eq!(
                load_service_config_at(&base, "alpha", me).err(),
                Some(LifecycleRefusal::BoundaryUnverified),
                "a base of mode {open:o} lets another principal swap the root"
            );
        }
        for closed in [0o755, 0o1777] {
            base_mode(closed);
            assert!(
                load_service_config_at(&base, "alpha", me).is_ok(),
                "a base of mode {closed:o} admits"
            );
        }
        // A writable directory two levels up refuses as the parent does.
        let deeper = base.join("deeper");
        write_root(&deeper.join("beta"));
        std::fs::set_permissions(deeper.join("beta"), std::fs::Permissions::from_mode(0o755))
            .unwrap();
        std::fs::set_permissions(&deeper, std::fs::Permissions::from_mode(0o755)).unwrap();
        base_mode(0o777);
        assert_eq!(
            load_service_config_at(&deeper, "beta", me).err(),
            Some(LifecycleRefusal::BoundaryUnverified),
            "a writable grandparent"
        );
        base_mode(0o755);
        let _ = std::fs::remove_dir_all(&base);
    }

    /// **A declaration directory with no `agent.toml` is no agent, and one
    /// that is not a closed regular file refuses**, per Spec section 9: the
    /// root's keys standing do not make an agent. Perturbations: drop the
    /// declaration check, or judge presence alone, and a case here reads.
    #[test]
    fn a_declaration_directory_without_a_declaration_is_no_agent() {
        use std::os::unix::fs::PermissionsExt;
        let me = nix::unistd::getuid().as_raw();
        let base =
            std::env::temp_dir().join(format!("weaver-admin-no-decl-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let root = base.join("alpha");
        let declarations = write_root(&root);
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
        let config = load_service_config_at(&base, "alpha", me).expect("a declared agent reads");
        assert_eq!(config.agent, "alpha");
        assert_eq!(
            config.declaration_directory,
            std::fs::canonicalize(&declarations).unwrap()
        );
        std::fs::remove_file(declarations.join("agent.toml")).unwrap();
        assert_eq!(
            load_service_config_at(&base, "alpha", me).err(),
            Some(LifecycleRefusal::NoSuchAgent),
            "no agent.toml is no agent"
        );
        std::fs::create_dir(declarations.join("agent.toml")).unwrap();
        assert_eq!(
            load_service_config_at(&base, "alpha", me).err(),
            Some(LifecycleRefusal::BoundaryUnverified),
            "a directory named agent.toml"
        );
        std::fs::remove_dir(declarations.join("agent.toml")).unwrap();
        std::os::unix::fs::symlink(declarations.join("gone"), declarations.join("agent.toml"))
            .unwrap();
        assert_eq!(
            load_service_config_at(&base, "alpha", me).err(),
            Some(LifecycleRefusal::BoundaryUnverified),
            "a link named agent.toml"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    /// **The declaration directory is closed to everyone but the operator**,
    /// per Spec section 9: owned by exactly the root's `operator`, granting
    /// nothing to group or other, and never a link at its own name.
    /// Perturbations: drop the owner comparison and the foreign operator
    /// reads; test only the write bits and the group-readable directory reads.
    #[test]
    fn the_declaration_directory_is_the_operators_and_closed() {
        use std::os::unix::fs::PermissionsExt;
        let me = nix::unistd::getuid().as_raw();
        let base = std::env::temp_dir().join(format!("weaver-admin-decl-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let root = base.join("alpha");
        let declarations = write_root(&root);
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(load_service_config_at(&base, "alpha", me).is_ok());
        for open in [0o740, 0o750, 0o704, 0o701] {
            std::fs::set_permissions(&declarations, std::fs::Permissions::from_mode(open)).unwrap();
            assert_eq!(
                load_service_config_at(&base, "alpha", me).err(),
                Some(LifecycleRefusal::BoundaryUnverified),
                "{open:o} grants group or other a permission"
            );
        }
        std::fs::set_permissions(&declarations, std::fs::Permissions::from_mode(0o700)).unwrap();
        std::fs::write(root.join("operator"), (me + 1).to_string()).unwrap();
        assert_eq!(
            load_service_config_at(&base, "alpha", me).err(),
            Some(LifecycleRefusal::BoundaryUnverified),
            "a directory another operator owns"
        );
        std::fs::write(root.join("operator"), me.to_string()).unwrap();
        let link = base.join("linked.decl");
        std::os::unix::fs::symlink(&declarations, &link).unwrap();
        std::fs::write(
            root.join("declaration-directory"),
            link.display().to_string(),
        )
        .unwrap();
        assert_eq!(
            load_service_config_at(&base, "alpha", me).err(),
            Some(LifecycleRefusal::BoundaryUnverified),
            "a link at the directory's own name"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    /// **The prompt file is read through the judged directory and judged as
    /// `agent.toml` is**, per Spec section 4 as of 2026-10-02: a regular
    /// file the operator owns, writable by no group or other, answers its
    /// bytes. An absent one answers nothing, which the parse refuses as the
    /// declaration's omission. A link, a group-writable file, a FIFO, or a
    /// directory no longer closed refuses `BoundaryUnverified`, the
    /// boundary's fault and never the declaration's.
    /// A hard link is not a symbolic link, so a file carrying a second
    /// link refuses on its link count, judged on the opened descriptor,
    /// and reads again once the link is gone.
    /// Perturbations: drop `O_NOFOLLOW` and the link reads its target; drop
    /// the write-bit test and the group-writable file reads; drop the
    /// directory's re-judgment and the opened directory reads; drop the
    /// regular-file test and the FIFO answers empty bytes; drop the link
    /// count and the hard-linked file reads; drop both the size judgment
    /// and the read's bound and the file one byte past the ceiling reads,
    /// either alone being caught by the other. A failure to open for any
    /// reason but absence refuses the boundary, which no honest case can
    /// make as the operator, so the arms carry it.
    #[test]
    fn the_prompt_file_is_read_through_the_judged_directory() {
        use std::os::unix::fs::PermissionsExt;
        let me = nix::unistd::getuid().as_raw();
        let base = std::env::temp_dir().join(format!("weaver-admin-prompt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        std::fs::set_permissions(&base, std::fs::Permissions::from_mode(0o700)).unwrap();
        let prompt = base.join("system-prompt.md");
        std::fs::write(&prompt, "You are Karl.\n").unwrap();
        std::fs::set_permissions(&prompt, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(
            read_prompt(&base, me, "system-prompt.md"),
            Ok(Some(b"You are Karl.\n".to_vec()))
        );
        assert_eq!(
            read_prompt(&base, me, "absent.md"),
            Ok(None),
            "absent answers nothing"
        );
        let refused = Err(LifecycleRefusal::BoundaryUnverified);
        std::os::unix::fs::symlink(&prompt, base.join("linked.md")).unwrap();
        assert_eq!(read_prompt(&base, me, "linked.md"), refused, "a link");
        std::fs::set_permissions(&prompt, std::fs::Permissions::from_mode(0o620)).unwrap();
        assert_eq!(
            read_prompt(&base, me, "system-prompt.md"),
            refused,
            "group-writable"
        );
        std::fs::set_permissions(&prompt, std::fs::Permissions::from_mode(0o600)).unwrap();
        nix::unistd::mkfifo(
            &base.join("fifo.md"),
            nix::sys::stat::Mode::from_bits_truncate(0o600),
        )
        .unwrap();
        assert_eq!(read_prompt(&base, me, "fifo.md"), refused, "a FIFO");
        std::fs::hard_link(&prompt, base.join("hard.md")).unwrap();
        assert_eq!(
            read_prompt(&base, me, "system-prompt.md"),
            refused,
            "a file carrying a second link"
        );
        assert_eq!(
            read_prompt(&base, me, "hard.md"),
            refused,
            "and the link itself"
        );
        std::fs::remove_file(base.join("hard.md")).unwrap();
        assert_eq!(
            read_prompt(&base, me, "system-prompt.md"),
            Ok(Some(b"You are Karl.\n".to_vec())),
            "the link gone, the file reads again"
        );
        // **The read is bounded**: a file at the ceiling reads, one byte
        // past it refuses on its size before any byte is read.
        let at = base.join("at.md");
        std::fs::write(&at, vec![b'x'; PROMPT_CEILING as usize]).unwrap();
        std::fs::set_permissions(&at, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(
            read_prompt(&base, me, "at.md").map(|b| b.map(|v| v.len())),
            Ok(Some(PROMPT_CEILING as usize)),
            "a file at the ceiling reads"
        );
        let past = base.join("past.md");
        std::fs::write(&past, vec![b'x'; PROMPT_CEILING as usize + 1]).unwrap();
        std::fs::set_permissions(&past, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(
            read_prompt(&base, me, "past.md"),
            refused,
            "one byte past the ceiling refuses"
        );
        std::fs::set_permissions(&base, std::fs::Permissions::from_mode(0o750)).unwrap();
        assert_eq!(
            read_prompt(&base, me, "system-prompt.md"),
            refused,
            "a directory opened since its judgment"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    /// **A missing or malformed boundary file refuses naming it, at the verbs
    /// that require it and no others**, per Spec section 9: the root still
    /// reads, so `unload`, `stop` and `show` keep a running agent's recovery
    /// path with no digest in their log lines, and `require_boundary`, which
    /// `validate` and `load` call, refuses naming `roles.toml`. Every other
    /// value's failure names nothing. Perturbation: refuse the read itself on
    /// a damaged boundary file and the first assertion fails.
    #[test]
    fn the_boundary_file_is_required_only_where_named() {
        use std::os::unix::fs::PermissionsExt;
        let me = nix::unistd::getuid().as_raw();
        let base = std::env::temp_dir().join(format!("weaver-admin-roles-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let root = base.join("alpha");
        write_root(&root);
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
        let named = Err(LifecycleRefusal::ConfigInvalid {
            field: Some(weaver_types::FieldName("roles.toml".into())),
        });
        for (damage, why) in [
            (None, "missing"),
            (Some("trace-reader = \"x\"\nroles = 1\n"), "an unknown key"),
        ] {
            match damage {
                None => std::fs::remove_file(root.join("roles.toml")).unwrap(),
                Some(text) => std::fs::write(root.join("roles.toml"), text).unwrap(),
            }
            let config = load_service_config_at(&base, "alpha", me)
                .unwrap_or_else(|e| panic!("{why}: the root still reads, got {e:?}"));
            assert_eq!(config.boundary_digest(), None, "{why}: no digest to log");
            assert_eq!(config.require_boundary().map(|_| ()), named, "{why}");
        }
        std::fs::write(root.join("roles.toml"), "trace-reader = \"x\"\n").unwrap();
        std::fs::remove_file(root.join("spu-binary")).unwrap();
        assert_eq!(
            load_service_config_at(&base, "alpha", me).err(),
            Some(LifecycleRefusal::ConfigInvalid { field: None }),
            "another value names nothing"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    /// **Every key is closed as the root is.** A key a group or the world may
    /// write, a key that is a link, and a directory inside the root each refuse
    /// `BoundaryUnverified`; a root of closed files reads. Codex on #45, round 7:
    /// the directory's own mode let a writable key name the program admin runs
    /// as root. Perturbation: drop `judge_entries`, and each case reads.
    #[test]
    fn every_key_of_the_root_is_closed() {
        use std::os::unix::fs::PermissionsExt;
        let me = nix::unistd::getuid().as_raw();
        let base = std::env::temp_dir().join(format!("weaver-admin-keys-{}", std::process::id()));
        let fresh = || {
            let _ = std::fs::remove_dir_all(&base);
            let root = base.join("alpha");
            write_root(&root);
            std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
            for entry in std::fs::read_dir(&root).unwrap() {
                let path = entry.unwrap().path();
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
            }
            root
        };
        let root = fresh();
        assert!(
            load_service_config_at(&base, "alpha", me).is_ok(),
            "a closed root reads"
        );
        std::fs::set_permissions(
            root.join("worker-binary"),
            std::fs::Permissions::from_mode(0o666),
        )
        .unwrap();
        assert_eq!(
            load_service_config_at(&base, "alpha", me).err(),
            Some(LifecycleRefusal::BoundaryUnverified),
            "a world-writable key"
        );
        let root = fresh();
        std::fs::remove_file(root.join("gate-binary")).unwrap();
        std::fs::write(base.join("elsewhere"), "/opt/weaver/bin/weaver-gate").unwrap();
        std::os::unix::fs::symlink(base.join("elsewhere"), root.join("gate-binary")).unwrap();
        assert_eq!(
            load_service_config_at(&base, "alpha", me).err(),
            Some(LifecycleRefusal::BoundaryUnverified),
            "a key that is a link"
        );
        let root = fresh();
        std::fs::create_dir(root.join("stray")).unwrap();
        assert_eq!(
            load_service_config_at(&base, "alpha", me).err(),
            Some(LifecycleRefusal::BoundaryUnverified),
            "a directory inside the root"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    /// **A malformed name refuses before any path is built from it**, so
    /// `.`, `..` and a name carrying `/` never reach the base.
    #[test]
    fn a_malformed_name_refuses_before_the_base_is_read() {
        for bad in [".", "..", "a/b", "../alpha", "", "al pha"] {
            assert_eq!(
                load_service_config(&AgentName(bad.into())).err(),
                Some(LifecycleRefusal::NoSuchAgent),
                "{bad:?}"
            );
        }
    }

    /// **An optional value is absent only where nothing stands at its path**,
    /// **Every path a key names is absolute, and the enter's bound forms a
    /// deadline**, per Spec sections 9 and 2: a relative value at each path key
    /// fails the read naming the key, as does a `load-bound-seconds` too large
    /// to add to the clock. Perturbations: accept a relative path and the
    /// root reads with its paths resolved against the working directory;
    /// accept any positive count and `u64::MAX` reads, to panic at the
    /// enter's wait.
    #[test]
    fn a_relative_path_or_an_unreachable_bound_fails_the_read() {
        let root =
            std::env::temp_dir().join(format!("weaver-admin-relative-{}", std::process::id()));
        let fresh = || {
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).unwrap();
            write_root(&root);
        };
        for name in [
            "worker-binary",
            "spu-binary",
            "gate-binary",
            "coordination-root",
            "declaration-directory",
            "library-path",
        ] {
            fresh();
            std::fs::write(root.join(name), "relative/path").unwrap();
            let failure = load_service_config_from(&root, "alpha")
                .err()
                .unwrap_or_default();
            assert!(
                failure.contains(name) && failure.contains("not an absolute path"),
                "{name}: {failure:?}"
            );
        }
        fresh();
        std::fs::write(root.join("load-bound-seconds"), u64::MAX.to_string()).unwrap();
        let failure = load_service_config_from(&root, "alpha")
            .err()
            .unwrap_or_default();
        assert!(failure.contains("load-bound-seconds"), "{failure:?}");
        fresh();
        std::fs::write(root.join("load-bound-seconds"), "1800").unwrap();
        assert_eq!(
            load_service_config_from(&root, "alpha").unwrap().load_bound,
            std::time::Duration::from_secs(1800)
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// per Spec section 9: at each optional path a directory and bytes that are
    /// not UTF-8 and a dangling link fail the read, naming the value, and an
    /// absent file leaves the read standing. Perturbations: read every error as
    /// absent, as the first form of this loader did, or decide presence by
    /// following the link, as the second did, and the failing cases are accepted.
    #[test]
    fn an_optional_value_that_does_not_read_fails_the_read() {
        let root =
            std::env::temp_dir().join(format!("weaver-admin-optional-{}", std::process::id()));
        let fresh = || {
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).unwrap();
            write_root(&root);
        };
        for name in ["headroom-bytes", "library-path", "load-bound-seconds"] {
            fresh();
            assert!(
                load_service_config_from(&root, "alpha").is_ok(),
                "{name} absent"
            );
            std::fs::create_dir(root.join(name)).unwrap();
            let failure = load_service_config_from(&root, "alpha")
                .err()
                .unwrap_or_default();
            assert!(failure.contains(name), "{name} as a directory: {failure:?}");
            fresh();
            std::fs::write(root.join(name), [0xff, 0xfe, 0x00]).unwrap();
            let failure = load_service_config_from(&root, "alpha")
                .err()
                .unwrap_or_default();
            assert!(failure.contains(name), "{name} not UTF-8: {failure:?}");
            fresh();
            std::os::unix::fs::symlink(root.join("gone"), root.join(name)).unwrap();
            let failure = load_service_config_from(&root, "alpha")
                .err()
                .unwrap_or_default();
            assert!(
                failure.contains(name),
                "{name} a dangling link: {failure:?}"
            );
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A configuration whose values are never read by the arm under test.
    /// `dispatch`'s observation arm touches no field, which is what lets this
    /// stand without a filesystem.
    fn unread_config() -> ServiceConfig {
        ServiceConfig {
            agent: "alpha".into(),
            coordination_root: PathBuf::from("/nonexistent"),
            worker: PathBuf::from("/nonexistent/bin/worker"),
            spu: PathBuf::from("/nonexistent/python/python-spu.pyz"),
            gate: PathBuf::from("/nonexistent/bin/weaver-gate"),
            headroom_bytes: None,
            library_path: None,
            load_bound: DEFAULT_LOAD_BOUND,
            declaration_directory: PathBuf::from("/nonexistent/declarations"),
            operator: 1000,
            operator_gid: 1000,
            boundary: Ok(BoundaryRead {
                digest: "0".repeat(64),
                reader: "weaver-alpha-admincon".into(),
            }),
        }
    }

    #[test]
    fn stop_checks_the_name_before_building_a_path() {
        let config = unread_config();
        for hostile in ["../../etc", "alpha/../beta", "a/b", "..", ""] {
            let refused = dispatch(
                &config,
                surface::Request::Stop(AgentName(hostile.to_string())),
            );
            assert_eq!(
                refused,
                Err(LifecycleRefusal::NoSuchAgent),
                "{hostile:?} is refused before any path is built"
            );
        }
        // And a well-formed name that is not this root's agent is refused by
        // the same check, so what a verb may name has one answer.
        let refused = dispatch(&config, surface::Request::Stop(AgentName("beta".into())));
        assert_eq!(refused, Err(LifecycleRefusal::NoSuchAgent));
    }

    /// The shared check admits exactly the agent whose root was read and
    /// refuses every shape that could leave the directory.
    #[test]
    fn the_name_check_is_one_answer_for_every_verb() {
        let config = unread_config();
        assert_eq!(admissible(&config, &AgentName("alpha".into())), Ok(()));
        for bad in ["alpha/", "/alpha", "al pha", "alpha\u{0}", "ALPHA/../x"] {
            assert_eq!(
                admissible(&config, &AgentName(bad.to_string())),
                Err(LifecycleRefusal::NoSuchAgent),
                "{bad:?} is not admissible"
            );
        }
    }

    /// **`show` reads the run lock before it dials, and names the run's
    /// constituents beside the harness's word**, per Spec section 3 and
    /// toddwbucy/WeaverWeb#15. A free lock answers `Unloaded` without
    /// dialing, so no state is invented from a missing socket. A lock held by
    /// a stand-in constituent with no worker listening answers `Unloaded`
    /// with that constituent's pid, a run that never entered, for the caller
    /// to end with `unload`. Perturbation: answer the constituents empty and
    /// the second case fails; read a held lock as `Idle` and it fails too.
    #[test]
    fn show_reads_the_lock_and_names_the_constituents() {
        use std::os::unix::process::CommandExt;
        let base = std::env::temp_dir().join(format!("weaver-admin-show-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let mut config = unread_config();
        config.coordination_root = base.clone();
        let run_directory = config.run_directory();
        std::fs::create_dir_all(&run_directory).unwrap();
        assert_eq!(
            show(&config),
            Ok(LifecycleAnswer::State {
                state: weaver_types::AgentState::Unloaded,
                load: None,
                constituents: Vec::new(),
            }),
            "a free lock is no run"
        );
        let lock = start::take_run_lock(&run_directory).unwrap().unwrap();
        let high = start::high(lock.raw()).unwrap();
        let raw = std::os::fd::AsRawFd::as_raw_fd(&high);
        let mut command = std::process::Command::new("/bin/sleep");
        command.arg("30");
        // SAFETY: dup2 is async-signal-safe.
        unsafe {
            command.pre_exec(move || start::place(raw, start::RUN_LOCK_FD));
        }
        let mut child = command.spawn().unwrap();
        drop((high, lock));
        assert_eq!(
            show(&config),
            Ok(LifecycleAnswer::State {
                state: weaver_types::AgentState::Unloaded,
                load: None,
                constituents: vec![child.id()],
            }),
            "a held lock with no worker names who holds it"
        );
        let _ = child.kill();
        let _ = child.wait();
        let _ = std::fs::remove_dir_all(&base);
    }

    /// A scratch coordination root for one test, with the agent's run
    /// directory made inside it as this test's user.
    fn scratch_config(tag: &str) -> (ServiceConfig, crate::scratch::Scratch) {
        let base = std::env::temp_dir().join(format!("weaver-admin-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let mut config = unread_config();
        config.coordination_root = base.clone();
        std::fs::create_dir_all(config.run_directory()).unwrap();
        (config, crate::scratch::Scratch(base))
    }

    /// A stand-in constituent: `sleep` holding the run lock's description at
    /// its fixed number, as a member or a worker does.
    fn stand_in_holder(config: &ServiceConfig) -> std::process::Child {
        use std::os::unix::process::CommandExt;
        let lock = start::take_run_lock(&config.run_directory())
            .unwrap()
            .expect("a free run lock");
        let high = start::high(lock.raw()).unwrap();
        let raw = std::os::fd::AsRawFd::as_raw_fd(&high);
        let mut command = std::process::Command::new("/bin/sleep");
        command.arg("30");
        // SAFETY: dup2 is async-signal-safe.
        unsafe {
            command.pre_exec(move || start::place(raw, start::RUN_LOCK_FD));
        }
        command.spawn().unwrap()
    }

    /// A stand-in worker that binds the coordination socket and accepts, and
    /// then answers nothing: a silent run.
    fn silent_worker(config: &ServiceConfig) -> std::os::fd::OwnedFd {
        let socket = config.coordination_socket();
        std::fs::create_dir_all(socket.parent().unwrap()).unwrap();
        let listener = nix::sys::socket::socket(
            nix::sys::socket::AddressFamily::Unix,
            nix::sys::socket::SockType::SeqPacket,
            nix::sys::socket::SockFlag::SOCK_CLOEXEC,
            None,
        )
        .unwrap();
        let address = nix::sys::socket::UnixAddr::new(&socket).unwrap();
        nix::sys::socket::bind(std::os::fd::AsRawFd::as_raw_fd(&listener), &address).unwrap();
        nix::sys::socket::listen(&listener, nix::sys::socket::Backlog::new(8).unwrap()).unwrap();
        listener
    }

    /// **The coordination root and every directory above it are held closed
    /// before the run directory is made**, per Spec section 3: an open root,
    /// sticky or not, or an open directory above it, refuses
    /// `BoundaryUnverified` and makes nothing, and a closed one admits and
    /// makes the run directory.
    /// Judged against this test's uid, production's owner being uid 0.
    /// Perturbation: drop the judgment from `prepare_run_directory` and the
    /// open cases make the run directory.
    #[test]
    fn the_coordination_root_is_judged_before_any_lock() {
        use std::os::unix::fs::PermissionsExt;
        let me = nix::unistd::getuid().as_raw();
        let base = std::env::temp_dir().join(format!("weaver-admin-coord-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let root = base.join("run");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::set_permissions(&base, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut config = unread_config();
        config.coordination_root = root.clone();
        let mode = |path: &std::path::Path, bits| {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(bits)).unwrap()
        };
        for open in [0o775, 0o757, 0o777, 0o1777] {
            mode(&root, open);
            assert_eq!(
                prepare_run_directory(&config, me).err(),
                Some(LifecycleRefusal::BoundaryUnverified),
                "a coordination root of mode {open:o}, sticky or not"
            );
            assert!(
                !root.join("weaver.run").exists(),
                "and nothing is made in it"
            );
        }
        mode(&root, 0o755);
        assert!(
            prepare_run_directory(&config, me).is_ok(),
            "a closed root admits"
        );
        assert!(
            config.run_directory().is_dir(),
            "and the run directory stands"
        );
        mode(&base, 0o777);
        assert_eq!(
            prepare_run_directory(&config, me).err(),
            Some(LifecycleRefusal::BoundaryUnverified),
            "an open directory above the root"
        );
        mode(&base, 0o755);
        let _ = std::fs::remove_dir_all(&base);
    }

    /// **The trace reader is judged at `validate` and `load`**, per Spec
    /// section 9: the agent's own account or its member's refuses naming
    /// `roles.toml`, a reader the box does not carry refuses the same way, a
    /// reader outside the access group refuses too, and a box with no access
    /// group refuses `BoundaryUnverified`. A reader holding the group by its
    /// primary gid or by membership is admitted. Perturbation: drop the
    /// membership test and the outsider is admitted.
    #[test]
    fn the_trace_reader_is_judged() {
        let agent = AgentName("alpha".into());
        let named = Err(LifecycleRefusal::ConfigInvalid {
            field: Some(weaver_types::FieldName("roles.toml".into())),
        });
        let group = || Some((5000, vec!["weaver-alpha-admincon".to_string()]));
        assert_eq!(
            reader_verdict("weaver-alpha", &agent, Some(5000), group()),
            named
        );
        assert_eq!(
            reader_verdict("weaver-alpha-state", &agent, Some(5000), group()),
            named
        );
        assert_eq!(
            reader_verdict("ghost", &agent, None, group()),
            named,
            "no such user"
        );
        assert_eq!(
            reader_verdict("outsider", &agent, Some(100), group()),
            named,
            "not in the group"
        );
        assert_eq!(
            reader_verdict("weaver-alpha-admincon", &agent, Some(100), None),
            Err(LifecycleRefusal::BoundaryUnverified),
            "the box carries no access group"
        );
        assert_eq!(
            reader_verdict("weaver-alpha-admincon", &agent, Some(100), group()),
            Ok(())
        );
        assert_eq!(
            reader_verdict("primary", &agent, Some(5000), group()),
            Ok(()),
            "by primary gid"
        );
    }

    /// A stand-in worker that answers each dial with the next scripted payload:
    /// it accepts, reads the directive, and closes the exchange with the
    /// answer, one connection per answer, as admin dials once per exchange.
    fn answering_worker(
        config: &ServiceConfig,
        answers: Vec<weaver_types::Payload>,
    ) -> std::thread::JoinHandle<()> {
        let listener = silent_worker(config);
        std::thread::spawn(move || {
            for payload in answers {
                let Ok(raw) = nix::sys::socket::accept(std::os::fd::AsRawFd::as_raw_fd(&listener))
                else {
                    return;
                };
                // SAFETY: accept answered a fresh descriptor this thread owns.
                let fd =
                    unsafe { <std::os::fd::OwnedFd as std::os::fd::FromRawFd>::from_raw_fd(raw) };
                let peer = channel::Coordination::adopt(fd);
                let Ok(request) = peer.recv() else { return };
                let _ = peer.send(&weaver_types::OrganEnvelope {
                    exchange: request.exchange,
                    position: weaver_types::Position::Close,
                    payload,
                });
            }
        })
    }

    /// Short bounds for the unload path, the production values being fixed.
    const TEST_UNLOAD_BOUNDS: UnloadBounds = UnloadBounds {
        leave: std::time::Duration::from_secs(2),
        after_left: std::time::Duration::from_millis(300),
        term: std::time::Duration::from_secs(2),
        kill: std::time::Duration::from_secs(2),
    };

    /// **`unload`'s leave, the wait after `left`, then the escalation**, per
    /// Spec section 3: a worker observed `Idle` is directed to leave and
    /// answers `Left`, but a constituent keeps the run lock past the wait, so
    /// the escalation ends it and the verb answers once the lock is free. The
    /// same path runs where the observation is refused, a refusal being
    /// silence to `unload`. Perturbation: answer on `Left` alone and the holder
    /// survives; propagate the observation's refusal and the second run
    /// refuses `Malformed` with the holder standing.
    #[test]
    fn unload_leaves_waits_and_escalates() {
        for (tag, observed) in [
            (
                "idle",
                weaver_types::Payload::Answer(LifecycleAnswer::State {
                    state: weaver_types::AgentState::Idle,
                    load: None,
                    constituents: Vec::new(),
                }),
            ),
            (
                "refused",
                weaver_types::Payload::Refusal(LifecycleRefusal::Malformed),
            ),
        ] {
            let (config, _scratch) = scratch_config(&format!("unload-leave-{tag}"));
            let mut holder = stand_in_holder(&config);
            let worker = answering_worker(
                &config,
                vec![
                    observed,
                    weaver_types::Payload::Answer(LifecycleAnswer::Left),
                ],
            );
            assert_eq!(
                unload_within(&config, TEST_UNLOAD_BOUNDS),
                Ok(LifecycleAnswer::State {
                    state: weaver_types::AgentState::Unloaded,
                    load: None,
                    constituents: Vec::new(),
                }),
                "{tag}"
            );
            assert!(holder.wait().is_ok(), "{tag}: the holder ended");
            assert!(
                !start::run_lock_held(&config.run_directory()).unwrap(),
                "{tag}"
            );
            let _ = worker.join();
        }
    }

    /// **A load meeting a run whose worker refuses the observation answers
    /// `AgentRunning`**, per Spec section 3: a refusal is an answer, so a run
    /// stands, and the load touches nothing. Perturbation: propagate the
    /// observation's refusal and the load answers `Malformed`.
    #[test]
    fn a_refused_observation_is_a_running_agent_to_a_load() {
        let (config, _scratch) = scratch_config("load-refused-observe");
        let mut holder = stand_in_holder(&config);
        let worker = answering_worker(
            &config,
            vec![weaver_types::Payload::Refusal(LifecycleRefusal::Malformed)],
        );
        let mut standing = Standing::default();
        assert_eq!(
            run_load(&config, &AgentName("alpha".into()), &mut standing),
            Err(LifecycleRefusal::AgentRunning)
        );
        assert!(holder.try_wait().unwrap().is_none(), "the run still stands");
        let _ = holder.kill();
        let _ = holder.wait();
        let _ = worker.join();
    }

    /// **A refused leave returns to the operator unchanged**, per Spec
    /// section 3: `ActivityNotAtRest` answers and nothing is ended.
    /// Perturbation: escalate on any leave failure and the busy run dies.
    #[test]
    fn a_refused_leave_ends_nothing() {
        let (config, _scratch) = scratch_config("unload-busy");
        let mut holder = stand_in_holder(&config);
        let worker = answering_worker(
            &config,
            vec![
                weaver_types::Payload::Answer(LifecycleAnswer::State {
                    state: weaver_types::AgentState::Active,
                    load: None,
                    constituents: Vec::new(),
                }),
                weaver_types::Payload::Refusal(LifecycleRefusal::ActivityNotAtRest),
            ],
        );
        assert_eq!(
            unload_within(&config, TEST_UNLOAD_BOUNDS),
            Err(LifecycleRefusal::ActivityNotAtRest)
        );
        assert!(holder.try_wait().unwrap().is_none(), "the busy run stands");
        let _ = holder.kill();
        let _ = holder.wait();
        let _ = worker.join();
    }

    /// **A failed dial is answered from the worker's own exit**, per Spec
    /// section 6: a worker that already exited refuses `NoResidency`, its
    /// status named on standard error, and one still running with no socket
    /// is a bind that failed. Perturbation: drop the status read and the exited
    /// worker reads as `BindFailed`.
    #[test]
    fn a_failed_dial_reads_the_worker_exit() {
        let mut exited = std::process::Command::new("/bin/sh")
            .args(["-c", "exit 3"])
            .spawn()
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while std::fs::read_to_string(format!("/proc/{}/stat", exited.id()))
            .map(|stat| {
                !stat
                    .rsplit(')')
                    .next()
                    .unwrap_or("")
                    .trim_start()
                    .starts_with('Z')
            })
            .unwrap_or(false)
            && std::time::Instant::now() < deadline
        {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(
            refusal_from_worker(&mut exited),
            LifecycleRefusal::NoResidency
        );
        let mut running = std::process::Command::new("/bin/sleep")
            .arg("30")
            .spawn()
            .unwrap();
        assert_eq!(
            refusal_from_worker(&mut running),
            LifecycleRefusal::BindFailed
        );
        let _ = running.kill();
        let _ = running.wait();
    }

    /// **The reserved suffixes refuse at the name check**, per Spec section
    /// 4: an agent named `x-relay` would share agent `x`'s relay account.
    /// Perturbation: drop the suffix check and each name passes.
    #[test]
    fn a_reserved_suffix_refuses() {
        for reserved in ["x-state", "x-trace", "x-relay", "x-admin", "x-admincon"] {
            assert!(!well_formed(reserved), "{reserved}");
            assert_eq!(
                load_service_config(&AgentName(reserved.into())).err(),
                Some(LifecycleRefusal::NoSuchAgent),
                "{reserved}"
            );
        }
        assert!(well_formed("x-relays"), "a suffix is matched whole");
    }

    /// **A load never ends an existing run**, per Spec section 3: a held run
    /// lock with no worker listening refuses `AgentRunning`, and the holder
    /// still runs. Perturbation: let the load end what holds the lock and the
    /// holder dies under it.
    #[test]
    fn a_load_never_ends_an_existing_run() {
        let (config, _scratch) = scratch_config("load-held");
        let mut holder = stand_in_holder(&config);
        let mut standing = Standing::default();
        assert_eq!(
            run_load(&config, &AgentName("alpha".into()), &mut standing),
            Err(LifecycleRefusal::AgentRunning)
        );
        assert!(holder.try_wait().unwrap().is_none(), "the run still stands");
        let _ = holder.kill();
        let _ = holder.wait();
    }

    /// **A silent worker is never reaped, and the observation is bounded**,
    /// per Spec section 3: a held lock whose worker accepts and answers
    /// nothing refuses the load `Unanswered` and `show` `Unanswered`, each
    /// within the observation's bound, and the holder runs on. Perturbation:
    /// drop the bound from the observation's read and the verb never returns;
    /// read silence as no listener and the load answers `AgentRunning`.
    #[test]
    fn a_silent_run_is_bounded_and_never_reaped() {
        let (config, _scratch) = scratch_config("silent");
        let mut holder = stand_in_holder(&config);
        let _listener = silent_worker(&config);
        let started = std::time::Instant::now();
        let mut standing = Standing::default();
        assert_eq!(
            run_load(&config, &AgentName("alpha".into()), &mut standing),
            Err(LifecycleRefusal::Unanswered)
        );
        assert_eq!(show(&config), Err(LifecycleRefusal::Unanswered));
        assert!(
            started.elapsed() < OBSERVE_BOUND * 2 + std::time::Duration::from_secs(4),
            "both answered within their bounds"
        );
        assert!(
            holder.try_wait().unwrap().is_none(),
            "the silent run still stands"
        );
        let _ = holder.kill();
        let _ = holder.wait();
    }

    /// **`unload` ends a run that never entered**, per Spec section 3: a held
    /// lock with no worker listening goes straight to the escalation, and the
    /// verb answers once the lock is free. Perturbation: direct leave at the
    /// unentered run and refuse on its answer, and the holder survives.
    #[test]
    fn unload_ends_a_run_that_never_entered() {
        let (config, _scratch) = scratch_config("unload-unentered");
        let mut holder = stand_in_holder(&config);
        assert_eq!(
            unload(&config),
            Ok(LifecycleAnswer::State {
                state: weaver_types::AgentState::Unloaded,
                load: None,
                constituents: Vec::new(),
            })
        );
        assert!(holder.wait().is_ok());
        assert!(!start::run_lock_held(&config.run_directory()).unwrap());
    }

    /// **`show` answers a transition in flight**, per Spec section 3: another
    /// process holding the invocation lock exclusively makes `show` answer
    /// `InTransition` at once, without dialing. Perturbation: dial instead and
    /// the answer is the observation's.
    #[test]
    fn show_answers_a_transition_in_flight() {
        let (config, _scratch) = scratch_config("in-transition");
        let path = config.run_directory().join("admin.lock");
        let (ready_read, ready_write) = nix::unistd::pipe().unwrap();
        // Built before the fork, so the child only calls what a fork of a
        // threaded process may.
        let c_path = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
        // SAFETY: the child calls only async-signal-safe functions.
        match unsafe { nix::unistd::fork() }.unwrap() {
            nix::unistd::ForkResult::Child => {
                // SAFETY: open, fcntl, write and pause are async-signal-safe.
                unsafe {
                    let fd = nix::libc::open(
                        c_path.as_ptr(),
                        nix::libc::O_RDWR | nix::libc::O_CREAT,
                        0o600 as nix::libc::c_uint,
                    );
                    let lock = nix::libc::flock {
                        l_type: nix::libc::F_WRLCK as i16,
                        l_whence: nix::libc::SEEK_SET as i16,
                        l_start: 0,
                        l_len: 0,
                        l_pid: 0,
                    };
                    nix::libc::fcntl(fd, nix::libc::F_SETLK, &lock);
                    nix::libc::write(
                        std::os::fd::AsRawFd::as_raw_fd(&ready_write),
                        [1u8].as_ptr().cast(),
                        1,
                    );
                    nix::libc::pause();
                    nix::libc::_exit(0);
                }
            }
            nix::unistd::ForkResult::Parent { child } => {
                drop(ready_write);
                let mut byte = [0u8; 1];
                nix::unistd::read(&ready_read, &mut byte).unwrap();
                assert_eq!(show(&config), Ok(LifecycleAnswer::InTransition));
                // SAFETY: kill on the child this test forked.
                unsafe { nix::libc::kill(child.as_raw(), nix::libc::SIGKILL) };
                let _ = nix::sys::wait::waitpid(child, None);
            }
        }
    }

    /// **The member's territory is the member's own, and a load closes a
    /// room that was left open rather than opening a closed one.**
    ///
    /// Both halves matter and the second is the one issue #545 found: the
    /// preparation ran on every load, so whatever the operator's
    /// provisioning made the room, one load rewrote it to this crate's uid,
    /// the parent's group, and `0750`. A member-owned `0700` room did not
    /// survive a single load, which is why the charter's uid could not be
    /// held by provisioning alone.
    ///
    /// The account stood up is this process's own, which is what lets the
    /// chown run without privilege: a test that named another account would
    /// assert nothing off a root box.
    ///
    /// Perturbation: restore `mode(0o750)`, the `set_permissions(0o750)`,
    /// and the chown of the parent's group, and the fresh room reads `0750`
    /// where `0700` is asserted. Watched failing 2026-09-15, at the first
    /// assertion, which is where the run stops.
    ///
    /// conforms: admin-member-territory-is-the-members-own
    #[test]
    fn the_territory_is_owned_by_the_member_and_closed_on_every_load() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let root = crate::scratch::Scratch(
            std::env::temp_dir().join(format!("wt-territory-{}", std::process::id())),
        );
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("the operator-side directory");
        let member = inventory::MemberAccount {
            uid: nix::unistd::getuid().as_raw(),
            gid: nix::unistd::getgid().as_raw(),
        };

        let territory = prepare_territory(&root, member).expect("the territory is made");
        let made = std::fs::metadata(&territory).expect("it stands");
        assert_eq!(
            made.mode() & 0o777,
            0o700,
            "the room the member is handed grants nobody else anything"
        );
        assert_eq!(made.uid(), member.uid, "and the member owns it");

        // A room widened between loads, which is the case the repair exists
        // for: the operator, another tool, or an earlier build of this crate.
        std::fs::set_permissions(&territory, std::fs::Permissions::from_mode(0o755))
            .expect("widen it");
        let again = prepare_territory(&root, member).expect("the second load");
        assert_eq!(again, territory, "the same room, not a second one");
        assert_eq!(
            std::fs::metadata(&again).expect("it stands").mode() & 0o777,
            0o700,
            "a load closes a room that was left open"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// **The member spawn drops to the member's account, driven through
    /// `stand_state_member` itself** (#673 item 6). Needs euid 0, and runs
    /// through `the_member_spawn_is_watched_inside_a_user_namespace` on a box
    /// where that is not the invoking uid. A stand-in `weaver-state` beside
    /// the worker records the identity it runs under into the territory the
    /// real path prepared, and the reading is the kernel's own status after
    /// exec: every uid the member's, every gid its group's, and the
    /// supplementary set that group alone, none of root's. The member's end
    /// is read too, a socket at the fixed number.
    ///
    /// `drop_to`'s own instrument, in the inventory module, watches the order
    /// of the three calls and the saved ids. This
    /// one watches that the member's spawn takes the drop at all, which no
    /// reading of `drop_to` can see.
    ///
    /// Perturbations, each watched failing 2026-09-26 through the namespace
    /// watch: remove `become_member` from the spawn's pre-exec and the member
    /// runs as uid 0; hand `drop_to` root's group beside the member's and the
    /// supplementary set carries 0.
    ///
    /// conforms: admin-member-spawn-drops-to-its-account
    #[test]
    #[ignore = "needs euid 0: run by the_member_spawn_is_watched_inside_a_user_namespace"]
    fn the_member_spawn_lands_the_members_identity_as_root() {
        use std::os::unix::fs::PermissionsExt;
        assert!(
            nix::unistd::geteuid().is_root(),
            "this instrument needs euid 0"
        );
        /// Removes the tree however the reading ends. It runs as root in
        /// the namespace, the only party that can enter the member's room.
        struct Yard(std::path::PathBuf);
        impl Drop for Yard {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let yard =
            Yard(std::env::temp_dir().join(format!("wt-member-spawn-{}", std::process::id())));
        let _ = std::fs::remove_dir_all(&yard.0);
        let bin = yard.0.join("bin");
        let sink = yard.0.join("sink");
        for dir in [&bin, &sink] {
            std::fs::create_dir_all(dir).expect("the yard's directories");
            std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o755))
                .expect("the member can traverse them");
        }
        std::fs::set_permissions(&yard.0, std::fs::Permissions::from_mode(0o755))
            .expect("and the yard");
        // The stand-in writes its reading whole and then renames it, so a
        // partial file is never what the poll below reads.
        let stand_in = bin.join("weaver-state");
        std::fs::write(
            &stand_in,
            concat!(
                "#!/bin/sh
",
                "for a; do t=$a; done
",
                "/usr/bin/readlink /proc/self/fd/3 > \"$t/fd3\"\n",
                "/usr/bin/readlink /proc/self/fd/9 > \"$t/fd9\"\n",
                "/bin/cat /proc/self/status > \"$t/status.part\" && ",
                "/bin/mv \"$t/status.part\" \"$t/status\"\n",
            ),
        )
        .expect("the stand-in is written");
        std::fs::set_permissions(&stand_in, std::fs::Permissions::from_mode(0o755))
            .expect("and made executable");

        let source = format!(
            concat!(
                "session = \"s-1\"\n",
                "tool-set = []\n",
                "permission-mode = \"ask\"\n",
                "\n",
                "[spu-instruction.decoder]\n",
                "residual-readout-election = false\n",
                "identity-file = \"system-prompt.md\"\n",
                "tunable-values = {{}}\n",
                "\n",
                "[spu-instruction.decoder.model-binding]\n",
                "artifact = \"qwen3-4b-instruct\"\n",
                "devices = [0]\n",
                "\n",
                "[gate-instruction.access-rule]\n",
                "allowed-uids = [0]\n",
                "allowed-gids = []\n",
                "denied-uids = [1701]\n",
                "\n",
                "[trace-sink]\n",
                "kind = \"file\"\n",
                "path = \"{}/trace.ndjson\"\n",
                "create = true\n",
                "\n",
                "[state-store]\n",
                "engine = \"sqlite\"\n",
            ),
            sink.display()
        );
        let config = weaver_types::parse(&source, |_| Ok::<_, ()>(Some(Vec::new())))
            .expect("the declaration parses");
        let gate_instruction = config
            .gate_instruction
            .clone()
            .expect("a serving declaration carries its instruction");
        let member = inventory::MemberAccount {
            uid: 4242,
            gid: 4243,
        };
        let inventory = inventory::Inventory {
            config,
            identity: "weaver-alpha".into(),
            declaration: String::new(),
            identity_file: String::new(),
            binding: weaver_types::EnterBinding::Serving { gate_instruction },
            lineage: None,
            member_account: Some(member),
        };
        let mut service = unread_config();
        service.worker = bin.join("weaver-worker");
        let run_directory = sink.join("run");
        std::fs::create_dir_all(&run_directory).unwrap();
        let run_lock = start::take_run_lock(&run_directory)
            .unwrap()
            .expect("a free run lock");

        let harness_end = stand_state_member(&service, &inventory, &run_lock);
        assert!(harness_end.is_some(), "the member stands");
        let territory = sink.join("state");
        let status = territory.join("status");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !status.exists() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let read = std::fs::read_to_string(&status).expect("the member recorded itself");
        let line = |key: &str| {
            read.lines()
                .find_map(|l| l.strip_prefix(key))
                .map(|rest| rest.split_whitespace().collect::<Vec<_>>().join(" "))
                .unwrap_or_default()
        };
        assert_eq!(
            line("Uid:"),
            "4242 4242 4242 4242",
            "every uid the member's"
        );
        assert_eq!(line("Gid:"), "4243 4243 4243 4243", "every gid its group's");
        assert_eq!(line("Groups:"), "4243", "its group alone, none of root's");
        let fd3 = std::fs::read_to_string(territory.join("fd3")).expect("the end was read");
        assert!(
            fd3.starts_with("socket:"),
            "the member's end is a socket at the fixed number: {fd3}"
        );
        // **The member holds the run lock's description at 9**, per Spec
        // section 3. Perturbation: drop the placement from the member's
        // spawn and nothing stands at 9.
        let fd9 = std::fs::read_to_string(territory.join("fd9")).expect("the lock was read");
        assert_eq!(
            fd9.trim(),
            run_directory.join("run.lock").display().to_string(),
            "the member holds the run lock at its fixed number"
        );
    }

    /// **The worker's spawn, as root inside a user namespace**, per Spec
    /// sections 6 and 10's third walk: the worker runs as the agent and holds
    /// no group root left it, leads its own session with no signal ignored and
    /// no new privileges, carries the fixed environment and nothing else, and
    /// crosses the exec with exactly its allowlist of descriptors: standard
    /// input at `/dev/null`, output and error at the worker log, the run lock
    /// at 9 and the relay's write end at 8. A stand-in worker, `sh -c`,
    /// records itself and sleeps. Run by the watch below.
    ///
    /// Perturbations: skip the supplementary narrowing and `Groups:` holds
    /// root's; drop the run lock's placement and 9 is absent from the list;
    /// let the caller's environment through and the environment carries more.
    #[test]
    #[ignore = "needs root; run inside a user namespace by the watch below"]
    fn the_worker_spawn_lands_its_identity_and_allowlist_as_root() {
        use std::os::unix::fs::PermissionsExt;
        let base = std::env::temp_dir().join(format!("weaver-admin-worker-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        std::fs::set_permissions(&base, std::fs::Permissions::from_mode(0o755)).unwrap();
        let run_directory = base.join("run");
        std::fs::create_dir_all(&run_directory).unwrap();
        let run_lock = start::take_run_lock(&run_directory).unwrap().unwrap();
        let (relay_read, relay_write) = nix::unistd::pipe2(nix::fcntl::OFlag::O_CLOEXEC).unwrap();
        let log = start::open_log(&base.join("worker.log"), None).unwrap();
        // **A descriptor the caller left inheritable**, as a root shell may:
        // without the seal it would cross the worker's exec.
        // SAFETY: F_DUPFD without the flag on a descriptor this test owns.
        let leaked = unsafe {
            nix::libc::fcntl(
                std::os::fd::AsRawFd::as_raw_fd(&relay_read),
                nix::libc::F_DUPFD,
                50,
            )
        };
        assert!(leaked >= 50);
        let mut child = start::spawn_worker(start::WorkerStart {
            binary: std::path::Path::new("/bin/sleep"),
            arguments: vec!["30".to_string()],
            uid: 4242,
            gid: 4243,
            home: std::path::Path::new("/nonexistent/home/alpha"),
            library_path: Some(std::path::Path::new("/opt/weaver/lib")),
            log: &log,
            run_lock: &run_lock,
            relay_write: Some(std::os::fd::AsRawFd::as_raw_fd(&relay_write)),
        })
        .expect("the worker spawns");
        // The stand-in is `sleep` itself, which changes nothing it inherited,
        // read from outside once its exec has landed.
        let pid = child.id();
        let proc = std::path::PathBuf::from(format!("/proc/{pid}"));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while std::fs::read_to_string(proc.join("comm"))
            .map(|c| c.trim() != "sleep")
            .unwrap_or(true)
            && std::time::Instant::now() < deadline
        {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let status = std::fs::read_to_string(proc.join("status")).unwrap();
        let line = |key: &str| {
            status
                .lines()
                .find_map(|l| l.strip_prefix(key))
                .map(|rest| rest.split_whitespace().collect::<Vec<_>>().join(" "))
                .unwrap_or_default()
        };
        assert_eq!(line("Uid:"), "4242 4242 4242 4242", "every uid the agent's");
        assert_eq!(line("Gid:"), "4243 4243 4243 4243", "every gid its group's");
        assert_eq!(line("Groups:"), "4243", "its group alone, none of root's");
        assert_eq!(line("SigIgn:"), "0000000000000000", "no signal ignored");
        assert_eq!(line("NoNewPrivs:"), "1", "no new privileges");
        assert_eq!(line("Umask:"), "0027", "a fixed file-creation mask");
        assert_eq!(
            std::fs::read_link(proc.join("cwd")).unwrap(),
            std::path::Path::new("/"),
            "a fixed working directory"
        );
        let stat = std::fs::read_to_string(proc.join("stat")).unwrap();
        let fields: Vec<&str> = stat
            .rsplit(')')
            .next()
            .unwrap()
            .split_whitespace()
            .collect();
        assert_eq!(fields[3], pid.to_string(), "it leads its own session");
        // **Read once the stand-in is actually sleeping**: `sleep` opens its
        // locale files for an instant after the exec, so the table is read
        // until it settles, within the bound, and the last reading is judged.
        let table = || {
            let mut fds: Vec<u32> = std::fs::read_dir(proc.join("fd"))
                .unwrap()
                .filter_map(|entry| entry.ok()?.file_name().to_str()?.parse().ok())
                .collect();
            fds.sort_unstable();
            fds
        };
        let settle = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut fds = table();
        while fds != [0, 1, 2, 8, 9] && std::time::Instant::now() < settle {
            std::thread::sleep(std::time::Duration::from_millis(20));
            fds = table();
        }
        assert_eq!(
            fds,
            vec![0, 1, 2, 8, 9],
            "exactly the allowlist crosses the exec"
        );
        assert_eq!(
            std::fs::read_link(proc.join("fd/0")).unwrap(),
            std::path::Path::new("/dev/null")
        );
        let lock_meta = std::fs::metadata(run_directory.join("run.lock")).unwrap();
        let held = std::fs::metadata(proc.join("fd/9")).unwrap();
        {
            use std::os::unix::fs::MetadataExt;
            assert_eq!((held.dev(), held.ino()), (lock_meta.dev(), lock_meta.ino()));
        }
        let environ = std::fs::read(proc.join("environ")).unwrap();
        let mut environment: Vec<String> = environ
            .split(|b| *b == 0)
            .filter(|v| !v.is_empty())
            .map(|v| String::from_utf8_lossy(v).into_owned())
            .collect();
        environment.sort();
        assert_eq!(
            environment,
            vec![
                "HOME=/nonexistent/home/alpha".to_string(),
                "LANG=C.UTF-8".to_string(),
                "LD_LIBRARY_PATH=/opt/weaver/lib".to_string(),
                "PATH=/usr/bin:/bin".to_string(),
            ],
            "the fixed environment and nothing else"
        );
        let _ = child.kill();
        let _ = child.wait();
        drop(relay_read);
        let _ = std::fs::remove_dir_all(&base);
    }

    /// **The relay's spawn, as root inside a user namespace**, per Spec
    /// sections 6 and 10's third walk: the relay runs as its account and its
    /// one group, the trace group, holds no group root left it, leads its own
    /// session with no signal ignored and no new privileges, and crosses the
    /// exec with exactly its allowlist: its standard streams at `/dev/null`,
    /// the listener at 3, the sink at 4, the log at 5, the lifetime pipe at 6
    /// and the run lock at 9. Its environment is the fixed set, so the relay's
    /// test-only wait knob, set in this process, never reaches it. The
    /// stand-in is a script that becomes `sleep`. Run by the watch below.
    /// Perturbations: drop the log's placement and 5 is absent from the list,
    /// and pass the knob through and the environment carries it.
    #[test]
    #[ignore = "needs root; run inside a user namespace by the watch below"]
    fn the_relay_spawn_lands_its_identity_and_allowlist_as_root() {
        use std::os::unix::fs::PermissionsExt;
        let base = std::env::temp_dir().join(format!("weaver-admin-relay-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        std::fs::set_permissions(&base, std::fs::Permissions::from_mode(0o755)).unwrap();
        let stand_in = base.join("weaver-trace-relay");
        std::fs::write(&stand_in, "#!/bin/sh\nexec /bin/sleep 30\n").unwrap();
        std::fs::set_permissions(&stand_in, std::fs::Permissions::from_mode(0o755)).unwrap();
        let run_directory = base.join("run");
        std::fs::create_dir_all(&run_directory).unwrap();
        let run_lock = start::take_run_lock(&run_directory).unwrap().unwrap();
        let listener = start::bind_trace_door(&run_directory, 0).unwrap();
        {
            use std::os::unix::fs::MetadataExt;
            let door = std::fs::symlink_metadata(run_directory.join("trace.sock")).unwrap();
            assert_eq!(door.mode() & 0o777, 0o660, "the door is 0660");
            assert_eq!((door.uid(), door.gid()), (0, 0), "root's, grouped as asked");
        }
        let sink_path = base.join("trace.ndjson");
        std::fs::write(&sink_path, "").unwrap();
        let sink: std::os::fd::OwnedFd = std::fs::File::open(&sink_path).unwrap().into();
        let log = start::open_log(&base.join("admin.log"), None).unwrap();
        let (lifetime_read, lifetime_write) =
            nix::unistd::pipe2(nix::fcntl::OFlag::O_CLOEXEC).unwrap();
        // SAFETY: the watch runs this instrument alone, one thread.
        unsafe { std::env::set_var("WEAVER_TRACE_RELAY_TEST_MS", "1") };
        let mut child = start::spawn_relay(start::RelayStart {
            binary: &stand_in,
            reader_uid: 4246,
            agent: "alpha",
            boundary_digest: "b0b0",
            uid: 4244,
            gid: 4245,
            listener: &listener,
            sink: &sink,
            log: &log,
            lifetime_read: &lifetime_read,
            run_lock: &run_lock,
        })
        .expect("the relay spawns");
        let pid = child.id();
        let proc = std::path::PathBuf::from(format!("/proc/{pid}"));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while std::fs::read_to_string(proc.join("comm"))
            .map(|c| c.trim() != "sleep")
            .unwrap_or(true)
            && std::time::Instant::now() < deadline
        {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let status = std::fs::read_to_string(proc.join("status")).unwrap();
        let line = |key: &str| {
            status
                .lines()
                .find_map(|l| l.strip_prefix(key))
                .map(|rest| rest.split_whitespace().collect::<Vec<_>>().join(" "))
                .unwrap_or_default()
        };
        assert_eq!(
            line("Uid:"),
            "4244 4244 4244 4244",
            "every uid the relay account's"
        );
        assert_eq!(
            line("Gid:"),
            "4245 4245 4245 4245",
            "every gid the trace group's"
        );
        assert_eq!(line("Groups:"), "4245", "the trace group alone");
        assert_eq!(line("SigIgn:"), "0000000000000000");
        assert_eq!(line("NoNewPrivs:"), "1");
        let environ = std::fs::read(proc.join("environ")).unwrap();
        let names: Vec<String> = environ
            .split(|b| *b == 0)
            .filter(|entry| !entry.is_empty())
            .map(|entry| {
                let name = entry.split(|b| *b == b'=').next().unwrap_or_default();
                String::from_utf8_lossy(name).into_owned()
            })
            .collect();
        assert!(
            !names
                .iter()
                .any(|name| name == "WEAVER_TRACE_RELAY_TEST_MS"),
            "the test-only knob never reaches a started relay: {names:?}"
        );
        assert!(names.iter().any(|name| name == "PATH"), "{names:?}");
        let table = || {
            let mut fds: Vec<u32> = std::fs::read_dir(proc.join("fd"))
                .unwrap()
                .filter_map(|entry| entry.ok()?.file_name().to_str()?.parse().ok())
                .collect();
            fds.sort_unstable();
            fds
        };
        let settle = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut fds = table();
        while fds != [0, 1, 2, 3, 4, 5, 6, 9] && std::time::Instant::now() < settle {
            std::thread::sleep(std::time::Duration::from_millis(20));
            fds = table();
        }
        assert_eq!(
            fds,
            vec![0, 1, 2, 3, 4, 5, 6, 9],
            "exactly the relay's allowlist"
        );
        let _ = child.kill();
        let _ = child.wait();
        drop(lifetime_write);
        let _ = std::fs::remove_dir_all(&base);
    }

    /// The watch for the relay spawn, the worker's pattern.
    #[test]
    fn the_relay_spawn_is_watched_inside_a_user_namespace() {
        if nix::unistd::geteuid().is_root() {
            return the_relay_spawn_lands_its_identity_and_allowlist_as_root();
        }
        let exe = std::env::current_exe().expect("the test binary names itself");
        let ran = std::process::Command::new("unshare")
            .args(["--map-auto", "--map-root-user"])
            .arg(&exe)
            .args([
                "--exact",
                "tests::the_relay_spawn_lands_its_identity_and_allowlist_as_root",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .stdin(std::process::Stdio::null())
            .output();
        let output = match ran {
            Ok(output) => output,
            Err(e) => {
                eprintln!("SKIP relay spawn watch: unshare could not run: {e}");
                return;
            }
        };
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.starts_with("unshare:") {
            eprintln!(
                "SKIP relay spawn watch: no user namespace here: {}",
                stderr.trim()
            );
            return;
        }
        assert!(
            output.status.success() && stdout.contains("test result: ok. 1 passed"),
            "the relay spawn failed inside the namespace\nstdout:\n{stdout}\nstderr:\n{stderr}"
        );
    }

    /// The watch for the worker spawn, the member's pattern: re-executes this
    /// test binary inside `unshare --map-auto --map-root-user` and requires
    /// the instrument above to report exactly one test passed. A box where the
    /// namespace cannot be entered prints a SKIP naming why and passes.
    #[test]
    fn the_worker_spawn_is_watched_inside_a_user_namespace() {
        if nix::unistd::geteuid().is_root() {
            return the_worker_spawn_lands_its_identity_and_allowlist_as_root();
        }
        let exe = std::env::current_exe().expect("the test binary names itself");
        let ran = std::process::Command::new("unshare")
            .args(["--map-auto", "--map-root-user"])
            .arg(&exe)
            .args([
                "--exact",
                "tests::the_worker_spawn_lands_its_identity_and_allowlist_as_root",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .stdin(std::process::Stdio::null())
            .output();
        let output = match ran {
            Ok(output) => output,
            Err(e) => {
                diag!("SKIP worker spawn watch: unshare could not run: {e}");
                return;
            }
        };
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.starts_with("unshare:") {
            diag!(
                "SKIP worker spawn watch: no user namespace here: {}",
                stderr.trim()
            );
            return;
        }
        assert!(
            output.status.success() && stdout.contains("test result: ok. 1 passed"),
            "the worker spawn failed inside the namespace\nstdout:\n{stdout}\nstderr:\n{stderr}"
        );
    }

    /// **The watch that runs on an ordinary `cargo test`**, the store
    /// probe's pattern: re-executes this test binary inside `unshare
    /// --map-auto --map-root-user`, where the process is uid 0 over the
    /// invoking user's subordinate ids, and requires the instrument above to
    /// report exactly one test passed, so a filter matching nothing cannot
    /// read as green. A box where the namespace cannot be entered prints a
    /// SKIP naming why and passes, and has no watch.
    ///
    /// Perturbation: as the instrument's, watched through this test.
    #[test]
    fn the_member_spawn_is_watched_inside_a_user_namespace() {
        if nix::unistd::geteuid().is_root() {
            return the_member_spawn_lands_the_members_identity_as_root();
        }
        let exe = std::env::current_exe().expect("the test binary names itself");
        let ran = std::process::Command::new("unshare")
            .args(["--map-auto", "--map-root-user"])
            .arg(&exe)
            .args([
                "--exact",
                "tests::the_member_spawn_lands_the_members_identity_as_root",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .stdin(std::process::Stdio::null())
            .output();
        let output = match ran {
            Ok(output) => output,
            Err(e) => {
                diag!("SKIP member spawn watch: unshare could not run: {e}");
                return;
            }
        };
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.starts_with("unshare:") {
            diag!(
                "SKIP member spawn watch: no user namespace here: {}",
                stderr.trim()
            );
            return;
        }
        assert!(
            output.status.success() && stdout.contains("test result: ok. 1 passed"),
            "the member spawn failed inside the namespace\nstdout:\n{stdout}\nstderr:\n{stderr}"
        );
    }
}
