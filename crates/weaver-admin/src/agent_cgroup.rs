//! **The agent's cgroup, as the verbs meet it** (`weaver-admin-Spec`
//! section 6.1, #111): the start step's making, the run end and the read
//! `show`'s ladder takes, each composed from `containment`'s rows and
//! primitives. The rows decide and these functions only gather the facts a
//! row reads and act on its answer, so every judgment stays in one place.
//!
//! "Root's" is the invoking uid's, root in production: the instruments run
//! the same code under a delegated scope of the suite's own account, with a
//! fake PID1 standing in for the scope's request (`scope`).

use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::path::Path;
use std::time::Duration;

use nix::fcntl::OFlag;
use nix::sys::stat::{Mode, fstat};

use crate::containment::{
    self, Cgroup, MadeAction, MadeStep, Record, RecordAction, RecordAtStart, RecordedAt,
    RunEndAction, RunEndStep, Standing, StartAction, StartFacts, StartRefusal,
};
use crate::scope::{self, ScopeRefused, Site};

/// The agent's cgroup's name beneath its scope, named for the agent's
/// account.
pub fn cgroup_name(agent: &str) -> String {
    format!("weaver-{agent}")
}

/// Opens the run directory the record stands in.
pub fn open_run_directory(run_directory: &Path) -> std::io::Result<OwnedFd> {
    nix::fcntl::open(
        run_directory,
        OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC | OFlag::O_RDONLY,
        Mode::empty(),
    )
    .map_err(std::io::Error::from)
}

/// Every pid of a cgroup and its descendants, sorted and each named once,
/// for naming: a listing that fails names fewer. Never read as emptiness,
/// which `populated` answers.
pub fn pids(cgroup: &Cgroup) -> Vec<i32> {
    let mut pids: Vec<i32> = cgroup
        .subtree()
        .unwrap_or_default()
        .iter()
        .flat_map(|inner| inner.procs().unwrap_or_default())
        .collect();
    pids.sort_unstable();
    pids.dedup();
    pids
}

/// What stands in a cgroup the start step met: not root's, empty, or
/// populated, a failed read of `cgroup.events` read as populated.
fn standing_of(cgroup: &Cgroup) -> Standing {
    match fstat(cgroup.fd()) {
        Ok(stat) if stat.st_uid == nix::unistd::geteuid().as_raw() => match cgroup.populated() {
            Some(false) => Standing::Empty,
            _ => Standing::Populated,
        },
        _ => Standing::NotRoots,
    }
}

/// The parent of a recorded path and the cgroup's name, for the removal.
fn parent_and_name(relative: &str) -> Option<(String, String)> {
    let (parent, name) = relative.rsplit_once('/')?;
    let parent = if parent.is_empty() { "/" } else { parent };
    Some((parent.to_string(), name.to_string()))
}

// ------------------------------------------------------------- the start

/// What the start step made, held until the constituents stand: the record
/// and the two `cgroup.procs` descriptors the constituents' children move
/// themselves through, opened before any fork.
#[derive(Debug)]
pub struct Made {
    pub record: Record,
    constituents: OwnedFd,
    run: OwnedFd,
}

impl Made {
    /// `constituents/cgroup.procs`: the state member and the trace relay.
    pub fn constituents_procs(&self) -> BorrowedFd<'_> {
        self.constituents.as_fd()
    }

    /// `worker/run/cgroup.procs`: the worker, and every organ it forks.
    pub fn run_procs(&self) -> BorrowedFd<'_> {
        self.run.as_fd()
    }
}

/// Why the start step stood nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartRefused {
    /// `AgentRunning` at the run lock, a run standing. The load takes the
    /// lock before the start step, so this answers only a caller that
    /// passes it held.
    RunLockHeld,
    /// `AgentRunning`: an ended run's leftovers, named and never killed (I8).
    Leftovers(Vec<i32>),
    /// `BoundaryUnverified`, naming the cgroup and which question failed.
    Unverified(String),
}

/// **The start step's making of the agent's cgroup** (6.1), holding both
/// locks and before any fork: the scope step, which asks PID1 for the
/// agent's scope holding this process and confirms the move by its own
/// reading; then, beneath the scope, a temporary record left behind
/// removed; the floor's mount and the scope's delegate mark and ownership;
/// what stands at the deterministic path and what the record names, judged
/// by `start_judgment`; then the cgroup made, its `cgroup.kill` required, the
/// layout made `0755` by `fchmod` with the three files set to `owner`, and the
/// record written for `run`. Each failure after the `mkdir` removes what it
/// made, by `made_judgment`.
pub fn make(
    run_directory: BorrowedFd<'_>,
    agent: &str,
    run: &str,
    owner: (u32, u32),
) -> Result<Made, StartRefused> {
    make_in_scope(
        &Site::of(agent),
        std::process::id(),
        scope::busctl,
        scope::SCOPE_BOUND,
        run_directory,
        agent,
        run,
        owner,
        ROOT,
    )
}

/// The owner the floor requires of the scope's cgroup in production.
const ROOT: u32 = 0;

/// **[`make`]'s seam**: the scope step for `pid` at `site` by `request`
/// within `bound`, then [`make_beneath`] the scope's cgroup it answers.
/// Production passes the agent's site, its own pid, `busctl`, the scope
/// bound and root. The instruments pass a site beneath their own delegated
/// cgroup, a fake PID1 and their own uid (6.1, the instruments: the scope
/// step, the confirm, the standing scope).
#[allow(clippy::too_many_arguments)]
pub fn make_in_scope(
    site: &Site,
    pid: u32,
    request: impl FnOnce(&scope::Call) -> Result<String, String>,
    bound: Duration,
    run_directory: BorrowedFd<'_>,
    agent: &str,
    run: &str,
    owner: (u32, u32),
    floor_owner: u32,
) -> Result<Made, StartRefused> {
    let held = scope::enter(site, pid, request, bound).map_err(|refused| match refused {
        ScopeRefused::Leftovers(pids) => StartRefused::Leftovers(pids),
        ScopeRefused::Unverified(why) => StartRefused::Unverified(why),
    })?;
    make_beneath(Ok(held), run_directory, agent, run, owner, floor_owner)
}

/// **The root instruments' making**: the agent's cgroup beneath the cgroup
/// the instrument runs in, a root-owned delegated scope the operator stands
/// it in, judged by the floor as the agent's scope is, so those instruments
/// never ask the system's PID1 for a scope.
#[cfg(test)]
pub(crate) fn make_beneath_own_cgroup(
    run_directory: BorrowedFd<'_>,
    agent: &str,
    run: &str,
    owner: (u32, u32),
) -> Result<Made, StartRefused> {
    let own = containment::own_cgroup().and_then(|path| Cgroup::open(&path));
    make_beneath(own, run_directory, agent, run, owner, ROOT)
}

/// The agent's cgroup made beneath a named cgroup, the agent's scope in
/// production, so an instrument can judge the floor on a cgroup of its own.
/// `floor_owner` is the owner the floor requires of the scope's directory
/// and its delegated files: root in production, the suite's own uid under a
/// user scope, which is the level the instruments reach.
pub fn make_beneath(
    scope: std::io::Result<Cgroup>,
    run_directory: BorrowedFd<'_>,
    agent: &str,
    run: &str,
    owner: (u32, u32),
    floor_owner: u32,
) -> Result<Made, StartRefused> {
    let name = cgroup_name(agent);
    let _ = nix::unistd::unlinkat(
        run_directory,
        containment::RECORD_TEMPORARY,
        nix::unistd::UnlinkatFlags::NoRemoveDir,
    );
    let cgroup2 = containment::is_cgroup2();
    let delegated = scope.as_ref().is_ok_and(containment::delegated);
    let scope_roots = scope
        .as_ref()
        .is_ok_and(|held| containment::owned_by(held, floor_owner));
    let scope_path = scope.as_ref().map_or_else(
        |_| "an unread cgroup".to_string(),
        |held| held.relative().to_string(),
    );
    // What stands at the deterministic path: absent only where the open
    // answers `ENOENT`, any other failure refusing rather than reading as
    // absent.
    let at_path_cgroup = match scope.as_ref().map(|held| held.child(&name)) {
        Ok(Ok(cgroup)) => Some(cgroup),
        Ok(Err(error)) if error.kind() != std::io::ErrorKind::NotFound => {
            return Err(StartRefused::Unverified(format!(
                "the cgroup {name} beneath {scope_path} does not open: {error}"
            )));
        }
        Ok(Err(_)) | Err(_) => None,
    };
    let at_path = at_path_cgroup
        .as_ref()
        .map_or(Standing::Absent, standing_of);
    // The record's cgroup is judged once, and opened once where it is the
    // recorded one.
    let record = containment::read_record(run_directory, agent);
    let (recorded, recorded_cgroup) = match &record {
        Err(()) => (RecordAtStart::Unreadable, None),
        Ok(None) => (RecordAtStart::None, None),
        Ok(Some(record)) => match containment::recorded_at(record) {
            RecordedAt::Absent => (RecordAtStart::Absent, None),
            RecordedAt::Other => (RecordAtStart::Other, None),
            RecordedAt::Unreadable => (RecordAtStart::Unreadable, None),
            RecordedAt::Same => match Cgroup::open(&record.path) {
                Ok(cgroup) => (RecordAtStart::Same(standing_of(&cgroup)), Some(cgroup)),
                Err(_) => (RecordAtStart::Unreadable, None),
            },
        },
    };
    let facts = StartFacts {
        // Taken by this invocation before the start step runs (section 3).
        run_lock_held: false,
        cgroup2,
        delegated,
        scope_roots,
        at_path,
        recorded,
    };
    match containment::start_judgment(facts) {
        StartAction::Refuse(refusal) => {
            return Err(match refusal {
                StartRefusal::RunLockHeld => StartRefused::RunLockHeld,
                StartRefusal::Leftovers => {
                    let mut standing: Vec<i32> = at_path_cgroup
                        .iter()
                        .chain(recorded_cgroup.iter())
                        .flat_map(pids)
                        .collect();
                    standing.sort_unstable();
                    standing.dedup();
                    StartRefused::Leftovers(standing)
                }
                StartRefusal::NoCgroup2 => StartRefused::Unverified(format!(
                    "no cgroup2 filesystem at {}",
                    containment::CGROUP_ROOT
                )),
                StartRefusal::NotDelegated => StartRefused::Unverified(format!(
                    "the scope's cgroup {scope_path} is not delegated (no user.delegate or trusted.delegate mark)"
                )),
                StartRefusal::ScopeNotRoots => StartRefused::Unverified(format!(
                    "the scope's cgroup {scope_path}, a file delegated with it, or a cgroup above it is not root's, so a process of the agent could be moved out of its cgroup (#166)"
                )),
                StartRefusal::NotRoots => {
                    StartRefused::Unverified(format!("a cgroup {name} stands that is not root's"))
                }
                StartRefusal::RecordUnverified => StartRefused::Unverified(
                    "the cgroup record does not read, or names another cgroup at its path"
                        .to_string(),
                ),
            });
        }
        StartAction::RemoveThenMake => {
            // A1: the empty cgroup standing is removed with any record, then
            // made afresh.
            if at_path == Standing::Empty
                && let Ok(held) = &scope
                && containment::remove_layout(held, &name).is_err()
            {
                return Err(StartRefused::Unverified(format!(
                    "the empty cgroup {name} beneath {scope_path} does not remove"
                )));
            }
            if let Ok(Some(record)) = &record
                && recorded_cgroup.is_some()
            {
                let removed = parent_and_name(&record.path).is_some_and(|(parent, name)| {
                    Cgroup::open(&parent)
                        .is_ok_and(|parent| containment::remove_layout(&parent, &name).is_ok())
                });
                if !removed {
                    return Err(StartRefused::Unverified(format!(
                        "the empty recorded cgroup {} does not remove",
                        record.path
                    )));
                }
            }
            if containment::remove_record(run_directory).is_err() {
                return Err(StartRefused::Unverified(
                    "the stale cgroup record does not remove".to_string(),
                ));
            }
        }
        StartAction::RemoveRecordThenMake => {
            // A0 with a stale record: the record goes before the mkdir.
            if containment::remove_record(run_directory).is_err() {
                return Err(StartRefused::Unverified(
                    "the stale cgroup record does not remove".to_string(),
                ));
            }
        }
        StartAction::Make => {}
    }
    let scope = scope.map_err(|error| {
        StartRefused::Unverified(format!("the scope's cgroup does not open: {error}"))
    })?;
    let made = scope.make_child(&name);
    let check = |step: MadeStep, what: &str| -> Result<(), StartRefused> {
        match containment::made_judgment(step) {
            MadeAction::GoOn => Ok(()),
            MadeAction::RemoveAndRefuse => {
                // Only what this step made is removed: a `mkdir` that failed
                // made nothing, so whatever stands at the path, another
                // account's included, is left alone.
                if matches!(made, Ok(_) | Err(containment::MakeChildError::Mode(_))) {
                    let _ = containment::remove_layout(&scope, &name);
                }
                let _ = containment::remove_record(run_directory);
                Err(StartRefused::Unverified(format!(
                    "the cgroup {name} beneath {scope_path}: {what}"
                )))
            }
        }
    };
    // The floor's third question: a `mkdir` the scope refuses, or what it
    // made not root's, or a mode that does not set.
    let agent_cgroup = match &made {
        Ok(cgroup) => cgroup,
        Err(containment::MakeChildError::Mode(_)) => {
            check(MadeStep::ModesSet(false), "its mode does not set")?;
            unreachable!("made_judgment refuses a mode not set")
        }
        Err(error) => {
            check(
                MadeStep::CgroupMade(false),
                &format!("it is not made: {error:?}"),
            )?;
            unreachable!("made_judgment refuses a cgroup not made")
        }
    };
    check(
        MadeStep::KillFilePresent(agent_cgroup.has_file("cgroup.kill")),
        "no cgroup.kill (Linux 5.14)",
    )?;
    if let Err(step) = containment::make_layout(agent_cgroup, owner.0, owner.1) {
        let what = match step {
            MadeStep::LayoutMade(_) => "its layout is not made",
            MadeStep::ModesSet(_) => "a mode does not set",
            MadeStep::OwnersSet(_) => "the three files' owner does not set",
            MadeStep::CgroupMade(_) | MadeStep::KillFilePresent(_) | MadeStep::RecordWritten(_) => {
                "a step failed"
            }
        };
        check(step, what)?;
    }
    let identity = agent_cgroup.identity();
    let opened = |path: &str| {
        agent_cgroup
            .child(path)
            .and_then(|leaf| leaf.procs_for_placement())
    };
    let (constituents, run_procs) = match (opened("constituents"), opened("worker/run")) {
        (Ok(constituents), Ok(run_procs)) => (constituents, run_procs),
        _ => {
            check(
                MadeStep::LayoutMade(false),
                "a leaf's cgroup.procs does not open",
            )?;
            unreachable!("made_judgment refuses a layout not made")
        }
    };
    let record = match identity {
        Ok((dev, ino)) => Record {
            path: agent_cgroup.relative().to_string(),
            dev,
            ino,
            run: run.to_string(),
        },
        Err(_) => {
            check(MadeStep::RecordWritten(false), "its identity does not read")?;
            unreachable!("made_judgment refuses a record not written")
        }
    };
    check(
        MadeStep::RecordWritten(containment::write_record(run_directory, &record).is_ok()),
        "the record does not write",
    )?;
    Ok(Made {
        record,
        constituents,
        run: run_procs,
    })
}

// ------------------------------------------------------------- the run end

/// Which run the conclusion ends: the one it names, or, where it holds the
/// invocation lock over a run no marker names (an ended run, S0d, or a run
/// never entered), the one the record names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Concluding<'a> {
    Run(&'a str),
    Recorded,
}

/// What the run end did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ended {
    /// No record stands: nothing to end.
    NoRecord,
    /// The recorded path was absent, A0: the stale record removed, or, where
    /// its removal failed, left for the next verb.
    Stale { record_removed: bool },
    /// The record names another load's run: that cgroup is left alone.
    NotOwned,
    /// Killed, emptied and removed. `standing` names what the kill found
    /// still running, and `record_removed` whether the record went too.
    Removed {
        standing: Vec<i32>,
        record_removed: bool,
    },
}

/// Why the run end refused, nothing published and no marker written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EndRefused {
    /// `WorkerWouldNotExit`, naming the pids still standing.
    WouldNotExit(Vec<i32>),
    /// `BoundaryUnverified`, naming the record or the cgroup.
    Unverified(String),
}

/// **The run end** (6.1, admin's run end), the run lock free and the
/// invocation lock held: the record read and judged by `record_judgment`,
/// ownership by `owns`, then the kill, the wait or the pinned fallback, the
/// removal deepest first and the record's removal, stepped by
/// `run_end_next` within `bound`.
pub fn run_end(
    run_directory: BorrowedFd<'_>,
    agent: &str,
    concluding: Concluding<'_>,
    bound: Duration,
) -> Result<Ended, EndRefused> {
    let record = match containment::read_record(run_directory, agent) {
        Ok(Some(record)) => record,
        Ok(None) => return Ok(Ended::NoRecord),
        Err(()) => {
            return Err(EndRefused::Unverified(
                "the cgroup record does not read, or names anything but this agent's cgroup"
                    .to_string(),
            ));
        }
    };
    let at = containment::recorded_at(&record);
    match containment::record_judgment(at) {
        RecordAction::Refuse => {
            return Err(EndRefused::Unverified(format!(
                "the recorded cgroup {} is not the one recorded (device {} inode {})",
                record.path, record.dev, record.ino
            )));
        }
        RecordAction::RemoveStaleAndGoOn => {
            return Ok(Ended::Stale {
                record_removed: containment::remove_record(run_directory).is_ok(),
            });
        }
        RecordAction::Proceed => {}
    }
    let concluded = match concluding {
        Concluding::Run(run) => run,
        Concluding::Recorded => record.run.as_str(),
    };
    if !containment::owns(&record.run, concluded, at) {
        return Ok(Ended::NotOwned);
    }
    let unverified = || EndRefused::Unverified(format!("the cgroup {}", record.path));
    // **A cgroup gone since the record's judgment is A0** (6.1, the scope's
    // end): PID1 collects the agent's scope once its last process ends, the
    // agent's cgroup with it, so a run end can meet the path, or its scope's,
    // absent here as it would at the judgment.
    let absent = |error: &std::io::Error| error.kind() == std::io::ErrorKind::NotFound;
    let (parent, name) = parent_and_name(&record.path).ok_or_else(unverified)?;
    let opened = Cgroup::open(&record.path).and_then(|cgroup| Ok((cgroup, Cgroup::open(&parent)?)));
    let (cgroup, parent) = match opened {
        Ok(opened) => opened,
        Err(error) if absent(&error) => {
            return Ok(Ended::Stale {
                record_removed: containment::remove_record(run_directory).is_ok(),
            });
        }
        Err(_) => return Err(unverified()),
    };
    let standing = pids(&cgroup);
    let mut next = containment::run_end_next(RunEndStep::Kill {
        written: cgroup.kill().is_ok(),
    });
    loop {
        next = match next {
            RunEndAction::Wait => containment::run_end_next(RunEndStep::Wait {
                emptied: cgroup.wait_emptied(bound),
            }),
            RunEndAction::Fallback => containment::run_end_next(RunEndStep::Fallback {
                emptied: cgroup.fallback_until_empty(bound),
            }),
            RunEndAction::Remove => containment::run_end_next(RunEndStep::Remove {
                removed: containment::remove_layout(&parent, &name).is_ok(),
            }),
            RunEndAction::RemoveRecord => containment::run_end_next(RunEndStep::RemoveRecord {
                removed: containment::remove_record(run_directory).is_ok(),
            }),
            RunEndAction::Conclude => {
                return Ok(Ended::Removed {
                    standing,
                    record_removed: true,
                });
            }
            RunEndAction::LogAndConclude => {
                return Ok(Ended::Removed {
                    standing,
                    record_removed: false,
                });
            }
            RunEndAction::RefuseWorkerWouldNotExit => {
                return Err(EndRefused::WouldNotExit(pids(&cgroup)));
            }
            RunEndAction::RefuseBoundaryUnverified => {
                return Err(EndRefused::Unverified(format!(
                    "the cgroup {} does not remove",
                    record.path
                )));
            }
        };
    }
}

// ------------------------------------------------------------- the ladder

/// **What the agent's cgroup holds, for the ladder's rung (iv)** (6.1, C2):
/// `None` where no record stands, its path is absent (A0) or the cgroup
/// reads `populated 0`, and otherwise the pids it lists, **which may be
/// none**: the kernel's listing skips a process mid-exit while `populated`
/// stays 1 through its exit's release of its files and memory (measured on
/// 2026-10-15: 646 ms after the kill of a process holding 32 GiB), and a
/// listing that fails names none too, a failed read being read as not
/// empty. A refusal's account where the record does not read or names
/// another cgroup there. Found through the record, never through this
/// invocation's own cgroup.
pub fn holds(run_directory: BorrowedFd<'_>, agent: &str) -> Result<Option<Vec<i32>>, String> {
    let record = match containment::read_record(run_directory, agent) {
        Ok(Some(record)) => record,
        Ok(None) => return Ok(None),
        Err(()) => return Err("the cgroup record does not read".to_string()),
    };
    match containment::recorded_at(&record) {
        RecordedAt::Absent => Ok(None),
        RecordedAt::Same => {
            let cgroup = match Cgroup::open(&record.path) {
                Ok(cgroup) => cgroup,
                // Collected with its scope since the judgment: A0.
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(error) => {
                    return Err(format!("the cgroup {} does not open: {error}", record.path));
                }
            };
            // **Emptiness is `populated 0`, a failed read read as not empty**
            // (6.1), unless the cgroup is gone from its path: a listing that
            // is empty, or fails, never reads as holding nothing while
            // `populated` does not read 0.
            if cgroup.emptied() {
                Ok(None)
            } else {
                Ok(Some(pids(&cgroup)))
            }
        }
        RecordedAt::Other | RecordedAt::Unreadable => Err(format!(
            "the recorded cgroup {} is not the one recorded",
            record.path
        )),
    }
}

#[cfg(test)]
mod tests;
