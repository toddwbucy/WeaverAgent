//! **The agent's scope** (`weaver-admin-Spec` section 6.1, the scope step,
//! #166): holding both locks and immediately before it makes the agent's
//! cgroup, the start step asks PID1 for the transient scope
//! `weaver-<agent>.scope` in `weaver.slice`, holding its own pid, and makes the
//! agent's cgroup beneath it once its own reading of its cgroup names it.
//!
//! Three units, each its own function, so that another election of the
//! client (section 11) changes [`busctl`] alone:
//! - [`standing`], the read before the request: a scope standing populated
//!   refuses, one standing empty is waited out, one absent goes on,
//! - [`busctl`], the request: an exec of systemd's own `busctl` at a fixed
//!   path, forked before the move and reaped before the confirm (D2),
//! - [`confirm`], the start step's own reading of its cgroup until it names
//!   the scope, within the scope bound, never the call's answer (D1).
//!
//! [`enter`] composes them by the rows [`standing_judgment`] and
//! [`request_judgment`], the scope's state table's events admin causes.

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::containment::{self, Cgroup};

/// The slice every agent's scope stands in (6.1, one scope per agent).
pub const SLICE: &str = "weaver.slice";

/// **The scope bound**, 6.1's election, a ceiling: how long the start step
/// reads for the move after its request, and how long it waits for PID1 to
/// collect a scope standing empty.
pub const SCOPE_BOUND: Duration = Duration::from_secs(10);

/// How often the start step reads for the move or the collection (6.1, the
/// scope bound): the wait ends at the move, so the ceiling costs nothing
/// where PID1 answers.
pub const SCOPE_POLL: Duration = Duration::from_millis(1);

/// **systemd's own `busctl`, at the fixed path** section 11's election names,
/// a host requirement beside the `cgroup2` mount.
pub const BUSCTL: &str = "/usr/bin/busctl";

/// The scope's unit name, the agent's (6.1, one scope per agent: PID1 refuses
/// a second unit of that name, D7).
pub fn unit_name(agent: &str) -> String {
    format!("weaver-{agent}.scope")
}

/// **Where an agent's scope stands** beneath the mount, and the unit name its
/// request carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Site {
    /// The slice's path beneath the mount, `/` first.
    slice_path: String,
    /// The unit the request names.
    unit: String,
}

impl Site {
    /// Production's site: `/weaver.slice/weaver-<agent>.scope`.
    pub fn of(agent: &str) -> Self {
        Site {
            slice_path: format!("/{SLICE}"),
            unit: unit_name(agent),
        }
    }

    /// A site whose slice stands at `slice_path` beneath the mount, for the
    /// instruments, which stand the scope beneath a delegated cgroup of the
    /// suite's own.
    #[cfg(test)]
    pub(crate) fn at(slice_path: &str, agent: &str) -> Self {
        Site {
            slice_path: slice_path.trim_end_matches('/').to_string(),
            unit: unit_name(agent),
        }
    }

    /// The scope's path beneath the mount, as `/proc/<pid>/cgroup` names it.
    pub fn path(&self) -> String {
        format!("{}/{}", self.slice_path, self.unit)
    }
}

/// **The one call**, `StartTransientUnit` in mode `fail`, as the request
/// carries it: the unit, the slice, the pid it holds and how long the client
/// may wait for PID1's answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    pub unit: String,
    pub slice: String,
    pub pid: u32,
    pub timeout: Duration,
}

/// Why the scope step stood nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScopeRefused {
    /// `AgentRunning`: the scope stands populated, a run's leftovers, named
    /// and never killed (I8).
    Leftovers(Vec<i32>),
    /// `BoundaryUnverified`, naming the scope and what the request answered,
    /// or that no move followed, or that the scope stood past the bound.
    Unverified(String),
}

// ------------------------------------------------------------- the rows

/// What the read before the request finds at the scope's path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeStanding {
    Absent,
    /// Standing and `populated 0`: ending, PID1 stopping and collecting it
    /// (P4).
    Empty,
    /// Standing and populated, or its `cgroup.events` not read: a run's
    /// leftovers (P3).
    Populated,
}

/// The start step's answer to what stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StandingAction {
    /// P0: go on to the request.
    Request,
    /// P4: wait within the scope bound for PID1 to collect it, then the
    /// request, or `BoundaryUnverified` where it still stands at the bound.
    AwaitCollection,
    /// P3: `AgentRunning`, naming the processes, no request and nothing
    /// killed (I8).
    RefuseLeftovers,
}

/// **The scope's rows for what stands before the request** (6.1, the
/// scope's state table): absent goes on, empty is waited out, populated
/// refuses.
pub fn standing_judgment(standing: ScopeStanding) -> StandingAction {
    match standing {
        ScopeStanding::Absent => StandingAction::Request,
        ScopeStanding::Empty => StandingAction::AwaitCollection,
        ScopeStanding::Populated => StandingAction::RefuseLeftovers,
    }
}

/// The start step's answer to its request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestAction {
    /// P2: its own reading names the scope. The floor and the agent's
    /// cgroup follow, judging the scope's cgroup.
    GoOn,
    /// The request refused: `BoundaryUnverified`, naming what it answered.
    RefuseAnswered,
    /// The request answered and no move read within the bound:
    /// `BoundaryUnverified`, naming that no move followed (D1: the answer is
    /// never read as the move).
    RefuseNoMove,
}

/// **The scope's rows for the request** (6.1, the scope's state table): only
/// the start step's own reading of the move goes on.
pub fn request_judgment(answered: bool, moved: bool) -> RequestAction {
    match (answered, moved) {
        (true, true) => RequestAction::GoOn,
        (true, false) => RequestAction::RefuseNoMove,
        (false, _) => RequestAction::RefuseAnswered,
    }
}

// ------------------------------------------------------------- the units

/// **The read before the request**: what stands at the scope's path, and
/// the scope's cgroup where one stands, for naming its processes. A path
/// that does not open other than as absent refuses rather than read as
/// absent.
pub fn standing(site: &Site) -> Result<(ScopeStanding, Option<Cgroup>), ScopeRefused> {
    let path = site.path();
    match Cgroup::open(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok((ScopeStanding::Absent, None))
        }
        Err(error) => Err(ScopeRefused::Unverified(format!(
            "the scope {path} does not open: {error}"
        ))),
        Ok(scope) => {
            let standing = match scope.populated() {
                Some(false) => ScopeStanding::Empty,
                _ => ScopeStanding::Populated,
            };
            Ok((standing, Some(scope)))
        }
    }
}

/// **The wait for a scope standing empty**: its path read every
/// [`SCOPE_POLL`] until it is absent, PID1 having collected it, or until
/// `deadline`. Answers whether it was collected.
pub fn await_collection(site: &Site, deadline: Instant) -> bool {
    let path = site.path();
    loop {
        if matches!(Cgroup::open(&path), Err(error) if error.kind() == std::io::ErrorKind::NotFound)
        {
            return true;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return false;
        }
        std::thread::sleep(SCOPE_POLL.min(remaining));
    }
}

/// **The confirm** (D1): the start step's own reading of its cgroup, every
/// [`SCOPE_POLL`] until it names `path` or until `deadline`. Production
/// passes its own pid, so the reading is `/proc/self/cgroup`'s. Answers
/// whether the move read.
pub fn confirm(pid: u32, path: &str, deadline: Instant) -> bool {
    loop {
        if containment::process_cgroup(pid as i32).as_deref() == Some(path) {
            return true;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return false;
        }
        std::thread::sleep(SCOPE_POLL.min(remaining));
    }
}

/// **The request, section 11's election**: an exec of systemd's own
/// `busctl` at [`BUSCTL`], with no environment and no standard input.
/// `output` waits for it, so the helper, admin's own and root's, is forked
/// before the move and reaped before the confirm reads (D2). Answers the
/// reply where it succeeds, and what it said where it fails.
pub fn busctl(call: &Call) -> Result<String, String> {
    let output = Command::new(BUSCTL)
        .args(busctl_arguments(call))
        .env_clear()
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("{BUSCTL} does not run: {error}"))?;
    let said = |bytes: &[u8]| String::from_utf8_lossy(bytes).trim().to_string();
    if output.status.success() {
        Ok(said(&output.stdout))
    } else {
        Err(format!(
            "{BUSCTL} {}: {}",
            output.status,
            said(&output.stderr)
        ))
    }
}

/// **The call's arguments** (6.1, Where): `StartTransientUnit` on
/// `org.freedesktop.systemd1.Manager` in mode `fail`, with the properties
/// `PIDs` (the one pid), `Delegate=yes`, `Slice` and
/// `CollectMode=inactive-or-failed`, and no auxiliary units. The client's
/// timeout is the call's, so a stalled PID1 holds the request no longer.
pub fn busctl_arguments(call: &Call) -> Vec<String> {
    let pid = call.pid.to_string();
    let timeout = format!("--timeout={}ms", call.timeout.as_millis().max(1));
    [
        "--system",
        timeout.as_str(),
        "call",
        "org.freedesktop.systemd1",
        "/org/freedesktop/systemd1",
        "org.freedesktop.systemd1.Manager",
        "StartTransientUnit",
        "ssa(sv)a(sa(sv))",
        call.unit.as_str(),
        "fail",
        "4",
        "PIDs",
        "au",
        "1",
        pid.as_str(),
        "Delegate",
        "b",
        "true",
        "Slice",
        "s",
        call.slice.as_str(),
        "CollectMode",
        "s",
        "inactive-or-failed",
        "0",
    ]
    .iter()
    .map(|argument| argument.to_string())
    .collect()
}

// ------------------------------------------------------------- the step

/// **The scope step** (6.1): the read before the request, the wait for a
/// scope standing empty, the request for `pid`, and the confirm, each
/// within `bound`, judged by the rows. Answers the scope's cgroup, opened
/// once the start step's own reading names it, for the floor to judge.
/// Production passes [`Site::of`] the agent, its own pid, [`busctl`] and
/// [`SCOPE_BOUND`]. The instruments pass a site beneath their own delegated
/// cgroup and a fake PID1.
pub fn enter(
    site: &Site,
    pid: u32,
    request: impl FnOnce(&Call) -> Result<String, String>,
    bound: Duration,
) -> Result<Cgroup, ScopeRefused> {
    let path = site.path();
    let (found, scope) = standing(site)?;
    match standing_judgment(found) {
        StandingAction::RefuseLeftovers => {
            return Err(ScopeRefused::Leftovers(
                scope
                    .as_ref()
                    .map(crate::agent_cgroup::pids)
                    .unwrap_or_default(),
            ));
        }
        StandingAction::AwaitCollection => {
            drop(scope);
            if !await_collection(site, Instant::now() + bound) {
                return Err(ScopeRefused::Unverified(format!(
                    "the scope {path} stands empty past the scope bound ({} ms): PID1 has not collected it",
                    bound.as_millis()
                )));
            }
        }
        StandingAction::Request => {}
    }
    let deadline = Instant::now() + bound;
    let call = Call {
        unit: site.unit.clone(),
        slice: SLICE.to_string(),
        pid,
        timeout: bound,
    };
    let answered = request(&call);
    let moved = answered.is_ok() && confirm(pid, &path, deadline);
    match request_judgment(answered.is_ok(), moved) {
        RequestAction::GoOn => Cgroup::open(&path).map_err(|error| {
            ScopeRefused::Unverified(format!(
                "the scope {path} does not open after the move: {error}"
            ))
        }),
        RequestAction::RefuseAnswered => Err(ScopeRefused::Unverified(format!(
            "the request for the scope {path} was refused: {}",
            answered.err().unwrap_or_default()
        ))),
        RequestAction::RefuseNoMove => Err(ScopeRefused::Unverified(format!(
            "the request for the scope {path} answered {} and no move followed within the scope bound ({} ms)",
            answered.unwrap_or_default(),
            bound.as_millis()
        ))),
    }
}

#[cfg(test)]
mod tests;
