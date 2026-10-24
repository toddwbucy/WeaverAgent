//! Contained pre-agent seed operation, per ADMIN-SEED and TYPES-SEED.
//!
//! This replacement component is not selected by the running command parser.
//! FOUNDATION-TRANSITION activates it with the qualified serving stack. Admin
//! verifies reviewed bytes, the member creates SQLite, and publication follows
//! successful member and containment cleanup. No serving trace or classifier is
//! started by this operation.

pub(crate) mod preparation;
mod process;
mod transport;

use std::os::fd::AsFd;
use std::time::Instant;

use weaver_types::state_flow::{PersistenceCase, SeedRefusal, StartIdentity};
use weaver_types::{AgentName, LifecycleRefusal};

use crate::save_points::Owner;
use crate::save_points::serving::publication::{self, Progress, Publication, SeedImage};
use crate::save_points::serving::repository::{LoadScope, Repository};
use crate::save_points::serving::trace_path;
use crate::{ServiceConfig, start, verb_table};

/// Typed internal outcomes, adapted to the replacement lifecycle surface at
/// cutover. A missing answer is a component failure, never an explicit refusal.
#[derive(Debug)]
pub(crate) enum Failure {
    Admission(Box<LifecycleRefusal>),
    Preparation(preparation::PreparationFailure),
    Refused {
        case: PersistenceCase,
        detail: String,
    },
    Component(String),
    Publication(publication::Failure),
    Cleanup {
        cause: Box<Failure>,
        detail: String,
    },
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Admission(error) => write!(f, "seed admission: {error:?}"),
            Self::Preparation(error) => write!(f, "seed preparation: {error}"),
            Self::Refused { case, detail } => write!(f, "seed {case:?}: {detail}"),
            Self::Component(detail) => write!(f, "seed component failure: {detail}"),
            Self::Publication(error) => error.fmt(f),
            Self::Cleanup { cause, detail } => write!(f, "{cause}; cleanup failed: {detail}"),
        }
    }
}

impl std::error::Error for Failure {}

impl From<LifecycleRefusal> for Failure {
    fn from(value: LifecycleRefusal) -> Self {
        Self::Admission(Box::new(value))
    }
}

/// The accepted invocation supplies the original start anchor. No later stage
/// receives a fresh load budget. The caller's config is the judged root config,
/// and this entry adds seed-specific authoring inventory under both locks.
pub(crate) fn run(
    config: &ServiceConfig,
    agent: &AgentName,
    accepted: Instant,
) -> Result<StartIdentity, Failure> {
    if !crate::surface::running_as_root()
        || crate::cause_from(std::env::var_os("SUDO_UID").as_deref())?.uid != 0
    {
        return Err(LifecycleRefusal::Unauthorized.into());
    }
    run_authorized(config, agent, accepted, preparation::prepare)
}

/// Only `run` enters after root authentication. Tests replace host inventory,
/// while retaining the actual invocation/run locks and cgroup ladder.
fn run_authorized(
    config: &ServiceConfig,
    agent: &AgentName,
    accepted: Instant,
    prepare: impl FnOnce(
        &ServiceConfig,
        &AgentName,
        String,
    ) -> Result<preparation::PreparedSeed, preparation::PreparationFailure>,
) -> Result<StartIdentity, Failure> {
    crate::admissible(config, agent)?;
    let deadline = accepted
        .checked_add(config.load_bound)
        .ok_or_else(|| Failure::Component("load deadline overflow".into()))?;
    checkpoint(deadline).map_err(Failure::Component)?;
    let run_directory = crate::prepare_run_directory(config, nix::unistd::geteuid().as_raw())?;
    let invocation =
        start::take_invocation_lock_until(&run_directory, verb_table::HolderVerb::Seed, deadline);
    checkpoint(deadline).map_err(Failure::Component)?;
    let _invocation = invocation?;
    let run_lock = start::take_run_lock(&run_directory)?.ok_or_else(|| Failure::Refused {
        case: PersistenceCase::SeedRequiresUnloaded,
        detail: "the agent's run lock is held".into(),
    })?;
    checkpoint(deadline).map_err(Failure::Component)?;
    crate::require_unloaded(config).map_err(|error| match error {
        LifecycleRefusal::AgentRunning => Failure::Refused {
            case: PersistenceCase::SeedRequiresUnloaded,
            detail: "the agent's recorded cgroup still holds processes".into(),
        },
        other => Failure::from(other),
    })?;
    let mut repository = repository(config)?;
    repository
        .require_unseeded()
        .map_err(seed_repository_fault)?;
    let prepared = prepare(config, agent, mint_seed_id()?).map_err(Failure::Preparation)?;
    checkpoint(deadline).map_err(Failure::Component)?;
    let child = process::SeedChild::start(config, agent, &prepared, &run_lock, deadline)
        .map_err(|error| Failure::Component(error.to_string()))?;
    publish_member(&prepared, &mut repository, child, deadline)
}

/// Complete seed protocol against a contained member. The process type has no
/// public constructor that can stand in for containment or a cleanup result.
fn publish_member(
    prepared: &preparation::PreparedSeed,
    repository: &mut Repository,
    mut child: process::SeedChild<'_>,
    deadline: Instant,
) -> Result<StartIdentity, Failure> {
    let result = transport::exchange(child.channel(), prepared.request(), deadline);
    let created = match result {
        Ok(created) => created,
        Err(error) => return Err(with_cleanup(exchange_failure(error), child.rollback())),
    };
    // Hold the same room independently so liveness can be checked between the
    // filesystem steps without re-resolving any path or ending the channel.
    let room = match child.room().try_clone_to_owned() {
        Ok(room) => room,
        Err(error) => {
            return Err(with_cleanup(
                Failure::Component(error.to_string()),
                child.rollback(),
            ));
        }
    };
    let draft = prepared.draft();
    let publication = Publication::seed(
        repository,
        &LoadScope {
            session: &draft.agent().session,
            trace_path: trace_path(&draft.agent().trace_sink),
        },
        draft.clone(),
        SeedImage {
            expected_seed_id: &prepared.request().seed_id,
            created: &created,
            room: room.as_fd(),
            member_uid: prepared.member_account().uid,
        },
        &mut |_| {
            checkpoint(deadline)?;
            child.check_alive().map_err(|error| error.to_string())
        },
    );
    match publication {
        Ok(publication) => finish_publication(publication, deadline, || {
            child.finish(deadline).map_err(|error| error.to_string())
        }),
        Err(error) => Err(with_cleanup(Failure::Publication(error), child.rollback())),
    }
}

/// The only path from a copied seed to publication. Cleanup runs even if the
/// load deadline expired. Its failure cancels the unlisted copy before return.
fn finish_publication(
    mut publication: Publication<'_>,
    deadline: Instant,
    cleanup: impl FnOnce() -> Result<(), String>,
) -> Result<StartIdentity, Failure> {
    if let Err(detail) = cleanup() {
        let cancelled = publication.cancel(format!("seed cleanup failed: {detail}"));
        return Err(Failure::Cleanup {
            cause: Box::new(Failure::Publication(cancelled)),
            detail,
        });
    }
    let mut check = |_| checkpoint(deadline);
    publication
        .seed_cleanup_complete(&mut check)
        .map_err(Failure::Publication)?;
    loop {
        match publication
            .advance(&mut check)
            .map_err(Failure::Publication)?
        {
            Progress::Renamed | Progress::Verified => {}
            Progress::Complete(identity) => return Ok(identity),
            Progress::Published(_) => {
                return Err(Failure::Publication(
                    publication.failed("seed publication entered serving confirmation phase"),
                ));
            }
        }
    }
}

fn with_cleanup(cause: Failure, cleanup: Result<(), process::ProcessFailure>) -> Failure {
    match cleanup {
        Ok(()) => cause,
        Err(error) => Failure::Cleanup {
            cause: Box::new(cause),
            detail: error.to_string(),
        },
    }
}

fn exchange_failure(error: transport::ExchangeFailure) -> Failure {
    match error {
        transport::ExchangeFailure::Refused { code, detail } => Failure::Refused {
            case: if code == SeedRefusal::SeedCertificateInvalid {
                PersistenceCase::SeedCertificateInvalid
            } else {
                PersistenceCase::SeedFailed
            },
            detail: format!("{code:?}: {detail}"),
        },
        component => Failure::Component(component.to_string()),
    }
}

fn seed_repository_fault(fault: crate::save_points::serving::Fault) -> Failure {
    if fault.case == PersistenceCase::SeedExists {
        Failure::Refused {
            case: fault.case,
            detail: fault.detail,
        }
    } else {
        // The closed persistence vocabulary has no seed-stage layout refusal.
        // A malformed repository therefore follows failed-start, not a made-up
        // stage/case combination or a claim that this is an empty first chain.
        Failure::Component(format!("seed repository: {fault}"))
    }
}

fn repository(config: &ServiceConfig) -> Result<Repository, Failure> {
    let directory = config
        .save_points_fd()?
        .try_clone_to_owned()
        .map_err(|error| Failure::Component(error.to_string()))?;
    let (uid, gid) = config.file_owner();
    Repository::from_judged_directory(directory, Owner { uid, gid }, config.access_gid)
        .map_err(seed_repository_fault)
}

fn checkpoint(deadline: Instant) -> Result<(), String> {
    if Instant::now() >= deadline {
        Err("seed operation exceeded its original load deadline".into())
    } else {
        Ok(())
    }
}

fn mint_seed_id() -> Result<String, Failure> {
    crate::random::random_hex(16)
        .map_err(|error| Failure::Component(format!("seed randomness: {error}")))
}

#[cfg(test)]
mod tests;
