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
mod save_points;
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

use weaver_types::{AgentName, FieldName, LifecycleAnswer, LifecycleDirective, LifecycleRefusal};

/// One agent's operator-installed configuration, read from that agent's own
/// root, per Spec section 9: the coordination root, the agent's binaries, the
/// optional values, the agent's territory and the operator's uid, and the
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
    /// **The agent's territory**, root-owned, canonical as judged, per
    /// section 9 on the operator's ruling of 2026-10-07 on #1: it holds
    /// `agent.toml`, the prompt draft, `admin.log`, `worker.log`,
    /// `save-points/`, the trace and the member's room, the whole agent in
    /// one directory under one ownership.
    territory: PathBuf,
    /// **The territory as opened at its judgment**, held for the verb's life,
    /// per section 9: the declaration is read through it and never through
    /// the path again (Codex on #94, round 7). `None` only in a test's unread
    /// configuration, where every use refuses `BoundaryUnverified`.
    territory_fd: Option<std::os::fd::OwnedFd>,
    /// **`save-points/` in the territory as opened at the judgment**, through
    /// the territory's descriptor: section 6's publication, the manifest and
    /// the `restore` verb's judgment go through it, so nothing this root
    /// process writes is reached through a path after the judgment.
    save_points: Option<std::os::fd::OwnedFd>,
    /// The operator's uid, the box's own fact about whose data defines the
    /// agent, per section 9: the harness admits the seeding line from it.
    operator: u32,
    /// The access group's gid, the territory's group, which the logs and the
    /// published save points take so the operator and the connector read
    /// them and nothing else does, per sections 6 and 8.
    access_gid: u32,
    /// The boundary file, `roles.toml`, as read: its sha256 hex and its one
    /// trace reader, or why it did not read. **Required at `validate` and
    /// `load` alone**, per section 9: a damaged file never takes `unload`,
    /// `stop` or `show` from a running agent, whose log lines then carry no
    /// digest.
    boundary: Result<BoundaryRead, String>,
    /// The agent's config root as judged, where the clean-unload marker is
    /// written under this crate's own custody, per Spec section 4.
    root: PathBuf,
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
        self.territory.join("admin.log")
    }

    /// The judged territory's descriptor, per section 9.
    fn territory_fd(&self) -> Result<std::os::fd::BorrowedFd<'_>, LifecycleRefusal> {
        use std::os::fd::AsFd;
        self.territory_fd
            .as_ref()
            .map(|fd| fd.as_fd())
            .ok_or_else(|| {
                diag!("weaver-admin: the territory was not opened at the judgment");
                LifecycleRefusal::BoundaryUnverified
            })
    }

    /// The judged territory's `save-points/` descriptor, per section 9.
    fn save_points_fd(&self) -> Result<std::os::fd::BorrowedFd<'_>, LifecycleRefusal> {
        use std::os::fd::AsFd;
        self.save_points
            .as_ref()
            .map(|fd| fd.as_fd())
            .ok_or_else(|| {
                diag!(
                    "weaver-admin: the territory's save-points directory was not opened at the judgment"
                );
                LifecycleRefusal::BoundaryUnverified
            })
    }

    /// The worker's own log, per section 6, never the operations log.
    fn worker_log(&self) -> PathBuf {
        self.territory.join("worker.log")
    }

    /// Who owns the operator's logs: the `operator` uid and the declaration
    /// directory's group, set through the open descriptor, per section 8.
    /// **The owner of what this crate writes in the territory**: this
    /// process's uid, root in production and the suite's own under test, and
    /// the access group, so the logs and the published save points are root's
    /// files in root's directory that the group reads, per sections 6 and 8.
    fn file_owner(&self) -> (u32, u32) {
        (nix::unistd::geteuid().as_raw(), self.access_gid)
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
        surface::Request::SavePoint(agent) => ("save-point", save_point(config, &agent)),
        surface::Request::Restore(agent) => ("restore", restore(config, &agent)),
        surface::Request::ForceUnload(_) => ("force-unload", force_unload(config)),
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
/// The save point's descriptor in the member, per `weaver-state-Spec`
/// section 2: a fixed convention between this crate and the member.
const SAVE_POINT_FD: std::os::fd::RawFd = 4;

fn stand_state_member(
    config: &ServiceConfig,
    inventory: &inventory::Inventory,
    run_lock: &start::RunLock,
    save_point: Option<std::os::fd::OwnedFd>,
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
        .args(member_vector(&territory, &inventory.binding))
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
    // **The save point the load restores rides at descriptor 4**, per Spec
    // section 6 and `weaver-state-Spec` section 2, copied high like the
    // others and placed at the spawn alone; absent, the number holds
    // nothing and the member stands empty.
    let save_point = match save_point {
        Some(fd) => Some(
            start::high({
                use std::os::fd::AsRawFd;
                fd.as_raw_fd()
            })
            .ok()?,
        ),
        None => None,
    };
    let (raw_member_end, raw_lock, raw_save_point) = {
        use std::os::fd::AsRawFd;
        (
            member_end.as_raw_fd(),
            lock.as_raw_fd(),
            save_point.as_ref().map(|fd| fd.as_raw_fd()),
        )
    };
    // SAFETY: every call below is async-signal-safe, run in the child
    // between fork and exec.
    unsafe {
        use std::os::unix::process::CommandExt;
        let access_gid = config.access_gid;
        member.pre_exec(move || {
            // **The member takes its own session and resets the invocation's
            // ignored signals**, per Spec section 6, and holds the run lock's
            // description for its life, per section 3.
            start::place(raw_lock, start::RUN_LOCK_FD)?;
            // The member's allowlist: its first door's end at 3, armed below,
            // the save point at 4 only where one is handed, so a descriptor
            // the invoking shell left at 4 never crosses as a save point,
            // and the run lock at 9.
            if let Some(raw) = raw_save_point {
                start::place(raw, SAVE_POINT_FD)?;
                start::seal_except(&[3, SAVE_POINT_FD, start::RUN_LOCK_FD])?;
            } else {
                start::seal_except(&[3, start::RUN_LOCK_FD])?;
            }
            start::detach_and_reset()?;
            become_member(member_account, access_gid)?;
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
fn become_member(member: inventory::MemberAccount, access_gid: u32) -> std::io::Result<()> {
    // The drop lives in `inventory::drop_to` since issue #675, so the order
    // described above is implemented once.
    inventory::drop_to(member.uid, &member_groups(member, access_gid))
}

/// **The member's group set: its own group first, the access group beside
/// it**, per Spec section 6 on the operator's ruling of 2026-10-07 on #1
/// (Codex on #94, round 8): the territory is root's and grouped to the access
/// group, and `drop_to` sets the supplementary set from this slice alone,
/// never from the account database, so the passage through the territory to
/// the member's own room is granted here from the territory's group as
/// judged, whatever the account's memberships say. Nothing else: the trace's
/// group is not among them, so the member cannot read the record.
fn member_groups(member: inventory::MemberAccount, access_gid: u32) -> [nix::libc::gid_t; 2] {
    [
        member.gid as nix::libc::gid_t,
        access_gid as nix::libc::gid_t,
    ]
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
) -> Vec<std::ffi::OsString> {
    // **The territory leads and no flag rides**, per Spec section 6: the one
    // engine is the embedded one, so the engine flag left the vector with
    // the service engine on the operator's ruling of 2026-10-02 on #1.
    let mut vector: Vec<std::ffi::OsString> = vec![territory.as_os_str().to_owned()];
    // The door's name rides the vector under a diagnostic binding alone,
    // per Spec section 6: a serving load restores through descriptor 4
    // since A3.2 and binds no door, the record restore of issue #432 having
    // retired; the member binds the name only where this value is there,
    // and this crate names the door and dials it never.
    if matches!(binding, weaver_types::EnterBinding::Diagnostic) {
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

/// **The declaration is read through the territory's descriptor**, per Spec
/// section 9 (closing #95): `agent.toml` opened beneath the descriptor the
/// judgment holds, without following a link, judged on its own descriptor a
/// regular file of this process's uid, root in production, writable by no
/// group or other, and read; absent is `NoSuchAgent`, anything else that
/// fails is the provisioning, refusing `BoundaryUnverified`.
fn read_declaration(config: &ServiceConfig) -> Result<String, LifecycleRefusal> {
    use std::io::Read;
    let mut file = open_declaration(config.territory_fd()?, &config.territory)?;
    let mut source = String::new();
    file.read_to_string(&mut source).map_err(|_| {
        diag!(
            "weaver-admin: the territory's agent.toml does not read as text, in {}",
            config.territory.display()
        );
        LifecycleRefusal::BoundaryUnverified
    })?;
    Ok(source)
}

/// Open `agent.toml` beneath the territory's descriptor and judge it, as
/// `read_declaration` says; `directory` names the territory in the refusal.
fn open_declaration(
    territory: std::os::fd::BorrowedFd<'_>,
    directory: &std::path::Path,
) -> Result<std::fs::File, LifecycleRefusal> {
    use std::os::unix::fs::MetadataExt;
    let refuse = |what: &str| {
        diag!(
            "weaver-admin: the territory's agent.toml {what}, in {}",
            directory.display()
        );
        LifecycleRefusal::BoundaryUnverified
    };
    let fd = match nix::fcntl::openat(
        territory,
        "agent.toml",
        nix::fcntl::OFlag::O_RDONLY | nix::fcntl::OFlag::O_NOFOLLOW | nix::fcntl::OFlag::O_CLOEXEC,
        nix::sys::stat::Mode::empty(),
    ) {
        Ok(fd) => fd,
        Err(nix::errno::Errno::ENOENT) => return Err(LifecycleRefusal::NoSuchAgent),
        Err(nix::errno::Errno::ELOOP) => return Err(refuse("is a link")),
        Err(_) => return Err(refuse("does not open")),
    };
    let file = std::fs::File::from(fd);
    let metadata = file.metadata().map_err(|_| refuse("does not stat"))?;
    if !metadata.is_file() {
        return Err(refuse("is not a regular file"));
    }
    if metadata.uid() != nix::unistd::geteuid().as_raw() {
        return Err(refuse("is not root's"));
    }
    if metadata.mode() & 0o022 != 0 {
        return Err(refuse("is writable by group or other"));
    }
    Ok(file)
}

fn take_inventory(
    config: &ServiceConfig,
    agent: &AgentName,
) -> Result<inventory::Inventory, LifecycleRefusal> {
    admissible(config, agent)?;
    judge_reader(&config.require_boundary()?.reader, agent)?;
    let source = read_declaration(config)?;
    let inventory = take_inventory_from(config, agent, &source)?;
    // **The sink's directory is the territory** (Codex on #94, round 10),
    // per Spec section 9: the trace and the member's room are derived from
    // the declaration's sink, and the territory is the agent whole, so a
    // declaration naming a sink elsewhere would stand the trace and the room
    // outside what the territory's custody, group and archive cover.
    sink_within_territory(&inventory.config.trace_sink, &config.territory)?;
    Ok(inventory)
}

/// **A sink outside the territory refuses `ConfigInvalid` naming
/// `trace-sink`**, per Spec section 9: the sink's directory must be the judged
/// territory itself.
fn sink_within_territory(
    sink: &weaver_types::TraceSink,
    territory: &std::path::Path,
) -> Result<(), LifecycleRefusal> {
    let directory = inventory::sink_directory(sink);
    if directory == territory {
        return Ok(());
    }
    diag!(
        "weaver-admin: config invalid: the trace sink's directory {} is not the territory {}",
        directory.display(),
        territory.display()
    );
    Err(LifecycleRefusal::ConfigInvalid {
        field: Some(FieldName("trace-sink".into())),
    })
}

/// The inventory from the declaration's text, as before the sink's place is
/// judged against the territory.
fn take_inventory_from(
    config: &ServiceConfig,
    agent: &AgentName,
    source: &str,
) -> Result<inventory::Inventory, LifecycleRefusal> {
    let identity = inventory::identity_for(agent);
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
    inventory::take_inventory(agent, source, &boundary)
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
    /// The marker as it stood before this load wrote it open, once it did:
    /// restored by the rollback, per Spec section 4.
    marker_before: Option<Option<save_points::Marker>>,
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
    let mut inventory = take_inventory(config, agent)?;
    // **The publication opens the validate step, under the run lock**, per
    // Spec sections 3 and 6: what an unclean stop left in the room is
    // published now and selectable below.
    // Refused here only where the manifest or the directory refuses, which
    // the selection below would refuse too; a room with nothing to publish
    // is no refusal.
    publish_from_room(config, agent, &[])?;
    // **The selection**, per Spec section 4: the save point `restore` names
    // or the latest the manifest names, judged through the descriptor the
    // member will inherit; no member elected selects nothing, and a restore
    // named beside the `none` engine refuses.
    let selected = select_save_point(config, &inventory)?;
    if let Some(selected) = selected.as_ref() {
        record(
            config,
            "load",
            &format!(
                "restoring {} ordinal {} {}",
                selected.line.name,
                selected.line.ordinal,
                line_arrival(&selected.line)
            ),
        );
    }
    inventory.lineage = selected.as_ref().map(|selected| selected.lineage.clone());
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
    let worker_log = start::open_log(&config.worker_log(), Some(config.file_owner()))
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
    let member_elected = inventory.member_account.is_some();
    let state_end = stand_state_member(
        config,
        &inventory,
        &run_lock,
        selected.map(|selected| selected.descriptor),
    );
    standing.forked |= state_end.is_some();
    // **An elected member that does not stand refuses the load**: the
    // harness would otherwise enter with no state end, skip the `restored`
    // agreement no member can answer, and author a `load` naming a lineage
    // nobody restored, or run an agent the declaration gave a store without
    // one. `BindFailed` names it, and the rollback ends what forked.
    if member_elected && state_end.is_none() {
        diag!(
            "weaver-admin: the declaration elects a state member and none stood, so the load does not go on without it"
        );
        return Err(LifecycleRefusal::BindFailed);
    }
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
                run: run_reference.clone(),
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
                restore: inventory.lineage.clone(),
                // **The reset is the marker's**, per Spec section 4: the
                // prior run still open, or forced closed without its save
                // point, rides the enter beside the lineage.
                reset: save_points::reset_from(save_points::read_marker(&config.root).as_ref()),
                stack,
                // The boundary file's digest, the cause and the judged
                // libraries, per `weaver-types-Spec` section 4 as of
                // 2026-10-03, which the harness records on the load event.
                boundary: config.require_boundary()?.digest.clone(),
                cause: invocation_cause(),
                // The operator's uid, the root's key, so the harness admits
                // the seeding line from the operator alone, per
                // `weaver-types-Spec` section 4 on the ruling of 2026-10-06.
                operator: config.operator,
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
            weaver_types::Payload::Answer(LifecycleAnswer::Ready) => {
                // **The marker is written only once the run stands**, per
                // Spec section 4 on A3.0 item 5: after the enter answers,
                // its prior state kept for the rollback of any later step,
                // so a load that never authored `load` never opened a run.
                open_marker(&config.root, standing, &run_reference.0)
            }
            weaver_types::Payload::Refusal(refusal) => Err(refusal),
            _ => Err(LifecycleRefusal::Malformed),
        },
        Err(_) => Err(LifecycleRefusal::NoResidency),
    }
}

/// **Write the run's open marker once the run stands**, per Spec section 4
/// on A3.0 item 5. **What stood before is recorded ahead of the write**
/// (Codex on #94, round 4): a replacement that lands and whose root does
/// not sync fails the write with the open marker in place, so the rollback
/// must know what to put back whether the write failed before or after the
/// rename. **A marker that cannot be written fails the load**: a run
/// standing with no open marker would stop unclean without its reset at the
/// next load, so the enter is rolled back instead, nothing standing that
/// the record would misname.
fn open_marker(
    root: &std::path::Path,
    standing: &mut Standing,
    run: &str,
) -> Result<(), LifecycleRefusal> {
    standing.marker_before = Some(save_points::read_marker(root));
    let marker = save_points::Marker::Open {
        run: run.to_string(),
    };
    if let Err(e) = save_points::write_marker(root, Some(&marker)) {
        diag!(
            "weaver-admin: the clean-unload marker in {} does not write: {e}",
            root.display()
        );
        return Err(LifecycleRefusal::BoundaryUnverified);
    }
    Ok(())
}

/// **Select the save point this load restores**, per Spec section 4: nothing
/// where no member stands, a `restore` named beside the `none` engine
/// refusing `ConfigInvalid` naming `restore`; otherwise the manifest's
/// answer, the named one or the latest.
fn select_save_point(
    config: &ServiceConfig,
    inventory: &inventory::Inventory,
) -> Result<Option<save_points::Selected>, LifecycleRefusal> {
    let restore = inventory
        .config
        .restore
        .as_ref()
        .map(|restore| restore.save_point.as_str());
    if inventory.member_account.is_none() {
        if restore.is_some() {
            diag!(
                "weaver-admin: config invalid: restore names a save point and the store engine is none, so no member would restore it"
            );
            return Err(LifecycleRefusal::ConfigInvalid {
                field: Some(FieldName("restore".into())),
            });
        }
        return Ok(None);
    }
    save_points::select(
        config.save_points_fd()?,
        config.file_owner().0,
        restore,
        save_points::ROOT,
    )
}

/// **A save point on demand**, the `save-point` verb, per Spec sections 2
/// and 6: valid while the run stands, one directive and one answer on the
/// coordination channel, the finished save point published at once with the
/// event's position the harness reported.
fn save_point(
    config: &ServiceConfig,
    agent: &AgentName,
) -> Result<LifecycleAnswer, LifecycleRefusal> {
    let run_directory = config.run_directory();
    let _invocation = start::take_invocation_lock(&run_directory)?;
    if !start::run_lock_held(&run_directory)? {
        diag!("weaver-admin: no run stands, and a save point is taken of a running agent alone");
        return Err(LifecycleRefusal::OutOfOrder);
    }
    let mut coordination =
        channel::dial(&config.coordination_socket()).map_err(|_| LifecycleRefusal::Unanswered)?;
    let ordinal = coordination.next_ordinal();
    coordination
        .send_directive(
            ordinal,
            LifecycleDirective::SavePoint {
                cause: invocation_cause(),
            },
        )
        .map_err(|_| LifecycleRefusal::Unanswered)?;
    let report = match coordination.recv_within(LEAVE_BOUND) {
        Ok(answer) => match answer.payload {
            weaver_types::Payload::Answer(LifecycleAnswer::SavePointTaken { report }) => report,
            weaver_types::Payload::Refusal(refusal) => return Err(refusal),
            _ => return Err(LifecycleRefusal::Malformed),
        },
        Err(_) => return Err(LifecycleRefusal::Unanswered),
    };
    // **The verb answers only a published save point**: the harness's
    // report is answered once its file stands in the territory's save-points
    // under a manifest line, and a publication that refuses, or does not
    // reach this save point, refuses the verb, the file standing in the
    // room for the next verb and said so in the log.
    let lines = publish_from_room(
        config,
        agent,
        &[(report.clone(), save_points::Arrival::Demand)],
    )?;
    if !lines.iter().any(|line| line.digest == report.save_point) {
        diag!(
            "weaver-admin: the save point {} was taken and not published; it stands in the member's room for the next verb",
            report.save_point
        );
        return Err(LifecycleRefusal::BoundaryUnverified);
    }
    Ok(LifecycleAnswer::SavePointTaken { report })
}

/// **Name the save point the next load restores**, the `restore` verb, per
/// Spec sections 2 and 4: the one the declaration's `[restore]` names, never
/// the caller's, judged as a load judges one and entered in the manifest as
/// named at a restore, so a file that arrived by no publication becomes
/// loadable by this verb alone. The live restore of a running agent waits on
/// the loop act's `Reopen`; until then a restore is this verb and then a
/// load.
fn restore(config: &ServiceConfig, agent: &AgentName) -> Result<LifecycleAnswer, LifecycleRefusal> {
    let _invocation = start::take_invocation_lock(&config.run_directory())?;
    let inventory = take_inventory(config, agent)?;
    let Some(named) = inventory.config.restore.as_ref() else {
        diag!(
            "weaver-admin: config invalid: the declaration names no restore, and this verb names what the declaration names"
        );
        return Err(LifecycleRefusal::ConfigInvalid {
            field: Some(FieldName("restore".into())),
        });
    };
    if inventory.member_account.is_none() {
        diag!(
            "weaver-admin: config invalid: restore names a save point and the store engine is none"
        );
        return Err(LifecycleRefusal::ConfigInvalid {
            field: Some(FieldName("restore".into())),
        });
    }
    let line = save_points::name_at_restore(
        config.save_points_fd()?,
        config.file_owner().0,
        &named.save_point,
        save_points::ROOT,
    )?;
    Ok(LifecycleAnswer::RestoreNamed {
        save_point: line.digest,
        name: line.name,
    })
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
    let log = log::open_append(&config.admin_log(), Some(config.file_owner()))
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
    if let Some(prior) = standing.marker_before.take() {
        let restored = save_points::write_marker(&config.root, prior.as_ref()).is_ok();
        account.push(format!(
            "marker {}",
            if restored { "restored" } else { "not restored" }
        ));
    }
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
    unload_within(config, UNLOAD_BOUNDS, false)
}

/// **The forced unload**, per Spec section 3 on the operator's ruling of
/// 2026-10-06 on #1 (A3.0 item 6): `unload` in every respect but one, the
/// leave directed with `forced`, so the harness leaves without its save
/// point and records that it was not taken, and the marker stays open under
/// `ForcedUnload` for the next load's reset. The loss is the operator's
/// recorded choice.
fn force_unload(config: &ServiceConfig) -> Result<LifecycleAnswer, LifecycleRefusal> {
    unload_within(config, UNLOAD_BOUNDS, true)
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
    forced: bool,
) -> Result<LifecycleAnswer, LifecycleRefusal> {
    unload_with(config, bounds, forced, &mut |reports| {
        publish_from_room(config, &AgentName(config.agent.clone()), reports)
    })
}

/// `unload` and `force-unload` with the publication the conclusion runs:
/// `publish_from_room` on a box, a stand-in in the tests that count it.
fn unload_with(
    config: &ServiceConfig,
    bounds: UnloadBounds,
    forced: bool,
    publish: &mut Publication<'_>,
) -> Result<LifecycleAnswer, LifecycleRefusal> {
    let run_directory = config.run_directory();
    let unloaded = Ok(LifecycleAnswer::State {
        state: weaver_types::AgentState::Unloaded,
        load: None,
        constituents: Vec::new(),
    });
    // **The invocation lock is held from the first step to the conclusion**,
    // per Spec section 3 on the operator's ruling of 2026-10-07 on #1, a
    // graceful unload's drain included, so no other verb runs between its
    // leave and its publication. **A `force-unload` that finds the lock held
    // goes to the harness without it** (`force_beside_holder`).
    let _invocation = match start::take_invocation_lock(&run_directory) {
        Ok(lock) => lock,
        Err(LifecycleRefusal::InvocationInFlight) if forced => {
            match force_beside_holder(config, bounds, &run_directory)? {
                ForceBeside::Joined | ForceBeside::Escalated => return unloaded,
                ForceBeside::Holds(lock) => lock,
            }
        }
        Err(refusal) => return Err(refusal),
    };
    // **The leave's budget runs from here**, per Spec section 3: the
    // observation and both dials spend it, so the verb holds the invocation
    // lock at most the leave's sixty seconds and the escalation's forty-five.
    let leave_deadline = std::time::Instant::now() + bounds.leave;
    if !start::run_lock_held(&run_directory)? {
        // **A forced verb that finds the run already ended closes the
        // marker as forced where it stands open** (Codex on #94, round 6),
        // per Spec section 3: a forced unload refused after the harness had
        // left, retried, still records the operator's choice rather than
        // `NoCleanUnload`. An unforced verb closes nothing here: a run that
        // ended on its own is the unclean stop the next load records.
        if forced {
            close_marker(config, true)?;
        }
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
        // **A graceful unload drains with no time bound of its own**, per
        // Spec section 3 on the operator's rulings of 2026-10-07 on #1: the
        // leave lets the turn in flight finish, so its answer may take the
        // length of a turn, and a `force-unload` reaches the harness without
        // the lock this verb holds. A harness that goes away answers nothing
        // and the escalation below follows at once. A forced unload keeps the
        // leave's bound.
        let bound = if forced { Some(leave_deadline) } else { None };
        let answered = direct_leave_within(
            config,
            bound,
            LifecycleDirective::Leave {
                cause: invocation_cause(),
                forced,
            },
        );
        match answered {
            Ok(report) => {
                if start::wait_free(&run_directory, bounds.after_left) {
                    return conclude_left(config, report, forced, save_points::ROOT, publish);
                }
            }
            // A refusal on leave returns to the operator unchanged and
            // answers nothing further: `ActivityNotAtRest` above all, and
            // since A3.2 `SavePointNotTaken`, on which the unload does not
            // complete, the run staying open with its lock, per Spec section
            // 3 on A3.0 item 6, so the operator retries or forces.
            Err(LeaveFault::Refused(refusal)) => {
                // **A refusal after the run ended still closes the forced
                // marker** (Codex on #94, round 6): a refusal the harness
                // sends past `Left`, its organs going down behind it, ends
                // the run all the same, so where the lock frees inside the
                // after-left wait the marker closes as forced before the
                // refusal returns.
                if forced && start::wait_free(&run_directory, bounds.after_left) {
                    close_marker(config, true)?;
                }
                return Err(refusal);
            }
            // The leave went unanswered inside its bound: a worker that
            // would not exit, so the escalation follows.
            Err(LeaveFault::Unanswered | LeaveFault::Gone) => {}
        }
    }
    // **The escalation is the last resort**: the harness hears a
    // `force-unload` even through a stalled drain and takes the save point,
    // so reaching here means the harness itself did not answer, and the run
    // is ended with no save point taken, which the log records.
    if entered {
        record(
            config,
            "unload",
            "escalated: the harness did not answer the leave, so no save point was taken",
        );
    }
    start::escalate_within(&run_directory, bounds.term, bounds.kill)?;
    // A run that had to be ended by force took no leave save point: the
    // room's finished files are recovered at the next load. The marker
    // stays open, which the next load reads as `NoCleanUnload`, unless the
    // operator forced this unload, in which case it closes as forced so the
    // record carries the operator's choice, as it does on a forced leave.
    if forced {
        close_marker(config, true)?;
    }
    unloaded
}

/// The publication a conclusion runs: `publish_from_room` on a box, a
/// stand-in in the tests that count publications.
type Publication<'a> = dyn FnMut(
        &[(weaver_types::SavePointReport, save_points::Arrival)],
    ) -> Result<Vec<save_points::ManifestLine>, LifecycleRefusal>
    + 'a;

/// **The leave's conclusion**, per Spec section 3, run with the invocation
/// lock held, as it is from the verb's first step, and the member stopped:
/// the leave's save point carries its event's position, anything else the
/// room still held is recovered, and the marker closes on a clean unload or
/// stays open under `ForcedUnload` where a forced one kept no state. **A
/// reported save point already in the manifest is not published again**, a
/// defence for an unload retried after its publication landed: its line
/// stands for it and the marker closes as it would have.
fn conclude_left(
    config: &ServiceConfig,
    report: Option<weaver_types::SavePointReport>,
    forced: bool,
    owner: save_points::Owner,
    publish: &mut Publication<'_>,
) -> Result<LifecycleAnswer, LifecycleRefusal> {
    let unloaded = Ok(LifecycleAnswer::State {
        state: weaver_types::AgentState::Unloaded,
        load: None,
        constituents: Vec::new(),
    });
    let standing = report
        .as_ref()
        .and_then(|report| standing_line(config, owner, &report.save_point));
    let reports: Vec<_> = report
        .into_iter()
        .map(|report| (report, save_points::Arrival::Leave))
        .collect();
    // **The unload does not complete without its save point published**
    // (Codex on #94, round 9), per Spec section 3 on A3.0 item 6: the leave's
    // reported digest must be among the lines this publication appended, or
    // the marker stays open, so the next load records `NoCleanUnload` and
    // recovers the room's file or names it as unpublishable, and the verb
    // refuses naming the publication rather than answering Unloaded over a
    // stale restore.
    let published = match standing {
        Some(line) => {
            record(
                config,
                "publish",
                &format!(
                    "{} stands in the manifest already; not published again",
                    line.name
                ),
            );
            Ok(vec![line])
        }
        None => publish(&reports),
    };
    // **A forced unload whose save point published closes the marker
    // clean**, on the operator's clarification of 2026-10-07 on #1, no state
    // having been lost; one with no save point, or one that did not publish,
    // comes down all the same with the marker open under `ForcedUnload`, the
    // next load recording the reset.
    if forced {
        close_marker(config, !forced_kept_state(&reports, &published))?;
        return unloaded;
    }
    if let Some(digest) = unpublished_leave(&reports, &published) {
        diag!(
            "weaver-admin: the leave's save point {digest} did not publish; the marker stays open and the unload does not complete"
        );
        record(
            config,
            "unload",
            "refused: the leave's save point did not publish",
        );
        return Err(LifecycleRefusal::SavePointNotTaken {
            missed: weaver_types::SavePointLeg::Published,
        });
    }
    close_marker(config, false)?;
    unloaded
}

/// The manifest's line for `digest`, where one stands and the manifest
/// reads; none otherwise, and the publication runs.
fn standing_line(
    config: &ServiceConfig,
    owner: save_points::Owner,
    digest: &str,
) -> Option<save_points::ManifestLine> {
    let directory = config.save_points_fd().ok()?;
    save_points::read_manifest(directory, owner)
        .ok()?
        .into_iter()
        .find(|line| line.digest == digest)
}

/// **Whether a forced unload kept state**, per Spec section 3 on the
/// operator's clarification of 2026-10-07 on #1: its leave save point was
/// reported and published, so the marker closes clean; otherwise it stays
/// open under `ForcedUnload`.
fn forced_kept_state(
    reports: &[(weaver_types::SavePointReport, save_points::Arrival)],
    published: &Result<Vec<save_points::ManifestLine>, LifecycleRefusal>,
) -> bool {
    !reports.is_empty() && unpublished_leave(reports, published).is_none()
}

/// **The leave's save point that did not publish**, per Spec section 3: the
/// digest of a reported leave save point that is not among the lines the
/// publication answered, or none where no save point was reported (a forced
/// leave, or no member standing) or every reported one published.
fn unpublished_leave(
    reports: &[(weaver_types::SavePointReport, save_points::Arrival)],
    published: &Result<Vec<save_points::ManifestLine>, LifecycleRefusal>,
) -> Option<String> {
    reports
        .iter()
        .map(|(report, _)| &report.save_point)
        .find(|digest| {
            !matches!(published, Ok(lines) if lines.iter().any(|line| &line.digest == *digest))
        })
        .cloned()
}

/// **Publish the member's finished save points into the operator's
/// directory**, per Spec section 6, from the room the declaration's
/// territory holds: the member's uid and the room are the inventory's, and
/// an inventory that does not read leaves the files for the next load,
/// said in the log. The lines appended are logged.
fn publish_from_room(
    config: &ServiceConfig,
    agent: &AgentName,
    reports: &[(weaver_types::SavePointReport, save_points::Arrival)],
) -> Result<Vec<save_points::ManifestLine>, LifecycleRefusal> {
    let inventory = match take_inventory(config, agent) {
        Ok(inventory) => inventory,
        Err(refusal) => {
            record(
                config,
                "publish",
                &format!(
                    "skipped: the inventory refuses {}; the room's save points are published at the next load",
                    surface::render_refusal(&refusal)
                ),
            );
            return Err(refusal);
        }
    };
    let Some(member) = inventory.member_account else {
        return Ok(Vec::new());
    };
    let room = save_points::room_of(inventory::sink_directory(&inventory.config.trace_sink));
    let directory = match config.save_points_fd() {
        Ok(directory) => directory,
        Err(refusal) => {
            record(
                config,
                "publish",
                &format!("refused: {}", surface::render_refusal(&refusal)),
            );
            return Err(refusal);
        }
    };
    match save_points::publish(
        &room,
        member.uid,
        directory,
        config.file_owner(),
        save_points::ROOT,
        reports,
    ) {
        Ok(lines) => {
            for line in &lines {
                record(
                    config,
                    "publish",
                    &format!(
                        "{} ordinal {} {}",
                        line.name,
                        line.ordinal,
                        line_arrival(line)
                    ),
                );
            }
            Ok(lines)
        }
        Err(refusal) => {
            record(
                config,
                "publish",
                &format!("refused: {}", surface::render_refusal(&refusal)),
            );
            Err(refusal)
        }
    }
}

fn line_arrival(line: &save_points::ManifestLine) -> &'static str {
    match line.arrived {
        save_points::Arrival::Leave => "at the leave",
        save_points::Arrival::Demand => "on demand",
        save_points::Arrival::Recovered => "recovered from the room",
        save_points::Arrival::Restore => "named at a restore",
    }
}

/// Close the marker on a clean unload, or leave it open under `ForcedUnload`
/// where the leave was forced, per Spec section 4.
fn close_marker(config: &ServiceConfig, forced: bool) -> Result<(), LifecycleRefusal> {
    let run = match save_points::read_marker(&config.root) {
        Some(save_points::Marker::Open { run }) | Some(save_points::Marker::Forced { run }) => run,
        Some(save_points::Marker::Closed { .. }) | None => return Ok(()),
    };
    let marker = if forced {
        save_points::Marker::Forced { run }
    } else {
        save_points::Marker::Closed { run }
    };
    // **A marker that does not write refuses the verb**, at this end as at
    // the load's: the run is gone either way, and the operator reads that
    // the next load will record a reset the marker could not say, rather
    // than an unload answered as recorded.
    save_points::write_marker(&config.root, Some(&marker)).map_err(|e| {
        diag!(
            "weaver-admin: the clean-unload marker in {} does not write at the unload: {e}",
            config.root.display()
        );
        record(config, "marker", &format!("not written: {e}"));
        LifecycleRefusal::BoundaryUnverified
    })
}

/// Why a directed leave did not answer `Left`.
enum LeaveFault {
    Refused(LifecycleRefusal),
    /// The harness took the directive and answered nothing inside the bound:
    /// a live worker that does not answer.
    Unanswered,
    /// No harness to answer: the dial found none, or the connection ended.
    Gone,
}

/// How often a `force-unload` waiting on a held lock asks the harness to
/// join again, per Spec section 3.
const JOIN_RETRY: std::time::Duration = std::time::Duration::from_secs(1);

/// What a `force-unload` that met a held lock came to.
enum ForceBeside {
    /// It joined a graceful unload's pending leave and was answered `Left`.
    Joined,
    /// The harness answered no join inside the leave's bound, and the force
    /// ended the run itself.
    Escalated,
    /// The holder let the lock go; the force holds it and unloads alone.
    Holds(start::InvocationLock),
}

/// **A `force-unload` beside the lock's holder**, per Spec section 3 on the
/// operator's rulings of 2026-10-07 on #1. It sends `JoinLeave`: answered
/// `Left`, it joined a graceful unload's leave, and the holder publishes and
/// closes the marker. **Unanswered inside the leave's bound**, the harness is
/// alive and silent, and the force does not wait on it: it escalates against
/// the run's processes without the lock, so the holder's wait reads the end
/// of its connection, and then closes the marker as forced, since the
/// operator forced a run whose save point was not taken; the run is ended
/// first, so a marker that will not write never leaves it standing. Refused, `OutOfOrder` where
/// no leave is pending, or met by no harness, it tries the lock and, the lock
/// still held, asks again a second later, so a force that came before the
/// holder's leave was pending joins as soon as it is; once the holder lets
/// the lock go, the force holds it and unloads alone.
fn force_beside_holder(
    config: &ServiceConfig,
    bounds: UnloadBounds,
    run_directory: &std::path::Path,
) -> Result<ForceBeside, LifecycleRefusal> {
    loop {
        match direct_leave_within(
            config,
            Some(std::time::Instant::now() + bounds.leave),
            LifecycleDirective::JoinLeave {
                cause: invocation_cause(),
            },
        ) {
            Ok(_) => {
                record(
                    config,
                    "unload",
                    "forced: joined the unload in progress, whose invocation publishes and closes the marker",
                );
                return Ok(ForceBeside::Joined);
            }
            Err(LeaveFault::Unanswered) => {
                record(
                    config,
                    "unload",
                    "forced: the harness answered no join inside the leave's bound; the run is ended without the lock, no save point taken",
                );
                // **The run is ended before the marker is written** (Codex on
                // #94, round 18): a marker that will not write never leaves a
                // silent harness holding the agent. The escalation's refusal
                // outranks the marker's, which is answered after it.
                let escalated = start::escalate_within(run_directory, bounds.term, bounds.kill);
                let marked = close_marker(config, true);
                escalated?;
                marked?;
                return Ok(ForceBeside::Escalated);
            }
            Err(LeaveFault::Refused(_) | LeaveFault::Gone) => {}
        }
        match start::take_invocation_lock(run_directory) {
            Ok(lock) => {
                record(
                    config,
                    "unload",
                    "forced: the lock's holder let it go; unloading alone",
                );
                return Ok(ForceBeside::Holds(lock));
            }
            Err(LifecycleRefusal::InvocationInFlight) => std::thread::sleep(JOIN_RETRY),
            Err(refusal) => return Err(refusal),
        }
    }
}

/// **Directs leave under the leave's own bound**, per Spec section 3.
fn direct_leave(
    config: &ServiceConfig,
) -> Result<Option<weaver_types::SavePointReport>, LeaveFault> {
    direct_leave_within(
        config,
        Some(std::time::Instant::now() + LEAVE_BOUND),
        LifecycleDirective::Leave {
            cause: invocation_cause(),
            forced: false,
        },
    )
}

/// Directs a leave, or a lock-free force's join, and waits for its answer
/// until `deadline`, the dial spending nothing of the bound. **The answer
/// names the leave's save point**, per `weaver-admin-harness-contract`
/// section 3 as of A3.2, none where none was taken or the binding is
/// diagnostic, so the publication that follows carries the event's position.
fn direct_leave_within(
    config: &ServiceConfig,
    deadline: Option<std::time::Instant>,
    directive: LifecycleDirective,
) -> Result<Option<weaver_types::SavePointReport>, LeaveFault> {
    let Ok(mut coordination) = channel::dial(&config.coordination_socket()) else {
        return Err(LeaveFault::Gone);
    };
    let ordinal = coordination.next_ordinal();
    coordination
        .send_directive(ordinal, directive)
        .map_err(|_| LeaveFault::Gone)?;
    let bound = deadline.map_or(std::time::Duration::MAX, |deadline| {
        deadline.saturating_duration_since(std::time::Instant::now())
    });
    match coordination.recv_within(bound) {
        Ok(answer) => match answer.payload {
            weaver_types::Payload::Answer(LifecycleAnswer::Left { save_point }) => Ok(save_point),
            weaver_types::Payload::Refusal(refusal) => Err(LeaveFault::Refused(refusal)),
            _ => Err(LeaveFault::Refused(LifecycleRefusal::Malformed)),
        },
        Err(channel::ChannelFault::Unanswered) => Err(LeaveFault::Unanswered),
        Err(_) => Err(LeaveFault::Gone),
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
        log::OperationsLog::open(&config.admin_log(), Some(config.file_owner()))
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
/// values read, then the agent's territory judged against the
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
    let judged = judge_territory(&config.territory)?;
    config.territory = judged.canonical;
    config.access_gid = judged.access_gid;
    config.territory_fd = Some(judged.territory);
    config.save_points = Some(judged.save_points);
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

/// What the territory's judgment answers: the canonical path, the two
/// descriptors held for the verb's life and the access group's gid.
struct JudgedTerritory {
    canonical: std::path::PathBuf,
    territory: std::os::fd::OwnedFd,
    save_points: std::os::fd::OwnedFd,
    access_gid: u32,
}

/// **The territory is judged before any value in it is read, on its
/// descriptor**, per Spec section 9 on the operator's ruling of 2026-10-07 on
/// #1: opened once with no link followed, a directory owned by this process's
/// uid, root in production and the suite's own under test, mode `0710`
/// exactly, grouped to the access group, which passes by name and never
/// lists, nothing for other, so neither of the agent's uids enters; carrying
/// no access-control entry beyond its mode; every directory above it held
/// closed by root as the root's ancestors are. `save-points/` beneath it is
/// opened through that descriptor and judged the same way at mode `0750` and
/// the territory's group. The ancestors' walk and the access-control look
/// stay by path, being about the path; everything read after is through the
/// descriptors.
fn judge_territory(directory: &std::path::Path) -> Result<JudgedTerritory, LifecycleRefusal> {
    use std::os::fd::AsFd;
    use std::os::unix::fs::MetadataExt;
    let refuse = |what: &str| {
        diag!("weaver-admin: the territory {} {what}", directory.display());
        LifecycleRefusal::BoundaryUnverified
    };
    let opened = save_points::open_directory(directory).map_err(|e| match e.raw_os_error() {
        Some(nix::libc::ENOENT) => refuse("does not exist"),
        Some(nix::libc::ENOTDIR) => refuse("is not a directory"),
        Some(nix::libc::ELOOP) => refuse("is a link"),
        _ => refuse("does not open"),
    })?;
    let stat = |fd: std::os::fd::BorrowedFd<'_>,
                what: &str|
     -> Result<std::fs::Metadata, LifecycleRefusal> {
        std::fs::File::from(nix::unistd::dup(fd).map_err(|_| refuse("does not duplicate"))?)
            .metadata()
            .map_err(|_| refuse(what))
    };
    let own = nix::unistd::geteuid().as_raw();
    let metadata = stat(opened.as_fd(), "does not stat")?;
    if !metadata.is_dir() {
        return Err(refuse("is not a directory"));
    }
    if metadata.uid() != own {
        return Err(refuse("is not root's"));
    }
    if metadata.mode() & 0o7777 != 0o710 {
        return Err(refuse(
            "is not mode 0710, root's with passage for the access group and nothing for other",
        ));
    }
    if carries_access_entries(directory) {
        return Err(refuse("carries an access-control entry beyond its mode"));
    }
    let access_gid = metadata.gid();
    let canonical = judge_ancestors(directory, &[own, 0])?;
    let save_points = nix::fcntl::openat(
        opened.as_fd(),
        "save-points",
        nix::fcntl::OFlag::O_RDONLY
            | nix::fcntl::OFlag::O_DIRECTORY
            | nix::fcntl::OFlag::O_NOFOLLOW
            | nix::fcntl::OFlag::O_CLOEXEC,
        nix::sys::stat::Mode::empty(),
    )
    .map_err(|e| match e {
        nix::errno::Errno::ENOENT => refuse("holds no save-points directory"),
        nix::errno::Errno::ENOTDIR => refuse("holds a save-points that is not a directory"),
        nix::errno::Errno::ELOOP => refuse("holds a save-points that is a link"),
        _ => refuse("holds a save-points that does not open"),
    })?;
    let metadata = stat(
        save_points.as_fd(),
        "holds a save-points that does not stat",
    )?;
    if metadata.uid() != own {
        return Err(refuse("holds a save-points that is not root's"));
    }
    if metadata.gid() != access_gid {
        return Err(refuse(
            "holds a save-points not grouped to the access group",
        ));
    }
    if metadata.mode() & 0o7777 != 0o750 {
        return Err(refuse("holds a save-points that is not mode 0750"));
    }
    // **A territory holding no `agent.toml` is no agent**, `NoSuchAgent` as a
    // root holding none was, and one holding a declaration that is not a
    // closed regular file of root's is the provisioning, refused: judged here
    // through the descriptor, as the inventory reads it.
    drop(open_declaration(opened.as_fd(), directory)?);
    Ok(JudgedTerritory {
        canonical,
        territory: opened,
        save_points,
        access_gid,
    })
}

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
/// `spu-binary`, `gate-binary`, `coordination-root`, `territory`,
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
        territory: path("territory")?,
        territory_fd: None,
        save_points: None,
        operator,
        access_gid: 0,
        boundary,
        root: root.to_path_buf(),
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
        let serving = member_vector(territory, &serving_binding);
        assert_eq!(
            serving.len(),
            1,
            "a serving load carries the territory alone"
        );
        assert_eq!(serving[0], territory.as_os_str());
        let diagnostic = member_vector(territory, &weaver_types::EnterBinding::Diagnostic);
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
        // **A serving load that elects a restore names no door**, per Spec
        // section 6 as of A3.2: it restores through descriptor 4, so the
        // vector is the territory alone whatever the lineage.
        assert_eq!(member_vector(territory, &serving_binding).len(), 1);
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
    /// agent's territory beside it, `<root>.territory`, laid out as the
    /// judgment asks with this test's uid in root's place: mode 0710, its
    /// `save-points/` 0750, and an empty `agent.toml` 0644. Answers the
    /// territory.
    fn write_root(root: &std::path::Path) -> std::path::PathBuf {
        use std::os::unix::fs::PermissionsExt;
        std::fs::create_dir_all(root).unwrap();
        let territory = root.with_extension("territory");
        let _ = std::fs::remove_dir_all(&territory);
        std::fs::create_dir_all(territory.join("save-points")).unwrap();
        std::fs::set_permissions(
            territory.join("save-points"),
            std::fs::Permissions::from_mode(0o750),
        )
        .unwrap();
        std::fs::write(territory.join("agent.toml"), "").unwrap();
        std::fs::set_permissions(
            territory.join("agent.toml"),
            std::fs::Permissions::from_mode(0o644),
        )
        .unwrap();
        std::fs::set_permissions(&territory, std::fs::Permissions::from_mode(0o710)).unwrap();
        let operator = nix::unistd::getuid().as_raw().to_string();
        let territory_path = territory.display().to_string();
        for (name, text) in [
            ("coordination-root", "/run/weaver"),
            ("worker-binary", "/opt/weaver/bin/worker"),
            ("spu-binary", "/opt/weaver/bin/weaver-spu"),
            ("gate-binary", "/opt/weaver/bin/weaver-gate"),
            ("territory", territory_path.as_str()),
            ("operator", operator.as_str()),
            ("roles.toml", "trace-reader = \"weaver-alpha-admincon\"\n"),
        ] {
            std::fs::write(root.join(name), text).unwrap();
        }
        territory
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

    /// **A territory with no `agent.toml` is no agent, and one
    /// that is not a closed regular file refuses**, per Spec section 9: the
    /// root's keys standing do not make an agent. Perturbations: drop the
    /// declaration check, or judge presence alone, and a case here reads.
    #[test]
    fn a_territory_without_a_declaration_is_no_agent() {
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
            config.territory,
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

    /// **The territory is root's at mode 0710, its `save-points/` 0750 and
    /// its declaration closed to writers, and never a link at its own name**,
    /// per Spec section 9 on the operator's ruling of 2026-10-07 on #1, with
    /// this test's uid in root's place. Perturbations: test only the write
    /// bits and the 0750 territory reads; drop the save-points judgment and
    /// the 0770 one reads; drop the declaration's mode check and the
    /// group-writable declaration reads.
    #[test]
    fn the_territory_is_roots_at_0710_and_never_a_link() {
        use std::os::unix::fs::PermissionsExt;
        let me = nix::unistd::getuid().as_raw();
        let base = std::env::temp_dir().join(format!("weaver-admin-decl-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let root = base.join("alpha");
        let territory = write_root(&root);
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(load_service_config_at(&base, "alpha", me).is_ok());
        for open in [0o750, 0o711, 0o700, 0o770, 0o1710] {
            std::fs::set_permissions(&territory, std::fs::Permissions::from_mode(open)).unwrap();
            assert_eq!(
                load_service_config_at(&base, "alpha", me).err(),
                Some(LifecycleRefusal::BoundaryUnverified),
                "{open:o} is not the territory's 0710"
            );
        }
        std::fs::set_permissions(&territory, std::fs::Permissions::from_mode(0o710)).unwrap();
        let save_points = territory.join("save-points");
        for open in [0o770, 0o755, 0o700] {
            std::fs::set_permissions(&save_points, std::fs::Permissions::from_mode(open)).unwrap();
            assert_eq!(
                load_service_config_at(&base, "alpha", me).err(),
                Some(LifecycleRefusal::BoundaryUnverified),
                "{open:o} is not save-points' 0750"
            );
        }
        std::fs::set_permissions(&save_points, std::fs::Permissions::from_mode(0o750)).unwrap();
        std::fs::rename(&save_points, territory.join("aside")).unwrap();
        assert_eq!(
            load_service_config_at(&base, "alpha", me).err(),
            Some(LifecycleRefusal::BoundaryUnverified),
            "no save-points directory is the provisioning incomplete"
        );
        std::fs::rename(territory.join("aside"), &save_points).unwrap();
        let declaration = territory.join("agent.toml");
        std::fs::set_permissions(&declaration, std::fs::Permissions::from_mode(0o664)).unwrap();
        assert_eq!(
            load_service_config_at(&base, "alpha", me).err(),
            Some(LifecycleRefusal::BoundaryUnverified),
            "a declaration the group could write"
        );
        std::fs::set_permissions(&declaration, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(load_service_config_at(&base, "alpha", me).is_ok());
        let link = base.join("linked.territory");
        std::os::unix::fs::symlink(&territory, &link).unwrap();
        std::fs::write(root.join("territory"), link.display().to_string()).unwrap();
        assert_eq!(
            load_service_config_at(&base, "alpha", me).err(),
            Some(LifecycleRefusal::BoundaryUnverified),
            "a link at the territory's own name"
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
            "territory",
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
            territory: PathBuf::from("/nonexistent/territory"),
            territory_fd: None,
            save_points: None,
            operator: 1000,
            access_gid: 1000,
            boundary: Ok(BoundaryRead {
                digest: "0".repeat(64),
                reader: "weaver-alpha-admincon".into(),
            }),
            root: PathBuf::from("/nonexistent/root"),
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
        config.root = base.join("root");
        std::fs::create_dir_all(&config.root).unwrap();
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
                let Ok(raw) = nix::sys::socket::accept4(
                    std::os::fd::AsRawFd::as_raw_fd(&listener),
                    nix::sys::socket::SockFlag::SOCK_CLOEXEC,
                ) else {
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
    /// A worker that answers as `answering_worker` does and keeps every
    /// directive it was sent, so a test reads what admin directed.
    fn recording_worker(
        config: &ServiceConfig,
        answers: Vec<weaver_types::Payload>,
    ) -> std::thread::JoinHandle<Vec<weaver_types::Payload>> {
        let listener = silent_worker(config);
        std::thread::spawn(move || {
            let mut directed = Vec::new();
            for payload in answers {
                let Ok(raw) = nix::sys::socket::accept4(
                    std::os::fd::AsRawFd::as_raw_fd(&listener),
                    nix::sys::socket::SockFlag::SOCK_CLOEXEC,
                ) else {
                    break;
                };
                let fd =
                    unsafe { <std::os::fd::OwnedFd as std::os::fd::FromRawFd>::from_raw_fd(raw) };
                let peer = channel::Coordination::adopt(fd);
                let Ok(request) = peer.recv() else { break };
                directed.push(request.payload);
                let _ = peer.send(&weaver_types::OrganEnvelope {
                    exchange: request.exchange,
                    position: weaver_types::Position::Close,
                    payload,
                });
            }
            directed
        })
    }

    fn report() -> weaver_types::SavePointReport {
        weaver_types::SavePointReport {
            save_point: "ab".into(),
            name: "ab.save-point".into(),
            run: weaver_types::RunId("r-1".into()),
            sequence: 5,
            turn: 1,
            event_run: weaver_types::RunId("r-1".into()),
            position: 7,
        }
    }

    /// **An unload whose leave save point is not taken does not complete**,
    /// per Spec section 3 on the operator's ruling of 2026-10-06 on #1 (A3.0
    /// item 6): the harness's `SavePointNotTaken` returns as the verb's
    /// refusal, no escalation ends the run, and the holder keeps the run
    /// lock; **`force-unload` directs the leave with `forced`** and completes,
    /// the escalation ending what still holds the lock. Perturbations: treat
    /// the refusal as silence and the first case escalates, the holder
    /// ending; send `forced: false` from `force_unload` and the second
    /// assertion fails.
    #[test]
    fn an_unload_without_its_save_point_stops_and_a_forced_one_completes() {
        let (config, _scratch) = scratch_config("unload-stops");
        let mut holder = stand_in_holder(&config);
        let worker = recording_worker(
            &config,
            vec![
                weaver_types::Payload::Answer(LifecycleAnswer::State {
                    state: weaver_types::AgentState::Idle,
                    load: None,
                    constituents: Vec::new(),
                }),
                weaver_types::Payload::Refusal(LifecycleRefusal::SavePointNotTaken {
                    missed: weaver_types::SavePointLeg::Finished,
                }),
            ],
        );
        assert_eq!(
            unload_within(&config, TEST_UNLOAD_BOUNDS, false),
            Err(LifecycleRefusal::SavePointNotTaken {
                missed: weaver_types::SavePointLeg::Finished,
            })
        );
        let directed = worker.join().unwrap();
        assert!(matches!(
            directed[1],
            weaver_types::Payload::Directive(LifecycleDirective::Leave { forced: false, .. })
        ));
        assert!(
            start::run_lock_held(&config.run_directory()).unwrap(),
            "the run stays open, its lock held"
        );
        assert!(
            holder.try_wait().unwrap().is_none(),
            "the holder was not ended"
        );

        // The first worker's name goes before the second binds it, and an
        // open marker stands, which the forced unload closes as forced even
        // through the escalation (Codex on #94, round 1).
        let _ = std::fs::remove_file(config.coordination_socket());
        save_points::write_marker(
            &config.root,
            Some(&save_points::Marker::Open { run: "r-1".into() }),
        )
        .unwrap();
        let worker = recording_worker(
            &config,
            vec![
                weaver_types::Payload::Answer(LifecycleAnswer::State {
                    state: weaver_types::AgentState::Idle,
                    load: None,
                    constituents: Vec::new(),
                }),
                weaver_types::Payload::Answer(LifecycleAnswer::Left { save_point: None }),
            ],
        );
        assert_eq!(
            unload_within(&config, TEST_UNLOAD_BOUNDS, true),
            Ok(LifecycleAnswer::State {
                state: weaver_types::AgentState::Unloaded,
                load: None,
                constituents: Vec::new(),
            })
        );
        let directed = worker.join().unwrap();
        assert!(
            matches!(
                directed[1],
                weaver_types::Payload::Directive(LifecycleDirective::Leave { forced: true, .. })
            ),
            "the forced unload directs a forced leave: {directed:?}"
        );
        let _ = holder.wait();
        assert!(!start::run_lock_held(&config.run_directory()).unwrap());
        assert_eq!(
            save_points::read_marker(&config.root),
            Some(save_points::Marker::Forced { run: "r-1".into() }),
            "the escalated forced unload still closes the marker as forced"
        );

        // **A marker that cannot be written refuses the unload** (Codex on
        // #94, round 2, the load's class at the other end): the root made
        // unwritable, an open marker standing, the forced unload ends the
        // run and answers `BoundaryUnverified` rather than unloaded.
        // Perturbation: log the failed write and answer unloaded again.
        let _ = std::fs::remove_file(config.coordination_socket());
        save_points::write_marker(
            &config.root,
            Some(&save_points::Marker::Open { run: "r-2".into() }),
        )
        .unwrap();
        let mut holder = stand_in_holder(&config);
        let worker = recording_worker(
            &config,
            vec![
                weaver_types::Payload::Answer(LifecycleAnswer::State {
                    state: weaver_types::AgentState::Idle,
                    load: None,
                    constituents: Vec::new(),
                }),
                weaver_types::Payload::Answer(LifecycleAnswer::Left { save_point: None }),
            ],
        );
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&config.root, std::fs::Permissions::from_mode(0o500)).unwrap();
        }
        let answered = unload_within(&config, TEST_UNLOAD_BOUNDS, true);
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&config.root, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let _ = worker.join();
        let _ = holder.wait();
        if nix::unistd::getuid().is_root() {
            // Root writes through the mode; the case cannot stand as root.
            assert!(answered.is_ok());
        } else {
            assert_eq!(answered, Err(LifecycleRefusal::BoundaryUnverified));
            assert_eq!(
                save_points::read_marker(&config.root),
                Some(save_points::Marker::Open { run: "r-2".into() }),
                "the marker stands as it was"
            );
        }
    }

    /// **The `save-point` verb asks the running worker and answers the
    /// report only once it is published**, per Spec sections 2 and 6: one
    /// directive, one answer, out of order where no run stands, and a refusal
    /// where the publication cannot be made. Perturbations: drop the run-lock
    /// check and the second case dials an absent worker and answers
    /// `Unanswered` instead; answer the report whatever the publication did
    /// and the third assertion sees `SavePointTaken`.
    #[test]
    fn the_save_point_verb_answers_the_report_and_is_out_of_order_without_a_run() {
        let (config, _scratch) = scratch_config("save-point-verb");
        let agent = AgentName("alpha".into());
        assert_eq!(
            save_point(&config, &agent),
            Err(LifecycleRefusal::OutOfOrder),
            "no run, no save point"
        );
        let mut holder = stand_in_holder(&config);
        let worker = recording_worker(
            &config,
            vec![weaver_types::Payload::Answer(
                LifecycleAnswer::SavePointTaken { report: report() },
            )],
        );
        // The fixture holds no declaration to publish from, so the verb
        // refuses rather than answering a report of a save point that
        // stands in the room alone (Codex on #94, round 1); the directive
        // reached the worker all the same.
        let answered = save_point(&config, &agent);
        assert!(
            matches!(
                answered,
                Err(LifecycleRefusal::NoSuchAgent)
                    | Err(LifecycleRefusal::BoundaryUnverified)
                    | Err(LifecycleRefusal::ConfigInvalid { .. })
            ),
            "answers only a published save point: {answered:?}"
        );
        let directed = worker.join().unwrap();
        assert!(matches!(
            directed[0],
            weaver_types::Payload::Directive(LifecycleDirective::SavePoint { .. })
        ));
        let _ = holder.kill();
        let _ = holder.wait();
    }

    /// **A forced verb closes the marker where the run already ended**, per
    /// Spec section 3 (Codex on #94, round 6): with the lock free and an open
    /// marker standing, `force-unload` answers unloaded and the marker reads
    /// forced, while `unload` leaves it open as the unclean stop it is; and a
    /// forced leave refused by a harness that then goes down closes the marker
    /// as forced inside the after-left wait, the refusal still returned.
    /// Perturbations: skip the early branch's close and the first marker
    /// stays open; return the refusal without the wait and the third does.
    #[test]
    fn a_forced_verb_closes_the_marker_where_the_run_already_ended() {
        let (config, _scratch) = scratch_config("forced-ended");
        let open = |run: &str| save_points::Marker::Open { run: run.into() };
        save_points::write_marker(&config.root, Some(&open("r-1"))).unwrap();
        let answered = unload_within(&config, TEST_UNLOAD_BOUNDS, false);
        assert!(matches!(
            answered,
            Ok(LifecycleAnswer::State {
                state: weaver_types::AgentState::Unloaded,
                ..
            })
        ));
        assert_eq!(
            save_points::read_marker(&config.root),
            Some(open("r-1")),
            "an unforced verb leaves the unclean stop for the next load"
        );
        let answered = unload_within(&config, TEST_UNLOAD_BOUNDS, true);
        assert!(matches!(
            answered,
            Ok(LifecycleAnswer::State {
                state: weaver_types::AgentState::Unloaded,
                ..
            })
        ));
        assert_eq!(
            save_points::read_marker(&config.root),
            Some(save_points::Marker::Forced { run: "r-1".into() }),
            "the forced verb records the operator's choice"
        );
        // The refusal arm: the holder keeps the lock while the worker
        // refuses the leave, then goes down inside the after-left wait.
        save_points::write_marker(&config.root, Some(&open("r-2"))).unwrap();
        let _ = std::fs::remove_file(config.coordination_socket());
        let mut holder = stand_in_holder(&config);
        let worker = recording_worker(
            &config,
            vec![
                weaver_types::Payload::Answer(LifecycleAnswer::State {
                    state: weaver_types::AgentState::Idle,
                    load: None,
                    constituents: Vec::new(),
                }),
                weaver_types::Payload::Refusal(LifecycleRefusal::ActivityNotAtRest),
            ],
        );
        let ender = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(100));
            let _ = holder.kill();
            let _ = holder.wait();
        });
        let answered = unload_within(&config, TEST_UNLOAD_BOUNDS, true);
        ender.join().unwrap();
        let _ = worker.join();
        assert!(matches!(answered, Err(LifecycleRefusal::ActivityNotAtRest)));
        assert_eq!(
            save_points::read_marker(&config.root),
            Some(save_points::Marker::Forced { run: "r-2".into() }),
            "the refusal after the run ended still closed the marker as forced"
        );
    }

    /// **The member's group set carries the access group beside its own**,
    /// per Spec section 6 on the operator's ruling of 2026-10-07 on #1 (Codex
    /// on #94, round 8): the territory is `0710` to the access group, and the
    /// drop sets the supplementary set from this slice alone, so a member
    /// dropped to its own group alone could not reach its room. Perturbation:
    /// answer the member's group alone and the assertion fails.
    #[test]
    fn the_members_group_set_carries_the_access_group() {
        let member = inventory::MemberAccount {
            uid: 1501,
            gid: 1501,
        };
        assert_eq!(member_groups(member, 1600), [1501, 1600]);
    }

    /// **A clean unload whose leave save point did not publish does not
    /// complete**, per Spec section 3 on A3.0 item 6 (Codex on #94, round 9):
    /// the worker answers `Left` naming a save point, the publication refuses
    /// (here the scratch inventory's refusal; on a box, a room file past the
    /// bound or any publication that does not land), the verb refuses
    /// `SavePointNotTaken` naming the publication and the marker stays open,
    /// so the next load records `NoCleanUnload`. The forced unload's case,
    /// no save point reported, is `a_forced_verb_closes_the_marker_where_the_run_already_ended`.
    /// Perturbation: discard the publication's result again and the verb
    /// answers unloaded with the marker closed.
    #[test]
    fn a_clean_unload_whose_save_point_did_not_publish_stops() {
        let (config, _scratch) = scratch_config("unload-unpublished");
        save_points::write_marker(
            &config.root,
            Some(&save_points::Marker::Open { run: "r-1".into() }),
        )
        .unwrap();
        let mut holder = stand_in_holder(&config);
        let worker = recording_worker(
            &config,
            vec![
                weaver_types::Payload::Answer(LifecycleAnswer::State {
                    state: weaver_types::AgentState::Idle,
                    load: None,
                    constituents: Vec::new(),
                }),
                weaver_types::Payload::Answer(LifecycleAnswer::Left {
                    save_point: Some(report()),
                }),
            ],
        );
        let ender = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(100));
            let _ = holder.kill();
            let _ = holder.wait();
        });
        let answered = unload_within(&config, TEST_UNLOAD_BOUNDS, false);
        ender.join().unwrap();
        let _ = worker.join();
        assert_eq!(
            answered,
            Err(LifecycleRefusal::SavePointNotTaken {
                missed: weaver_types::SavePointLeg::Published,
            })
        );
        assert_eq!(
            save_points::read_marker(&config.root),
            Some(save_points::Marker::Open { run: "r-1".into() }),
            "the marker stays open for the next load's reset"
        );
        // The helper's own cases: a reported digest among the lines passes,
        // one missing or a refused publication names it, none reported passes.
        let line = save_points::ManifestLine {
            ordinal: 1,
            digest: "ab".into(),
            name: "x".into(),
            stamp: save_points::Stamp {
                run: "r-1".into(),
                sequence: 5,
                turn: 1,
                schema: String::new(),
                wall_ns: 0,
            },
            position: None,
            arrived: save_points::Arrival::Leave,
        };
        let reports = vec![(report(), save_points::Arrival::Leave)];
        assert_eq!(unpublished_leave(&reports, &Ok(vec![line])), None);
        assert_eq!(unpublished_leave(&reports, &Ok(vec![])), Some("ab".into()));
        assert_eq!(
            unpublished_leave(&reports, &Err(LifecycleRefusal::BoundaryUnverified)),
            Some("ab".into())
        );
        assert_eq!(
            unpublished_leave(&[], &Err(LifecycleRefusal::BoundaryUnverified)),
            None
        );
    }

    /// **A sink outside the territory refuses naming `trace-sink`**, per Spec
    /// section 9 (Codex on #94, round 10): the sink's directory must be the
    /// judged territory itself, a file, a pipe or a socket alike.
    /// Perturbation: compare the sink's path prefix instead and a sink in a
    /// subdirectory of the territory passes.
    #[test]
    fn a_sink_outside_the_territory_refuses() {
        let territory = std::path::Path::new("/var/lib/weaver-agent/weaver-alpha");
        let file = |path: &str| weaver_types::TraceSink::File {
            path: path.into(),
            create: false,
        };
        assert!(
            sink_within_territory(
                &file("/var/lib/weaver-agent/weaver-alpha/trace.ndjson"),
                territory
            )
            .is_ok()
        );
        for elsewhere in [
            "/var/lib/weaver-agent/weaver-beta/trace.ndjson",
            "/var/lib/weaver-agent/weaver-alpha/state/trace.ndjson",
            "/srv/trace.ndjson",
        ] {
            assert_eq!(
                sink_within_territory(&file(elsewhere), territory).err(),
                Some(LifecycleRefusal::ConfigInvalid {
                    field: Some(FieldName("trace-sink".into()))
                }),
                "{elsewhere}"
            );
        }
    }

    /// **A forced unload keeps state where it can**, per Spec section 3 on
    /// the operator's clarification of 2026-10-07 on #1: a reported save
    /// point that published means state was kept and the marker closes
    /// clean; no save point, or one that did not publish, means it was not
    /// and the marker stays open under `ForcedUnload`. Perturbation: answer
    /// forced always and the first case reads lost.
    #[test]
    fn a_forced_unload_kept_state_only_where_its_save_point_published() {
        let line = save_points::ManifestLine {
            ordinal: 1,
            digest: "ab".into(),
            name: "x".into(),
            stamp: save_points::Stamp {
                run: "r-1".into(),
                sequence: 5,
                turn: 1,
                schema: String::new(),
                wall_ns: 0,
            },
            position: None,
            arrived: save_points::Arrival::Leave,
        };
        let reports = vec![(report(), save_points::Arrival::Leave)];
        assert!(forced_kept_state(&reports, &Ok(vec![line])));
        assert!(!forced_kept_state(&reports, &Ok(vec![])));
        assert!(!forced_kept_state(
            &reports,
            &Err(LifecycleRefusal::BoundaryUnverified)
        ));
        assert!(
            !forced_kept_state(&[], &Ok(vec![])),
            "no save point, nothing kept"
        );
    }

    /// A scratch config whose save-points directory is open, judged against
    /// this test's uid, with the marker open, for the conclusion's order.
    fn conclusion_scratch(
        tag: &str,
    ) -> (ServiceConfig, crate::scratch::Scratch, save_points::Owner) {
        let (mut config, scratch) = scratch_config(tag);
        let directory = scratch.0.join("save-points");
        std::fs::create_dir_all(&directory).unwrap();
        config.save_points = Some(save_points::open_directory(&directory).unwrap());
        save_points::write_marker(
            &config.root,
            Some(&save_points::Marker::Open { run: "r-1".into() }),
        )
        .unwrap();
        let mine = save_points::Owner {
            uid: nix::unistd::getuid().as_raw(),
            gid: nix::unistd::getgid().as_raw(),
        };
        (config, scratch, mine)
    }

    /// A publication stand-in that appends the reported save point's line to
    /// the manifest, as `publish` does on a box, and counts its calls.
    fn appending_publisher<'a>(
        config: &'a ServiceConfig,
        owner: save_points::Owner,
        calls: &'a std::cell::Cell<u32>,
    ) -> impl FnMut(
        &[(weaver_types::SavePointReport, save_points::Arrival)],
    ) -> Result<Vec<save_points::ManifestLine>, LifecycleRefusal>
    + 'a {
        move |reports| {
            calls.set(calls.get() + 1);
            let directory = config.save_points_fd()?;
            let mut lines = Vec::new();
            for (report, arrived) in reports {
                let line = save_points::ManifestLine {
                    ordinal: u64::from(calls.get()),
                    digest: report.save_point.clone(),
                    name: report.name.clone(),
                    stamp: save_points::Stamp {
                        run: report.run.0.clone(),
                        sequence: report.sequence,
                        turn: report.turn,
                        schema: String::new(),
                        wall_ns: 0,
                    },
                    position: None,
                    arrived: *arrived,
                };
                save_points::append_line(directory, owner, &line)?;
                lines.push(line);
            }
            Ok(lines)
        }
    }

    fn manifest_digests(config: &ServiceConfig, owner: save_points::Owner) -> Vec<String> {
        save_points::read_manifest(config.save_points_fd().unwrap(), owner)
            .unwrap()
            .into_iter()
            .map(|line| line.digest)
            .collect()
    }

    /// A config at `base`, laid out as `scratch_config` lays one: the force's
    /// child process builds the same config from the base its parent names.
    fn config_at(base: &std::path::Path) -> ServiceConfig {
        let mut config = unread_config();
        config.coordination_root = base.to_path_buf();
        config.root = base.join("root");
        config
    }

    /// A publication stand-in that counts its calls and answers a line for
    /// every reported save point, as a publication that landed does.
    fn counting_publisher(
        calls: &std::cell::Cell<u32>,
    ) -> impl FnMut(
        &[(weaver_types::SavePointReport, save_points::Arrival)],
    ) -> Result<Vec<save_points::ManifestLine>, LifecycleRefusal>
    + '_ {
        move |reports| {
            calls.set(calls.get() + 1);
            Ok(reports
                .iter()
                .map(|(report, arrived)| save_points::ManifestLine {
                    ordinal: u64::from(calls.get()),
                    digest: report.save_point.clone(),
                    name: report.name.clone(),
                    stamp: save_points::Stamp {
                        run: report.run.0.clone(),
                        sequence: report.sequence,
                        turn: report.turn,
                        schema: String::new(),
                        wall_ns: 0,
                    },
                    position: None,
                    arrived: *arrived,
                })
                .collect())
        }
    }

    /// **A worker whose run ends as it answers `Left`**, as the real worker's
    /// does: it answers each dial in turn with the next answer, `None`
    /// closing the dial unanswered as a harness that went away, and keeps
    /// what it was sent. The run-lock holder is ended just before a `Left`
    /// goes out. A `JoinLeave` is answered `join` where one is given, as a
    /// harness with no leave pending answers every join alike, without
    /// spending an answer. Answers what it was sent and whether the holder
    /// had ended within ten seconds of the last answer, by whatever ended it.
    fn ending_worker(
        config: &ServiceConfig,
        answers: Vec<Option<weaver_types::Payload>>,
        join: Option<weaver_types::Payload>,
        mut holder: std::process::Child,
    ) -> std::thread::JoinHandle<(Vec<weaver_types::Payload>, bool)> {
        let listener = silent_worker(config);
        std::thread::spawn(move || {
            let mut directed = Vec::new();
            let mut answers = answers.into_iter();
            while answers.len() > 0 {
                // Each dial is waited for ten seconds at most, so an admin
                // that never dials fails its test rather than hanging it.
                let mut ready = nix::libc::pollfd {
                    fd: std::os::fd::AsRawFd::as_raw_fd(&listener),
                    events: nix::libc::POLLIN,
                    revents: 0,
                };
                // SAFETY: poll on one descriptor this thread owns.
                if unsafe { nix::libc::poll(&mut ready, 1, 10_000) } <= 0 {
                    break;
                }
                let Ok(raw) = nix::sys::socket::accept4(
                    std::os::fd::AsRawFd::as_raw_fd(&listener),
                    nix::sys::socket::SockFlag::SOCK_CLOEXEC,
                ) else {
                    break;
                };
                let fd =
                    unsafe { <std::os::fd::OwnedFd as std::os::fd::FromRawFd>::from_raw_fd(raw) };
                let peer = channel::Coordination::adopt(fd);
                let Ok(request) = peer.recv() else { break };
                let joining = matches!(
                    request.payload,
                    weaver_types::Payload::Directive(LifecycleDirective::JoinLeave { .. })
                );
                directed.push(request.payload);
                let answer = match (&join, joining) {
                    (Some(join), true) => Some(join.clone()),
                    _ => answers.next().expect("an answer remains"),
                };
                let Some(payload) = answer else {
                    drop(peer);
                    continue;
                };
                if matches!(
                    payload,
                    weaver_types::Payload::Answer(LifecycleAnswer::Left { .. })
                ) {
                    let _ = holder.kill();
                    let _ = holder.wait();
                }
                let _ = peer.send(&weaver_types::OrganEnvelope {
                    exchange: request.exchange,
                    position: weaver_types::Position::Close,
                    payload,
                });
            }
            let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
            let ended = loop {
                match holder.try_wait() {
                    Ok(Some(_)) => break true,
                    _ if std::time::Instant::now() > until => break false,
                    _ => std::thread::sleep(std::time::Duration::from_millis(20)),
                }
            };
            let _ = holder.kill();
            let _ = holder.wait();
            (directed, ended)
        })
    }

    fn state(state: weaver_types::AgentState) -> Option<weaver_types::Payload> {
        Some(weaver_types::Payload::Answer(LifecycleAnswer::State {
            state,
            load: None,
            constituents: Vec::new(),
        }))
    }

    fn left_with_report() -> Option<weaver_types::Payload> {
        Some(weaver_types::Payload::Answer(LifecycleAnswer::Left {
            save_point: Some(report()),
        }))
    }

    fn is_unloaded(answered: &Result<LifecycleAnswer, LifecycleRefusal>) -> bool {
        matches!(
            answered,
            Ok(LifecycleAnswer::State {
                state: weaver_types::AgentState::Unloaded,
                ..
            })
        )
    }

    /// **A graceful unload holds the invocation lock through its drain, so a
    /// load in it refuses there**, per Spec section 3 on the operator's
    /// ruling of 2026-10-07 on #1: the worker holds the leave's answer, as a
    /// turn running to its close does; another process's exclusive take,
    /// which is a load's first step, is refused, and the run lock is held;
    /// once the worker answers `Left` the unload publishes, closes the marker
    /// clean and answers unloaded. The lock is a process's `fcntl` lock and
    /// never conflicts with a take from the same process, so a child process
    /// asks. Perturbation: release the lock once the leave is sent, as the
    /// drain did at 1de527c, and the take during the drain succeeds.
    #[test]
    fn a_load_during_a_graceful_drain_refuses_at_the_invocation_lock() {
        let (config, _scratch) = scratch_config("drain-holds-lock");
        save_points::write_marker(
            &config.root,
            Some(&save_points::Marker::Open { run: "r-1".into() }),
        )
        .unwrap();
        let mut holder = stand_in_holder(&config);
        let listener = silent_worker(&config);
        let (heard_tx, heard) = std::sync::mpsc::channel::<()>();
        let (release, release_rx) = std::sync::mpsc::channel::<()>();
        let worker = std::thread::spawn(move || {
            for at in 0..2 {
                let Ok(raw) = nix::sys::socket::accept4(
                    std::os::fd::AsRawFd::as_raw_fd(&listener),
                    nix::sys::socket::SockFlag::SOCK_CLOEXEC,
                ) else {
                    return;
                };
                let fd =
                    unsafe { <std::os::fd::OwnedFd as std::os::fd::FromRawFd>::from_raw_fd(raw) };
                let peer = channel::Coordination::adopt(fd);
                let Ok(request) = peer.recv() else { return };
                let payload = if at == 0 {
                    state(weaver_types::AgentState::Active).unwrap()
                } else {
                    heard_tx.send(()).unwrap();
                    let _ = release_rx.recv();
                    weaver_types::Payload::Answer(LifecycleAnswer::Left { save_point: None })
                };
                let _ = peer.send(&weaver_types::OrganEnvelope {
                    exchange: request.exchange,
                    position: weaver_types::Position::Close,
                    payload,
                });
            }
        });
        let calls = std::cell::Cell::new(0);
        let (answered, taken_in_drain, run_lock_free) = std::thread::scope(|scope| {
            let unload = scope.spawn(|| {
                let calls = std::cell::Cell::new(0);
                let mut publisher = counting_publisher(&calls);
                let answered = unload_with(&config, TEST_UNLOAD_BOUNDS, false, &mut publisher);
                (answered, calls.get())
            });
            heard
                .recv_timeout(std::time::Duration::from_secs(5))
                .expect("the leave arrives");
            let probe = std::process::Command::new("python3")
                .args([
                    "-c",
                    "import fcntl, sys\nf = open(sys.argv[1], 'r+')\nfcntl.lockf(f, fcntl.LOCK_EX | fcntl.LOCK_NB)",
                ])
                .arg(config.run_directory().join("admin.lock"))
                .stderr(std::process::Stdio::null())
                .status()
                .expect("python3 runs");
            let run_lock_free = start::take_run_lock(&config.run_directory())
                .unwrap()
                .is_some();
            // Released before anything is judged, so a failing case fails
            // rather than holding the drain for ever.
            let _ = release.send(());
            let _ = holder.kill();
            let _ = holder.wait();
            let (answered, published) = unload.join().unwrap();
            calls.set(published);
            (answered, probe.success(), run_lock_free)
        });
        worker.join().unwrap();
        assert!(
            !taken_in_drain,
            "the drain holds the invocation lock, so a load's take refuses"
        );
        assert!(!run_lock_free, "the worker stands, holding the run lock");
        assert!(is_unloaded(&answered), "{answered:?}");
        assert_eq!(calls.get(), 1, "the graceful unload published");
        assert_eq!(
            save_points::read_marker(&config.root),
            Some(save_points::Marker::Closed { run: "r-1".into() })
        );
    }

    /// The force's child process for
    /// `a_force_during_a_graceful_drain_is_delivered_lock_free_and_the_graceful_unload_publishes_once`:
    /// a `force-unload` from another process, which the lock its parent holds
    /// refuses. Does nothing where no parent names its base.
    #[test]
    #[ignore = "the force's child process; run by its parent test"]
    fn force_unload_in_a_child() {
        let Ok(base) = std::env::var("WEAVER_ADMIN_TEST_FORCE_BASE") else {
            return;
        };
        let config = config_at(std::path::Path::new(&base));
        let answered = unload_within(&config, TEST_UNLOAD_BOUNDS, true);
        println!("force answered: {answered:?}");
    }

    /// **A force during a graceful drain is delivered without the lock, and
    /// the graceful unload publishes once**, per Spec section 3 on the
    /// operator's ruling of 2026-10-07 on #1: the graceful unload holds the
    /// lock with its leave pending; a `force-unload` in another process finds
    /// the lock held, sends `JoinLeave` to the harness and is answered with
    /// the shared `Left`, publishing nothing and touching no marker; the
    /// graceful unload publishes once and closes the marker clean.
    /// Perturbation: have the force refuse at the held lock, as every other
    /// verb does, and it never reaches the harness.
    #[test]
    fn a_force_during_a_graceful_drain_is_delivered_lock_free_and_the_graceful_unload_publishes_once()
     {
        let (config, scratch) = scratch_config("force-joins");
        save_points::write_marker(
            &config.root,
            Some(&save_points::Marker::Open { run: "r-1".into() }),
        )
        .unwrap();
        let mut holder = stand_in_holder(&config);
        let listener = silent_worker(&config);
        let (heard_tx, heard) = std::sync::mpsc::channel::<()>();
        // The harness: the observation, the graceful leave held pending, the
        // force's join; then the run ends and both are answered `Left`.
        let worker = std::thread::spawn(move || {
            let mut directed = Vec::new();
            let mut pending = Vec::new();
            for at in 0..3 {
                // The force's dial is waited for ten seconds at most, so a
                // force that never reaches the harness fails the test rather
                // than holding the graceful leave for ever.
                if at == 2 {
                    let mut ready = nix::libc::pollfd {
                        fd: std::os::fd::AsRawFd::as_raw_fd(&listener),
                        events: nix::libc::POLLIN,
                        revents: 0,
                    };
                    // SAFETY: poll on one descriptor this thread owns.
                    if unsafe { nix::libc::poll(&mut ready, 1, 10_000) } <= 0 {
                        break;
                    }
                }
                let Ok(raw) = nix::sys::socket::accept4(
                    std::os::fd::AsRawFd::as_raw_fd(&listener),
                    nix::sys::socket::SockFlag::SOCK_CLOEXEC,
                ) else {
                    break;
                };
                let fd =
                    unsafe { <std::os::fd::OwnedFd as std::os::fd::FromRawFd>::from_raw_fd(raw) };
                let peer = channel::Coordination::adopt(fd);
                let Ok(request) = peer.recv() else { break };
                directed.push(request.payload.clone());
                if at == 0 {
                    let _ = peer.send(&weaver_types::OrganEnvelope {
                        exchange: request.exchange,
                        position: weaver_types::Position::Close,
                        payload: state(weaver_types::AgentState::Active).unwrap(),
                    });
                    continue;
                }
                if at == 1 {
                    heard_tx.send(()).unwrap();
                }
                pending.push((peer, request.exchange));
            }
            let _ = holder.kill();
            let _ = holder.wait();
            for (peer, exchange) in pending.into_iter().rev() {
                let _ = peer.send(&weaver_types::OrganEnvelope {
                    exchange,
                    position: weaver_types::Position::Close,
                    payload: left_with_report().unwrap(),
                });
            }
            directed
        });
        let (answered, published, child) = std::thread::scope(|scope| {
            let unload = scope.spawn(|| {
                let calls = std::cell::Cell::new(0);
                let mut publisher = counting_publisher(&calls);
                let answered = unload_with(&config, TEST_UNLOAD_BOUNDS, false, &mut publisher);
                (answered, calls.get())
            });
            heard
                .recv_timeout(std::time::Duration::from_secs(5))
                .expect("the graceful leave arrives");
            let child = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--ignored",
                    "--exact",
                    "tests::force_unload_in_a_child",
                    "--nocapture",
                    "--test-threads=1",
                ])
                .env("WEAVER_ADMIN_TEST_FORCE_BASE", &scratch.0)
                .stdin(std::process::Stdio::null())
                .output()
                .expect("the force's child runs");
            let (answered, published) = unload.join().unwrap();
            (answered, published, child)
        });
        let directed = worker.join().unwrap();
        let child_out = String::from_utf8_lossy(&child.stdout);
        assert!(
            child.status.success()
                && child_out.contains("force answered: Ok(State { state: Unloaded"),
            "{child_out}\n{}",
            String::from_utf8_lossy(&child.stderr)
        );
        assert!(
            matches!(
                directed.as_slice(),
                [
                    weaver_types::Payload::Directive(LifecycleDirective::Observe),
                    weaver_types::Payload::Directive(LifecycleDirective::Leave {
                        forced: false,
                        ..
                    }),
                    weaver_types::Payload::Directive(LifecycleDirective::JoinLeave { .. }),
                ]
            ),
            "the force reached the harness as a join, without the lock: {directed:?}"
        );
        assert!(is_unloaded(&answered), "{answered:?}");
        assert_eq!(published, 1, "the graceful unload published, once");
        assert_eq!(
            save_points::read_marker(&config.root),
            Some(save_points::Marker::Closed { run: "r-1".into() })
        );
    }

    /// The force's child process, started against `base`, its output piped.
    fn spawn_child_force(base: &std::path::Path) -> std::process::Child {
        std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "tests::force_unload_in_a_child",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("WEAVER_ADMIN_TEST_FORCE_BASE", base)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("the force's child runs")
    }

    /// Accepts the next dial within ten seconds and reads its directive. The
    /// stand-in harnesses accept close-on-exec, so a force's child process
    /// spawned meanwhile holds none of their connections open.
    fn accept_within(
        listener: &std::os::fd::OwnedFd,
    ) -> Option<(channel::Coordination, weaver_types::OrganEnvelope)> {
        let mut ready = nix::libc::pollfd {
            fd: std::os::fd::AsRawFd::as_raw_fd(listener),
            events: nix::libc::POLLIN,
            revents: 0,
        };
        // SAFETY: poll on one descriptor this thread owns.
        if unsafe { nix::libc::poll(&mut ready, 1, 10_000) } <= 0 {
            return None;
        }
        let raw = nix::sys::socket::accept4(
            std::os::fd::AsRawFd::as_raw_fd(listener),
            nix::sys::socket::SockFlag::SOCK_CLOEXEC,
        )
        .ok()?;
        let fd = unsafe { <std::os::fd::OwnedFd as std::os::fd::FromRawFd>::from_raw_fd(raw) };
        let peer = channel::Coordination::adopt(fd);
        let request = peer.recv().ok()?;
        Some((peer, request))
    }

    fn close_with(
        peer: &channel::Coordination,
        exchange: weaver_types::ExchangeId,
        payload: weaver_types::Payload,
    ) {
        let _ = peer.send(&weaver_types::OrganEnvelope {
            exchange,
            position: weaver_types::Position::Close,
            payload,
        });
    }

    /// **A live, silent harness does not hold the agent**, per Spec section
    /// 3 on the operator's ruling of 2026-10-07 on #1: the harness takes the
    /// graceful leave and the force's join and answers neither; the force,
    /// its join unanswered inside the leave's bound (injected at two seconds
    /// here), closes the marker as forced and escalates against the run
    /// without the lock; the run's processes end, the graceful holder's wait
    /// reads the end of its connection and concludes with no save point
    /// taken, the marker forced. **With a root that will not take the marker**
    /// (Codex on #94, round 18), the run is ended all the same and the force
    /// answers the marker's refusal after it, the marker left as it stood.
    /// Perturbations: the force waits for the lock instead, and nothing ends
    /// the run until the stand-in's patience runs out; write the marker
    /// before the escalation and return on its refusal, and the unwritable
    /// case leaves the run standing.
    #[test]
    fn a_silent_harness_is_ended_by_the_force_without_the_lock() {
        for unwritable in [false, true] {
            let (config, scratch) = scratch_config(if unwritable {
                "force-escalates-ro"
            } else {
                "force-escalates"
            });
            save_points::write_marker(
                &config.root,
                Some(&save_points::Marker::Open { run: "r-1".into() }),
            )
            .unwrap();
            if unwritable {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&config.root, std::fs::Permissions::from_mode(0o500))
                    .unwrap();
            }
            let mut holder = stand_in_holder(&config);
            let listener = silent_worker(&config);
            let (heard_tx, heard) = std::sync::mpsc::channel::<()>();
            // The harness: answers the observation, then takes the leave and the
            // join and answers neither; its connections close when the run's
            // processes end, as a worker's do when it dies, or past a patience.
            let worker = std::thread::spawn(move || {
                let mut directed = Vec::new();
                let mut held = Vec::new();
                if let Some((peer, request)) = accept_within(&listener) {
                    directed.push(request.payload.clone());
                    close_with(
                        &peer,
                        request.exchange,
                        state(weaver_types::AgentState::Active).unwrap(),
                    );
                }
                for at in 0..2 {
                    let Some((peer, request)) = accept_within(&listener) else {
                        break;
                    };
                    directed.push(request.payload.clone());
                    held.push(peer);
                    if at == 0 {
                        heard_tx.send(()).unwrap();
                    }
                }
                let until = std::time::Instant::now() + std::time::Duration::from_secs(12);
                let ended_by_the_force = loop {
                    match holder.try_wait() {
                        Ok(Some(_)) => break true,
                        _ if std::time::Instant::now() > until => break false,
                        _ => std::thread::sleep(std::time::Duration::from_millis(20)),
                    }
                };
                drop(held);
                let _ = holder.kill();
                let _ = holder.wait();
                (directed, ended_by_the_force)
            });
            let (answered, published, child) = std::thread::scope(|scope| {
                let unload = scope.spawn(|| {
                    let calls = std::cell::Cell::new(0);
                    let mut publisher = counting_publisher(&calls);
                    let answered = unload_with(&config, TEST_UNLOAD_BOUNDS, false, &mut publisher);
                    (answered, calls.get())
                });
                heard
                    .recv_timeout(std::time::Duration::from_secs(5))
                    .expect("the graceful leave arrives");
                let child = spawn_child_force(&scratch.0)
                    .wait_with_output()
                    .expect("the force's child finishes");
                let (answered, published) = unload.join().unwrap();
                (answered, published, child)
            });
            let (directed, ended_by_the_force) = worker.join().unwrap();
            if unwritable {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&config.root, std::fs::Permissions::from_mode(0o700))
                    .unwrap();
            }
            let child_out = String::from_utf8_lossy(&child.stdout);
            let expected = if unwritable {
                "force answered: Err(BoundaryUnverified)"
            } else {
                "force answered: Ok(State { state: Unloaded"
            };
            assert!(
                child_out.contains(expected),
                "unwritable {unwritable}: {child_out}\n{}",
                String::from_utf8_lossy(&child.stderr)
            );
            assert!(
                matches!(
                    directed.as_slice(),
                    [
                        weaver_types::Payload::Directive(LifecycleDirective::Observe),
                        weaver_types::Payload::Directive(LifecycleDirective::Leave {
                            forced: false,
                            ..
                        }),
                        weaver_types::Payload::Directive(LifecycleDirective::JoinLeave { .. }),
                    ]
                ),
                "unwritable {unwritable}: {directed:?}"
            );
            assert!(
                ended_by_the_force,
                "unwritable {unwritable}: the force ended the run, not the stand-in's patience"
            );
            assert!(is_unloaded(&answered), "{answered:?}");
            assert_eq!(published, 0, "no save point was taken");
            let marker = if unwritable {
                save_points::Marker::Open { run: "r-1".into() }
            } else {
                save_points::Marker::Forced { run: "r-1".into() }
            };
            assert_eq!(save_points::read_marker(&config.root), Some(marker));
        }
    }

    /// **A force that comes before the leave is pending joins once it is**,
    /// per Spec section 3 on the operator's ruling of 2026-10-07 on #1: the
    /// graceful unload holds the lock and is still observing when the force's
    /// first join arrives, which the harness refuses `OutOfOrder`; the force,
    /// the lock still held, asks again a second later, by when the graceful
    /// leave is pending, and joins it, answered with the shared `Left`; the
    /// graceful unload publishes once. Perturbation: a single join attempt,
    /// and the force waits behind the whole drain instead.
    #[test]
    fn a_force_before_the_leave_joins_once_it_is_pending() {
        let (config, scratch) = scratch_config("force-before-leave");
        save_points::write_marker(
            &config.root,
            Some(&save_points::Marker::Open { run: "r-1".into() }),
        )
        .unwrap();
        let mut holder = stand_in_holder(&config);
        let listener = silent_worker(&config);
        let (observed_tx, observed) = std::sync::mpsc::channel::<()>();
        let worker = std::thread::spawn(move || {
            let mut directed = Vec::new();
            // The observation, held until the force's first join is refused.
            let (observe_peer, observe) = accept_within(&listener).expect("the observation");
            directed.push(observe.payload.clone());
            observed_tx.send(()).unwrap();
            if let Some((peer, request)) = accept_within(&listener) {
                directed.push(request.payload.clone());
                close_with(
                    &peer,
                    request.exchange,
                    weaver_types::Payload::Refusal(LifecycleRefusal::OutOfOrder),
                );
            }
            close_with(
                &observe_peer,
                observe.exchange,
                state(weaver_types::AgentState::Active).unwrap(),
            );
            // The graceful leave, pending; then the force's next join.
            let mut pending = Vec::new();
            for _ in 0..2 {
                let Some((peer, request)) = accept_within(&listener) else {
                    break;
                };
                directed.push(request.payload.clone());
                pending.push((peer, request.exchange));
            }
            let _ = holder.kill();
            let _ = holder.wait();
            for (peer, exchange) in pending.into_iter().rev() {
                close_with(&peer, exchange, left_with_report().unwrap());
            }
            directed
        });
        let (answered, published, child) = std::thread::scope(|scope| {
            let unload = scope.spawn(|| {
                let calls = std::cell::Cell::new(0);
                let mut publisher = counting_publisher(&calls);
                let answered = unload_with(&config, TEST_UNLOAD_BOUNDS, false, &mut publisher);
                (answered, calls.get())
            });
            observed
                .recv_timeout(std::time::Duration::from_secs(5))
                .expect("the observation arrives");
            let child = spawn_child_force(&scratch.0)
                .wait_with_output()
                .expect("the force's child finishes");
            let (answered, published) = unload.join().unwrap();
            (answered, published, child)
        });
        let directed = worker.join().unwrap();
        let child_out = String::from_utf8_lossy(&child.stdout);
        assert!(
            child_out.contains("force answered: Ok(State { state: Unloaded"),
            "{child_out}\n{}",
            String::from_utf8_lossy(&child.stderr)
        );
        assert!(
            matches!(
                directed.as_slice(),
                [
                    weaver_types::Payload::Directive(LifecycleDirective::Observe),
                    weaver_types::Payload::Directive(LifecycleDirective::JoinLeave { .. }),
                    weaver_types::Payload::Directive(LifecycleDirective::Leave {
                        forced: false,
                        ..
                    }),
                    weaver_types::Payload::Directive(LifecycleDirective::JoinLeave { .. }),
                ]
            ),
            "the force asked again and joined the pending leave: {directed:?}"
        );
        assert!(is_unloaded(&answered), "{answered:?}");
        assert_eq!(published, 1, "the graceful unload published, once");
        assert_eq!(
            save_points::read_marker(&config.root),
            Some(save_points::Marker::Closed { run: "r-1".into() })
        );
    }

    /// **A force with no unload running takes the lock and does everything**,
    /// per Spec section 3 on the operator's ruling of 2026-10-07 on #1: it
    /// observes, directs the leave forced, publishes the reported save point
    /// and closes the marker clean, state having been kept; it never sends a
    /// join. Perturbation: send the join whatever the lock says and the
    /// worker's first directive is a `JoinLeave`.
    #[test]
    fn a_force_with_no_unload_running_takes_the_lock_and_does_everything() {
        let (config, _scratch) = scratch_config("force-alone");
        save_points::write_marker(
            &config.root,
            Some(&save_points::Marker::Open { run: "r-1".into() }),
        )
        .unwrap();
        let holder = stand_in_holder(&config);
        let worker = ending_worker(
            &config,
            vec![state(weaver_types::AgentState::Idle), left_with_report()],
            None,
            holder,
        );
        let calls = std::cell::Cell::new(0);
        let answered = unload_with(
            &config,
            TEST_UNLOAD_BOUNDS,
            true,
            &mut counting_publisher(&calls),
        );
        let (directed, ended) = worker.join().unwrap();
        assert!(
            matches!(
                directed.as_slice(),
                [
                    weaver_types::Payload::Directive(LifecycleDirective::Observe),
                    weaver_types::Payload::Directive(LifecycleDirective::Leave {
                        forced: true,
                        ..
                    }),
                ]
            ),
            "{directed:?}"
        );
        assert!(ended);
        assert!(is_unloaded(&answered), "{answered:?}");
        assert_eq!(calls.get(), 1);
        assert_eq!(
            save_points::read_marker(&config.root),
            Some(save_points::Marker::Closed { run: "r-1".into() })
        );
    }

    /// **A force while a `show` holds the lock waits and unloads alone**,
    /// per Spec section 3 on the operator's ruling of 2026-10-07 on #1:
    /// another process holds the lock shared past `show`'s wait; the force's
    /// take refuses, its join reaches the harness, which answers
    /// `OutOfOrder` with no leave pending; the force reads that as a holder
    /// that is no unload, waits for the lock, and then observes, leaves
    /// forced, publishes and closes the marker itself. Perturbation: read any
    /// answer to the join as joined and the force answers unloaded with the
    /// run still standing, the worker never directed to leave.
    #[test]
    fn a_force_while_a_show_holds_the_lock_waits_and_unloads_alone() {
        let (config, _scratch) = scratch_config("force-behind-show");
        save_points::write_marker(
            &config.root,
            Some(&save_points::Marker::Open { run: "r-1".into() }),
        )
        .unwrap();
        let holder = stand_in_holder(&config);
        let worker = ending_worker(
            &config,
            vec![state(weaver_types::AgentState::Idle), left_with_report()],
            Some(weaver_types::Payload::Refusal(LifecycleRefusal::OutOfOrder)),
            holder,
        );
        // The lock file stands before the reader opens it, as after any verb.
        drop(start::take_invocation_lock(&config.run_directory()).unwrap());
        let mut show = std::process::Command::new("python3")
            .args([
                "-c",
                "import fcntl, sys, time\nf = open(sys.argv[1], 'r+')\nfcntl.lockf(f, fcntl.LOCK_SH | fcntl.LOCK_NB)\nprint('held', flush=True)\ntime.sleep(1.6)",
            ])
            .arg(config.run_directory().join("admin.lock"))
            .stdout(std::process::Stdio::piped())
            .spawn()
            .expect("python3 runs");
        let mut line = String::new();
        std::io::BufRead::read_line(
            &mut std::io::BufReader::new(show.stdout.as_mut().unwrap()),
            &mut line,
        )
        .unwrap();
        assert_eq!(line.trim(), "held");
        let calls = std::cell::Cell::new(0);
        let answered = unload_with(
            &config,
            TEST_UNLOAD_BOUNDS,
            true,
            &mut counting_publisher(&calls),
        );
        let _ = show.wait();
        let (directed, ended) = worker.join().unwrap();
        // Every join is refused while the `show` holds the lock, the force
        // asking again each second; then it unloads alone.
        let joins = directed
            .iter()
            .take_while(|payload| {
                matches!(
                    payload,
                    weaver_types::Payload::Directive(LifecycleDirective::JoinLeave { .. })
                )
            })
            .count();
        assert!(joins >= 1, "{directed:?}");
        assert!(
            matches!(
                &directed[joins..],
                [
                    weaver_types::Payload::Directive(LifecycleDirective::Observe),
                    weaver_types::Payload::Directive(LifecycleDirective::Leave {
                        forced: true,
                        ..
                    }),
                ]
            ),
            "{directed:?}"
        );
        assert!(ended, "the run ended under the force");
        assert!(is_unloaded(&answered), "{answered:?}");
        assert_eq!(calls.get(), 1);
        assert_eq!(
            save_points::read_marker(&config.root),
            Some(save_points::Marker::Closed { run: "r-1".into() })
        );
    }

    /// **An unreachable harness is escalated by the graceful holder**, per
    /// Spec section 3 on the operator's ruling of 2026-10-07 on #1: the
    /// worker takes the leave and goes away without answering; the graceful
    /// unload, holding the lock, reads the end of the connection and
    /// escalates at once, ending what holds the run lock, so nothing waits on
    /// a dead process; the marker stays open for the next load's reset.
    /// Perturbation: answer unloaded on the unanswered leave without the
    /// escalation and the holder survives.
    #[test]
    fn an_unreachable_harness_is_escalated_by_the_graceful_holder() {
        let (config, _scratch) = scratch_config("graceful-escalates");
        save_points::write_marker(
            &config.root,
            Some(&save_points::Marker::Open { run: "r-1".into() }),
        )
        .unwrap();
        let holder = stand_in_holder(&config);
        let worker = ending_worker(
            &config,
            vec![state(weaver_types::AgentState::Active), None],
            None,
            holder,
        );
        let calls = std::cell::Cell::new(0);
        let answered = unload_with(
            &config,
            TEST_UNLOAD_BOUNDS,
            false,
            &mut counting_publisher(&calls),
        );
        let (directed, ended) = worker.join().unwrap();
        assert_eq!(directed.len(), 2, "{directed:?}");
        assert!(ended, "the escalation ended the run");
        assert!(is_unloaded(&answered), "{answered:?}");
        assert_eq!(calls.get(), 0);
        assert_eq!(
            save_points::read_marker(&config.root),
            Some(save_points::Marker::Open { run: "r-1".into() })
        );
    }

    /// **A reported save point already in the manifest is not published
    /// again**, per Spec section 3: the conclusion's defence for an unload
    /// retried after its publication landed; the standing line counts as
    /// published and the marker closes clean. Perturbation: drop the
    /// manifest read and the publication runs a second time.
    #[test]
    fn a_save_point_already_in_the_manifest_is_not_published_again() {
        let (config, _scratch, mine) = conclusion_scratch("conclude-standing");
        let calls = std::cell::Cell::new(0);
        let mut publisher = appending_publisher(&config, mine, &calls);
        publisher(&[(report(), save_points::Arrival::Leave)]).expect("the first publication");
        let answered = conclude_left(&config, Some(report()), false, mine, &mut publisher);
        assert!(is_unloaded(&answered), "{answered:?}");
        assert_eq!(calls.get(), 1, "one publication");
        assert_eq!(manifest_digests(&config, mine), ["ab"]);
        assert_eq!(
            save_points::read_marker(&config.root),
            Some(save_points::Marker::Closed { run: "r-1".into() })
        );
    }

    /// **The marker is restored by the rollback**, per Spec section 4 on
    /// A3.0 item 5: a load that wrote the marker open and then failed puts
    /// back what stood before, a closed marker or none. Perturbation: skip
    /// the marker in `roll_back` and the open marker survives the failure.
    #[test]
    fn the_rollback_restores_the_marker_it_found() {
        let (config, _scratch) = scratch_config("marker-rollback");
        let closed = save_points::Marker::Closed { run: "r-0".into() };
        save_points::write_marker(&config.root, Some(&closed)).unwrap();
        let mut standing = Standing {
            marker_before: Some(Some(closed.clone())),
            ..Standing::default()
        };
        save_points::write_marker(
            &config.root,
            Some(&save_points::Marker::Open { run: "r-1".into() }),
        )
        .unwrap();
        let account = roll_back(&config, &mut standing);
        assert!(account.contains("marker restored"), "{account}");
        assert_eq!(save_points::read_marker(&config.root), Some(closed.clone()));
        let mut standing = Standing {
            marker_before: Some(None),
            ..Standing::default()
        };
        save_points::write_marker(
            &config.root,
            Some(&save_points::Marker::Open { run: "r-1".into() }),
        )
        .unwrap();
        roll_back(&config, &mut standing);
        assert_eq!(save_points::read_marker(&config.root), None);
        // **What stood before is recorded ahead of the write** (Codex on
        // #94, round 4): the root made unwritable, the open marker does not
        // write, the load refuses, and the standing still carries the closed
        // marker it found, so the rollback puts it back once the root
        // writes. Perturbation: record `marker_before` after the write and
        // the refused write records nothing.
        save_points::write_marker(&config.root, Some(&closed)).unwrap();
        let mut standing = Standing::default();
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&config.root, std::fs::Permissions::from_mode(0o500)).unwrap();
        }
        let refused = open_marker(&config.root, &mut standing, "r-1");
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&config.root, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        assert!(matches!(refused, Err(LifecycleRefusal::BoundaryUnverified)));
        assert_eq!(standing.marker_before, Some(Some(closed.clone())));
        save_points::write_marker(
            &config.root,
            Some(&save_points::Marker::Open { run: "r-1".into() }),
        )
        .unwrap();
        roll_back(&config, &mut standing);
        assert_eq!(save_points::read_marker(&config.root), Some(closed));
    }

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
                    weaver_types::Payload::Answer(LifecycleAnswer::Left { save_point: None }),
                ],
            );
            assert_eq!(
                unload_within(&config, TEST_UNLOAD_BOUNDS, false),
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
            unload_within(&config, TEST_UNLOAD_BOUNDS, false),
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
                start::close_inherited_except(Some(std::os::fd::AsRawFd::as_raw_fd(&ready_write)));
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
    /// supplementary set that group and the territory's access group alone,
    /// none of root's, the access group riding the drop from the territory as
    /// judged (Codex on #94, round 8). The member's end is read too, a socket
    /// at the fixed number.
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
        let config = weaver_types::parse(&source).expect("the declaration parses");
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
            binding: weaver_types::EnterBinding::Serving { gate_instruction },
            lineage: None,
            member_account: Some(member),
        };
        let mut service = unread_config();
        // The territory's access group as judged, which rides the drop.
        service.access_gid = 4244;
        service.worker = bin.join("weaver-worker");
        let run_directory = sink.join("run");
        std::fs::create_dir_all(&run_directory).unwrap();
        let run_lock = start::take_run_lock(&run_directory)
            .unwrap()
            .expect("a free run lock");

        let harness_end = stand_state_member(&service, &inventory, &run_lock, None);
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
        assert_eq!(
            line("Groups:"),
            "4243 4244",
            "its group and the territory's access group, none of root's"
        );
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
