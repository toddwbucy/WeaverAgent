//! The one admitted overlap: force joins an unload, never its publisher.
use super::{Concluded, Failure, FailureCause};
use crate::channel::{ChannelFault, ReceivedCapture};
use crate::save_points::serving::repository::Repository;
use crate::serving_capture_window::CaptureWindows;
use crate::{
    ServiceConfig, UnloadBounds, agent_cgroup, conclusion, save_points, start, verb_table,
};
use weaver_types::state_flow::capture::{
    CaptureAnswer, CapturePayload, CaptureRefusal, SaveRefusal,
};
use weaver_types::{
    Cause, ExchangeId, LifecycleAnswer, LifecycleDirective, LifecycleRefusal, Opener,
    OrganEnvelope, Payload, Position,
};

#[derive(Debug)]
pub(crate) struct Evidence {
    /// Some(None) means a readable marker with no open run. None means unreadable
    /// until the existing marker rule reads it again behind the invocation lock.
    pub(crate) noted_run: Option<Option<String>>,
    pub(crate) directive: Option<OrganEnvelope>,
    pub(crate) received: Option<ReceivedCapture>,
}

pub(super) enum Taken {
    Sole(start::InvocationLock),
    Joined(Box<Concluded>),
}

pub(super) fn take_or_join(
    config: &ServiceConfig,
    bounds: UnloadBounds,
    cause: Cause,
) -> Result<Taken, Failure> {
    let directory = config.run_directory();
    loop {
        match start::take_invocation_lock_judged(&directory, verb_table::HolderVerb::ForceUnload) {
            Ok(lock) => return Ok(Taken::Sole(lock)),
            Err(start::TakeRefused::Starved) => {
                return Err(LifecycleRefusal::InvocationInFlight.into());
            }
            Err(start::TakeRefused::Other(LifecycleRefusal::InvocationInFlight)) => (),
            Err(start::TakeRefused::Other(refusal)) => return Err(refusal.into()),
        }
        match start::read_holder(&directory)? {
            start::LockHolder::Free | start::LockHolder::Shared => continue,
            start::LockHolder::Exclusive(verb_table::HolderVerb::Unload) => (),
            start::LockHolder::Exclusive(_) => {
                return Err(LifecycleRefusal::InvocationInFlight.into());
            }
        }
        let mut evidence = Box::new(Evidence {
            noted_run: crate::open_run(config).ok(),
            directive: None,
            received: None,
        });
        match join(config, bounds, cause, &mut evidence) {
            Ok(Some(answer)) => {
                return Ok(Taken::Joined(Box::new(Concluded {
                    answer,
                    evidence: None,
                    joined: Some(evidence),
                })));
            }
            // The unload retired or another holder won while an OutOfOrder
            // was retried. Judge the lock afresh, never infer the new verb.
            Ok(None) => (),
            Err(mut failure) => {
                failure.joined = Some(evidence);
                return Err(failure);
            }
        }
    }
}

fn join(
    config: &ServiceConfig,
    bounds: UnloadBounds,
    cause: Cause,
    evidence: &mut Evidence,
) -> Result<Option<LifecycleAnswer>, Failure> {
    loop {
        // The existing join bound covers a busy dial, send and final answer.
        // An OutOfOrder exchange retires before the specified retry begins.
        let windows = CaptureWindows::join(bounds.leave).map_err(FailureCause::Channel)?;
        let mut channel = loop {
            match crate::dial_worker(config) {
                Ok(channel) => break channel,
                Err(ChannelFault::NoListener) => {
                    return crate::wait_after_seal(config, bounds, &evidence.noted_run)
                        .map(Some)
                        .map_err(Into::into);
                }
                Err(ChannelFault::WrongPeer { .. }) => {
                    return crate::escalate_joined(config, bounds, &evidence.noted_run)
                        .map(Some)
                        .map_err(Into::into);
                }
                Err(_) if windows.check().is_ok() => (),
                Err(_) => {
                    return crate::escalate_joined(config, bounds, &evidence.noted_run)
                        .map(Some)
                        .map_err(Into::into);
                }
            }
        };
        evidence.received = None;
        evidence.directive = Some(OrganEnvelope {
            exchange: ExchangeId {
                opener: Opener::Admin,
                ordinal: channel.next_ordinal(),
            },
            position: Position::Open,
            payload: Payload::Directive(LifecycleDirective::JoinLeave { cause }),
        });
        let directive = evidence.directive.as_ref().unwrap();
        if let Err(error) = channel.send_capture_join(directive, &windows) {
            return if error == ChannelFault::Unanswered {
                crate::escalate_joined(config, bounds, &evidence.noted_run)
                    .map(Some)
                    .map_err(Into::into)
            } else {
                Err(after_failed_answer(
                    config,
                    bounds,
                    FailureCause::Channel(error).into(),
                ))
            };
        }
        let received = match channel.recv_capture(&windows) {
            Ok(received) => received,
            Err(ChannelFault::Undecodable | ChannelFault::Truncated { .. }) => {
                return Err(after_failed_answer(
                    config,
                    bounds,
                    FailureCause::Protocol("joined answer is malformed").into(),
                ));
            }
            Err(ChannelFault::Unanswered) => {
                return crate::escalate_joined(config, bounds, &evidence.noted_run)
                    .map(Some)
                    .map_err(Into::into);
            }
            Err(error) => {
                return Err(after_failed_answer(
                    config,
                    bounds,
                    FailureCause::Channel(error).into(),
                ));
            }
        };
        evidence.received = Some(received);
        let received = evidence.received.as_ref().unwrap();
        if received.envelope.exchange != directive.exchange
            || received.envelope.position != Position::Close
        {
            return Err(after_failed_answer(
                config,
                bounds,
                FailureCause::Protocol("joined answer has a foreign exchange or phase").into(),
            ));
        }
        match &received.envelope.payload {
            CapturePayload::Answer(CaptureAnswer::Left {
                save_point: report,
                no_state: false,
                forced,
            }) => {
                if (config.territory_unjudged || report.is_none()) && !forced {
                    return Err(after_failed_answer(
                        config,
                        bounds,
                        FailureCause::Protocol("unjudged territory requires a forced Left").into(),
                    ));
                }
                if evidence
                    .noted_run
                    .as_ref()
                    .and_then(Option::as_ref)
                    .is_some_and(|run| {
                        report
                            .as_ref()
                            .is_some_and(|report| *run != report.event_run.0)
                    })
                {
                    return Err(after_failed_answer(
                        config,
                        bounds,
                        FailureCause::Protocol("joined Left names another run").into(),
                    ));
                }
                return conclude(config, bounds, evidence, report.as_ref(), *forced).map(Some);
            }
            CapturePayload::Refusal(CaptureRefusal::Lifecycle(LifecycleRefusal::OutOfOrder)) => {
                std::thread::sleep(crate::JOIN_RETRY);
                if start::read_holder(&config.run_directory())?
                    != start::LockHolder::Exclusive(verb_table::HolderVerb::Unload)
                {
                    return Ok(None);
                }
            }
            CapturePayload::Refusal(_) => {
                return Err(
                    match refusal_outcome(&received.envelope, &evidence.noted_run) {
                        Ok(true) => {
                            after_failed_answer(config, bounds, FailureCause::WorkerOutcome.into())
                        }
                        Ok(false) => FailureCause::WorkerOutcome.into(),
                        Err(cause) => after_failed_answer(config, bounds, cause.into()),
                    },
                );
            }
            _ => {
                return Err(after_failed_answer(
                    config,
                    bounds,
                    FailureCause::Protocol("joined exchange did not finish with a serving Left")
                        .into(),
                ));
            }
        }
    }
}

/// The join has no Beginning and no capture ID of its own. Compare every run
/// identity it did observe, and never project a foreign/unverifiable FailedRun.
fn refusal_outcome(
    envelope: &weaver_types::state_flow::capture::CaptureEnvelope,
    noted_run: &Option<Option<String>>,
) -> Result<bool, FailureCause> {
    let CapturePayload::Refusal(refusal) = &envelope.payload else {
        return Err(FailureCause::Protocol("joined answer is not a refusal"));
    };
    if matches!(refusal, CaptureRefusal::Save(SaveRefusal::FailedRun { .. }))
        && !noted_run
            .as_ref()
            .and_then(Option::as_ref)
            .is_some_and(|run| {
                crate::serving_leave::same_run(envelope, &weaver_types::RunId(run.clone()))
            })
    {
        return Err(FailureCause::Protocol(
            "joined refusal lacks the observed run",
        ));
    }
    if crate::terminal_unwind_refusal(refusal) {
        return Ok(true);
    }
    if matches!(refusal, CaptureRefusal::Lifecycle(value) if crate::standing_leave_refusal(value)) {
        return Ok(false);
    }
    Err(FailureCause::Protocol(
        "joined refusal is not an admitted Leave outcome",
    ))
}

fn after_failed_answer(
    config: &ServiceConfig,
    bounds: UnloadBounds,
    mut failure: Failure,
) -> Failure {
    failure.cleanup = crate::end_after_left(&config.run_directory(), bounds)
        .err()
        .map(Box::new);
    failure
}

fn conclude(
    config: &ServiceConfig,
    bounds: UnloadBounds,
    evidence: &Evidence,
    report: Option<&weaver_types::state_flow::SavePointReport>,
    forced: bool,
) -> Result<LifecycleAnswer, Failure> {
    let _behind = start::take_invocation_lock_waiting(
        &config.run_directory(),
        verb_table::HolderVerb::ForceUnload,
    )?;
    let run = crate::run_noted(config, &evidence.noted_run)?;
    if let Some(answer) = crate::behind_another_run(config, bounds, run.as_deref())? {
        return answer.map_err(Into::into);
    }
    // The marker may first become readable here. A report that was not compared
    // with an open run before this lock still cannot conclude a different run.
    if run
        .as_deref()
        .is_some_and(|run| report.is_some_and(|report| run != report.event_run.0))
    {
        return Err(FailureCause::Protocol("joined Left differs from the resolved run").into());
    }
    crate::end_after_left(&config.run_directory(), bounds)?;
    crate::end_agent_cgroup(
        config,
        "force-unload",
        run.as_deref().map_or(
            agent_cgroup::Concluding::Recorded,
            agent_cgroup::Concluding::Run,
        ),
    )?;
    if !matches!(crate::read_marker_or_refuse(config)?,
        Some(save_points::Marker::Open { run: open }) if Some(open.as_str()) == run.as_deref())
    {
        return crate::unloaded_by_ladder(config).map_err(Into::into);
    }
    if config.territory_unjudged {
        crate::write_by_rule(config, run.as_deref(), conclusion::MarkerWrite::Forced)?;
    } else if let Some(report) = report {
        let verify = || -> Result<(), crate::save_points::serving::Fault> {
            let repository = Repository::from_judged_directory(
                config
                    .save_points_fd()
                    .map_err(|_| artifact_failure("missing save directory"))?
                    .try_clone_to_owned()
                    .map_err(|_| artifact_failure("clone save directory"))?,
                save_points::ROOT,
                config.access_gid,
            )?;
            repository.verify_completed_leave(report, forced, &config.territory)
        };
        verify().map_err(|error| {
            Failure::from(LifecycleRefusal::StatePersistenceInvalid {
                stage: weaver_types::state_flow::PersistenceStage::Publish,
                case: error.case,
                artifact_id: Some(report.start.artifact_id.clone()),
                detail: error.to_string(),
            })
        })?;
        crate::write_by_rule(config, run.as_deref(), conclusion::MarkerWrite::Closed)?;
    } else {
        // The original publisher concluded a refused capture. This join has
        // no artifact to verify or publish, but still owes the actual run end.
        crate::write_by_rule(config, run.as_deref(), conclusion::MarkerWrite::Closed)?;
    }
    crate::unloaded_by_ladder(config).map_err(Into::into)
}

fn artifact_failure(detail: &str) -> crate::save_points::serving::Fault {
    crate::save_points::serving::Fault {
        case: weaver_types::state_flow::PersistenceCase::PublicationFailed,
        detail: detail.into(),
    }
}
