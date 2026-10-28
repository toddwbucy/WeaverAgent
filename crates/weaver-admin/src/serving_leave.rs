//! Shared admin capture sequence for a demand save or an accepted Leave.
//! The original observation and publisher stay owned through actual Left.
//! Run-end/conclusion and the refused-capture disposition remain the outer owner's.
use crate::channel::{ChannelFault, Coordination, ReceivedCapture};
use crate::save_points::serving::Fault;
#[cfg(test)]
use crate::save_points::serving::codec::ArtifactId;
use crate::save_points::serving::publication::{self, Publication, ServingImage};
use crate::save_points::serving::repository::{LoadScope, Repository, VerifiedArtifact};
use crate::serving_capture_window::CaptureWindows;
use std::os::fd::{AsFd, OwnedFd};
use std::path::PathBuf;
use std::time::Duration;
use weaver_types::state_flow::capture::{
    CaptureAnswer, CaptureArtifact, CaptureDirective, CaptureEnvelope, CaptureImageChecked,
    CapturePayload, SavePointLeg,
};
use weaver_types::state_flow::enter::{LoadFacts, LoadedStateFacts, ObservationAnswer};
use weaver_types::state_flow::{PersistenceCause, PersistencePhase, SavePointReport};
use weaver_types::{AgentState, Cause, ExchangeId, LifecycleDirective, Opener, Position};

#[path = "serving_leave_invocation.rs"]
pub(crate) mod invocation;

#[path = "serving_save_invocation.rs"]
pub(crate) mod save;

#[path = "serving_leave_refusal.rs"]
mod refusal;

#[derive(Debug)]
pub(crate) enum FailureCause {
    Observe(Box<crate::serving_observe::Failure>),
    InTransition {
        constituents: Vec<u32>,
    },
    Artifact(Box<Fault>),
    Channel(ChannelFault),
    /// The retained packet elects neither a final Leave nor healthy cleanup.
    WorkerOutcome,
    Protocol(&'static str),
    Publication(Box<publication::Failure>),
}

/// Constructed only by the paired Observe exchange and indexed-copy judgment.
pub(crate) struct Observed {
    facts: Box<LoadFacts>,
    source: VerifiedArtifact,
    trace_path: PathBuf,
}

impl Observed {
    pub(crate) fn ask_loaded(
        channel: &mut Coordination,
        repository: &Repository,
        territory: &std::path::Path,
        graceful: bool,
    ) -> Result<Self, FailureCause> {
        let facts = Self::facts(channel, graceful)?;
        let LoadedStateFacts::Serving { start, .. } = &facts.state else {
            unreachable!("facts rejects diagnostic loads")
        };
        let source = repository
            .verify_loaded(start, &facts.session, &facts.declaration, territory)
            .map_err(|error| FailureCause::Artifact(Box::new(error)))?;
        Self::checked(facts, source)
    }

    #[cfg(test)]
    pub(crate) fn ask(
        channel: &mut Coordination,
        repository: &Repository,
        scope: &LoadScope<'_>,
    ) -> Result<Self, FailureCause> {
        let facts = Self::facts(channel, false)?;
        if facts.session != *scope.session {
            return Err(FailureCause::Protocol("observation has a foreign session"));
        }
        let LoadedStateFacts::Serving { start, .. } = &facts.state else {
            unreachable!()
        };
        let id = ArtifactId::parse(&start.artifact_id)
            .map_err(|_| FailureCause::Protocol("invalid observed artifact ID"))?;
        let source = repository
            .verify_indexed(&id, scope)
            .map_err(|error| FailureCause::Artifact(Box::new(error)))?;
        Self::checked(facts, source)
    }

    fn facts(channel: &mut Coordination, graceful: bool) -> Result<Box<LoadFacts>, FailureCause> {
        let reply = crate::serving_observe::ask(channel)
            .map_err(|error| FailureCause::Observe(Box::new(error)))?;
        if graceful && let ObservationAnswer::InTransition { constituents } = reply {
            return Err(FailureCause::InTransition { constituents });
        }
        let ObservationAnswer::State {
            state: AgentState::Idle | AgentState::Active,
            load: Some(facts),
            ..
        } = reply
        else {
            return Err(FailureCause::Protocol("capture requires a loaded run"));
        };
        let LoadedStateFacts::Serving { start, .. } = &facts.state else {
            return Err(FailureCause::Protocol("capture requires Serving facts"));
        };
        if facts.run.0.is_empty() || facts.declaration != start.config_sha256.as_str() {
            return Err(FailureCause::Protocol(
                "observation has a foreign load identity",
            ));
        }
        Ok(facts)
    }

    fn checked(facts: Box<LoadFacts>, source: VerifiedArtifact) -> Result<Self, FailureCause> {
        let LoadedStateFacts::Serving {
            start,
            state_management,
            ..
        } = &facts.state
        else {
            unreachable!("facts rejects diagnostic loads")
        };
        if source.identity() != *start
            || source.declaration().state_management() != state_management
        {
            return Err(FailureCause::Protocol(
                "observed settings differ from loaded copy",
            ));
        }
        let trace_path =
            crate::save_points::serving::trace_path(&source.declaration().agent().trace_sink)
                .into();
        Ok(Self {
            facts,
            source,
            trace_path,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    SendRequest,
    Beginning,
    Image,
    Hashing,
    Finished,
    Preparing,
    Commit,
    Publishing,
    Complete,
    Left,
    LeftReceived,
    TakenReceived,
    Aborting,
    AwaitAbortOutcome,
    AwaitCaptureRefused,
    AwaitRefusedLeave,
    Stopped,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Progress {
    Pending,
    LeftReceived,
    TakenReceived,
    Stopped,
}

/// The containing invocation retains its lock and judges the room before begin.
/// This owner neither writes a marker nor grants a successful command result.
pub(crate) struct Capture<'repo> {
    observed: Observed,
    repository: Option<&'repo mut Repository>,
    publication: Option<Publication<'repo>>,
    channel: Coordination,
    room: OwnedFd,
    member_uid: u32,
    directive: RequestEnvelope,
    windows: CaptureWindows,
    stage: Stage,
    received: Vec<ReceivedCapture>,
    unexpected: Option<ReceivedCapture>,
    prepared: Option<CaptureArtifact>,
    published: Option<weaver_types::state_flow::StartIdentity>,
    committed: bool,
    report: Option<SavePointReport>,
    failure: Option<FailureCause>,
    publication_failure: Option<publication::Failure>,
    checkpoint_failure: Option<FailureCause>,
    abort: Option<CaptureEnvelope>,
    aborted_at: Option<Stage>,
    abort_crossed: bool,
    refused: Option<SavePointLeg>,
    cleanup_failure: Option<FailureCause>,
}

#[derive(Debug)]
enum RequestEnvelope {
    Leave(weaver_types::OrganEnvelope),
    Demand(CaptureEnvelope),
}
impl RequestEnvelope {
    fn exchange(&self) -> &ExchangeId {
        match self {
            Self::Leave(value) => &value.exchange,
            Self::Demand(value) => &value.exchange,
        }
    }
}

struct Request {
    forced: bool,
    cause: Cause,
    tail: Duration,
}

impl<'repo> Capture<'repo> {
    pub(crate) fn demand(
        observed: Observed,
        repository: &'repo mut Repository,
        mut channel: Coordination,
        room: OwnedFd,
        member_uid: u32,
        cause: Cause,
    ) -> Result<Self, FailureCause> {
        let wait = observed
            .source
            .declaration()
            .state_management()
            .persistence
            .save_wait
            .elapsed_ns
            .0;
        let windows = CaptureWindows::demand(wait).map_err(FailureCause::Channel)?;
        let directive = CaptureEnvelope {
            exchange: ExchangeId {
                opener: Opener::Admin,
                ordinal: channel.next_ordinal(),
            },
            position: Position::Open,
            payload: CapturePayload::Directive(CaptureDirective::SavePoint {
                cause,
                window: windows.demand_window().expect("original demand window"),
            }),
        };
        Ok(Self::new(
            observed,
            repository,
            channel,
            room,
            member_uid,
            RequestEnvelope::Demand(directive),
            windows,
        ))
    }

    pub(crate) fn begin(
        observed: Observed,
        repository: &'repo mut Repository,
        channel: Coordination,
        room: OwnedFd,
        member_uid: u32,
        cause: Cause,
        outer: Duration,
    ) -> Result<Self, FailureCause> {
        Self::begin_request(
            observed,
            repository,
            channel,
            room,
            member_uid,
            Request {
                forced: true,
                cause,
                tail: outer,
            },
        )
    }

    pub(crate) fn graceful(
        observed: Observed,
        repository: &'repo mut Repository,
        channel: Coordination,
        room: OwnedFd,
        member_uid: u32,
        cause: Cause,
        tail: Duration,
    ) -> Result<Self, FailureCause> {
        Self::begin_request(
            observed,
            repository,
            channel,
            room,
            member_uid,
            Request {
                forced: false,
                cause,
                tail,
            },
        )
    }

    fn begin_request(
        observed: Observed,
        repository: &'repo mut Repository,
        mut channel: Coordination,
        room: OwnedFd,
        member_uid: u32,
        request: Request,
    ) -> Result<Self, FailureCause> {
        let save_wait = observed
            .source
            .declaration()
            .state_management()
            .persistence
            .save_wait
            .elapsed_ns
            .0;
        let windows = if request.forced {
            CaptureWindows::begin(request.tail, save_wait)
        } else {
            CaptureWindows::graceful(
                request.tail,
                observed.facts.drain_bound,
                observed.facts.wind_down_bound,
                save_wait,
            )
        }
        .map_err(FailureCause::Channel)?;
        let directive = weaver_types::OrganEnvelope {
            exchange: ExchangeId {
                opener: Opener::Admin,
                ordinal: channel.next_ordinal(),
            },
            position: Position::Open,
            payload: weaver_types::Payload::Directive(LifecycleDirective::Leave {
                cause: request.cause,
                forced: request.forced,
                rollback: false,
            }),
        };
        Ok(Self::new(
            observed,
            repository,
            channel,
            room,
            member_uid,
            RequestEnvelope::Leave(directive),
            windows,
        ))
    }

    fn new(
        observed: Observed,
        repository: &'repo mut Repository,
        channel: Coordination,
        room: OwnedFd,
        member_uid: u32,
        directive: RequestEnvelope,
        windows: CaptureWindows,
    ) -> Self {
        Self {
            observed,
            repository: Some(repository),
            publication: None,
            channel,
            room,
            member_uid,
            directive,
            windows,
            stage: Stage::SendRequest,
            received: Vec::new(),
            unexpected: None,
            prepared: None,
            published: None,
            committed: false,
            report: None,
            failure: None,
            publication_failure: None,
            checkpoint_failure: None,
            abort: None,
            aborted_at: None,
            abort_crossed: false,
            refused: None,
            cleanup_failure: None,
        }
    }

    pub(crate) fn progress(&self) -> Progress {
        match self.stage {
            Stage::LeftReceived => Progress::LeftReceived,
            Stage::TakenReceived => Progress::TakenReceived,
            Stage::Stopped => Progress::Stopped,
            _ => Progress::Pending,
        }
    }

    pub(crate) fn failure(&self) -> Option<&FailureCause> {
        self.failure.as_ref()
    }

    pub(crate) fn cleanup_failure(&self) -> Option<&FailureCause> {
        self.cleanup_failure.as_ref()
    }

    /// Finish only the exact timely demand outcome. The normal peer closes
    /// after Taken, so guard removal checks the retained deadline, not new I/O.
    pub(crate) fn finish_demand(
        &mut self,
    ) -> Result<weaver_types::state_flow::StartIdentity, FailureCause> {
        if self.stage != Stage::TakenReceived || self.windows.demand_window().is_none() {
            return Err(FailureCause::Protocol(
                "demand publication lacks SavePointTaken",
            ));
        }
        let result = self
            .publication
            .as_mut()
            .expect("Taken follows indexed publication")
            .finish(
                publication::FinalOutcome::SavePointTaken(
                    self.report.as_ref().expect("Taken retains report"),
                ),
                &mut |_| {
                    self.windows
                        .check()
                        .map_err(|error| format!("save outcome window: {error:?}"))
                },
            );
        match result {
            Ok(identity) => Ok(identity),
            Err(error) => {
                self.publication_failure = Some(error.clone());
                self.stage = Stage::Stopped;
                Err(FailureCause::Publication(Box::new(error)))
            }
        }
    }

    /// The containing invocation has completed its normal successful-Left
    /// duties. Use the validated whole reply's settled mode, never the request.
    pub(crate) fn finish_leave(
        &mut self,
    ) -> Result<weaver_types::state_flow::StartIdentity, FailureCause> {
        if self.stage != Stage::LeftReceived {
            return Err(FailureCause::Protocol("Leave publication lacks Left"));
        }
        let CapturePayload::Answer(CaptureAnswer::Left {
            save_point: Some(report),
            forced,
            ..
        }) = &self
            .received
            .last()
            .expect("LeftReceived retains reply")
            .envelope
            .payload
        else {
            return Err(FailureCause::Protocol(
                "refused capture has no completed save",
            ));
        };
        self.publication
            .as_mut()
            .expect("Left follows publication")
            .finish(
                publication::FinalOutcome::Left {
                    report,
                    forced: *forced,
                },
                &mut |_| Ok(()),
            )
            .map_err(|error| {
                self.publication_failure = Some(error.clone());
                self.stage = Stage::Stopped;
                FailureCause::Publication(Box::new(error))
            })
    }

    /// An expired original initial-Leave wait, or a forced directive's matching
    /// OutOfOrder, selects S11. A capture that has begun, EOF, a protocol
    /// failure or a different refusal supplies no such escalation authority.
    fn initial_escalation_due(&self) -> bool {
        if self.windows.demand_window().is_some()
            || self.windows.capture().is_some()
            || self.publication.is_some()
        {
            return false;
        }
        match self.failure.as_ref() {
            Some(FailureCause::Channel(ChannelFault::Unanswered)) => self.received.is_empty(),
            Some(FailureCause::WorkerOutcome) => {
                self.windows.matches_leave(true)
                    && matches!(
                        self.received.as_slice(),
                        [ReceivedCapture {
                            envelope: CaptureEnvelope {
                                payload: CapturePayload::Refusal(
                                    weaver_types::state_flow::capture::CaptureRefusal::Lifecycle(
                                        weaver_types::LifecycleRefusal::OutOfOrder
                                    )
                                ),
                                ..
                            },
                            ..
                        }]
                    )
            }
            _ => false,
        }
    }

    /// A standing-run refusal grants no escalation. After capture, require its
    /// cleanup acknowledgment and the matching final graceful refusal first.
    fn standing_refusal(&self) -> bool {
        if self.refused.is_some() {
            return self.refused_leave_standing();
        }
        self.windows.matches_leave(false)
            && self.windows.capture().is_none()
            && self.publication.is_none()
            && matches!(self.failure, Some(FailureCause::WorkerOutcome))
            && matches!(self.received.as_slice(), [ReceivedCapture {
                envelope: CaptureEnvelope { payload: CapturePayload::Refusal(
                    weaver_types::state_flow::capture::CaptureRefusal::Lifecycle(refusal)
                ), .. }, ..
            }] if crate::standing_leave_refusal(refusal))
    }

    fn verb(&self) -> &'static str {
        crate::verb_of(self.windows.matches_leave(true))
    }

    pub(crate) fn advance(&mut self) -> Progress {
        if self.progress() != Progress::Pending {
            return self.progress();
        }
        if let Err(mut cause) = self.step() {
            let local_preparation_failed = !self.committed
                && self.refused.is_none()
                && self.failure.is_none()
                && self.checkpoint_failure.is_none()
                && matches!(&cause, FailureCause::Publication(_))
                && self.windows.capture().is_some();
            if let FailureCause::Publication(error) = &cause {
                self.publication_failure = Some((**error).clone());
            }
            // A checkpoint can receive the acknowledgement while hashing or
            // preparing. Publication retains its interrupted work; this owner
            // validates the packet and continues the original Leave instead.
            if self.unexpected.as_ref().is_some_and(|received| {
                matches!(
                    received.envelope.payload,
                    CapturePayload::SavePointCaptureRefused(_)
                )
            }) {
                match self.accept_checkpoint_refusal() {
                    Ok(()) => return self.progress(),
                    Err(error) => {
                        self.checkpoint_failure = None;
                        cause = error;
                    }
                }
            }
            let mut cause = self.checkpoint_failure.take().unwrap_or(cause);
            if matches!(cause, FailureCause::Channel(ChannelFault::Unanswered)) {
                match self.begin_expired_refusal() {
                    Ok(true) => return self.progress(),
                    Ok(false) => (),
                    Err(error) => cause = error,
                }
            }
            if let Some(publication) = &mut self.publication {
                self.publication_failure = Some(publication.failed(format!("{cause:?}")));
            }
            if self.failure.is_none() {
                self.failure = Some(cause);
            } else {
                self.cleanup_failure = Some(cause);
            }
            if local_preparation_failed && self.refused.is_none() {
                if self.windows.demand_window().is_none()
                    && let Err(error) = self.windows.begin_refusal()
                {
                    self.cleanup_failure = Some(FailureCause::Channel(error));
                    self.stage = Stage::Stopped;
                } else {
                    self.prepare_abort(
                        "publication_failed",
                        "admin could not prepare the capture for publication",
                    );
                }
            } else {
                self.stage = Stage::Stopped;
            }
        }
        self.progress()
    }

    fn send(&mut self, payload: CapturePayload, next: Stage) -> Result<(), FailureCause> {
        let mut sent = false;
        let result = self
            .channel
            .send_capture_tracked(
                &CaptureEnvelope {
                    exchange: self.directive.exchange().clone(),
                    position: Position::Continue,
                    payload,
                },
                &self.windows,
                &mut sent,
            )
            .map_err(FailureCause::Channel);
        if sent {
            // Physical carry determines which frame can cross cancellation,
            // even when the post-send deadline check withdraws success.
            self.stage = next;
        }
        result
    }

    fn receive(&mut self) -> Result<bool, FailureCause> {
        self.receive_correlated()?;
        if let CapturePayload::SavePointCaptureRefused(refused) =
            &self.received.last().unwrap().envelope.payload
        {
            self.accept_capture_refused(refused.clone())?;
            return Ok(false);
        }
        if negative(
            &self.received.last().unwrap().envelope,
            &self.windows,
            self.committed,
        ) {
            return Err(FailureCause::WorkerOutcome);
        }
        Ok(true)
    }

    fn receive_correlated(&mut self) -> Result<(), FailureCause> {
        let read = self
            .channel
            .recv_capture_retained(&self.windows)
            .map_err(FailureCause::Channel)?;
        self.received.push(read.received);
        let envelope = &self.received.last().unwrap().envelope;
        if &envelope.exchange != self.directive.exchange()
            || !same_run(envelope, &self.observed.facts.run)
            || self
                .windows
                .capture()
                .is_some_and(|beginning| !same_capture(envelope, beginning.capture_id))
        {
            return Err(FailureCause::Protocol("foreign capture continuation"));
        }
        if let Err(error) = read.timing {
            self.unexpected = self.received.pop();
            return Err(FailureCause::Channel(error));
        }
        Ok(())
    }

    fn step(&mut self) -> Result<(), FailureCause> {
        self.windows.check().map_err(FailureCause::Channel)?;
        match self.stage {
            Stage::Aborting => {
                match self.channel.send_capture_attempt(
                    self.abort.as_ref().expect("original local abort"),
                    &self.windows,
                    &mut false,
                ) {
                    Ok(()) => (),
                    Err(crate::channel::CaptureSendFailure::PeerClosed)
                        if self.windows.demand_window().is_none()
                            && !self.windows.is_join()
                            && self.windows.capture().is_some()
                            && !self.committed =>
                    {
                        // The worker can complete refusal and close before
                        // this send. Closure grants no delivery or cleanup:
                        // drain only the existing correlated refusal path,
                        // under its unchanged bounds, before judging the run.
                    }
                    Err(failure) => {
                        return Err(FailureCause::Channel(failure.channel_fault()));
                    }
                }
                self.stage = if self.windows.demand_window().is_some() {
                    Stage::AwaitAbortOutcome
                } else {
                    Stage::AwaitCaptureRefused
                };
            }
            Stage::AwaitCaptureRefused => self.await_capture_refused()?,
            Stage::AwaitRefusedLeave => self.await_refused_leave()?,
            Stage::AwaitAbortOutcome => {
                self.receive_correlated()?;
                if !matches!(self.received.last().unwrap().envelope.payload,
                    CapturePayload::Refusal(weaver_types::state_flow::capture::CaptureRefusal::Save(
                        weaver_types::state_flow::capture::SaveRefusal::SavePointNotTaken { missed }
                    )) if missed != weaver_types::state_flow::capture::SavePointLeg::CaptureNotReady)
                {
                    return Err(FailureCause::Protocol(
                        "local abort lacks the worker's capture cleanup outcome",
                    ));
                }
                self.stage = Stage::Stopped;
            }
            Stage::SendRequest => {
                match &self.directive {
                    RequestEnvelope::Leave(request) => {
                        self.channel.send_capture_leave(request, &self.windows)
                    }
                    RequestEnvelope::Demand(request) => {
                        self.channel.send_capture(request, &self.windows)
                    }
                }
                .map_err(FailureCause::Channel)?;
                self.stage = Stage::Beginning;
            }
            Stage::Beginning => {
                if !self.receive()? {
                    return Ok(());
                }
                let CapturePayload::SavePointBeginning(value) =
                    &self.received.last().unwrap().envelope.payload
                else {
                    return Err(FailureCause::Protocol("expected Beginning"));
                };
                self.windows.accept(value).map_err(FailureCause::Channel)?;
                self.stage = Stage::Image;
            }
            Stage::Image => {
                if !self.receive()? {
                    return Ok(());
                }
                if !matches!(
                    self.received.last().unwrap().envelope.payload,
                    CapturePayload::SavePointImage(_)
                ) {
                    return Err(FailureCause::Protocol("expected Image"));
                }
                self.stage = Stage::Hashing;
            }
            Stage::Hashing => {
                let CapturePayload::SavePointImage(image) =
                    &self.received.last().unwrap().envelope.payload
                else {
                    unreachable!("the original Image remains owned")
                };
                let publication = Publication::serving(
                    self.repository.take().expect("one publication owner"),
                    &LoadScope {
                        session: &self.observed.facts.session,
                        trace_path: &self.observed.trace_path,
                    },
                    ServingImage {
                        loaded: &self.observed.source.identity(),
                        run: self.observed.facts.run.clone(),
                        cause: if self.windows.demand_window().is_some() {
                            PersistenceCause::Requested
                        } else if self.windows.matches_leave(true) {
                            PersistenceCause::ForcedUnload
                        } else {
                            PersistenceCause::GracefulUnload
                        },
                        capture_id: image.capture_id,
                        room_name: &image.room_name,
                        size_bytes: image.size_bytes,
                        position: image.position.clone(),
                        room: self.room.as_fd(),
                        member_uid: self.member_uid,
                    },
                    &mut checkpoint(
                        &self.channel,
                        &self.windows,
                        self.directive.exchange(),
                        &self.observed.facts.run,
                        self.committed,
                        &mut self.unexpected,
                        &mut self.checkpoint_failure,
                    ),
                )
                .map_err(|error| FailureCause::Publication(Box::new(error)))?;
                let checked = CaptureImageChecked {
                    capture_id: image.capture_id,
                    dump_sha256: publication.dump_sha256().clone(),
                };
                self.publication = Some(publication);
                self.send(
                    CapturePayload::SavePointImageChecked(checked),
                    Stage::Finished,
                )?;
            }
            Stage::Finished => {
                if !self.receive()? {
                    return Ok(());
                }
                if !matches!(
                    self.received.last().unwrap().envelope.payload,
                    CapturePayload::SavePointImageFinished(_)
                ) {
                    return Err(FailureCause::Protocol("expected ImageFinished"));
                }
                self.stage = Stage::Preparing;
            }
            Stage::Preparing => {
                let CapturePayload::SavePointImageFinished(finished) =
                    &self.received.last().unwrap().envelope.payload
                else {
                    unreachable!("the original Finished remains owned")
                };
                let identity = self
                    .publication
                    .as_mut()
                    .unwrap()
                    .image_finished(
                        &self.observed.facts.run,
                        finished.capture_id,
                        &finished.room_name,
                        &mut checkpoint(
                            &self.channel,
                            &self.windows,
                            self.directive.exchange(),
                            &self.observed.facts.run,
                            self.committed,
                            &mut self.unexpected,
                            &mut self.checkpoint_failure,
                        ),
                    )
                    .map_err(|error| FailureCause::Publication(Box::new(error)))?;
                let prepared = CaptureArtifact {
                    capture_id: finished.capture_id,
                    artifact_id: identity.artifact_id,
                    dump_sha256: identity.dump_sha256,
                    config_sha256: identity.config_sha256,
                };
                self.prepared = Some(prepared.clone());
                self.send(CapturePayload::SavePointPrepared(prepared), Stage::Commit)?;
            }
            Stage::Commit => {
                if !self.receive()? {
                    return Ok(());
                }
                let CapturePayload::SavePointCommit(value) =
                    &self.received.last().unwrap().envelope.payload
                else {
                    return Err(FailureCause::Protocol("expected Commit"));
                };
                self.committed = true;
                self.publication
                    .as_mut()
                    .unwrap()
                    .commit(
                        &self.observed.facts.run,
                        value.capture_id,
                        &mut checkpoint(
                            &self.channel,
                            &self.windows,
                            self.directive.exchange(),
                            &self.observed.facts.run,
                            self.committed,
                            &mut self.unexpected,
                            &mut self.checkpoint_failure,
                        ),
                    )
                    .map_err(|error| FailureCause::Publication(Box::new(error)))?;
                self.stage = Stage::Publishing;
            }
            Stage::Publishing => {
                let progress = self
                    .publication
                    .as_mut()
                    .unwrap()
                    .advance(&mut checkpoint(
                        &self.channel,
                        &self.windows,
                        self.directive.exchange(),
                        &self.observed.facts.run,
                        self.committed,
                        &mut self.unexpected,
                        &mut self.checkpoint_failure,
                    ))
                    .map_err(|error| FailureCause::Publication(Box::new(error)))?;
                match progress {
                    publication::Progress::Renamed | publication::Progress::Verified => (),
                    publication::Progress::Published(identity) => {
                        self.published = Some(identity);
                        self.send(
                            CapturePayload::SavePointPublished(
                                self.prepared.as_ref().unwrap().clone(),
                            ),
                            Stage::Complete,
                        )?;
                    }
                    publication::Progress::Complete(_) => {
                        return Err(FailureCause::Protocol(
                            "Serving publication cannot finish alone",
                        ));
                    }
                }
            }
            Stage::Complete => {
                if !self.receive()? {
                    return Ok(());
                }
                if self.windows.demand_window().is_some() {
                    let CapturePayload::Answer(CaptureAnswer::SavePointTaken { report }) =
                        &self.received.last().unwrap().envelope.payload
                    else {
                        return Err(FailureCause::Protocol("expected SavePointTaken"));
                    };
                    if report.event_run != self.observed.facts.run
                        || Some(&report.start) != self.published.as_ref()
                    {
                        return Err(FailureCause::Protocol(
                            "SavePointTaken differs from the indexed capture",
                        ));
                    }
                    self.report = Some(report.clone());
                    self.stage = Stage::TakenReceived;
                    return Ok(());
                }
                let CapturePayload::SavePointCaptureComplete(value) =
                    &self.received.last().unwrap().envelope.payload
                else {
                    return Err(FailureCause::Protocol("expected CaptureComplete"));
                };
                self.windows
                    .complete(value.capture_id)
                    .map_err(FailureCause::Channel)?;
                self.stage = Stage::Left;
            }
            Stage::Left => {
                if !self.receive()? {
                    return Ok(());
                }
                let CapturePayload::Answer(CaptureAnswer::Left {
                    save_point: Some(report),
                    forced,
                    no_state: false,
                }) = &self.received.last().unwrap().envelope.payload
                else {
                    return Err(FailureCause::Protocol(
                        "expected the captured Leave outcome",
                    ));
                };
                if self.windows.matches_leave(true) && !forced {
                    return Err(FailureCause::Protocol("forced Leave answered as graceful"));
                }
                if report.event_run != self.observed.facts.run
                    || Some(&report.start) != self.published.as_ref()
                {
                    return Err(FailureCause::Protocol(
                        "Left differs from the published capture",
                    ));
                }
                self.report = Some(report.clone());
                // Keep the guard. The containing lifecycle owner still owes
                // real run end and the marker/conclusion before command success.
                self.stage = Stage::LeftReceived;
            }
            Stage::LeftReceived | Stage::TakenReceived | Stage::Stopped => {
                unreachable!("terminal actor does no work")
            }
        }
        Ok(())
    }
}

fn checkpoint<'a>(
    channel: &'a Coordination,
    windows: &'a CaptureWindows,
    exchange: &'a ExchangeId,
    run: &'a weaver_types::RunId,
    committed: bool,
    unexpected: &'a mut Option<ReceivedCapture>,
    failure: &'a mut Option<FailureCause>,
) -> impl FnMut(PersistencePhase) -> Result<(), String> + 'a {
    move |_| match channel.try_recv_capture_retained(windows) {
        Ok(None) => Ok(()),
        Ok(Some(read)) => {
            let received = read.received;
            let correlated = &received.envelope.exchange == exchange
                && same_run(&received.envelope, run)
                && windows
                    .capture()
                    .is_none_or(|beginning| same_capture(&received.envelope, beginning.capture_id));
            let cause = if !correlated {
                FailureCause::Protocol("packet during publication work")
            } else if let Err(error) = read.timing {
                FailureCause::Channel(error)
            } else if negative(&received.envelope, windows, committed) {
                FailureCause::WorkerOutcome
            } else {
                FailureCause::Protocol("packet during publication work")
            };
            *unexpected = Some(received);
            *failure = Some(cause);
            Err("packet during publication work".into())
        }
        Err(error) => {
            let detail = format!("publication transport: {error:?}");
            *failure = Some(FailureCause::Channel(error));
            Err(detail)
        }
    }
}

fn negative(envelope: &CaptureEnvelope, windows: &CaptureWindows, committed: bool) -> bool {
    use weaver_types::state_flow::capture::{CaptureRefusal, SaveRefusal};
    match &envelope.payload {
        CapturePayload::Refusal(refusal) if crate::terminal_unwind_refusal(refusal) => true,
        CapturePayload::Refusal(CaptureRefusal::Lifecycle(refusal)) => {
            !committed && windows.capture().is_none() && crate::standing_leave_refusal(refusal)
        }
        CapturePayload::Refusal(CaptureRefusal::Save(SaveRefusal::SavePointNotTaken {
            missed,
        })) => {
            !committed
                && windows.demand_window().is_some()
                && ((*missed == SavePointLeg::CaptureNotReady) == windows.capture().is_none())
        }
        CapturePayload::SavePointAborted(_) => !committed,
        _ => false,
    }
}

fn same_capture(
    envelope: &CaptureEnvelope,
    capture_id: weaver_types::state_flow::DecimalU64,
) -> bool {
    use weaver_types::state_flow::capture::{CaptureRefusal, SaveRefusal};
    match &envelope.payload {
        CapturePayload::Refusal(CaptureRefusal::Save(SaveRefusal::FailedRun {
            capture_id: actual,
            ..
        })) => *actual == Some(capture_id),
        _ => envelope.capture_id().is_none_or(|id| id == capture_id),
    }
}

pub(super) fn same_run(envelope: &CaptureEnvelope, run: &weaver_types::RunId) -> bool {
    match &envelope.payload {
        CapturePayload::Refusal(weaver_types::state_flow::capture::CaptureRefusal::Save(
            weaver_types::state_flow::capture::SaveRefusal::FailedRun { run: actual, .. },
        )) => actual == run,
        _ => true,
    }
}

pub(crate) fn project_worker_refusal(
    refusal: &weaver_types::state_flow::capture::CaptureRefusal,
) -> weaver_types::state_flow::capture::CaptureRefusal {
    use weaver_types::LifecycleRefusal;
    use weaver_types::state_flow::capture::{CaptureRefusal, SaveRefusal};
    if crate::terminal_unwind_refusal(refusal) {
        return match refusal {
            CaptureRefusal::Save(SaveRefusal::FailedRun { .. }) => {
                CaptureRefusal::Lifecycle(LifecycleRefusal::Unanswered)
            }
            _ => refusal.clone(),
        };
    }
    // The owner admits these only at initial admission or after demand/graceful
    // capture cleanup. They do not become failed-unwind codes by exclusion.
    if matches!(refusal, CaptureRefusal::Lifecycle(value) if crate::standing_leave_refusal(value))
        || matches!(
            refusal,
            CaptureRefusal::Save(SaveRefusal::SavePointNotTaken { .. })
        )
    {
        return refusal.clone();
    }
    CaptureRefusal::Lifecycle(LifecycleRefusal::Malformed)
}
