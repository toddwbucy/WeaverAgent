//! On-demand admission retains the actual invocation lock and loaded copy.
//! This replacement remains unselected until the coupled serving cutover.
use super::{Capture, FailureCause, Observed, Progress, RequestEnvelope};
use crate::save_points::serving::repository::Repository;
use crate::{ServiceConfig, inventory, save_points, start};
use std::os::fd::AsFd;
use weaver_types::state_flow::capture::{
    CaptureEnvelope, CapturePayload, CaptureRefusal, SaveRefusal,
};
use weaver_types::state_flow::{SavePointReport, StartIdentity};
use weaver_types::{AgentName, Cause, LifecycleRefusal, RunId};

#[derive(Debug)]
pub(crate) enum Fault {
    Admission(LifecycleRefusal),
    Sequence(FailureCause),
}

#[derive(Debug)]
pub(crate) struct Evidence {
    pub(crate) run: RunId,
    pub(crate) loaded: StartIdentity,
    pub(crate) request: CaptureEnvelope,
    pub(crate) received: Vec<crate::channel::ReceivedCapture>,
    pub(crate) unexpected: Option<crate::channel::ReceivedCapture>,
    pub(crate) report: Option<SavePointReport>,
    pub(crate) publication_failure: Option<super::publication::Failure>,
    pub(crate) committed: bool,
    pub(crate) completed: Option<StartIdentity>,
    pub(crate) abort: Option<CaptureEnvelope>,
    pub(crate) cleanup_failure: Option<FailureCause>,
    pub(crate) capture_id: Option<weaver_types::state_flow::DecimalU64>,
}

#[derive(Debug)]
pub(crate) struct Failure {
    pub(crate) cause: Fault,
    pub(crate) evidence: Option<Box<Evidence>>,
}
impl From<LifecycleRefusal> for Failure {
    fn from(value: LifecycleRefusal) -> Self {
        Self {
            cause: Fault::Admission(value),
            evidence: None,
        }
    }
}
impl From<FailureCause> for Failure {
    fn from(value: FailureCause) -> Self {
        Self {
            cause: Fault::Sequence(value),
            evidence: None,
        }
    }
}

#[derive(Debug)]
pub(crate) struct Taken {
    pub(crate) report: SavePointReport,
    pub(crate) evidence: Box<Evidence>,
}

pub(crate) fn save(config: &ServiceConfig, agent: &AgentName) -> Result<Taken, Failure> {
    if !crate::surface::running_as_root() {
        return Err(LifecycleRefusal::Unauthorized.into());
    }
    let cause = crate::cause_from(std::env::var_os("SUDO_UID").as_deref())?;
    run(config, agent, cause, |agent| {
        nix::unistd::User::from_name(&inventory::member_identity_for(agent))
            .map_err(|_| LifecycleRefusal::BoundaryUnverified)?
            .map(|user| user.uid.as_raw())
            .ok_or(LifecycleRefusal::BoundaryUnverified)
    })
}

/// Tests replace only account lookup. All locks, peer checks and artifact work
/// still execute on the actual judged directories and serving connection.
pub(super) fn run(
    config: &ServiceConfig,
    agent: &AgentName,
    cause: Cause,
    member: impl FnOnce(&AgentName) -> Result<u32, LifecycleRefusal>,
) -> Result<Taken, Failure> {
    crate::admissible(config, agent)?;
    let directory = crate::prepare_run_directory(config, nix::unistd::geteuid().as_raw())?;
    let _invocation =
        start::take_invocation_lock(&directory, crate::verb_table::HolderVerb::SavePoint)?;
    if !start::run_lock_held(&directory)? {
        return Err(LifecycleRefusal::OutOfOrder.into());
    }
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
    let mut observation = crate::dial_worker(config).map_err(FailureCause::Channel)?;
    let observed = Observed::ask_loaded(&mut observation, &repository, &config.territory, true)?;
    require_open(config, &observed.facts.run)?;
    let member_uid = member(agent)?;
    let room = save_points::open_room(config.territory_fd()?)?
        .ok_or(LifecycleRefusal::BoundaryUnverified)?;
    let stat =
        nix::sys::stat::fstat(room.as_fd()).map_err(|_| LifecycleRefusal::BoundaryUnverified)?;
    if stat.st_uid != member_uid || stat.st_mode & 0o7777 != 0o700 {
        return Err(LifecycleRefusal::BoundaryUnverified.into());
    }
    let channel = crate::dial_worker(config).map_err(FailureCause::Channel)?;
    channel
        .require_same_worker(&observation)
        .map_err(FailureCause::Channel)?;
    drop(observation);
    // Save-specific send/copy/publication work uses this original loaded window.
    let mut capture = Capture::demand(observed, &mut repository, channel, room, member_uid, cause)?;
    while capture.advance() == Progress::Pending {}
    if capture.progress() == Progress::Stopped {
        let cause = capture
            .failure
            .take()
            .expect("stopped capture retains its first failure");
        return Err(Failure {
            cause: Fault::Sequence(cause),
            evidence: Some(Box::new(evidence(capture, None))),
        });
    }
    if capture.progress() != Progress::TakenReceived {
        return Err(Failure {
            cause: Fault::Sequence(FailureCause::Protocol(
                "demand capture did not answer Taken",
            )),
            evidence: Some(Box::new(evidence(capture, None))),
        });
    }
    if let Err(cause) = require_open(config, &capture.observed.facts.run) {
        if let Some(publication) = &mut capture.publication {
            capture.publication_failure =
                Some(publication.failed("save outcome no longer matches the Open marker"));
        }
        return Err(Failure {
            cause: Fault::Admission(cause),
            evidence: Some(Box::new(evidence(capture, None))),
        });
    }
    let identity = match capture.finish_demand() {
        Ok(identity) => identity,
        Err(cause) => {
            return Err(Failure {
                cause: Fault::Sequence(cause),
                evidence: Some(Box::new(evidence(capture, None))),
            });
        }
    };
    let report = capture.report.as_ref().unwrap().clone();
    Ok(Taken {
        report,
        evidence: Box::new(evidence(capture, Some(identity))),
    })
}

fn require_open(config: &ServiceConfig, run: &RunId) -> Result<(), LifecycleRefusal> {
    match crate::read_marker_or_refuse(config)? {
        Some(save_points::Marker::Open { run: current }) if current == run.0 => Ok(()),
        _ => Err(LifecycleRefusal::BoundaryUnverified),
    }
}

fn evidence(capture: Capture<'_>, completed: Option<StartIdentity>) -> Evidence {
    let capture_id = capture.windows.capture().map(|value| value.capture_id);
    let RequestEnvelope::Demand(request) = capture.directive else {
        unreachable!("demand invocation owns its original SavePoint")
    };
    Evidence {
        run: capture.observed.facts.run,
        loaded: capture.observed.source.identity(),
        request,
        received: capture.received,
        unexpected: capture.unexpected,
        report: capture.report,
        publication_failure: capture.publication_failure,
        committed: capture.committed,
        completed,
        abort: capture.abort,
        cleanup_failure: capture.cleanup_failure,
        capture_id,
    }
}

impl Failure {
    /// Only an actual correlated worker refusal supplies a retryable save code.
    /// Failed publication locations always come from the root-owned publisher.
    pub(crate) fn refusal(&self) -> CaptureRefusal {
        if matches!(
            self.cause,
            Fault::Sequence(FailureCause::Channel(
                crate::channel::ChannelFault::Unanswered
            ))
        ) || self.evidence.as_ref().is_some_and(|evidence| {
            matches!(
                evidence.cleanup_failure,
                Some(FailureCause::Channel(
                    crate::channel::ChannelFault::Unanswered
                ))
            )
        }) {
            return CaptureRefusal::Lifecycle(LifecycleRefusal::Unanswered);
        }
        if let Some(evidence) = &self.evidence
            && evidence.committed
            && evidence.completed.is_none()
            && evidence.report.is_none()
            && let Some(publication) = &evidence.publication_failure
        {
            return CaptureRefusal::Save(SaveRefusal::FailedRun {
                run: evidence.run.clone(),
                capture_id: evidence.capture_id,
                phase: publication.phase,
                published: publication.published.clone(),
                artifacts: publication.artifacts.clone(),
                detail: self.to_string(),
            });
        }
        let lifecycle = match &self.cause {
            Fault::Admission(refusal) => refusal.clone(),
            Fault::Sequence(FailureCause::Observe(failure)) => match &**failure {
                crate::serving_observe::Failure::Refused(refusal) => refusal.clone(),
                crate::serving_observe::Failure::Channel(fault) => channel_refusal(fault),
            },
            Fault::Sequence(FailureCause::InTransition { .. }) => LifecycleRefusal::OutOfOrder,
            Fault::Sequence(FailureCause::Channel(fault)) => channel_refusal(fault),
            Fault::Sequence(FailureCause::Protocol(_)) => LifecycleRefusal::Malformed,
            Fault::Sequence(FailureCause::Artifact(fault)) => {
                LifecycleRefusal::StatePersistenceInvalid {
                    stage: weaver_types::state_flow::PersistenceStage::Load,
                    case: fault.case,
                    artifact_id: None,
                    detail: self.to_string(),
                }
            }
            Fault::Sequence(FailureCause::Publication(_)) => {
                LifecycleRefusal::StatePersistenceInvalid {
                    stage: weaver_types::state_flow::PersistenceStage::Publish,
                    case: weaver_types::state_flow::PersistenceCase::PublicationFailed,
                    artifact_id: self.evidence.as_ref().and_then(|evidence| {
                        evidence
                            .publication_failure
                            .as_ref()?
                            .published
                            .as_ref()
                            .map(|value| value.artifact_id.clone())
                    }),
                    detail: self.to_string(),
                }
            }
            Fault::Sequence(FailureCause::WorkerOutcome) => {
                if let Some(evidence) = &self.evidence
                    && let Some(received) = evidence
                        .unexpected
                        .as_ref()
                        .or_else(|| evidence.received.last())
                    && let CapturePayload::Refusal(refusal) = &received.envelope.payload
                    && !matches!(refusal, CaptureRefusal::Save(SaveRefusal::FailedRun { .. }))
                {
                    return refusal.clone();
                }
                LifecycleRefusal::Unanswered
            }
        };
        CaptureRefusal::Lifecycle(lifecycle)
    }
}

fn channel_refusal(fault: &crate::channel::ChannelFault) -> LifecycleRefusal {
    use crate::channel::ChannelFault;
    match fault {
        ChannelFault::WrongPeer { .. } => LifecycleRefusal::BoundaryUnverified,
        ChannelFault::Undecodable | ChannelFault::Truncated { .. } => LifecycleRefusal::Malformed,
        ChannelFault::Closed
        | ChannelFault::NoListener
        | ChannelFault::NotDialable
        | ChannelFault::Unanswered => LifecycleRefusal::Unanswered,
    }
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "save-point: {:?}", self.cause)?;
        if let Some(evidence) = &self.evidence {
            if let Some(cleanup) = &evidence.cleanup_failure {
                write!(f, "; capture cleanup: {cleanup:?}")?;
            }
            if let Some(publication) = &evidence.publication_failure {
                write!(f, "; retained publication: {publication}")?;
                for artifact in &publication.artifacts {
                    write!(f, "; {:?}: {}", artifact.kind, artifact.location)?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "serving_save_response_tests.rs"]
mod tests;
