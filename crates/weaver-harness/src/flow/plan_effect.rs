//! Root effect construction requires the exact root input, actual filing and
//! recorded catalog/closure evidence. Values copied from a model cannot enter.
use super::*;
use crate::flow::context::RecordedRead;
use crate::flow::templates::{RootPlanTemplate, TemplateExpansion};
use s::{PlanOperation as Op, PlanUpdateCase as Case};

pub(in crate::flow) struct RootPlanEvidence<'a> {
    pub context: &'a RecordingContext,
    pub current: &'a RecordedRead,
    pub marks: &'a RecordedRead,
    pub input: &'a FilingCompletion,
    pub closures: &'a [RecordedRead],
}

/// This retains the independent operator digest alongside the exact effect.
/// It is neither Clone nor a store writer; the certified author still must file.
pub(in crate::flow) struct PreparedRootEffect {
    origin: Arc<()>,
    effect: s::PlanEffect,
    document_sha256: s::Digest,
    input_sequence: weaver_trace::Sequence,
}
impl PreparedRootEffect {
    pub(in crate::flow) fn digest_for_commit(
        &self,
        context: &RecordingContext,
        commit: &crate::flow::ledger::CommittedSource,
    ) -> Result<&s::Digest, &'static str> {
        let LedgerFile::Statement(file) = commit.file() else {
            return Err("root commit is not a statement");
        };
        if !self.belongs_to(context)
            || file.effect.as_ref()
                != Some(&s::StatementEffect::Plan(Box::new(self.effect.clone())))
            || file.source.turn.is_some()
            || file.source.causal_parent.map(|value| value.0) != Some(self.input_sequence.0)
            || file.block_id.sequence.0 <= self.input_sequence.0
        {
            return Err("root commit differs from the retained operator effect");
        }
        Ok(&self.document_sha256)
    }
    pub(in crate::flow) fn template(&self) -> Result<RootPlanTemplate, &'static str> {
        match &self.effect {
            s::PlanEffect::Authorize { .. } => Ok(RootPlanTemplate::Authorize),
            s::PlanEffect::Lock {
                source: s::PlanLockSource::Operator { .. },
                ..
            } => Ok(RootPlanTemplate::Lock),
            s::PlanEffect::Replace { .. } => Ok(RootPlanTemplate::Replace),
            s::PlanEffect::Lock {
                source: s::PlanLockSource::Model { .. },
                ..
            }
            | s::PlanEffect::Activate { .. }
            | s::PlanEffect::Close { .. }
            | s::PlanEffect::Break { .. } => Err("root grant has a non-root effect"),
        }
    }
    pub(in crate::flow) fn event(
        &self,
        context: &RecordingContext,
        expansion: &TemplateExpansion,
    ) -> Result<AuthoredEvent, &'static str> {
        if !self.belongs_to(context) || expansion.template_id() != self.template()?.id() {
            return Err("root effect lost its context or compiled template family");
        }
        Ok(AuthoredEvent {
            kind: weaver_trace::serving::Kind::PlanEvent,
            subsystem: weaver_trace::Subsystem::Harness,
            turn: None,
            source: AuthoredSource::Loop,
            causal_parent: Some(self.input_sequence),
            payload: Some(weaver_trace::serving::Payload::PlanEffect(Box::new(
                weaver_trace::serving::payloads::PlanEffectRecord {
                    message: serde_json::value::to_raw_value(expansion.message())
                        .expect("Message serializes"),
                    context: crate::flow::mirror::root_plan_effect(&self.effect)?,
                    template_id: expansion.template_id().into(),
                },
            ))),
        })
    }
    /// The complete filing is retained before this check. A correctly rejected
    /// template is an operation refusal, never evidence that the effect committed.
    pub(in crate::flow) fn check_filing(
        &self,
        context: &RecordingContext,
        expansion: &TemplateExpansion,
        filing: &FilingCompletion,
        bound: AbsoluteBound,
    ) -> Result<bool, &'static str> {
        if !self.belongs_to(context)
            || !filing.belongs_to(context)
            || filing.wait_key() != PumpDeadline::Decision
            || filing.wait_bound() != bound
            || bound.expired(filing.recorded_ns())
            || filing.sequence().0 <= self.input_sequence.0
            || !filing.receipts().is_complete()
            || filing.receipts().sources().count() != 1
            || filing.receipts().dispositions().count() != 1
            || expansion.template_id() != self.template()?.id()
        {
            return Err("root effect lost its original complete filing and bound");
        }
        let (file, rejected) = match filing.receipts().dispositions().next().unwrap() {
            SourceDisposition::Committed(value) => {
                let LedgerFile::Statement(file) = value.file() else {
                    return Err("root effect has a pair receipt");
                };
                (file.as_ref(), false)
            }
            SourceDisposition::CorrectlyRejected(value) => (value.file(), true),
        };
        let metadata = file
            .context
            .as_ref()
            .ok_or("root effect lost its typed context")?;
        let effect: s::PlanEffect = s::decode_strict(metadata.raw.get())
            .map_err(|_| "root effect has malformed context")?;
        if file.block_id.run != expansion.run_id()
            || file.block_id.sequence.0 != filing.sequence().0
            || file.block_id.block.0 != 0
            || file.source.kind != "plan.event"
            || file.source.channel != "internal"
            || file.source.origin != "loop"
            || file.source.turn.is_some()
            || file.source.causal_parent.map(|v| v.0) != Some(self.input_sequence.0)
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
            return Err("root effect filing differs from its certified source or private grant");
        }
        Ok(rejected)
    }
    pub(in crate::flow) fn belongs_to(&self, context: &RecordingContext) -> bool {
        Arc::ptr_eq(&self.origin, context.enter_origin())
    }
    pub(in crate::flow) fn effect(&self) -> &s::PlanEffect {
        &self.effect
    }
    pub(in crate::flow) fn document_sha256(&self) -> &s::Digest {
        &self.document_sha256
    }
    pub(in crate::flow) fn input_sequence(&self) -> weaver_trace::Sequence {
        self.input_sequence
    }
}

impl AcceptedPlanUpdate {
    /// Exactly the next closure in the retained operator order. An ordinary
    /// statement selector cannot borrow this read continuation.
    pub(in crate::flow) fn closure_selector(
        &self,
        context: &RecordingContext,
        current: &RecordedRead,
        index: usize,
    ) -> Result<Option<s::ReadSelector>, &'static str> {
        if !self.belongs_to(context)
            || !current.belongs_to(context)
            || current.selector() != &(s::ReadSelector::PlanCurrent {})
        {
            return Err("closure read lacks the root input and its current-plan proof");
        }
        let preserved = match &self.document.operation {
            Op::Replace(value) | Op::AnswerBreak { replacement: value } => {
                &value.preserved_closures
            }
            Op::Create { .. } | Op::Authorize { .. } => return Ok(None),
        };
        if index > preserved.len() {
            return Err("closure read passed the retained request list");
        }
        let Some(id) = preserved.get(index) else {
            return Ok(None);
        };
        let s::QueryResult::PlanCurrent {
            current: Some(plan),
            ..
        } = current.result()
        else {
            return Err("preserved closure has no current plan");
        };
        let step = plan
            .steps
            .iter()
            .find(|step| step.step_id == *id)
            .ok_or("preserved closure lost its preceding step")?;
        Ok(Some(s::ReadSelector::Statement {
            statement: step.instruction.clone(),
        }))
    }

    pub(in crate::flow) fn prepare_effect(
        &self,
        proof: RootPlanEvidence<'_>,
    ) -> Result<Result<PreparedRootEffect, s::PlanUpdateRefused>, &'static str> {
        let RootPlanEvidence {
            context,
            current,
            marks,
            input,
            closures,
        } = proof;
        let (start, scheme, run, bound) = context
            .root_plan_effect_identity(self)
            .ok_or("root effect has no live allocator association")?;
        let seed = start.seed_id.as_str();
        // Recheck actual source association here, at the authority boundary.
        if let Some(refusal) = self.check_filing(context, input, run, &scheme.scheme_id, bound)? {
            return Ok(Err(refusal));
        }
        if !current.belongs_to(context)
            || !marks.belongs_to(context)
            || current.selector() != &(s::ReadSelector::PlanCurrent {})
            || marks.selector() != &(s::ReadSelector::Marks {})
            || current.recall_sequence().0 >= marks.recall_sequence().0
            || marks.recall_sequence().0 >= input.sequence().0
        {
            return Err("root effect lacks its ordered preflight and input evidence");
        }
        let s::QueryResult::PlanCurrent {
            current: previous,
            next_plan_id,
            next_step_id,
        } = current.result()
        else {
            return Err("root effect lacks its plan catalog");
        };
        let s::QueryResult::Marks {
            next_span_id,
            current_task,
            open_scopes,
        } = marks.result()
        else {
            return Err("root effect lacks its marks catalog");
        };
        if next_plan_id.0 == 0 || next_step_id.0 == 0 || next_span_id.0 == 0 {
            return Err("root effect received a zero allocation counter");
        }
        if let Some(plan) = previous {
            let valid_span = |span: &s::SpanRef| {
                span.seed_id == seed
                    && span.revision.0 == 1
                    && span.local_id.0 != 0
                    && span.local_id < *next_span_id
            };
            if plan.plan.plan_id.seed_id != seed
                || plan.plan.plan_id.local_id >= *next_plan_id
                || !valid_span(&plan.task.task)
                || !valid_span(&plan.task.prompt)
                || plan.steps.iter().any(|step| {
                    step.step_id.seed_id != seed
                        || step.step_id.local_id >= *next_step_id
                        || step.span.as_ref().is_some_and(|span| !valid_span(span))
                        || step
                            .outcome_span
                            .as_ref()
                            .is_some_and(|span| !valid_span(span))
                })
                || plan
                    .groups
                    .iter()
                    .any(|group| group.span.as_ref().is_some_and(|span| !valid_span(span)))
            {
                return Err("root allocation catalogs contradict held plan identities");
            }
        }
        let (task_changed, steps, categories, review, preserved) = match &self.context.operation {
            s::PlanInputOperation::Create {
                steps,
                categories,
                review,
                ..
            } => (true, steps.as_slice(), categories, review, &[][..]),
            s::PlanInputOperation::Authorize {
                categories, review, ..
            } => (true, &[][..], categories, review, &[][..]),
            s::PlanInputOperation::Replace(value)
            | s::PlanInputOperation::AnswerBreak { replacement: value } => (
                value.task.is_some(),
                value.steps.as_slice(),
                &value.categories,
                &value.review,
                value.preserved_closures.as_slice(),
            ),
        };
        if closures.len() != preserved.len() {
            return Err("root effect lacks all required closure reads");
        }
        let mut prior_sequence = input.sequence().0;
        for (index, read) in closures.iter().enumerate() {
            if !read.belongs_to(context)
                || Some(read.selector()) != self.closure_selector(context, current, index)?.as_ref()
                || read.recall_sequence().0 <= prior_sequence
            {
                return Err("root effect has foreign or unordered closure evidence");
            }
            prior_sequence = read.recall_sequence().0;
        }
        let task = if !task_changed
            && previous.as_ref().is_some_and(|plan| {
                open_scopes
                    .iter()
                    .any(|scope| scope.kind == s::SpanKind::Task && scope.span == plan.task.task)
            }) {
            let task = &previous.as_ref().unwrap().task;
            if current_task.as_ref() != Some(task) {
                return Err("plan task disagrees with its open marks path");
            }
            task.clone()
        } else {
            let Some(prompt) = next_span_id.0.checked_add(1) else {
                return Ok(Err(refusal(
                    Case::CounterExhausted,
                    "task prompt counter exhausted",
                )));
            };
            let span = |id| s::SpanRef {
                seed_id: seed.into(),
                local_id: id,
                revision: 1.into(),
            };
            s::TaskScopeRef {
                task: span(*next_span_id),
                prompt: span(prompt.into()),
            }
        };
        let mut next_step = Some(next_step_id.0);
        let mut allocated = Vec::with_capacity(steps.len());
        for step in steps {
            let file = committed_block(input, step.block)?;
            let id = if let Some(id) = &step.id {
                id.clone()
            } else {
                let Some(next) = next_step else {
                    return Ok(Err(refusal(
                        Case::CounterExhausted,
                        "step counter exhausted",
                    )));
                };
                next_step = next.checked_add(1);
                s::StepId {
                    seed_id: seed.into(),
                    local_id: next.into(),
                }
            };
            let instruction =
                if let Some(index) = preserved.iter().position(|preserved| *preserved == id) {
                    let Some(previous) = previous.as_ref() else {
                        return Err("preserved closure lost its plan");
                    };
                    let old = previous
                        .steps
                        .iter()
                        .find(|old| old.step_id == id)
                        .ok_or("preserved closure lost its old step")?;
                    let read = &closures[index];
                    let s::QueryResult::Entries { entries } = read.result() else {
                        return Err("closure read lacks statements");
                    };
                    let [entry] = entries.as_slice() else {
                        return Err("closure read lacks one exact statement");
                    };
                    let [binding] = entry.bindings.as_slice() else {
                        return Err("closure read lacks one exact binding");
                    };
                    let old_text = slice(&entry.text, binding.value.statement_span)?;
                    let new_text = slice(&file.text, file.bindings[0].statement_span)?;
                    let old_category = previous
                        .categories
                        .iter()
                        .find(|category| category.id == old.category);
                    let new_category = categories
                        .iter()
                        .find(|category| category.verbs.contains(&file.bindings[0].verb));
                    if old_text != new_text
                        || old_category.is_none()
                        || old_category != new_category
                        || previous.review != *review
                        || old.status != s::StepStatus::Closed
                        || old.span.is_none()
                        || old.outcome_span.is_none()
                        || old.work.is_none()
                        || old.closure_source.is_none()
                    {
                        return Ok(Err(refusal(
                            Case::ClosureChanged,
                            "preserved closure instruction, category or evidence changed",
                        )));
                    }
                    old.instruction.clone()
                } else {
                    s::StatementRef {
                        block_id: file.block_id.clone(),
                        binding: 0.into(),
                    }
                };
            allocated.push(s::PlanStepInit {
                step_id: id,
                instruction,
            });
        }
        let source = committed_block(input, 0.into())?.block_id.clone();
        let effect = match &self.document.operation {
            Op::Create { .. } | Op::Authorize { .. } => {
                let plan = s::PlanRef {
                    plan_id: s::PlanId {
                        seed_id: seed.into(),
                        local_id: *next_plan_id,
                    },
                    revision: 1.into(),
                };
                if matches!(self.document.operation, Op::Authorize { .. }) {
                    s::PlanEffect::Authorize {
                        plan,
                        source,
                        task,
                        review: review.clone(),
                    }
                } else {
                    s::PlanEffect::Lock {
                        plan,
                        source: s::PlanLockSource::Operator { input: source },
                        task,
                        steps: allocated,
                        review: review.clone(),
                    }
                }
            }
            Op::Replace(_) | Op::AnswerBreak { .. } => {
                let expected = self
                    .document
                    .expected
                    .as_ref()
                    .ok_or("replacement lost its expected plan")?;
                if previous.as_ref().is_none_or(|plan| plan.plan != *expected) {
                    return Err("root replacement lost its checked revision");
                }
                let Some(revision) = expected.revision.0.checked_add(1) else {
                    return Ok(Err(refusal(
                        Case::CounterExhausted,
                        "plan revision counter exhausted",
                    )));
                };
                s::PlanEffect::Replace {
                    expected: expected.clone(),
                    next: s::PlanRef {
                        plan_id: expected.plan_id.clone(),
                        revision: revision.into(),
                    },
                    source,
                    task,
                    steps: allocated,
                    review: review.clone(),
                }
            }
        };
        Ok(Ok(PreparedRootEffect {
            origin: context.enter_origin().clone(),
            effect,
            document_sha256: self.context.document_sha256.clone(),
            input_sequence: input.sequence(),
        }))
    }
}
fn committed_block(
    input: &FilingCompletion,
    block: s::DecimalU64,
) -> Result<&s::StatementFile, &'static str> {
    let source = input
        .receipts()
        .dispositions()
        .find(|source| source.source().block == block)
        .ok_or("operator block has no disposition")?;
    let SourceDisposition::Committed(source) = source else {
        return Err("operator block is not committed");
    };
    let LedgerFile::Statement(file) = source.file() else {
        return Err("operator block is not a statement");
    };
    Ok(file)
}
fn slice(text: &str, span: s::Span) -> Result<&str, &'static str> {
    let start = usize::try_from(span.start.0).map_err(|_| "statement offset overflow")?;
    let end = usize::try_from(span.end.0).map_err(|_| "statement offset overflow")?;
    text.get(start..end).ok_or("statement range is invalid")
}
fn refusal(case: Case, detail: &str) -> s::PlanUpdateRefused {
    s::PlanUpdateRefused {
        case,
        detail: detail.into(),
        rejected_blocks: vec![],
        read_code: None,
    }
}
