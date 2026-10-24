//! Native positions use actual task/effect receipts and observed argument work.
//! A stored outcome or a later review author cannot supply an argument endpoint.
use super::*;
use crate::flow::ledger::{LedgerFile, SourceDisposition};
use crate::flow::material::SelectedUnit;
use weaver_types::state_flow as s;

#[derive(Debug)]
enum NativeAuthority {
    Task(Arc<SelectedUnit>),
    Effect(Arc<EffectPlanSummary>),
}

/// Only this owner constructs a native position, retaining the real receipt.
/// The journal can inspect it but cannot accept public span/cursor scalars.
#[derive(Debug)]
pub(in crate::flow) struct NativePosition {
    origin: Arc<()>,
    authority: Arc<NativeAuthority>,
    span: s::SpanRef,
    end: s::ArgumentCursor,
    event: s::SourceBlockId,
}
impl NativePosition {
    pub(in crate::flow) fn origin(&self) -> &Arc<()> {
        &self.origin
    }
    pub(in crate::flow) fn span(&self) -> &s::SpanRef {
        &self.span
    }
    pub(in crate::flow) fn end(&self) -> &s::ArgumentCursor {
        &self.end
    }
    pub(in crate::flow) fn event(&self) -> &s::SourceBlockId {
        &self.event
    }
    pub(in crate::flow) fn receipt_matches(&self) -> bool {
        match self.authority.as_ref() {
            NativeAuthority::Task(unit) => {
                unit.task().and_then(BegunTask::mark).is_some_and(|filing| {
                    receipt_file(filing).is_some_and(|file| file.block_id == self.event)
                })
            }
            NativeAuthority::Effect(proof) => {
                receipt_file(proof.filing()).is_some_and(|file| file.block_id == self.event)
            }
        }
    }
}

#[derive(Debug, Clone)]
struct ObservedScope {
    span: s::SpanRef,
    end: Option<s::ArgumentCursor>,
}
#[derive(Debug, Clone)]
struct PromptLicense {
    span: s::SpanRef,
    source: Option<s::SourceBlockId>,
    event: s::SourceBlockId,
    authority: Arc<NativeAuthority>,
}
#[derive(Debug, Clone)]
pub(in crate::flow) struct NativeScopes {
    summary: Option<s::PlanSummary>,
    active: Vec<ObservedScope>,
    prompt: Option<PromptLicense>,
    registered_prompt: Option<(s::SpanRef, String, u64)>,
    // Only pre-send validation installs this exact context/grant identity. It is
    // consumed with the terminal, including stopped trace-only accounting.
    admitted: Option<(Arc<()>, Arc<()>)>,
}
impl NativeScopes {
    pub(in crate::flow) fn from_boot(boot: &BootPlanSummary) -> Self {
        let active = current(boot.read())
            .map(active_spans)
            .unwrap_or_default()
            .into_iter()
            .map(|span| ObservedScope { span, end: None })
            .collect();
        Self {
            summary: boot.summary().cloned(),
            active,
            prompt: None,
            registered_prompt: None,
            admitted: None,
        }
    }
    fn prompt_registered(
        &self,
        journal: &crate::flow::journal::Journal,
        span: &s::SpanRef,
    ) -> bool {
        self.registered_prompt
            .as_ref()
            .is_some_and(|(registered, run, ordinal)| {
                registered == span && journal.project_mark(run, *ordinal, registered).is_ok()
            })
    }
}

#[derive(Debug)]
pub(in crate::flow) struct NativeMarkBeginError {
    pub prerequisite: EffectPlanSummary,
    pub refusal: PlaceError,
    pub failure: Option<RunFailure>,
}
#[derive(Debug)]
pub(in crate::flow) struct NativeMarkCompletion {
    prerequisite: Arc<EffectPlanSummary>,
    positions: Vec<t::MarkPosition>,
    recorded: Vec<Sequence>,
}
impl NativeMarkCompletion {
    pub(in crate::flow) fn prerequisite(&self) -> &EffectPlanSummary {
        &self.prerequisite
    }
    pub(in crate::flow) fn positions(&self) -> &[t::MarkPosition] {
        &self.positions
    }
    pub(in crate::flow) fn recorded_sequences(&self) -> &[Sequence] {
        &self.recorded
    }
}
#[derive(Debug)]
pub(in crate::flow) struct NativeMarkTransaction {
    pub(super) author: Option<PlaceAuthor>,
    pub(super) bound: AbsoluteBound,
    pub(super) key: PumpDeadline,
    pub(super) cancelled: bool,
    pub(super) done: bool,
    pub(super) incomplete: Option<String>,
    pub(super) account_deadline: Option<u64>,
    positions: VecDeque<NativePosition>,
    registered: Vec<t::MarkPosition>,
    recorded: Vec<Sequence>,
    prerequisite: Arc<EffectPlanSummary>,
    // The exact proposal remains with this owner even after engine adoption.
    // Later engine edits use copy-on-write and cannot rewrite account evidence.
    next: Arc<NativeScopes>,
}

fn current(read: &RecordedRead) -> Option<&s::PlanSnapshot> {
    let s::QueryResult::PlanCurrent { current, .. } = read.result() else {
        return None;
    };
    current.as_ref()
}
fn active_spans(plan: &s::PlanSnapshot) -> Vec<s::SpanRef> {
    if matches!(
        plan.status,
        s::PlanStatus::Complete | s::PlanStatus::Superseded
    ) {
        return vec![];
    }
    let mut active = vec![plan.task.task.clone()];
    for group in &plan.groups {
        if let Some(span) = &group.span
            && group.step_ids.iter().any(|id| {
                plan.steps.iter().any(|step| {
                    &step.step_id == id
                        && matches!(step.status, s::StepStatus::Waiting | s::StepStatus::Active)
                })
            })
        {
            active.push(span.clone());
        }
    }
    active.extend(
        plan.steps
            .iter()
            .filter(|step| step.status == s::StepStatus::Active)
            .filter_map(|step| step.span.clone()),
    );
    active
}
fn receipt_file(filing: &FilingCompletion) -> Option<&s::StatementFile> {
    if !filing.receipts().is_complete() || filing.receipts().is_cancelled() {
        return None;
    }
    let mut receipts = filing.receipts().dispositions();
    let SourceDisposition::Committed(source) = receipts.next()? else {
        return None;
    };
    if receipts.next().is_some() {
        return None;
    }
    let LedgerFile::Statement(file) = source.file() else {
        return None;
    };
    if source.source() != &file.block_id {
        return None;
    }
    Some(file)
}

impl RecordingContext {
    /// A new committed cache cannot bypass its native-scope receipt handoff.
    pub(in crate::flow) fn native_place_cache_ready(&mut self) -> bool {
        if self.place_admission_stopped() {
            return false;
        }
        let Ok(summary) = self.committed_plan_summary() else {
            return false;
        };
        let summary = summary.cloned();
        self.native_marks.is_none()
            && self
                .place_engine
                .as_ref()
                .is_some_and(|engine| engine.native.summary == summary)
    }

    #[allow(clippy::result_large_err)]
    pub(in crate::flow) fn begin_place_effect_marks(
        &mut self,
        prerequisite: EffectPlanSummary,
        io: &mut impl PumpIo,
    ) -> Result<(), NativeMarkBeginError> {
        let failure = self.live(io).err();
        let refusal = if failure.is_some()
            || self.phase != RecordingPhase::Entered
            || self.place_admission_stopped()
        {
            Some(PlaceError::WrongPhase)
        } else if self.native_marks.is_some()
            || self.place_busy_except_reload()
            || (self
                .reload
                .as_ref()
                .is_some_and(|work| work.bound.is_some())
                && !self.reload_draining())
        {
            Some(PlaceError::Busy(self.held_input()))
        } else if self.place_engine.is_none()
            || !prerequisite.filing().belongs_to(self)
            || !prerequisite.read().belongs_to(self)
            || self.committed_plan_summary().ok() != Some(prerequisite.summary())
        {
            Some(PlaceError::WrongProof)
        } else if self.deadlines.get(prerequisite.filing().wait_key())
            != Some(prerequisite.filing().wait_bound())
        {
            Some(PlaceError::MissingWait)
        } else {
            None
        };
        if let Some(refusal) = refusal {
            return Err(NativeMarkBeginError {
                prerequisite,
                refusal,
                failure,
            });
        }
        let Some(file) = receipt_file(prerequisite.filing()) else {
            return Err(NativeMarkBeginError {
                prerequisite,
                refusal: PlaceError::WrongProof,
                failure: None,
            });
        };
        let Some(s::StatementEffect::Plan(effect)) = &file.effect else {
            return Err(NativeMarkBeginError {
                prerequisite,
                refusal: PlaceError::WrongProof,
                failure: None,
            });
        };
        let Some(plan) = current(prerequisite.read()) else {
            return Err(NativeMarkBeginError {
                prerequisite,
                refusal: PlaceError::WrongProof,
                failure: None,
            });
        };
        let expected = active_spans(plan);
        let engine = self.place_engine.as_ref().unwrap();
        let closing: Vec<_> = engine
            .native
            .active
            .iter()
            .filter(|old| !expected.contains(&old.span))
            .cloned()
            .collect();
        let closes = matches!(
            effect.as_ref(),
            s::PlanEffect::Close { .. } | s::PlanEffect::Replace { .. }
        );
        let end = self.boot_journal.as_ref().unwrap().current_end();
        if closing.iter().any(|scope| {
            scope
                .end
                .as_ref()
                .is_some_and(|prior| !closes || prior != &end)
        }) {
            return Err(NativeMarkBeginError {
                prerequisite,
                refusal: PlaceError::Stale,
                failure: None,
            });
        }
        let event = file.block_id.clone();
        let prompt_span = plan.task.prompt.clone();
        // Create/authorize operator block zero is the complete task Text. A
        // replacement can omit a task and renew the prior prompt: in that case
        // only an actual closed-prompt unit below can establish its source.
        let prompt_source = match effect.as_ref() {
            s::PlanEffect::Authorize { source, .. } => Some(source.clone()),
            s::PlanEffect::Lock {
                source: s::PlanLockSource::Operator { input },
                ..
            } => Some(input.clone()),
            _ => None,
        };
        let establishes_prompt = matches!(
            effect.as_ref(),
            s::PlanEffect::Authorize { .. }
                | s::PlanEffect::Lock {
                    source: s::PlanLockSource::Operator { .. },
                    ..
                }
                | s::PlanEffect::Replace { .. }
        );
        let bound = prerequisite.filing().wait_bound();
        let key = prerequisite.filing().wait_key();
        let prerequisite = Arc::new(prerequisite);
        let authority = Arc::new(NativeAuthority::Effect(prerequisite.clone()));
        let mut next = engine.native.as_ref().clone();
        next.summary = prerequisite.summary().cloned();
        next.active = expected
            .into_iter()
            .map(|span| {
                engine
                    .native
                    .active
                    .iter()
                    .find(|old| old.span == span)
                    .cloned()
                    .unwrap_or(ObservedScope { span, end: None })
            })
            .collect();
        if establishes_prompt
            && !next.prompt_registered(self.boot_journal.as_ref().unwrap(), &prompt_span)
        {
            next.prompt = Some(PromptLicense {
                span: prompt_span,
                source: prompt_source,
                event: event.clone(),
                authority: authority.clone(),
            });
        }
        let mut positions: VecDeque<_> = closing
            .into_iter()
            .filter_map(|scope| {
                scope.end.map(|end| NativePosition {
                    origin: self.read_origin.clone(),
                    authority: authority.clone(),
                    span: scope.span,
                    end,
                    event: event.clone(),
                })
            })
            .collect();
        if let Some(license) = next.prompt.as_ref()
            && !next.prompt_registered(self.boot_journal.as_ref().unwrap(), &license.span)
            && let Some((source, text)) =
                prompt_text(license, self.boot_journal.as_ref().unwrap().held_units())
            && let Some(end) = self
                .boot_journal
                .as_ref()
                .unwrap()
                .full_text_end(&source, &text)
        {
            positions.push_back(NativePosition {
                origin: self.read_origin.clone(),
                authority: license.authority.clone(),
                span: license.span.clone(),
                end,
                event: license.event.clone(),
            });
        }
        self.barrier.set_place_owed(true);
        self.native_marks = Some(NativeMarkTransaction {
            author: None,
            bound,
            key,
            cancelled: false,
            done: false,
            incomplete: None,
            account_deadline: None,
            positions,
            registered: vec![],
            recorded: vec![],
            prerequisite,
            next: Arc::new(next),
        });
        Ok(())
    }

    pub(in crate::flow) fn advance_place_marks<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<PlaceProgress, RunFailure> {
        self.live(ports.io)?;
        let Some(work) = self.native_marks.as_ref() else {
            return Err(RunFailure::PlaceOwned);
        };
        if self.deadlines.get(work.key) != Some(work.bound) {
            return Err(self.place_fault(ports.io, "native mark lost original filing wait"));
        }
        if work.done {
            return Ok(PlaceProgress::Complete);
        }
        if work.author.is_some() {
            self.advance_place_author(ports)?;
            if self
                .native_marks
                .as_ref()
                .unwrap()
                .author
                .as_ref()
                .unwrap()
                .sequence
                .is_some()
            {
                self.consume_place_completion()?;
                self.retain_native_recorded()?;
                self.native_marks.as_mut().unwrap().author = None;
            }
            return Ok(PlaceProgress::Progress);
        }
        if self.prepare_native_mark_author(ports.io)? {
            return Ok(PlaceProgress::Progress);
        }
        let work = self.native_marks.as_mut().unwrap();
        // All admitted positions have actual Recorded receipts. The engine and
        // owner share that exact immutable proposal, without cloning metadata or
        // reconstructing a cancelled account from a later engine snapshot.
        if self.phase == RecordingPhase::Entered {
            self.place_engine.as_mut().unwrap().native = Arc::clone(&work.next);
        }
        work.done = true;
        Ok(PlaceProgress::Complete)
    }
    pub(super) fn retain_native_recorded(&mut self) -> Result<(), RunFailure> {
        let work = self.native_marks.as_mut().ok_or(RunFailure::PlaceOwned)?;
        let sequence = work
            .author
            .as_ref()
            .and_then(|author| author.sequence)
            .ok_or(RunFailure::PlaceOwned)?;
        if work.recorded.contains(&sequence) {
            return Err(RunFailure::PlaceOwned);
        }
        work.recorded.push(sequence);
        Ok(())
    }
    /// The stopped account calls this only under its original bound after the
    /// first fault attempt. It constructs no grant and retains the real receipt.
    pub(super) fn prepare_native_mark_author(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<bool, RunFailure> {
        if self
            .native_marks
            .as_ref()
            .is_none_or(|work| work.author.is_some())
        {
            return Ok(false);
        }
        let Some(position) = self.native_marks.as_mut().unwrap().positions.pop_front() else {
            return Ok(false);
        };
        let record = self
            .boot_journal
            .as_mut()
            .unwrap()
            .native_mark(&position)
            .map_err(|error| self.place_fault(io, format!("native position: {error:?}")))?;
        let event = place_event(
            Kind::StateMarkPosition,
            AuthoredSource::Loop,
            Payload::MarkPosition(Box::new(record.clone())),
            None,
            None,
        );
        let work = self.native_marks.as_mut().unwrap();
        let next = Arc::make_mut(&mut work.next);
        if next
            .prompt
            .as_ref()
            .is_some_and(|license| license.span == *position.span())
        {
            next.registered_prompt = Some((
                position.span().clone(),
                record.id.run.clone(),
                record.id.ordinal.0,
            ));
        }
        work.registered.push(record);
        work.author = Some(PlaceAuthor::new(event));
        Ok(true)
    }
    pub(in crate::flow) fn cancel_place_marks(&mut self) -> Result<(), RunFailure> {
        let Some(work) = self.native_marks.as_mut() else {
            return Err(RunFailure::PlaceOwned);
        };
        work.cancelled = true;
        Ok(())
    }
    pub(in crate::flow) fn take_place_marks(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<Option<NativeMarkCompletion>, RunFailure> {
        self.live(io)?;
        if self.phase != RecordingPhase::Entered
            || !self
                .native_marks
                .as_ref()
                .is_some_and(|work| work.done && !work.cancelled)
        {
            return Ok(None);
        }
        let work = self.native_marks.take().unwrap();
        self.barrier.set_place_owed(self.place_owed());
        Ok(Some(NativeMarkCompletion {
            prerequisite: work.prerequisite,
            positions: work.registered,
            recorded: work.recorded,
        }))
    }

    pub(super) fn take_native_account_work(&mut self) -> Option<NativeMarkTransaction> {
        self.native_marks.take()
    }

    /// A settled cancellation can leave Entered without resetting its original
    /// wait or granting another turn. Failed/Releasing keep their account path.
    pub(in crate::flow) fn take_cancelled_place_marks(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<Option<PlaceAccount>, RunFailure> {
        self.live(io)?;
        if self.phase != RecordingPhase::Entered
            || !self.native_marks.as_ref().is_some_and(|work| {
                work.done
                    && work.cancelled
                    && work.incomplete.is_none()
                    && work.author.is_none()
                    && work.positions.is_empty()
                    && work.registered.len() == work.recorded.len()
            })
        {
            return Ok(None);
        }
        let work = self.take_native_account_work().unwrap();
        self.barrier.set_place_owed(self.place_owed());
        Ok(Some(PlaceAccount {
            phase: self.phase,
            work: AccountWork::Native(Box::new(work)),
            measure: None,
        }))
    }

    /// Validate the actual selected task proof before send, retaining its proposed
    /// scopes with this exact grant. Only a complete terminal adopts the proposal.
    /// It cannot retire another task or manufacture the missing closing receipt.
    pub(super) fn native_place_before_append(
        &mut self,
        grant: &PlacementGrant,
    ) -> Result<NativeScopes, PlaceError> {
        self.native_for_append(grant)
    }
    fn native_for_append(&mut self, grant: &PlacementGrant) -> Result<NativeScopes, PlaceError> {
        if !self.native_place_cache_ready() {
            return Err(PlaceError::WrongProof);
        }
        let mut native = self.place_engine.as_ref().unwrap().native.as_ref().clone();
        for unit in &grant.units {
            let Some(task) = unit.task() else {
                continue;
            };
            if !Arc::ptr_eq(task.origin(), &self.read_origin) {
                return Err(PlaceError::WrongProof);
            }
            let scope = task.scope();
            if native
                .active
                .first()
                .is_some_and(|old| old.span != scope.task)
            {
                return Err(PlaceError::Stale);
            }
            if native.active.is_empty() {
                native.active.push(ObservedScope {
                    span: scope.task.clone(),
                    end: None,
                });
            }
            let Some(filing) = task.mark() else {
                continue;
            };
            let Some(file) = receipt_file(filing) else {
                return Err(PlaceError::WrongProof);
            };
            if !matches!(&file.effect, Some(s::StatementEffect::Mark(s::MarkEffect::BeginTask { task: held_task, prompt, source: s::ContentRef::Event { block_id } }))
                if held_task == &scope.task && prompt == &scope.prompt && block_id == task.input().source())
            {
                return Err(PlaceError::WrongProof);
            }
            let authority = Arc::new(NativeAuthority::Task(unit.clone()));
            if !native.prompt_registered(self.boot_journal.as_ref().unwrap(), &scope.prompt) {
                native.prompt = Some(PromptLicense {
                    span: scope.prompt.clone(),
                    source: Some(task.input().source().clone()),
                    event: file.block_id.clone(),
                    authority: authority.clone(),
                });
            }
        }
        native.admitted = Some((self.read_origin.clone(), grant.identity.clone()));
        Ok(native)
    }

    /// Consume the retained pre-send admission after the actual complete terminal.
    /// Failed/Releasing accounting must keep truthful native facts without asking
    /// a revoked live cache for a new grant or changing the first failure.
    pub(super) fn native_place_completed(
        &mut self,
        proof: &ConfirmedGeneration,
        mut native: NativeScopes,
    ) -> Result<Vec<t::MarkPosition>, &'static str> {
        let Some((origin, grant)) = native.admitted.take() else {
            return Err("confirmed generation lost native admission");
        };
        if !Arc::ptr_eq(&origin, &self.read_origin)
            || !Arc::ptr_eq(&origin, &proof.grant.origin)
            || !Arc::ptr_eq(&grant, &proof.grant.identity)
        {
            return Err("confirmed generation has foreign native admission");
        }
        self.place_engine
            .as_mut()
            .ok_or("native position lacks boot")?
            .native = Arc::new(native);
        let end = self
            .boot_journal
            .as_ref()
            .ok_or("native position lacks journal")?
            .current_end();
        let engine = self
            .place_engine
            .as_mut()
            .ok_or("native position lacks boot")?;
        let native = Arc::make_mut(&mut engine.native);
        for scope in &mut native.active {
            scope.end = Some(end.clone());
        }
        let Some(license) = native.prompt.as_ref() else {
            return Ok(vec![]);
        };
        if native.prompt_registered(self.boot_journal.as_ref().unwrap(), &license.span) {
            return Ok(vec![]);
        }
        let Some((source, text)) =
            prompt_text(license, self.boot_journal.as_ref().unwrap().held_units())
        else {
            return Ok(vec![]);
        };
        let Some(end) = self
            .boot_journal
            .as_ref()
            .unwrap()
            .full_text_end(&source, &text)
        else {
            return Ok(vec![]);
        };
        let position = NativePosition {
            origin: self.read_origin.clone(),
            authority: license.authority.clone(),
            span: license.span.clone(),
            end,
            event: license.event.clone(),
        };
        let record = self
            .boot_journal
            .as_mut()
            .unwrap()
            .native_mark(&position)
            .map_err(|_| "native prompt position is invalid")?;
        native.registered_prompt = Some((
            license.span.clone(),
            record.id.run.clone(),
            record.id.ordinal.0,
        ));
        Ok(vec![record])
    }
}

fn prompt_text<'a>(
    license: &PromptLicense,
    units: impl IntoIterator<Item = &'a Arc<SelectedUnit>>,
) -> Option<(s::SourceBlockId, String)> {
    for unit in units {
        if let Some(source) = unit.filed_source()
            && let LedgerFile::Statement(file) = source.file()
            && license.source.as_ref() == Some(&file.block_id)
            && !file.text.is_empty()
        {
            let material = s::MaterialRef::Text {
                source: s::ContentRef::Event {
                    block_id: file.block_id.clone(),
                },
                span: s::Span {
                    start: 0.into(),
                    end: (file.text.len() as u64).into(),
                },
            };
            if unit.sources().contains(&material) {
                return Some((file.block_id.clone(), file.text.clone()));
            }
        }
        if let Some(task) = unit.task()
            && task.scope().prompt == license.span
            && let [weaver_traits::ContentBlock::Text { text }] =
                task.input().message().content.as_slice()
        {
            return Some((task.input().source().clone(), text.clone()));
        }
        for read in unit.reads() {
            let entries = match read.result() {
                s::QueryResult::Entries { entries } => entries,
                _ => continue,
            };
            for entry in entries {
                let s::ReadSource::Event(source) = &entry.source else {
                    continue;
                };
                let matches_span = unit.spans().contains(&license.span);
                if !matches_span && license.source.as_ref() != Some(&source.block_id) {
                    continue;
                }
                let material = s::MaterialRef::Text {
                    source: s::ContentRef::Event {
                        block_id: source.block_id.clone(),
                    },
                    span: s::Span {
                        start: 0.into(),
                        end: (entry.text.len() as u64).into(),
                    },
                };
                if !entry.text.is_empty() && unit.sources().contains(&material) {
                    return Some((source.block_id.clone(), entry.text.clone()));
                }
            }
        }
    }
    None
}

impl PlaceAccount {
    pub(in crate::flow) fn native_prerequisite(&self) -> Option<&EffectPlanSummary> {
        match &self.work {
            AccountWork::Native(work) => Some(&work.prerequisite),
            _ => None,
        }
    }
    pub(in crate::flow) fn native_proposal(&self) -> Option<&NativeScopes> {
        match &self.work {
            AccountWork::Native(work) => Some(work.next.as_ref()),
            _ => None,
        }
    }
    pub(in crate::flow) fn native_positions(&self) -> &[t::MarkPosition] {
        match &self.work {
            AccountWork::Native(work) => &work.registered,
            _ => &[],
        }
    }
    pub(in crate::flow) fn native_recorded_sequences(&self) -> &[Sequence] {
        match &self.work {
            AccountWork::Native(work) => &work.recorded,
            _ => &[],
        }
    }
}
