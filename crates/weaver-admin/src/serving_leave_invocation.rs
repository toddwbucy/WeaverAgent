//! Serving force admission and conclusion under the real invocation lock.
//! A graceful unload may be joined without taking over its publisher. Workerless
//! runs end without recovering room images or inventing a Left.
use super::{Capture, FailureCause, Observed, Progress};
use crate::save_points::serving::repository::Repository;
use crate::{ServiceConfig, UnloadBounds, agent_cgroup, conclusion, inventory, save_points, start};
use std::os::fd::AsFd;
use weaver_types::state_flow::{DecimalU64, SavePointReport, StartIdentity};
use weaver_types::{AgentName, Cause, LifecycleAnswer, LifecycleRefusal, RunId};

#[derive(Debug)]
pub(crate) enum Fault {
    Admission(LifecycleRefusal),
    Sequence(FailureCause),
}

/// Exact packets and publication locations survive a failed invocation. The
/// caller's eventual public error adapter must report these, not scan the room.
#[derive(Debug)]
pub(crate) struct Evidence {
    pub(crate) run: RunId,
    pub(crate) loaded: StartIdentity,
    pub(crate) received: Vec<crate::channel::ReceivedCapture>,
    pub(crate) unexpected: Option<crate::channel::ReceivedCapture>,
    pub(crate) report: Option<SavePointReport>,
    pub(crate) publication_failure: Option<super::publication::Failure>,
    pub(crate) completed: Option<StartIdentity>,
    pub(crate) capture_id: Option<DecimalU64>,
    pub(crate) committed: bool,
    /// Original failure that selected the initial-Leave escalation. This is
    /// neither a Left nor a capture/publication completion receipt.
    pub(crate) force_trigger: Option<FailureCause>,
}

#[derive(Debug)]
pub(crate) struct Failure {
    pub(crate) cause: Fault,
    pub(crate) cleanup: Option<Box<LifecycleRefusal>>,
    pub(crate) evidence: Option<Box<Evidence>>,
    pub(crate) joined: Option<Box<join::Evidence>>,
}

impl From<LifecycleRefusal> for Failure {
    fn from(refusal: LifecycleRefusal) -> Self {
        Self {
            cause: Fault::Admission(refusal),
            cleanup: None,
            evidence: None,
            joined: None,
        }
    }
}
impl From<FailureCause> for Failure {
    fn from(cause: FailureCause) -> Self {
        Self {
            cause: Fault::Sequence(cause),
            cleanup: None,
            evidence: None,
            joined: None,
        }
    }
}

#[derive(Debug)]
pub(crate) struct Concluded {
    pub(crate) answer: LifecycleAnswer,
    /// No capture was requested when the run was already down or workerless.
    pub(crate) evidence: Option<Box<Evidence>>,
    pub(crate) joined: Option<Box<join::Evidence>>,
}

/// This is not selected by the public force-unload command before cutover.
pub(crate) fn force(config: &ServiceConfig, agent: &AgentName) -> Result<Concluded, Failure> {
    authorized(config, agent, true)
}

/// Dormant until the actual graceful worker and coupled cutover are complete.
pub(crate) fn unload(config: &ServiceConfig, agent: &AgentName) -> Result<Concluded, Failure> {
    authorized(config, agent, false)
}

fn authorized(
    config: &ServiceConfig,
    agent: &AgentName,
    forced: bool,
) -> Result<Concluded, Failure> {
    if !crate::surface::running_as_root() {
        return Err(LifecycleRefusal::Unauthorized.into());
    }
    let cause = crate::cause_from(std::env::var_os("SUDO_UID").as_deref())?;
    run_mode(
        config,
        agent,
        cause,
        crate::UNLOAD_BOUNDS,
        forced,
        |agent| {
            nix::unistd::User::from_name(&inventory::member_identity_for(agent))
                .map_err(|_| LifecycleRefusal::BoundaryUnverified)?
                .map(|user| user.uid.as_raw())
                .ok_or(LifecycleRefusal::BoundaryUnverified)
        },
    )
}

/// Instruments replace account lookup only. Locks, peer authentication, copy,
/// publication, waits, escalation, cgroup end and marker writes remain real.
fn run_authorized(
    config: &ServiceConfig,
    agent: &AgentName,
    cause: Cause,
    bounds: UnloadBounds,
    member: impl FnOnce(&AgentName) -> Result<u32, LifecycleRefusal>,
) -> Result<Concluded, Failure> {
    run_mode(config, agent, cause, bounds, true, member)
}

fn run_mode(
    config: &ServiceConfig,
    agent: &AgentName,
    cause: Cause,
    bounds: UnloadBounds,
    forced: bool,
    member: impl FnOnce(&AgentName) -> Result<u32, LifecycleRefusal>,
) -> Result<Concluded, Failure> {
    crate::admissible(config, agent)?;
    let directory = crate::prepare_run_directory(config, nix::unistd::geteuid().as_raw())?;
    let _invocation = if forced {
        match join::take_or_join(config, bounds, cause)? {
            join::Taken::Sole(lock) => lock,
            join::Taken::Joined(concluded) => return Ok(*concluded),
        }
    } else {
        start::take_invocation_lock(&directory, crate::verb_table::HolderVerb::Unload)?
    };
    let verb = crate::verb_of(forced);
    if !start::run_lock_held(&directory)? {
        return conclude_workerless(config, verb);
    }
    let mut observation = match crate::dial_worker(config) {
        Ok(channel) => channel,
        Err(crate::channel::ChannelFault::NoListener) => {
            // S8b: the worker is gone. End the actual remaining lock holders,
            // then the recorded cgroup, preserving the prior marker. These
            // leftovers provide no capture or successful-Leave evidence.
            start::escalate_within(&directory, bounds.term, bounds.kill)?;
            return conclude_workerless(config, verb);
        }
        Err(error) => return Err(FailureCause::Channel(error).into()),
    };
    if config.territory_unjudged {
        return Err(LifecycleRefusal::BoundaryUnverified.into());
    }
    let mut repository = Repository::from_judged_directory(
        config
            .save_points_fd()?
            .try_clone_to_owned()
            .map_err(|_| LifecycleRefusal::BoundaryUnverified)?,
        save_points::ROOT,
        config.access_gid,
    )
    .map_err(|error| FailureCause::Artifact(Box::new(error)))?;
    let observed = Observed::ask_loaded(&mut observation, &repository, &config.territory, !forced)?;
    let member_uid = member(agent)?;
    let room = save_points::open_room(config.territory_fd()?)?
        .ok_or(LifecycleRefusal::BoundaryUnverified)?;
    let stat =
        nix::sys::stat::fstat(room.as_fd()).map_err(|_| LifecycleRefusal::BoundaryUnverified)?;
    if stat.st_uid != member_uid || stat.st_mode & 0o7777 != 0o700 {
        return Err(LifecycleRefusal::BoundaryUnverified.into());
    }
    // Foundation management closes a completed Observe connection. Redial and
    // prove it is the same live worker before sending the Leave on its new seat.
    let channel = crate::dial_worker(config).map_err(FailureCause::Channel)?;
    channel
        .require_same_worker(&observation)
        .map_err(FailureCause::Channel)?;
    drop(observation);
    let begin = if forced {
        Capture::begin
    } else {
        Capture::graceful
    };
    let mut leave = begin(
        observed,
        &mut repository,
        channel,
        room,
        member_uid,
        cause,
        bounds.leave,
    )?;
    while leave.advance() == Progress::Pending {}
    if leave.progress() == Progress::Stopped {
        let standing = leave.standing_refusal();
        let force_escalation = leave.initial_escalation_due();
        let cause = leave
            .failure
            .take()
            .expect("stopped owner retains first cause");
        if standing {
            return Err(Failure {
                cause: Fault::Sequence(cause),
                cleanup: None,
                evidence: Some(Box::new(evidence(leave, None))),
                joined: None,
            });
        }
        if force_escalation {
            let ended = escalate_initial_leave(config, bounds, &leave.observed.facts.run, verb);
            let mut retained = Box::new(evidence(leave, None));
            return match ended {
                Ok(answer) => {
                    retained.force_trigger = Some(cause);
                    Ok(Concluded {
                        answer,
                        evidence: Some(retained),
                        joined: None,
                    })
                }
                Err(cleanup) => Err(Failure {
                    cause: Fault::Sequence(cause),
                    cleanup: Some(Box::new(cleanup)),
                    evidence: Some(retained),
                    joined: None,
                }),
            };
        }
        // No Left: there is no clean conclusion and no marker write. The
        // actual failed run still ends under the same invocation lock.
        let cleanup = end_run(config, bounds, &leave.observed.facts.run, verb)
            .err()
            .map(Box::new);
        return Err(Failure {
            cause: Fault::Sequence(cause),
            cleanup,
            evidence: Some(Box::new(evidence(leave, None))),
            joined: None,
        });
    }
    let mut completed = None;
    let mut cleanup = None;
    match conclude(config, bounds, &mut leave, &mut completed, &mut cleanup) {
        Ok(answer) => Ok(Concluded {
            answer,
            evidence: Some(Box::new(evidence(leave, completed))),
            joined: None,
        }),
        Err(cause) => {
            if let Some(publication) = &mut leave.publication {
                leave.publication_failure = Some(publication.failed(format!("{cause:?}")));
            }
            Err(Failure {
                cause,
                cleanup,
                evidence: Some(Box::new(evidence(leave, completed))),
                joined: None,
            })
        }
    }
}

/// The sole caller already holds its invocation lock. An unanswered initial
/// Leave or a forced Leave's correlated OutOfOrder ends the run, then writes Forced
/// by the existing marker rule. No capture was begun and nothing is published.
fn escalate_initial_leave(
    config: &ServiceConfig,
    bounds: UnloadBounds,
    run: &RunId,
    verb: &'static str,
) -> Result<LifecycleAnswer, LifecycleRefusal> {
    if let Some(answer) = crate::behind_another_run(config, bounds, Some(&run.0))? {
        return answer;
    }
    start::escalate_within(&config.run_directory(), bounds.term, bounds.kill)?;
    crate::end_agent_cgroup(config, verb, agent_cgroup::Concluding::Run(&run.0))?;
    crate::write_by_rule(config, Some(&run.0), conclusion::MarkerWrite::Forced)?;
    crate::unloaded_by_ladder(config)
}

/// ADMIN-RECOVERY replaces S0d/S8b room publication. Only containment is owed
/// here: no artifact selection, room scan, guard repair or marker conclusion.
/// The caller holds the invocation lock and has established a free run lock.
fn conclude_workerless(config: &ServiceConfig, verb: &'static str) -> Result<Concluded, Failure> {
    crate::end_agent_cgroup(config, verb, agent_cgroup::Concluding::Recorded)?;
    Ok(Concluded {
        answer: crate::unloaded_by_ladder(config)?,
        evidence: None,
        joined: None,
    })
}

fn end_run(
    config: &ServiceConfig,
    bounds: UnloadBounds,
    run: &RunId,
    verb: &'static str,
) -> Result<(), LifecycleRefusal> {
    crate::end_after_left(&config.run_directory(), bounds)?;
    crate::end_agent_cgroup(config, verb, agent_cgroup::Concluding::Run(&run.0))
}

fn conclude(
    config: &ServiceConfig,
    bounds: UnloadBounds,
    leave: &mut Capture<'_>,
    completed: &mut Option<StartIdentity>,
    cleanup: &mut Option<Box<LifecycleRefusal>>,
) -> Result<LifecycleAnswer, Fault> {
    let run = leave.observed.facts.run.clone();
    let marker = crate::read_marker_or_refuse(config);
    if matches!(&marker, Ok(Some(save_points::Marker::Open { run: open })) if *open != run.0) {
        // The normal marker rule forbids escalation of another run. Leave all
        // publication metadata alone; this path claims no completed save.
        if !start::wait_free(&config.run_directory(), bounds.after_left) {
            return Err(Fault::Admission(LifecycleRefusal::AgentRunning));
        }
        crate::end_agent_cgroup(config, leave.verb(), agent_cgroup::Concluding::Run(&run.0))
            .map_err(Fault::Admission)?;
        return crate::unloaded_by_ladder(config).map_err(Fault::Admission);
    }
    // A validated Left and this run's eligible marker complete the exact
    // guarded save before process cleanup. A later cleanup failure still
    // refuses Unloaded, but cannot make that completed save ineligible.
    let publication = if matches!(&marker, Ok(Some(save_points::Marker::Open { run: open })) if *open == run.0)
    {
        leave
            .finish_leave()
            .map(|identity| *completed = Some(identity))
            .map_err(Fault::Sequence)
    } else {
        Ok(())
    };
    let ended = end_run(config, bounds, &run, leave.verb());
    if let Err(cause) = publication {
        *cleanup = ended.err().map(Box::new);
        return Err(cause);
    }
    ended.map_err(Fault::Admission)?;
    if !matches!(marker.map_err(Fault::Admission)?, Some(save_points::Marker::Open { run: open }) if open == run.0)
    {
        // The idempotent conclusion writes/publishes nothing for an already
        // closed or absent marker. Any guarded capture remains ineligible.
        return crate::unloaded_by_ladder(config).map_err(Fault::Admission);
    }
    // Marker closure and Unloaded still require successful run cleanup.
    crate::write_by_rule(config, Some(&run.0), conclusion::MarkerWrite::Closed)
        .map_err(Fault::Admission)?;
    crate::unloaded_by_ladder(config).map_err(Fault::Admission)
}

fn evidence(leave: Capture<'_>, completed: Option<StartIdentity>) -> Evidence {
    let capture_id = leave
        .windows
        .capture()
        .map(|beginning| beginning.capture_id);
    Evidence {
        run: leave.observed.facts.run,
        loaded: leave.observed.source.identity(),
        received: leave.received,
        unexpected: leave.unexpected,
        report: leave.report,
        publication_failure: leave.publication_failure,
        completed,
        capture_id,
        committed: leave.committed,
        force_trigger: None,
    }
}

#[path = "serving_leave_response.rs"]
mod response;

#[path = "serving_leave_join.rs"]
mod join;

#[cfg(test)]
#[path = "serving_leave_invocation_tests.rs"]
mod tests;
