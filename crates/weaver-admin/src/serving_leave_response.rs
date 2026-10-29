//! Public refusal projection of the retained, terminal Leave account. This
//! performs no filesystem work, consumes no evidence and grants no retry.
use super::{Evidence, Failure, Fault};
use crate::channel::ChannelFault;
use crate::serving_leave::FailureCause;
use weaver_types::LifecycleRefusal;
use weaver_types::state_flow::capture::{CapturePayload, CaptureRefusal, SaveRefusal};
use weaver_types::state_flow::{PersistenceCase, PersistenceStage};

impl Failure {
    /// The paired serving surface stays dormant with its caller until cutover.
    /// Worker-supplied paths and identities never become publication evidence.
    pub(crate) fn refusal(&self) -> CaptureRefusal {
        if let Some(evidence) = &self.evidence
            && evidence.committed
            && evidence.report.is_none()
            && evidence.completed.is_none()
            && capture_failed(&self.cause)
        {
            // Commit can only be accepted after an owned publication exists.
            // Its failure account probes extant files and verifies index append.
            let publication = evidence
                .publication_failure
                .as_ref()
                .expect("failed committed capture retains its publication account");
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
            Fault::Sequence(FailureCause::Observe(failure)) => match failure.as_ref() {
                crate::serving_observe::Failure::Refused(refusal) => refusal.clone(),
                crate::serving_observe::Failure::Channel(fault) => channel_refusal(fault),
            },
            Fault::Sequence(FailureCause::InTransition { .. }) => LifecycleRefusal::OutOfOrder,
            Fault::Sequence(FailureCause::Artifact(fault)) => {
                LifecycleRefusal::StatePersistenceInvalid {
                    stage: PersistenceStage::Load,
                    case: fault.case,
                    artifact_id: None,
                    detail: self.to_string(),
                }
            }
            Fault::Sequence(FailureCause::Publication(_)) => {
                LifecycleRefusal::StatePersistenceInvalid {
                    stage: PersistenceStage::Publish,
                    case: PersistenceCase::PublicationFailed,
                    artifact_id: self.evidence.as_deref().and_then(|evidence| {
                        evidence
                            .publication_failure
                            .as_ref()?
                            .published
                            .as_ref()
                            .map(|identity| identity.artifact_id.clone())
                    }),
                    detail: self.to_string(),
                }
            }
            Fault::Sequence(FailureCause::Channel(fault)) => channel_refusal(fault),
            Fault::Sequence(FailureCause::Protocol(_)) => LifecycleRefusal::Malformed,
            Fault::Sequence(FailureCause::WorkerOutcome) => {
                if let Some(refusal) =
                    self.evidence
                        .as_deref()
                        .and_then(worker_refusal)
                        .or_else(|| {
                            match &self.joined.as_deref()?.received.as_ref()?.envelope.payload {
                                CapturePayload::Refusal(refusal) => Some(refusal),
                                _ => None,
                            }
                        })
                {
                    return crate::serving_leave::project_worker_refusal(refusal);
                }
                LifecycleRefusal::Unanswered
            }
        };
        CaptureRefusal::Lifecycle(lifecycle)
    }
}

fn capture_failed(cause: &Fault) -> bool {
    matches!(
        cause,
        Fault::Sequence(
            FailureCause::WorkerOutcome | FailureCause::Protocol(_) | FailureCause::Publication(_)
        ) | Fault::Sequence(FailureCause::Channel(
            ChannelFault::Closed | ChannelFault::Truncated { .. } | ChannelFault::Undecodable
        ))
    )
}

fn channel_refusal(fault: &ChannelFault) -> LifecycleRefusal {
    match fault {
        ChannelFault::WrongPeer { .. } => LifecycleRefusal::BoundaryUnverified,
        ChannelFault::Undecodable | ChannelFault::Truncated { .. } => LifecycleRefusal::Malformed,
        ChannelFault::Closed
        | ChannelFault::NoListener
        | ChannelFault::NotDialable
        | ChannelFault::Unanswered => LifecycleRefusal::Unanswered,
    }
}

fn worker_refusal(evidence: &Evidence) -> Option<&CaptureRefusal> {
    // WorkerOutcome is set only after correlation; raw packets from a protocol
    // failure never reach this arm, including a foreign run or capture ID.
    let received = evidence
        .unexpected
        .as_ref()
        .or_else(|| evidence.received.last())?;
    match &received.envelope.payload {
        CapturePayload::Refusal(refusal) => Some(refusal),
        _ => None,
    }
}

/// Keep the first cause and separate cleanup result available to stderr and the
/// operations log. This describes known evidence without rereading a live room.
impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.cause {
            Fault::Admission(refusal) => write!(f, "Leave admission or conclusion: {refusal:?}")?,
            Fault::Sequence(FailureCause::Observe(failure)) => {
                write!(f, "Leave observation: {failure:?}")?
            }
            Fault::Sequence(FailureCause::InTransition { constituents }) => {
                write!(f, "graceful Leave found InTransition: {constituents:?}")?
            }
            Fault::Sequence(FailureCause::Artifact(fault)) => {
                write!(f, "loaded artifact: {fault}")?
            }
            Fault::Sequence(FailureCause::Channel(fault)) => write!(f, "Leave channel: {fault:?}")?,
            Fault::Sequence(FailureCause::Protocol(detail)) => {
                write!(f, "Leave protocol: {detail}")?
            }
            Fault::Sequence(FailureCause::WorkerOutcome) => {
                f.write_str("worker refused the capture or ended it without successful Left")?
            }
            Fault::Sequence(FailureCause::Publication(failure)) => {
                write!(f, "Leave publication: {failure}")?
            }
        }
        if let Some(evidence) = &self.evidence {
            if let Some(publication) = &evidence.publication_failure {
                write!(f, "; publication account: {publication}")?;
                for artifact in &publication.artifacts {
                    write!(f, "; retained {:?}: {}", artifact.kind, artifact.location)?;
                }
            }
            if let Some(completed) = &evidence.completed {
                write!(f, "; completed save remains {}", completed.artifact_id)?;
            }
        }
        if let Some(cleanup) = &self.cleanup {
            write!(f, "; separate run cleanup refusal: {cleanup:?}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "serving_leave_response_tests.rs"]
mod tests;
