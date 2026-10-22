//! Mechanical catalog checks for the one explicit begin-task allocation owner.
//! No counter is persisted here and no value grants placement or generation.

use weaver_types::state_flow::{QueryResult, SpanKind, SpanRef, SpanStatus, TaskScopeRef};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CatalogError {
    Invalid,
    CounterExhausted,
}

#[derive(Debug)]
pub(super) enum TaskCatalog {
    Existing(TaskScopeRef),
    Proposed(TaskScopeRef),
}

/// Validate only facts actually carried by Marks. The closed prompt's contents
/// and boundary sources remain store facts, not invented read-side evidence.
pub(super) fn catalog(result: &QueryResult, seed: &str) -> Result<TaskCatalog, CatalogError> {
    let QueryResult::Marks {
        next_span_id,
        current_task,
        open_scopes,
    } = result
    else {
        return Err(CatalogError::Invalid);
    };
    let valid = |span: &SpanRef| {
        span.seed_id == seed
            && span.revision.0 == 1
            && span.local_id.0 != 0
            && span.local_id < *next_span_id
    };
    if next_span_id.0 == 0 {
        return Err(CatalogError::Invalid);
    }
    if let Some(current) = current_task
        && (!valid(&current.task)
            || !valid(&current.prompt)
            || current.task.local_id.0.checked_add(1) != Some(current.prompt.local_id.0))
    {
        return Err(CatalogError::Invalid);
    }
    let mut parent = None;
    let mut parent_kind = None;
    for scope in open_scopes {
        if !valid(&scope.span)
            || current_task
                .as_ref()
                .is_some_and(|current| current.prompt == scope.span)
            || scope.parent.as_ref() != parent
            || scope.status != SpanStatus::Open
            || scope.opened_source.is_none()
            || scope.ended_source.is_some()
            || scope.replacement.is_some()
            || (parent.is_none()
                && (scope.kind != SpanKind::Task
                    || current_task
                        .as_ref()
                        .is_none_or(|current| current.task != scope.span)))
            || (parent.is_some()
                && !matches!(
                    (parent_kind, scope.kind),
                    (Some(SpanKind::Task), SpanKind::Subtask | SpanKind::Step)
                        | (Some(SpanKind::Subtask), SpanKind::Step)
                ))
            || parent.is_some_and(|previous: &SpanRef| previous.local_id >= scope.span.local_id)
        {
            return Err(CatalogError::Invalid);
        }
        parent = Some(&scope.span);
        parent_kind = Some(scope.kind);
    }
    if !open_scopes.is_empty() {
        return Ok(TaskCatalog::Existing(
            current_task.clone().ok_or(CatalogError::Invalid)?,
        ));
    }
    let prompt = next_span_id
        .0
        .checked_add(1)
        .ok_or(CatalogError::CounterExhausted)?;
    let span = |local_id| SpanRef {
        seed_id: seed.into(),
        local_id,
        revision: 1.into(),
    };
    Ok(TaskCatalog::Proposed(TaskScopeRef {
        task: span(*next_span_id),
        prompt: span(prompt.into()),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_revision_defense_rejects_an_inspection_value() {
        // The actual read validator rejects these revisions before a
        // RecordedRead exists. Exercise this downstream defense directly on
        // plain query data rather than manufacturing a recorded owner.
        let seed = "a".repeat(32);
        let span = |id| SpanRef {
            seed_id: seed.clone(),
            local_id: id,
            revision: 1.into(),
        };
        let mut current = TaskScopeRef {
            task: span(1.into()),
            prompt: span(2.into()),
        };
        for change_task in [false, true] {
            let result = QueryResult::Marks {
                next_span_id: 3.into(),
                current_task: Some(current.clone()),
                open_scopes: vec![],
            };
            assert!(matches!(
                catalog(&result, &seed),
                Ok(TaskCatalog::Proposed(_))
            ));
            if change_task {
                current.task.revision = 2.into();
            } else {
                current.prompt.revision = 2.into();
            }
            let result = QueryResult::Marks {
                next_span_id: 3.into(),
                current_task: Some(current.clone()),
                open_scopes: vec![],
            };
            assert!(matches!(
                catalog(&result, &seed),
                Err(CatalogError::Invalid)
            ));
            current = TaskScopeRef {
                task: span(1.into()),
                prompt: span(2.into()),
            };
        }
    }
}
