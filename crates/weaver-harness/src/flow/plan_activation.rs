//! Activation consumes the actual successor proof and allocates only from the
//! recorded marks catalog. Neither public metadata nor a model supplies a grant.
use super::*;
use crate::flow::author::{AuthoredEvent, AuthoredSource};
use crate::flow::clock::AbsoluteBound;
use crate::flow::context::FilingCompletion;
use crate::flow::ledger::{LedgerFile, SourceDisposition};
use crate::flow::pump::PumpDeadline;
use crate::flow::templates::TemplateExpansion;
use std::sync::Arc;
use weaver_trace::{Sequence, TurnRef};

/// The released STEP-QUERY result, retaining its original read and trace proof.
/// Public reference metadata is an inspection view, not an activation grant.
#[derive(Debug)]
pub struct CheckedStepSuccessor {
    read: Box<RecordedRead>,
}

impl CheckedStepSuccessor {
    #[allow(clippy::result_large_err)]
    pub(in crate::flow) fn from_read(
        context: &RecordingContext,
        read: RecordedRead,
    ) -> Result<Self, (RecordedRead, &'static str)> {
        let valid = read.belongs_to(context)
            && matches!(
                (read.selector(), read.result()),
                (
                    s::ReadSelector::StepSuccessor { plan, previous_step },
                    s::QueryResult::StepSuccessor { reference, instruction }
                ) if &reference.plan == plan
                    && &reference.previous_step == previous_step
                    && instruction.step_query.as_ref() == Some(reference)
            );
        if !valid {
            return Err((read, "successor has no matching recorded step query"));
        }
        Ok(Self {
            read: Box::new(read),
        })
    }

    pub fn reference(&self) -> &s::StepQueryRef {
        let s::QueryResult::StepSuccessor { reference, .. } = self.read.result() else {
            unreachable!("private checked successor")
        };
        reference
    }

    pub(in crate::flow) fn sequence(&self) -> Sequence {
        self.read.recall_sequence()
    }
}

pub(in crate::flow) fn waiting<'a>(
    request: &PlanRequest,
    context: &RecordingContext,
    current: &'a CommittedPlanSummary,
) -> Result<(&'a s::PlanSnapshot, &'a s::PlanStep), PlanCheckFailure> {
    let plan = locked(request, context, current)?;
    let PlanRequest::Activate { successor, .. } = request else {
        return Err(PlanCheckFailure::Protocol("activation has another request"));
    };
    if !successor.read.belongs_to(context) {
        return Err(PlanCheckFailure::Protocol(
            "activation has a foreign successor proof",
        ));
    }
    let reference = successor.reference();
    let step = plan
        .steps
        .iter()
        .filter(|step| step.status == s::StepStatus::Waiting)
        .min_by_key(|step| step.ordinal)
        .ok_or_else(|| {
            refuse(
                s::PlanOperationCase::InvalidTarget,
                "plan has no waiting step",
            )
        })?;
    let predecessor = step.ordinal.0.checked_sub(1).and_then(|ordinal| {
        plan.steps
            .iter()
            .find(|previous| previous.ordinal.0 == ordinal)
    });
    if Some(&step.step_id) != request.step_id()
        || reference.plan != plan.plan
        || reference.step_id != step.step_id
        || reference.instruction != step.instruction
        || step.ordinal.0 == 0
        || plan.steps.iter().any(|other| {
            other.status == s::StepStatus::Active
                || (other.ordinal < step.ordinal && other.status != s::StepStatus::Closed)
        })
        || if step.ordinal.0 == 1 {
            reference.previous_step.is_some()
        } else {
            predecessor.is_none_or(|previous| {
                previous.status != s::StepStatus::Closed
                    || reference.previous_step.as_ref() != Some(&previous.step_id)
            })
        }
    {
        return Err(refuse(
            s::PlanOperationCase::InvalidTarget,
            "successor is not the current first waiting step",
        ));
    }
    Ok((plan, step))
}

pub(in crate::flow) struct PreparedActivation {
    origin: Arc<()>,
    effect: s::PlanEffect,
    parent: Sequence,
    turn: TurnRef,
    bound: AbsoluteBound,
}

pub(in crate::flow) fn prepare(
    request: &PlanRequest,
    context: &RecordingContext,
    current: &CommittedPlanSummary,
    marks: &RecordedRead,
    turn: TurnRef,
    bound: AbsoluteBound,
) -> Result<PreparedActivation, PlanCheckFailure> {
    let (plan, step) = waiting(request, context, current)?;
    let PlanRequest::Activate { successor, .. } = request else {
        unreachable!()
    };
    if !marks.belongs_to(context)
        || marks.selector() != &(s::ReadSelector::Marks {})
        || marks.recall_sequence().0 <= current.read().recall_sequence().0
        || marks.recall_sequence().0 <= successor.sequence().0
        || turn.0.is_empty()
    {
        return Err(PlanCheckFailure::Protocol(
            "activation lost its ordered marks proof or turn",
        ));
    }
    let s::QueryResult::Marks {
        next_span_id,
        current_task,
        open_scopes,
    } = marks.result()
    else {
        return Err(PlanCheckFailure::Protocol(
            "activation has no marks catalog",
        ));
    };
    let seed = &plan.plan.plan_id.seed_id;
    if !matches!(crate::flow::marks::catalog(marks.result(), seed),
        Ok(crate::flow::marks::TaskCatalog::Existing(task)) if task == plan.task)
        || current_task.as_ref() != Some(&plan.task)
    {
        return Err(PlanCheckFailure::Protocol(
            "plan task disagrees with its open marks path",
        ));
    }
    let span = |id| s::SpanRef {
        seed_id: seed.clone(),
        local_id: id,
        revision: 1.into(),
    };
    let mut next = *next_span_id;
    let group_span = match &step.group {
        None => None,
        Some(id) => {
            let group = plan.groups.iter().find(|group| &group.id == id).ok_or(
                PlanCheckFailure::Protocol("activation step group is missing"),
            )?;
            if !group.step_ids.contains(&step.step_id) {
                return Err(PlanCheckFailure::Protocol(
                    "activation step is outside its group",
                ));
            }
            Some(match &group.span {
                Some(existing) => existing.clone(),
                None => {
                    let allocated = span(next);
                    next = next
                        .0
                        .checked_add(1)
                        .ok_or_else(|| {
                            refuse(
                                s::PlanOperationCase::CounterExhausted,
                                "activation step counter exhausted",
                            )
                        })?
                        .into();
                    allocated
                }
            })
        }
    };
    let existing_group = step
        .group
        .as_ref()
        .and_then(|id| plan.groups.iter().find(|group| &group.id == id))
        .and_then(|group| group.span.as_ref());
    let expected_scopes: Vec<_> = std::iter::once((&plan.task.task, s::SpanKind::Task))
        .chain(existing_group.map(|span| (span, s::SpanKind::Subtask)))
        .collect();
    if open_scopes.len() != expected_scopes.len()
        || open_scopes
            .iter()
            .zip(expected_scopes)
            .any(|(actual, (span, kind))| &actual.span != span || actual.kind != kind)
    {
        return Err(PlanCheckFailure::Protocol(
            "activation marks differ from its task/group scopes",
        ));
    }
    Ok(PreparedActivation {
        origin: Arc::clone(marks.origin()),
        effect: s::PlanEffect::Activate {
            expected: plan.plan.clone(),
            step_id: step.step_id.clone(),
            step_span: span(next),
            group_span,
            successor: successor.reference().clone(),
        },
        parent: marks.recall_sequence(),
        turn,
        bound,
    })
}

impl PreparedActivation {
    pub(in crate::flow) fn effect(&self) -> &s::PlanEffect {
        &self.effect
    }

    pub(in crate::flow) fn event(
        &self,
        context: &RecordingContext,
        expansion: &TemplateExpansion,
    ) -> Result<AuthoredEvent, &'static str> {
        if !Arc::ptr_eq(&self.origin, context.enter_origin())
            || expansion.template_id() != "plan.activate"
        {
            return Err("activation lost its context or compiled template family");
        }
        Ok(AuthoredEvent {
            kind: weaver_trace::serving::Kind::PlanEvent,
            subsystem: weaver_trace::Subsystem::Harness,
            turn: Some(self.turn.clone()),
            source: AuthoredSource::Loop,
            causal_parent: Some(self.parent),
            payload: Some(weaver_trace::serving::Payload::PlanEffect(Box::new(
                weaver_trace::serving::payloads::PlanEffectRecord {
                    message: serde_json::value::to_raw_value(expansion.message())
                        .expect("Message serializes"),
                    context: crate::flow::mirror::activation_effect(&self.effect)?,
                    template_id: expansion.template_id().into(),
                },
            ))),
        })
    }

    pub(in crate::flow) fn check_filing(
        &self,
        context: &RecordingContext,
        expansion: &TemplateExpansion,
        filing: &FilingCompletion,
    ) -> Result<bool, &'static str> {
        if !Arc::ptr_eq(&self.origin, context.enter_origin())
            || !filing.belongs_to(context)
            || filing.wait_key() != PumpDeadline::Decision
            || filing.wait_bound() != self.bound
            || self.bound.expired(filing.recorded_ns())
            || filing.sequence().0 <= self.parent.0
            || !filing.receipts().is_complete()
            || filing.receipts().sources().count() != 1
            || filing.receipts().dispositions().count() != 1
            || expansion.template_id() != "plan.activate"
        {
            return Err("activation lost its original complete filing and bound");
        }
        let (file, rejected) = match filing.receipts().dispositions().next().unwrap() {
            SourceDisposition::Committed(value) => {
                let LedgerFile::Statement(file) = value.file() else {
                    return Err("activation has a pair receipt");
                };
                (file.as_ref(), false)
            }
            SourceDisposition::CorrectlyRejected(value) => (value.file(), true),
        };
        let metadata = file
            .context
            .as_ref()
            .ok_or("activation lost its typed context")?;
        let effect: s::PlanEffect =
            s::decode_strict(metadata.raw.get()).map_err(|_| "activation has malformed context")?;
        if file.block_id.run != expansion.run_id()
            || file.block_id.sequence.0 != filing.sequence().0
            || file.block_id.block.0 != 0
            || file.source.kind != "plan.event"
            || file.source.channel != "internal"
            || file.source.origin != "loop"
            || file.source.turn.as_deref() != Some(self.turn.0.as_str())
            || file.source.causal_parent.map(|v| v.0) != Some(self.parent.0)
            || file.scheme_id != expansion.scheme().scheme_id
            || file.text != expansion.text()
            || metadata.source_path != [s::SourcePathPart::Key("context".into())]
            || effect != self.effect
            || if rejected {
                file.effect.is_some()
            } else {
                file.effect.as_ref()
                    != Some(&s::StatementEffect::Plan(Box::new(self.effect.clone())))
            }
        {
            return Err("activation filing differs from its certified source or private grant");
        }
        Ok(rejected)
    }
}
