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

mod channel;
mod inventory;
mod log;
mod sink;
mod stack;
mod surface;
mod unit;
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
/// root, per Spec section 9: the coordination socket's per-agent name, the
/// log, the unit template with the agent's own binaries, and the declaration
/// beside them. **These are deployment facts the operator installs** - crossing
/// no seam, and **none of them discovered at runtime by searching**. Admin is
/// one agent's organ, on the operator's ruling of 2026-10-01, so it reads this
/// agent's root and nothing shared.
///
/// The operator socket's path left this list with the socket on 2026-08-05.
/// The coordination name stayed and changed hands: the operator places it, the
/// harness binds it, and admin dials it.
struct ServiceConfig {
    /// The agent this configuration is, named by the root it was read from.
    agent: String,
    /// The agent's root, `<base>/<agent>/`, which holds the declaration as
    /// `agent.toml` beside the values below.
    root: PathBuf,
    coordination_root: PathBuf,
    log_path: PathBuf,
    unit: unit::UnitTemplate,
    /// The directory the store's unix socket stands in, per Spec section 9
    /// as of 2026-09-04: read under a service election and otherwise idle.
    /// Optional in the file, the engine's conventional directory standing
    /// where the file is silent.
    state_store_socket: PathBuf,
}

impl ServiceConfig {
    /// The unit a load of this agent starts, per Spec sections 6 and 9: the
    /// template the agent's root names, and the only value `unit::start` takes.
    fn agent_unit(&self, agent: &str) -> unit::AgentUnit {
        unit::AgentUnit::for_agent(&self.unit, agent)
    }

    /// The per-agent socket the harness binds inside the unit's runtime
    /// directory. Admin resolves the same name to dial it, which is the one
    /// value that reaches two crates and the reason the operator's file is
    /// where they agree.
    fn coordination_socket(&self, agent: &str) -> PathBuf {
        self.coordination_root
            .join(unit::runtime_directory_name(agent))
            .join("coordination.sock")
    }
}

fn main() {
    // **The store's second question is answered by this binary under the
    // agent's uid**, per `weaver-admin-Spec` section 4 as of 2026-09-04: the
    // parent sets the variable and three arguments, and the child asks the
    // store and exits with the answer. This is not a verb of the surface,
    // and an invocation that carries the variable serves no verb.
    if std::env::var_os(inventory::PROBE_STORE_VARIABLE).is_some() {
        let arguments: Vec<String> = std::env::args().skip(1).collect();
        let admitted = match arguments.as_slice() {
            [socket, database, role] => {
                inventory::store_admits(std::path::Path::new(socket), database, role)
            }
            _ => Err(std::io::Error::other(
                "the probe takes socket, database, role",
            )),
        };
        std::process::exit(match admitted {
            Ok(true) => 0,
            Ok(false) => 1,
            Err(_) => 2,
        });
    }
    let outcome = run();
    // **The answer is one JSON object on standard output and the exit status
    // agrees with it.** Zero exits an answer and a non-zero status exits a
    // refusal, so a shell reads the status and a tool reads the object and the
    // two never disagree.
    match outcome {
        Ok(answer) => {
            println!("{}", surface::render_answer(&answer));
            std::process::exit(0);
        }
        Err(refusal) => {
            println!("{}", surface::render_refusal(&refusal));
            std::process::exit(surface::EXIT_REFUSED);
        }
    }
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
    let request = surface::parse_arguments(std::env::args().skip(1))?;
    let config = load_service_config(request.agent())?;
    dispatch(&config, request)
}

fn dispatch(
    config: &ServiceConfig,
    request: surface::Request,
) -> Result<LifecycleAnswer, LifecycleRefusal> {
    match request {
        surface::Request::Validate(agent) => validate(config, &agent),
        surface::Request::Load(agent) => load(config, &agent),
        surface::Request::Unload(agent) => unload(config, &agent),
        surface::Request::Stop(agent) => stop(config, &agent),
        // **`show` answers through the observation exchange**, per Spec
        // section 3 as of 2026-09-04: the harness's own word where a worker
        // answers the dial, and `Unloaded` from the absence where none does,
        // which is the one place residency is read and it is read as the
        // absence of a worker and not as a state.
        surface::Request::Show(agent) => show(config, &agent),
    }
}

/// **`validate` is the load's front half**, and it stops at the report: it
/// touches no seam and starts no process, which is what makes `Validated`
/// mean an outcome rather than a transition.
fn validate(
    config: &ServiceConfig,
    agent: &AgentName,
) -> Result<LifecycleAnswer, LifecycleRefusal> {
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
/// are refused.
fn well_formed(agent: &str) -> bool {
    !agent.is_empty()
        && agent
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
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
) -> Option<std::os::fd::OwnedFd> {
    // `none` declines the member, per `weaver-state-PRD` section 4 as of
    // 2026-09-04: nothing is stood, no territory is made, and the harness's
    // end is absent from the enter by the declaration's own word.
    let store = inventory.config.state_store.clone().unwrap_or_default();
    if store.engine == weaver_types::StoreEngine::None {
        return None;
    }
    let binary_directory = config.unit.worker.parent()?;
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
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(territory.join("state.log"));
    let mut member = std::process::Command::new(&binary);
    member
        .args(member_vector(
            &territory,
            &inventory.binding,
            inventory.lineage.is_some(),
            &store,
            &config.state_store_socket,
        ))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null());
    if let Ok(log) = log {
        member.stderr(log);
    } else {
        member.stderr(std::process::Stdio::null());
    }
    // The member's end arrives at the fixed number `weaver-state` adopts.
    // Relocated first where it sits at or below the arming target, because
    // the spawn places the standard streams onto 0 through 2 before any
    // pre-exec closure runs, and the arming itself carries the
    // equal-number corner `weaver-harness-Spec` section 2.2 records,
    // repaired inside `arm_member_end`.
    let member_end = {
        use std::os::fd::AsRawFd;
        if member_end.as_raw_fd() <= 3 {
            match nix::fcntl::fcntl(&member_end, nix::fcntl::FcntlArg::F_DUPFD_CLOEXEC(4)) {
                Ok(raw) => {
                    // SAFETY: F_DUPFD_CLOEXEC answered a fresh descriptor
                    // this process owns, adopted exactly once, and the low
                    // original drops here.
                    unsafe {
                        use std::os::fd::FromRawFd;
                        std::os::fd::OwnedFd::from_raw_fd(raw)
                    }
                }
                Err(_) => return None,
            }
        } else {
            member_end
        }
    };
    let raw_member_end = {
        use std::os::fd::AsRawFd;
        member_end.as_raw_fd()
    };
    unsafe {
        use std::os::unix::process::CommandExt;
        member.pre_exec(move || {
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
    // The drop lives in `inventory::drop_to`, shared with the store probe
    // since issue #675, so the order described above is implemented once.
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
    store: &weaver_types::StateStore,
    store_socket: &std::path::Path,
) -> Vec<std::ffi::OsString> {
    // **The engine leads**, so the member knows which port to stand before
    // it reads a positional, per Spec section 6 as of 2026-09-04, and under
    // the service engine the socket, the database, and the role follow it.
    let mut vector: Vec<std::ffi::OsString> = vec!["--engine".into()];
    vector.push(
        match store.engine {
            weaver_types::StoreEngine::None => "none",
            weaver_types::StoreEngine::Sqlite => "sqlite",
            weaver_types::StoreEngine::Postgres => "postgres",
        }
        .into(),
    );
    if store.engine == weaver_types::StoreEngine::Postgres {
        vector.push("--store-socket".into());
        vector.push(store_socket.as_os_str().to_owned());
        for (flag, value) in [("--database", &store.database), ("--role", &store.role)] {
            if let Some(value) = value {
                vector.push(flag.into());
                vector.push(value.into());
            }
        }
    }
    vector.push(territory.as_os_str().to_owned());
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
) -> std::collections::BTreeMap<String, String> {
    let worker = config.unit.worker.as_path();
    let member = worker
        .parent()
        .map(|directory| directory.join("weaver-state"))
        .unwrap_or_else(|| std::path::PathBuf::from("weaver-state"));
    let spu = config.unit.spu.as_path();
    let mut binaries = vec![worker];
    if member_started {
        binaries.push(member.as_path());
    }
    binaries.push(spu);
    binaries.push(config.unit.gate.as_path());
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
    let identity = inventory::identity_for(agent);
    let source_path = config.root.join("agent.toml");
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
        // The two box facts the store rules read, per Spec section 4 as of
        // 2026-09-04: the member's binary beside the worker's, and the
        // store's socket directory from this crate's own file.
        member_binary: config
            .unit
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
        store_socket: config.state_store_socket.clone(),
        // **The member's own account, looked up from the derived name.** It
        // is read here rather than constructed, for the reason the home is:
        // the uid the spawn drops to, the uid that owns the territory, and
        // the uid the store's first gate answers about are the account
        // database's fact and not this crate's. An absent account is a box
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
    inventory::take_inventory(agent, &source, &boundary)
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

/// `load` runs the charter's seven steps in order, and answers only on a ready
/// aggregate: every other outcome enters the rollback carrying the step's name
/// and answers nothing.
fn load(config: &ServiceConfig, agent: &AgentName) -> Result<LifecycleAnswer, LifecycleRefusal> {
    let mut operations = log::OperationsLog::open(&config.log_path)
        .map_err(|_| LifecycleRefusal::BoundaryUnverified)?;
    let mut standing = verbs::Standing::default();

    let outcome = run_load(config, agent, &mut standing);
    match outcome {
        Ok(()) => {
            // The line names the agent's SPU, per Spec section 8: the
            // `spu-binary` its root names.
            let _ = operations.record(&log::Act {
                verb: "load",
                agent: agent.0.clone(),
                outcome: "ready".into(),
                undone: None,
                spu: Some(config.unit.spu.display().to_string()),
            });
            // Idle is honest here and only here: the enter aggregate came back
            // ready, which is the interior serving and at rest. It is not read
            // from the unit, which could not tell idle from active.
            Ok(LifecycleAnswer::State {
                state: weaver_types::AgentState::Idle,
                load: None,
            })
        }
        Err(refusal) => {
            settle_refused_load(config, agent, &standing, &refusal, &mut operations);
            Err(refusal)
        }
    }
}

/// What a refused load leaves is undone and logged before the refusal is
/// answered: the rollback of what stood, then the failed unit cleared.
///
/// **The unit is cleared by admin, not left for the operator**, on the
/// operator's ruling of 2026-10-01, per `weaver-admin-Spec` section 3. A worker
/// that exited non-zero leaves its unit `failed`, holding the name, and the
/// next load under it would be refused `PriorUnitUnreaped`. The state ask
/// decides, and only `failed` is cleared. The ask runs where this load
/// started a unit, or where its start was refused over a failed one, which
/// covers a unit that failed while serving: that load answers
/// `PriorUnitUnreaped` and the one after it starts. **There is no automatic
/// reload.** The refusal is answered as it stood, and a retry is the
/// operator's next load. The clear is a logged act of the rollback, so the
/// log records that the unit was failed, and the worker's own output stays in
/// the journal, so the clear discards the manager's record of the failure and
/// not the evidence of its cause.
///
/// A function rather than a branch inside `load` so a test drives it with a
/// manager double: `load` reaches it only past an inventory that needs a
/// provisioned account.
fn settle_refused_load(
    config: &ServiceConfig,
    agent: &AgentName,
    standing: &verbs::Standing,
    refusal: &LifecycleRefusal,
    operations: &mut log::OperationsLog,
) {
    // **Rollback walks what stands**, and its account is logged.
    let mut account = verbs::rollback(
        standing,
        // **The leave is directed where a run was entered**, per charter
        // section 5 and Spec section 3. A refused fan-out leaves the harness
        // holding a partial run it will unwind along the seams it fanned out
        // on, and the directive is what asks it to. The account reports
        // whether the ask landed, so a leave that could not be sent is
        // recorded as held rather than claimed.
        || direct_leave(config, agent).is_ok(),
        || unit::stop(&config.unit, &agent.0).is_ok(),
        || true,
    );
    if (standing.unit_started || *refusal == LifecycleRefusal::PriorUnitUnreaped)
        && unit::residency(&config.unit, &agent.0) == unit::Residency::Failed
    {
        let _ = unit::reset_failed(&config.unit, &agent.0);
        account.push(verbs::Undone {
            act: "unit-reset-failed",
            succeeded: unit::residency(&config.unit, &agent.0) != unit::Residency::Failed,
        });
    }
    for undone in &account {
        let _ = operations.record(&log::Act {
            verb: "rollback",
            agent: agent.0.clone(),
            outcome: format!("{refusal:?}"),
            undone: Some(format!(
                "{}:{}",
                undone.act,
                if undone.succeeded { "undone" } else { "held" }
            )),
            spu: None,
        });
    }
}

fn run_load(
    config: &ServiceConfig,
    agent: &AgentName,
    standing: &mut verbs::Standing,
) -> Result<(), LifecycleRefusal> {
    let inventory = take_inventory(config, agent)?;
    let sink = sink::open(&inventory.config.trace_sink)?;
    standing.sink_opened = true;

    // The unit starts bare and binds its own coordination socket as its first
    // act, so nothing is placed into it here.
    // **A start ask that returns non-zero started nothing**, and the status is
    // what carries that. Unit-name uniqueness is the concurrency guard
    // `weaver-admin-systemd-contract` section 5 relies on, so a second load of
    // a live agent fails here, and a discarded status would let it proceed to
    // dial the first load's worker.
    // The socket path is derived here from the same validated name the unit's
    // own name carries, and the worker binds what it is told, so the two
    // agree by construction rather than by two readings of one convention.
    let socket_path = config.coordination_socket(&agent.0);
    started(unit::start(
        &config.agent_unit(&agent.0),
        &inventory.identity,
        &socket_path,
        inventory.config.loop_file.as_deref(),
    ))
    .map_err(|from_status| refusal_for_failed_start(config, &agent.0, from_status))?;
    standing.unit_started = true;

    // **The dial races the worker's bind and the bound covers it.** A bound
    // exceeded is not an absent residency, so the refusal consults the unit's
    // state before returning and carries what the manager said. The path is
    // the one the start ask carried, so admin dials exactly what it told the
    // worker to bind.
    let mut coordination = match channel::dial(&socket_path) {
        Ok(coordination) => coordination,
        Err(_) => return Err(refusal_for_absent_worker(config, &agent.0)),
    };

    // The state member stands after the worker's bind and before the enter,
    // and nothing is waited on: the pair exists before the member does, per
    // the ruling of 2026-08-26. Best-effort whole, per the contract's
    // dead-peer clause - an absent binary or a failed spawn leaves the leg
    // down and the load unrefused, the harness's end below then absent from
    // the enter and the directive carrying the sink alone.
    let state_end = stand_state_member(config, &inventory);
    // **The stack names the binaries this crate started and handed the
    // worker**, per `weaver-admin-harness-contract` section 3: the worker
    // always, the member only where it stood, a declined or failed spawn being
    // a binary admin did not start, and the agent's SPU and the gate always.
    let stack = stack_digests(config, state_end.is_some());

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
                // The resolution happened at the inventory, the one site, so
                // what crosses is the kind decided with what it requires
                // riding inside it.
                binding: inventory.binding.clone(),
                // The resolution site the contract names: an absent
                // declaration becomes the ruled default here, so what
                // crosses is always the election whole and the worker
                // never re-derives an absence.
                state_election: inventory.config.state_election.clone().unwrap_or_default(),
                // The same resolution for the store, per `weaver-types-Spec`
                // section 4 as of 2026-09-04: an absent election is the
                // embedded engine, and the record names what was resolved.
                state_store: inventory.config.state_store.clone().unwrap_or_default(),
                declaration: inventory.declaration.clone(),
                // The lineage the inventory resolved, never the record's
                // path, and the digests of the two binaries this crate
                // starts, per `weaver-admin-harness-contract` section 3 as
                // of 2026-09-04 and issue #432.
                restore: inventory.lineage.clone(),
                stack,
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

    match coordination.recv() {
        Ok(answer) => match answer.payload {
            weaver_types::Payload::Answer(LifecycleAnswer::Ready) => Ok(()),
            weaver_types::Payload::Refusal(refusal) => Err(refusal),
            _ => Err(LifecycleRefusal::Malformed),
        },
        Err(_) => Err(LifecycleRefusal::NoResidency),
    }
    // The connection closes here with the verb: a per-invocation admin holds
    // no standing end.
}

/// Whether a start ask started anything.
///
/// **A start ask that returns non-zero started nothing**, and the status is
/// what carries that. Unit-name uniqueness is the concurrency guard
/// `weaver-admin-systemd-contract` section 5 relies on, so a second load of a
/// live agent fails here, and a discarded status would let that load proceed
/// to dial the first load's worker and direct a second enter at it.
///
/// It is a function rather than a match inside `run_load` so that the decision
/// is reachable by a test: discarding the status compiles, so nothing but a
/// watch catches it.
fn started(asked: std::io::Result<std::process::ExitStatus>) -> Result<(), LifecycleRefusal> {
    match asked {
        Ok(status) if status.success() => Ok(()),
        _ => Err(LifecycleRefusal::BindFailed),
    }
}

/// **A failed start ask is asked about rather than guessed at.**
///
/// The status cannot say which failure it was: `weaver-admin-systemd-contract`
/// section 3 measures a duplicate unit name and a malformed property failing
/// with the same status, differing only in prose this crate does not read. So
/// the refusal comes from the state ask, where `failed` covers one condition,
/// and every other reading keeps the answer the status alone could give.
///
/// It is a function rather than a branch inside `run_load` for the reason
/// `started` is: the decision is reachable by a test.
fn refusal_for_failed_start(
    config: &ServiceConfig,
    agent: &str,
    from_status: LifecycleRefusal,
) -> LifecycleRefusal {
    start_refusal_for_residency(unit::residency(&config.unit, agent), from_status)
}

/// The mapping alone, testable without a manager.
fn start_refusal_for_residency(
    residency: unit::Residency,
    from_status: LifecycleRefusal,
) -> LifecycleRefusal {
    match residency {
        unit::Residency::Failed => LifecycleRefusal::PriorUnitUnreaped,
        _ => from_status,
    }
}

/// **A failed dial is followed by a state ask, so a refusal names the right
/// thing.** A start ask can succeed over a unit that never runs, so the dial's
/// bound is what proves liveness, and the bound alone would report an absent
/// residency where the truth is a unit that is not running.
///
/// What the ask yields is a state and never a reason. `Failed` means one
/// thing; `Inactive` covers a unit that stopped cleanly, one that never
/// existed, and one whose exec never succeeded, so nothing is claimed beyond
/// the value.
/// **Observe one agent**, per `weaver-admin-harness-contract` section 3 as
/// of 2026-09-04: dial the coordination socket and open the exchange, and
/// carry the harness's answer whole. No socket, or no worker answering the
/// dial, is `Unloaded` with no load, read from the absence and never from
/// the unit.
fn observe(
    config: &ServiceConfig,
    agent: &AgentName,
) -> Result<
    (
        weaver_types::AgentState,
        Option<Box<weaver_types::LoadFacts>>,
    ),
    LifecycleRefusal,
> {
    let socket_path = config.coordination_socket(&agent.0);
    let Ok(mut coordination) = channel::dial(&socket_path) else {
        return Ok((weaver_types::AgentState::Unloaded, None));
    };
    let ordinal = coordination.next_ordinal();
    if coordination
        .send_directive(ordinal, LifecycleDirective::Observe)
        .is_err()
    {
        return Ok((weaver_types::AgentState::Unloaded, None));
    }
    match coordination.recv() {
        Ok(answer) => match answer.payload {
            weaver_types::Payload::Answer(LifecycleAnswer::State { state, load }) => {
                Ok((state, load))
            }
            weaver_types::Payload::Refusal(refusal) => Err(refusal),
            _ => Err(LifecycleRefusal::Malformed),
        },
        Err(_) => Ok((weaver_types::AgentState::Unloaded, None)),
    }
}

fn show(config: &ServiceConfig, agent: &AgentName) -> Result<LifecycleAnswer, LifecycleRefusal> {
    admissible(config, agent)?;
    let (state, load) = observe(config, agent)?;
    Ok(LifecycleAnswer::State { state, load })
}

fn refusal_for_absent_worker(config: &ServiceConfig, agent: &str) -> LifecycleRefusal {
    refusal_for_residency(unit::residency(&config.unit, agent))
}

/// The mapping alone, separated from the ask for the reason `unload_answer` is:
/// a decision reachable by a test rather than only by a live manager.
fn refusal_for_residency(residency: unit::Residency) -> LifecycleRefusal {
    match residency {
        // A prior process exited non-zero and its name is still held, which
        // is what refuses a later start under it. Not a bind failure: the
        // state says a process exited non-zero and says nothing about whether
        // it bound, so naming a socket here would assert what the boundary
        // did not, per `weaver-admin-systemd-contract` section 3.
        unit::Residency::Failed => LifecycleRefusal::PriorUnitUnreaped,
        // The unit is running and its socket was not reachable, so what failed
        // is the bind rather than the residency. Reporting no residency here
        // would name the one thing the manager just said was present.
        unit::Residency::Active => LifecycleRefusal::BindFailed,
        unit::Residency::Inactive | unit::Residency::Unknown => LifecycleRefusal::NoResidency,
    }
}

/// `unload` directs leave, stops the unit, and answers only once the stop has
/// been confirmed.
fn unload(config: &ServiceConfig, agent: &AgentName) -> Result<LifecycleAnswer, LifecycleRefusal> {
    take_inventory(config, agent).map(|_| ()).or_else(|e| {
        // A name that does not resolve cannot be unloaded, but a config that
        // has since gone bad must not strand a running worker, so only the
        // name's own refusal stops this verb.
        if matches!(e, LifecycleRefusal::NoSuchAgent) {
            Err(e)
        } else {
            Ok(())
        }
    })?;

    let socket_path = config.coordination_socket(&agent.0);
    let mut coordination =
        channel::dial(&socket_path).map_err(|_| LifecycleRefusal::NoResidency)?;
    let ordinal = coordination.next_ordinal();
    coordination
        .send_directive(ordinal, LifecycleDirective::Leave)
        .map_err(|_| LifecycleRefusal::NoResidency)?;
    match coordination.recv() {
        Ok(answer) => match answer.payload {
            weaver_types::Payload::Answer(LifecycleAnswer::Left) => Ok(()),
            weaver_types::Payload::Refusal(refusal) => Err(refusal),
            _ => Err(LifecycleRefusal::Malformed),
        },
        Err(_) => Err(LifecycleRefusal::NoResidency),
    }?;
    drop(coordination);

    // **A stop that is accepted is not a stop that has happened**, and this
    // verb waits for the difference. An agent reported unloaded while its
    // worker still runs is the one report this verb must never produce.
    let stopped = unit::stop(&config.unit, &agent.0);
    let mut operations = log::OperationsLog::open(&config.log_path)
        .map_err(|_| LifecycleRefusal::BoundaryUnverified)?;
    let residency = unit::residency(&config.unit, &agent.0);
    let _ = operations.record(&log::Act {
        verb: "unload",
        agent: agent.0.clone(),
        outcome: residency.as_str().into(),
        undone: None,
        spu: None,
    });
    // **The state ask decides and the stop ask's status does not**, per Spec
    // section 6. That status returns the same value for a unit that is still
    // there as for one already gone, and the second is the ordinary end of a
    // clean unload: the leave confirmed above causes the worker to exit and a
    // transient unit is collected the moment its main process does, so the
    // stop that follows names a unit the manager no longer knows. The status
    // is still read, because the log records what the ask returned.
    let _ = stopped;
    unload_answer(residency)
}

/// The unload's answer from the state ask alone, split out so the rule is
/// reachable by a test rather than reachable only by unloading a live agent.
///
/// **Two of the four cases refuse and the reason is one rule**: an agent
/// reported unloaded while its worker still runs is the report this verb must
/// never produce, per `weaver-admin-Spec` section 6. `Active` is that case
/// directly. `Unknown` is the ask itself having failed, so the verb cannot
/// tell, and answering unloaded over a state nobody could read would risk the
/// same report by another route. The Spec's clause names `active` because that
/// is the case it was written against, and the rule behind it is what carries
/// `Unknown` here.
///
/// `Inactive` and `Failed` both mean the worker is not running, which is the
/// outcome charter step 2 asks for. A unit that failed is reported by the log,
/// which carries the residency's own name, rather than by refusing an unload
/// that achieved what it directed.
fn unload_answer(residency: unit::Residency) -> Result<LifecycleAnswer, LifecycleRefusal> {
    match residency {
        unit::Residency::Active => Err(LifecycleRefusal::ActivityNotAtRest),
        unit::Residency::Unknown => Err(LifecycleRefusal::BindFailed),
        unit::Residency::Inactive | unit::Residency::Failed => Ok(LifecycleAnswer::State {
            state: weaver_types::AgentState::Unloaded,
            load: None,
        }),
    }
}

/// Direct one leave over a fresh dial, for the rollback path.
///
/// **Admin is per-invocation, so the connection the load used is gone by the
/// time the rollback runs.** Dialing again is what every verb does, and the
/// worker is still there to answer: a refused fan-out leaves the harness
/// holding the partial run rather than exiting, which is what makes the
/// directive reach something.
///
/// A dial that fails is not an error to report upward. It means the worker is
/// already gone, so the run it held is gone with it, and the rollback's
/// account records the leave as held rather than done. That is the truthful
/// reading, and the act after this one stops the unit either way.
fn direct_leave(config: &ServiceConfig, agent: &AgentName) -> Result<(), LifecycleRefusal> {
    let socket_path = config.coordination_socket(&agent.0);
    let mut coordination =
        channel::dial(&socket_path).map_err(|_| LifecycleRefusal::NoResidency)?;
    let ordinal = coordination.next_ordinal();
    coordination
        .send_directive(ordinal, LifecycleDirective::Leave)
        .map_err(|_| LifecycleRefusal::NoResidency)?;
    match coordination.recv() {
        Ok(answer) => match answer.payload {
            weaver_types::Payload::Answer(LifecycleAnswer::Left) => Ok(()),
            weaver_types::Payload::Refusal(refusal) => Err(refusal),
            _ => Err(LifecycleRefusal::Malformed),
        },
        Err(_) => Err(LifecycleRefusal::NoResidency),
    }
}

/// `stop` is a conveyance and its answer is a relay. Admin holds no opinion
/// about which fate the harness reports: authorizing a stop and deciding what
/// a stop found are different acts, and the second is the harness's.
fn stop(config: &ServiceConfig, agent: &AgentName) -> Result<LifecycleAnswer, LifecycleRefusal> {
    // This verb reaches no inventory, so the name check is called directly:
    // a path is built from the name below, and an unchecked name would
    // traverse out of the runtime root.
    admissible(config, agent)?;
    let socket_path = config.coordination_socket(&agent.0);
    let mut coordination =
        channel::dial(&socket_path).map_err(|_| LifecycleRefusal::NoResidency)?;
    let ordinal = coordination.next_ordinal();
    coordination
        .send_directive(ordinal, LifecycleDirective::Stop)
        .map_err(|_| LifecycleRefusal::NoResidency)?;
    match coordination.recv() {
        // The relay is a function rather than a bare return so that
        // "unchanged" is a property one place holds and one test reads.
        Ok(answer) => match answer.payload {
            weaver_types::Payload::Answer(answer) => Ok(verbs::relay_stop_answer(answer)),
            weaver_types::Payload::Refusal(refusal) => Err(refusal),
            _ => Err(LifecycleRefusal::Malformed),
        },
        Err(_) => Err(LifecycleRefusal::NoResidency),
    }
}

/// The service configuration is the operator's to place. Its shape is section
/// 11's open election, so this reads the minimum the modules need and refuses
/// rather than searching for a default.
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

/// The admission and the read under a given base and owner, so a test can judge a
/// root it made; every invocation passes the environment's base and root's uid.
/// **A root with no declaration is no agent, for every verb**: `agent.toml` must be
/// a regular file, a link followed to one, judged here with the root, so `show` and
/// `stop`, which take no inventory, refuse a root without one as `load` and
/// `validate` do. A directory or a dangling link at the name is no declaration.
fn load_service_config_at(
    base: &std::path::Path,
    agent: &str,
    owner: u32,
) -> Result<ServiceConfig, LifecycleRefusal> {
    let root = base.join(agent);
    judge_root(&root, owner)?;
    match std::fs::metadata(root.join("agent.toml")) {
        Ok(declaration) if declaration.is_file() => {}
        _ => return Err(LifecycleRefusal::NoSuchAgent),
    }
    load_service_config_from(&root, agent)
        .map_err(|_| LifecycleRefusal::ConfigInvalid { field: None })
}

/// **The agent's root is admitted only as a directory `owner` holds that no
/// other principal may write**, per Spec section 9: absent, or not a
/// directory, is no such agent, and a root another uid owns or a group or the
/// world may write is a boundary this invocation cannot vouch for. The owner
/// is a parameter so a test can judge a directory it made; every invocation
/// passes root's uid. The look does not follow a link at the root itself, so
/// what is judged is what is read.
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

/// The agent's configuration read from its root, so a test can hand one in
/// without the process environment.
fn load_service_config_from(root: &std::path::Path, agent: &str) -> Result<ServiceConfig, String> {
    // A required value refuses where it is absent and where it does not read,
    // and the message says which, a file that stands and fails not being one
    // the operator never placed.
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
    // **An optional value is absent only where nothing stands at its path.** Any
    // other failure - a directory, bytes that are not UTF-8, a read the kernel
    // refuses - is the operator's file failing to read, and it fails the
    // invocation before any verb rather than reading as absent and standing a
    // default the operator did not choose, per Spec section 9.
    // **Presence is `symlink_metadata`'s answer**: nothing at the path is
    // absence, and anything there, a dangling link included, is read and fails
    // if it does not read, a link's missing target answering `NotFound` to the
    // read itself.
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
    let worker = PathBuf::from(read("worker-binary")?);
    // Required rather than defaulted, on the same ground as every other value
    // here: a missing one refuses and names itself rather than being searched
    // for, per Spec section 9. `spu-binary` is this agent's SPU.
    let spu = PathBuf::from(read("spu-binary")?);
    let gate = PathBuf::from(read("gate-binary")?);
    stack::judge_names(&worker, &gate, &spu)?;
    Ok(ServiceConfig {
        agent: agent.to_string(),
        root: root.to_path_buf(),
        coordination_root: PathBuf::from(read("coordination-root")?),
        log_path: PathBuf::from(read("log-path")?),
        unit: unit::UnitTemplate {
            run_tool: read("run-tool")?,
            control_tool: read("control-tool")?,
            properties: optional("unit-properties")?
                .unwrap_or_default()
                .lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect(),
            worker,
            spu,
            gate,
            // Optional, unlike the binaries above: an installation that states
            // no headroom leaves the organ's compiled default standing.
            headroom_bytes: optional("headroom-bytes")?.filter(|v| !v.is_empty()),
        },
        state_store_socket: optional("state-store-socket")?
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(inventory::STORE_SOCKET_DIRECTORY)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let embedded = weaver_types::StateStore::default();
        let socket = std::path::Path::new("/run/postgresql");
        let serving_binding = weaver_types::EnterBinding::Serving {
            gate_instruction: weaver_types::GateInstruction {
                access_rule: weaver_types::AccessRule {
                    allowed_uids: Default::default(),
                    allowed_gids: Default::default(),
                    denied_uids: Default::default(),
                },
            },
        };
        let serving = member_vector(territory, &serving_binding, false, &embedded, socket);
        assert_eq!(
            serving.len(),
            3,
            "a serving load carries the engine pair and the territory alone"
        );
        assert_eq!(serving[0], "--engine");
        assert_eq!(serving[1], "sqlite");
        assert_eq!(serving[2], territory.as_os_str());
        let diagnostic = member_vector(
            territory,
            &weaver_types::EnterBinding::Diagnostic,
            false,
            &embedded,
            socket,
        );
        assert_eq!(
            diagnostic.len(),
            4,
            "a diagnostic load carries the preload path"
        );
        assert_eq!(diagnostic[2], territory.as_os_str());
        assert_eq!(
            diagnostic[3],
            territory.join("preload.sock").into_os_string(),
            "the territory with the fixed leaf, no invocation input composing it"
        );
        // **A serving load that elects a restore names the door too**, per
        // Spec section 6 as of 2026-09-04 and issue #432, the same arm.
        let restoring = member_vector(territory, &serving_binding, true, &embedded, socket);
        assert_eq!(
            restoring.len(),
            4,
            "a restoring serving load carries the preload path"
        );
        assert_eq!(
            restoring[3],
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
        let without = stack_digests(&config, false);
        let names: Vec<&str> = without.keys().map(String::as_str).collect();
        assert_eq!(names, ["python-spu.pyz", "weaver-gate", "worker"]);
        let with = stack_digests(&config, true);
        assert_eq!(
            with.len(),
            4,
            "the worker, the member, the SPU and the gate"
        );
        assert!(with.contains_key("weaver-state"));
    }

    /// **The vector carries the SPU the agent's root names**, per Spec
    /// sections 6 and 9. `unit::start` takes an `AgentUnit` alone, and this
    /// watches the one constructor. Perturbation: `for_agent` substituting a
    /// fixed SPU path, and the assertion fails.
    #[test]
    fn the_roots_spu_reaches_the_vector() {
        let config = unread_config();
        let socket = std::path::Path::new("/run/weaver/alpha/coordination.sock");
        let alpha = config.agent_unit("alpha").arguments("alpha", socket, None);
        assert!(
            alpha
                .iter()
                .any(|a| a == "/nonexistent/python/python-spu.pyz"),
            "{alpha:?}"
        );
    }

    /// The values every root carries, written into a scratch root.
    fn write_root(root: &std::path::Path) {
        std::fs::create_dir_all(root).unwrap();
        for (name, text) in [
            ("coordination-root", "/run/weaver"),
            ("log-path", "/var/log/weaver/alpha/admin.log"),
            ("run-tool", "/usr/bin/systemd-run"),
            ("control-tool", "/usr/bin/systemctl"),
            ("worker-binary", "/opt/weaver/bin/worker"),
            ("spu-binary", "/opt/weaver/bin/weaver-spu"),
            ("gate-binary", "/opt/weaver/bin/weaver-gate"),
        ] {
            std::fs::write(root.join(name), text).unwrap();
        }
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
        assert_eq!(config.root.join("agent.toml"), root.join("agent.toml"));
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

    /// **A root with no declaration is no agent, for every verb**, per Spec
    /// section 9: the root's other keys standing do not make one. Found by
    /// Codex on #45, where `show` and `stop`, taking no inventory, read such a
    /// root as an agent, and then a directory or dangling link at the name.
    /// Perturbations: drop the declaration check, or judge presence alone with
    /// `symlink_metadata`, and a case here reads.
    #[test]
    fn a_root_without_a_declaration_is_no_agent() {
        use std::os::unix::fs::PermissionsExt;
        let me = nix::unistd::getuid().as_raw();
        let base =
            std::env::temp_dir().join(format!("weaver-admin-no-decl-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let root = base.join("alpha");
        write_root(&root);
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(
            load_service_config_at(&base, "alpha", me).err(),
            Some(LifecycleRefusal::NoSuchAgent)
        );
        // A directory, and a link that leads nowhere, at the name are no declaration.
        std::fs::create_dir(root.join("agent.toml")).unwrap();
        assert_eq!(
            load_service_config_at(&base, "alpha", me).err(),
            Some(LifecycleRefusal::NoSuchAgent),
            "a directory named agent.toml"
        );
        std::fs::remove_dir(root.join("agent.toml")).unwrap();
        std::os::unix::fs::symlink(root.join("gone"), root.join("agent.toml")).unwrap();
        assert_eq!(
            load_service_config_at(&base, "alpha", me).err(),
            Some(LifecycleRefusal::NoSuchAgent),
            "a dangling link named agent.toml"
        );
        std::fs::remove_file(root.join("agent.toml")).unwrap();
        std::fs::write(root.join("agent.toml"), "").unwrap();
        let config = load_service_config_at(&base, "alpha", me).expect("a declared root reads");
        assert_eq!(config.agent, "alpha");
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
        for name in ["unit-properties", "headroom-bytes", "state-store-socket"] {
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

    /// A manager double for the unit's state: `is-active` reads a state file,
    /// `reset-failed` empties a failed one to `inactive`, and every call is
    /// appended to a calls file. Answers the configuration pointed at it and
    /// the two files.
    fn manager_double(root: &std::path::Path, state: &str) -> (ServiceConfig, PathBuf, PathBuf) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::create_dir_all(root).unwrap();
        let state_file = root.join("state");
        let calls = root.join("calls");
        std::fs::write(&state_file, format!("{state}\n")).unwrap();
        let tool = root.join("control-tool");
        std::fs::write(
            &tool,
            format!(
                "#!/bin/sh\necho \"$1\" >> '{calls}'\ncase \"$1\" in\n  is-active) cat '{state}';;\n  reset-failed) echo inactive > '{state}';;\nesac\n",
                calls = calls.display(),
                state = state_file.display(),
            ),
        )
        .unwrap();
        std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut config = unread_config();
        config.unit.control_tool = tool.to_string_lossy().into_owned();
        config.log_path = root.join("operations.log");
        (config, calls, root.join("operations.log"))
    }

    /// **A failed load leaves its unit cleared, so the next load is not
    /// refused unreaped**, per `weaver-admin-Spec` section 3 and the
    /// operator's ruling of 2026-10-01. The clear is logged as a rollback
    /// act, and the refusal itself is not this function's to change.
    ///
    /// Perturbation: remove the clear from `settle_refused_load` and the
    /// state stays `failed`, which the next start reads as
    /// `PriorUnitUnreaped`.
    #[test]
    fn a_failed_load_clears_its_unit_for_the_next() {
        let root = std::env::temp_dir().join(format!("weaver-admin-reap-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (config, calls, log_path) = manager_double(&root, "failed");
        let agent = AgentName("alpha".into());
        let standing = verbs::Standing {
            unit_started: true,
            ..Default::default()
        };
        let mut operations = log::OperationsLog::open(&log_path).unwrap();
        settle_refused_load(
            &config,
            &agent,
            &standing,
            &LifecycleRefusal::NoResidency,
            &mut operations,
        );
        let residency = unit::residency(&config.unit, &agent.0);
        assert_eq!(residency, unit::Residency::Inactive);
        assert_ne!(
            start_refusal_for_residency(residency, LifecycleRefusal::BindFailed),
            LifecycleRefusal::PriorUnitUnreaped,
            "the next start is not refused unreaped"
        );
        let logged = std::fs::read_to_string(&log_path).unwrap();
        assert!(logged.contains("unit-reset-failed:undone"), "{logged}");
        assert!(
            std::fs::read_to_string(&calls)
                .unwrap()
                .contains("reset-failed")
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// **Only `failed` is cleared, and the state ask decides.** A unit the
    /// manager reports at rest is not asked to reset, and a load that never
    /// reached a start does not touch the manager at all.
    #[test]
    fn a_unit_not_failed_is_not_reset() {
        let root = std::env::temp_dir().join(format!("weaver-admin-noreap-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (config, calls, log_path) = manager_double(&root, "inactive");
        let agent = AgentName("alpha".into());
        let mut operations = log::OperationsLog::open(&log_path).unwrap();
        let started = verbs::Standing {
            unit_started: true,
            ..Default::default()
        };
        settle_refused_load(
            &config,
            &agent,
            &started,
            &LifecycleRefusal::NoResidency,
            &mut operations,
        );
        let asked = std::fs::read_to_string(&calls).unwrap();
        assert!(!asked.contains("reset-failed"), "{asked}");

        std::fs::write(root.join("state"), "failed\n").unwrap();
        std::fs::write(&calls, "").unwrap();
        settle_refused_load(
            &config,
            &agent,
            &verbs::Standing::default(),
            &LifecycleRefusal::ConfigInvalid { field: None },
            &mut operations,
        );
        assert_eq!(
            std::fs::read_to_string(&calls).unwrap(),
            "",
            "a load that started nothing asks nothing"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A configuration whose values are never read by the arm under test.
    /// `dispatch`'s observation arm touches no field, which is what lets this
    /// stand without a filesystem.
    fn unread_config() -> ServiceConfig {
        ServiceConfig {
            state_store_socket: PathBuf::from(inventory::STORE_SOCKET_DIRECTORY),
            coordination_root: PathBuf::from("/nonexistent"),
            log_path: PathBuf::from("/nonexistent/log"),
            agent: "alpha".into(),
            root: PathBuf::from("/nonexistent/admin/alpha"),
            unit: unit::UnitTemplate {
                run_tool: "/bin/false".into(),
                control_tool: "/bin/false".into(),
                properties: vec![],
                worker: PathBuf::from("/nonexistent/bin/worker"),
                spu: PathBuf::from("/nonexistent/python/python-spu.pyz"),
                gate: PathBuf::from("/nonexistent/bin/weaver-gate"),
                headroom_bytes: None,
            },
        }
    }

    /// **`show` constructs no `AgentState`.** The party that knows an
    /// agent's lifecycle state is the harness and no chartered exchange asks
    /// it, so these verbs refuse rather than return a value read from
    /// residency, which is a different fact.
    ///
    /// This tests `dispatch` rather than the binary because the root guard
    /// answers first in an unprivileged suite, so the binary can never reach
    /// this arm. A test that ran the binary here would pass under the very
    /// invention it claims to forbid, which is the never-failing perturbation
    /// apex section 11 counts as worse than no test. That is not hypothetical:
    /// the binary-level form of this test was written first and passed under
    /// the substitution, which is how the flaw was found.
    ///
    /// Perturbation: make the `Show` arm answer with a state read from
    /// `unit::residency` and this test fails. Watched under exactly that
    /// substitution.
    /// **Every verb that builds a path from a name checks the name first.**
    /// `stop` reaches no inventory, so it calls the shared check directly: a
    /// name carrying a separator or `..` would otherwise be interpolated into
    /// the runtime path and traverse out of the root the operator placed.
    ///
    /// Perturbation: remove the `admissible` call from `stop` and the
    /// traversal names below reach `coordination_socket`. Watched under
    /// exactly that removal.
    /// **A start ask that returns non-zero started nothing.** The manager
    /// reports a duplicate unit name that way, which is the concurrency guard
    /// this crate leans on, so a discarded status would let a second load of a
    /// live agent dial the first load's worker.
    ///
    /// Perturbation: replace `started` with a discard and this test fails.
    /// Watched under exactly that removal, which compiles, which is why the
    /// decision is a function rather than a match.
    #[test]
    fn a_non_success_start_starts_nothing() {
        use std::os::unix::process::ExitStatusExt;
        let ok = std::process::ExitStatus::from_raw(0);
        assert_eq!(started(Ok(ok)), Ok(()));
        // The manager's own answer for a duplicate unit name is a non-zero
        // status, measured 2026-08-05.
        let refused = std::process::ExitStatus::from_raw(1 << 8);
        assert_eq!(started(Ok(refused)), Err(LifecycleRefusal::BindFailed));
        assert_eq!(
            started(Err(std::io::Error::other("no such tool"))),
            Err(LifecycleRefusal::BindFailed)
        );
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

    #[test]
    fn show_answers_the_absence_as_unloaded() {
        // **Where no worker answers the dial, the answer is `Unloaded` from
        // the absence and constructs no state from the unit**, per Spec
        // section 3 as of 2026-09-04. Perturbation: map the manager's
        // `active` onto `Idle` in `observe` and a box with the unit running
        // fails this, which is the invention the clause forbids. The
        // coordination root below does not exist, so no socket does.
        let config = unread_config();
        assert_eq!(
            dispatch(&config, surface::Request::Show(AgentName("alpha".into()))),
            Ok(LifecycleAnswer::State {
                state: weaver_types::AgentState::Unloaded,
                load: None
            }),
            "an admitted agent with no socket answers unloaded with no load"
        );
        assert_eq!(
            dispatch(&config, surface::Request::Show(AgentName("nobody".into()))),
            Err(LifecycleRefusal::NoSuchAgent),
            "a name that is not this root's agent refuses as every verb does"
        );
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
    /// `drop_to`'s own instrument, beside the store probe's in the inventory
    /// module, watches the order of the three calls and the saved ids. This
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
                "identity = []\n",
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
        service.unit.worker = bin.join("weaver-worker");

        let harness_end = stand_state_member(&service, &inventory);
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
                eprintln!("SKIP member spawn watch: unshare could not run: {e}");
                return;
            }
        };
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.starts_with("unshare:") {
            eprintln!(
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

#[cfg(test)]
mod unload_answer_tests {
    use super::*;

    /// **A clean unload answers unloaded**, which is the defect this rule
    /// closes: the leave confirmed, the worker gone, and the transient unit
    /// collected with it, so the stop that followed named a unit the manager
    /// no longer knew and the verb refused over an agent that unloaded
    /// exactly as asked.
    ///
    /// Perturbation: let the stop ask's status decide again and every unload
    /// of a live agent refuses. Watched by running the verb, which is how the
    /// defect was found.
    #[test]
    fn a_stopped_unit_answers_unloaded() {
        assert!(matches!(
            unload_answer(unit::Residency::Inactive),
            Ok(LifecycleAnswer::State {
                state: weaver_types::AgentState::Unloaded,
                load: None,
            })
        ));
    }

    /// A unit still running refuses, which is the report this verb must never
    /// produce and the case the clause was written against.
    #[test]
    fn a_running_unit_refuses_not_at_rest() {
        assert!(matches!(
            unload_answer(unit::Residency::Active),
            Err(LifecycleRefusal::ActivityNotAtRest)
        ));
    }

    /// **A state nobody could read refuses too.** The ask having failed is not
    /// evidence the worker stopped, and answering unloaded over it would risk
    /// the same false report by another route.
    #[test]
    fn an_unreadable_state_refuses() {
        assert!(matches!(
            unload_answer(unit::Residency::Unknown),
            Err(LifecycleRefusal::BindFailed)
        ));
    }

    /// **The two conditions stop sharing a word**, per `weaver-admin-Spec`
    /// and the ruling of 2026-08-16. A unit the manager reports `failed` left
    /// a name held, which is what refuses a later start. A unit it reports
    /// `active` with an unreachable socket is a bind that failed.
    ///
    /// Perturbation: answer `BindFailed` for `Failed` and this test fails,
    /// which is the state the defect was found in.
    #[test]
    fn a_failed_prior_unit_is_not_a_bind_failure() {
        assert_eq!(
            refusal_for_residency(unit::Residency::Failed),
            LifecycleRefusal::PriorUnitUnreaped
        );
        assert_eq!(
            refusal_for_residency(unit::Residency::Active),
            LifecycleRefusal::BindFailed,
            "a running unit with an unreachable socket is still a bind failure"
        );
        // Both patterns of the shared arm, not one of them. They answer
        // together today and a later act splitting them would pass a test
        // that named only `Inactive`.
        for absent in [unit::Residency::Inactive, unit::Residency::Unknown] {
            assert_eq!(
                refusal_for_residency(absent),
                LifecycleRefusal::NoResidency,
                "neither reading claims more than an absent residency"
            );
        }
    }

    /// **A failed start ask is asked about rather than guessed at.** The
    /// status cannot say which failure it was, so the state decides, and every
    /// reading but `failed` keeps the answer the status alone could give.
    ///
    /// Perturbation: return `from_status` for every residency and the first
    /// case fails, which is how an operator came to read `bind_failed` over a
    /// healthy socket.
    #[test]
    fn a_failed_start_consults_the_state() {
        let from_status = LifecycleRefusal::BindFailed;
        assert_eq!(
            start_refusal_for_residency(unit::Residency::Failed, from_status.clone()),
            LifecycleRefusal::PriorUnitUnreaped
        );
        for other in [
            unit::Residency::Active,
            unit::Residency::Inactive,
            unit::Residency::Unknown,
        ] {
            assert_eq!(
                start_refusal_for_residency(other, from_status.clone()),
                LifecycleRefusal::BindFailed,
                "nothing but failed is claimed beyond the status"
            );
        }
    }

    /// A unit that ran and exited non-zero is not running, which is the
    /// outcome the step asks for. The log carries the residency's own name, so
    /// the failure is reported rather than swallowed.
    #[test]
    fn a_failed_unit_answers_unloaded() {
        assert!(matches!(
            unload_answer(unit::Residency::Failed),
            Ok(LifecycleAnswer::State {
                state: weaver_types::AgentState::Unloaded,
                load: None,
            })
        ));
    }
}
