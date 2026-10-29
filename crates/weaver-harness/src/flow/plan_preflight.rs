//! Structural root checks use the recorded catalogs and the actual loaded scheme.
//! They reserve no ID and grant no effect or closure preservation.
use super::*;
use crate::flow::context::RecordedRead;
use s::{PlanOperation as Op, PlanUpdateCase as Case};

impl AcceptedPlanUpdate {
    pub(in crate::flow) fn check_snapshots(
        &self,
        context: &RecordingContext,
        current: &RecordedRead,
        marks: &RecordedRead,
        seed: &str,
        scheme: &s::Ste100Scheme,
    ) -> Result<Option<s::PlanUpdateRefused>, &'static str> {
        if !self.belongs_to(context)
            || !current.belongs_to(context)
            || !marks.belongs_to(context)
            || current.selector() != &(s::ReadSelector::PlanCurrent {})
            || marks.selector() != &(s::ReadSelector::Marks {})
            || current.recall_sequence().0 >= marks.recall_sequence().0
        {
            return Err("root preflight lacks its ordered recorded catalogs");
        }
        let s::QueryResult::PlanCurrent { current: plan, .. } = current.result() else {
            return Err("root preflight lacks a current-plan result");
        };
        match crate::flow::marks::catalog(marks.result(), seed) {
            Err(crate::flow::marks::CatalogError::Invalid) => {
                return Err("root preflight has an invalid marks catalog");
            }
            Err(crate::flow::marks::CatalogError::CounterExhausted) => {
                return Ok(Some(refusal(
                    Case::CounterExhausted,
                    "task prompt counter exhausted",
                )));
            }
            Ok(_) => (),
        }
        Ok(check_document(&self.document, plan.as_ref(), seed, scheme).err())
    }
}

fn refusal(case: Case, detail: &str) -> s::PlanUpdateRefused {
    s::PlanUpdateRefused {
        case,
        detail: detail.into(),
        rejected_blocks: vec![],
        read_code: None,
    }
}
fn check_document(
    document: &s::PlanUpdateDocument,
    current: Option<&s::PlanSnapshot>,
    seed: &str,
    scheme: &s::Ste100Scheme,
) -> Result<(), s::PlanUpdateRefused> {
    if let Some(expected) = &document.expected {
        if expected.plan_id.seed_id != seed {
            return Err(refusal(
                Case::WrongSeed,
                "expected plan belongs to another seed",
            ));
        }
        if current.is_none_or(|current| current.plan != *expected) {
            return Err(refusal(
                Case::StalePlan,
                "expected plan is not the current revision",
            ));
        }
        if expected.revision.0 == u64::MAX {
            return Err(refusal(
                Case::CounterExhausted,
                "plan revision counter exhausted",
            ));
        }
    }
    if matches!(document.operation, Op::Create { .. } | Op::Authorize { .. })
        && current.is_some_and(|plan| {
            matches!(
                plan.status,
                s::PlanStatus::Authorized | s::PlanStatus::Locked
            )
        })
    {
        return Err(refusal(
            Case::PlanExists,
            "an unfinished current plan already exists",
        ));
    }
    if matches!(document.operation, Op::AnswerBreak { .. })
        && current.is_none_or(|plan| plan.status != s::PlanStatus::Broken)
    {
        return Err(refusal(
            Case::StalePlan,
            "answer_break requires the current broken plan",
        ));
    }
    let (steps, categories, groups, review, preserved, authorized) = match &document.operation {
        Op::Create {
            steps,
            categories,
            groups,
            review,
            ..
        } => (steps.as_slice(), categories, groups, review, &[][..], false),
        Op::Authorize {
            categories,
            groups,
            review,
            ..
        } => (&[][..], categories, groups, review, &[][..], true),
        Op::Replace(value) | Op::AnswerBreak { replacement: value } => (
            value.steps.as_slice(),
            &value.categories,
            &value.groups,
            &value.review,
            value.preserved_closures.as_slice(),
            false,
        ),
    };
    let technical = |name: &str| {
        scheme
            .technical_names
            .iter()
            .any(|value| value.text == name)
    };
    if review.setup_id.is_empty() || !technical(&review.setup_id) {
        return Err(refusal(
            Case::MalformedPlan,
            "review setup is not a loaded scheme technical name",
        ));
    }
    let mut ids = std::collections::HashSet::new();
    let mut verbs = std::collections::HashSet::new();
    for category in categories {
        if !technical(&category.id) || !ids.insert(&category.id) || category.verbs.is_empty() {
            return Err(refusal(
                Case::InvalidCategory,
                "category identity or verb set is invalid",
            ));
        }
        for verb in &category.verbs {
            if !verbs.insert(verb) || !scheme.verbs.iter().any(|entry| entry.key == *verb) {
                return Err(refusal(
                    Case::InvalidCategory,
                    "category verbs must be disjoint loaded scheme keys",
                ));
            }
        }
    }
    let mut group_ids = std::collections::HashSet::new();
    let mut grouped = std::collections::HashSet::new();
    for group in groups {
        if group.id.is_empty() || !group_ids.insert(&group.id) || group.step_indices.is_empty() {
            return Err(refusal(
                Case::InvalidGroup,
                "group identity or membership is invalid",
            ));
        }
        for (position, index) in group.step_indices.iter().enumerate() {
            if !grouped.insert(index.0)
                || (!authorized && index.0 >= steps.len() as u64)
                || (position > 0
                    && group.step_indices[position - 1].0.checked_add(1) != Some(index.0))
            {
                return Err(refusal(
                    Case::InvalidGroup,
                    "groups must contain disjoint contiguous steps in order",
                ));
            }
        }
    }
    let mut step_ids = std::collections::HashSet::new();
    for step in steps {
        let Some(id) = &step.id else {
            continue;
        };
        if id.seed_id != seed {
            return Err(refusal(Case::WrongSeed, "step belongs to another seed"));
        }
        if !step_ids.insert(id.local_id.0)
            || current.is_none_or(|plan| {
                !plan
                    .steps
                    .iter()
                    .any(|old| old.step_id == *id && old.status != s::StepStatus::Retired)
            })
        {
            return Err(refusal(
                Case::MalformedPlan,
                "step identity is duplicated, unknown or retired",
            ));
        }
    }
    let mut closures = std::collections::HashSet::new();
    for id in preserved {
        if id.seed_id != seed {
            return Err(refusal(
                Case::WrongSeed,
                "preserved closure belongs to another seed",
            ));
        }
        let old = current.and_then(|plan| plan.steps.iter().find(|old| old.step_id == *id));
        if !closures.insert(id.local_id.0)
            || !step_ids.contains(&id.local_id.0)
            || old.is_none_or(|old| {
                old.status != s::StepStatus::Closed
                    || old.span.is_none()
                    || old.outcome_span.is_none()
                    || old.work.is_none()
                    || old.closure_source.is_none()
            })
            || current.is_none_or(|plan| plan.review != *review)
        {
            return Err(refusal(
                Case::ClosureChanged,
                "preserved closure lacks unchanged filed evidence",
            ));
        }
    }
    Ok(())
}
