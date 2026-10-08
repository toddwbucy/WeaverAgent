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
    /// **Set only for `force-unload` whose territory did not judge** (the
    /// operator's ruling of 2026-10-08 on #99, K5): the force still ends the
    /// run, but nothing is written into or read from the unjudged territory,
    /// no `admin.log` line and no publication, and the marker closes forced.
    territory_unjudged: bool,
    /// The operator's uid, the box's own fact about whose data defines the
    /// agent, per section 9: the harness admits the seeding line from it.
    operator: u32,
    /// The access group's gid, the group of the files beneath the territory
    /// (the territory's own is the state group), which the declaration, the
    /// logs and the published save points take so the operator and the connector read
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
    let forced = matches!(request, surface::Request::ForceUnload(_));
    let config = load_service_config(request.agent(), forced).inspect_err(|refusal| {
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
/// section 2. The binary is discovered beside the worker's own. A `None`
/// return is the leg not standing, which the load refuses where the
/// declaration elects a member (Codex on #94), never running without a leg
/// the declaration asked for.
/// The save point's descriptor in the member, per `weaver-state-Spec`
/// section 2: a fixed convention between this crate and the member.
const SAVE_POINT_FD: std::os::fd::RawFd = 4;

/// `state.log`'s owner: root's, and the member's group, `0640`, so the
/// member reads its own stderr and never writes past root's append.
fn state_log_owner(member: inventory::MemberAccount) -> (u32, u32) {
    (0, member.gid)
}

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
    let territory = prepare_territory(config.territory_fd().ok()?, territory_root, member_account)?;
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
    // could hold the load. Grouped to the member, so the scripts that read
    // it as the member find what it wrote (#99 area 2, K12).
    let log = log::open_append(
        &territory.join("state.log"),
        Some(state_log_owner(member_account)),
    );
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
/// the agent's territory, which the sink stands in, made if absent and
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
/// other: the territory, `0710` under the state group, denies it the search
/// bit, which section 4 verified before this ran, and this directory grants
/// it nothing through owner, group, or other.
///
/// **A failure here is the leg not standing**, which the load then refuses
/// where a member is elected: a member spawned into a room it cannot write
/// is worse than none.
///
/// conforms: admin-member-territory-is-the-members-own
fn prepare_territory(
    territory_fd: std::os::fd::BorrowedFd<'_>,
    root: &std::path::Path,
    member: inventory::MemberAccount,
) -> Option<std::path::PathBuf> {
    use std::os::fd::AsFd;
    // **Made, opened and handed over through the territory's descriptor**
    // (the custody audit's G13), per Spec section 9: the room is created
    // beneath the descriptor the judgment holds, at the mode it ends with so
    // it never stands wider, then opened without following a link and given
    // its owner and mode on that descriptor, so a link or a file at `state`
    // is refused and never has its target's owner or mode changed.
    match nix::sys::stat::mkdirat(
        territory_fd,
        save_points::ROOM,
        nix::sys::stat::Mode::from_bits_truncate(0o700),
    ) {
        Ok(()) | Err(nix::errno::Errno::EEXIST) => {}
        Err(_) => return None,
    }
    let room = nix::fcntl::openat(
        territory_fd,
        save_points::ROOM,
        nix::fcntl::OFlag::O_RDONLY
            | nix::fcntl::OFlag::O_DIRECTORY
            | nix::fcntl::OFlag::O_NOFOLLOW
            | nix::fcntl::OFlag::O_CLOEXEC,
        nix::sys::stat::Mode::empty(),
    )
    .ok()?;
    nix::unistd::fchown(
        room.as_fd(),
        Some(nix::unistd::Uid::from_raw(member.uid)),
        Some(nix::unistd::Gid::from_raw(member.gid)),
    )
    .ok()?;
    nix::sys::stat::fchmod(
        room.as_fd(),
        nix::sys::stat::Mode::from_bits_truncate(0o700),
    )
    .ok()?;
    Some(root.join(save_points::ROOM))
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
    inventory::drop_to(member.uid, &member_groups(member))
}

/// **The member's group set is its own group alone**, per Spec section 6 on
/// the operator's ruling of 2026-10-08 on #1 (the custody audit's G11): the
/// territory is `0710` under the state group, the member's own primary group,
/// so the member passes to its room by that group alone, and the access group, which
/// reads `admin.log`, `worker.log` and the published save points, is not
/// the member's. `drop_to` sets the supplementary set from this slice alone,
/// never from the account database, so no membership the account carries
/// reaches the member. The trace's group is not among them either, so the
/// member cannot read the record.
fn member_groups(member: inventory::MemberAccount) -> [nix::libc::gid_t; 1] {
    [member.gid as nix::libc::gid_t]
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
    let mut file = open_declaration(config.territory_fd()?, &config.territory, config.access_gid)?;
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
/// **Root's, grouped to the access group, mode `0640` exactly**, on the
/// operator's ruling of 2026-10-08 on #1: the state group passes the
/// territory and the member holds it, so the declaration's own mode is its
/// wall, read by the access group alone and written by no one but root.
fn open_declaration(
    territory: std::os::fd::BorrowedFd<'_>,
    directory: &std::path::Path,
    access_gid: u32,
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
        nix::fcntl::OFlag::O_RDONLY
            | nix::fcntl::OFlag::O_NOFOLLOW
            | nix::fcntl::OFlag::O_NONBLOCK
            | nix::fcntl::OFlag::O_CLOEXEC,
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
    if metadata.gid() != access_gid {
        return Err(refuse("is not grouped to the access group"));
    }
    if metadata.mode() & 0o7777 != 0o640 {
        return Err(refuse(
            "is not mode 0640, root's and read by the access group alone",
        ));
    }
    Ok(file)
}

fn take_inventory(
    config: &ServiceConfig,
    agent: &AgentName,
) -> Result<inventory::Inventory, LifecycleRefusal> {
    judge_reader(&config.require_boundary()?.reader, agent)?;
    room_inventory(config, agent)
}

/// **The inventory a verb that starts nothing derives**, `restore`'s
/// (Codex on #94, at 9fa18b4): the name admitted, the
/// declaration read and the sink held to the territory, and no boundary
/// judgment, which Spec section 9 asks of `validate` and `load` alone;
/// `take_inventory` adds the boundary to this for the two verbs that start a
/// run. The publication reads no declaration and takes no inventory.
fn room_inventory(
    config: &ServiceConfig,
    agent: &AgentName,
) -> Result<inventory::Inventory, LifecycleRefusal> {
    admissible(config, agent)?;
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
        member_group: nix::unistd::Group::from_name(&inventory::member_identity_for(agent))
            .ok()
            .flatten()
            .map(|group| group.gid.as_raw()),
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

/// The leave's own bound, per Spec section 3: 150 seconds from the
/// directive, past the harness's 120 for the save point's answer leg (the
/// operator's ruling of 2026-10-08 on #1) and its two-second legs, so admin
/// never gives up on a save point the harness is still waiting for.
const LEAVE_BOUND: std::time::Duration = std::time::Duration::from_secs(150);

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
    /// The run this load opened the marker for, which the rollback leaves
    /// the marker open on where the entered run did not leave.
    run_reference: Option<String>,
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
    // **The marker is read before anything stands, and a marker that does
    // not read refuses the load** (the custody audit's G8): it is the reset
    // the enter carries, and read as absent it would load a run left open
    // without its `NoCleanUnload`.
    let prior_marker = read_marker_or_refuse(config)?;
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
    let selected = publish_then_select(config, agent, &inventory)?;
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
    // Held before the enter goes, so a rollback after it names the run
    // whatever the answer was (K1).
    standing.run_reference = Some(run_reference.0.clone());
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
                reset: save_points::reset_from(prior_marker.as_ref()),
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
    standing.marker_before = Some(save_points::load_marker(root).map_err(|why| {
        diag!(
            "weaver-admin: the clean-unload marker in {}: {why}",
            root.display()
        );
        LifecycleRefusal::BoundaryUnverified
    })?);
    standing.run_reference = Some(run.to_string());
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

/// **The load's publication, its gate, then its selection, in that order**
/// (the #99 area 1 review, T2): the room is published under the run lock,
/// a publication that left any file refuses the load before anything is
/// selected, and only then is the save point selected, per Spec section 4:
/// the one `restore` names or the latest the manifest names, judged through
/// the descriptor the member will inherit; no member elected selects nothing,
/// and a restore named beside the `none` engine refuses.
fn publish_then_select(
    config: &ServiceConfig,
    agent: &AgentName,
    inventory: &inventory::Inventory,
) -> Result<Option<save_points::Selected>, LifecycleRefusal> {
    let (_, deferred) = publish_from_room_noting(config, agent, &[])?;
    load_may_select(config, deferred)?;
    select_save_point(config, inventory)
}

/// **A load selects only once the room is published whole** (Codex on #94,
/// at ab8acef): where the publication left room files for a later verb, past
/// the cap or the free space, a newer save point than the manifest's latest
/// may wait in the room, and a load that selected now would restore stale
/// state and let the next save point outrank the newer one waiting. It
/// refuses `BoundaryUnverified` instead, saying so; each load publishes up to
/// the cap more, so the room drains and a later load stands.
fn load_may_select(config: &ServiceConfig, deferred: bool) -> Result<(), LifecycleRefusal> {
    if !deferred {
        return Ok(());
    }
    diag!(
        "weaver-admin: the room holds save points not yet published; this load published what it could and refuses. A later load continues where the cap or the free space stopped this one; a room file that fails its judgment at every copy stops every load until the operator clears it (the log above names it)"
    );
    record(
        config,
        "load",
        "refused: the room holds save points not yet published; a later load continues, unless a room file the log names fails at every copy",
    );
    Err(LifecycleRefusal::BoundaryUnverified)
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
        config.file_owner(),
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
    // reach this save point, refuses the verb `SavePointNotTaken` naming
    // `published`, as the unload names it (the #94 survey's S12), the file
    // standing in the room for the next verb and said so in the log.
    let published = publish_from_room(
        config,
        agent,
        &[(report.clone(), save_points::Arrival::Demand)],
    );
    if !matches!(&published, Ok(lines) if lines.iter().any(|line| line.digest == report.save_point))
    {
        diag!(
            "weaver-admin: the save point {} was taken and not published; it stands in the member's room for the next verb",
            report.save_point
        );
        return Err(LifecycleRefusal::SavePointNotTaken {
            missed: weaver_types::SavePointLeg::Published,
        });
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
    let inventory = room_inventory(config, agent)?;
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
        config.file_owner(),
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
    // **The run being undone leaves forced** (the #94 survey's S10): it
    // takes no save point, a run rolled back having nothing to keep, and
    // its `unload` says forced.
    let leave = standing
        .entered
        .then(|| direct_leave_within(config, std::time::Instant::now() + LEAVE_BOUND, true));
    let left = matches!(leave, Some(Ok(_)));
    // **A run whose `load` is on the trace leaves the marker open on it**
    // (the #94 survey's S10, and the operator's ruling of 2026-10-08 on #99,
    // K1), whether or not this crate read its `Ready`: an enter refused
    // after `load`, or a `Ready` past the load bound, authored `load` all
    // the same, and the next load must record the reset. The rollback's
    // forced leave says which: `OutOfOrder` is a harness never entered, so
    // no `load`, and the marker stays as it was found; anything else, `Left`
    // or no answer, is a run that may have authored `load`, so the marker
    // names it, a conservative reset rather than a false clean one.
    let load_on_trace = match &leave {
        None => false,
        Some(Err(LeaveFault::Refused(LifecycleRefusal::OutOfOrder))) => false,
        Some(_) => true,
    };
    let marker = match (&standing.run_reference, load_on_trace) {
        (Some(run), true) => Some(Some(save_points::Marker::Open { run: run.clone() })),
        _ => standing.marker_before.take(),
    };
    if let Some(marker) = marker {
        let written = save_points::write_marker(&config.root, marker.as_ref()).is_ok();
        account.push(format!(
            "marker {}",
            match (load_on_trace, written) {
                (true, true) => "left open on the run",
                (false, true) => "restored",
                (_, false) => "not restored",
            }
        ));
    }
    if standing.entered {
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

/// Spec section 3's values: the leave's 150 seconds, thirty after `left`,
/// ten from `SIGTERM` to `SIGKILL` and five for the last read, at most 195
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
    let run_directory = config.run_directory();
    let _invocation = start::take_invocation_lock(&run_directory)?;
    // **The leave's budget runs from here**, per Spec section 3: the
    // observation and both dials spend it, so the verb holds the invocation
    // lock at most the leave's 150 seconds and the escalation's forty-five.
    let leave_deadline = std::time::Instant::now() + bounds.leave;
    if !start::run_lock_held(&run_directory)? {
        return conclude_ended(config, forced);
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
        match direct_leave_within(config, leave_deadline, forced) {
            Ok(report) => {
                // **A leave whose lock outlives the after-left wait keeps its
                // save point** (the #94 survey's S7): the escalation ends the
                // holders, and the report is published as on a lock that
                // freed, so `Unloaded` is never answered over a leave save
                // point left unpublished in the room.
                if !start::wait_free(&run_directory, bounds.after_left) {
                    start::escalate_within(&run_directory, bounds.term, bounds.kill)?;
                }
                return conclude_left(config, report, forced);
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
                // **Only a refusal that can follow `Left` is waited on** (the
                // #99 area 1 review, K3): `ActivityNotAtRest` and
                // `OutOfOrder` are answered with the run plainly standing or
                // never entered, so waiting the after-left wait on them only
                // holds the invocation lock and answers late.
                let can_follow_left = !matches!(
                    refusal,
                    LifecycleRefusal::ActivityNotAtRest | LifecycleRefusal::OutOfOrder
                );
                if forced && can_follow_left && start::wait_free(&run_directory, bounds.after_left)
                {
                    close_marker(config, true)?;
                }
                return Err(refusal);
            }
            // The leave went unanswered inside its bound: a worker that
            // would not exit, so the escalation follows.
            Err(LeaveFault::Unanswered) => {}
        }
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
    Ok(unloaded_answer())
}

/// The answer an unload gives where the run has ended.
fn unloaded_answer() -> LifecycleAnswer {
    LifecycleAnswer::State {
        state: weaver_types::AgentState::Unloaded,
        load: None,
        constituents: Vec::new(),
    }
}

/// **The member has stopped: publish, then close the marker**, per Spec
/// sections 3 and 6: the leave's save point carries its event's position,
/// anything else the room still held is recovered, and the marker closes on a
/// clean unload or stays open under `ForcedUnload`. **The unload does not
/// complete without its save point published** (Codex on #94, round 9), per
/// Spec section 3 on A3.0 item 6: the leave's reported digest must be among
/// the lines this publication appended, or the marker stays open, so the next
/// load records `NoCleanUnload` and recovers the room's file or names it as
/// unpublishable, and the verb refuses naming the publication rather than
/// answering Unloaded over a stale restore. A forced unload reports no save
/// point and is unchanged. **The marker closes clean only for this run's own
/// save point** (the push review of 197e80b): the report's `event_run` must
/// be the run the marker stands open on, the reference admin minted, or the
/// marker stays open and the next load records the reset. The report's
/// covered `run` is not compared, since a run restored and left with no turn
/// covers the prior run's position.
fn conclude_left(
    config: &ServiceConfig,
    report: Option<weaver_types::SavePointReport>,
    forced: bool,
) -> Result<LifecycleAnswer, LifecycleRefusal> {
    let reports: Vec<_> = report
        .into_iter()
        .map(|report| (report, save_points::Arrival::Leave))
        .collect();
    let published = publish_from_room(config, &AgentName(config.agent.clone()), &reports);
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
    if let Some((report, _)) = reports.first() {
        let open = open_run(config)?;
        if open.as_deref() != Some(report.event_run.0.as_str()) {
            diag!(
                "weaver-admin: the leave reported a save point of run {}, not the run the marker stands open on; the marker stays open",
                report.event_run.0
            );
            record(
                config,
                "unload",
                "the leave's save point is not this run's; the marker stays open",
            );
            return Ok(unloaded_answer());
        }
    }
    close_marker(config, forced)?;
    Ok(unloaded_answer())
}

/// The run the clean-unload marker stands open on, or none where it stands
/// otherwise.
fn open_run(config: &ServiceConfig) -> Result<Option<String>, LifecycleRefusal> {
    Ok(match read_marker_or_refuse(config)? {
        Some(save_points::Marker::Open { run }) => Some(run),
        _ => None,
    })
}

/// **An unload that finds the run already ended publishes the room first**
/// (the #94 survey's S11), per Spec section 3: after an unload refused
/// `SavePointNotTaken` naming `published`, the leave's save point stands in
/// the room and the operator retries, plainly or forced. A publication that
/// refuses or leaves a file refuses `published` again. **The marker never
/// closes clean here** (Codex on #94 at 197e80b), on the rule that nothing
/// is lost silently, a `Closed` marker meaning a save point of this run's
/// state published: with the run gone, nothing tells a leave whose
/// publication failed from a run that crashed with an on-demand save point
/// in its room, and a published file proves no leave. So a forced verb closes the marker as forced, the
/// operator's choice (Codex on #94, round 6), and an unforced one closes
/// nothing, the next load recording `NoCleanUnload`: a conservative label,
/// never a false one. A retry after `published` may so record a reset over
/// a leave that did take its save point; the leave's provenance in the
/// marker is the lifecycle act's.
fn conclude_ended(
    config: &ServiceConfig,
    forced: bool,
) -> Result<LifecycleAnswer, LifecycleRefusal> {
    let not_published = || {
        record(
            config,
            "unload",
            "refused: the room's save points did not publish",
        );
        LifecycleRefusal::SavePointNotTaken {
            missed: weaver_types::SavePointLeg::Published,
        }
    };
    let (_, deferred) = publish_from_room_noting(config, &AgentName(config.agent.clone()), &[])
        .map_err(|_| not_published())?;
    if deferred {
        return Err(not_published());
    }
    if forced {
        close_marker(config, true)?;
    }
    Ok(unloaded_answer())
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

/// **Publish the member's finished save points into the territory's
/// save-points directory**, per Spec section 6, from the room the territory
/// holds: the member's uid from the account database by its derived name and
/// the room through the territory's descriptor, the declaration never read.
/// The lines appended are logged.
fn publish_from_room(
    config: &ServiceConfig,
    agent: &AgentName,
    reports: &[(weaver_types::SavePointReport, save_points::Arrival)],
) -> Result<Vec<save_points::ManifestLine>, LifecycleRefusal> {
    publish_from_room_noting(config, agent, reports).map(|(lines, _)| lines)
}

/// `publish_from_room`, answering besides the lines whether the room still
/// holds save points the publication left for a later verb, past the cap or
/// the free space (Codex on #94, at ab8acef).
fn publish_from_room_noting(
    config: &ServiceConfig,
    agent: &AgentName,
    reports: &[(weaver_types::SavePointReport, save_points::Arrival)],
) -> Result<(Vec<save_points::ManifestLine>, bool), LifecycleRefusal> {
    // **The publication needs the member's uid and the room, and never the
    // declaration** (the #94 survey's S8): it runs after a run has ended,
    // at an unload and a `save-point`, where a declaration saved invalid
    // while the run stood, or a box step swapping a binary, would otherwise
    // strand the leave's save point. The member's uid is the account
    // database's, by the derived name; a member stands where its room does,
    // a `none` agent having none.
    admissible(config, agent)?;
    // **A force over an unjudged territory publishes nothing** (K5): the
    // room's files wait for a load, which judges the territory first.
    if config.territory_unjudged {
        return Ok((Vec::new(), false));
    }
    #[cfg(test)]
    if let Some((member, owner)) = TEST_PUBLICATION.with(std::cell::Cell::get) {
        return publish_room_as(config, member, owner, reports);
    }
    let member = match nix::unistd::User::from_name(&inventory::member_identity_for(agent)) {
        Ok(Some(user)) => user.uid.as_raw(),
        Ok(None) => return Ok((Vec::new(), false)),
        Err(e) => {
            diag!("weaver-admin: the member's account does not read ({e}); nothing is published");
            record(
                config,
                "publish",
                "refused: the member's account does not read",
            );
            return Err(LifecycleRefusal::BoundaryUnverified);
        }
    };
    publish_room_as(config, member, save_points::ROOT, reports)
}

// The member and manifest owner a test's publication runs as, this box
// holding no agent accounts; production reads the account database.
#[cfg(test)]
thread_local! {
    static TEST_PUBLICATION: std::cell::Cell<Option<(u32, save_points::Owner)>> =
        const { std::cell::Cell::new(None) };
}

/// The publication from the territory's room as the member `member`, the
/// uid production reads from the account database, with the manifest
/// `owner`'s, root's in production; a test sets both to its own.
fn publish_room_as(
    config: &ServiceConfig,
    member: u32,
    owner: save_points::Owner,
    reports: &[(weaver_types::SavePointReport, save_points::Arrival)],
) -> Result<(Vec<save_points::ManifestLine>, bool), LifecycleRefusal> {
    // The room is the territory's own entry, opened through the territory's
    // descriptor.
    let room = match config.territory_fd().and_then(save_points::open_room) {
        Ok(Some(room)) => room,
        Ok(None) => return Ok((Vec::new(), false)),
        Err(refusal) => {
            record(
                config,
                "publish",
                &format!("refused: {}", surface::render_refusal(&refusal)),
            );
            return Err(refusal);
        }
    };
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
    match save_points::publish_noting_deferral(
        std::os::fd::AsFd::as_fd(&room),
        member,
        directory,
        config.file_owner(),
        owner,
        reports,
    ) {
        Ok((lines, deferred)) => {
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
            Ok((lines, deferred))
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

/// **The marker, or the verb's refusal** (the custody audit's G8): a marker
/// that stands and does not read refuses `BoundaryUnverified` naming it, at
/// the load and at the unload alike, never read as no marker.
fn read_marker_or_refuse(
    config: &ServiceConfig,
) -> Result<Option<save_points::Marker>, LifecycleRefusal> {
    save_points::load_marker(&config.root).map_err(|why| {
        diag!(
            "weaver-admin: the clean-unload marker in {}: {why}",
            config.root.display()
        );
        record(config, "marker", &format!("does not read: {why}"));
        LifecycleRefusal::BoundaryUnverified
    })
}

/// Close the marker on a clean unload, or leave it open under `ForcedUnload`
/// where the leave was forced, per Spec section 4.
fn close_marker(config: &ServiceConfig, forced: bool) -> Result<(), LifecycleRefusal> {
    let run = match read_marker_or_refuse(config)? {
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
    Unanswered,
}

/// Directs leave, forced or not, and waits for its answer until `deadline`,
/// the dial spending nothing of the bound. **The answer names the leave's
/// save point**, per `weaver-admin-harness-contract` section 3 as of A3.2,
/// none where the leave was forced or the binding diagnostic, so the
/// publication that follows carries the event's position.
fn direct_leave_within(
    config: &ServiceConfig,
    deadline: std::time::Instant,
    forced: bool,
) -> Result<Option<weaver_types::SavePointReport>, LeaveFault> {
    let Ok(mut coordination) = channel::dial(&config.coordination_socket()) else {
        return Err(LeaveFault::Unanswered);
    };
    let ordinal = coordination.next_ordinal();
    coordination
        .send_directive(
            ordinal,
            LifecycleDirective::Leave {
                cause: invocation_cause(),
                forced,
            },
        )
        .map_err(|_| LeaveFault::Unanswered)?;
    match coordination.recv_within(deadline.saturating_duration_since(std::time::Instant::now())) {
        Ok(answer) => match answer.payload {
            weaver_types::Payload::Answer(LifecycleAnswer::Left { save_point }) => Ok(save_point),
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
    // An unjudged territory is written nothing (K5): the line goes to
    // standard error alone.
    if config.territory_unjudged {
        diag!(
            "weaver-admin: {verb}: {outcome} (the territory did not judge; admin.log not written)"
        );
        return;
    }
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
fn load_service_config(agent: &AgentName, forced: bool) -> Result<ServiceConfig, LifecycleRefusal> {
    if !well_formed(&agent.0) {
        return Err(LifecycleRefusal::NoSuchAgent);
    }
    let base = std::env::var_os("WEAVER_ADMIN_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_BASE));
    // **The access group and the state group are resolved by name** (Codex
    // on #94, and the operator's ruling of 2026-10-08 on #1), as the trace
    // door and the reader's judgment resolve them: the territory's group is
    // judged against the state group and `save-points/`'s against the access
    // group, never taken as they stand.
    let group_name = format!("{}-admin", inventory::identity_for(agent));
    let state_name = inventory::member_identity_for(agent);
    let access = nix::unistd::Group::from_name(&group_name).ok().flatten();
    let state = nix::unistd::Group::from_name(&state_name).ok().flatten();
    let resolved = match (access, state) {
        (Some(access), Some(state)) => TerritoryGroups {
            access: access.gid.as_raw(),
            state: state.gid.as_raw(),
        }
        .distinct(),
        (None, _) => Err(format!("the access group {group_name} is not provisioned")),
        (_, None) => Err(format!("the state group {state_name} is not provisioned")),
    };
    let groups = match resolved {
        Ok(groups) => Some(groups),
        Err(cause) => {
            diag!("weaver-admin: {cause}");
            // **A force does not wait on the territory** (K5): without its
            // groups the territory cannot be judged, and the force goes on
            // without it.
            if !forced {
                return Err(LifecycleRefusal::BoundaryUnverified);
            }
            None
        }
    };
    load_service_config_judged(&base, &agent.0, 0, groups, forced)
}

/// **The territory's two groups**, on the operator's ruling of 2026-10-08 on
/// #1: the state group `weaver-<agent>-state`, the member's primary group,
/// which the territory is grouped to for passage, and the access group
/// `weaver-<agent>-admin`, which the files beneath it are grouped to for
/// reading. The agent's own uid holds neither.
#[derive(Debug, Clone, Copy)]
struct TerritoryGroups {
    access: u32,
    state: u32,
}

impl TerritoryGroups {
    /// **The two groups are two, and neither is root's** (#99 area 2, H2):
    /// one gid for both would let the member read the declaration, the logs
    /// and every published save point, every judgment still passing.
    fn distinct(self) -> Result<Self, String> {
        if self.access == self.state || self.access == 0 || self.state == 0 {
            return Err(format!(
                "the access group (gid {}) and the state group (gid {}) are not two \
                 groups apart from root's",
                self.access, self.state
            ));
        }
        Ok(self)
    }
}

/// The judgments under one group for both, as the suite's own uid holds no
/// second group it can rely on; a test of the two apart calls
/// `load_service_config_with`.
#[cfg(test)]
fn load_service_config_at(
    base: &std::path::Path,
    agent: &str,
    owner: u32,
    access_gid: u32,
) -> Result<ServiceConfig, LifecycleRefusal> {
    load_service_config_with(
        base,
        agent,
        owner,
        TerritoryGroups {
            access: access_gid,
            state: access_gid,
        },
    )
}

/// The judgments in Spec section 9's order, against `owner` for the root's
/// files, which production fixes at uid 0 and a test sets to its own uid: the
/// root admitted and closed, its ancestors closed, its entries closed, its
/// values read, then the agent's territory judged against `groups`, which
/// production resolves by name and a test sets to its own, and
/// `library-path` judged as the root is.
#[cfg(test)]
fn load_service_config_with(
    base: &std::path::Path,
    agent: &str,
    owner: u32,
    groups: TerritoryGroups,
) -> Result<ServiceConfig, LifecycleRefusal> {
    load_service_config_judged(base, agent, owner, Some(groups), false)
}

/// The judgments, with the territory's judgment required except for a
/// `force-unload` (the operator's ruling of 2026-10-08 on #99, K5): there a
/// territory that does not judge, or whose groups did not resolve, leaves
/// the configuration marked `territory_unjudged` rather than refusing, so
/// the force always ends the run. The root and its ancestors are judged for
/// every verb, the force's included: they name what this crate runs.
fn load_service_config_judged(
    base: &std::path::Path,
    agent: &str,
    owner: u32,
    groups: Option<TerritoryGroups>,
    forced: bool,
) -> Result<ServiceConfig, LifecycleRefusal> {
    let root = base.join(agent);
    judge_root(&root, owner)?;
    let root = judge_ancestors(&root, &[owner, 0])?;
    judge_entries(&root, owner)?;
    refuse_retired_keys(&root, agent)?;
    let mut config = load_service_config_from(&root, agent).map_err(|failure| {
        diag!("weaver-admin: {failure}");
        // A value of the root's failing names no field; the boundary file is
        // named where it is required, at `validate` and `load`.
        LifecycleRefusal::ConfigInvalid { field: None }
    })?;
    match groups
        .ok_or(LifecycleRefusal::BoundaryUnverified)
        .and_then(|groups| judge_territory(&config.territory, groups))
    {
        Ok(judged) => {
            config.territory = judged.canonical;
            config.access_gid = judged.access_gid;
            config.territory_fd = Some(judged.territory);
            config.save_points = Some(judged.save_points);
        }
        Err(_) if forced => {
            diag!(
                "weaver-admin: the territory {} does not judge; the force ends the run without it, publishing nothing, and the marker closes forced",
                config.territory.display()
            );
            config.territory_unjudged = true;
        }
        Err(refusal) => return Err(refusal),
    }
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

/// **The root keys of layouts this build no longer reads**, each refused by
/// name (the operator's ruling of 2026-10-08 on #1: an agent of an older
/// layout is recreated, never migrated), with the layout each belonged to.
const RETIRED_ROOT_KEYS: [(&str, &str); 7] = [
    (
        "declaration-directory",
        "the layout of before 2026-10-07, the declaration in the operator's directory",
    ),
    (
        "agent.toml",
        "the layout of before #50, the declaration in the root",
    ),
    ("run-tool", "the layout of before #50, under systemd"),
    ("control-tool", "the layout of before #50, under systemd"),
    ("unit-properties", "the layout of before #50, under systemd"),
    ("log-path", "the layout of before #50, under systemd"),
    ("log-directory", "the layout of before #50, under systemd"),
];

/// **A root holding a retired key refuses by its name**, per Spec section
/// 9, for every verb: nothing of an older layout is read or tolerated, and
/// the agent is recreated with `deploy/create-agent.sh` after its take-down.
fn refuse_retired_keys(root: &std::path::Path, agent: &str) -> Result<(), LifecycleRefusal> {
    for (key, layout) in RETIRED_ROOT_KEYS {
        if std::fs::symlink_metadata(root.join(key)).is_ok() {
            diag!(
                "weaver-admin: {agent}'s root holds {key}, a key of {layout}. Nothing \
                 migrates it: recreate {agent} with deploy/create-agent.sh after its \
                 take-down by deploy/HowToDeployANewAgent.md section 7"
            );
            return Err(LifecycleRefusal::ConfigInvalid {
                field: Some(FieldName(key.to_string())),
            });
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

/// **The territory's mode**, `0710`, on the operator's ruling of 2026-10-08
/// on #1, and the one `deploy/create-agent.sh` lays it out at, which
/// `the_territory_create_agent_lays_out_passes_both_judgments` holds equal.
const TERRITORY_MODE: u32 = 0o710;

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
/// exactly and grouped to the state group, judged by name, on the operator's
/// ruling of 2026-10-08 on #1: passage for the state group alone, which the
/// member holds as its primary group and the operator and the connector join,
/// and nothing for the agent's own uid, which holds neither group, so section
/// 4's denial of the sink's directory holds; carrying
/// no access-control entry beyond its mode; every directory above it held
/// closed by root as the root's ancestors are. `save-points/` beneath it is
/// opened through that descriptor and judged the same way at mode `0750` and
/// the access group. The ancestors' walk and the access-control look
/// stay by path, being about the path; everything read after is through the
/// descriptors.
fn judge_territory(
    directory: &std::path::Path,
    groups: TerritoryGroups,
) -> Result<JudgedTerritory, LifecycleRefusal> {
    let access_gid = groups.access;
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
    if metadata.mode() & 0o7777 != TERRITORY_MODE {
        return Err(refuse(
            "is not mode 0710, root's with passage for the state group alone",
        ));
    }
    if carries_access_entries(directory) {
        return Err(refuse("carries an access-control entry beyond its mode"));
    }
    // **Grouped to the state group by name, never taken as it stands**
    // (Codex on #94, and the operator's ruling of 2026-10-08 on #1): its
    // group is who passes, so a territory under any other group would pass
    // a principal the ruling did not name.
    if metadata.gid() != groups.state {
        return Err(refuse("is not grouped to the state group"));
    }
    let canonical = judge_ancestors(directory, &[own, 0])?;
    // **Named by its canonical path** (#99 area 2, H3): the territory is
    // opened by the key and the logs are made at the canonical path, so the
    // two must be one name.
    if canonical != directory {
        return Err(refuse("is not named by its canonical path"));
    }
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
    // **Judged for access-control entries as the territory is** (the custody
    // audit's G7), per Spec section 9's "judged the same way": a default
    // entry on `save-points/` would be inherited by every published copy
    // and the manifest, granting a principal outside the access group read.
    // Looked at on the descriptor, and a look that cannot answer refuses.
    match carries_access_entries_fd(save_points.as_fd()) {
        Ok(false) => {}
        Ok(true) => {
            return Err(refuse(
                "holds a save-points carrying an access-control entry beyond its mode",
            ));
        }
        Err(e) => {
            return Err(refuse(&format!(
                "holds a save-points whose access-control entries cannot be read ({e})"
            )));
        }
    }
    // **The declaration is judged where it is read, not here** (the #94
    // survey's S8): `validate`, `load` and `restore` read it through
    // `read_declaration`, which refuses an absent one `NoSuchAgent` and one
    // that is not root's at the access group's `0640` the provisioning's;
    // `unload`, `force-unload`, `stop`, `show` and `save-point` end or read a
    // run and never parse the declaration, so one saved invalid while the
    // run stood does not strand the run.
    Ok(JudgedTerritory {
        canonical,
        territory: opened,
        save_points,
        access_gid,
    })
}

/// Whether a path carries a POSIX access-control list, access or default,
/// read without following a link.
/// **Whether a directory carries an access-control entry, on its
/// descriptor** (the custody audit's G7): either ACL attribute present is
/// an entry; a filesystem without ACLs, or a directory without the
/// attribute, carries none; any other failure to look is answered as the
/// error, never as none.
fn carries_access_entries_fd(directory: std::os::fd::BorrowedFd<'_>) -> nix::Result<bool> {
    use std::os::fd::AsRawFd;
    for name in [c"system.posix_acl_access", c"system.posix_acl_default"] {
        // SAFETY: a size query with no buffer, on a descriptor this frame
        // borrows and a NUL-terminated name.
        let size = unsafe {
            nix::libc::fgetxattr(
                directory.as_raw_fd(),
                name.as_ptr(),
                std::ptr::null_mut(),
                0,
            )
        };
        if size >= 0 {
            return Ok(true);
        }
        match nix::errno::Errno::last() {
            nix::errno::Errno::ENODATA | nix::errno::Errno::EOPNOTSUPP => {}
            other => return Err(other),
        }
    }
    Ok(false)
}

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
        territory_unjudged: false,
        operator,
        access_gid: 0,
        boundary,
        root: root.to_path_buf(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **`state.log` is root's and the member's group** (#99 area 2, K12):
    /// verify-load and update-stack read it as the member, and a log
    /// `root:root` reads empty to them. Perturbation: answer root's group
    /// and this fails.
    /// **The territory's two groups are two, and neither is gid 0** (#99
    /// area 2, H2). Perturbation: drop either half of the comparison and
    /// this fails.
    #[test]
    fn the_territory_groups_are_two_and_not_roots() {
        let groups = |access, state| TerritoryGroups { access, state }.distinct();
        assert!(groups(1001, 1002).is_ok());
        assert!(groups(1001, 1001).is_err(), "one gid for both refuses");
        assert!(groups(0, 1002).is_err(), "an access group of gid 0 refuses");
        assert!(groups(1001, 0).is_err(), "a state group of gid 0 refuses");
    }

    #[test]
    fn the_state_log_is_grouped_to_the_member() {
        let member = inventory::MemberAccount {
            uid: 4242,
            gid: 4343,
        };
        assert_eq!(state_log_owner(member), (0, 4343));
    }

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

    /// The configuration as `validate`, `load` and `restore` read it: the
    /// territory judged, then the declaration read through it and judged
    /// where it is read (the #94 survey's S8).
    fn load_and_read(
        base: &std::path::Path,
        agent: &str,
        owner: u32,
        access_gid: u32,
    ) -> Result<ServiceConfig, LifecycleRefusal> {
        let config = load_service_config_at(base, agent, owner, access_gid)?;
        read_declaration(&config)?;
        Ok(config)
    }

    /// The values every root carries, written into a scratch root, with the
    /// agent's territory beside it, `<root>.territory`, laid out as the
    /// judgment asks with this test's uid in root's place: mode 0710, its
    /// `save-points/` 0750, and an empty `agent.toml` 0640. Answers the
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
            std::fs::Permissions::from_mode(0o640),
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
                load_service_config_at(&base, "alpha", me, nix::unistd::getegid().as_raw()).err(),
                Some(LifecycleRefusal::BoundaryUnverified),
                "a base of mode {open:o} lets another principal swap the root"
            );
        }
        for closed in [0o755, 0o1777] {
            base_mode(closed);
            assert!(
                load_service_config_at(&base, "alpha", me, nix::unistd::getegid().as_raw()).is_ok(),
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
            load_service_config_at(&deeper, "beta", me, nix::unistd::getegid().as_raw()).err(),
            Some(LifecycleRefusal::BoundaryUnverified),
            "a writable grandparent"
        );
        base_mode(0o755);
        let _ = std::fs::remove_dir_all(&base);
    }

    /// **The territory and `save-points/` are grouped by name** (Codex on
    /// #94), per Spec section 9: the territory's gid is judged against the
    /// state group and `save-points/`'s against the access group, each
    /// resolved by name in production and never taken as it stands.
    /// The territory stands under this test's own gid and the judgment is
    /// given a gid one off, so the case needs no supplementary group and
    /// never skips: one off refuses, its own reads. Perturbation: take the
    /// territory's gid as it stands again and the one-off case reads.
    #[test]
    fn a_territory_under_another_group_than_the_access_group_refuses() {
        use std::os::unix::fs::PermissionsExt;
        let me = nix::unistd::getuid().as_raw();
        let mine = nix::unistd::getegid().as_raw();
        let base =
            std::env::temp_dir().join(format!("weaver-admin-territory-gid-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let root = base.join("alpha");
        write_root(&root);
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(
            load_service_config_at(&base, "alpha", me, mine.wrapping_add(1)).err(),
            Some(LifecycleRefusal::BoundaryUnverified),
            "a territory under another group than the access group refuses"
        );
        let config = load_service_config_at(&base, "alpha", me, mine).expect("its own group reads");
        assert_eq!(config.access_gid, mine);
        // **The territory is the state group's, `save-points/` and the
        // declaration the access group's** (the operator's ruling of
        // 2026-10-08 on #1): judged against two groups apart, the territory
        // under the access group alone refuses. Perturbation: judge the
        // territory against the access group and it reads.
        let apart = TerritoryGroups {
            access: mine,
            state: mine.wrapping_add(1),
        };
        assert_eq!(
            load_service_config_with(&base, "alpha", me, apart).err(),
            Some(LifecycleRefusal::BoundaryUnverified),
            "a territory under the access group rather than the state group refuses"
        );
        let _ = std::fs::remove_dir_all(&base);
        let _ = std::fs::remove_dir_all(base.join("alpha").with_extension("territory"));
    }

    /// **The territory `deploy/create-agent.sh` lays out passes both of
    /// admin's judgments of it** (the #94 survey's S1): `judge_territory`,
    /// which asks the ruled mode and group, and section 4's denial walk,
    /// `agent_can_traverse` on the sink's directory, which asks that the
    /// agent's uid cannot pass. At `0711` the two could never both hold, and
    /// every load refused. The mode and the group are read from the script's
    /// own line, so the script, the judgment and the walk cannot drift apart
    /// again. Perturbation: lay the territory out, or judge it, at `0711`.
    #[test]
    fn the_territory_create_agent_lays_out_passes_both_judgments() {
        use std::os::unix::fs::PermissionsExt;
        let script = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../deploy/create-agent.sh"),
        )
        .unwrap();
        let line = script
            .lines()
            .find(|line| line.starts_with("sudo install -d") && line.ends_with("\"$HOME_DIR\""))
            .expect("create-agent.sh lays the territory out with one install line");
        let words: Vec<&str> = line.split_whitespace().collect();
        let after = |flag: &str| words[words.iter().position(|w| *w == flag).unwrap() + 1];
        assert_eq!(after("-o"), "root", "{line}");
        assert_eq!(after("-g"), "\"$STATE_GROUP\"", "{line}");
        let mode = u32::from_str_radix(after("-m"), 8).unwrap();
        assert!(
            script.contains("STATE_GROUP=\"$MEMBER_USER\""),
            "the state group is the member's own primary group"
        );
        let me = nix::unistd::getuid().as_raw();
        let mine = nix::unistd::getegid().as_raw();
        let base = std::env::temp_dir().join(format!("weaver-admin-laid-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let territory = write_root(&base.join("alpha"));
        std::fs::set_permissions(&territory, std::fs::Permissions::from_mode(mode)).unwrap();
        let groups = TerritoryGroups {
            access: mine,
            state: mine,
        };
        assert!(
            judge_territory(&territory, groups).is_ok(),
            "create-agent's mode {mode:o} passes the territory's judgment"
        );
        // The agent's uid is not the territory's owner and holds neither
        // the state group nor the access group.
        let boundary = inventory::Boundary {
            agent_uid: me.wrapping_add(1),
            admin_uid: me,
            agent_gids: vec![mine.wrapping_add(1)],
            home: territory.clone(),
            member_binary: None,
            member_account: None,
            member_group: None,
        };
        assert!(
            !inventory::agent_can_traverse(&territory, &boundary),
            "create-agent's mode {mode:o} lets the agent's uid pass to the trace"
        );
        assert!(inventory::admin_holds_custody(&territory, &boundary));
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
        let config = load_and_read(&base, "alpha", me, nix::unistd::getegid().as_raw())
            .expect("a declared agent reads");
        assert_eq!(config.agent, "alpha");
        assert_eq!(
            config.territory,
            std::fs::canonicalize(&declarations).unwrap()
        );
        std::fs::remove_file(declarations.join("agent.toml")).unwrap();
        assert_eq!(
            load_and_read(&base, "alpha", me, nix::unistd::getegid().as_raw()).err(),
            Some(LifecycleRefusal::NoSuchAgent),
            "no agent.toml is no agent"
        );
        std::fs::create_dir(declarations.join("agent.toml")).unwrap();
        assert_eq!(
            load_and_read(&base, "alpha", me, nix::unistd::getegid().as_raw()).err(),
            Some(LifecycleRefusal::BoundaryUnverified),
            "a directory named agent.toml"
        );
        std::fs::remove_dir(declarations.join("agent.toml")).unwrap();
        std::os::unix::fs::symlink(declarations.join("gone"), declarations.join("agent.toml"))
            .unwrap();
        assert_eq!(
            load_and_read(&base, "alpha", me, nix::unistd::getegid().as_raw()).err(),
            Some(LifecycleRefusal::BoundaryUnverified),
            "a link named agent.toml"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_root_holding_a_retired_key_refuses_by_its_name() {
        // **Nothing of an older layout is read or tolerated** (the operator's
        // ruling of 2026-10-08 on #1): each retired key refuses
        // `ConfigInvalid` naming it, whatever else the root holds, and the
        // force with it. Perturbation: drop `refuse_retired_keys` from the
        // judgment and a root holding one loads.
        use std::os::unix::fs::PermissionsExt;
        let me = nix::unistd::getuid().as_raw();
        let base =
            std::env::temp_dir().join(format!("weaver-admin-retired-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let root = base.join("alpha");
        write_root(&root);
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
        let gid = nix::unistd::getegid().as_raw();
        assert!(load_and_read(&base, "alpha", me, gid).is_ok());
        for (key, _) in RETIRED_ROOT_KEYS {
            let stale = root.join(key);
            std::fs::write(&stale, "/old\n").unwrap();
            std::fs::set_permissions(&stale, std::fs::Permissions::from_mode(0o644)).unwrap();
            assert_eq!(
                load_and_read(&base, "alpha", me, gid).err(),
                Some(LifecycleRefusal::ConfigInvalid {
                    field: Some(FieldName(key.to_string())),
                }),
                "{key} refuses by its name"
            );
            assert_eq!(
                load_service_config_judged(
                    &base,
                    "alpha",
                    me,
                    Some(TerritoryGroups {
                        access: gid,
                        state: gid
                    }),
                    true
                )
                .err(),
                Some(LifecycleRefusal::ConfigInvalid {
                    field: Some(FieldName(key.to_string())),
                }),
                "{key} refuses the force too"
            );
            std::fs::remove_file(&stale).unwrap();
        }
        assert!(load_and_read(&base, "alpha", me, gid).is_ok());
        let _ = std::fs::remove_dir_all(&base);
    }

    /// **The territory is root's at mode 0710, its `save-points/` 0750 and
    /// its declaration the access group's at 0640, and never a link at its
    /// own name**, per Spec section 9 on the operator's rulings of 2026-10-07
    /// and 2026-10-08 on #1, with this test's uid in root's place: the `0711`
    /// of the first 2026-10-08 ruling refuses now, as does every other mode.
    /// Perturbations: test only the write bits and the 0750 territory reads;
    /// judge 0711 again and the 0710 territory refuses; drop the save-points
    /// judgment and the 0770 one reads; judge the declaration's write bits
    /// alone and the 0644 one reads; drop its group comparison and one under
    /// another group reads.
    #[test]
    fn the_territory_is_roots_at_0710_and_never_a_link() {
        use std::os::unix::fs::PermissionsExt;
        let me = nix::unistd::getuid().as_raw();
        let base = std::env::temp_dir().join(format!("weaver-admin-decl-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let root = base.join("alpha");
        let territory = write_root(&root);
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(load_and_read(&base, "alpha", me, nix::unistd::getegid().as_raw()).is_ok());
        for open in [0o750, 0o711, 0o700, 0o770, 0o1710] {
            std::fs::set_permissions(&territory, std::fs::Permissions::from_mode(open)).unwrap();
            assert_eq!(
                load_and_read(&base, "alpha", me, nix::unistd::getegid().as_raw()).err(),
                Some(LifecycleRefusal::BoundaryUnverified),
                "{open:o} is not the territory's 0710"
            );
        }
        std::fs::set_permissions(&territory, std::fs::Permissions::from_mode(0o710)).unwrap();
        let save_points = territory.join("save-points");
        for open in [0o770, 0o755, 0o700] {
            std::fs::set_permissions(&save_points, std::fs::Permissions::from_mode(open)).unwrap();
            assert_eq!(
                load_and_read(&base, "alpha", me, nix::unistd::getegid().as_raw()).err(),
                Some(LifecycleRefusal::BoundaryUnverified),
                "{open:o} is not save-points' 0750"
            );
        }
        std::fs::set_permissions(&save_points, std::fs::Permissions::from_mode(0o750)).unwrap();
        std::fs::rename(&save_points, territory.join("aside")).unwrap();
        assert_eq!(
            load_and_read(&base, "alpha", me, nix::unistd::getegid().as_raw()).err(),
            Some(LifecycleRefusal::BoundaryUnverified),
            "no save-points directory is the provisioning incomplete"
        );
        std::fs::rename(territory.join("aside"), &save_points).unwrap();
        let declaration = territory.join("agent.toml");
        std::fs::set_permissions(&declaration, std::fs::Permissions::from_mode(0o664)).unwrap();
        assert_eq!(
            load_and_read(&base, "alpha", me, nix::unistd::getegid().as_raw()).err(),
            Some(LifecycleRefusal::BoundaryUnverified),
            "a declaration the group could write"
        );
        // **Root's and the access group's, 0640 exactly** (the operator's
        // ruling of 2026-10-08 on #1): the territory's passage is the state
        // group's, whose member must not read the declaration, so one any uid
        // could read refuses, and one read by the access group alone reads. Perturbation: judge the write
        // bits alone again and the 0644 declaration reads.
        std::fs::set_permissions(&declaration, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(
            load_and_read(&base, "alpha", me, nix::unistd::getegid().as_raw()).err(),
            Some(LifecycleRefusal::BoundaryUnverified),
            "a declaration every uid could read"
        );
        std::fs::set_permissions(&declaration, std::fs::Permissions::from_mode(0o640)).unwrap();
        assert!(load_and_read(&base, "alpha", me, nix::unistd::getegid().as_raw()).is_ok());
        // **Grouped to the access group**: a declaration at 0640 under any
        // other group is read by that group, so it refuses. Set to another
        // of this uid's own groups, and skipped, naming why, where it holds
        // none. Perturbation: drop the group comparison and it reads.
        let mine = nix::unistd::getegid();
        match nix::unistd::getgroups()
            .unwrap_or_default()
            .into_iter()
            .find(|g| *g != mine)
        {
            Some(other) => {
                nix::unistd::chown(&declaration, None, Some(other)).unwrap();
                assert_eq!(
                    load_and_read(&base, "alpha", me, mine.as_raw()).err(),
                    Some(LifecycleRefusal::BoundaryUnverified),
                    "a declaration grouped to another group than the access group"
                );
                nix::unistd::chown(&declaration, None, Some(mine)).unwrap();
            }
            None => eprintln!("skipped the declaration's group case: this uid holds one group"),
        }
        let link = base.join("linked.territory");
        std::os::unix::fs::symlink(&territory, &link).unwrap();
        std::fs::write(root.join("territory"), link.display().to_string()).unwrap();
        assert_eq!(
            load_and_read(&base, "alpha", me, nix::unistd::getegid().as_raw()).err(),
            Some(LifecycleRefusal::BoundaryUnverified),
            "a link at the territory's own name"
        );
        // **A link above the territory's name** (#99 area 2, H3): the open
        // follows it and the logs would be made at the canonical path, so
        // a key that is not canonical refuses. Perturbation: drop the
        // canonical comparison and this loads.
        let via = base.join("via");
        std::os::unix::fs::symlink(territory.parent().unwrap(), &via).unwrap();
        let through = via.join(territory.file_name().unwrap());
        std::fs::write(root.join("territory"), through.display().to_string()).unwrap();
        assert_eq!(
            load_and_read(&base, "alpha", me, nix::unistd::getegid().as_raw()).err(),
            Some(LifecycleRefusal::BoundaryUnverified),
            "a territory named through a link above it"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    /// **`save-points/` is judged for access-control entries as the
    /// territory is** (the custody audit's G7): a default entry granting a
    /// principal outside the access group read refuses the territory. Skips,
    /// naming why, where `setfacl` cannot set the entry. Perturbation: drop
    /// the look and the territory reads.
    #[test]
    fn a_save_points_carrying_an_access_entry_refuses() {
        use std::os::unix::fs::PermissionsExt;
        let me = nix::unistd::getuid().as_raw();
        let base = std::env::temp_dir().join(format!("weaver-admin-acl-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let root = base.join("alpha");
        write_root(&root);
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
        let save_points = root.with_extension("territory").join("save-points");
        let set = std::process::Command::new("setfacl")
            .args(["-d", "-m", "u:nobody:r"])
            .arg(&save_points)
            .status();
        if !set.is_ok_and(|status| status.success()) {
            eprintln!("SKIP: setfacl could not set a default entry here");
            let _ = std::fs::remove_dir_all(&base);
            return;
        }
        assert_eq!(
            load_service_config_at(&base, "alpha", me, nix::unistd::getegid().as_raw()).err(),
            Some(LifecycleRefusal::BoundaryUnverified),
            "an access-control entry on save-points refuses"
        );
        let _ = std::fs::remove_dir_all(&base);
        let _ = std::fs::remove_dir_all(root.with_extension("territory"));
    }

    /// **A load with save points still waiting in the room refuses before it
    /// selects** (Codex on #94, at ab8acef, and the #99 area 1 review, T2):
    /// the load's publication, its gate and its selection run in that order
    /// in `publish_then_select`, and a room file the copy cannot publish, its
    /// stamp sound and its body not, defers, so the load refuses
    /// `BoundaryUnverified` and never reaches the selection; the helper says
    /// nothing deferred goes on. Perturbations: drop the gate from
    /// `publish_then_select` and the deferred load selects nothing and goes
    /// on; ignore the flag in `load_may_select` and the same.
    #[test]
    fn a_load_with_save_points_waiting_in_the_room_refuses() {
        let (config, report, base) = territory_with_one_room_file("load-deferred");
        // The room's file keeps its sound stamp and its name, and its body
        // no longer digests to the name: the scan admits it, every copy
        // refuses it, and the publication defers.
        let room = config.territory.join(save_points::ROOM).join(&report.name);
        let mut bytes = std::fs::read(&room).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 0xff;
        std::fs::write(&room, &bytes).unwrap();
        let declaration = weaver_types::parse(&format!(
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
            config.territory.display()
        ))
        .expect("the declaration parses");
        let gate_instruction = declaration
            .gate_instruction
            .clone()
            .expect("a serving declaration");
        let me = nix::unistd::getuid().as_raw();
        let inventory = inventory::Inventory {
            config: declaration,
            identity: "weaver-alpha".into(),
            declaration: String::new(),
            binding: weaver_types::EnterBinding::Serving { gate_instruction },
            lineage: None,
            member_account: Some(inventory::MemberAccount {
                uid: me,
                gid: nix::unistd::getgid().as_raw(),
            }),
        };
        assert_eq!(
            publish_then_select(&config, &AgentName("alpha".into()), &inventory).err(),
            Some(LifecycleRefusal::BoundaryUnverified),
            "the deferred load refuses before it selects"
        );
        assert!(room.exists(), "the file stays for the operator");
        assert_eq!(load_may_select(&config, false), Ok(()));
        let _ = std::fs::remove_dir_all(&base);
    }

    /// **The publication does not ask the boundary file** (Codex on #94, at
    /// 9fa18b4), per Spec section 9, which asks it of `validate` and `load`
    /// alone: with `roles.toml` gone, the load's inventory refuses naming it,
    /// and the publication an unload runs over a run already gone, and
    /// `restore`, derive the room without it. This box holds no agent
    /// accounts, so the publication stops at the account lookup either way;
    /// what it must never answer is the boundary's refusal. Perturbation:
    /// derive the publication's inventory through `take_inventory` again and
    /// it refuses naming `roles.toml`.
    #[test]
    fn the_publication_does_not_ask_the_boundary_file() {
        use std::os::unix::fs::PermissionsExt;
        let me = nix::unistd::getuid().as_raw();
        let base =
            std::env::temp_dir().join(format!("weaver-admin-room-only-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let root = base.join("alpha");
        write_root(&root);
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::remove_file(root.join("roles.toml")).unwrap();
        let config = load_service_config_at(&base, "alpha", me, nix::unistd::getegid().as_raw())
            .expect("the root reads without its boundary file");
        let agent = AgentName("alpha".into());
        let boundary = LifecycleRefusal::ConfigInvalid {
            field: Some(weaver_types::FieldName("roles.toml".into())),
        };
        assert_eq!(
            take_inventory(&config, &agent).err(),
            Some(boundary.clone()),
            "the load's inventory asks the boundary"
        );
        assert_ne!(
            room_inventory(&config, &agent).err(),
            Some(boundary.clone())
        );
        assert_ne!(
            publish_from_room(&config, &agent, &[]).err(),
            Some(boundary.clone()),
            "the publication never refuses for the boundary"
        );
        assert_ne!(restore(&config, &agent).err(), Some(boundary));
        let _ = std::fs::remove_dir_all(&base);
        let _ = std::fs::remove_dir_all(root.with_extension("territory"));
    }

    /// **The publication never reads the declaration** (the #94 survey's
    /// S8): it runs after a run ended, so a declaration saved unparsable
    /// while the run stood must not strand the leave's save point. With
    /// `agent.toml` holding no TOML, a sound room file publishes through
    /// `publish_from_room`, the entry point the unload and the `save-point`
    /// verb call, its test seam standing in only for the account lookup.
    /// Perturbation: derive the room through `room_inventory` again and it
    /// refuses.
    #[test]
    fn the_publication_does_not_read_the_declaration() {
        use std::os::unix::fs::PermissionsExt;
        let me = nix::unistd::getuid().as_raw();
        let base =
            std::env::temp_dir().join(format!("weaver-admin-no-decl-read-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let root = base.join("alpha");
        let territory = write_root(&root);
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(territory.join("agent.toml"), "this is [ not toml").unwrap();
        let room = territory.join(save_points::ROOM);
        std::fs::create_dir(&room).unwrap();
        let bytes = save_points::tests::save_point("r-1", 1, 0, 1_000_000_000, b"room");
        let digest = save_points::judge(&bytes).unwrap().digest;
        std::fs::write(room.join(format!("{digest}.save-point")), &bytes).unwrap();
        let config = load_service_config_at(&base, "alpha", me, nix::unistd::getegid().as_raw())
            .expect("the root reads");
        let mine = save_points::Owner {
            uid: me,
            gid: nix::unistd::getgid().as_raw(),
        };
        TEST_PUBLICATION.with(|cell| cell.set(Some((me, mine))));
        let lines = publish_from_room(&config, &AgentName("alpha".into()), &[])
            .expect("the room publishes");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].digest, digest);
        assert!(!room.join(format!("{digest}.save-point")).exists());
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
            let config =
                load_service_config_at(&base, "alpha", me, nix::unistd::getegid().as_raw())
                    .unwrap_or_else(|e| panic!("{why}: the root still reads, got {e:?}"));
            assert_eq!(config.boundary_digest(), None, "{why}: no digest to log");
            assert_eq!(config.require_boundary().map(|_| ()), named, "{why}");
        }
        std::fs::write(root.join("roles.toml"), "trace-reader = \"x\"\n").unwrap();
        std::fs::remove_file(root.join("spu-binary")).unwrap();
        assert_eq!(
            load_service_config_at(&base, "alpha", me, nix::unistd::getegid().as_raw()).err(),
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
            load_service_config_at(&base, "alpha", me, nix::unistd::getegid().as_raw()).is_ok(),
            "a closed root reads"
        );
        std::fs::set_permissions(
            root.join("worker-binary"),
            std::fs::Permissions::from_mode(0o666),
        )
        .unwrap();
        assert_eq!(
            load_service_config_at(&base, "alpha", me, nix::unistd::getegid().as_raw()).err(),
            Some(LifecycleRefusal::BoundaryUnverified),
            "a world-writable key"
        );
        let root = fresh();
        std::fs::remove_file(root.join("gate-binary")).unwrap();
        std::fs::write(base.join("elsewhere"), "/opt/weaver/bin/weaver-gate").unwrap();
        std::os::unix::fs::symlink(base.join("elsewhere"), root.join("gate-binary")).unwrap();
        assert_eq!(
            load_service_config_at(&base, "alpha", me, nix::unistd::getegid().as_raw()).err(),
            Some(LifecycleRefusal::BoundaryUnverified),
            "a key that is a link"
        );
        let root = fresh();
        std::fs::create_dir(root.join("stray")).unwrap();
        assert_eq!(
            load_service_config_at(&base, "alpha", me, nix::unistd::getegid().as_raw()).err(),
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
                load_service_config(&AgentName(bad.into()), false).err(),
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
            territory_unjudged: false,
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
                let Ok(raw) = nix::sys::socket::accept(std::os::fd::AsRawFd::as_raw_fd(&listener))
                else {
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
    /// directive, one answer, out of order where no run stands, a refusal
    /// where the publication cannot be made, and the report where it is: with
    /// the reported file in a scratch territory's room, the verb answers
    /// `SavePointTaken` and the manifest holds its line as arrived on demand
    /// at the event's position. Perturbations: drop the run-lock check and
    /// the second case dials an absent worker and answers `Unanswered`
    /// instead; answer the report whatever the publication did and the third
    /// assertion sees `SavePointTaken`; refuse whatever the publication did
    /// and the last case refuses.
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
        // This box holds no member account and the fixture no room, so
        // nothing is published and the verb refuses rather than answering a
        // report of a save point that stands in the room alone (Codex on
        // #94, round 1); the directive reached the worker all the same.
        // A save point taken and not published is named so (the #94
        // survey's S12), as the unload names it.
        let answered = save_point(&config, &agent);
        assert_eq!(
            answered,
            Err(LifecycleRefusal::SavePointNotTaken {
                missed: weaver_types::SavePointLeg::Published,
            }),
            "answers only a published save point"
        );
        let directed = worker.join().unwrap();
        assert!(matches!(
            directed[0],
            weaver_types::Payload::Directive(LifecycleDirective::SavePoint { .. })
        ));
        let _ = holder.kill();
        let _ = holder.wait();
        // The reported file stands in the territory's room: it publishes,
        // and the verb answers the report.
        let (config, report, base) = territory_with_one_room_file("save-point-verb-published");
        let mut holder = stand_in_holder(&config);
        let worker = recording_worker(
            &config,
            vec![weaver_types::Payload::Answer(
                LifecycleAnswer::SavePointTaken {
                    report: report.clone(),
                },
            )],
        );
        let answered = save_point(&config, &agent);
        let _ = worker.join();
        let _ = holder.kill();
        let _ = holder.wait();
        assert_eq!(
            answered,
            Ok(LifecycleAnswer::SavePointTaken {
                report: report.clone()
            })
        );
        let lines = manifest_lines(&config);
        assert_eq!(lines.len(), 1);
        assert_eq!(
            (
                lines[0].digest.as_str(),
                lines[0].arrived,
                lines[0].position.clone()
            ),
            (
                report.save_point.as_str(),
                save_points::Arrival::Demand,
                Some((report.event_run.0.clone(), report.position))
            )
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    /// **A force refused before `Left` answers at once** (the #99 area 1
    /// review, K3): a worker refusing the forced leave `ActivityNotAtRest`,
    /// its run plainly standing, is answered without the after-left wait.
    /// Perturbation: wait on every refusal again and the verb takes the
    /// whole wait.
    #[test]
    fn a_force_refused_before_left_answers_without_the_wait() {
        let (config, _scratch) = scratch_config("forced-not-at-rest");
        let mut holder = stand_in_holder(&config);
        let worker = recording_worker(
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
        let bounds = UnloadBounds {
            after_left: std::time::Duration::from_secs(5),
            ..TEST_UNLOAD_BOUNDS
        };
        let started = std::time::Instant::now();
        let answered = unload_within(&config, bounds, true);
        let took = started.elapsed();
        let _ = worker.join();
        let _ = holder.kill();
        let _ = holder.wait();
        assert_eq!(answered, Err(LifecycleRefusal::ActivityNotAtRest));
        assert!(
            took < std::time::Duration::from_secs(3),
            "answered in {took:?}, inside the five-second wait"
        );
    }

    /// **A force does not depend on the territory** (the operator's ruling of
    /// 2026-10-08 on #99, K5): with the territory at a mode the judgment
    /// refuses, every other verb refuses at the configuration, while the
    /// force's configuration stands marked unjudged; the force over a run
    /// already ended publishes nothing from the room, writes no `admin.log`
    /// line, and closes the marker forced. Perturbation: judge the territory
    /// for the force too and its configuration refuses.
    #[test]
    fn a_force_ends_the_run_where_the_territory_does_not_judge() {
        use std::os::unix::fs::PermissionsExt;
        let me = nix::unistd::getuid().as_raw();
        let mine = nix::unistd::getegid().as_raw();
        let base = std::env::temp_dir().join(format!(
            "weaver-admin-force-unjudged-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&base);
        let root = base.join("alpha");
        let territory = write_root(&root);
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
        let room = territory.join(save_points::ROOM);
        std::fs::create_dir(&room).unwrap();
        let bytes = save_points::tests::save_point("r-1", 1, 0, 1_000_000_000, b"room");
        let digest = save_points::judge(&bytes).unwrap().digest;
        std::fs::write(room.join(format!("{digest}.save-point")), &bytes).unwrap();
        std::fs::set_permissions(&territory, std::fs::Permissions::from_mode(0o711)).unwrap();
        let groups = TerritoryGroups {
            access: mine,
            state: mine,
        };
        assert_eq!(
            load_service_config_judged(&base, "alpha", me, Some(groups), false).err(),
            Some(LifecycleRefusal::BoundaryUnverified),
            "every other verb refuses the drifted territory"
        );
        let mut config = load_service_config_judged(&base, "alpha", me, Some(groups), true)
            .expect("the force's configuration stands");
        assert!(config.territory_unjudged);
        config.coordination_root = base.join("coordination");
        std::fs::create_dir_all(config.run_directory()).unwrap();
        save_points::write_marker(
            &config.root,
            Some(&save_points::Marker::Open { run: "r-1".into() }),
        )
        .unwrap();
        // The publication runs as this uid, so only the unjudged mark keeps
        // it from the room.
        TEST_PUBLICATION.with(|cell| {
            cell.set(Some((
                me,
                save_points::Owner {
                    uid: me,
                    gid: nix::unistd::getgid().as_raw(),
                },
            )))
        });
        assert_eq!(
            unload_within(&config, TEST_UNLOAD_BOUNDS, true),
            Ok(unloaded_answer())
        );
        record(&config, "force-unload", "unloaded");
        assert_eq!(
            save_points::read_marker(&config.root),
            Some(save_points::Marker::Forced { run: "r-1".into() })
        );
        assert!(
            room.join(format!("{digest}.save-point")).exists(),
            "nothing is published from the unjudged territory"
        );
        assert!(
            !territory.join("admin.log").exists(),
            "nothing is written there"
        );
        // Without its groups the force still stands.
        assert!(
            load_service_config_judged(&base, "alpha", me, None, true)
                .expect("stands")
                .territory_unjudged
        );
        let _ = std::fs::remove_dir_all(&base);
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
        // refuses the leave past `Left`, a drain that failed being one such
        // refusal, then goes down inside the after-left wait. A refusal
        // answered before `Left` is never waited on (K3).
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
                weaver_types::Payload::Refusal(LifecycleRefusal::DescriptorsUnusable),
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
        assert!(matches!(
            answered,
            Err(LifecycleRefusal::DescriptorsUnusable)
        ));
        assert_eq!(
            save_points::read_marker(&config.root),
            Some(save_points::Marker::Forced { run: "r-2".into() }),
            "the refusal after the run ended still closed the marker as forced"
        );
    }

    /// **The member's group set is its own group alone**, per Spec section 6
    /// on the operator's ruling of 2026-10-08 on #1 (the custody audit's
    /// G11): the territory is `0710` under the state group, the member's own
    /// primary group, so the member needs no other group to reach its room,
    /// and the access group, which reads the logs and the
    /// published save points, is not the member's. Perturbation: put the
    /// access group back beside the member's own and the assertion fails.
    #[test]
    fn the_members_group_set_carries_no_access_group() {
        let member = inventory::MemberAccount {
            uid: 1501,
            gid: 1501,
        };
        assert_eq!(member_groups(member), [1501]);
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

    /// A configuration over a judged scratch territory whose room holds one
    /// sound finished save point, with a run directory beside it and the
    /// publication run as this test's uid; answers the config, the room
    /// file's report and the base to remove.
    fn territory_with_one_room_file(
        tag: &str,
    ) -> (
        ServiceConfig,
        weaver_types::SavePointReport,
        std::path::PathBuf,
    ) {
        use std::os::unix::fs::PermissionsExt;
        let me = nix::unistd::getuid().as_raw();
        let base = std::env::temp_dir().join(format!("weaver-admin-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let root = base.join("alpha");
        let territory = write_root(&root);
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
        let room = territory.join(save_points::ROOM);
        std::fs::create_dir(&room).unwrap();
        let bytes = save_points::tests::save_point("r-1", 5, 1, 1_000_000_000, b"room");
        let digest = save_points::judge(&bytes).unwrap().digest;
        std::fs::write(room.join(format!("{digest}.save-point")), &bytes).unwrap();
        let mut config =
            load_service_config_at(&base, "alpha", me, nix::unistd::getegid().as_raw()).unwrap();
        config.coordination_root = base.join("coordination");
        std::fs::create_dir_all(config.run_directory()).unwrap();
        TEST_PUBLICATION.with(|cell| {
            cell.set(Some((
                me,
                save_points::Owner {
                    uid: me,
                    gid: nix::unistd::getgid().as_raw(),
                },
            )))
        });
        let report = weaver_types::SavePointReport {
            save_point: digest.clone(),
            name: format!("{digest}.save-point"),
            ..report()
        };
        (config, report, base)
    }

    /// The lines the scratch territory's manifest holds.
    fn manifest_lines(config: &ServiceConfig) -> Vec<save_points::ManifestLine> {
        let me = nix::unistd::getuid().as_raw();
        save_points::read_manifest(
            config.save_points_fd().unwrap(),
            save_points::Owner {
                uid: me,
                gid: nix::unistd::getgid().as_raw(),
            },
        )
        .unwrap()
    }

    /// The digests the scratch territory's manifest names.
    fn manifest_digests(config: &ServiceConfig) -> Vec<String> {
        manifest_lines(config)
            .into_iter()
            .map(|line| line.digest)
            .collect()
    }

    /// **A leave whose lock outlives the after-left wait keeps its save
    /// point** (the #94 survey's S7): the worker answers `Left` naming the
    /// room's file, a holder outlives the wait and is ended by the
    /// escalation, and the report is published before `Unloaded`, its line
    /// arrived at the leave with the event's position, the marker closing
    /// clean; and where the reported file is not in the room, the unload
    /// refuses `SavePointNotTaken` naming `published` and the marker stays
    /// open. Perturbation: drop the report on the escalation's path again
    /// and the first file publishes as recovered with no position, and the
    /// second unload answers unloaded over a marker closed clean.
    #[test]
    fn a_leave_ended_by_the_escalation_still_publishes_its_save_point() {
        let escalated_leave = |config: &ServiceConfig, report: &weaver_types::SavePointReport| {
            save_points::write_marker(
                &config.root,
                Some(&save_points::Marker::Open { run: "r-1".into() }),
            )
            .unwrap();
            let mut holder = stand_in_holder(config);
            let worker = recording_worker(
                config,
                vec![
                    weaver_types::Payload::Answer(LifecycleAnswer::State {
                        state: weaver_types::AgentState::Idle,
                        load: None,
                        constituents: Vec::new(),
                    }),
                    weaver_types::Payload::Answer(LifecycleAnswer::Left {
                        save_point: Some(report.clone()),
                    }),
                ],
            );
            let answered = unload_within(config, TEST_UNLOAD_BOUNDS, false);
            let _ = holder.kill();
            let _ = holder.wait();
            let _ = worker.join();
            answered
        };
        let (config, report, base) = territory_with_one_room_file("left-escalated");
        assert_eq!(escalated_leave(&config, &report), Ok(unloaded_answer()));
        let lines = manifest_lines(&config);
        assert_eq!(lines.len(), 1);
        assert_eq!(
            (
                lines[0].digest.as_str(),
                lines[0].arrived,
                lines[0].position.clone()
            ),
            (
                report.save_point.as_str(),
                save_points::Arrival::Leave,
                Some((report.event_run.0.clone(), report.position))
            ),
            "published as the leave's, at the event's position"
        );
        assert_eq!(
            save_points::read_marker(&config.root),
            Some(save_points::Marker::Closed { run: "r-1".into() })
        );
        let _ = std::fs::remove_dir_all(&base);
        // The reported file absent from the room: the leave's save point
        // did not publish, so the unload does not complete.
        let (config, report, base) = territory_with_one_room_file("left-escalated-absent");
        let room = config.territory.join(save_points::ROOM);
        std::fs::remove_file(room.join(&report.name)).unwrap();
        assert_eq!(
            escalated_leave(&config, &report),
            Err(LifecycleRefusal::SavePointNotTaken {
                missed: weaver_types::SavePointLeg::Published,
            })
        );
        assert!(manifest_lines(&config).is_empty());
        assert_eq!(
            save_points::read_marker(&config.root),
            Some(save_points::Marker::Open { run: "r-1".into() }),
            "the marker stays open for the next load's reset"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    /// **A retry on a run already ended publishes the room first** (the #94
    /// survey's S11): after an unload refused `published`, the leave's save
    /// point stands in the room; a `force-unload` with the run lock free
    /// publishes it and closes the marker as forced, never clean, since a
    /// published file proves no leave (Codex on #94 at 197e80b).
    /// Perturbation: drop the publication from the lock-free branch and the
    /// file stays in the room.
    #[test]
    fn a_retry_on_an_ended_run_publishes_the_room_first() {
        let (config, report, base) = territory_with_one_room_file("ended-retry");
        save_points::write_marker(
            &config.root,
            Some(&save_points::Marker::Open { run: "r-1".into() }),
        )
        .unwrap();
        assert_eq!(
            unload_within(&config, TEST_UNLOAD_BOUNDS, true),
            Ok(unloaded_answer())
        );
        assert_eq!(manifest_digests(&config), [report.save_point]);
        assert_eq!(
            save_points::read_marker(&config.root),
            Some(save_points::Marker::Forced { run: "r-1".into() })
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    /// **A run ended with its own on-demand save point in the room
    /// concludes open** (Codex on #94 at 197e80b): the file is
    /// this run's, its stamp naming the run the marker stands open on, but
    /// no leave answered, so a plain unload publishes it and the marker
    /// stays open for the next load's `NoCleanUnload`. Perturbation: close
    /// the marker clean on this run's own file and it reads closed.
    #[test]
    fn a_run_ended_with_its_own_demand_file_concludes_open() {
        let (config, report, base) = territory_with_one_room_file("own-demand");
        save_points::write_marker(
            &config.root,
            Some(&save_points::Marker::Open { run: "r-1".into() }),
        )
        .unwrap();
        assert_eq!(
            unload_within(&config, TEST_UNLOAD_BOUNDS, false),
            Ok(unloaded_answer())
        );
        assert_eq!(manifest_digests(&config), [report.save_point]);
        assert_eq!(
            save_points::read_marker(&config.root),
            Some(save_points::Marker::Open { run: "r-1".into() })
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    /// **A file of an older run makes no run clean** (the push review of
    /// 197e80b): a run that crashed with only an older run's file in its
    /// room publishes it as recovered, and the marker stays open on the
    /// crashed run, so the next load records `NoCleanUnload`. Perturbation:
    /// close the marker on any publication and it reads closed.
    #[test]
    fn a_recovered_file_of_an_older_run_leaves_the_marker_open() {
        let (config, report, base) = territory_with_one_room_file("older-run");
        save_points::write_marker(
            &config.root,
            Some(&save_points::Marker::Open { run: "r-2".into() }),
        )
        .unwrap();
        assert_eq!(
            unload_within(&config, TEST_UNLOAD_BOUNDS, false),
            Ok(unloaded_answer())
        );
        assert_eq!(manifest_digests(&config), [report.save_point]);
        assert_eq!(
            save_points::read_marker(&config.root),
            Some(save_points::Marker::Open { run: "r-2".into() })
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    /// **A leave's save point of another run makes no run clean** (the push
    /// review of 197e80b): the worker reports a save point whose event run
    /// is not the run the marker stands open on; it publishes, the verb
    /// answers unloaded, and the marker stays open for the next load's
    /// reset. The run compared is the event's, never the covered run: a run
    /// restored and left with no turn covers the prior run's position, so a
    /// report covering `r-0` whose event is `r-1`, with the marker open on
    /// `r-1`, closes it clean. Perturbations: close the marker whatever the
    /// report's run and the first reads closed; compare the covered run and
    /// the second stays open.
    #[test]
    fn a_leave_reporting_another_runs_save_point_leaves_the_marker_open() {
        let leave =
            |config: &ServiceConfig, report: &weaver_types::SavePointReport, marker_run: &str| {
                save_points::write_marker(
                    &config.root,
                    Some(&save_points::Marker::Open {
                        run: marker_run.into(),
                    }),
                )
                .unwrap();
                let mut holder = stand_in_holder(config);
                let worker = recording_worker(
                    config,
                    vec![
                        weaver_types::Payload::Answer(LifecycleAnswer::State {
                            state: weaver_types::AgentState::Idle,
                            load: None,
                            constituents: Vec::new(),
                        }),
                        weaver_types::Payload::Answer(LifecycleAnswer::Left {
                            save_point: Some(report.clone()),
                        }),
                    ],
                );
                let ender = std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    let _ = holder.kill();
                    let _ = holder.wait();
                });
                let answered = unload_within(config, TEST_UNLOAD_BOUNDS, false);
                ender.join().unwrap();
                let _ = worker.join();
                answered
            };
        let (config, report, base) = territory_with_one_room_file("other-run-leave");
        assert_eq!(leave(&config, &report, "r-2"), Ok(unloaded_answer()));
        assert_eq!(manifest_digests(&config), [report.save_point]);
        assert_eq!(
            save_points::read_marker(&config.root),
            Some(save_points::Marker::Open { run: "r-2".into() })
        );
        let _ = std::fs::remove_dir_all(&base);
        // Covering the prior run, the event this run's: the marker closes.
        let (config, report, base) = territory_with_one_room_file("restored-run-leave");
        let report = weaver_types::SavePointReport {
            run: weaver_types::RunId("r-0".into()),
            event_run: weaver_types::RunId("r-1".into()),
            ..report
        };
        assert_eq!(leave(&config, &report, "r-1"), Ok(unloaded_answer()));
        assert_eq!(manifest_digests(&config), [report.save_point]);
        assert_eq!(
            save_points::read_marker(&config.root),
            Some(save_points::Marker::Closed { run: "r-1".into() }),
            "the event's run is compared, not the covered run"
        );
        let _ = std::fs::remove_dir_all(&base);
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
        assert_eq!(save_points::read_marker(&config.root), Some(closed.clone()));
        // **The run being undone leaves forced, and one that does not leave
        // keeps the marker open on it** (the #94 survey's S10): the worker
        // refuses the leave, so the trace holds this run's `load` and no
        // `unload`, and the marker names the run for the next load's reset.
        // Perturbations: direct an unforced leave and the directive says so;
        // restore the prior marker whatever the leave did and it reads
        // closed.
        let mut standing = Standing {
            entered: true,
            marker_before: Some(Some(closed)),
            run_reference: Some("r-1".into()),
            ..Standing::default()
        };
        save_points::write_marker(
            &config.root,
            Some(&save_points::Marker::Open { run: "r-1".into() }),
        )
        .unwrap();
        let worker = recording_worker(
            &config,
            vec![weaver_types::Payload::Refusal(
                LifecycleRefusal::ActivityNotAtRest,
            )],
        );
        roll_back(&config, &mut standing);
        let directed = worker.join().unwrap();
        assert!(matches!(
            directed[0],
            weaver_types::Payload::Directive(LifecycleDirective::Leave { forced: true, .. })
        ));
        assert_eq!(
            save_points::read_marker(&config.root),
            Some(save_points::Marker::Open { run: "r-1".into() })
        );
        let _ = std::fs::remove_file(config.coordination_socket());
        // **A run whose `load` is on the trace, its `Ready` never read,
        // leaves the marker open on it** (the operator's ruling of 2026-10-08
        // on #99, K1): an enter refused after `load`, or a `Ready` past the
        // load bound, leaves `marker_before` unset; the rollback's forced
        // leave answers `Left`, so the run was entered, and the marker names
        // it, never the closed marker found. Perturbation: write the marker
        // only where `marker_before` was set and it stays closed.
        let closed = save_points::Marker::Closed { run: "r-0".into() };
        save_points::write_marker(&config.root, Some(&closed)).unwrap();
        let mut standing = Standing {
            entered: true,
            run_reference: Some("r-2".into()),
            ..Standing::default()
        };
        let worker = recording_worker(
            &config,
            vec![weaver_types::Payload::Answer(LifecycleAnswer::Left {
                save_point: None,
            })],
        );
        let account = roll_back(&config, &mut standing);
        let _ = worker.join();
        assert!(account.contains("left open on the run"), "{account}");
        assert_eq!(
            save_points::read_marker(&config.root),
            Some(save_points::Marker::Open { run: "r-2".into() })
        );
        // A harness that never entered answers the forced leave
        // `OutOfOrder`: no `load` is on the trace, so the marker found is
        // put back. Perturbation: name the run whatever the leave answered
        // and this reads open.
        let _ = std::fs::remove_file(config.coordination_socket());
        let mut standing = Standing {
            entered: true,
            run_reference: Some("r-3".into()),
            marker_before: Some(Some(closed.clone())),
            ..Standing::default()
        };
        let worker = recording_worker(
            &config,
            vec![weaver_types::Payload::Refusal(LifecycleRefusal::OutOfOrder)],
        );
        roll_back(&config, &mut standing);
        let _ = worker.join();
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
                load_service_config(&AgentName(reserved.into()), false).err(),
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
        use std::os::fd::AsFd;
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

        let root_fd = save_points::open_directory(&root).unwrap();
        let territory =
            prepare_territory(root_fd.as_fd(), &root, member).expect("the territory is made");
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
        let again = prepare_territory(root_fd.as_fd(), &root, member).expect("the second load");
        assert_eq!(again, territory, "the same room, not a second one");
        assert_eq!(
            std::fs::metadata(&again).expect("it stands").mode() & 0o777,
            0o700,
            "a load closes a room that was left open"
        );
        // **A link at the room's name is refused and its target untouched**
        // (the custody audit's G13). Perturbation: make the room by path
        // again and the link's target is chowned and narrowed to 0700.
        std::fs::remove_dir(&territory).unwrap();
        let elsewhere = root.0.join("elsewhere");
        std::fs::create_dir(&elsewhere).unwrap();
        std::fs::set_permissions(&elsewhere, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::os::unix::fs::symlink(&elsewhere, &territory).unwrap();
        assert!(
            prepare_territory(root_fd.as_fd(), &root, member).is_none(),
            "a link at the room's name refuses the leg"
        );
        assert_eq!(
            std::fs::metadata(&elsewhere).unwrap().mode() & 0o777,
            0o755,
            "the link's target keeps its mode"
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
        // The territory's access group as judged, which no longer rides the
        // drop (the custody audit's G11), and the territory's descriptor, as
        // the judgment opens it, through which the room is made.
        service.access_gid = 4244;
        service.territory_fd = Some(save_points::open_directory(&sink).unwrap());
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
            "4243",
            "its own group alone: not the access group, none of root's"
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
