//! Outbound decisions retain the effective emission through their exact record.
//! Filing custody also observes selected runtime template checks and records an
//! owed template fault before returning refusal. No pair or decoder port is used.

use super::receipt_driver::FilingOwner;
use super::*;
use crate::flow::author::AuthoredSource;
use crate::flow::ledger::SourceDisposition;
use crate::flow::sort::{CheckedOutbound, OutboundInput, OutboundRefusal, identity_sort};
use crate::flow::templates::{
    ExpansionIdentity, ExpansionRefusal, TemplateCertificate, expand_outbound,
};
use weaver_trace::serving::payloads as trace;
use weaver_types::{FaultCase, FaultReport};

#[derive(Debug)]
pub(in crate::flow) struct OutboundBeginError {
    pub input: OutboundInput,
    pub invalid_candidate: Option<OutboundRefusal>,
    pub template: Option<ExpansionRefusal>,
    pub refusal: Option<PairBeginRefusal>,
    pub failure: Option<RunFailure>,
}
impl OutboundBeginError {
    /// Invalid expansion evidence stays owed to the caller, just as at repair
    /// entry. Consuming this error transfers the exact input and refusal author.
    pub(in crate::flow) fn into_parts(self) -> (OutboundInput, Option<AuthoredEvent>) {
        let event = self.template.map(|refusal| {
            let record = weaver_types::RefusalRecord::Instruction {
                template_id: "sort.outbound".into(),
                case: weaver_types::state_flow::InstructionRefusal::InvalidTemplate,
                detail: format!("outbound template expansion: {refusal:?}"),
            };
            AuthoredEvent {
                kind: Kind::Refusal,
                subsystem: weaver_trace::Subsystem::Harness,
                turn: Some(self.input.effective.account().turn().clone()),
                source: AuthoredSource::Runtime,
                causal_parent: Some(Sequence(self.input.source.block_id.sequence.0)),
                payload: Some(Payload::Refusal(
                    serde_json::value::to_raw_value(&record).expect("typed refusal serializes"),
                )),
            }
        });
        (self.input, event)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum OutboundProgress {
    Pending,
    Complete,
}

/// Only this successful result makes the immutable ordered acts available.
#[derive(Debug)]
pub(in crate::flow) struct SortedOutbound {
    checked: CheckedOutbound,
    record: FilingCompletion,
}
impl SortedOutbound {
    pub(in crate::flow) fn acts(&self) -> &[weaver_types::state_flow::SortAct] {
        &self.checked.input().acts
    }
    pub(in crate::flow) fn into_parts(self) -> (OutboundInput, FilingCompletion) {
        (self.checked.into_input(), self.record)
    }
}

#[derive(Debug)]
pub(in crate::flow) enum OutboundCompletion {
    Sorted(SortedOutbound),
    Refused {
        input: OutboundInput,
        record: FilingCompletion,
        fault: Sequence,
    },
    Cancelled {
        input: OutboundInput,
        record: FilingCompletion,
        fault: Option<Sequence>,
    },
}

pub(super) struct OutboundWork {
    checked: CheckedOutbound,
    completion: Option<FilingCompletion>,
    cancelled: bool,
    fault_started: bool,
    fault_source: Option<Sequence>,
}
impl OutboundWork {
    fn rejected(&self) -> bool {
        self.completion.as_ref().is_some_and(|completion| {
            completion
                .receipts()
                .dispositions()
                .any(|outcome| matches!(outcome, SourceDisposition::CorrectlyRejected(_)))
        })
    }
    fn complete(&self) -> bool {
        self.completion.is_some() && (!self.rejected() || self.fault_source.is_some())
    }
}

impl RecordingContext {
    #[allow(clippy::result_large_err)]
    pub(in crate::flow) fn begin_outbound(
        &mut self,
        input: OutboundInput,
        certificate: &TemplateCertificate,
        io: &mut impl PumpIo,
    ) -> Result<(), OutboundBeginError> {
        let error = |input, invalid_candidate, template, refusal, failure| OutboundBeginError {
            input,
            invalid_candidate,
            template,
            refusal,
            failure,
        };
        if let Err(failure) = self.live(io) {
            return Err(error(input, None, None, None, Some(failure)));
        }
        if let Some(refusal) = self.pair_begin_refusal() {
            return Err(error(input, None, None, Some(refusal), None));
        }
        if !self.repair_pair_custody_matches(input.effective.account()) {
            return Err(error(
                input,
                Some(OutboundRefusal::InvalidCandidate),
                None,
                None,
                None,
            ));
        }
        let checked = identity_sort(input)
            .map_err(|(input, refusal)| error(input, Some(refusal), None, None, None))?;
        if !self.repair_certificate_matches(certificate) {
            return Err(error(
                checked.into_input(),
                None,
                Some(ExpansionRefusal::ForeignCertificate),
                None,
                None,
            ));
        }
        let load = self
            .load_duty
            .as_ref()
            .expect("entered run has a recorded load");
        let expansion = expand_outbound(
            certificate,
            ExpansionIdentity {
                run_id: self.flow.state().identity().1,
                scheme: &self.declaration.scheme,
                checker_artifact_sha256: certificate.checker_artifact_sha256(),
                inventory_sha256: certificate.inventory_sha256(),
            },
            &load.scheme_text,
        );
        // The expected inventory is the recorded load, never the supplied certificate.
        let expansion = match expansion {
            Ok(value)
                if value.inventory_sha256().as_str()
                    == load.template_inventory.inventory_sha256 =>
            {
                value
            }
            Ok(_) => {
                return Err(error(
                    checked.into_input(),
                    None,
                    Some(ExpansionRefusal::ForeignCertificate),
                    None,
                    None,
                ));
            }
            Err(refusal) => {
                return Err(error(checked.into_input(), None, Some(refusal), None, None));
            }
        };
        let input = checked.input();
        let event = AuthoredEvent {
            kind: Kind::SortDecision,
            subsystem: weaver_trace::Subsystem::Harness,
            turn: Some(input.effective.account().turn().clone()),
            source: AuthoredSource::Loop,
            causal_parent: Some(Sequence(input.source.block_id.sequence.0)),
            payload: Some(Payload::SortDecision(Box::new(trace::SortDecisionRecord {
                template_id: expansion.template_id().into(),
                message: serde_json::value::to_raw_value(expansion.message())
                    .expect("Message serializes"),
                decision: crate::flow::mirror::outbound(&input.source, &input.acts),
            }))),
        };
        if let Err(failure) =
            self.begin_owned_filing(event, PumpDeadline::Decision, FilingOwner::Outbound, io)
        {
            return Err(error(
                checked.into_input(),
                None,
                None,
                Some(PairBeginRefusal::Busy),
                failure.failure,
            ));
        }
        self.filing.as_mut().unwrap().outbound = Some(OutboundWork {
            checked,
            completion: None,
            cancelled: false,
            fault_started: false,
            fault_source: None,
        });
        Ok(())
    }

    pub(in crate::flow) fn advance_outbound<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<OutboundProgress, RunFailure> {
        self.live(ports.io)?;
        if self.outbound_work().is_none() {
            return Err(RunFailure::PairOwned);
        }
        if self.outbound_work().unwrap().completion.is_none() {
            if self.advance_owned_filing(FilingOwner::Outbound, ports)? != FilingProgress::Complete
            {
                return Ok(OutboundProgress::Pending);
            }
            let Some(completion) = self.collect_owned_filing(FilingOwner::Outbound, ports.io)?
            else {
                return Ok(OutboundProgress::Pending);
            };
            self.outbound_work_mut().unwrap().completion = Some(completion);
        }
        let work = self.outbound_work().unwrap();
        if work.complete() {
            // A lifecycle wait can interrupt after the Recorded hook, leaving
            // this owner's completed author to consume on its next advance.
            if self.completed_author.is_some() {
                if self.completed_author != work.fault_source {
                    return Err(self.fail(
                        ports.io,
                        FlowHaltReason::ProtocolFailed,
                        HaltPoint::Author,
                        "outbound retained another author's completion",
                    ));
                }
                self.completed_author = None;
            }
            return Ok(OutboundProgress::Complete);
        }
        if !work.fault_started {
            let completion = work.completion.as_ref().unwrap();
            let rejected = completion
                .receipts()
                .dispositions()
                .find_map(|outcome| match outcome {
                    SourceDisposition::CorrectlyRejected(value) => Some(value),
                    SourceDisposition::Committed(_) => None,
                })
                .expect("incomplete outbound has a runtime rejection");
            let fault = FaultReport {
                case: FaultCase::StateTemplateRejected,
                account: serde_json::value::to_raw_value(&serde_json::json!({
                    "template_id": "sort.outbound", "source_id": rejected.source(), "result": rejected.reply(),
                })).expect("typed rejection serializes"),
            };
            self.waiting_content = Some(AuthoredEvent {
                kind: Kind::Fault,
                subsystem: weaver_trace::Subsystem::Harness,
                turn: Some(work.checked.input().effective.account().turn().clone()),
                source: AuthoredSource::Runtime,
                causal_parent: Some(completion.sequence()),
                payload: Some(Payload::Fault(
                    serde_json::value::to_raw_value(&fault).expect("typed fault serializes"),
                )),
            });
            self.outbound_work_mut().unwrap().fault_started = true;
        }
        if let Some(sequence) = self.completed_author.take() {
            debug_assert_eq!(self.outbound_work().unwrap().fault_source, Some(sequence));
        } else if self.waiting_content.is_some() {
            self.continue_waiting_content(ports)?;
        } else {
            let progress =
                self.barrier
                    .resume_with_clock(&mut self.inbox, &mut self.budget, &mut || ports.io.now());
            self.wait_recorded(ports, progress)?;
        }
        self.live(ports.io)?;
        Ok(OutboundProgress::Complete)
    }

    pub(super) fn bind_outbound_fault(&mut self, sequence: Sequence) -> bool {
        let Some(work) = self.outbound_work_mut() else {
            return false;
        };
        if !work.fault_started || work.fault_source.is_some() {
            return false;
        }
        work.fault_source = Some(sequence);
        true
    }

    pub(in crate::flow) fn take_outbound(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<Option<OutboundCompletion>, RunFailure> {
        self.live(io)?;
        if !self.outbound_work().is_some_and(OutboundWork::complete)
            || self.completed_author.is_some()
        {
            return Ok(None);
        }
        let work = self.filing.take().unwrap().outbound.unwrap();
        let record = work.completion.unwrap();
        let result = if work.cancelled {
            OutboundCompletion::Cancelled {
                input: work.checked.into_input(),
                record,
                fault: work.fault_source,
            }
        } else if let Some(fault) = work.fault_source {
            OutboundCompletion::Refused {
                input: work.checked.into_input(),
                record,
                fault,
            }
        } else {
            OutboundCompletion::Sorted(SortedOutbound {
                checked: work.checked,
                record,
            })
        };
        Ok(Some(result))
    }

    /// Finish the same recorded decision and any owed template fault, but never
    /// issue a successful result. The caller retains its original bound.
    pub(in crate::flow) fn cancel_outbound(&mut self) {
        if let Some(work) = self.outbound_work_mut() {
            work.cancelled = true;
        }
    }
    fn outbound_work(&self) -> Option<&OutboundWork> {
        self.filing.as_ref()?.outbound.as_ref()
    }
    fn outbound_work_mut(&mut self) -> Option<&mut OutboundWork> {
        self.filing.as_mut()?.outbound.as_mut()
    }
}
