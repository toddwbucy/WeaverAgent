//! Plan mutations share the source filing transaction and the store's mark helpers.

use rusqlite::{Connection, OptionalExtension, Transaction, params};
use weaver_types::state_flow::*;

use super::super::{ServingContext, reads};
use super::{blob, decode_counter, invalid_effect, json_fault, marks, source_id};
use crate::FileFault;

mod execution;
mod image;
mod image_sources;
mod revision;
pub(in crate::engine::serving_sqlite) use image::verify_image_plan;
#[cfg(test)]
mod tests;

pub(super) fn apply(
    tx: &Transaction<'_>,
    context: &ServingContext,
    filing: &StatementFile,
    source: i64,
    effect: &PlanEffect,
    limits: &ReadLimits,
) -> Result<(), FileFault> {
    let operation = Operation {
        tx,
        context,
        filing,
        source,
        limits,
    };
    match effect {
        PlanEffect::Authorize {
            plan,
            source,
            task,
            review,
        } => operation.authorize(plan, source, task, review),
        PlanEffect::Lock {
            plan,
            source,
            task,
            steps,
            review,
        } => operation.lock(plan, source, task, steps, review),
        PlanEffect::Replace {
            expected,
            next,
            source,
            task,
            steps,
            review,
        } => operation.replace(expected, next, source, task, steps, review),
        PlanEffect::Activate {
            expected,
            step_id,
            step_span,
            group_span,
            successor,
        } => operation.activate(expected, step_id, step_span, group_span.as_ref(), successor),
        PlanEffect::Close {
            expected,
            step_id,
            review_call,
            work,
            outcome_span,
        } => operation.close(expected, step_id, review_call.as_ref(), work, outcome_span),
        PlanEffect::Break {
            expected,
            step_id,
            case,
            proposed_source,
        } => operation.break_plan(expected, step_id.as_ref(), *case, proposed_source.as_ref()),
    }
}

struct Operation<'a, 'connection> {
    tx: &'a Transaction<'connection>,
    context: &'a ServingContext,
    filing: &'a StatementFile,
    source: i64,
    limits: &'a ReadLimits,
}

fn read_fault(error: reads::ReadFailure) -> FileFault {
    match error {
        reads::ReadFailure::Sqlite(error) => error.into(),
        other => invalid_effect(format!("plan evidence is unavailable: {other:?}")),
    }
}

fn next_counter(db: &Connection, sql: &str) -> Result<u64, FileFault> {
    let value: Option<Vec<u8>> = db.query_row(sql, [], |row| row.get(0))?;
    value
        .map(decode_counter)
        .transpose()?
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(|| invalid_effect("plan counter exhausted"))
}

fn checked_one(changed: usize) -> Result<(), FileFault> {
    if changed != 1 {
        return Err(invalid_effect("plan transition did not match one row"));
    }
    Ok(())
}

fn status_name(status: PlanStatus) -> &'static str {
    match status {
        PlanStatus::Authorized => "authorized",
        PlanStatus::Locked => "locked",
        PlanStatus::Broken => "broken",
        PlanStatus::Complete => "complete",
        PlanStatus::Superseded => "superseded",
    }
}

impl Operation<'_, '_> {
    fn current(&self) -> Result<Option<PlanSnapshot>, FileFault> {
        reads::current_plan_snapshot(self.tx, &self.context.seed_id, self.limits.max_result_bytes)
            .map_err(read_fault)
    }

    fn expected(&self, expected: &PlanRef) -> Result<PlanSnapshot, FileFault> {
        self.valid_plan(expected)?;
        let current = self
            .current()?
            .ok_or_else(|| invalid_effect("no current plan"))?;
        if current.plan != *expected {
            return Err(invalid_effect("stale plan revision"));
        }
        Ok(current)
    }

    fn locked(&self, expected: &PlanRef) -> Result<PlanSnapshot, FileFault> {
        let current = self.expected(expected)?;
        if current.status != PlanStatus::Locked {
            return Err(invalid_effect("plan is not locked"));
        }
        Ok(current)
    }

    fn valid_plan(&self, plan: &PlanRef) -> Result<(), FileFault> {
        if plan.plan_id.seed_id != self.context.seed_id
            || plan.plan_id.local_id.0 == 0
            || plan.revision.0 == 0
        {
            return Err(invalid_effect("invalid plan identity"));
        }
        Ok(())
    }

    fn held_source(&self, source: &SourceBlockId) -> Result<i64, FileFault> {
        let id = source_id(self.tx, source)?.ok_or_else(|| invalid_effect("source not held"))?;
        if id >= self.source {
            return Err(invalid_effect("evidence must precede its effect"));
        }
        Ok(id)
    }

    fn status(&self, current: &PlanSnapshot, status: &str) -> Result<(), FileFault> {
        checked_one(self.tx.execute(
            "UPDATE plan_revision SET status=?1,last_source=?2 WHERE plan_id=?3 AND revision=?4 AND status=?5",
            params![status, self.source, blob(current.plan.plan_id.local_id.0), blob(current.plan.revision.0), status_name(current.status)],
        )?)
    }

    fn advance(&self, current: &PlanSnapshot) -> Result<(), FileFault> {
        self.status(current, status_name(current.status))
    }

    fn retire_execution(&self, current: &PlanSnapshot) -> Result<(), FileFault> {
        // Keep the task when a replacement supplies no new task. Terminal historical
        // spans keep their original parents and evidence.
        for step in &current.steps {
            if step.status == StepStatus::Active {
                marks::finish(
                    self.tx,
                    self.context,
                    self.source,
                    step.span
                        .as_ref()
                        .ok_or_else(|| invalid_effect("active step has no span"))?,
                    SpanStatus::Retired,
                    None,
                )?;
                checked_one(self.tx.execute("UPDATE plan_step SET status='retired' WHERE plan_id=?1 AND revision=?2 AND step_id=?3 AND status='active'",params![blob(current.plan.plan_id.local_id.0),blob(current.plan.revision.0),blob(step.step_id.local_id.0)])?)?;
            }
        }
        for group in &current.groups {
            if let Some(span) = &group.span
                && self.span_is_open(span)?
            {
                marks::finish(
                    self.tx,
                    self.context,
                    self.source,
                    span,
                    SpanStatus::Retired,
                    None,
                )?;
            }
        }
        Ok(())
    }

    fn span_is_open(&self, span: &SpanRef) -> Result<bool, FileFault> {
        if span.seed_id != self.context.seed_id || span.revision.0 != 1 {
            return Err(invalid_effect("invalid span identity"));
        }
        let status: Option<String> = self
            .tx
            .query_row(
                "SELECT status FROM span WHERE local_id=?1 AND revision=?2",
                params![blob(span.local_id.0), blob(span.revision.0)],
                |row| row.get(0),
            )
            .optional()?;
        status
            .map(|status| status == "open")
            .ok_or_else(|| invalid_effect("unknown span"))
    }
}
