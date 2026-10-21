use super::*;
use rusqlite::named_params;
use sha2::{Digest as _, Sha256};

impl Operation<'_, '_> {
    pub(super) fn activate(
        &self,
        expected: &PlanRef,
        step_id: &StepId,
        step_span: &SpanRef,
        group_span: Option<&SpanRef>,
        successor: &StepQueryRef,
    ) -> Result<(), FileFault> {
        let current = self.locked(expected)?;
        let (checked, _) = reads::step_successor(
            self.tx,
            &self.context.seed_id,
            expected,
            successor.previous_step.as_ref(),
            self.limits.max_result_bytes,
        )
        .map_err(read_fault)?;
        if checked != *successor || checked.step_id != *step_id {
            return Err(invalid_effect(
                "successor does not name the first waiting step",
            ));
        }
        let step = current
            .steps
            .iter()
            .find(|s| s.step_id == *step_id)
            .ok_or_else(|| invalid_effect("unknown step"))?;
        let parent = match &step.group {
            None => {
                if group_span.is_some() {
                    return Err(invalid_effect("ungrouped step supplied a group span"));
                }
                &current.task.task
            }
            Some(id) => {
                let group = current
                    .groups
                    .iter()
                    .find(|g| g.id == *id)
                    .ok_or_else(|| invalid_effect("missing step group"))?;
                let span = group_span
                    .ok_or_else(|| invalid_effect("grouped step requires a group span"))?;
                match &group.span {
                    Some(existing) => {
                        if existing != span || !self.span_is_open(span)? {
                            return Err(invalid_effect("group execution span changed or ended"));
                        }
                    }
                    None => {
                        marks::open(
                            self.tx,
                            self.context,
                            self.source,
                            span,
                            &current.task.task,
                            SpanKind::Subtask,
                            &[],
                        )?;
                        checked_one(self.tx.execute("UPDATE plan_group SET span_id=?1,span_revision=?2 WHERE plan_id=?3 AND revision=?4 AND group_id=?5 AND span_id IS NULL",params![blob(span.local_id.0),blob(span.revision.0),blob(expected.plan_id.local_id.0),blob(expected.revision.0),id])?)?;
                    }
                }
                span
            }
        };
        let instruction = marks::resolve_statement(self.tx, &step.instruction)?;
        let prefix = SpanText {
            source: ContentRef::Event {
                block_id: step.instruction.block_id.clone(),
            },
            start: instruction.start.into(),
            end: instruction.end.into(),
            step_query: Some(checked),
        };
        marks::open(
            self.tx,
            self.context,
            self.source,
            step_span,
            parent,
            SpanKind::Step,
            &[prefix],
        )?;
        checked_one(self.tx.execute("UPDATE plan_step SET status='active',span_id=?1,span_revision=?2 WHERE plan_id=?3 AND revision=?4 AND step_id=?5 AND status='waiting'",params![blob(step_span.local_id.0),blob(step_span.revision.0),blob(expected.plan_id.local_id.0),blob(expected.revision.0),blob(step_id.local_id.0)])?)?;
        self.advance(&current)
    }

    pub(super) fn break_plan(
        &self,
        expected: &PlanRef,
        step_id: Option<&StepId>,
        case: PlanBreakCase,
        proposed: Option<&SourceBlockId>,
    ) -> Result<(), FileFault> {
        let current = self.locked(expected)?;
        let active = current
            .steps
            .iter()
            .find(|s| s.status == StepStatus::Active);
        if active.map(|s| &s.step_id) != step_id {
            return Err(invalid_effect("break does not name the active step"));
        }
        if let Some(source) = proposed {
            self.held_source(source)?;
        }
        let record = PlanBreakRecord {
            case,
            step_id: step_id.cloned(),
            proposed_source: proposed.cloned(),
            source: self.filing.block_id.clone(),
        };
        checked_one(self.tx.execute("UPDATE plan_revision SET status='broken',broken=?1,last_source=?2 WHERE plan_id=?3 AND revision=?4 AND status='locked'",params![serde_json::to_string(&record).map_err(json_fault)?,self.source,blob(expected.plan_id.local_id.0),blob(expected.revision.0)])?)
    }

    pub(super) fn close(
        &self,
        expected: &PlanRef,
        step_id: &StepId,
        review_call: Option<&CallId>,
        work: &WorkRef,
        outcome: &SpanRef,
    ) -> Result<(), FileFault> {
        let current = self.locked(expected)?;
        let step = current
            .steps
            .iter()
            .find(|s| s.step_id == *step_id && s.status == StepStatus::Active)
            .ok_or_else(|| invalid_effect("step is not active"))?;
        let span = step
            .span
            .as_ref()
            .ok_or_else(|| invalid_effect("active step has no span"))?;
        let category = current
            .categories
            .iter()
            .find(|c| c.id == step.category)
            .ok_or_else(|| invalid_effect("step category is absent"))?;
        if work.plan != *expected
            || work.step_id != *step_id
            || work.span != *span
            || (review_call.is_none() && category.review_required)
        {
            return Err(invalid_effect(
                "closure evidence does not match the active step or review rule",
            ));
        }
        let frontier = self.held_source(&work.through.block_id)?;
        let encoded = reads::span_preview_encoded(
            self.tx,
            &self.context.seed_id,
            span,
            &work.through,
            self.limits.max_result_bytes,
        )
        .map_err(read_fault)?;
        if format!("{:x}", Sha256::digest(&encoded)) != work.content_sha256.as_str() {
            return Err(invalid_effect("work preview changed"));
        }
        let mut allowed = vec![self.source];
        if let Some(call) = review_call {
            let row:Option<(String,String,String,i64,i64,String)>=self.tx.query_row(
                "SELECT t.lane,t.fate,t.request,s.block_id,t.opened_event,b.raw_block FROM tool_pair t JOIN statement_binding s ON s.id=t.statement_id JOIN content_block b ON b.id=t.terminal_event WHERE t.run=?1 AND t.sequence=?2 AND t.call=?3",
                params![call.run,blob(call.sequence.0),blob(call.call.0)],
                |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?)),
            ).optional()?;
            let (lane, fate, request, request_id, opened, terminal) =
                row.ok_or_else(|| invalid_effect("review pair is absent or unfinished"))?;
            let request: PairRequest = decode_strict(&request)
                .map_err(|_| invalid_effect("invalid held review request"))?;
            let ended: PairEnded = decode_strict(&terminal)
                .map_err(|_| invalid_effect("invalid held review result"))?;
            if lane != "review"
                || fate != "completed"
                || request_id <= frontier
                || ended.call_id != *call
                || ended.fate != TerminalPairFate::Completed
                || !matches!(
                    ended.outcome,
                    Some(weaver_types::ToolOutcome::Result { .. })
                )
                || !matches!(request,PairRequest::Review{ref setup,work:ref requested,..} if setup==&current.review && requested.as_ref()==work)
            {
                return Err(invalid_effect(
                    "review pair does not establish this work and setup",
                ));
            }
            let terminal: i64 = self.tx.query_row(
                "SELECT terminal_event FROM tool_pair WHERE run=?1 AND sequence=?2 AND call=?3",
                params![call.run, blob(call.sequence.0), blob(call.call.0)],
                |row| row.get(0),
            )?;
            allowed.extend([request_id, opened, terminal]);
        }
        self.close_finish(
            &current,
            step,
            work,
            outcome,
            review_call,
            frontier,
            &mut allowed,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn close_finish(
        &self,
        current: &PlanSnapshot,
        step: &PlanStep,
        work: &WorkRef,
        outcome: &SpanRef,
        review: Option<&CallId>,
        frontier: i64,
        allowed: &mut [i64],
    ) -> Result<(), FileFault> {
        let mut statement=self.tx.prepare("SELECT id FROM content_block WHERE source_kind='event' AND id>?1 AND id<=?2 ORDER BY id")?;
        let mut rows = statement.query(params![frontier, self.source])?;
        while let Some(row) = rows.next()? {
            let id: i64 = row.get(0)?;
            if !allowed.contains(&id) {
                return Err(invalid_effect("work changed after the closure frontier"));
            }
        }
        let span = step
            .span
            .as_ref()
            .ok_or_else(|| invalid_effect("active step has no span"))?;
        let prefix = SpanText {
            source: ContentRef::Event {
                block_id: self.filing.block_id.clone(),
            },
            start: 0.into(),
            end: (self.filing.text.len() as u64).into(),
            step_query: None,
        };
        marks::open(
            self.tx,
            self.context,
            self.source,
            outcome,
            span,
            SpanKind::StepOutcome,
            &[prefix],
        )?;
        marks::finish(
            self.tx,
            self.context,
            self.source,
            outcome,
            SpanStatus::Closed,
            None,
        )?;
        marks::finish(
            self.tx,
            self.context,
            self.source,
            span,
            SpanStatus::Closed,
            None,
        )?;
        let review_run = review.map(|call| call.run.as_str());
        let review_sequence = review.map(|call| blob(call.sequence.0));
        let review_call = review.map(|call| blob(call.call.0));
        let work_json = serde_json::to_string(work).map_err(json_fault)?;
        checked_one(self.tx.execute(
            "UPDATE plan_step AS s SET status='closed',outcome_id=:outcome_id,outcome_revision=:outcome_revision,review_run=:review_run,review_sequence=:review_sequence,review_call=:review_call,work=:work,closure_source=:source_id WHERE s.plan_id=:plan_id AND s.revision=:revision AND s.step_id=:step_id AND s.status='active' AND EXISTS(SELECT 1 FROM plan_revision p WHERE p.plan_id=s.plan_id AND p.revision=s.revision AND p.status='locked') AND EXISTS(SELECT 1 FROM plan_category c WHERE c.plan_id=s.plan_id AND c.revision=s.revision AND c.category_id=s.category_id AND ((json_extract(c.category,'$.review_required')=0 AND :review_run IS NULL AND :review_sequence IS NULL AND :review_call IS NULL) OR (:review_run IS NOT NULL AND :review_sequence IS NOT NULL AND :review_call IS NOT NULL AND EXISTS(SELECT 1 FROM tool_pair t WHERE t.run=:review_run AND t.sequence=:review_sequence AND t.call=:review_call AND t.lane='review' AND t.fate='completed'))))",
            named_params!{":outcome_id":blob(outcome.local_id.0),":outcome_revision":blob(outcome.revision.0),":review_run":review_run,":review_sequence":review_sequence,":review_call":review_call,":work":work_json,":source_id":self.source,":plan_id":blob(current.plan.plan_id.local_id.0),":revision":blob(current.plan.revision.0),":step_id":blob(step.step_id.local_id.0)},
        )?)?;
        if let Some(group_id) = &step.group {
            let pending:bool=self.tx.query_row("SELECT EXISTS(SELECT 1 FROM plan_group_step g JOIN plan_step s ON s.plan_id=g.plan_id AND s.revision=g.revision AND s.step_id=g.step_id WHERE g.plan_id=?1 AND g.revision=?2 AND g.group_id=?3 AND s.status!='closed')",params![blob(current.plan.plan_id.local_id.0),blob(current.plan.revision.0),group_id],|row|row.get(0))?;
            if !pending {
                let group = current
                    .groups
                    .iter()
                    .find(|g| g.id == *group_id)
                    .and_then(|g| g.span.as_ref())
                    .ok_or_else(|| invalid_effect("active group has no span"))?;
                marks::finish(
                    self.tx,
                    self.context,
                    self.source,
                    group,
                    SpanStatus::Closed,
                    None,
                )?;
            }
        }
        let pending:bool=self.tx.query_row("SELECT EXISTS(SELECT 1 FROM plan_step WHERE plan_id=?1 AND revision=?2 AND status!='closed')",params![blob(current.plan.plan_id.local_id.0),blob(current.plan.revision.0)],|row|row.get(0))?;
        if pending {
            self.advance(current)
        } else {
            marks::finish(
                self.tx,
                self.context,
                self.source,
                &current.task.task,
                SpanStatus::Closed,
                None,
            )?;
            self.status(current, "complete")
        }
    }
}
