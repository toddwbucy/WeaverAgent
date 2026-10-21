//! Offline state selection and same-state declaration binding.
//!
//! ADMIN-SELECTION and ADMIN-REBIND are complete operations over the artifact
//! repository. They start no member, use no classifier and change no active
//! command surface. FOUNDATION-TRANSITION selects them at coupled cutover.

mod preparation;

use weaver_types::state_flow::{PersistencePhase, StartIdentity};
use weaver_types::{AgentName, Cause, LifecycleRefusal};

use crate::save_points::Owner;
use crate::save_points::serving::Fault;
use crate::save_points::serving::publication::{self, Progress, Publication};
use crate::save_points::serving::repository::{LoadScope, Repository, RestoreFailure};
use crate::save_points::serving::trace_path;
use crate::{ServiceConfig, require_unloaded, start, verb_table};

/// Preserve uncertainty and retained evidence from the artifact primitives.
/// Adapting these outcomes to stdout and admin.log belongs to coupled cutover.
#[derive(Debug)]
pub(crate) enum Failure {
    Admission(Box<LifecycleRefusal>),
    Artifact(Fault),
    Restore(RestoreFailure),
    Publication(publication::Failure),
    Component(String),
}

impl From<LifecycleRefusal> for Failure {
    fn from(error: LifecycleRefusal) -> Self {
        Self::Admission(Box::new(error))
    }
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Admission(error) => write!(f, "offline admission: {error:?}"),
            Self::Artifact(error) => error.fmt(f),
            Self::Restore(error) => write!(
                f,
                "restore: {}, selection may be appended: {}",
                error.fault, error.selection_may_be_appended
            ),
            Self::Publication(error) => error.fmt(f),
            Self::Component(detail) => write!(f, "offline component failure: {detail}"),
        }
    }
}

impl std::error::Error for Failure {}

#[derive(Clone, Copy)]
enum Operation {
    Restore,
    BindState,
}

impl Operation {
    fn holder(self) -> verb_table::HolderVerb {
        match self {
            Self::Restore => verb_table::HolderVerb::Restore,
            Self::BindState => verb_table::HolderVerb::BindState,
        }
    }

    fn requires_selector(self) -> bool {
        matches!(self, Self::Restore)
    }
}

pub(crate) fn restore(config: &ServiceConfig, agent: &AgentName) -> Result<StartIdentity, Failure> {
    run(config, agent, Operation::Restore)
}

pub(crate) fn bind_state(
    config: &ServiceConfig,
    agent: &AgentName,
) -> Result<StartIdentity, Failure> {
    run(config, agent, Operation::BindState)
}

fn run(
    config: &ServiceConfig,
    agent: &AgentName,
    operation: Operation,
) -> Result<StartIdentity, Failure> {
    // Match normal admin admission. A connector's valid sudo cause may restore,
    // whereas the new bind-state act requires the root operator's own cause.
    authorize(
        operation,
        crate::surface::running_as_root(),
        crate::cause_from(std::env::var_os("SUDO_UID").as_deref()),
    )?;
    run_authorized(config, agent, operation, preparation::prepare)
}

fn authorize(
    operation: Operation,
    root: bool,
    cause: Result<Cause, LifecycleRefusal>,
) -> Result<(), LifecycleRefusal> {
    if !root {
        return Err(LifecycleRefusal::Unauthorized);
    }
    let cause = cause?;
    if matches!(operation, Operation::BindState) && cause.uid != 0 {
        return Err(LifecycleRefusal::Unauthorized);
    }
    Ok(())
}

/// Only `run` enters this path in production, after authentication. Tests inject
/// host inventory facts through preparation, not lock or unloaded verdicts.
fn run_authorized(
    config: &ServiceConfig,
    agent: &AgentName,
    operation: Operation,
    prepare: impl FnOnce(
        &ServiceConfig,
        &AgentName,
        bool,
    ) -> Result<preparation::Prepared, LifecycleRefusal>,
) -> Result<StartIdentity, Failure> {
    crate::admissible(config, agent)?;
    let directory = crate::prepare_run_directory(config, nix::unistd::geteuid().as_raw())?;
    let _invocation = start::take_invocation_lock(&directory, operation.holder())?;
    let _run = start::take_run_lock(&directory)?.ok_or(LifecycleRefusal::AgentRunning)?;
    require_unloaded(config)?;
    let prepared = prepare(config, agent, operation.requires_selector())?;
    let directory = config
        .save_points_fd()?
        .try_clone_to_owned()
        .map_err(|error| Failure::Component(error.to_string()))?;
    let (uid, gid) = config.file_owner();
    let mut repository =
        Repository::from_judged_directory(directory, Owner { uid, gid }, config.access_gid)
            .map_err(Failure::Artifact)?;
    // No serving peer or decoder is involved. Offline publication has no new
    // elapsed-time budget. Both locks remain held while each checkpoint judges
    // the cgroup record, including the final check after the durable index append.
    perform(&mut repository, prepared, operation, &mut |_| {
        require_unloaded(config).map_err(|error| format!("offline state changed: {error:?}"))
    })
}

fn perform(
    repository: &mut Repository,
    prepared: preparation::Prepared,
    operation: Operation,
    checkpoint: &mut impl FnMut(PersistencePhase) -> Result<(), String>,
) -> Result<StartIdentity, Failure> {
    // Expectations come from the independently judged territory declaration.
    // Neither operation may use a candidate artifact to certify its own identity.
    let scope = LoadScope {
        session: &prepared.draft.agent().session,
        trace_path: trace_path(&prepared.draft.agent().trace_sink),
    };
    match operation {
        Operation::Restore => {
            let selector = prepared.selector.as_ref().ok_or_else(|| {
                Failure::Admission(Box::new(LifecycleRefusal::ConfigInvalid {
                    field: Some(weaver_types::FieldName("restore-artifact".into())),
                }))
            })?;
            repository
                .restore(selector, &scope, checkpoint)
                .map(|artifact| artifact.identity())
                .map_err(Failure::Restore)
        }
        Operation::BindState => {
            let mut publication = Publication::rebind(
                repository,
                &scope,
                prepared.selector.as_ref(),
                prepared.draft.clone(),
                checkpoint,
            )
            .map_err(Failure::Publication)?;
            loop {
                match publication
                    .advance(checkpoint)
                    .map_err(Failure::Publication)?
                {
                    Progress::Renamed | Progress::Verified => {}
                    Progress::Complete(identity) => return Ok(identity),
                    Progress::Published(_) => {
                        return Err(Failure::Publication(
                            publication.failed("offline rebind entered serving confirmation phase"),
                        ));
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
