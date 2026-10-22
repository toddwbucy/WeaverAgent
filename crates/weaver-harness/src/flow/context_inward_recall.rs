//! Explicit inward execution reuses the pair receipts and ordinary read owner.
//! No gate exchange, active-turn Cut, tool timer or placement is introduced.

use super::*;
use crate::flow::read::ReadFailure;
use weaver_types::ToolOutcome;
use weaver_types::state_flow::{CallId, PairLane, PairRequest, ReadSelector};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum InwardRefusal {
    NotReady,
    OtherLane,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum InwardProgress {
    Pending,
    Terminal,
}

#[derive(Debug)]
pub(super) enum InwardEvidence {
    Read(ReadCompletion),
    EmptySubject,
}

impl InwardEvidence {
    // Called only by the pair owner. Raw encoded result bytes are the return,
    // never a normalized QueryResult serialization or a new statement filing.
    pub(super) fn outcome(&self, subject: &str) -> Option<ToolOutcome> {
        match self {
            Self::EmptySubject if subject.is_empty() => Some(ToolOutcome::Refused {
                reason: "empty_subject".into(),
            }),
            Self::Read(ReadCompletion::Recorded(read)) => {
                if read.selector() != &(ReadSelector::History {
                    noun: subject.to_owned(), verb: None, condition: None,
                }) {
                    return None;
                }
                Some(ToolOutcome::Result {
                    content: std::str::from_utf8(read.raw_bytes()).ok()?.to_owned(),
                })
            }
            Self::Read(ReadCompletion::Refused(ReadFailure::Refused { code, detail })) => {
                // Named field order is part of HARNESS-RECALL, independent of
                // serde_json::Map's sorting or optional preserve_order feature.
                Some(ToolOutcome::Refused {
                    reason: format!("{{\"code\":{},\"detail\":{}}}",
                        serde_json::to_string(code).ok()?, serde_json::to_string(detail).ok()?),
                })
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Starting,
    Reading,
    Terminal,
}

pub(super) struct InwardWork {
    call: CallId,
    subject: String,
    stage: Stage,
    driving: bool,
    // An invalid consumed completion remains owned on the fatal path.
    refused: Option<InwardEvidence>,
}

impl RecordingContext {
    pub(in crate::flow) fn begin_inward_recall(
        &mut self,
        call: &CallId,
        io: &mut impl PumpIo,
    ) -> Result<Result<(), InwardRefusal>, RunFailure> {
        self.live(io)?;
        let Some(ready) = self.next_pair_execution().filter(|ready| &ready.call_id == call) else {
            return Ok(Err(InwardRefusal::NotReady));
        };
        let PairRequest::Recall { subject } = &ready.request else {
            return Ok(Err(InwardRefusal::OtherLane));
        };
        if ready.lane != PairLane::Recall {
            return Ok(Err(InwardRefusal::OtherLane));
        }
        let subject = subject.clone();
        self.pair_wait_matches(io)?;
        self.pair_work.as_mut().unwrap().inward = Some(InwardWork {
            call: call.clone(), subject, stage: Stage::Starting, driving: false, refused: None,
        });
        Ok(Ok(()))
    }

    pub(super) fn inward_blocks_external(&self) -> bool {
        self.pair_work.as_ref().and_then(|work| work.inward.as_ref())
            .is_some_and(|work| !work.driving)
    }

    pub(super) fn inward_read_allowed(&self, call: &CallId) -> bool {
        self.pair_work.as_ref().and_then(|work| work.inward.as_ref())
            .is_some_and(|work| work.driving && work.stage == Stage::Starting && &work.call == call)
    }

    // The read owner has installed its original bound. No yielding check may
    // split this transfer from retiring the completed request-receipt barrier.
    pub(super) fn admit_inward_read(&mut self, call: &CallId, io: &mut impl PumpIo) -> Result<(), RunFailure> {
        self.pair_wait_matches(io)?;
        self.pairs.start_inward(call)
            .map_err(|error| self.pair_fault(io, format!("inward admission: {error}")))?;
        self.deadlines.disarm(PumpDeadline::Decision);
        let work = self.pair_work.as_mut().unwrap();
        work.bound = None;
        work.inward.as_mut().unwrap().stage = Stage::Reading;
        Ok(())
    }

    pub(in crate::flow) fn advance_inward_recall<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<InwardProgress, RunFailure> {
        self.live(ports.io)?;
        self.pair_wait_matches(ports.io)?;
        let Some(work) = self.pair_work.as_mut().and_then(|work| work.inward.as_mut()) else {
            return Err(RunFailure::PairOwned);
        };
        work.driving = true;
        let result = self.drive_inward_recall(ports);
        if let Some(work) = self.pair_work.as_mut().and_then(|work| work.inward.as_mut()) {
            work.driving = false;
        }
        result
    }

    fn drive_inward_recall<I: PumpIo>(&mut self, ports: &mut PumpPorts<'_, I>) -> Result<InwardProgress, RunFailure> {
        let work = self.pair_work.as_ref().unwrap().inward.as_ref().unwrap();
        let call = work.call.clone();
        if work.stage == Stage::Starting {
            // A caller may bar this still-unexecuted request after begin.
            if self.pairs.next_ready().map(|ready| &ready.call_id) != Some(&call) {
                self.pair_work.as_mut().unwrap().inward = None;
                return Ok(InwardProgress::Terminal);
            }
            let selector = match weaver_internal::recall::query(&work.subject) {
                Ok(query) => ReadSelector::History { noun: query.subject().to_owned(), verb: None, condition: None },
                Err(_) => {
                    self.pairs.start_inward(&call)
                        .map_err(|error| self.pair_fault(ports.io, format!("empty inward admission: {error}")))?;
                    return self.finish_inward_recall(call, InwardEvidence::EmptySubject, ports.io);
                }
            };
            let turn = self.pairs.original().expect("active pair batch").generation().original().turn.clone();
            return match self.begin_inward_read(call, selector, turn, ports.io) {
                Ok(()) => Ok(InwardProgress::Pending),
                Err(error) if error.failure.is_none() && error.refusal == ReadBeginRefusal::Busy => {
                    self.reduce_flow(ports.io)?;
                    Ok(InwardProgress::Pending)
                }
                Err(error) => Err(error.failure.unwrap_or_else(|| self.pair_fault(ports.io, "inward read refused its owner"))),
            };
        }
        if work.stage == Stage::Terminal {
            // A terminal is installed before its atomic wait can fail fatally.
            return Err(self.pair_fault(ports.io, "inward terminal lost its completion handoff"));
        }
        if self.advance_read(ports)? != ReadProgress::Complete {
            return Ok(InwardProgress::Pending);
        }
        let Some(completion) = self.take_read(ports.io)? else {
            return Err(self.pair_fault(ports.io, "complete inward read lost its result"));
        };
        self.finish_inward_recall(call, InwardEvidence::Read(completion), ports.io)
    }

    fn finish_inward_recall(&mut self, call: CallId, evidence: InwardEvidence, io: &mut impl PumpIo) -> Result<InwardProgress, RunFailure> {
        // take_read checked live before transfer. No recoverable hook follows
        // moving the exact proof into the terminal and its measured receipt wait.
        if let InwardEvidence::Read(ReadCompletion::Recorded(read)) = &evidence
            && !read.belongs_to(self)
        {
            self.pair_work.as_mut().unwrap().inward.as_mut().unwrap().refused = Some(evidence);
            return Err(self.pair_fault(io, "inward read belongs to another context"));
        }
        if let Err((evidence, error)) = self.pairs.finish_inward(&call, evidence) {
            self.pair_work.as_mut().unwrap().inward.as_mut().unwrap().refused = Some(evidence);
            return Err(self.pair_fault(io, format!("inward terminal: {error}")));
        }
        self.pair_work.as_mut().unwrap().inward.as_mut().unwrap().stage = Stage::Terminal;
        self.arm_pair_terminal_wait(io)?;
        self.pair_work.as_mut().unwrap().inward = None;
        Ok(InwardProgress::Terminal)
    }
}
