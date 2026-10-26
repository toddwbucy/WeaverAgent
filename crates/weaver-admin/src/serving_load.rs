//! Root-side composition of the replacement Serving load.
//!
//! Both locks cover indexed selection, retained-image startup, the original
//! Enter answer and any rollback. No running command dispatches here until
//! FOUNDATION-TRANSITION's qualified coupled cutover.

mod member;
pub(crate) mod window;
use window::LoadWindow;

use std::os::fd::{AsFd, AsRawFd};
use std::process::Child;
#[cfg(test)]
use std::time::Instant;

use weaver_types::state_flow::enter::LoadedStateFacts;
use weaver_types::{AgentName, Cause, LifecycleAnswer, LifecycleRefusal, Position};

use crate::save_points::serving::repository::{LoadScope, Repository, VerifiedArtifact};
use crate::save_points::serving::{Fault, trace_path};
use crate::seed::preparation::PreparationFailure;
use crate::serving_enter::{PreparedEnter, RuntimeFacts};
use crate::{AgentAccount, ServiceConfig, Standing, channel, inventory, save_points, start};

#[derive(Debug)]
pub(crate) enum Failure {
    Admission(Box<LifecycleRefusal>),
    Artifact(Fault),
    Preparation(PreparationFailure),
    Cleanup {
        cause: Box<Failure>,
        refusal: LifecycleRefusal,
    },
}
impl From<LifecycleRefusal> for Failure {
    fn from(refusal: LifecycleRefusal) -> Self {
        Self::Admission(Box::new(refusal))
    }
}
impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Admission(refusal) => write!(f, "serving load: {refusal:?}"),
            Self::Artifact(fault) => fault.fmt(f),
            Self::Preparation(fault) => fault.fmt(f),
            Self::Cleanup { cause, refusal } => write!(f, "{cause}; rollback: {refusal:?}"),
        }
    }
}
impl std::error::Error for Failure {}

/// Returned only after the matching whole Ready and the durable open marker.
/// Adapting these retained facts to the replacement show surface is cutover work.
#[derive(Debug)]
pub(crate) struct Loaded {
    pub(crate) state: LoadedStateFacts,
}

pub(crate) fn run(config: &ServiceConfig, agent: &AgentName) -> Result<Loaded, Failure> {
    if !crate::surface::running_as_root() {
        return Err(LifecycleRefusal::Unauthorized.into());
    }
    let cause = crate::cause_from(std::env::var_os("SUDO_UID").as_deref())?;
    run_authorized(
        config,
        agent,
        cause,
        PreparedEnter::prepare,
        AgentAccount::resolve,
    )
}

/// Only host identity/inventory resolution is injected by the instruments.
/// Locks, selection, descriptors, containment, exec, channel and rollback are real.
fn run_authorized(
    config: &ServiceConfig,
    agent: &AgentName,
    cause: Cause,
    prepare: impl FnOnce(
        &ServiceConfig,
        &AgentName,
        VerifiedArtifact,
        RuntimeFacts,
        u64,
    ) -> Result<PreparedEnter, PreparationFailure>,
    account: impl FnOnce(&str) -> Result<AgentAccount, LifecycleRefusal>,
) -> Result<Loaded, Failure> {
    crate::admissible(config, agent)?;
    let window = LoadWindow::begin(config.load_bound)?;
    let directory = crate::prepare_run_directory(config, nix::unistd::geteuid().as_raw())?;
    window.check()?;
    let _invocation = start::take_invocation_lock_for_enter(&directory, &window)?;
    window.check()?;
    let mut standing = Standing::default();
    let mut children = Vec::new();
    let result = start_load(
        config,
        LoadRequest {
            agent,
            cause,
            window: &window,
        },
        &mut standing,
        &mut children,
        prepare,
        account,
    );
    match result {
        Ok(loaded) => Ok(loaded),
        Err(cause) => {
            let (detail, contained) = crate::roll_back(config, &mut standing);
            if !detail.is_empty() {
                crate::record(config, "load", &format!("rolled back: {detail}"));
            }
            // The run end includes descendants. Reap our direct children where
            // ended; never wait without a bound after a failed containment end.
            for child in &mut children {
                let _ = child.try_wait();
            }
            match contained {
                Ok(()) => Err(cause),
                Err(refusal) => Err(Failure::Cleanup {
                    cause: Box::new(cause),
                    refusal,
                }),
            }
        }
    }
}

struct LoadRequest<'a> {
    agent: &'a AgentName,
    cause: Cause,
    window: &'a LoadWindow,
}

fn start_load(
    config: &ServiceConfig,
    request: LoadRequest<'_>,
    standing: &mut Standing,
    children: &mut Vec<Child>,
    prepare: impl FnOnce(
        &ServiceConfig,
        &AgentName,
        VerifiedArtifact,
        RuntimeFacts,
        u64,
    ) -> Result<PreparedEnter, PreparationFailure>,
    account: impl FnOnce(&str) -> Result<AgentAccount, LifecycleRefusal>,
) -> Result<Loaded, Failure> {
    let LoadRequest {
        agent,
        cause,
        window,
    } = request;
    window.check()?;
    let prior_marker = crate::read_marker_or_refuse(config)?;
    window.check()?;
    let Some(run_lock) = start::take_run_lock(&config.run_directory())? else {
        // A held earlier run is observed, never rolled back by this invocation.
        let seen = crate::observe(config)
            .map(crate::worker_seen)
            .map(|seen| seen.0)
            .unwrap_or(crate::verb_table::WorkerSeen::Active);
        return Err(match crate::verb_table::act(
            crate::verb_table::Verb::Load,
            crate::verb_table::class_of(crate::verb_table::Seen::RunLockHeld(seen)),
        ) {
            crate::verb_table::VerbAction::RefuseUnanswered => LifecycleRefusal::Unanswered,
            _ => LifecycleRefusal::AgentRunning,
        }
        .into());
    };
    standing.run_lock = Some(run_lock);
    window.check()?;
    crate::require_unloaded(config)?;
    window.check()?;
    let artifact = select(config, agent)?;
    window.check()?;
    let run = channel::mint_run_reference(&agent.0).ok_or(LifecycleRefusal::BoundaryUnverified)?;
    // This is a new coordination connection and its first exchange. Reserve
    // the same ordinal before preparing the complete envelope, before any fork.
    let prepared = prepare(
        config,
        agent,
        artifact,
        RuntimeFacts {
            enter_window: window.original(),
            run: run.clone(),
            reset: save_points::reset_from(prior_marker.as_ref()),
            stack: crate::stack_digests(config, true, None),
            cause,
        },
        1,
    )
    .map_err(Failure::Preparation)?;
    window.check()?;
    // Enforce the complete paired wire bound before any child can stand.
    prepared.envelope().encode().map_err(|error| {
        match error {
            weaver_types::state_flow::enter::CodecError::TooLarge => {
                crate::record(config, "load", "enter_frame_too_large")
            }
            other => crate::record(config, "load", &format!("Enter encoding failed: {other:?}")),
        }
        Failure::from(LifecycleRefusal::DescriptorsUnusable)
    })?;
    window.check()?;
    let inventory = prepared.inventory();
    let custody = crate::sink::FileCustody {
        owners: vec![0, nix::unistd::geteuid().as_raw()],
        trace_group: format!("{}-trace", inventory::identity_for(agent)),
    };
    let sink = crate::sink::open(
        &inventory.config.trace_sink,
        &custody,
        config.territory_fd()?,
    )?;
    standing.sink_opened = true;
    window.check()?;
    let account = account(&inventory.identity)?;
    window.check()?;
    start::prepare_runtime_directory(
        &config.coordination_root,
        &agent.0,
        account.uid,
        account.gid,
    )?;
    window.check()?;
    let worker_log = start::open_log(&config.worker_log(), Some(config.file_owner()))
        .map_err(|_| LifecycleRefusal::BoundaryUnverified)?;
    window.check()?;
    let group = crate::make_agent_cgroup(config, agent, &run.0, &account)?;
    standing.cgroup_run = Some(run.0.clone());
    window.check()?;
    if let Some(judged) = inventory.shell.as_ref() {
        crate::shell::build_commands(&config.run_directory(), judged)
            .map_err(|_| LifecycleRefusal::BoundaryUnverified)?;
    }
    window.check()?;
    let run_lock = standing.run_lock.as_ref().expect("retained run lock");
    start::clear_trace_door(&config.run_directory())?;
    window.check()?;
    let relay_write = if matches!(
        inventory.config.trace_sink,
        weaver_types::TraceSink::File { .. }
    ) {
        let (relay, write) = crate::stand_relay(
            config,
            agent,
            &sink,
            run_lock,
            group.constituents_procs().as_raw_fd(),
        )?;
        children.push(relay);
        standing.forked = true;
        Some(write)
    } else {
        None
    };
    window.check()?;
    let (member, state_end) =
        member::spawn(config, &prepared, run_lock, group.constituents_procs())?;
    children.push(member);
    standing.forked = true;
    window.check()?;
    let worker = start::spawn_worker(start::WorkerStart {
        binary: &config.worker,
        // The existing optional classifier executable slot carries the judged
        // STE100 executable to the replacement worker composition. The legacy
        // worker cannot consume this Enter; neither dispatcher is activated here.
        arguments: start::worker_arguments(
            &config.coordination_socket(),
            &config.spu,
            &config.gate,
            config.headroom_bytes.as_deref(),
            Some(prepared.checker_binary()),
        ),
        uid: account.uid,
        gid: account.gid,
        home: &account.home,
        library_path: config.library_path.as_deref(),
        log: &worker_log,
        run_lock,
        relay_write: relay_write.as_ref().map(AsRawFd::as_raw_fd),
        placement: group.run_procs().as_raw_fd(),
    })
    .map_err(|_| LifecycleRefusal::BindFailed)?;
    children.push(worker);
    drop(relay_write);
    window.check()?;
    let mut peer = match crate::dial_worker_for_enter(config, window) {
        Ok(peer) => peer,
        Err(channel::ChannelFault::WrongPeer { .. }) => {
            return Err(LifecycleRefusal::BoundaryUnverified.into());
        }
        Err(channel::ChannelFault::Unanswered) => return Err(LifecycleRefusal::NoResidency.into()),
        Err(_) => {
            return Err(crate::refusal_from_worker(children.last_mut().expect("worker")).into());
        }
    };
    window.check()?;
    if peer.next_ordinal() != prepared.envelope().exchange.ordinal {
        return Err(LifecycleRefusal::Malformed.into());
    }
    standing.run_reference = Some(run.0.clone());
    prepared
        .send_within(&peer, sink.as_fd(), state_end.as_fd(), window)
        .map_err(|fault| {
            if fault == channel::ChannelFault::Unanswered {
                LifecycleRefusal::NoResidency
            } else {
                LifecycleRefusal::DescriptorsUnusable
            }
        })?;
    standing.entered = true;
    window.check()?;
    let answer = peer
        .recv_enter_answer(window)
        .map_err(|_| LifecycleRefusal::NoResidency)?;
    window.check()?;
    if answer.exchange != prepared.envelope().exchange || answer.position != Position::Close {
        return Err(LifecycleRefusal::Malformed.into());
    }
    match answer.payload {
        weaver_types::Payload::Answer(LifecycleAnswer::Ready) => {
            crate::open_marker(&config.root, standing, &run.0)?;
            window.check()?;
            Ok(Loaded {
                state: prepared.loaded_state(),
            })
        }
        weaver_types::Payload::Refusal(refusal) => Err(refusal.into()),
        _ => Err(LifecycleRefusal::Malformed.into()),
    }
}

/// Parse the draft for its independent identity/path comparison. Operational
/// inventory and every Enter setting come from the verified declaration copy.
/// Selection never publishes room leftovers and never falls back to an old save.
fn select(config: &ServiceConfig, agent: &AgentName) -> Result<VerifiedArtifact, Failure> {
    let owner = nix::unistd::geteuid().as_raw();
    crate::judge_root(&config.root, owner)?;
    let root = crate::judge_ancestors(&config.root, &[owner, 0])?;
    crate::refuse_retired_keys(&root, &agent.0)?;
    crate::judge_entries(&root, owner)?;
    let root =
        save_points::open_directory(&root).map_err(|_| LifecycleRefusal::BoundaryUnverified)?;
    let selector = crate::offline_state::preparation::read_selector(root.as_fd(), owner)?;
    let source = crate::read_declaration(config)?;
    let draft = weaver_types::parse_serving_draft(&source)
        .map_err(|error| LifecycleRefusal::ConfigInvalid { field: error.field })?;
    crate::sink_within_territory(&draft.agent().trace_sink, &config.territory)?;
    let directory = config
        .save_points_fd()?
        .try_clone_to_owned()
        .map_err(|_| LifecycleRefusal::BoundaryUnverified)?;
    let (uid, gid) = config.file_owner();
    Repository::from_judged_directory(
        directory,
        save_points::Owner { uid, gid },
        config.access_gid,
    )
    .map_err(Failure::Artifact)?
    .load(
        selector.as_ref(),
        &LoadScope {
            session: &draft.agent().session,
            trace_path: trace_path(&draft.agent().trace_sink),
        },
    )
    .map_err(Failure::Artifact)
}

#[cfg(test)]
mod tests;
