use super::*;

struct Input {
    source_id: i64,
    context: PlanInputContext,
    task: Option<ContentRef>,
    steps: Vec<(PlanInputStep, StatementRef)>,
    categories: Vec<Category>,
    groups: Vec<GroupDraft>,
    review: ReviewSetupRef,
    preserved: Vec<StepId>,
}

impl Operation<'_, '_> {
    fn operator_input(&self, source: &SourceBlockId) -> Result<Input, FileFault> {
        if source.block.0 != 0 || self.filing.source.turn.is_some() {
            return Err(invalid_effect(
                "operator changes must be turnless and name block zero",
            ));
        }
        let id = self.held_source(source)?;
        let context = reads::operator_context(self.tx, id).map_err(read_fault)?;
        let (task, steps, categories, groups, review, preserved) = match &context.operation {
            PlanInputOperation::Create {
                task,
                steps,
                categories,
                groups,
                review,
            } => (
                Some(*task),
                steps.clone(),
                categories.clone(),
                groups.clone(),
                review.clone(),
                vec![],
            ),
            PlanInputOperation::Authorize {
                task,
                categories,
                groups,
                review,
            } => (
                Some(*task),
                vec![],
                categories.clone(),
                groups.clone(),
                review.clone(),
                vec![],
            ),
            PlanInputOperation::Replace(r) | PlanInputOperation::AnswerBreak { replacement: r } => {
                (
                    r.task,
                    r.steps.clone(),
                    r.categories.clone(),
                    r.groups.clone(),
                    r.review.clone(),
                    r.preserved_closures.clone(),
                )
            }
        };
        if task.is_some_and(|index| index.0 != 0) || review.setup_id.is_empty() {
            return Err(invalid_effect("invalid task block or review identity"));
        }
        let is_authorize = matches!(context.operation, PlanInputOperation::Authorize { .. });
        if !is_authorize && steps.is_empty() {
            return Err(invalid_effect("empty supplied plan"));
        }
        if matches!(
            context.operation,
            PlanInputOperation::Create { .. } | PlanInputOperation::Authorize { .. }
        ) && (context.expected.is_some() || steps.iter().any(|s| s.id.is_some()))
        {
            return Err(invalid_effect("new plan must not name existing identity"));
        }
        let offset = u64::from(task.is_some());
        let count = offset
            .checked_add(steps.len() as u64)
            .ok_or_else(|| invalid_effect("block count overflow"))?;
        let held: i64 = self.tx.query_row(
            "SELECT COUNT(*) FROM content_block WHERE run=?1 AND sequence=?2",
            params![source.run, blob(source.sequence.0)],
            |row| row.get(0),
        )?;
        if u64::try_from(held).ok() != Some(count) {
            return Err(invalid_effect(
                "operator event is not fully held or has unused blocks",
            ));
        }
        for index in 0..count {
            let block = SourceBlockId {
                block: index.into(),
                ..source.clone()
            };
            let block_id = self.held_source(&block)?;
            let valid: bool = self.tx.query_row("SELECT source_kind='event' AND kind='plan.event' AND channel='input' AND origin='operator' AND turn IS NULL AND scheme_id=?2 AND content_type='statement' AND length(CAST(statement_text AS BLOB))>0 AND effect IS NULL AND ((?3=0 AND context_raw IS NOT NULL) OR (?3!=0 AND context_raw IS NULL AND context_path IS NULL)) FROM content_block WHERE id=?1",params![block_id,self.context.scheme_id,index as i64],|row|row.get(0))?;
            if !valid {
                return Err(invalid_effect("operator block has invalid provenance"));
            }
        }
        let mut bound = Vec::with_capacity(steps.len());
        for (index, step) in steps.into_iter().enumerate() {
            if step.block.0 != offset + index as u64 {
                return Err(invalid_effect(
                    "operator Text indices must enumerate the whole event",
                ));
            }
            let block = SourceBlockId {
                block: step.block,
                ..source.clone()
            };
            let block_id = self.held_source(&block)?;
            let bindings: i64 = self.tx.query_row(
                "SELECT COUNT(*) FROM statement_binding WHERE block_id=?1",
                [block_id],
                |row| row.get(0),
            )?;
            if bindings != 1 {
                return Err(invalid_effect(
                    "operator step must have exactly one binding",
                ));
            }
            let statement = StatementRef {
                block_id: block,
                binding: 0.into(),
            };
            marks::resolve_statement(self.tx, &statement)?;
            bound.push((step, statement));
        }
        validate_categories(&categories)?;
        validate_groups(
            &groups,
            if is_authorize {
                None
            } else {
                Some(bound.len())
            },
        )?;
        Ok(Input {
            source_id: id,
            context,
            task: task.map(|_| ContentRef::Event {
                block_id: source.clone(),
            }),
            steps: bound,
            categories,
            groups,
            review,
            preserved,
        })
    }

    fn prepare_new(&self, plan: &PlanRef) -> Result<(), FileFault> {
        self.valid_plan(plan)?;
        if plan.revision.0 != 1
            || plan.plan_id.local_id.0
                != next_counter(self.tx, "SELECT MAX(plan_id) FROM plan_revision")?
        {
            return Err(invalid_effect("new plan does not use the next identity"));
        }
        if let Some(current) = self.current()? {
            if matches!(current.status, PlanStatus::Authorized | PlanStatus::Locked) {
                return Err(invalid_effect("an unfinished plan already exists"));
            }
            if current.status == PlanStatus::Broken {
                self.retire_execution(&current)?;
                marks::retire_open_path(self.tx, self.context, self.source)?;
                self.status(&current, "superseded")?;
            }
        }
        // A directly opened task is also retired when an operator starts a plan.
        marks::retire_open_path(self.tx, self.context, self.source)?;
        Ok(())
    }

    pub(super) fn authorize(
        &self,
        plan: &PlanRef,
        source: &SourceBlockId,
        task: &TaskScopeRef,
        review: &ReviewSetupRef,
    ) -> Result<(), FileFault> {
        let input = self.operator_input(source)?;
        if !matches!(
            input.context.operation,
            PlanInputOperation::Authorize { .. }
        ) || input.review != *review
        {
            return Err(invalid_effect("authorize does not match operator input"));
        }
        self.prepare_new(plan)?;
        marks::begin_task(
            self.tx,
            self.context,
            self.source,
            &task.task,
            &task.prompt,
            input
                .task
                .as_ref()
                .ok_or_else(|| invalid_effect("missing task"))?,
        )?;
        self.insert_revision(plan, task, "authorized", &input, None)?;
        Ok(())
    }

    pub(super) fn lock(
        &self,
        plan: &PlanRef,
        source: &PlanLockSource,
        task: &TaskScopeRef,
        steps: &[PlanStepInit],
        review: &ReviewSetupRef,
    ) -> Result<(), FileFault> {
        match source {
            PlanLockSource::Operator { input: source } => {
                let input = self.operator_input(source)?;
                if !matches!(input.context.operation, PlanInputOperation::Create { .. })
                    || input.review != *review
                {
                    return Err(invalid_effect("lock does not match operator create"));
                }
                self.prepare_new(plan)?;
                self.validate_steps(steps, &input, None)?;
                marks::begin_task(
                    self.tx,
                    self.context,
                    self.source,
                    &task.task,
                    &task.prompt,
                    input
                        .task
                        .as_ref()
                        .ok_or_else(|| invalid_effect("missing task"))?,
                )?;
                self.insert_revision(plan, task, "locked", &input, Some(self.source))?;
                self.insert_steps(plan, steps, &input.categories, &input.groups, None, &[])?;
            }
            PlanLockSource::Model {
                authorization,
                candidate,
            } => {
                let current = self.expected(plan)?;
                if current.status != PlanStatus::Authorized
                    || current.operator_source != *authorization
                    || current.task != *task
                    || current.review != *review
                    || steps.is_empty()
                    || candidate.len() != steps.len()
                {
                    return Err(invalid_effect("model lock changed its authorization"));
                }
                self.held_source(authorization)?;
                let mut next = next_counter(self.tx, "SELECT MAX(step_id) FROM plan_step")?;
                let mut seen = Vec::new();
                for (index, (step, statement)) in steps.iter().zip(candidate).enumerate() {
                    if seen.contains(statement)
                        || step.instruction != *statement
                        || step.step_id.seed_id != self.context.seed_id
                        || step.step_id.local_id.0 != next
                    {
                        return Err(invalid_effect("invalid model candidate identity or order"));
                    }
                    let held = marks::resolve_statement(self.tx, statement)?;
                    if held.block_id >= self.source {
                        return Err(invalid_effect("candidate is not prior held content"));
                    }
                    seen.push(statement.clone());
                    if index + 1 < steps.len() {
                        next = next
                            .checked_add(1)
                            .ok_or_else(|| invalid_effect("step counter exhausted"))?;
                    }
                }
                validate_groups(&current.group_drafts, Some(steps.len()))?;
                self.insert_steps(
                    plan,
                    steps,
                    &current.categories,
                    &current.group_drafts,
                    None,
                    &[],
                )?;
                checked_one(self.tx.execute("UPDATE plan_revision SET status='locked',lock_source=?1,last_source=?1 WHERE plan_id=?2 AND revision=?3 AND status='authorized'",params![self.source,blob(plan.plan_id.local_id.0),blob(plan.revision.0)])?)?;
            }
        }
        Ok(())
    }

    pub(super) fn replace(
        &self,
        expected: &PlanRef,
        next: &PlanRef,
        source: &SourceBlockId,
        task: &TaskScopeRef,
        steps: &[PlanStepInit],
        review: &ReviewSetupRef,
    ) -> Result<(), FileFault> {
        let current = self.expected(expected)?;
        let input = self.operator_input(source)?;
        if !matches!(
            input.context.operation,
            PlanInputOperation::Replace(_) | PlanInputOperation::AnswerBreak { .. }
        ) || input.context.expected.as_ref() != Some(expected)
            || input.review != *review
            || next.plan_id != expected.plan_id
            || expected.revision.0.checked_add(1) != Some(next.revision.0)
        {
            return Err(invalid_effect(
                "replacement does not match the current operator revision",
            ));
        }
        if matches!(
            input.context.operation,
            PlanInputOperation::AnswerBreak { .. }
        ) && current.status != PlanStatus::Broken
        {
            return Err(invalid_effect("answer_break requires a broken plan"));
        }
        self.validate_steps(steps, &input, Some(&current))?;
        self.retire_execution(&current)?;
        let task_source = if let Some(source) = &input.task {
            marks::retire_open_path(self.tx, self.context, self.source)?;
            Some(source.clone())
        } else if self.span_is_open(&current.task.task)? {
            if *task != current.task {
                return Err(invalid_effect("unchanged open task must retain its scope"));
            }
            None
        } else {
            let content_id: i64 = self.tx.query_row(
                "SELECT block_id FROM span_prefix WHERE local_id=?1 AND revision=?2 AND ordinal=?3",
                params![
                    blob(current.task.prompt.local_id.0),
                    blob(current.task.prompt.revision.0),
                    blob(0)
                ],
                |row| row.get(0),
            )?;
            let (run, sequence, block): (String, Vec<u8>, Vec<u8>) = self.tx.query_row(
                "SELECT run,sequence,block FROM content_block WHERE id=?1 AND source_kind='event'",
                [content_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )?;
            Some(ContentRef::Event {
                block_id: SourceBlockId {
                    run,
                    sequence: decode_counter(sequence)?.into(),
                    block: decode_counter(block)?.into(),
                },
            })
        };
        if let Some(source) = task_source {
            marks::begin_task(
                self.tx,
                self.context,
                self.source,
                &task.task,
                &task.prompt,
                &source,
            )?;
        }
        self.status(&current, "superseded")?;
        self.insert_revision(next, task, "locked", &input, Some(self.source))?;
        self.insert_steps(
            next,
            steps,
            &input.categories,
            &input.groups,
            Some(&current),
            &input.preserved,
        )?;
        if input.preserved.len() == steps.len() {
            if self.span_is_open(&task.task)? {
                marks::finish(
                    self.tx,
                    self.context,
                    self.source,
                    &task.task,
                    SpanStatus::Closed,
                    None,
                )?;
            }
            checked_one(self.tx.execute("UPDATE plan_revision SET status='complete' WHERE plan_id=?1 AND revision=?2 AND status='locked'",params![blob(next.plan_id.local_id.0),blob(next.revision.0)])?)?;
        }
        Ok(())
    }

    fn validate_steps(
        &self,
        steps: &[PlanStepInit],
        input: &Input,
        previous: Option<&PlanSnapshot>,
    ) -> Result<(), FileFault> {
        if steps.is_empty() || steps.len() != input.steps.len() {
            return Err(invalid_effect("step count does not match input"));
        }
        let mut next = next_counter(self.tx, "SELECT MAX(step_id) FROM plan_step")?;
        let mut seen = Vec::new();
        for (step, (draft, statement)) in steps.iter().zip(&input.steps) {
            if step.step_id.seed_id != self.context.seed_id
                || step.step_id.local_id.0 == 0
                || seen.contains(&step.step_id)
            {
                return Err(invalid_effect("duplicate or foreign step identity"));
            }
            seen.push(step.step_id.clone());
            if let Some(id) = &draft.id {
                if id != &step.step_id
                    || previous.is_none_or(|p| !p.steps.iter().any(|s| s.step_id == *id))
                {
                    return Err(invalid_effect(
                        "step identity is not in the preceding revision",
                    ));
                }
            } else {
                if step.step_id.local_id.0 != next {
                    return Err(invalid_effect("new step must use the next identity"));
                }
                // u64::MAX may be assigned once, but another new step must refuse.
                next = next.checked_add(1).unwrap_or(0);
            }
            if input.preserved.contains(&step.step_id) {
                let old = previous
                    .and_then(|p| p.steps.iter().find(|s| s.step_id == step.step_id))
                    .ok_or_else(|| invalid_effect("unknown preserved step"))?;
                let old_held = marks::resolve_statement(self.tx, &old.instruction)?;
                let proposed = marks::resolve_statement(self.tx, statement)?;
                let old_category =
                    previous.and_then(|p| p.categories.iter().find(|c| c.id == old.category));
                let category = self.category(statement, &input.categories)?;
                if old.status != StepStatus::Closed
                    || old.instruction != step.instruction
                    || held_text(&old_held)? != held_text(&proposed)?
                    || old_category != Some(category)
                    || previous.is_none_or(|p| p.review != input.review)
                    || old.span.is_none()
                    || old.outcome_span.is_none()
                    || old.work.is_none()
                    || old.closure_source.is_none()
                {
                    return Err(invalid_effect("preserved closure changed"));
                }
            } else if step.instruction != *statement {
                return Err(invalid_effect(
                    "step instruction does not name the operator block",
                ));
            }
        }
        for (index, id) in input.preserved.iter().enumerate() {
            if input.preserved[..index].contains(id) || !seen.contains(id) {
                return Err(invalid_effect("invalid preserved closure list"));
            }
        }
        Ok(())
    }

    fn category<'a>(
        &self,
        statement: &StatementRef,
        categories: &'a [Category],
    ) -> Result<&'a Category, FileFault> {
        let held = marks::resolve_statement(self.tx, statement)?;
        if held.block_id >= self.source {
            return Err(invalid_effect("instruction must precede effect"));
        }
        let verb: String = self.tx.query_row(
            "SELECT verb FROM statement_binding WHERE id=?1",
            [held.id],
            |row| row.get(0),
        )?;
        let mut matches = categories.iter().filter(|c| c.verbs.contains(&verb));
        let category = matches
            .next()
            .ok_or_else(|| invalid_effect("instruction has no category"))?;
        if matches.next().is_some() {
            return Err(invalid_effect("ambiguous instruction category"));
        }
        Ok(category)
    }

    fn insert_revision(
        &self,
        plan: &PlanRef,
        task: &TaskScopeRef,
        status: &str,
        input: &Input,
        lock: Option<i64>,
    ) -> Result<(), FileFault> {
        self.tx.execute("INSERT INTO plan_revision(plan_id,revision,task_id,task_revision,prompt_id,prompt_revision,status,operator_source,lock_source,last_source,review_setup,group_drafts) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",params![blob(plan.plan_id.local_id.0),blob(plan.revision.0),blob(task.task.local_id.0),blob(task.task.revision.0),blob(task.prompt.local_id.0),blob(task.prompt.revision.0),status,input.source_id,lock,self.source,serde_json::to_string(&input.review).map_err(json_fault)?,serde_json::to_string(&input.groups).map_err(json_fault)?])?;
        for (ordinal, category) in input.categories.iter().enumerate() {
            self.tx.execute("INSERT INTO plan_category(plan_id,revision,category_id,ordinal,category) VALUES (?1,?2,?3,?4,?5)",params![blob(plan.plan_id.local_id.0),blob(plan.revision.0),category.id,blob(ordinal as u64),serde_json::to_string(category).map_err(json_fault)?])?;
        }
        Ok(())
    }

    fn insert_steps(
        &self,
        plan: &PlanRef,
        steps: &[PlanStepInit],
        categories: &[Category],
        groups: &[GroupDraft],
        previous: Option<&PlanSnapshot>,
        preserved: &[StepId],
    ) -> Result<(), FileFault> {
        for (ordinal, step) in steps.iter().enumerate() {
            let held = marks::resolve_statement(self.tx, &step.instruction)?;
            let category = self.category(&step.instruction, categories)?;
            if preserved.contains(&step.step_id) {
                let old = previous.ok_or_else(|| invalid_effect("no preceding closure"))?;
                checked_one(self.tx.execute("INSERT INTO plan_step(plan_id,revision,step_id,ordinal,instruction_id,category_id,status,span_id,span_revision,outcome_id,outcome_revision,review_run,review_sequence,review_call,work,closure_source) SELECT ?1,?2,step_id,?3,instruction_id,category_id,status,span_id,span_revision,outcome_id,outcome_revision,review_run,review_sequence,review_call,work,closure_source FROM plan_step WHERE plan_id=?4 AND revision=?5 AND step_id=?6 AND status='closed'",params![blob(plan.plan_id.local_id.0),blob(plan.revision.0),blob(ordinal as u64+1),blob(old.plan.plan_id.local_id.0),blob(old.plan.revision.0),blob(step.step_id.local_id.0)])?)?;
            } else {
                self.tx.execute("INSERT INTO plan_step(plan_id,revision,step_id,ordinal,instruction_id,category_id,status) VALUES (?1,?2,?3,?4,?5,?6,'waiting')",params![blob(plan.plan_id.local_id.0),blob(plan.revision.0),blob(step.step_id.local_id.0),blob(ordinal as u64+1),held.id,category.id])?;
            }
        }
        for (ordinal, group) in groups.iter().enumerate() {
            self.tx.execute(
                "INSERT INTO plan_group(plan_id,revision,group_id,ordinal) VALUES (?1,?2,?3,?4)",
                params![
                    blob(plan.plan_id.local_id.0),
                    blob(plan.revision.0),
                    group.id,
                    blob(ordinal as u64)
                ],
            )?;
            for index in &group.step_indices {
                let step = steps
                    .get(
                        usize::try_from(index.0)
                            .map_err(|_| invalid_effect("group index overflow"))?,
                    )
                    .ok_or_else(|| invalid_effect("group index outside plan"))?;
                self.tx.execute("INSERT INTO plan_group_step(plan_id,revision,group_id,step_id) VALUES (?1,?2,?3,?4)",params![blob(plan.plan_id.local_id.0),blob(plan.revision.0),group.id,blob(step.step_id.local_id.0)])?;
            }
        }
        Ok(())
    }
}

pub(super) fn validate_categories(categories: &[Category]) -> Result<(), FileFault> {
    let mut ids = Vec::new();
    let mut verbs = Vec::new();
    for category in categories {
        if category.id.is_empty() || ids.contains(&category.id) || category.verbs.is_empty() {
            return Err(invalid_effect("invalid categories"));
        }
        ids.push(category.id.clone());
        for verb in &category.verbs {
            if verb.is_empty() || verbs.contains(verb) {
                return Err(invalid_effect(
                    "category verbs must be nonempty and disjoint",
                ));
            }
            verbs.push(verb.clone());
        }
    }
    Ok(())
}

pub(super) fn validate_groups(
    groups: &[GroupDraft],
    count: Option<usize>,
) -> Result<(), FileFault> {
    let mut ids = Vec::new();
    let mut used = Vec::new();
    for group in groups {
        if group.id.is_empty() || ids.contains(&group.id) || group.step_indices.is_empty() {
            return Err(invalid_effect("invalid group identity"));
        }
        ids.push(group.id.clone());
        for (position, index) in group.step_indices.iter().enumerate() {
            if used.contains(index)
                || count.is_some_and(|count| index.0 >= count as u64)
                || (position > 0
                    && group.step_indices[position - 1].0.checked_add(1) != Some(index.0))
            {
                return Err(invalid_effect("groups must be disjoint contiguous steps"));
            }
            used.push(*index);
        }
    }
    Ok(())
}

pub(super) fn held_text(held: &marks::HeldStatement) -> Result<&str, FileFault> {
    let start =
        usize::try_from(held.start).map_err(|_| invalid_effect("statement offset overflow"))?;
    let end = usize::try_from(held.end).map_err(|_| invalid_effect("statement offset overflow"))?;
    held.text
        .get(start..end)
        .ok_or_else(|| invalid_effect("statement range is invalid"))
}
