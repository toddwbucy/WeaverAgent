//! Explicit flag observations keep the reader through the final Recorded sample.
//! This port has no content, placement, plan-mutation or progression grant.
use super::*;
use crate::flow::author::AuthoredSource;
use crate::flow::mirror;
use weaver_types::state_flow::{
    PlanFlagObservation, PlanFlagSnapshot, PlanMeasureEvent, PlanMeasurement, PlanRef,
    ReadRefusalCode, SourceBlockId,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::flow) enum FlagObservation {
    Measured(PlanFlagSnapshot),
    Unavailable(ReadRefusalCode),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum FlagsBeginRefusal {
    WrongPhase,
    Busy,
    InvalidAttribution,
}

#[derive(Debug)]
pub(in crate::flow) struct FlagsBeginError {
    pub request: ReadSelector,
    pub refusal: FlagsBeginRefusal,
    pub failure: Option<RunFailure>,
}

#[derive(Debug, Default)]
pub(super) struct FlagRead {
    pub(super) request_id: Option<DecimalU64>,
    observation: Option<FlagObservation>,
    pub(super) event: Option<AuthoredEvent>,
    pub(super) ticket: Option<u64>,
    sequence: Option<Sequence>,
    returned: Option<AuthoredEvent>,
}

/// Only the live owner transfers this after its exact measurement is Recorded.
/// A cancelled account retains evidence but exposes no successful sample.
#[derive(Debug)]
pub(in crate::flow) struct RecordedFlags {
    account: RetainedReadCompletion,
    observation: FlagObservation,
    sequence: Sequence,
    request_id: DecimalU64,
    origin: std::sync::Arc<()>,
}
impl RecordedFlags {
    pub(in crate::flow) fn observation(&self) -> Option<&FlagObservation> {
        (!matches!(self.account.completion, ReadCompletion::Cancelled(_)))
            .then_some(&self.observation)
    }
    pub(in crate::flow) fn sequence(&self) -> Sequence {
        self.sequence
    }
    pub(in crate::flow) fn request_id(&self) -> DecimalU64 {
        self.request_id
    }
    pub(in crate::flow) fn belongs_to(&self, context: &RecordingContext) -> bool {
        std::sync::Arc::ptr_eq(&self.origin, &context.read_origin)
    }
}

impl RecordingContext {
    #[allow(clippy::result_large_err)] // Busy returns the exact requested identities.
    pub(in crate::flow) fn observe_flags(
        &mut self,
        plan: PlanRef,
        after: SourceBlockId,
        io: &mut impl PumpIo,
    ) -> Result<(), FlagsBeginError> {
        let request = ReadSelector::PlanFlags { plan, after };
        if let Err(failure) = self.live(io) {
            return Err(FlagsBeginError {
                request,
                refusal: FlagsBeginRefusal::WrongPhase,
                failure: Some(failure),
            });
        }
        if !self.flag_selector_matches_loaded_seed(&request) {
            return Err(FlagsBeginError {
                request,
                refusal: FlagsBeginRefusal::InvalidAttribution,
                failure: None,
            });
        }
        self.begin_read_with_purpose(
            request,
            None,
            ReadPurpose::PlanFlags(FlagRead::default()),
            io,
        )
        .map_err(|error| FlagsBeginError {
            request: error.selector,
            refusal: match error.refusal {
                ReadBeginRefusal::WrongPhase => FlagsBeginRefusal::WrongPhase,
                ReadBeginRefusal::Busy => FlagsBeginRefusal::Busy,
            },
            failure: error.failure,
        })
    }

    pub(super) fn flag_selector_matches_loaded_seed(&self, selector: &ReadSelector) -> bool {
        let ReadSelector::PlanFlags { plan, after } = selector else {
            return false;
        };
        self.open_start
            .as_ref()
            .is_some_and(|start| start.seed_id == plan.plan_id.seed_id)
            && plan.plan_id.local_id.0 > 0
            && plan.revision.0 > 0
            && !after.run.is_empty()
            && after.sequence.0 > 0
    }

    pub(in crate::flow) fn flags_pending(&self) -> bool {
        self.flags().is_some()
    }

    fn flags(&self) -> Option<&FlagRead> {
        match &self.read.as_ref()?.purpose {
            ReadPurpose::PlanFlags(flags) => Some(flags),
            _ => None,
        }
    }
    fn flags_mut(&mut self) -> Option<&mut FlagRead> {
        match &mut self.read.as_mut()?.purpose {
            ReadPurpose::PlanFlags(flags) => Some(flags),
            _ => None,
        }
    }

    pub(in crate::flow) fn take_observed_flags(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<Option<RecordedFlags>, RunFailure> {
        self.live(io)?;
        let Some(read) = &self.read else {
            return Ok(None);
        };
        let ReadPurpose::PlanFlags(flags) = &read.purpose else {
            return Err(RunFailure::ReadOwned);
        };
        if !read.complete {
            return Ok(None);
        }
        let (Some(sequence), Some(request_id), Some(observation)) =
            (flags.sequence, flags.request_id, flags.observation.clone())
        else {
            return Err(
                self.read_fault(io, "flag observation completed without its Recorded sample")
            );
        };
        let account = self
            .take_read_retained_owned(io, true)?
            .expect("checked completed flag read");
        Ok(Some(RecordedFlags {
            account,
            observation,
            sequence,
            request_id,
            origin: self.read_origin.clone(),
        }))
    }

    pub(super) fn advance_read_observation<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<ReadProgress, RunFailure> {
        let read = self.read.as_ref().expect("owned read observation");
        if matches!(read.purpose, ReadPurpose::PlanFlags(_))
            && (read.recorded.is_some() || matches!(read.released, Some(Err(_))))
        {
            self.advance_flag_measurement(ports)
        } else {
            self.advance_read_recall(ports)
        }
    }

    fn prepare_flag_measurement(&mut self, io: &mut impl PumpIo) -> Result<(), RunFailure> {
        if self.flags().is_some_and(|flags| flags.event.is_some()) {
            return Ok(());
        }
        let request_id = self
            .flags()
            .and_then(|flags| flags.request_id)
            .ok_or_else(|| {
                self.read_fault(io, "flag observation lost its original read request")
            })?;
        let read = self.read.as_ref().unwrap();
        let ReadSelector::PlanFlags { plan, after } = read.pages.selector() else {
            unreachable!()
        };
        let (observation, parent) = match &read.released {
            Some(Ok(completed)) => {
                let QueryResult::PlanFlags { snapshot } = completed.result() else {
                    return Err(self.read_fault(io, "flag observation has another read result"));
                };
                if snapshot.plan != *plan || snapshot.after != *after || !snapshot.counts_agree() {
                    return Err(
                        self.read_fault(io, "flag observation changed its identity or counts")
                    );
                }
                let Some(parent) = read.recorded else {
                    return Err(RunFailure::ReadOwned);
                };
                (FlagObservation::Measured(snapshot.clone()), parent)
            }
            Some(Err(ReadFailure::Refused { code, .. })) => {
                let Some(proof) = &read.refused else {
                    return Err(RunFailure::ReadOwned);
                };
                // The generic reader permits this only after exact cursor release.
                (
                    FlagObservation::Unavailable(*code),
                    proof.refused.measurement_sequence,
                )
            }
            _ => return Err(RunFailure::ReadOwned),
        };
        let payload = mirror::plan_measurement(&PlanMeasurement {
            instrument_version: "plan-observation/1".into(),
            plan: plan.clone(),
            event: PlanMeasureEvent::Flags {
                after: after.clone(),
                request_id,
                observation: match &observation {
                    FlagObservation::Measured(snapshot) => PlanFlagObservation::Measured {
                        snapshot: snapshot.clone(),
                    },
                    FlagObservation::Unavailable(code) => {
                        PlanFlagObservation::Unavailable { code: *code }
                    }
                },
            },
        });
        let flags = self.flags_mut().unwrap();
        flags.observation = Some(observation);
        flags.event = Some(AuthoredEvent {
            kind: Kind::StatePlanMeasurement,
            subsystem: weaver_trace::Subsystem::Harness,
            turn: None,
            source: AuthoredSource::Loop,
            causal_parent: Some(parent),
            payload: Some(Payload::PlanMeasurement(Box::new(payload))),
        });
        Ok(())
    }

    fn advance_flag_measurement<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<ReadProgress, RunFailure> {
        self.live(ports.io)?;
        self.read_wait_matches(ports.io)?;
        if self.flags().is_some_and(|flags| flags.sequence.is_some()) {
            return self.finish_recorded_read_observation(ports.io, true);
        }
        if self.flow.has_pending_record()
            || self.waiting_content.is_some()
            || self.completed_author.is_some()
        {
            return Err(self.read_fault(ports.io, "flag measurement has an unsettled predecessor"));
        }
        self.prepare_flag_measurement(ports.io)?;
        let flags = self.flags().unwrap();
        let progress = if let Some(ticket) = flags.ticket {
            if self.barrier.pending_author().map(|held| held.ticket) != Some(ticket) {
                return Err(
                    self.read_fault(ports.io, "flag measurement lost its original author ticket")
                );
            }
            self.barrier
                .resume_with_clock(&mut self.inbox, &mut self.budget, &mut || ports.io.now())
        } else {
            let event = flags.event.as_ref().unwrap().clone();
            let wall = wall_ms().map_err(|detail| {
                self.fail(
                    ports.io,
                    FlowHaltReason::TraceFailed,
                    HaltPoint::Author,
                    detail,
                )
            })?;
            self.barrier.submit_authored_with_clock(
                event,
                RecordPurpose::Observation,
                &self.author,
                wall,
                &mut self.inbox,
                &mut self.budget,
                &mut || ports.io.now(),
            )
        };
        match progress {
            RecordProgress::Recorded {
                sequence,
                proof: None,
            } => {
                self.flags_mut().unwrap().sequence = Some(sequence);
                self.flags_mut().unwrap().ticket = None;
                self.finish_recorded_read_observation(ports.io, true)
            }
            RecordProgress::Pending => {
                let ticket = self.barrier.pending_author().map(|held| held.ticket);
                if ticket.is_none() {
                    return Err(self.read_fault(ports.io, "pending flag measurement has no ticket"));
                }
                self.flags_mut().unwrap().ticket = ticket;
                self.live(ports.io)?;
                Ok(ReadProgress::Pending)
            }
            RecordProgress::Failed => {
                let sequence = self.barrier.admitted_before_failure();
                let ticket = self.barrier.pending_author().map(|held| held.ticket);
                let flags = self.flags_mut().unwrap();
                flags.sequence = sequence;
                flags.ticket = ticket;
                Err(self.record_failed(ports.io))
            }
            RecordProgress::BusyAuthored { event, .. } => {
                self.flags_mut().unwrap().returned = Some(event);
                Err(self.record_failed(ports.io))
            }
            RecordProgress::Busy { .. } | RecordProgress::Recorded { proof: Some(_), .. } => {
                Err(self.read_fault(
                    ports.io,
                    "flag observation received another author's completion",
                ))
            }
        }
    }
}
