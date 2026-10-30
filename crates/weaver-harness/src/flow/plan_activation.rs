//! Activation consumes the actual successor proof and allocates only from the
//! recorded marks catalog. Neither public metadata nor a model supplies a grant.
use super::effect::PreparedPortEffect;
use super::*;
use crate::flow::clock::AbsoluteBound;
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

pub(in crate::flow) fn prepare(
    request: &PlanRequest,
    context: &RecordingContext,
    current: &CommittedPlanSummary,
    marks: &RecordedRead,
    turn: TurnRef,
    bound: AbsoluteBound,
) -> Result<PreparedPortEffect, PlanCheckFailure> {
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
    let open_scopes = task_catalog(marks, plan)?;
    let seed = &plan.plan.plan_id.seed_id;
    let mut spans = SpanAllocator::from_marks(context, marks, seed)?;
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
                None => spans.take("activation step counter exhausted")?,
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
    Ok(PreparedPortEffect {
        origin: Arc::clone(marks.origin()),
        effect: s::PlanEffect::Activate {
            expected: plan.plan.clone(),
            step_id: step.step_id.clone(),
            step_span: spans.peek(),
            group_span,
            successor: successor.reference().clone(),
        },
        parent: marks.recall_sequence(),
        turn: Some(turn),
        bound,
    })
}

/// The catalog activation and close allocate from: a coherent Marks result
/// whose open path is the plan's own task. It returns the open scopes.
pub(in crate::flow) fn task_catalog<'a>(
    marks: &'a RecordedRead,
    plan: &s::PlanSnapshot,
) -> Result<&'a [s::SpanInfo], PlanCheckFailure> {
    let s::QueryResult::Marks {
        current_task,
        open_scopes,
        ..
    } = marks.result()
    else {
        return Err(PlanCheckFailure::Protocol("marks read has no catalog"));
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
    Ok(open_scopes)
}

/// The span allocator over a recorded marks catalog. Activation and close
/// allocate their new span IDs through it.
/// The store still requires each opened span to be its exact next ID.
pub(in crate::flow) struct SpanAllocator {
    seed: String,
    next: s::DecimalU64,
}

impl SpanAllocator {
    pub(in crate::flow) fn from_marks(
        context: &RecordingContext,
        marks: &RecordedRead,
        seed: &str,
    ) -> Result<Self, PlanCheckFailure> {
        let s::QueryResult::Marks { next_span_id, .. } = marks.result() else {
            return Err(PlanCheckFailure::Protocol(
                "span allocation has no marks catalog",
            ));
        };
        if !marks.belongs_to(context) || marks.selector() != &(s::ReadSelector::Marks {}) {
            return Err(PlanCheckFailure::Protocol(
                "span allocation has a foreign marks proof",
            ));
        }
        Ok(Self {
            seed: seed.into(),
            next: *next_span_id,
        })
    }

    /// The next unallocated span. It does not advance the catalog.
    pub(in crate::flow) fn peek(&self) -> s::SpanRef {
        s::SpanRef {
            seed_id: self.seed.clone(),
            local_id: self.next,
            revision: 1.into(),
        }
    }

    /// Allocate one span before a later one in the same effect.
    pub(in crate::flow) fn take(&mut self, detail: &str) -> Result<s::SpanRef, PlanCheckFailure> {
        let allocated = self.peek();
        self.next = self
            .next
            .0
            .checked_add(1)
            .ok_or_else(|| refuse(s::PlanOperationCase::CounterExhausted, detail))?
            .into();
        Ok(allocated)
    }
}
