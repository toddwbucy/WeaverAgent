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

mod agent_cgroup;
mod channel;
mod conclusion;
mod containment;
mod inventory;
mod log;
// Complete replacement operations, inactive until FOUNDATION-TRANSITION cutover.
#[allow(dead_code)]
mod offline_state;
// Paired replacement Enter producer, dormant until the coupled cutover.
mod random;
mod save_points;
#[allow(dead_code)]
mod serving_enter;
// Dormant root startup composition; selected only with the qualified worker.
#[allow(dead_code)]
mod serving_load;
// Complete replacement component, inactive until FOUNDATION-TRANSITION cutover.
#[allow(dead_code)]
mod seed;
mod shell;
mod sink;
mod stack;
mod start;
mod surface;
mod verb_table;
mod verbs;

/// A path under the temp directory for this crate's tests, removed when the
/// test ends, pass or fail: the guard drops on the unwind a failed assertion
/// takes as on a clean return, so no run leaves a directory behind (#690
/// item C2.9).
#[cfg(test)]
mod scratch;

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
    /// **The shell's static busybox**, per section 9 (#29): required where
    /// the declaration elects `sh`, judged at the inventory (section 4).
    shell_binary: Option<PathBuf>,
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
    /// no `admin.log` line and no publication, and the marker closes forced
    /// where the force ends a run (3.0, the conclusion's row 2).
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
    let _invocation =
        start::take_invocation_lock(&config.run_directory(), verb_table::HolderVerb::Validate)?;
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
    placement: std::os::fd::RawFd,
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
    match log {
        Ok(log) => {
            member.stderr(log);
        }
        // The member stands with no log rather than with one this crate
        // could not vouch for (#98), and the refusal is named.
        Err(refused) => {
            diag!("weaver-admin: the member's state.log is not opened: {refused}");
            member.stderr(std::process::Stdio::null());
        }
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
            // **The move into the agent's cgroup comes first** (6.1, C4),
            // before any descriptor is placed and before the privilege drop:
            // a failure ends the child before its exec.
            containment::place_self(placement)?;
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
    // **And no access entry beyond its mode** (#99 area 2, H5; #107): an
    // entry granting the member read hides under the mask, which still
    // shows 0640.
    match carries_access_entries_fd(std::os::fd::AsFd::as_fd(&file)) {
        Ok(false) => Ok(file),
        Ok(true) => Err(refuse("carries an access-control entry beyond its mode")),
        Err(_) => Err(refuse(
            "does not say whether it carries an access-control entry",
        )),
    }
}

fn take_inventory(
    config: &ServiceConfig,
    agent: &AgentName,
) -> Result<inventory::Inventory, LifecycleRefusal> {
    judge_reader(&config.require_boundary()?.reader, agent)?;
    room_inventory(config, agent)
}

/// **The shell's grant, built at the inventory where the declaration elects
/// `sh`** (#29, Spec section 4): the root's `shell-binary` required, then
/// judged by `shell::judge`, and the grant names the judged binary and the
/// run directory's `commands/`, the gate reading no root of this crate's. A
/// missing key refuses `ConfigInvalid` with no field, as every root value
/// that fails to read does (section 9). A judgment that fails refuses
/// `BoundaryUnverified`, said on standard error, nothing repaired.
fn grant_shell(
    config: &ServiceConfig,
    inventory: &mut inventory::Inventory,
) -> Result<(), LifecycleRefusal> {
    let elected = inventory
        .config
        .tool_set
        .iter()
        .any(|tool| tool.0 == weaver_types::SHELL_TOOL);
    let Some((judged, grant)) = shell_grant(
        elected,
        config.shell_binary.as_deref(),
        &config.run_directory(),
        nix::unistd::geteuid().as_raw(),
    )?
    else {
        return Ok(());
    };
    if let weaver_types::EnterBinding::Serving { shell, .. } = &mut inventory.binding {
        *shell = Some(grant);
    }
    inventory.shell = Some(judged);
    Ok(())
}

/// [`grant_shell`]'s judgment, of the values alone: `None` where the shell is
/// not elected, the judged busybox and the grant where it is.
fn shell_grant(
    elected: bool,
    shell_binary: Option<&std::path::Path>,
    run_directory: &std::path::Path,
    owner: u32,
) -> Result<Option<(shell::Judged, weaver_types::ShellGrant)>, LifecycleRefusal> {
    if !elected {
        return Ok(None);
    }
    let Some(binary) = shell_binary else {
        diag!(
            "weaver-admin: the declaration elects sh, and the service configuration has no shell-binary"
        );
        return Err(LifecycleRefusal::ConfigInvalid { field: None });
    };
    let judged = shell::judge(binary, owner).map_err(|why| {
        diag!("weaver-admin: shell-binary: {why}");
        LifecycleRefusal::BoundaryUnverified
    })?;
    let grant = weaver_types::ShellGrant {
        binary: judged.binary.clone(),
        commands: run_directory.join(shell::COMMANDS),
    };
    Ok(Some((judged, grant)))
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
    // **Every inventory that reads a declaration electing `sh` carries its
    // grant** (#29): built here, so no path that takes an inventory, a
    // verb that starts nothing among them, holds a serving binding electing
    // the shell with no grant.
    let mut inventory = inventory;
    grant_shell(config, &mut inventory)?;
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
    let boundary = inventory_boundary(config, agent)?;
    inventory::take_inventory(agent, source, &boundary)
}

/// Host facts shared by legacy and replacement authoring inventory.
fn inventory_boundary(
    config: &ServiceConfig,
    agent: &AgentName,
) -> Result<inventory::Boundary, LifecycleRefusal> {
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
    Ok(boundary)
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
mod gid_tests;

/// The observation's bound, per Spec section 3: five seconds from the
/// `Observe`, after the dial's bound, which covers only the connect.
const OBSERVE_BOUND: std::time::Duration = std::time::Duration::from_secs(5);

/// The leave's own bound, per Spec section 3: 150 seconds from the
/// directive, past the harness's 120 for the save point's answer leg (the
/// operator's ruling of 2026-10-08 on #1) and its two-second legs, so admin
/// never gives up on a save point the harness is still waiting for. The
/// floor's `LEAVE_BOUND_MS`, the one value the harness's shared deadline reads.
const LEAVE_BOUND: std::time::Duration =
    std::time::Duration::from_millis(weaver_types::LEAVE_BOUND_MS);

/// The stop's bound, per Spec section 3: sixty seconds from the directive.
const STOP_BOUND: std::time::Duration = std::time::Duration::from_secs(60);

/// What an observation of a held run found, per Spec section 3. **Silence is
/// not evidence**: only `Unloaded` or no listener at all says no run entered.
enum Observation {
    State(
        weaver_types::AgentState,
        Option<Box<weaver_types::LoadFacts>>,
    ),
    /// The worker answered `InTransition`: a leave is pending
    /// (`weaver-types-Spec` section 3.1).
    InTransition,
    /// No worker listens: no name bound, or the connection refused, through
    /// the dial's whole bound.
    NoListener,
    /// A worker accepted, or the backlog was full, and nothing answered
    /// inside the bounds: a busy run, never a stranded one.
    Silent,
}

// #118: a test-only hook, kept beside the production code it observes: the
// worker uid a test's dial expects where no account resolves, set to a uid
// the suite's stand-in listener is not, to stand a wrong peer.
#[cfg(test)]
thread_local! {
    static TEST_WORKER_UID: std::cell::Cell<Option<u32>> = const { std::cell::Cell::new(None) };
}

/// **The coordination socket dialed, its peer judged the agent's worker**
/// (#98, #140; Spec section 7): the expected uid is the worker account's,
/// resolved by name as the start step resolves it, and the listening process
/// must hold the run lock in the agent's run directory. An account that does not resolve
/// stands no worker, and the dial is refused as one nothing could answer.
fn dial_worker(config: &ServiceConfig) -> Result<channel::Coordination, channel::ChannelFault> {
    let identity = inventory::identity_for(&AgentName(config.agent.clone()));
    let worker_uid = match nix::unistd::User::from_name(&identity).ok().flatten() {
        Some(worker) => worker.uid.as_raw(),
        // #118: a test-only hook, kept beside the production code it observes:
        // the suite's stand-in workers listen under its own uid, no
        // `weaver-<agent>` account standing on a test box.
        #[cfg(test)]
        None => TEST_WORKER_UID
            .with(std::cell::Cell::get)
            .unwrap_or_else(|| nix::unistd::getuid().as_raw()),
        #[cfg(not(test))]
        None => {
            diag!(
                "weaver-admin: the worker account {identity} does not resolve, so the coordination socket is not dialed"
            );
            return Err(channel::ChannelFault::NotDialable);
        }
    };
    channel::dial(
        &config.coordination_socket(),
        worker_uid,
        &config.run_directory(),
    )
}

/// **Observes the run through the harness's own word**, per Spec section 3
/// and `weaver-admin-harness-contract` section 3, within the dial's bound and
/// the observation's.
fn observe(config: &ServiceConfig) -> Result<Observation, LifecycleRefusal> {
    let mut coordination = match dial_worker(config) {
        Ok(coordination) => coordination,
        Err(channel::ChannelFault::NoListener) => return Ok(Observation::NoListener),
        // **A listener that is not the worker is no observation** (#98): its
        // word is not taken, and the verb refuses rather than reading the
        // run as silent.
        Err(channel::ChannelFault::WrongPeer { .. }) => {
            return Err(LifecycleRefusal::BoundaryUnverified);
        }
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
            // The harness's answer while a leave is pending, not a state.
            weaver_types::Payload::Answer(LifecycleAnswer::InTransition { .. }) => {
                Ok(Observation::InTransition)
            }
            weaver_types::Payload::Refusal(refusal) => Err(refusal),
            _ => Err(LifecycleRefusal::Malformed),
        },
        Err(_) => Ok(Observation::Silent),
    }
}

/// **What admin sees with the invocation lock its own** (`weaver-admin-Spec`
/// 3.0, the verb table; `verb_table`'s `Seen`): the run lock first, then,
/// where it is held, the worker's word to `Observe`. Answers the load's facts
/// beside it where the worker named them, for `show`'s rung (i). A worker
/// that refuses the observation is the caller's to read: `show` answers the
/// refusal, `unload` reads it as silence.
fn see_run(
    config: &ServiceConfig,
) -> Result<(verb_table::Seen, Option<Box<weaver_types::LoadFacts>>), LifecycleRefusal> {
    use verb_table::Seen;
    if !start::run_lock_held(&config.run_directory())? {
        return Ok((Seen::RunLockFree, None));
    }
    let (worker, load) = worker_seen(observe(config)?);
    Ok((Seen::RunLockHeld(worker), load))
}

/// The worker's word as `verb_table` reads it, with the load's facts where
/// the worker named them.
fn worker_seen(
    observation: Observation,
) -> (verb_table::WorkerSeen, Option<Box<weaver_types::LoadFacts>>) {
    use verb_table::WorkerSeen;
    match observation {
        Observation::State(weaver_types::AgentState::Idle, load) => (WorkerSeen::Idle, load),
        Observation::State(weaver_types::AgentState::Active, load) => (WorkerSeen::Active, load),
        // A worker answering that nothing is resident: a run never entered.
        Observation::State(
            weaver_types::AgentState::Unloaded | weaver_types::AgentState::Absent,
            _,
        ) => (WorkerSeen::Unloaded, None),
        Observation::InTransition => (WorkerSeen::InTransition, None),
        Observation::NoListener => (WorkerSeen::NoSocket, None),
        Observation::Silent => (WorkerSeen::Silent, None),
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
    /// The run this load made the agent's cgroup for, once made: the
    /// rollback always ends that cgroup, whatever the marker says (6.1).
    cgroup_run: Option<String>,
}

/// **`load` keeps one promise**, per Spec section 3 and the operator's ruling
/// of 2026-10-03 on #72: a run started by this load, or a refusal with nothing
/// changed. A held run lock refuses `AgentRunning`, or `Unanswered` where the
/// worker is silent, and a load never ends an existing run.
fn load(config: &ServiceConfig, agent: &AgentName) -> Result<LifecycleAnswer, LifecycleRefusal> {
    let _invocation =
        start::take_invocation_lock(&config.run_directory(), verb_table::HolderVerb::Load)?;
    let mut standing = Standing::default();
    match run_load(config, agent, &mut standing) {
        Ok(()) => Ok(LifecycleAnswer::State {
            state: weaver_types::AgentState::Idle,
            load: None,
            constituents: Vec::new(),
        }),
        Err(refusal) => {
            let (account, contained) = roll_back(config, &mut standing);
            if !account.is_empty() {
                record(config, "load", &format!("rolled back: {account}"));
            }
            // **A rollback's run end that refuses is the load's answer** (6.1,
            // the run end's failures, a rollback's included): the operator
            // reads that the agent's cgroup still holds processes, or could
            // not be removed, rather than the load's first refusal.
            Err(contained.err().unwrap_or(refusal))
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
        // silence is `Unanswered`, per Spec section 3 and 3.0's verb table.
        let worker = match observe(config) {
            Ok(observation) => worker_seen(observation).0,
            Err(_) => verb_table::WorkerSeen::Active,
        };
        let seen = verb_table::Seen::RunLockHeld(worker);
        return Err(
            match verb_table::act(verb_table::Verb::Load, verb_table::class_of(seen)) {
                verb_table::VerbAction::RefuseUnanswered => LifecycleRefusal::Unanswered,
                _ => LifecycleRefusal::AgentRunning,
            },
        );
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
    let sink = sink::open(
        &inventory.config.trace_sink,
        &custody,
        config.territory_fd()?,
    )?;
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
    // **The run is minted before the agent's cgroup is made**, per Spec
    // section 7 and 6.1: the record names the run its load made it for, and
    // is written before the first fork. The reference is this crate's, and a
    // load that cannot read the randomness it rests on refuses rather than
    // carrying a reference that looks like the others and is not guaranteed.
    let run_reference =
        channel::mint_run_reference(&agent.0).ok_or(LifecycleRefusal::BoundaryUnverified)?;
    // **The agent's cgroup is made holding both locks, before any
    // constituent forks** (Spec section 3, the order within a load; 6.1).
    let cgroup = make_agent_cgroup(config, agent, &run_reference.0, &account)?;
    standing.cgroup_run = Some(run_reference.0.clone());
    // **The commands directory is rebuilt at every load where the shell is
    // granted** (#29, Spec section 6), after the agent's cgroup so the
    // rollback's run end removes it, and before any constituent forks. A
    // rebuild that fails refuses the load and stands nothing.
    if let Some(judged) = inventory.shell.as_ref() {
        shell::build_commands(&config.run_directory(), judged).map_err(|why| {
            diag!("weaver-admin: the shell's commands directory: {why}");
            LifecycleRefusal::BoundaryUnverified
        })?;
    }
    record(
        config,
        "load",
        &format!(
            "the agent's cgroup {} for run {}",
            cgroup.record.path, cgroup.record.run
        ),
    );
    let constituents_procs = {
        use std::os::fd::AsRawFd;
        cgroup.constituents_procs().as_raw_fd()
    };

    // **The trace door stands only for a file sink**, per Spec section 6: a
    // stale door is cleared on every load, and for a file sink the relay is
    // stood before the member and the worker, the lifetime pipe made before
    // either fork.
    start::clear_trace_door(&config.run_directory())?;
    let relay_write = match &inventory.config.trace_sink {
        weaver_types::TraceSink::File { .. } => {
            let (_relay, write) = stand_relay(config, agent, &sink, &run_lock, constituents_procs)?;
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
        constituents_procs,
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
            classify.as_deref(),
        ),
        uid: account.uid,
        gid: account.gid,
        home: &account.home,
        library_path: config.library_path.as_deref(),
        log: &worker_log,
        run_lock: &run_lock,
        relay_write: relay_write.as_ref().map(std::os::fd::AsRawFd::as_raw_fd),
        placement: {
            use std::os::fd::AsRawFd;
            cgroup.run_procs().as_raw_fd()
        },
    })
    .map_err(|_| LifecycleRefusal::BindFailed)?;
    standing.forked = true;
    // **The start step closes its own copy of the write end once both
    // children hold theirs**, per Spec section 6, so the relay reads
    // end-of-file at the worker's death and not at this invocation's.
    drop(relay_write);
    standing.run_lock = Some(run_lock);

    let mut coordination = match dial_worker(config) {
        Ok(coordination) => coordination,
        // Not the worker this load started (#98): the load refuses, and its
        // rollback ends what it stood.
        Err(channel::ChannelFault::WrongPeer { .. }) => {
            return Err(LifecycleRefusal::BoundaryUnverified);
        }
        Err(_) => return Err(refusal_from_worker(&mut worker)),
    };
    let ordinal = coordination.next_ordinal();
    // **The session is read**, per Spec section 7: the operator's, declared
    // in the config and carried uninterpreted. The run was minted above.
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
                // **The bounds cross on the enter**, per `weaver-types-Spec`
                // section 2: the harness's only source for timing the drain
                // and the wind-down, read from the one parse admin made.
                drain_bound: inventory
                    .config
                    .lifecycle
                    .as_ref()
                    .and_then(|bounds| bounds.drain_bound),
                wind_down_bound: inventory
                    .config
                    .lifecycle
                    .as_ref()
                    .and_then(|bounds| bounds.wind_down_bound),
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
    let _invocation =
        start::take_invocation_lock(&run_directory, verb_table::HolderVerb::SavePoint)?;
    // **By the verb table** (3.0): a save point is directed to a worker
    // that answers or is wedged; with none to take it (down, a leave
    // pending, a run never entered, the worker gone) it is out of order.
    let (seen, _) = see_run(config)?;
    match verb_table::act(verb_table::Verb::SavePoint, verb_table::class_of(seen)) {
        verb_table::VerbAction::DirectSavePoint => {}
        _ => {
            diag!("weaver-admin: no worker stands to take a save point");
            return Err(LifecycleRefusal::OutOfOrder);
        }
    }
    let mut coordination = dial_worker(config).map_err(|fault| match fault {
        channel::ChannelFault::WrongPeer { .. } => LifecycleRefusal::BoundaryUnverified,
        _ => LifecycleRefusal::Unanswered,
    })?;
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
    // **A report is accepted only for the open marker's run** (S17, the
    // operator's ruling of 2026-10-14 on #140; 3.0, the `save-point` verb's
    // publication): the worker runs as the agent's uid, so a report naming
    // another run is not this run's save point, and its file publishes as
    // recovered. A marker that reads other than `Open` is no match; **one
    // that does not read refuses** `BoundaryUnverified` before anything is
    // published, the file left in the room for the next verb, never minted
    // recovered on a read that failed.
    let this_run = open_run(config)?.as_deref() == Some(report.event_run.0.as_str());
    let reports = match conclusion::publication_of(this_run) {
        conclusion::Publication::Reported => {
            vec![(report.clone(), save_points::Arrival::Demand)]
        }
        conclusion::Publication::Recovered => {
            diag!(
                "weaver-admin: the save point {} reports run {}, not the run the marker stands open on; its file publishes as recovered",
                report.save_point,
                report.event_run.0
            );
            record(
                config,
                "save-point",
                "the report is not this run's; its file publishes as recovered",
            );
            Vec::new()
        }
    };
    // **The verb answers only a published save point of this run**: the
    // harness's report is answered once its file stands in the territory's
    // save-points under a manifest line, and a publication that refuses, or
    // does not reach this save point, refuses the verb `SavePointNotTaken`
    // naming `published`, as the unload names it (the #94 survey's S12), the
    // file standing in the room for the next verb and said so in the log.
    let published = publish_from_room(config, agent, &reports);
    let landed = matches!(&published, Ok(lines) if lines.iter().any(|line| line.digest == report.save_point));
    match conclusion::save_point_answer(this_run, landed) {
        conclusion::SavePointAnswer::Taken => Ok(LifecycleAnswer::SavePointTaken { report }),
        conclusion::SavePointAnswer::NotTakenPublished => {
            if this_run {
                diag!(
                    "weaver-admin: the save point {} was taken and not published; it stands in the member's room for the next verb",
                    report.save_point
                );
            }
            Err(LifecycleRefusal::SavePointNotTaken {
                missed: weaver_types::SavePointLeg::Published,
            })
        }
    }
}

/// **Name the save point the next load restores**, the `restore` verb, per
/// Spec sections 2 and 4: the one the declaration's `[restore]` names, never
/// the caller's, judged as a load judges one and entered in the manifest as
/// named at a restore, so a file that arrived by no publication becomes
/// loadable by this verb alone. The live restore of a running agent waits on
/// the loop act's `Reopen`; until then a restore is this verb and then a
/// load.
fn restore(config: &ServiceConfig, agent: &AgentName) -> Result<LifecycleAnswer, LifecycleRefusal> {
    let _invocation =
        start::take_invocation_lock(&config.run_directory(), verb_table::HolderVerb::Restore)?;
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
    placement: std::os::fd::RawFd,
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
        placement,
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
fn roll_back(
    config: &ServiceConfig,
    standing: &mut Standing,
) -> (String, Result<(), LifecycleRefusal>) {
    let mut account = Vec::new();
    let mut contained = Ok(());
    // **The run being undone leaves forced** (the #94 survey's S10): it
    // takes no save point, a run rolled back having nothing to keep, and
    // its `unload` says forced.
    let leave = standing
        .entered
        .then(|| direct_leave_within(config, LEAVE_BOUND, LeaveDirected::Rollback));
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
    // The rollback's row of the outcome table (3.0, the conclusion's row 1):
    // a rollback's `Leave` is forced and takes no save point, so its row is
    // reached whatever the leave answered.
    let row = conclusion::classify(conclusion::Seen {
        rollback: Some(load_on_trace),
        territory_judged: !config.territory_unjudged,
        forced: true,
        no_state: false,
        reported: None,
    })
    .map(conclusion::conclude);
    let write = match row {
        Ok(conclusion::Conclusion::Settled(outcome)) => outcome.marker,
        // Unreachable by the table: the conservative label, the run named.
        Ok(conclusion::Conclusion::Publish { .. }) | Err(_) => conclusion::MarkerWrite::Open,
    };
    let marker = match (&standing.run_reference, write) {
        (Some(run), conclusion::MarkerWrite::Open) => {
            Some(Some(save_points::Marker::Open { run: run.clone() }))
        }
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
    let mut lock_free = true;
    if standing.forked {
        let ended = start::escalate(&config.run_directory());
        lock_free = ended.is_ok();
        account.push(format!(
            "constituents {}",
            match ended {
                Ok(()) => "ended".to_string(),
                Err(refusal) => format!("held: {}", surface::render_refusal(&refusal)),
            }
        ));
    }
    // **The rollback always ends the cgroup its own load made** (6.1,
    // ownership is the record's), whatever the marker says.
    if let Some(run) = standing.cgroup_run.take() {
        let (said, ended) = end_rolled_back_cgroup(config, &run, lock_free);
        account.push(said);
        contained = ended;
    }
    if standing.sink_opened {
        account.push("sink closed".to_string());
    }
    (account.join(", "), contained)
}

/// **The rollback's run end, once the run lock is free** (6.1, admin's run
/// end: when): an escalation that left the run lock held ended no run, so
/// the cgroup it made is left with its record for the next `unload`, and
/// the rollback refuses `WorkerWouldNotExit` as a run end past its bound
/// does. The account names which.
fn end_rolled_back_cgroup(
    config: &ServiceConfig,
    run: &str,
    lock_free: bool,
) -> (String, Result<(), LifecycleRefusal>) {
    if !lock_free {
        return (
            "cgroup held: the run lock did not free, left for the next unload".to_string(),
            Err(LifecycleRefusal::WorkerWouldNotExit),
        );
    }
    let contained = end_agent_cgroup(config, "load", agent_cgroup::Concluding::Run(run));
    let said = format!(
        "cgroup {}",
        match &contained {
            Ok(()) => "ended".to_string(),
            Err(refusal) => format!("held: {}", surface::render_refusal(refusal)),
        }
    );
    (said, contained)
}

/// **The start step's making of the agent's cgroup** (Spec 6.1), its
/// refusals named on standard error and in `admin.log`: an ended run's
/// leftovers refuse `AgentRunning`, naming them and killing nothing (I8),
/// and every other refusal is `BoundaryUnverified`, naming the cgroup and
/// which question failed.
fn make_agent_cgroup(
    config: &ServiceConfig,
    agent: &AgentName,
    run: &str,
    account: &AgentAccount,
) -> Result<agent_cgroup::Made, LifecycleRefusal> {
    use std::os::fd::AsFd;
    let run_directory = agent_cgroup::open_run_directory(&config.run_directory())
        .map_err(|_| LifecycleRefusal::BoundaryUnverified)?;
    agent_cgroup::make(
        run_directory.as_fd(),
        &agent.0,
        run,
        (account.uid, account.gid),
    )
    .map_err(|refused| match refused {
        agent_cgroup::StartRefused::RunLockHeld => LifecycleRefusal::AgentRunning,
        agent_cgroup::StartRefused::Leftovers(pids) => {
            let named = name_pids(&pids);
            diag!(
                "weaver-admin: the agent's cgroup holds an ended run's leftovers ({named}); unload ends them"
            );
            record(config, "load", &format!("refused: leftovers {named}"));
            LifecycleRefusal::AgentRunning
        }
        agent_cgroup::StartRefused::Unverified(why) => {
            diag!("weaver-admin: the agent's cgroup: {why}");
            record(config, "load", &format!("refused: {why}"));
            LifecycleRefusal::BoundaryUnverified
        }
    })
}

/// **The run end of the agent's cgroup** (Spec 6.1, admin's run end), under
/// the invocation lock once the run lock is free, before anything is
/// published, a marker written or a state answered. A kill that found
/// processes standing, and a record that did not remove, are said in
/// `admin.log`. Past the bound it refuses `WorkerWouldNotExit`, naming the
/// pids, and a removal or a record that fails refuses `BoundaryUnverified`.
fn end_agent_cgroup(
    config: &ServiceConfig,
    verb: &'static str,
    concluding: agent_cgroup::Concluding<'_>,
) -> Result<(), LifecycleRefusal> {
    use std::os::fd::AsFd;
    let run_directory = agent_cgroup::open_run_directory(&config.run_directory())
        .map_err(|_| LifecycleRefusal::BoundaryUnverified)?;
    match agent_cgroup::run_end(
        run_directory.as_fd(),
        &config.agent,
        concluding,
        containment::CONTAINMENT_BOUND,
    ) {
        Ok(agent_cgroup::Ended::Removed {
            standing,
            record_removed,
        }) => {
            if !standing.is_empty() {
                record(
                    config,
                    verb,
                    &format!(
                        "the run end's kill found {} still standing",
                        name_pids(&standing)
                    ),
                );
            }
            if !record_removed {
                record(
                    config,
                    verb,
                    "the agent's cgroup is removed and its record did not remove; the next verb removes it",
                );
            }
            remove_commands(config, verb);
            Ok(())
        }
        Ok(agent_cgroup::Ended::Stale { record_removed }) => {
            if !record_removed {
                record(
                    config,
                    verb,
                    "a stale cgroup record did not remove; the next verb removes it",
                );
            }
            remove_commands(config, verb);
            Ok(())
        }
        Ok(agent_cgroup::Ended::NoRecord) => {
            remove_commands(config, verb);
            Ok(())
        }
        // Another run's record: its `commands/` is that run's, left standing.
        Ok(agent_cgroup::Ended::NotOwned) => Ok(()),
        Err(agent_cgroup::EndRefused::WouldNotExit(pids)) => {
            let named = name_pids(&pids);
            diag!(
                "weaver-admin: the agent's cgroup did not empty within the containment bound ({named})"
            );
            record(config, verb, &format!("refused: would not exit {named}"));
            Err(LifecycleRefusal::WorkerWouldNotExit)
        }
        Err(agent_cgroup::EndRefused::Unverified(why)) => {
            diag!("weaver-admin: the agent's cgroup: {why}");
            record(config, verb, &format!("refused: {why}"));
            Err(LifecycleRefusal::BoundaryUnverified)
        }
    }
}

/// **The ladder's rung (iv)** (Spec section 3; 6.1, C2), the run lock free:
/// `Unloaded` where the agent's cgroup is absent or empty, `InTransition`
/// naming what it holds otherwise, naming none where it reads populated
/// and lists nothing, never `Unloaded` over a process of the agent. A record that does not read refuses `BoundaryUnverified`.
fn unloaded_by_ladder(config: &ServiceConfig) -> Result<LifecycleAnswer, LifecycleRefusal> {
    use std::os::fd::AsFd;
    let run_directory = agent_cgroup::open_run_directory(&config.run_directory())
        .map_err(|_| LifecycleRefusal::BoundaryUnverified)?;
    match agent_cgroup::holds(run_directory.as_fd(), &config.agent) {
        Ok(None) => Ok(unloaded_answer()),
        // Named or not: a cgroup that does not read `populated 0` holds a
        // process of the agent, one mid-exit among them, and is never
        // `Unloaded`.
        Ok(Some(pids)) => Ok(LifecycleAnswer::InTransition {
            constituents: pids.into_iter().map(|pid| pid as u32).collect(),
        }),
        Err(why) => {
            diag!("weaver-admin: the agent's cgroup: {why}");
            Err(LifecycleRefusal::BoundaryUnverified)
        }
    }
}

/// Offline operations require the ladder's complete Unloaded judgment even
/// when their caller already holds the run lock. Surviving cgroup members can
/// outlive the process that held that lock.
fn require_unloaded(config: &ServiceConfig) -> Result<(), LifecycleRefusal> {
    match unloaded_by_ladder(config)? {
        LifecycleAnswer::State {
            state: weaver_types::AgentState::Unloaded,
            ..
        } => Ok(()),
        _ => Err(LifecycleRefusal::AgentRunning),
    }
}

/// Pids as `admin.log` names them.
fn name_pids(pids: &[i32]) -> String {
    let named: Vec<String> = pids.iter().map(i32::to_string).collect();
    format!("pids [{}]", named.join(", "))
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

/// **`show` answers by the ladder** (`weaver-admin-Spec` section 3; 3.0,
/// the verb table's `show` column), holding the invocation lock shared:
/// (0) `InTransition` at once where an exclusive holder stands; (i) the
/// worker's word, `Idle` or `Active` with its facts, `InTransition` while a
/// leave is pending, and `InTransition` in place of an `Unloaded` while the
/// run lock is held; (ii) `Unanswered` where the worker accepts and says
/// nothing; (iii) `InTransition` where no worker listens and the run lock is
/// held; (iv) `Unloaded` without dialing where the run lock is free.
/// **Wherever a run holds the run lock, its constituents are named**
/// (toddwbucy/WeaverWeb#15), on the worker's word and on `InTransition`
/// alike; rung (0) names none, the holder being another invocation.
fn show(config: &ServiceConfig) -> Result<LifecycleAnswer, LifecycleRefusal> {
    use verb_table::{Seen, VerbAction, WorkerSeen};
    let run_directory = config.run_directory();
    let _shared = match start::take_shared_invocation_lock(&run_directory)? {
        start::Shared::Held(lock) => lock,
        start::Shared::InTransition => {
            return Ok(LifecycleAnswer::InTransition {
                constituents: Vec::new(),
            });
        }
    };
    let (seen, load) = see_run(config)?;
    // **The constituents are read after the run lock, so an empty read is
    // read against the lock again** (Codex on #126, round 4): the last
    // member or relay can let go during the observation's failed dial, and
    // `InTransition` naming nothing over a run that is down would be false.
    // The lock free, the run is down, rung (iv); still held, something holds
    // it that the inventory does not list, and `InTransition` naming none is
    // the truthful answer.
    let in_transition = || -> Result<LifecycleAnswer, LifecycleRefusal> {
        let pids = constituents(&run_directory);
        if pids.is_empty() && !start::run_lock_held(&run_directory)? {
            return unloaded_by_ladder(config);
        }
        Ok(LifecycleAnswer::InTransition { constituents: pids })
    };
    Ok(
        match verb_table::act(verb_table::Verb::Show, verb_table::class_of(seen)) {
            VerbAction::ShowUnloaded => unloaded_by_ladder(config)?,
            VerbAction::ShowWorkersWord => match seen {
                Seen::RunLockHeld(WorkerSeen::Idle) => LifecycleAnswer::State {
                    state: weaver_types::AgentState::Idle,
                    load,
                    constituents: constituents(&run_directory),
                },
                Seen::RunLockHeld(WorkerSeen::Active) => LifecycleAnswer::State {
                    state: weaver_types::AgentState::Active,
                    load,
                    constituents: constituents(&run_directory),
                },
                // A leave pending, the one other word the table answers.
                _ => in_transition()?,
            },
            VerbAction::ShowUnanswered => return Err(LifecycleRefusal::Unanswered),
            VerbAction::ShowInTransitionConstituents => in_transition()?,
            // Rung (0) above, and no other action answers `show`.
            _ => LifecycleAnswer::InTransition {
                constituents: Vec::new(),
            },
        },
    )
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

/// **`unload` ends whatever holds the run lock**, per Spec section 3 and its
/// row in the 3.0 verb table: where the worker answers `Idle` or `Active`, or
/// is silent, it directs leave under the leave's bound; where it answers
/// `Unloaded`, or no worker listens, no run was entered and the escalation
/// ends it, the room then published; where a leave is pending it refuses
/// `OutOfOrder`. It answers provisioned-and-unloaded only once the lock is
/// free.
fn unload(config: &ServiceConfig) -> Result<LifecycleAnswer, LifecycleRefusal> {
    unload_within(config, UNLOAD_BOUNDS, false)
}

/// **The forced unload**, per Spec 3.0 (the forced unload, sole and
/// joining): sole, it directs the leave with `forced`, the gate lowered at
/// once with no drain or wind-down, the leave save point still taken, and
/// concludes by its `Left`'s row (a save point of this run published closes
/// the marker `Closed`; one not taken writes `Forced`, the next load
/// recording `ForcedUnload`); beside an `unload` it joins lock-free, and
/// beside any other holder it refuses. Where the harness refuses the forced leave `OutOfOrder`,
/// a leave pending or a run never entered, the escalation ends the run and
/// the marker reads `Forced`; where no worker listens, or the run is already
/// down, the marker is left as it stands (3.0, S0d and S8b).
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
/// ten from `SIGTERM` to `SIGKILL` and five for the last read. A forced
/// unload holds the invocation lock at most 232 seconds before its
/// publication: the observation's dial 1 and answer 5, the leave's dial 1,
/// the leave 150, the escalation 45 (thirty, ten and five) and the run end's
/// containment bound 30 (6.1); a graceful one those past its declared drain
/// and wind-down bounds, or without a deadline where either is undeclared. A
/// number WeaverWeb builds against.
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
    let _invocation = if forced {
        match take_or_join(config, bounds)? {
            Taken::Sole(lock) => lock,
            Taken::Joined(answer) => return Ok(answer),
        }
    } else {
        start::take_invocation_lock(&run_directory, verb_table::HolderVerb::Unload)?
    };
    // **A refused observation is silence**, `unload`'s promise being to end
    // whatever holds the lock: it directs leave as for any silent run.
    let (seen, window) = if start::run_lock_held(&run_directory)? {
        let observed = match observe(config) {
            // **A graceful unload refuses a listener that is not the
            // worker** (#98), at its first look, the run left standing; a
            // force reads it as silence and ends the run on its lock.
            Err(LifecycleRefusal::BoundaryUnverified) if !forced => {
                return Err(LifecycleRefusal::BoundaryUnverified);
            }
            observed => observed.unwrap_or(Observation::Silent),
        };
        let (worker, load) = worker_seen(observed);
        (
            verb_table::Seen::RunLockHeld(worker),
            leave_window(bounds, forced, worker, load.as_deref()),
        )
    } else {
        (verb_table::Seen::RunLockFree, bounds.leave)
    };
    let verb = if forced {
        verb_table::Verb::ForceUnload
    } else {
        verb_table::Verb::Unload
    };
    match verb_table::act(verb, verb_table::class_of(seen)) {
        // **The marker as it stands** (3.0, S0d x force-unload): a force on
        // an agent already down ended nothing, so the next load records the
        // cause the marker holds.
        verb_table::VerbAction::PublishRoomThenUnloaded => conclude_ended(config),
        verb_table::VerbAction::DirectLeave | verb_table::VerbAction::DirectForcedLeave => {
            leave_within(config, bounds, window, forced, false)
        }
        // **A force on a pending leave or a run never entered directs its
        // forced leave first** (3.0, outside the envelope), and only where the
        // harness refuses it `OutOfOrder` ends the run by the escalation.
        verb_table::VerbAction::ForcedLeaveElseEscalateForced => {
            leave_within(config, bounds, bounds.leave, true, true)
        }
        // **No worker listens, or none entered** (3.0, S8b): the escalation
        // ends what holds the run lock, then the S0d path publishes the room
        // as recovered, the marker as it stands.
        verb_table::VerbAction::EndByEscalation => {
            start::escalate_within(&run_directory, bounds.term, bounds.kill)?;
            conclude_ended(config)
        }
        // **An unload meeting a pending leave directs nothing** (3.0, outside
        // the envelope): the leave finishes on its own, `force-unload` being
        // the recourse.
        verb_table::VerbAction::RefuseOutOfOrder => Err(LifecycleRefusal::OutOfOrder),
        // The invocation lock is this verb's, taken above, so no other
        // holder is seen: the arms below are the table's for other verbs.
        verb_table::VerbAction::Join
        | verb_table::VerbAction::RefuseInvocationInFlight
        | verb_table::VerbAction::RefuseAgentRunning
        | verb_table::VerbAction::RefuseUnanswered
        | verb_table::VerbAction::Start
        | verb_table::VerbAction::DirectSavePoint
        | verb_table::VerbAction::NameForNextLoad
        | verb_table::VerbAction::ShowUnloaded
        | verb_table::VerbAction::ShowWorkersWord
        | verb_table::VerbAction::ShowUnanswered
        | verb_table::VerbAction::ShowInTransition
        | verb_table::VerbAction::ShowInTransitionConstituents => {
            Err(LifecycleRefusal::InvocationInFlight)
        }
    }
}

/// **The leave's window, from its direction** (Spec section 3, the leave's
/// bound; 3.0, the bounds), run from the moment the leave is sent, the
/// observation having spent its own bound. A forced leave has the leave's
/// bound. A graceful one has it past the drain and wind-down bounds the run
/// holds, read from the observation's load facts, the harness's own (so the
/// two agree by construction), with no deadline where either is undeclared,
/// a joining force the recourse. **Where the observation taught no bounds**,
/// a silent harness or a leave pending, the leave's bound alone. The sum
/// saturates: a bound past the clock reads as no deadline.
fn leave_window(
    bounds: UnloadBounds,
    forced: bool,
    worker: verb_table::WorkerSeen,
    load: Option<&weaver_types::LoadFacts>,
) -> std::time::Duration {
    use verb_table::WorkerSeen;
    if forced {
        return bounds.leave;
    }
    match worker {
        WorkerSeen::Idle | WorkerSeen::Active => {
            match load.map(|facts| (facts.drain_bound, facts.wind_down_bound)) {
                Some((Some(drain), Some(wind_down))) => bounds
                    .leave
                    .saturating_add(std::time::Duration::from_secs(drain))
                    .saturating_add(std::time::Duration::from_secs(wind_down)),
                _ => std::time::Duration::MAX,
            }
        }
        WorkerSeen::Silent
        | WorkerSeen::InTransition
        | WorkerSeen::Unloaded
        | WorkerSeen::NoSocket => bounds.leave,
    }
}

/// What the force found at the invocation lock: the lock its own, a sole
/// force, or an `unload`'s leave joined and ended.
enum Taken {
    Sole(start::InvocationLock),
    Joined(LifecycleAnswer),
}

/// How often a join refused `OutOfOrder` is sent again, per Spec section 3.
const JOIN_RETRY: std::time::Duration = std::time::Duration::from_secs(1);

/// **The force takes the lock, or reads its holder's verb and joins**
/// (Spec section 3, the forced unload, sole and joining; 3.0's verb table;
/// I1): the lock taken, it is a sole force; held by an `unload`, it joins
/// lock-free; held by anything else, or by a holder whose record cannot be
/// read, it refuses `InvocationInFlight`. A `show`'s shared hold is waited
/// out by the take itself, and a take that readers starved refuses as every
/// verb's does. A lock that frees between the take and the read, a `show`
/// that takes it there, or a joined holder that gives way to another, is
/// judged afresh.
fn take_or_join(config: &ServiceConfig, bounds: UnloadBounds) -> Result<Taken, LifecycleRefusal> {
    take_or_join_reading(config, bounds, start::read_holder)
}

/// [`take_or_join`] over a holder read, so a test can stand a `show` in the
/// gap between the refused take and the read (#125).
fn take_or_join_reading(
    config: &ServiceConfig,
    bounds: UnloadBounds,
    mut read_holder: impl FnMut(&std::path::Path) -> Result<start::LockHolder, LifecycleRefusal>,
) -> Result<Taken, LifecycleRefusal> {
    let run_directory = config.run_directory();
    loop {
        match start::take_invocation_lock_judged(
            &run_directory,
            verb_table::HolderVerb::ForceUnload,
        ) {
            Ok(lock) => return Ok(Taken::Sole(lock)),
            // The take waited out `show`'s bound before refusing: readers
            // starved it, and it refuses as every verb does.
            Err(start::TakeRefused::Starved) => {
                return Err(LifecycleRefusal::InvocationInFlight);
            }
            Err(start::TakeRefused::Other(LifecycleRefusal::InvocationInFlight)) => {}
            Err(start::TakeRefused::Other(refusal)) => return Err(refusal),
        }
        let seen = match read_holder(&run_directory)? {
            start::LockHolder::Free => continue,
            // **A `show` took the lock after the exclusive holder the take
            // met let go** (#125): the take refused at once and waited for no
            // reader, so it takes again, its wait for readers one bound.
            start::LockHolder::Shared => continue,
            start::LockHolder::Exclusive(verb) => verb_table::Seen::Holder(verb),
        };
        match verb_table::act(verb_table::Verb::ForceUnload, verb_table::class_of(seen)) {
            verb_table::VerbAction::Join => {
                // **The run the force ends is the one the marker stands open
                // on as it joins**, read holding nothing: the conclusion and
                // the escalation's `Forced` close that run only, a load that
                // takes the lock between the holder and the force having
                // opened another (section 4, the marker-write rule (c)).
                // A marker that does not read here is read again under the
                // lock, where the conclusion or the escalation writes.
                let run = open_run(config).ok();
                if let Some(answer) = join_within(config, bounds, &run)? {
                    return Ok(Taken::Joined(answer));
                }
            }
            _ => return Err(LifecycleRefusal::InvocationInFlight),
        }
    }
}

/// **The join** (Spec section 3, the forced unload, joining): `JoinLeave`
/// over the socket, lock-free. Answered `Left`, or finding no socket past the
/// seal, the force waits for the unload's conclusion; refused `OutOfOrder`,
/// the unload not yet having directed its leave, it is sent again every
/// second while the holder's verb stays `unload`, and answers none where the
/// holder gave way, for the caller to judge afresh; unanswered within the
/// leave's bound, a silent harness, it escalates.
fn join_within(
    config: &ServiceConfig,
    bounds: UnloadBounds,
    run: &Option<Option<String>>,
) -> Result<Option<LifecycleAnswer>, LifecycleRefusal> {
    let run_directory = config.run_directory();
    loop {
        // **The join's bound runs from each join sent**: a backlog full
        // behind the harness is a busy worker, not a silent one, and is
        // dialed again inside it.
        let deadline = std::time::Instant::now() + bounds.leave;
        let mut coordination = loop {
            match dial_worker(config) {
                Ok(coordination) => break coordination,
                Err(channel::ChannelFault::NoListener) => {
                    return wait_after_seal(config, bounds, run).map(Some);
                }
                // Not the worker: no join is directed through it, and the
                // force ends the run by the escalation (#98), which acts on
                // the run lock's holders, never on whoever answered.
                Err(channel::ChannelFault::WrongPeer { .. }) => {
                    return escalate_joined(config, bounds, run).map(Some);
                }
                Err(_) if std::time::Instant::now() < deadline => {}
                Err(_) => return escalate_joined(config, bounds, run).map(Some),
            }
        };
        let ordinal = coordination.next_ordinal();
        if coordination
            .send_directive(
                ordinal,
                LifecycleDirective::JoinLeave {
                    cause: invocation_cause(),
                },
            )
            .is_err()
        {
            return escalate_joined(config, bounds, run).map(Some);
        }
        match coordination
            .recv_within(deadline.saturating_duration_since(std::time::Instant::now()))
        {
            Ok(answer) => match LeaveAnswer::of(answer.payload) {
                LeaveAnswer::Left(left) => {
                    return wait_after_left(config, bounds, run, &left).map(Some);
                }
                LeaveAnswer::Refused(LifecycleRefusal::OutOfOrder) => {
                    std::thread::sleep(JOIN_RETRY);
                    if start::read_holder(&run_directory)?
                        != start::LockHolder::Exclusive(verb_table::HolderVerb::Unload)
                    {
                        return Ok(None);
                    }
                }
                // **A join refused past the seal, or answered out of form,
                // concludes as no answer**, as the holder's leave does: the
                // run's processes ended, the marker left as it stands, the
                // refusal returned (I3). An out-of-form answer reads as
                // `Malformed`, as the sole force's does (Codex on #126,
                // round 2).
                LeaveAnswer::Refused(refusal) if follows_the_unwind(&refusal) => {
                    end_after_left(&run_directory, bounds)?;
                    return Err(refusal);
                }
                LeaveAnswer::Other => {
                    end_after_left(&run_directory, bounds)?;
                    return Err(LifecycleRefusal::Malformed);
                }
                LeaveAnswer::Refused(refusal) => return Err(refusal),
            },
            Err(_) => return escalate_joined(config, bounds, run).map(Some),
        }
    }
}

/// **The joined force answered `Left` concludes behind the holder**
/// (Spec 3.0, the conclusion: "it takes the invocation lock first"): it
/// queues on the invocation lock behind the holder, which concludes first,
/// and only then, holding the lock, waits for the run lock (the after-left
/// wait, the processes ended where they outlive it) and concludes by its own
/// `Left`, finding the marker closed where the holder did and changing
/// nothing. **The lock comes first** (Codex on #126, round 1): a force that
/// waited on the run lock first left a gap after the holder let go, in
/// which a `load` could take the lock and start a run the force then
/// answered `Unloaded` over.
fn wait_after_left(
    config: &ServiceConfig,
    bounds: UnloadBounds,
    run: &Option<Option<String>>,
    left: &Left,
) -> Result<LifecycleAnswer, LifecycleRefusal> {
    let run_directory = config.run_directory();
    let _behind =
        start::take_invocation_lock_waiting(&run_directory, verb_table::HolderVerb::ForceUnload)?;
    let run = run_noted(config, run)?;
    if let Some(answer) = behind_another_run(config, bounds, run.as_deref())? {
        return answer;
    }
    end_after_left(&run_directory, bounds)?;
    conclude_answered(config, run.as_deref(), left)
}

/// **Behind the lock, another run's marker is left whole** (Spec 3.0, the
/// forced unload, joining; section 4, the marker-write rule (c); the
/// Planner's ruling of 2026-10-12 on #126): a `load` that wins the lock
/// between the holder and this force, outside the envelope, opens a run the
/// force did not join. Read under the lock before anything is ended: where
/// the marker stands open on another run, the force escalates nothing and
/// writes nothing, answering `Unloaded` where the run lock frees within the
/// after-left wait and `AgentRunning` where it is still held, a run standing
/// that is not the force's, never `Unloaded` over it. Answers none where the
/// marker is this run's, closed or absent, and the force goes on.
fn behind_another_run(
    config: &ServiceConfig,
    bounds: UnloadBounds,
    run: Option<&str>,
) -> Result<Option<Result<LifecycleAnswer, LifecycleRefusal>>, LifecycleRefusal> {
    let another = matches!(
        read_marker_or_refuse(config)?,
        Some(save_points::Marker::Open { run: ref open }) if Some(open.as_str()) != run
    );
    if !another {
        return Ok(None);
    }
    diag!(
        "weaver-admin: the force took the lock behind another run's load; it ends nothing of that run"
    );
    Ok(Some(
        if start::wait_free(&config.run_directory(), bounds.after_left) {
            unloaded_by_ladder(config)
        } else {
            Err(LifecycleRefusal::AgentRunning)
        },
    ))
}

/// The run the joined force noted as it joined, or, where the marker did
/// not read then, the marker read now, under the lock.
fn run_noted(
    config: &ServiceConfig,
    noted: &Option<Option<String>>,
) -> Result<Option<String>, LifecycleRefusal> {
    match noted {
        Some(run) => Ok(run.clone()),
        None => open_run(config),
    }
}

/// **The joined force waits for the unload's conclusion** (Spec section 3,
/// the forced unload, joining; 3.0, S8a x force-unload), in two waits split
/// by the run lock: while it is held, at most the leave's bound from here,
/// then the escalation; once it is free, the holder publishing, for the
/// invocation lock with no deadline and no escalation. Its answer is the
/// ladder's `Unloaded`.
fn wait_after_seal(
    config: &ServiceConfig,
    bounds: UnloadBounds,
    run: &Option<Option<String>>,
) -> Result<LifecycleAnswer, LifecycleRefusal> {
    let run_directory = config.run_directory();
    if !start::wait_free(&run_directory, bounds.leave) {
        return escalate_joined(config, bounds, run);
    }
    let _behind =
        start::take_invocation_lock_waiting(&run_directory, verb_table::HolderVerb::ForceUnload)?;
    let run = run_noted(config, run)?;
    if let Some(answer) = behind_another_run(config, bounds, run.as_deref())? {
        return answer;
    }
    unloaded_by_ladder(config)
}

/// **The joined force's escalation, in two phases** (Spec section 3, S11;
/// I3): (a) the run's processes ended without the invocation lock, the kill
/// never waiting on it; (b) the lock taken behind the concluding holder and
/// the marker written `Forced` by the marker-write rule, no `Left` being
/// held: a marker the holder closed for the run, or another run's, is left
/// alone.
fn escalate_joined(
    config: &ServiceConfig,
    bounds: UnloadBounds,
    run: &Option<Option<String>>,
) -> Result<LifecycleAnswer, LifecycleRefusal> {
    let run_directory = config.run_directory();
    start::escalate_within(&run_directory, bounds.term, bounds.kill)?;
    let _behind =
        start::take_invocation_lock_waiting(&run_directory, verb_table::HolderVerb::ForceUnload)?;
    let run = run_noted(config, run)?;
    if let Some(answer) = behind_another_run(config, bounds, run.as_deref())? {
        return answer;
    }
    // **The run end before the marker** (6.1; 3.0, every row that prints
    // `Unloaded` passes the run end first), under the lock, never lock-free.
    // **The escalation concludes the record's run** (6.1, ownership is the
    // record's, never the marker's): what it ended is whatever held the run
    // lock, and the cgroup they ran in is the one the record names, a load
    // killed between its record and its marker leaving the marker on an
    // earlier run. Another load that stands is caught above, by the marker.
    end_agent_cgroup(config, "force-unload", agent_cgroup::Concluding::Recorded)?;
    write_by_rule(config, run.as_deref(), conclusion::MarkerWrite::Forced)?;
    unloaded_by_ladder(config)
}

/// **The leave directed and concluded**, per Spec section 3: `forced` names
/// the leave's kind, and `escalate_out_of_order` the force on a pending leave
/// or a run never entered, where an `OutOfOrder` refusal is answered by the
/// escalation and a `Forced` marker rather than returned.
fn leave_within(
    config: &ServiceConfig,
    bounds: UnloadBounds,
    window: std::time::Duration,
    forced: bool,
    escalate_out_of_order: bool,
) -> Result<LifecycleAnswer, LifecycleRefusal> {
    let run_directory = config.run_directory();
    match direct_leave_within(config, window, LeaveDirected::of_unload(forced)) {
        Ok(left) => {
            // **A leave whose lock outlives the after-left wait keeps its
            // save point** (the #94 survey's S7): the escalation ends the
            // holders, and the `Left` concludes by its row as on a lock that
            // freed, so `Unloaded` is never answered over a leave save
            // point left unpublished in the room.
            end_after_left(&run_directory, bounds)?;
            let run = open_run(config)?;
            return conclude_answered(config, run.as_deref(), &left);
        }
        Err(LeaveFault::Refused(LifecycleRefusal::OutOfOrder)) if escalate_out_of_order => {}
        // **A leave refused in S7 concludes as no answer** (3.0, the
        // conclusion): `DescriptorsUnusable`, or any first refusal of an
        // unwind run whole, answered at the seal with the worker exiting,
        // publishes nothing and leaves the marker open, forced or not, the
        // run's processes ended by the escalation where they still stand,
        // and the refusal returned.
        Err(LeaveFault::Refused(refusal)) if follows_the_unwind(&refusal) => {
            end_after_left(&run_directory, bounds)?;
            return Err(refusal);
        }
        // A refusal met with the run standing returns to the operator
        // unchanged: `SavePointNotTaken` above all, on which the unload does
        // not complete, the run staying open with its lock (S9), so the
        // operator retries or forces.
        Err(LeaveFault::Refused(refusal)) => return Err(refusal),
        // **A graceful unload refuses a listener that is not the worker**
        // (#98), the run left standing; a force ends the run by the
        // escalation below, which acts on the run lock's holders and never
        // on whoever answered the socket.
        Err(LeaveFault::WrongPeer) if !forced => {
            return Err(LifecycleRefusal::BoundaryUnverified);
        }
        // The leave went unanswered inside its bound: a worker that
        // would not exit, so the escalation follows.
        Err(LeaveFault::Unanswered | LeaveFault::WrongPeer) => {}
    }
    start::escalate_within(&run_directory, bounds.term, bounds.kill)?;
    // **No `Left` is held, so the escalation writes `Forced`** (3.0, S11;
    // section 4, the marker-write rule), whichever verb escalated: `Forced`
    // names how the run ended, not who ended it, and the next load records
    // `ForcedUnload`. The room's finished files are recovered at that load.
    // The escalation concludes the record's run, as `escalate_joined` says.
    let run = open_run(config)?;
    end_agent_cgroup(config, verb_of(forced), agent_cgroup::Concluding::Recorded)?;
    write_by_rule(config, run.as_deref(), conclusion::MarkerWrite::Forced)?;
    unloaded_by_ladder(config)
}

/// The verb `admin.log` names for an unload.
fn verb_of(forced: bool) -> &'static str {
    if forced { "force-unload" } else { "unload" }
}

/// The answer an unload gives where the run has ended.
fn unloaded_answer() -> LifecycleAnswer {
    LifecycleAnswer::State {
        state: weaver_types::AgentState::Unloaded,
        load: None,
        constituents: Vec::new(),
    }
}

/// The `Left` a leave answered, whole: the save point it names, and the
/// leave's state at its end (`forced`, `no_state`), which the conclusion
/// reads.
#[derive(Debug, Clone, PartialEq)]
struct Left {
    save_point: Option<weaver_types::SavePointReport>,
    forced: bool,
    no_state: bool,
}

/// What a leave's dialer was answered: its `Left`, a refusal, or anything
/// else, which is malformed.
enum LeaveAnswer {
    Left(Left),
    Refused(LifecycleRefusal),
    Other,
}

impl LeaveAnswer {
    fn of(payload: weaver_types::Payload) -> Self {
        match payload {
            weaver_types::Payload::Answer(LifecycleAnswer::Left {
                save_point,
                forced,
                no_state,
            }) => LeaveAnswer::Left(Left {
                save_point,
                forced,
                no_state,
            }),
            weaver_types::Payload::Refusal(refusal) => LeaveAnswer::Refused(refusal),
            _ => LeaveAnswer::Other,
        }
    }
}

/// **A refusal the harness answers after the unwind**, per Spec 3.0 (the
/// conclusion: a leave refused in S7) and `weaver-harness-Spec` section 6:
/// the unwind runs whole, `unload` included, and its first refusal (the
/// drain's `DescriptorsUnusable`, the lower's or a release's) is answered to
/// every dialer at the seal, the worker exiting. The refusals a leave meets
/// with the run standing are named: `OutOfOrder` (never entered, or a leave
/// pending), `SavePointNotTaken` (S9), `ActivityNotAtRest` and
/// `Unauthorized`; any other follows the unwind.
fn follows_the_unwind(refusal: &LifecycleRefusal) -> bool {
    !matches!(
        refusal,
        LifecycleRefusal::OutOfOrder
            | LifecycleRefusal::SavePointNotTaken { .. }
            | LifecycleRefusal::ActivityNotAtRest
            | LifecycleRefusal::Unauthorized
    )
}

/// **The run's processes end within the after-left wait, or are ended**
/// (Spec section 3, the after-left wait; S11's phase (a), lock-free): the
/// worker exits after its answer, and whatever still holds the run lock past
/// the wait is escalated.
fn end_after_left(
    run_directory: &std::path::Path,
    bounds: UnloadBounds,
) -> Result<(), LifecycleRefusal> {
    if !start::wait_free(run_directory, bounds.after_left) {
        start::escalate_within(run_directory, bounds.term, bounds.kill)?;
    }
    Ok(())
}

/// **Any caller answered `Left` concludes, by rule** (Spec 3.0, the
/// conclusion and its outcome table; section 4, the marker-write rule; I1,
/// I2, I4), the invocation lock held and the run lock free. `run` is the run
/// the caller ended, the one the marker stood open on when it acted. **The
/// marker is read first**: open on another run, or closed for this one,
/// clean or forced, it is left alone and the caller answers `Unloaded`, so
/// the conclusion is idempotent per run. Otherwise the `Left` is classified
/// by the table's keys, the reported save point published where its row
/// orders it, and the marker written as its row gives. The rollback's row
/// is the load's (`roll_back`).
fn conclude_answered(
    config: &ServiceConfig,
    run: Option<&str>,
    left: &Left,
) -> Result<LifecycleAnswer, LifecycleRefusal> {
    // **A run no marker names concludes the record's** (6.1, ownership),
    // as the escalation's conclusion does, so this path too passes the run
    // end before it prints.
    let Some(run) = run else {
        end_agent_cgroup(config, "unload", agent_cgroup::Concluding::Recorded)?;
        return unloaded_by_ladder(config);
    };
    // **The run end comes first, under the lock, the run lock free** (6.1;
    // 3.0, the conclusion): it ends the cgroup only where the record names
    // it made for this run, and a refusal publishes nothing and writes no
    // marker. Where the marker-write rule leaves the marker alone, nothing
    // is published and the answer is the ladder's.
    end_agent_cgroup(config, "unload", agent_cgroup::Concluding::Run(run))?;
    if !matches!(
        read_marker_or_refuse(config)?,
        Some(save_points::Marker::Open { run: ref open }) if open == run
    ) {
        return unloaded_by_ladder(config);
    }
    let seen = conclusion::Seen {
        rollback: None,
        territory_judged: !config.territory_unjudged,
        forced: left.forced,
        no_state: left.no_state,
        reported: left.save_point.as_ref().map(|report| conclusion::Reported {
            this_run: report.event_run.0 == run,
        }),
    };
    let Ok(kind) = conclusion::classify(seen) else {
        // A `Left` the table lacks is a defect (section 10): nothing is
        // written, the marker standing open for the next load's reset.
        diag!("weaver-admin: the leave answered a Left the outcome table lacks: {left:?}");
        record(config, "unload", "refused: a Left the outcome table lacks");
        return Err(LifecycleRefusal::Malformed);
    };
    let outcome = match conclusion::conclude(kind) {
        conclusion::Conclusion::Settled(outcome) => outcome,
        conclusion::Conclusion::Publish {
            this_run,
            publication,
        } => {
            // **A report of another run publishes as recovered** (S17, row
            // 5): the room is published with no report, so the file takes no
            // position and no leave arrival.
            let reports: Vec<_> = match publication {
                conclusion::Publication::Reported => left
                    .save_point
                    .iter()
                    .cloned()
                    .map(|report| (report, save_points::Arrival::Leave))
                    .collect(),
                conclusion::Publication::Recovered => Vec::new(),
            };
            let published = publish_from_room(config, &AgentName(config.agent.clone()), &reports);
            let landed = unpublished_leave(&reports, &published).is_none();
            if !this_run {
                diag!(
                    "weaver-admin: the leave reported a save point of another run than the one the marker stands open on; its file publishes as recovered and the marker stays open"
                );
                record(
                    config,
                    "unload",
                    "the leave's save point is not this run's; its file publishes as recovered and the marker stays open",
                );
            }
            conclusion::after_publication(this_run, landed)
        }
    };
    write_by_rule(config, Some(run), outcome.marker)?;
    match outcome.printed {
        conclusion::Printed::Unloaded => unloaded_by_ladder(config),
        conclusion::Printed::SavePointNotTakenPublished => {
            diag!(
                "weaver-admin: the leave's save point did not publish; the marker stays open and the unload does not complete"
            );
            record(
                config,
                "unload",
                "refused: the leave's save point did not publish",
            );
            Err(LifecycleRefusal::SavePointNotTaken {
                missed: weaver_types::SavePointLeg::Published,
            })
        }
        // The load's own row, never classified here.
        conclusion::Printed::LoadsOwnRefusal => Err(LifecycleRefusal::Malformed),
    }
}

/// **Section 4's marker-write rule**, for the writes that close a run: under
/// the invocation lock, the caller's; the marker read first; one naming
/// another run, or already closed for `run`, left alone; otherwise written.
/// `AsFound` and `LeaveOpen` write nothing, and `Open` is the rollback's, not
/// a close.
fn write_by_rule(
    config: &ServiceConfig,
    run: Option<&str>,
    write: conclusion::MarkerWrite,
) -> Result<(), LifecycleRefusal> {
    let marker = |run: &str| match write {
        conclusion::MarkerWrite::Closed => Some(save_points::Marker::Closed { run: run.into() }),
        conclusion::MarkerWrite::Forced => Some(save_points::Marker::Forced { run: run.into() }),
        conclusion::MarkerWrite::AsFound
        | conclusion::MarkerWrite::LeaveOpen
        | conclusion::MarkerWrite::Open => None,
    };
    let Some(run) = run else { return Ok(()) };
    let Some(marker) = marker(run) else {
        return Ok(());
    };
    match read_marker_or_refuse(config)? {
        Some(save_points::Marker::Open { run: open }) if open == run => {}
        _ => return Ok(()),
    }
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
/// refuses or leaves a file refuses `published` again. **The marker is left
/// as it stands** (3.0, S0d and S8b x force-unload): with the run gone,
/// nothing tells a leave whose publication failed from a run that crashed,
/// and a force on a run already ended ended nothing, so the next load
/// records the cause the marker holds, `NoCleanUnload` on an open one: a
/// conservative label, never a false one. A retry after `published` may so
/// record a reset over a leave that did take its save point; the leave's
/// provenance in the marker is the lifecycle act's.
fn conclude_ended(config: &ServiceConfig) -> Result<LifecycleAnswer, LifecycleRefusal> {
    // **An ended run's leftovers are killed whole first** (3.0, S0d; 6.1):
    // the run end of the run its record names, before the publication.
    end_agent_cgroup(config, "unload", agent_cgroup::Concluding::Recorded)?;
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
    unloaded_by_ladder(config)
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
    // #118: a test-only hook, kept beside the production code it observes.
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
// #118: a test-only hook, kept beside the production code it observes.
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
    let mut finished = None;
    let published = save_points::publish_counting(
        std::os::fd::AsFd::as_fd(&room),
        member,
        directory,
        config.file_owner(),
        owner,
        reports,
        &mut finished,
    );
    // **Every scan records the room's count** (S16, the operator's ruling of
    // 2026-10-14 on #140; Spec section 6), so the provisional bound can be
    // judged from what rooms hold.
    if let Some(count) = finished {
        record(config, "publish", &room_count_outcome(count));
    }
    match published {
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

/// The `publish` line's outcome naming the room's finished-name count, in the
/// one form the lifecycle verification reads: `the room holds <n> finished
/// names`, or `more than <bound>` where the scan refused past it.
fn room_count_outcome(count: usize) -> String {
    if count > save_points::ROOM_SCAN_BOUND {
        format!(
            "the room holds more than {} finished names",
            save_points::ROOM_SCAN_BOUND
        )
    } else {
        format!("the room holds {count} finished names")
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

/// Why a directed leave did not answer `Left`.
enum LeaveFault {
    Refused(LifecycleRefusal),
    Unanswered,
    /// The dial met a listener that is not the worker (#98): no leave was
    /// directed through it.
    WrongPeer,
}

/// **The three leaves admin directs**, each one wire shape: the graceful
/// unload's, the force's, and a load's rollback, forced by definition. A
/// rollback that is not forced is never sent, so it cannot be spelled.
#[derive(Clone, Copy)]
enum LeaveDirected {
    Graceful,
    Forced,
    Rollback,
}

impl LeaveDirected {
    /// The verb's leave: forced where the operator forced it.
    fn of_unload(forced: bool) -> Self {
        if forced { Self::Forced } else { Self::Graceful }
    }
}

/// Directs leave, graceful, forced or a rollback, and waits for its answer
/// `window` from the send, the dial spending nothing of it, `Duration::MAX`
/// waiting without a deadline. **The answer names
/// the leave's save point**, per `weaver-admin-harness-contract` section 3, none
/// where none was taken (a rollback, a run with no state to keep, or a forced
/// leave whose save point missed), so the publication that follows carries the
/// event's position.
fn direct_leave_within(
    config: &ServiceConfig,
    window: std::time::Duration,
    leave: LeaveDirected,
) -> Result<Left, LeaveFault> {
    let (forced, rollback) = match leave {
        LeaveDirected::Graceful => (false, false),
        LeaveDirected::Forced => (true, false),
        LeaveDirected::Rollback => (true, true),
    };
    let mut coordination = match dial_worker(config) {
        Ok(coordination) => coordination,
        Err(channel::ChannelFault::WrongPeer { .. }) => return Err(LeaveFault::WrongPeer),
        Err(_) => return Err(LeaveFault::Unanswered),
    };
    let ordinal = coordination.next_ordinal();
    coordination
        .send_directive(
            ordinal,
            LifecycleDirective::Leave {
                cause: invocation_cause(),
                forced,
                rollback,
            },
        )
        .map_err(|_| LeaveFault::Unanswered)?;
    match coordination.recv_within(window) {
        Ok(answer) => match LeaveAnswer::of(answer.payload) {
            LeaveAnswer::Left(left) => Ok(left),
            LeaveAnswer::Refused(refusal) => Err(LeaveFault::Refused(refusal)),
            LeaveAnswer::Other => Err(LeaveFault::Refused(LifecycleRefusal::Malformed)),
        },
        Err(_) => Err(LeaveFault::Unanswered),
    }
}

/// **`stop` is a conveyance and its answer is a relay**, per Spec section 3,
/// under the stop's own bound: a worker that accepts stop and answers nothing
/// is not ended, a stop being no unload, and the verb refuses `Unanswered`.
fn stop(config: &ServiceConfig) -> Result<LifecycleAnswer, LifecycleRefusal> {
    let _invocation =
        start::take_invocation_lock(&config.run_directory(), verb_table::HolderVerb::Stop)?;
    let mut coordination = dial_worker(config).map_err(|fault| match fault {
        channel::ChannelFault::WrongPeer { .. } => LifecycleRefusal::BoundaryUnverified,
        _ => LifecycleRefusal::NoResidency,
    })?;
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
    // **The root existing is the admission, before anything is resolved**
    // (Spec section 9): a name with no root is no agent, and answers
    // `NoSuchAgent` at every verb, a force's included, rather than the
    // `BoundaryUnverified` its absent groups would answer below.
    admit_root(&base.join(&agent.0))?;
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
// #118: a test-only hook, kept beside the production code it observes.
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
// #118: a test-only hook, kept beside the production code it observes.
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
    // **The retired keys first** (#107): a retired name standing as a
    // directory or a link would otherwise meet `judge_entries`' generic
    // refusal and never be named.
    refuse_retired_keys(&root, agent)?;
    judge_entries(&root, owner)?;
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
                "weaver-admin: the territory {} does not judge; the force ends the run without it, publishing nothing, and the marker closes forced where it ends a run",
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
/// itself.
fn admit_root(root: &std::path::Path) -> Result<std::fs::Metadata, LifecycleRefusal> {
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
    Ok(metadata)
}

/// **The admitted root is root's and writable by no group or other.**
fn judge_root(root: &std::path::Path, owner: u32) -> Result<(), LifecycleRefusal> {
    use std::os::unix::fs::MetadataExt;
    let metadata = admit_root(root)?;
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
    // **On the descriptor, failing closed** (#98; the custody audit's G16):
    // a look that errs for any reason but the absence of an entry or of
    // support refuses, never reading as no entry.
    match carries_access_entries_fd(opened.as_fd()) {
        Ok(false) => {}
        Ok(true) => return Err(refuse("carries an access-control entry beyond its mode")),
        Err(_) => return Err(refuse("has access-control entries that cannot be read")),
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

/// **Whether a directory or a file carries an access-control entry, on its
/// descriptor** (the custody audit's G7; #99 area 2, H5): either ACL
/// attribute present is an entry; a filesystem without ACLs, or a node
/// without the attribute, carries none; any other failure to look is
/// answered as the error, never as none.
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
        if acl_entry_answer(size, nix::errno::Errno::last())? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// **One extended-attribute look's answer** (#98, G16): a size is an entry;
/// no such attribute, or no support for them, is none; any other error is
/// the look failing, answered as an error, never as no entry.
fn acl_entry_answer(size: isize, errno: nix::errno::Errno) -> nix::Result<bool> {
    if size >= 0 {
        return Ok(true);
    }
    match errno {
        nix::errno::Errno::ENODATA | nix::errno::Errno::EOPNOTSUPP => Ok(false),
        other => Err(other),
    }
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
/// `operator` and `roles.toml`. Optional: `headroom-bytes`, `library-path`,
/// `load-bound-seconds`, and `shell-binary`, which the inventory requires
/// where the declaration elects the shell (#29). A failure names the value,
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
        shell_binary: optional_path("shell-binary")?,
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
mod tests;

/// **The run directory's `commands/`, removed after the agent's cgroup and its
/// record** (#29, Spec section 6.1), a rollback's run end included. A removal
/// that fails is said in `admin.log`, and the next load removes the stale one
/// before it builds.
fn remove_commands(config: &ServiceConfig, verb: &'static str) {
    if let Err(error) = shell::remove_commands(&config.run_directory()) {
        record(
            config,
            verb,
            &format!(
                "the shell's commands directory did not remove ({error}); the next load removes it"
            ),
        );
    }
}
